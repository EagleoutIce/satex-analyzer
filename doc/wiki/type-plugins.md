<!-- generated from doc/src/wiki/type-plugins.md.in by satex-doc; do not edit -->

# Type plugins

A type plugin adds editor features where text of a type stands. The type comes from the run, as the [type at the cursor](lsp.md#completion) finds it: what TeX read the text as, the kind of name a `\csname` formed from it, else the role of the argument. No plugin looks at a command's name or a text pattern. A new command that takes a color gets the color plugin without any setup.

Each plugin has a block under `type_plugins:` in `satex.yaml`, grouped there by its category: `widget` (dialogs and lenses), `hover`, `completion`, `lint`, `incremental` (whether an edit needs a new run) and `build` (the build file a project is read with, its settings under `l3build:`). `enabled: false` takes all its features away. The list below is what `satex query type-plugins` prints, by category:

```text
$ satex query type-plugins -f samples/paper.tex
category     name             types                                       
widget       color            color name                                  
widget       environment      environment name                            
widget       table            environment the run met                     
widget       image            graphic file                                
widget       tikz             environment the run met                     
widget       forest           environment the run met                     
widget       savebox          environment name                            
widget       glossary         declared key                                
widget       bibliography     citation key                                
widget       index            index entry                                 
widget       theorem          environment name                            
hover        units            dimen, glue, dimen register, skip register  
completion   keys             options, option key, pgf key, declared key  
incremental  comment_edit                                                 
incremental  whitespace_edit                                              
incremental  typing                                                       
build        l3build                                                      
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
| manage | `satex/view` with `plugin`, no `position` | the `manage` command | `satex: Manage…` | the view at a name |

## keys

Key lists of pgfkeys, l3keys, kvoptions and xkeyval, and package options. Completion opens at `{`, `[`, `,` and `=` with nothing typed. A family (`/demo`) inserts `/demo/` and opens again on its keys (`family_continue`). Hover shows the key's handler kind, its initial or default value, where it was defined and the comment next to the definition.

## units

A length written in the source (`12pt`, `-2.5cm`) or a length register (`\textwidth`, its value when the run ended, a range when the run kept one). Hover shows it in each unit of `units`, in `em` and `ex` of the font of the text set just before when the run loaded that font, and in scaled points. One action per unit rewrites the literal. TeX keeps a length in scaled points (sp), so the action says *exact* when TeX reads the new text as the same sp, else how many sp it is off. It writes the fewest decimals up to `decimals` that are exact, else rounds to `decimals`. The view is a converter: type a value in any unit, click a unit to write it.

## color

A color name or an xcolor expression (`red!40!blue`). Completion lists the colors defined at the position, newest first, each with its swatch. Hover shows the value in each model the document can write: `HTML`, `rgb`, `RGB`, `cmyk`, `gray` and `hsb` with xcolor, `rgb`, `cmyk` and `gray` with the color package. An expression is mixed as xcolor mixes, in rgb. On a `\definecolor` an action converts the spec to another model.

The picker (`view`) has a saturation and brightness square, a hue strip, the spec in each model and the colors defined at the position. It writes what works at the position:

- the first argument of an xcolor command (`\color`, `\textcolor`, `\colorbox`): `\color[HTML]{1F77B4}`
- anywhere else (a TikZ key, a command without a model argument, or without xcolor): a `\definecolor{mycolor}{…}{…}` before `\begin{document}` and its name in place. Without any color package it adds `\usepackage{xcolor}` too.

VS Code's own color picker goes through `textDocument/colorPresentation`, which writes the same.

## environment

On the name of a `\begin` or `\end` the run met. Its view, "Change environment…", lists the environments defined there that make the same kind of material, with their required arguments. The kind is the first kernel builder the environment's code reaches, the most specific first: an alignment preamble (`tabular`, `array`, `longtable`), a list (`itemize`), a trivlist (`center`), a float (`figure`, `table`). An environment that reaches none, as `document`, offers no change. Picking one changes this occurrence only: both names, and the arguments, kept where both environments take them, added empty or dropped otherwise. Renaming an environment the project does not define is refused: it would change only its uses.

## table

A table is an environment whose code reaches `\@array`: `tabular`, `tabular*`, `tabularx`, `array` and the ones a document defines on them. A code lens above its `\begin` and a link in the hover open a grid dialog (`satex/dialog`). The dialog edits cells, adds, removes and moves rows and columns, sets each column's alignment (`l`, `c`, `r`, `p{…}`, siunitx `S`), the vertical rules and the rule before each row, and merges cells into a `\multicolumn`. `satex/dialogApply` writes it back. A row the dialog left as it was keeps its text, comments included. A new or edited row follows the table's style: indentation, spaces around `&`, padded columns, rules on the row's line.

Two rewrites are code actions and buttons in the dialog:

- booktabs: `\toprule` first, `\midrule` between, `\bottomrule` last, `\cmidrule(lr)` for `\cline`, no vertical rules
- siunitx: a column whose cells below the header are all numbers becomes `S[table-format=I.D]`, its header cell braced.

Either adds `\usepackage{booktabs}` or `\usepackage{siunitx}` before `\begin{document}` when the document does not load it, and says so.

## image

A figure is a file `\includegraphics` read. Its code lens and hover link open a dialog with the figure: a PNG or JPEG as it is, a PDF's first page as `pdftocairo` renders it where one is installed (the playground's server, VS Code), else its box. The engine's own size of the file (`natural`, in bp) scales the crop. The dialog crops with drag handles, rotates in 90° steps or by any angle, and sizes by `width`, `height`, `scale` and `keepaspectratio`. It writes graphicx options:

- `trim=l b r t` in bp with `clip`, or `viewport=llx lly urx ury` where the figure had one
- `angle` counterclockwise
- graphicx reads the keys in order: `angle=90,width=3cm` makes the rotated figure 3cm wide, `width=3cm,angle=90` makes it 3cm high. The dialog's *size applies to* chooses the order.

A key the dialog does not edit (`page`, `draft`) stays where it was, and so do a crop it cannot read and the units of an unchanged crop. A client without dialogs gets two code actions: rotate by 90° and fit to `\linewidth`.

## tikz

A `tikzpicture` the run met: a lens "Edit drawing…" on its `\begin` line and the view anywhere inside. The canvas shows the nodes the run made, where pgf put each, with its shape and size (`\pgf@sh@ns@⟨node⟩` and its saved anchors). Plain paths between coordinates show too.

- Tools: add a rectangle or circle node, move one by drag (snapped to `grid` cm), connect two with `->`, `<->` or `--`, delete, edit a label.
- It writes `\node[draw] (n1) at (x,y) {label};` and `\draw[->] (a) -- (b);`. Statements it did not change stay as they were.
- Anything else stays code and shows read-only: a `\foreach`, a scope, a node placed relative to another. A node the run put elsewhere than its text says counts as code too. Nothing is dropped.
- Mermaid: paste a flowchart (`graph TD`, `A[x] -->|y| B((z))`) to add its nodes, ranked by longest path, `spacing` cm apart. Export writes the picture's nodes and arrows as a flowchart.

On a blank line of the document body, "Insert tikzpicture" writes a small picture and the view "New drawing" starts an empty one. Both add `\usepackage{tikz}` when the run has no tikz.

## savebox

On the name of a `\begin` in the document's body. "Store in a box" moves the environment to the preamble as a saved box and writes `\usebox` in its place: `\newsavebox` and `\sbox`, or `lrbox` when its `\begin` made characters of what TeX's initial catcodes make commands (`&` in TikZ, everything in a listing). A body the run read in vertical mode goes into a `minipage` of `\textwidth`. Identical copies get a second action that stores one box for all of them. The hover says which applies. Content that reads where it stands is refused, with the reason: a label, reference or citation the run met in it, or a command whose code reads a counter or a reference (`\thepage`, `\footnote`).

## Managers

The next four are managers. Each opens on a name of its type, focused on that name, or without a position from a sidebar (`satex/view` with `plugin` and no `position`). A manager is one searchable, sortable list. A record opens to its fields, a table and its actions. A field edit writes the source back, so the run sees it at once.

### glossary

Every key a document call declared (`\newglossaryentry`, `\newacronym`, acro's `\DeclareAcronym`), taken from the run, not a list of commands. The fields are the key list's keys, else each argument named by the field the run stored its text in (`\glo@cfg@short`). The table holds the uses and every value the run stored, the plural and first forms too. Unused entries are marked. Delete is refused while an entry is used. "Replace uses and delete" moves the uses to another key first. A new entry is written like the first one of the same command in the document.

### bibliography

The entries of the databases the run read. Search by key, author, title, year or type, sort by each. Fields are edited in the `.bib` file. An entry is deleted only while nothing cites it. A new entry gets the fields the document's style reads of its type: a `.bst` function's fields, or biblatex's data model.

### index

Every term an index line carried (`\index{graph!directed}`), by its top level, sorted as makeindex sorts it, with the subentries and where each stands. Renaming a term rewrites it in every call.

### theorem

Every environment a document call defined together with its number (`\the⟨env⟩`), with any package: the counter it owns, shares or is numbered within, and the commands its code has that the others lack (its style). Each instance shows its labels and the lines referencing them. Renaming rewrites the definition, both ends of each instance and the kinds that share its counter. Numbering writes the group the document's own calls of the same command use for a shared counter or a parent counter.

## forest

A `forest` environment the run met: a lens "Edit tree…" on its `\begin` line and the view anywhere inside. The dialog reads the bracket notation `[content, options [child] …]` and draws the tree. Where the run computed forest's packing for every node (`fRsT⟨id⟩/x` and `/y`), it draws the nodes there. A node's `name` shows on hover.

- Select a node to edit its content and its options. Add a child or a sibling, delete a node with its subtree, move it left or right.
- Drag a node onto another: onto its middle it becomes the last child, onto its left or right third a sibling before or after it.
- `for tree={…}` in the preamble edits as one field. The rest of the preamble and the code after the tree stay as they were.
- Indented outline: paste lines indented by depth (`-`, `*`, `+` or `1.` markers dropped) to add them under the selected node or to replace the tree. Export writes the tree as such an outline.
- It writes only what changed. An untouched node keeps its text, spacing and comments. A new or moved node takes the spacing of the sibling before it. Content with a comma or a bracket goes in braces.

On a blank line of the document body, "Insert forest tree" writes a small tree and the view "New tree" starts one. Both add `\usepackage{forest}` when the run has no forest.

## Incremental plugins

An incremental plugin decides for one edit whether the last run still holds. It compares the text the run read with the new one, finds the one stretch they differ in, and asks the run's facts about it, never the text alone. Each mouth read of a line records where its comment began and which spaces it skipped (`Facts::lines`), merged over every read on every path. A plugin that says yes keeps the run: the rules run again, the diagnostics and their fixes, the outline, the lenses, inlays and colors are moved by the edit's offsets and published for the new version. Anything no plugin takes is analyzed again. `satex/caches` counts each reuse (`reused comment_edit 3`), the status bar shows it, and the server's log names it.

- `comment_edit`: every read of the line had its comment begin before the edit, and the edit adds or removes no line end. TeX drops a comment's characters unread ([tex.web § 347](https://github.com/TeX-Live/texlive-source/blob/73900753804455f80d9e04cbed39eea2b415968e/texk/web2c/tex.web#L7326)), so no read sees them. A `%` of category 12 (verbatim, `\catcode`) is no comment there and is not taken. A `%\satex@` directive and a magic comment (`% !TeX`) change a run, so an edit that makes, changes or removes one is not taken either. The rules read the new text, so a `satex-disable` comment or a comment marker counts at once.
- `whitespace_edit`: copies of a space put into a run of it where every read skipped one in state N or S. The copies are read at the same moment under the same categories, so they are skipped too, and the line gets no new line end, so no `\par`. Or white space where no read got to: the line before it was never read to its end, and the run stopped at no limit. A user pattern (`lints.user-pattern`) may match spaces, so with one set only the second case is taken. A new line in the middle of the text is never taken: it moves every line after it.
- `typing`: letters typed into a control word the run met undefined on every path, while the new name has no meaning or definition in the run. Its results stand, marked provisional, until typing pauses for `pause_ms`, then the run follows. This one is not claimed sound, only likely. The playground asks for its runs itself, so there it runs at once.

Writing a plugin: [Writing a type plugin](writing-type-plugins.md).
