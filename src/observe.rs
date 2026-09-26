//! Facts the interpreter notes while a construct runs, rather than by
//! modeling the command that caused it: the lines a `\write` hands a file
//! and the keys they declare, what a `\csname` was formed for, the payloads
//! handed to the PDF writer.

use crate::builtins::{Cond, OccKind, PdfOp, Primitive};
use crate::facts::CsnameRole;
use crate::machine::Machine;
use crate::tex::{Catcode, Span, Tok, Token};

/// A `\csname…\endcsname` just formed `name`.
pub fn csname(m: &mut Machine, name: &str, span: Span) {
    let role = role(m, span);
    m.note_csname(span, role);
    // `\let\x=\csname…\endcsname` reads the name as surely as a use
    // does: cleveref's `\cref@getref` takes `\r@⟨label⟩@cref` so.
    if matches!(role, CsnameRole::Use | CsnameRole::Test | CsnameRole::LetTo) {
        read_name(m, name);
    }
}

/// Where an occurrence recorded once the run has ended belongs.
#[derive(Clone)]
pub struct OccContext {
    pub package: Option<crate::tex::Sym>,
    pub expanded: Option<Span>,
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
    /// The arguments that are text the document gave the writing call.
    keys: Vec<String>,
    /// The contents of the writing call's own braced arguments.
    parts: std::rc::Rc<[String]>,
    defined: Vec<String>,
    span: Span,
    context: OccContext,
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
    let Some((here, text)) = m.call_extent(span) else { return std::rc::Rc::from([]) };
    if let Some((at, until, parts)) = &m.call_parts
        && (*at, *until) == (span, here)
    {
        return parts.clone();
    }
    let parts: std::rc::Rc<[String]> = argument_parts(&text).into();
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

/// A name a body call read — used, tested, or taken by `\let`: matched
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
/// ⟨n⟩ undefined`.  On a first run it is the only trace a reference leaves.
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
/// a new value there is an environment beginning.  `\begin{document}`
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
        m.out.preamble_files = Some(m.out.files.len());
        m.out.preamble = Some(Box::new((m.env.snapshot(), m.catcodes.clone())));
    } else {
        // `document` itself is not on the stack: `document_depth` already
        // answers the preamble/document axis, so this is only the user
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
    let at = m.file_call.map_or(span, |(_, at)| at);
    if let Some(node) = m.occurrence(OccKind::EndEnvironment, inner.clone(), None, at) {
        let end = m.intern(&format!("end{inner}"));
        m.reads_definition_of(node, end);
    }
}

/// `\@ifundefined{⟨name⟩}` tests the name `\csname` would build from its
/// argument: the characters up to the first control sequence that is left.
/// The kernel's empty macros, `\@empty` among them, are no-ops in this
/// interpreter, so they are passed over as their expansion would be.
pub fn tested_tokens(m: &mut Machine, tokens: &[Token]) {
    let mut name = String::new();
    for token in tokens {
        match token.tok {
            Tok::Chr(c, cat) if cat != Catcode::Active => name.push(c),
            Tok::Param(_) => {}
            Tok::Cs(sym) if m.env.meaning(sym).prim() == Some(Primitive::Relax) => {}
            _ => break,
        }
    }
    read_name(m, &name);
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
    // A stream never opened writes to the log (tex.web § 1370), but a
    // fragment run without `\begin{document}` never opens `\@auxout`: what
    // the fragment itself writes there stands for a line of it.  A package
    // writing to a closed stream (`\typeout` to `\@unused`) only logs.
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
/// `\begin{document}`, where the `.aux` file is read.  A line an argument
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
    let mut keys = Vec::new();
    let here = m.call_extent(span).map(|(here, _)| here);
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
    let text = crate::tex::text_with_groups(tokens, &m.out.interner);
    let defined = sandbox_run(m, &text);
    let parts = call_parts(m, span);
    m.written_lines.push(WrittenLine { head, args, keys, parts, defined, span, context });
}

/// The keys the document gave a written argument: runs of characters
/// whose tokens stand in the calling file no later than it has been read,
/// split at commas (`\citation{a,b}`, cite.sty).  What a package adds to
/// them (cleveref's `@cref`) ends a run.
fn given(group: &[Token], call: Span, here: Option<Span>) -> Vec<String> {
    let Some(here) = here else { return Vec::new() };
    let mut keys = Vec::new();
    let mut run = String::new();
    let mut flush = |run: &mut String| {
        keys.extend(run.split(',').map(str::trim).filter(|k| !k.is_empty()).map(str::to_string));
        run.clear();
    };
    for token in group {
        let ours = token.span.file == call.file && (token.span.line, token.span.col) <= (here.line, here.col);
        match token.tok {
            Tok::Chr(c, cat) if ours && !matches!(cat, Catcode::Begin | Catcode::End) => run.push(c),
            _ => flush(&mut run),
        }
    }
    flush(&mut run);
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
    // (`\numberline{1}`); `\protect` is gone once shipped.
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
        let sandbox = Machine::sandbox(m.cfg, &m.out, true).map(Box::new);
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

/// Relates what was written to what was read, once the run has ended.  A
/// name a written line defined (`\newlabel{k}` defines `\r@k`) is a key the
/// writing call declares, and a name of the same shape (`r@⟨key⟩`) a body
/// call reads is a use of that key, declared or not.  A line that defines
/// nothing but whose argument its own call reads in such a name (`\cite{k}`
/// writes `\citation{k}` and reads `\b@k`) names the key for the program
/// that reads the file: a citation, and the declarations of that shape are
/// bibliography entries.  What is left names files: the databases.
pub fn relate(m: &mut Machine) {
    let lines = std::mem::take(&mut m.written_lines);
    let reads = std::mem::take(&mut m.name_reads);
    // A shape `prefix⟨key⟩suffix`, the command whose line declares names of
    // it and the one whose line names keys of it.
    let mut shapes: Vec<Shape> = Vec::new();
    let mut declared: Vec<(usize, String, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let mut found: Vec<(String, String, String)> = Vec::new();
        for name in &line.defined {
            if let Some(hit) = within(name, &line.keys) {
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
    for &(at, _, i) in &declared {
        shapes[at].2.get_or_insert_with(|| lines[i].head.clone());
    }
    let declaring: std::collections::HashSet<usize> = declared.iter().map(|d| d.2).collect();
    let mut named: Vec<(usize, String, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate().filter(|(i, l)| !declaring.contains(i) && !l.head.is_empty()) {
        for read in reads.iter().filter(|r| r.span == line.span) {
            let name = m.name(read.name).to_string();
            if let Some((key, prefix, suffix)) =
                within(&name, &line.keys).filter(|(_, p, s)| !p.is_empty() || !s.is_empty())
            {
                let at = shape(&mut shapes, &prefix, &suffix);
                shapes[at].3.get_or_insert_with(|| line.head.clone());
                if !named.iter().any(|n| n.1 == key && n.2 == i) {
                    named.push((at, key, i));
                }
            }
        }
    }
    // Keys document calls declared while they ran: not a name the call
    // read as its own (`\begin{document}` reads `\document`), nor a file
    // it loaded (`\usepackage{x}` defines `\ver@x.sty`).
    let names: Vec<_> = std::mem::take(&mut m.key_names)
        .into_iter()
        .filter(|n| !reads.iter().any(|r| r.span == n.3 && m.name(r.name) == n.1))
        .filter(|n| !m.out.facts.loads.iter().any(|l| l.span == n.3 && l.name == n.1))
        .map(|(name, key, call, span, context)| {
            let at = name.find(key.as_str()).unwrap_or(0);
            let shape = (name[..at].to_string(), name[at + key.len()..].to_string());
            (shape, key, call, span, context)
        })
        .collect();
    // The first call to declare a key, written or while running, declares
    // it; a later one in another shape (`\ac{k}` marking `k` used) uses it,
    // one in the same shape declares it again.
    let order = |s: &Span| (s.file != m.out.main_file, s.file, s.line, s.col);
    let mut first: std::collections::HashMap<String, (Span, bool)> = std::collections::HashMap::new();
    let written = declared.iter().map(|(_, k, i)| (k.clone(), lines[*i].span, false));
    for (key, span, running) in written.chain(names.iter().map(|n| (n.1.clone(), n.3, true))) {
        let at = first.entry(key).or_insert((span, running));
        if order(&span) < order(&at.0) {
            *at = (span, running);
        } else if span == at.0 {
            at.1 &= running;
        }
    }
    let mut first_shapes: std::collections::HashSet<(String, (String, String))> = std::collections::HashSet::new();
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
    let mut seen: std::collections::HashSet<(OccKind, String, Span)> = m
        .out
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Ref)
        .map(|o| (o.kind, o.key.clone(), o.span))
        .collect();
    let written_at: std::collections::HashSet<(String, Span)> =
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
        if seen.insert((kind, key.clone(), line.span)) {
            m.occurrence_in(kind, key, Some(line.head.clone()), line.span, &line.context);
        }
    }
    for (_, key, i) in &named {
        let line = &lines[*i];
        if seen.insert((OccKind::Cite, key.clone(), line.span)) {
            m.occurrence_in(OccKind::Cite, key.clone(), Some(line.head.clone()), line.span, &line.context);
        }
    }
    // The uses: a read of a name of a declared shape by a call given the
    // key, not by the line of the file being read back, nor by the call
    // that declares the key.
    let heads: std::collections::HashSet<&str> =
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
            .any(|k| seen.contains(&(*k, key.clone(), read.span)));
        if !related && seen.insert((kind, key.clone(), read.span)) {
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
            .any(|k| seen.contains(&(*k, key.clone(), *span)));
        if written_at.contains(&(key.clone(), *span)) || related {
            continue;
        }
        let kind =
            if first_shapes.contains(&(key.clone(), shape.clone())) && first.get(key).is_some_and(|f| f.0 == *span) {
                OccKind::Key
            } else {
                OccKind::KeyUse
            };
        if seen.insert((kind, key.clone(), *span)) {
            let head = m.name(*call).to_string();
            m.occurrence_in(kind, key.clone(), Some(head), *span, context);
        }
    }
    // The keys the other lines carry are entries for the program that
    // reads their file; the rest names files, the bibliography databases.
    let mut tried = std::collections::HashSet::new();
    for (i, line) in lines.iter().enumerate() {
        if declaring.contains(&i) || named.iter().any(|n| n.2 == i) {
            continue;
        }
        for key in &line.keys {
            if seen.insert((OccKind::Entry, key.clone(), line.span)) {
                m.occurrence_in(OccKind::Entry, key.clone(), Some(line.head.clone()), line.span, &line.context);
            }
        }
        for arg in &line.args {
            for name in crate::tex::comma_split(arg) {
                let name = name.trim().to_string();
                if name.is_empty() || name.contains(char::is_whitespace) || !tried.insert(name.clone()) {
                    continue;
                }
                if name.ends_with(".bib") || crate::plugin::bib::resolve(&m.out, &name).is_some() {
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
/// file: the image the call it stands in includes, under the name the
/// driver handed the engine and where that name resolves.  Which names the
/// driver tried first, and in which directories, is the package's business
/// (`\Ginput@path`, `\Gin@extensions`); only the file the engine opens is
/// recorded.
pub fn image(m: &mut Machine, name: &str, span: Span) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let at = m.file_call.map_or(span, |(_, at)| at);
    let base = m.base().to_path_buf();
    let found =
        m.resolver_mut().resolve(name, crate::builtins::LoadKind::Input, &base).map(|p| p.display().to_string());
    m.occurrence(OccKind::Graphics, name.to_string(), found, at);
}

/// `\accent⟨8-bit number⟩⟨character⟩` (tex.web § 1123): the accent is
/// placed over the character by the typesetter, so the word it stands in
/// is no longer one TeX can hyphenate (The TeXbook, appendix H).
pub fn accent(m: &mut Machine, span: Span) {
    let code = m.scan_number().unwrap_or(0);
    let at = m.file_call.map_or(span, |(_, at)| at);
    m.occurrence(OccKind::Accent, code.to_string(), None, at);
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
/// `\ProvidesFile` write it, `\@ifpackagelater` reads it).  Made by the call
/// the file itself read, it identifies that file.
pub fn version_record(m: &mut Machine, name: &str, meaning: &crate::tex::Meaning) {
    let Some(file) = name.strip_prefix("ver@") else { return };
    let Some(mac) = meaning.as_macro() else { return };
    let Some((_, span)) = m.file_call else { return };
    let path = m.out.file_name(span.file).to_string();
    let base = path.rsplit('/').next().unwrap_or(&path);
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
#[derive(Default)]
struct Run {
    number: String,
    unit: String,
    span: Span,
    number_end: Span,
    unit_span: Span,
    via: Option<crate::tex::Sym>,
    within: std::rc::Rc<[crate::tex::FileId]>,
}

/// The number and unit tracker's state, held by the [`Machine`].
#[derive(Default)]
pub struct Quantities {
    run: Option<Run>,
    /// `H2O` is a word, not a quantity.
    after_letter: bool,
}

/// A character typeset outside package code: a digit run and the letters set
/// right after it become a [`crate::facts::Quantity`].  A space token ends
/// the run, so `2 ms` is not one.
pub fn typeset(m: &mut Machine, c: char, span: Span) {
    if m.package().is_some() || m.reading_format() {
        m.quantity = Quantities::default();
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
        let via = m.file_call.map(|(sym, _)| sym);
        if let Some(q) = run.as_mut() {
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

/// `\sum_{i=1}^{N}` sets a `1` and an `N`, not one newton.
pub fn text_break(m: &mut Machine) {
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
    });
}
