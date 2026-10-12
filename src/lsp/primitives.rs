//! TeXbook-style syntax of common primitives for hover (The TeXbook, ch. 24;
//! e-TeX manual), shown only where the name still means the primitive.

use crate::machine::Analysis;
use crate::tex::Meaning;

const D: &str = "⟨control sequence⟩⟨parameter text⟩{⟨replacement text⟩}";
const IF: &str = "⟨token₁⟩⟨token₂⟩";

/// Name, syntax after the name, e-TeX.
const SYNTAX: &[(&str, &str, bool)] = &[
    ("def", D, false),
    ("edef", D, false),
    ("gdef", D, false),
    ("xdef", D, false),
    ("let", "⟨control sequence⟩⟨equals⟩⟨one optional space⟩⟨token⟩", false),
    ("futurelet", "⟨control sequence⟩⟨token₁⟩⟨token₂⟩", false),
    ("expandafter", "⟨token₁⟩⟨token₂⟩", false),
    ("noexpand", "⟨token⟩", false),
    ("csname", "⟨tokens⟩\\endcsname", false),
    ("endcsname", " (ends \\csname⟨tokens⟩\\endcsname)", false),
    ("the", "⟨internal quantity⟩", false),
    ("relax", "", false),
    ("if", IF, false),
    ("ifx", IF, false),
    ("ifcat", IF, false),
    ("ifnum", "⟨number₁⟩⟨relation⟩⟨number₂⟩", false),
    ("ifdim", "⟨dimen₁⟩⟨relation⟩⟨dimen₂⟩", false),
    ("ifcase", "⟨number⟩⟨case 0⟩\\or⟨case 1⟩\\or…\\else⟨otherwise⟩\\fi", false),
    ("else", " (\\if…⟨true text⟩\\else⟨false text⟩\\fi)", false),
    ("fi", " (\\if…⟨text⟩\\fi)", false),
    ("or", " (\\ifcase⟨number⟩⟨case 0⟩\\or⟨case 1⟩…\\fi)", false),
    ("advance", "⟨numeric variable⟩⟨optional by⟩⟨number, dimen or glue⟩", false),
    ("multiply", "⟨numeric variable⟩⟨optional by⟩⟨number⟩", false),
    ("divide", "⟨numeric variable⟩⟨optional by⟩⟨number⟩", false),
    ("count", "⟨register⟩⟨equals⟩⟨number⟩", false),
    ("dimen", "⟨register⟩⟨equals⟩⟨dimen⟩", false),
    ("skip", "⟨register⟩⟨equals⟩⟨glue⟩", false),
    ("toks", "⟨register⟩⟨equals⟩{⟨balanced text⟩}", false),
    ("hbox", "⟨box specification⟩{⟨horizontal material⟩}", false),
    ("vbox", "⟨box specification⟩{⟨vertical material⟩}", false),
    ("setbox", "⟨register⟩⟨equals⟩⟨box⟩", false),
    ("wd", "⟨register⟩⟨equals⟩⟨dimen⟩", false),
    ("ht", "⟨register⟩⟨equals⟩⟨dimen⟩", false),
    ("dp", "⟨register⟩⟨equals⟩⟨dimen⟩", false),
    ("catcode", "⟨character code⟩⟨equals⟩⟨number⟩", false),
    ("input", "⟨file name⟩", false),
    ("write", "⟨number⟩{⟨balanced text⟩}", false),
    ("read", "⟨number⟩to⟨control sequence⟩", false),
    ("openout", "⟨number⟩⟨equals⟩⟨file name⟩", false),
    ("immediate", "⟨\\openout, \\write or \\closeout⟩", false),
    ("number", "⟨number⟩", false),
    ("romannumeral", "⟨number⟩", false),
    ("string", "⟨token⟩", false),
    ("meaning", "⟨token⟩", false),
    ("uppercase", "{⟨balanced text⟩}", false),
    ("lowercase", "{⟨balanced text⟩}", false),
    ("global", "⟨assignment⟩", false),
    ("long", "⟨definition⟩", false),
    ("outer", "⟨definition⟩", false),
    ("protected", "⟨definition⟩", true),
    ("detokenize", "{⟨balanced text⟩}", true),
    ("unexpanded", "{⟨balanced text⟩}", true),
    ("numexpr", "⟨integer expression⟩\\relax", true),
    ("dimexpr", "⟨dimen expression⟩\\relax", true),
];

/// The syntax line for `\name` hovered in `path` at `line`:`col`, when its
/// meaning there is the primitive of that name: the document has no
/// definition of it before that place, and the kernel left it alone.
pub fn syntax(analysis: &Analysis, name: &str, path: &str, line: u32, col: u32) -> Option<String> {
    let (_, rest, etex) = SYNTAX.iter().find(|(n, ..)| *n == name)?;
    let want = *crate::builtins::primitive_names(analysis.plugins.engine).get(name)?;
    let sym = analysis.interner.lookup(name)?;
    let file = (0..analysis.file_names().len() as crate::tex::FileId).find(|&f| analysis.file_name(f) == path)?;
    let defs: Vec<_> = analysis.facts.defs.iter().filter(|d| d.name == sym).collect();
    let primitive = if defs.is_empty() {
        analysis.env.meaning(sym) == Meaning::Primitive(want)
    } else {
        // Defined somewhere in the run: only before every definition, and
        // only where the kernel did not redefine it.
        !analysis.kernel_sites.contains_key(&sym) && !defs.iter().any(|d| analysis.visible_in(d.span, file, line, col))
    };
    let kind = if *etex { "e-TeX primitive" } else { "TeX primitive" };
    primitive.then(|| format!("`\\{name}{rest}`\n\n*{kind}*\n\n"))
}
