//! One pinned case per TeX (tex.web) and e-TeX (etex.ch) primitive that no
//! other test pins.  Each row runs a primitive-only snippet on the bare
//! machine and compares the text of `\R` (set with `\edef`/`\xdef`, white
//! space ignored) with what `etex -ini` computes for the same snippet; the
//! expectations are TeX's own values, checked by `target/scratch/pin/verify.sh`.
//! Unobservable primitives (tracing, `\show...`, math spacing) are pinned by
//! their parameter value or by the group structure they leave.

use crate::support::assert_r;

/// One test per row: `name: snippet => expected \R;`.
macro_rules! pin {
    ($($(#[$m:meta])* $name:ident: $src:literal => $exp:literal;)*) => {
        $($(#[$m])* #[test] fn $name() { assert_r($src, $exp) })*
    };
}

// Part 17: the table of equivalents, integer parameters (tex.web § 236).
pin! {
    /// `\adjdemerits` (tex.web § 236): an integer parameter.
    p_adjdemerits: r#"\adjdemerits=11 \edef\R{\the\adjdemerits}"# => "11";
    /// `\binoppenalty` (tex.web § 236): an integer parameter.
    p_binoppenalty: r#"\binoppenalty=12 \edef\R{\the\binoppenalty}"# => "12";
    /// `\brokenpenalty` (tex.web § 236): an integer parameter.
    p_brokenpenalty: r#"\brokenpenalty=13 \edef\R{\the\brokenpenalty}"# => "13";
    /// `\clubpenalty` (tex.web § 236): an integer parameter.
    p_clubpenalty: r#"\clubpenalty=14 \edef\R{\the\clubpenalty}"# => "14";
    /// `\defaulthyphenchar` (tex.web § 236): an integer parameter.
    p_defaulthyphenchar: r#"\defaulthyphenchar=45 \edef\R{\the\defaulthyphenchar}"# => "45";
    /// `\delimiterfactor` (tex.web § 236): an integer parameter.
    p_delimiterfactor: r#"\delimiterfactor=901 \edef\R{\the\delimiterfactor}"# => "901";
    /// `\displaywidowpenalty` (tex.web § 236): an integer parameter.
    p_displaywidowpenalty: r#"\displaywidowpenalty=15 \edef\R{\the\displaywidowpenalty}"# => "15";
    /// `\doublehyphendemerits` (tex.web § 236): an integer parameter.
    p_doublehyphendemerits: r#"\doublehyphendemerits=16 \edef\R{\the\doublehyphendemerits}"# => "16";
    /// `\errorcontextlines` (tex.web § 236): an integer parameter.
    p_errorcontextlines: r#"\errorcontextlines=2 \edef\R{\the\errorcontextlines}"# => "2";
    /// `\exhyphenpenalty` (tex.web § 236): an integer parameter.
    p_exhyphenpenalty: r#"\exhyphenpenalty=17 \edef\R{\the\exhyphenpenalty}"# => "17";
    /// `\fam` (tex.web § 236): an integer parameter, assignable outside math.
    p_fam: r#"\fam=5 \edef\R{\the\fam}"# => "5";
    /// `\finalhyphendemerits` (tex.web § 236): an integer parameter.
    p_finalhyphendemerits: r#"\finalhyphendemerits=18 \edef\R{\the\finalhyphendemerits}"# => "18";
    /// `\floatingpenalty` (tex.web § 236): an integer parameter.
    p_floatingpenalty: r#"\floatingpenalty=19 \edef\R{\the\floatingpenalty}"# => "19";
    /// `\hangafter` (tex.web § 236): an integer parameter, negative values allowed.
    p_hangafter: r#"\hangafter=-3 \edef\R{\the\hangafter}"# => "-3";
    /// `\hbadness` (tex.web § 236): an integer parameter.
    p_hbadness: r#"\hbadness=20 \edef\R{\the\hbadness}"# => "20";
    /// `\holdinginserts` (tex.web § 236): an integer parameter.
    p_holdinginserts: r#"\holdinginserts=1 \edef\R{\the\holdinginserts}"# => "1";
    /// `\interlinepenalty` (tex.web § 236): an integer parameter.
    p_interlinepenalty: r#"\interlinepenalty=21 \edef\R{\the\interlinepenalty}"# => "21";
    /// `\language` (tex.web § 236): an integer parameter.
    p_language: r#"\language=3 \edef\R{\the\language}"# => "3";
    /// `\lefthyphenmin` (tex.web § 236): an integer parameter.
    p_lefthyphenmin: r#"\lefthyphenmin=2 \edef\R{\the\lefthyphenmin}"# => "2";
    /// `\linepenalty` (tex.web § 236): an integer parameter.
    p_linepenalty: r#"\linepenalty=22 \edef\R{\the\linepenalty}"# => "22";
    /// `\looseness` (tex.web § 236): an integer parameter.
    p_looseness: r#"\looseness=-1 \edef\R{\the\looseness}"# => "-1";
    /// `\maxdeadcycles` (tex.web § 236): an integer parameter.
    p_maxdeadcycles: r#"\maxdeadcycles=23 \edef\R{\the\maxdeadcycles}"# => "23";
    /// `\month` (tex.web § 236): an integer parameter, assignable.
    p_month: r#"\month=7 \edef\R{\the\month}"# => "7";
    /// `\pausing` (tex.web § 236): an integer parameter.
    p_pausing: r#"\pausing=1 \edef\R{\the\pausing}"# => "1";
    /// `\postdisplaypenalty` (tex.web § 236): an integer parameter.
    p_postdisplaypenalty: r#"\postdisplaypenalty=24 \edef\R{\the\postdisplaypenalty}"# => "24";
    /// `\predisplaypenalty` (tex.web § 236): an integer parameter.
    p_predisplaypenalty: r#"\predisplaypenalty=25 \edef\R{\the\predisplaypenalty}"# => "25";
    /// `\relpenalty` (tex.web § 236): an integer parameter.
    p_relpenalty: r#"\relpenalty=26 \edef\R{\the\relpenalty}"# => "26";
    /// `\righthyphenmin` (tex.web § 236): an integer parameter.
    p_righthyphenmin: r#"\righthyphenmin=3 \edef\R{\the\righthyphenmin}"# => "3";
    /// `\showboxbreadth` (tex.web § 236): an integer parameter.
    p_showboxbreadth: r#"\showboxbreadth=27 \edef\R{\the\showboxbreadth}"# => "27";
    /// `\showboxdepth` (tex.web § 236): an integer parameter.
    p_showboxdepth: r#"\showboxdepth=28 \edef\R{\the\showboxdepth}"# => "28";
    /// `\tracinglostchars` (tex.web § 236): an integer parameter.
    p_tracinglostchars: r#"\tracinglostchars=1 \edef\R{\the\tracinglostchars}"# => "1";
    /// `\tracingoutput` (tex.web § 236): an integer parameter.
    p_tracingoutput: r#"\tracingoutput=1 \edef\R{\the\tracingoutput}"# => "1";
    /// `\tracingpages` (tex.web § 236): an integer parameter.
    p_tracingpages: r#"\tracingpages=1 \edef\R{\the\tracingpages}"# => "1";
    /// `\tracingparagraphs` (tex.web § 236): an integer parameter.
    p_tracingparagraphs: r#"\tracingparagraphs=1 \edef\R{\the\tracingparagraphs}"# => "1";
    /// `\tracingrestores` (tex.web § 236): an integer parameter.
    p_tracingrestores: r#"\tracingrestores=1 \edef\R{\the\tracingrestores}"# => "1";
    /// `\tracingstats` (tex.web § 236): an integer parameter.
    p_tracingstats: r#"\tracingstats=1 \edef\R{\the\tracingstats}"# => "1";
    /// `\uchyph` (tex.web § 236): an integer parameter.
    p_uchyph: r#"\uchyph=1 \edef\R{\the\uchyph}"# => "1";
    /// `\vbadness` (tex.web § 236): an integer parameter.
    p_vbadness: r#"\vbadness=29 \edef\R{\the\vbadness}"# => "29";
    /// `\widowpenalty` (tex.web § 236): an integer parameter.
    p_widowpenalty: r#"\widowpenalty=30 \edef\R{\the\widowpenalty}"# => "30";
    /// `\year` (tex.web § 236): an integer parameter, assignable.
    p_year: r#"\year=1999 \edef\R{\the\year}"# => "1999";
    /// `\hyphenpenalty` (tex.web § 236): an integer parameter.
    p_hyphenpenalty: r#"\hyphenpenalty=31 \edef\R{\the\hyphenpenalty}"# => "31";
    /// `\deadcycles` (tex.web § 421): a special integer, assignable.
    p_deadcycles: r#"\deadcycles=2 \edef\R{\the\deadcycles}"# => "2";
    /// `\insertpenalties` (tex.web § 421): a special integer, assignable.
    p_insertpenalties: r#"\insertpenalties=4 \edef\R{\the\insertpenalties}"# => "4";
    /// `\badness` (tex.web § 424): the badness of the last box, here a hbox of stretch ratio 1/2.
    p_badness: r#"\hbadness=10000 \setbox0\hbox to 10pt{\hskip 0pt plus 20pt}\edef\R{\the\badness}"# => "12";
}

// Part 17: dimension and glue parameters (tex.web § 247, § 224).
pin! {
    /// `\delimitershortfall` (tex.web § 247): a dimension parameter.
    p_delimitershortfall: r#"\delimitershortfall=1pt \edef\R{\the\delimitershortfall}"# => "1.0pt";
    /// `\displayindent` (tex.web § 247): a dimension parameter.
    p_displayindent: r#"\displayindent=2pt \edef\R{\the\displayindent}"# => "2.0pt";
    /// `\displaywidth` (tex.web § 247): a dimension parameter.
    p_displaywidth: r#"\displaywidth=3pt \edef\R{\the\displaywidth}"# => "3.0pt";
    /// `\emergencystretch` (tex.web § 247): a dimension parameter.
    p_emergencystretch: r#"\emergencystretch=4pt \edef\R{\the\emergencystretch}"# => "4.0pt";
    /// `\hangindent` (tex.web § 247): a dimension parameter.
    p_hangindent: r#"\hangindent=5pt \edef\R{\the\hangindent}"# => "5.0pt";
    /// `\hfuzz` (tex.web § 247): a dimension parameter.
    p_hfuzz: r#"\hfuzz=6pt \edef\R{\the\hfuzz}"# => "6.0pt";
    /// `\hoffset` (tex.web § 247): a dimension parameter.
    p_hoffset: r#"\hoffset=7pt \edef\R{\the\hoffset}"# => "7.0pt";
    /// `\lineskiplimit` (tex.web § 247): a dimension parameter.
    p_lineskiplimit: r#"\lineskiplimit=8pt \edef\R{\the\lineskiplimit}"# => "8.0pt";
    /// `\maxdepth` (tex.web § 247): a dimension parameter.
    p_maxdepth: r#"\maxdepth=9pt \edef\R{\the\maxdepth}"# => "9.0pt";
    /// `\nulldelimiterspace` (tex.web § 247): a dimension parameter.
    p_nulldelimiterspace: r#"\nulldelimiterspace=10pt \edef\R{\the\nulldelimiterspace}"# => "10.0pt";
    /// `\overfullrule` (tex.web § 247): a dimension parameter.
    p_overfullrule: r#"\overfullrule=11pt \edef\R{\the\overfullrule}"# => "11.0pt";
    /// `\predisplaysize` (tex.web § 247): a dimension parameter.
    p_predisplaysize: r#"\predisplaysize=12pt \edef\R{\the\predisplaysize}"# => "12.0pt";
    /// `\splitmaxdepth` (tex.web § 247): a dimension parameter.
    p_splitmaxdepth: r#"\splitmaxdepth=13pt \edef\R{\the\splitmaxdepth}"# => "13.0pt";
    /// `\vfuzz` (tex.web § 247): a dimension parameter.
    p_vfuzz: r#"\vfuzz=14pt \edef\R{\the\vfuzz}"# => "14.0pt";
    /// `\voffset` (tex.web § 247): a dimension parameter.
    p_voffset: r#"\voffset=15pt \edef\R{\the\voffset}"# => "15.0pt";
    /// `\vsize` (tex.web § 247): a dimension parameter.
    p_vsize: r#"\vsize=16pt \edef\R{\the\vsize}"# => "16.0pt";
    /// `\parindent` (tex.web § 247): a dimension parameter.
    p_parindent: r#"\parindent=17pt \edef\R{\the\parindent}"# => "17.0pt";
    /// `\mathsurround` (tex.web § 247): a dimension parameter.
    p_mathsurround: r#"\mathsurround=18pt \edef\R{\the\mathsurround}"# => "18.0pt";
    /// `\pagedepth` (tex.web § 982): a page dimension, assignable.
    p_pagedepth: r#"\vsize=100pt \hrule height 1pt \penalty0 \pagedepth=1pt \edef\R{\the\pagedepth}"# => "1.0pt";
    /// `\pageshrink` (tex.web § 982): a page dimension, assignable.
    p_pageshrink: r#"\vsize=100pt \hrule height 1pt \penalty0 \pageshrink=2pt \edef\R{\the\pageshrink}"# => "2.0pt";
    /// `\pagestretch` (tex.web § 982): a page dimension, assignable.
    p_pagestretch: r#"\vsize=100pt \hrule height 1pt \penalty0 \pagestretch=3pt \edef\R{\the\pagestretch}"# => "3.0pt";
    /// `\pagefilstretch` (tex.web § 982): a page dimension, assignable.
    p_pagefilstretch: r#"\vsize=100pt \hrule height 1pt \penalty0 \pagefilstretch=4pt \edef\R{\the\pagefilstretch}"# => "4.0pt";
    /// `\pagefillstretch` (tex.web § 982): a page dimension, assignable.
    p_pagefillstretch: r#"\vsize=100pt \hrule height 1pt \penalty0 \pagefillstretch=5pt \edef\R{\the\pagefillstretch}"# => "5.0pt";
    /// `\pagefilllstretch` (tex.web § 982): a page dimension, assignable.
    p_pagefilllstretch: r#"\vsize=100pt \hrule height 1pt \penalty0 \pagefilllstretch=6pt \edef\R{\the\pagefilllstretch}"# => "6.0pt";
    /// `\pagetotal` (tex.web § 982): a page dimension, assignable.
    p_pagetotal: r#"\vsize=100pt \hrule height 1pt \penalty0 \pagetotal=7pt \edef\R{\the\pagetotal}"# => "7.0pt";
    /// `\prevdepth` (tex.web § 418): inside a vbox it is assignable.
    p_prevdepth: r#"\setbox0\vbox{\prevdepth=3pt \xdef\R{\the\prevdepth}}"# => "3.0pt";
    /// `\abovedisplayshortskip` (tex.web § 224): a glue parameter.
    p_abovedisplayshortskip: r#"\abovedisplayshortskip=1pt plus 2pt minus 3pt \edef\R{\the\abovedisplayshortskip}"# => "1.0ptplus2.0ptminus3.0pt";
    /// `\abovedisplayskip` (tex.web § 224): a glue parameter.
    p_abovedisplayskip: r#"\abovedisplayskip=2pt plus 3pt minus 4pt \edef\R{\the\abovedisplayskip}"# => "2.0ptplus3.0ptminus4.0pt";
    /// `\baselineskip` (tex.web § 224): a glue parameter.
    p_baselineskip: r#"\baselineskip=12pt plus 1fil \edef\R{\the\baselineskip}"# => "12.0ptplus1.0fil";
    /// `\belowdisplayshortskip` (tex.web § 224): a glue parameter.
    p_belowdisplayshortskip: r#"\belowdisplayshortskip=3pt plus 4pt \edef\R{\the\belowdisplayshortskip}"# => "3.0ptplus4.0pt";
    /// `\belowdisplayskip` (tex.web § 224): a glue parameter.
    p_belowdisplayskip: r#"\belowdisplayskip=4pt minus 5pt \edef\R{\the\belowdisplayskip}"# => "4.0ptminus5.0pt";
    /// `\lineskip` (tex.web § 224): a glue parameter.
    p_lineskip: r#"\lineskip=1pt plus 1fill \edef\R{\the\lineskip}"# => "1.0ptplus1.0fill";
    /// `\spaceskip` (tex.web § 224): a glue parameter.
    p_spaceskip: r#"\spaceskip=5pt plus 6pt \edef\R{\the\spaceskip}"# => "5.0ptplus6.0pt";
    /// `\splittopskip` (tex.web § 224): a glue parameter.
    p_splittopskip: r#"\splittopskip=6pt minus 1fil \edef\R{\the\splittopskip}"# => "6.0ptminus1.0fil";
    /// `\topskip` (tex.web § 224): a glue parameter.
    p_topskip: r#"\topskip=7pt plus 8pt \edef\R{\the\topskip}"# => "7.0ptplus8.0pt";
    /// `\xspaceskip` (tex.web § 224): a glue parameter.
    p_xspaceskip: r#"\xspaceskip=8pt plus 1filll \edef\R{\the\xspaceskip}"# => "8.0ptplus1.0filll";
    /// `\medmuskip` (tex.web § 224): a muglue parameter.
    p_medmuskip: r#"\medmuskip=4mu plus 2mu minus 4mu \edef\R{\the\medmuskip}"# => "4.0muplus2.0muminus4.0mu";
    /// `\thickmuskip` (tex.web § 224): a muglue parameter.
    p_thickmuskip: r#"\thickmuskip=5mu plus 5mu \edef\R{\the\thickmuskip}"# => "5.0muplus5.0mu";
}

// Part 17: token list parameters (tex.web § 230) and the register shorthands (§ 1224).
pin! {
    /// `\errhelp` (tex.web § 230): a token list parameter.
    p_errhelp: r#"\errhelp={help me}\edef\R{\the\errhelp}"# => "helpme";
    /// `\everycr` (tex.web § 230): a token list parameter.
    p_everycr: r#"\everycr={\relax}\edef\R{\the\everycr}"# => r"\relax";
    /// `\everyjob` (tex.web § 230): a token list parameter.
    p_everyjob: r#"\everyjob={\relax}\edef\R{\the\everyjob}"# => r"\relax";
    /// `\everydisplay` (tex.web § 230): a token list parameter.
    p_everydisplay: r#"\everydisplay={\relax}\edef\R{\the\everydisplay}"# => r"\relax";
    /// `\everymath` (tex.web § 230): a token list parameter.
    p_everymath: r#"\everymath={\relax}\edef\R{\the\everymath}"# => r"\relax";
    /// `\everyvbox` (tex.web § 230): a token list parameter.
    p_everyvbox: r#"\everyvbox={\relax}\edef\R{\the\everyvbox}"# => r"\relax";
    /// `\dimendef` (tex.web § 1224): binds a name to a dimen register.
    p_dimendef: r#"\dimendef\d=7 \d=2pt \edef\R{\the\dimen7}"# => "2.0pt";
    /// `\skipdef` (tex.web § 1224): binds a name to a skip register.
    p_skipdef: r#"\skipdef\s=8 \s=2pt plus 1pt \edef\R{\the\skip8}"# => "2.0ptplus1.0pt";
    /// `\muskipdef` (tex.web § 1224): binds a name to a muskip register.
    p_muskipdef: r#"\muskipdef\m=9 \m=3mu minus 1mu \edef\R{\the\muskip9}"# => "3.0muminus1.0mu";
    /// `\jobname` (tex.web § 472): the name of the main file, expandable.
    p_jobname: r#"\edef\R{\jobname}"# => "texput";
}

// Part 22-24: interaction, dumping and `\show...` (tex.web § 1265, § 1335, § 1293).
pin! {
    /// `\scrollmode` (tex.web § 1265): sets the interaction mode to 2.
    p_scrollmode: r#"\scrollmode \edef\R{\the\interactionmode}"# => "2";
    /// `\showlists` (tex.web § 1293): writes to the log and changes no state.
    p_showlists: r#"\def\R{a}\showlists \edef\R{\R b}"# => "ab";
    /// `\dump` (tex.web § 1335): ends the run at the outer level.
    p_dump: r#"\def\R{a}\dump \def\R{b}"# => "a";
}

// Part 24: glue, leaders, boxes (tex.web § 1057-1082, § 1110, § 977).
pin! {
    /// `\hfil` (tex.web § 1058): appends glue with one `fil` of stretch.
    p_hfil: r#"\setbox0\hbox{\hfil\xdef\R{\the\lastskip}}"# => "0.0ptplus1.0fil";
    /// `\hfill` (tex.web § 1058): appends glue with one `fill` of stretch.
    p_hfill: r#"\setbox0\hbox{\hfill\xdef\R{\the\lastskip}}"# => "0.0ptplus1.0fill";
    /// `\hfilneg` (tex.web § 1058): appends glue with minus one `fil` of stretch.
    p_hfilneg: r#"\setbox0\hbox{\hfilneg\xdef\R{\the\lastskip}}"# => "0.0ptplus-1.0fil";
    /// `\hss` (tex.web § 1058): appends glue stretching and shrinking by one `fil`.
    p_hss: r#"\setbox0\hbox{\hss\xdef\R{\the\lastskip}}"# => "0.0ptplus1.0filminus1.0fil";
    /// `\vfil` (tex.web § 1058): appends vertical glue with one `fil` of stretch.
    p_vfil: r#"\setbox0\vbox{\vfil\xdef\R{\the\lastskip}}"# => "0.0ptplus1.0fil";
    /// `\vfill` (tex.web § 1058): appends vertical glue with one `fill` of stretch.
    p_vfill: r#"\setbox0\vbox{\vfill\xdef\R{\the\lastskip}}"# => "0.0ptplus1.0fill";
    /// `\vfilneg` (tex.web § 1058): appends vertical glue with minus one `fil` of stretch.
    p_vfilneg: r#"\setbox0\vbox{\vfilneg\xdef\R{\the\lastskip}}"# => "0.0ptplus-1.0fil";
    /// `\vss` (tex.web § 1058): appends vertical glue stretching and shrinking by one `fil`.
    p_vss: r#"\setbox0\vbox{\vss\xdef\R{\the\lastskip}}"# => "0.0ptplus1.0filminus1.0fil";
    /// `\cleaders` (tex.web § 1078): leader glue keeps its natural skip.
    p_cleaders: r#"\setbox0\hbox{\cleaders\hrule\hskip 5pt plus 1fil\xdef\R{\the\lastskip}}"# => "5.0ptplus1.0fil";
    /// `\xleaders` (tex.web § 1078): leader glue keeps its natural skip.
    p_xleaders: r#"\setbox0\hbox{\xleaders\hrule\hskip 6pt minus 1fill\xdef\R{\the\lastskip}}"# => "6.0ptminus1.0fill";
    /// `\mskip` (tex.web § 1057): math glue in mu units.
    p_mskip: r#"\catcode`\$=3 $\mskip 3mu plus 1mu\xdef\R{\the\lastskip}$"# => "3.0muplus1.0mu";
    /// `\mkern` (tex.web § 1057): math kern in mu units.
    p_mkern: r#"\catcode`\$=3 $\mkern 3mu\xdef\R{\the\lastkern}$"# => "3.0pt";
    /// `\nonscript` (tex.web § 1171): appends conditional math glue.
    p_nonscript: r#"\catcode`\$=3 ${}\nonscript\xdef\R{\the\lastnodetype}$"# => "11";
    /// `\unvbox` (tex.web § 1110): unpacks a vbox and voids the register.
    p_unvbox: r#"\setbox1\vbox{\hrule height 3pt}\setbox2\vbox{\unvbox1 \xdef\R{\the\lastnodetype,\the\ht1}}"# => "3,0.0pt";
    /// `\unvcopy` (tex.web § 1110): unpacks a copy and keeps the register.
    p_unvcopy: r#"\setbox1\vbox{\hrule height 3pt}\setbox2\vbox{\unvcopy1 }\edef\R{\the\ht1,\the\ht2}"# => "3.0pt,3.0pt";
    /// `\vsplit` (tex.web § 977): splits a vbox at the best break below the height.
    p_vsplit: r#"\setbox1\vbox{\hrule height 4pt \penalty0 \hrule height 4pt}\setbox2\vsplit1 to 4pt \edef\R{\the\ht2,\the\ht1}"# => "4.0pt,4.0pt";
    /// `\splitfirstmark` (tex.web § 979): first mark of the box `\vsplit` cut off.
    p_splitfirstmark: r#"\setbox1\vbox{\mark{a}\hrule height 4pt\penalty0\mark{b}\hrule height 4pt}\setbox2\vsplit1 to 4pt \edef\R{\splitfirstmark}"# => "a";
    /// `\splitbotmark` (tex.web § 979): last mark of the box `\vsplit` cut off.
    p_splitbotmark: r#"\setbox1\vbox{\mark{a}\hrule height 4pt\mark{c}\penalty0\mark{b}\hrule height 4pt}\setbox2\vsplit1 to 4pt \edef\R{\splitbotmark}"# => "c";
    /// `\insert` (tex.web § 1099): its material is in an insert group (11).
    p_insert: r#"\setbox0\vbox{\insert100{\xdef\R{\the\currentgrouptype}}}"# => "11";
    /// `\vadjust` (tex.web § 1097): its material is in an insert group (11).
    p_vadjust: r#"\setbox0\hbox{\vadjust{\xdef\R{\the\currentgrouptype}}}"# => "11";
    /// `\setlanguage` (tex.web § 1376): appends a whatsit in unrestricted horizontal mode.
    p_setlanguage: r#"\setbox0\vbox{\noindent\setlanguage5 \xdef\R{\the\lastnodetype,\the\language}}"# => "9,0";
    /// `\ ` (tex.web § 1041): control space appends the interword glue.
    p_ctrl_space: r#"\setbox0\hbox{\spaceskip=3pt \ \ }\edef\R{\the\wd0}"# => "6.0pt";
    /// `\/` (tex.web § 1113): italic correction after no character adds no node.
    p_ctrl_slash: r#"\setbox0\hbox{\/\xdef\R{\the\lastnodetype}}"# => "-1";
    /// `\-` (tex.web § 1117): appends an empty discretionary.
    p_ctrl_hyphen: r#"\setbox0\hbox{\-\xdef\R{\the\lastnodetype}}"# => "8";
}

// Part 37: alignment (tex.web § 768-812).
pin! {
    /// `\crcr` (tex.web § 785): ignored right after a `\cr`, otherwise ends the row.
    p_crcr: r#"\count1=0 \halign{\global\advance\count1 1 #\cr a\crcr\crcr b\crcr}\edef\R{\the\count1}"# => "2";
    /// `\span` (tex.web § 789): merges two columns, so the second template is used inside the first cell.
    p_span: r#"\catcode`\&=4 \count1=0 \halign{\global\advance\count1 1 #&\global\advance\count1 10 #\cr a\span b\cr}\edef\R{\the\count1}"# => "11";
    /// `\omit` (tex.web § 789): the cell skips its template.
    p_omit: r#"\count1=0 \halign{\global\advance\count1 1 #\cr\omit a\cr b\cr}\edef\R{\the\count1}"# => "1";
    /// `\everycr` (tex.web § 785): inserted after the preamble and after every `\cr`.
    p_everycr_effect: r#"\count1=0 \everycr={\noalign{\global\advance\count1 1 }}\halign{#\cr a\cr b\cr}\edef\R{\the\count1}"# => "3";
}

// Part 48: math (tex.web § 1136-1206).
pin! {
    /// `\eqno` (tex.web § 1140): the equation number is set in a math shift group (15).
    p_eqno: r#"\catcode`\$=3 \hsize=100pt \parindent=0pt $$\eqno\xdef\R{\the\currentgrouptype}$$"# => "15";
    /// `\leqno` (tex.web § 1140): the equation number is set in a math shift group (15).
    p_leqno: r#"\catcode`\$=3 \hsize=100pt \parindent=0pt $$\leqno\xdef\R{\the\currentgrouptype}$$"# => "15";
    /// `\mathord` (tex.web § 1151): its argument is a math group (9).
    p_mathord: r#"\catcode`\$=3 $\mathord{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\mathop` (tex.web § 1151): its argument is a math group (9).
    p_mathop: r#"\catcode`\$=3 $\mathop{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\mathbin` (tex.web § 1151): its argument is a math group (9).
    p_mathbin: r#"\catcode`\$=3 $\mathbin{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\mathrel` (tex.web § 1151): its argument is a math group (9).
    p_mathrel: r#"\catcode`\$=3 $\mathrel{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\mathopen` (tex.web § 1151): its argument is a math group (9).
    p_mathopen: r#"\catcode`\$=3 $\mathopen{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\mathclose` (tex.web § 1151): its argument is a math group (9).
    p_mathclose: r#"\catcode`\$=3 $\mathclose{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\mathpunct` (tex.web § 1151): its argument is a math group (9).
    p_mathpunct: r#"\catcode`\$=3 $\mathpunct{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\mathinner` (tex.web § 1151): its argument is a math group (9).
    p_mathinner: r#"\catcode`\$=3 $\mathinner{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\underline` (tex.web § 1151): its argument is a math group (9).
    p_underline: r#"\catcode`\$=3 $\underline{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\overline` (tex.web § 1151): its argument is a math group (9).
    p_overline: r#"\catcode`\$=3 $\overline{\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\radical` (tex.web § 1163): its radicand is a math group (9).
    p_radical: r#"\catcode`\$=3 $\radical"161 {\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\mathaccent` (tex.web § 1165): its nucleus is a math group (9).
    p_mathaccent: r#"\catcode`\$=3 $\mathaccent"7016 {\xdef\R{\the\currentgrouptype}}$"# => "9";
    /// `\mathchoice` (tex.web § 1172): each of the four branches is a math choice group (13).
    p_mathchoice: r#"\catcode`\$=3 $\mathchoice{}{}{}{\xdef\R{\the\currentgrouptype}}$"# => "13";
    /// `\left` (tex.web § 1191): the delimited material is a math left group (16).
    p_left: r#"\catcode`\$=3 $\left(\xdef\R{\the\currentgrouptype}\right)$"# => "16";
    /// `\right` (tex.web § 1191): closes a math left group, back to math shift (15).
    p_right: r#"\catcode`\$=3 $\left(\right)\xdef\R{\the\currentgrouptype}$"# => "15";
    /// `\delimiter` (tex.web § 1160): a delimiter in math mode starts no group.
    p_delimiter: r#"\catcode`\$=3 $\delimiter"4162362 \xdef\R{\the\currentgrouptype}$"# => "15";
    /// `\atop` (tex.web § 1178): ends the numerator inside the braces, leaving a fraction as last node.
    p_atop: r#"\catcode`\$=3 ${a\atop b}\xdef\R{\the\lastnodetype}$"# => "15";
    /// `\atopwithdelims` (tex.web § 1178): fraction without rule and with delimiters.
    p_atopwithdelims: r#"\catcode`\$=3 ${a\atopwithdelims() b}\xdef\R{\the\lastnodetype}$"# => "15";
    /// `\abovewithdelims` (tex.web § 1178): fraction with explicit rule thickness and delimiters.
    p_abovewithdelims: r#"\catcode`\$=3 ${a\abovewithdelims() 1pt b}\xdef\R{\the\lastnodetype}$"# => "15";
    /// `\overwithdelims` (tex.web § 1178): fraction with default rule thickness and delimiters.
    p_overwithdelims: r#"\catcode`\$=3 ${a\overwithdelims() b}\xdef\R{\the\lastnodetype}$"# => "15";
    /// `\limits` (tex.web § 1159): only changes how the preceding operator sets its scripts.
    p_limits: r#"\catcode`\$=3 $\mathop{a}\limits\xdef\R{\the\currentgrouptype}$"# => "15";
    /// `\nolimits` (tex.web § 1159): only changes how the preceding operator sets its scripts.
    p_nolimits: r#"\catcode`\$=3 $\mathop{a}\nolimits\xdef\R{\the\currentgrouptype}$"# => "15";
    /// `\displaylimits` (tex.web § 1159): only changes how the preceding operator sets its scripts.
    p_displaylimits: r#"\catcode`\$=3 $\mathop{a}\displaylimits\xdef\R{\the\currentgrouptype}$"# => "15";
    /// `\displaystyle` (tex.web § 1169): a style change starts no group.
    p_displaystyle: r#"\catcode`\$=3 $\displaystyle\xdef\R{\the\currentgrouptype}$"# => "15";
    /// `\textstyle` (tex.web § 1169): a style change starts no group.
    p_textstyle: r#"\catcode`\$=3 $\textstyle\xdef\R{\the\currentgrouptype}$"# => "15";
    /// `\scriptstyle` (tex.web § 1169): a style change starts no group.
    p_scriptstyle: r#"\catcode`\$=3 $\scriptstyle\xdef\R{\the\currentgrouptype}$"# => "15";
    /// `\scriptscriptstyle` (tex.web § 1169): a style change starts no group.
    p_scriptscriptstyle: r#"\catcode`\$=3 $\scriptscriptstyle\xdef\R{\the\currentgrouptype}$"# => "15";
}

// e-TeX (etex.ch).
pin! {
    /// `\TeXXeTstate` (etex.ch): the TeX--XeT switch, an integer parameter.
    p_texxetstate: r#"\TeXXeTstate=1 \edef\R{\the\TeXXeTstate}"# => "1";
    /// `\beginL` (etex.ch): with TeX--XeT on, appends a math node (type 10).
    p_beginl: r#"\TeXXeTstate=1 \setbox0\hbox{\beginL\xdef\R{\the\lastnodetype}}"# => "10";
    /// `\beginR` (etex.ch): with TeX--XeT on, appends a math node (type 10).
    p_beginr: r#"\TeXXeTstate=1 \setbox0\hbox{\beginR\xdef\R{\the\lastnodetype}}"# => "10";
    /// `\endL` (etex.ch): with TeX--XeT on, closes `\beginL` with a math node (type 10).
    p_endl: r#"\TeXXeTstate=1 \setbox0\hbox{\beginL\endL\xdef\R{\the\lastnodetype}}"# => "10";
    /// `\endR` (etex.ch): with TeX--XeT on, closes `\beginR` with a math node (type 10).
    p_endr: r#"\TeXXeTstate=1 \setbox0\hbox{\beginR\endR\xdef\R{\the\lastnodetype}}"# => "10";
    /// `\tracinggroups` (etex.ch): an integer parameter.
    p_tracinggroups: r#"\tracinggroups=1 \edef\R{\the\tracinggroups}"# => "1";
    /// `\tracingifs` (etex.ch): an integer parameter.
    p_tracingifs: r#"\tracingifs=1 \edef\R{\the\tracingifs}"# => "1";
    /// `\tracingnesting` (etex.ch): an integer parameter.
    p_tracingnesting: r#"\tracingnesting=2 \edef\R{\the\tracingnesting}"# => "2";
    /// `\tracingscantokens` (etex.ch): an integer parameter.
    p_tracingscantokens: r#"\tracingscantokens=1 \edef\R{\the\tracingscantokens}"# => "1";
    /// `\predisplaydirection` (etex.ch): an integer parameter.
    p_predisplaydirection: r#"\predisplaydirection=-1 \edef\R{\the\predisplaydirection}"# => "-1";
    /// `\savinghyphcodes` (etex.ch): an integer parameter.
    p_savinghyphcodes: r#"\savinghyphcodes=1 \edef\R{\the\savinghyphcodes}"# => "1";
    /// `\lastlinefit` (etex.ch): an integer parameter.
    p_lastlinefit: r#"\lastlinefit=500 \edef\R{\the\lastlinefit}"# => "500";
    /// `\savingvdiscards` (etex.ch): an integer parameter.
    p_savingvdiscards: r#"\savingvdiscards=1 \edef\R{\the\savingvdiscards}"# => "1";
    /// `\clubpenalties` (etex.ch): index 0 is the count, indexes past the end repeat the last entry.
    p_clubpenalties: r#"\clubpenalties=3 100 200 300 \edef\R{\the\clubpenalties0,\the\clubpenalties2,\the\clubpenalties5}"# => "3,200,300";
    /// `\widowpenalties` (etex.ch): index 0 is the count, indexes past the end repeat the last entry.
    p_widowpenalties: r#"\widowpenalties=2 7 8 \edef\R{\the\widowpenalties0,\the\widowpenalties1,\the\widowpenalties9}"# => "2,7,8";
    /// `\displaywidowpenalties` (etex.ch): index 0 is the count, indexes past the end repeat the last entry.
    p_displaywidowpenalties: r#"\displaywidowpenalties=2 5 6 \edef\R{\the\displaywidowpenalties0,\the\displaywidowpenalties1,\the\displaywidowpenalties4}"# => "2,5,6";
    /// `\mutoglue` (etex.ch): converts a muglue to glue, same numbers in pt.
    p_mutoglue: r#"\edef\R{\the\mutoglue\muexpr 3mu plus 1fill\relax}"# => "3.0ptplus1.0fill";
    /// `\fontcharwd` (etex.ch): width of a character, 0pt when the font lacks it.
    p_fontcharwd: r#"\edef\R{\the\fontcharwd\nullfont 65 }"# => "0.0pt";
    /// `\fontcharht` (etex.ch): height of a character, 0pt when the font lacks it.
    p_fontcharht: r#"\edef\R{\the\fontcharht\nullfont 65 }"# => "0.0pt";
    /// `\fontchardp` (etex.ch): depth of a character, 0pt when the font lacks it.
    p_fontchardp: r#"\edef\R{\the\fontchardp\nullfont 65 }"# => "0.0pt";
    /// `\fontcharic` (etex.ch): italic correction of a character, 0pt when the font lacks it.
    p_fontcharic: r#"\edef\R{\the\fontcharic\nullfont 65 }"# => "0.0pt";
    /// `\marks` (etex.ch): `\marks3` is read back with `\splitfirstmarks3` after a split.
    p_marks: r#"\setbox1\vbox{\marks3{x}\hrule height 5pt}\setbox2\vsplit1 to 5pt \edef\R{\splitfirstmarks3}"# => "x";
    /// `\splitfirstmarks` (etex.ch): first mark of class 3 in the split-off box.
    p_splitfirstmarks: r#"\setbox1\vbox{\marks3{a}\hrule height 4pt\penalty0\marks3{b}\hrule height 4pt}\setbox2\vsplit1 to 4pt \edef\R{\splitfirstmarks3}"# => "a";
    /// `\splitbotmarks` (etex.ch): last mark of class 3 in the split-off box.
    p_splitbotmarks: r#"\setbox1\vbox{\marks3{a}\hrule height 4pt\marks3{c}\penalty0\marks3{b}\hrule height 4pt}\setbox2\vsplit1 to 4pt \edef\R{\splitbotmarks3}"# => "c";
    /// `\firstmarks` (etex.ch): first mark of class 3 on the page, read in the output routine.
    p_firstmarks: r#"\vsize=20pt \output={\xdef\R{\firstmarks3}\setbox0\box255 \deadcycles=0 }\marks3{a}\hrule height 5pt\marks3{b}\penalty-10000 "# => "a";
    /// `\botmarks` (etex.ch): last mark of class 3 on the page, read in the output routine.
    p_botmarks: r#"\vsize=20pt \output={\xdef\R{\botmarks3}\setbox0\box255 \deadcycles=0 }\marks3{a}\hrule height 5pt\marks3{b}\penalty-10000 "# => "b";
    /// `\topmarks` (etex.ch): the previous page's bottom mark of class 3.
    p_topmarks: r#"\vsize=20pt \output={\xdef\R{\topmarks3}\setbox0\box255 \deadcycles=0 }\marks3{a}\hrule height 5pt\penalty-10000 \marks3{b}\hrule height 5pt\penalty-10000 "# => "a";
    /// `\splitdiscards` (etex.ch): the glue discarded at a `\vsplit` break.
    p_splitdiscards: r#"\savingvdiscards=1 \setbox1\vbox{\hrule height 5pt\penalty0\vskip 3pt\hrule height 5pt}\setbox2\vsplit1 to 5pt \setbox3\vbox{\splitdiscards}\edef\R{\the\ht3}"# => "3.0pt";
    /// `\pagediscards` (etex.ch): the glue discarded at a page break, read in the output routine.
    p_pagediscards: r#"\vsize=20pt \savingvdiscards=1 \output={\setbox0\box255 \setbox3\vbox{\pagediscards}\xdef\R{\the\ht3}\deadcycles=0 }\hrule height 5pt\penalty-10000 \vskip 3pt\hrule height 5pt\penalty-10000 "# => "3.0pt";
    /// `\showtokens` (etex.ch): writes to the log and changes no state.
    p_showtokens: r#"\def\R{a}\showtokens{xy}\edef\R{\R b}"# => "ab";
    /// `\showgroups` (etex.ch): writes to the log and changes no state.
    p_showgroups: r#"\def\R{a}\begingroup\showgroups\endgroup\edef\R{\R b}"# => "ab";
    /// `\showifs` (etex.ch): writes to the log and changes no state.
    p_showifs: r#"\def\R{a}\iftrue\showifs\fi\edef\R{\R b}"# => "ab";
    /// `\muexpr` (etex.ch): evaluates a mu expression with glue arithmetic.
    p_muexpr: r#"\edef\R{\the\muexpr 2mu*3+1mu plus 1fil\relax}"# => "7.0muplus1.0fil";
    /// `\middle` (etex.ch): inside `\left...\right` it keeps the math left group (16).
    p_middle: r#"\catcode`\$=3 $\left(\middle|\xdef\R{\the\currentgrouptype}\right)$"# => "16";
    /// `\interactionmode` (etex.ch): assigning it selects the mode, 1 is `\nonstopmode`.
    p_interactionmode: r#"\interactionmode=1 \edef\R{\the\interactionmode}"# => "1";
    /// `\tracingassigns` (etex.ch): an integer parameter.
    p_tracingassigns: r#"\tracingassigns=1 \edef\R{\the\tracingassigns}"# => "1";
}

// More tex.web primitives whose only other mention is incidental.
pin! {
    /// `\outputpenalty` (tex.web § 1012): the penalty at the page break, read in the output routine.
    p_outputpenalty: r#"\vsize=20pt \output={\xdef\R{\the\outputpenalty}\setbox0\box255 \deadcycles=0 }\hrule height 5pt\penalty-10000 "# => "-10000";
    /// `\batchmode` (tex.web § 1265): sets the interaction mode to 0.
    p_batchmode: r#"\batchmode \edef\R{\the\interactionmode}"# => "0";
    /// `\errorstopmode` (tex.web § 1265): sets the interaction mode to 3.
    p_errorstopmode: r#"\errorstopmode \edef\R{\the\interactionmode}"# => "3";
    /// `\boxmaxdepth` (tex.web § 247): a dimension parameter.
    p_boxmaxdepth: r#"\boxmaxdepth=3pt \edef\R{\the\boxmaxdepth}"# => "3.0pt";
    /// `\defaultskewchar` (tex.web § 236): an integer parameter.
    p_defaultskewchar: r#"\defaultskewchar=7 \edef\R{\the\defaultskewchar}"# => "7";
    /// `\skewchar` (tex.web § 1253): a font integer, assignable per font.
    p_skewchar: r#"\skewchar\nullfont=5 \edef\R{\the\skewchar\nullfont}"# => "5";
    /// `\everyhbox` (tex.web § 230): a token list parameter.
    p_everyhbox: r#"\everyhbox={\relax}\edef\R{\the\everyhbox}"# => r"\relax";
    /// `\leftskip` (tex.web § 224): a glue parameter.
    p_leftskip: r#"\leftskip=1pt plus 2pt \edef\R{\the\leftskip}"# => "1.0ptplus2.0pt";
    /// `\rightskip` (tex.web § 224): a glue parameter.
    p_rightskip: r#"\rightskip=2pt minus 1pt \edef\R{\the\rightskip}"# => "2.0ptminus1.0pt";
    /// `\tabskip` (tex.web § 224): a glue parameter.
    p_tabskip: r#"\tabskip=3pt plus 1fil \edef\R{\the\tabskip}"# => "3.0ptplus1.0fil";
    /// `\thinmuskip` (tex.web § 224): a muglue parameter.
    p_thinmuskip: r#"\thinmuskip=3mu minus 1mu \edef\R{\the\thinmuskip}"# => "3.0muminus1.0mu";
    /// `\toksdef` (tex.web § 1224): binds a name to a token register.
    p_toksdef: r#"\toksdef\t=3 \t={ab}\edef\R{\the\toks3}"# => "ab";
    /// `\day` (tex.web § 236): an integer parameter, assignable.
    p_day: r#"\day=9 \edef\R{\the\day}"# => "9";
    /// `\tracingcommands` (tex.web § 236): an integer parameter.
    p_tracingcommands: r#"\tracingcommands=2 \edef\R{\the\tracingcommands}"# => "2";
    /// `\tracingmacros` (tex.web § 236): an integer parameter.
    p_tracingmacros: r#"\tracingmacros=2 \edef\R{\the\tracingmacros}"# => "2";
    /// `\tracingonline` (tex.web § 236): an integer parameter.
    p_tracingonline: r#"\tracingonline=1 \edef\R{\the\tracingonline}"# => "1";
    /// `\show` (tex.web § 1293): writes to the log and changes no state.
    p_show: r#"\def\R{a}\show\R \edef\R{\R b}"# => "ab";
    /// `\showbox` (tex.web § 1296): writes to the log and changes no state.
    p_showbox: r#"\def\R{a}\setbox0\hbox{}\showbox0 \edef\R{\R b}"# => "ab";
    /// `\hyphenation` (tex.web § 934): adds exceptions and changes no macro state.
    p_hyphenation: r#"\def\R{a}\hyphenation{ab-c}\edef\R{\R b}"# => "ab";
    /// `\patterns` (tex.web § 960): in INITEX it adds patterns and changes no macro state.
    p_patterns: r#"\def\R{a}\patterns{a1b}\edef\R{\R b}"# => "ab";
    /// `\scriptfont` (tex.web § 1230): a math font slot, assignable.
    p_scriptfont: r#"\scriptfont3=\nullfont \edef\R{\fontname\scriptfont3}"# => "nullfont";
    /// `\scriptscriptfont` (tex.web § 1230): a math font slot, assignable.
    p_scriptscriptfont: r#"\scriptscriptfont3=\nullfont \edef\R{\fontname\scriptscriptfont3}"# => "nullfont";
    /// `\leaders` (tex.web § 1078): leader glue keeps its natural skip.
    p_leaders: r#"\setbox0\hbox{\leaders\hrule\hskip 5pt\xdef\R{\the\lastskip}}"# => "5.0pt";
    /// `\noboundary` (tex.web § 1045): appends no node.
    p_noboundary: r#"\setbox0\hbox{\noboundary\xdef\R{\the\lastnodetype}}"# => "-1";
    /// `\raise` (tex.web § 1073): shifts a box up, so the enclosing box gets that height.
    p_raise: r#"\setbox0\hbox{\raise3pt\hbox{}}\edef\R{\the\ht0}"# => "3.0pt";
    /// `\lower` (tex.web § 1073): shifts a box down, so the enclosing box gets that depth.
    p_lower: r#"\setbox0\hbox{\lower3pt\hbox{}}\edef\R{\the\dp0}"# => "3.0pt";
    /// `\moveright` (tex.web § 1073): shifts a box right, widening the enclosing vbox.
    p_moveright: r#"\setbox0\vbox{\moveright5pt\hbox to 2pt{}}\edef\R{\the\wd0}"# => "7.0pt";
    /// `\moveleft` (tex.web § 1073): shifts a box left, narrowing the enclosing vbox.
    p_moveleft: r#"\setbox0\vbox{\moveleft5pt\hbox to 8pt{}}\edef\R{\the\wd0}"# => "3.0pt";
}
