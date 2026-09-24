# chktex warning coverage

Source: `/usr/local/texlive/2026/texmf-dist/doc/chktex/ChkTeX.pdf` (ChkTeX
v1.7.9, TeX Live 2026), section 8 "Explanation of error messages" (pp. 28-39),
read via `pdftotext`. Warnings 32-34, 36-37 and 47-48 are documented as pairs
or triples in that section; they are listed separately below. Default
on/off state and message wording were cross-checked by running the installed
`/usr/local/bin/chktex` (v1.7.9) against small test files, and the
`Primitives`, `NonItalic`, `Italic`, `Linker`, `NotPreSpaced`, `MathEnvir` and
`NoCharNext` lists were read from
`/usr/local/texlive/2026/texmf-dist/chktex/chktexrc`, which ships as chktex's
own example configuration and documents chktex's default settings for the
list-valued warnings.

chktex works line-by-line on raw source text and tracks the literal
characters of the document: spacing, quote characters, dashes, periods,
parentheses, and whether the file is currently inside `$...$` or `$$...$$`.
satex is an abstract interpreter of TeX's macro layer: `analysis.facts`
records definitions, macro expansions (with the *macro's own* arguments, for
macros satex knows how to read), structured occurrences (`\label`, `\ref`,
`\cite`, `\includegraphics`, environment `\begin`/`\end`, section titles, and
a handful of other kernel interfaces), loads and diagnostics. It does not
tokenize `$`, quote characters or plain prose into any fact, and a `Rule` is
given `analysis`, not the source text, so it cannot fall back to reading the
file itself. That single gap rules out most of chktex's checks: they all key
off characters satex never turns into a fact.

31 of chktex's 49 warnings are about text satex cannot see at all (spacing,
quotes, dashes, ellipsis dots, parenthesis spacing, italic correction,
punctuation position, the character before or after a command) and are marked
"no" below with the specific character or context that would be needed. 6
more depend on a math-mode state satex's interpreter does not track — it
treats `$` as an ordinary character token, so there is no "are we in math
mode" fact to test. 2 (20, 44) are chktex's *user-configurable* pattern
warnings, with no satex equivalent to hang them off. 2 (47, 48) are
ConTeXt-only and satex only interprets LaTeX/plain TeX. 1 (14) is a limitation
of chktex's own line-based parser that satex, being a real interpreter, does
not share. 1 (22) has no fact to hang off at all, since `%` comments are
discarded by the tokenizer before the interpreter ever sees them.

That leaves 4 warnings (9, 10, 15, 41) newly covered by two rules added in
this change, and 1 (27) already covered by an existing rule.

| # | chktex warning | satex rule | checkable from facts? |
|---|---|---|---|
| 1 | Command terminated with space. | none | no — needs the character right after the control sequence name; not tokenized into any fact |
| 2 | Non-breaking space (`~`) should have been used (before `\ref`, `\vref`, `\pageref`, `\eqref`, `\cite`). | none | no — needs the character immediately before the `\ref`/`\cite` call; `Occurrence` has no "preceding token" field |
| 3 | You should enclose the previous parenthesis with `{}`. | none | no — needs raw math-mode superscript/subscript text |
| 4 | Italic correction (`\/`) found in non-italic buffer. | none | no — needs a running "are we in an italic font" state over plain text, which satex does not track (font switches are just macro calls to satex) |
| 5 | Italic correction (`\/`) found more than once. | none | no — same as 4 |
| 6 | No italic correction (`\/`) found. | none | no — same as 4 |
| 7 | Accent command `command` needs use of `command` (dotless i/j). | none | no — the accent primitives (`\'`, `` \` ``, `\^`, …) are not modeled, so their argument is never captured as a fact |
| 8 | Wrong length of dash may have been used. | none | no — needs the literal run of `-` characters and their neighbors |
| 9 | `'%s'` expected, found `'%s'` (bracket/environment mismatch). | `environment-mismatch` (new) | environments: yes, via `OccKind::BeginEnvironment`/`EndEnvironment`. `{`/`\begingroup` mismatches are also already caught by the interpreter itself as it runs (`mismatched-group`, surfaced through `analysis-imprecision`), since a real run has to track group kind to know how `}` and `\endgroup` behave. Generic `[]`/`()` bracket matching: no, LaTeX gives those no paired meaning at the TeX level, so there is nothing for the interpreter to track |
| 10 | Solo `'%s'` found (unmatched close). | `environment-mismatch` (new) | environments: yes (same rule as 9). `}`/`\endgroup` with nothing open: yes, already surfaced as `extra-right-brace`/`mismatched-group` via `analysis-imprecision`. Generic brackets: no |
| 11 | You should use `\ldots`/`\cdots` to achieve an ellipsis. | none | no — needs the literal `.` characters in ordinary text or math; only present inside a modeled macro's own arguments, not general prose |
| 12 | Interword spacing (`\ `) should perhaps be used. | none | no — needs the word before an abbreviation and the space/newline after it |
| 13 | Intersentence spacing (`\@`) should perhaps be used. | none | no — same as 12, for sentence-ending periods |
| 14 | Could not find argument for command. | none | not applicable — this is chktex's own line-by-line parser losing an argument that spans a line break; satex is a real interpreter and reads across lines, so it does not have this failure mode |
| 15 | No match found for `'%s'` (unmatched open). | `environment-mismatch` (new) | environments: yes (same rule as 9/10, reported for anything left open at EOF). `{`/`\begingroup` left open at EOF: yes, already surfaced as `unbalanced-group`/`unbalanced-file` via `analysis-imprecision`. Generic brackets: no |
| 16 | Mathmode still on at end of LaTeX file. | none | no — satex does not model math mode as a state; `$` is just a character token |
| 17 | Number of `character` doesn't match the number of `character`. | none, but related | no dedicated rule for chktex's general character-pair count check; satex's own interpreter already reports the brace-balance problems it hits while actually running the document — `unbalanced-group`, `unbalanced-file`, `extra-right-brace`, `mismatched-group` — all surfaced through the existing `analysis-imprecision` rule, which covers the common case (`{`/`}`) but not arbitrary character pairs |
| 18 | You should use `` ` `` or `''` as an alternative to `"`. | none | no — needs the literal `"` character in text |
| 19 | You should use `'` (ASCII 39) instead of `´` (ASCII 180). | none | no — needs the literal character |
| 20 | User-specified pattern found (`UserWarn`). | none | not applicable — chktex-config-only feature (a list of literal strings to flag); satex has no equivalent user-pattern list, and doing this at the fact level is not possible since it is meant to match arbitrary text |
| 21 | This command might not be intended (`\TeX. Right?` instead of `\TeX\. Right?`). | none | no — needs the character right after the control sequence |
| 22 | Comment displayed. | none | not applicable — `%` comments are discarded by TeX's own tokenizer before satex's interpreter ever sees them; there is no fact to hang this on |
| 23 | Either ``` `` ``` or ``` '' ``` will look better (three quotes in a row). | none | no — needs literal quote characters |
| 24 | Delete this space to maintain correct page references (space before `\index`/`\label`). | none | no — needs the character before the call |
| 25 | You might wish to put this between a pair of `{}` (multi-digit sub/superscript). | none | no — needs literal math-mode digit/letter runs |
| 26 | You ought to remove spaces in front of punctuation. | none | no — needs literal text spacing |
| 27 | Could not execute LaTeX command (chiefly: `\input` file does not exist). | `unresolved-file` (existing) | yes — already covered; `unresolved-file` reports `\input`/`\include`/`\usepackage`/`\documentclass` targets that satex's own distribution search could not find |
| 28 | Don't use `\/` in front of small punctuation. | none | no — same text/font-state gap as 4-6 |
| 29 | `$\times$` may look prettier here (`640x200`). | none | no — needs the literal digit-x-digit text |
| 30 | Multiple spaces detected in output. | none | no — needs literal whitespace runs |
| 31 | This text may be ignored (trailing text on the `\end{verbatim}` line). | none | no — satex's verbatim reader does not record what, if anything, followed the closing tag on its line |
| 32 | Use `` ` `` to begin quotation, not `'`. | none | no — needs literal quote characters |
| 33 | Use `'` to end quotation, not `` ` ``. | none | no — same as 32 |
| 34 | Don't mix quotes. | none | no — same as 32 |
| 35 | You should perhaps use `cmd` instead (roman math operator, e.g. `sin` vs `\sin`). | none | no — needs literal math-mode letter runs |
| 36 | You should put a space in front of/after parenthesis. | none | no — needs literal text spacing |
| 37 | You should avoid spaces in front of/after parenthesis. | none | no — same as 36 |
| 38 | You should not use punctuation in front of/after quotes. | none | no — needs literal text |
| 39 | Double space found (space next to a hard space). | none | no — needs literal whitespace |
| 40 | You should put punctuation outside/inside (inner/display) math mode. | none | no — math mode is not modeled |
| 41 | You ought to not use primitive TeX in LaTeX code. | `primitive-tex-command` (new) | yes — every control-sequence use, defined or not, is recorded in `analysis.facts.expansions` with its name, so chktexrc's default `Primitives` list can be matched by name alone |
| 42 | You should remove spaces in front of `'%s'` (`\footnote`, `\footnotemark`, `\/`). | none | no — needs the character before the call |
| 43 | `'%s'` is normally not followed by `'%c'` (`\left`/`\right` before `{`, `}`, `$`). | none | no — `\left`/`\right` are not modeled at all (satex has no math-mode primitives), and even if they were, the next character is not a recorded fact |
| 44 | User Regex (`UserWarnRegex`). | none | not applicable — same as 20, but regex-based |
| 45 | Use `\[ ... \]` instead of `$$ ... $$`. | none | no — `$` is not tokenized into any fact |
| 46 | Use `\( ... \)` instead of `$ ... $`. | none | no — same as 45 |
| 47 | `'%s'` expected, found `'%s'` (ConTeXt `\start.../\stop...`). | none | not applicable — satex interprets LaTeX/plain TeX, not ConTeXt; `\start`/`\stop` pairs are not modeled |
| 48 | Solo `'%s'` found (ConTeXt). | none | not applicable — same as 47 |
| 49 | Expected math mode to be `%s` here. | none | no — math mode is not modeled |

## Summary

- 49 chktex warnings total.
- 4 warnings now map to a satex rule added in this change: 9, 10 and 15
  (restricted to `\begin`/`\end` pairs, one rule: `environment-mismatch`) and
  41 (`primitive-tex-command`).
- 1 warning (27) was already covered by the existing `unresolved-file` rule.
- 1 warning (17) has a related-but-not-equivalent existing diagnostic
  (`unbalanced-group`, surfaced through `analysis-imprecision`): it fires when
  the interpreter hits a group that never closes, not on any character-pair
  count mismatch in general.
- The remaining 43 warnings are out of reach: 31 need literal source
  characters or spacing that satex never turns into a fact (1, 2, 3-8, 11-13,
  18-19, 21, 23-26, 28-31, 32-39, 42), 6 need a math-mode state satex's
  interpreter does not track (16, 40, 43, 45, 46, 49), 2 are chktex's own
  user-configurable pattern warnings with no satex equivalent (20, 44), 2 are
  ConTeXt-only (47, 48), 1 is a limitation of chktex's line-based parser that
  does not apply to a real interpreter (14), and 1 has no fact to hang off at
  all because comments never reach the interpreter (22).

Net result: satex's linter goes from 0 to 2 new rules covering 4 of chktex's
49 warnings by number, with a 5th (27) already covered before this change.
