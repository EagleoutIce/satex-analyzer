//! Every engine primitive must be known (even `Unmodeled` ones) for `\ifx`.
//! Which engine has which name was probed against the TeX Live 2026 binaries
//! (see `engine_catalogs_match_the_installed_engines` in `tests/it/engine.rs`),
//! and the tables are checked against the TeX Live 2023 measurements in
//! `doc/primitive-facts/*.tsv` (doc/primitive-facts.md).

use satex::fixed::HashMap;

use satex::builtins::{Primitive, initial_meanings};
use satex::config::Engine;
use satex::tex::{Interner, Meaning, Sym};

const ENGINES: [Engine; 4] = [Engine::Tex, Engine::PdfTeX, Engine::XeTeX, Engine::LuaTeX];

fn table(engine: Engine) -> (Interner, HashMap<Sym, Meaning>) {
    let mut it = Interner::default();
    let table = initial_meanings(&mut it, engine);
    (it, table)
}

fn meaning<'a>(it: &Interner, table: &'a HashMap<Sym, Meaning>, name: &str) -> Option<&'a Meaning> {
    it.lookup(name).and_then(|s| table.get(&s))
}

fn assert_known(engine: Engine, name: &str) {
    let (it, table) = table(engine);
    let m = meaning(&it, &table, name).unwrap_or_else(|| panic!("{name} should be defined for {engine:?} but is not"));
    assert_ne!(
        *m,
        Meaning::Primitive(Primitive::Relax),
        "{name} must not be \\relax under {engine:?} (that would answer \\ifx\\{name}\\relax wrong)"
    );
}

fn assert_absent(engine: Engine, name: &str) {
    let (it, table) = table(engine);
    let m = meaning(&it, &table, name);
    assert!(m.is_none(), "{name} should not be defined for {engine:?}, found {m:?}");
}

#[test]
fn tex82_primitives_are_known_by_every_engine() {
    for engine in ENGINES {
        for name in [
            "def",
            "hsize",
            "vsize",
            "parindent",
            "baselineskip",
            "tolerance",
            "hbadness",
            "hbox",
            "vbox",
            "vtop",
            "vsplit",
            "unhbox",
            "unvbox",
            "unkern",
            "unskip",
            "unpenalty",
            "lastbox",
            "lastkern",
            "lastskip",
            "lastpenalty",
            "raise",
            "lower",
            "moveleft",
            "moveright",
            "mathchar",
            "mathcode",
            "delimiter",
            "radical",
            "mathaccent",
            "mskip",
            "mkern",
            "nonscript",
            "vcenter",
            "left",
            "right",
            "over",
            "atop",
            "above",
            "overwithdelims",
            "font",
            "fontdimen",
            "fontname",
            "nullfont",
            "skewchar",
            "hyphenchar",
            "char",
            "chardef",
            "accent",
            "discretionary",
            "-",
            "input",
            "endinput",
            "openin",
            "closein",
            "read",
            "openout",
            "closeout",
            "write",
            "special",
            "immediate",
            "shipout",
            "mark",
            "insert",
            "vadjust",
            "topmark",
            "firstmark",
            "botmark",
            "splitfirstmark",
            "splitbotmark",
            "halign",
            "valign",
            "noalign",
            "omit",
            "span",
            "cr",
            "crcr",
            "tabskip",
            "meaning",
            "string",
            "number",
            "romannumeral",
            "jobname",
            "the",
            "csname",
            "endcsname",
            "expandafter",
            "noexpand",
            "end",
            "dump",
            "batchmode",
        ] {
            assert_known(engine, name);
        }
    }
}

#[test]
fn etex_extensions_are_known_by_every_engine_but_tex() {
    for engine in ENGINES {
        for name in [
            "protected",
            "detokenize",
            "unexpanded",
            "scantokens",
            "readline",
            "unless",
            "ifdefined",
            "ifcsname",
            "iffontchar",
            "eTeXversion",
            "eTeXrevision",
            "everyeof",
            "tracingassigns",
            "tracinggroups",
            "currentgrouplevel",
            "currentgrouptype",
            "currentiflevel",
            "currentiftype",
            "currentifbranch",
            "lastnodetype",
            "interactionmode",
            "showgroups",
            "showtokens",
            "showifs",
            "middle",
            "numexpr",
            "dimexpr",
            "glueexpr",
            "muexpr",
            "gluestretch",
            "glueshrink",
            "gluestretchorder",
            "glueshrinkorder",
            "gluetomu",
            "mutoglue",
            "marks",
            "topmarks",
            "firstmarks",
            "botmarks",
            "splitfirstmarks",
            "splitbotmarks",
            "parshapelength",
            "parshapeindent",
            "parshapedimen",
            "fontcharwd",
            "fontcharht",
            "fontchardp",
            "fontcharic",
            "pagediscards",
            "splitdiscards",
            "tracingifs",
            "tracingscantokens",
            "tracingnesting",
            "predisplaydirection",
            "lastlinefit",
            "savingvdiscards",
            "savinghyphcodes",
            "displaywidowpenalties",
            "interlinepenalties",
            "clubpenalties",
            "widowpenalties",
        ] {
            if engine == Engine::Tex {
                assert_absent(engine, name);
            } else {
                assert_known(engine, name);
            }
        }
        // TeX--XeT: LuaTeX has its own directions instead.
        for name in ["beginL", "endL", "beginR", "endR", "TeXXeTstate", "tracingstacklevels"] {
            if matches!(engine, Engine::PdfTeX | Engine::XeTeX) {
                assert_known(engine, name);
            } else {
                assert_absent(engine, name);
            }
        }
    }
}

#[test]
fn pdftex_parameters_and_kerning_codes_are_pdftex_only() {
    // Probed against TeX Live 2026: plain `tex` has none of these, XeTeX and
    // LuaTeX have only the protrusion and margin-kerning ones, and `\efcode`,
    // `\letterspacefont` and `\tagcode` reach LuaTeX but not XeTeX
    // (pdftex manual, "Primitives"; LuaTeX manual, "Changes from pdfTeX").
    for name in [
        "pdfpxdimen",
        "pdfdestmargin",
        "pdflinkmargin",
        "pdfcolorstack",
        "pdffontattr",
        "pdfmajorversion",
        "pdflastobj",
        "knaccode",
        "shbscode",
    ] {
        assert_known(Engine::PdfTeX, name);
        assert_absent(Engine::Tex, name);
        assert_absent(Engine::XeTeX, name);
        assert_absent(Engine::LuaTeX, name);
    }
    for name in ["lpcode", "rpcode", "leftmarginkern", "rightmarginkern", "synctex", "showstream"] {
        assert_known(Engine::PdfTeX, name);
        assert_known(Engine::XeTeX, name);
        assert_known(Engine::LuaTeX, name);
        assert_absent(Engine::Tex, name);
    }
    for name in ["efcode", "letterspacefont", "tagcode"] {
        assert_known(Engine::PdfTeX, name);
        assert_known(Engine::LuaTeX, name);
        assert_absent(Engine::XeTeX, name);
        assert_absent(Engine::Tex, name);
    }
}

#[test]
fn pdftex_primitives_are_known_only_by_pdftex() {
    // LuaTeX dropped the `\pdf…` primitives for `\pdfextension` and friends
    // (LuaTeX manual, "Changes from pdfTeX") and XeTeX never took them.
    for name in [
        "pdfoutput",
        "pdftexversion",
        "pdftexrevision",
        "pdfliteral",
        "pdfobj",
        "pdfxform",
        "pdfrefxform",
        "pdfximage",
        "pdfrefximage",
        "pdfannot",
        "pdfstartlink",
        "pdfendlink",
        "pdfoutline",
        "pdfdest",
        "pdfthread",
        "pdfinfo",
        "pdfcatalog",
        "pdfnames",
        "pdftrailer",
        "pdfpageattr",
        "pdfpagesattr",
        "pdfpageresources",
        "pdfcompresslevel",
        "pdfdecimaldigits",
        "pdfhorigin",
        "pdfvorigin",
        "pdfmapfile",
        "pdfmapline",
        "pdffontexpand",
        "pdfprotrudechars",
        "pdfadjustspacing",
        "pdfuniqueresname",
        "pdfescapestring",
        "pdfescapename",
        "pdfescapehex",
        "pdfunescapehex",
        "pdffiledump",
        "pdffilesize",
        "pdffilemoddate",
        "pdfmdfivesum",
        "pdfstrcmp",
        "pdfmatch",
        "pdflastmatch",
        "pdfshellescape",
        "pdfprimitive",
        "ifpdfprimitive",
        "ifpdfabsnum",
        "ifpdfabsdim",
        "pdfnormaldeviate",
        "pdfuniformdeviate",
        "pdfrandomseed",
        "pdfsetrandomseed",
        "pdfelapsedtime",
        "pdfresettimer",
        "pdfcreationdate",
        "pdfdraftmode",
        "pdfinsertht",
        "pdfnoligatures",
        "pdfglyphtounicode",
        "pdfgentounicode",
        "pdfinterwordspaceon",
        "pdfinterwordspaceoff",
    ] {
        assert_known(Engine::PdfTeX, name);
        assert_absent(Engine::Tex, name);
        assert_absent(Engine::XeTeX, name);
        assert_absent(Engine::LuaTeX, name);
    }
    // Position tracking and page size are the ones XeTeX kept under their
    // pdfTeX names, and LuaTeX renamed to `\savepos` and `\pagewidth`.
    for name in ["pdfsavepos", "pdflastxpos", "pdflastypos", "pdfpagewidth", "pdfpageheight"] {
        assert_known(Engine::PdfTeX, name);
        assert_known(Engine::XeTeX, name);
        assert_absent(Engine::Tex, name);
        assert_absent(Engine::LuaTeX, name);
    }
}

#[test]
fn pdftex_extensions_reached_xetex_and_luatex() {
    // The unprefixed extensions pdfTeX added, which both successors kept.
    for name in ["expanded", "ifincsname", "ignoreprimitiveerror", "partokenname", "partokencontext"] {
        assert_known(Engine::PdfTeX, name);
        assert_known(Engine::XeTeX, name);
        assert_known(Engine::LuaTeX, name);
        assert_absent(Engine::Tex, name);
    }
    // `\quitvmode` went to LuaTeX but not to XeTeX.
    assert_known(Engine::PdfTeX, "quitvmode");
    assert_known(Engine::LuaTeX, "quitvmode");
    assert_absent(Engine::XeTeX, "quitvmode");
    assert_absent(Engine::Tex, "quitvmode");
}

#[test]
fn xetex_primitives_are_known_only_by_xetex() {
    for name in [
        "XeTeXversion",
        "XeTeXrevision",
        "XeTeXinterchartoks",
        "XeTeXcharclass",
        "XeTeXinterchartokenstate",
        "XeTeXglyph",
        "XeTeXpicfile",
        "XeTeXpdffile",
        "XeTeXpdfpagecount",
        "XeTeXlinebreaklocale",
        "XeTeXlinebreakskip",
        "XeTeXlinebreakpenalty",
        "XeTeXhyphenatablelength",
        "XeTeXgenerateactualtext",
        "XeTeXinputencoding",
        "XeTeXdefaultencoding",
        "XeTeXinputnormalization",
        "XeTeXinterwordspaceshaping",
        "XeTeXcountglyphs",
        "XeTeXglyphindex",
        "XeTeXglyphname",
        "XeTeXglyphbounds",
        "XeTeXfirstfontchar",
        "XeTeXlastfontchar",
        "XeTeXfonttype",
        "XeTeXOTcountscripts",
        "XeTeXOTcountlanguages",
        "XeTeXOTcountfeatures",
        "XeTeXOTscripttag",
        "XeTeXOTlanguagetag",
        "XeTeXOTfeaturetag",
        "XeTeXcountfeatures",
        "XeTeXfeaturecode",
        "XeTeXfeaturename",
        "XeTeXfindfeaturebyname",
        "XeTeXisexclusivefeature",
        "XeTeXcountselectors",
        "XeTeXselectorcode",
        "XeTeXselectorname",
        "XeTeXfindselectorbyname",
        "XeTeXisdefaultselector",
        "XeTeXcountvariations",
        "XeTeXvariation",
        "XeTeXvariationname",
        "XeTeXvariationdefault",
        "XeTeXvariationmin",
        "XeTeXvariationmax",
        "XeTeXfindvariationbyname",
        "XeTeXtracingfonts",
        "XeTeXuseglyphmetrics",
        "XeTeXdashbreakstate",
        "XeTeXupwardsmode",
        "XeTeXprotrudechars",
        "XeTeXcharglyph",
        // XeTeX offers pdfTeX's string and file utilities without the prefix;
        // pdfTeX itself only has the `\pdf…` spellings.
        "creationdate",
        "elapsedtime",
        "resettimer",
        "filedump",
        "filemoddate",
        "filesize",
        "mdfivesum",
        "shellescape",
        "strcmp",
        "Ucharcat",
    ] {
        assert_known(Engine::XeTeX, name);
        assert_absent(Engine::Tex, name);
        assert_absent(Engine::PdfTeX, name);
        assert_absent(Engine::LuaTeX, name);
    }
}

#[test]
fn unicode_math_primitives_are_shared_by_xetex_and_luatex() {
    // XeTeX's `\U…` extensions, which LuaTeX adopted, together with the
    // random numbers and `\primitive` both engines took from pdfTeX.
    for name in [
        "Uchar",
        "Umathchar",
        "Umathchardef",
        "Umathcharnum",
        "Umathcharnumdef",
        "Umathcode",
        "Umathcodenum",
        "Udelcode",
        "Udelcodenum",
        "Udelimiter",
        "Umathaccent",
        "Uradical",
        "primitive",
        "ifprimitive",
        "normaldeviate",
        "uniformdeviate",
        "randomseed",
        "setrandomseed",
        "suppressfontnotfounderror",
    ] {
        assert_known(Engine::XeTeX, name);
        assert_known(Engine::LuaTeX, name);
        assert_absent(Engine::Tex, name);
        assert_absent(Engine::PdfTeX, name);
    }
}

#[test]
fn luatex_primitives_are_known_only_by_luatex() {
    for name in [
        "directlua",
        "luaescapestring",
        "luafunction",
        "luafunctioncall",
        "luadef",
        "luabytecode",
        "luabytecodecall",
        "luatexversion",
        "luatexrevision",
        "luatexbanner",
        "latelua",
        "lateluafunction",
        "catcodetable",
        "initcatcodetable",
        "savecatcodetable",
        "scantextokens",
        "csstring",
        "begincsname",
        "lastnamedcs",
        "nokerns",
        "noligs",
        "formatname",
        "attribute",
        "attributedef",
        "nospaces",
        "gleaders",
        "localbrokenpenalty",
        "localinterlinepenalty",
        "localleftbox",
        "localrightbox",
        "mathstyle",
        "alignmark",
        "aligntab",
        "outputbox",
        "pageleftoffset",
        "pagetopoffset",
        "pagewidth",
        "pageheight",
        "pardir",
        "textdir",
        "bodydir",
        "mathdir",
        "linedir",
        "pagedir",
        "boxdir",
        "protrudechars",
        "adjustspacing",
        "outputmode",
        "savepos",
        "lastxpos",
        "lastypos",
        "draftmode",
        "pdfextension",
        "pdffeedback",
        "pdfvariable",
        "dviextension",
        "dvifeedback",
        "dvivariable",
        "ifabsnum",
        "ifabsdim",
        "ifcondition",
        "hjcode",
        "hyphenationmin",
        "hyphenationbounds",
        "toksapp",
        "tokspre",
        "etoksapp",
        "etokspre",
        "gtoksapp",
        "gtokspre",
        "xtoksapp",
        "xtokspre",
        "eTeXgluestretchorder",
        "eTeXglueshrinkorder",
        "Uskewed",
        "Uskewedwithdelims",
        "Ustack",
        "Ustartmath",
        "Ustopmath",
        "Ustartdisplaymath",
        "Ustopdisplaymath",
        "Usubscript",
        "Usuperscript",
        "Uroot",
        "Uoverdelimiter",
        "Uunderdelimiter",
        "Umathquad",
        "Umathaxis",
    ] {
        assert_known(Engine::LuaTeX, name);
        assert_absent(Engine::Tex, name);
        assert_absent(Engine::PdfTeX, name);
        assert_absent(Engine::XeTeX, name);
    }
}

#[test]
fn directlua_reads_its_body_as_lua() {
    // `\directlua{…}` must consume the general text, or its braces are read
    // as a TeX group and the Lua inside is executed as TeX.
    let (it, table) = table(Engine::LuaTeX);
    assert_eq!(
        meaning(&it, &table, "directlua"),
        Some(&Meaning::Primitive(Primitive::Lua(satex::builtins::LuaOp::Direct)))
    );
    assert_eq!(
        meaning(&it, &table, "latelua"),
        Some(&Meaning::Primitive(Primitive::Lua(satex::builtins::LuaOp::Late)))
    );
    assert_eq!(satex::builtins::takes(Primitive::Lua(satex::builtins::LuaOp::Direct)), (1, Some(1)));
}

#[test]
fn per_engine_counts_meet_the_documented_floor() {
    // Only the engine's own primitives count: everything the formats define
    // comes from their code.  tex.web has 322 primitives (plus the frozen
    // ones), e-TeX adds its 60-odd, pdfTeX, XeTeX and LuaTeX theirs: at
    // least what doc/primitive-facts/*.tsv counts.
    let floors = [(Engine::Tex, 322), (Engine::PdfTeX, 545), (Engine::XeTeX, 498), (Engine::LuaTeX, 722)];
    for (engine, floor) in floors {
        let (_, table) = table(engine);
        let count = table.values().filter(|m| m.prim().is_some_and(satex::builtins::engine_primitive)).count();
        assert!(count >= floor, "{engine:?} has {count} engine primitives, expected at least {floor}");
    }
}

// The tables against the measured engines (doc/primitive-facts.md).

/// The measurement each satex engine is compared to: `pdflatex` runs pdfTeX
/// with eTeX, which is `etex.tsv`.
fn facts_file(engine: Engine) -> &'static str {
    match engine {
        Engine::Tex => "tex",
        Engine::PdfTeX => "etex",
        Engine::XeTeX => "xetex",
        Engine::LuaTeX => "luatex",
    }
}

/// One row of a facts file.
struct Fact {
    name: String,
    command: String,
    value_class: String,
    initial: String,
}

fn facts(engine: Engine) -> Vec<Fact> {
    let path = format!("{}/doc/primitive-facts/{}.tsv", env!("CARGO_MANIFEST_DIR"), facts_file(engine));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    text.lines()
        .skip(1)
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            Fact {
                name: fields[0].to_string(),
                command: fields[1].to_string(),
                value_class: fields[2].to_string(),
                initial: fields[5].to_string(),
            }
        })
        .collect()
}

/// Names satex registers that the TeX Live 2023 engines lack: TeX Live 2025
/// added them, and satex targets the newer engines.
fn newer_than_the_facts(engine: Engine) -> &'static [&'static str] {
    match engine {
        Engine::Tex => &[],
        Engine::PdfTeX => &["ignoreprimitiveerror", "pdfptexuseunderscore"],
        Engine::XeTeX => &["ignoreprimitiveerror"],
        Engine::LuaTeX => &["ignoreprimitiveerror", "mathemptydisplaymode"],
    }
}

#[test]
fn every_engine_registers_the_measured_primitives() {
    for engine in ENGINES {
        let (it, table) = table(engine);
        let ours: std::collections::BTreeSet<String> = table
            .keys()
            .map(|s| it.name(*s).to_string())
            .filter(|n| n.chars().all(|c| c.is_ascii_alphabetic()))
            .collect();
        let theirs: std::collections::BTreeSet<String> = facts(engine).into_iter().map(|f| f.name).collect();
        let missing: Vec<&String> = theirs.difference(&ours).collect();
        assert!(missing.is_empty(), "{engine:?} lacks primitives the engine has: {missing:?}");
        let newer = newer_than_the_facts(engine);
        let extra: Vec<&String> = ours.difference(&theirs).filter(|n| !newer.contains(&n.as_str())).collect();
        assert!(extra.is_empty(), "{engine:?} registers names the engine does not have: {extra:?}");
        for name in newer {
            assert_known(engine, name);
        }
        for name in [" ", "/", "-"] {
            assert_known(engine, name);
        }
    }
}

#[test]
fn parameter_kinds_match_the_measured_value_classes() {
    for engine in ENGINES {
        let (it, table) = table(engine);
        for fact in facts(engine) {
            let Some(Meaning::Primitive(p)) = meaning(&it, &table, &fact.name) else { continue };
            // A quantity the current list or the page builder owns reads
            // nothing from the probe's vertical-mode context.
            if satex::builtins::owned(&fact.name).is_some() {
                continue;
            }
            let class = match p {
                Primitive::IntegerParameter => "int",
                Primitive::DimenParameter => "dimen",
                Primitive::GlueParameter { mu: false } => "glue",
                Primitive::GlueParameter { mu: true } => "muglue",
                Primitive::TokensParameter => "toks",
                _ => {
                    // The engine stores it in its parameter table, so satex
                    // has to as well: `\catcodetable` alone is a Lua op that
                    // reads back as the number of the table in force.  Only
                    // LuaTeX's commands are its own; the other files borrow
                    // them, and `\setlanguage` is no parameter outside LuaTeX
                    // ("You can't use `\setlanguage' after \the").
                    let assigned = engine == Engine::LuaTeX
                        && matches!(
                            fact.command.as_str(),
                            "assign_int"
                                | "assign_dimen"
                                | "assign_glue"
                                | "assign_mu_glue"
                                | "assign_toks"
                                | "assign_direction"
                        );
                    let read = matches!(fact.value_class.as_str(), "int" | "dimen" | "glue" | "muglue" | "toks");
                    assert!(
                        !(assigned && read) || fact.name == "catcodetable",
                        "{engine:?} \\{}: the engine has a {} parameter, satex {p:?}",
                        fact.name,
                        fact.value_class
                    );
                    continue;
                }
            };
            // An empty token list prints nothing, which the probe could not
            // tell from other text.
            let measured = if fact.value_class == "toks/other" { "toks" } else { fact.value_class.as_str() };
            assert_eq!(measured, class, "{engine:?} \\{}: satex {p:?}", fact.name);
        }
    }
}

#[test]
fn parameters_start_where_the_measured_engines_start_them() {
    use satex::config::{Config, Load, Texmf};
    use satex::plugin::Kernel;
    for engine in ENGINES {
        let (it, table) = table(engine);
        let mut probed = Vec::new();
        let mut source = String::from(crate::support::PRELUDE);
        for fact in facts(engine) {
            let Some(Meaning::Primitive(p)) = meaning(&it, &table, &fact.name) else { continue };
            if !matches!(p, Primitive::IntegerParameter | Primitive::DimenParameter)
                || satex::builtins::owned(&fact.name).is_some()
                // The clock and the interaction the probe ran in.
                || matches!(fact.name.as_str(), "year" | "month" | "day" | "time" | "interactionmode")
                // The output plugin sets them, as the run's driver does.
                || matches!(fact.name.as_str(), "pdfoutput" | "outputmode")
                // Fixed at 1 in the LuaTeX satex targets, writable and 0 in
                // LuaTeX 1.17.
                || engine == Engine::LuaTeX && fact.name == "mathdefaultsmode"
            {
                continue;
            }
            let expected = fact.initial.trim_end_matches("sp").to_string();
            source.push_str(&format!("\\edef\\zz{}{{\\number\\{}}}\n", fact.name, fact.name));
            probed.push((fact.name, expected));
        }
        let cfg = Config {
            engine: Some(engine),
            kernel: Some(Kernel::None),
            load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
            texmf: Texmf { kpsewhich: false, ..Default::default() },
            ..crate::support::config()
        };
        let analysis = satex::machine::Machine::analyze(&source, None, &cfg);
        let wrong: Vec<String> = probed
            .iter()
            .filter_map(|(name, expected)| {
                let ours = crate::support::body(&analysis, &format!("zz{name}"));
                (ours != *expected).then(|| format!("\\{name}: satex {ours}, engine {expected}"))
            })
            .collect();
        assert!(probed.len() > 50, "{engine:?}: only {} parameters probed", probed.len());
        assert!(wrong.is_empty(), "{engine:?} starts these parameters elsewhere:\n{}", wrong.join("\n"));
    }
}
