<!-- generated from doc/src/wiki/type-plugins.md.in by satex-doc; do not edit -->

# Type plugins

A type plugin adds editor features where text of a type stands. The type comes from the run, as the [type at the cursor](lsp.md#completion) finds it: what TeX read the text as, the kind of name a `\csname` formed from it, else the role of the argument. No plugin looks at a command's name or a text pattern. A new command that takes a color gets the color plugin without any setup.

Each plugin has a block under `type_plugins:` in `satex.yaml`. `enabled: false` takes all its features away. The list below is what `satex query type-plugins` prints:

```text
$ satex query type-plugins -f samples/paper.tex
name         types                                       
keys         options, option key, pgf key, declared key  
units        dimen, glue, dimen register, skip register  
color        color name                                  
environment  environment name                            
table                                                    
image        graphic file                                
```

## What each editor gets

| Feature | LSP | Playground | VS Code | Other clients |
| --- | --- | --- | --- | --- |
| hover | `textDocument/hover` | hover card | hover | hover |
| complete | `textDocument/completion` | completion | completion | completion |
| actions | `textDocument/codeAction` at the cursor | hover card buttons, context menu | light bulb | code actions |
| view | `satex/view` | a dialog | a webview beside the editor (light bulb or `satex: Open Picker, Converter or Drawing Here`) | the actions and the hover instead |
| lens | `textDocument/codeLens`, one handler over every plugin | a button above the line | a code lens | a code lens |
| colors | `textDocument/documentColor`, `colorPresentation` | swatches, a click opens the picker | the native color picker | swatches where supported |
| dialog (table, image) | `satex/dialog`, `satex/dialogApply` | a button above the line, a link in the hover | a code lens and a webview beside the editor (`satex: Edit Table or Image…`) | the code actions with their defaults |

## keys

Key lists of pgfkeys, l3keys, kvoptions and xkeyval, and package options. Completion opens at `{`, `[`, `,` and `=` with nothing typed. A family (`/demo`) inserts `/demo/` and opens again on its keys (`family_continue`). Hover shows the key's handler kind, its initial or default value, where it was defined and the comment next to the definition.

## units

A length written in the source (`12pt`, `-2.5cm`) or a length register (`\textwidth`, its value when the run ended, a range when the run kept one). Hover shows it in each unit of `units`, in `em` and `ex` of the font of the text set just before when the run loaded that font, and in scaled points. One action per unit rewrites the literal. TeX keeps a length in scaled points (sp), so the action says *exact* when TeX reads the new text as the same sp, else how many sp it is off. It writes the fewest decimals up to `decimals` that are exact, else rounds to `decimals`. The view is a converter: type a value in any unit, click a unit to write it.

## color

A color name or an xcolor expression (`red!40!blue`). Completion lists the colors defined at the position, newest first, each with its swatch. Hover shows the value in each model the document can write: `HTML`, `rgb`, `RGB`, `cmyk`, `gray` and `hsb` with xcolor, `rgb`, `cmyk` and `gray` with the color package. An expression is mixed as xcolor mixes, in rgb. On a `\definecolor` an action converts the spec to another model.

The picker (`view`) has a saturation and brightness square, a hue strip, the spec in each model and the colors defined at the position. It writes what works at the position:

- the first argument of an xcolor command (`\color`, `\textcolor`, `\colorbox`): `\color[HTML]{1F77B4}`;
- anywhere else (a TikZ key, a command without a model argument, or without xcolor): a `\definecolor{mycolor}{…}{…}` before `\begin{document}` and its name in place. Without any color package it adds `\usepackage{xcolor}` too.

VS Code's own color picker goes through `textDocument/colorPresentation`, which writes the same.

## environment

On the name of a `\begin` or `\end` the run met. Its view lists every environment defined there with its required arguments. Picking one changes this occurrence only: both names, and the arguments, kept where both environments take them, added empty or dropped otherwise. Every client also gets one action per environment the document already uses ("Change center to flushright"). Renaming an environment the project does not define is refused: it would change only its uses.

## table

A table is an environment whose code reaches `\@array`: `tabular`, `tabular*`, `tabularx`, `array` and the ones a document defines on them. A code lens above its `\begin` and a link in the hover open a grid dialog (`satex/dialog`). The dialog edits cells, adds, removes and moves rows and columns, sets each column's alignment (`l`, `c`, `r`, `p{…}`, siunitx `S`), the vertical rules and the rule before each row, and merges cells into a `\multicolumn`. `satex/dialogApply` writes it back. A row the dialog left as it was keeps its text, comments included. A new or edited row follows the table's style: indentation, spaces around `&`, padded columns, rules on the row's line.

Two rewrites are code actions and buttons in the dialog:

- booktabs: `\toprule` first, `\midrule` between, `\bottomrule` last, `\cmidrule(lr)` for `\cline`, no vertical rules;
- siunitx: a column whose cells below the header are all numbers becomes `S[table-format=I.D]`, its header cell braced.

Either adds `\usepackage{booktabs}` or `\usepackage{siunitx}` before `\begin{document}` when the document does not load it, and says so.

## image

A figure is a file `\includegraphics` read. Its code lens and hover link open a dialog with the figure: a PNG or JPEG as it is, a PDF's first page as `pdftocairo` renders it where one is installed (the playground's server, VS Code), else its box. The engine's own size of the file (`natural`, in bp) scales the crop. The dialog crops with drag handles, rotates in 90° steps or by any angle, and sizes by `width`, `height`, `scale` and `keepaspectratio`. It writes graphicx options:

- `trim=l b r t` in bp with `clip`, or `viewport=llx lly urx ury` where the figure had one;
- `angle` counterclockwise;
- graphicx reads the keys in order: `angle=90,width=3cm` makes the rotated figure 3cm wide, `width=3cm,angle=90` makes it 3cm high. The dialog's *size applies to* chooses the order.

A key the dialog does not edit (`page`, `draft`) stays where it was, and so do a crop it cannot read and the units of an unchanged crop. A client without dialogs gets two code actions: rotate by 90° and fit to `\linewidth`.

## tikz

A `tikzpicture` the run met: a lens "Edit drawing…" on its `\begin` line and the view anywhere inside. The canvas shows the nodes the run made, where pgf put each, with its shape and size (`\pgf@sh@ns@⟨node⟩` and its saved anchors). Plain paths between coordinates show too.

- Tools: add a rectangle or circle node, move one by drag (snapped to `grid` cm), connect two with `->`, `<->` or `--`, delete, edit a label.
- It writes `\node[draw] (n1) at (x,y) {label};` and `\draw[->] (a) -- (b);`. Statements it did not change stay as they were.
- Anything else stays code and shows read-only: a `\foreach`, a scope, a node placed relative to another. A node the run put elsewhere than its text says counts as code too. Nothing is dropped.
- Mermaid: paste a flowchart (`graph TD`, `A[x] -->|y| B((z))`) to add its nodes, ranked by longest path, `spacing` cm apart. Export writes the picture's nodes and arrows as a flowchart.

On a blank line of the document body, "Insert tikzpicture" writes a small picture and the view "New drawing" starts an empty one. Both add `\usepackage{tikz}` when the run has no tikz.

Writing a plugin: [Writing a type plugin](writing-type-plugins.md).
