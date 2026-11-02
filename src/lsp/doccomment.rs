//! What a comment block right above a definition documents: the command,
//! per parameter what it takes (`% #1: the name`), and an example.
//!
//! Packages write these blocks in several styles: `% #1 = …` lines (pgf),
//! a usage line of `{<name>}` groups (etoolbox), a blank line or `%` rules
//! between block and code, an `Example:` section, and the doc classes'
//! markup (`% \begin{macro}{\foo}`, `\cs{x}`) in code comments.

#[derive(Default, Debug, PartialEq)]
pub struct DocComment {
    pub text: String,
    /// Parameter number and its text, in the order given.
    pub params: Vec<(u8, String)>,
    /// Code lines the block shows how to use it with.
    pub example: Vec<String>,
}

/// The comment block that ends on the line above 1-based `line` of
/// `source` (one blank line may stand between), and the comment that ends
/// that line. A `#n` in the text becomes `⟨argn⟩`, the name the usage line
/// gives that argument.
pub fn above(source: &str, line: usize) -> Option<DocComment> {
    let lines: Vec<&str> = source.lines().collect();
    let at = line.checked_sub(1)?;
    let mut block = Vec::new();
    if let Some(trailing) = lines.get(at).and_then(|l| trailing_comment(l)) {
        block.push(trailing);
    }
    let comment = |i: usize| lines.get(i).and_then(|l| l.trim().strip_prefix('%'));
    let blank = |i: usize| lines.get(i).is_some_and(|l| l.trim().is_empty());
    // pgf and etoolbox leave one blank line between the block and the code.
    let mut i = at;
    if i >= 2 && blank(i - 1) && comment(i - 2).is_some() {
        i -= 1;
    }
    while i > 0 {
        let Some(text) = comment(i - 1) else { break };
        let text = text.trim_start_matches('%').trim_end();
        // A rule above text already read closes the block: what stands above it heads a section.
        if is_rule(text) && block.iter().any(|l| !l.is_empty() && !is_rule(l)) {
            break;
        }
        block.push(text);
        i -= 1;
    }
    block.reverse();
    let mut doc = parse(&block)?;
    doc.text = arg_names(&doc.text);
    Some(doc)
}

/// The comment after code on `line`: `\def\x#1% the year`.
fn trailing_comment(line: &str) -> Option<&str> {
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            '%' if !escaped => {
                let code = line[..i].trim();
                return (!code.is_empty()).then(|| line[i..].trim_start_matches('%').trim()).filter(|t| !t.is_empty());
            }
            '\\' => escaped = !escaped,
            _ => escaped = false,
        }
    }
    None
}

/// A line of rule characters only (`-----`, `=====`, `***`).
fn is_rule(line: &str) -> bool {
    !line.is_empty() && line.chars().all(|c| "-=*#~_+/".contains(c) || c.is_whitespace())
}

fn arg_names(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match chars.peek().filter(|d| c == '#' && ('1'..='9').contains(d)) {
            Some(&d) => {
                out.push_str(&format!("⟨arg{d}⟩"));
                chars.next();
            }
            None => out.push(c),
        }
    }
    out
}

/// `block`'s lines, each as the comment holds it after the `%`.
fn parse(block: &[&str]) -> Option<DocComment> {
    let mut doc = DocComment::default();
    let indent = |raw: &str| raw.len() - raw.trim_start().len();
    // The doc classes' markup in a code comment (`\begin{macro}{\foo}`): rendered as the manuals are.
    let markup = block.iter().any(|l| names_a_command(l.trim()));
    let mut in_example = false;
    let mut usage = Vec::new();
    // The indentation of the parameter line read last: deeper lines carry its text on.
    let mut param_indent = None;
    for raw in block {
        let line = raw.trim();
        if in_example && !line.is_empty() {
            doc.example.push(raw.strip_prefix(' ').unwrap_or(raw).to_string());
            continue;
        }
        // Rules, magic comments (`%!TEX`), empty lines and environment markup say nothing.
        if line.is_empty() || line.starts_with('!') || is_rule(line) || (markup && environment_line(line)) {
            param_indent = None;
            continue;
        }
        if let Some(head) = heading(line) {
            in_example = head.to_lowercase().starts_with("example");
            continue;
        }
        let carried = param_indent.is_some_and(|i| indent(raw) > i) || line.starts_with(char::is_lowercase);
        if !markup && line.starts_with('\\') {
            doc.example.push(line.to_string());
        } else if let Some(param) = param(line) {
            doc.params.push(param);
            param_indent = Some(indent(raw));
        } else if let Some(names) = usage_line(line) {
            usage = names;
        } else if let Some((_, text)) = doc.params.last_mut().filter(|_| carried && param_indent.is_some()) {
            text.push(' ');
            text.push_str(line);
        } else {
            if !doc.text.is_empty() {
                doc.text.push(' ');
            }
            match markup {
                true => doc.text.push_str(&crate::plugin::docs::markup::render(line, false)),
                false => doc.text.push_str(line),
            }
        }
    }
    // A usage line names the parameters no `#n` line describes.
    if doc.params.is_empty() {
        doc.params = usage.into_iter().zip(1..).map(|(name, n)| (n, format!("⟨{name}⟩"))).collect();
    }
    (!doc.text.is_empty() || !doc.params.is_empty() || !doc.example.is_empty()).then_some(doc)
}

/// `\begin{macro}{\foo}`: environment markup naming a command, as the doc classes document one.
fn names_a_command(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("\\begin") else { return false };
    let Some((_, rest)) = crate::plugin::docs::markup::group(rest, '{', '}') else { return false };
    crate::plugin::docs::markup::group(rest, '{', '}').is_some_and(|(name, _)| name.trim_start().starts_with('\\'))
}

/// `\begin{macro}{\foo}`, `\end{macrocode}`: a line of environment markup only.
fn environment_line(line: &str) -> bool {
    let Some(mut rest) = line.strip_prefix("\\begin").or_else(|| line.strip_prefix("\\end")) else { return false };
    while let Some((_, after)) = crate::plugin::docs::markup::group(rest, '{', '}') {
        rest = after;
    }
    rest.trim().is_empty()
}

/// `Example:`, `Description:`: a section heading of one or two words.
fn heading(line: &str) -> Option<&str> {
    let head = line.strip_suffix(':')?;
    (head.split_whitespace().count() <= 2 && head.starts_with(char::is_alphabetic)).then_some(head)
}

/// `{<name>}[<options>]`, optionally after the command: the argument names
/// of a usage line, in order.
fn usage_line(line: &str) -> Option<Vec<String>> {
    let mut rest = line.trim_start_matches('\\').trim_start_matches(|c: char| c.is_alphanumeric() || "@*".contains(c));
    let mut names = Vec::new();
    while !rest.trim().is_empty() {
        let (inner, after) = match rest.trim_start().chars().next()? {
            '{' => crate::plugin::docs::markup::group(rest, '{', '}')?,
            '[' => crate::plugin::docs::markup::group(rest, '[', ']')?,
            _ => return None,
        };
        let inner = inner.trim();
        let name = inner.strip_prefix('<').and_then(|n| n.strip_suffix('>'));
        let name = name.or_else(|| inner.strip_prefix('⟨').and_then(|n| n.strip_suffix('⟩')))?;
        names.push(name.to_string());
        rest = after;
    }
    (!names.is_empty()).then_some(names)
}

/// `#1: the name`, `#2 = …`, `@param 1 …`, `arg 1 - …`, `Argument 2: …`.
fn param(line: &str) -> Option<(u8, String)> {
    const WORDS: [&str; 6] = ["@param", "\\param", "parameter", "param", "argument", "arg"];
    let lower = line.to_lowercase();
    let rest = match line.strip_prefix('#') {
        Some(rest) => rest,
        None => {
            let word = WORDS.iter().find(|w| lower.starts_with(*w))?;
            line[word.len()..].trim_start().trim_start_matches('#')
        }
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let n: u8 = rest[..digits].parse().ok().filter(|n| (1..=9).contains(n))?;
    let text = rest[digits..].trim_start_matches(|c: char| c.is_whitespace() || ":=-–—.)".contains(c)).trim(); // not prose
    (!text.is_empty()).then(|| (n, text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_gives_the_text_and_each_parameter() {
        let source = "% unrelated\n\n% Greets someone.\n%\n% #1: who is greeted,\n%     by name\n% Arg 2 - the greeting\n\\newcommand{\\greet}[2]{#2, #1!}\n";
        let doc = above(source, 8).expect("documented");
        assert_eq!(doc.text, "Greets someone.");
        assert_eq!(doc.params, vec![(1, "who is greeted, by name".into()), (2, "the greeting".into())]);
    }

    #[test]
    fn the_comment_on_the_definition_line_names_its_arguments() {
        let source = "\\def\\ckleapyear#1% check if #1 is a leap year.\n  {#1}\n";
        assert_eq!(above(source, 1).expect("documented").text, "check if ⟨arg1⟩ is a leap year.");
        assert_eq!(above("\\def\\x{50\\%}\n", 1), None);
    }

    #[test]
    fn no_block_no_documentation() {
        assert_eq!(above("\\def\\x{}\n", 1), None);
        assert_eq!(above("%%%%%%\n\\def\\x{}\n", 2), None);
        // Two blank lines part a block from the code.
        assert_eq!(above("% A header.\n\n\n\\def\\x{}\n", 4), None);
        // Code above ends the block.
        assert_eq!(above("\\let\\a\\b\n\n\\def\\x{}\n", 3), None);
    }

    /// pgf's style: a blank line before the code, `#n = …`, an example.
    #[test]
    fn a_block_a_blank_line_above_documents_with_its_example() {
        let source = "\\relax\n\n\n% Return a point\n%\n% #1 = x-coordinate of the point\n% #2 = y-coordinate of the point\n%\n% Example:\n%\n% \\pathto{\\point{2pt}{3cm}}\n\n\\def\\point#1#2{}\n";
        let doc = above(source, 13).expect("documented");
        assert_eq!(doc.text, "Return a point");
        assert_eq!(doc.params, vec![(1, "x-coordinate of the point".into()), (2, "y-coordinate of the point".into())]);
        assert_eq!(doc.example, vec!["\\pathto{\\point{2pt}{3cm}}".to_string()]);
    }

    /// A parameter's text goes on in deeper lines; an example keeps its environments.
    #[test]
    fn deeper_lines_carry_a_parameter_on() {
        let source = "% #1 = a list of colors\n%      \"rgb(1cm)=(1,0,0)\". More are\n%      separated by semicolons.\n%\n% Example:\n%\n% \\begin{document}\n%   \\shade{x}\n% \\end{document}\n\n\\def\\shading#1{}\n";
        let doc = above(source, 11).expect("documented");
        assert_eq!(
            doc.params,
            vec![(1, "a list of colors \"rgb(1cm)=(1,0,0)\". More are separated by semicolons.".into())]
        );
        assert_eq!(doc.example, ["\\begin{document}", "  \\shade{x}", "\\end{document}"]);
    }

    /// etoolbox's style: a usage line of `{<name>}` groups names the arguments.
    #[test]
    fn a_usage_line_names_the_arguments() {
        let source = "% {<string1>}{<string2>}[<flag>]\n\n\\newrobustcmd{\\same}[3]{}\n";
        let doc = above(source, 3).expect("documented");
        assert_eq!(doc.params, vec![(1, "⟨string1⟩".into()), (2, "⟨string2⟩".into()), (3, "⟨flag⟩".into())]);
        assert!(doc.text.is_empty());
    }

    /// A rule closes a block from the section heading above it; rules framing the block are skipped.
    #[test]
    fn rules_part_sections_from_the_block() {
        let source = "% -----------\n% Utilities\n% -----------\n% Adds one.\n\\def\\inc{}\n";
        assert_eq!(above(source, 5).expect("documented").text, "Adds one.");
        let framed = "%%%%%%%%%%%%\n%% Adds one.\n%%%%%%%%%%%%\n\\def\\inc{}\n";
        assert_eq!(above(framed, 4).expect("documented").text, "Adds one.");
    }

    /// A key in an indented key list, documented by the comment above it.
    #[test]
    fn an_indented_key_has_its_comment() {
        let source = "\\keyset{\n  % The arrow at the end of a wire.\n  %\n  wire arrow/.style = {->},\n}\n";
        assert_eq!(above(source, 4).expect("documented").text, "The arrow at the end of a wire.");
    }

    /// expl3 code comments in the doc classes' markup.
    #[test]
    fn doc_markup_in_code_comments_is_rendered() {
        let source = "% \\begin{macro}{\\mod_set:n}\n%   Sets \\meta{value} through \\cs{mod_put:n}.\n%    \\begin{macrocode}\n\\cs_new:Npn \\mod_set:n #1 {}\n";
        let doc = above(source, 4).expect("documented");
        assert_eq!(doc.text, "Sets ⟨value⟩ through `\\mod_put:n`.");
        assert!(doc.example.is_empty());
    }
}
