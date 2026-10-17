//! Soundness oracle: `pdftex -ini -etex` runs, one per `\pdfsetrandomseed`,
//! write which names are defined and every `\count`/`\dimen`; SaTeX's end
//! state must allow each of them.

use std::path::Path;
use std::process::Command;

use crate::builtins::CodeTable::{Delimiter, Lower, Math, Space, Upper};
use crate::config::Config;
use crate::machine::{Analysis, Machine};
use std::collections::BTreeSet;

use crate::tex::{Meaning, TextClass, Tok, Token};
use crate::value::Value;

/// INITEX alone: no format, files or installation.
pub fn config() -> Config {
    Config {
        load: crate::config::Load {
            packages: false,
            classes: false,
            inputs: false,
            format: false,
            ..Default::default()
        },
        ..Config::default()
    }
}

pub const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 ";

const REGISTERS: i64 = 256;

#[derive(Debug, Default, Clone)]
pub struct Run {
    pub seed: u32,
    pub defined: Vec<(String, bool)>,
    /// Parameterless macros and their bodies, as `\meaning` prints them.
    pub bodies: Vec<(String, String)>,
    /// `("count", 3, 7)`; dimensions in sp.
    pub registers: Vec<(&'static str, i64, i64)>,
    /// `("lccode", 65, 97)`: every code table's entries 0..255.
    pub codes: Vec<(&'static str, i64, i64)>,
    pub errors: usize,
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub seed: u32,
    pub what: String,
}

/// The control words `source` spells.
pub fn names(source: &str) -> Vec<String> {
    let mut out: Vec<String> = regex::Regex::new(r"\\([A-Za-z]+)")
        .map(|re| re.captures_iter(source).map(|c| c[1].to_string()).collect())
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

/// The code tables the oracle reads back, as their primitives name them.
const TABLES: [&str; 6] = ["catcode", "lccode", "uccode", "sfcode", "mathcode", "delcode"];

/// Log lines `D:name=0|1`, `C:n=v`, `X:n=v`, `K:table.n=v`.
fn observe(names: &[String]) -> String {
    let mut out =
        String::from("\\catcode`\\{=1 \\catcode`\\}=2 \\escapechar=92 \\def\\satexW#1{\\immediate\\write-1{#1}}");
    for name in names {
        out.push_str(&format!(
            "\\satexW{{D:{name}=\\ifdefined\\{name} 1\\else 0\\fi}}\\satexW{{M:{name}=\\meaning\\{name}}}"
        ));
    }
    for n in 0..REGISTERS {
        out.push_str(&format!("\\satexW{{C:{n}=\\the\\count{n}}}\\satexW{{X:{n}=\\number\\dimen{n}}}"));
    }
    for table in TABLES {
        for n in 0..256 {
            out.push_str(&format!("\\satexW{{K:{table}.{n}=\\the\\{table}{n}}}"));
        }
        // One line per table, within TeX's buffer.
        out.push('\n');
    }
    out
}

pub fn concrete(source: &str, seed: u32, names: &[String], dir: &Path) -> Option<Run> {
    std::fs::create_dir_all(dir).ok()?;
    let text = format!("{PRELUDE}\\pdfsetrandomseed {seed} {source}\n{}\\end\n", observe(names));
    std::fs::write(dir.join("p.tex"), text).ok()?;
    let _ = std::fs::remove_file(dir.join("p.log"));
    Command::new(crate::paths::which("pdftex").ok()?)
        .args(["-ini", "-etex", "-interaction=batchmode", "-halt-on-error=false", "p.tex"])
        .current_dir(dir)
        .env("max_print_line", "1000000")
        .output()
        .ok()?;
    let log = std::fs::read_to_string(dir.join("p.log")).ok()?;
    let mut run = Run { seed, ..Run::default() };
    run.errors = log.lines().filter(|l| l.starts_with("! ")).count();
    for line in log.lines() {
        let Some((tag, rest)) = line.split_once(':') else { continue };
        let Some((key, value)) = rest.split_once('=') else { continue };
        match tag {
            "D" => run.defined.push((key.to_string(), value.trim() == "1")),
            "M" => {
                let meaning = value.trim_start_matches(|c| c != 'm').strip_prefix("macro:->");
                if let Some(body) = meaning.filter(|_| !value.contains("\\outer")) {
                    run.bodies.push((key.to_string(), body.to_string()));
                }
            }
            "C" | "X" => {
                let (Ok(n), Ok(v)) = (key.parse(), value.trim().parse()) else { continue };
                run.registers.push((if tag == "C" { "count" } else { "dimen" }, n, v));
            }
            "K" => {
                let Some((table, n)) = key.split_once('.') else { continue };
                let Some(table) = TABLES.into_iter().find(|t| *t == table) else { continue };
                let (Ok(n), Ok(v)) = (n.parse(), value.trim().parse()) else { continue };
                run.codes.push((table, n, v));
            }
            _ => {}
        }
    }
    // A run that stopped early observed nothing.
    (!run.defined.is_empty() || names.is_empty()).then_some(run)
}

fn allows(value: &Value, n: i64) -> bool {
    match value {
        Value::Int(v) | Value::Dimen(v) => *v == n,
        Value::Num { num, .. } => (num.lo..=num.hi).contains(&n),
        Value::OneOf(values) => values.iter().any(|v| allows(v, n)),
        Value::Unknown => true,
        _ => false,
    }
}

fn allows_defined(analysis: &Analysis, name: &str, want: bool) -> bool {
    let Some(sym) = analysis.interner.lookup(name) else { return !want };
    let Some(binding) = analysis.env.slot(sym) else { return !want };
    let undefined = |m: &Meaning| matches!(m, Meaning::Undefined);
    match &binding.meaning {
        Meaning::Unknown => match &binding.may {
            Some(may) => may.iter().any(|m| undefined(m) != want),
            None => true,
        },
        m if want => !undefined(m) || !binding.certain,
        m => undefined(m) || !binding.certain,
    }
}

/// Whether the end state allows `name` to be a parameterless macro with
/// `body`: one of its meanings is, or is not known exactly.
fn allows_body(analysis: &Analysis, name: &str, body: &str) -> bool {
    let Some(binding) = analysis.interner.lookup(name).and_then(|sym| analysis.env.slot(sym)) else { return true };
    let members = binding.may.as_deref().unwrap_or(std::slice::from_ref(&binding.meaning));
    members.iter().any(|m| match m {
        Meaning::Unknown => true,
        Meaning::Macro(d) => !d.parameter_text.items.is_empty() || covers_text(&d.replacement_text, analysis, body),
        _ => false,
    })
}

/// Whether `text` may print as `body`, spaces and the `##` a `\\write`
/// doubles aside: a token of unknown text matches only what it stands
/// for (an interval, a class, its members).
pub fn covers_text(text: &[Token], analysis: &Analysis, body: &str) -> bool {
    let want: Vec<char> = squeeze(body).chars().collect();
    ends(text, analysis, &want, BTreeSet::from([0]), 0).contains(&want.len())
}

fn squeeze(text: &str) -> String {
    text.replace("##", "#").split_whitespace().collect()
}

/// Where `text` may end when it starts at any of `from`. A run of known
/// tokens is read as one string, as the engine's answer is.
fn ends(text: &[Token], a: &Analysis, want: &[char], mut at: BTreeSet<usize>, depth: u8) -> BTreeSet<usize> {
    let known = |t: &Token| match t.tok {
        Tok::Chr(c, _) => !crate::tex::is_unknown_char(c),
        Tok::Cs(sym) => !matches!(a.interner.name(sym).chars().next(), Some('\u{1}'..='\u{8}')),
        _ => true,
    };
    let mut rest = text;
    while !rest.is_empty() && !at.is_empty() {
        let run = rest.iter().take_while(|t| known(t)).count();
        if run > 0 {
            let printed: Vec<char> = squeeze(&crate::tex::detokenize(&rest[..run], &a.interner)).chars().collect();
            at = at.into_iter().filter(|p| want[*p..].starts_with(&printed)).map(|p| p + printed.len()).collect();
            rest = &rest[run..];
        } else {
            at = at.into_iter().flat_map(|p| token_ends(rest[0], a, want, p, depth)).collect();
            rest = &rest[1..];
        }
    }
    at
}

fn token_ends(t: Token, a: &Analysis, want: &[char], at: usize, depth: u8) -> BTreeSet<usize> {
    use crate::machine::{LEAD_MARK, MORE_DIGITS, REST_SERIAL, TYPED_TEXT, UNKNOWN_NAME};
    let any = || (at..=want.len()).collect();
    let rest = &want[at..];
    if let Tok::Chr(c, _) = t.tok
        && crate::tex::is_unknown_char(c)
    {
        let ok = |x: char| match crate::tex::unknown_number_chars(c) {
            Some(chars) => chars.contains(x),
            None => true,
        };
        let mut out: BTreeSet<usize> = rest.first().filter(|x| ok(**x)).map(|_| at + 1).into_iter().collect();
        if crate::tex::unknown_number_chars(c).is_none() {
            out.insert(at);
        }
        return out;
    }
    let Some(sym) = t.cs() else { return any() };
    let name = a.interner.name(sym);
    let len = name.rsplit_once(crate::env::LEN_MARK).and_then(|(_, n)| n.parse::<usize>().ok()).unwrap_or(usize::MAX);
    let digits = |min: usize, sign: bool, lead: bool| -> BTreeSet<usize> {
        let signed = sign && rest.first() == Some(&'-');
        let start = usize::from(signed);
        let run = rest[start..].iter().take_while(|c| c.is_ascii_digit()).count();
        (min..=run)
            .filter(|n| start + n <= len && !(lead && *n > 1 && rest[start] == '0'))
            .map(|n| at + start + n)
            .collect()
    };
    if let Some(typed) = name.strip_prefix(crate::scan::TYPED_DIGITS) {
        let Some((range, tail)) = typed.split_once(['#', '@']) else {
            return digits(1, true, name.contains(LEAD_MARK));
        };
        let parsed = range.split_once("..").and_then(|(lo, hi)| Some((lo.parse().ok()?, hi.parse().ok()?)));
        let Some((lo, hi)) = a.env.slot(sym).and_then(|b| b.value.num()).map(|(_, n)| (n.lo, n.hi)).or(parsed) else {
            return any();
        };
        // A slice is places `[i, j)` of a `k`-digit spelling.
        let places: Vec<usize> = tail.split(':').skip(1).filter_map(|x| x.parse().ok()).collect();
        let width = typed.contains('@').then(|| places.get(2).zip(places.get(1)).map(|(j, i)| j - i)).flatten();
        let first = typed.contains('@') && places.get(1) == Some(&0) && places.first().is_some_and(|k| *k > 1);
        return digits(1, width.is_none(), width.is_none())
            .into_iter()
            .filter(|end| {
                let spelled: String = want[at..*end].iter().collect();
                let fits = spelled.parse::<i64>().is_ok_and(|v| (lo..=hi).contains(&v));
                fits && width.is_none_or(|w| end - at == w) && !(first && spelled.starts_with('0'))
            })
            .collect();
    }
    if name.starts_with(MORE_DIGITS) {
        return digits(0, false, false);
    }
    let Some(unknown) = name.strip_prefix(UNKNOWN_NAME) else { return any() };
    if unknown.starts_with(crate::machine::explore::ONE_OF) {
        let members = a.env.slot(sym).and_then(|b| b.may.as_ref());
        let Some(members) = members.filter(|_| depth < 8) else { return any() };
        let texts = members.iter().filter_map(|m| m.as_macro().map(|m| m.replacement_text.clone()));
        let mut out = BTreeSet::new();
        for text in texts {
            out.extend(ends(&text, a, want, BTreeSet::from([at]), depth + 1));
        }
        return out;
    }
    if unknown.starts_with(TYPED_TEXT) {
        let Some((class, one, cs)) = crate::env::Typed::parse_name(name) else { return any() };
        if class.meets(TextClass::CS) {
            return any();
        }
        let cs: Vec<Vec<char>> = cs.iter().map(|n| format!("\\{n}").chars().collect()).collect();
        let char_ok = |c: char| {
            (class.meets(TextClass::DIGIT) && c.is_ascii_digit())
                || (class.meets(TextClass::SIGN) && "+-.,".contains(c))
                || (class.meets(TextClass::BRACE) && "{}".contains(c))
                || class.meets(TextClass(TextClass::LETTER.0 | TextClass::OTHER.0))
        };
        // Over (position, tokens read); a space prints as nothing here.
        let mut out = BTreeSet::new();
        let mut front = vec![(at, 0usize)];
        let mut seen = BTreeSet::new();
        while let Some((p, n)) = front.pop() {
            if !seen.insert((p, n)) || n > len {
                continue;
            }
            if n > 0 || !one || class.meets(TextClass::SPACE) {
                out.insert(p);
            }
            if p < want.len() && char_ok(want[p]) {
                front.push((p + 1, n + 1));
            }
            for c in cs.iter().filter(|c| want[p..].starts_with(c)) {
                front.push((p + c.len(), n + 1));
            }
        }
        return out;
    }
    // A name built around gaps: its known parts in order.
    let spelled = unknown.split(REST_SERIAL).next().unwrap_or(unknown);
    let mut at_set = BTreeSet::from([at]);
    let mut parts = spelled.split([crate::machine::UNKNOWN_NAME_GAP, crate::machine::UNKNOWN_DIGITS_GAP]);
    let lit = |at_set: BTreeSet<usize>, s: &str| -> BTreeSet<usize> {
        let s: Vec<char> = s.chars().collect();
        at_set.into_iter().filter(|p| want[*p..].starts_with(&s)).map(|p| p + s.len()).collect()
    };
    at_set = lit(at_set, &format!("\\{}", parts.next().unwrap_or("")));
    for part in parts {
        at_set = at_set.into_iter().flat_map(|p| p..=want.len()).collect();
        at_set = lit(at_set, part);
    }
    at_set
}

fn register(analysis: &Analysis, kind: &str, n: i64) -> Value {
    let name = format!("{}{kind}{n}", crate::machine::REGISTER_PREFIX);
    analysis.interner.lookup(&name).and_then(|sym| analysis.env.slot(sym)).map_or(Value::Int(0), |b| b.value.clone())
}

/// What entry `n` of `table` may hold at the end: its category, or its
/// code joined with what an unpinned write that may have named it stored.
fn code(analysis: &Analysis, table: &str, n: i64) -> Value {
    let Some(c) = u32::try_from(n).ok().and_then(char::from_u32) else { return Value::Unknown };
    if table == "catcode" {
        let cats = analysis.catcodes.cats(c);
        return Value::OneOf((0..16).filter(|k| cats & 1 << k != 0).map(Value::Int).collect());
    }
    let Some(kind) = [Lower, Upper, Space, Math, Delimiter].into_iter().find(|t| t.as_str() == table) else {
        return Value::Unknown;
    };
    let slot = |name: String| analysis.interner.lookup(&name).and_then(|sym| analysis.env.slot(sym));
    let code = crate::exec::CODE;
    let own = slot(format!("{code}{table}{c}")).map_or(Value::Int(kind.default_for(c)), |b| b.value.clone());
    let any = format!("{code}{table}{code}any");
    match slot(any.clone()).map(|b| b.value.num()) {
        None => own,
        Some(Some((_, num))) if !(num.lo..=num.hi).contains(&n) => own,
        _ => slot(format!("{any}=")).map_or(Value::Unknown, |b| own.join(&b.value, analysis.env.sets)),
    }
}

/// What the runs found: violations (unsound), facts every run agrees on
/// that SaTeX leaves open (imprecise), and the TeX errors raised.
#[derive(Debug, Default)]
pub struct Report {
    pub violations: Vec<Violation>,
    pub imprecise: Vec<String>,
    pub errors: usize,
}

pub fn check(source: &str, runs: &[Run], cfg: &Config) -> Report {
    let analysis = Machine::analyze(&format!("{PRELUDE}{source}"), None, cfg);
    let mut out = Vec::new();
    for run in runs {
        for (name, defined) in &run.defined {
            if !allows_defined(&analysis, name, *defined) {
                let what = if *defined { "defined" } else { "undefined" };
                out.push(Violation { seed: run.seed, what: format!("\\{name} {what} in TeX, not in SaTeX") });
            }
        }
        for (name, body) in &run.bodies {
            if !allows_body(&analysis, name, body) {
                out.push(Violation { seed: run.seed, what: format!("\\{name} is `{body}` in TeX, not in SaTeX") });
            }
        }
        for &(kind, n, v) in &run.registers {
            let value = register(&analysis, kind, n);
            if !allows(&value, v) {
                out.push(Violation { seed: run.seed, what: format!("\\{kind}{n}={v} in TeX, SaTeX has {value:?}") });
            }
        }
        for &(table, n, v) in &run.codes {
            let value = code(&analysis, table, n);
            if !allows(&value, v) {
                out.push(Violation { seed: run.seed, what: format!("\\{table}{n}={v} in TeX, SaTeX has {value:?}") });
            }
        }
    }
    out.sort_by(|a, b| a.what.cmp(&b.what).then(a.seed.cmp(&b.seed)));
    out.dedup_by(|a, b| a.what == b.what);
    let mut imprecise = Vec::new();
    if let Some(first) = runs.first().filter(|_| runs.len() > 1) {
        for (i, &(kind, n, v)) in first.registers.iter().enumerate() {
            let agreed = runs.iter().all(|r| r.registers.get(i).is_some_and(|x| x.2 == v));
            let value = register(&analysis, kind, n);
            if agreed && !matches!(value, Value::Int(x) | Value::Dimen(x) if x == v) {
                imprecise.push(format!("\\{kind}{n}={v}, SaTeX has {value:?}"));
            }
        }
    }
    Report { violations: out, imprecise, errors: runs.iter().map(|r| r.errors).sum() }
}

/// [`check`] over runs with `seeds`; `None` without pdftex.
pub fn verify(source: &str, seeds: &[u32], cfg: &Config, dir: &Path) -> Option<Report> {
    let names = names(source);
    let runs: Vec<Run> = seeds.iter().filter_map(|&s| concrete(source, s, &names, &dir.join(s.to_string()))).collect();
    (!runs.is_empty()).then(|| check(source, &runs, cfg))
}

/// xorshift: a seed names a program.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

/// A terminating program of the hard constructs (unknown loop bounds and
/// tests, `\csname` families, groups, `\global`, `\edef` growth); markers
/// `\mA`… are defined where control reaches.
pub fn generate(seed: u64, statements: usize) -> String {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut out = String::from(
        "\\countdef\\i=200 \\countdef\\j=201 \\countdef\\n=202 \\def\\x{}\\def\\vA{a}\\def\\vB{b}\\let\\vC\\vA\\def\\gA{\\advance\\count4 1 }",
    );
    let mut marker = 0;
    let typeset = rng.below(2) == 0;
    if typeset {
        out.push_str("\\font\\tenrm=cmr10 \\tenrm ");
    }
    for k in 0..statements {
        out.push_str(&match typeset && rng.below(2) == 0 {
            true => boxed(&mut rng),
            false => statement(&mut rng, k, 0, &mut marker),
        });
    }
    // Box metrics into `\dimen230`…, where the observations read them.
    for b in 0..4 {
        out.push_str(&format!(
            "\\dimen{}=\\wd{b} \\dimen{}=\\ht{b} \\dimen{}=\\dp{b} ",
            230 + 3 * b,
            231 + 3 * b,
            232 + 3 * b
        ));
    }
    out
}

/// A box of typeset material: characters, kerns, glue, rules, accents,
/// discretionaries, shifted and nested boxes, some of unknown size.
fn boxed(rng: &mut Rng) -> String {
    let size = |rng: &mut Rng| match rng.below(3) {
        0 => format!("{}pt", rng.below(9)),
        1 => format!("\\pdfuniformdeviate {}sp", 1 + rng.below(400_000)),
        _ => format!("\\count{}pt", 1 + rng.below(4)),
    };
    let item = |rng: &mut Rng| -> String {
        match rng.below(10) {
            0 => ["ab", "fi", "AV", "x."][rng.below(4) as usize].to_string(),
            1 => format!("\\kern{} ", size(rng)),
            2 => format!("\\hskip{} plus 1fil ", size(rng)),
            3 => format!("\\vrule width{} height{} ", size(rng), size(rng)),
            4 => format!("\\char{} ", 65 + rng.below(26)),
            5 => "\\accent127 a".to_string(),
            6 => "\\discretionary{a-}{b}{ab}".to_string(),
            7 => format!("\\raise{}\\hbox{{x}}", size(rng)),
            8 => format!("\\unhcopy{} ", rng.below(4)),
            _ => format!("\\hbox to{}{{a\\hfil b}}", size(rng)),
        }
    };
    let b = rng.below(4);
    let body: String = (0..1 + rng.below(4)).map(|_| item(rng)).collect();
    match rng.below(4) {
        0 => format!("\\setbox{b}\\vbox{{\\hbox{{{body}}}\\vskip{}\\hbox{{y}}}}", size(rng)),
        1 => format!("\\setbox{b}\\hbox to{}{{{body}}}", size(rng)),
        _ => format!("\\setbox{b}\\hbox{{{body}}}"),
    }
}

fn statement(rng: &mut Rng, k: usize, depth: usize, marker: &mut usize) -> String {
    let mut mark = || {
        *marker += 1;
        format!("\\def\\m{}{{}}", letters(*marker))
    };
    let leaf = depth >= 2;
    let reg = |rng: &mut Rng| 1 + rng.below(4);
    let var = |rng: &mut Rng| ["A", "B", "C"][rng.below(3) as usize];
    match rng.below(if leaf { 13 } else { 17 }) {
        0 => format!("\\advance\\count{} by {} ", reg(rng), rng.below(7) as i64 - 3),
        1 => format!("\\count{}=\\count{} ", reg(rng), reg(rng)),
        2 => format!("\\edef\\x{{\\x {}}}", ["a", "1", "\\the\\i", "\\the\\count1"][rng.below(4) as usize]),
        3 => format!("\\expandafter\\def\\csname h{}\\endcsname{{\\advance\\count3 1 }}", rng.below(3)),
        4 => format!("\\ifcsname h\\the\\count{}\\endcsname {}\\fi ", 1 + rng.below(2), mark()),
        5 => format!("{}{}", if rng.below(2) == 0 { "\\global" } else { "" }, mark()),
        6 => format!("\\ifnum\\count{}>{} {}\\fi ", reg(rng), rng.below(5), mark()),
        7 => format!("\\let\\v{}\\v{} ", var(rng), var(rng)),
        8 => format!("\\ifx\\v{}\\v{} {}\\fi ", var(rng), var(rng), mark()),
        9 => format!("\\count{}=\\numexpr\\count{}*{}-{}\\relax ", reg(rng), reg(rng), rng.below(4), rng.below(3)),
        10 => format!("\\dimen{}=\\count{}pt \\advance\\dimen1 by {}.5pt ", reg(rng), reg(rng), rng.below(3)),
        11 => format!("\\ifdim\\dimen{}>{}pt {}\\fi ", reg(rng), rng.below(3), mark()),
        12 => format!("\\global\\advance\\count{} by {} ", reg(rng), rng.below(3)),
        13 => {
            let (a, b) = (statement(rng, k, depth + 1, marker), statement(rng, k, depth + 1, marker));
            format!("\\ifnum\\pdfuniformdeviate 2=0 {a}\\else {b}\\fi ")
        }
        14 => {
            let body = statement(rng, k, depth + 1, marker);
            let (open, close) = [("\\begingroup ", "\\endgroup "), ("{", "}")][rng.below(2) as usize];
            let after = ["", "\\aftergroup\\relax ", "\\aftergroup\\gA "][rng.below(3) as usize];
            format!("{open}{after}{body}{close}")
        }
        15 => {
            let body = statement(rng, k, depth + 1, marker);
            let name = format!("lp{}", letters(k + 1));
            format!(
                "\\i=0 \\n=\\pdfuniformdeviate {} \\def\\{name}{{\\ifnum\\i<\\n \\advance\\i1 {body}\\csname h\\the\\i\\endcsname\\expandafter\\{name}\\fi}}\\{name} ",
                2 + rng.below(6)
            )
        }
        _ => {
            let body = statement(rng, k, depth + 1, marker);
            format!("\\ifodd\\count{} {body}\\fi ", reg(rng))
        }
    }
}

/// `1` → `A`, `27` → `AA`.
fn letters(mut n: usize) -> String {
    let mut out = String::new();
    while n > 0 {
        n -= 1;
        out.insert(0, (b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    out
}

/// Control words, balanced groups and runs of other text.
fn units(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        match chars[i] {
            '\\' => {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if i == start + 1 && i < chars.len() {
                    i += 1;
                }
            }
            '{' => {
                let mut depth = 0;
                while i < chars.len() {
                    match chars[i] {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        '\\' => i += 1,
                        _ => {}
                    }
                    i += 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            _ => {
                while i < chars.len() && !matches!(chars[i], '\\' | '{') {
                    i += 1;
                }
            }
        }
        out.push(chars[start..i.min(chars.len())].iter().collect());
    }
    out
}

/// ddmin over [`units`], then inside groups, keeping violation `what` and
/// adding no TeX error.
pub fn reduce(source: &str, what: &str, seeds: &[u32], cfg: &Config, dir: &Path) -> String {
    let errors = verify(source, seeds, cfg, dir).map_or(0, |r| r.errors);
    let fails = |text: &str| {
        verify(text, seeds, cfg, dir).is_some_and(|r| r.errors <= errors && r.violations.iter().any(|v| v.what == what))
    };
    let mut current = units(source);
    let mut chunk = current.len() / 2;
    while chunk >= 1 {
        let mut i = 0;
        let mut removed = false;
        while i < current.len() {
            let mut trial = current.clone();
            trial.drain(i..(i + chunk).min(trial.len()));
            if fails(&trial.concat()) {
                current = trial;
                removed = true;
            } else {
                i += chunk;
            }
        }
        if !removed {
            chunk /= 2;
        }
    }
    for k in 0..current.len() {
        let unit = current[k].clone();
        if !(unit.starts_with('{') && unit.ends_with('}') && unit.len() > 2) {
            continue;
        }
        let inner = &unit[1..unit.len() - 1];
        let mut parts = units(inner);
        let mut i = 0;
        while i < parts.len() {
            let mut trial = parts.clone();
            trial.remove(i);
            let mut whole = current.clone();
            whole[k] = format!("{{{}}}", trial.concat());
            if fails(&whole.concat()) {
                parts = trial;
                current = whole;
            } else {
                i += 1;
            }
        }
    }
    current.concat()
}
