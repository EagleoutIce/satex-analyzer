<!-- generated from doc/src/wiki/writing-type-plugins.md.in by satex-doc; do not edit -->

# Writing a type plugin

A [type plugin](type-plugins.md) is one file in `src/lsp/plugins/`. It names the types of the lattice it triggers on and implements the features it has. Every feature has a default that adds nothing.

```rust
pub(super) struct Units;

impl TypePlugin for Units {
    fn name(&self) -> &'static str { "units" }            // its key under `type_plugins:`
    fn about(&self) -> &'static str { "a length in every unit" }
    fn types(&self) -> &'static [Ty] { &[Ty::Dimen, Ty::Glue] }
    fn features(&self) -> &'static [&'static str] { &["hover", "actions", "view"] }
    fn enabled(&self, cfg: &TypePlugins) -> bool { cfg.units.enabled }
    fn hover(&self, cx: &Cx, hit: &Hit) -> Option<String> { … }   // Markdown
    fn actions(&self, cx: &Cx, hit: &Hit) -> Vec<CodeAction> { … }
    fn view(&self, cx: &Cx, hit: &Hit) -> Option<Json> { … }      // a dialog
}
```

`Cx` holds the run, the document, the position and the types found there. `Hit` is one of those types, with the text it is part of. The registry asks each enabled plugin whose `wants(hit)` holds; by default that is `types()` containing the hit's type. `complete` returns `None` to leave a hit to the next plugin. A plugin with no `types()` is triggered by the run's place (an environment the run met, as `tikz`): it is asked once, with a `Ty::Bottom` hit, and finds its place with `pairs(analysis, path, environment)`. `lenses` adds code lenses to the whole document.

Steps:

1. Add the file and its entry in `PLUGINS` (`src/lsp/plugins/mod.rs`).
2. Add `pub <name>: <Name>Plugin` with `enabled: bool` to `TypePlugins` (`src/project/config.rs`).
3. Document each setting under `type_plugins.<name>:` in `satex.yaml`, with `#!` lines. The schema and the [configuration page](configuration.md) follow from it.
4. Test each feature on a small document through the language server (`tests/it/lsp.rs`), and that `enabled: false` removes it.

Every number a plugin uses is a setting. A plugin reads the run's facts, never a list of command names.

## Views

`view` returns a dialog as JSON. One renderer, `doc/site/views.js`, draws it in the playground and in the VS Code webview. Clients without views get the plugin's code actions and hover.

```json
{ "title": "Length 12pt", "sections": [ { "kind": "…", … } ] }
```

| `kind` | Fields | Shows |
| --- | --- | --- |
| `text` | `markdown` | a note |
| `table` | `columns`, `rows` | a table |
| `actions` | `items: [{title, edits}]` | a button per item, which applies its edits |
| `choices` | `placeholder`, `items: [{title, detail, edits}]` | a filterable list; a choice applies its edits |
| `converter` | `lo`, `hi` (sp), `units: [{name, sp}]`, `unit`, `decimals`, `range` | a length in every unit; clicking a unit writes it over `range` |
| `color` | `name`, `rgb`, `models`, `expressions`, `defined: [{name, rgb, css}]`, `pick`, `apply: {title, name, edits}` | the picker; `apply`'s `newText` holds `${model}`, `${spec}`, `${name}` |
| `drawing` | `range`, `body`, `nodes: [{name, shape, x, y, w, h, line}]`, `grid`, `spacing`, `wrap`, `extra`, `note` | a canvas of a picture's body. It writes the body over `range`, wrapped in a `tikzpicture` when `wrap`, plus `extra` |
| `records` | `placeholder`, `sorts`, `focus`, `items: [{title, detail, badge, search, sort, fields, forms, table, actions}]`, `add: {title, kinds: [{name, fields, edit}]}` | a manager: a searchable, sortable list, a record opens to a form of its fields, more forms, a table and buttons |

In `records`, every `edit` is a WorkspaceEdit (`{changes: {uri: [TextEdit]}}`), so a record may write another file. A field is `{name, value, edit}`, and its `newText` holds `${value}`. A form's fields name the template variables, as `${key}`. An action with `disabled` shows the reason and applies nothing. The renderer applies nothing while a value's braces do not balance.

Edits are LSP `TextEdit`s on the document the view was asked for, or a WorkspaceEdit. A new kind needs a renderer: `satexViews.RENDER[kind] = (section, view, ctx) => element`. The models that need no page are tested in node: `node doc/site/views.test.mjs`.
