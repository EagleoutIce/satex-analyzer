
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use satex::config::Config;
use satex::machine::Machine;
use satex::query::{self, Filter, Query};
use satex::tex::{CatcodeTable, Interner, Mouth};

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
        source.push_str(&format!(
            "\\newcommand{{\\bench@m{i}}}[2][x]{{#1:#2}}\n\\def\\bench@d{i}#1,#2.{{(#1|#2)}}\n"
        ));
    }
    source.push_str("\\makeatother\n");
    source
}

fn local() -> Config {
    Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
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

fn querying(c: &mut Criterion) {
    let analysis = Machine::analyze(&document(200), None, &local());
    let filter = Filter::parse(r#"tag=macro and name~"^..hl""#).expect("valid filter");
    let mut group = c.benchmark_group("query");
    group.bench_function("definitions", |b| {
        b.iter(|| black_box(query::run(&analysis, Query::Definitions, &Filter::Always).len()))
    });
    group.bench_function("filtered", |b| {
        b.iter(|| black_box(query::run(&analysis, Query::Definitions, &filter).len()))
    });
    group.bench_function("lint", |b| b.iter(|| black_box(satex::lint::lint(&analysis).len())));
    group.bench_function("slice", |b| {
        b.iter(|| {
            black_box(
                query::slice(&analysis, &["hl".to_string()], None, query::Direction::Backward).len(),
            )
        })
    });
    group.bench_function("summary", |b| b.iter(|| black_box(query::summary(&analysis).children.len())));
    group.finish();
}

criterion_group!(benches, tokenizing, interpreting, querying);
criterion_main!(benches);
