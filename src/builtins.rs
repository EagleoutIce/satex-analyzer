//! Primitives the interpreter models (behavior in exec.rs).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::Engine;
use crate::tex::{Interner, Meaning, RegKind, Sym};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum DefMode {
    New,
    Renew,
    Provide,
    Declare,
}

impl DefMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DefMode::New => "new",
            DefMode::Renew => "renew",
            DefMode::Provide => "provide",
            DefMode::Declare => "declare",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Prefix {
    Global,
    Long,
    Outer,
    Protected,
    Immediate,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Cond {
    True,
    False,
    IfX,
    Defined,
    Csname,
    Num,
    Dim,
    Odd,
    Chars,
    Catcodes,
    Case,
    /// `\ifeof`: satex resolves files so it knows EOF state.
    Eof,
    /// A conditional the configuration asks to leave undecided.
    Opaque,
    /// `\ifvmode`, `\ifhmode`, `\ifinner`: the mode follows typesetting,
    /// which only the format is known not to do.
    VMode,
    HMode,
    InnerMode,
    /// `\ifmmode`: math mode is entered by a math shift, so the groups
    /// that are open say whether it is in force.
    MathMode,
    /// `\ifvoid`, `\ifhbox`, `\ifvbox`: they read a register number
    /// (tex.web § 505), and the box contents are typesetting.
    Box,
    /// pdfTeX's `\ifpdfabsnum` and `\ifpdfabsdim` (LuaTeX's `\ifabsnum`,
    /// `\ifabsdim`): `\ifnum`/`\ifdim` on absolute values.
    AbsNum,
    AbsDim,
    /// e-TeX's `\iffontchar⟨font⟩⟨number⟩`: whether the font has the
    /// character (etex_man § 3.10).
    FontChar,
    /// pdfTeX's `\ifincsname`: whether a `\csname` or `\ifcsname` is
    /// scanning its name.
    InCsname,
    /// `\ifpdfprimitive⟨cs⟩` (`\ifprimitive`): whether the control sequence
    /// still has the meaning of the primitive of that name.
    Primitive,
}

/// What an engine computes rather than stores: TeX's `last_item` commands
/// (tex.web § 416) and their e-TeX and pdfTeX relatives (etex_man § 3; the
/// pdfTeX manual), with the operands each reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum LastItem {
    Penalty,
    Kern,
    Skip,
    NodeType,
    Badness,
    InputLineNo,
    EtexVersion,
    PdftexVersion,
    GroupLevel,
    GroupType,
    IfLevel,
    IfType,
    IfBranch,
    GlueStretch,
    GlueShrink,
    GlueStretchOrder,
    GlueShrinkOrder,
    GlueToMu,
    MuToGlue,
    FontCharWd,
    FontCharHt,
    FontCharDp,
    FontCharIc,
    ParShapeLength,
    ParShapeIndent,
    ParShapeDimen,
    ElapsedTime,
    RandomSeed,
    ShellEscape,
    LastXPos,
    LastYPos,
    /// `\XeTeXversion`.
    XetexVersion,
    /// `\luatexversion`: read-only, its value the real `luatex --version`
    /// probed for the run rather than a constant table.
    LuatexVersion,
    /// XeTeX's font and glyph queries.
    Xetex(XeQuery),
    /// `\XeTeXpdfpagecount⟨file name⟩`.
    PdfPageCount,
}

/// XeTeX's queries of a font's OpenType and AAT tables (XeTeX reference,
/// "Font-related primitives").  A TFM font answers at once and reads
/// nothing past the font (xetex.web, `not_native_font_error`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum XeQuery {
    FontType,
    FirstFontChar,
    LastFontChar,
    CountGlyphs,
    CountVariations,
    CountFeatures,
    Variation,
    VariationMin,
    VariationMax,
    VariationDefault,
    FeatureCode,
    CountSelectors,
    SelectorCode,
    IsExclusiveFeature,
    IsDefaultSelector,
    FindVariationByName,
    FindFeatureByName,
    FindSelectorByName,
    OtCountScripts,
    OtCountLanguages,
    OtCountFeatures,
    OtScriptTag,
    OtLanguageTag,
    OtFeatureTag,
    /// These three ask the current font.
    CharGlyph,
    GlyphIndex,
    GlyphBounds,
    /// `\XeTeXvariationname`, `\XeTeXfeaturename`, `\XeTeXglyphname`,
    /// `\XeTeXselectorname`: text, expanded like `\fontname`.
    VariationName,
    FeatureName,
    GlyphName,
    SelectorName,
}

impl XeQuery {
    /// Whether a font comes first, how many numbers follow it, and whether
    /// a name ends the operands.
    pub fn operands(self) -> (bool, usize, bool) {
        use XeQuery as Q;
        match self {
            Q::FontType
            | Q::FirstFontChar
            | Q::LastFontChar
            | Q::CountGlyphs
            | Q::CountVariations
            | Q::CountFeatures
            | Q::OtCountScripts => (true, 0, false),
            Q::Variation
            | Q::VariationMin
            | Q::VariationMax
            | Q::VariationDefault
            | Q::FeatureCode
            | Q::CountSelectors
            | Q::IsExclusiveFeature
            | Q::OtCountLanguages
            | Q::OtScriptTag
            | Q::VariationName
            | Q::FeatureName
            | Q::GlyphName => (true, 1, false),
            Q::SelectorCode | Q::IsDefaultSelector | Q::OtCountFeatures | Q::OtLanguageTag | Q::SelectorName => {
                (true, 2, false)
            }
            Q::OtFeatureTag => (true, 3, false),
            Q::FindVariationByName | Q::FindFeatureByName => (true, 0, true),
            Q::FindSelectorByName => (true, 1, true),
            Q::CharGlyph => (false, 1, false),
            Q::GlyphIndex => (false, 0, true),
            Q::GlyphBounds => (false, 2, false),
        }
    }

    /// What a TFM font answers, probed against XeTeX 0.999998.
    pub fn tfm_answer(self) -> i64 {
        use XeQuery as Q;
        match self {
            Q::FeatureCode
            | Q::CountSelectors
            | Q::SelectorCode
            | Q::IsExclusiveFeature
            | Q::IsDefaultSelector
            | Q::FindVariationByName
            | Q::FindFeatureByName
            | Q::FindSelectorByName
            | Q::OtCountLanguages
            | Q::OtCountFeatures
            | Q::OtScriptTag
            | Q::OtLanguageTag
            | Q::OtFeatureTag => -1,
            _ => 0,
        }
    }

    pub fn is_text(self) -> bool {
        matches!(self, XeQuery::VariationName | XeQuery::FeatureName | XeQuery::GlyphName | XeQuery::SelectorName)
    }
}

/// Engine state assigned outside the table of equivalents (tex.web
/// §§ 1242-1248, etex_man § 3): the page and paragraph registers, box
/// dimensions, and e-TeX's penalty arrays.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Special {
    /// `\wd`, `\ht`, `\dp`: a box register's dimension.
    BoxDimen,
    /// `\parshape⟨n⟩` followed by n pairs of dimensions.
    ParShape,
    /// `\interlinepenalties⟨n⟩` and its relatives, followed by n numbers.
    Penalties,
    /// XeTeX's `\XeTeXcharclass⟨char⟩`, a local code like `\sfcode`.
    CharClass,
    /// XeTeX's `\XeTeXinterchartoks⟨class⟩⟨class⟩`, a local token list.
    InterCharToks,
    /// `\Umathcode`, `\Udelcode` and their `…num` forms: the math or
    /// delimiter code table, as the engine packs its 21-bit entries.
    UCode { math: bool, num: bool },
}

/// The font tables (tex.web §§ 578-580, 1253): `\fontdimen⟨n⟩⟨font⟩`,
/// `\hyphenchar⟨font⟩`, `\skewchar⟨font⟩`, and pdfTeX's per-character codes
/// `\lpcode⟨font⟩⟨char⟩` and relatives.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum FontParam {
    Dimen,
    HyphenChar,
    SkewChar,
    /// With the value a character has until one is assigned: 1000 for
    /// `\efcode`, 1 for `\tagcode`, 0 for the rest (probed, pdfTeX 1.40).
    CharCode(i16),
}

/// What a command the stomach obeys reads before it is done, when its
/// effect is typesetting, output or diagnostics (tex.web part 49; the pdfTeX
/// manual).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Syntax {
    Nothing,
    Number,
    Dimen,
    /// `\abovewithdelims` and relatives: two delimiters, then for
    /// `\abovewithdelims` a dimension (tex.web § 1178).
    Delimiters {
        dimen: bool,
    },
    /// `\batchmode` … `\errorstopmode`: they set `\interactionmode`
    /// (tex.web § 1264).
    Interaction(u8),
    GeneralText,
    NumberText,
    FontText,
    TwoTexts,
    Font,
    /// `\show⟨token⟩`.
    Token,
    /// `\showthe⟨internal quantity⟩`.
    Internal,
    /// `\hbox`, `\vbox`, `\vtop`, `\vcenter`, `\halign`, `\valign`:
    /// `to⟨dimen⟩` or `spread⟨dimen⟩` (tex.web § 645).
    BoxSpec,
    /// `\hrule`, `\vrule`: any of `width`, `height`, `depth` with a
    /// dimension each (tex.web § 463).
    RuleSpec,
    /// `\setbox⟨number⟩⟨optional =⟩` (tex.web § 1241).
    SetBox,
    /// `\vsplit⟨number⟩to⟨dimen⟩` (tex.web § 1082).
    VSplit,
    /// `\openout⟨number⟩⟨optional =⟩⟨file name⟩` (tex.web § 1351).
    OpenOut,
    /// `\closeout⟨number⟩` (tex.web § 1352).
    CloseOut,
    /// `\textfont⟨number⟩⟨optional =⟩⟨font⟩` (tex.web § 1234).
    FamilyFont,
    /// `\letterspacefont⟨cs⟩⟨font⟩⟨number⟩`, `\pdfcopyfont⟨cs⟩⟨font⟩`.
    CopyFont {
        amount: bool,
    },
    /// `\pdffontexpand⟨font⟩⟨stretch⟩⟨shrink⟩⟨step⟩[autoexpand]`.
    FontExpand,
    /// `\readline⟨number⟩to⟨cs⟩` (etex_man § 3.2).
    ReadLine,
    /// pdfTeX's `\pdfannot`, `\pdfstartlink`, `\pdfdest`, `\pdfoutline`,
    /// `\pdfthread`/`\pdfstartthread`, `\pdfxform`, `\pdfximage`,
    /// `\pdfcolorstack`, `\pdfcatalog`, with the keywords the manual gives.
    PdfAnnot,
    PdfStartLink,
    PdfDest,
    PdfOutline,
    PdfThread,
    PdfXForm,
    PdfXImage,
    PdfColorStack,
    PdfCatalog,
    /// XeTeX's `\XeTeXlinebreaklocale⟨file name⟩` and relatives.
    FileName,
    /// `\Umathchardef⟨cs⟩=⟨class⟩⟨family⟩⟨slot⟩`, `\Umathcharnumdef⟨cs⟩=⟨n⟩`.
    UMathCharDef {
        num: bool,
    },
    /// This many numbers.
    Numbers(u8),
    /// LuaTeX's `\setfontid⟨number⟩`: selects the font of that number.
    SetFontId,
    /// `\Umathaccent`: LuaTeX's keywords, then class, family and slot.
    MathAccent,
    /// `\XeTeXpicfile⟨file name⟩` and `\XeTeXpdffile`, with the page, box
    /// and scaling keywords of the XeTeX reference.
    PicFile,
}

/// An expandable primitive that yields text (tex.web § 468, `convert`; the
/// e-TeX and pdfTeX additions).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Convert {
    EtexRevision,
    PdftexRevision,
    PdftexBanner,
    FontName,
    StrCmp,
    EscapeHex,
    UnescapeHex,
    EscapeString,
    EscapeName,
    FileSize,
    FileModDate,
    FileDump,
    MdFiveSum,
    Match,
    LastMatch,
    UniformDeviate,
    NormalDeviate,
    CreationDate,
    PdfFontName,
    PdfFontObjNum,
    PdfFontSize,
    PageRef,
    XFormName,
    InsertHt,
    MarginKern,
    ColorStackInit,
    /// `\pdfximagebbox⟨image⟩⟨corner⟩`.
    ImageBBox,
    /// `\topmark` and the other marks of the page builder.
    Mark,
    /// e-TeX's `\topmarks⟨number⟩` and relatives.
    Marks,
    /// `\pdfprimitive⟨cs⟩`: the primitive of that name, whatever the name
    /// means now.
    Primitive,
    /// `\XeTeXrevision`.
    XetexRevision,
    /// LuaTeX's `\fontid⟨font⟩`: the font's number.
    FontId,
    /// XeTeX's names of a font's variations, features, selectors, glyphs.
    Xetex(XeQuery),
    /// `\Uchar⟨number⟩`: the character, of category 12 (10 for a space);
    /// XeTeX's `\Ucharcat⟨number⟩⟨number⟩`: of the category given.
    Uchar {
        catcode: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Arith {
    Advance,
    Multiply,
    Divide,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum LoadKind {
    Package,
    Class,
    Input,
    Include,
    Conditional,
    Inherited,
    /// A Lua module a `require` or `dofile` in `\directlua` reads.
    Lua,
}

/// What a LuaTeX primitive of the Lua interface does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum LuaOp {
    /// `\directlua{…}`: the expanded text is Lua, run at once; what it
    /// prints is read next.
    Direct,
    /// `\latelua{…}`: the same, run when the page is shipped out.
    Late,
    /// `\luaescapestring{…}`: the expanded text, escaped for a Lua string.
    Escape,
    /// `\initcatcodetable n`: table `n` holds the IniTeX catcodes.
    InitCatcodes,
    /// `\savecatcodetable n`: table `n` holds the catcodes in force.
    SaveCatcodes,
    /// `\catcodetable n`: table `n`'s catcodes come in force, locally.
    SelectCatcodes,
}

impl LoadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LoadKind::Package => "package",
            LoadKind::Class => "class",
            LoadKind::Input => "input",
            LoadKind::Include => "include",
            LoadKind::Conditional => "conditional",
            LoadKind::Inherited => "inherited",
            LoadKind::Lua => "lua",
        }
    }
    pub fn is_package(self) -> bool {
        matches!(self, LoadKind::Package | LoadKind::Class | LoadKind::Inherited)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum OccKind {
    Label,
    Ref,
    Cite,
    Bibliography,
    Graphics,
    Section,
    BeginEnvironment,
    EndEnvironment,
    PassedOption,
    Identification,
    Message,
    Catcode,
    Write,
    ShellEscape,
    /// The body of a `\directlua` or `\latelua`.
    Lua,
    /// A file a docstrip batch file generates.
    Generate,
    /// An entry of a bibliography: the key a `\bibitem` writes to the `.aux`
    /// file for `\bibcite`.
    BibItem,
    /// A key a document call declares while it runs: a name it defines is
    /// made of its argument (`\newglossaryentry{k}` defines `\glo@k@name`).
    Key,
    /// A read of a name of a [`OccKind::Key`]'s shape.
    KeyUse,
    /// A key a written line carries for the program that reads its file
    /// (`\indexentry`, `\glossaryentry`).
    Entry,
    /// A character built with `\accent` rather than taken from the font.
    Accent,
    /// What the document hands the PDF writer: a `\pdfliteral`, a `\pdfobj`
    /// or a `\special`, with the payload as its key.
    Pdf,
}

impl OccKind {
    /// The name the configuration uses for this kind.
    pub fn named(name: &str) -> Option<OccKind> {
        [
            OccKind::Label,
            OccKind::Ref,
            OccKind::Cite,
            OccKind::Bibliography,
            OccKind::Graphics,
            OccKind::Section,
            OccKind::Write,
            OccKind::ShellEscape,
            OccKind::Lua,
            OccKind::BibItem,
            OccKind::Key,
            OccKind::KeyUse,
            OccKind::Entry,
            OccKind::EndEnvironment,
            OccKind::PassedOption,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == name)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            OccKind::Label => "label",
            OccKind::Ref => "ref",
            OccKind::Cite => "cite",
            OccKind::Bibliography => "bibliography",
            OccKind::Graphics => "graphics",
            OccKind::Section => "section",
            OccKind::BeginEnvironment => "begin-environment",
            OccKind::EndEnvironment => "end-environment",
            OccKind::PassedOption => "passed-option",
            OccKind::Identification => "identification",
            OccKind::Message => "message",
            OccKind::Write => "write",
            OccKind::ShellEscape => "shell-escape",
            OccKind::Lua => "lua",
            OccKind::Catcode => "catcode",
            OccKind::Generate => "generate",
            OccKind::BibItem => "bibitem",
            OccKind::Key => "key",
            OccKind::KeyUse => "key-use",
            OccKind::Entry => "entry",
            OccKind::Accent => "accent",
            OccKind::Pdf => "pdf",
        }
    }
}

/// The commands a docstrip batch file (`.ins`) is written in (docstrip.dtx,
/// "How to use the docstrip program").
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum DocstripOp {
    /// `\generate{\file{out}{\from{src}{guards}…}…}`.
    Generate,
    /// `\generateFile{out}{ask}{\from{src}{guards}…}`, the older interface.
    GenerateFile,
    /// `\usedir{dir}`: the directory the files generated next belong in.
    UseDir,
    /// `\preamble`…`\endpreamble` and its named and postamble forms: text
    /// docstrip copies into what it writes, not TeX to run.
    Preamble { named: bool, post: bool },
    /// `\batchinput{file}`: another batch file, run from this one.
    BatchInput,
    /// A setting that does not change which files are generated, with the
    /// arguments it reads: `\keepsilent`, `\usepreamble⟨cs⟩`.
    Setting { arguments: u8 },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum KeyDialect {
    /// LaTeX 2022 kernel `\DeclareKeys`.
    Kernel,
    /// `l3keys`.
    L3,
}

impl KeyDialect {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyDialect::Kernel => "kernel",
            KeyDialect::L3 => "l3keys",
        }
    }
}

/// Text a gullet primitive expands to.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum TextOf {
    String,
    Meaning,
    The,
    Number,
    Roman,
    Detokenize,
    JobName,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum HookOp {
    Add,
    AddNext,
    AddLegacy,
    AddEnvironment,
    New,
    NewPair,
    Use,
    UseOnce,
    Remove,
    Rule,
    IfEmpty,
    Show,
}

/// One of TeX's per-character code tables (tex.web § 230).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum CodeTable {
    Lower,
    Upper,
    Space,
    Math,
    Delimiter,
}

impl CodeTable {
    pub fn as_str(self) -> &'static str {
        match self {
            CodeTable::Lower => "lccode",
            CodeTable::Upper => "uccode",
            CodeTable::Space => "sfcode",
            CodeTable::Math => "mathcode",
            CodeTable::Delimiter => "delcode",
        }
    }

    /// The IniTeX default for a character (tex.web § 232): letters map to
    /// their own case pair, everything else to zero.
    pub fn default_for(self, c: char) -> i64 {
        match self {
            CodeTable::Lower if c.is_ascii_alphabetic() => c.to_ascii_lowercase() as i64,
            CodeTable::Upper if c.is_ascii_alphabetic() => c.to_ascii_uppercase() as i64,
            // tex.web § 232: `\sfcode` is 1000 for everything but an
            // uppercase letter, which starts at 999.
            CodeTable::Space if c.is_ascii_uppercase() => 999,
            CodeTable::Space => 1000,
            CodeTable::Math if c.is_ascii_digit() => c as i64 + 0x7000,
            CodeTable::Math if c.is_ascii_alphabetic() => c as i64 + 0x7100,
            CodeTable::Math => c as i64,
            // tex.web § 240: every `\delcode` is -1 but that of `.`.
            CodeTable::Delimiter if c == '.' => 0,
            CodeTable::Delimiter => -1,
            _ => 0,
        }
    }
}

/// The primitives whose text goes to the PDF writer: pdfTeX's `\pdfliteral`
/// and `\pdfobj` (pdfTeX manual, "PDF objects", "Literals"), LuaTeX's
/// `\pdfextension` (LuaTeX manual, "PDF extensions") and `\special`, which
/// dvipdfmx and XeTeX read as `pdf:` commands (dvipdfmx manual, "PDF
/// specials").
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum PdfOp {
    Literal,
    Object,
    Special,
    Extension,
    /// `\pdfpageresources`, where a page's `/Properties` name what its
    /// marked content refers to.
    Resources,
}

/// What a typesetting primitive scans before the material it sets.  It
/// matters when a token list is read as text rather than set: hyperref drops
/// the primitive from a PDF string and takes the value it scans with it
/// (hyperref.sty, `\HyPsd@CheckCatcodes`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Typeset {
    /// Boxes, rules and the glue orders: nothing precedes the material.
    Plain,
    /// `\penalty`: a number.
    Number,
    /// `\kern`, `\raise`, `\lower`, `\moveleft`, `\moveright`: a dimension.
    Dimen,
    /// `\hskip`, `\vskip`, `\mskip`: glue.
    Glue,
    /// `\left`, `\right`, `\middle`: a delimiter, which is one character or
    /// one control sequence (tex.web § 1161).
    Delimiter,
}

/// What a token held back waits for.  tex.web § 265 makes `\aftergroup` and
/// `\afterassignment` two commands, so a copy such as `\tex_aftergroup:D`
/// carries which one it is and not the name it was reached through.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Deferred {
    /// `\aftergroup`: the end of the current group (tex.web § 326).
    Group,
    /// `\afterassignment`: the next assignment (tex.web § 1269).
    Assignment,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Primitive {
    Def {
        global: bool,
        expand: bool,
    },
    Let {
        global: bool,
        future: bool,
    },
    Prefix(Prefix),

    ExpandAfter,
    NoExpand,
    /// What a token behind `\noexpand`'s `dont_expand` marker means to
    /// `\ifx` (tex.web § 358): `relax` with `no_expand_flag`.
    DontExpand,
    /// satex's own marker where skipping for a conditional begun in another
    /// file reached this file's text: the path that skips on as TeX does,
    /// `nested` levels deep, and the one that ended the skip there.
    SkipCrossing {
        nested: u8,
        to_fi: bool,
    },
    Csname,
    /// LuaTeX's `\begincsname`: `\csname` that leaves an undefined name
    /// undefined and expands to nothing then.
    BeginCsname,
    /// LuaTeX's `\lastnamedcs`: the name the last `\csname`,
    /// `\begincsname` or `\ifcsname` built.
    LastNamedCs,
    Endcsname,
    /// A primitive that expands to text: `\string`, `\meaning`, `\the`,
    /// `\number`, `\romannumeral`, `\detokenize`, `\jobname`.
    Text(TextOf),
    /// `\unexpanded`: the group stands, and `\edef` stores it unexpanded
    /// (eTeX manual § 3.4).
    Reinject,
    /// `\scantokens`: the group is written out as characters and read again
    /// under the category codes now in force (eTeX manual § 3.9).
    /// LuaTeX's `\scantextokens` (`text`) appends no `\endlinechar` to the
    /// last line and no `\everyeof` (LuaTeX manual, "\scantextokens").
    ScanTokens {
        text: bool,
    },
    /// `\use:n` and its relatives: read that many arguments and leave them in
    /// the input, where the expansion carries on over them (interface3,
    /// "Selecting tokens").
    Use(u8),
    /// `\expanded`, `\use:e`: expand the group, then re-inject the result.
    Expanded,

    If(Cond),
    Else,
    Or,
    Fi,
    Unless,
    BeginGroup,
    EndGroup,
    /// `\aftergroup`, `\afterassignment`.
    Defer(Deferred),

    Allocate(RegKind),
    Register(RegKind),
    RegisterDef(RegKind),
    /// `\openin`, `\closein` and `\read`: the input streams `\ifeof` asks about.
    /// A variant `\prg_new_conditional:Npnn` generates: the arguments of the
    /// test, then its branches, of which satex analyzes every one.
    /// `\font\name=file at size`: defines the control sequence as a font
    /// identifier (tex.web § 1256).
    FontDef,
    OpenIn,
    CloseIn,
    Read,
    Arith(Arith),
    IntegerParameter,
    DimenParameter,
    CatcodeAssign,
    /// `\lccode`, `\uccode` and the other per-character code tables
    /// (tex.web § 1230), which take a character code before the `=`.
    CharCode(CodeTable),
    /// `\uppercase`/`\lowercase` (tex.web § 1288).
    CaseShift(CodeTable),

    Load(LoadKind),
    Endinput,

    End,

    Message {
        error: bool,
    },
    /// `\write⟨number⟩{…}`; stream 18 is the shell (TeX Live, "Shell escape").
    Write,
    /// `\numexpr`, `\dimexpr`, `\glueexpr`, `\muexpr` (eTeX manual § 3.5).
    Expr(RegKind),
    Relax,
    /// A primitive that only sets material on the page: a box, a rule, glue,
    /// a kern, a penalty (The TeXbook, chs. 11-14).  satex does not build
    /// pages, so it runs none of them; the distinction is what tells a
    /// command that contributes no text from one that does.
    Typeset(Typeset),
    /// LuaTeX's Lua interface and the catcode tables that come with it
    /// (LuaTeX manual, "Lua related primitives", "Catcode tables");
    /// [`crate::plugin::lua`] runs them.
    Lua(LuaOp),
    /// A docstrip batch-file command, in the `.ins` file that drives the
    /// extraction of a `.dtx`.
    Docstrip(DocstripOp),
    /// `\accent⟨number⟩`: an accent the typesetter builds over the character
    /// that follows (tex.web § 1123).
    Accent,
    /// A primitive whose text goes to the PDF writer.
    Pdf(PdfOp),
    /// A primitive of the engine this run models whose effect is not
    /// interpreted.  It does nothing, but it is a meaning of its own: a
    /// package asks `\ifx\csname expanded\endcsname\relax` to find out
    /// whether the primitive exists, and `\ifx` compares meanings (The
    /// TeXbook, ch. 20), so answering `\relax` would deny every engine
    /// primitive this interpreter leaves alone.
    Unmodeled,
    /// LuaTeX's `\alignmark` and `\aligntab`, which stand for `#` and `&`
    /// where a definition or an alignment is scanned (LuaTeX manual,
    /// "Macros"): meanings of their own, so `\ifx` tells them apart, and
    /// unmodeled otherwise.
    AlignMark,
    AlignTab,
    /// TeX's glue, muglue and token-list parameters (tex.web §§ 224, 230).
    GlueParameter {
        mu: bool,
    },
    TokensParameter,
    LastItem(LastItem),
    Special(Special),
    FontParam(FontParam),
    /// A control sequence Lua defined with `token.set_lua`: it calls
    /// function `id` of `lua.get_functions_table()`; `protected` ones are
    /// not expanded.
    LuaCall {
        id: u32,
        protected: bool,
    },
    /// A font identifier (tex.web § 1257): the font it selects, numbered in
    /// the order the fonts were loaded, `\nullfont` being 0.
    FontIdent(u32),
    Command(Syntax),
    Convert(Convert),
    /// A command whose meaning depends on the mode or changes it
    /// (tex.web §§ 1045-1200, `main_control`).
    Mode(ModeCmd),
    /// `\ignorespaces` (tex.web § 1045): the next non-blank token after
    /// expansion is read as a command.
    IgnoreSpaces,
}

/// A command of TeX's stomach that the mode governs (tex.web § 1045 cases
/// on `abs(mode)+cur_cmd`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum ModeCmd {
    /// `\par` (tex.web § 1094).
    Par,
    /// `\indent`, `\noindent` (tex.web § 1090 `start_par`).
    StartPar { indent: bool },
    /// pdfTeX's `\quitvmode`: a paragraph starts in vertical mode.
    QuitVMode,
    /// Horizontal material: in a vertical mode it starts a paragraph and
    /// is read again (tex.web § 1090).
    Horizontal(Material),
    /// Vertical material: in horizontal mode it ends the paragraph first
    /// (tex.web § 1094 `head_for_vmode`).
    Vertical(Material),
    /// `make_box` (tex.web § 1071): `\hbox`, `\vbox`, `\vtop`, `\copy`,
    /// `\lastbox`, `\vsplit`, and `vcenter`'s `\vcenter` (§ 1167).
    Box(BoxCmd),
    /// `\raise`, `\lower` (`vertical`), `\moveleft`, `\moveright`: a
    /// dimension, then a box (tex.web § 1073).
    Shift { vertical: bool },
    /// `\leaders`, `\cleaders`, `\xleaders`: a box or rule, then glue
    /// (tex.web § 1078).
    Leaders,
    /// `\shipout⟨box⟩` (tex.web § 1073).
    ShipOut,
    /// `\insert⟨number⟩{…}` and `\vadjust{…}` (tex.web § 1097).
    Insert { vadjust: bool },
    /// `\noalign{…}` (tex.web § 785).
    NoAlign,
    /// `\mathchoice{…}{…}{…}{…}` (tex.web § 1172).
    MathChoice,
    /// `\eqno`, `\leqno` (tex.web § 1140).
    EqNo,
    /// `\left` (tex.web § 1191).
    Left,
}

/// What a [`ModeCmd::Horizontal`] or [`ModeCmd::Vertical`] command scans.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Material {
    /// `\char⟨number⟩`.
    Char,
    /// `\unhbox`, `\unhcopy`, `\unvbox`, `\unvcopy`: a register number;
    /// `take` for the forms that leave it void.
    Unpackage { take: bool },
    /// `\pagediscards`, `\splitdiscards`.
    Discards,
    /// `\vrule`, `\hrule`: a rule specification.
    Rule,
    /// `\hskip`, `\vskip`: glue.
    Glue,
    /// `\hfil`, `\hfill`, `\hss`, `\hfilneg` and the vertical ones.
    Fil,
    /// `\discretionary{…}{…}{…}` (tex.web § 1117).
    Discretionary,
    /// `\-`.
    Hyphen,
    /// Control space.
    Space,
    /// `\noboundary`.
    NoBoundary,
    /// `\valign`, `\halign`.
    Align,
}

/// The `make_box` commands (tex.web § 1071).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum BoxCmd {
    HBox,
    VBox,
    VTop,
    VCenter,
    Copy,
    LastBox,
    VSplit,
}

impl ModeCmd {
    /// What hyperref drops with the command from a PDF string (see
    /// [`pdf_string_skip`]).
    pub fn scan(self) -> Typeset {
        match self {
            ModeCmd::Horizontal(Material::Glue) | ModeCmd::Vertical(Material::Glue) => Typeset::Glue,
            ModeCmd::Shift { .. } => Typeset::Dimen,
            ModeCmd::Left => Typeset::Delimiter,
            _ => Typeset::Plain,
        }
    }
}

/// The tags a definition can carry, as `category` gives them.
pub fn definition_tag(tag: &str) -> Option<&'static str> {
    ["macro", "alias", "environment", "register", "counter", "length", "switch", "option", "key", "variable"]
        .into_iter()
        .find(|known| *known == tag)
}

pub fn category(p: Primitive) -> &'static str {
    use Primitive as P;
    match p {
        P::Def { .. } => "macro",
        P::Let { .. } => "alias",
        P::Allocate(_) | P::RegisterDef(_) => "register",
        _ => "macro",
    }
}

/// Where a command is documented, by where it comes from: an engine
/// primitive lives in tex.web, an eTeX extension in the eTeX manual, expl3
/// names in interface3, and the LaTeX commands satex models in source2e.
pub fn reference(p: Primitive) -> &'static str {
    use Primitive as P;
    match p {
        // `\ifx` compares meanings, which is the chapter that defines them.
        P::If(Cond::IfX) => "The TeXbook, chapter 20",
        P::Lua(_) => "The LuaTeX reference manual",
        P::Expr(_)
        | P::Reinject
        | P::Expanded
        | P::Unless
        | P::Prefix(Prefix::Protected)
        | P::If(Cond::Defined | Cond::Csname) => "The eTeX manual",
        P::Docstrip(_) => "The docstrip manual",
        _ => "tex.web",
    }
}

/// What a command consumes, as `min` and `max` arguments; `None` for a max
/// means it depends on what follows, as `\halign` and `\csname` do.  A
/// command that dispatches on a star or the next character takes one more
/// than its body shows (usrguide "Commands with *-forms").
pub fn takes(p: Primitive) -> (u8, Option<u8>) {
    use Primitive as P;
    match p {
        // `\input ⟨file name⟩`.
        P::Load(_) => (1, Some(1)),
        P::Def { .. } => (2, None),
        P::Let { .. } => (2, Some(2)),
        P::Message { .. } => (1, Some(1)),
        P::Write | P::Expr(_) | P::Lua(LuaOp::Direct | LuaOp::Late | LuaOp::Escape) => (1, Some(1)),
        P::Docstrip(DocstripOp::Generate | DocstripOp::UseDir | DocstripOp::BatchInput) => (1, Some(1)),
        P::Docstrip(DocstripOp::GenerateFile) => (3, Some(3)),
        P::Docstrip(DocstripOp::Setting { arguments }) => (arguments, Some(arguments)),
        P::Text(_) | P::Expanded => (1, Some(1)),
        _ => (0, Some(0)),
    }
}

/// Whether the gullet expands this primitive (tex.web § 366).
/// The commands the gullet expands (tex.web § 366): conditionals, `\csname`,
/// the convert commands, `\expandafter`, `\noexpand` and the eTeX
/// re-injections.  Everything else is a command the stomach obeys, which an
/// `\edef` body keeps as it stands rather than running.
pub fn gullet(p: Primitive) -> bool {
    use Primitive as P;
    matches!(
        p,
        P::If(_)
            | P::Else
            | P::Or
            | P::Fi
            | P::Unless
            | P::Csname
            | P::BeginCsname
            | P::LastNamedCs
            | P::Text(_)
            | P::Reinject
            | P::Expanded
            | P::ScanTokens { .. }
            | P::ExpandAfter
            | P::NoExpand
            | P::Use(_)
            | P::Convert(_)
    )
}

/// Whether `p` is a primitive of the engine rather than a command satex
/// models: the kernel is free to redefine the former, as latex.ltx does
/// `\end`, and satex must not put it back.
pub fn engine_primitive(p: Primitive) -> bool {
    matches!(reference(p), "tex.web" | "The TeXbook, chapter 20" | "The eTeX manual" | "The LuaTeX reference manual")
}

pub fn expandable(p: Primitive) -> bool {
    use Primitive as P;
    if let P::LuaCall { protected, .. } = p {
        return !protected;
    }
    !matches!(
        p,
        P::Def { .. }
            | P::Let { .. }
            | P::Prefix(_)
            | P::Endcsname
            | P::BeginGroup
            | P::EndGroup
            | P::Defer(_)
            | P::Register(_)
            | P::RegisterDef(_)
            | P::Arith(_)
            | P::IntegerParameter
            | P::DimenParameter
            | P::CatcodeAssign
            | P::CharCode(_)
            // The TeXbook, ch. 7: `\uppercase` and `\lowercase` are not
            // expandable, so an `\edef` body keeps them literally.
            | P::CaseShift(_)
            | P::Relax
            | P::Typeset(_)
            | P::Expr(_)
            | P::Lua(
                LuaOp::Late | LuaOp::InitCatcodes | LuaOp::SaveCatcodes | LuaOp::SelectCatcodes
            )
            | P::Docstrip(_)
            | P::Accent
            | P::Pdf(_)
            | P::End
            | P::Unmodeled
            | P::AlignMark
            | P::AlignTab
            | P::GlueParameter { .. }
            | P::TokensParameter
            | P::LastItem(_)
            | P::Special(_)
            | P::FontParam(_)
            | P::FontIdent(_)
            | P::Command(_)
            // tex.web § 1210: `\font`, `\openin`, `\closein`, `\read`,
            // `\write` and `\message` are commands of the stomach.
            | P::FontDef
            | P::OpenIn
            | P::CloseIn
            | P::Read
            | P::Write
            | P::Message { .. }
            | P::Mode(_)
            | P::IgnoreSpaces
    )
}

/// What `\pdfstringdef` makes of a primitive the gullet left standing:
/// `None` when it contributes text, otherwise the value it scans, which is
/// dropped with it.  hyperref removes every unexpandable token from the
/// string it builds and warns about it, scanning past the number, dimension
/// or glue of `\penalty`, `\kern` and `\hskip` (hyperref.sty,
/// `\HyPsd@CheckCatcodes`).  A box's contents are tokens of their own and
/// stay.
pub fn pdf_string_skip(p: Primitive) -> Option<Typeset> {
    match p {
        Primitive::Typeset(scan) => Some(scan),
        Primitive::Mode(cmd) => Some(cmd.scan()),
        Primitive::Accent => Some(Typeset::Number),
        _ if !expandable(p) => Some(Typeset::Plain),
        _ => None,
    }
}

/// Whether a primitive is one of TeX's assignments — a case of
/// `prefixed_command` (tex.web § 1211) rather than of `main_control`.  Those
/// are the commands `\afterassignment` waits for; `\afterassignment` itself
/// is not one, which is why a second one overwrites the first token instead
/// of releasing it.
pub fn assignment(p: Primitive) -> bool {
    use Primitive as P;
    matches!(
        p,
        P::Def { .. }
            | P::Let { .. }
            | P::Register(_)
            | P::RegisterDef(_)
            | P::Arith(_)
            | P::IntegerParameter
            | P::DimenParameter
            | P::CatcodeAssign
            | P::CharCode(_)
            | P::FontDef
            | P::Read
            | P::GlueParameter { .. }
            | P::TokensParameter
            | P::Special(_)
            | P::FontParam(_)
            | P::FontIdent(_)
            | P::Command(Syntax::SetBox | Syntax::FamilyFont | Syntax::CopyFont { .. } | Syntax::ReadLine)
    )
}

pub fn initial_meanings(it: &mut Interner, engine: Engine) -> HashMap<Sym, Meaning> {
    use Primitive as P;
    let mut table: HashMap<Sym, Meaning> = HashMap::with_capacity(512);
    let mut put = |it: &mut Interner, name: &str, p: Primitive| {
        let s = it.intern(name);
        let previous = table.insert(s, Meaning::Primitive(p));
        debug_assert!(previous.is_none(), "{name} is registered twice, the later entry wins");
    };

    put(it, "def", P::Def { global: false, expand: false });
    put(it, "gdef", P::Def { global: true, expand: false });
    put(it, "edef", P::Def { global: false, expand: true });
    put(it, "xdef", P::Def { global: true, expand: true });
    put(it, "let", P::Let { global: false, future: false });
    put(it, "futurelet", P::Let { global: false, future: true });
    put(it, "global", P::Prefix(Prefix::Global));
    put(it, "long", P::Prefix(Prefix::Long));
    put(it, "outer", P::Prefix(Prefix::Outer));
    put(it, "protected", P::Prefix(Prefix::Protected));
    put(it, "immediate", P::Prefix(Prefix::Immediate));
    put(it, "begingroup", P::BeginGroup);
    put(it, "endgroup", P::EndGroup);
    put(it, "aftergroup", P::Defer(Deferred::Group));
    put(it, "afterassignment", P::Defer(Deferred::Assignment));

    put(it, "expandafter", P::ExpandAfter);
    put(it, "noexpand", P::NoExpand);
    put(it, "csname", P::Csname);
    put(it, "endcsname", P::Endcsname);
    for (n, kind) in [
        ("string", TextOf::String),
        ("meaning", TextOf::Meaning),
        ("the", TextOf::The),
        ("number", TextOf::Number),
        ("romannumeral", TextOf::Roman),
        ("detokenize", TextOf::Detokenize),
        ("jobname", TextOf::JobName),
    ] {
        put(it, n, P::Text(kind));
    }
    put(it, "unexpanded", P::Reinject);
    put(it, "scantokens", P::ScanTokens { text: false });

    put(it, "iftrue", P::If(Cond::True));
    put(it, "iffalse", P::If(Cond::False));
    put(it, "ifx", P::If(Cond::IfX));
    put(it, "ifdefined", P::If(Cond::Defined));
    put(it, "ifcsname", P::If(Cond::Csname));
    put(it, "ifnum", P::If(Cond::Num));
    put(it, "ifdim", P::If(Cond::Dim));
    put(it, "ifodd", P::If(Cond::Odd));
    put(it, "if", P::If(Cond::Chars));
    put(it, "ifcat", P::If(Cond::Catcodes));
    put(it, "ifcase", P::If(Cond::Case));
    put(it, "ifvmode", P::If(Cond::VMode));
    put(it, "ifhmode", P::If(Cond::HMode));
    put(it, "ifinner", P::If(Cond::InnerMode));
    put(it, "ifmmode", P::If(Cond::MathMode));
    for n in ["ifvoid", "ifhbox", "ifvbox"] {
        put(it, n, P::If(Cond::Box));
    }
    put(it, "ifeof", P::If(Cond::Eof));
    put(it, "write", P::Write);
    put(it, "font", P::FontDef);
    put(it, "openin", P::OpenIn);
    put(it, "closein", P::CloseIn);
    put(it, "read", P::Read);
    put(it, "else", P::Else);
    put(it, "or", P::Or);
    put(it, "fi", P::Fi);
    put(it, "unless", P::Unless);

    for (n, k) in [
        ("count", RegKind::Count),
        ("dimen", RegKind::Dimen),
        ("skip", RegKind::Skip),
        ("muskip", RegKind::MuSkip),
        ("toks", RegKind::Toks),
        ("box", RegKind::Box),
    ] {
        put(it, n, P::Register(k));
    }
    for (n, k) in [
        ("countdef", RegKind::Count),
        ("dimendef", RegKind::Dimen),
        ("skipdef", RegKind::Skip),
        ("toksdef", RegKind::Toks),
        ("chardef", RegKind::Char),
        ("mathchardef", RegKind::MathChar),
    ] {
        put(it, n, P::RegisterDef(k));
    }
    put(it, "advance", P::Arith(Arith::Advance));
    put(it, "multiply", P::Arith(Arith::Multiply));
    put(it, "divide", P::Arith(Arith::Divide));
    put(it, "catcode", P::CatcodeAssign);
    for (n, table) in [
        ("lccode", CodeTable::Lower),
        ("uccode", CodeTable::Upper),
        ("sfcode", CodeTable::Space),
        ("mathcode", CodeTable::Math),
        ("delcode", CodeTable::Delimiter),
    ] {
        put(it, n, P::CharCode(table));
    }
    // Integer parameters of TeX82 (TeXbook, appendix B).
    for n in [
        "adjdemerits",
        "binoppenalty",
        "brokenpenalty",
        "deadcycles",
        "defaulthyphenchar",
        "defaultskewchar",
        "delimiterfactor",
        "displaywidowpenalty",
        "doublehyphendemerits",
        "exhyphenpenalty",
        "fam",
        "finalhyphendemerits",
        "floatingpenalty",
        "globaldefs",
        "hangafter",
        "holdinginserts",
        "insertpenalties",
        "lefthyphenmin",
        "righthyphenmin",
        "linepenalty",
        "looseness",
        "mag",
        "maxdeadcycles",
        "outputpenalty",
        "pausing",
        "postdisplaypenalty",
        "predisplaypenalty",
        "pretolerance",
        "prevgraf",
        "relpenalty",
        "showboxbreadth",
        "showboxdepth",
        "spacefactor",
        "tracingstacklevels",
        "uchyph",
    ] {
        put(it, n, P::IntegerParameter);
    }
    for n in [
        "boxmaxdepth",
        "delimitershortfall",
        "displayindent",
        "displaywidth",
        "emergencystretch",
        "hangindent",
        "hfuzz",
        "vfuzz",
        "hoffset",
        "voffset",
        "lineskiplimit",
        "mathsurround",
        "maxdepth",
        "nulldelimiterspace",
        "overfullrule",
        "pagegoal",
        "pagetotal",
        "pagestretch",
        "pagefilstretch",
        "pagefillstretch",
        "pagefilllstretch",
        "pageshrink",
        "pagedepth",
        "predisplaysize",
        "prevdepth",
        "scriptspace",
        "splitmaxdepth",
    ] {
        put(it, n, P::DimenParameter);
    }
    for n in [
        "escapechar",
        "endlinechar",
        "newlinechar",
        "tolerance",
        "hbadness",
        "vbadness",
        "time",
        "day",
        "month",
        "year",
        "language",
        "hyphenpenalty",
        "clubpenalty",
        "widowpenalty",
        "interlinepenalty",
    ] {
        put(it, n, P::IntegerParameter);
    }
    for n in ["parindent", "hsize", "vsize"] {
        put(it, n, P::DimenParameter);
    }

    // tex.web § 537; latex.ltx keeps it as `\@@input` and loads packages,
    // classes and documents through it.
    put(it, "input", P::Load(LoadKind::Input));
    put(it, "endinput", P::Endinput);

    put(it, "end", P::End);
    // tex.web § 1279; the kernel's `\PackageWarning`, `\GenericError` and
    // `\typeout` are its own code, which ends in these or in an
    // `\immediate\write` to a stream that is not open.
    put(it, "message", P::Message { error: false });
    put(it, "errmessage", P::Message { error: true });

    // Boxes, glue and penalties (The TeXbook, chs. 11-14), by what each one
    // scans before the material it sets.
    for (n, scan) in [
        ("kern", Typeset::Dimen),
        ("mkern", Typeset::Dimen),
        ("mskip", Typeset::Glue),
        ("penalty", Typeset::Number),
        ("right", Typeset::Delimiter),
        ("middle", Typeset::Delimiter),
    ] {
        put(it, n, P::Typeset(scan));
    }
    for n in ["unpenalty", "unkern", "unskip"] {
        put(it, n, P::Typeset(Typeset::Plain));
    }

    // What follows was probed against the TeX Live 2026 binaries: a name is
    // registered for an engine only where `\meaning` there reports the
    // primitive itself.
    if engine.has_pdftex_extensions() {
        // pdftex manual, "Primitives" — the unprefixed extensions XeTeX and
        // LuaTeX inherited with the rest of pdfTeX's front end.
        put(it, "partokenname", P::Command(Syntax::Token));
        for n in ["synctex", "showstream"] {
            put(it, n, P::IntegerParameter);
        }
        // pdftex manual, "\expanded": the argument is expanded and the
        // result replaces it.  Leaving it unmodeled turns the braces into a
        // group, and the kernel's `\use:e` idiom then leaks one open group
        // per use.
        put(it, "expanded", P::Expanded);
    }
    if engine.has_pdftex() {
        // `\pdfoutput` decides `\ifpdf` and every driver test built on it;
        // its value comes from the output plugin (pdfTeX manual).
        put(it, "pdfoutput", P::IntegerParameter);
        // pdfTeX's own integer and dimension parameters, and the kerning
        // codes only it has (pdftex manual, "Primitives").
        for n in [
            "pdfmajorversion",
            "pdfimageresolution",
            "pdfpkresolution",
            "pdfimagehicolor",
            "pdfimageapplygamma",
            "pdfgamma",
            "pdfimagegamma",
            "pdfinfoomitdate",
            "pdfomitcharset",
            "pdfsuppressptexinfo",
            "pdfptexuseunderscore",
            "pdfappendkern",
            "pdfprependkern",
            "pdfpagebox",
            "pdftracingfonts",
            "pdfinclusioncopyfonts",
        ] {
            put(it, n, P::IntegerParameter);
        }
        // pdfTeX manual: `\pdfnobuiltintounicode` takes a font, not a
        // number, so it belongs with `\pdfnoligatures` and the other
        // per-font switches, not with the integer parameters above.
        put(it, "pdfnobuiltintounicode", P::Command(Syntax::Font));
        // pdfTeX manual: these are read-only "last" integers, not
        // parameters; an assignment to one is an engine error, which
        // leaves the "=…" it wrote unread and free to start a paragraph,
        // as satex's own read-only `LastItem`s already do. The concrete
        // value (0 while the run has made no annotations, objects, forms
        // or links) is genuinely unknowable without the PDF writer satex
        // does not run, so it widens rather than answering a specific
        // number.
        for n in [
            "pdfretval",
            "pdflastannot",
            "pdflastobj",
            "pdflastxform",
            "pdflastximage",
            "pdflastximagepages",
            "pdflastlink",
        ] {
            put(it, n, P::LastItem(LastItem::LastXPos));
        }
        for n in [
            "pdfdestmargin",
            "pdflinkmargin",
            "pdfthreadmargin",
            "pdfpxdimen",
            "pdfignoreddimen",
            "pdfeachlineheight",
            "pdfeachlinedepth",
            "pdffirstlineheight",
            "pdflastlinedepth",
        ] {
            put(it, n, P::DimenParameter);
        }
        put(it, "pdfximagebbox", P::Convert(Convert::ImageBBox));
    }
    matches!(engine, Engine::PdfTeX | Engine::XeTeX);
    if engine.has_unicode_math() {
        // The Unicode-math, randomness and `\primitive` extensions XeTeX and
        // LuaTeX share (XeTeX reference; LuaTeX manual, "Math").
        // XeTeX reference and LuaTeX manual, "Math": 21-bit math codes,
        // class, family and slot given apart or packed in one number.
        put(it, "Umathcode", P::Special(Special::UCode { math: true, num: false }));
        put(it, "Umathcodenum", P::Special(Special::UCode { math: true, num: true }));
        put(it, "Udelcode", P::Special(Special::UCode { math: false, num: false }));
        put(it, "Udelcodenum", P::Special(Special::UCode { math: false, num: true }));
        put(it, "Umathchardef", P::Command(Syntax::UMathCharDef { num: false }));
        put(it, "Umathcharnumdef", P::Command(Syntax::UMathCharDef { num: true }));
        put(it, "Umathchar", P::Command(Syntax::Numbers(3)));
        put(it, "Umathcharnum", P::Command(Syntax::Numbers(1)));
        put(it, "Udelimiter", P::Command(Syntax::Numbers(3)));
        put(it, "Uradical", P::Command(Syntax::Numbers(2)));
        put(it, "Umathaccent", P::Command(Syntax::MathAccent));
        put(it, "Uchar", P::Convert(Convert::Uchar { catcode: false }));
        put(it, "ifprimitive", P::If(Cond::Primitive));
        put(it, "primitive", P::Convert(Convert::Primitive));
        put(it, "normaldeviate", P::Convert(Convert::NormalDeviate));
        put(it, "uniformdeviate", P::Convert(Convert::UniformDeviate));
        put(it, "randomseed", P::LastItem(LastItem::RandomSeed));
        put(it, "setrandomseed", P::Command(Syntax::Number));
        put(it, "suppressfontnotfounderror", P::IntegerParameter);
    }
    if engine.has_xetex() {
        // XeTeX reference, "XeTeX primitives": the unprefixed forms of
        // pdfTeX's string and file utilities, font and OpenType
        // introspection, Unicode line breaking and picture inclusion.
        put(it, "Ucharcat", P::Convert(Convert::Uchar { catcode: true }));
        put(it, "strcmp", P::Convert(Convert::StrCmp));
        put(it, "filesize", P::Convert(Convert::FileSize));
        put(it, "filemoddate", P::Convert(Convert::FileModDate));
        put(it, "filedump", P::Convert(Convert::FileDump));
        put(it, "mdfivesum", P::Convert(Convert::MdFiveSum));
        put(it, "creationdate", P::Convert(Convert::CreationDate));
        put(it, "elapsedtime", P::LastItem(LastItem::ElapsedTime));
        put(it, "shellescape", P::LastItem(LastItem::ShellEscape));
        put(it, "resettimer", P::Command(Syntax::Nothing));
        put(it, "XeTeXversion", P::LastItem(LastItem::XetexVersion));
        put(it, "XeTeXrevision", P::Convert(Convert::XetexRevision));
        for n in [
            "XeTeXdashbreakstate",
            "XeTeXgenerateactualtext",
            "XeTeXhyphenatablelength",
            "XeTeXinputnormalization",
            "XeTeXinterchartokenstate",
            "XeTeXinterwordspaceshaping",
            "XeTeXlinebreakpenalty",
            "XeTeXprotrudechars",
            "XeTeXtracingfonts",
            "XeTeXupwardsmode",
            "XeTeXuseglyphmetrics",
        ] {
            put(it, n, P::IntegerParameter);
        }
        put(it, "XeTeXlinebreakskip", P::GlueParameter { mu: false });
        for n in ["XeTeXlinebreaklocale", "XeTeXinputencoding", "XeTeXdefaultencoding"] {
            put(it, n, P::Command(Syntax::FileName));
        }
        put(it, "XeTeXpicfile", P::Command(Syntax::PicFile));
        put(it, "XeTeXpdffile", P::Command(Syntax::PicFile));
        put(it, "XeTeXpdfpagecount", P::LastItem(LastItem::PdfPageCount));
        put(it, "XeTeXglyph", P::Mode(ModeCmd::Horizontal(Material::Char)));
        put(it, "XeTeXcharclass", P::Special(Special::CharClass));
        put(it, "XeTeXinterchartoks", P::Special(Special::InterCharToks));
        use XeQuery as Q;
        for (n, q) in [
            ("XeTeXfonttype", Q::FontType),
            ("XeTeXfirstfontchar", Q::FirstFontChar),
            ("XeTeXlastfontchar", Q::LastFontChar),
            ("XeTeXcountglyphs", Q::CountGlyphs),
            ("XeTeXcountvariations", Q::CountVariations),
            ("XeTeXcountfeatures", Q::CountFeatures),
            ("XeTeXvariation", Q::Variation),
            ("XeTeXvariationmin", Q::VariationMin),
            ("XeTeXvariationmax", Q::VariationMax),
            ("XeTeXvariationdefault", Q::VariationDefault),
            ("XeTeXfeaturecode", Q::FeatureCode),
            ("XeTeXcountselectors", Q::CountSelectors),
            ("XeTeXselectorcode", Q::SelectorCode),
            ("XeTeXisexclusivefeature", Q::IsExclusiveFeature),
            ("XeTeXisdefaultselector", Q::IsDefaultSelector),
            ("XeTeXfindvariationbyname", Q::FindVariationByName),
            ("XeTeXfindfeaturebyname", Q::FindFeatureByName),
            ("XeTeXfindselectorbyname", Q::FindSelectorByName),
            ("XeTeXOTcountscripts", Q::OtCountScripts),
            ("XeTeXOTcountlanguages", Q::OtCountLanguages),
            ("XeTeXOTcountfeatures", Q::OtCountFeatures),
            ("XeTeXOTscripttag", Q::OtScriptTag),
            ("XeTeXOTlanguagetag", Q::OtLanguageTag),
            ("XeTeXOTfeaturetag", Q::OtFeatureTag),
            ("XeTeXcharglyph", Q::CharGlyph),
            ("XeTeXglyphindex", Q::GlyphIndex),
            ("XeTeXglyphbounds", Q::GlyphBounds),
        ] {
            put(it, n, P::LastItem(LastItem::Xetex(q)));
        }
        for (n, q) in [
            ("XeTeXvariationname", Q::VariationName),
            ("XeTeXfeaturename", Q::FeatureName),
            ("XeTeXglyphname", Q::GlyphName),
            ("XeTeXselectorname", Q::SelectorName),
        ] {
            put(it, n, P::Convert(Convert::Xetex(q)));
        }
    }
    if engine.has_luatex() {
        // LuaTeX replaced `\pdfoutput` with `\outputmode` (LuaTeX manual,
        // "Changes from pdfTeX").
        put(it, "outputmode", P::IntegerParameter);
        // `\luatexversion` is the version times one hundred, which is what
        // `ltluatex` and `iftex` test (LuaTeX manual, "Version information").
        // It is read-only, like `\pdftexversion`, but its value varies across
        // installs, so it is not a constant: `last_item` answers it from the
        // real `luatex --version` probed for this run.
        put(it, "luatexversion", P::LastItem(LastItem::LuatexVersion));
        // `\directlua{…}` and `\latelua{…}` take a general text holding Lua,
        // not TeX (LuaTeX manual, "Lua related primitives"), and catcode
        // tables are how Lua and TeX code agree on a regime ("Catcode tables").
        for (n, op) in [
            ("directlua", LuaOp::Direct),
            ("latelua", LuaOp::Late),
            ("luaescapestring", LuaOp::Escape),
            ("initcatcodetable", LuaOp::InitCatcodes),
            ("savecatcodetable", LuaOp::SaveCatcodes),
            ("catcodetable", LuaOp::SelectCatcodes),
        ] {
            put(it, n, P::Lua(op));
        }
        // Each of these takes a ⟨number⟩ naming a Lua function or a bytecode
        // register, and runs it at once (LuaTeX manual, "Lua related
        // primitives"): a command, not a parameter with a stored value, and
        // an assignment to one (`\luafunction=3`) is not the engine's
        // syntax at all, so satex must not accept it either.
        for n in ["luafunction", "lateluafunction", "luafunctioncall", "luabytecode", "luabytecodecall"] {
            put(it, n, P::Command(Syntax::Number));
        }
        // `\attribute` is LuaTeX's own numbered register file (LuaTeX
        // manual, "Attributes"), disjoint from `\count`, and `\attributedef`
        // names one the way `\countdef` names a `\count`; `\luadef` names a
        // Lua function the way `\chardef` names a character. satex has no
        // register kind of its own for any of the three (`RegKind` lives in
        // `src/tex.rs`, out of this module's reach), so modeling them as
        // `\count`-alike would keep aliasing `\attribute255` onto
        // `\count255`, which is the wrong classification the manual and
        // `\ifx` both reject. Left unmodeled instead: satex widens rather
        // than answering confidently for the wrong register.
        put(it, "attribute", P::Unmodeled);
        put(it, "attributedef", P::Unmodeled);
        put(it, "luadef", P::Unmodeled);
        // `\csstring` is `\string` without the escape character, and
        // `\scantextokens` is `\scantokens` without the file wrapper.
        put(it, "csstring", P::Text(TextOf::String));
        put(it, "scantextokens", P::ScanTokens { text: true });
        // LuaTeX manual, "TeX and LuaTeX primitives" — node attributes,
        // directions, Unicode math, hyphenation control and the rest.
        for n in [
            "Udelimiterover",
            "Udelimiterunder",
            "Uhextensible",
            "Uleft",
            "Umathaxis",
            "Umathbinbinspacing",
            "Umathbinclosespacing",
            "Umathbininnerspacing",
            "Umathbinopenspacing",
            "Umathbinopspacing",
            "Umathbinordspacing",
            "Umathbinpunctspacing",
            "Umathbinrelspacing",
            "Umathcharclass",
            "Umathcharfam",
            "Umathcharslot",
            "Umathclosebinspacing",
            "Umathcloseclosespacing",
            "Umathcloseinnerspacing",
            "Umathcloseopenspacing",
            "Umathcloseopspacing",
            "Umathcloseordspacing",
            "Umathclosepunctspacing",
            "Umathcloserelspacing",
            "Umathconnectoroverlapmin",
            "Umathfractiondelsize",
            "Umathfractiondenomdown",
            "Umathfractiondenomvgap",
            "Umathfractionnumup",
            "Umathfractionnumvgap",
            "Umathfractionrule",
            "Umathinnerbinspacing",
            "Umathinnerclosespacing",
            "Umathinnerinnerspacing",
            "Umathinneropenspacing",
            "Umathinneropspacing",
            "Umathinnerordspacing",
            "Umathinnerpunctspacing",
            "Umathinnerrelspacing",
            "Umathlimitabovebgap",
            "Umathlimitabovekern",
            "Umathlimitabovevgap",
            "Umathlimitbelowbgap",
            "Umathlimitbelowkern",
            "Umathlimitbelowvgap",
            "Umathnolimitsubfactor",
            "Umathnolimitsupfactor",
            "Umathopbinspacing",
            "Umathopclosespacing",
            "Umathopenbinspacing",
            "Umathopenclosespacing",
            "Umathopeninnerspacing",
            "Umathopenopenspacing",
            "Umathopenopspacing",
            "Umathopenordspacing",
            "Umathopenpunctspacing",
            "Umathopenrelspacing",
            "Umathoperatorsize",
            "Umathopinnerspacing",
            "Umathopopenspacing",
            "Umathopopspacing",
            "Umathopordspacing",
            "Umathoppunctspacing",
            "Umathoprelspacing",
            "Umathordbinspacing",
            "Umathordclosespacing",
            "Umathordinnerspacing",
            "Umathordopenspacing",
            "Umathordopspacing",
            "Umathordordspacing",
            "Umathordpunctspacing",
            "Umathordrelspacing",
            "Umathoverbarkern",
            "Umathoverbarrule",
            "Umathoverbarvgap",
            "Umathoverdelimiterbgap",
            "Umathoverdelimitervgap",
            "Umathpunctbinspacing",
            "Umathpunctclosespacing",
            "Umathpunctinnerspacing",
            "Umathpunctopenspacing",
            "Umathpunctopspacing",
            "Umathpunctordspacing",
            "Umathpunctpunctspacing",
            "Umathpunctrelspacing",
            "Umathquad",
            "Umathradicaldegreeafter",
            "Umathradicaldegreebefore",
            "Umathradicaldegreeraise",
            "Umathradicalkern",
            "Umathradicalrule",
            "Umathradicalvgap",
            "Umathrelbinspacing",
            "Umathrelclosespacing",
            "Umathrelinnerspacing",
            "Umathrelopenspacing",
            "Umathrelopspacing",
            "Umathrelordspacing",
            "Umathrelpunctspacing",
            "Umathrelrelspacing",
            "Umathskewedfractionhgap",
            "Umathskewedfractionvgap",
            "Umathspaceafterscript",
            "Umathstackdenomdown",
            "Umathstacknumup",
            "Umathstackvgap",
            "Umathsubshiftdown",
            "Umathsubshiftdrop",
            "Umathsubsupshiftdown",
            "Umathsubsupvgap",
            "Umathsubtopmax",
            "Umathsupbottommin",
            "Umathsupshiftdrop",
            "Umathsupshiftup",
            "Umathsupsubbottommax",
            "Umathunderbarkern",
            "Umathunderbarrule",
            "Umathunderbarvgap",
            "Umathunderdelimiterbgap",
            "Umathunderdelimitervgap",
            "Umiddle",
            "Unosubscript",
            "Unosuperscript",
            "Uoverdelimiter",
            "Uright",
            "Uroot",
            "Uskewed",
            "Uskewedwithdelims",
            "Ustack",
            "Ustartdisplaymath",
            "Ustartmath",
            "Ustopdisplaymath",
            "Ustopmath",
            "Usubscript",
            "Usuperscript",
            "Uunderdelimiter",
            "Uvextensible",
            "automaticdiscretionary",
            "bodydir",
            "bodydirection",
            "boundary",
            "boxdir",
            "boxdirection",
            "clearmarks",
            "crampeddisplaystyle",
            "crampedscriptscriptstyle",
            "crampedscriptstyle",
            "crampedtextstyle",
            "deferred",
            "dviextension",
            "dvifeedback",
            "dvivariable",
            "eTeXVersion",
            "eTeXglueshrinkorder",
            "eTeXgluestretchorder",
            "eTeXminorversion",
            "endlocalcontrol",
            "etoksapp",
            "etokspre",
            "explicitdiscretionary",
            "formatname",
            "gleaders",
            "gtoksapp",
            "gtokspre",
            "hjcode",
            "hpack",
            "hyphenationmin",
            "ifcondition",
            "immediateassigned",
            "immediateassignment",
            "insertht",
            "lastsavedboxresourceindex",
            "lastsavedimageresourceindex",
            "lastsavedimageresourcepages",
            "lastxpos",
            "lastypos",
            "leftghost",
            "letcharcode",
            "linedir",
            "linedirection",
            "localleftbox",
            "localrightbox",
            "luatexbanner",
            "luatexrevision",
            "mathdir",
            "mathdirection",
            "mathoption",
            "mathstyle",
            "nohrule",
            "novrule",
            "pagedir",
            "pagedirection",
            "pardir",
            "pardirection",
            "pdffeedback",
            "pdfvariable",
            "protrusionboundary",
            "rightghost",
            "saveboxresource",
            "savepos",
            "textdir",
            "textdirection",
            "toksapp",
            "tokspre",
            "tpack",
            "useboxresource",
            "useimageresource",
            "vpack",
            "wordboundary",
            "xtoksapp",
            "xtokspre",
        ] {
            put(it, n, P::Unmodeled);
        }
        put(it, "alignmark", P::AlignMark);
        put(it, "aligntab", P::AlignTab);
        // LuaTeX manual, "Images": `\saveimageresource` is pdfTeX's
        // `\pdfximage` under its engine-neutral name, with the same syntax.
        // LuaTeX manual, "Fonts": fonts by their number, and pdfTeX's
        // `\pdfcopyfont`, `\pdffontexpand`, `\pdfnoligatures` renamed.
        // pdfTeX's `\ifpdfabsnum` and `\ifpdfabsdim` renamed.
        put(it, "ifabsnum", P::If(Cond::AbsNum));
        put(it, "ifabsdim", P::If(Cond::AbsDim));
        // LuaTeX manual, "TeX and LuaTeX primitives": its own integer,
        // dimension and glue parameters.  The range-checked and read-only
        // ones among them are in [`limit`], which decides what a write does.
        for n in [
            "automatichyphenpenalty",
            "explicithyphenpenalty",
            "exceptionpenalty",
            "hyphenpenaltymode",
            "automatichyphenmode",
            "compoundhyphenmode",
            "breakafterdirmode",
            "discretionaryligaturemode",
            "draftmode",
            "firstvalidlanguage",
            "fixupboxesmode",
            "glyphdimensionsmode",
            "hyphenationbounds",
            "localbrokenpenalty",
            "localinterlinepenalty",
            "mathdefaultsmode",
            "mathdelimitersmode",
            "mathdisplayskipmode",
            "mathemptydisplaymode",
            "matheqnogapstep",
            "mathflattenmode",
            "mathitalicsmode",
            "mathnolimitsmode",
            "mathpenaltiesmode",
            "mathrulesfam",
            "mathrulesmode",
            "mathrulethicknessmode",
            "mathscriptboxmode",
            "mathscriptcharmode",
            "mathscriptsmode",
            "mathsurroundmode",
            "matheqdirmode",
            "nokerns",
            "noligs",
            "nospaces",
            "outputbox",
            "predisplaygapfactor",
            "prebinoppenalty",
            "prerelpenalty",
            "prehyphenchar",
            "posthyphenchar",
            "preexhyphenchar",
            "postexhyphenchar",
            "exhyphenchar",
            "shapemode",
            "suppressifcsnameerror",
            "suppresslongerror",
            "suppressmathparerror",
            "suppressoutererror",
            "suppressprimitiveerror",
            "variablefam",
            "tracingfonts",
            "luacopyinputnodes",
        ] {
            put(it, n, P::IntegerParameter);
        }
        for n in [
            "pageheight",
            "pagewidth",
            "pxdimen",
            "pagebottomoffset",
            "pagetopoffset",
            "pageleftoffset",
            "pagerightoffset",
        ] {
            put(it, n, P::DimenParameter);
        }
        put(it, "mathsurroundskip", P::GlueParameter { mu: false });
        put(it, "fontid", P::Convert(Convert::FontId));
        put(it, "setfontid", P::Command(Syntax::SetFontId));
        put(it, "copyfont", P::Command(Syntax::CopyFont { amount: false }));
        put(it, "expandglyphsinfont", P::Command(Syntax::FontExpand));
        put(it, "ignoreligaturesinfont", P::Command(Syntax::Font));
        // LuaTeX manual, "Font expansion" and "Character protrusion":
        // pdfTeX's `\pdfadjustspacing` and `\pdfprotrudechars` renamed.
        put(it, "adjustspacing", P::IntegerParameter);
        put(it, "protrudechars", P::IntegerParameter);
        put(it, "saveimageresource", P::Command(Syntax::PdfXImage));
        put(it, "begincsname", P::BeginCsname);
        put(it, "lastnamedcs", P::LastNamedCs);
    }

    // The engine primitives in full: each one's own kind, which says what it
    // reads and what it does (tex.web parts 49 and 30; etex_man § 3; the
    // pdfTeX manual).
    put(it, "abovedisplayshortskip", P::GlueParameter { mu: false });
    put(it, "abovedisplayskip", P::GlueParameter { mu: false });
    put(it, "baselineskip", P::GlueParameter { mu: false });
    put(it, "belowdisplayshortskip", P::GlueParameter { mu: false });
    put(it, "belowdisplayskip", P::GlueParameter { mu: false });
    put(it, "leftskip", P::GlueParameter { mu: false });
    put(it, "lineskip", P::GlueParameter { mu: false });
    put(it, "parfillskip", P::GlueParameter { mu: false });
    put(it, "parskip", P::GlueParameter { mu: false });
    put(it, "rightskip", P::GlueParameter { mu: false });
    put(it, "spaceskip", P::GlueParameter { mu: false });
    put(it, "splittopskip", P::GlueParameter { mu: false });
    put(it, "tabskip", P::GlueParameter { mu: false });
    put(it, "topskip", P::GlueParameter { mu: false });
    put(it, "xspaceskip", P::GlueParameter { mu: false });
    put(it, "thinmuskip", P::GlueParameter { mu: true });
    put(it, "medmuskip", P::GlueParameter { mu: true });
    put(it, "thickmuskip", P::GlueParameter { mu: true });
    put(it, "everypar", P::TokensParameter);
    put(it, "everymath", P::TokensParameter);
    put(it, "everydisplay", P::TokensParameter);
    put(it, "everyhbox", P::TokensParameter);
    put(it, "everyvbox", P::TokensParameter);
    put(it, "everyjob", P::TokensParameter);
    put(it, "everycr", P::TokensParameter);
    put(it, "everyeof", P::TokensParameter);
    put(it, "output", P::TokensParameter);
    put(it, "errhelp", P::TokensParameter);
    put(it, "tracingmacros", P::IntegerParameter);
    put(it, "tracingcommands", P::IntegerParameter);
    put(it, "tracingonline", P::IntegerParameter);
    put(it, "tracingassigns", P::IntegerParameter);
    put(it, "tracinggroups", P::IntegerParameter);
    put(it, "errorcontextlines", P::IntegerParameter);
    put(it, "lastlinefit", P::IntegerParameter);
    put(it, "savinghyphcodes", P::IntegerParameter);
    put(it, "savingvdiscards", P::IntegerParameter);
    put(it, "tracingstats", P::IntegerParameter);
    put(it, "tracingparagraphs", P::IntegerParameter);
    put(it, "tracingpages", P::IntegerParameter);
    put(it, "tracingoutput", P::IntegerParameter);
    put(it, "tracinglostchars", P::IntegerParameter);
    put(it, "tracingrestores", P::IntegerParameter);
    put(it, "predisplaydirection", P::IntegerParameter);
    put(it, "tracingifs", P::IntegerParameter);
    put(it, "tracingscantokens", P::IntegerParameter);
    put(it, "tracingnesting", P::IntegerParameter);
    put(it, "TeXXeTstate", P::IntegerParameter);
    put(it, "interactionmode", P::IntegerParameter);
    put(it, "wd", P::Special(Special::BoxDimen));
    put(it, "ht", P::Special(Special::BoxDimen));
    put(it, "dp", P::Special(Special::BoxDimen));
    put(it, "parshape", P::Special(Special::ParShape));
    put(it, "interlinepenalties", P::Special(Special::Penalties));
    put(it, "clubpenalties", P::Special(Special::Penalties));
    put(it, "widowpenalties", P::Special(Special::Penalties));
    put(it, "displaywidowpenalties", P::Special(Special::Penalties));
    put(it, "lastpenalty", P::LastItem(LastItem::Penalty));
    put(it, "lastkern", P::LastItem(LastItem::Kern));
    put(it, "lastskip", P::LastItem(LastItem::Skip));
    put(it, "lastnodetype", P::LastItem(LastItem::NodeType));
    put(it, "badness", P::LastItem(LastItem::Badness));
    put(it, "inputlineno", P::LastItem(LastItem::InputLineNo));
    put(it, "eTeXversion", P::LastItem(LastItem::EtexVersion));
    put(it, "currentgrouplevel", P::LastItem(LastItem::GroupLevel));
    put(it, "currentgrouptype", P::LastItem(LastItem::GroupType));
    put(it, "currentiflevel", P::LastItem(LastItem::IfLevel));
    put(it, "currentiftype", P::LastItem(LastItem::IfType));
    put(it, "currentifbranch", P::LastItem(LastItem::IfBranch));
    put(it, "gluestretch", P::LastItem(LastItem::GlueStretch));
    put(it, "glueshrink", P::LastItem(LastItem::GlueShrink));
    put(it, "gluestretchorder", P::LastItem(LastItem::GlueStretchOrder));
    put(it, "glueshrinkorder", P::LastItem(LastItem::GlueShrinkOrder));
    put(it, "gluetomu", P::LastItem(LastItem::GlueToMu));
    put(it, "mutoglue", P::LastItem(LastItem::MuToGlue));
    put(it, "fontcharwd", P::LastItem(LastItem::FontCharWd));
    put(it, "fontcharht", P::LastItem(LastItem::FontCharHt));
    put(it, "fontchardp", P::LastItem(LastItem::FontCharDp));
    put(it, "fontcharic", P::LastItem(LastItem::FontCharIc));
    put(it, "parshapelength", P::LastItem(LastItem::ParShapeLength));
    put(it, "parshapeindent", P::LastItem(LastItem::ParShapeIndent));
    put(it, "parshapedimen", P::LastItem(LastItem::ParShapeDimen));
    put(it, "fontdimen", P::FontParam(FontParam::Dimen));
    put(it, "hyphenchar", P::FontParam(FontParam::HyphenChar));
    put(it, "skewchar", P::FontParam(FontParam::SkewChar));
    put(it, "nullfont", P::FontIdent(0));
    put(it, "muskipdef", P::RegisterDef(RegKind::MuSkip));
    put(it, "showlists", P::Command(Syntax::Nothing));
    put(it, "showgroups", P::Command(Syntax::Nothing));
    put(it, "showifs", P::Command(Syntax::Nothing));
    put(it, "dump", P::Command(Syntax::Nothing));
    put(it, "noboundary", P::Mode(ModeCmd::Horizontal(Material::NoBoundary)));
    put(it, "indent", P::Mode(ModeCmd::StartPar { indent: true }));
    put(it, "-", P::Mode(ModeCmd::Horizontal(Material::Hyphen)));
    put(it, "/", P::Command(Syntax::Nothing));
    put(it, "cr", P::Command(Syntax::Nothing));
    put(it, "crcr", P::Command(Syntax::Nothing));
    put(it, "span", P::Command(Syntax::Nothing));
    put(it, "omit", P::Command(Syntax::Nothing));
    put(it, "noalign", P::Mode(ModeCmd::NoAlign));
    put(it, "displaylimits", P::Command(Syntax::Nothing));
    put(it, "limits", P::Command(Syntax::Nothing));
    put(it, "nolimits", P::Command(Syntax::Nothing));
    put(it, "displaystyle", P::Command(Syntax::Nothing));
    put(it, "textstyle", P::Command(Syntax::Nothing));
    put(it, "scriptstyle", P::Command(Syntax::Nothing));
    put(it, "scriptscriptstyle", P::Command(Syntax::Nothing));
    put(it, "mathord", P::Command(Syntax::Nothing));
    put(it, "mathop", P::Command(Syntax::Nothing));
    put(it, "mathbin", P::Command(Syntax::Nothing));
    put(it, "mathrel", P::Command(Syntax::Nothing));
    put(it, "mathopen", P::Command(Syntax::Nothing));
    put(it, "mathclose", P::Command(Syntax::Nothing));
    put(it, "mathpunct", P::Command(Syntax::Nothing));
    put(it, "mathinner", P::Command(Syntax::Nothing));
    put(it, "underline", P::Command(Syntax::Nothing));
    put(it, "overline", P::Command(Syntax::Nothing));
    put(it, "over", P::Command(Syntax::Nothing));
    put(it, "atop", P::Command(Syntax::Nothing));
    put(it, "eqno", P::Mode(ModeCmd::EqNo));
    put(it, "leqno", P::Mode(ModeCmd::EqNo));
    put(it, "nonscript", P::Command(Syntax::Nothing));
    put(it, "beginL", P::Command(Syntax::Nothing));
    put(it, "endL", P::Command(Syntax::Nothing));
    put(it, "beginR", P::Command(Syntax::Nothing));
    put(it, "endR", P::Command(Syntax::Nothing));
    put(it, "pagediscards", P::Mode(ModeCmd::Vertical(Material::Discards)));
    put(it, "splitdiscards", P::Mode(ModeCmd::Vertical(Material::Discards)));
    put(it, "shipout", P::Mode(ModeCmd::ShipOut));
    put(it, "discretionary", P::Mode(ModeCmd::Horizontal(Material::Discretionary)));
    put(it, "mathchoice", P::Mode(ModeCmd::MathChoice));
    for n in ["hfil", "hfill", "hss", "hfilneg"] {
        put(it, n, P::Mode(ModeCmd::Horizontal(Material::Fil)));
    }
    for n in ["vfil", "vfill", "vss", "vfilneg"] {
        put(it, n, P::Mode(ModeCmd::Vertical(Material::Fil)));
    }
    put(it, "hskip", P::Mode(ModeCmd::Horizontal(Material::Glue)));
    put(it, "vskip", P::Mode(ModeCmd::Vertical(Material::Glue)));
    put(it, " ", P::Mode(ModeCmd::Horizontal(Material::Space)));
    put(it, "raise", P::Mode(ModeCmd::Shift { vertical: true }));
    put(it, "lower", P::Mode(ModeCmd::Shift { vertical: true }));
    put(it, "moveleft", P::Mode(ModeCmd::Shift { vertical: false }));
    put(it, "moveright", P::Mode(ModeCmd::Shift { vertical: false }));
    for n in ["leaders", "cleaders", "xleaders"] {
        put(it, n, P::Mode(ModeCmd::Leaders));
    }
    put(it, "vadjust", P::Mode(ModeCmd::Insert { vadjust: true }));
    put(it, "left", P::Mode(ModeCmd::Left));
    put(it, "lastbox", P::Mode(ModeCmd::Box(BoxCmd::LastBox)));
    put(it, "batchmode", P::Command(Syntax::Interaction(0)));
    put(it, "nonstopmode", P::Command(Syntax::Interaction(1)));
    put(it, "scrollmode", P::Command(Syntax::Interaction(2)));
    put(it, "errorstopmode", P::Command(Syntax::Interaction(3)));
    put(it, "above", P::Command(Syntax::Dimen));
    put(it, "abovewithdelims", P::Command(Syntax::Delimiters { dimen: true }));
    put(it, "atopwithdelims", P::Command(Syntax::Delimiters { dimen: false }));
    put(it, "overwithdelims", P::Command(Syntax::Delimiters { dimen: false }));
    put(it, "char", P::Mode(ModeCmd::Horizontal(Material::Char)));
    put(it, "mathchar", P::Command(Syntax::Number));
    put(it, "delimiter", P::Command(Syntax::Number));
    put(it, "radical", P::Command(Syntax::Number));
    put(it, "mathaccent", P::Command(Syntax::Number));
    put(it, "setlanguage", P::Command(Syntax::Number));
    put(it, "closeout", P::Command(Syntax::CloseOut));
    put(it, "showbox", P::Command(Syntax::Number));
    put(it, "copy", P::Mode(ModeCmd::Box(BoxCmd::Copy)));
    put(it, "unhbox", P::Mode(ModeCmd::Horizontal(Material::Unpackage { take: true })));
    put(it, "unvbox", P::Mode(ModeCmd::Vertical(Material::Unpackage { take: true })));
    put(it, "unhcopy", P::Mode(ModeCmd::Horizontal(Material::Unpackage { take: false })));
    put(it, "unvcopy", P::Mode(ModeCmd::Vertical(Material::Unpackage { take: false })));
    put(it, "insert", P::Mode(ModeCmd::Insert { vadjust: false }));
    put(it, "mark", P::Command(Syntax::GeneralText));
    put(it, "hyphenation", P::Command(Syntax::GeneralText));
    put(it, "patterns", P::Command(Syntax::GeneralText));
    put(it, "showtokens", P::Command(Syntax::GeneralText));
    put(it, "marks", P::Command(Syntax::NumberText));
    put(it, "show", P::Command(Syntax::Token));
    put(it, "showthe", P::Command(Syntax::Internal));
    put(it, "hbox", P::Mode(ModeCmd::Box(BoxCmd::HBox)));
    put(it, "vbox", P::Mode(ModeCmd::Box(BoxCmd::VBox)));
    put(it, "vtop", P::Mode(ModeCmd::Box(BoxCmd::VTop)));
    put(it, "vcenter", P::Mode(ModeCmd::Box(BoxCmd::VCenter)));
    put(it, "halign", P::Mode(ModeCmd::Vertical(Material::Align)));
    put(it, "valign", P::Mode(ModeCmd::Horizontal(Material::Align)));
    put(it, "hrule", P::Mode(ModeCmd::Vertical(Material::Rule)));
    put(it, "vrule", P::Mode(ModeCmd::Horizontal(Material::Rule)));
    put(it, "setbox", P::Command(Syntax::SetBox));
    put(it, "vsplit", P::Mode(ModeCmd::Box(BoxCmd::VSplit)));
    put(it, "openout", P::Command(Syntax::OpenOut));
    put(it, "textfont", P::Command(Syntax::FamilyFont));
    put(it, "scriptfont", P::Command(Syntax::FamilyFont));
    put(it, "scriptscriptfont", P::Command(Syntax::FamilyFont));
    put(it, "readline", P::Command(Syntax::ReadLine));
    put(it, "eTeXrevision", P::Convert(Convert::EtexRevision));
    put(it, "fontname", P::Convert(Convert::FontName));
    put(it, "topmark", P::Convert(Convert::Mark));
    put(it, "firstmark", P::Convert(Convert::Mark));
    put(it, "botmark", P::Convert(Convert::Mark));
    put(it, "splitfirstmark", P::Convert(Convert::Mark));
    put(it, "splitbotmark", P::Convert(Convert::Mark));
    put(it, "topmarks", P::Convert(Convert::Marks));
    put(it, "firstmarks", P::Convert(Convert::Marks));
    put(it, "botmarks", P::Convert(Convert::Marks));
    put(it, "splitfirstmarks", P::Convert(Convert::Marks));
    put(it, "splitbotmarks", P::Convert(Convert::Marks));
    put(it, "iffontchar", P::If(Cond::FontChar));
    if engine.has_pdftex() {
        put(it, "pdfpageattr", P::TokensParameter);
        put(it, "pdfpagesattr", P::TokensParameter);
        put(it, "pdfpkmode", P::TokensParameter);
        put(it, "pdfadjustspacing", P::IntegerParameter);
        put(it, "pdfcompresslevel", P::IntegerParameter);
        put(it, "pdfdecimaldigits", P::IntegerParameter);
        put(it, "pdfdraftmode", P::IntegerParameter);
        put(it, "pdfgentounicode", P::IntegerParameter);
        put(it, "pdfminorversion", P::IntegerParameter);
        put(it, "pdfobjcompresslevel", P::IntegerParameter);
        put(it, "pdfprotrudechars", P::IntegerParameter);
        put(it, "pdfuniqueresname", P::IntegerParameter);
        put(it, "pdfhorigin", P::DimenParameter);
        put(it, "pdfvorigin", P::DimenParameter);
        put(it, "pdftexversion", P::LastItem(LastItem::PdftexVersion));
        put(it, "pdfelapsedtime", P::LastItem(LastItem::ElapsedTime));
        put(it, "pdfrandomseed", P::LastItem(LastItem::RandomSeed));
        put(it, "pdfshellescape", P::LastItem(LastItem::ShellEscape));
        put(it, "knaccode", P::FontParam(FontParam::CharCode(0)));
        put(it, "knbccode", P::FontParam(FontParam::CharCode(0)));
        put(it, "knbscode", P::FontParam(FontParam::CharCode(0)));
        put(it, "shbscode", P::FontParam(FontParam::CharCode(0)));
        put(it, "stbscode", P::FontParam(FontParam::CharCode(0)));
        put(it, "pdfsave", P::Command(Syntax::Nothing));
        put(it, "pdfrestore", P::Command(Syntax::Nothing));
        put(it, "pdfresettimer", P::Command(Syntax::Nothing));
        put(it, "pdfrunninglinkoff", P::Command(Syntax::Nothing));
        put(it, "pdfrunninglinkon", P::Command(Syntax::Nothing));
        put(it, "pdfinterwordspaceoff", P::Command(Syntax::Nothing));
        put(it, "pdfinterwordspaceon", P::Command(Syntax::Nothing));
        put(it, "pdffakespace", P::Command(Syntax::Nothing));
        put(it, "pdfendlink", P::Command(Syntax::Nothing));
        put(it, "pdfendthread", P::Command(Syntax::Nothing));
        put(it, "pdfrefobj", P::Command(Syntax::Number));
        put(it, "pdfrefxform", P::Command(Syntax::Number));
        put(it, "pdfrefximage", P::Command(Syntax::Number));
        put(it, "pdfsetrandomseed", P::Command(Syntax::Number));
        put(it, "pdfmapfile", P::Command(Syntax::GeneralText));
        put(it, "pdfmapline", P::Command(Syntax::GeneralText));
        put(it, "pdftrailer", P::Command(Syntax::GeneralText));
        put(it, "pdftrailerid", P::Command(Syntax::GeneralText));
        put(it, "pdfinfo", P::Command(Syntax::GeneralText));
        put(it, "pdfnames", P::Command(Syntax::GeneralText));
        put(it, "pdfsetmatrix", P::Command(Syntax::GeneralText));
        put(it, "pdfglyphtounicode", P::Command(Syntax::TwoTexts));
        put(it, "pdffontattr", P::Command(Syntax::FontText));
        put(it, "pdfincludechars", P::Command(Syntax::FontText));
        put(it, "pdfnoligatures", P::Command(Syntax::Font));
        put(it, "pdfcopyfont", P::Command(Syntax::CopyFont { amount: false }));
        put(it, "pdffontexpand", P::Command(Syntax::FontExpand));
        put(it, "pdfannot", P::Command(Syntax::PdfAnnot));
        put(it, "pdfstartlink", P::Command(Syntax::PdfStartLink));
        put(it, "pdfdest", P::Command(Syntax::PdfDest));
        put(it, "pdfoutline", P::Command(Syntax::PdfOutline));
        put(it, "pdfthread", P::Command(Syntax::PdfThread));
        put(it, "pdfstartthread", P::Command(Syntax::PdfThread));
        put(it, "pdfxform", P::Command(Syntax::PdfXForm));
        put(it, "pdfximage", P::Command(Syntax::PdfXImage));
        put(it, "pdfcolorstack", P::Command(Syntax::PdfColorStack));
        put(it, "pdfcatalog", P::Command(Syntax::PdfCatalog));
        put(it, "pdftexrevision", P::Convert(Convert::PdftexRevision));
        put(it, "pdftexbanner", P::Convert(Convert::PdftexBanner));
        put(it, "pdfstrcmp", P::Convert(Convert::StrCmp));
        put(it, "pdfescapehex", P::Convert(Convert::EscapeHex));
        put(it, "pdfunescapehex", P::Convert(Convert::UnescapeHex));
        put(it, "pdfescapestring", P::Convert(Convert::EscapeString));
        put(it, "pdfescapename", P::Convert(Convert::EscapeName));
        put(it, "pdffilesize", P::Convert(Convert::FileSize));
        put(it, "pdffilemoddate", P::Convert(Convert::FileModDate));
        put(it, "pdffiledump", P::Convert(Convert::FileDump));
        put(it, "pdfmdfivesum", P::Convert(Convert::MdFiveSum));
        put(it, "pdfmatch", P::Convert(Convert::Match));
        put(it, "pdflastmatch", P::Convert(Convert::LastMatch));
        put(it, "pdfuniformdeviate", P::Convert(Convert::UniformDeviate));
        put(it, "pdfnormaldeviate", P::Convert(Convert::NormalDeviate));
        put(it, "pdfcreationdate", P::Convert(Convert::CreationDate));
        put(it, "pdffontname", P::Convert(Convert::PdfFontName));
        put(it, "pdffontobjnum", P::Convert(Convert::PdfFontObjNum));
        put(it, "pdffontsize", P::Convert(Convert::PdfFontSize));
        put(it, "pdfpageref", P::Convert(Convert::PageRef));
        put(it, "pdfxformname", P::Convert(Convert::XFormName));
        put(it, "pdfinsertht", P::Convert(Convert::InsertHt));
        put(it, "pdfcolorstackinit", P::Convert(Convert::ColorStackInit));
        put(it, "pdfprimitive", P::Convert(Convert::Primitive));
        put(it, "ifpdfabsnum", P::If(Cond::AbsNum));
        put(it, "ifpdfabsdim", P::If(Cond::AbsDim));
        put(it, "ifpdfprimitive", P::If(Cond::Primitive));
    }
    if engine.has_pdftex_extensions() {
        put(it, "ignoreprimitiveerror", P::IntegerParameter);
        put(it, "partokencontext", P::IntegerParameter);
        put(it, "lpcode", P::FontParam(FontParam::CharCode(0)));
        put(it, "rpcode", P::FontParam(FontParam::CharCode(0)));
        put(it, "leftmarginkern", P::Convert(Convert::MarginKern));
        put(it, "rightmarginkern", P::Convert(Convert::MarginKern));
        put(it, "ifincsname", P::If(Cond::InCsname));
    }
    if matches!(engine, Engine::PdfTeX | Engine::XeTeX) {
        put(it, "pdfpageheight", P::DimenParameter);
        put(it, "pdfpagewidth", P::DimenParameter);
        put(it, "pdflastxpos", P::LastItem(LastItem::LastXPos));
        put(it, "pdflastypos", P::LastItem(LastItem::LastYPos));
        put(it, "pdfsavepos", P::Command(Syntax::Nothing));
    }
    if matches!(engine, Engine::PdfTeX | Engine::LuaTeX) {
        put(it, "efcode", P::FontParam(FontParam::CharCode(1000)));
        // pdfTeX reads this from the font's TFM tag per character, so no
        // single default is right: cmr10 gives 1 for `A` and 0 for `B`.
        put(it, "tagcode", P::FontParam(FontParam::CharCode(1)));
        put(it, "quitvmode", P::Mode(ModeCmd::QuitVMode));
        put(it, "letterspacefont", P::Command(Syntax::CopyFont { amount: true }));
    }

    put(it, "lowercase", P::CaseShift(CodeTable::Lower));
    put(it, "uppercase", P::CaseShift(CodeTable::Upper));
    // ε-TeX expressions (eTeX manual § 3.5): the operand is an expression,
    // not a number, so they are scanned where a value is wanted.
    put(it, "numexpr", P::Expr(RegKind::Count));
    put(it, "dimexpr", P::Expr(RegKind::Dimen));
    put(it, "glueexpr", P::Expr(RegKind::Skip));
    put(it, "muexpr", P::Expr(RegKind::MuSkip));
    put(it, "accent", P::Accent);
    put(it, "special", P::Pdf(PdfOp::Special));
    if matches!(engine, Engine::PdfTeX) {
        put(it, "pdfliteral", P::Pdf(PdfOp::Literal));
        put(it, "pdfobj", P::Pdf(PdfOp::Object));
        put(it, "pdfpageresources", P::Pdf(PdfOp::Resources));
    }
    if matches!(engine, Engine::LuaTeX) {
        put(it, "pdfextension", P::Pdf(PdfOp::Extension));
    }

    put(it, "relax", P::Relax);
    put(it, "par", P::Mode(ModeCmd::Par));
    put(it, "noindent", P::Mode(ModeCmd::StartPar { indent: false }));
    put(it, "ignorespaces", P::IgnoreSpaces);

    table
}

/// What `\input docstrip` makes available to a batch file: satex models the
/// commands rather than running docstrip's own code, which writes files
/// (docstrip.dtx, "The user interface").
pub fn docstrip_meanings(it: &mut Interner) -> Vec<(Sym, Meaning)> {
    use DocstripOp as D;
    use Primitive as P;
    let mut table = Vec::new();
    let mut put = |it: &mut Interner, name: &str, op: DocstripOp| {
        table.push((it.intern(name), Meaning::Primitive(P::Docstrip(op))));
    };
    put(it, "generate", D::Generate);
    put(it, "generateFile", D::GenerateFile);
    put(it, "usedir", D::UseDir);
    put(it, "batchinput", D::BatchInput);
    put(it, "preamble", D::Preamble { named: false, post: false });
    put(it, "postamble", D::Preamble { named: false, post: true });
    put(it, "declarepreamble", D::Preamble { named: true, post: false });
    put(it, "declarepostamble", D::Preamble { named: true, post: true });
    for n in ["usepreamble", "usepostamble"] {
        put(it, n, D::Setting { arguments: 1 });
    }
    for n in [
        "keepsilent",
        "showprogress",
        "askforoverwritetrue",
        "askforoverwritefalse",
        "askonceonly",
        "nopreamble",
        "nopostamble",
        "defaultpreamble",
        "defaultpostamble",
        "ifToplevel",
    ] {
        put(it, n, D::Setting { arguments: 0 });
    }
    table.push((it.intern("endbatchfile"), Meaning::Primitive(P::Endinput)));
    table.push((it.intern("Msg"), Meaning::Primitive(P::Message { error: false })));
    table
}

/// The character a packed math code carries, in its low 21 bits (XeTeX
/// reference, "Math"; LuaTeX manual, "Math").
const MATH_SLOT_MASK: i64 = 0x1f_ffff;
/// The highest character a packed math code may carry.
const MATH_SLOT_MAX: i64 = 0x10_ffff;
/// XeTeX's active marker, which is a math code of its own.
const MATH_ACTIVE: i64 = 0x1f_ffff;
/// tex.web § 1232: `"8000` is TeX82's active math code.
const MATH_CLASSIC_ACTIVE: i64 = 0x8000;

/// XeTeX and LuaTeX keep one math code per character, packed as a family, a
/// class and a character (XeTeX reference; LuaTeX manual, "Math").  TeX82's
/// fifteen-bit code goes into the same table, so it is packed on the way in.
pub fn pack_math_code(code: i64, engine: Engine) -> i64 {
    if !engine.has_unicode_math() {
        return code;
    }
    // XeTeX keeps the active math code as a marker of its own, while LuaTeX
    // packs it like any other, as class 8.
    if engine == Engine::XeTeX && code == MATH_CLASSIC_ACTIVE {
        return MATH_ACTIVE;
    }
    ((code >> 8) & 0xf) << 24 | (code >> 12) << 21 | (code & 0xff)
}

/// What `\the\mathcode` gives for a packed code.  XeTeX reads it back as
/// TeX82's, and gives 0 where it does not fit ("Extended mathchar used as
/// mathchar"); LuaTeX hands the packed code over as it stands.
pub fn read_math_code(code: i64, engine: Engine) -> i64 {
    if engine != Engine::XeTeX {
        return code;
    }
    if code == MATH_ACTIVE {
        return MATH_CLASSIC_ACTIVE;
    }
    let (family, class, slot) = ((code >> 24) & 0xff, (code >> 21) & 7, code & MATH_SLOT_MASK);
    if family > 0xf || slot > 0xff { 0 } else { class << 12 | family << 8 | slot }
}

/// What an engine does with a value a parameter will not take, and what a
/// read gives before anything is assigned.  Every entry of [`limit`] was
/// measured with `\showthe` on a real engine, not read off a manual.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Limit {
    /// The parameter takes `lo..=hi` and starts at `start`.  Anything else
    /// is an error, after which `bad` is stored, or the old value stays
    /// where there is none (tex.web §§ 1243, 1244).
    Range { start: i64, lo: i64, hi: i64, bad: Option<i64> },
    /// The parameter is read-only: a write is accepted and ignored, and a
    /// read always gives this value (LuaTeX manual, "Math").
    Fixed(i64),
    /// A packed math code.  An invalid one is an error and a recovery value
    /// is stored: XeTeX's active marker where the low bits ask for it, zero
    /// otherwise.
    MathCode { active: bool },
}

/// What an engine does with one assigned value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Taken {
    /// Stored as it is.
    Yes,
    /// Refused, and the old value stays.
    Old,
    /// Refused, and this value is stored instead.
    Instead(i64),
}

impl Limit {
    /// What a read gives before anything is assigned, where a parameter has
    /// one of its own.  A code table has none: its default is per character.
    pub fn start(self) -> Option<i64> {
        match self {
            Limit::Range { start, .. } => Some(start),
            Limit::Fixed(value) => Some(value),
            Limit::MathCode { .. } => None,
        }
    }

    /// What the engine does with `value`.
    pub fn take(self, value: i64) -> Taken {
        match self {
            Limit::Range { lo, hi, bad, .. } => match bad {
                _ if (lo..=hi).contains(&value) => Taken::Yes,
                Some(bad) => Taken::Instead(bad),
                None => Taken::Old,
            },
            Limit::Fixed(_) => Taken::Old,
            // Both engines check the low 21 bits alone, so a negative value
            // whose character fits is taken.  XeTeX takes `0x1FFFFF` as the
            // active marker and falls back to it for any other value that
            // asks for it.  LuaTeX has no marker and falls back to zero.
            Limit::MathCode { active } => {
                let slot = value & MATH_SLOT_MASK;
                if slot <= MATH_SLOT_MAX || (active && value == MATH_ACTIVE) {
                    Taken::Yes
                } else if active && slot == MATH_ACTIVE {
                    Taken::Instead(MATH_ACTIVE)
                } else {
                    Taken::Instead(0)
                }
            }
        }
    }

    /// Every value the parameter can end up holding, which an assignment
    /// satex could not pin down joins the old value with.
    pub fn reach(self) -> (i64, i64) {
        match self {
            Limit::Range { lo, hi, .. } => (lo, hi),
            Limit::Fixed(value) => (value, value),
            Limit::MathCode { .. } => (crate::value::WORD_MIN, crate::value::WORD_MAX),
        }
    }

    /// Why the engine refused `value`, in the words `assign_char_code` uses
    /// for the same shape.
    pub fn refusal(self, name: &str, value: i64) -> String {
        match self {
            Limit::Range { lo, hi, bad, .. } => {
                let after = match bad {
                    Some(bad) => format!("{bad} is used"),
                    None => "the old value is kept".into(),
                };
                format!("invalid value ({value}) for \\{name}, which takes {lo}..{hi}; {after}")
            }
            Limit::Fixed(reads) => format!("\\{name} is read-only and reads {reads}; this assignment does nothing"),
            Limit::MathCode { .. } => {
                let stored = match self.take(value) {
                    Taken::Instead(stored) => stored,
                    _ => 0,
                };
                format!("invalid math code ({value}) for \\{name}; {stored} is used")
            }
        }
    }
}

/// The parameters an engine range-checks, refuses to change, or starts away
/// from zero, and what it does with a value it will not take.  The ranges
/// and the recovery values differ by engine, so the engine decides.
pub fn limit(name: &str, engine: Engine) -> Option<Limit> {
    Some(match name {
        // tex.web § 1244: `\prevgraf` may not be negative.
        "prevgraf" => Limit::Range { start: 0, lo: 0, hi: crate::value::WORD_MAX, bad: None },
        // tex.web § 1265 (`new_interaction`): batch, nonstop, scroll and
        // error-stop mode, and § 74 starts a run in the last of them.
        "interactionmode" => Limit::Range { start: crate::commands::ERROR_STOP_MODE, lo: 0, hi: 3, bad: None },
        // LuaTeX manual, "The output routine": `\outputbox` names the box
        // the output routine finds the page in, and starts at 255.
        "outputbox" if engine == Engine::LuaTeX => Limit::Range { start: 255, lo: 0, hi: 65535, bad: None },
        // LuaTeX manual, "Math": these report how the math machinery is
        // built, so a write is accepted and ignored.
        "mathnolimitsmode" | "mathitalicsmode" | "mathdelimitersmode" if engine == Engine::LuaTeX => Limit::Fixed(0),
        "mathdefaultsmode" | "mathscriptboxmode" | "mathscriptcharmode" if engine == Engine::LuaTeX => Limit::Fixed(1),
        // tex.web § 1243 (`alter_aux`): "Bad space factor", after which the
        // engine goes on with 1000, which is also a fresh list's.
        "spacefactor" => Limit::Range { start: 1000, lo: 1, hi: 32767, bad: Some(1000) },
        // XeTeX reference and LuaTeX manual, "Math": the packed form of a
        // math code, which only XeTeX gives an active marker.  A
        // `\Umathcharnumdef` takes the same packed code and recovers the
        // same way.
        "Umathcodenum" | "Umathcharnumdef" if engine.has_unicode_math() => {
            Limit::MathCode { active: engine == Engine::XeTeX }
        }
        _ => return None,
    })
}

/// tex.web § 212 (`ignore_depth`): what `\prevdepth` is on a vertical list
/// that has nothing on it yet, which is below any real depth, so the first
/// box takes no interline glue.
pub const IGNORE_DEPTH: i64 = -1000 * 65536;

/// A quantity the engine keeps outside the parameter table: the current
/// list's own, or the page builder's.  Reading or writing one where its
/// owner is not there is an error, so the answer is not what was assigned.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owned {
    /// The current list's, and only readable and writable while the mode is
    /// one of `modes` (tex.web §§ 418, 1243).  Elsewhere a read is
    /// "Improper" and gives zero, and a write is `report_illegal_case`,
    /// which scans nothing, so the `=⟨value⟩` is left in the input.  A list
    /// of its own starts at `fresh` (tex.web § 1083).
    List { modes: crate::mode::Modes, fresh: i64 },
    /// The page builder's (tex.web §§ 982, 986).  The write is an ordinary
    /// assignment, but while the page is empty the read gives this instead
    /// of what was assigned.
    PageSoFar { empty: i64 },
}

/// The quantities [`Owned`] covers.  Every engine has all of them: they are
/// TeX82's (tex.web §§ 982, 1243).
pub fn owned(name: &str) -> Option<Owned> {
    use crate::mode::Modes;
    Some(match name {
        // tex.web § 418: `\spacefactor` is the horizontal list's and
        // `\prevdepth` the vertical list's.
        "spacefactor" => Owned::List { modes: Modes::ANY_HORIZONTAL, fresh: 1000 },
        "prevdepth" => Owned::List { modes: Modes::ANY_VERTICAL, fresh: IGNORE_DEPTH },
        // tex.web § 986 (`set_page_dimen`): while the page is empty
        // `\pagegoal` reads `\maxdimen` and the other seven read zero.
        "pagegoal" => Owned::PageSoFar { empty: crate::value::MAX_DIMEN },
        "pagetotal" | "pagedepth" | "pagestretch" | "pageshrink" | "pagefilstretch" | "pagefillstretch"
        | "pagefilllstretch" => Owned::PageSoFar { empty: 0 },
        _ => return None,
    })
}

/// A name of the expl3 programming layer: a function (`\tl_set:Nn`) or one
/// of the quarks and scan marks (`\q_nil`, `\s__tl_stop`) that
/// `expl3-code.tex` defines.
pub fn is_expl3_name(name: &str) -> bool {
    name.contains(':') || name.starts_with("q_") || name.starts_with("s__")
}

pub fn kernel_meanings(it: &mut Interner, engine: Engine) -> HashMap<Sym, Meaning> {
    let mut table = initial_meanings(it, engine);
    // The expl3 layer comes from `expl3-code.tex` itself.
    table.retain(|sym, _| !is_expl3_name(it.name(*sym)));
    table
}

/// Primitives whose current arm is a known bug, not a sound verdict: the
/// control sequence name and where the bug is. `doc/wiki/primitive-coverage.md`
/// flags these instead of counting them as modeled. Empty once fixed.
pub fn known_bugs() -> &'static [(&'static str, &'static str)] {
    &[]
}

/// How faithfully satex models a primitive: generates
/// `doc/wiki/primitive-coverage.md`. A primitive that scans its operands and
/// then drops their effect, with neither a widening marker nor a
/// diagnostic, is a bug to fix, not a fourth verdict; [`verdict`] never
/// returns one, so a bug found this way is reported separately, not
/// documented here as if it were sound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// satex reproduces the engine's own behavior.
    Exact,
    /// A sound over-approximation. The detail says how.
    Approximated,
    /// Not modeled, but every effect it could have is widened and the run
    /// says so (a diagnostic, e.g. `unmodeled-primitive`, `unknown-catcode`).
    Unsupported,
}

/// Where a verdict is decided: the source file and the line of the arm
/// that returns it. Filled by the `at!` macro, so a link cannot drift.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Site {
    /// Path from the crate root, as `file!()` gives it.
    pub file: &'static str,
    /// Line of the arm.
    pub line: u32,
}

/// A verdict, why, and the arm that decided it.
pub type Judgment = (Verdict, &'static str, Site);

/// Tags a verdict with the [`Site`] of the arm it is written on.
macro_rules! at {
    ($verdict:expr, $detail:expr $(,)?) => {
        ($verdict, $detail, Site { file: file!(), line: line!() })
    };
}

/// The verdict for a [`Primitive`] and why, from what `src/exec.rs`,
/// `src/mode.rs` and `src/commands.rs` actually do with it. Grouped by
/// sub-enum so a new case cannot be forgotten (exhaustive matches).
pub fn verdict(p: Primitive) -> Judgment {
    use Primitive as P;
    use Verdict as V;
    match p {
        P::Text(t) => text_verdict(t),
        P::If(c) => cond_verdict(c),
        P::LastItem(item) => last_item_verdict(item),
        P::Special(s) => special_verdict(s),
        P::Command(syntax) => syntax_verdict(syntax),
        P::Convert(c) => convert_verdict(c),
        P::Mode(m) => mode_verdict(m),
        P::Typeset(t) => typeset_verdict(t),
        P::Lua(op) => lua_verdict(op),

        // Definitions, grouping and expansion: tex.web parts 24-27 and the
        // eTeX manual §§ 3.1-3.4, matched by `src/exec.rs`.
        P::Def { .. } => {
            at!(V::Exact, "tex.web § 476. An unterminated definition is diagnosed, not read past the file's end")
        }
        P::Let { .. } => at!(V::Exact, "tex.web § 1221, including the two-token `\\futurelet` lookahead"),
        P::Prefix(_) => at!(V::Exact, "tex.web § 1211"),
        P::ExpandAfter => at!(V::Exact, "tex.web § 367"),
        P::NoExpand | P::DontExpand => at!(V::Exact, "tex.web §§ 367, 358"),
        P::SkipCrossing { .. } => {
            at!(V::Exact, "satex's own marker for a conditional a file boundary interrupted, not an engine primitive")
        }
        P::Csname => at!(
            V::Approximated,
            "exact once the name is resolved. A bounded join of candidate meanings when it is built from unknown digits (`one_of_names`)",
        ),
        P::BeginCsname | P::LastNamedCs | P::Endcsname => at!(V::Exact, "LuaTeX manual, \"Macros\""),
        P::Reinject => at!(V::Exact, "eTeX manual § 3.4"),
        P::ScanTokens { .. } => {
            at!(V::Exact, "eTeX manual § 3.9. A capacity overflow (`MAX_IN_OPEN`) is diagnosed, not assumed away")
        }
        P::Use(_) | P::Expanded => at!(V::Exact, "interface3, \"Selecting tokens\"; pdfTeX manual, \"\\expanded\""),
        P::Else | P::Or | P::Fi => {
            at!(V::Exact, "tex.web § 510, including `insert_relax` for an else or fi met mid-test")
        }
        P::Unless => at!(
            V::Approximated,
            "etex.ch. Exact before a conditional; before a name of unknown meaning the conditional it may open is left undecided",
        ),
        P::BeginGroup | P::EndGroup | P::Defer(_) => {
            at!(V::Exact, "tex.web §§ 1211, 1 (semi-simple groups, `\\aftergroup`/`\\afterassignment`)")
        }

        // Registers, arithmetic and per-character tables: tex.web part 25.
        P::Allocate(_) => at!(V::Exact, "hands out a fresh register, tracked exactly"),
        P::Register(_) | P::RegisterDef(_) => at!(
            V::Approximated,
            "exact for a known register index. An unknown index joins the value into every register it could name (`weaken_registers`)",
        ),
        P::FontDef => at!(V::Exact, "tex.web § 1257 (`\\font`)"),
        P::OpenIn | P::CloseIn | P::Read => at!(
            V::Approximated,
            "tex.web § 1351. Exact for a known stream number; one satex cannot pin down leaves every stream's state unknown",
        ),
        P::Arith(_) => at!(
            V::Approximated,
            "exact for known operands. Interval arithmetic and tex.web § 222's zero default otherwise",
        ),
        P::IntegerParameter | P::DimenParameter | P::GlueParameter { .. } => {
            at!(V::Exact, "the stored parameter table")
        }
        P::TokensParameter => at!(
            V::Approximated,
            "the stored parameter table, but setting any `\\every…` list widens the dimensions of boxes that were already packed before it was read",
        ),
        P::CatcodeAssign => at!(
            V::Unsupported,
            "exact for one known character and category. Diagnosed (`unknown-catcode`) and widened when either can't be pinned down",
        ),
        P::CharCode(_) => at!(
            V::Unsupported,
            "exact for one known character. Diagnosed (`unknown-char-code`) and the whole table widened when the candidates can't be enumerated at all",
        ),
        P::CaseShift(_) => at!(V::Exact, "tex.web § 1288. Widens a code an unknown token was assigned"),
        P::FontParam(_) => at!(
            V::Approximated,
            "reads the stored font table; a read past the last-loaded font's dimensions is discarded, matching tex.web § 580's own recovery. An entry satex cannot pin down widens every one it may name",
        ),
        P::FontIdent(_) => at!(V::Exact, "tex.web § 1217: selecting a font is a local assignment"),

        // File and job control.
        P::Load(_) => at!(V::Exact, "tex.web § 537 (`\\input`)"),
        P::Endinput => at!(
            V::Unsupported,
            "exact when unconditional. Diagnosed (`conditional-endinput`) and the file read on when reached under an undecided conditional",
        ),
        P::End => at!(V::Exact, "tex.web § 1054 (`end_job`)"),
        P::Message { .. } => at!(V::Exact, "tex.web § 1279"),
        P::Write => at!(
            V::Approximated,
            "`\\immediate\\write` is exact. A deferred `\\write`'s text is recorded unexpanded, since satex never reaches shipout to expand it",
        ),
        P::Expr(_) => at!(V::Exact, "eTeX manual § 3.5"),
        P::Relax => at!(V::Exact, "tex.web § 1211"),
        P::IgnoreSpaces => at!(V::Exact, "tex.web § 1045"),

        // Output, PDF, Lua and docstrip, and the primitives satex leaves
        // alone on purpose.
        P::Accent => {
            at!(V::Approximated, "widens to Unknown when the accented character does not resolve in horizontal mode")
        }
        P::Pdf(_) => at!(
            V::Approximated,
            "the manual's keywords are scanned exactly. No PDF is built, so the payload is recorded as an occurrence, not replayed",
        ),
        P::LuaCall { .. } => {
            at!(V::Approximated, "runs the named Lua function where a host is available. Widens where it is not")
        }
        P::Unmodeled | P::AlignMark | P::AlignTab => at!(V::Unsupported, "widens the mode"),
        P::Docstrip(_) => {
            at!(V::Exact, "recorded as generate/preamble facts, not run as TeX (docstrip.dtx, \"The user interface\")")
        }
    }
}

fn text_verdict(t: TextOf) -> Judgment {
    use Verdict as V;
    match t {
        TextOf::JobName => at!(V::Exact, "the run's real main file name"),
        _ => at!(
            V::Approximated,
            "exact for a known operand. Widens when the token's meaning or a register's digits are unresolved",
        ),
    }
}

fn cond_verdict(c: Cond) -> Judgment {
    use Cond as C;
    use Verdict as V;
    match c {
        C::True | C::False => at!(V::Exact, "a constant"),
        C::IfX => at!(
            V::Approximated,
            "exact for known meanings (tex.web § 507's `original` handles a redefined primitive). Member-by-member across a set of possible meanings otherwise, decided only when they agree",
        ),
        C::Defined => {
            at!(V::Approximated, "exact for a known name. Narrowed by which names could still be undefined otherwise")
        }
        C::Csname => at!(V::Exact, "builds the name and looks it up"),
        C::Num | C::Dim | C::Odd => {
            at!(V::Approximated, "exact for known operands. Narrows the interval on each branch otherwise")
        }
        C::Chars | C::Catcodes => {
            at!(V::Approximated, "exact for known characters. Undecided when either operand is an unresolved character")
        }
        C::Case => {
            at!(V::Approximated, "tex.web § 509. Every arm is analyzed as its own path when the selector is unknown")
        }
        C::Eof => at!(
            V::Approximated,
            "satex resolves the real file, so end-of-file is known for a known stream number; one it cannot pin down leaves the test undecided",
        ),
        C::Opaque => at!(V::Approximated, "both branches are analyzed and their state joined, by configuration"),
        C::VMode | C::HMode | C::InnerMode | C::MathMode => at!(
            V::Approximated,
            "decided from the mode satex tracks through the run. Open groups the run couldn't follow leave it undecided",
        ),
        C::Box => at!(
            V::Approximated,
            "exact for a known box state (void, `\\hbox`, `\\vbox`). Undecided once the register is Unknown",
        ),
        C::AbsNum | C::AbsDim | C::InCsname | C::Primitive => {
            at!(V::Exact, "computed from state satex already tracks (`eval_extra_condition`)")
        }
        C::FontChar => at!(V::Exact, "eTeX manual § 3.10: read from the font's real character table"),
    }
}

fn last_item_verdict(item: LastItem) -> Judgment {
    use LastItem as L;
    use Verdict as V;
    match item {
        L::Penalty | L::Kern | L::Skip => {
            at!(V::Approximated, "reads the tail of the list satex tracks. Unknown once the tail itself is")
        }
        L::NodeType => at!(V::Approximated, "-1 for a genuinely empty list (etex.ch). Unknown otherwise"),
        L::Badness => at!(V::Approximated, "always Unknown. Badness needs the line-breaking satex does not run"),
        L::InputLineNo | L::EtexVersion | L::PdftexVersion | L::GroupLevel | L::GroupType => {
            at!(V::Exact, "read from state satex already tracks")
        }
        L::IfLevel | L::IfType | L::IfBranch => {
            at!(V::Approximated, "exact once the conditional stack has settled. Unknown while a branch is undecided")
        }
        L::GlueStretch | L::GlueShrink | L::GlueStretchOrder | L::GlueShrinkOrder | L::GlueToMu | L::MuToGlue => {
            at!(V::Exact, "computed from a known glue spec")
        }
        L::FontCharWd => at!(V::Exact, "a TFM's or a native font's real glyph advance"),
        L::FontCharHt | L::FontCharDp | L::FontCharIc => at!(
            V::Approximated,
            "exact for a TFM font (etex_man § 3.10). A native font's height, depth and italic correction are not read, so it widens to Unknown",
        ),
        L::ParShapeLength | L::ParShapeIndent | L::ParShapeDimen => at!(V::Exact, "read from the stored `\\parshape`"),
        L::ShellEscape => at!(V::Exact, "the configuration's own setting"),
        L::ElapsedTime | L::RandomSeed | L::LastXPos | L::LastYPos => at!(
            V::Approximated,
            "genuinely unknowable without the real engine's clock, RNG or page builder. Widens to Unknown",
        ),
        L::XetexVersion => at!(V::Exact, "a constant"),
        L::LuatexVersion => at!(V::Exact, "probed from the real `luatex --version` for this run"),
        L::PdfPageCount => {
            at!(V::Approximated, "widens to Unknown. The file's page count needs parsing a PDF satex does not read")
        }
        L::Xetex(_) => at!(V::Approximated, "the font's real tables"),
    }
}

fn special_verdict(s: Special) -> Judgment {
    use Special as S;
    use Verdict as V;
    match s {
        S::BoxDimen => {
            at!(V::Approximated, "round-trips through the register consistently. The box behind it is never built")
        }
        S::ParShape | S::Penalties | S::CharClass | S::InterCharToks | S::UCode { .. } => at!(
            V::Approximated,
            "a table satex stores and reads back exactly for a known entry; one it cannot pin down widens every entry the write may have reached",
        ),
    }
}

fn syntax_verdict(s: Syntax) -> Judgment {
    use Syntax as S;
    use Verdict as V;
    match s {
        S::Nothing => at!(V::Exact, "no operand"),
        S::Interaction(_) => at!(V::Exact, "sets `\\interactionmode`"),
        S::Token => at!(V::Exact, "`\\show`"),
        S::Internal => at!(V::Exact, "`\\showthe`"),
        S::BoxSpec | S::RuleSpec | S::SetBox => at!(V::Exact, "forwarded to the real box-scanning path"),
        S::OpenOut | S::CloseOut => at!(
            V::Approximated,
            "tracks the stream state a later `\\input` of the written file reads back; a stream number satex cannot pin down widens every stream",
        ),
        S::FamilyFont | S::CopyFont { .. } | S::UMathCharDef { .. } | S::SetFontId => at!(
            V::Approximated,
            "stored and consumed by satex's own font and math-code tables; an operand satex cannot pin down leaves the entry, or the font the name means, unknown",
        ),
        S::ReadLine => at!(
            V::Approximated,
            "eTeX manual § 3.2, including the terminal-fatal case; a stream number satex cannot pin down gives an unknown line and does not end the job",
        ),
        S::PdfXImage => at!(V::Exact, "the file name reaches `observe::image`"),
        S::PdfColorStack => at!(
            V::Approximated,
            "the operation and stack id are tracked. The colour text itself is a recorded occurrence, not replayed",
        ),
        S::VSplit => {
            at!(V::Exact, "unreachable in practice. `\\vsplit` is `ModeCmd::Box(BoxCmd::VSplit)`, not this arm")
        }
        _ => at!(V::Approximated, "scanned, not built"),
    }
}

fn convert_verdict(c: Convert) -> Judgment {
    use Convert as C;
    use Verdict as V;
    match c {
        C::EtexRevision | C::PdftexRevision | C::XetexRevision | C::FontId | C::FontName => {
            at!(V::Exact, "a real constant or stored name")
        }
        C::StrCmp | C::EscapeHex | C::UnescapeHex | C::EscapeString | C::EscapeName => {
            at!(V::Exact, "the manual's own algorithm, run for real")
        }
        C::FileSize | C::FileDump | C::MdFiveSum => at!(V::Exact, "reads the real file"),
        C::FileModDate => at!(V::Approximated, "always Unknown. satex does not read file modification times"),
        C::Uchar { .. } => at!(V::Exact, "tex.web, eTeX and XeTeX's character and category-code construction rules"),
        C::PdfFontSize => at!(V::Exact, "the font's real design size"),
        C::PdfFontName | C::PdfFontObjNum => at!(V::Approximated, "widens to Unknown. No PDF font object is built"),
        C::ColorStackInit => at!(
            V::Approximated,
            "`\\pdfcolorstackinit` hands out the next colour stack number, widened rather than counted exactly. The colour text is a recorded occurrence, not replayed",
        ),
        C::Primitive => {
            at!(V::Exact, "re-injects the frozen meaning for normal expansion to re-process, as `\\pdfprimitive` does")
        }
        C::Xetex(_) => {
            at!(V::Approximated, "exact from a loaded font's real tables. Unknown once they aren't resolved")
        }
        _ => at!(V::Approximated, "scanned, answer unknown"),
    }
}

fn mode_verdict(m: ModeCmd) -> Judgment {
    use ModeCmd as M;
    use Verdict as V;
    match m {
        M::Par | M::StartPar { .. } | M::QuitVMode | M::Shift { .. } | M::EqNo => {
            at!(V::Exact, "tex.web's own mode-transition rules")
        }
        M::Horizontal(Material::Rule) => at!(
            V::Approximated,
            "a real rule item in a horizontal list, but a `\\vrule` met in a vertical list starts a paragraph and widens the enclosing box's dimensions rather than answering them exactly",
        ),
        M::Horizontal(mat) | M::Vertical(mat) => material_verdict(mat),
        M::Box(b) => box_verdict(b),
        M::Leaders => at!(
            V::Approximated,
            "the box or rule is scanned exactly. tex.web § 1078's repeated copies are not laid out, since satex tracks natural size, not a page",
        ),
        M::ShipOut => at!(V::Exact, "removes the box from the list, tex.web § 1073's own effect on it"),
        M::Insert { .. } => {
            at!(V::Approximated, "opens the insert's own list. Not told apart from the vertical list around it")
        }
        M::NoAlign => {
            at!(V::Approximated, "cells run in the alignment's own list, not distinguished from the preamble")
        }
        M::MathChoice => at!(V::Exact, "tex.web § 1172: opens all four styles in sequence"),
        M::Left => {
            at!(
                V::Approximated,
                "documented: satex keeps no group for it, so the display may read as inner from here on"
            )
        }
    }
}

fn material_verdict(m: Material) -> Judgment {
    use Material as M;
    use Verdict as V;
    match m {
        M::Char | M::Rule | M::Glue | M::Fil | M::Discretionary | M::Space | M::NoBoundary => {
            at!(V::Exact, "a real item, appended to the list satex tracks")
        }
        M::Unpackage { .. } => at!(
            V::Approximated,
            "a void register adds nothing. A real one becomes Unknown, since its content is not tracked once boxed",
        ),
        M::Discards | M::Hyphen => {
            at!(
                V::Approximated,
                "widens to Unknown. A rarely-taken tex.web path this pass does not distinguish further"
            )
        }
        M::Align => at!(V::Approximated, "cells run in the alignment's own list, not distinguished from the preamble"),
    }
}

fn box_verdict(b: BoxCmd) -> Judgment {
    use BoxCmd as B;
    use Verdict as V;
    match b {
        B::HBox | B::VBox | B::VTop | B::VCenter | B::Copy | B::LastBox => at!(V::Exact, "the real box machinery"),
        B::VSplit => at!(
            V::Approximated,
            "the result always widens to Unknown, even from a void register. A real split's remainder is not tracked",
        ),
    }
}

fn typeset_verdict(t: Typeset) -> Judgment {
    use Typeset as T;
    use Verdict as V;
    match t {
        T::Plain => at!(V::Exact, "`\\unskip`, `\\unkern` and `\\unpenalty` remove the real tail item"),
        T::Number => at!(V::Exact, "the penalty is appended as scanned"),
        T::Dimen => at!(
            V::Approximated,
            "`\\kern` is exact. `\\mkern`'s math-unit dimension always widens to Unknown, since it needs math-font setup satex does not run",
        ),
        T::Delimiter => at!(V::Exact, "tex.web § 1161: the delimiter is the one token the primitive itself consumes"),
        T::Glue => at!(
            V::Approximated,
            "`\\mskip` appends an item of unknown size: its glue is in math units, which need math-font setup satex does not run",
        ),
    }
}

fn lua_verdict(op: LuaOp) -> Judgment {
    use LuaOp as L;
    use Verdict as V;
    match op {
        L::Direct | L::Late => at!(
            V::Approximated,
            "the chunk runs for real where a Lua host is available. A failed or unavailable run is diagnosed and widened",
        ),
        L::Escape | L::InitCatcodes | L::SaveCatcodes | L::SelectCatcodes => {
            at!(V::Exact, "a real string escape or catcode-table operation")
        }
    }
}
