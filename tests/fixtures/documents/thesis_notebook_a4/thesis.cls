\NeedsTeXFormat{LaTeX2e}
\def\thesis@classname{thesis}
\ProvidesClass{\thesis@classname}[2024/12/24 Florian Sihler - Template for Theses]
\def\thesis@warn{\ClassWarning{\thesis@classname}}
\def\thesis@error{\ClassError{\thesis@classname}}

% region setup-keys
\RequirePackage{kvoptions}
\SetupKeyvalOptions{family=thesis,prefix=thesis@}

% general
\DeclareBoolOption{draft}
\DeclareBoolOption{verbose}
\DeclareStringOption[_config]{configpath}
\DeclareStringOption[uulm-sp]{profile}
\DeclareStringOption[master-thesis]{type}
\DeclareStringOption[CCBY]{license}
\DeclareStringOption[\translate{computer-science}]{field}
\DeclareBoolOption{print}
\DeclareComplementaryOption{digital}{print}

% layout
\DeclareStringOption[10pt]{fontsize}
\DeclareStringOption[a4]{paper}
\DeclareBoolOption[true]{oneside}
\DeclareBoolOption[true]{enhanceMath}
\DeclareComplementaryOption{twoside}{oneside}
% also this option still exists, disabling the marginpar is no longer (fully) supported
\DeclareBoolOption[true]{marginpar}
\DeclareComplementaryOption{nomarginpar}{marginpar}
\DeclareBoolOption[true]{citeInMarginpar}
\DeclareBoolOption[false]{sftitle}
\DeclareComplementaryOption{rmtitle}{sftitle}
\DeclareBoolOption[true]{pageInFloatRef}
\DeclareStringOption[auto]{floatRefSidenotes}

% links
\DeclareBoolOption[true]{hyperref}
\DeclareComplementaryOption{nohyperref}{hyperref}

% fonts
\DeclareBoolOption[true]{beramono}
\DeclareBoolOption[true]{palatino}
\DeclareBoolOption[true]{notomath}
\DeclareBoolOption[true]{fira}

\ProcessKeyvalOptions*

\newif\ifthesis@floatRefSidenotes
\def\thesis@@val{auto}
\ifx\thesis@floatRefSidenotes\thesis@@val
    \ifthesis@pageInFloatRef\else\thesis@floatRefSidenotestrue\fi
\else
    \def\thesis@@val{true}%
    \ifx\thesis@floatRefSidenotes\thesis@@val\thesis@floatRefSidenotestrue\else
        \def\thesis@@val{false}%
        \ifx\thesis@floatRefSidenotes\thesis@@val\else
            \thesis@error{Invalid value '\thesis@floatRefSidenotes' for option floatRefSidenotes}{Please use 'true', 'false', or 'auto'.}%
        \fi
    \fi
\fi
% endregion

% region deal with verbose
\def\thesis@info{\ClassInfo{\thesis@classname}}
\ifthesis@verbose
    \errorcontextlines=9999
    \let\thesis@log\typeout
    \let\thesis@debug\typeout
\else
    \let\thesis@log\@gobble
    \let\thesis@debug\@gobble
\fi
% endregion

\RequirePackage{etoolbox}

\def\InputFrom#1{\thesis@info{Adding '#1' to the input path.}\appto\input@path{{#1/}}\appto\graphicspath{{#1/}}}

% region setup the input path
\expandafter\ifblank\expandafter{\thesis@configpath}{:
    \thesis@debug{No config path given, does not set up \string\input@path}%
}{%
    \InputFrom{\thesis@configpath}%
}
% endregion

% region load data
\def\thesis@internal@path{internal/}
% may be defined before \documentclass to relocate the profile folder
\providecommand*\thesisProfilesPath{profiles/\thesis@profile}
\def\thesis@profiles@path{\thesisProfilesPath}

\def\thesis@load@profile#1{%
    \InputIfFileExists{\thesis@profiles@path/#1.tex}{%
         \thesis@info{Loaded '#1' from profile '\thesis@profile' (from: '\thesis@profiles@path')}%
    }{%
        \thesis@error{Profile '\thesis@profile' does not have '#1' (searched at \thesis@profiles@path/#1.tex)}{Please make sure the configuration file is present.}%
    }%
}
\def\thesis@internal@load#1{\makeatletter
    \InputIfFileExists{\thesis@internal@path/#1.tex}{%
         \thesis@log{Loaded internal::#1}%
    }{%
        \thesis@error{unable to find '#1' internally (searched at \thesis@internal@path/#1.tex)}{Please make sure the file is present.}%
    }%
}
\let\ThesisModule\thesis@internal@load
% endregion

% region load class
\PassOptionsToClass{
   fontsize=\thesis@fontsize,
   paper=\thesis@paper,
   numbers=noendperiod,
   chapterprefix=true,
   appendixprefix=true,
   titlepage=firstiscover
}{scrbook}

\ifthesis@oneside
    \PassOptionsToClass{twoside=false}{scrbook}
\else
    \PassOptionsToClass{twoside=true}{scrbook}
\fi

\ifthesis@draft
    \PassOptionsToClass{draft=true,overfullrule=true}{scrbook}
\fi

\LoadClass{scrbook}
\RequirePackage{scrhack}
\RequirePackage{scrwfile}
\RequirePackage{iftex}
\ifthesis@draft
    \RequirePackage[draft=false]{scrlayer-scrpage}
\fi
\ifluatex
\RequirePackage{polyglossia}
\setmainlanguage{english}
\else
\RequirePackage{babel}
\fi
% endregion

\ifthesis@draft
\long\def\hideindraft#1{}
\else
\long\def\hideindraft#1{#1}
\fi

\def\thesis@voidpar{{\parskip=0pt\parindent=0pt\par}}

\thesis@internal@load{translation}
\thesis@internal@load{colors}
\thesis@internal@load{fonts}
\thesis@internal@load{page}
\thesis@internal@load{headfoot}
\thesis@internal@load{layout}
\thesis@internal@load{marginpar}
\thesis@internal@load{tables}
\thesis@internal@load{footnote}
\thesis@internal@load{chapter}
\thesis@internal@load{license}
\thesis@internal@load{floats}
\thesis@internal@load{titlepage}
\RequirePackage{mathtools}
\thesis@internal@load{hyperref}
\thesis@internal@load{biblatex}

\thesis@internal@load{environments}
\thesis@load@profile{defaults}

% TODO: one format fmt for default configuration?
% TODO: externalize support?

\endinput