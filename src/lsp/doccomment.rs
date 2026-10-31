//! What a comment block right above a definition documents: the command,
//! and per parameter what it takes (`% #1: the name`).

#[derive(Default, Debug, PartialEq)]
pub struct DocComment {
    pub text: String,
    /// Parameter number and its text, in the order given.
    pub params: Vec<(u8, String)>,
}

/// The comment block that ends on the line above 1-based `line` of
/// `source`, and the comment that ends that line. A `#n` in the text
/// becomes `⟨argn⟩`, the name the usage line gives that argument.
pub fn above(source: &str, line: usize) -> Option<DocComment> {
    let lines: Vec<&str> = source.lines().collect();
    let mut at = line.checked_sub(1)?;
    let mut block = Vec::new();
    if let Some(trailing) = lines.get(at).and_then(|l| trailing_comment(l)) {
        block.push(trailing);
    }
    while at > 0 {
        let Some(comment) = lines.get(at - 1)?.trim().strip_prefix('%') else { break };
        block.push(comment.trim_start_matches('%').trim());
        at -= 1;
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

fn parse(block: &[&str]) -> Option<DocComment> {
    let mut doc = DocComment::default();
    for line in block {
        // Rules, magic comments (`%!TEX`) and empty lines say nothing.
        if line.is_empty() || line.starts_with('!') || line.chars().all(|c| "-=*#~_".contains(c)) {
            continue;
        }
        if let Some(param) = param(line) {
            doc.params.push(param);
        } else if let Some((_, text)) = doc.params.last_mut().filter(|_| line.starts_with(char::is_lowercase)) {
            // A parameter's text, carried on.
            text.push(' ');
            text.push_str(line);
        } else {
            if !doc.text.is_empty() {
                doc.text.push(' ');
            }
            doc.text.push_str(line);
        }
    }
    (!doc.text.is_empty() || !doc.params.is_empty()).then_some(doc)
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
    }
}
