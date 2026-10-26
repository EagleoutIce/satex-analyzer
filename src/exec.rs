//! The effect of every primitive in `builtins`.

use std::rc::Rc;

use crate::builtins::{
    Arith, CodeTable, Cond, DefMode, Deferred, Limit, LoadKind, OccKind, Owned, Prefix, Primitive, Taken, TextOf,
};
use crate::env::{Binding, GroupKind};

use crate::facts::Severity;
use crate::graph::ControlDep;
use crate::graph::EdgeKind;
use crate::machine::{Accum, CondFrame, CondLimit, Machine, Step, Unit, Unk};
use crate::scan::glue_of;
use crate::tex::TextClass;
use crate::tex::{Catcode, MacroDef, Meaning, ParameterText, RegKind, Span, Sym, Tok, Token};
use crate::value::{self, Glue, Num, Quantity, Value};

/// What a test teaches each arm: modes where it holds/fails, and the
/// register interval it read on either side.
type Learn = (Option<(crate::mode::Modes, crate::mode::Modes)>, Option<(Sym, Option<Value>, Option<Value>)>);

/// The operands of `\if` or `\ifcat` a token may stand for, one list of
/// (character, category) pairs per text, and the rest of a one-of text to
/// put back.
type Operands = (Vec<Vec<(u32, u8)>>, Option<Token>);
pub(crate) type Narrowing = (Sym, crate::value::Quantity, crate::value::Num, Option<Vec<i64>>, char, (i64, i64));

/// tex.web § 1232: `"8000` is the largest math code.
const MATH_CODE_MAX: i64 = 0x8000;

/// e-TeX's `max_reg_num`: the largest `\count`, `\dimen`, `\skip`,
/// `\muskip`, `\toks` or `\box` number (etex.ch; tex.web § 433 is 255).
const MAX_REGISTER: i64 = 32767;

/// TeX Live's `max_in_open`: input files open at a time.
const MAX_IN_OPEN: usize = 15;

/// What `\if` and `\ifcat` compare for a token whose meaning is unknown.
const UNKNOWN_OPERAND: (u32, u8) = (u32::MAX, u8::MAX);

/// Keeps the per-character code tables out of the namespace control
/// sequences live in, the way the register names are kept out of it.
pub(crate) const CODE: char = '\u{3}';

impl Machine<'_> {
    pub fn execute(&mut self, p: Primitive, by: Sym, span: Span) {
        self.drop_prefixes(Some(p));
        self.forget_main_tail(p);
        self.align_peek(p, by);
        // What may put a node on the current list.
        if matches!(
            p,
            // `\unskip`, `\unkern`, `\unpenalty` only take an item away.
            Primitive::Typeset(
                crate::builtins::Typeset::Number
                    | crate::builtins::Typeset::Dimen
                    | crate::builtins::Typeset::Glue
                    | crate::builtins::Typeset::Delimiter
            ) | Primitive::Accent
                | Primitive::Pdf(_)
                | Primitive::Command(_)
                | Primitive::Unmodeled
                | Primitive::AlignMark
                | Primitive::AlignTab
                | Primitive::Lua(_)
                | Primitive::Write
        ) && !self.immediate_whatsit(p)
            && !crate::builtins::assignment(p)
            || matches!(p, Primitive::Mode(cmd) if cmd != crate::builtins::ModeCmd::Par)
        {
            // A `\write` is a whatsit, which the page builder does not count.
            self.material |= match p {
                // What a box brings along it says itself.
                Primitive::Write
                | Primitive::Mode(
                    crate::builtins::ModeCmd::Vertical(crate::builtins::Material::Unpackage { .. })
                    | crate::builtins::ModeCmd::Horizontal(crate::builtins::Material::Unpackage { .. }),
                ) => crate::mode::MATERIAL,
                // tex.web §§ 1075, 1100: `\shipout` puts nothing on the list,
                // and an insertion is no box the page builder counts (§ 1000).
                Primitive::Mode(crate::builtins::ModeCmd::ShipOut) => 0,
                Primitive::Mode(crate::builtins::ModeCmd::Insert { .. }) => crate::mode::MATERIAL,
                _ => crate::mode::MATERIAL | crate::mode::BOXED,
            };
            self.natural_before(p, by, span);
        }
        if p == Primitive::Typeset(crate::builtins::Typeset::Plain) {
            let removed = match self.name(self.env.identity(by)) {
                "unskip" => crate::mode::Removed::Glue,
                "unkern" => crate::mode::Removed::Kern,
                _ => crate::mode::Removed::Penalty,
            };
            self.remove_item(removed);
        }
        self.scoped(p, span, |m| m.execute_command(p, by, span));
        if p == Primitive::IntegerParameter && self.env.identity(by) == self.intern("spacefactor") {
            let sf = self.env.value(self.env.identity(by)).as_int();
            if let Some(n) = &mut self.natural {
                n.space_factor = sf;
            }
        }
        // tex.web § 679: `\prevdepth` decides interline glue before the next
        // box, so an empty list may now take some. tex.web § 1086 reads
        // `\boxmaxdepth` before the group ends, past where `group_closed`
        // can see it.
        if p == Primitive::DimenParameter {
            let which = self.env.identity(by);
            let (prevdepth, boxmaxdepth) = (self.intern("prevdepth"), self.intern("boxmaxdepth"));
            if let Some(n) = &mut self.natural {
                n.prior |= which == prevdepth;
                n.capped |= which == boxmaxdepth;
            }
            // tex.web § 1245: `page_so_far` is no table entry, so global.
            if let Some(crate::builtins::Owned::PageSoFar { .. }) = crate::builtins::owned(self.name(which)) {
                let key = format!("page.set.{}", self.name(which));
                let value = self.env.value(which);
                self.set_engine(&key, value, true);
                self.set_engine("page.set", Value::Int(1), true);
            }
        }
        // tex.web § 1269: the `\afterassignment` token is put back only
        // once the assignment it followed is done.
        if crate::builtins::assignment(p)
            && let Some(token) = self.after_assignment.take()
        {
            self.unread(token);
        }
    }

    /// tex.web § 1214: `\globaldefs` decides an assignment's scope. An
    /// unknown one leaves it open: assign globally, then widen to either.
    pub(crate) fn scoped(&mut self, p: Primitive, span: Span, run: impl FnOnce(&mut Self)) {
        let scope = crate::builtins::assignment(p) && self.apply_global_defs();
        let recent = scope.then(|| {
            let recent = self.env.recent();
            self.env.clear_recent();
            recent
        });
        run(self);
        if let Some(recent) = recent {
            self.diagnose(
                Severity::Imprecision,
                "unknown-globaldefs",
                span,
                "\\globaldefs holds a number satex does not know; what the assignment left is unknown".into(),
            );
            self.widen_recent();
            self.env.set_recent(recent);
        }
    }

    /// tex.web § 1375: `\immediate` before `\write`, `\openout` or
    /// `\closeout` does it at once and leaves no whatsit on the list.
    fn immediate_whatsit(&self, p: Primitive) -> bool {
        use crate::builtins::Syntax;
        self.prefixes.immediate
            && matches!(p, Primitive::Write | Primitive::Command(Syntax::OpenOut | Syntax::CloseOut))
    }

    /// tex.web § 1212: a prefix before a command that is no assignment is
    /// dropped ("You can't use a prefix with"). § 1375: `\immediate` looks
    /// at one command only. `None` is a character.
    pub(crate) fn drop_prefixes(&mut self, p: Option<Primitive>) {
        use Primitive as P;
        let main = match p {
            None => true,
            Some(p) => matches!(
                p,
                P::Mode(_)
                    | P::Typeset(_)
                    | P::Accent
                    | P::Message { .. }
                    | P::Write
                    | P::BeginGroup
                    | P::EndGroup
                    | P::End
                    | P::AlignMark
                    | P::AlignTab
                    | P::IgnoreSpaces
                    | P::Defer(_)
            ),
        };
        if main {
            let immediate = self.prefixes.immediate && p == Some(P::Write);
            self.prefixes = crate::machine::Prefixes { immediate, ..Default::default() };
        } else if p.is_some_and(|p| {
            matches!(p, P::Relax | P::Prefix(Prefix::Global | Prefix::Long | Prefix::Outer | Prefix::Protected))
                || crate::builtins::assignment(p)
        }) {
            self.prefixes.immediate = false;
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
            // Only `\ifx` sees this meaning. No control sequence has it.
            P::DontExpand => {}
            P::SkipCrossing { nested, to_fi } => self.skip_crossing(nested, to_fi, by, span),
            P::NoExpand => {
                // tex.web § 367: the following token goes back behind a
                // `dont_expand` marker, making its next read act as `\relax`
                // (§ 358). Read with the scanner normal, so a file may end
                // before it (the `\everyeof{\noexpand}` idiom).
                let outer = std::mem::replace(&mut self.scanner, crate::machine::Scanner::Normal);
                let next = self.next_token();
                self.scanner = outer;
                match next {
                    Some(token) if token.cs().is_some() || self.active_cs(token).is_some() => {
                        self.unread_noexpanded(token)
                    }
                    Some(token) => self.unread(token),
                    None => {}
                }
            }
            P::Text(kind) => self.produce_text(kind, by, span),
            // e-TeX's `\unexpanded`/`\scantokens` and pdfTeX's `\expanded` read a
            // ⟨general text⟩. `scan_left_brace` expands until the `{` (tex.web § 403, § 473).
            P::Reinject => {
                let body = self.read_general_text();
                self.push_tokens(Rc::from(body));
            }
            // e-TeX manual, `\scantokens`: text is written to a pseudo file
            // (split at `\newlinechar`) and read back like a file, each line
            // ending in `\endlinechar`, `\everyeof` inserted at its end
            // (expl3's `\tl_set_rescan:Nnn` relies on that end marker).
            P::ScanTokens { text: pseudo_text } => {
                let body = self.read_general_text();
                // etex.ch `pseudo_start`: printed to a string (tex.web § 59).
                let escape = self.escape_char();
                let text = crate::tex::detokenize_in(
                    &body,
                    &self.out.interner,
                    escape,
                    &self.catcodes,
                    crate::tex::Selector::NewString,
                );
                self.sync_end_line();
                let newline_sym = self.intern("newlinechar");
                let newline =
                    self.env.value(newline_sym).as_int().and_then(|c| u32::try_from(c).ok()).and_then(char::from_u32);
                // `\scantextokens` has no pseudo file, so its end is no end of file.
                // A definition or argument can go past it.
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
                    self.push_tokens(Rc::from(tokens));
                    return;
                }
                // Lines end at `\newlinechar`. The last one ends too.
                let mut lines: String = match newline {
                    Some(c) if c != '\n' => text.replace(c, "\n"),
                    _ => text,
                };
                lines.push('\n');
                let eof: Rc<[Token]> = Rc::from(Vec::new());
                // An open input file until its `\everyeof` is read (etex.ch). TeX Live
                // caps open files at `max_in_open`.
                let pseudo = self.pseudo_file;
                let open = self.expanding.get(pseudo.0 as usize).copied().unwrap_or(0) as usize;
                if open + usize::from(self.file_depth) >= MAX_IN_OPEN {
                    return self.halt_capacity("text input levels");
                }
                self.bump_expanding(pseudo, 1);
                let mouth = Box::new(crate::tex::Mouth::new(&lines, span.file));
                self.push_pseudo(mouth, span, eof);
            }
            // pdfTeX's `\expanded` scans while expanding, like `\edef`, so
            // `\expanded{{#1\iffalse}}\fi …}` reaches past the `}` its test
            // skips (expl3's key paths).
            P::Expanded => {
                if self.scan_left_brace() {
                    let result = self.scan_expanded_body(false);
                    self.push_tokens(Rc::from(result));
                }
            }
            P::Use(arguments) => {
                let Some(read) = self.read_call_arguments(by, usize::from(arguments), span) else { return };
                let kept: Vec<Token> = read.into_iter().flatten().collect();
                self.push_tokens(Rc::from(kept));
            }
            P::Csname => self.build_csname(span),
            // LuaTeX manual, `\begincsname`: name built like `\csname`, but
            // an undefined one stays undefined, nothing left in the input.
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
            // tex.web § 1135: "Extra \endcsname", ignored.
            P::Endcsname => {
                self.diagnose(Severity::Warning, "extra-endcsname", span, "\\endcsname without \\csname".into())
            }

            P::If(cond) => self.conditional(cond, by, span, false),
            P::Unless => {
                let negated = self.next_token().and_then(|t| t.cs());
                match negated.map(|sym| (sym, self.env.meaning(sym))) {
                    Some((sym, meaning)) if matches!(meaning.prim(), Some(P::If(_))) => {
                        let Some(P::If(cond)) = meaning.prim() else { unreachable!("matched above") };
                        self.conditional(cond, sym, span, true);
                    }
                    // etex.ch: `\unless` before non-conditionals errors and
                    // does nothing. Before an unknown name it may still open
                    // one, so every arm is run.
                    Some((sym, Meaning::Unknown)) => {
                        let base = self.conds.len();
                        self.undecided(sym, span, base, None, None);
                    }
                    _ => {}
                }
            }
            P::Else | P::Or | P::Fi if self.testing.last() == Some(&self.conds.len()) => {
                self.insert_relax(by, span);
            }
            P::Else | P::Or => self.conditional_else(span),
            P::Fi => self.conditional_fi(span),
            P::BeginGroup => self.open_group(GroupKind::SemiSimple, span),
            P::EndGroup => self.close_group(GroupKind::SemiSimple, Token::new(Tok::Cs(by), span)),
            // tex.web § 326: `\aftergroup` saves the token and the group
            // gives it back when it ends.
            P::Defer(Deferred::Group) => self.defer_after_group(),
            // tex.web § 1269: `\afterassignment` holds one token until the
            // next assignment is done.
            P::Defer(Deferred::Assignment) => {
                let Some(token) = self.next_token() else { return };
                self.after_assignment = Some(token);
            }

            P::Allocate(kind) => self.allocate(by, span, kind),
            P::RegisterDef(kind) => self.register_def(by, span, kind),
            P::FontDef => self.font_def(by, span),
            P::Write => self.write_stream(span),
            P::Expr(kind) => {
                let value = self.scan_expr(kind);
                let tokens =
                    match value {
                        Value::Num { quantity: crate::value::Quantity::Int, num } => self.digits_in(num, span),
                        Value::Num { .. } => vec![self.unknown_text(
                            Unk::Class(TextClass::NUMBER.union(TextClass::LETTER), true, None, &[]),
                            span,
                        )],
                        Value::Unknown => vec![self.unknown_text(Unk::Any, span)],
                        value => self.text_tokens(&value.render(), span),
                    };
                self.unread_all(&tokens);
            }
            P::OpenIn => self.open_in(by, span),
            P::CloseIn => {
                let stream = self.scan_stream(by, span);
                self.close_in(stream);
            }
            P::Read => self.read_line_into(by, span),
            // tex.web § 1079: `\box⟨n⟩` is `make_box`, appended to the current list.
            // Only `\setbox` assigns a box register.
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
                if !self.owner_is_here(by, span) {
                    return;
                }
                self.read_equals();
                let value = self.scan_number();
                // tex.web § 1265: `\interactionmode` is not on the save
                // stack, so the enclosing group never takes it back.
                let global = self.prefixes.global || self.name(by) == "interactionmode";
                let limit = crate::builtins::limit(self.name(by), self.out.plugins.engine);
                let stored = match limit {
                    Some(limit) => self.limited(by, by, limit, value, span),
                    None => Some(value.map_or(Value::Unknown, Value::Int)),
                };
                if let Some(stored) = stored {
                    self.env.set_value(by, stored, global);
                    self.note_assignment(by, global, span);
                }
                // `\endlinechar` is what the mouth appends to every line
                // (tex.web § 240). expl3 sets it to an ignorable space,
                // which ends a parameter text there.
                if self.name(by) == "endlinechar" {
                    match value.and_then(|value| i32::try_from(value).ok()) {
                        Some(value) => self.set_end_line(value),
                        // A value satex cannot follow leaves the mouth at the
                        // top of the lattice, never at the old character.
                        None => self.set_end_line(crate::tex::EndLineChar::UNKNOWN.0),
                    }
                }
            }
            P::DimenParameter => {
                let by = self.env.identity(by);
                if !self.owner_is_here(by, span) {
                    return;
                }
                self.read_equals();
                let value = self.scan_dimen();
                let global = self.prefixes.global;
                self.env.set_value(by, value.map_or(Value::Unknown, Value::Dimen), global);
                // tex.web § 1243: the vertical list's own, for its next box.
                if self.name(by) == "prevdepth"
                    && let Some(n) = &mut self.natural
                {
                    n.prev_depth = match value {
                        Some(d) if d <= crate::builtins::IGNORE_DEPTH => None,
                        d => Some(d.map_or(crate::mode::Dim::ANY, crate::mode::Dim::exact)),
                    };
                }
            }
            P::CatcodeAssign => self.assign_catcode(span),
            P::CharCode(table) => self.assign_char_code(table, span),
            P::CaseShift(table) => self.case_shift(table),

            P::Load(kind) => self.load_command(span, kind),
            // Each path ends its own file (tex.web § 362).
            P::Endinput => self.end_input(span),

            // The engine's `\end` (tex.web § 1054). latex.ltx keeps it as `\@@end`
            // and defines its own `\end{…}`.
            // § 1094: in horizontal mode it heads for vertical mode first.
            // § 1054: `its_all_over` is `privileged`, so a list of its own
            // reports the illegal case and goes on.
            P::End => {
                let token = Token::new(Tok::Cs(by), span);
                let horizontal = self.mode.within(crate::mode::Modes::ANY_HORIZONTAL);
                if horizontal && !self.mode.is_empty() && self.vertical_material(token) {
                    return;
                }
                match self.mode.within(crate::mode::Modes::INTERNAL_VERTICAL) && !self.mode.is_empty() {
                    true => self.certain_error("You can't use `\\end' in internal vertical mode", true, span),
                    false => self.end_job(),
                }
            }

            P::Message { .. } => self.message(by, span),
            P::Lua(op) => crate::plugin::lua::primitive(self, op, by, span),
            P::LuaCall { id, .. } => crate::plugin::lua::call(self, id, by, span),
            P::Docstrip(op) => crate::literate::docstrip(self, op, span),
            P::Accent => {
                if !self.horizontal_material(Token::new(Tok::Cs(by), span)) {
                    let code = crate::observe::accent(self, span);
                    self.make_accent(code);
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
                // The item before it, where the list's tail is followed.
                let before = self.natural.map_or(crate::mode::Last::UNKNOWN, |n| n.tail[0]);
                self.append_item(crate::mode::Item::Penalty(penalty));
                // tex.web §§ 1005, 1012: a forced break on the main vertical
                // list ejects the page, which is then empty.
                // § 1103: a penalty on the main vertical list builds the page.
                if self.mode == crate::mode::Modes::VERTICAL {
                    self.take_contributions();
                    match penalty.filter(|p| *p <= -10000) {
                        Some(p) => self.fire_up(p, before, span),
                        None => self.build_page(span),
                    }
                }
            }
            P::Typeset(crate::builtins::Typeset::Dimen) => {
                if self.name(self.env.identity(by)) == "mkern" {
                    let kern = self.scan_mu_dimen();
                    match kern.filter(|_| self.math_followed()) {
                        Some(k) => {
                            let width = self.mu_width(k, self.math_size());
                            self.append_item(crate::mode::Item::Kern(width));
                            // tex.web § 424: `\lastkern` reads a math kern
                            // in mu, before § 717 converts it.
                            self.set_tail(crate::mode::Last::Kern(k));
                        }
                        None => self.append_item(crate::mode::Item::Unknown),
                    }
                } else {
                    let kern = self.scan_dimen();
                    self.append_item(crate::mode::Item::Kern(kern));
                }
            }
            // tex.web §§ 1060, 732: `\mskip`'s glue is in math units, its
            // width known in followed inline math (text style).
            P::Typeset(crate::builtins::Typeset::Glue) => {
                let glue = self.scan_glue(true);
                match glue.filter(|_| self.math_followed()) {
                    Some(g) => {
                        let width = self.mu_width(g.width, self.math_size());
                        // Glue that neither stretches nor shrinks keeps its
                        // formula one rigid box to the line breaker.
                        let rigid = g.stretch.amount == 0 && g.shrink.amount == 0;
                        let spec = width.filter(|_| rigid).map(|width| value::Glue { width, ..Default::default() });
                        self.append_item(crate::mode::Item::glue(width, spec));
                        // tex.web § 424: `\lastskip` reads it as muglue.
                        if let Some(crate::mode::Last::Glue(w, ..)) = self.natural.map(|n| n.tail[0]) {
                            self.set_tail(crate::mode::Last::Glue(w, Some(value::GlueHull::exact(&g)), true));
                        }
                    }
                    None => self.append_item(crate::mode::Item::Unknown),
                }
            }
            P::Pdf(op) => crate::observe::pdf(self, op, span),
            P::Relax | P::Typeset(_) => {}
            // An engine primitive satex doesn't interpret. Does nothing, but
            // records the gap so unmodeled effects are on record.
            // LuaTeX's `\aligntab` is an `&` (§ 785).
            P::AlignTab => {
                if !self.template_cell_end(Token::new(Tok::Cs(by), span), false) {
                    self.end_word();
                    self.align_cell(true);
                    self.template_next_cell(false, span);
                }
            }
            P::Unmodeled | P::AlignMark => {
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

    /// Whether a definition's scan reads `sym` as `#`: LuaTeX does so for
    /// a token meaning `\alignmark`, `\let` alias and all.
    pub fn means_align_mark(&self, sym: Sym) -> bool {
        self.out.plugins.engine.has_luatex()
            && matches!(self.env.meaning(sym), Meaning::Primitive(Primitive::AlignMark))
    }

    pub fn make_macro(&self, parameter_text: ParameterText, body: Vec<Token>) -> Meaning {
        let mut replacement = crate::tex::parameterize(body, |sym| self.means_align_mark(sym));
        // `\def\a#1#{…}`: the `{` that ended the parameter text belongs to the
        // replacement text as well (tex.web § 476).
        if parameter_text.brace_end {
            let span = replacement.last().map(|t| t.span).unwrap_or_default();
            replacement.push(Token::new(Tok::Chr('{', Catcode::Begin), span));
        }
        Meaning::Macro(Rc::new(MacroDef {
            parameter_text,
            arg_spec: None,
            replacement_text: Rc::from(replacement),
            long: self.prefixes.long,
            outer: self.prefixes.outer,
            protected: self.prefixes.protected,
        }))
    }

    /// Where a definition is placed: at the name's own file when a macro of
    /// another file does the defining (expl3's `\cs_new:Npn` reaches
    /// `\tex_gdef:D` in `expl3-code.tex`), else at the defining command.
    pub(crate) fn definition_site(&mut self, span: Span) -> Span {
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
        // The parameter text ends at the `{` opening the replacement text.
        // No `{` means the catcodes are not what the file assumed. Reading on
        // would consume the rest of it.
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
        let parameter_text = ParameterText::from_tokens(&pattern, |sym| self.means_align_mark(sym));
        // tex.web § 477: an `\edef` body is scanned while expanded, ending at
        // the `}` balancing what expansion leaves. A conditional satex can't
        // decide takes its true arm, as in any expanded text.
        let outer = std::mem::replace(&mut self.scanner, crate::machine::Scanner::Defining);
        // The name's own text met in its `\edef` body is copied through.
        let target = std::mem::replace(&mut self.copy_target, expand.then_some((name, self.edef_depth + 1)));
        let mark = self.copies.len();
        let id = self.scan_begin();
        let mut bodies = match expand {
            true => vec![self.scan_expanded_body(true)],
            false => vec![self.read_group_body()],
        };
        if self.scan_end(id) {
            let unknown = self.unknown_text(Unk::Any, self.position_span());
            bodies.iter_mut().for_each(|b| b.push(unknown));
        }
        self.copy_target = target;
        let copies = self.copies.split_off(mark.min(self.copies.len()));
        self.scanner = outer;
        if global && self.global_defs() >= 0 {
            self.prefixes.global = true;
        }
        let global = self.prefixes.global;
        for body in bodies.into_iter().rev() {
            let meaning = self.make_macro(parameter_text.clone(), body);
            let splice = self.resolve_copies(name, &meaning, &copies, global);
            self.prefixes.global = global;
            self.define(name, meaning, by, DefMode::Declare, span);
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
            // tex.web § 1221: `\futurelet` reads with plain `get_token`, no
            // `scan_optional_equals`. An `=` after the name is the first lookahead.
            let a = self.next_token();
            let b = self.next_token();
            // A rest of unknown text, perhaps none: a path where it is empty
            // and one where it is not, both re-reading `\futurelet`.
            if let (Some(held), Some(more)) = (a, b)
                && let Some((class, false)) = self.typed_text_of(more)
                && self.split_rest(more, class)
            {
                self.unread_all(&[Token::new(Tok::Cs(by), span), Token::new(Tok::Cs(name), span), held]);
                return;
            }
            let b = b.map(|t| self.one_token(t, false).unwrap_or(t));
            (b, a)
        } else {
            // tex.web § 1221: optional spaces, optional `=`, then at most one
            // space. `\let\@sptoken= ` makes a space the meaning. Only an
            // `=` of category 12 (`other_token+"="`): an active `=` is the source.
            self.skip_spaces();
            let mut token = self.next_token();
            if token.is_some_and(|t| t.tok == Tok::Chr('=', Catcode::Other)) {
                token = self.next_token();
                if token.is_some_and(|t| t.is_space()) {
                    token = self.next_token();
                }
            }
            (token, None)
        };
        let Some(source) = source else { return };
        // One of several texts: `\let` copies its first token, on a path per text.
        if self.split_one_of(source) {
            let again: Vec<Token> = [Some(Token::new(Tok::Cs(by), span)), Some(Token::new(Tok::Cs(name), span)), held]
                .into_iter()
                .flatten()
                .collect();
            self.unread_all(&again);
            return;
        }
        let meaning = match source.tok {
            Tok::Cs(src) => self.env.meaning(src),
            // tex.web § 1221: an active character is an `eqtb` entry like a
            // control sequence, so `\let` copies its current meaning.
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
        // A `\chardef` constant's number is its value, copied with the
        // meaning (tex.web § 1221 copies the whole `eqtb` entry).
        let constant = match (source.tok, &meaning) {
            (Tok::Cs(src), Meaning::Register(RegKind::Char | RegKind::MathChar | RegKind::UMathChar, _)) => {
                Some(self.env.value(src))
            }
            _ => None,
        };
        let global = self.prefixes.global;
        // tex.web § 1221 copies the primitive's `chr` code too. satex
        // remembers which primitive it is, since it models some alike.
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
        // `\let` copies the whole meaning. A name a join left one of several
        // meanings stays one of them.
        let may = match source.tok {
            Tok::Cs(src) => self.env.slot(src).and_then(|b| b.may.clone()).or_else(|| self.typed_meanings(src)),
            _ => None,
        };
        let node = self.define(name, meaning, by, DefMode::Declare, span);
        if let Some(may) = may.filter(|_| !self.unknown_text_name(name))
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
            && let Some(reference) = self.reference(src, source.span)
        {
            self.out.graph.edge(node, reference, EdgeKind::DEFINED_BY);
        }
    }

    /// One step of expansion, as `\expandafter` asks. An unexpandable token
    /// stays where it is (tex.web § 367).
    pub(crate) fn expand_once(&mut self) {
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
            // An unknown meaning may expand to anything, unless every meaning a
            // join left it with does not expand.
            Some(sym)
                if matches!(self.env.meaning(sym), Meaning::Unknown)
                    && !self.is_unknown_marker(sym)
                    && !self.read_noexpanded(token)
                    && !self
                        .env
                        .slot(sym)
                        .and_then(|b| b.may.clone())
                        .is_some_and(|may| may.iter().all(|m| !self.expandable_meaning(m))) =>
            {
                let unknown = self.one_of_texts(sym, token.span);
                self.unread(unknown);
            }
            // One of several texts: what its first token expands to, on a
            // path per text.
            _ if self.split_one_of(token) => self.expand_once(),
            // Typed text that may hold a name that expands: its first token
            // may expand to anything, as in a full expansion.
            Some(sym)
                if self.is_unknown_marker(sym)
                    && self.typed_shape(token).is_some()
                    && self.typed_text_expands(token)
                    && !self.read_noexpanded(token) =>
            {
                let unknown = self.unknown_text(Unk::Any, token.span);
                self.unread(unknown);
            }
            // tex.web § 370: an undefined name is an error and expands to nothing.
            Some(sym) if matches!(self.env.meaning(sym), Meaning::Undefined) && !self.may_be_defined(sym) => {
                let text = format!("Undefined control sequence \\{}", self.name(sym));
                self.certain_error(&text, true, token.span);
            }
            _ => self.unread(token),
        }
    }

    /// What expanding an unknown name once gives. If a join left it one of
    /// several parameterless macros, a token holding their texts as its
    /// meanings. Otherwise unknown text.
    pub(crate) fn one_of_texts(&mut self, sym: Sym, span: Span) -> Token {
        let may = self.env.slot(sym).and_then(|b| b.may.clone());
        let plain = may.as_ref().is_some_and(|may| {
            may.iter()
                .all(|m| m.as_macro().is_some_and(|m| m.parameter_text.items.is_empty() && !m.parameter_text.brace_end))
        });
        let (Some(may), true) = (may, plain) else { return self.unknown_text(Unk::Any, span) };
        let texts = may.iter().filter_map(|m| m.as_macro().map(|m| m.replacement_text.clone())).collect();
        self.one_of(texts, span)
    }

    /// Expand a gullet primitive and put its characters back. Swallowing them
    /// lets a delimited argument scan run past its delimiter.
    fn produce_text(&mut self, kind: TextOf, by: Sym, span: Span) {
        let mut at = span;
        let text = match kind {
            TextOf::JobName => self.job_name(),
            // Arms of a test inside the scan join at its end.
            TextOf::Number | TextOf::Roman => match self.scan_number_joined() {
                Some(value) if kind == TextOf::Roman => roman(value),
                Some(value) => value.to_string(),
                None => {
                    // A bounded value prints as digits typed with its bound,
                    // a roman numeral as letters of category 12, perhaps none.
                    let digits = match self.abs.take() {
                        Some(num) if kind == TextOf::Number => self.digits_in(num, span),
                        // A small interval is one of its numerals, so a name
                        // spelled from it splits per value (tex.web § 69).
                        Some(num)
                            if num.lo >= value::WORD_MIN
                                && num.hi <= value::WORD_MAX
                                && num.hi - num.lo < i64::from(self.limits.meaning_set) =>
                        {
                            let texts =
                                (num.lo..=num.hi).map(|v| Rc::from(self.text_tokens(&roman(v), span))).collect();
                            vec![self.one_of(texts, span)]
                        }
                        _ if kind == TextOf::Roman => {
                            vec![self.unknown_text(Unk::Class(TextClass::OTHER, false, None, &[]), span)]
                        }
                        // Any 32-bit value: a sign and at most 10 digits
                        // (tex.web § 65), as `\the` prints it.
                        _ => self.digits_in(Num::range(value::WORD_MIN, value::WORD_MAX), span),
                    };
                    return self.unread_all(&digits);
                }
            },
            TextOf::Detokenize => {
                let body = self.read_general_text();
                // Text with unknown parts: each known run is its characters,
                // each unknown token stays what it is, so what is known of
                // the text (a prefix such as `data:,`) is kept.
                if self.has_unknown(&body) {
                    let tokens = self.detokenize_known_runs(&body);
                    return self.unread_all(&tokens);
                }
                match self.text_known(&body) {
                    // Each character stands where it was written.
                    Some(text) if text.chars().count() == body.len() && body.iter().all(|t| t.cs().is_none()) => {
                        let tokens: Vec<Token> = text
                            .chars()
                            .zip(&body)
                            .map(|(c, t)| {
                                Token::new(Tok::Chr(c, if c == ' ' { Catcode::Space } else { Catcode::Other }), t.span)
                            })
                            .collect();
                        return self.unread_all(&tokens);
                    }
                    Some(text) => text,
                    None => return self.unread_unknown(span),
                }
            }
            TextOf::String | TextOf::Meaning | TextOf::The => {
                // tex.web § 471: `\string` and `\meaning` read with the scanner
                // normal, so an `\outer` name is no error there.
                let outer = std::mem::replace(&mut self.scanner, crate::machine::Scanner::Normal);
                let token = self.next_token();
                self.scanner = outer;
                let Some(token) = token else { return };
                if self.split_one_of(token) {
                    return self.produce_text(kind, by, span);
                }
                // A name of one of several meanings prints each on its path,
                // a built name each name it may be, `\relax` if none
                // (tex.web § 372).
                if kind == TextOf::Meaning
                    && let Some(sym) = token.cs()
                    && (self.split_meaning(sym, token.span) || self.split_family(sym, token.span))
                {
                    self.unread(token);
                    return self.produce_text(kind, by, span);
                }
                // Unknown digits are category 12, which `\string` gives back as
                // they are (tex.web § 465); letters of unknown text come back
                // as category 12 too.
                if kind == TextOf::String && self.typed_shape(token).is_some() {
                    let typed = token.cs().is_some_and(|sym| self.name(sym).starts_with(crate::scan::TYPED_DIGITS));
                    let token = if typed { token } else { self.printed(token, true, span) };
                    return self.unread(token);
                }
                // Unknown text, or a name with an unknown escape, prints unknown.
                let unknown_meaning = match token.tok {
                    Tok::Cs(sym) => {
                        sym == self.unknown
                            || self.is_unknown_digits(sym)
                            || (kind != TextOf::Meaning && self.is_unknown_name(sym))
                            || (kind == TextOf::Meaning
                                && matches!(
                                    self.env.meaning(sym),
                                    Meaning::Unknown | Meaning::Char(crate::tex::UNKNOWN_CHAR, _)
                                ))
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
                // `\string` and `\meaning` print characters of category 12
                // and spaces (tex.web § 464) whatever they print: typed text,
                // with what is known of a built name around it.
                if unknown_meaning || (escaped && self.escape_state().is_none()) {
                    // Unknown text is more than the one token printed.
                    let text = token.cs().is_some_and(|sym| sym == self.unknown || self.is_unknown_digits(sym));
                    if kind == TextOf::The || text {
                        return self.unread_unknown(span);
                    }
                    // A name printed under an unknown `\escapechar` is that
                    // name after at most one character (none when the code
                    // names none, tex.web § 63): what the name spells stays
                    // known, so a scan of the text for a character it cannot
                    // hold decides at once.
                    let escape = |m: &mut Self| match m.escape_state() {
                        Some(escape) => m.text_tokens(&escape.map(String::from).unwrap_or_default(), token.span),
                        None => vec![m.unknown_text(Unk::Class(TextClass::PRINTED, false, Some(1), &[]), token.span)],
                    };
                    if kind == TextOf::String
                        && !unknown_meaning
                        && let Tok::Cs(sym) = token.tok
                        && !self.name(sym).is_empty()
                    {
                        let name = self.name(sym).to_string();
                        let mut tokens = escape(self);
                        tokens.extend(self.text_tokens(&name, token.span));
                        return self.unread_all(&tokens);
                    }
                    let mut tokens = Vec::new();
                    let known = match (kind, token.cs().and_then(|sym| self.name_pattern(sym))) {
                        (TextOf::String, Some(parts)) => Some(parts),
                        _ => None,
                    };
                    let one = known.is_none();
                    if let Some((prefix, _)) = &known {
                        tokens.extend(escape(self));
                        tokens.extend(self.text_tokens(prefix, token.span));
                    }
                    tokens.push(self.unknown_text(Unk::Class(TextClass::PRINTED, one, None, &[]), token.span));
                    if let Some((_, suffix)) = &known {
                        tokens.extend(self.text_tokens(suffix, token.span));
                    }
                    return self.unread_all(&tokens);
                }
                // The characters of a name stand where the name was written.
                if kind == TextOf::String {
                    at = token.span;
                }
                match (kind, token.tok) {
                    (TextOf::String, Tok::Cs(sym)) => {
                        // tex.web § 69: the escape is `\escapechar`, none if out of
                        // range. `\csstring` never prints one.
                        let bare = self.name(self.env.identity(by)) == "csstring";
                        let escape = if bare { None } else { self.escape_char() };
                        let name = self.name(sym).to_string();
                        let mut text: String = escape.into_iter().collect();
                        // tex.web § 263: empty name prints as `\csname\endcsname`.
                        if name.is_empty() {
                            text.push_str("csname");
                            text.extend(escape);
                            text.push_str("endcsname");
                        }
                        text + &name
                    }
                    (TextOf::String, Tok::Chr(c, _)) => c.to_string(),
                    // A font name prints with a size clause satex may not know
                    // (xetex.web `font_name_code`).
                    (TextOf::Meaning, Tok::Cs(sym))
                        if let Meaning::Primitive(Primitive::FontIdent(font)) = self.env.meaning(sym)
                            && self.font_name_known(font).is_none() =>
                    {
                        let mut tokens = self.text_tokens("select font ", at);
                        tokens.push(self.unknown_text(Unk::Class(TextClass::PRINTED, false, None, &[]), at));
                        return self.unread_all(&tokens);
                    }
                    // A register named through a number satex could not pin
                    // down prints that number (tex.web § 1224).
                    (TextOf::Meaning, Tok::Cs(sym))
                        if matches!(self.env.meaning(sym), Meaning::Register(k, crate::tex::UNNUMBERED)
                            if !matches!(k, RegKind::Char | RegKind::MathChar | RegKind::UMathChar)) =>
                    {
                        let text = self.meaning_text(sym);
                        let mut tokens = self.text_tokens(&text, at);
                        tokens.push(self.unknown_digits_token(at));
                        return self.unread_all(&tokens);
                    }
                    (TextOf::Meaning, Tok::Cs(_) | Tok::Chr(_, Catcode::Active)) => {
                        let sym = match token.tok {
                            Tok::Chr(c, _) => self.active_sym(c),
                            _ => token.cs().unwrap_or(self.unknown),
                        };
                        // Unknown text in a body prints as typed text, never
                        // as the name that stands for it (tex.web § 294).
                        if let Meaning::Macro(m) = self.env.meaning(sym)
                            && self.has_unknown(&m.replacement_text)
                        {
                            let escape = self.escape_char();
                            let head = self.macro_head(&m, escape);
                            let mut tokens = self.text_tokens(&head, at);
                            tokens.extend(self.detokenize_known_runs(&m.replacement_text));
                            return self.unread_all(&tokens);
                        }
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
        // Only the interval this quantity leaves may type its digits.
        self.abs = None;
        let value = self.internal_quantity(sym, span);
        // Glue prints its `plus` and `minus` parts unless both are zero
        // (tex.web § 178).
        let tail = match &value {
            Some(Value::GlueNum { hull, .. }) => hull.stretch != Num::exact(0) || hull.shrink != Num::exact(0),
            _ => matches!(level, Some(RegKind::Skip | RegKind::MuSkip)),
        };
        match value {
            Some(Value::Toks(toks)) => toks.to_vec(),
            // An unknown dimension still prints as `⟨digits⟩.⟨digits⟩pt`
            // (tex.web § 103), which `\def\getpt#1pt{…}` splits at. The digits
            // are unknown text holding no delimiter.
            Some(Value::Unknown | Value::GlueNum { .. }) | None
                if matches!(level, Some(RegKind::Dimen | RegKind::Skip | RegKind::MuSkip)) =>
            {
                let unit = if level == Some(RegKind::MuSkip) { "mu" } else { "pt" };
                let num = if level == Some(RegKind::Dimen) { self.abs.take() } else { None };
                // A known sign: the integer part typed with its interval,
                // which a scan reads back (tex.web § 103).
                let mut tokens = match num {
                    Some(n) if n.lo >= 0 || n.hi < 0 => {
                        let (lo, hi) = if n.lo >= 0 { (n.lo, n.hi) } else { (-n.hi, -n.lo) };
                        let mut t = if n.lo < 0 { self.text_tokens("-", span) } else { Vec::new() };
                        let unit = crate::value::UNIT;
                        t.extend(self.digits_in(Num::range(lo / unit, hi / unit), span));
                        t
                    }
                    _ => self.unknown_number(num, crate::value::UNIT, span),
                };
                tokens.extend(self.text_tokens(".", span));
                // At most five places after the point (tex.web § 103).
                tokens.push(self.unknown_text(Unk::Class(TextClass::DIGIT, true, Some(5), &[]), span));
                tokens.extend(self.text_tokens(unit, span));
                if tail {
                    tokens.push(self.unknown_text(Unk::Class(TextClass::PRINTED, false, None, &[]), span));
                }
                tokens
            }
            Some(Value::Unknown) | None if level == Some(RegKind::Toks) => self.toks_member(sym, span),
            // A bounded number (`\the\numexpr`, a register in an interval)
            // prints as digits typed with its bound.
            Some(Value::Unknown) | None if level == Some(RegKind::Count) => match self.abs.take() {
                Some(num) if matches!(self.env.meaning(sym), Meaning::Register(RegKind::Count, _)) => {
                    self.register_digits(sym, num, span)
                }
                Some(num) => self.digits_in(num, span),
                // No interval: any 32-bit value, as an unknown count
                // register prints (tex.web § 465 prints both alike).
                None => self.digits_in(Num::range(value::WORD_MIN, value::WORD_MAX), span),
            },
            Some(Value::Unknown) | None if level.is_some() => self.unknown_number(None, 1, span),
            Some(value) => {
                let text = value.render();
                self.text_tokens(&text, span)
            }
            None => Vec::new(),
        }
    }

    /// An unknown value in `num` as `\the` prints it, its integer part the
    /// value over `unit`: digits, after a `-` where the value is negative.
    /// A sign it may or may not have makes the first character unknown (a
    /// digit or `-`). An integer part of at least 1 begins with a digit 1-9,
    /// else its digits have no leading zero (tex.web §§ 103, 104,
    /// [`crate::machine::LEAD_MARK`]).
    /// The integer part has at most the digits of the largest value: of
    /// 2^31 - 1 for a count, 16383 for a dimension (tex.web §§ 65, 103).
    fn unknown_number(&mut self, num: Option<crate::value::Num>, unit: i64, span: Span) -> Vec<Token> {
        let max = num.map_or(if unit == 1 { crate::value::MAX_INT } else { crate::value::MAX_DIMEN }, |n| {
            n.lo.unsigned_abs().max(n.hi.unsigned_abs()) as i64
        });
        let len = (max / unit).checked_ilog10().unwrap_or(0) + 1;
        let rest = |me: &mut Self, len: u32| {
            (len > 0).then(|| me.unknown_text(Unk::Class(TextClass::DIGIT, false, Some(len), &[]), span))
        };
        let digits = |me: &mut Self, nonzero: bool| match nonzero {
            true => {
                let mut tokens = vec![Token::new(Tok::Chr(crate::tex::UNKNOWN_NONZERO, Catcode::Other), span)];
                tokens.extend(rest(me, len - 1));
                tokens
            }
            false => vec![me.lead_digits(true, len, span)],
        };
        match num {
            Some(n) if n.lo >= 0 => digits(self, n.lo >= unit),
            Some(n) if n.hi < 0 => {
                let mut tokens = vec![Token::new(Tok::Chr('-', Catcode::Other), span)];
                tokens.extend(digits(self, n.hi <= -unit));
                tokens
            }
            // After a `-`, every digit is still to come.
            _ => {
                let mut tokens = vec![Token::new(Tok::Chr(crate::tex::UNKNOWN_CHAR, Catcode::Other), span)];
                tokens.push(self.lead_digits(false, len, span));
                tokens
            }
        }
    }

    /// Unknown text in place of what an expandable primitive yields.
    fn unread_unknown(&mut self, span: Span) {
        let token = self.unknown_text(Unk::Any, span);
        self.unread(token);
    }

    /// `\detokenize` of a text with unknown tokens in it: the known runs
    /// as characters, the unknown tokens as they are. A run that cannot be
    /// spelled (a name under an unknown `\escapechar`) is unknown text.
    fn detokenize_known_runs(&mut self, body: &[Token]) -> Vec<Token> {
        let mut out = Vec::with_capacity(body.len());
        let mut run: Vec<Token> = Vec::new();
        let flush = |me: &mut Self, run: &mut Vec<Token>, out: &mut Vec<Token>| {
            if run.is_empty() {
                return;
            }
            let span = run[0].span;
            match me.text_known(run) {
                Some(text) => out.extend(me.text_tokens(&text, span)),
                None => out.push(me.unknown_text(Unk::Class(TextClass::PRINTED, false, None, &[]), span)),
            }
            run.clear();
        };
        for &t in body {
            if self.has_unknown(&[t]) {
                flush(self, &mut run, &mut out);
                // One of several known texts prints as one of their spellings.
                let spelled: Option<Vec<Vec<Token>>> = self.one_of_members(t).and_then(|members| {
                    members.iter().map(|m| self.text_known(m).map(|text| self.text_tokens(&text, t.span))).collect()
                });
                if let Some(texts) = spelled {
                    let texts: Vec<&[Token]> = texts.iter().map(Vec::as_slice).collect();
                    out.push(self.unknown_text(Unk::Of(&texts), t.span));
                    continue;
                }
                out.push(self.printed(t, false, t.span));
            } else {
                run.push(t);
            }
        }
        flush(self, &mut run, &mut out);
        out
    }

    /// `\escapechar` as a character. `Some(None)` names none, `None` is unknown.
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

    /// `\meaning` of a macro up to its replacement text:
    /// `⟨prefixes⟩macro:⟨parameter text⟩->` (tex.web §§ 294, 296).
    fn macro_head(&self, m: &crate::tex::MacroDef, escape: Option<char>) -> String {
        let mut prefix = String::new();
        for (set, word) in [(m.protected, "protected"), (m.long, "long"), (m.outer, "outer")] {
            if set {
                prefix.extend(escape);
                prefix.push_str(word);
            }
        }
        if !prefix.is_empty() {
            prefix.push(' ');
        }
        // tex.web § 294: parameter text ending in `#{` shows only the `{`.
        let mut parameters = String::new();
        for item in &m.parameter_text.items {
            match item {
                crate::tex::ParamItem::Param(n) => parameters.push_str(&format!("#{n}")),
                crate::tex::ParamItem::Lit(tok) => parameters.push_str(&crate::tex::detokenize_in(
                    &[Token::new(*tok, Span::default())],
                    &self.out.interner,
                    escape,
                    &self.catcodes,
                    crate::tex::Selector::NewString,
                )),
            }
        }
        if m.parameter_text.brace_end {
            parameters.push('{');
        }
        format!("{prefix}macro:{parameters}->")
    }

    /// `\meaning` writes `macro:⟨parameter text⟩->⟨replacement text⟩` for a
    /// macro, and the primitive's own name otherwise (The TeXbook, ch. 20).
    fn meaning_text(&mut self, sym: Sym) -> String {
        let escape = self.escape_char();
        let esc: String = escape.into_iter().collect();
        match self.env.meaning(sym) {
            // tex.web § 296 (with e-TeX's `\protected`): prefixes print before
            // `macro`, in the order protected, long, outer.
            Meaning::Macro(m) => {
                let head = self.macro_head(&m, escape);
                head + &crate::tex::detokenize_in(
                    &m.replacement_text,
                    &self.out.interner,
                    escape,
                    &self.catcodes,
                    crate::tex::Selector::NewString,
                )
            }
            Meaning::Undefined => "undefined".into(),
            Meaning::Char(c, cat) => char_meaning_text(c, cat),
            // tex.web § 1224: a `\chardef` shows as `\char"⟨hex⟩`. Before the
            // unnumbered arm: code 65535 is a real character, not the marker.
            Meaning::Register(crate::tex::RegKind::Char, index) => format!("{esc}char\"{index:X}"),
            // tex.web § 1224: a `\mathchardef` shows as `\mathchar"<hex>`.
            // LuaTeX pads to four digits (LuaTeX manual, "Math").
            Meaning::Register(crate::tex::RegKind::MathChar, index) => {
                if self.out.plugins.engine.has_luatex() {
                    format!("{esc}mathchar\"{index:04X}")
                } else {
                    format!("{esc}mathchar\"{index:X}")
                }
            }
            // A `\Umathchardef` shows class, family and slot apart. `\ifx` tells
            // it from a `\mathchardef` (XeTeX reference; LuaTeX manual, "Math").
            Meaning::Register(crate::tex::RegKind::UMathChar, _) => {
                let code = self.env.value(sym).as_int().unwrap_or(0);
                let engine = self.out.plugins.engine;
                let (class, family, slot) = crate::builtins::split_math_code(code, engine);
                if engine.has_luatex() {
                    format!("{esc}Umathchar\"{class:X}\"{family:02X}\"{slot:06X}")
                } else {
                    format!("{esc}Umathchar\"{class:X}\"{family:X}\"{slot:X}")
                }
            }
            Meaning::Register(kind, crate::tex::UNNUMBERED) => format!("{esc}{}", kind.as_str()),
            Meaning::Register(kind, index) => format!("{esc}{}{index}", kind.as_str()),
            // tex.web § 296: a primitive shows its own name, so
            // `\csname undefined\endcsname` reads back as `\relax`.
            Meaning::Primitive(Primitive::Relax) => format!("{esc}relax"),
            // tex.web § 296: a mark command shows a colon and the mark's text,
            // empty until a page is built.
            Meaning::Primitive(p @ Primitive::Convert(crate::builtins::Convert::Mark(_))) => {
                let name = primitive_name(self.out.plugins.engine, p);
                let original = self.env.identity(sym);
                format!("{esc}{}:", name.as_deref().unwrap_or_else(|| self.name(original)))
            }
            // tex.web § 1261: a font identifier shows the font it selects
            // (expl3's `\__cctab_chk_if_valid_aux:NTF` looks for it).
            Meaning::Primitive(Primitive::FontIdent(font)) => {
                format!("select font {}", self.font_name_text(font))
            }
            // tex.web § 1222: a `\let` copy of a primitive still prints its own
            // name. pgfkeys tests `\meaning` of its `\expanded` copy for that.
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
        // tex.web § 532: a job read from no file is `texput`.
        if path == crate::machine::NO_FILE {
            return "texput".into();
        }
        std::path::Path::new(&path).file_stem().and_then(|stem| stem.to_str()).unwrap_or("texput").to_string()
    }

    /// Characters produced by the gullet are `other`, except the space.
    pub(crate) fn text_tokens(&mut self, text: &str, span: Span) -> Vec<Token> {
        text.chars()
            .map(|c| {
                let catcode = if c == ' ' { Catcode::Space } else { Catcode::Other };
                Token::new(Tok::Chr(c, catcode), span)
            })
            .collect()
    }

    /// `\csname` reads with `get_x_token`, so it stops at the `\endcsname`
    /// expansion produces (tex.web § 372). This keeps `\expandafter\endcsname`.
    fn build_csname(&mut self, span: Span) {
        let (name, at, files) = self.scan_csname_at(span);
        // A name built from unknown text is known by the text around it.
        let name = match name {
            Ok(name) => name,
            Err((prefix, suffix, digits, number)) => {
                let token = self.unknown_name_token(&prefix, &suffix, digits, number, at);
                return self.unread(token);
            }
        };
        // One unknown digit makes one of ten names, an unknown character
        // of a number one of its characters. The token holds one of their
        // meanings (an undefined one made `\relax`, tex.web § 372).
        if name.contains(|c| crate::tex::unknown_number_chars(c).is_some()) {
            let token = self.one_of_names(&name, at);
            return self.unread(token);
        }
        self.last_named_cs = Some(self.intern(&name));
        crate::observe::csname(self, &name, span);
        let sym = self.intern(&name);
        if let Meaning::Register(kind, _) = self.env.shape(sym) {
            crate::observe::arg_type(self, at, crate::roles::Ty::Named(kind));
        }
        let relax = Meaning::Primitive(Primitive::Relax);
        if !self.env.is_defined(sym) {
            // A name of a family a loop may have defined (`\a5` after a
            // widened `\a\the\n`) is that definition or `\relax`.
            let binding = self.wild_binding(sym);
            self.env.set(sym, binding, false);
        } else if let Some(b) = self.env.slot(sym)
            && let Some(may) = b.may.as_ref().filter(|may| may.contains(&Meaning::Undefined))
        {
            // Defined on some paths only: on the others it is now `\relax`.
            let mut binding = b.clone();
            let mut meanings: Vec<Meaning> = may.iter().filter(|m| **m != Meaning::Undefined).cloned().collect();
            if !meanings.contains(&relax) {
                meanings.push(relax);
            }
            // Two meanings now, both defined: read as either, never as the
            // one the join kept.
            if meanings.len() > 1 {
                binding.meaning = Meaning::Unknown;
            }
            binding.may = Some(meanings.into());
            self.env.set(sym, binding, false);
        }
        // The token stands where the document wrote part of the name. Only a
        // name whose characters it all wrote is its own.
        if self.own_file(at.file) && !files.iter().all(|&f| self.own_file(f)) {
            self.carried.foreign_names.insert((sym, at));
        }
        self.built_name = Some((sym, at));
        self.unread(Token::new(Tok::Cs(sym), at));
    }

    /// An undefined name a family write may have reached
    /// ([`crate::machine::Wild`]): `\relax` or one of the family's meanings.
    fn wild_binding(&self, sym: Sym) -> Binding {
        let relax = Meaning::Primitive(Primitive::Relax);
        match self.wild.meanings(self.name(sym)) {
            Some(ms) if ms.is_empty() => Binding::builtin(relax),
            Some(mut ms) => {
                if !ms.contains(&relax) {
                    ms.insert(0, relax);
                }
                let mut b = Binding::builtin(Meaning::Unknown);
                b.may = Some(ms.into());
                b
            }
            None => Binding::builtin(Meaning::Unknown),
        }
    }

    /// Token for a `\csname` name with unknown digits: one of the names they
    /// make, each possibly made `\relax`.
    fn one_of_names(&mut self, name: &str, at: Span) -> Token {
        let mut candidates = vec![String::new()];
        for c in name.chars() {
            // What [`crate::tex::may_equal_unknown_char`] lets each be.
            let options: Vec<char> = crate::tex::unknown_number_chars(c).map_or(vec![c], |cs| cs.chars().collect());
            candidates =
                candidates.iter().flat_map(|p| options.iter().map(move |o| format!("{p}{o}"))).take(10_000).collect();
        }
        let mut meanings: Vec<Meaning> = Vec::new();
        for candidate in &candidates {
            let sym = self.intern(candidate);
            let meaning = match self.env.meaning(sym) {
                Meaning::Undefined if self.may_be_real(sym) => {
                    let binding = self.wild_binding(sym);
                    for m in binding.may.as_deref().unwrap_or(std::slice::from_ref(&binding.meaning)) {
                        if !meanings.contains(m) {
                            meanings.push(m.clone());
                        }
                    }
                    continue;
                }
                Meaning::Undefined => {
                    let (prefix, suffix) =
                        name.split_once(|c| crate::tex::unknown_number_chars(c).is_some()).unwrap_or((name, ""));
                    self.wild.add(prefix.to_string(), suffix.to_string(), false, None);
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
            // The set is exact, not a join. It is kept whole, up to ten meanings.
            if meanings.len() <= 10 && !meanings.contains(&Meaning::Unknown) {
                b.may = Some(meanings.into());
            }
            b
        };
        self.env.set(sym, binding, true);
        Token::new(Tok::Cs(sym), at)
    }

    /// The name `\csname` and `\ifcsname` build: every character token up to
    /// `\endcsname`, read with `get_x_token` (tex.web § 372, etex.ch). The first
    /// unexpandable control sequence ends it, and TeX inserts `\endcsname` there.
    fn scan_csname_text(&mut self, span: Span) -> Option<String> {
        self.scan_csname_at(span).0.ok()
    }

    /// The name, where its first character was written, and the files its
    /// characters came from. An unknown part leaves only the text around it.
    #[allow(clippy::type_complexity)]
    fn scan_csname_at(
        &mut self,
        span: Span,
    ) -> (Result<String, crate::machine::Gapped>, Span, Vec<crate::tex::FileId>) {
        let depth = crate::commands::enter_csname();
        let found = self.scan_csname_chars(span);
        crate::commands::leave_csname(depth);
        found
    }

    #[allow(clippy::type_complexity)]
    fn scan_csname_chars(
        &mut self,
        span: Span,
    ) -> (Result<String, crate::machine::Gapped>, Span, Vec<crate::tex::FileId>) {
        let mut start = Accum::for_name();
        start.name.as_deref_mut().expect("a name scan").site = span;
        let mut acc = match self.cfg.arm_joins {
            true => self.scan_paths(Unit::Name, start),
            false => {
                let mut acc = start;
                self.scans.push(acc);
                loop {
                    let done = self.name_step();
                    if done || self.halted || self.out.exhausted {
                        break;
                    }
                }
                acc = self.scans.pop().unwrap_or_else(Accum::for_name);
                acc
            }
        };
        // A name that is one of several: a path per name, on which the
        // command goes on. Each replay runs the scan's own choices again,
        // so a site that parted `meaning_set` paths since source was read
        // keeps the name whole: the family the names share.
        let names = acc.name.as_deref().map_or(0, |n| n.names.len());
        if names > 1 && self.site_forked(span) >= u32::from(self.limits.meaning_set) {
            acc.name.as_deref_mut().expect("the name scan").collapse();
        } else if names > 1 {
            let chosen = self.split(self.unknown, span, names, |_, arm| {
                let n = acc.name.as_deref_mut().expect("the name scan");
                // A replayed choice may count other arms: then any name.
                match n.names.get(arm).cloned() {
                    Some(one) => n.names = vec![one],
                    None => n.collapse(),
                }
                true
            });
            if !chosen {
                acc.name.as_deref_mut().expect("the name scan").collapse();
            }
        }
        let built = acc.built();
        let n = acc.name.as_deref().cloned().unwrap_or_default();
        if !n.closed {
            let why = n.stopped.map_or_else(
                || "the input ends".to_string(),
                |sym| format!("\\{} ({}) cannot be expanded", self.name(sym), self.env.meaning(sym).kind()),
            );
            // tex.web § 373: `\endcsname` is inserted, the name is what
            // was built, and the token that stopped it is read again.
            self.diagnose(
                Severity::Warning,
                "missing-endcsname",
                span,
                format!("\\csname has no \\endcsname: {why} after `{}`", n.names[0].1),
            );
        }
        (built, n.at.unwrap_or(span), n.files)
    }

    /// One token of the name a `\csname` builds, on the scan on top of
    /// `scans`: whether the name is complete.
    pub(crate) fn name_step(&mut self) -> bool {
        let Some(t) = self.next_token() else { return true };
        let file_call = self.file_call;
        let acc = self.scans.last_mut().and_then(|a| a.name.as_deref_mut());
        let Some(n) = acc else { return true };
        let site = n.site;
        let cs = match t.tok {
            // A brace after an unknown part: the name is unknown whatever
            // TeX read into it, and what follows the brace is left where
            // it is rather than eaten into the name (containment). The
            // `\endcsname` is taken as inserted here.
            Tok::Chr(_, Catcode::Begin | Catcode::End) if n.names.iter().any(|(prefix, _)| prefix.is_some()) => {
                n.closed = true;
                self.unread(t);
                self.diagnose(
                    Severity::Assumption,
                    "unknown-name-brace",
                    t.span,
                    "a brace after an unknown part of a \\csname name: the name ends there and is unknown".into(),
                );
                return true;
            }
            Tok::Chr(c, cat) if cat != Catcode::Active => {
                n.push(c);
                if !n.files.contains(&t.span.file) {
                    n.files.push(t.span.file);
                }
                // A character the calling file wrote in this call names
                // the place.
                let written = file_call.is_some_and(|(_, call)| {
                    call.file == t.span.file && (t.span.line, t.span.col) >= (call.line, call.col)
                });
                if t.span.line > 0 && (n.at.is_none() || written && !n.at_written) {
                    n.at = Some(t.span);
                    n.at_written = written;
                }
                return false;
            }
            Tok::Param(_) => return false,
            _ => t.cs().or_else(|| self.active_cs(t)),
        };
        let Some(sym) = cs else { return false };
        if matches!(self.env.meaning(sym).prim(), Some(Primitive::Endcsname)) {
            self.scans.last_mut().and_then(|a| a.name.as_deref_mut()).expect("the name scan").closed = true;
            return true;
        }
        // One of several texts, or a name of several meanings: a path
        // per one where the run may part. Read past `LOOP_ENTRIES` times at
        // this `\csname` here, the token is an unknown part of the name: a
        // summary would join the arms into the token again.
        if !self.loop_capped(site, sym) && self.split_one_of_at(t, site) {
            return false;
        }
        // A path per meaning, each reading the name on with its own: read
        // again, the token would meet the joined meaning once more.
        if matches!(self.env.meaning(sym), Meaning::Unknown) {
            self.split_meaning(sym, t.span);
        }
        // tex.web § 372: the name is spelled from the number's digits. A
        // number in a small interval is each of its values on a path of its
        // own, so each path builds the one name TeX builds.
        if let Some(num) = self.typed_digits_of(sym)
            && num.hi.saturating_sub(num.lo) < i64::from(self.limits.meaning_set)
            && !self.loop_capped(site, sym)
        {
            let count = usize::try_from(num.hi - num.lo + 1).unwrap_or(0);
            if self.split(sym, t.span, count, |m, arm| {
                let v = num.lo + arm as i64;
                m.refine_value(sym, crate::value::Value::from_num(crate::value::Quantity::Int, Num::exact(v)));
                let digits = m.text_tokens(&v.to_string(), t.span);
                m.unread_all(&digits);
                true
            }) {
                return false;
            }
        }
        // One digit is one character: the name is one of ten.
        if let Some(num) = self.typed_digits_of(sym).filter(|n| n.lo >= 0 && n.hi <= 9) {
            let c = if num.lo > 0 { crate::tex::UNKNOWN_NONZERO } else { crate::tex::UNKNOWN_DIGIT };
            self.unread(Token::new(Tok::Chr(c, Catcode::Other), t.span));
            return false;
        }
        // Unknown text of characters only (typed, no control sequence) is
        // part of the name, as unknown digits are.
        if self.is_unknown_digits(sym)
            || matches!(self.env.meaning(sym), Meaning::Unknown)
            || self.typed_text_of(t).is_some()
        {
            let digits = self.is_unknown_digits(sym);
            let number = self.typed_digits_of(sym).map(|_| sym);
            self.scans
                .last_mut()
                .and_then(|a| a.name.as_deref_mut())
                .expect("the name scan")
                .unknown_part(digits, number);
            return false;
        }
        // tex.web § 370: expanding an undefined control sequence reports
        // it and removes it, so the name goes on after it.
        if matches!(self.env.meaning(sym), Meaning::Undefined) {
            let node = self.reference(sym, t.span);
            let kind = self.undefined_kind();
            self.note_expansion(sym, t.span, kind, node, Vec::new());
            self.record(Step::Undefined, sym, t.span);
            return false;
        }
        self.unread(t);
        if !self.expandable(sym) || !self.step() {
            self.scans.last_mut().and_then(|a| a.name.as_deref_mut()).expect("the name scan").stopped = Some(sym);
            return true;
        }
        false
    }

    /// A conditional is read one token at a time. `pass_text` skips the untaken
    /// arm and `\else`/`\fi` stay in the stream, so a macro delimited by `\fi`
    /// still sees it (tex.web §§ 489-500). An undecided one runs every arm.
    fn conditional(&mut self, cond: Cond, by: Sym, span: Span, negate: bool) {
        self.scratch.cond_gaps.insert(span, self.gaps_raised);
        self.command_level = false;
        // `satex controls` asks what a switch governs, which only has an
        // answer if the switch is left undecided.
        let opaque = self.cfg.opaque_conditionals.iter().any(|wanted| {
            wanted == self.name(by)
                // Only the document's own switches. The kernel's and packages' drive
                // code never meant to run both ways.
                || (wanted == crate::config::EVERY_SWITCH && self.carried.own_switches.contains(&by) && self.is_switch(by))
        });
        let kind = self.if_type(cond, by, negate);
        let cond = if opaque { Cond::Opaque } else { cond };
        if cond == Cond::Case {
            // tex.web § 498 (`save_cond_ptr`): where this conditional will sit,
            // below any the test itself opens.
            let base = self.conds.len();
            self.testing.push(base);
            self.testing_kind.push(kind);
            self.abs = None;
            self.abs_exact = None;
            let selector = self.scan_number();
            // An unknown selector whose interval lies in one case range decides
            // it: all negative (`\else`, as -1 selects it) or one value.
            // Otherwise only the arms in its interval are split.
            let range = selector.map(Num::exact).or_else(|| self.abs.take());
            self.abs_exact = None;
            self.testing_kind.pop();
            self.testing.pop();
            let decided = match &range {
                Some(num) if num.hi < 0 => Some(-1),
                Some(num) if num.lo == num.hi => Some(num.lo),
                _ => None,
            };
            return match selector.or(decided) {
                Some(n) => self.case_conditional(n, by, span, base, kind, None),
                None => self.undecided_case(by, span, base, kind, range),
            };
        }
        if cond == Cond::IfX {
            self.split_ifx();
        }
        let mut operands = vec![by];
        // tex.web § 498: the conditional is on the stack while its test is
        // read, below any the test starts.
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
        // A conditional left open on request (`satex controls`) needs only what
        // each arm does. A path per arm multiplies paths through every loop
        // testing it. A mode test splits the modes between its arms.
        let refine = self.mode_parts(cond).map(|(yes, no)| if negate { (no, yes) } else { (yes, no) });
        // What the test teaches each arm of the register it read.
        let narrow = narrowing.map(|(sym, quantity, num, exact, relation, bound)| {
            // While a join left the register a set of values or intervals,
            // each arm keeps each member's part that still satisfies the
            // test, not just the hull's (the set lift narrows too).
            let members = self.members_of(sym, exact.as_deref(), num);
            let arm = |holds: bool| {
                let holds = holds != negate;
                let bound = if holds { bound.0 } else { bound.1 };
                if let Some(members) = &members {
                    let kept: Vec<Value> = members
                        .iter()
                        .filter_map(|m| m.narrow(relation, bound, holds))
                        .map(|n| Value::from_num(quantity, n))
                        .collect();
                    if !kept.is_empty() {
                        return match kept.len() {
                            1 => kept.into_iter().next(),
                            _ => Some(Value::OneOf(kept.into())),
                        };
                    }
                }
                num.narrow(relation, bound, holds).map(|n| Value::from_num(quantity, n))
            };
            (sym, arm(true), arm(false))
        });
        match taken {
            Some(holds) => self.enter_arm(holds, by, span, base, kind, None),
            None => self.undecided(by, span, base, kind, Some((refine, narrow))),
        }
    }

    /// tex.web § 507 for one member meaning of each `\ifx` operand:
    /// `None` where unknown text leaves it open.
    fn ifx_pair(&self, x: &Meaning, y: &Meaning) -> Option<bool> {
        match (x, y) {
            (Meaning::Unknown, _) | (_, Meaning::Unknown) => None,
            // tex.web § 507: undefined equals undefined only.
            (Meaning::Undefined, _) | (_, Meaning::Undefined) => Some(x == y),
            (Meaning::Macro(p), Meaning::Macro(q)) => {
                if Rc::ptr_eq(p, q) {
                    return Some(true);
                }
                if !self.has_unknown(&p.replacement_text) && !self.has_unknown(&q.replacement_text) {
                    return Some(x == y);
                }
                (!self.texts_may_equal(&p.replacement_text, &q.replacement_text)).then_some(false)
            }
            // A macro is never a character or a primitive.
            (Meaning::Macro(_), _) | (_, Meaning::Macro(_)) => Some(false),
            (Meaning::Char(c, k), Meaning::Char(d, l))
                if k == l && crate::tex::may_equal_unknown_char(*c as u32, *d as u32) =>
            {
                None
            }
            _ => Some(x == y),
        }
    }

    /// `tokens` with typed digits a test narrowed to one number spelled
    /// out: that is the text TeX holds there.
    fn settled<'a>(&self, tokens: &'a [Token]) -> std::borrow::Cow<'a, [Token]> {
        let one = |t: &Token| {
            let sym = t.cs()?;
            let place = self.typed_slice(sym).is_none_or(|(_, _, i, j)| j == i + 1);
            self.typed_digits_of(sym).filter(|n| place && n.lo == n.hi).map(|n| n.lo)
        };
        if !tokens.iter().any(|t| one(t).is_some()) {
            return std::borrow::Cow::Borrowed(tokens);
        }
        let spell = |t: &Token| match one(t) {
            Some(n) => n.to_string().chars().map(|c| Token::new(Tok::Chr(c, Catcode::Other), t.span)).collect(),
            None => vec![*t],
        };
        std::borrow::Cow::Owned(tokens.iter().flat_map(spell).collect())
    }

    /// Whether two texts may be one: unknown parts stand for any text
    /// they allow.
    fn texts_may_equal(&self, p: &[Token], q: &[Token]) -> bool {
        if !self.has_unknown(p) && !self.has_unknown(q) {
            return p.iter().map(|t| t.tok).eq(q.iter().map(|t| t.tok));
        }
        let ((pmin, pmax), (qmin, qmax)) = (self.text_length(p), self.text_length(q));
        !(pmax.is_some_and(|m| m < qmin) || qmax.is_some_and(|m| m < pmin) || self.texts_differ(p, q))
    }

    /// What a test taught the path of `sym`: that it means `meaning`
    /// ([`crate::env::Env::refine`]). The other names of the join the path
    /// came from keep the arms that may mean it.
    pub(crate) fn refine(&mut self, sym: Sym, meaning: Meaning) {
        self.refine_to(sym, &[meaning]);
    }

    /// [`Machine::refine`] to one of `members`.
    pub(crate) fn refine_to(&mut self, sym: Sym, members: &[Meaning]) {
        let keep: Vec<bool> =
            self.env.arms(sym).iter().map(|arm| members.iter().any(|m| self.may_mean(arm, m))).collect();
        self.env.refine_to(sym, members, &keep);
    }

    /// [`Machine::refine`] for a register's value.
    pub(crate) fn refine_value(&mut self, sym: Sym, value: Value) {
        let keep: Vec<bool> =
            self.env.arms(sym).iter().map(|arm| arm.as_ref().is_none_or(|b| self.may_be(&b.value, &value))).collect();
        self.env.refine_value(sym, value, &keep);
    }

    /// Whether an arm's binding may mean `to`, as `\ifx` decides: no
    /// binding is undefined.
    fn may_mean(&self, arm: &Option<crate::env::Binding>, to: &Meaning) -> bool {
        let Some(arm) = arm else { return !matches!(to, Meaning::Macro(_)) };
        let members = arm.may.as_deref().unwrap_or(std::slice::from_ref(&arm.meaning));
        members.iter().any(|m| self.ifx_pair(m, to) != Some(false))
    }

    /// Whether value `arm` may be `to`.
    fn may_be(&self, arm: &Value, to: &Value) -> bool {
        match (arm, to) {
            (Value::OneOf(ms), v) => ms.iter().any(|m| self.may_be(m, v)),
            (v, Value::OneOf(ts)) => ts.iter().any(|t| self.may_be(v, t)),
            (Value::Toks(x), Value::Toks(y)) => self.texts_may_equal(x, y),
            (Value::Int(x), Value::Int(y)) => x == y,
            (Value::Dimen(x), Value::Dimen(y)) => x == y,
            _ => true,
        }
    }

    /// The meanings the first token of typed unknown text `sym` may have,
    /// one per kind of token its class holds (tex.web § 507 compares one
    /// token): `\\ifx` against `\\bgroup` then knows letters are no brace.
    /// `None` for other names, text that may be empty or hold control
    /// sequences, and a class whose tokens have one meaning already.
    pub(crate) fn typed_meanings(&self, sym: Sym) -> Option<Rc<[Meaning]>> {
        use crate::tex::{TextClass as C, UNKNOWN_DIGIT, UNKNOWN_LETTER, UNKNOWN_OTHER};
        let (class, one, cs) = crate::env::Typed::parse_name(self.name(sym))?;
        if !one || !cs.is_empty() || class.meets(C::CS) || class.one_char().is_some() {
            return None;
        }
        let kinds: [(C, &[Meaning]); 6] = [
            (C::DIGIT, &[Meaning::Char(UNKNOWN_DIGIT, Catcode::Other)]),
            (
                C::SIGN,
                &[
                    Meaning::Char('+', Catcode::Other),
                    Meaning::Char('-', Catcode::Other),
                    Meaning::Char('.', Catcode::Other),
                    Meaning::Char(',', Catcode::Other),
                ],
            ),
            (C::LETTER, &[Meaning::Char(UNKNOWN_LETTER, Catcode::Letter)]),
            // Other categories too, active characters among them.
            (C::OTHER, &[Meaning::Char(UNKNOWN_OTHER, Catcode::Other), Meaning::Unknown]),
            (C::SPACE, &[Meaning::Char(' ', Catcode::Space)]),
            (C::BRACE, &[Meaning::Char('{', Catcode::Begin), Meaning::Char('}', Catcode::End)]),
        ];
        Some(kinds.into_iter().filter(|(k, _)| class.meets(*k)).flat_map(|(_, ms)| ms.iter().cloned()).collect())
    }

    /// The meanings a name may have: its members, or its one meaning.
    /// Read untracked: the test records what of each meaning it reads.
    fn ifx_members(&mut self, t: Token) -> Vec<Meaning> {
        let sym = t.cs().or_else(|| self.active_cs(t));
        let tracking = self.env.track_reads(false);
        let members = match sym.and_then(|s| self.env.slot(s)?.may.clone().or_else(|| self.typed_meanings(s))) {
            Some(may) => may.to_vec(),
            None => vec![match (t.tok, sym) {
                (Tok::Chr(c, cat), None) if cat != Catcode::Active => Meaning::Char(c, cat),
                (_, Some(sym)) => self.env.meaning(sym),
                _ => Meaning::Unknown,
            }],
        };
        self.env.track_reads(tracking);
        members
    }

    /// Split `\ifx` on operands with multiple meanings, comparing each separately.
    fn split_ifx(&mut self) {
        let Some(a) = self.next_token() else { return };
        let marked = self.read_noexpanded(a);
        // One of several texts: the operand is that text's first token, on
        // a path per text.
        if !marked && self.split_one_of(a) {
            return self.split_ifx();
        }
        let b = self.next_token();
        let b_marked = b.is_some_and(|b| self.read_noexpanded(b));
        // A list library code tests against an empty macro: per body
        // ([`Machine::spread_joined_body`]), as LaTeX's list loops need. The
        // document's own test stays undecided, its text read per arm.
        if let Some(b) = b.filter(|b| !self.own_file(a.span.file) && !self.own_file(b.span.file)) {
            let empty = |m: &Self, t: Token| {
                t.cs().is_some_and(|s| {
                    m.env
                        .meaning(s)
                        .as_macro()
                        .is_some_and(|d| d.replacement_text.is_empty() && d.parameter_text.items.is_empty())
                })
            };
            // Only two macros can have a body to spread: what tells a
            // macro from the rest is all this reads of them.
            let macro_ = |m: &Self, t: Token| t.cs().is_some_and(|s| m.env.shape(s).as_macro().is_some());
            if macro_(self, a) && macro_(self, b) {
                if empty(self, b) {
                    self.spread_joined_body(a);
                } else if empty(self, a) {
                    self.spread_joined_body(b);
                }
            }
        }
        let split_b = b.is_some_and(|b| self.split_one_of(b));
        // A look-ahead TeX does not make: both operands go back as they
        // were read, behind their markers.
        let back = |m: &mut Self, t: Token, marked: bool| {
            if marked { m.unread_noexpanded(t) } else { m.unread(t) }
        };
        if let Some(b) = b
            && !split_b
        {
            back(self, b, b_marked);
        }
        back(self, a, marked);
        if split_b {
            return self.split_ifx();
        }
        // Members that all compare alike decide the test: no path each.
        if let Some(b) = b.filter(|_| !marked) {
            let (xs, ys) = (self.ifx_members(a), self.ifx_members(b));
            let answers: Option<Vec<bool>> =
                xs.iter().flat_map(|x| ys.iter().map(move |y| (x, y))).map(|(x, y)| self.ifx_pair(x, y)).collect();
            if (xs.len() > 1 || ys.len() > 1) && answers.and_then(|a| alike(a.into_iter())).is_some() {
                return;
            }
            // Several meanings against one: a path per answer, not per member.
            for (t, other) in [(a, &ys), (b, &xs)] {
                let sym = t.cs().or_else(|| self.active_cs(t));
                if let (Some(sym), [y]) = (sym, &other[..])
                    && self.split_meaning_by(sym, t.span, |m, x| m.ifx_pair(x, y))
                {
                    return;
                }
            }
        }
        for t in std::iter::once(a).chain(b) {
            if let Some(sym) = t.cs().or_else(|| self.active_cs(t)) {
                self.split_meaning(sym, t.span);
            }
        }
    }

    /// e-TeX's `\currentiftype` of a conditional (etex.ch: `cur_if + 1`,
    /// negated after `\unless`). `None` where satex models several alike.
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

    /// Undecided `\ifcase`: a path per selector in range. Negative values
    /// select `\else` (tex.web § 509).
    fn undecided_case(&mut self, by: Sym, span: Span, base: usize, kind: Option<i8>, range: Option<Num>) {
        let on = self.force_reference(by, span);
        self.record_with(Step::Condition, by, span, || Some("undecided"));
        self.note_conditional(by, span, None);
        self.note_test_end(span);
        let (lo, hi) = range.map_or((0, i64::MAX), |n| (n.lo.max(-1), n.hi));
        // A path per arm the selector may reach: the values past the last
        // `\or` (or below 0) all take the same one.
        let selectors: Option<Vec<i64>> = self.case_arms().map(|ors| {
            let last = ors as i64;
            let mut values: Vec<i64> = (lo.max(0)..=hi.min(last)).collect();
            if lo < 0 || hi > last {
                values.push(if lo < 0 { -1 } else { last + 1 });
            }
            values
        });
        let arms = selectors.as_ref().map_or(usize::MAX, Vec::len);
        let Some(arm) = self.branch(by, arms, span) else {
            return self.widen_conditional(by, span, base);
        };
        let n = match &selectors {
            Some(values) => values.get(arm).copied(),
            None => lo.checked_add(arm as i64).filter(|&n| n <= hi),
        };
        match n {
            Some(n) => self.case_conditional(n, by, span, base, kind, Some(on)),
            None => self.no_arm(),
        }
    }

    /// How many `\or`s the `\ifcase` about to be read has at its own level
    /// (tex.web § 509), read without expansion and put back. `None` when a
    /// name of unknown meaning may be a conditional, or no `\fi` is found.
    fn case_arms(&mut self) -> Option<usize> {
        let mut read: Vec<Token> = Vec::new();
        let (mut nesting, mut ors) = (crate::machine::CondDepth::default(), 0usize);
        let found = loop {
            if read.len() >= self.cfg.limits.conditional_tokens {
                break false;
            }
            let Some(t) = self.next_token() else { break false };
            read.push(t);
            let Some(sym) = t.cs().or_else(|| self.active_cs(t)) else { continue };
            match self.env.meaning(sym) {
                Meaning::Unknown => break false,
                m => match nesting.feed(m.prim()) {
                    Some(Primitive::Fi) => break true,
                    Some(Primitive::Or) => ors += 1,
                    _ => {}
                },
            }
        };
        self.unread_all(&read);
        found.then_some(ors)
    }

    /// When a skip crosses into another file (SEMANTICS.md § 14): one path
    /// ends the conditional, the other skips through nested levels to `\else`.
    fn skip_crossing(&mut self, nested: u8, to_fi: bool, by: Sym, span: Span) {
        self.command_level = false;
        self.split(by, span, 2, |m, arm| match arm {
            0 => true,
            1 => {
                let mut open = nested;
                while let Some((end, _)) = m.pass_text(to_fi, span) {
                    match end {
                        Primitive::Fi if open == 0 => break,
                        Primitive::Fi => open -= 1,
                        _ if open > 0 => {}
                        _ => {
                            let frame =
                                CondFrame { limit: CondLimit::Fi, dep: false, at: span, kind: None, many: false };
                            Rc::make_mut(&mut m.conds).push(frame);
                            break;
                        }
                    }
                }
                true
            }
            _ => false,
        });
    }

    /// Undecided condition: each arm is a path learning modes and intervals.
    pub fn undecided(&mut self, by: Sym, span: Span, base: usize, kind: Option<i8>, learn: Option<Learn>) {
        let on = self.force_reference(by, span);
        self.record_with(Step::Condition, by, span, || Some("undecided"));
        self.note_conditional(by, span, None);
        self.note_test_end(span);
        let Some(arm) = self.branch(by, 2, span) else {
            return self.widen_conditional(by, span, base);
        };
        let holds = arm == 0;
        if let Some((modes, narrow)) = learn {
            if let Some((yes, no)) = modes {
                self.mode = if holds { yes } else { no };
            }
            if let Some((sym, yes, no)) = narrow
                && let Some(value) = if holds { yes } else { no }
            {
                self.refine_value(sym, value);
            }
        }
        self.enter_arm(holds, by, span, base, kind, Some(on));
    }

    /// Enter the arm a condition selected (tex.web § 498). `base` is the
    /// stack depth when the test began (tex.web § 500).
    pub fn enter_arm(
        &mut self,
        holds: bool,
        by: Sym,
        span: Span,
        base: usize,
        kind: Option<i8>,
        on: Option<crate::env::NodeId>,
    ) {
        let base = base.min(self.conds.len());
        let dep = on.is_some();
        let open = |m: &mut Self, limit: CondLimit, at: usize| {
            if let Some(on) = on {
                Rc::make_mut(&mut m.cds).push(ControlDep { on, taken: holds });
            }
            Rc::make_mut(&mut m.conds).insert(at, CondFrame { limit, dep, at: span, kind, many: false });
        };
        if holds {
            self.record_with(Step::Branch, by, span, || Some("true"));
            self.note_conditional(by, span, Some(Some(0)));
            return open(self, CondLimit::Else, base);
        }
        self.record_with(Step::Branch, by, span, || Some("false"));
        match self.pass_outer(false, span, base) {
            Some((Primitive::Else | Primitive::Or, mark)) => {
                self.note_conditional(by, span, Some(Some(1)));
                self.note_cond_mark(span, mark, false);
                let at = self.conds.len();
                open(self, CondLimit::Fi, at)
            }
            found => {
                self.note_conditional(by, span, Some(None));
                if let Some((_, mark)) = found {
                    self.note_cond_mark(span, mark, true);
                }
                self.close_conditional(span)
            }
        }
    }

    /// Past the choices one command may make, an undecided conditional
    /// widens: both arms are skipped to its `\fi`, what their code may do
    /// is done ([`Machine::may_run`]), and it gives unknown text, so no
    /// path is dropped.
    fn widen_conditional(&mut self, by: Sym, span: Span, base: usize) {
        // Refused because its site was cut: the expansion goes as a whole.
        if self.cutting() {
            return;
        }
        let message = format!(
            "\\{} could not be decided past the choices one command may make; it is taken as unknown text",
            self.name(by)
        );
        self.diagnose(Severity::Assumption, "undecided-condition", span, message);
        self.record_with(Step::Branch, by, span, || Some("widened"));
        let outer = self.skip_log.replace(Vec::new());
        self.pass_outer(true, span, base);
        let skipped = std::mem::replace(&mut self.skip_log, outer).unwrap_or_default();
        if let Some(log) = &mut self.skip_log {
            log.extend_from_slice(&skipped);
        }
        self.may_run(by, &[], &[&skipped], span);
        if self.scanning > 0 {
            self.scan_undecided = true;
        } else {
            self.unread_unknown(span);
        }
    }

    /// `\ifcase`: skip to the selected arm, or run them all when the number
    /// is not known (tex.web § 509).
    fn case_conditional(
        &mut self,
        n: i64,
        by: Sym,
        span: Span,
        base: usize,
        kind: Option<i8>,
        on: Option<crate::env::NodeId>,
    ) {
        self.record_with(Step::Branch, by, span, || Some(format!("case {n}")));
        let mut remaining = if n < 0 { i64::MAX } else { n };
        let base = base.min(self.conds.len());
        let mut marks = Vec::new();
        let limit = loop {
            if remaining == 0 {
                break Some(CondLimit::Or);
            }
            // tex.web § 509: the selector's own test may leave a conditional open.
            match self.pass_outer(false, span, base) {
                Some((Primitive::Or, mark)) => {
                    remaining -= 1;
                    marks.push((mark, false));
                }
                // Past the last arm every selector is the same path.
                Some(_) | None if on.is_some() && remaining > 1 && n >= 0 => return self.no_arm(),
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
        // tex.web § 509 ends with `change_if_limit(or_code, save_cond_ptr)`:
        // the frame belongs below the ones the test left open.
        if let Some(on) = on {
            crate::push_shared(&mut self.cds, ControlDep { on, taken: n == 0 });
        }
        Rc::make_mut(&mut self.conds).insert(base, CondFrame { limit, dep: on.is_some(), at: span, kind, many: false });
    }

    /// tex.web § 510: an `\else`, `\or` or `\fi` met while its own test is read
    /// (`\ifnum…>20\else`) waits. An unredefinable `\relax` ends the test first.
    fn insert_relax(&mut self, by: Sym, span: Span) {
        let relax = self.intern("\u{4}primitive.relax");
        self.env.set(relax, Binding::builtin(Meaning::Primitive(Primitive::Relax)), true);
        self.unread(Token::new(Tok::Cs(by), span));
        self.unread(Token::new(Tok::Cs(relax), span));
    }

    /// tex.web § 1100: an output routine of a document that compiles
    /// closes only the conditionals it opened. Library code that would
    /// close one opened before it runs on approximated values: the path
    /// is no run of the document.
    fn closes_before_output(&mut self, span: Span) -> bool {
        let Some(opened) =
            self.env.nests().flatten().find(|n| n.code == Some(crate::mode::group::OUTPUT)).map(|n| n.conds)
        else {
            return false;
        };
        if self.conds.len() > opened as usize
            || self.own_file(span.file)
            || !self.cfg.compiles()
            || self.probe.is_some()
        {
            return false;
        }
        self.dropping = Some("the output routine closes a conditional opened before it".into());
        self.dropping_possible = false;
        true
    }

    /// `\else` and `\or`: the taken arm is over.
    fn conditional_else(&mut self, span: Span) {
        self.unfold_conditional(span);
        if self.closes_before_output(span) {
            return;
        }
        if let Some(frame) = self.conds.last() {
            self.note_cond_mark(frame.at, span, false);
        }
        match Rc::make_mut(&mut self.conds).last_mut() {
            None => {
                // tex.web § 510: "Extra \else", ignored.
                self.diagnose(Severity::Warning, "extra-else", span, "\\else or \\or outside a conditional".into())
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

    /// The innermost conditional stands for any number of them and a
    /// closer meets it: a path where it is one of them, and one where there
    /// are none. Where no more paths may part, the one below joins it when
    /// alike, so one path covers both.
    fn unfold_conditional(&mut self, span: Span) {
        if !self.conds.last().is_some_and(|c| c.many) {
            return;
        }
        let by = self.unknown;
        let unfold = |m: &mut Self, arm: usize| {
            let conds = Rc::make_mut(&mut m.conds);
            match arm {
                0 => conds.push(CondFrame { many: false, dep: false, ..conds.last().expect("checked").clone() }),
                _ => {
                    if conds.pop().is_some_and(|c| c.dep) {
                        Rc::make_mut(&mut m.cds).pop();
                    }
                }
            }
            true
        };
        if self.split(by, span, 2, unfold) {
            return;
        }
        let n = self.conds.len();
        let alike = n >= 2 && {
            let (a, b) = (&self.conds[n - 2], &self.conds[n - 1]);
            (a.limit, a.at, a.kind) == (b.limit, b.at, b.kind)
        };
        if alike {
            unfold(self, 1);
            Rc::make_mut(&mut self.conds).last_mut().expect("alike").many = true;
        } else {
            self.diagnose(
                Severity::Assumption,
                "unbalanced-path",
                span,
                "a closer met any number of open conditionals and no more paths could part; it is assumed one of them"
                    .into(),
            );
        }
        unfold(self, 0);
    }

    /// `\fi` ends the level.
    pub(crate) fn conditional_fi(&mut self, span: Span) {
        self.unfold_conditional(span);
        if self.closes_before_output(span) {
            return;
        }
        if self.conds.is_empty() {
            // tex.web § 510: "Extra \fi", ignored.
            self.diagnose(Severity::Warning, "extra-fi", span, "\\fi outside a conditional".into());
            return;
        }
        if let Some(frame) = self.conds.last() {
            self.note_cond_mark(frame.at, span, true);
        }
        self.pop_conditional();
    }

    pub(crate) fn pop_conditional(&mut self) {
        if let Some(frame) = Rc::make_mut(&mut self.conds).pop() {
            self.close_conditional(frame.at);
            if frame.dep {
                Rc::make_mut(&mut self.cds).pop();
            }
        }
    }

    /// A conditional over several lines of a file is one construct, from
    /// the `\if…` to its `\fi`.
    fn close_conditional(&mut self, at: Span) {
        self.fi_mark = Some((at, self.conds.len()));
        if let Some(line) = self.source_extent(at) {
            self.out.graph.note_extent(at, line);
        }
    }

    /// One operand of `\if` or `\ifcat`: the next token, expanded, as a
    /// (character, category) pair. A control sequence is no character
    /// (tex.web § 506). `\ifcat\noexpand~\noexpand#1` tests for active.
    fn if_operand(&mut self) -> Option<Operands> {
        use crate::machine::classes::CONTROL_SEQUENCE;
        let one = |o: (u32, u8)| Some((vec![vec![o]], None));
        // tex.web § 506: a token behind a `\noexpand` marker is taken as it
        // stands, an active character as that character.
        let unexpanded = |m: &Self, token: Token| match token.tok {
            Tok::Chr(c, cat) => (c as u32, cat as u8),
            Tok::Cs(inner) => match m.env.meaning(inner) {
                Meaning::Char(c, cat) => (c as u32, cat as u8),
                _ => CONTROL_SEQUENCE,
            },
            _ => CONTROL_SEQUENCE,
        };
        loop {
            let token = self.next_token()?;
            let noexpanded = self.read_noexpanded(token);
            if self.split_empty_pending(token, noexpanded) {
                continue;
            }
            if let Some(operands) = self.abstract_operands(token, noexpanded) {
                return Some(operands);
            }
            if noexpanded {
                return one(unexpanded(self, token));
            }
            // tex.web § 506: an active character that is not behind a
            // `\noexpand` is taken by its meaning, as a control sequence is.
            let Some(sym) = token.cs().or_else(|| self.active_cs(token)) else {
                return one(match token.tok {
                    Tok::Chr(c, cat) => (c as u32, cat as u8),
                    _ => CONTROL_SEQUENCE,
                });
            };
            match self.env.meaning(sym) {
                // One of several meanings: the operands they give, when
                // none expands.
                Meaning::Unknown => {
                    self.approximated = true;
                    let may = self.env.slot(sym).and_then(|b| b.may.clone());
                    let operand = |m: &Meaning| match m {
                        Meaning::Char(c, cat) => Some((*c as u32, *cat as u8)),
                        Meaning::Unknown | Meaning::Undefined => None,
                        m if self.expandable_meaning(m) => None,
                        _ => Some(CONTROL_SEQUENCE),
                    };
                    let all: Option<Vec<(u32, u8)>> = may.and_then(|may| may.iter().map(operand).collect());
                    return Some((vec![all.unwrap_or(vec![UNKNOWN_OPERAND])], None));
                }
                Meaning::Primitive(Primitive::NoExpand) => {
                    let next = self.noexpanded_next()?;
                    if let Some(operands) = self.abstract_operands(next, true) {
                        return Some(operands);
                    }
                    return one(unexpanded(self, next));
                }
                // `\let\bgroup={` makes the control sequence that character.
                Meaning::Char(c, cat) => return one((c as u32, cat as u8)),
                meaning if self.expandable_meaning(&meaning) => {
                    self.unread(token);
                    if !self.step() {
                        return None;
                    }
                }
                _ => return one(CONTROL_SEQUENCE),
            }
        }
    }

    /// The operands of an abstract token `t` ([`Machine::token_class`]):
    /// what its first token may be, the rest of a one-of text put back.
    /// `None` for a concrete token; an unknown operand where its first
    /// token is not known without expanding it.
    fn abstract_operands(&mut self, t: Token, noexpanded: bool) -> Option<Operands> {
        let sym = t.cs()?;
        let one_of = self.one_of_members(t);
        if one_of.is_none() && !self.is_unknown_marker(sym) {
            return None;
        }
        self.approximated = true;
        let unknown = Some((vec![vec![UNKNOWN_OPERAND]], None));
        let Some(texts) = one_of else {
            return self.class_operands(t, noexpanded).map(|o| (vec![o], Some(t))).or(unknown);
        };

        // Per text, from its first token.
        let members: Option<Vec<Vec<(u32, u8)>>> =
            texts.iter().map(|x| self.class_operands(*x.first()?, noexpanded)).collect();
        let Some(members) = members else { return unknown };
        self.unread_rests(&texts, t.span);
        Some((members, Some(t)))
    }

    /// What a test of typed digits `t` against the character `c` teaches
    /// its arms: digits begin with `0` only where the number is 0, with
    /// `-` where it is negative, and one digit is the number itself.
    fn digit_narrowing(&self, t: Token, c: char) -> Option<Narrowing> {
        let sym = t.cs()?;
        let num = self.typed_digits_of(sym)?;
        let (relation, bound) = match c {
            '0' if num.lo >= 0 => ('=', 0),
            '-' => ('<', 0),
            '1'..='9' if num.lo >= 0 && num.hi <= 9 => ('=', i64::from(c as u8 - b'0')),
            _ => return None,
        };
        Some((sym, crate::value::Quantity::Int, num, None, relation, (bound, bound)))
    }

    /// A path per answer of a test that `t`'s texts decide apart: `true`
    /// on the first. `None` when no more paths may part here.
    fn split_answer(&mut self, t: Token) -> Option<bool> {
        let mut first = false;
        self.split(t.cs()?, t.span, 2, |_, arm| {
            first = arm == 0;
            true
        })
        .then_some(first)
    }

    /// A one-of token `t` with an empty text, read where one token is
    /// taken: two paths, one where it is empty, one where it is one of
    /// the other texts, put back. `false` for any other token, or when
    /// no more paths may part.
    fn split_empty(&mut self, t: Token) -> bool {
        if let Some((class, false)) = self.typed_text_of(t) {
            return self.split_rest(t, class);
        }
        let Some(texts) = self.one_of_members(t) else { return false };
        if !texts.iter().any(|x| x.is_empty()) {
            return false;
        }
        let full: Vec<Rc<[Token]>> = texts.into_iter().filter(|x| !x.is_empty()).collect();
        let Some(sym) = t.cs() else { return false };
        self.split(sym, t.span, 2, |m, arm| {
            match (arm, &full[..]) {
                (0, _) | (_, []) => {}
                (_, [one]) => m.unread_all(one),
                (_, texts) => {
                    let token = m.one_of(texts.to_vec(), t.span);
                    m.unread(token);
                }
            }
            true
        })
    }

    /// A typed rest `t` that may be empty, read where one token is taken:
    /// a path where it is empty and one where it is not, its copies still
    /// to be read narrowed alike (condition refinement).
    pub(crate) fn split_rest(&mut self, t: Token, class: crate::tex::TextClass) -> bool {
        let Some(sym) = t.cs() else { return false };
        let full = self.unknown_text(Unk::Class(class, true, self.text_len(t), &[]), t.span);
        self.split(sym, t.span, 2, |m, arm| {
            let to = (arm == 1).then_some(full);
            m.narrow_pending(t, to);
            if let Some(full) = to {
                m.unread(full);
            }
            true
        })
    }

    /// [`Machine::split_empty`] for an `\if` operand read with expansion.
    fn split_empty_pending(&mut self, t: Token, noexpanded: bool) -> bool {
        !noexpanded && self.split_empty(t)
    }

    /// The rests of one-of texts whose first token was read: the rest
    /// they share, or one token for either rest.
    fn unread_rests(&mut self, texts: &[Rc<[Token]>], span: Span) {
        let rests: Vec<Rc<[Token]>> = texts.iter().map(|x| Rc::from(&x[1.min(x.len())..])).collect();
        let first = &rests[0];
        if rests.iter().all(|r| r.iter().map(|t| t.tok).eq(first.iter().map(|t| t.tok))) {
            let first = first.clone();
            self.unread_all(&first);
        } else {
            let token = self.one_of(rests, span);
            self.unread(token);
        }
    }

    /// One operand of `\ifx`: the token, and for a one-of token
    /// ([`Machine::one_of`]) the first token of each text, the rests
    /// put back. Texts that may be empty leave it unknown.
    fn ifx_operand(&mut self) -> Option<(Token, Option<Vec<Token>>)> {
        let mut t = self.next_token()?;
        if self.split_empty(t) {
            t = self.next_token()?;
        }
        let Some(texts) = self.one_of_members(t) else { return Some((t, None)) };
        if texts.iter().any(|x| x.is_empty()) {
            return Some((t, None));
        }
        let firsts = texts.iter().map(|x| x[0]).collect();
        self.unread_rests(&texts, t.span);
        Some((t, Some(firsts)))
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
                let stream = four_bit(self.scan_number()?);
                self.stream_at_end(stream)
            }
            // tex.web § 505: a register no `\setbox` has filled is void.
            Cond::Box => {
                let n = self.scan_box_number()?;
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
            Cond::FontChar | Cond::InCsname | Cond::Primitive => self.eval_extra_condition(cond),
            // tex.web § 501; see [`crate::mode`].
            Cond::VMode | Cond::HMode | Cond::InnerMode | Cond::MathMode => self.mode_test(cond),
            Cond::Opaque => None,
            Cond::Dim | Cond::AbsDim => {
                self.abs_from = None;
                let a = self.scan_dimen();
                let xs = a.map(Num::exact).or_else(|| self.abs.take());
                self.abs_exact = None;
                let from = self.abs_from.take();
                let relation = self.scan_relation();
                let b = self.scan_dimen();
                let ys = b.map(Num::exact).or_else(|| self.abs.take());
                self.abs_exact = None;
                let from_b = self.abs_from.take();
                let ((xs, _, a, from), (ys, _, b, from_b)) =
                    absolute(cond == Cond::AbsDim, [(xs, None, a, from), (ys, None, b, from_b)]);
                self.decide_numbers(xs, None, relation, ys, None, (a, b), (from, from_b))
            }
            // A number read from a register a join left one of a few
            // values is compared member by member: the test is decided
            // when every member decides it alike.
            Cond::Num | Cond::Odd | Cond::AbsNum => {
                self.abs_from = None;
                self.abs_odd = None;
                let a = self.scan_number();
                let odd = self.abs_odd.take();
                let xs = a.map(Num::exact).or_else(|| self.abs.take());
                let from = self.abs_from.take();
                // A register (or sub-expression) a join left one of a few
                // exact values keeps them, the set lift for the decision
                // below.
                let xs_exact = a.map(|n| vec![n]).or_else(|| self.abs_exact.take());
                if cond == Cond::Odd {
                    let exact = xs_exact.as_ref().and_then(|xs| {
                        let mut odds = xs.iter().map(|n| n % 2 != 0);
                        let first = odds.next()?;
                        odds.all(|o| o == first).then_some(first)
                    });
                    return exact.or_else(|| xs?.odd()).or(odd);
                }
                let relation = self.scan_relation();
                let b = self.scan_number();
                let ys = b.map(Num::exact).or_else(|| self.abs.take());
                let from_b = self.abs_from.take();
                let ys_exact = b.map(|n| vec![n]).or_else(|| self.abs_exact.take());
                let ((xs, xs_exact, a, from), (ys, ys_exact, b, from_b)) =
                    absolute(cond == Cond::AbsNum, [(xs, xs_exact, a, from), (ys, ys_exact, b, from_b)]);
                self.decide_numbers(xs, xs_exact, relation, ys, ys_exact, (a, b), (from, from_b))
            }
            Cond::IfX => {
                // Read untracked: what of each meaning counts as read is
                // decided below. An operand that stands for one of several
                // texts is that text on a path per text (tex.web § 507
                // compares one token), so the test is decided per member.
                let (a, firsts_a) = self.ifx_operand()?;
                let tracking = self.env.track_reads(false);
                let ma = self.ifx_meaning(a);
                self.env.track_reads(tracking);
                let (b, firsts_b) = self.ifx_operand()?;
                let tracking = self.env.track_reads(false);
                let mb = self.ifx_meaning(b);
                self.env.track_reads(tracking);
                // One name compared with itself, both expandable or both not:
                // one meaning, whatever it is.
                if a.tok == b.tok
                    && firsts_a.is_none()
                    && firsts_b.is_none()
                    && matches!(ma, Meaning::Primitive(Primitive::DontExpand))
                        == matches!(mb, Meaning::Primitive(Primitive::DontExpand))
                    && a.cs().or_else(|| self.active_cs(a)).is_some_and(|s| !self.is_unknown_marker(s))
                {
                    return Some(true);
                }
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
                // Macros whose texts cannot be as long as each other, or begin
                // alike, differ whatever their unknown parts stand for.
                let bodies_differ = match (&ma, &mb) {
                    (Meaning::Macro(x), Meaning::Macro(y)) => {
                        let ((xmin, xmax), (ymin, ymax)) =
                            (self.text_length(&x.replacement_text), self.text_length(&y.replacement_text));
                        xmax.is_some_and(|m| m < ymin)
                            || ymax.is_some_and(|m| m < xmin)
                            || self.texts_differ(&x.replacement_text, &y.replacement_text)
                    }
                    _ => false,
                };
                let null = Meaning::Primitive(Primitive::FontIdent(0));
                let font_open = (self.font_may_be_null(&ma) && (mb == null || self.font_may_be_null(&mb)))
                    || (self.font_may_be_null(&mb) && ma == null);
                let (sa, sb) = (a.cs().or_else(|| self.active_cs(a)), b.cs().or_else(|| self.active_cs(b)));
                // A character whose code is unknown equals a character of its
                // category only perhaps, and nothing else.
                // Digits typed with their interval begin with a digit the
                // interval allows, and never with `-`.
                let typed_differ = |t: Token, m: &Meaning, me: &Self| -> bool {
                    let Some(num) = t.cs().and_then(|sym| me.typed_digits_of(sym)) else { return false };
                    // Typed digits are other characters, a digit or `-` first.
                    match m {
                        Meaning::Char(c, Catcode::Other) if *c == '-' || c.is_ascii_digit() => {
                            (*c == '-' && num.lo >= 0)
                                || (c.is_ascii_digit() && num.lo >= 0 && !crate::scan::first_digits(num).contains(c))
                        }
                        Meaning::Char(c, _) => !crate::tex::is_unknown_char(*c),
                        _ => false,
                    }
                };
                let char_open = matches!((&ma, &mb), (Meaning::Char(x, k), Meaning::Char(y, l))
                    if k == l && crate::tex::may_equal_unknown_char(*x as u32, *y as u32))
                    && !typed_differ(a, &mb, self)
                    && !typed_differ(b, &ma, self);
                // Typed digits against `0` or `-`: the arms learn the number
                // is zero or not, negative or not.
                if char_open {
                    let typed = |t: Token, m: &Meaning| Some((t.cs()?, self.typed_digits_of(t.cs()?)?, m.clone()));
                    // One digit known exactly is that character.
                    let same = |t: Token, m: &Meaning| {
                        typed(t, m).is_some_and(|(_, n, m)| {
                            n.lo == n.hi
                                && (0..=9).contains(&n.lo)
                                && m == Meaning::Char(char::from(b'0' + n.lo as u8), Catcode::Other)
                        })
                    };
                    if same(a, &mb) || same(b, &ma) {
                        return Some(true);
                    }
                    let digit = |t: Token, m: &Meaning, me: &Self| match m {
                        Meaning::Char(c, Catcode::Other) => me.digit_narrowing(t, *c),
                        _ => None,
                    };
                    self.narrowing = digit(a, &mb, self).or_else(|| digit(b, &ma, self));
                }
                // A name that holds one of several meanings is compared
                // member by member: decided when they all agree.
                let members = |m: &Meaning, sym: Option<Sym>, me: &Self| -> Vec<Meaning> {
                    match (m, sym.and_then(|s| me.env.slot(s)?.may.clone().or_else(|| me.typed_meanings(s)))) {
                        // Also a name with a meaning that may be undefined.
                        (_, Some(may)) => may.to_vec(),
                        // An undefined name a family write may have made
                        // `\relax` or given a meaning.
                        (Meaning::Undefined, None) if sym.is_some_and(|s| me.may_be_defined(s)) => {
                            let b = me.wild_binding(sym.expect("a name"));
                            let mut all = vec![Meaning::Undefined];
                            all.extend(b.may.as_deref().map_or(vec![b.meaning.clone()], <[Meaning]>::to_vec));
                            all
                        }
                        _ => vec![m.clone()],
                    }
                };
                // A one-of token is the first tokens of its texts.
                let firsts = |me: &mut Self, firsts: Option<Vec<Token>>, m: &Meaning, sym: Option<Sym>| match firsts {
                    Some(ts) => {
                        let mut out: Vec<Meaning> = Vec::new();
                        for t in ts {
                            let m = me.meaning_of(t);
                            if !out.contains(&m) {
                                out.push(m);
                            }
                        }
                        out
                    }
                    None => members(m, sym, me),
                };
                let (texts_a, texts_b) = (firsts_a.is_some(), firsts_b.is_some());
                let texts = texts_a || texts_b;
                let (xs, ys) = (firsts(self, firsts_a, &ma, sa), firsts(self, firsts_b, &mb, sb));
                if texts || xs.len() > 1 || ys.len() > 1 {
                    let pair = |x: &Meaning, y: &Meaning, me: &Self| me.ifx_pair(x, y);
                    let answers: Option<Vec<bool>> =
                        xs.iter().flat_map(|x| ys.iter().map(move |y| (x, y))).map(|(x, y)| pair(x, y, self)).collect();
                    if let Some(answer) = answers.and_then(|a| alike(a.into_iter())) {
                        return Some(answer);
                    }
                    // Texts that each decide it, some one way and some the
                    // other: a path per answer.
                    let decides = |xs: &[Meaning], ys: &[Meaning], me: &Self| {
                        xs.iter().all(|x| {
                            ys.iter()
                                .map(|y| pair(x, y, me))
                                .collect::<Option<Vec<bool>>>()
                                .and_then(|a| alike(a.into_iter()))
                                .is_some()
                        })
                    };
                    let parts = [(texts_a, a, decides(&xs, &ys, self)), (texts_b, b, decides(&ys, &xs, self))];
                    return parts.into_iter().find(|p| p.0 && p.2).and_then(|(_, t, _)| self.split_answer(t));
                }
                if matches!(ma, Meaning::Unknown)
                    || matches!(mb, Meaning::Unknown)
                    || font_open
                    || char_open
                    || (!same_macro && !bodies_differ && (vague(&ma, self) || vague(&mb, self)))
                {
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
                if sym == Some(self.unknown) {
                    return None;
                }
                // A name `\csname` built from unknown text is defined while
                // the meaning it gave the name holds: `\relax` where it can
                // spell no defined name (tex.web § 372), or one assigned since.
                if let Some(sym) = sym.filter(|&s| self.is_unknown_name(s)) {
                    let built =
                        self.name(sym).contains([crate::machine::UNKNOWN_NAME_GAP, crate::machine::UNKNOWN_DIGITS_GAP]);
                    return self.env.slot(sym).filter(|b| built && b.may.is_none()).and_then(|b| match b.meaning {
                        Meaning::Unknown | Meaning::Undefined => None,
                        _ => Some(true),
                    });
                }
                self.decide_defined(sym?)
            }
            Cond::Csname => {
                let at = self.peek().map(|t| t.span).unwrap_or_default();
                let name = self.scan_csname_text(at)?;
                crate::observe::read_name(self, &name);
                let sym = self.intern(&name);
                self.last_named_cs = Some(sym);
                operands.push(sym);
                self.decide_defined(sym)
            }
            Cond::Chars | Cond::Catcodes => {
                // Operands that may be several tokens are decided pair by
                // pair: true when all agree, false when none can.
                let (a, ta) = self.if_operand()?;
                let (b, tb) = self.if_operand()?;
                let by_code = cond == Cond::Chars;
                let (fa, fb) = (a.concat(), b.concat());
                let digit = |t: Option<Token>, other: &[(u32, u8)], m: &Self| match other {
                    [(c, 12)] => m.digit_narrowing(t?, char::from_u32(*c)?),
                    _ => None,
                };
                self.narrowing = digit(ta, &fb, self).or_else(|| digit(tb, &fa, self));
                if fa.contains(&UNKNOWN_OPERAND) || fb.contains(&UNKNOWN_OPERAND) {
                    return None;
                }
                if let Some(answer) = crate::machine::decide(&fa, &fb, by_code) {
                    return Some(answer);
                }
                // Texts that each decide it, some one way and some the
                // other: a path per answer.
                let decides = |xs: &[Vec<(u32, u8)>], ys: &[(u32, u8)]| {
                    xs.iter().all(|x| crate::machine::decide(x, ys, by_code).is_some())
                };
                let parts = [(ta, decides(&a, &fb)), (tb, decides(&b, &fa))];
                parts.into_iter().find_map(|(t, d)| t.filter(|_| d)).and_then(|t| self.split_answer(t))
            }
            Cond::Case => None,
        }
    }

    /// Whether `sym` is defined, for `\ifdefined` and `\ifcsname`: a name
    /// a join left one of several meanings is defined when every one of
    /// them is, undefined when none is; `None` when that differs.
    fn decide_defined(&mut self, sym: Sym) -> Option<bool> {
        if let Some(may) = self.env.slot(sym).and_then(|b| b.may.clone()) {
            self.env.note_read(sym, crate::env::ReadKind::Defined);
            let defined = alike(may.iter().map(|m| !matches!(m, Meaning::Undefined)));
            return defined.filter(|defined| *defined || !self.may_be_defined(sym));
        }
        Some(self.env.is_defined(sym)).filter(|defined| *defined || !self.may_be_defined(sym))
    }

    /// The meaning `\ifx` compares: tex.web § 358 reads a token behind a
    /// `\noexpand` marker as `relax` with `no_expand_flag` when it is
    /// expandable, a meaning no other token has.
    fn ifx_meaning(&mut self, token: Token) -> Meaning {
        let meaning = match self.meaning_of(token) {
            Meaning::Macro(m) => match self.settled(&m.replacement_text) {
                std::borrow::Cow::Owned(text) => {
                    Meaning::Macro(Rc::new(crate::tex::MacroDef { replacement_text: text.into(), ..(*m).clone() }))
                }
                std::borrow::Cow::Borrowed(_) => Meaning::Macro(m),
            },
            meaning => meaning,
        };
        if self.read_noexpanded(token) && self.expandable_meaning(&meaning) {
            return Meaning::Primitive(Primitive::DontExpand);
        }
        meaning
    }

    fn meaning_of(&mut self, token: Token) -> Meaning {
        if let [Token { tok: Tok::Chr(c, cat), .. }] = self.settled(&[token])[..] {
            return Meaning::Char(c, cat);
        }
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
    /// `\ifnum\ifcase1 \or1 \fi>0` reaches the `>` past the `\fi`. A missing
    /// one is `=`. Unfollowed text may be any of the three, leaving it undecided.
    fn scan_relation(&mut self) -> Option<char> {
        if let Some(relation) = self.scan_relation_x() {
            return Some(relation);
        }
        let unknown = self.peek().and_then(|t| t.cs()).is_some_and(|sym| self.is_unknown_marker(sym));
        (!unknown).then_some('=')
    }

    /// ⟨number⟩ (tex.web § 440); see [`crate::scan`].
    pub fn scan_number(&mut self) -> Option<i64> {
        self.scanning_value(Self::scan_int)
    }

    /// A box register's number (tex.web § 1079).
    pub(crate) fn scan_box_number(&mut self) -> Option<i64> {
        self.scratch.box_number = true;
        let n = self.scan_number();
        self.scratch.box_number = false;
        n
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
            self.abs_exact = None;
            None
        } else {
            value
        }
    }

    /// ⟨glue⟩ or ⟨muglue⟩ (tex.web § 461); see [`crate::scan`].
    pub fn scan_glue(&mut self, mu: bool) -> Option<Glue> {
        self.scan_glue_hull(mu).glue()
    }

    /// ⟨glue⟩ as one interval per component, any where not followed.
    pub fn scan_glue_hull(&mut self, mu: bool) -> crate::value::GlueHull {
        self.scanning_value(|m| m.scan_glue_spec(mu)).unwrap_or_else(crate::value::GlueHull::full)
    }

    /// ⟨dimen⟩ in scaled points (tex.web § 448); see [`crate::scan`].
    pub fn scan_dimen(&mut self) -> Option<i64> {
        self.scanning_value(Self::scan_dimension)
    }

    /// tex.web § 1243 (`alter_aux`): `\spacefactor` and `\prevdepth` may be
    /// set only where their list is.  In another mode the engine reports an
    /// illegal case and scans nothing, so the `=⟨value⟩` is left in the
    /// input and becomes material.  `false` says the assignment does not
    /// happen and its operand is not read.
    fn owner_is_here(&mut self, named: Sym, span: Span) -> bool {
        let Some(Owned::List { modes, .. }) = crate::builtins::owned(self.name(named)) else {
            return true;
        };
        match self.mode.test(modes) {
            Some(true) => true,
            Some(false) => {
                let name = self.name(named).to_string();
                let modes = modes.names().join(" or ");
                self.diagnose(
                    Severity::Warning,
                    "wrong-mode",
                    span,
                    format!("\\{name} can only be set in {modes} mode; this assignment does nothing"),
                );
                false
            }
            // Unknown mode: the engine may have taken the value or left it to
            // start a paragraph. Read the operand to stay in step and widen the
            // mode. Reads go through [`Machine::owned_value`], unknown meanwhile.
            None => {
                self.widen_mode();
                true
            }
        }
    }

    /// What a quantity the list or page builder owns reads as, which is not
    /// what was assigned. `None` leaves it unknown.
    fn owned_value(&mut self, owned: Owned, named: Sym) -> Option<Value> {
        use crate::mode::Modes;
        match owned {
            Owned::List { modes, .. } => match self.mode.test(modes) {
                // tex.web § 418: "Improper \spacefactor" and "Improper
                // \prevdepth" read as zero, and as an integer under `\the`.
                Some(false) => Some(Value::Int(0)),
                // The horizontal list carries its own space factor as it
                // sets characters.
                Some(true) if modes == Modes::ANY_HORIZONTAL => {
                    self.natural.and_then(|n| n.space_factor).map(Value::Int)
                }
                // `\prevdepth` is held under the parameter's own name, which
                // `ignore_depth` makes fresh for each vertical list.
                Some(true) => Some(self.env.value(named)),
                None => None,
            },
            // tex.web § 986: while the page is empty the page builder has
            // not frozen its dimensions, so the read is the empty page's
            // whatever was assigned.
            Owned::PageSoFar { empty } if self.page_empty() => Some(Value::Dimen(empty)),
            // § 987: then the goal is `\vsize`, less what inserts took
            // (§ 1009), and the total no more than what reached the list
            // since the last page.
            Owned::PageSoFar { .. } => {
                use crate::mode::Dim;
                // §§ 421, 1245: an assignment reads back until the page
                // builder moves material (§ 1004).
                if let Some(v) = self.page_assigned(named).filter(|v| !matches!(v, Value::Int(_))) {
                    return (v != Value::Unknown).then_some(v);
                }
                let v = self.dimen_parameter("vsize").known()?;
                let built = self.engine_slot_int("page", "page.built") == Some(1);
                let inserts = self.engine_slot_int("page", "page.inserts") != Some(0);
                match self.name(named) {
                    // An unknown flag reads as not built and as inserted:
                    // each only widens the goal toward what it may be.
                    "pagegoal" => Some(
                        Dim::range(
                            if inserts { -crate::value::MAX_DIMEN } else { v },
                            if built { v } else { crate::value::MAX_DIMEN },
                        )
                        .value(),
                    ),
                    // At most the length and the `\topskip` glue (§ 1001):
                    // it leaves out the last depth, the glue the page
                    // discarded and what waits to be added.
                    "pagetotal" => {
                        let l = self.page_length();
                        let top = self.glue_parameter("topskip")?.width.max(0);
                        (!l.is_any()).then(|| Dim::range(0, l.hi.max(0).saturating_add(top)).value())
                    }
                    _ => None,
                }
            }
        }
    }

    /// A read of an abstract number leaves its interval, and, while a join
    /// left it one of a few exact values, those (the set lift), to the test
    /// that reads it, as a register does.
    fn read_abstract(&mut self, value: Value) -> Option<Value> {
        if !matches!(value, Value::Num { .. } | Value::OneOf(_)) {
            return Some(value);
        }
        self.abs = value.num().map(|(_, n)| n);
        self.abs_exact = value.exact_ints();
        self.abs_parts = value.member_nums().filter(|_| self.abs_exact.is_none());
        self.abs_from = None;
        Some(Value::Unknown)
    }

    pub(crate) fn internal_quantity(&mut self, sym: Sym, span: Span) -> Option<Value> {
        if let Some(p) = self.env.meaning(sym).prim()
            && Machine::extra_level(p).is_some()
        {
            // A quantity known by its bound leaves the bound to the scan,
            // as a register in an interval does.
            return match self.extra_quantity(p, sym) {
                Some(value @ Value::Num { .. }) => {
                    self.abs = value.num().map(|(_, n)| n);
                    self.abs_exact = None;
                    self.abs_from = None;
                    Some(Value::Unknown)
                }
                // An operand's bound is not the quantity's.
                value => {
                    (self.abs, self.abs_exact, self.abs_from) = (None, None, None);
                    value
                }
            };
        }
        // tex.web §§ 418, 986: a quantity the current list or the page
        // builder owns does not read back what was assigned to it.
        if matches!(self.env.meaning(sym).prim(), Some(Primitive::IntegerParameter | Primitive::DimenParameter)) {
            let named = self.env.identity(sym);
            if let Some(owned) = crate::builtins::owned(self.name(named)) {
                return self.owned_value(owned, named).and_then(|v| self.read_abstract(v));
            }
            if matches!(self.name(named), "year" | "month" | "day" | "time") {
                self.clock_read(span);
            }
        }
        match self.env.meaning(sym).prim() {
            // An expression of unknown value leaves its interval, and, while
            // a join left it one of a few exact values, those (the set
            // lift), to the test that reads it, as a register does.
            Some(Primitive::Expr(kind)) => {
                let value = self.scan_expr(kind);
                self.read_abstract(value)
            }
            // `\the\catcode⟨number⟩` is the category code in force (The TeXbook,
            // ch. 24). expl3 saves and restores its catcodes with it.
            Some(Primitive::CatcodeAssign) => {
                let code = self.scan_number()?;
                let character = u32::try_from(code).ok().and_then(char::from_u32)?;
                let cats = self.catcodes.cats(character);
                let sets = self.env.sets;
                let value = (0..16).filter(|k| cats & 1 << k != 0).map(Value::Int).reduce(|a, b| a.join(&b, sets));
                self.read_abstract(value.unwrap_or(Value::Unknown))
            }
            Some(Primitive::CharCode(table)) => {
                let code = self.scan_number()?;
                let character = u32::try_from(code).ok().and_then(char::from_u32)?;
                let engine = self.out.plugins.engine;
                let value = self.code_value(table, character);
                let Some(value) = value.as_int() else {
                    // Read as stored where every member is.
                    let stored = value.num().is_some_and(|(_, n)| match table {
                        CodeTable::Math => !engine.has_unicode_math() && n.hi <= MATH_CODE_MAX,
                        CodeTable::Delimiter => n.hi < 1 << 30,
                        _ => true,
                    });
                    return self.read_abstract(if stored { value } else { Value::Unknown });
                };
                // XeTeX and LuaTeX keep one packed math code per character.
                if table == CodeTable::Math && engine.has_unicode_math() {
                    return Some(Value::Int(crate::builtins::read_math_code(value, engine)));
                }
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
                self.abs = None;
                self.abs_exact = None;
                self.abs_from = None;
                // An unpinned number names any register. It reads whatever any may hold.
                let Some(sym) = index.and_then(|n| self.one_register(kind, n)) else {
                    return Some(self.any_register_value(kind));
                };
                self.reference(sym, span);
                // tex.web § 232: every register starts at zero, so one that
                // was never assigned reads as zero and not as an unknown.
                match self.env.get(sym) {
                    Some(_) => Some(self.register_value(sym, kind)),
                    None => Some(self.unseen_register(kind, sym)),
                }
            }
            // A parameter reads as its value, as `\ifnum\pdfoutput>0` asks.
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
                        None => Some(self.unseen_register(kind, storage)),
                    }
                }
                _ => None,
            },
        }
    }

    /// The largest register number: 32767 (etex.ch `max_reg_num`), 65535
    /// under LuaTeX (LuaTeX manual § 2.4, "the register ranges").
    fn max_register(&self) -> i64 {
        if self.out.plugins.engine.has_luatex() { 65535 } else { MAX_REGISTER }
    }

    /// tex.web § 433 (`scan_register_num`): an out-of-range number is "Bad
    /// register code" and names register zero.
    fn register_indices(&self, index: i64) -> Vec<i64> {
        if (0..=self.max_register()).contains(&index) { vec![index] } else { vec![0] }
    }

    /// The one register a scanned number names, when it names only one.
    fn one_register(&mut self, kind: RegKind, index: i64) -> Option<Sym> {
        (0..=self.max_register()).contains(&index).then(|| self.register_sym(kind, index))
    }

    /// The name for every register of `kind` satex has not seen written.
    /// `i64::MIN` is past what `scan_int` gives (tex.web § 445 stops at 2^31-1).
    fn any_register_sym(&mut self, kind: RegKind) -> Sym {
        self.register_sym(kind, i64::MIN)
    }

    /// The number of register slot `sym` of `kind`, `None` for a name of its own.
    pub(crate) fn register_index(&mut self, kind: RegKind, sym: Sym) -> Option<i64> {
        let zero = self.register_sym(kind, 0);
        let prefix = self.name(zero).strip_suffix('0').map(str::to_string)?;
        self.name(sym).strip_prefix(prefix.as_str())?.parse().ok()
    }

    /// Whether an unpinned write may have reached register `index` of
    /// `kind` (`None`: some register).
    pub(crate) fn any_register_written(&mut self, kind: RegKind, index: Option<i64>) -> bool {
        let sym = self.any_register_sym(kind);
        self.family_reaches(sym, index)
    }

    /// An unpinned write reached the family `any` stands for: its slot holds
    /// the hull of the indices it may have named, unknown for any index.
    /// Joins and loop widening act on that hull like on any number.
    pub(crate) fn reach_family(&mut self, any: Sym, index: Option<&Num>, value: &Value, global: bool) {
        let written = self.written_sym(any);
        let joined = match self.env.slot(written) {
            Some(b) => b.value.clone().join(value, self.env.sets),
            None => value.clone(),
        };
        self.env.set_value(written, joined, global);
        // Bound once on every path to the empty hull, so a path that never
        // reached the family joins as reaching nothing.
        if self.env.slot(any).is_none() {
            let empty = Value::Num { quantity: Quantity::Int, num: Num { lo: i64::MAX, hi: i64::MIN } };
            self.env.constant(
                any,
                crate::env::Binding { value: empty, ..crate::env::Binding::builtin(Meaning::Undefined) },
            );
        }
        let hull = match (self.env.get(any).map(|b| b.value.num()), index) {
            (None, Some(n)) => Some(*n),
            (Some(Some((_, num))), Some(n)) => Some(num.join(n)),
            _ => None,
        };
        let value = hull.map_or(Value::Unknown, |num| Value::Num { quantity: Quantity::Int, num });
        self.env.set_value(any, value, global);
    }

    /// The slot the values written through family `any` are joined in.
    fn written_sym(&mut self, any: Sym) -> Sym {
        let name = format!("{}=", self.name(any));
        self.intern(&name)
    }

    /// What an entry of family `any` at `index` holds given its own value
    /// `own`: that, or anything an unpinned write that may have named it stored.
    pub(crate) fn family_value(&mut self, any: Sym, index: Option<i64>, own: Value) -> Value {
        if !self.family_reaches(any, index) {
            return own;
        }
        let written = self.written_sym(any);
        match self.env.slot(written) {
            Some(b) => own.join(&b.value.clone(), self.env.sets),
            None => Value::Unknown,
        }
    }

    /// Whether such a write may have named `index` (`None`: some index).
    pub(crate) fn family_reaches(&mut self, any: Sym, index: Option<i64>) -> bool {
        match (self.env.get(any).map(|b| b.value.num()), index) {
            (None, _) => false,
            (Some(Some((_, num))), Some(k)) => num.lo <= k && k <= num.hi,
            _ => true,
        }
    }

    /// Value of any register when the index is unpinned.
    fn any_register_value(&mut self, kind: RegKind) -> Value {
        self.abs = register_top(kind);
        self.abs_exact = None;
        // Nothing to narrow: the read names no one slot.
        self.abs_from = None;
        Value::Unknown
    }

    /// What an unassigned register holds: zero (tex.web § 232), or anything
    /// of its kind if an unpinned write may have reached it.
    fn unseen_register(&mut self, kind: RegKind, sym: Sym) -> Value {
        let index = self.register_index(kind, sym);
        let any = self.any_register_sym(kind);
        let value = self.family_value(any, index, zero_of(kind));
        if matches!(value, Value::Unknown) {
            return self.any_register_value(kind);
        }
        self.abs = value.num().map(|(_, n)| n);
        self.abs_exact = value.exact_ints();
        self.abs_from = None;
        value
    }

    /// The registers a scanned index may name, if its interval is narrow
    /// enough to list. `None` means every register of the kind.
    fn index_targets(&mut self, kind: RegKind, index: Option<&Num>) -> Option<Vec<Sym>> {
        let limit = self.env.sets.values.max(1) as i64;
        let num = index?;
        let width = num.hi.checked_sub(num.lo)?;
        if !(0..limit).contains(&width) {
            return None;
        }
        let mut out: Vec<Sym> = Vec::new();
        let indices: Vec<i64> = (num.lo..=num.hi).flat_map(|n| self.register_indices(n)).collect();
        for n in indices {
            let sym = self.register_sym(kind, n);
            if !out.contains(&sym) {
                out.push(sym);
            }
        }
        Some(out)
    }

    /// A write through an unpinned register number, which may lie in any of
    /// `index`'s intervals (any register at all for `None`): each is
    /// weakened in turn.
    pub(crate) fn weaken_registers(&mut self, kind: RegKind, index: Option<&[Num]>, value: Value, global: bool) {
        match index {
            Some(parts) => {
                parts.iter().for_each(|part| self.weaken_register_range(kind, Some(part), value.clone(), global))
            }
            None => self.weaken_register_range(kind, None, value, global),
        }
    }

    /// A write through a register number in `index`. Every register it may
    /// name is joined with the value, and unseen ones are no longer known to
    /// be zero (tex.web §§ 232, 1224).
    fn weaken_register_range(&mut self, kind: RegKind, index: Option<&Num>, value: Value, global: bool) {
        let members = self.env.sets;
        let weaken = |m: &mut Self, sym: Sym, base: Value| {
            let bound = m.env.slot(sym).is_some();
            let old = m.env.slot(sym).map_or(base, |b| b.value.clone());
            let joined = old.clone().join(&value, members);
            // A slot the join does not move is left alone, so a repeat sweep is free.
            let same = joined == old || matches!((&joined, &old), (Value::Unknown, Value::Unknown));
            if !same || !bound {
                m.env.set_value(sym, joined, global);
            }
        };
        if let Some(targets) = self.index_targets(kind, index) {
            for sym in targets {
                weaken(self, sym, zero_of(kind));
            }
            return;
        }
        let all = self.any_register_sym(kind);
        self.reach_family(all, index, &value, global);
        // Seen registers in reach keep their slots, so a later known-number
        // write is precise again.
        let prefix = {
            let zero = self.register_sym(kind, 0);
            let name = self.name(zero).to_string();
            name[..name.len() - 1].to_string()
        };
        for sym in self.env.bound() {
            let at = self.name(sym).strip_prefix(&prefix).and_then(|n| n.parse::<i64>().ok());
            if sym != all && at.is_some_and(|n| index.is_none_or(|r| r.lo <= n && n <= r.hi)) {
                weaken(self, sym, zero_of(kind));
            }
        }
    }

    /// A register's value. Sets [`Machine::abs`] for interval reads, and,
    /// while a join left it one of a few exact values, [`Machine::abs_exact`]
    /// for those (the set lift).
    fn register_value(&mut self, sym: Sym, kind: RegKind) -> Value {
        let value = self.env.value(sym);
        let raw = self.env.get(sym).map(|b| &b.value);
        (self.abs, self.abs_exact, self.abs_from) = match (&value, self.env.num(sym), kind) {
            (Value::Unknown, Some((quantity, num)), _) => {
                (Some(num), raw.and_then(Value::exact_ints), Some((sym, quantity)))
            }
            (Value::Unknown, None, RegKind::Count | RegKind::Dimen) => {
                let quantity = if kind == RegKind::Dimen { Quantity::Dimen } else { Quantity::Int };
                (register_top(kind), None, Some((sym, quantity)))
            }
            _ => (None, None, None),
        };
        value
    }

    /// What a count or dimension scan left when it read no known number: the
    /// interval of the register it read, or, while a join left it one of a
    /// few exact values, those (the set lift), or unknown.
    pub(crate) fn scanned_abs(&mut self, dimen: bool) -> Value {
        self.abs_from = None;
        let quantity = if dimen { Quantity::Dimen } else { Quantity::Int };
        let exact = self.abs_exact.take();
        match (self.abs.take(), exact) {
            (_, Some(members)) if !members.is_empty() => match members[..] {
                [n] => Value::from_num(quantity, Num::exact(n)),
                _ => Value::OneOf(members.into_iter().map(|n| Value::from_num(quantity, Num::exact(n))).collect()),
            },
            (Some(num), _) => Value::from_num(quantity, num),
            (None, _) => Value::Unknown,
        }
    }

    /// The index a number scan read without pinning it, as the intervals it
    /// may lie in: its exact members while a join left few, the parts an
    /// expression left ([`Machine::abs_parts`]), else its one interval.
    /// `None` when it may be any number. What the scan left is cleared.
    pub(crate) fn take_index(&mut self) -> Option<Vec<Num>> {
        let (abs, exact, parts) = (self.abs.take(), self.abs_exact.take(), self.abs_parts.take());
        self.abs_from = None;
        let abs = abs?;
        Some(match (exact, parts) {
            (Some(xs), _) if !xs.is_empty() => xs.into_iter().map(Num::exact).collect(),
            (_, Some(parts)) if hull(&parts) == Some(abs) => parts,
            _ => vec![abs],
        })
    }

    fn allocate(&mut self, by: Sym, span: Span, kind: RegKind) {
        let Some(name) = self.read_cs() else { return };
        // A package cache replays an allocation by allocating again. The
        // counters it advances are its effect, not something the package read.
        let tracking = self.env.track_reads(false);
        let index = self.allocation_number(kind);
        self.env.track_reads(tracking);
        self.define(name, Meaning::Register(kind, index), by, DefMode::New, span);
        let storage = self.storage(name);
        let tracking = self.env.track_reads(false);
        self.env.set_value(storage, zero_of(kind), true);
        self.env.track_reads(tracking);
        // `\newbox`, `\newread` and `\newwrite` are `\chardef`s of the number
        // they allocate (The TeXbook, appendix B).
        if matches!(kind, RegKind::Box | RegKind::Read | RegKind::Write) && index != crate::tex::UNNUMBERED {
            self.env.set_value(name, Value::Int(i64::from(index)), true);
        }
    }

    /// The number `\newcount` and relatives hand out: one past the last, kept
    /// per kind in `\count10` to `\count17` (The TeXbook, appendix B,
    /// `\alloc@`). An unknown counter leaves the register unnumbered.
    pub(crate) fn allocation_number(&mut self, kind: RegKind) -> u16 {
        /// plain.tex: `\count10=22`, and 9 for other kinds, so the scratch
        /// registers below are never handed out.
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
            RegKind::Read | RegKind::Write | RegKind::Char | RegKind::MathChar | RegKind::UMathChar => {
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

    /// The number a `\countdef` and relatives accept. tex.web § 433
    /// `scan_eight_bit_int`, § 434 `scan_char_num` and § 1224's
    /// `scan_fifteen_bit_int` report a bad number and go on with zero.
    /// A unicode engine takes a character code past 255.
    fn register_def_number(&mut self, kind: RegKind, index: i64, span: Span) -> i64 {
        let unicode = self.out.plugins.engine.has_unicode_math() || self.out.plugins.engine.has_luatex();
        let (most, what) = match kind {
            // xetex.web, luatex: `biggest_usv` = 0x10FFFF.
            RegKind::Char if unicode => (0x10FFFF, "character code"),
            RegKind::Char => (255, "character code"),
            RegKind::MathChar => (0x7fff, "mathchar"),
            RegKind::UMathChar => (crate::value::WORD_MAX, "Umathchar"),
            _ => (self.max_register(), "register code"),
        };
        if (0..=most).contains(&index) {
            return index;
        }
        self.diagnose(
            Severity::Warning,
            "bad-register-code",
            span,
            format!("bad {what} ({index}), which must be between 0 and {most}; 0 is used"),
        );
        0
    }

    fn register_def(&mut self, by: Sym, span: Span, kind: RegKind) {
        // `\newcount\x` reaches `\countdef` inside the kernel's `\alloc@`. The
        // definition is placed where the file wrote the name.
        let span = self.definition_site(span);
        let Some(name) = self.read_r_token() else { return };
        // tex.web § 1224: the name means \relax while the number is scanned.
        // Only a macro or an undefined name reads differently; an unknown
        // meaning already covers \relax.
        self.redefines_hint = Some((name, self.redefines(name)));
        if matches!(self.env.meaning(name), Meaning::Macro(_) | Meaning::Undefined) {
            let relax = crate::env::Binding::builtin(Meaning::Primitive(Primitive::Relax));
            self.env.set(name, relax, self.prefixes.global);
        }
        self.read_equals();
        // An unknown number names a register all the same: one satex
        // cannot reach by number, with its own storage, as an allocation
        // whose counter is unknown hands out ([`Machine::allocation_number`]).
        // A `\chardef` of an unknown number is a name of unknown meaning.
        let Some(index) = self.scan_number() else {
            let meaning = match kind {
                RegKind::Char | RegKind::MathChar | RegKind::UMathChar => Meaning::Unknown,
                kind => Meaning::Register(kind, crate::tex::UNNUMBERED),
            };
            self.define(name, meaning, by, DefMode::Declare, span);
            return;
        };
        let index = self.register_def_number(kind, index, span);
        self.define(name, Meaning::Register(kind, index as u16), by, DefMode::Declare, span);
        if matches!(kind, RegKind::Char | RegKind::MathChar | RegKind::UMathChar) {
            // The binding `define` saved already, so a group takes it back whole.
            self.env.set_value_in_place(name, Value::Int(index));
        }
    }

    /// `\m@ne=-1`: a register control sequence followed by `=` is an
    /// assignment. Anything else uses its value.
    pub fn assign_register(&mut self, name: Sym, kind: RegKind, span: Span) {
        // tex.web § 1224: a register in the stomach is always assigned. The
        // `=` is optional (§ 405).
        crate::observe::arg_type(self, span, crate::roles::Ty::Reg(kind));
        self.read_equals();
        let value = self.scan_register_value(kind);
        let global = self.take_global();
        let target = self.storage(name);
        self.assign_through(name, target, value, global, span);
    }

    /// The value a register of `kind` is set to, read after its optional `=`.
    fn scan_register_value(&mut self, kind: RegKind) -> Value {
        let value = match kind {
            RegKind::Dimen => self.scan_dimen().map_or_else(|| self.scanned_abs(true), Value::Dimen),
            RegKind::Skip | RegKind::MuSkip => {
                self.scan_glue_hull(kind == RegKind::MuSkip).value(kind == RegKind::MuSkip)
            }
            RegKind::Toks => Value::Toks(Rc::from(self.read_general_text())),
            _ => self.scan_number().map_or_else(|| self.scanned_abs(false), Value::Int),
        };
        value::trap_zero_glue(value)
    }

    /// tex.web § 477: a ⟨general text⟩ is a braced list reached through filler.
    /// tex.web § 403 `scan_left_brace`: the next non-blank non-`\relax` token
    /// expansion leaves must be a `{`, which is consumed.
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
        let id = self.scan_begin();
        let mut text = self.read_general_text_absorbing();
        if self.scan_end(id) {
            text.push(self.unknown_text(Unk::Any, self.position_span()));
        }
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
                    // Token list overflow ends run (tex.web § 120).
                    if body.len() >= self.cfg.limits.held_tokens {
                        self.halt_capacity("main memory");
                    }
                    return body;
                }
                Tok::Cs(sym) => match self.env.meaning(sym) {
                    Meaning::Primitive(Primitive::Relax) => continue,
                    // `\toks1=\toks0` copies the other register, by number or `\toksdef` name.
                    Meaning::Primitive(Primitive::Register(RegKind::Toks)) => {
                        // An unpinned number names any token register. It holds unknown, not
                        // nothing.
                        let Some(index) = self.scan_number() else {
                            return vec![self.unknown_text(Unk::Any, token.span)];
                        };
                        let source = self.register_sym(RegKind::Toks, index);
                        return self.toks_of(source, token.span);
                    }
                    Meaning::Register(RegKind::Toks, _) => {
                        let storage = self.storage(sym);
                        return self.toks_of(storage, token.span);
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

    /// A token register a join left one of a few lists: a path per list,
    /// on which it holds that one, as a test refines a count. Unknown text
    /// otherwise.
    fn toks_member(&mut self, sym: Sym, span: Span) -> Vec<Token> {
        let storage = self.storage(sym);
        let lists: Option<Vec<Rc<[Token]>>> = match self.env.slot(storage).map(|b| &b.value) {
            Some(Value::OneOf(set)) => {
                set.iter().map(|v| if let Value::Toks(t) = v { Some(t.clone()) } else { None }).collect()
            }
            _ => None,
        };
        let mut chosen = None;
        if let Some(lists) = lists
            && self.split(storage, span, lists.len(), |m, arm| {
                m.refine_value(storage, Value::Toks(lists[arm].clone()));
                chosen = Some(lists[arm].to_vec());
                true
            })
            && let Some(list) = chosen
        {
            return list;
        }
        vec![self.unknown_text(Unk::Any, span)]
    }

    /// The tokens a token register holds. One never assigned holds none
    /// (tex.web § 232), as `\the` reads it.
    fn toks_of(&mut self, sym: Sym, span: Span) -> Vec<Token> {
        let value = match self.env.get(sym) {
            Some(_) => self.env.value(sym),
            None => self.unseen_register(RegKind::Toks, sym),
        };
        match value {
            Value::Toks(toks) => toks.to_vec(),
            _ => self.toks_member(sym, span),
        }
    }

    /// `\count0=1`, `\toks@{…}`: a register named by number (tex.web § 1224).
    fn assign_numbered_register(&mut self, kind: RegKind, span: Span) {
        let index = self.scan_number();
        // An unpinned number names any register. The value is still read before
        // it is spread over them.
        let spread = index.is_none().then(|| self.take_index());
        self.abs = None;
        self.abs_exact = None;
        self.abs_from = None;
        let target = match index {
            Some(n) if self.one_register(kind, n).is_none() => {
                // tex.web § 433: "Bad register code", register zero is used.
                let most = self.max_register();
                self.diagnose(
                    Severity::Warning,
                    "bad-register-code",
                    span,
                    format!("bad register code ({n}), which must be between 0 and {most}; 0 is used"),
                );
                Some(self.register_sym(kind, 0))
            }
            Some(n) => self.one_register(kind, n),
            None => None,
        };
        let spread = spread.flatten();
        self.read_equals();
        let value = self.scan_register_value(kind);
        let global = self.take_global();
        match target {
            Some(target) => self.assign(target, value, global, span),
            None => self.weaken_registers(kind, spread.as_deref(), value, global),
        }
    }

    fn arithmetic(&mut self, op: Arith, span: Span) {
        let Some((via, target, kind, spread)) = self.read_register() else { return };
        // `\advance\count\x` with unknown `\x` may reach any register it names.
        // None keeps a value satex can name.
        let Some((via, target)) = via.zip(target) else {
            self.scan_keyword("by");
            match kind {
                RegKind::Toks => {}
                RegKind::Skip | RegKind::MuSkip if matches!(op, Arith::Advance) => {
                    self.scan_glue(kind == RegKind::MuSkip);
                }
                RegKind::Dimen if matches!(op, Arith::Advance) => {
                    self.scan_dimen();
                }
                _ => {
                    self.scan_number();
                }
            }
            let global = self.take_global();
            return self.weaken_registers(kind, spread.as_deref(), Value::Unknown, global);
        };
        // tex.web § 1236: the `by` is an optional keyword.
        self.scan_keyword("by");
        // Unassigned register holds zero (tex.web § 222).
        let current = match self.env.get(target) {
            Some(_) => self.env.value(target),
            None => self.unseen_register(kind, target),
        };
        let value = match kind {
            // tex.web § 1240: `\advance` on glue takes a ⟨glue⟩. `\multiply` and
            // `\divide` take an integer and scale all three components.
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
                let (cur, cur_x) = self.current_abs(target, &current);
                let n = match op {
                    Arith::Advance => self.scan_dimen(),
                    _ => self.scan_number(),
                };
                let (operand, operand_x) = self.scanned_abs_num(n);
                self.abs_arith(Quantity::Dimen, cur, cur_x, operand, operand_x, op, span)
            }
            _ => {
                let (cur, cur_x) = self.current_abs(target, &current);
                let n = self.scan_number();
                let (operand, operand_x) = self.scanned_abs_num(n);
                self.abs_arith(Quantity::Int, cur, cur_x, operand, operand_x, op, span)
            }
        };
        let value = value::trap_zero_glue(value);
        let global = self.take_global();
        self.assign_from_old(via, target, value, global, span);
    }

    /// The abstract number a register named `target` holds before
    /// `\advance`/`\multiply`/`\divide` overwrites it: the interval, and,
    /// while a join left it few exact values, those (the set lift, for
    /// [`Machine::abs_arith`] to apply the operation member by member).
    /// Never assigned holds zero; unknown is any number of its kind.
    fn current_abs(&mut self, target: Sym, current: &Value) -> (Option<Num>, Option<Vec<i64>>) {
        let (num, exact) = match self.env.get(target) {
            Some(_) => (self.env.num(target).map(|(_, n)| n), self.env.get(target).and_then(|b| b.value.exact_ints())),
            None => (current.num().map(|(_, n)| n), current.exact_ints()),
        };
        let num =
            num.or_else(|| matches!(current, Value::Unknown).then(|| Num::range(value::WORD_MIN, value::WORD_MAX)));
        (num, exact)
    }

    /// A scanned `\advance`/`\multiply`/`\divide` operand: a literal number
    /// exactly, or the interval (and, while it is a register a join left
    /// few exact values, those) [`Machine::scanned_abs`] read.
    fn scanned_abs_num(&mut self, n: Option<i64>) -> (Option<Num>, Option<Vec<i64>>) {
        match n {
            Some(n) => (Some(Num::exact(n)), Some(vec![n])),
            None => (self.abs.take(), self.abs_exact.take()),
        }
    }

    /// The register an arithmetic command names: the source's name, the
    /// storage it stands for, and its kind.
    #[allow(clippy::type_complexity)]
    fn read_register(&mut self) -> Option<(Option<Sym>, Option<Sym>, RegKind, Option<Vec<Num>>)> {
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
            && let Some(Primitive::Register(kind)) = self.env.meaning(sym).prim()
        {
            self.next_token();
            let index = self.scan_number();
            let spread = index.is_none().then(|| self.take_index()).flatten();
            self.abs = None;
            self.abs_exact = None;
            self.abs_from = None;
            let target = index.and_then(|n| self.one_register(kind, n));
            let spread = match index {
                Some(n) if target.is_none() => Some(vec![Num::range(n.min(0), n.max(0))]),
                _ => spread,
            };
            return Some((target, target, kind, spread));
        }
        let sym = self.read_cs()?;
        // `\advance\tex_escapechar:D` changes `\escapechar` itself.
        let sym = self.env.identity(sym);
        let kind = match self.env.meaning(sym) {
            Meaning::Register(kind, _) => return Some((Some(sym), Some(self.storage(sym)), kind, None)),
            _ => match self.env.value(sym) {
                Value::Dimen(_) => RegKind::Dimen,
                Value::Glue(_) => RegKind::Skip,
                Value::MuGlue(_) => RegKind::MuSkip,
                _ => RegKind::Count,
            },
        };
        Some((Some(sym), Some(sym), kind, None))
    }

    /// A parameter or code the engine range-checks (tex.web §§ 1232, 1244;
    /// LuaTeX manual, "Math"): what to store, or `None` when refused and the old
    /// value stays. `named` is the source's primitive, `target` its storage.
    pub(crate) fn limited(
        &mut self,
        named: Sym,
        target: Sym,
        limit: Limit,
        value: Option<i64>,
        span: Span,
    ) -> Option<Value> {
        let Some(value) = value else {
            // A read-only parameter takes no value, so even an unknown one changes
            // nothing.
            if matches!(limit, Limit::Fixed(_)) {
                return None;
            }
            // Either the old value stayed or the engine took one: both.
            let old = match self.env.get(target) {
                Some(_) => {
                    self.env.num(target).map(|(_, n)| n).unwrap_or_else(|| Num::range(value::WORD_MIN, value::WORD_MAX))
                }
                None => Num::exact(limit.start().unwrap_or(0)),
            };
            let (lo, hi) = limit.reach();
            let num = old.join(&Num::range(lo, hi));
            return Some(if num.lo == num.hi {
                Value::Int(num.lo)
            } else {
                Value::Num { quantity: Quantity::Int, num }
            });
        };
        let taken = limit.take(value);
        match taken {
            Taken::Yes => return Some(Value::Int(value)),
            // Taken, so no error, but not stored as written.
            Taken::Recomposed(stored) => return Some(Value::Int(stored)),
            _ => {}
        }
        let name = self.name(named).to_string();
        let (severity, code) = match limit {
            Limit::Fixed(_) => (Severity::Info, "read-only-parameter"),
            _ => (Severity::Warning, "invalid-code"),
        };
        self.diagnose(severity, code, span, limit.refusal(&name, value));
        match taken {
            Taken::Instead(stored) => Some(Value::Int(stored)),
            _ => None,
        }
    }

    /// The codes tex.web § 1232 takes, and whether a negative is one. Anything
    /// else is "Invalid code" and stores 0. Only `\delcode` takes `-1`. A
    /// Unicode engine takes a case code up to 0x10FFFF (XeTeX reference
    /// guide, "Character codes"; LuaTeX manual § 2.2, "Unicode text").
    fn code_limit(&self, table: CodeTable) -> (i64, bool) {
        let unicode = self.out.plugins.engine.has_unicode_math();
        match table {
            CodeTable::Lower | CodeTable::Upper if unicode => (0x10_ffff, false),
            CodeTable::Lower | CodeTable::Upper => (255, false),
            CodeTable::Space => (0x7fff, false),
            CodeTable::Math => (MATH_CODE_MAX, false),
            CodeTable::Delimiter => (0xff_ffff, true),
        }
    }

    fn assign_char_code(&mut self, table: CodeTable, span: Span) {
        self.abs = None;
        self.abs_exact = None;
        let code = self.scan_number();
        let parts = self.take_index();
        let abs = parts.as_deref().and_then(hull);
        let chars = self.scanned_chars(code, abs);
        self.read_equals();
        self.abs = None;
        self.abs_exact = None;
        let value = self.scan_number().map_or_else(|| self.scanned_abs(false), Value::Int);
        // tex.web § 1232: an out-of-range code is "Invalid code" and the engine
        // goes on with 0.
        let (most, signed) = self.code_limit(table);
        let value = match value {
            // A set or interval of codes stays one while every member is valid
            // and stored as read.
            Value::OneOf(_) | Value::Num { .. }
                if table == CodeTable::Math
                    || value.num().is_none_or(|(_, n)| n.hi > most || (n.lo < 0 && !signed)) =>
            {
                Value::Unknown
            }
            Value::Int(n) if n > most || (n < 0 && !signed) => {
                self.diagnose(
                    Severity::Warning,
                    "invalid-code",
                    span,
                    format!("invalid code ({n}) for \\{}, which takes at most {most}; 0 is used", table.as_str()),
                );
                Value::Int(0)
            }
            value => value,
        };
        // Where the engine packs math codes, a TeX82 code is packed on the way in.
        let value = match (table, &value) {
            (CodeTable::Math, Value::Int(n)) => {
                Value::Int(crate::builtins::pack_math_code(*n, self.out.plugins.engine))
            }
            _ => value,
        };
        self.abs = None;
        self.abs_exact = None;
        let global = self.take_global();
        let Some(chars) = chars else {
            // Any character in the intervals may be the one assigned, each
            // interval apart. With no interval, any at all, and no entry is
            // known.
            let (mut widened, mut reach) = (Vec::new(), Vec::new());
            for part in parts.iter().flatten() {
                match Self::interval_chars(Some(*part)) {
                    Some(chars) => widened.extend(chars),
                    None => reach.push(*part),
                }
            }
            for character in widened {
                self.join_code(table, character, &value, global, span);
            }
            match parts {
                Some(_) => reach.iter().for_each(|part| self.weaken_code_table(table, Some(part), &value, global)),
                None => self.weaken_code_table(table, None, &value, global),
            }
            let what = match parts.is_none() || !reach.is_empty() {
                true => "each entry it may name holds its old code or the new",
                false => "every code it may name is unknown",
            };
            return self.diagnose(
                Severity::Imprecision,
                "unknown-char-code",
                span,
                format!("\\{} of a character satex could not pin down; {what}", table.as_str()),
            );
        };
        // Any may be the one assigned, so each holds its old code or the new.
        if let [character] = chars[..] {
            let target = self.char_code_sym(table, character);
            self.env.set_value(target, value, global);
            return self.record(Step::Assign, target, span);
        }
        for character in chars {
            self.join_code(table, character, &value, global, span);
        }
    }

    /// An assignment that may have named `c`'s entry: it holds its old code or `value`.
    fn join_code(&mut self, table: CodeTable, c: char, value: &Value, global: bool, span: Span) {
        let target = self.char_code_sym(table, c);
        let joined = self.code_value(table, c).join(value, self.env.sets);
        self.env.set_value(target, joined, global);
        self.record(Step::Assign, target, span);
    }

    /// Every character a scanned interval covers, to widen each. `None` when
    /// too wide to name them.
    fn interval_chars(abs: Option<crate::value::Num>) -> Option<Vec<char>> {
        /// Past this, widening per entry costs more than it keeps, and the whole
        /// table goes at once.
        const MOST: i64 = 4096;
        let num = abs?;
        let (lo, hi) = (num.lo.max(0), num.hi.min(i64::from(char::MAX as u32)));
        (hi - lo < MOST).then(|| (lo..=hi).filter_map(|n| u32::try_from(n).ok().and_then(char::from_u32)).collect())
    }

    /// The characters a scanned number may name: the one it is, or the few its
    /// interval lists. `None` when too many to widen each.
    fn scanned_chars(&self, scanned: Option<i64>, abs: Option<crate::value::Num>) -> Option<Vec<char>> {
        let limit = usize::from(self.limits.value_set).max(1) as i64;
        let members: Vec<i64> = match scanned {
            Some(n) => vec![n],
            None => {
                let num = abs?;
                let width = num.hi.checked_sub(num.lo)?;
                if width >= limit {
                    return None;
                }
                (num.lo..=num.hi).collect()
            }
        };
        (members.len() as i64 <= limit)
            .then(|| members.iter().filter_map(|n| u32::try_from(*n).ok().and_then(char::from_u32)).collect())
    }

    pub(crate) fn char_code_sym(&mut self, table: CodeTable, c: char) -> Sym {
        self.intern(&format!("{CODE}{}{c}", table.as_str()))
    }

    /// The slot for every character of a table. Real entries are one
    /// character long, so `{CODE}any` is none of them.
    fn any_code_sym(&mut self, table: CodeTable) -> Sym {
        self.intern(&format!("{CODE}{}{CODE}any", table.as_str()))
    }

    /// A write through an unpinned character number may reach any entry
    /// `index` spans (all for `None`): each holds its old code or `value`.
    pub(crate) fn weaken_code_table(&mut self, table: CodeTable, index: Option<&Num>, value: &Value, global: bool) {
        let any = self.any_code_sym(table);
        self.reach_family(any, index, value, global);
    }

    /// A character's initial code in the table's form. Math codes are packed
    /// where the engine packs them.
    fn code_default(&self, table: CodeTable, c: char) -> i64 {
        let value = table.default_for(c);
        match table {
            CodeTable::Math => crate::builtins::pack_math_code(value, self.out.plugins.engine),
            _ => value,
        }
    }

    /// What `c`'s entry holds: its code, one of a few, or unknown. Unpinned
    /// writes that may have named it add what they stored.
    fn code_value(&mut self, table: CodeTable, c: char) -> Value {
        let sym = self.char_code_sym(table, c);
        let own = match self.env.get(sym).map(|b| b.value.clone()) {
            Some(own) => {
                let _ = self.env.value(sym);
                own
            }
            None => Value::Int(self.code_default(table, c)),
        };
        let any = self.any_code_sym(table);
        self.family_value(any, Some(i64::from(c as u32)), own)
    }

    /// [`Machine::char_code`], `None` where it is not one known code.
    pub(crate) fn char_code_known(&mut self, table: CodeTable, c: char) -> Option<i64> {
        self.code_value(table, c).as_int()
    }

    /// tex.web § 1288 (`case_shift`): each character with a non-zero
    /// `\lccode`/`\uccode` becomes that character, keeping its category.
    /// Control sequences pass through. The result goes back into the input.
    fn case_shift(&mut self, table: CodeTable) {
        let body = self.read_general_text();
        let shift =
            |c: char, code: i64| u32::try_from(code).ok().and_then(char::from_u32).filter(|s| *s != '\0').unwrap_or(c);
        let mut shifted = Vec::with_capacity(body.len());
        for t in body.iter() {
            let Tok::Chr(c, cat) = t.tok else {
                shifted.push(*t);
                continue;
            };
            let value = self.code_value(table, c);
            let most = usize::from(self.limits.value_set);
            let codes = match value {
                Value::Int(code) => vec![code],
                Value::Num { num, .. } if num.hi.saturating_sub(num.lo) < most as i64 => (num.lo..=num.hi).collect(),
                _ => value.exact_ints().filter(|v| v.len() <= most).unwrap_or_default(),
            };
            let token = |code: i64| Token::new(Tok::Chr(shift(c, code), cat), t.span);
            // A code a join left one of a few: a path for each where the
            // category shapes what reads it, else one of the characters.
            let mut taken = None;
            shifted.push(match codes[..] {
                [code] => token(code),
                [] if matches!(cat, Catcode::Letter | Catcode::Other) => self.unknown_text(Unk::Any, t.span),
                [] => Token::new(Tok::Chr(crate::tex::UNKNOWN_ANY, cat), t.span),
                _ if !matches!(cat, Catcode::Letter | Catcode::Other)
                    && self.split(self.unknown, t.span, codes.len(), |_, arm| {
                        taken = Some(codes[arm]);
                        true
                    }) =>
                {
                    token(taken.unwrap_or(c as i64))
                }
                _ => {
                    let texts = codes.iter().map(|&code| Rc::from([token(code)])).collect();
                    self.one_of(texts, t.span)
                }
            });
        }
        self.push_tokens(Rc::from(shifted));
    }

    fn assign_catcode(&mut self, span: Span) {
        self.abs = None;
        self.abs_exact = None;
        let id = self.scan_begin();
        let code = self.scan_number();
        let parts = self.take_index();
        let abs = parts.as_deref().and_then(hull);
        let chars = self.scanned_chars(code, abs);
        self.read_equals();
        self.abs = None;
        self.abs_exact = None;
        let cat = self.scan_number();
        let cat_abs = self.abs.take();
        self.abs_exact = None;
        let catcode = cat.and_then(|n| Catcode::from_u8(n as u8));
        // A cut dropped the text the operands stood in: the assignment is the
        // skipped region's, whose category changes are assumed put back.
        if (self.scan_end(id) || self.cutting()) && (chars.is_none() || catcode.is_none()) {
            return self.diagnose(
                Severity::Assumption,
                "region-catcodes",
                span,
                "a \\catcode in a skipped expansion lost its operands to the cut; categories are assumed unchanged"
                    .into(),
            );
        }
        if chars.as_ref().is_some_and(Vec::is_empty) {
            return;
        }
        // The categories the value may be, one bit each.
        let cats: u16 = match (catcode, cat_abs) {
            (Some(c), _) => 1 << c as u8,
            (None, Some(n)) if n.lo <= 15 && n.hi >= 0 => (n.lo.max(0)..=n.hi.min(15)).fold(0, |m, k| m | 1 << k),
            _ => u16::MAX,
        };
        let (Some(ch), Some(catcode)) = (chars.as_ref().filter(|c| c.len() == 1).map(|c| c[0]), catcode) else {
            // Each character it may name keeps its category or takes one of
            // `cats`, each interval of them apart.
            let parts: Vec<Option<Num>> = match (&chars, parts) {
                (None, Some(parts)) => parts.into_iter().map(Some).collect(),
                _ => vec![abs],
            };
            let lone = parts.len() == 1;
            for part in parts {
                match chars.clone().or_else(|| Self::interval_chars(part)) {
                    // One character takes one of `cats`, each of several keeps
                    // its own or takes one.
                    Some(chars) => {
                        let one = lone && chars.len() == 1;
                        for ch in chars {
                            let old = if one { 0 } else { self.catcodes.cats(ch) };
                            self.set_catcode_may(ch, old | cats);
                        }
                    }
                    None => self.set_catcodes_reach(part, cats),
                }
            }
            return self.diagnose(
                Severity::Imprecision,
                "unknown-catcode",
                span,
                "\\catcode of a character or with a value satex could not pin down; each category it may name is its old one or the one assigned".into(),
            );
        };
        self.set_catcode(ch, catcode);
        // `\ExplSyntaxOn` changes catcodes from inside the kernel. The change
        // belongs to the file line that asked for it.
        let at = self.raised_at(span);
        self.occurrence(OccKind::Catcode, ch.to_string(), Some((catcode as u8).to_string()), at);
    }

    /// `\input⟨file name⟩` (tex.web § 537): the file is read next.
    fn load_command(&mut self, span: Span, kind: LoadKind) {
        let name = self.read_file_name();
        self.load(&name, kind, span, Vec::new(), None);
    }

    /// `\write⟨number⟩{…}` (tex.web § 1350): text is expanded when written, at
    /// once after `\immediate` (tex.web § 1375), else at shipout, which satex
    /// does not do. Stream 18 is the shell, run only with shell escape.
    fn write_stream(&mut self, span: Span) {
        let immediate = std::mem::take(&mut self.prefixes.immediate);
        let number = self.scan_number();
        let text = self.read_general_text();
        // A deferred one is expanded at shipout (tex.web § 1370): here, with
        // the page number not known, which is all LaTeX's `\protected@write`
        // leaves to then.
        let text = match immediate {
            true => self.expand_tokens(Rc::from(text)),
            false => crate::observe::shipped(self, &text),
        };
        crate::observe::write(self, &text, number.unwrap_or(-1));
        // tex.web § 1370: printed to the file, broken at `\newlinechar` (§ 59).
        let newlinechar = self.intern("newlinechar");
        let newline = self.env.value(newlinechar).as_int().and_then(|c| u32::try_from(c).ok()).and_then(char::from_u32);
        let to = crate::tex::Selector::File { newline };
        let escape = self.escape_char();
        // Unknown text goes to the file as one character that reads back
        // as unknown text of its class ([`crate::tex::Mouth::next_with`]).
        let syms = self.typed_syms();
        let typed = crate::env::typed!(self, syms);
        let file: Vec<Token> = text
            .iter()
            .map(|t| match t.cs().is_some_and(|s| self.name(s).starts_with(crate::tex::UNKNOWN_PREFIX)) {
                true => {
                    // One of several texts: the class they share.
                    let c = typed
                        .classify(t)
                        .or_else(|| typed.one_of_class(t))
                        .map_or(crate::tex::UNKNOWN_ANY, |(c, one)| crate::tex::written_unknown(c, one));
                    Token::new(Tok::Chr(c, Catcode::Other), t.span)
                }
                false => *t,
            })
            .collect();
        let line = crate::tex::detokenize_in(&file, &self.out.interner, escape, &self.catcodes, to);
        self.write_output(number, &line, !immediate);
        if number == Some(18) {
            self.run_shell(&line);
        }
        let text = self.text_of(&text);
        // An unpinned stream may be the shell or a file the run opened, so both
        // are recorded. The log case writes nothing read back.
        let Some(stream) = number else {
            self.diagnose(
                Severity::Imprecision,
                "unknown-write-stream",
                span,
                "\\write to a stream satex could not pin down; the shell and every open file may get it".into(),
            );
            let label = Some("an unknown stream".to_string());
            self.occurrence(OccKind::ShellEscape, text.clone(), label.clone(), span);
            self.occurrence(OccKind::Write, text, label, span);
            return;
        };
        // tex.web § 1370: a stream not open writes to terminal and log. That is a
        // message.
        let open = (0..16).contains(&stream) && self.write_open(stream) != Some(false);
        if immediate && stream != 18 && !open {
            let at = self.raised_at(span);
            crate::observe::warned(self, &text, at);
            if self.probe.is_none() {
                self.note_missing_file_error(&text, at);
            }
            self.occurrence(OccKind::Message, text, Some(format!("stream {stream}")), at);
            return;
        }
        let kind = if stream == 18 { OccKind::ShellEscape } else { OccKind::Write };
        self.occurrence(kind, text, Some(format!("stream {stream}")), span);
    }

    /// `\font\name=⟨file⟩ at ⟨size⟩` (tex.web § 1256): the name becomes a font
    /// identifier, which NFSS's `\ifx\font@name\relax` tests for.
    fn font_def(&mut self, by: Sym, span: Span) {
        self.load_font(by, span);
    }

    /// `\openin⟨number⟩=⟨file name⟩` (tex.web § 1275). The name resolves as the
    /// engine would, so `\ifeof` answers for real.
    fn open_in(&mut self, by: Sym, span: Span) {
        let stream = self.scan_stream(by, span);
        self.read_equals();
        let name = self.read_file_name();
        let base = self.base().to_path_buf();
        let path = self.resolver_mut().resolve(&name, LoadKind::Input, &base);
        if let Some(path) = &path {
            self.note_read_file(path);
            if let Some(stream) = stream {
                self.carried.stream_paths.insert(stream, path.display().to_string());
            }
        }
        // A file this run wrote with unknown content leaves the stream unknown.
        let Some(bytes) = self.input_bytes(&name) else { return self.unknown_stream(stream) };
        let lines = bytes.map(|b| crate::source::decode(&b).lines().map(str::to_string).collect::<Vec<_>>());
        self.set_stream(stream, lines);
    }

    /// `\read⟨number⟩to⟨cs⟩` (tex.web § 1225): one line becomes the replacement
    /// text.
    fn read_line_into(&mut self, by: Sym, span: Span) {
        self.env.effective += 1;
        let stream = self.scan_number();
        // tex.web § 1225: a missing `to` is inserted.
        self.scan_keyword("to");
        let Some(name) = self.read_r_token() else { return };
        let open = self.stream_open(stream);
        if open != Some(true) {
            // tex.web § 484: a stream not open reads the terminal, giving whatever
            // the user types. An unpinned number may name an open stream, so it
            // never ends the job.
            if open == Some(false) && self.terminal_read_is_fatal(span) {
                return;
            }
            // A probe asks what a command reads: what follows a prompt is
            // the user's answer, so the run ends at it (a missing file's
            // `\typein` would otherwise ask again until the budget is spent).
            if open == Some(false) && self.probe.is_some() {
                self.probe_waits();
                return;
            }
            let what = if open == Some(false) { "the terminal" } else { "a stream satex could not pin down" };
            self.diagnose(
                Severity::Imprecision,
                "terminal-read",
                span,
                format!("\\{} reads from {what}; what it gives is unknown", self.name(by)),
            );
            self.define(name, Meaning::Unknown, by, DefMode::Declare, span);
            return;
        }
        let stream = stream.expect("an open stream has a number");
        // tex.web § 483: each line gets `\endlinechar`, read until braces
        // balance. Past the file's end the line is empty, making `\par` (§ 486).
        let mut body = Vec::new();
        let mut depth = 0i64;
        loop {
            let line = self.take_stream_line(stream).unwrap_or_default();
            for t in self.tokenize_line_at(&line, span) {
                match t.tok {
                    Tok::Chr(_, Catcode::Begin) => depth += 1,
                    // tex.web § 483: an unmatched `}` aborts the line.
                    Tok::Chr(_, Catcode::End) if depth == 0 => break,
                    Tok::Chr(_, Catcode::End) => depth -= 1,
                    _ => {}
                }
                body.push(t);
            }
            if depth == 0 || self.stream_at_end(stream) != Some(false) {
                break;
            }
        }
        let meaning = self.make_macro(ParameterText::default(), body);
        self.define(name, meaning, by, DefMode::Declare, span);
    }

    /// tex.web § 484: in `\batchmode` and `\nonstopmode` a terminal read is
    /// fatal. With the mode unknown the read may happen and the caller goes on.
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

    /// File name and whether it has a `"` quoted part (for XeTeX `\font`).
    pub(crate) fn read_file_name_quoted(&mut self) -> (String, bool) {
        self.skip_spaces();
        match self.peek() {
            Some(t) if t.is_cat(Catcode::Begin) => (self.read_text(), false),
            _ => {
                // tex.web § 526: scanned with `get_x_token`, so `\@@input\jobname.aux`
                // names the job. What the gullet cannot remove ends the name and stays.
                // web2c's `more_name`: `"` opens or closes a quoted part and is not part
                // of the name. A space inside quotes is (`\@@input\@filef@und`).
                let mut name = String::new();
                let (mut quoted, mut any_quote) = (false, false);
                while let Some(t) = self.next_token() {
                    match t.tok {
                        Tok::Chr('"', Catcode::Letter | Catcode::Other) => {
                            quoted = !quoted;
                            any_quote = true;
                        }
                        Tok::Chr(c, Catcode::Space) if quoted => name.push(c),
                        // tex.web § 526: consume space that ends the name.
                        Tok::Chr(_, Catcode::Space) => break,
                        // A character of any category up to 12 is part of the
                        // name (`cur_cmd <= other_char`): `_`, `^`, `&`, `$`
                        // and braces too (`pgfplotsoldpgfsupp_loader.code.tex`).
                        Tok::Chr(
                            c,
                            Catcode::Letter
                            | Catcode::Other
                            | Catcode::Begin
                            | Catcode::End
                            | Catcode::Math
                            | Catcode::Tab
                            | Catcode::Param
                            | Catcode::Sup
                            | Catcode::Sub,
                        ) => name.push(c),
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

    /// `\newif`: `\if⟨name⟩` with `\⟨name⟩true` defined (latex.ltx).
    /// Primitive copies like expl3's `\if_false:` are not switches.
    pub(crate) fn is_switch(&self, sym: Sym) -> bool {
        if !matches!(self.env.meaning(sym).prim(), Some(Primitive::If(Cond::True | Cond::False))) {
            return false;
        }
        let Some(rest) = self.name(sym).strip_prefix("if").filter(|r| !r.is_empty()) else { return false };
        self.out.interner.lookup(&format!("{rest}true")).is_some_and(|t| self.env.is_defined(t))
    }

    /// `\message` and `\errmessage` (tex.web § 1279): expand text like `\edef`.
    fn message(&mut self, by: Sym, span: Span) {
        let name = self.name(by).to_string();
        let raw = self.read_undelimited();
        let expanded = self.expand_tokens(Rc::from(raw.clone()));
        let text = crate::tex::text_with_groups(&expanded, &self.out.interner);
        // The message belongs to the command the file called, wherever the
        // raising code was defined.
        let at = self.raised_at(span);
        crate::observe::warned(self, &text, at);
        if let (Some(watch), Some(Primitive::Message { error: true })) = (&mut self.probe, self.env.meaning(by).prim())
        {
            watch.error.get_or_insert_with(|| text.clone());
            watch.errors += 1;
        }
        if self.probe.is_none() && matches!(self.env.meaning(by).prim(), Some(Primitive::Message { error: true })) {
            self.note_raised(&text, at);
            self.certain_error(&text, false, span);
            // tex.web § 1283: `\errmessage` is one of the engine's errors.
            self.count_error(at);
        }
        if self.probe.is_none() {
            self.note_missing_file_error(&text, at);
        }
        if let Some(node) = self.occurrence(OccKind::Message, text, Some(name), at) {
            self.reads_value(node, &raw);
        }
    }

    /// Attribute `text` to every name whose doing it still is: the macro call
    /// stack, plus the command the file called (`self.file_call`). `\item` has
    /// left the stack when `\makelabel` raises "Lonely \item", yet owns it.
    pub(crate) fn note_raised(&mut self, text: &str, at: Span) {
        let context: Rc<[Sym]> = Rc::from(self.out.env_stack.clone());
        let certain = self.exact();
        let mut names: crate::fixed::HashSet<Sym> = self.call_stack().collect();
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

/// tex.web § 298 (`print_cmd_chr`): a character meaning is named by one fixed
/// phrase per command code, never by the category's number.
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

/// tex.web §§ 1238-1240: `\advance` is 32-bit addition, which wraps.
/// `\multiply` uses `mult_integers` (count) or `nx_plus_y` (dimension, up to
/// `\maxdimen`), `\divide` uses `x_over_n`. `None` is "Arithmetic overflow".
fn int_arith(a: i64, b: i64, op: Arith, dimen: bool) -> Option<i64> {
    match op {
        Arith::Advance => Some(value::wrap(a + b)),
        Arith::Multiply => value::mult_and_add(a, b, 0, if dimen { value::MAX_DIMEN } else { value::MAX_INT }),
        Arith::Divide => value::x_over_n(a, b),
    }
}

/// [`int_arith`] on unbounded integers, for interval corners ([`Num::lift`]).
/// `None` only for a zero divisor.
fn ideal_arith(a: i64, b: i64, op: Arith) -> Option<i64> {
    match op {
        Arith::Advance => Some(a + b),
        Arith::Multiply => Some(a * b),
        Arith::Divide => (b != 0).then(|| a / b),
    }
}

/// tex.web § 1239: glues add componentwise. A higher order of infinity
/// replaces a lower one instead of adding.
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

/// tex.web § 1240: scale glue width and components, keep infinity orders.
fn scale_glue(g: Glue, n: i64, op: Arith) -> Option<Glue> {
    let scale = |x: i64| -> Option<i64> { int_arith(x, n, op, true).filter(|_| op != Arith::Advance) };
    Some(Glue {
        width: scale(g.width)?,
        stretch: value::Stretch { amount: scale(g.stretch.amount)?, order: g.stretch.order },
        shrink: value::Stretch { amount: scale(g.shrink.amount)?, order: g.shrink.order },
    })
}

/// All a count or dimension register may hold. A count holds any 32-bit
/// word, since `\advance` wraps (tex.web § 1238). An unfollowed dimension is
/// within `\maxdimen`: assignments pass `attach_sign` (tex.web § 460), and
/// `\advance` past it leaves its own interval.
/// The interval spanning every part.
fn hull(parts: &[Num]) -> Option<Num> {
    parts.iter().copied().reduce(|a, b| a.join(&b))
}

fn register_top(kind: RegKind) -> Option<Num> {
    match kind {
        RegKind::Count => Some(Num::range(value::WORD_MIN, value::WORD_MAX)),
        RegKind::Dimen => Some(Num::full(true)),
        _ => None,
    }
}

/// tex.web § 433 (`scan_four_bit_int`): stream numbers are 0..15. "Bad
/// number" names stream 0 (probed: `\closein99` closes stream 0).
impl Machine<'_> {
    /// A stream's ⟨4-bit number⟩ (tex.web § 436: outside 0..=15 it is "Bad
    /// number" and 0). One read in an interval parts the path per stream it
    /// may name, the register it came from narrowed to match, so a later
    /// read of that register names the same stream. `None`: any stream.
    fn scan_stream(&mut self, by: Sym, span: Span) -> Option<i64> {
        self.abs_from = None;
        if let Some(n) = self.scan_number() {
            return Some(four_bit(n));
        }
        let from = self.abs_from.take();
        let mut arms: Vec<(i64, Num)> = Vec::new();
        for p in self.take_index()? {
            arms.extend((p.lo.max(0)..=p.hi.min(15)).map(|s| (s, Num::exact(s))));
            if p.lo < 0 {
                arms.push((0, Num::range(p.lo, p.hi.min(-1))));
            }
            if p.hi > 15 {
                arms.push((0, Num::range(p.lo.max(16), p.hi)));
            }
        }
        if let [(s, _), rest @ ..] = &arms[..]
            && rest.iter().all(|(t, _)| t == s)
        {
            return Some(*s);
        }
        let mut stream = None;
        self.split(by, span, arms.len(), |m, arm| {
            let (s, part) = arms[arm];
            if let Some((sym, quantity)) = from {
                m.refine_value(sym, Value::from_num(quantity, part));
            }
            stream = Some(s);
            true
        });
        stream
    }
}

fn four_bit(n: i64) -> i64 {
    if (0..16).contains(&n) { n } else { 0 }
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
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
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
    /// `\advance`, `\multiply`, `\divide` (tex.web §§ 1238–1240). `\multiply`
    /// and `\divide` keep the register on error. `\advance` wraps, so a sum
    /// leaving 32 bits may be anything. An unknown operand is any value.
    /// While a join left the register and the operand few enough exact
    /// values, the set lift applies the operation member by member
    /// (`current_x`, `operand_x`); otherwise the interval version below.
    #[allow(clippy::too_many_arguments)]
    fn abs_arith(
        &mut self,
        quantity: Quantity,
        current: Option<Num>,
        current_x: Option<Vec<i64>>,
        operand: Option<Num>,
        operand_x: Option<Vec<i64>>,
        op: Arith,
        span: Span,
    ) -> Value {
        self.abs_from = None;
        let dimen = quantity == Quantity::Dimen;
        let Some(current) = current else { return Value::Unknown };
        if let (Some(xs), Some(ys)) = (&current_x, &operand_x)
            && xs.len() * ys.len() <= 64
        {
            // Kept on error, the way `\multiply`/`\divide` keep the register
            // (`\advance`'s `int_arith` always wraps, never errs): every
            // member that erred for some operand joins in the whole of
            // `current` (its abstract value before the operation), the way
            // the interval fallback below joins in the whole interval.
            let mut out: Vec<i64> = Vec::new();
            let mut any_err = false;
            for &x in xs {
                for &y in ys {
                    match int_arith(x, y, op, dimen) {
                        Some(v) if !out.contains(&v) => out.push(v),
                        Some(_) => {}
                        None => any_err = true,
                    }
                }
            }
            if any_err {
                for &x in xs {
                    if !out.contains(&x) {
                        out.push(x);
                    }
                }
            }
            if out.len() <= self.env.sets.values {
                out.sort_unstable();
                return match out[..] {
                    [n] => Value::from_num(quantity, Num::exact(n)),
                    _ => Value::OneOf(out.into_iter().map(|n| Value::from_num(quantity, Num::exact(n))).collect()),
                };
            }
        }
        let advance = matches!(op, Arith::Advance);
        let operand = operand.unwrap_or_else(|| {
            if dimen && advance { Num::full(true) } else { Num::range(value::WORD_MIN, value::WORD_MAX) }
        });
        // `\multiply` errs past its `max_answer`. `\advance` and `\divide` wrap
        // (x_over_n of -2^31 by -1).
        let range = match op {
            Arith::Multiply if dimen => (-value::MAX_DIMEN, value::MAX_DIMEN),
            Arith::Multiply => (-value::MAX_INT, value::MAX_INT),
            _ => (value::WORD_MIN, value::WORD_MAX),
        };
        let (num, err) = Num::lift(
            &[&current, &operand],
            range,
            op != Arith::Multiply,
            |v| int_arith(v[0], v[1], op, dimen),
            |v| ideal_arith(v[0], v[1], op),
        );
        // `mult_and_add` and `x_over_n` of -2^31 on 32 bits (`negate` wraps) may
        // give -2^31 … 0 where unbounded integers do not.
        let exact = current.lo == current.hi && operand.lo == operand.hi;
        let num = if op != Arith::Advance && !exact && (current.lo == value::WORD_MIN || operand.lo == value::WORD_MIN)
        {
            num.map(|n| n.join(&Num::range(value::WORD_MIN, 0)))
        } else {
            num
        };
        let num = match num {
            Some(num) if err => num.join(&current),
            Some(num) => num,
            None => {
                self.diagnose(
                    Severity::Warning,
                    "arithmetic-overflow",
                    span,
                    "arithmetic overflow; the register keeps its value".into(),
                );
                current
            }
        };
        Value::from_num(quantity, num)
    }

    /// The members a register's set holds as intervals: its exact values,
    /// or the intervals a join left ([`Value::member_nums`]).
    /// Only when their hull is `whole`, the operand the test read.
    fn members_of(&self, sym: Sym, exact: Option<&[i64]>, whole: Num) -> Option<Vec<Num>> {
        let members = match exact {
            Some(exact) => exact.iter().map(|&n| Num::exact(n)).collect(),
            None => self.env.get(sym).and_then(|b| b.value.member_nums())?,
        };
        (members.iter().copied().reduce(|a, b| a.join(&b)) == Some(whole)).then_some(members)
    }

    /// `\ifnum`/`\ifdim` on abstract numbers, decided when the intervals decide.
    /// An undecided test of a register against a known value is remembered so
    /// each arm learns its implication ([`Machine::narrowing`]).
    #[allow(clippy::type_complexity)]
    #[allow(clippy::too_many_arguments)]
    fn decide_numbers(
        &mut self,
        xs: Option<Num>,
        xs_exact: Option<Vec<i64>>,
        relation: Option<char>,
        ys: Option<Num>,
        ys_exact: Option<Vec<i64>>,
        (a, b): (Option<i64>, Option<i64>),
        (from_a, from_b): (Option<(Sym, crate::value::Quantity)>, Option<(Sym, crate::value::Quantity)>),
    ) -> Option<bool> {
        self.narrowing = None;
        let relation = relation?;
        // A register a join left a set of values or intervals is decided
        // member by member: every pair of members gives the same answer.
        let members = |exact: &Option<Vec<i64>>, from: Option<(Sym, crate::value::Quantity)>, whole: Option<Num>| {
            whole.and_then(|w| from.and_then(|(sym, _)| self.members_of(sym, exact.as_deref(), w)).or(Some(vec![w])))
        };
        if let (Some(xm), Some(ym)) = (members(&xs_exact, from_a, xs), members(&ys_exact, from_b, ys))
            && let Some(Some(answer)) =
                alike_option(xm.iter().flat_map(|x| ym.iter().map(move |y| x.compare(relation, y))))
        {
            return Some(answer);
        }
        let decided = match (&xs, &ys) {
            (Some(x), Some(y)) => x.compare(relation, y),
            _ => None,
        };
        if decided.is_some() {
            return decided;
        }
        // A register on either side against a known number. On the right, the
        // relation reads the other way round.
        let flipped = match relation {
            '<' => '>',
            '>' => '<',
            r => r,
        };
        // Against an interval, each arm takes the end that keeps every
        // member: `x<y` gives `x<y.hi`, its failure `x>=y.lo`.
        let bounds = |relation: char, exact: Option<i64>, other: Option<Num>| match (exact, other, relation) {
            (Some(n), ..) => Some((n, n)),
            (None, Some(o), '<') => Some((o.hi, o.lo)),
            (None, Some(o), '>') => Some((o.lo, o.hi)),
            _ => None,
        };
        self.narrowing = match (from_a, xs, bounds(relation, b, ys), from_b, ys, bounds(flipped, a, xs)) {
            (Some((sym, dimen)), Some(x), Some(bound), ..) => Some((sym, dimen, x, xs_exact, relation, bound)),
            (_, _, _, Some((sym, dimen)), Some(y), Some(bound)) => Some((sym, dimen, y, ys_exact, flipped, bound)),
            _ => None,
        };
        None
    }
}

/// An operand of a comparison: its interval, exact members, value and register.
type Operand = (Option<Num>, Option<Vec<i64>>, Option<i64>, Option<(Sym, crate::value::Quantity)>);

/// pdfTeX's `\ifpdfabsnum`/`\ifpdfabsdim`: both operands as absolute
/// values; the register is dropped, so no arm narrows it.
fn absolute(abs: bool, [x, y]: [Operand; 2]) -> (Operand, Operand) {
    if !abs {
        return (x, y);
    }
    let f = |(n, e, v, _): Operand| -> Operand {
        (n.map(|n| n.abs()), e.map(|e| e.into_iter().map(i64::abs).collect()), v.map(i64::abs), None)
    };
    (f(x), f(y))
}

/// The common answer when every member gives one and all agree.
fn alike_option(answers: impl Iterator<Item = Option<bool>>) -> Option<Option<bool>> {
    let answers: Option<Vec<bool>> = answers.collect();
    answers.map(|a| alike(a.into_iter()))
}

/// Common answer if all members agree, else `None`.
fn alike(mut answers: impl Iterator<Item = bool>) -> Option<bool> {
    let first = answers.next()?;
    answers.all(|x| x == first).then_some(first)
}

/// An engine's startup primitives: the one name of each, `None` when several
/// share the modeled meaning, and the primitive each name means.
#[derive(Default)]
struct Primitives {
    names: crate::fixed::HashMap<Primitive, Option<String>>,
    by_name: crate::fixed::HashMap<String, Primitive>,
}

/// `f` of `engine`'s [`Primitives`], built once per thread and engine.
fn with_primitives<R>(engine: crate::config::Engine, f: impl FnOnce(&Primitives) -> R) -> R {
    thread_local! {
        static CACHE: std::cell::RefCell<Vec<(crate::config::Engine, Primitives)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        let at = cache.iter().position(|(e, _)| *e == engine).unwrap_or_else(|| {
            let mut interner = crate::tex::Interner::default();
            let mut all = Primitives::default();
            for (sym, meaning) in crate::builtins::initial_meanings(&mut interner, engine) {
                let Meaning::Primitive(q) = meaning else { continue };
                let name = interner.name(sym).to_string();
                all.by_name.insert(name.clone(), q);
                // tex.web § 226: primitives are control words of letters and a few
                // control symbols. Other entries model macros, such as expl3's `\use:e`.
                if name.chars().all(|c| c.is_ascii_alphabetic()) || name.chars().count() == 1 {
                    all.names
                        .entry(q)
                        .and_modify(|seen| {
                            if seen.as_deref() != Some(name.as_str()) {
                                *seen = None;
                            }
                        })
                        .or_insert(Some(name));
                }
            }
            cache.push((engine, all));
            cache.len() - 1
        });
        f(&cache[at].1)
    })
}

/// The engine's one name for a primitive, or `None` when several share the
/// modeled meaning.
fn primitive_name(engine: crate::config::Engine, p: Primitive) -> Option<String> {
    with_primitives(engine, |all| all.names.get(&p).cloned().flatten())
}

/// Whether `name` is an engine primitive meaning `p` at startup.
/// `\aftergroup` is. The `\relax` `\csname` gives an unknown name is not.
fn is_primitive_named(engine: crate::config::Engine, name: &str, p: Primitive) -> bool {
    with_primitives(engine, |all| all.by_name.get(name) == Some(&p))
}
