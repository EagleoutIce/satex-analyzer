use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use satex::analysis::machine::Machine;
use satex::engine::tex::{CatcodeTable, Interner, Mouth};
use satex::project::config::{Config, Load, Texmf};
use satex::queries::query::{self, Filter, Query};

fn document(sections: usize) -> String {
    let mut source = String::from(
        r"\documentclass{article}
\newif\ifdraft \drafttrue
\newcommand{\hl}[1]{\textbf{#1}}
\newcommand{\pair}[2][left]{#1--#2}
\newcounter{step}
\newlength{\gap}\setlength{\gap}{1.5em}
\begin{document}
",
    );
    for i in 0..sections {
        source.push_str(&format!(
            r"\section{{Section {i}}}\label{{sec:{i}}}
Text with \hl{{emphasis}} and \pair[right]{{value}}.
\ifdraft \stepcounter{{step}} \else \addtocounter{{step}}{{2}} \fi
\ifvmode \def\maybe{{a}} \else \def\maybe{{b}} \fi
\begin{{itemize}} \item one \item two \end{{itemize}}
See \ref{{sec:{i}}} for details.
"
        ));
    }
    source.push_str("\\end{document}\n");
    source
}

fn package(macros: usize) -> String {
    let mut source = String::from(
        r"\NeedsTeXFormat{LaTeX2e}
\ProvidesPackage{bench}[2026/01/01 v1.0 benchmark]
\makeatletter
\DeclareOption{draft}{\def\bench@mode{draft}}
\DeclareOption*{\PackageWarning{bench}{\CurrentOption}}
\ProcessOptions\relax
",
    );
    for i in 0..macros {
        source.push_str(&format!("\\newcommand{{\\bench@m{i}}}[2][x]{{#1:#2}}\n\\def\\bench@d{i}#1,#2.{{(#1|#2)}}\n"));
    }
    source.push_str("\\makeatother\n");
    source
}

fn local() -> Config {
    Config {
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Texmf::default() },
        ..Config::default()
    }
}

fn tokenizing(c: &mut Criterion) {
    let source = document(200);
    let mut group = c.benchmark_group("mouth");
    group.throughput(Throughput::Bytes(source.len() as u64));
    group.bench_function("tokenize", |b| {
        b.iter(|| {
            let catcodes = CatcodeTable::latex();
            let mut interner = Interner::default();
            let mut mouth = Mouth::new(&source, 0);
            let mut n = 0usize;
            while mouth.next(&catcodes, &mut interner).is_some() {
                n += 1;
            }
            black_box(n)
        })
    });
    group.finish();
}

fn interpreting(c: &mut Criterion) {
    let cfg = local();
    let mut group = c.benchmark_group("analyze");
    group.measurement_time(Duration::from_secs(8));
    for sections in [10usize, 100, 400] {
        let source = document(sections);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::new("document", sections), &source, |b, source| {
            b.iter(|| black_box(Machine::analyze(source, None, &cfg).facts.defs.len()))
        });
    }
    for macros in [50usize, 500] {
        let source = package(macros);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::new("package", macros), &source, |b, source| {
            b.iter(|| black_box(Machine::analyze(source, None, &cfg).facts.defs.len()))
        });
    }
    group.finish();
}

/// Interpreter throughput on self-contained inputs: criterion reports
/// elements per second, here the analysis steps (tokens read), so a
/// tracked run shows steps/s apart from the inputs' sizes.
fn throughput(c: &mut Criterion) {
    let cfg = local();
    let fib = r"\def\loop#1\repeat{\def\body{#1}\iterate}
\def\iterate{\body \let\next\iterate \else\let\next\relax\fi \next}
\countdef\n=200 \n=0
\def\fib#1{\ifnum#1<2 #1\else\the\numexpr\fib{\numexpr#1-1\relax}+\fib{\numexpr#1-2\relax}\relax\fi}
\loop \edef\x{\csname y\the\n\endcsname \fib{10}}\advance\n by 1 \ifnum\n<200 \repeat
";
    let inputs = [("macro_loop", fib.to_string()), ("document", document(400))];
    let mut group = c.benchmark_group("throughput");
    group.measurement_time(Duration::from_secs(8));
    for (name, source) in &inputs {
        let steps = Machine::analyze(source, None, &cfg).steps;
        group.throughput(Throughput::Elements(steps));
        group.bench_with_input(BenchmarkId::new("steps", name), source, |b, source| {
            b.iter(|| black_box(Machine::analyze(source, None, &cfg).steps))
        });
    }
    group.finish();
}

fn querying(c: &mut Criterion) {
    let analysis = Machine::analyze(&document(200), None, &local());
    let filter = Filter::parse(r#"tag=macro and name~"^..hl""#).expect("valid filter");
    let mut group = c.benchmark_group("query");
    group.bench_function("definitions", |b| {
        b.iter(|| black_box(query::run(&analysis, Query::Definitions, &Filter::Always).len()))
    });
    group
        .bench_function("filtered", |b| b.iter(|| black_box(query::run(&analysis, Query::Definitions, &filter).len())));
    group.bench_function("lint", |b| b.iter(|| black_box(satex::linter::lint(&analysis).len())));
    group.bench_function("slice", |b| {
        b.iter(|| black_box(query::slice(&analysis, &["hl".to_string()], None, query::Direction::Backward).len()))
    });
    group.bench_function("summary", |b| b.iter(|| black_box(query::summary(&analysis).children.len())));
    group.finish();
}

criterion_group!(benches, tokenizing, interpreting, throughput, querying);
criterion_main!(benches);
