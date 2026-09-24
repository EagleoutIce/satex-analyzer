//! Mutation testing: find blind spots in the analysis.

use satex::config::Config;
use satex::machine::{Analysis, Machine};

const CORPUS: [&str; 5] = [
    r"\NeedsTeXFormat{LaTeX2e}
\ProvidesPackage{sample}[2026/01/01 v1.0 sample]
\makeatletter
\newif\ifsample@draft
\DeclareOption{draft}{\sample@drafttrue}
\DeclareOption*{\PackageWarning{sample}{unknown \CurrentOption}}
\ProcessOptions\relax
\newcounter{item}\newlength{\indent@}\setlength{\indent@}{1.5em}
\newcommand{\parcel}[2][plain]{\fbox{#1:#2}}
\newenvironment{frame}[1]{\begin{center}#1}{\end{center}}
\ifsample@draft\def\mode{draft}\else\def\mode{final}\fi",
    r"\documentclass{article}
\begin{document}
\section{One}\label{a}
See \ref{a} and \ref{b}.
\begin{itemize}\item x\end{itemize}
\end{document}",
    r"\def\a#1,#2.{(#1|#2)}\a x,y.
\def\rec#1{#1\rec{#1}}\rec{z}
\let\alias\rec
\expandafter\def\csname dynamic\endcsname{d}",
    r"\ExplSyntaxOn
\tl_new:N \l_sample_tl
\bool_new:N \l_sample_bool
\bool_set_true:N \l_sample_bool
\cs_new_protected:Npn \sample_do:nn #1#2 { #1 #2 }
\cs_generate_variant:Nn \sample_do:nn { Vn , cn }
\ExplSyntaxOff",
    r"\makeatletter
\@ifundefined{foo}{\def\foo{1}}{\def\foo{2}}
\ifnum\value{page}>1\def\late{y}\else\def\late{n}\fi
\catcode`\!=11 \def\bang!{x}
\makeatother",
];

fn config() -> Config {
    Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    }
}

/// The corpus is LaTeX, so it runs on the (cached) kernel: `\\newif`,
/// `\\makeatletter` and expl3 are the kernel's own code.
fn analyze(source: &str) -> Analysis {
    let cfg = Config { load_format: true, use_kpsewhich: true, ..config() };
    Machine::analyze(source, None, &cfg)
}

/// xorshift64* PRNG (seed can be replayed).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const NOISE: [char; 12] = ['\\', '{', '}', '#', '$', '&', '%', '^', '_', '~', '[', ']'];

fn mutate(source: &str, rng: &mut Rng) -> String {
    let mut chars: Vec<char> = source.chars().collect();
    if chars.is_empty() {
        return source.to_string();
    }
    let at = rng.below(chars.len());
    match rng.below(7) {
        0 => {
            chars.remove(at);
        }
        1 => chars.insert(at, *rng.pick(&NOISE)),
        2 => {
            let other = rng.below(chars.len());
            chars.swap(at, other);
        }
        3 => chars.truncate(at),
        4 => {
            let c = chars[at];
            chars.insert(at, c);
        }
        5 => {
            let text: String = chars.iter().collect();
            let mut lines: Vec<&str> = text.lines().collect();
            if !lines.is_empty() {
                let line = rng.below(lines.len());
                lines.remove(line);
            }
            return lines.join("\n");
        }
        _ => {
            let text: String = chars.iter().collect();
            let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
            if !lines.is_empty() {
                let line = rng.below(lines.len());
                lines.insert(line, lines[line].clone());
            }
            return lines.join("\n");
        }
    }
    chars.into_iter().collect()
}

#[test]
fn mutants_never_run_away() {
    let cfg = config();
    let mut rng = Rng(0x5A7E_C0DE_1234_5678);
    for seed in 0..4000 {
        let source = mutate(CORPUS[seed % CORPUS.len()], &mut rng);
        let analysis = Machine::analyze(&source, None, &cfg);
        assert!(
            analysis.steps <= cfg.limits.steps,
            "seed {seed} ran past its budget: {}",
            analysis.stats(std::time::Duration::ZERO)
        );
        assert!(analysis.graph.len() <= cfg.limits.vertices, "seed {seed} grew the graph past its ceiling");
        let facts = &analysis.facts;
        assert!(
            facts.defs.len() + facts.expansions.len() + facts.occurrences.len() <= cfg.limits.facts,
            "seed {seed} grew the fact tables past their ceiling"
        );
    }
}

fn fingerprint(analysis: &Analysis) -> Vec<String> {
    let mut out: Vec<String> = analysis
        .facts
        .defs
        .iter()
        .map(|d| {
            format!(
                "def {} {} {} {} {}",
                analysis.interner.cs(d.name),
                d.tag,
                d.arity(),
                d.mac.as_ref().map_or(String::new(), |m| {
                    let spec = m.arg_spec.as_ref().map_or("", |s| s.raw.as_str());
                    format!("{}{spec}", m.parameter_text.render(&analysis.interner))
                }),
                d.certain
            ) + " = "
                + &d.mac.as_ref().map_or(String::new(), |m| {
                    satex::tex::detokenize(&m.replacement_text, &analysis.interner)
                })
        })
        .collect();
    out.extend(
        analysis
            .facts
            .occurrences
            .iter()
            .map(|o| format!("occ {} {}", o.kind.as_str(), o.key)),
    );
    out.extend(
        analysis
            .facts
            .loads
            .iter()
            .map(|l| format!("load {} {}", l.kind.as_str(), l.name)),
    );
    out.sort();
    out
}

const SEMANTIC_MUTANTS: [(&str, &str, &str); 12] = [
    // `\box` is already `latex.ltx`'s own primitive (tex.web): `\newcommand`
    // there always fails with "already defined", both before and after a
    // mutation that only changes what the failed call would have done, so
    // a free name is used instead.
    ("rename a definition", r"\newcommand{\parcel}", r"\newcommand{\crate}"),
    ("change an arity", r"\newcommand{\parcel}[2]", r"\newcommand{\parcel}[3]"),
    ("change an optional default", r"[plain]", r"[fancy]"),
    ("drop a definition", r"\newcounter{item}", r""),
    ("flip a switch", r"\sample@drafttrue", r"\sample@draftfalse"),
    ("invert a conditional", r"\ifsample@draft", r"\ifsample@wide"),
    ("remove an option", r"\DeclareOption{draft}", r"\DeclareOption{final}"),
    ("break a label", r"\label{a}", r"\label{c}"),
    ("drop a reference", r"\ref{a}", r"\ref{z}"),
    ("change a delimiter", r"\def\a#1,#2.", r"\def\a#1;#2."),
    ("rename an expl3 function", r"\sample_do:nn", r"\sample_run:nn"),
    ("change an environment name", r"\newenvironment{frame}", r"\newenvironment{slide}"),
];

#[test]
fn semantic_mutants_are_detected() {
    let mut applied = 0;
    for (what, from, to) in SEMANTIC_MUTANTS {
        for original in CORPUS {
            if !original.contains(from) {
                continue;
            }
            applied += 1;
            let mutant = original.replacen(from, to, 1);
            let before = fingerprint(&analyze(original));
            let after = fingerprint(&analyze(&mutant));
            assert_ne!(before, after, "mutant survived: {what}");
        }
    }
    assert_eq!(applied, SEMANTIC_MUTANTS.len(), "every mutant should apply to exactly one corpus entry");
}

#[test]
fn analysis_is_deterministic() {
    for source in CORPUS {
        assert_eq!(fingerprint(&analyze(source)), fingerprint(&analyze(source)));
    }
}

#[test]
fn truncation_at_every_position_is_safe() {
    for source in CORPUS {
        let chars: Vec<char> = source.chars().collect();
        for cut in (0..chars.len()).step_by(7) {
            let prefix: String = chars[..cut].iter().collect();
            let analysis = analyze(&prefix);
            assert!(analysis.steps <= config().limits.steps, "truncation at {cut} ran away");
        }
    }
}
