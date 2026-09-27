<!-- generated from doc/src/chktex-coverage.md.in by satex-doc; do not edit -->

# chktex coverage

[ChkTeX](https://www.nongnu.org/chktex/) reports numbered warnings about typography and style. It reads the source line by line. satex runs the document, so it sees the characters that reach the page, the mode each one was set in, and whether TeX joined them into a ligature.

Every satex rule names the warning it follows, and each row below links to the rule's own explanation on [Lints](Lints). See [Primitive-coverage](Primitive-coverage) for how satex models the primitives a rule reads from, and [Domains](Domains) for the values it reads.

Source: [ChkTeX 1.7.9 manual, § 8](https://www.nongnu.org/chktex/).

"chktex enables it" is chktex's own default, from that manual: whether the warning is reported without `-w`. "satex enables it" is a different question answered from a different place — `satex.yaml`'s `lint_off` — and the two can and do disagree: a rule chktex is silent about by default may fire in satex, and a rule chktex reports by default may be off for some [profile](Configuration#profiles). See [Configuration](Configuration#profiles) for what the profiles are and what each one is for.

| # | chktex reports | chktex enables it | satex rule | satex enables it |
| --- | --- | --- | --- | --- |
| 1 | Command terminated with space. | on | [`command-terminated-space`](Lints#command-terminated-space) | yes |
| 2 | Non-breaking space (`~`) should have been used. | on | [`missing-nonbreaking-space`](Lints#missing-nonbreaking-space) | yes |
| 3 | You should enclose the previous parenthesis with `{}`. | on | [`math-script-braces`](Lints#math-script-braces) | yes |
| 4 | Italic correction (`\/`) found in non-italic buffer. | on | [`zero-italic-correction`](Lints#zero-italic-correction) | yes |
| 5 | Italic correction (`\/`) found more than once. | on | [`stray-italic-correction`](Lints#stray-italic-correction) | yes |
| 6 | No italic correction (`\/`) found. | on | [`missing-italic-correction`](Lints#missing-italic-correction) | yes |
| 7 | Accent command needs use of the dotless letter. | on | [`accent-on-dotted-letter`](Lints#accent-on-dotted-letter) | yes |
| 8 | Wrong length of dash may have been used. | on | [`wrong-dash`](Lints#wrong-dash) | yes |
| 9 | `'%s'` expected, found `'%s'`. | on | [`environment-mismatch`](Lints#environment-mismatch) | yes |
| 10 | Solo `'%s'` found. | on | [`environment-mismatch`](Lints#environment-mismatch) | yes |
| 11 | You should use `\ldots` to achieve an ellipsis. | on | [`ellipsis-dots`](Lints#ellipsis-dots) | yes |
| 12 | Interword spacing (`\ `) should perhaps be used. | on | [`sentence-spacing`](Lints#sentence-spacing) | yes |
| 13 | Intersentence spacing (`\@`) should perhaps be used. | on | [`sentence-spacing`](Lints#sentence-spacing) | yes |
| 14 | Could not find argument for command. | off | — | — |
| 15 | No match found for `'%s'`. | on | [`environment-mismatch`](Lints#environment-mismatch) | yes |
| 16 | Mathmode still on at end of LaTeX file. | on | [`math-still-open`](Lints#math-still-open) | yes |
| 17 | Number of one character doesn't match the number of another. | on | [`unbalanced-brackets`](Lints#unbalanced-brackets) | yes |
| 18 | You should use either ``` `` ``` or `''` as an alternative to `"`. | on | [`wrong-quote-character`](Lints#wrong-quote-character), [`csquotes-available`](Lints#csquotes-available) | `wrong-quote-character`: yes; `csquotes-available`: yes |
| 19 | You should use `'` (ASCII 39) instead of `´` (ASCII 180). | off | [`wrong-quote-character`](Lints#wrong-quote-character) | yes |
| 20 | User-specified pattern found. | on | [`user-pattern`](Lints#user-pattern) | yes |
| 21 | This command might not be intended. | off | [`unintended-escape`](Lints#unintended-escape) | yes |
| 22 | Comment displayed. | off | [`comment-marker`](Lints#comment-marker) | yes |
| 23 | Either ``` `` ``` or `''` will look better. | on | [`quote-ligature`](Lints#quote-ligature) | yes |
| 24 | Delete this space to maintain correct page references. | on | [`space-before-command`](Lints#space-before-command) | yes |
| 25 | You might wish to put this between a pair of `{}`. | on | [`math-script-braces`](Lints#math-script-braces) | yes |
| 26 | You ought to remove spaces in front of punctuation. | on | [`space-before-punctuation`](Lints#space-before-punctuation) | yes |
| 27 | Could not execute LaTeX command. | on | [`unresolved-file`](Lints#unresolved-file) | yes |
| 28 | Don't use `\/` in front of small punctuation. | on | [`stray-italic-correction`](Lints#stray-italic-correction) | yes |
| 29 | `$\times$` may look prettier here. | on | [`dimension-times`](Lints#dimension-times) | yes |
| 30 | Multiple spaces detected in output. | off | [`double-space`](Lints#double-space) | yes |
| 31 | This text may be ignored. | on | [`ignored-verbatim-tail`](Lints#ignored-verbatim-tail) | yes |
| 32 | Use ``` ` ``` to begin quotation, not `'`. | on | [`quote-direction`](Lints#quote-direction) | yes |
| 33 | Use `'` to end quotation, not ``` ` ```. | on | [`quote-direction`](Lints#quote-direction) | yes |
| 34 | Don't mix quotes. | on | [`quote-direction`](Lints#quote-direction) | yes |
| 35 | You should perhaps use a command instead. | on | [`math-operator-name`](Lints#math-operator-name) | yes |
| 36 | You should put a space in front of/after parenthesis. | on | [`parenthesis-spacing`](Lints#parenthesis-spacing) | yes |
| 37 | You should avoid spaces in front of/after parenthesis. | on | [`parenthesis-spacing`](Lints#parenthesis-spacing) | yes |
| 38 | You should not use punctuation in front of/after quotes. | on | [`quote-punctuation`](Lints#quote-punctuation) | yes |
| 39 | Double space found. | on | [`double-space`](Lints#double-space) | yes |
| 40 | You should put punctuation outside inner/inside display math mode. | on | [`math-punctuation`](Lints#math-punctuation) | yes |
| 41 | You ought to not use primitive TeX in LaTeX code. | off | [`primitive-tex-command`](Lints#primitive-tex-command) | yes |
| 42 | You should remove spaces in front of a command. | on | [`space-before-command`](Lints#space-before-command) | yes |
| 43 | A command is normally not followed by this character. | on | [`delimiter-not-followed`](Lints#delimiter-not-followed) | yes |
| 44 | User regex matched. | on | [`user-pattern`](Lints#user-pattern) | yes |
| 45 | Use `\[ … \]` instead of `$$ … $$`. | on | [`math-shift-dollars`](Lints#math-shift-dollars) | yes |
| 46 | Use `\( … \)` instead of `$ … $`. | off | [`math-shift-dollars`](Lints#math-shift-dollars) | yes |
| 47 | `'%s'` expected, found `'%s'` (ConTeXt). | off | — | — |
| 48 | Solo `'%s'` found (ConTeXt). | on | — | — |
| 49 | Expected math mode to be in another state here. | on | [`math-mode-mismatch`](Lints#math-mode-mismatch) | yes |

## What satex does not cover

* chktex's own parser limits, such as warning 14, do not arise in an interpreter that reads across lines.
* ConTeXt warnings 47 and 48 are out of scope. satex interprets LaTeX and plain TeX.
