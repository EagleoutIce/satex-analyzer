//! Facts the interpreter notes as a construct runs, rather than by
//! modeling the command that caused it: `\write` lines and their keys,
//! what a `\csname` was formed for, payloads handed to the PDF writer.

use crate::builtins::{Cond, OccKind, PdfOp, Primitive};
use crate::facts::CsnameRole;
use crate::machine::Machine;
use crate::tex::{Catcode, Span, Tok, Token};

/// A `\csname…\endcsname` just formed `name`.
pub fn csname(m: &mut Machine, name: &str, span: Span) {
    let role = role(m, span);
    m.note_csname(span, role);
    // `\let\x=\csname…\endcsname` reads the name like a use: cleveref's
    // `\cref@getref` takes `\r@⟨label⟩@cref` this way.
    if matches!(role, CsnameRole::Use | CsnameRole::Test | CsnameRole::LetTo) {
        read_name(m, name);
    }
    // A document color's control sequence carries an extra literal
    // backslash before `color@` (`\@backslashchar`), unlike xcolor's own
    // dispatch macros like "color@rgb" — COLOR_NAME_PREFIX tells them apart.
    if let Some(color_name) = name.strip_prefix(COLOR_NAME_PREFIX)
        && !color_name.is_empty()
        && matches!(role, CsnameRole::Use | CsnameRole::Test | CsnameRole::LetTo)
    {
        color_use(m, name, color_name, role, span);
    }
}

/// A color control sequence's name prefix: `\@backslashchar` then `color@`.
const COLOR_NAME_PREFIX: &str = "\\color@";

/// A use, test, or `\colorlet` source of `\color@⟨name⟩`. `full` is the
/// control sequence's own name text, for looking its meaning up.
fn color_use(m: &mut Machine, full: &str, color_name: &str, role: CsnameRole, span: Span) {
    // A package's own use is recorded too: the cache carries the fact and
    // a replay pushes it again, so cache hit and miss report the same.
    if m.out.facts.color_uses.len() >= m.cfg.limits.facts {
        return;
    }
    let defined =
        m.out.interner.lookup(full).is_some_and(|sym| !matches!(m.env.meaning(sym), crate::tex::Meaning::Undefined));
    m.out.facts.color_uses.push(crate::facts::ColorUse {
        name: color_name.to_string(),
        role,
        defined,
        span,
        via: m.file_call.map(|(sym, _)| sym),
        package: m.package(),
        certain: m.exact(),
    });
}

/// `\color@⟨name⟩` was just (re)defined or `\let`-aliased: xcolor stores
/// every color as `\xcolor@{cls}{drv}{model}{spec}` in one of its core
/// models (xcolor § 2.5, `\XC@definec@lor`). Fired from
/// [`crate::machine::Machine::define`], so `\definecolor`, `\providecolor`,
/// `\colorlet` and `\definecolorset` all arrive here alike.
pub fn color_def(m: &mut Machine, name: &str, meaning: &crate::tex::Meaning, span: Span) {
    let Some(color_name) = name.strip_prefix(COLOR_NAME_PREFIX) else { return };
    // A package-defined color is recorded too (see `color_use`).
    if color_name.is_empty() || m.out.facts.colors.len() >= m.cfg.limits.facts {
        return;
    }
    let value = meaning.as_macro().and_then(|mac| parse_stored_color(m, &mac.replacement_text));
    let via = m.file_call.map(|(sym, _)| sym);
    // A package's own call declares nothing the document wrote.
    let from_document = m.file_call.is_some_and(|(_, at)| m.project_file(at.file));
    let declared = from_document.then(|| declared_spec(m, color_name)).flatten();
    m.out.facts.colors.push(crate::facts::ColorDef {
        name: color_name.to_string(),
        value,
        declared: declared.as_ref().map(|(model, spec, _)| (*model, spec.clone())),
        spec_span: declared.as_ref().map(|(.., span)| *span),
        span,
        via,
        package: m.package(),
        certain: m.exact(),
    });
}

/// `\xcolor@{cls}{drv}{model}{spec}`, as `\XC@definec@lor` stores every
/// color (xcolor.sty): read the last two groups back as a [`Colors`].
fn parse_stored_color(m: &Machine, body: &[Token]) -> Option<crate::value::Colors> {
    let xcolor_at = m.out.interner.lookup("xcolor@")?;
    let start = body.iter().position(|t| matches!(t.tok, Tok::Cs(sym) if sym == xcolor_at))?;
    let groups = brace_groups(&body[start + 1..], 4)?;
    let model = crate::tex::text_of(groups[2], &m.out.interner);
    let spec = crate::tex::text_of(groups[3], &m.out.interner);
    let model = crate::value::Model::parse(model.trim())?;
    let color = crate::value::parse_color(model, &spec)?;
    Some(crate::value::Colors::exact(color))
}

/// The model and spec text `\XC@definec@lor` read for `color_name`, from the
/// kept argument capture. `None` when arguments aren't kept, so `declared`
/// stays unclaimed rather than guessed.
fn declared_spec(m: &Machine, color_name: &str) -> Option<(crate::value::Model, String, Span)> {
    let target = m.out.interner.lookup("XC@definec@lor")?;
    m.out.facts.expansions.iter().rev().find_map(|e| {
        if e.name != target || e.arguments.len() < 5 {
            return None;
        }
        let read_name = crate::tex::text_of(&e.arguments[1], &m.out.interner);
        if read_name != color_name {
            return None;
        }
        let model_text = crate::tex::text_of(&e.arguments[3], &m.out.interner);
        let spec = crate::tex::text_of(&e.arguments[4], &m.out.interner);
        let model = crate::value::Model::parse(model_text.trim())?;
        let span = e.arguments[4].first().map(|t| t.span).unwrap_or(span_of(e));
        Some((model, spec, span))
    })
}

fn span_of(e: &crate::facts::Expansion) -> Span {
    e.span
}

/// Split `toks` into `n` balanced brace groups written back to back
/// (`{a}{b}{c}{d}`), as xcolor always writes them.
fn brace_groups(toks: &[Token], n: usize) -> Option<Vec<&[Token]>> {
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    for _ in 0..n {
        if !matches!(toks.get(i)?.tok, Tok::Chr(_, Catcode::Begin)) {
            return None;
        }
        let start = i + 1;
        let mut depth = 1;
        i = start;
        while depth > 0 {
            match toks.get(i)?.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        out.push(&toks[start..i - 1]);
    }
    Some(out)
}

/// A `\pdfcolorstack` the run executed (pdfTeX manual § 8.9.8): `stack` is
/// `None` when [`Machine::scan_int`] could not decide it.
pub fn color_stack(m: &mut Machine, stack: Option<i64>, op: crate::facts::StackOp, span: Span) {
    // A driver's own `\pdfcolorstack` (pdftex.def sets the page's starting
    // color this way) is recorded too, so an unbalanced push inside a
    // package is still reported.
    if m.out.facts.color_stack.len() >= m.cfg.limits.facts {
        return;
    }
    m.out.facts.color_stack.push(crate::facts::ColorStack { stack, op, span, certain: m.exact() });
}

/// Where an occurrence recorded once the run has ended belongs.
#[derive(Clone)]
pub struct OccContext {
    pub package: Option<crate::tex::Sym>,
    pub expanded: Option<Span>,
    pub call: Option<(crate::tex::Sym, Span)>,
    pub within: Option<crate::env::NodeId>,
    pub cds: Vec<crate::graph::ControlDep>,
    pub section: Option<std::rc::Rc<str>>,
    pub certain: bool,
}

/// A line a `\write` handed a file: the command it starts with (empty for
/// text such as a `.bcf`'s XML), the text of its arguments, and the names
/// running it where the file is read back defined.
pub struct WrittenLine {
    head: String,
    args: Vec<String>,
    /// The arguments that are text the document gave the writing call,
    /// each where its own text stands.
    keys: Vec<(String, Span)>,
    /// The contents of the writing call's own braced arguments.
    parts: std::rc::Rc<[String]>,
    defined: Vec<String>,
    span: Span,
    context: OccContext,
}

impl WrittenLine {
    /// The key texts alone, for matching against the names the line defined.
    fn keys(&self) -> Vec<String> {
        self.keys.iter().map(|(key, _)| key.clone()).collect()
    }

    /// Where an occurrence of `key` belongs: at the key's own text, with the
    /// writing call kept as what expanded it. amsmath defers a display's
    /// `\label` to the `\end`, so the write is not where the key stands.
    fn at(&self, key: &str) -> (Span, OccContext) {
        let Some(span) = self.keys.iter().find(|(k, _)| k == key).map(|(_, s)| *s) else {
            return (self.span, self.context.clone());
        };
        let mut context = self.context.clone();
        context.expanded = Some(self.span);
        (span, context)
    }
}

/// A name a call in the document body read.
pub struct NameRead {
    name: crate::tex::Sym,
    /// The contents of the call's braced arguments.
    parts: std::rc::Rc<[String]>,
    call: crate::tex::Sym,
    span: Span,
    context: OccContext,
}

/// A definition made while a written line runs in the sandbox.
pub fn defined_name(m: &mut Machine, sym: crate::tex::Sym) {
    if let Some(defined) = &mut m.line_defined {
        defined.push(sym);
        return;
    }
    // Otherwise a name a document call defines from its own arguments.
    if m.package().is_some() || m.reading_format() {
        return;
    }
    let Some((call, span)) = m.file_call else { return };
    if !m.project_file(span.file) {
        return;
    }
    let parts = call_parts(m, span);
    let name = m.name(sym).to_string();
    let Some((key, _, _)) = within(&name, &parts).filter(|(_, p, s)| !p.is_empty() || !s.is_empty()) else { return };
    let context = m.occurrence_context();
    m.key_names.push((name, key, call, span, context));
}

/// What the call at `span` in a document file was given: the contents of
/// its braced arguments, so far as it has read them.
fn call_parts(m: &mut Machine, span: Span) -> std::rc::Rc<[String]> {
    if !m.project_file(span.file) {
        return std::rc::Rc::from([]);
    }
    let Some(mouth) = m.reading(span.file) else { return std::rc::Rc::from([]) };
    let here = mouth.here();
    if let Some((at, until, parts)) = &m.call_parts
        && (*at, *until) == (span, here)
    {
        return parts.clone();
    }
    let parts: std::rc::Rc<[String]> = argument_parts(&mouth.since(span.line, span.col)).into();
    m.call_parts = Some((span, here, parts.clone()));
    parts
}

/// The contents of the braced arguments in `text`.
fn argument_parts(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (at, c) in text.char_indices() {
        match c {
            '{' => {
                depth += 1;
                if depth == 1 {
                    start = at + 1;
                }
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    parts.push(text[start..at].trim().to_string());
                }
            }
            _ => {}
        }
    }
    parts.retain(|p| !p.is_empty() && !p.contains(['{', '}', '\\']));
    parts.sort();
    parts.dedup();
    parts
}

/// The argument a name is made of, whole and bounded by what is not a
/// letter or digit, and the shape around it: the one that leaves least.
fn within(name: &str, args: &[String]) -> Option<(String, String, String)> {
    args.iter()
        .filter(|a| !a.is_empty() && a.len() < name.len())
        .filter_map(|a| {
            name.match_indices(a.as_str())
                .map(|(at, _)| at)
                .find(|&at| {
                    let (before, after) = (&name[..at], &name[at + a.len()..]);
                    !before.ends_with(|c: char| c.is_alphanumeric())
                        && !after.starts_with(|c: char| c.is_alphanumeric())
                })
                .map(|at| (a.clone(), name[..at].to_string(), name[at + a.len()..].to_string()))
        })
        .min_by_key(|(_, p, s)| p.len() + s.len())
}

/// A name a body call read, used, tested, or taken by `\let`: matched
/// against what written lines define once the run has ended.
pub fn read_name(m: &mut Machine, name: &str) {
    if m.line_defined.is_some() || m.package().is_some() || m.reading_format() {
        return;
    }
    let Some((call, span)) = m.file_call else { return };
    let name = m.intern(name);
    let recent = m.name_reads.iter().rev().take(8).any(|r| r.name == name && r.span == span);
    if !recent {
        let context = m.occurrence_context();
        let parts = call_parts(m, span);
        m.name_reads.push(NameRead { name, parts, call, span, context });
    }
}

/// The kernel's warning for a reference to a label no `.aux` line defined
/// (latex.ltx `\@setref`, and cleveref's own): `Reference `⟨key⟩' on page
/// ⟨n⟩ undefined`. On a first run it is the only trace a reference leaves.
pub fn warned(m: &mut Machine, text: &str, span: Span) {
    // latex.ltx's `\@latex@warning` and cleveref's `\PackageWarning`
    // prefix the text with who says it.
    let Some(at) = text.find("Reference `") else { return };
    let rest = &text[at + "Reference `".len()..];
    let Some((key, tail)) = rest.split_once('\'') else { return };
    if key.trim().is_empty() || !tail.contains("undefined") {
        return;
    }
    let at = m.file_call.map_or(span, |(_, at)| at);
    m.occurrence(OccKind::Ref, key.trim().to_string(), None, at);
}

/// The environment the kernel is in: `\begin{x}` sets `\@currenvir` to `x`
/// inside the group it opens (latex.ltx).
pub fn current_environment(m: &mut Machine) -> Option<String> {
    let sym = m.out.interner.lookup("@currenvir")?;
    let crate::tex::Meaning::Macro(mac) = m.env.meaning(sym) else { return None };
    Some(m.text_of(&mac.replacement_text))
}

/// `\begin{x}` sets `\@currenvir` to `x` in the group it opens (latex.ltx):
/// a new value there is an environment beginning. `\begin{document}`
/// starts the document.
pub fn environment_opened(m: &mut Machine, outer: Option<String>, span: Span) {
    let Some(name) = current_environment(m).filter(|n| !n.is_empty()) else { return };
    if outer.as_deref() == Some(name.as_str()) && name != "document" {
        return;
    }
    if m.reading_format() {
        return;
    }
    let at = m.file_call.map_or(span, |(_, at)| at);
    if name == "document" {
        if m.out.document_depth.is_some() {
            return;
        }
        // `\@execute@begin@hook` closes the group `\begin` opened, so the
        // body is read one level out.
        m.out.document_depth = Some(m.env.group_level().saturating_sub(1) as u16);
        m.out.preamble_cost = Some((m.out.files.len(), m.out.steps));
        m.out.preamble = Some(Box::new((m.env.snapshot(), m.catcodes.clone())));
    } else {
        // `document` itself is not on the stack: `document_depth` already
        // answers the preamble/document axis. This is only user
        // environments nested inside it.
        let sym = m.intern(&name);
        m.out.env_stack.push(sym);
    }
    if let Some(node) = m.occurrence(OccKind::BeginEnvironment, name.clone(), None, at) {
        let sym = m.intern(&name);
        m.reads_definition_of(node, sym);
    }
}

/// A group closed: when `\@currenvir` differs outside it, the group was an
/// environment's, and the environment ended here.
pub fn environment_closed(m: &mut Machine, inner: Option<String>, span: Span) {
    let Some(inner) = inner.filter(|name| !name.is_empty()) else { return };
    if current_environment(m).as_deref() == Some(inner.as_str()) {
        return;
    }
    if inner != "document" {
        let sym = m.intern(&inner);
        if let Some(pos) = m.out.env_stack.iter().rposition(|s| *s == sym) {
            m.out.env_stack.remove(pos);
        }
    }
    // amsmath reads a display's body as one argument (`\collect@@body#1\end#2`):
    // the `\end` was matched as a delimiter, so the call is still the `\begin`.
    let call = m.file_call.map(|(_, at)| at);
    let begun = m
        .out
        .facts
        .occurrences
        .iter()
        .rev()
        .find(|o| o.kind == OccKind::BeginEnvironment && o.key == inner)
        .map(|o| o.span);
    let at = match (call, m.carried.absorbed) {
        (Some(call), Some(end))
            if begun == Some(call) && end.file == call.file && (end.line, end.col) > (call.line, call.col) =>
        {
            end
        }
        _ => call.unwrap_or(span),
    };
    if let Some(node) = m.occurrence(OccKind::EndEnvironment, inner.clone(), None, at) {
        let end = m.intern(&format!("end{inner}"));
        m.reads_definition_of(node, end);
    }
}

/// What the command an `\expandafter` held back does with the name: only a
/// `\csname` that `\expandafter` expanded directly is its operand.
fn role(m: &mut Machine, span: Span) -> CsnameRole {
    let primitive = |m: &Machine, token: Token| token.cs().and_then(|sym| m.env.meaning(sym).prim());
    let mut frames = m.held.iter().rev().copied();
    let Some((held, target)) = frames.next() else { return CsnameRole::Use };
    if target.map(|t| t.span) != Some(span) {
        return CsnameRole::Use;
    }
    match primitive(m, held) {
        Some(Primitive::Def { global, expand }) => CsnameRole::Define { global, expand },
        Some(Primitive::Let { future: false, .. }) => CsnameRole::Let,
        // The operand the name is compared with follows `\endcsname`.
        Some(Primitive::If(Cond::IfX)) => {
            let next = m.peek();
            match next.and_then(|t| primitive(m, t)) {
                Some(Primitive::Relax) => CsnameRole::Test,
                _ => CsnameRole::Use,
            }
        }
        _ => match frames.next() {
            Some((outer, Some(chained)))
                if primitive(m, chained) == Some(Primitive::ExpandAfter)
                    && matches!(primitive(m, outer), Some(Primitive::Let { future: false, .. })) =>
            {
                CsnameRole::LetTo
            }
            _ => CsnameRole::Use,
        },
    }
}

/// A `\write` to a file (stream 18 is the shell, and a stream never opened
/// writes to the terminal, tex.web § 1370): `tokens` as expanded when
/// `immediate`, otherwise as shipout expands them.
pub fn write(m: &mut Machine, tokens: &[Token], stream: i64, immediate: bool) {
    if m.line_defined.is_some() || stream == 18 {
        return;
    }
    let Some((_, span)) = m.file_call else { return };
    // A stream never opened writes to the log (tex.web § 1370). A fragment
    // run without `\begin{document}` never opens `\@auxout`, so what it
    // writes there stands in for a line of it. A package writing to a
    // closed stream (`\typeout` to `\@unused`) only logs.
    match m.stream_files.get(&stream).cloned() {
        Some(file) => {
            m.out.facts.written_files.insert(file);
        }
        None if (0..16).contains(&stream) && m.out.document_depth.is_none() && m.package().is_none() => {}
        None => return,
    }
    let expanded = if immediate { tokens.to_vec() } else { shipped(m, tokens) };
    line(m, &expanded, span);
}

/// A deferred `\write` is expanded at shipout, where the output routine
/// has `\protect` act as `\noexpand` (latex.ltx, `\@outputpage`).
fn shipped(m: &mut Machine, tokens: &[Token]) -> Vec<Token> {
    let protect = m.out.interner.lookup("protect");
    let noexpand = m.out.interner.lookup("noexpand").and_then(|s| m.env.get(s).cloned());
    let (Some(protect), Some(noexpand)) = (protect, noexpand) else { return m.expand_tokens(tokens.into()) };
    m.env.push_group(crate::env::GroupKind::SemiSimple, Span::default());
    m.env.set(protect, noexpand, false);
    let out = m.expand_tokens(tokens.into());
    m.env.pop_group(&mut m.catcodes);
    out
}

/// A written line, run where its file is read back: the kernel's state at
/// `\begin{document}`, where the `.aux` file is read. A line an argument
/// carries (`\@writefile{toc}{\contentsline…}`) is written onward.
fn line(m: &mut Machine, tokens: &[Token], span: Span) {
    let context = m.occurrence_context();
    let Some((head, mut at)) = head(m, tokens) else {
        // Text, not a command: the element contents of an XML file.
        let text = m.text_of(tokens);
        let args = text.split(['<', '>']).skip(2).step_by(2).map(|t| t.trim().to_string()).collect();
        m.written_lines.push(WrittenLine {
            head: String::new(),
            args,
            keys: Vec::new(),
            parts: std::rc::Rc::from([]),
            defined: Vec::new(),
            span,
            context,
        });
        return;
    };
    let mut args = Vec::new();
    let mut keys: Vec<(String, Span)> = Vec::new();
    let extent = m.call_extent(span);
    let here = extent.as_ref().map(|(here, _)| *here);
    let own = extent.as_ref().map(|(_, text)| own_text(span, text));
    while let Some((group, end)) = group_span(tokens, at) {
        onward(m, &group, span);
        let text = characters(&group).trim().to_string();
        // A line written onward carries its own keys.
        if self::head(m, &group).is_none() {
            keys.extend(given(&group, span, here));
        }
        args.push(text);
        at = end;
    }
    // A key the writing call itself carries stands at that call: the call is
    // the command to point at, and the key text is only its argument.
    for (_, at) in &mut keys {
        let inside = |own: Span| {
            at.file == span.file
                && (at.line, at.col) >= (span.line, span.col)
                && (at.line, at.col) < (own.line, own.col)
        };
        if own.is_some_and(inside) {
            *at = span;
        }
    }
    let text = crate::tex::text_with_groups(tokens, &m.out.interner);
    let defined = sandbox_run(m, &text);
    if m.out.timings.is_enabled()
        && let Some(Some(sandbox)) = &m.line_sandbox
    {
        let steps = sandbox.out.steps;
        m.out.timings.written_line((span.file, span.line), steps, &text);
    }
    let parts = call_parts(m, span);
    m.written_lines.push(WrittenLine { head, args, keys, parts, defined, span, context });
}

/// Where the writing call's own text ends: at the next control sequence
/// after its name, which is a command of its own and reads what follows.
fn own_text(call: Span, text: &str) -> Span {
    let (mut line, mut col) = (call.line, call.col);
    for (i, c) in text.char_indices() {
        if c == '\\' && i > 0 {
            break;
        }
        match c {
            '\n' => (line, col) = (line + 1, 1),
            _ => col += 1,
        }
    }
    Span::new(call.file, line, col)
}

/// The keys the document gave a written argument: what the write leaves
/// after full expansion, split at commas (`\citation{a,b}`, cite.sty). A
/// run with a character from the calling file, no later than it has been
/// read, is kept whole, so a counter or macro lands in the key it names
/// (`\label{sec:\thechapter}`). A run only a package wrote is no key.
fn given(group: &[Token], call: Span, here: Option<Span>) -> Vec<(String, Span)> {
    let Some(here) = here else { return Vec::new() };
    let mut keys = Vec::new();
    let mut run: Vec<(char, Span)> = Vec::new();
    let mut ours = false;
    let mut flush = |run: &mut Vec<(char, Span)>, ours: &mut bool| {
        for part in run.split(|(c, _)| *c == ',').filter(|_| *ours) {
            let key = part.iter().map(|(c, _)| *c).collect::<String>().trim().to_string();
            let at = part.iter().find(|(c, _)| !c.is_whitespace()).map(|(_, s)| *s);
            match at {
                Some(at) if !key.is_empty() => keys.push((key, at)),
                _ => {}
            }
        }
        run.clear();
        *ours = false;
    };
    for token in group {
        match token.tok {
            Tok::Chr(c, cat) if !matches!(cat, Catcode::Begin | Catcode::End) => {
                run.push((c, token.span));
                ours |= token.span.file == call.file && (token.span.line, token.span.col) <= (here.line, here.col);
            }
            _ => flush(&mut run, &mut ours),
        }
    }
    flush(&mut run, &mut ours);
    keys
}

/// A line written onward to a list file, `\contentsline{⟨level⟩}{⟨title⟩}…`
/// (ltsect, `\addcontentsline`): a level with a running head of its own
/// (`\sectionmark`) is a sectioning unit.
fn onward(m: &mut Machine, tokens: &[Token], span: Span) {
    let Some((_, at)) = head(m, tokens) else { return };
    let Some((level, end)) = group_span(tokens, at) else { return };
    let Some((title, _)) = group_span(tokens, end) else { return };
    let level = characters(&level).trim().to_string();
    let heads = m.out.interner.lookup(&format!("{level}mark")).is_some_and(|s| m.env.is_defined(s));
    if !heads {
        return;
    }
    // The title follows the number, which a command sets as its argument
    // (`\numberline{1}`). `\protect` is gone once shipped.
    let mut start = 0;
    while title.get(start).is_some_and(|t| t.cs().is_some()) {
        let numbered = group_span(&title, start + 1)
            .filter(|(g, _)| characters(g).chars().all(|c| c.is_ascii_digit() || c == '.'));
        start = numbered.map_or(start + 1, |(_, end)| end);
    }
    let key = characters(&title[start.min(title.len())..]).trim().to_string();
    if !key.is_empty() {
        m.occurrence(OccKind::Section, key, Some(level), span);
    }
}

/// The command a line starts with, as a token or as the characters
/// `\string` made of it, and where its arguments start.
fn head(m: &Machine, tokens: &[Token]) -> Option<(String, usize)> {
    let at = tokens.iter().position(|t| !t.is_cat(Catcode::Space))?;
    match tokens[at].cs() {
        Some(sym) => Some((m.name(sym).to_string(), at + 1)),
        None => stringified(tokens, at),
    }
}

/// The names running `text` defines, in a copy of the kernel's state at
/// `\begin{document}` kept for the whole run.
fn sandbox_run(m: &mut Machine, text: &str) -> Vec<String> {
    if m.line_sandbox.is_none() {
        // A fragment has no `\begin{document}`: its state as it stands.
        let fragment = m.out.preamble.is_none();
        if fragment {
            m.out.preamble = Some(Box::new((m.env.snapshot(), m.catcodes.clone())));
        }
        let sandbox = Machine::sandbox(m.cfg, &m.out, crate::probe::State::Preamble).map(Box::new);
        if fragment {
            m.out.preamble = None;
        }
        m.line_sandbox = Some(sandbox);
    }
    match &mut m.line_sandbox {
        Some(Some(sandbox)) => sandbox.run_line(text),
        _ => Vec::new(),
    }
}

/// A shape `prefix⟨key⟩suffix`, the command whose line declares names of
/// it and the one whose line names keys of it.
type Shape = (String, String, Option<String>, Option<String>);

fn shape(shapes: &mut Vec<Shape>, prefix: &str, suffix: &str) -> usize {
    match shapes.iter().position(|s| s.0 == prefix && s.1 == suffix) {
        Some(i) => i,
        None => {
            shapes.push((prefix.to_string(), suffix.to_string(), None, None));
            shapes.len() - 1
        }
    }
}

/// Relates what was written to what was read, once the run has ended. A
/// name a written line defined (`\newlabel{k}` defines `\r@k`) is a key the
/// writing call declares. A name of the same shape (`r@⟨key⟩`) a body call
/// reads is a use of that key, declared or not. A line that defines nothing
/// but whose argument its own call reads in such a name (`\cite{k}` writes
/// `\citation{k}` and reads `\b@k`) is a citation, and declarations of that
/// shape are bibliography entries. What is left names files: the databases.
pub fn relate(m: &mut Machine) {
    let lines = std::mem::take(&mut m.written_lines);
    let reads = std::mem::take(&mut m.name_reads);
    let mut shapes: Vec<Shape> = Vec::new();
    let mut declared: Vec<(usize, String, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let mut found: Vec<(String, String, String)> = Vec::new();
        let keys = line.keys();
        for name in &line.defined {
            if let Some(hit) = within(name, &keys) {
                match found.iter_mut().find(|f| f.0 == hit.0) {
                    Some(f) if f.1.len() + f.2.len() <= hit.1.len() + hit.2.len() => {}
                    Some(f) => *f = hit,
                    None => found.push(hit),
                }
            }
        }
        for (key, prefix, suffix) in found {
            let at = shape(&mut shapes, &prefix, &suffix);
            declared.push((at, key, i));
        }
    }
    // Two lines from one call (`\label` and cleveref's `\newlabel{⟨key⟩@cref}`)
    // each give a key: one inside the other is the key the document gave,
    // and the rest a package's addition around it.
    let spans: Vec<Span> = declared.iter().map(|d| lines[d.2].span).collect();
    let mut at_span: crate::fixed::HashMap<Span, Vec<usize>> = crate::fixed::HashMap::default();
    for (k, span) in spans.iter().enumerate() {
        at_span.entry(*span).or_default().push(k);
    }
    let canon: Vec<String> = (0..declared.len())
        .map(|j| {
            at_span[&spans[j]]
                .iter()
                .copied()
                .filter(|&k| declared[j].1.len() > declared[k].1.len())
                .map(|k| &declared[k].1)
                .filter(|shorter| declared[j].1.contains(shorter.as_str()))
                .min_by_key(|shorter| shorter.len())
                .cloned()
                .unwrap_or_else(|| declared[j].1.clone())
        })
        .collect();
    for (d, key) in declared.iter_mut().zip(canon) {
        d.1 = key;
    }
    for &(at, _, i) in &declared {
        shapes[at].2.get_or_insert_with(|| lines[i].head.clone());
    }
    let declaring: crate::fixed::HashSet<usize> = declared.iter().map(|d| d.2).collect();
    // Text a `\documentclass` or `\usepackage` call was given as an option
    // is that option wherever it is written again (babel writes its
    // languages to the `.aux`), never a key a read name was made from. The
    // option is recorded at its call, which may begin lines above it.
    let options: crate::fixed::HashSet<(crate::tex::FileId, &str)> = m
        .out
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::PassedOption)
        .map(|o| (o.span.file, o.key.as_str()))
        .collect();
    let mut reads_at: crate::fixed::HashMap<Span, Vec<&NameRead>> = crate::fixed::HashMap::default();
    for read in &reads {
        reads_at.entry(read.span).or_default().push(read);
    }
    let mut named: Vec<(usize, String, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate().filter(|(i, l)| !declaring.contains(i) && !l.head.is_empty()) {
        let keys = line.keys();
        for read in reads_at.get(&line.span).into_iter().flatten() {
            let name = m.name(read.name).to_string();
            if let Some((key, prefix, suffix)) = within(&name, &keys).filter(|(_, p, s)| !p.is_empty() || !s.is_empty())
            {
                let at = shape(&mut shapes, &prefix, &suffix);
                shapes[at].3.get_or_insert_with(|| line.head.clone());
                let written = line.at(&key).0;
                let option = options.contains(&(written.file, key.as_str()));
                if !option && !named.iter().any(|n| n.1 == key && n.2 == i) {
                    named.push((at, key, i));
                }
            }
        }
    }
    // Keys document calls declared while they ran: not a name the call
    // read as its own (`\begin{document}` reads `\document`), nor a file
    // it loaded (`\usepackage{x}` defines `\ver@x.sty`).
    let mut loads_at: crate::fixed::HashMap<Span, Vec<&String>> = crate::fixed::HashMap::default();
    for load in &m.out.facts.loads {
        loads_at.entry(load.span).or_default().push(&load.name);
    }
    let names: Vec<_> = std::mem::take(&mut m.key_names)
        .into_iter()
        .filter(|n| !reads_at.get(&n.3).into_iter().flatten().any(|r| m.name(r.name) == n.1))
        .filter(|n| !loads_at.get(&n.3).into_iter().flatten().any(|l| **l == n.1))
        .map(|(name, key, call, span, context)| {
            let at = name.find(key.as_str()).unwrap_or(0);
            let shape = (name[..at].to_string(), name[at + key.len()..].to_string());
            (shape, key, call, span, context)
        })
        .collect();
    // The first call to declare a key, written or while running, declares
    // it. A later one in another shape (`\ac{k}` marking `k` used) uses it,
    // and one in the same shape declares it again.
    let order = |s: &Span| (s.file != m.out.main_file, s.file, s.line, s.col);
    let mut first: crate::fixed::HashMap<String, (Span, bool)> = crate::fixed::HashMap::default();
    let written = declared.iter().map(|(_, k, i)| (k.clone(), lines[*i].span, false));
    for (key, span, running) in written.chain(names.iter().map(|n| (n.1.clone(), n.3, true))) {
        let at = first.entry(key).or_insert((span, running));
        if order(&span) < order(&at.0) {
            *at = (span, running);
        } else if span == at.0 {
            at.1 &= running;
        }
    }
    let mut first_shapes: crate::fixed::HashSet<(String, (String, String))> = crate::fixed::HashSet::default();
    for (at, key, i) in &declared {
        if first.get(key).is_some_and(|f| f.0 == lines[*i].span) {
            first_shapes.insert((key.clone(), (shapes[*at].0.clone(), shapes[*at].1.clone())));
        }
    }
    for n in &names {
        if first.get(&n.1).is_some_and(|f| f.0 == n.3) {
            first_shapes.insert((n.1.clone(), n.0.clone()));
        }
    }
    let running = |key: &str| first.get(key).is_some_and(|f| f.1);
    // A key a call declared while running is an entry where a line names
    // it for another program (`\glossaryentry`), not a citation.
    named.retain(|n| !running(&n.1));
    let use_kind = |key: &str| if running(key) { OccKind::KeyUse } else { OccKind::Ref };
    // A reference the kernel warned about is recorded already.
    let mut seen: crate::fixed::HashSet<(OccKind, String, Span, Option<Span>)> = m
        .out
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Ref)
        .map(|o| (o.kind, o.key.clone(), o.span, o.call.map(|c| c.1)))
        .collect();
    let written_at: crate::fixed::HashSet<(String, Span)> =
        declared.iter().map(|(_, k, i)| (k.clone(), lines[*i].span)).collect();
    for (at, key, i) in declared {
        let line = &lines[i];
        let shape = (shapes[at].0.clone(), shapes[at].1.clone());
        let again = !running(&key) && first_shapes.contains(&(key.clone(), shape));
        let kind = if again || first.get(&key).is_some_and(|f| f.0 == line.span) {
            if shapes[at].3.is_some() { OccKind::BibItem } else { OccKind::Label }
        } else if line.parts.contains(&key) {
            use_kind(&key)
        } else {
            continue;
        };
        if seen.insert((kind, key.clone(), line.span, line.context.call.map(|c| c.1))) {
            let (at, context) = line.at(&key);
            m.occurrence_in(kind, key, Some(line.head.clone()), at, &context);
        }
    }
    for (_, key, i) in &named {
        let line = &lines[*i];
        if seen.insert((OccKind::Cite, key.clone(), line.span, line.context.call.map(|c| c.1))) {
            let (at, context) = line.at(key);
            m.occurrence_in(OccKind::Cite, key.clone(), Some(line.head.clone()), at, &context);
        }
    }
    // The uses: a read of a name of a declared shape by a call given the
    // key, not by the line of the file being read back, nor by the call
    // that declares the key.
    let heads: crate::fixed::HashSet<&str> =
        shapes.iter().flat_map(|s| [s.2.as_deref(), s.3.as_deref()]).flatten().collect();
    let mut every: Vec<(String, String, bool)> =
        shapes.iter().filter(|s| s.2.is_some() && s.3.is_none()).map(|s| (s.0.clone(), s.1.clone(), false)).collect();
    every.extend(names.iter().map(|n| (n.0.0.clone(), n.0.1.clone(), true)));
    every.sort_by_key(|(p, s, _)| std::cmp::Reverse(p.len() + s.len()));
    every.dedup();
    let mut uses = Vec::new();
    for read in &reads {
        if heads.contains(m.name(read.call)) {
            continue;
        }
        let name = m.name(read.name);
        let key = every.iter().find_map(|(p, s, run)| {
            let key = name.strip_prefix(p.as_str())?.strip_suffix(s.as_str())?;
            (!key.is_empty() && read.parts.iter().any(|part| part == key)).then(|| (key.to_string(), *run))
        });
        let Some((key, run)) = key else { continue };
        if first.get(&key).is_some_and(|f| f.0 == read.span) {
            continue;
        }
        let kind = match first.get(&key) {
            Some(_) => use_kind(&key),
            None if run => OccKind::KeyUse,
            None => OccKind::Ref,
        };
        let related = [OccKind::Ref, OccKind::KeyUse, OccKind::Label, OccKind::BibItem, OccKind::Key]
            .iter()
            .any(|k| seen.contains(&(*k, key.clone(), read.span, read.context.call.map(|c| c.1))));
        if !related && seen.insert((kind, key.clone(), read.span, read.context.call.map(|c| c.1))) {
            uses.push((kind, key, read));
        }
    }
    for (kind, key, read) in uses {
        let head = m.name(read.call).to_string();
        m.occurrence_in(kind, key, Some(head), read.span, &read.context);
    }
    for (shape, key, call, span, context) in &names {
        let related = [OccKind::Ref, OccKind::KeyUse, OccKind::Label, OccKind::BibItem]
            .iter()
            .any(|k| seen.contains(&(*k, key.clone(), *span, context.call.map(|c| c.1))));
        if written_at.contains(&(key.clone(), *span)) || related {
            continue;
        }
        let kind =
            if first_shapes.contains(&(key.clone(), shape.clone())) && first.get(key).is_some_and(|f| f.0 == *span) {
                OccKind::Key
            } else {
                OccKind::KeyUse
            };
        if seen.insert((kind, key.clone(), *span, context.call.map(|c| c.1))) {
            let head = m.name(*call).to_string();
            m.occurrence_in(kind, key.clone(), Some(head), *span, context);
        }
    }
    // The keys the other lines carry are entries for the program that
    // reads their file. The rest names files, the bibliography databases.
    let mut tried = crate::fixed::HashSet::default();
    for (i, line) in lines.iter().enumerate() {
        if declaring.contains(&i) || named.iter().any(|n| n.2 == i) {
            continue;
        }
        for (key, at) in &line.keys {
            if seen.insert((OccKind::Entry, key.clone(), line.span, line.context.call.map(|c| c.1))) {
                let context = line.at(key).1;
                m.occurrence_in(OccKind::Entry, key.clone(), Some(line.head.clone()), *at, &context);
            }
        }
        for arg in &line.args {
            for name in crate::tex::comma_split(arg) {
                let name = name.trim().to_string();
                if name.is_empty() || name.contains(char::is_whitespace) || !tried.insert(name.clone()) {
                    continue;
                }
                // BibTeX's `\bibdata` names databases, found or not.
                if name.ends_with(".bib")
                    || line.head == "bibdata"
                    || crate::plugin::bib::resolve(&m.out, &name).is_some()
                {
                    m.occurrence_in(OccKind::Bibliography, name, None, line.span, &line.context);
                }
            }
        }
    }
}

/// A control sequence name `\string` spelled out at `at`: the escape
/// character and the letters after it (tex.web § 262), and the index after
/// them.
fn stringified(tokens: &[Token], at: usize) -> Option<(String, usize)> {
    let Tok::Chr('\\', Catcode::Other) = tokens[at].tok else { return None };
    let mut name = String::new();
    let mut end = at + 1;
    while let Some(Tok::Chr(c, Catcode::Other | Catcode::Letter)) = tokens.get(end).map(|t| t.tok) {
        if !(c.is_ascii_alphabetic() || c == '@') {
            break;
        }
        name.push(c);
        end += 1;
    }
    (!name.is_empty()).then_some((name, end))
}

/// The balanced group that starts at `start`, braces stripped, and the
/// index after its closing brace.
fn group_span(tokens: &[Token], start: usize) -> Option<(Vec<Token>, usize)> {
    let mut at = start;
    while tokens.get(at)?.is_cat(Catcode::Space) {
        at += 1;
    }
    if !tokens[at].is_cat(Catcode::Begin) {
        return None;
    }
    let mut depth = 1usize;
    let mut out = Vec::new();
    for (i, token) in tokens.iter().enumerate().skip(at + 1) {
        match token.tok {
            Tok::Chr(_, Catcode::Begin) => depth += 1,
            Tok::Chr(_, Catcode::End) => {
                depth -= 1;
                if depth == 0 {
                    return Some((out, i + 1));
                }
            }
            _ => {}
        }
        out.push(*token);
    }
    None
}

/// An image primitive (`\pdfximage`, pdfTeX manual § "Graphics") opens a
/// file: the image the call includes, under the name the driver handed the
/// engine and where that name resolves. Names the driver tried first
/// (`\Ginput@path`, `\Gin@extensions`) are the package's business. Only the
/// file the engine opens is recorded.
pub fn image(m: &mut Machine, name: &str, span: Span) -> Option<std::path::PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let at = m.file_call.map_or(span, |(_, at)| at);
    let base = m.base().to_path_buf();
    let path = m.resolver_mut().resolve(name, crate::builtins::LoadKind::Input, &base);
    m.occurrence(OccKind::Graphics, name.to_string(), path.as_ref().map(|p| p.display().to_string()), at);
    if let Some(path) = &path {
        m.note_read_file(path);
    }
    path
}

/// `\accent⟨8-bit number⟩⟨character⟩` (tex.web § 1123): the accent is
/// placed over the character by the typesetter, so the word it stands in
/// is no longer one TeX can hyphenate (The TeXbook, appendix H).
pub fn accent(m: &mut Machine, span: Span) -> Option<i64> {
    let code = m.scan_number();
    let at = m.file_call.map_or(span, |(_, at)| at);
    m.occurrence(OccKind::Accent, code.unwrap_or(0).to_string(), None, at);
    code
}

/// The payload of a PDF primitive, in the syntax the pdfTeX manual gives
/// (`\pdfliteral [shipout] [direct|page] ⟨general text⟩`, `\pdfobj
/// [useobjnum n] [stream [attr ⟨general text⟩]] [file] ⟨general text⟩`) and
/// the LuaTeX manual gives for `\pdfextension literal` and `obj`.
pub fn pdf(m: &mut Machine, op: PdfOp, span: Span) {
    let op = match op {
        PdfOp::Extension if m.scan_keyword("literal") => PdfOp::Literal,
        PdfOp::Extension if m.scan_keyword("obj") => PdfOp::Object,
        // Another extension reads what follows by its own syntax, which the
        // run leaves in the input as before.
        PdfOp::Extension => return,
        op => op,
    };
    let detail = match op {
        PdfOp::Literal => {
            m.scan_keyword("shipout");
            let _ = m.scan_keyword("direct") || m.scan_keyword("page") || m.scan_keyword("raw");
            "literal"
        }
        PdfOp::Object => {
            if m.scan_keyword("reserveobjnum") {
                return;
            }
            if m.scan_keyword("useobjnum") {
                m.scan_number();
            }
            if m.scan_keyword("stream") && m.scan_keyword("attr") {
                let attributes = m.read_general_text();
                record_pdf(m, &attributes, "object", span);
            }
            if m.scan_keyword("file") {
                m.read_general_text();
                return;
            }
            "object"
        }
        // A token list parameter: `\pdfpageresources ⟨equals⟩ ⟨general text⟩`.
        PdfOp::Resources => {
            m.scan_keyword("=");
            "resources"
        }
        PdfOp::Special | PdfOp::Extension => "special",
    };
    let payload = m.read_general_text();
    record_pdf(m, &payload, detail, span);
}

fn record_pdf(m: &mut Machine, payload: &[Token], detail: &str, span: Span) {
    // The engine expands the text when it writes it (pdfTeX manual,
    // `\pdfliteral`), which for a literal inside a box is at shipout.
    let expanded = m.expand_tokens(payload.into());
    let text = crate::tex::text_with_groups(&expanded, &m.out.interner);
    let at = m.file_call.map_or(span, |(_, at)| at);
    m.occurrence(OccKind::Pdf, text, Some(detail.to_string()), at);
}

/// The characters of an expanded argument: a control sequence that survived
/// expansion is not text.
fn characters(tokens: &[Token]) -> String {
    tokens
        .iter()
        .filter_map(|t| match t.tok {
            Tok::Chr(c, cat) if !matches!(cat, Catcode::Begin | Catcode::End | Catcode::Active) => Some(c),
            _ => None,
        })
        .collect()
}

/// `\ver@⟨file⟩` was defined: the kernel's record of what a file says it is
/// (clsguide, "Identification"; `\ProvidesPackage`, `\ProvidesClass` and
/// `\ProvidesFile` write it, `\@ifpackagelater` reads it). Made by the call
/// the file itself read, it identifies that file.
pub fn version_record(m: &mut Machine, name: &str, meaning: &crate::tex::Meaning) {
    let Some(file) = name.strip_prefix("ver@") else { return };
    let Some(mac) = meaning.as_macro() else { return };
    let Some((_, span)) = m.file_call else { return };
    // A recorded path is a `PathBuf`'s text, whose separator is not `/` on every host.
    let base = m.out.short_name(span.file).to_string();
    if base != file {
        return;
    }
    let seen = m.out.facts.occurrences.iter().any(|o| o.kind == OccKind::Identification && o.span == span);
    if seen {
        return;
    }
    // The text is looked at: when it holds a copied text, that is read.
    if let Some(sym) = m.out.interner.lookup(name)
        && m.env.carries(sym)
    {
        m.env.note_read(sym, crate::env::ReadKind::Meaning);
    }
    let info = m.text_of(&mac.replacement_text);
    m.record_identification(&info);
    let key = file.strip_suffix(".sty").or_else(|| file.strip_suffix(".cls")).unwrap_or(file).to_string();
    m.occurrence(OccKind::Identification, key, Some(info), span);
}

/// A number typeset, while letters may still follow it.
#[derive(Default, Clone)]
struct Run {
    number: String,
    unit: String,
    span: Span,
    number_end: Span,
    unit_span: Span,
    via: Option<crate::tex::Sym>,
    within: std::rc::Rc<[crate::tex::FileId]>,
    /// Every character of it was set on every run.
    certain: bool,
}

/// The number and unit tracker's state, held by the [`Machine`].
#[derive(Default, Clone)]
pub struct Quantities {
    run: Option<Run>,
    /// `H2O` is a word, not a quantity.
    after_letter: bool,
}

/// A character typeset outside package code: a digit run and the letters set
/// right after it become a [`crate::facts::Quantity`]. A space token ends
/// the run, so `2 ms` is not one.
pub fn typeset(m: &mut Machine, c: char, span: Span) {
    if m.package().is_some() || m.reading_format() {
        m.quantity = Quantities::default();
        m.text_broken = true;
        return;
    }
    if m.in_nullfont() {
        m.text_broken = true;
        return;
    }
    set(m, c, span);
    // A space after `\nobreak` (a tie, `~`) is no breakpoint (tex.web
    // § 831): the number and the unit stay together on the line, spaced
    // by hand, and the run goes on.
    let tied = c == ' '
        && m.quantity.run.as_ref().is_some_and(|q| q.unit.is_empty())
        && m.natural.is_some_and(|n| matches!(n.tail[0], crate::mode::Last::Penalty(p) if p >= 10_000));
    if tied {
        return;
    }
    let after_letter = std::mem::replace(&mut m.quantity.after_letter, c.is_alphabetic());
    let mut run = m.quantity.run.take();
    if c.is_ascii_digit() {
        match run.as_mut() {
            Some(q) if q.unit.is_empty() => {
                q.number.push(c);
                q.number_end = span;
            }
            _ => {
                flush(m, run.take());
                run = (!after_letter).then(|| Run {
                    number: c.to_string(),
                    span,
                    number_end: span,
                    unit_span: span,
                    within: m.within_files(),
                    certain: m.exact(),
                    ..Run::default()
                });
            }
        }
    } else if matches!(c, '.' | ',') {
        match run.as_mut() {
            Some(q) if q.unit.is_empty() && !q.number.ends_with(['.', ',']) => q.number.push(c),
            _ => {
                flush(m, run.take());
            }
        }
    } else if (c.is_alphabetic() || c == '%' || c == '\u{b0}') && run.is_some() {
        // A separator with no digit after it ends the number: `$N_1,N_2$`
        // lists, it sets no `1,` newton.
        if run.as_ref().is_some_and(|q| q.unit.is_empty() && q.number.ends_with(['.', ','])) {
            m.quantity.run = None;
            return;
        }
        let via = m.file_call.map(|(sym, _)| sym);
        let exact = m.exact();
        if let Some(q) = run.as_mut() {
            q.certain &= exact;
            if q.unit.is_empty() {
                q.unit_span = span;
                q.via = via;
            }
            q.unit.push(c);
        }
    } else {
        flush(m, run.take());
    }
    m.quantity.run = run;
}

/// A `\/` or a paragraph's end: the last character's overhang is taken
/// back or has nothing after it on its line.
pub fn close_overhang(m: &mut Machine) {
    let text = &mut m.out.facts.text;
    let at = text.at.len() as u32;
    if text.closed.last() != Some(&at) {
        text.closed.push(at);
    }
}

/// A group boundary breaks TeX's word, so no ligature joins across it
/// (tex.web § 1034), but `12{,}5 km` is still one quantity.
pub fn word_break(m: &mut Machine) {
    m.text_broken = true;
}

/// A box began or ended, or a box's list was placed: the next character
/// set may stand anywhere on the page relative to the last one.
pub fn box_break(m: &mut Machine) {
    m.text_apart = true;
    text_break(m);
}

/// The same, and the number-and-unit run ends too: `\sum_{i=1}^{N}` sets a
/// `1` and an `N`, not one newton.
pub fn text_break(m: &mut Machine) {
    word_break(m);
    let run = m.quantity.run.take();
    m.quantity.after_letter = false;
    flush(m, run);
}

fn flush(m: &mut Machine, run: Option<Run>) {
    let Some(q) = run else { return };
    if q.unit.is_empty() || m.out.facts.quantities.len() >= m.cfg.limits.facts {
        return;
    }
    m.out.facts.quantities.push(crate::facts::Quantity {
        number: q.number,
        unit: q.unit,
        span: q.span,
        number_end: q.number_end,
        unit_span: q.unit_span,
        via: q.via,
        within: q.within,
        certain: q.certain,
    });
}

/// Add `c` to [`crate::facts::Text`], the characters the document set in
/// order. A character is `broken` from the one before it when something
/// that is not a character came between them, or the two don't stand side
/// by side in one line of one file: TeX's ligature program joins neither
/// (tex.web § 1034).
fn set(m: &mut Machine, c: char, span: Span) {
    if m.out.facts.text.at.len() >= m.cfg.limits.facts {
        return;
    }
    let font_state = m.observed_font_state();
    let certain = m.exact();
    let first = m.cds.first().copied();
    let site = m.call_site(span);
    let text = &mut m.out.facts.text;
    // Both paths through an undecided conditional digest what follows it,
    // so the one character on the page arrives here once per path.
    if !certain && text.at.last().is_some_and(|last| last.span == span) && text.chars.ends_with(c) {
        // The same arm setting it again is a loop pass.
        let last = text.at.len() as u32 - 1;
        if first.is_some_and(|cd| text.arms.last() == Some(&(last, cd.on, cd.taken)))
            && text.repeats.last() != Some(&last)
        {
            text.repeats.push(last);
        }
        return;
    }
    let side_by_side = text
        .at
        .last()
        .is_some_and(|last| (last.span.file, last.span.line, last.span.col + 1) == (span.file, span.line, span.col));
    let broken = std::mem::take(&mut m.text_broken) || !side_by_side;
    if let Some(cd) = first.filter(|_| !certain) {
        text.arms.push((text.at.len() as u32, cd.on, cd.taken));
    }
    let apart = std::mem::take(&mut m.text_apart);
    text.chars.push(c);
    text.at.push(crate::facts::Spot { span, font_state, modes: m.mode, broken, apart, certain });
    text.site.push(site);
}
