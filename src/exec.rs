//! The effect of every primitive in `builtins`.

use std::rc::Rc;

use crate::builtins::{
    Arith, CodeTable, Cond, DefMode,
    LoadKind, OccKind, Prefix, Primitive,
    TextOf,
};
use crate::env::{Binding, GroupKind};

use crate::facts::Severity;
use crate::graph::EdgeKind;
use crate::graph::ControlDep;
use crate::machine::{CondFrame, CondLimit, Machine, Step};
use crate::tex::{
    ArgSpec, Catcode, MacroDef, Meaning, ParameterText, RegKind, Span, Sym, Tok, Token,
};
use crate::value::{self, Glue, Num, Value};

/// tex.web § 1232: `"8000` is the largest math code.
const MATH_CODE_MAX: i64 = 0x8000;

/// TeX Live's `max_in_open`: input files open at a time.
const MAX_IN_OPEN: usize = 15;

/// What `\if` and `\ifcat` compare for a token whose meaning is unknown.
const UNKNOWN_OPERAND: (u32, u8) = (u32::MAX, u8::MAX);

/// Keeps the per-character code tables out of the namespace control
/// sequences live in, the way the register names are kept out of it.
const CODE: char = '\u{3}';

impl Machine<'_> {
    pub fn execute(&mut self, p: Primitive, by: Sym, span: Span) {
        // What may put a node on the current list.
        if matches!(
            p,
            // `\unskip`, `\unkern`, `\unpenalty` only take an item away.
            Primitive::Typeset(crate::builtins::Typeset::Number | crate::builtins::Typeset::Dimen | crate::builtins::Typeset::Glue | crate::builtins::Typeset::Delimiter)
                | Primitive::Accent
                | Primitive::Pdf(_)
                | Primitive::Write
                | Primitive::Command(_)
                | Primitive::Unmodeled
                | Primitive::Lua(_)
        ) || matches!(p, Primitive::Mode(cmd) if cmd != crate::builtins::ModeCmd::Par)
        {
            self.material = true;
            self.natural_before(p);
        }
        if p == Primitive::Typeset(crate::builtins::Typeset::Plain) {
            let removed = match self.name(self.env.identity(by)) {
                "unskip" => crate::mode::Removed::Glue,
                "unkern" => crate::mode::Removed::Kern,
                _ => crate::mode::Removed::Penalty,
            };
            self.remove_item(removed);
        }
        if crate::builtins::assignment(p) {
            self.apply_global_defs();
        }
        self.execute_command(p, by, span);
        if p == Primitive::IntegerParameter && self.env.identity(by) == self.intern("spacefactor") {
            let sf = self.env.value(self.env.identity(by)).as_int();
            if let Some(n) = &mut self.natural {
                n.space_factor = sf;
            }
        }
        // tex.web § 1269: `prefixed_command` ends by putting back the token
        // `\afterassignment` saved, so it is read only once the assignment
        // that followed it has been carried out.
        if crate::builtins::assignment(p)
            && let Some(token) = self.after_assignment.take()
        {
            self.unread(token);
        }
    }

    fn execute_command(&mut self, p: Primitive, by: Sym, span: Span) {
        use Primitive as P;
        match p {
            P::Prefix(k) => match k {
                Prefix::Global => self.prefixes.global = true,
                Prefix::Long => self.prefixes.long = true,
                Prefix::Outer => self.prefixes.outer = true,
                Prefix::Protected => self.prefixes.protected = true,
                Prefix::Immediate => self.prefixes.immediate = true,
            },

            P::Def { global, expand } => self.primitive_def(by, span, global, expand),
            P::Let { global, future } => self.primitive_let(by, span, global, future),

            P::ExpandAfter => {
                if let Some(held) = self.next_token() {
                    let target = self.peek();
                    self.held.push((held, target));
                    self.expand_once();
                    self.held.pop();
                    self.unread(held);
                }
            }
            // Only `\ifx` sees this meaning; no control sequence has it.
            P::DontExpand => {}
            P::SkipCrossing { nested, to_fi } => self.skip_crossing(nested, to_fi, by, span),
            P::NoExpand => {
                // tex.web § 367: the token that follows goes back into the
                // input behind a `dont_expand` marker, which makes the next
                // read of it take it as `\relax` (§ 358).
                // tex.web § 367: the token is read with the scanner normal,
                // so a file may end before it (the `\everyeof{\noexpand}`
                // idiom).
                let outer = std::mem::replace(&mut self.scanner, crate::machine::Scanner::Normal);
                let next = self.next_token();
                self.scanner = outer;
                if let Some(token) = next {
                    let guarded = token.cs().is_some() || self.active_cs(token).is_some();
                    self.unread(token);
                    if guarded {
                        let marker = Token::new(Tok::Cs(self.dont_expand), token.span);
                        self.unread(marker);
                    }
                }
            }
            P::Text(kind) => self.produce_text(kind, span),
            // e-TeX's `\unexpanded`, `\scantokens` and pdfTeX's `\expanded`
            // read a ⟨general text⟩: `scan_left_brace` expands until the `{`
            // (tex.web § 403, § 473).
            P::Reinject => {
                let body = self.read_general_text();
                self.push_tokens(Rc::from(body), None);
            }
            // e-TeX manual, `\scantokens`: the text is written to a pseudo
            // file, a line wherever `\newlinechar` stands, and read back as a
            // file is: each line ends with `\endlinechar`, and `\everyeof`
            // is inserted when the pseudo file ends (expl3's
            // `\tl_set_rescan:Nnn` finds its end marker there).
            P::ScanTokens { text: pseudo_text } => {
                let body = self.read_general_text();
                let text = crate::tex::detokenize(&body, &self.out.interner);
                self.sync_end_line();
                let newline_sym = self.intern("newlinechar");
                let newline = self
                    .env
                    .value(newline_sym)
                    .as_int()
                    .and_then(|c| u32::try_from(c).ok())
                    .and_then(char::from_u32);
                // `\scantextokens` reads no pseudo file: its end is no end
                // of file, so a definition or argument goes on past it.
                if pseudo_text {
                    let lines: Vec<&str> = match newline {
                        Some(c) => text.split(c).collect(),
                        None => vec![text.as_str()],
                    };
                    let mut tokens = Vec::new();
                    let last = lines.len().saturating_sub(1);
                    for (i, line) in lines.into_iter().enumerate() {
                        if i == last {
                            let end_line = std::mem::replace(&mut self.end_line, crate::tex::EndLineChar(-1));
                            tokens.extend(self.tokenize_line_at(line, span));
                            self.end_line = end_line;
                        } else {
                            tokens.extend(self.tokenize_line_at(line, span));
                        }
                    }
                    self.push_tokens(Rc::from(tokens), None);
                    return;
                }
                // Lines end at `\newlinechar`; the last one ends too.
                let mut lines: String = match newline {
                    Some(c) if c != '\n' => text.replace(c, "\n"),
                    _ => text,
                };
                lines.push('\n');
                let eof = self.intern("everyeof");
                let eof = match self.env.value(eof) {
                    Value::Toks(extra) => extra.clone(),
                    _ => Rc::from(Vec::new()),
                };
                // The pseudo file is an input file until its `\everyeof` is
                // read (etex.ch), and TeX Live opens at most `max_in_open`
                // files at a time.
                let pseudo = self.pseudo_file;
                let open = self.expanding.get(pseudo.0 as usize).copied().unwrap_or(0) as usize;
                if open + usize::from(self.file_depth) >= MAX_IN_OPEN {
                    return self.halt_capacity("text input levels");
                }
                self.bump_expanding(pseudo, 1);
                let mouth = Box::new(crate::tex::Mouth::new(&lines, span.file));
                self.push_pseudo(mouth, span, eof);
            }
            // pdfTeX's `\expanded` scans its text while expanding it, as
            // `\edef` does, so `\expanded{{#1\iffalse}}\fi …}` reaches past
            // the `}` its test skips (expl3's key paths).
            P::Expanded => {
                if self.scan_left_brace() {
                    let result = self.scan_expanded_body(false);
                    self.push_tokens(Rc::from(result), None);
                }
            }
            P::Use(arguments) => {
                let mut kept = Vec::new();
                for _ in 0..arguments {
                    kept.extend(self.read_undelimited());
                }
                self.push_tokens(Rc::from(kept), None);
            }
            P::Csname => self.build_csname(span),
            // LuaTeX manual, "\begincsname": the name is built as by
            // `\csname`, but an undefined one stays undefined and nothing is
            // left in the input.
            P::BeginCsname => {
                let Some(name) = self.scan_csname_text(span) else {
                    return self.unread_unknown(span);
                };
                let sym = self.intern(&name);
                self.last_named_cs = Some(sym);
                if self.env.is_defined(sym) {
                    self.unread(Token::new(Tok::Cs(sym), span));
                }
            }
            P::LastNamedCs => {
                let sym = match self.last_named_cs {
                    Some(sym) => sym,
                    None => {
                        let relax = self.intern("\u{4}primitive.relax");
                        self.env.set(relax, Binding::builtin(Meaning::Primitive(Primitive::Relax)), true);
                        relax
                    }
                };
                self.unread(Token::new(Tok::Cs(sym), span));
            }
            P::Endcsname => {}

            P::If(cond) => self.conditional(cond, by, span, false),
            P::Unless => {
                let negated = self.next_token().and_then(|t| t.cs());
                if let Some(P::If(cond)) = negated.and_then(|sym| self.env.meaning(sym).prim()) {
                    self.conditional(cond, negated.expect("checked above"), span, true);
                }
            }
            P::Else | P::Or | P::Fi if self.testing.last() == Some(&self.conds.len()) => {
                self.insert_relax(by, span);
            }
            P::Else | P::Or => self.conditional_else(span, p == P::Else),
            P::Fi => self.conditional_fi(span),
            P::BeginGroup => self.open_group(GroupKind::SemiSimple, span),
            P::EndGroup => self.close_group(GroupKind::SemiSimple, span),
            P::Defer => {
                // tex.web § 326: `\aftergroup` saves the token and the group
                // gives it back when it ends.  tex.web § 1269:
                // `\afterassignment` holds one token until the next
                // assignment is done.
                let Some(token) = self.next_token() else { return };
                if self.name(by) == "aftergroup" {
                    self.env.save_after(token);
                } else {
                    self.after_assignment = Some(token);
                }
            }

            P::Allocate(kind) => self.allocate(by, span, kind),
            P::RegisterDef(kind) => self.register_def(by, span, kind),
            P::FontDef => self.font_def(by, span),
            P::Write => self.write_stream(span),
            P::Expr(kind) => {
                let value = self.eval_expression(kind);
                let tokens = match value {
                    Value::Unknown | Value::Range { .. } => vec![self.unknown_token(span)],
                    value => self.text_tokens(&value.render(), span),
                };
                self.unread_all(&tokens);
            }
            P::OpenIn => self.open_in(),
            P::CloseIn => {
                let stream = self.scan_number().unwrap_or(0);
                self.close_in(stream);
            }
            P::Read => self.read_line_into(by, span),
            // tex.web § 1079: `\box⟨n⟩` is `make_box`, whose box is appended
            // to the current list; only `\setbox` assigns a box register.
            P::Register(RegKind::Box) => {
                let n = self.scan_number();
                let dims = self.box_dims(n);
                let state = self.fetch_box(n, true);
                self.append_box(state, dims);
            }
            P::Register(kind) => self.assign_numbered_register(kind, span),
            P::Arith(op) => self.arithmetic(op, span),
            P::IntegerParameter => {
                // A copy such as `\tex_endlinechar:D` sets the parameter itself.
                let by = self.env.identity(by);
                self.read_equals();
                let value = self.scan_number();
                let global = self.prefixes.global;
                self.env.set_value(by, value.map_or(Value::Unknown, Value::Int), global);
                self.note_assignment(by, global, span);
                // `\endlinechar` is the character the mouth appends to every
                // line (tex.web § 240).  expl3 sets it to a space it has made
                // ignorable, which is what ends a parameter text there.
                if self.name(by) == "endlinechar"
                    && let Some(value) = value
                    && let Ok(value) = i32::try_from(value)
                {
                    self.set_end_line(value);
                }
            }
            P::DimenParameter => {
                let by = self.env.identity(by);
                self.read_equals();
                let value = self.scan_dimen();
                let global = self.prefixes.global;
                self.env.set_value(by, value.map_or(Value::Unknown, Value::Dimen), global);
            }
            P::CatcodeAssign => self.assign_catcode(span),
            P::CharCode(table) => self.assign_char_code(table, span),
            P::CaseShift(table) => self.case_shift(table),

            P::Load(kind) => self.load_command(span, kind),
            // `\endinput` reached only on some path would cut the analysis of
            // everything after it, so under a control dependency it is noted
            // and ignored.
            P::Endinput => {
                // On a path of its own the file ends there; arms run one
                // after the other cannot end it for one of them.
                if self.cds.is_empty() || (!self.split_at.is_empty() && !self.conds.iter().any(|c| c.undecided)) {
                    self.end_input();
                } else {
                    self.diagnose(
                        Severity::Unsupported,
                        "conditional-endinput",
                        span,
                        "\\endinput under an undecided condition; the file is read on".into(),
                    );
                }
            }


            // The engine's `\end` (tex.web § 1054); latex.ltx keeps it as
            // `\@@end` and defines its own `\end{…}`.
            P::End => self.end_job(),

            P::Message { .. } => self.message(by, span),
            P::Lua(op) => crate::plugin::lua::primitive(self, op, by, span),
            P::LuaCall { id, .. } => crate::plugin::lua::call(self, id, by, span),
            P::Docstrip(op) => crate::literate::docstrip(self, op, span),
            // A delimiter is one token and belongs to the primitive that
            // scans it (tex.web § 1161); everything else it sets is material,
            // which satex does not typeset.
            P::Typeset(crate::builtins::Typeset::Delimiter) => {
                self.skip_spaces();
                self.next_token();
            }
            P::Accent => {
                if !self.horizontal_material(Token::new(Tok::Cs(by), span)) {
                    crate::observe::accent(self, span);
                }
            }
            P::Mode(cmd) => self.mode_command(cmd, by, span),
            // tex.web § 1045: the next non-blank token after expansion is
            // read as a command.
            P::IgnoreSpaces => {
                while let Some(token) = self.next_x_token() {
                    if !matches!(self.category_of(token), Some(Catcode::Space)) {
                        self.unread(token);
                        break;
                    }
                }
            }
            // The value a penalty, kern or glue item scans (tex.web § 1057,
            // § 1060, § 1103); `\mkern` and `\mskip` in math units.
            P::Typeset(crate::builtins::Typeset::Number) => {
                let penalty = self.scan_number();
                self.append_item(crate::mode::Item::Penalty(penalty));
            }
            P::Typeset(crate::builtins::Typeset::Dimen) => {
                if self.name(self.env.identity(by)) == "mkern" {
                    self.scan_mu_dimen();
                    self.append_item(crate::mode::Item::Unknown);
                } else {
                    let kern = self.scan_dimen();
                    self.append_item(crate::mode::Item::Kern(kern));
                }
            }
            P::Typeset(crate::builtins::Typeset::Glue) => {
                self.scan_glue(true);
            }
            P::Pdf(op) => crate::observe::pdf(self, op, span),
            P::Relax | P::Typeset(_) => {}
            // A primitive of the engine whose effect satex does not
            // interpret.  It does nothing here, and the run says so, so that
            // what an analysis could not follow is on the record.
            P::Unmodeled => {
                self.note_gap("unmodeled-primitive", by, span);
                self.widen_mode();
            }
            P::GlueParameter { .. }
            | P::TokensParameter
            | P::LastItem(_)
            | P::Special(_)
            | P::FontParam(_)
            | P::FontIdent(_)
            | P::Command(_)
            | P::Convert(_) => self.execute_extra(p, by, span),
        }
    }

    pub fn make_macro(&self, parameter_text: ParameterText, arg_spec: Option<ArgSpec>, body: Vec<Token>) -> Meaning {
        let mut replacement = crate::tex::parameterize(body);
        // `\def\a#1#{…}`: the `{` that ended the parameter text belongs to the
        // replacement text as well (tex.web § 476).
        if parameter_text.brace_end {
            let span = replacement.last().map(|t| t.span).unwrap_or_default();
            replacement.push(Token::new(Tok::Chr('{', Catcode::Begin), span));
        }
        Meaning::Macro(Rc::new(MacroDef {
            parameter_text,
            arg_spec,
            replacement_text: Rc::from(replacement),
            long: self.prefixes.long,
            outer: self.prefixes.outer,
            protected: self.prefixes.protected,
        }))
    }

    /// Where a definition is placed: at the name when the file wrote it
    /// and a macro of another file does the defining — expl3's
    /// `\cs_new:Npn \my_fn:n` reaches `\tex_gdef:D` inside
    /// `expl3-code.tex` — and where the defining command stands otherwise.
    fn definition_site(&mut self, span: Span) -> Span {
        self.skip_spaces();
        match self.peek() {
            Some(token) if token.span.file != span.file && token.span.line > 0 => token.span,
            _ => span,
        }
    }

    fn primitive_def(&mut self, by: Sym, span: Span, global: bool, expand: bool) {
        let span = self.definition_site(span);
        // A prefix belongs to the command that follows it, so it is spent even
        // when that command finds nothing to define (tex.web § 1211).
        let Some(name) = self.read_r_token() else {
            self.prefixes = Default::default();
            return;
        };
        // The parameter text ends at the `{` that opens the replacement text.
        // If no `{` turns up, the catcodes are not what the file assumed, and
        // reading on would consume the rest of it.
        let mut pattern = Vec::new();
        loop {
            if pattern.len() >= self.cfg.limits.expansion_tokens {
                self.unread_all(&pattern);
                self.diagnose(
                    Severity::Unsupported,
                    "unterminated-definition",
                    span,
                    format!("\\{} has no replacement text", self.name(name)),
                );
                self.prefixes = Default::default();
                return;
            }
            match self.next_token() {
                None => {
                    self.unread_all(&pattern);
                    self.prefixes = Default::default();
                    return;
                }
                Some(t) if t.is_cat(Catcode::Begin) => break,
                Some(t) => pattern.push(t),
            }
        }
        let parameter_text = ParameterText::from_tokens(&pattern);
        // tex.web § 477: an `\edef` body is scanned while it is expanded, and
        // ends at the `}` that balances what expansion leaves.  A conditional
        // satex cannot decide there takes its true arm, as in any text an
        // expansion makes.
        let outer = std::mem::replace(&mut self.scanner, crate::machine::Scanner::Defining);
        // The name's own text met in its `\edef` body is copied through.
        let target = std::mem::replace(&mut self.copy_target, expand.then_some((name, self.edef_depth + 1)));
        let mark = self.copies.len();
        let bodies = match expand {
            true => vec![self.scan_expanded_def_body()],
            false => vec![self.read_group_body()],
        };
        self.copy_target = target;
        let copies = self.copies.split_off(mark.min(self.copies.len()));
        self.scanner = outer;
        if global && self.global_defs() >= 0 {
            self.prefixes.global = true;
        }
        let global = self.prefixes.global;
        for body in bodies.into_iter().rev() {
            let meaning = self.make_macro(parameter_text.clone(), None, body);
            let splice = self.resolve_copies(name, &meaning, &copies, global);
            self.prefixes.global = global;
            self.define(name, meaning, by, DefMode::Declare, span, None);
            if let Some((at, len)) = splice {
                self.note_copied(name, at, len);
            }
        }
    }

    fn primitive_let(&mut self, by: Sym, span: Span, global: bool, future: bool) {
        let span = self.definition_site(span);
        let Some(name) = self.read_r_token() else {
            self.prefixes = Default::default();
            return;
        };
        let (source, held) = if future {
            // tex.web § 1221: the `futurelet` branch of `prefixed_command`
            // reads its two lookahead tokens with plain `get_token` — no
            // `scan_optional_equals`, so an `=` written after the target name
            // is itself the first lookahead token.
            let a = self.next_token();
            let b = self.next_token();
            // Unknown digits that may be none: the token looked at is one
            // more digit on one path and what follows them on the other,
            // each read again from the `\futurelet`.
            if let (Some(held), Some(more)) = (a, b)
                && more.tok == Tok::Cs(self.unknown_more)
            {
                let again = [Token::new(Tok::Cs(by), span), Token::new(Tok::Cs(name), span), held];
                let split = self.split_moving(by, span, |m, arm| {
                    match arm {
                        0 => {
                            m.unread(more);
                            m.unread(Token::new(Tok::Chr(crate::tex::UNKNOWN_DIGIT, Catcode::Other), more.span));
                        }
                        1 => {}
                        _ => return false,
                    }
                    m.unread_all(&again);
                    true
                });
                if split {
                    return;
                }
            }
            let b = b.map(|t| self.one_token(t));
            (b, a)
        } else {
            // tex.web § 1221: optional spaces, an optional `=`, and then at
            // most one more optional space.  `\let\@sptoken= ` makes a space
            // token the meaning, so skipping every space would take the token
            // behind it instead.
            self.skip_spaces();
            let mut token = self.next_token();
            if token.is_some_and(|t| t.is_char('=')) {
                token = self.next_token();
                if token.is_some_and(|t| t.is_space()) {
                    token = self.next_token();
                }
            }
            (token, None)
        };
        let Some(source) = source else { return };
        let meaning = match source.tok {
            Tok::Cs(src) => self.env.meaning(src),
            // tex.web § 1221: an active character is a reference into the
            // same `eqtb` region control sequences live in, so `\let` copies
            // whatever it currently means, not the character itself.
            Tok::Chr(c, Catcode::Active) => {
                let src = self.active_sym(c);
                self.env.meaning(src)
            }
            Tok::Chr(c, cat) => Meaning::Char(c, cat),
            Tok::Param(_) => Meaning::Unknown,
        };
        if future {
            self.unread(source);
            if let Some(h) = held {
                self.unread(h);
            }
        }
        if global && self.global_defs() >= 0 {
            self.prefixes.global = true;
        }
        // A `\chardef` constant keeps its number as its value, which `\let`
        // copies with the meaning (tex.web § 1221 copies the whole `eqtb`
        // entry).
        let constant = match (source.tok, &meaning) {
            (Tok::Cs(src), Meaning::Register(RegKind::Char | RegKind::MathChar, _)) => Some(self.env.value(src)),
            _ => None,
        };
        let global = self.prefixes.global;
        // tex.web § 1221 copies the primitive's `chr` code with it; satex
        // remembers which primitive the copy is, since it models some alike.
        let original = match source.tok {
            Tok::Cs(src) => Some(src),
            Tok::Chr(c, Catcode::Active) => Some(self.active_sym(c)),
            _ => None,
        };
        let copied = match (original, &meaning) {
            (Some(src), Meaning::Primitive(_)) => Some(self.env.identity(src)).filter(|&id| id != name),
            _ => None,
        };
        if let Some(watch) = &mut self.probe {
            watch.peek(name, source);
        }
        // `\let` copies the whole meaning: of a name a join left one of
        // several meanings, that it is one of them.
        let may = match source.tok {
            Tok::Cs(src) => self.env.slot(src).and_then(|b| b.may.clone()),
            _ => None,
        };
        let node = self.define(name, meaning, by, DefMode::Declare, span, None);
        if let Some(may) = may
            && let Some(mut binding) = self.env.slot(name).cloned()
        {
            binding.may = Some(may);
            let global = binding.global;
            self.env.set(name, binding, global);
        }
        match copied {
            Some(id) => self.env.aliases.insert(name, id),
            None => self.env.aliases.remove(&name),
        };
        if let Some(value) = constant {
            self.env.set_value(name, value, global);
        }
        if let Tok::Cs(src) = source.tok
            && let Some(reference) = self.reference(src, source.span) {
                self.out.graph.edge(node, reference, EdgeKind::DEFINED_BY);
            }
    }

    /// One step of expansion, as `\expandafter` asks for: an unexpandable
    /// token stays where it is rather than being obeyed (tex.web § 367).
    fn expand_once(&mut self) {
        let Some(token) = self.next_token() else { return };
        let cs = match token.tok {
            Tok::Cs(sym) => Some(sym),
            _ => self.active_cs(token),
        };
        match cs {
            Some(sym) if self.expandable(sym) && !self.read_noexpanded(token) => {
                self.unread(token);
                self.step();
            }
            // A name of unknown meaning may expand to anything: unknown
            // text, unless every meaning a join left it with is one that
            // does not expand.
            Some(sym)
                if matches!(self.env.meaning(sym), Meaning::Unknown)
                    && !self.is_unknown_marker(sym)
                    && !self.read_noexpanded(token)
                    && !self.env.slot(sym).and_then(|b| b.may.clone()).is_some_and(|may| may.iter().all(|m| !self.expandable_meaning(m))) =>
            {
                let unknown = self.one_of_texts(sym, token.span);
                self.unread(unknown);
            }
            _ => self.unread(token),
        }
    }

    /// What expanding a name of unknown meaning once gives: when a join
    /// left it one of several macros without parameters, a token that
    /// stands for one of their texts (it holds them as its meanings, and is
    /// unknown text to whatever looks at text); otherwise unknown text.
    pub(crate) fn one_of_texts(&mut self, sym: Sym, span: Span) -> Token {
        let may = self.env.slot(sym).and_then(|b| b.may.clone());
        let plain = may.as_ref().is_some_and(|may| {
            may.iter().all(|m| m.as_macro().is_some_and(|m| m.parameter_text.items.is_empty() && !m.parameter_text.brace_end))
        });
        let (Some(may), true) = (may, plain) else { return self.unknown_token(span) };
        // One name per set: the set it holds stays the one it was made
        // for, whatever the name it came from is given later.
        let alias = self.intern(&format!("{}one of:{}:{:p}", crate::machine::UNKNOWN_NAME, self.name(sym), std::rc::Rc::as_ptr(&may)));
        let mut binding = Binding::builtin(Meaning::Unknown);
        binding.may = Some(may);
        self.env.set(alias, binding, true);
        Token::new(Tok::Cs(alias), span)
    }

    /// Expand a gullet primitive to the characters it produces, and put them
    /// back for the caller to read.  Swallowing them instead turns the
    /// primitive into a token sink and lets a delimited argument scan run on
    /// past its delimiter.
    fn produce_text(&mut self, kind: TextOf, span: Span) {
        let mut at = span;
        let text = match kind {
            TextOf::JobName => self.job_name(),
            TextOf::Number | TextOf::Roman => match self.scan_number() {
                Some(value) if kind == TextOf::Roman => roman(value),
                Some(value) => value.to_string(),
                None => {
                    let digits = self.unknown_digits_token(span);
                    return self.unread(digits);
                }
            },
            TextOf::Detokenize => {
                let body = self.read_general_text();
                match self.text_known(&body) {
                    // Each character stands where it was written.
                    Some(text) if text.chars().count() == body.len() && body.iter().all(|t| t.cs().is_none()) => {
                        let tokens: Vec<Token> = text
                            .chars()
                            .zip(&body)
                            .map(|(c, t)| Token::new(Tok::Chr(c, if c == ' ' { Catcode::Space } else { Catcode::Other }), t.span))
                            .collect();
                        return self.unread_all(&tokens);
                    }
                    Some(text) => text,
                    None => return self.unread_unknown(span),
                }
            }
            TextOf::String | TextOf::Meaning | TextOf::The => {
                let Some(token) = self.next_token() else { return };
                // Unknown digits are characters of category 12, which
                // `\string` gives back as they are (tex.web § 465).
                if kind == TextOf::String && (token.tok == Tok::Cs(self.unknown_digits) || token.tok == Tok::Cs(self.unknown_more)) {
                    return self.unread(token);
                }
                // Unknown text, or a name whose escape character is unknown,
                // prints as something unknown.
                let unknown_meaning = match token.tok {
                    Tok::Cs(sym) => {
                        sym == self.unknown
                            || sym == self.unknown_digits
                            || sym == self.unknown_more
                            || self.is_unknown_name(sym)
                            || (kind == TextOf::Meaning && matches!(self.env.meaning(sym), Meaning::Unknown | Meaning::Char(crate::tex::UNKNOWN_CHAR, _)))
                    }
                    Tok::Chr(c, Catcode::Active) if kind == TextOf::Meaning => {
                        let sym = self.active_sym(c);
                        matches!(self.env.meaning(sym), Meaning::Unknown)
                    }
                    _ => false,
                };
                let escaped = matches!(
                    (kind, token.tok),
                    (TextOf::String, Tok::Cs(_)) | (TextOf::Meaning, Tok::Cs(_) | Tok::Chr(_, Catcode::Active))
                );
                if unknown_meaning || (escaped && self.escape_state().is_none()) {
                    return self.unread_unknown(span);
                }
                // The characters of a name stand where the name was written.
                if kind == TextOf::String {
                    at = token.span;
                }
                match (kind, token.tok) {
                    (TextOf::String, Tok::Cs(sym)) => {
                        // tex.web § 69: the escape character comes from
                        // `\escapechar`, and none is printed when it is
                        // outside the character range.
                        let escape = self.escape_char();
                        let name = self.name(sym).to_string();
                        let mut text: String = escape.into_iter().collect();
                        // tex.web § 263 (`sprint_cs`): the empty name prints
                        // as `\csname\endcsname`.
                        if name.is_empty() {
                            text.push_str("csname");
                            text.extend(escape);
                            text.push_str("endcsname");
                        }
                        text + &name
                    }
                    (TextOf::String, Tok::Chr(c, _)) => c.to_string(),
                    // An installed font's name prints with a size clause
                    // satex may not know (xetex.web `font_name_code`).
                    (TextOf::Meaning, Tok::Cs(sym))
                        if let Meaning::Primitive(Primitive::FontIdent(font)) = self.env.meaning(sym)
                            && self.font_name_known(font).is_none() =>
                    {
                        let mut tokens = self.text_tokens("select font ", at);
                        tokens.push(self.unknown_token(at));
                        return self.unread_all(&tokens);
                    }
                    (TextOf::Meaning, Tok::Cs(sym)) => self.meaning_text(sym),
                    (TextOf::Meaning, Tok::Chr(c, Catcode::Active)) => {
                        let sym = self.active_sym(c);
                        self.meaning_text(sym)
                    }
                    (TextOf::Meaning, Tok::Chr(c, cat)) => char_meaning_text(c, cat),
                    (TextOf::The, _) => {
                        self.unread(token);
                        let tokens = self.the_tokens(span);
                        self.unread_all(&tokens);
                        return;
                    }
                    _ => String::new(),
                }
            }
        };
        let tokens = self.text_tokens(&text, at);
        self.unread_all(&tokens);
    }

    /// What `\the` yields (tex.web § 465): a token register gives back its
    /// tokens themselves, anything else the characters of its value.
    pub fn the_tokens(&mut self, span: Span) -> Vec<Token> {
        // `the_toks` reads its operand with `get_x_token`, so
        // `\the\csname c@page\endcsname` reaches the register.
        let sym = loop {
            let Some(token) = self.next_token() else { return Vec::new() };
            let Some(sym) = token.cs().or_else(|| self.active_cs(token)) else {
                return Vec::new();
            };
            if !self.expandable(sym) {
                if let Some(p) = self.env.meaning(sym).prim()
                    && let Some(ident) = self.font_identifier(p, sym, span)
                {
                    return ident;
                }
                break sym;
            }
            self.unread(token);
            if !self.step() {
                return Vec::new();
            }
        };
        let level = self.internal_level(&self.env.meaning(sym));
        match self.internal_quantity(sym, span) {
            Some(Value::Toks(toks)) => toks.to_vec(),
            // An unknown dimension still prints as `⟨digits⟩.⟨digits⟩pt`
            // (tex.web § 103 `print_scaled` gives at least one digit after
            // the point), which is what `\def\getpt#1pt{…}` and
            // `#1.#2\relax` split it at.  The digits are unknown text that
            // holds no delimiter: whatever reads them is unknown too.
            Some(Value::Unknown) | None if matches!(level, Some(RegKind::Dimen | RegKind::Skip | RegKind::MuSkip)) => {
                let unit = if level == Some(RegKind::MuSkip) { "mu" } else { "pt" };
                let mut tokens = vec![self.unknown_digits_token(span)];
                tokens.extend(self.text_tokens(".", span));
                tokens.push(self.unknown_digits_token(span));
                tokens.extend(self.text_tokens(unit, span));
                tokens
            }
            Some(Value::Unknown) | None if level == Some(RegKind::Toks) => vec![self.unknown_token(span)],
            Some(Value::Unknown) | None if level.is_some() => vec![self.unknown_digits_token(span)],
            Some(value) => {
                let text = value.render();
                self.text_tokens(&text, span)
            }
            None => Vec::new(),
        }
    }

    /// Unknown text in place of what an expandable primitive yields.
    fn unread_unknown(&mut self, span: Span) {
        let token = self.unknown_token(span);
        self.unread(token);
    }

    /// `\escapechar` as the character it names, `Some(None)` when it names
    /// none, and `None` when it is unknown.
    pub(crate) fn escape_state(&mut self) -> Option<Option<char>> {
        let sym = self.intern("escapechar");
        self.env.value(sym).as_int().map(|code| u8::try_from(code).ok().map(char::from))
    }

    /// The character `\escapechar` names, if it names one.
    fn escape_char(&mut self) -> Option<char> {
        let sym = self.intern("escapechar");
        match self.env.value(sym).as_int() {
            // tex.web § 63: only a code in 0..=255 names a character.
            Some(code) => u8::try_from(code).ok().map(char::from),
            None => Some('\\'),
        }
    }

    /// `\meaning` writes `macro:⟨parameter text⟩->⟨replacement text⟩` for a
    /// macro, and the primitive's own name otherwise (The TeXbook, ch. 20).
    fn meaning_text(&mut self, sym: Sym) -> String {
        let escape = self.escape_char();
        let esc: String = escape.into_iter().collect();
        match self.env.meaning(sym) {
            // tex.web § 296 with the pdfTeX/eTeX `\protected` extension: the
            // prefixes a macro was defined with are part of its meaning and
            // print before the word `macro`, in the order protected, long,
            // outer.
            Meaning::Macro(m) => {
                let mut prefix = String::new();
                for (set, word) in
                    [(m.protected, "protected"), (m.long, "long"), (m.outer, "outer")]
                {
                    if set {
                        prefix.extend(escape);
                        prefix.push_str(word);
                    }
                }
                if !prefix.is_empty() {
                    prefix.push(' ');
                }
                // tex.web § 294: a parameter text ending in `#{` shows only
                // the `{`, which the replacement text repeats.
                let mut parameters = String::new();
                for item in &m.parameter_text.items {
                    match item {
                        crate::tex::ParamItem::Param(n) => parameters.push_str(&format!("#{n}")),
                        crate::tex::ParamItem::Lit(tok) => parameters.push_str(
                            &crate::tex::detokenize_in(
                                &[Token::new(*tok, Span::default())],
                                &self.out.interner,
                                escape,
                                &self.catcodes,
                            ),
                        ),
                    }
                }
                if m.parameter_text.brace_end {
                    parameters.push('{');
                }
                format!(
                    "{prefix}macro:{parameters}->{}",
                    crate::tex::detokenize_in(&m.replacement_text, &self.out.interner, escape, &self.catcodes)
                )
            }
            Meaning::Undefined => "undefined".into(),
            Meaning::Char(c, cat) => char_meaning_text(c, cat),
            Meaning::Register(kind, crate::tex::UNNUMBERED) => format!("{esc}{}", kind.as_str()),
            // tex.web § 1224: a `\chardef` shows as `\char"⟨hex⟩`.
            Meaning::Register(crate::tex::RegKind::Char, index) => format!("{esc}char\"{index:X}"),
            Meaning::Register(crate::tex::RegKind::MathChar, index) => match self.env.value(sym).as_int() {
                Some(code) if code > MATH_CODE_MAX => {
                    format!("{esc}Umathchar\"{:X}\"{:X}\"{:X}", code >> 21 & 7, code >> 24, code & 0x1f_ffff)
                }
                _ => format!("{esc}mathchar\"{index:X}"),
            },
            Meaning::Register(kind, index) => format!("{esc}{}{index}", kind.as_str()),
            // tex.web § 296: a primitive shows its own name, which is how
            // `\csname undefined\endcsname` reads back as `\relax`.
            Meaning::Primitive(Primitive::Relax) => format!("{esc}relax"),
            // tex.web § 296: a mark command shows a colon and the mark's
            // text, which is empty until a page is built.
            Meaning::Primitive(p @ Primitive::Convert(crate::builtins::Convert::Mark)) => {
                let name = primitive_name(self.out.plugins.engine, p);
                let original = self.env.identity(sym);
                format!("{esc}{}:", name.as_deref().unwrap_or_else(|| self.name(original)))
            }
            // tex.web § 1261: a font identifier shows the font it selects,
            // which expl3's `\__cctab_chk_if_valid_aux:NTF` looks for.
            Meaning::Primitive(Primitive::FontIdent(font)) => {
                format!("select font {}", self.font_name_text(font))
            }
            // tex.web § 1222: `\let` copies the primitive itself, whose
            // meaning still prints under its own name — pgfkeys tests
            // `\meaning` of its copy of `\expanded` for exactly that.
            Meaning::Primitive(p) => {
                let name = primitive_name(self.out.plugins.engine, p);
                let original = self.env.identity(sym);
                format!("{esc}{}", name.as_deref().unwrap_or_else(|| self.name(original)))
            }
            _ => format!("{esc}{}", self.name(sym)),
        }
    }

    fn job_name(&mut self) -> String {
        let path = self.out.file_name(self.out.main_file).to_string();
        std::path::Path::new(&path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("texput")
            .to_string()
    }

    /// Characters produced by the gullet are `other`, except the space.
    fn text_tokens(&mut self, text: &str, span: Span) -> Vec<Token> {
        text.chars()
            .map(|c| {
                let catcode = if c == ' ' { Catcode::Space } else { Catcode::Other };
                Token::new(Tok::Chr(c, catcode), span)
            })
            .collect()
    }

    /// `\csname` reads its name with `get_x_token`, so the scan stops at the
    /// `\endcsname` that expansion produces, not at the first one written
    /// down (tex.web § 372).  Reading the name unexpanded loses the
    /// `\expandafter\endcsname` idiom, which moves that `\endcsname` past
    /// whatever follows it.
    fn build_csname(&mut self, span: Span) {
        let (name, at) = self.scan_csname_at(span);
        // A name built from unknown text is one name satex cannot know,
        // known by the text around the unknown part.
        let name = match name {
            Ok(name) => name,
            Err((prefix, suffix)) => {
                let token = self.unknown_name_token(&prefix, &suffix, at);
                return self.unread(token);
            }
        };
        // A name with one unknown digit in it is one of ten names: the
        // token stands for whichever it is, holding one of their meanings
        // (an undefined one made `\relax`, tex.web § 372).
        if name.contains(crate::tex::UNKNOWN_DIGIT) {
            let token = self.one_of_names(&name, at);
            return self.unread(token);
        }
        self.last_named_cs = Some(self.intern(&name));
        crate::observe::csname(self, &name, span);
        let sym = self.intern(&name);
        if !self.env.is_defined(sym) {
            let binding = Binding::builtin(Meaning::Primitive(Primitive::Relax));
            self.env.set(sym, binding, false);
        }
        self.unread(Token::new(Tok::Cs(sym), at));
    }

    /// The token for a name `\csname` built with unknown digits in it:
    /// one of the names each digit makes, as a name a join left with one
    /// of their meanings.  Which of them `\csname` made `\relax` is not
    /// known, so each may have been.
    fn one_of_names(&mut self, name: &str, at: Span) -> Token {
        let mut candidates = vec![String::new()];
        for c in name.chars() {
            let options: Vec<char> = if c == crate::tex::UNKNOWN_DIGIT { ('0'..='9').collect() } else { vec![c] };
            candidates = candidates
                .iter()
                .flat_map(|p| options.iter().map(move |o| format!("{p}{o}")))
                .take(10_000)
                .collect();
        }
        let mut meanings: Vec<Meaning> = Vec::new();
        for candidate in &candidates {
            let sym = self.intern(candidate);
            let meaning = match self.env.meaning(sym) {
                Meaning::Undefined => {
                    let (prefix, suffix) = name.split_once(crate::tex::UNKNOWN_DIGIT).unwrap_or((name, ""));
                    self.wild.add(prefix.to_string(), suffix.to_string());
                    Meaning::Primitive(Primitive::Relax)
                }
                m => m,
            };
            if !meanings.contains(&meaning) {
                meanings.push(meaning);
            }
        }
        let sym = self.intern(name);
        let binding = if meanings.len() == 1 {
            Binding::builtin(meanings.pop().expect("one meaning"))
        } else {
            let mut b = Binding::builtin(Meaning::Unknown);
            // The set is exact, not a join: it is kept whole, up to the
            // ten names one digit makes.
            if meanings.len() <= 10 && !meanings.contains(&Meaning::Unknown) {
                b.may = Some(meanings.into());
            }
            b
        };
        self.env.set(sym, binding, true);
        Token::new(Tok::Cs(sym), at)
    }

    /// The name `\csname` and `\ifcsname` build: every character token up to
    /// `\endcsname`, whatever its category — a space or a brace included —
    /// read with `get_x_token` (tex.web § 372; e-TeX's `\ifcsname` scans the
    /// same way).  The first control sequence or active character expansion
    /// cannot remove ends the name, and TeX inserts the missing `\endcsname`
    /// there rather than reading on.
    fn scan_csname_text(&mut self, span: Span) -> Option<String> {
        self.scan_csname_at(span).0.ok()
    }

    /// The name, and where its first character was written: the place a
    /// name built from a file's text belongs to.  A name with unknown text
    /// in it is known only by the text before and after that.
    fn scan_csname_at(&mut self, span: Span) -> (Result<String, (String, String)>, Span) {
        let depth = crate::commands::enter_csname();
        let found = self.scan_csname_chars(span);
        crate::commands::leave_csname(depth);
        found
    }

    fn scan_csname_chars(&mut self, span: Span) -> (Result<String, (String, String)>, Span) {
        let mut name = String::new();
        // The text before the first unknown part; `name` then collects
        // what follows the last.
        let mut prefix: Option<String> = None;
        let mut at = None;
        let mut at_written = false;
        let mut closed = false;
        let mut stopped = None;
        while let Some(t) = self.next_token() {
            let cs = match t.tok {
                Tok::Chr(c, cat) if cat != Catcode::Active => {
                    name.push(c);
                    // A character the calling file wrote in this call names
                    // the place.
                    let written = self.file_call.is_some_and(|(_, call)| {
                        call.file == t.span.file && (t.span.line, t.span.col) >= (call.line, call.col)
                    });
                    if t.span.line > 0 && (at.is_none() || written && !at_written) {
                        at = Some(t.span);
                        at_written = written;
                    }
                    continue;
                }
                Tok::Param(_) => continue,
                _ => t.cs().or_else(|| self.active_cs(t)),
            };
            let Some(sym) = cs else { continue };
            if matches!(self.env.meaning(sym).prim(), Some(Primitive::Endcsname)) {
                closed = true;
                break;
            }
            if sym == self.unknown_digits || sym == self.unknown_more || matches!(self.env.meaning(sym), Meaning::Unknown) {
                prefix.get_or_insert_with(|| name.clone());
                name.clear();
                continue;
            }
            // tex.web § 370: expanding an undefined control sequence reports
            // it and removes it, so the name goes on after it.
            if matches!(self.env.meaning(sym), Meaning::Undefined) {
                let node = self.reference(sym, t.span);
                self.note_expansion(sym, t.span, crate::facts::MeaningKind::Undefined, node, Vec::new());
                self.record(Step::Undefined, sym, t.span);
                continue;
            }
            self.unread(t);
            if !self.expandable(sym) || !self.step() {
                stopped = Some(sym);
                break;
            }
        }
        if !closed {
            let why = stopped.map_or_else(
                || "the input ends".to_string(),
                |sym| format!("\\{} ({}) cannot be expanded", self.name(sym), self.env.meaning(sym).kind()),
            );
            self.diagnose(
                Severity::Unsupported,
                "missing-endcsname",
                span,
                format!("\\csname has no \\endcsname: {why} after `{name}`"),
            );
        }
        let name = match prefix {
            None => Ok(name),
            Some(prefix) => Err((prefix, name)),
        };
        (name, at.unwrap_or(span))
    }

    /// A conditional is read one token at a time: the arm that is not taken
    /// is skipped by `pass_text`, and `\else`/`\fi` stay in the stream, so a
    /// macro whose parameter text is delimited by `\fi` still sees it
    /// (tex.web §§ 489-500).  An undecided condition runs every arm in turn.
    fn conditional(&mut self, cond: Cond, by: Sym, span: Span, negate: bool) {
        let command = std::mem::take(&mut self.command_level);
        // `satex controls` asks what a switch governs, which only has an
        // answer if the switch is left undecided.
        let opaque = self.cfg.opaque_conditionals.iter().any(|wanted| {
            wanted == self.name(by)
                // Only the document's own switches: the kernel's and the
                // packages' drive code that was never meant to run both ways.
                || (wanted == crate::config::EVERY_SWITCH && self.own_switches.contains(&by) && self.is_switch(by))
        });
        let kind = self.if_type(cond, by, negate);
        let cond = if opaque { Cond::Opaque } else { cond };
        if cond == Cond::Case {
            self.testing.push(self.conds.len());
            self.testing_kind.push(kind);
            self.abs = None;
            let selector = self.scan_number();
            // A selector of unknown value whose interval lies in one case
            // range decides it: every value negative (the `\else` text,
            // as -1 selects it) or one value; otherwise only the arms in
            // its interval are split.
            let range = selector.map(Num::exact).or_else(|| self.abs.take());
            self.testing_kind.pop();
            self.testing.pop();
            let decided = match &range {
                Some(num) if num.hi < 0 => Some(-1),
                Some(num) if num.lo == num.hi => Some(num.lo),
                _ => None,
            };
            let selector = selector.or(decided);
            if selector.is_none() && command && self.edef_depth == 0 && self.split_case(by, span, kind, range) {
                return;
            }
            return self.case_conditional(selector, by, span, kind);
        }
        if cond == Cond::IfX && command && !negate && self.edef_depth == 0 && self.split_ifx(by, span) {
            return;
        }
        let mut operands = vec![by];
        // tex.web § 498: the conditional is on the condition stack while its
        // test is read, below any conditional the test itself starts.
        let base = self.conds.len();
        self.testing.push(base);
        self.testing_kind.push(kind);
        self.narrowing = None;
        let decided = self.eval_condition(cond, &mut operands);
        let narrowing = self.narrowing.take();
        self.testing_kind.pop();
        self.testing.pop();
        // A test read straight from a file depends on what it compares.
        if self.at_top_level() {
            let node = self.force_reference(by, span);
            for sym in operands {
                self.reads_definition_of(node, sym);
            }
        }
        let taken = match (decided, negate) {
            (Some(v), true) => Some(!v),
            (v, _) => v,
        };
        // A conditional left open on request (`satex controls`) only needs
        // what each arm does, not a path per arm: splitting every switch
        // there multiplies the paths through every loop that tests one.
        // A mode test splits the modes between its arms.
        let refine = self.mode_parts(cond).map(|(yes, no)| if negate { (no, yes) } else { (yes, no) });
        // What the test teaches each arm of the register it read.
        let narrow = narrowing.map(|(sym, dimen, num, relation, bound)| {
            let arm = |holds: bool| num.narrow(relation, bound, holds != negate).map(|n| Value::from_num(dimen, n));
            (sym, arm(true), arm(false))
        });
        if taken.is_none() && command && self.edef_depth == 0 && self.split_conditional(by, span, kind, refine, narrow) {
            return;
        }
        self.choose_branch(taken, by, span, base, kind);
    }

    /// `\ifx` read by the main loop with an operand that holds one of
    /// several meanings (a join left it so): a path for each meaning, on
    /// which the operand has that meaning and the `\ifx` is read again, so
    /// that each is compared on its own.
    fn split_ifx(&mut self, by: Sym, span: Span) -> bool {
        if !self.may_split_meaning() {
            return false;
        }
        let Some(a) = self.next_token() else { return false };
        let marked = self.read_noexpanded(a);
        let Some(b) = self.next_token() else {
            self.unread(a);
            return false;
        };
        self.unread(b);
        // Reading `b` forgot that `a` stood behind a `\noexpand` marker.
        if marked {
            self.noexpanded = Some(a);
        }
        self.unread(a);
        let Some((sym, at, may)) = [a, b].into_iter().find_map(|t| {
            let sym = t.cs().or_else(|| self.active_cs(t))?;
            Some((sym, t.span, self.env.slot(sym)?.may.clone()?))
        }) else {
            return false;
        };
        let test = Token::new(Tok::Cs(by), span);
        self.split_moving(sym, at, |m, arm| {
            let Some(meaning) = may.get(arm) else { return false };
            m.env.refine(sym, meaning.clone());
            m.unread(test);
            true
        })
    }

    /// e-TeX's `\currentiftype` of a conditional (etex.ch: `cur_if + 1`,
    /// negated after `\unless`); `None` where satex models several alike.
    fn if_type(&mut self, cond: Cond, by: Sym, negate: bool) -> Option<i8> {
        let code = match cond {
            Cond::Chars => 1,
            Cond::Catcodes => 2,
            Cond::Num => 3,
            Cond::Dim => 4,
            Cond::Odd => 5,
            Cond::VMode => 6,
            Cond::HMode => 7,
            Cond::MathMode => 8,
            Cond::InnerMode => 9,
            Cond::Box => match self.name(self.env.identity(by)) {
                "ifvoid" => 10,
                "ifhbox" => 11,
                "ifvbox" => 12,
                _ => return None,
            },
            Cond::IfX => 13,
            Cond::Eof => 14,
            Cond::True => 15,
            Cond::False => 16,
            Cond::Case => 17,
            Cond::Defined => 18,
            Cond::Csname => 19,
            Cond::FontChar => 20,
            Cond::InCsname => 21,
            Cond::Primitive => 22,
            Cond::AbsNum => 23,
            Cond::AbsDim => 24,
            Cond::Opaque => return None,
        };
        Some(if negate { -code } else { code })
    }

    /// Run every arm of an undecided `\ifcase` as a path of its own, and
    /// without an `\else`, the outcome where none runs (tex.web § 509).
    /// With the selector's interval `range`, arms below it are not run, and
    /// past it only the `\else` text (or none), when a negative selector
    /// may choose it.
    fn split_case(&mut self, by: Sym, span: Span, kind: Option<i8>, range: Option<Num>) -> bool {
        let on = self.force_reference(by, span);
        self.record_with(Step::Condition, by, span, || Some("undecided"));
        self.note_conditional(by, span, None);
        let (lo, hi) = range.map_or((0, i64::MAX), |n| (n.lo, n.hi));
        let first = lo.max(0) as usize;
        self.run_paths(by, span, |m, arm| {
            let arm = first + arm;
            let mut limit = CondLimit::Or;
            if arm as i64 > hi {
                // Past the interval: the `\else` text, reached only by a
                // negative selector, and only once.
                if lo >= 0 || arm as i64 > hi.saturating_add(1) {
                    return false;
                }
                loop {
                    match m.pass_arm(false, span) {
                        Some(Primitive::Or) => {}
                        Some(Primitive::Else) => break limit = CondLimit::Fi,
                        Some(Primitive::Fi) => return true,
                        _ => return false,
                    }
                }
            }
            for skipped in 0..arm {
                if limit == CondLimit::Fi {
                    break;
                }
                let last = skipped + 1 == arm;
                match m.pass_arm(false, span) {
                    Some(Primitive::Or) => {}
                    Some(Primitive::Else) if last => limit = CondLimit::Fi,
                    Some(Primitive::Fi) if last => return true,
                    _ => return false,
                }
            }
            m.cds.push(ControlDep { on, taken: arm == 0 });
            m.conds.push(CondFrame { limit, undecided: false, dep: true, at: span, kind, modes: None });
            true
        })
    }

    /// The marker `pass_text` leaves where a skip ran into another file's
    /// text (SEMANTICS.md § 14): one path goes on from here, the conditional
    /// ended; the other skips on as TeX does, through `nested` inner levels,
    /// and at the level's `\else` runs the false text.
    fn skip_crossing(&mut self, nested: u8, to_fi: bool, by: Sym, span: Span) {
        let command = std::mem::take(&mut self.command_level);
        if !command || self.edef_depth > 0 {
            return;
        }
        self.run_paths(by, span, |m, arm| match arm {
            0 => true,
            1 => {
                let mut open = nested;
                while let Some((end, _)) = m.pass_text(to_fi, span) {
                    match end {
                        Primitive::Fi if open == 0 => break,
                        Primitive::Fi => open -= 1,
                        _ if open > 0 => {}
                        _ => {
                            let frame = CondFrame { limit: CondLimit::Fi, undecided: false, dep: false, at: span, kind: None, modes: None };
                            m.conds.push(frame);
                            break;
                        }
                    }
                }
                true
            }
            _ => false,
        });
    }

    /// Run the two arms of an undecided `\if…\fi` as paths of their own
    /// (see [`Machine::run_paths`]).
    fn split_conditional(
        &mut self,
        by: Sym,
        span: Span,
        kind: Option<i8>,
        refine: Option<(crate::mode::Modes, crate::mode::Modes)>,
        narrow: Option<(Sym, Option<Value>, Option<Value>)>,
    ) -> bool {
        let on = self.force_reference(by, span);
        self.record_with(Step::Condition, by, span, || Some("undecided"));
        self.note_conditional(by, span, None);
        self.run_paths(by, span, |m, arm| match arm {
            0 => {
                if let Some((yes, _)) = refine {
                    m.mode = yes;
                }
                if let Some((sym, Some(value), _)) = &narrow {
                    m.env.refine_value(*sym, value.clone());
                }
                m.cds.push(ControlDep { on, taken: true });
                m.conds.push(CondFrame { limit: CondLimit::Else, undecided: false, dep: true, at: span, kind, modes: None });
                true
            }
            1 => {
                if let Some((_, no)) = refine {
                    m.mode = no;
                }
                if let Some((sym, _, Some(value))) = &narrow {
                    m.env.refine_value(*sym, value.clone());
                }
                if let Some(Primitive::Else | Primitive::Or) = m.pass_arm(false, span) {
                    m.cds.push(ControlDep { on, taken: false });
                    m.conds.push(CondFrame { limit: CondLimit::Fi, undecided: false, dep: true, at: span, kind, modes: None });
                }
                true
            }
            _ => false,
        })
    }

    /// Enter the arm a condition selected, or, when it selected none, the
    /// arms satex must analyze instead (tex.web § 498).
    ///
    /// `base` is the depth of the condition stack when the test began: a
    /// conditional the test started and left open (`\if e\ifx AB x\else y\fi`)
    /// sits above this one and ends first (tex.web § 500).
    pub fn choose_branch(&mut self, taken: Option<bool>, by: Sym, span: Span, base: usize, kind: Option<i8>) {
        let base = base.min(self.conds.len());
        match taken {
            Some(true) => {
                self.record_with(Step::Branch, by, span, || Some("true"));
                self.note_conditional(by, span, Some(Some(0)));
                self.conds.insert(base, CondFrame { limit: CondLimit::Else, undecided: false, dep: false, at: span, kind, modes: None });
            }
            Some(false) => {
                self.record_with(Step::Branch, by, span, || Some("false"));
                // tex.web § 500: skipping, a `\fi` ends an inner conditional
                // still open, and an `\else` of one is passed over.
                loop {
                    match self.pass_text(false, span) {
                        Some((Primitive::Fi, mark)) if self.conds.len() > base => {
                            if let Some(inner) = self.conds.last() {
                                self.note_cond_mark(inner.at, mark, true);
                            }
                            self.pop_conditional()
                        }
                        Some((Primitive::Else | Primitive::Or, _)) if self.conds.len() > base => {}
                        Some((Primitive::Else | Primitive::Or, mark)) => {
                            self.note_conditional(by, span, Some(Some(1)));
                            self.note_cond_mark(span, mark, false);
                            self.conds.push(CondFrame { limit: CondLimit::Fi, undecided: false, dep: false, at: span, kind, modes: None });
                            break;
                        }
                        found => {
                            self.note_conditional(by, span, Some(None));
                            if let Some((_, mark)) = found {
                                self.note_cond_mark(span, mark, true);
                            }
                            self.close_conditional(span);
                            break;
                        }
                    }
                }
            }
            // Inside an `\edef` body the arms are text, and analyzing both
            // would read past the body; the true arm is read after unknown
            // text, which makes the result unknown.
            None if self.edef_depth > 0 => {
                self.diagnose(
                    Severity::Imprecision,
                    "undecided-condition",
                    span,
                    format!(
                        "\\{} could not be decided inside an \\edef body; its text is unknown",
                        self.name(by)
                    ),
                );
                self.record_with(Step::Branch, by, span, || Some("true"));
                self.note_conditional(by, span, None);
                self.conds.insert(base, CondFrame { limit: CondLimit::Else, undecided: false, dep: false, at: span, kind, modes: None });
                self.unread_unknown(span);
            }
            None if self.scanning > 0 => self.first_arm(by, span, CondLimit::Else, kind),
            None => self.undecided_conditional(by, span, kind),
        }
    }

    /// An undecided condition met while a value is scanned: its arms are
    /// operands, so the first is read and the value is marked unknown.
    fn first_arm(&mut self, by: Sym, span: Span, limit: CondLimit, kind: Option<i8>) {
        if self.scanning > 0 {
            self.scan_undecided = true;
        } else {
            self.unread_unknown(span);
        }
        self.diagnose(
            Severity::Imprecision,
            "undecided-condition",
            span,
            format!("\\{} could not be decided inside a value; its first arm was read", self.name(by)),
        );
        self.record_with(Step::Branch, by, span, || Some("first"));
        self.note_conditional(by, span, None);
        self.conds.push(CondFrame { limit, undecided: false, dep: false, at: span, kind, modes: None });
    }

    /// `\ifcase`: skip to the selected arm, or run them all when the number
    /// is not known (tex.web § 509).
    fn case_conditional(&mut self, selector: Option<i64>, by: Sym, span: Span, kind: Option<i8>) {
        let Some(n) = selector else {
            if self.scanning > 0 || self.edef_depth > 0 {
                return self.first_arm(by, span, CondLimit::Or, kind);
            }
            return self.undecided_conditional(by, span, kind);
        };
        self.record_with(Step::Branch, by, span, || Some(format!("case {n}")));
        // A negative selector matches no arm, so the `\else` text runs.
        let mut remaining = if n < 0 { i64::MAX } else { n };
        let mut marks = Vec::new();
        let limit = loop {
            if remaining == 0 {
                break Some(CondLimit::Or);
            }
            match self.pass_text(false, span) {
                Some((Primitive::Or, mark)) => {
                    remaining -= 1;
                    marks.push((mark, false));
                }
                Some((Primitive::Else, mark)) => {
                    marks.push((mark, false));
                    break Some(CondLimit::Fi);
                }
                found => {
                    marks.extend(found.map(|(_, mark)| (mark, true)));
                    break None;
                }
            }
        };
        let arms = marks.iter().filter(|(_, fi)| !fi).count() as u32;
        self.note_conditional(by, span, Some(limit.map(|_| arms)));
        for (mark, fi) in marks {
            self.note_cond_mark(span, mark, fi);
        }
        let Some(limit) = limit else { return };
        self.conds.push(CondFrame { limit, undecided: false, dep: false, at: span, kind, modes: None });
    }

    /// Over-approximation: every arm is analyzed, one after the other, and
    /// what each does is recorded as depending on this condition.
    fn undecided_conditional(&mut self, by: Sym, span: Span, kind: Option<i8>) {
        let node = self.force_reference(by, span);
        self.record_with(Step::Condition, by, span, || Some("undecided"));
        self.note_conditional(by, span, None);
        self.diagnose(
            Severity::Imprecision,
            "undecided-condition",
            span,
            format!("\\{} could not be decided; all branches analyzed", self.name(by)),
        );
        self.cds.push(ControlDep { on: node, taken: true });
        let entry = (self.mode, self.list);
        self.conds.push(CondFrame {
            limit: CondLimit::Else,
            undecided: true,
            dep: true,
            at: span,
            kind,
            modes: Some(crate::mode::ArmModes { entry, seen: Default::default(), other: false, size: self.natural, sizes: None }),
        });
    }

    /// tex.web § 510: an `\else`, `\or` or `\fi` met while the test of its
    /// own conditional is still being read — as the end of a number in
    /// `\ifnum…>20\else` — is not acted on yet; a `\relax` that no
    /// definition can change goes in front of it and ends the test.
    fn insert_relax(&mut self, by: Sym, span: Span) {
        let relax = self.intern("\u{4}primitive.relax");
        self.env.set(relax, Binding::builtin(Meaning::Primitive(Primitive::Relax)), true);
        self.unread(Token::new(Tok::Cs(by), span));
        self.unread(Token::new(Tok::Cs(relax), span));
    }

    /// `\else` and `\or`: the taken arm is over.
    fn conditional_else(&mut self, span: Span, is_else: bool) {
        if let Some(frame) = self.conds.last() {
            self.note_cond_mark(frame.at, span, false);
        }
        match self.conds.last_mut() {
            None => self.diagnose(
                Severity::Unsupported,
                "extra-else",
                span,
                "\\else or \\or outside a conditional".into(),
            ),
            Some(frame) if frame.undecided => {
                frame.limit = CondLimit::Fi;
                // The arms run one after the other; each starts in the modes
                // the test left, and the modes after `\fi` are those any arm
                // left.
                if let Some(arms) = &mut frame.modes {
                    arms.seen = (arms.seen.0.union(self.mode), arms.seen.1.union(self.list));
                    arms.other |= is_else;
                    (self.mode, self.list) = arms.entry;
                    let left = std::mem::replace(&mut self.natural, arms.size);
                    arms.sizes = Some(arms.sizes.map_or(left, |s| crate::mode::join_natural(s, left)));
                }
                if frame.dep && let Some(dep) = self.cds.last_mut() {
                    dep.taken = false;
                }
            }
            Some(frame) => {
                let at = frame.at;
                if let Some((_, fi)) = self.pass_text(true, span) {
                    self.note_cond_mark(at, fi, true);
                }
                self.pop_conditional();
            }
        }
    }

    /// `\fi` ends the level.
    fn conditional_fi(&mut self, span: Span) {
        if self.conds.is_empty() {
            self.diagnose(
                Severity::Unsupported,
                "extra-fi",
                span,
                "\\fi outside a conditional".into(),
            );
            return;
        }
        if let Some(frame) = self.conds.last() {
            self.note_cond_mark(frame.at, span, true);
        }
        self.pop_conditional();
    }

    fn pop_conditional(&mut self) {
        if let Some(frame) = self.conds.pop() {
            if let Some(arms) = frame.modes {
                // Without an `\else` the condition may select no arm.
                let (mode, list) = if arms.other { arms.seen } else { (arms.seen.0.union(arms.entry.0), arms.seen.1.union(arms.entry.1)) };
                self.mode = self.mode.union(mode);
                self.list = self.list.union(list);
                let sizes = arms.sizes.map_or(self.natural, |s| crate::mode::join_natural(s, self.natural));
                self.natural = if arms.other { sizes } else { crate::mode::join_natural(sizes, arms.size) };
            }
            self.close_conditional(frame.at);
            if frame.dep {
                self.cds.pop();
            }
        }
    }

    /// A conditional over several lines of a file is one construct, from
    /// the `\if…` to its `\fi`.
    fn close_conditional(&mut self, at: Span) {
        if let Some(line) = self.source_extent(at) {
            self.out.graph.note_extent(at, line);
        }
    }

    /// One operand of `\if` or `\ifcat`, as `get_x_token_or_active_char`
    /// delivers it: the next token, expanded, as a (character, category) pair
    /// in which a control sequence counts as character 256 of a category no
    /// character has (tex.web § 506).  `\noexpand` keeps the token after it
    /// from being expanded, which is how `\ifcat\noexpand~\noexpand#1` asks
    /// whether an argument is an active character.
    fn if_operand(&mut self) -> Option<(u32, u8)> {
        /// tex.web § 506: `m := relax; n := 256`.
        const CONTROL_SEQUENCE: (u32, u8) = (256, 16);
        loop {
            let token = self.next_token()?;
            // tex.web § 506: a token behind a `\noexpand` marker is taken
            // as it stands, an active character as that character.
            if self.read_noexpanded(token) {
                return Some(match token.tok {
                    Tok::Chr(c, cat) => (c as u32, cat as u8),
                    Tok::Cs(inner) => match self.env.meaning(inner) {
                        Meaning::Char(c, cat) => (c as u32, cat as u8),
                        _ => CONTROL_SEQUENCE,
                    },
                    _ => CONTROL_SEQUENCE,
                });
            }
            // tex.web § 506: an active character that is not behind a
            // `\noexpand` is taken by its meaning, as a control sequence is.
            let Some(sym) = token.cs().or_else(|| self.active_cs(token)) else {
                return Some(match token.tok {
                    Tok::Chr(c, cat) => (c as u32, cat as u8),
                    _ => CONTROL_SEQUENCE,
                });
            };
            match self.env.meaning(sym) {
                // One of several meanings: the operand every one of them
                // gives, when none expands and they agree.
                Meaning::Unknown => {
                    let may = self.env.slot(sym).and_then(|b| b.may.clone());
                    let operand = |m: &Meaning| match m {
                        Meaning::Char(c, cat) => Some((*c as u32, *cat as u8)),
                        Meaning::Unknown | Meaning::Undefined => None,
                        m if self.expandable_meaning(m) => None,
                        _ => Some(CONTROL_SEQUENCE),
                    };
                    let agreed = may.and_then(|may| {
                        let first = operand(may.first()?)?;
                        may.iter().all(|m| operand(m) == Some(first)).then_some(first)
                    });
                    return Some(agreed.unwrap_or(UNKNOWN_OPERAND));
                }
                Meaning::Primitive(Primitive::NoExpand) => {
                    let next = self.noexpanded_next()?;
                    return Some(match next.tok {
                        Tok::Chr(c, cat) => (c as u32, cat as u8),
                        Tok::Cs(inner) => match self.env.meaning(inner) {
                            Meaning::Char(c, cat) => (c as u32, cat as u8),
                            _ => CONTROL_SEQUENCE,
                        },
                        _ => CONTROL_SEQUENCE,
                    });
                }
                // `\let\bgroup={` makes the control sequence that character.
                Meaning::Char(c, cat) => return Some((c as u32, cat as u8)),
                meaning if self.expandable_meaning(&meaning) => {
                    self.unread(token);
                    if !self.step() {
                        return None;
                    }
                }
                _ => return Some(CONTROL_SEQUENCE),
            }
        }
    }

    /// `operands` collects the names the test compares.  tex.web § 495
    /// pushes the conditional before its test is read, so
    /// `\currentiflevel` counts it while the test runs.
    pub fn eval_condition(&mut self, cond: Cond, operands: &mut Vec<Sym>) -> Option<bool> {
        let depth = crate::commands::enter_test();
        let result = self.eval_test(cond, operands);
        crate::commands::leave_test(depth);
        result
    }

    fn eval_test(&mut self, cond: Cond, operands: &mut Vec<Sym>) -> Option<bool> {
        match cond {
            Cond::True => Some(true),
            Cond::False => Some(false),
            Cond::Eof => {
                let stream = self.scan_number().unwrap_or(-1);
                Some(self.stream_at_end(stream))
            }
            // tex.web § 505: a register no `\setbox` has filled is void.
            Cond::Box => {
                let n = self.scan_number()?;
                let state = self.box_state(n);
                if state == crate::mode::BoxState::Unknown {
                    return None;
                }
                Some(match self.name(self.env.identity(operands[0])) {
                    "ifvoid" => state == crate::mode::BoxState::Void,
                    "ifhbox" => state == crate::mode::BoxState::HBox,
                    _ => state == crate::mode::BoxState::VBox,
                })
            }
            Cond::AbsNum | Cond::AbsDim | Cond::FontChar | Cond::InCsname | Cond::Primitive => {
                self.eval_extra_condition(cond)
            }
            // tex.web § 501; see [`crate::mode`].
            Cond::VMode | Cond::HMode | Cond::InnerMode | Cond::MathMode => self.mode_test(cond),
            Cond::Opaque | Cond::Dim => {
                if cond == Cond::Dim {
                    self.abs_from = None;
                    let a = self.scan_dimen();
                    let xs = a.map(Num::exact).or_else(|| self.abs.take());
                    let from = self.abs_from.take();
                    let relation = self.scan_relation();
                    let b = self.scan_dimen();
                    let ys = b.map(Num::exact).or_else(|| self.abs.take());
                    let from_b = self.abs_from.take();
                    return self.decide_numbers(xs, relation, ys, (a, b), (from, from_b));
                }
                None
            }
            // A number read from a register a join left one of a few
            // values is compared member by member: the test is decided
            // when every member decides it alike.
            Cond::Num | Cond::Odd => {
                self.abs_from = None;
                let a = self.scan_number();
                let xs = a.map(Num::exact).or_else(|| self.abs.take());
                let from = self.abs_from.take();
                if cond == Cond::Odd {
                    return xs?.odd();
                }
                let relation = self.scan_relation();
                let b = self.scan_number();
                let ys = b.map(Num::exact).or_else(|| self.abs.take());
                let from_b = self.abs_from.take();
                self.decide_numbers(xs, relation, ys, (a, b), (from, from_b))
            }
            Cond::IfX => {
                // Read untracked: what of each meaning counts as read is
                // decided below.
                let a = self.next_token()?;
                let tracking = self.env.track_reads(false);
                let ma = self.ifx_meaning(a);
                self.env.track_reads(tracking);
                let b = self.next_token()?;
                let tracking = self.env.track_reads(false);
                let mb = self.ifx_meaning(b);
                self.env.track_reads(tracking);
                // A macro compared with anything else differs whatever its
                // text: only that it is a macro is read.
                if let Some(watch) = &mut self.probe {
                    watch.compared(a, &mb);
                    watch.compared(b, &ma);
                }
                let one_macro = ma.as_macro().is_some() != mb.as_macro().is_some();
                let kind = if one_macro { crate::env::ReadKind::Shape } else { crate::env::ReadKind::Meaning };
                for t in [a, b] {
                    let sym = t.cs().or_else(|| self.active_cs(t));
                    if let Some(sym) = sym {
                        self.env.note_read(sym, kind);
                    }
                    operands.extend(sym);
                }
                // A macro with unknown text in it may equal another macro.
                let vague = |m: &Meaning, me: &Self| {
                    !one_macro && m.as_macro().is_some_and(|m| me.has_unknown(&m.replacement_text))
                };
                let same_macro = matches!((&ma, &mb), (Meaning::Macro(x), Meaning::Macro(y)) if Rc::ptr_eq(x, y));
                // Macros whose texts cannot be as long as each other differ
                // whatever their unknown parts stand for.
                let lengths_differ = match (&ma, &mb) {
                    (Meaning::Macro(x), Meaning::Macro(y)) => {
                        let ((xmin, xmax), (ymin, ymax)) = (self.text_length(&x.replacement_text), self.text_length(&y.replacement_text));
                        xmax.is_some_and(|m| m < ymin) || ymax.is_some_and(|m| m < xmin)
                    }
                    _ => false,
                };
                let null = Meaning::Primitive(Primitive::FontIdent(0));
                let font_open = (self.font_may_be_null(&ma) && (mb == null || self.font_may_be_null(&mb)))
                    || (self.font_may_be_null(&mb) && ma == null);
                let (sa, sb) = (a.cs().or_else(|| self.active_cs(a)), b.cs().or_else(|| self.active_cs(b)));
                let open = |m: &Meaning, sym: Option<Sym>| {
                    matches!(m, Meaning::Unknown)
                        || (matches!(m, Meaning::Undefined) && sym.is_some_and(|sym| self.may_be_defined(sym)))
                };
                // A character whose code is unknown equals a character of its
                // category only perhaps, and nothing else.
                let char_open = matches!((&ma, &mb), (Meaning::Char(x, k), Meaning::Char(y, l))
                    if k == l && crate::tex::may_equal_unknown_char(*x as u32, *y as u32));
                // A name that holds one of several meanings is compared
                // member by member: decided when they all agree.
                let members = |m: &Meaning, sym: Option<Sym>, me: &Self| -> Vec<Meaning> {
                    match (m, sym.and_then(|s| me.env.slot(s)?.may.clone())) {
                        (Meaning::Unknown, Some(may)) => may.to_vec(),
                        _ => vec![m.clone()],
                    }
                };
                let (xs, ys) = (members(&ma, sa, self), members(&mb, sb, self));
                if xs.len() > 1 || ys.len() > 1 {
                    let pair = |x: &Meaning, y: &Meaning, me: &Self| -> Option<bool> {
                        match (x, y) {
                            (Meaning::Unknown | Meaning::Undefined, _) | (_, Meaning::Unknown | Meaning::Undefined) => None,
                            (Meaning::Macro(p), Meaning::Macro(q)) => {
                                if Rc::ptr_eq(p, q) {
                                    return Some(true);
                                }
                                let (pu, qu) = (me.has_unknown(&p.replacement_text), me.has_unknown(&q.replacement_text));
                                if !pu && !qu {
                                    return Some(x == y);
                                }
                                let ((pmin, pmax), (qmin, qmax)) = (me.text_length(&p.replacement_text), me.text_length(&q.replacement_text));
                                (pmax.is_some_and(|m| m < qmin) || qmax.is_some_and(|m| m < pmin)).then_some(false)
                            }
                            // A macro is never a character or a primitive.
                            (Meaning::Macro(_), _) | (_, Meaning::Macro(_)) => Some(false),
                            (Meaning::Char(c, k), Meaning::Char(d, l)) if k == l && crate::tex::may_equal_unknown_char(*c as u32, *d as u32) => None,
                            _ => Some(x == y),
                        }
                    };
                    let answers: Option<Vec<bool>> = xs.iter().flat_map(|x| ys.iter().map(move |y| (x, y))).map(|(x, y)| pair(x, y, self)).collect();
                    return answers.and_then(|a| alike(a.into_iter()));
                }
                if open(&ma, sa) || open(&mb, sb) || font_open || char_open || (!same_macro && !lengths_differ && (vague(&ma, self) || vague(&mb, self))) {
                    None
                } else if let (Meaning::Primitive(p), true) = (&ma, ma == mb)
                    && *p != Primitive::DontExpand
                    && primitive_name(self.out.plugins.engine, *p).is_none()
                {
                    // Primitives satex models alike are still different
                    // primitives (tex.web § 507 compares `chr` codes).
                    let p = *p;
                    let engine = self.out.plugins.engine;
                    let original = |m: &mut Self, t: Token| {
                        t.cs()
                            .or_else(|| m.active_cs(t))
                            .map(|sym| m.env.identity(sym))
                            .filter(|&sym| is_primitive_named(engine, m.name(sym), p))
                    };
                    match (original(self, a), original(self, b)) {
                        (Some(x), Some(y)) => Some(x == y),
                        _ => Some(true),
                    }
                } else {
                    Some(ma == mb)
                }
            }
            Cond::Defined => {
                let t = self.next_token()?;
                let sym = t.cs().or_else(|| self.active_cs(t));
                operands.extend(sym);
                if sym == Some(self.unknown) || sym == Some(self.unknown_rest) || sym.is_some_and(|s| self.is_unknown_name(s)) {
                    return None;
                }
                // A name a join left one of several meanings is defined
                // when every one of them is, undefined when none is.
                if let Some(may) = sym.and_then(|sym| self.env.slot(sym)?.may.clone()) {
                    self.env.note_read(sym.unwrap(), crate::env::ReadKind::Defined);
                    let defined = alike(may.iter().map(|m| !matches!(m, Meaning::Undefined)));
                    return defined.filter(|defined| *defined || !self.may_be_defined(sym.unwrap()));
                }
                sym.map(|sym| self.env.is_defined(sym)).filter(|defined| *defined || !self.may_be_defined(sym.unwrap()))
            }
            Cond::Csname => {
                let at = self.peek().map(|t| t.span).unwrap_or_default();
                let name = self.scan_csname_text(at)?;
                crate::observe::read_name(self, &name);
                let sym = self.intern(&name);
                self.last_named_cs = Some(sym);
                operands.push(sym);
                Some(self.env.is_defined(sym)).filter(|defined| *defined || !self.may_be_defined(sym))
            }
            Cond::Chars | Cond::Catcodes => {
                let a = self.if_operand()?;
                let b = self.if_operand()?;
                if a == UNKNOWN_OPERAND || b == UNKNOWN_OPERAND {
                    return None;
                }
                let ((ca, ka), (cb, kb)) = (a, b);
                let unknown = crate::tex::UNKNOWN_CHAR as u32;
                if cond == Cond::Chars && crate::tex::may_equal_unknown_char(ca, cb) {
                    return None;
                }
                let one = crate::tex::UNKNOWN_DIGIT as u32;
                if cond == Cond::Chars && (ca == unknown || cb == unknown || ca == one || cb == one) {
                    return Some(false);
                }
                Some(if cond == Cond::Chars { ca == cb } else { ka == kb })
            }
            Cond::Case => None,
        }
    }

    /// The meaning `\ifx` compares: tex.web § 358 reads a token behind a
    /// `\noexpand` marker as `relax` with `no_expand_flag` when it is
    /// expandable, a meaning no other token has.
    fn ifx_meaning(&mut self, token: Token) -> Meaning {
        let meaning = self.meaning_of(token);
        if self.read_noexpanded(token) && self.expandable_meaning(&meaning) {
            return Meaning::Primitive(Primitive::DontExpand);
        }
        meaning
    }

    fn meaning_of(&mut self, token: Token) -> Meaning {
        match token.tok {
            Tok::Cs(sym) => self.env.meaning(sym),
            Tok::Chr(c, Catcode::Active) => {
                let sym = self.active_sym(c);
                self.env.meaning(sym)
            }
            Tok::Chr(c, cat) => Meaning::Char(c, cat),
            Tok::Param(_) => Meaning::Unknown,
        }
    }

    /// tex.web § 503: the relation is read with `get_x_token`, so
    /// `\ifnum\ifcase1 \or1 \fi>0` reaches the `>` past the `\fi`; a missing
    /// one is taken as `=`.
    fn scan_relation(&mut self) -> char {
        self.scan_relation_x().unwrap_or('=')
    }

    /// ⟨number⟩ (tex.web § 440); see [`crate::scan`].
    pub fn scan_number(&mut self) -> Option<i64> {
        self.scanning_value(Self::scan_int)
    }

    /// Run a value scan; a condition it could not decide leaves the value
    /// unknown (see [`Machine::scanning`]).
    pub(crate) fn scanning_value<T>(&mut self, scan: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let outer = std::mem::take(&mut self.scan_undecided);
        self.scanning += 1;
        let value = scan(self);
        self.scanning -= 1;
        let undecided = self.scan_undecided;
        self.scan_undecided = outer || undecided;
        if undecided {
            // What a register read under an undecided condition held is
            // not the only thing the value may be.
            self.abs = None;
            None
        } else {
            value
        }
    }

    /// tex.web § 407; see [`crate::scan`].
    pub fn scan_keyword(&mut self, keyword: &str) -> bool {
        self.scan_keyword_x(keyword)
    }

    /// ⟨glue⟩ or ⟨muglue⟩ (tex.web § 461); see [`crate::scan`].
    pub fn scan_glue(&mut self, mu: bool) -> Option<Glue> {
        self.scanning_value(|m| m.scan_glue_spec(mu))
    }

    /// ⟨dimen⟩ in scaled points (tex.web § 448); see [`crate::scan`].
    pub fn scan_dimen(&mut self) -> Option<i64> {
        self.scanning_value(Self::scan_dimension)
    }

    pub(crate) fn internal_quantity(&mut self, sym: Sym, span: Span) -> Option<Value> {
        if let Some(p) = self.env.meaning(sym).prim()
            && Machine::extra_level(p).is_some()
        {
            return self.extra_quantity(p, sym);
        }
        match self.env.meaning(sym).prim() {
            // An expression of unknown value leaves its interval to the
            // test that reads it, as a register does.
            Some(Primitive::Expr(kind)) => match self.eval_expression(kind) {
                Value::Range { num, .. } => {
                    self.abs = Some(num);
                    self.abs_from = None;
                    Some(Value::Unknown)
                }
                value => Some(value),
            },
            // `\the\catcode⟨number⟩` is the category code in force (The
            // TeXbook, ch. 24); expl3 saves and restores its catcodes with it.
            Some(Primitive::CatcodeAssign) => {
                let code = self.scan_number()?;
                let character = u32::try_from(code).ok().and_then(char::from_u32)?;
                Some(Value::Int(self.catcodes.get(character) as u8 as i64))
            }
            Some(Primitive::CharCode(table)) => {
                let code = self.scan_number()?;
                let character = u32::try_from(code).ok().and_then(char::from_u32)?;
                let value = self.char_code(table, character);
                // "Extended mathchar used as mathchar", and its delimiter
                // counterpart, read as 0.
                let extended = match table {
                    CodeTable::Math => value > MATH_CODE_MAX,
                    CodeTable::Delimiter => value >= 1 << 30,
                    _ => false,
                };
                Some(Value::Int(if extended { 0 } else { value }))
            }
            Some(Primitive::Register(kind)) => {
                let index = self.scan_number();
                // The interval of the index is not the register's.
                self.abs = None;
                let sym = self.register_sym(kind, index?);
                self.reference(sym, span);
                // tex.web § 232: every register starts at zero, so one that
                // was never assigned reads as zero and not as an unknown.
                match self.env.get(sym) {
                    Some(_) => Some(self.register_value(sym, kind)),
                    None => Some(zero_of(kind)),
                }
            }
            // An integer or dimension parameter reads as its value, which is
            // what `\ifnum\pdfoutput>0` asks for.
            Some(p @ (Primitive::IntegerParameter | Primitive::DimenParameter)) => {
                let sym = self.env.identity(sym);
                let kind = if p == Primitive::DimenParameter { RegKind::Dimen } else { RegKind::Count };
                Some(self.register_value(sym, kind))
            }
            Some(Primitive::Lua(crate::builtins::LuaOp::SelectCatcodes)) => {
                Some(Value::Int(crate::plugin::lua::current_table(self)))
            }
            _ => match self.env.meaning(sym) {
                Meaning::Register(kind, _) => {
                    self.reference(sym, span);
                    let storage = self.storage(sym);
                    if storage != sym {
                        self.reference(storage, span);
                    }
                    match self.env.get(storage) {
                        Some(_) => Some(self.register_value(storage, kind)),
                        None => Some(zero_of(kind)),
                    }
                }
                _ => None,
            },
        }
    }

    /// A register's value; when it is not known but lies in an interval,
    /// that is [`Machine::abs`] for the test or assignment that reads it.
    /// A count or dimension register of unknown value holds some number
    /// of its kind: the full interval, which tests narrow.
    fn register_value(&mut self, sym: Sym, kind: RegKind) -> Value {
        let value = self.env.value(sym);
        (self.abs, self.abs_from) = match (&value, self.env.num(sym), kind) {
            (Value::Unknown, Some((dimen, num)), _) => (Some(num), Some((sym, dimen))),
            (Value::Unknown, None, RegKind::Count | RegKind::Dimen) => {
                let dimen = kind == RegKind::Dimen;
                (Some(Num::full(dimen)), Some((sym, dimen)))
            }
            _ => (None, None),
        };
        value
    }

    /// The value a scan of a count (`dimen: false`) or dimension left
    /// when it read no known number: the interval of the register it read
    /// as a whole, or unknown.
    pub(crate) fn scanned_abs(&mut self, dimen: bool) -> Value {
        self.abs_from = None;
        match self.abs.take() {
            Some(num) => Value::from_num(dimen, num),
            None => Value::Unknown,
        }
    }

    /// `\numexpr ⟨expression⟩ \relax` and its relatives (eTeX manual § 3.5).
    /// e-TeX's expressions; see [`crate::scan`].
    fn eval_expression(&mut self, kind: RegKind) -> Value {
        self.scan_expr(kind)
    }

    fn allocate(&mut self, by: Sym, span: Span, kind: RegKind) {
        let Some(name) = self.read_cs() else { return };
        // A package cache replays an allocation by allocating again, so the
        // counters it advances and the register it clears are its effect,
        // not something the package read.
        let tracking = self.env.track_reads(false);
        let index = self.allocation_number(kind);
        self.env.track_reads(tracking);
        self.define(name, Meaning::Register(kind, index), by, DefMode::New, span, None);
        let storage = self.storage(name);
        let tracking = self.env.track_reads(false);
        self.env.set_value(storage, zero_of(kind), true);
        self.env.track_reads(tracking);
        // `\newbox`, `\newread` and `\newwrite` are `\chardef`s of the
        // number they allocate (The TeXbook, appendix B).
        if matches!(kind, RegKind::Box | RegKind::Read | RegKind::Write) && index != crate::tex::UNNUMBERED {
            self.env.set_value(name, Value::Int(i64::from(index)), true);
        }
    }

    /// The number `\newcount` and its relatives hand out: one more than the
    /// last, which `\count10` to `\count17` remember per kind (The TeXbook,
    /// appendix B, `\alloc@`; `latex.ltx` keeps the same registers).  A
    /// counter satex does not know leaves the register unnumbered.
    pub(crate) fn allocation_number(&mut self, kind: RegKind) -> u16 {
        /// plain.tex: `\count10=22`, and 9 for every other kind, so that the
        /// scratch registers below are never handed out.
        const FIRST_COUNT_FLOOR: i64 = 22;
        const FIRST_OTHER_FLOOR: i64 = 9;
        /// `\count10` allocates counts, `\count11` dimens, and so on.
        const ALLOCATION_COUNTERS: i64 = 10;
        let (offset, floor) = match kind {
            RegKind::Count => (0, FIRST_COUNT_FLOOR),
            RegKind::Dimen => (1, FIRST_OTHER_FLOOR),
            RegKind::Skip => (2, FIRST_OTHER_FLOOR),
            RegKind::MuSkip => (3, FIRST_OTHER_FLOOR),
            RegKind::Box => (4, FIRST_OTHER_FLOOR),
            RegKind::Toks => (5, FIRST_OTHER_FLOOR),
            RegKind::Read | RegKind::Write | RegKind::Char | RegKind::MathChar => {
                return crate::tex::UNNUMBERED;
            }
        };
        let counter = self.register_sym(RegKind::Count, ALLOCATION_COUNTERS + offset);
        let Some(last) = self.env.value(counter).as_int() else { return crate::tex::UNNUMBERED };
        let next = last.max(floor) + 1;
        let Ok(index) = u16::try_from(next) else { return crate::tex::UNNUMBERED };
        if index == crate::tex::UNNUMBERED {
            return index;
        }
        self.env.set_value(counter, Value::Int(next), true);
        index
    }

    fn register_def(&mut self, by: Sym, span: Span, kind: RegKind) {
        // `\newcount\x` reaches `\countdef` inside the kernel's `\alloc@`:
        // the definition is placed where the file wrote the name.
        let span = self.definition_site(span);
        let Some(name) = self.read_r_token() else { return };
        self.read_equals();
        // An unknown number makes a name of unknown meaning.
        let Some(index) = self.scan_number() else {
            self.define(name, Meaning::Unknown, by, DefMode::Declare, span, None);
            return;
        };
        self.define(name, Meaning::Register(kind, index as u16), by, DefMode::Declare, span, None);
        if matches!(kind, RegKind::Char | RegKind::MathChar) {
            self.env.set_value(name, Value::Int(index), true);
        }
    }

    /// `\m@ne=-1`: a register control sequence followed by `=` is an
    /// assignment, anything else is a use of its value.
    pub fn assign_register(&mut self, name: Sym, kind: RegKind, span: Span) {
        // tex.web § 1224: a register in the stomach is always assigned; the
        // `=` is optional (§ 405).
        self.read_equals();
        let value = match kind {
            RegKind::Dimen => self.scan_dimen().map_or_else(|| self.scanned_abs(true), Value::Dimen),
            RegKind::Skip => self.scan_glue(false).map_or(Value::Unknown, Value::Glue),
            RegKind::MuSkip => self.scan_glue(true).map_or(Value::Unknown, Value::MuGlue),
            RegKind::Toks => Value::Toks(Rc::from(self.read_general_text())),
            _ => self.scan_number().map_or_else(|| self.scanned_abs(false), Value::Int),
        };
        let value = value::trap_zero_glue(value);
        let global = self.prefixes.global;
        self.prefixes = Default::default();
        let target = self.storage(name);
        self.assign_through(name, target, value, global, span);
    }

    /// Whether what follows can begin a value of this kind, which is how an
    /// assignment without `=` is told from a use of the register.
    /// tex.web § 477: a ⟨general text⟩ is a braced token list, reached through
    /// filler — spaces and `\relax` are skipped and everything expandable on
    /// the way is expanded.  `\toks0=\toks1` copies another register instead.
    /// tex.web § 403 `scan_left_brace`: the next non-blank non-`\relax`
    /// token that expansion leaves must be a `{`, which is consumed.
    pub fn scan_left_brace(&mut self) -> bool {
        loop {
            let Some(token) = self.next_x_token() else { return false };
            match token.tok {
                Tok::Chr(_, Catcode::Space) => {}
                Tok::Chr(_, Catcode::Begin) => return true,
                Tok::Cs(sym) if matches!(self.env.meaning(sym), Meaning::Primitive(Primitive::Relax)) => {}
                Tok::Cs(sym) if matches!(self.env.meaning(sym), Meaning::Char(_, Catcode::Begin)) => return true,
                _ => {
                    self.unread(token);
                    return false;
                }
            }
        }
    }

    pub fn read_general_text(&mut self) -> Vec<Token> {
        let outer = std::mem::replace(&mut self.scanner, crate::machine::Scanner::Absorbing);
        let text = self.read_general_text_absorbing();
        self.scanner = outer;
        text
    }

    fn read_general_text_absorbing(&mut self) -> Vec<Token> {
        loop {
            let Some(token) = self.next_token() else { return Vec::new() };
            match token.tok {
                Tok::Chr(_, Catcode::Space) => continue,
                Tok::Chr(_, Catcode::Begin) => {
                    self.unread(token);
                    let body = self.read_undelimited();
                    // tex.web § 120: a token list that does not fit in main
                    // memory ends the run.
                    if body.len() >= self.cfg.limits.held_tokens {
                        self.halt_capacity("main memory");
                    }
                    return body;
                }
                Tok::Cs(sym) => match self.env.meaning(sym) {
                    Meaning::Primitive(Primitive::Relax) => continue,
                    // `\toks1=\toks0` copies the other register, whether it
                    // is named by a number or by a `\toksdef` name.
                    Meaning::Primitive(Primitive::Register(RegKind::Toks)) => {
                        let Some(index) = self.scan_number() else { return Vec::new() };
                        let source = self.register_sym(RegKind::Toks, index);
                        return match self.env.value(source) {
                            Value::Toks(toks) => toks.to_vec(),
                            _ => vec![self.unknown_token(token.span)],
                        };
                    }
                    Meaning::Register(RegKind::Toks, _) => {
                        let storage = self.storage(sym);
                        return match self.env.value(storage) {
                            Value::Toks(toks) => toks.to_vec(),
                            _ => vec![self.unknown_token(token.span)],
                        };
                    }
                    meaning if self.expandable_meaning(&meaning) => {
                        self.unread(token);
                        if !self.step() {
                            return Vec::new();
                        }
                    }
                    _ => {
                        self.unread(token);
                        return Vec::new();
                    }
                },
                _ => {
                    self.unread(token);
                    return Vec::new();
                }
            }
        }
    }

    /// `\count0=1`, `\toks@{…}`: a register named by number rather than by a
    /// control sequence (tex.web § 1224).
    fn assign_numbered_register(&mut self, kind: RegKind, span: Span) {
        let Some(index) = self.scan_number() else { return };
        let target = self.register_sym(kind, index);
        self.read_equals();
        let value = match kind {
            RegKind::Toks => Value::Toks(Rc::from(self.read_general_text())),
            RegKind::Dimen => self.scan_dimen().map_or_else(|| self.scanned_abs(true), Value::Dimen),
            RegKind::Skip => self.scan_glue(false).map_or(Value::Unknown, Value::Glue),
            RegKind::MuSkip => self.scan_glue(true).map_or(Value::Unknown, Value::MuGlue),
            _ => self.scan_number().map_or_else(|| self.scanned_abs(false), Value::Int),
        };
        let value = value::trap_zero_glue(value);
        let global = self.prefixes.global;
        self.prefixes = Default::default();
        self.assign(target, value, global, span);
    }

    fn arithmetic(&mut self, op: Arith, span: Span) {
        let Some((via, target, kind)) = self.read_register() else { return };
        // tex.web § 1236: the `by` of `\advance⟨register⟩ by ⟨value⟩` is an
        // optional keyword, matched like any other.
        self.scan_keyword("by");
        // A register nothing has assigned yet holds zero (tex.web § 222).
        let current = match self.env.get(target) {
            Some(_) => self.env.value(target),
            None => zero_of(kind),
        };
        let value = match kind {
            // tex.web § 1240: `\advance` on a glue register takes a ⟨glue⟩,
            // while `\multiply` and `\divide` take an integer and scale all
            // three components.
            RegKind::Skip | RegKind::MuSkip => {
                let mu = kind == RegKind::MuSkip;
                let result = match op {
                    Arith::Advance => match (glue_of(&current), self.scan_glue(mu)) {
                        (Some(a), Some(b)) => Some(add_glue(a, b)),
                        _ => None,
                    },
                    _ => match (glue_of(&current), self.scan_number()) {
                        (Some(a), Some(n)) => scale_glue(a, n, op).or(Some(a)),
                        _ => None,
                    },
                };
                match result {
                    Some(g) if mu => Value::MuGlue(g),
                    Some(g) => Value::Glue(g),
                    None => Value::Unknown,
                }
            }
            RegKind::Dimen => {
                // A register never assigned holds zero (`current`); one of unknown
                // value, any number of its kind.
                let current = match self.env.get(target) {
                    Some(_) => self.env.num(target).map(|(_, n)| n),
                    None => current.num().map(|(_, n)| n),
                }
                .or_else(|| matches!(current, Value::Unknown).then(|| Num::range(value::WORD_MIN, value::WORD_MAX)));
                let operand = match op {
                    Arith::Advance => self.scan_dimen().map(Num::exact).or_else(|| self.abs.take()),
                    _ => self.scan_number().map(Num::exact).or_else(|| self.abs.take()),
                };
                self.abs_arith(true, current, operand, op, span)
            }
            _ => {
                // A register never assigned holds zero (`current`); one of unknown
                // value, any number of its kind.
                let current = match self.env.get(target) {
                    Some(_) => self.env.num(target).map(|(_, n)| n),
                    None => current.num().map(|(_, n)| n),
                }
                .or_else(|| matches!(current, Value::Unknown).then(|| Num::range(value::WORD_MIN, value::WORD_MAX)));
                let operand = self.scan_number().map(Num::exact).or_else(|| self.abs.take());
                self.abs_arith(false, current, operand, op, span)
            }
        };
        let value = value::trap_zero_glue(value);
        let global = self.prefixes.global;
        self.prefixes = Default::default();
        self.assign_from_old(via, target, value, global, span);
    }

    /// The register an assignment or `\advance` names, with the kind of
    /// value it holds: a control sequence, or a primitive and a number as in
    /// `\count5`.
    /// The register an arithmetic command names: the name the source used,
    /// the storage it stands for, and its kind.
    fn read_register(&mut self) -> Option<(Sym, Sym, RegKind)> {
        // tex.web § 1237 reads the register with `get_x_token`, so
        // `\advance\csname c@page\endcsname` reaches the register.
        let token = loop {
            self.skip_spaces();
            let token = self.peek()?;
            match token.cs().or_else(|| self.active_cs(token)) {
                Some(sym) if self.expandable(sym) => {
                    if !self.step() {
                        return None;
                    }
                }
                _ => break token,
            }
        };
        if let Tok::Cs(sym) = token.tok
            && let Some(Primitive::Register(kind)) = self.env.meaning(sym).prim() {
                self.next_token();
                let index = self.scan_number()?;
                let target = self.register_sym(kind, index);
                return Some((target, target, kind));
            }
        let sym = self.read_cs()?;
        // `\advance\tex_escapechar:D` changes `\escapechar` itself.
        let sym = self.env.identity(sym);
        let kind = match self.env.meaning(sym) {
            Meaning::Register(kind, _) => return Some((sym, self.storage(sym), kind)),
            _ => match self.env.value(sym) {
                Value::Dimen(_) => RegKind::Dimen,
                Value::Glue(_) => RegKind::Skip,
                Value::MuGlue(_) => RegKind::MuSkip,
                _ => RegKind::Count,
            },
        };
        Some((sym, sym, kind))
    }

    /// `\lccode⟨number⟩=⟨number⟩` and its relatives (tex.web § 1232): a
    /// character code selects the entry, which is held under its own name so
    /// the save stack restores it like any other assignment.
    fn assign_char_code(&mut self, table: CodeTable, span: Span) {
        let Some(code) = self.scan_number() else { return };
        self.read_equals();
        let Some(value) = self.scan_number() else { return };
        let Some(character) = u32::try_from(code).ok().and_then(char::from_u32) else { return };
        let target = self.char_code_sym(table, character);
        let global = self.prefixes.global;
        self.prefixes = Default::default();
        self.env.set_value(target, Value::Int(value), global);
        self.record(Step::Assign, target, span);
    }

    pub(crate) fn char_code_sym(&mut self, table: CodeTable, c: char) -> Sym {
        self.intern(&format!("{CODE}{}{c}", table.as_str()))
    }

    pub(crate) fn char_code(&mut self, table: CodeTable, c: char) -> i64 {
        let sym = self.char_code_sym(table, c);
        self.env.value(sym).as_int().unwrap_or_else(|| table.default_for(c))
    }

    /// tex.web § 1288 (`case_shift`): every character token of the ⟨general
    /// text⟩ whose `\lccode`/`\uccode` is non-zero is replaced by that
    /// character, keeping its category code; control sequences pass through.
    /// The rewritten list goes back into the input for the main scanner.
    fn case_shift(&mut self, table: CodeTable) {
        let body = self.read_general_text();
        let shifted: Vec<Token> = body
            .iter()
            .map(|t| match t.tok {
                // A code that is not known makes a character that is not.
                Tok::Chr(c, _) if {
                    let sym = self.char_code_sym(table, c);
                    matches!(self.env.value(sym), Value::Unknown) && self.env.get(sym).is_some()
                } => self.unknown_token(t.span),
                Tok::Chr(c, cat) => match u32::try_from(self.char_code(table, c))
                    .ok()
                    .and_then(char::from_u32)
                    .filter(|shifted| *shifted != '\0')
                {
                    Some(shifted) => Token::new(Tok::Chr(shifted, cat), t.span),
                    None => *t,
                },
                _ => *t,
            })
            .collect();
        self.push_tokens(Rc::from(shifted), None);
    }

    fn assign_catcode(&mut self, span: Span) {
        let Some(code) = self.scan_number() else { return };
        self.read_equals();
        let Some(cat) = self.scan_number() else { return };
        let (Some(ch), Some(catcode)) = (char::from_u32(code as u32), Catcode::from_u8(cat as u8)) else {
            return;
        };
        self.set_catcode(ch, catcode);
        // `\ExplSyntaxOn` changes the catcodes from inside the kernel; the
        // change belongs to the line of the file that asked for it.
        let at = self.file_call.map_or(span, |(_, call)| call);
        self.occurrence(OccKind::Catcode, ch.to_string(), Some(cat.to_string()), at);
    }

    /// `\input⟨file name⟩` (tex.web § 537): the file is read next.
    fn load_command(&mut self, span: Span, kind: LoadKind) {
        let name = self.read_file_name();
        self.load(&name, kind, span, Vec::new(), None);
    }

    /// `\write⟨number⟩{⟨text⟩}`.  Stream 18 is the shell, which only runs
    /// when the engine was started with shell escape (TeX Live manual,
    /// "Shell escape").
    /// `\write⟨number⟩{…}` (tex.web § 1350): the text is expanded when it is
    /// written — at once after `\immediate` (tex.web § 1375), otherwise when
    /// the page is shipped out, which satex does not do.
    fn write_stream(&mut self, span: Span) {
        let immediate = std::mem::take(&mut self.prefixes.immediate);
        let stream = self.scan_number().unwrap_or(-1);
        let text = self.read_general_text();
        let text = if immediate { self.expand_tokens(Rc::from(text)) } else { text };
        crate::observe::write(self, &text, stream, immediate);
        let text = self.text_of(&text);
        // tex.web § 1370: a stream that is not open writes to the terminal
        // and the log; that is a message.
        let open = (0..16).contains(&stream)
            && self.engine_int(&format!("write_open.{stream}")) == Some(1);
        if immediate && stream != 18 && !open {
            let at = self.file_call.map_or(span, |(_, call)| call);
            crate::observe::warned(self, &text, at);
            self.occurrence(OccKind::Message, text, Some(format!("stream {stream}")), at);
            return;
        }
        let kind = if stream == 18 { OccKind::ShellEscape } else { OccKind::Write };
        self.occurrence(kind, text, Some(format!("stream {stream}")), span);
    }

    /// `\font\name=⟨file⟩ at ⟨size⟩` (tex.web § 1256): the control sequence
    /// becomes a font identifier, which is what `\ifx\font@name\relax` in
    /// NFSS tests for.
    fn font_def(&mut self, by: Sym, span: Span) {
        self.load_font(by, span);
    }

    /// `\openin⟨number⟩=⟨file name⟩` (tex.web § 1275).  satex resolves the
    /// name the way the engine would, so `\ifeof` answers for real.
    fn open_in(&mut self) {
        let stream = self.scan_number().unwrap_or(0);
        self.read_equals();
        let name = self.read_file_name();
        let base = self.base().to_path_buf();
        let path = self.resolver_mut().resolve(&name, LoadKind::Input, &base);
        if let Some(path) = &path {
            self.note_opened(&path.display().to_string());
        }
        let lines = path
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| text.lines().map(str::to_string).collect::<Vec<_>>());
        self.set_stream(stream, lines);
    }

    /// `\read⟨number⟩to⟨cs⟩` (tex.web § 1225): one line of the stream becomes
    /// the replacement text.  A stream that is not open reads nothing.
    fn read_line_into(&mut self, by: Sym, span: Span) {
        let stream = self.scan_number().unwrap_or(-1);
        // tex.web § 1225: a missing `to` is inserted.
        self.scan_keyword("to");
        let Some(name) = self.read_r_token() else { return };
        // tex.web § 484: a stream that is not open is read from the
        // terminal, which gives whatever the user types.
        if !(0..16).contains(&stream) || self.stream_at_end(stream) {
            if self.terminal_read_is_fatal(span) {
                return;
            }
            self.diagnose(
                Severity::Imprecision,
                "terminal-read",
                span,
                format!("\\{} reads from the terminal; what it gives is unknown", self.name(by)),
            );
            self.define(name, Meaning::Unknown, by, DefMode::Declare, span, None);
            return;
        }
        // tex.web § 483: each line gets the `\endlinechar`, and lines are
        // read until the braces balance.  Past the end of the file the line
        // is empty, which makes it `\par` (§ 486).
        let mut body = Vec::new();
        let mut depth = 0i64;
        loop {
            let line = self.take_stream_line(stream).unwrap_or_default();
            let tokens = self.tokenize_line_at(&line, span);
            for t in &tokens {
                match t.tok {
                    Tok::Chr(_, Catcode::Begin) => depth += 1,
                    Tok::Chr(_, Catcode::End) => depth -= 1,
                    _ => {}
                }
            }
            body.extend(tokens);
            if depth <= 0 || self.stream_at_end(stream) {
                break;
            }
        }
        let meaning = self.make_macro(ParameterText::default(), None, body);
        self.define(name, meaning, by, DefMode::Declare, span, None);
    }

    /// tex.web § 484: in `\batchmode` and `\nonstopmode` nobody can answer,
    /// and reading the terminal is a fatal error that ends the job.  Where
    /// the mode is unknown the read may happen; the caller goes on.
    pub(crate) fn terminal_read_is_fatal(&mut self, span: Span) -> bool {
        let mode = self.intern("interactionmode");
        let Some(level) = self.env.value(mode).as_int() else { return false };
        if level > 1 {
            return false;
        }
        self.diagnose(
            Severity::Warning,
            "terminal-read-fatal",
            span,
            "*** (cannot \\read from terminal in nonstop modes)".into(),
        );
        self.halted = true;
        true
    }

    pub(crate) fn read_file_name(&mut self) -> String {
        self.read_file_name_quoted().0
    }

    /// A file name, and whether a `"` quoted part of it, which XeTeX's
    /// `\font` looks for.
    pub(crate) fn read_file_name_quoted(&mut self) -> (String, bool) {
        self.skip_spaces();
        match self.peek() {
            Some(t) if t.is_cat(Catcode::Begin) => (self.read_text(), false),
            _ => {
                // tex.web § 526: a file name is scanned with `get_x_token`, so
                // `\@@input\jobname.aux` names the job.  Anything the gullet
                // cannot remove ends the name and stays in the input.
                // web2c's `more_name`: a `"` opens or closes a quoted part,
                // is not part of the name, and a space inside quotes is
                // (`\@@input\@filef@und` reads `"blx-dm.def" `).
                let mut name = String::new();
                let (mut quoted, mut any_quote) = (false, false);
                while let Some(t) = self.next_token() {
                    match t.tok {
                        Tok::Chr('"', Catcode::Letter | Catcode::Other) => {
                            quoted = !quoted;
                            any_quote = true;
                        }
                        Tok::Chr(c, Catcode::Space) if quoted => name.push(c),
                        // tex.web § 526: the space that ends the name is
                        // consumed.
                        Tok::Chr(_, Catcode::Space) => break,
                        Tok::Chr(c, Catcode::Letter | Catcode::Other) => name.push(c),
                        Tok::Cs(sym) if self.expandable(sym) => {
                            self.unread(t);
                            if !self.step() {
                                break;
                            }
                        }
                        _ => {
                            self.unread(t);
                            break;
                        }
                    }
                }
                (name, any_quote)
            }
        }
    }

    /// A `\newif` switch: a name `\let` to `\iftrue` or `\iffalse`.
    /// A `\newif` switch: `\if⟨name⟩`, `\let` to `\iftrue` or `\iffalse`,
    /// with its `\⟨name⟩true` beside it (latex.ltx `\newif`).  Copies of the
    /// primitives such as expl3's `\if_false:` are not switches.
    fn is_switch(&self, sym: Sym) -> bool {
        if !matches!(self.env.meaning(sym).prim(), Some(Primitive::If(Cond::True | Cond::False))) {
            return false;
        }
        let Some(rest) = self.name(sym).strip_prefix("if").filter(|r| !r.is_empty()) else { return false };
        self.out.interner.lookup(&format!("{rest}true")).is_some_and(|t| self.env.is_defined(t))
    }

    /// `\message` and `\errmessage` (tex.web § 1279): the general text is
    /// expanded as an `\edef` body is.
    fn message(&mut self, by: Sym, span: Span) {
        let name = self.name(by).to_string();
        let raw = self.read_undelimited();
        let expanded = self.expand_tokens(Rc::from(raw.clone()));
        let text = crate::tex::text_with_groups(&expanded, &self.out.interner);
        // The message belongs to the command the file called, wherever the
        // code that raised it was defined.
        let at = self.file_call.map_or(span, |(_, call)| call);
        crate::observe::warned(self, &text, at);
        if let (Some(watch), Some(Primitive::Message { error: true })) = (&mut self.probe, self.env.meaning(by).prim()) {
            watch.error.get_or_insert_with(|| text.clone());
            watch.errors += 1;
        }
        if self.probe.is_none() && matches!(self.env.meaning(by).prim(), Some(Primitive::Message { error: true })) {
            self.note_raised(&text, at);
        }
        if let Some(node) = self.occurrence(OccKind::Message, text, Some(name), at) {
            self.reads_value(node, &raw);
        }
    }

    /// Attribute `text`, raised at `at`, to every name whose doing this
    /// still is: everything on the macro call stack, plus the command the
    /// file itself called (`self.file_call`), which owns whatever an
    /// expansion-time dispatcher it already returned from set in motion —
    /// `\item` is gone from the call stack by the time `\makelabel`'s
    /// default raises "Lonely \item", but it is still what the file called.
    /// Each is a name `explain` may be asked about, and this is what
    /// calling it (in the environments open right now) actually did.
    fn note_raised(&mut self, text: &str, at: Span) {
        let context: Rc<[Sym]> = Rc::from(self.out.env_stack.clone());
        let certain = self.exact();
        let mut names: std::collections::HashSet<Sym> = self.call_stack().collect();
        if let Some((sym, _)) = self.file_call {
            names.insert(sym);
        }
        for name in names {
            if self
                .out
                .facts
                .raised
                .iter()
                .any(|r| r.name == name && r.text == text && r.context.as_ref() == context.as_ref())
            {
                continue;
            }
            self.out.facts.raised.push(crate::facts::RaisedError {
                name,
                text: text.to_string(),
                span: at,
                context: context.clone(),
                certain,
            });
        }
    }
}

/// tex.web § 298 (`print_cmd_chr`): a character-valued meaning is named by
/// one fixed phrase per command code, never by the category's number.
fn char_meaning_text(c: char, cat: Catcode) -> String {
    let phrase = match cat {
        Catcode::Begin => "begin-group character",
        Catcode::End => "end-group character",
        Catcode::Math => "math shift character",
        Catcode::Tab => "alignment tab character",
        Catcode::Param => "macro parameter character",
        Catcode::Sup => "superscript character",
        Catcode::Sub => "subscript character",
        Catcode::Space => "blank space",
        Catcode::Letter => "the letter",
        _ => "the character",
    };
    format!("{phrase} {c}")
}

/// tex.web §§ 1238-1240: `\advance` on a number or dimension is plain
/// 32-bit addition, which wraps; `\multiply` goes through `mult_integers`
/// (a count) or `nx_plus_y` (a dimension, at most `\maxdimen`) with the
/// register's value as `n`, `\divide` through `x_over_n`.  `None` is
/// "Arithmetic overflow", which leaves the register as it was.
fn int_arith(a: i64, b: i64, op: Arith, dimen: bool) -> Option<i64> {
    match op {
        Arith::Advance => Some(value::wrap(a + b)),
        Arith::Multiply => value::mult_and_add(a, b, 0, if dimen { value::MAX_DIMEN } else { value::MAX_INT }),
        Arith::Divide => value::x_over_n(a, b),
    }
}

/// [`int_arith`] on unbounded integers, for the corners of intervals
/// ([`Num::lift`]): `None` only for a zero divisor.
fn ideal_arith(a: i64, b: i64, op: Arith) -> Option<i64> {
    match op {
        Arith::Advance => Some(a + b),
        Arith::Multiply => Some(a * b),
        Arith::Divide => (b != 0).then(|| a / b),
    }
}

/// tex.web § 1239: adding two glues adds their three components, and a
/// stretch or shrink of a higher order of infinity replaces a lower one
/// instead of being added to it.
fn add_glue(a: Glue, b: Glue) -> Glue {
    let combine = |mut x: value::Stretch, y: value::Stretch| {
        if x.amount == 0 {
            x.order = 0;
        }
        if x.order == y.order {
            x.amount = i64::from((x.amount as i32).wrapping_add(y.amount as i32));
        } else if x.order < y.order && y.amount != 0 {
            x = y;
        }
        x
    };
    Glue {
        width: i64::from((a.width as i32).wrapping_add(b.width as i32)),
        stretch: combine(a.stretch, b.stretch),
        shrink: combine(a.shrink, b.shrink),
    }
}

/// tex.web § 1240: `\multiply`/`\divide` on a glue scale its natural width
/// and both of its components, leaving the orders of infinity alone.
fn scale_glue(g: Glue, n: i64, op: Arith) -> Option<Glue> {
    let scale = |x: i64| -> Option<i64> { int_arith(x, n, op, true).filter(|_| op != Arith::Advance) };
    Some(Glue {
        width: scale(g.width)?,
        stretch: value::Stretch { amount: scale(g.stretch.amount)?, order: g.stretch.order },
        shrink: value::Stretch { amount: scale(g.shrink.amount)?, order: g.shrink.order },
    })
}

/// The glue a value holds, widening a plain number or dimension to a glue
/// with no stretch or shrink, as TeX's `scan_glue` coercion does.
fn glue_of(value: &Value) -> Option<Glue> {
    match value {
        Value::Glue(g) | Value::MuGlue(g) => Some(*g),
        Value::Int(n) | Value::Dimen(n) => Some(Glue { width: *n, ..Default::default() }),
        _ => None,
    }
}

fn zero_of(kind: RegKind) -> Value {
    match kind {
        RegKind::Dimen => Value::Dimen(0),
        RegKind::Skip => Value::Glue(Glue::default()),
        RegKind::MuSkip => Value::MuGlue(Glue::default()),
        RegKind::Toks => Value::Toks(Rc::from(Vec::new())),
        _ => Value::Int(0),
    }
}

/// Lower-case roman numerals, as `\romannumeral` writes them.
fn roman(value: i64) -> String {
    const NUMERALS: [(i64, &str); 13] = [
        (1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"),
        (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i"),
    ];
    if value <= 0 {
        return String::new();
    }
    let mut left = value;
    let mut out = String::new();
    for (amount, numeral) in NUMERALS {
        while left >= amount {
            out.push_str(numeral);
            left -= amount;
        }
    }
    out
}

impl Machine<'_> {
    /// `\advance`, `\multiply`, `\divide` of a count or dimension on
    /// intervals (tex.web §§ 1238–1240): the result of every pair of values.
    /// `\multiply` and `\divide` leave the register as it was on an
    /// arithmetic error; `\advance` wraps, so a sum that may leave the
    /// 32-bit range may be anything.  An operand of unknown value is any
    /// number of its kind.
    fn abs_arith(&mut self, dimen: bool, current: Option<Num>, operand: Option<Num>, op: Arith, span: Span) -> Value {
        self.abs_from = None;
        let Some(current) = current else { return Value::Unknown };
        let advance = matches!(op, Arith::Advance);
        let operand = operand.unwrap_or_else(|| {
            if dimen && advance { Num::full(true) } else { Num::range(value::WORD_MIN, value::WORD_MAX) }
        });
        // `\multiply` errs past its `max_answer`; `\advance` and `\divide`
        // wrap (x_over_n of -2^31 by -1).
        let range = match op {
            Arith::Multiply if dimen => (-value::MAX_DIMEN, value::MAX_DIMEN),
            Arith::Multiply => (-value::MAX_INT, value::MAX_INT),
            _ => (value::WORD_MIN, value::WORD_MAX),
        };
        let (num, err) = Num::lift(
            &[&current, &operand],
            self.env.sets.values,
            range,
            op != Arith::Multiply,
            |v| int_arith(v[0], v[1], op, dimen),
            |v| ideal_arith(v[0], v[1], op),
        );
        // `mult_and_add` and `x_over_n` of -2^31 on 32 bits (`negate`
        // wraps) may give -2^31 … 0 where unbounded integers do not.
        let enumerated = match (&current.set, &operand.set) {
            (Some(a), Some(b)) => a.len() * b.len() <= 64,
            _ => current.lo == current.hi && operand.lo == operand.hi,
        };
        let num = if op != Arith::Advance && !enumerated && (current.lo == value::WORD_MIN || operand.lo == value::WORD_MIN) {
            num.map(|n| n.join(&Num::range(value::WORD_MIN, 0), self.env.sets.values))
        } else {
            num
        };
        let num = match num {
            Some(num) if err => num.join(&current, self.env.sets.values),
            Some(num) => num,
            None => {
                self.diagnose(Severity::Warning, "arithmetic-overflow", span, "arithmetic overflow; the register keeps its value".into());
                current
            }
        };
        Value::from_num(dimen, num)
    }

    /// `\\ifnum`/`\\ifdim` on abstract numbers: decided when the intervals
    /// decide it.  Undecided, with a register read as the whole left side
    /// and a known right side, the test is remembered so that each arm
    /// learns what it implies of the register ([`Machine::narrowing`]).
    #[allow(clippy::type_complexity)]
    fn decide_numbers(
        &mut self,
        xs: Option<Num>,
        relation: char,
        ys: Option<Num>,
        (a, b): (Option<i64>, Option<i64>),
        (from_a, from_b): (Option<(Sym, bool)>, Option<(Sym, bool)>),
    ) -> Option<bool> {
        self.narrowing = None;
        let decided = match (&xs, &ys) {
            (Some(x), Some(y)) => x.compare(relation, y),
            _ => None,
        };
        if decided.is_some() {
            return decided;
        }
        // A register on either side against a known number; on the right,
        // the relation is read the other way round.
        let flipped = match relation {
            '<' => '>',
            '>' => '<',
            r => r,
        };
        self.narrowing = match (from_a, xs, b, from_b, ys, a) {
            (Some((sym, dimen)), Some(x), Some(bound), ..) => Some((sym, dimen, x, relation, bound)),
            (_, _, _, Some((sym, dimen)), Some(y), Some(bound)) => Some((sym, dimen, y, flipped, bound)),
            _ => None,
        };
        None
    }
}

/// The one answer every member gives, `None` when they differ or there
/// are none.
fn alike(mut answers: impl Iterator<Item = bool>) -> Option<bool> {
    let first = answers.next()?;
    answers.all(|x| x == first).then_some(first)
}

/// The one name the engine gives a primitive, or `None` when several share
/// the modeled meaning and the copy cannot tell which it came from.
fn primitive_name(engine: crate::config::Engine, p: Primitive) -> Option<String> {
    type Names = std::collections::HashMap<Primitive, Option<String>>;
    thread_local! {
        static NAMES: std::cell::RefCell<Vec<(crate::config::Engine, Names)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    NAMES.with(|cell| {
        let mut cache = cell.borrow_mut();
        if !cache.iter().any(|(e, _)| *e == engine) {
            let mut interner = crate::tex::Interner::default();
            let mut names: Names = Names::new();
            for (sym, meaning) in crate::builtins::initial_meanings(&mut interner, engine) {
                // tex.web § 226: the engine's primitives are control words
                // of letters, and a few control symbols; the table's other
                // entries model macros, such as expl3's `\use:e`.
                let name = interner.name(sym).to_string();
                let engine_word = name.chars().all(|c| c.is_ascii_alphabetic()) || name.chars().count() == 1;
                if let (Meaning::Primitive(q), true) = (meaning, engine_word) {
                    names
                        .entry(q)
                        .and_modify(|seen| {
                            if seen.as_deref() != Some(name.as_str()) {
                                *seen = None;
                            }
                        })
                        .or_insert(Some(name));
                }
            }
            cache.push((engine, names));
        }
        let (_, names) = cache.iter().find(|(e, _)| *e == engine)?;
        names.get(&p).cloned().flatten()
    })
}

/// Whether `name` is one of the engine's own primitives and means `p` when a
/// run starts: `\aftergroup` is, the `\relax` that `\csname` gives an unknown
/// name is not.
fn is_primitive_named(engine: crate::config::Engine, name: &str, p: Primitive) -> bool {
    type Names = std::collections::HashMap<String, Primitive>;
    thread_local! {
        static NAMES: std::cell::RefCell<Vec<(crate::config::Engine, Names)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    NAMES.with(|cell| {
        let mut cache = cell.borrow_mut();
        if !cache.iter().any(|(e, _)| *e == engine) {
            let mut interner = crate::tex::Interner::default();
            let mut names = Names::new();
            for (sym, meaning) in crate::builtins::initial_meanings(&mut interner, engine) {
                if let Meaning::Primitive(q) = meaning {
                    names.insert(interner.name(sym).to_string(), q);
                }
            }
            cache.push((engine, names));
        }
        cache.iter().find(|(e, _)| *e == engine).is_some_and(|(_, names)| names.get(name) == Some(&p))
    })
}
