//! What the document hands the PDF writer, read the way a PDF reader reads
//! it: content-stream operators from literals, names from objects (PDF 32000,
//! § 7.2 lexical conventions, § 8.4.2 graphics state operators, § 14.6
//! marked content, § 8.11 optional content).

/// What a payload is, by the primitive that carried it and, for a
/// `\special`, by the `pdf:` command it starts with (dvipdfmx manual, "PDF
/// specials").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Payload<'a> {
    /// Operators placed in the page's content stream.
    Content(&'a str),
    /// An object, a dictionary entry or page resources.
    Object(&'a str),
    Other,
}

pub fn classify<'a>(detail: &str, text: &'a str) -> Payload<'a> {
    match detail {
        "literal" => Payload::Content(text),
        "object" | "resources" => Payload::Object(text),
        "special" => {
            let Some(command) = text.trim_start().strip_prefix("pdf:") else { return Payload::Other };
            let command = command.trim_start();
            let keyword_len = command.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(command.len());
            let (keyword, rest) = command.split_at(keyword_len);
            match keyword {
                "content" | "literal" | "code" => {
                    let rest = rest.trim_start();
                    let rest = rest.strip_prefix("direct").unwrap_or(rest);
                    Payload::Content(rest)
                }
                "obj" | "put" | "stream" | "pageresources" => Payload::Object(rest),
                _ => Payload::Other,
            }
        }
        _ => Payload::Other,
    }
}

/// An operator that opens or closes a nesting in a content stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operator {
    /// `BMC`, or `BDC` with the property it names: `/OC /name BDC` refers to
    /// an optional content group through the page's `/Properties`.
    BeginMarked { operator: &'static str, optional_content: Option<String> },
    /// `EMC`.
    EndMarked,
    /// `q`.
    Save,
    /// `Q`.
    Restore,
}

impl Operator {
    pub fn name(&self) -> &'static str {
        match self {
            Operator::BeginMarked { operator, .. } => operator,
            Operator::EndMarked => "EMC",
            Operator::Save => "q",
            Operator::Restore => "Q",
        }
    }

    pub fn opens(&self) -> bool {
        matches!(self, Operator::BeginMarked { .. } | Operator::Save)
    }

    /// The operator that closes what this one opens.
    pub fn closer(&self) -> &'static str {
        match self {
            Operator::BeginMarked { .. } | Operator::EndMarked => "EMC",
            Operator::Save | Operator::Restore => "Q",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Lexeme {
    Name(String),
    Word(String),
    Other,
    DictOpen,
    DictClose,
    ArrayOpen,
    ArrayClose,
}

fn is_delimiter(c: char) -> bool {
    c.is_whitespace() || "()<>[]{}/%".contains(c)
}

fn lex(text: &str) -> Vec<Lexeme> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '%' => {
                for c in chars.by_ref() {
                    if c == '\n' || c == '\r' {
                        break;
                    }
                }
            }
            '(' => {
                let mut depth = 1;
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                out.push(Lexeme::Other);
            }
            '<' if chars.peek() == Some(&'<') => {
                chars.next();
                out.push(Lexeme::DictOpen);
            }
            '<' => {
                for c in chars.by_ref() {
                    if c == '>' {
                        break;
                    }
                }
                out.push(Lexeme::Other);
            }
            '>' if chars.peek() == Some(&'>') => {
                chars.next();
                out.push(Lexeme::DictClose);
            }
            '[' => out.push(Lexeme::ArrayOpen),
            ']' => out.push(Lexeme::ArrayClose),
            '/' => {
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if is_delimiter(c) {
                        break;
                    }
                    name.push(c);
                    chars.next();
                }
                out.push(Lexeme::Name(name));
            }
            c if is_delimiter(c) => out.push(Lexeme::Other),
            c => {
                let mut word = c.to_string();
                while let Some(&c) = chars.peek() {
                    if is_delimiter(c) {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                out.push(Lexeme::Word(word));
            }
        }
    }
    out
}

/// The nesting operators of a content stream, in order.  An operator stands
/// after its operands; a word inside a dictionary or array is an operand.
pub fn operators(content: &str) -> Vec<Operator> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut operands: Vec<Lexeme> = Vec::new();
    for lexeme in lex(content) {
        match &lexeme {
            Lexeme::DictOpen | Lexeme::ArrayOpen => depth += 1,
            Lexeme::DictClose | Lexeme::ArrayClose => depth = depth.saturating_sub(1),
            Lexeme::Word(word) if depth == 0 && !is_number(word) => {
                let operator = match word.as_str() {
                    "BMC" => Some(Operator::BeginMarked { operator: "BMC", optional_content: None }),
                    "BDC" => {
                        Some(Operator::BeginMarked { operator: "BDC", optional_content: optional_content(&operands) })
                    }
                    "EMC" => Some(Operator::EndMarked),
                    "q" => Some(Operator::Save),
                    "Q" => Some(Operator::Restore),
                    _ => None,
                };
                out.extend(operator);
                operands.clear();
                continue;
            }
            _ => {}
        }
        operands.push(lexeme);
    }
    out
}

fn is_number(word: &str) -> bool {
    word.parse::<f64>().is_ok() || matches!(word, "true" | "false" | "null" | "R")
}

/// `/OC /name BDC`: the tag and the property are the two operands.
fn optional_content(operands: &[Lexeme]) -> Option<String> {
    match operands {
        [.., Lexeme::Name(tag), Lexeme::Name(property)] if tag == "OC" => Some(property.clone()),
        _ => None,
    }
}

/// Every name an object mentions: a `/Properties` entry among them.
pub fn names(object: &str) -> Vec<String> {
    lex(object)
        .into_iter()
        .filter_map(|lexeme| match lexeme {
            Lexeme::Name(name) => Some(name),
            _ => None,
        })
        .collect()
}
