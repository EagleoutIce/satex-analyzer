// The widgets' shared parts for the playground and the VS Code webview: one popup frame for every
// dialog (full screen, detach into a movable panel, peek at the page under it, kept per name),
// the table and image dialogs of `satex/dialog`, and a small store for what survives a reload.
// `open(view, host)`: `view` is the server's `{kind, range, state, context}`; `host` writes and shows:
//   host.apply(state, op?) -> Promise<{state?, edits, notes, text}>   (the server's `satex/dialogApply` through its
//     snapshot printer, nothing written; `text`: the dialog's range as written then)
//   host.commit(edits) -> void                                  (writes the edits)
//   host.image(path) -> Promise<{url, kind: 'img'|'pdf'|null}>  (the figure's file, a PDF's first page as an image)
//   host.parent: an element to render into (VS Code), else a modal <dialog>.
export {};
declare global {
  /** A key the property editor knows: its value's type, default and choices. */
  interface PropKey { name: string; type: string; default?: string | null; choices?: string[] }
  interface Window { satexDialogs: typeof satexDialogs; satexStore: typeof satexDialogs.store }
}
/** The server's dialog: `kind`, the source range and the state the widget edits. */
interface DlgView { kind: string; range?: unknown; state: Rec; context?: Rec }
/** What a host (playground or webview) gives a dialog. */
interface DlgHost {
  apply(state: unknown, op?: unknown): Promise<Rec | undefined>; commit(edits: unknown): void; image(path: string): Promise<{ url: string; kind: 'img' | 'pdf' | null }>;
  parent?: HTMLElement; feedback?: () => void; closed?: () => void; reopen?: () => void;
  upload?(path: string, name: string, bytes: Uint8Array): Promise<{ beside?: boolean } | undefined>;
  download?(path: string): Promise<{ saved?: boolean; url?: string; name?: string } | null | undefined>;
}
/** The image dialog's state: the `\\includegraphics` options as numbers and texts. */
interface ImgState {
  file: string; natural: number[] | null; trim: number[] | null; angle: number | null; width: string | null; height: string | null; scale: number | null;
  keepaspectratio?: boolean; clip?: boolean; size_first?: boolean; decode: number[] | null; viewport?: boolean; unread?: string[];
}
/** The undo model of a widget. */
interface History {
  settle(): void; hold(on: boolean): void; undo(): boolean; redo(): boolean; reset(): void; readonly changed: boolean; readonly dirty: boolean;
  controls(): HTMLButtonElement[]; keys(root: HTMLElement): void;
}
/** The Apply lifecycle of a widget. */
interface Applier {
  wanted: boolean; error: string; readonly result: Rec | null; readonly waiting: boolean;
  change(op?: unknown): Promise<boolean>; apply(): void; cancel(): void;
}
/** A table cell, a row (rules before it, its cells, the `\\\\` after), a column and the table the dialog edits. */
interface TCell { text: string; span: number; spec: string }
interface TRow { before: string[]; inline: boolean; cells: TCell[]; end: string; comment: string; raw: string }
interface TCol { align: string; arg: string; pre: string; post: string; package?: string | null }
interface Tbl { rows: TRow[]; cols: TCol[]; seps: string[]; after: string[]; style: { aligned: boolean; spaced: boolean }; [more: string]: any }
type Rect = [number, number, number, number];
/** The cell covering a position of the grid: where it starts, how far it reaches. */
interface Slot { r: number; i: number; c: number; span: number; rows: number; clean?: boolean }
const satexDialogs = (() => {
  const CSS = `
.sd, dialog.pop { --c-bg: var(--panel, var(--vscode-editor-background, #fff)); --c-fg: var(--fg, var(--vscode-foreground, #1b1d21));
  --c-border: var(--border, var(--vscode-panel-border, #d6dae1)); --c-accent: var(--accent, var(--vscode-focusBorder, #0b3d91));
  --c-sunk: var(--sunk, var(--vscode-input-background, #f1f3f6)); --c-muted: var(--muted, var(--vscode-descriptionForeground, #5c6270));
  --c-accent-fg: var(--accent-fg, var(--vscode-button-foreground, #fff));
  --c-sel: color-mix(in srgb, var(--c-accent) 14%, transparent); --c-hover: color-mix(in srgb, var(--c-accent) 7%, transparent);
  --ease: cubic-bezier(.2, .7, .2, 1); }
.sd { color: var(--c-fg); background: var(--c-bg); font: 13px/1.45 system-ui, sans-serif; display: flex; flex-direction: column; min-height: 0; }
dialog.sd-dialog { width: min(1180px, 96vw); max-width: none; height: min(88vh, 50rem); padding: 0; border: 1px solid var(--c-border); border-radius: 14px; color: var(--c-fg); background: var(--c-bg); }
dialog.sd-dialog[open] { display: flex; }
dialog.sd-dialog > .sd { flex: 1; min-width: 0; }
/* the popup frame: any dialog with a header gets the same tools and states */
dialog.pop { box-shadow: 0 24px 64px -16px rgb(0 0 0 / .35), 0 2px 8px rgb(0 0 0 / .12); }
dialog.pop[open] { animation: pop-in .18s var(--ease); }
dialog.pop::backdrop { background: rgb(10 12 18 / .38); backdrop-filter: blur(2px); transition: opacity .15s; }
@keyframes pop-in { from { opacity: 0; transform: translateY(6px) scale(.985); } }
dialog.pop.pop-full { width: 100vw !important; height: 100vh !important; max-height: none !important; max-width: none; margin: 0; inset: 0; border-radius: 0; }
dialog.pop.pop-free { position: fixed; z-index: 1000; margin: 0; resize: both; overflow: hidden; min-width: 18rem; min-height: 10rem; max-height: none; }
dialog.pop.pop-free > header, dialog.pop.pop-free > .sd > header { cursor: move; user-select: none; }
dialog.pop.pop-peek { opacity: .06; pointer-events: none; transition: opacity .12s; }
dialog.pop.pop-peek::backdrop { opacity: 0; }
.pop-tools { display: inline-flex; gap: .15rem; margin-left: auto; }
.pop-tools + button { margin-left: .15rem; }
.pop-tools svg, .sd-icon svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
.sd > header, .sd > footer { max-width: none; width: auto; margin: 0; box-sizing: border-box; display: flex; align-items: center; gap: .5rem; padding: .55rem .6rem .55rem 1rem; flex-wrap: wrap; }
.sd > header { border-bottom: 1px solid var(--c-border); }
.sd-tools { display: inline-flex; gap: .1rem; margin-left: auto; } .sd-tools ~ .pop-tools { margin-left: .2rem; }
.sd > header h3 { margin: 0; font-size: 1rem; font-weight: 650; letter-spacing: -.01em; }
.sd > header .sd-sub { color: var(--c-muted); font-size: .8rem; }
.sd > footer { border-top: 1px solid var(--c-border); justify-content: flex-end; padding: .55rem 1rem; }
.sd-body { flex: 1; min-height: 0; overflow: auto; padding: .9rem 1rem; display: grid; gap: 1.1rem; grid-template-columns: minmax(0, 1fr) minmax(17rem, 23rem); }
.sd-body > * { min-width: 0; }
@media (max-width: 760px) { .sd-body { grid-template-columns: 1fr; padding: .7rem; } .sd > header { padding-left: .8rem; } }
@media (max-width: 600px) { dialog.sd-dialog { width: 100vw; height: 100dvh; max-height: none; margin: 0; border-radius: 0; border: 0; } .pop-tools .pop-btn:nth-child(2) { display: none; } }
.sd button, .sd select, .sd input, .sd textarea { font: inherit; color: inherit; max-width: 100%; }
.sd select:focus-visible, .sd input:focus-visible, .sd textarea:focus-visible, .sd [tabindex]:focus-visible { outline: 2px solid var(--c-accent); outline-offset: 1px; }
.sd input, .sd select, .sd textarea { background: var(--c-bg); border: 1px solid var(--c-border); border-radius: 6px; padding: .2rem .35rem; min-width: 0; }
.sd-bar { display: flex; flex-wrap: wrap; gap: .35rem; align-items: center; margin-bottom: .7rem; }
.sd-bar .sd-sep { width: 1px; align-self: stretch; background: var(--c-border); margin: 0 .25rem; }
.sd-muted { color: var(--c-muted); font-size: .85em; }
.sd-note { color: var(--warning, var(--vscode-editorWarning-foreground, #8a5a00)); font-size: .85em; }
.sd-note:empty { display: none; }
.sd-tex.sd-stale { opacity: .55; }
.sd-confirm { padding: .45rem 1rem; border-top: 1px solid var(--c-border); background: var(--c-sunk); }
.sd-err { color: var(--error, #c33); font-size: .8em; margin: .1rem 0 0; }
.sd-err:not(.on) { display: none; } .sd-err { display: flex; gap: .4rem; align-items: center; } .sd-err .sd-fix { min-height: 1.3rem; padding: 0 .4rem; }
.sd-tex { margin: 0; padding: .6rem .7rem; background: var(--c-sunk); border-radius: 8px; font: 12px/1.45 ui-monospace, monospace; white-space: pre; overflow: auto; max-height: 16rem; }
.sd-side { display: flex; flex-direction: column; gap: .2rem; }
.sd-side h4 { margin: .7rem 0 .3rem; font-size: .72rem; text-transform: uppercase; letter-spacing: .06em; color: var(--c-muted); font-weight: 650; }
.sd-side h4:first-child { margin-top: 0; }
.sd-chip { display: inline-block; font: 11px ui-monospace, monospace; background: var(--c-sunk); border-radius: 4px; padding: 0 .3rem; margin-right: .2rem; white-space: nowrap; }
.sd-row { display: flex; gap: .35rem; align-items: center; flex-wrap: wrap; margin: .25rem 0; }
.sd-row label { display: inline-flex; gap: .25rem; align-items: center; }
.sd-num { width: 4.2rem; }
.sd-len { width: 9rem; }
.sd-injects { display: grid; grid-template-columns: 1fr 1fr; gap: .15rem; margin-top: .2rem; } .sd-grid th.sd-colh .sd-injects .sd-tex-in { margin-top: 0; } .sd-injects .sd-tex-in { font-size: .8em; }
.sd-inj { display: inline-block; margin-right: .4rem; padding: 0 .25rem; border-radius: 3px; font-size: .75em; line-height: 1.4; vertical-align: .1em; color: var(--c-muted); background: var(--c-sunk); box-shadow: inset 0 0 0 1px var(--c-border); }
.sd-inj.sd-inj-post { margin: 0 0 0 .4rem; }
/* a TeX length field: the editor's token colours painted by a mirror over the input's transparent text */
.sd-tex-in { position: relative; display: inline-block; max-width: 100%; overflow: hidden; vertical-align: middle; border-radius: 6px; }
.sd-grid th.sd-colh .sd-tex-in { display: block; margin-top: .2rem; } .sd-grid th.sd-colh .sd-tex-in input { margin-top: 0; }
.sd-form > .sd-tex-in, .sd-form > .sd-tex-in > input { width: 100%; box-sizing: border-box; }
.sd .sd-tex-in > input { color: transparent; caret-color: var(--c-fg); font-family: ui-monospace, monospace; }
.sd .sd-tex-in > input::placeholder { color: var(--c-muted); }
.sd-tex-hl { position: absolute; inset: 0; padding: calc(.2rem + 1px) calc(.35rem + 1px); font: inherit; font-family: ui-monospace, monospace; white-space: pre; pointer-events: none; color: var(--c-fg); line-height: normal; display: flex; align-items: center; }
.sd-tex-hl .tok-cmd { pointer-events: auto; cursor: text; color: var(--c4, var(--vscode-symbolIcon-functionForeground, var(--c-accent))); }
.sd-tex-hl .tok-num { color: var(--c6, var(--vscode-symbolIcon-numberForeground, var(--c-fg))); }
.sd-inline { height: 100%; }
.sd kbd { font: 11px ui-monospace, monospace; border: 1px solid var(--c-border); border-bottom-width: 2px; border-radius: 4px; padding: 0 .25rem; }
/* table grid: a spreadsheet of TeX cells */
.sd-wrap { position: relative; overflow: auto; padding: .4rem .2rem 1rem; }
.sd-grid { border-collapse: separate; border-spacing: 0; outline: none; }
.sd-grid td, .sd-grid th { padding: 0; background: none; border: 0; }
.sd-grid th.sd-colh { padding: 0 .15rem .3rem; vertical-align: bottom; font-weight: normal; position: relative; }
.sd-grid th.sd-colh select { width: 100%; min-width: 5.5rem; }
.sd-grid th.sd-colh input { width: 100%; margin-top: .2rem; box-sizing: border-box; }
.sd-grid td.sd-cell { vertical-align: middle; position: relative; border: 1px solid var(--c-border); background: var(--c-bg); min-width: 5rem; height: 1.8rem; box-sizing: border-box; padding: 0 .5rem !important; white-space: nowrap; cursor: cell; transition: background .1s; outline: none; font: 13px/1.3 ui-monospace, monospace; }
.sd-grid td.sd-cell + td.sd-cell { border-left-width: 0; }
.sd-grid tr.sd-data + tr.sd-rule + tr.sd-data .sd-cell { border-top-width: 0; }
.sd-grid td.sd-cell[data-align=c], .sd-grid td.sd-cell[data-align=S] { text-align: center; }
.sd-grid td.sd-cell[data-align=r] { text-align: right; }
.sd-grid td.sd-cell:hover { background: var(--c-hover); }
.sd-grid td.sd-cell.sel { background: var(--c-sel); }
.sd-grid td.sd-cell.cur { box-shadow: inset 0 0 0 2px var(--c-accent); z-index: 1; }
.sd-cell.merged::after { content: ''; position: absolute; right: 3px; top: 3px; width: 5px; height: 5px; border-radius: 50%; background: var(--c-accent); opacity: .55; }
.sd-cell input { position: absolute; inset: 0; width: 100%; height: 100%; box-sizing: border-box; border: 0; border-radius: 0; box-shadow: inset 0 0 0 2px var(--c-accent); background: var(--c-bg); font: inherit; padding: 0 .5rem; text-align: inherit; z-index: 2; }
.sd-cell .sd-ph { color: var(--c-muted); opacity: .5; }
/* the row gutter: the rule above the row, its number; the − on hover */
.sd-grid .sd-rh { border: 0; padding: 0 .35rem 0 0 !important; color: var(--c-muted); font-size: .78em; white-space: nowrap; position: relative; }
.sd-gut { display: grid; grid-template-columns: 4.1rem 1.6em 1rem; gap: .3rem; align-items: center; line-height: 1; }
.sd-grid .sd-rh .sd-n { text-align: right; font-variant-numeric: tabular-nums; }
.sd .sd-rsel { width: 100%; font-size: .92em; height: 1.3rem; padding: 0 .15rem; border-radius: 5px; border-color: transparent; background: var(--c-sunk); color: var(--c-fg); cursor: pointer; }
.sd .sd-rsel.none { background: none; color: var(--c-muted); opacity: .45; }
.sd-grid tr:hover .sd-rsel.none, .sd .sd-rsel:focus { opacity: 1; border-color: var(--c-border); }
.sd-grid tr.sd-rule td { height: 3px; border: 0; }
.sd-grid tr.sd-rule td.sd-ruled { background: linear-gradient(var(--c-fg), var(--c-fg)) 0 50% / 100% var(--w, 1px) no-repeat; }
.sd-grid tr.sd-end td { height: 1.5rem; vertical-align: middle; }
.sd-grid .sd-chips { white-space: nowrap; padding-left: .4rem !important; }
.sd-grid td.sd-vcol, .sd-grid th.sd-vcol { width: .55rem; min-width: .55rem; cursor: pointer; border: 0; position: relative; }
.sd-grid th.sd-vcol.on, .sd-grid td.sd-vcol.on { background: linear-gradient(var(--c-fg), var(--c-fg)) 50% 0 / 1px 100% no-repeat; }
.sd-grid th.sd-vcol:hover { background: linear-gradient(var(--c-accent), var(--c-accent)) 50% 0 / 2px 100% no-repeat; }
/* inserting in place: a slim + on the line between rows (in the gutter) and columns (in the header) */
.sd .sd-ins { position: absolute; z-index: 3; display: grid; place-items: center; width: .95rem; height: .95rem; padding: 0; border-radius: 50%; font-size: .8rem; font-weight: 700; line-height: 1; border: 1px solid var(--c-accent); background: var(--c-bg); color: var(--c-accent); opacity: 0; transition: opacity .12s, transform .12s; }
.sd-grid:hover .sd-ins { opacity: .28; }
.sd .sd-ins:hover, .sd .sd-ins:focus-visible { opacity: 1; transform: scale(1.12); background: var(--c-accent); color: var(--c-accent-fg); }
.sd-rule .sd-ins { right: .37rem; top: 50%; margin-top: -.475rem; }
.sd-vcol .sd-ins { left: 50%; top: 50%; margin: -.475rem 0 0 -.475rem; }
.sd-grid tr.sd-rule:has(.sd-ins:hover) td:not(.sd-rh) { background: linear-gradient(var(--c-accent), var(--c-accent)) 0 50% / 100% 2px no-repeat; }
/* the − on a hovered row or column */
.sd-del { position: absolute; display: grid; place-items: center; width: 1rem; height: 1rem; padding: 0 !important; border-radius: 50% !important; font-size: .85rem; line-height: 1; opacity: 0; transform: scale(.8); transition: opacity .12s, transform .12s; color: var(--error, #b42318); }
tr:hover > .sd-rh .sd-del, .sd-colh:hover .sd-del, .sd-del:focus-visible { opacity: 1; transform: none; }
.sd-rh .sd-del { position: static; }
.sd-colh .sd-del { right: -.35rem; top: -.55rem; }
/* the selection's floating bar */
.sd-selbar { position: absolute; z-index: 5; display: flex; gap: .15rem; align-items: center; padding: .2rem; background: var(--c-bg); border: 1px solid var(--c-border); border-radius: 10px; box-shadow: 0 8px 24px -8px rgb(0 0 0 / .3); transition: opacity .12s, transform .12s var(--ease); white-space: nowrap; }
.sd-selbar[hidden] { display: flex; opacity: 0; transform: translateY(4px); pointer-events: none; }
.sd-selbar svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
.sd-selbar .sd-sep { width: 1px; align-self: stretch; background: var(--c-border); margin: .1rem .15rem; }
/* the typeset preview: as LaTeX sets the table */
.sd-prev { display: grid; place-items: center; padding: 1rem .6rem; background: var(--c-sunk); border-radius: 8px; overflow: auto; }
.sd-prev table { border-collapse: collapse; font: 15px/1.35 "Latin Modern Roman", "CMU Serif", "Computer Modern Serif", Georgia, serif; color: var(--c-fg); }
.sd-prev td, .sd-prev tr { padding: .12rem .5rem; vertical-align: top; border: 0; background: none; }
.sd-prev td.mr { vertical-align: middle; }
.sd-prev .math { font-style: italic; }
.sd-import { display: grid; gap: .35rem; }
.sd-data { flex-wrap: nowrap; } .sd-data-label { width: 3.6rem; color: var(--c-muted); font-size: .85em; }
.sd-wrap.sd-over { outline: 2px dashed var(--c-accent); outline-offset: -2px; border-radius: 8px; }
.sd-import textarea { min-height: 5rem; font: 12px/1.4 ui-monospace, monospace; resize: vertical; }
@media (prefers-reduced-motion: reduce) { dialog.pop[open], .sd *, .sd-selbar { animation: none !important; transition: none !important; } }
/* image */
.sd-stage { position: relative; display: inline-block; touch-action: none; user-select: none; background: repeating-conic-gradient(#8882 0 25%, transparent 0 50%) 0 0 / 16px 16px; max-width: 100%; overflow: hidden; border-radius: 6px; }
.sd-stage img, .sd-stage .sd-ph { display: block; max-width: 100%; max-height: 52vh; }
.sd-ph { display: grid; place-items: center; text-align: center; color: var(--c-muted); border: 1px dashed var(--c-border); padding: 1rem; box-sizing: border-box; }
.sd-crop { position: absolute; box-shadow: 0 0 0 9999px rgb(0 0 0 / .45); outline: 1px solid #fff; cursor: move; }
.sd-h { position: absolute; width: 14px; height: 14px; background: #fff; border: 1px solid #000; border-radius: 3px; transform: translate(-50%, -50%); }
.sd-prev-box { display: grid; place-items: center; min-height: 9rem; background: var(--c-sunk); border-radius: 8px; overflow: hidden; padding: .5rem; }
.sd-prev-crop { overflow: hidden; position: relative; outline: 1px solid var(--c-border); }
.sd-prev-crop > * { position: absolute; display: block; max-width: none; max-height: none; }
.sd-defs { position: absolute; width: 0; height: 0; }
.sd-unit { text-transform: none; letter-spacing: 0; font-weight: 400; margin-left: .3rem; opacity: .85; }
.sd-quad, .sd-form { display: grid; gap: .35rem .55rem; align-items: center; margin: .25rem 0 .35rem; }
.sd-quad { grid-template-columns: max-content minmax(0, 1fr) max-content minmax(0, 1fr); }
.sd-form { grid-template-columns: max-content minmax(0, 1fr); }
.sd-quad > label, .sd-form > label { color: var(--c-muted); font-size: .85em; text-align: right; }
.sd-quad .sd-num, .sd-form > input, .sd-form > select { width: 100%; box-sizing: border-box; }
.sd-inl { display: inline-flex; gap: .4rem; align-items: center; }
.sd-file { max-width: 14rem; overflow: hidden; text-overflow: ellipsis; font-size: 12px; padding: .15rem .4rem; }
.sd-out { min-width: 2.6rem; text-align: right; font-variant-numeric: tabular-nums; color: var(--c-muted); font-size: .85em; }
.sd-why { margin: .3rem 0 0; }
.sd-why:empty { display: none; }
.sd input[type=color] { width: 2.4rem; height: 1.7rem; padding: .1rem; cursor: pointer; }
.sd input[type=color]:disabled { opacity: .45; cursor: default; }
.sd input[type=checkbox][role=switch] { appearance: none; flex: none; width: 1.9rem; height: 1.05rem; margin: 0; padding: 0; border-radius: 999px; background: var(--c-sunk); border: 1px solid var(--c-border); position: relative; cursor: pointer; transition: background-color .15s, border-color .15s; }
.sd input[type=checkbox][role=switch]::after { content: ''; position: absolute; top: 50%; left: 2px; width: .7rem; height: .7rem; translate: 0 -50%; border-radius: 50%; background: var(--c-muted); transition: left .15s var(--ease), background-color .15s; }
.sd input[type=checkbox][role=switch]:checked { background: color-mix(in srgb, var(--c-accent) 25%, var(--c-sunk)); border-color: var(--c-accent); }
.sd input[type=checkbox][role=switch]:checked::after { left: calc(100% - .7rem - 2px); background: var(--c-accent); }
/* sliders: a thin track filled up to the value (--p), a round thumb */
.sd input.sd-range { -webkit-appearance: none; appearance: none; width: 100%; height: 1.3rem; padding: 0; border: 0; background: none; cursor: pointer; --track: linear-gradient(var(--c-accent), var(--c-accent)) 0 / var(--p, 50%) 100% no-repeat, color-mix(in srgb, var(--c-fg) 16%, transparent); }
.sd input.sd-range::-webkit-slider-runnable-track { height: 4px; border-radius: 2px; background: var(--track); }
.sd input.sd-range::-moz-range-track { height: 4px; border-radius: 2px; background: var(--track); }
.sd input.sd-range::-webkit-slider-thumb { -webkit-appearance: none; width: 14px; height: 14px; margin-top: -5px; border-radius: 50%; background: var(--c-bg); border: 2px solid var(--c-accent); box-shadow: 0 1px 3px rgb(0 0 0 / .25); transition: transform .12s; }
.sd input.sd-range::-moz-range-thumb { width: 10px; height: 10px; border-radius: 50%; background: var(--c-bg); border: 2px solid var(--c-accent); box-shadow: 0 1px 3px rgb(0 0 0 / .25); }
.sd input.sd-range:hover::-webkit-slider-thumb, .sd input.sd-range:active::-webkit-slider-thumb { transform: scale(1.15); }
.sd input.sd-range:disabled { opacity: .45; cursor: default; }
.sd input.sd-range:focus-visible { outline-offset: 3px; }
/* placement letters: one toggle each */
.sd-place .sd-chips { display: grid; gap: .45rem; }
.sd .sd-letter { display: grid; grid-template-columns: 2rem 1fr; align-items: center; gap: .6rem; text-align: left; padding: .45rem .7rem; border-radius: 10px; }
.sd .sd-letter b { display: grid; place-items: center; width: 1.8rem; height: 1.8rem; border-radius: 7px; background: var(--c-bg); border: 1px solid var(--c-border); font: 600 15px ui-monospace, monospace; }
.sd .sd-letter code { font: .85em ui-monospace, monospace; background: var(--c-sunk); border-radius: 4px; padding: 0 .25rem; }
.sd .sd-letter[aria-pressed=true] { background: var(--c-sel); border-color: var(--c-accent); color: var(--c-fg); }
.sd .sd-letter[aria-pressed=true] b { background: var(--c-accent); color: var(--c-accent-fg); border-color: var(--c-accent); }
/* the property editor: rows key | value | move and drop, then "Add property" */
.sd-props { --p-b: var(--c-border, var(--tv-border)); --p-m: var(--c-muted, var(--tv-muted)); display: grid; gap: .25rem; min-width: 0; }
.sd-prop { display: grid; grid-template-columns: minmax(5rem, 9rem) minmax(0, 1fr) auto; gap: .3rem; align-items: center; font-size: .8rem; }
.sd-prop > code { font: .78rem ui-monospace, monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sd-prop-val { display: flex; gap: .25rem; align-items: center; min-width: 0; } .sd-prop-val > input:not([type=color]), .sd-prop-val > select { flex: 1; min-width: 0; font: .8rem ui-monospace, monospace; }
.sd-prop-val > .sd-prop-unit { flex: 0 0 4rem; } .sd-prop-val > input[type=color] { width: 1.7rem; height: 1.5rem; padding: 0; border: 1px solid var(--p-b); border-radius: 5px; flex: none; }
.sd-prop-on { color: var(--p-m); font-size: .75rem; }
.sd-prop-btns { display: inline-flex; } .sd-prop-btns button { border: 0; background: none; color: var(--p-m); cursor: pointer; padding: 0 .2rem; font: inherit; line-height: 1.4; border-radius: 4px; }
.sd-prop-btns button:hover:not(:disabled) { color: var(--c-fg, var(--tv-fg)); background: var(--c-sunk, var(--tv-sunk)); } .sd-prop-btns button:disabled { opacity: .3; cursor: default; }
.sd-prop.sd-prop-pending code { font-style: italic; } .sd-prop .sd-bad, .sd-prop-add .sd-bad { border-color: var(--danger, #c33) !important; }
.sd-prop-add { display: flex; gap: .3rem; align-items: center; margin-top: .15rem; } .sd-prop-add input { flex: 1; min-width: 0; font: .8rem ui-monospace, monospace; }
.sd-prop-hint { color: var(--p-m); font-size: .72rem; }
/* the file picker: a button and a drop zone naming the file */
.sd-drop { display: flex; align-items: center; gap: .55rem; padding: .4rem .5rem; margin: .25rem 0; border: 1.5px dashed var(--c-border); border-radius: 8px; transition: border-color .12s, background .12s; min-width: 0; }
.sd-drop.over { border-color: var(--c-accent); background: var(--c-hover); }
.sd-drop-name { color: var(--c-muted); font-size: .82em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
`;
  const esc = (s: unknown) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c as '&']);
  const h = <K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Record<string, any> = {}, ...kids: unknown[]): HTMLElementTagNameMap[K] => {
    const e = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
      else if (k === 'class') e.className = v;
      else if (k === 'html') e.innerHTML = v;
      else if (v === true) e.setAttribute(k, '');
      else if (v !== false && v != null) e.setAttribute(k, v);
    }
    e.append(...(kids.flat().filter((k) => k != null) as (Node | string)[]));
    return e;
  };
  const clone = <T>(x: T): T => JSON.parse(JSON.stringify(x));
  /** A TeX length's text as the editor colours it: control sequences (`.tok-cmd`, each a `.cs-tip`
   * whose hover shows the name's card) and numbers (`.tok-num`). */
  const texTokens = (text: string) => esc(text).replace(/(\\(?:[A-Za-z@]+|.))|(-?(?:\d+\.?\d*|\.\d+))/g, (m, cs, n) =>
    cs ? `<span class="tok-cmd cs-tip" data-cs="${cs}">${cs}</span>` : `<span class="tok-num">${n}</span>`);
  /** A text input for a TeX length, highlighted like the editor: a mirror over it paints the tokens,
   * the input's own text is transparent; setting `value` repaints. */
  const texField = (input: HTMLInputElement) => {
    const hl = h('span', { class: 'sd-tex-hl', 'aria-hidden': 'true' });
    const box = h('span', { class: 'sd-tex-in' }, input, hl);
    const own = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!;
    const paint = () => { hl.innerHTML = texTokens(input.value); hl.style.translate = `${-input.scrollLeft}px 0`; };
    Object.defineProperty(input, 'value', { get: () => own.get!.call(input), set: (v) => { own.set!.call(input, v); paint(); } });
    input.addEventListener('input', paint);
    input.addEventListener('scroll', paint);
    // a press on a name in the mirror edits the field at its end
    hl.addEventListener('mousedown', (e) => { e.preventDefault(); input.focus(); });
    paint();
    return box;
  };
  let styled = false;
  const style = () => { if (!styled && self.document) { document.head.append(h('style', {}, CSS)); styled = true; } };

  // ------------------------------------------------------------ store

  /** What survives a reload: JSON under `satex:<key>`, dropped when written by another layout version. */
  const VERSION = 1;
  const store = {
    get<T = any>(key: string, fallback: T | null = null): T | null {
      try {
        const v = JSON.parse(localStorage.getItem(`satex:${key}`) ?? 'null');
        return v && v.v === VERSION ? v.x : fallback;
      } catch { return fallback; }
    },
    set(key: string, value: unknown) {
      try { if (value == null) localStorage.removeItem(`satex:${key}`); else localStorage.setItem(`satex:${key}`, JSON.stringify({ v: VERSION, x: value })); } catch {}
    },
    /** Forgets every kept view state (not the drafts). */
    clear() { try { for (const k of Object.keys(localStorage)) if (k.startsWith('satex:')) localStorage.removeItem(k); } catch {} },
  };

  // ------------------------------------------------------------ popup

  const ICON = {
    peek: '<svg viewBox="0 0 16 16"><path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8 12.1 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/></svg>',
    free: '<svg viewBox="0 0 16 16"><rect x="2" y="4.5" width="9.5" height="9" rx="1.5"/><path d="M8 2h6v6M14 2 8.5 7.5"/></svg>',
    dock: '<svg viewBox="0 0 16 16"><rect x="2" y="2" width="12" height="12" rx="1.5"/><path d="M5 8h6M8 5v6" opacity=".0"/><path d="M2 5.5h12"/></svg>',
    full: '<svg viewBox="0 0 16 16"><path d="M2 6V2h4M14 6V2h-4M2 10v4h4M14 10v4h-4"/></svg>',
    unfull: '<svg viewBox="0 0 16 16"><path d="M6 2v4H2M10 2v4h4M6 14v-4H2M10 14v-4h4"/></svg>',
  };
  /** Gives `dlg` (a <dialog> with a header) the popup's tools: peek (hold, or Alt+P), detach into a
   * movable, resizable panel (Alt+D), full screen (Alt+F); each dialog's mode and place are kept by `name`. */
  function enhance(dlg: HTMLDialogElement, name = dlg.id || 'dialog') {
    if (dlg.dataset.pop) return dlg;
    style();
    dlg.dataset.pop = name;
    dlg.classList.add('pop');
    const header = dlg.querySelector<HTMLElement>(':scope > header, :scope > .sd > header');
    if (!header) return dlg;
    const kept = (): Rec => store.get(`pop:${name}`, {})!;
    const keep = (patch: Rec) => store.set(`pop:${name}`, { ...kept(), ...patch });
    const btn = (icon: keyof typeof ICON, label: string, keys: string) => h('button', { type: 'button', class: 'pop-btn', title: `${label} (${keys})`, 'aria-label': label, 'aria-pressed': 'false', html: ICON[icon] });
    const peek = btn('peek', 'Peek: hold to see the page underneath', 'hold, or Alt+P');
    const free = btn('free', 'Detach into a movable panel', 'Alt+D');
    const full = btn('full', 'Full screen', 'Alt+F');
    const tools = h('span', { class: 'pop-tools' }, peek, free, full);
    const close = [...header.querySelectorAll(':scope > button')].pop();
    header.insertBefore(tools, close ?? null);
    const setFull = (on: boolean) => {
      dlg.classList.toggle('pop-full', on);
      full.setAttribute('aria-pressed', String(on));
      full.innerHTML = on ? ICON.unfull : ICON.full;
      keep({ full: on });
    };
    const place = (r: { x: number; y: number; w: number; h: number }) => {
      const W = innerWidth, H = innerHeight, w = Math.min(r.w, W), hh = Math.min(r.h, H);
      Object.assign(dlg.style, { left: `${Math.max(0, Math.min(r.x, W - w))}px`, top: `${Math.max(0, Math.min(r.y, H - hh))}px`, width: `${w}px`, height: `${hh}px` });
    };
    const setFree = (on: boolean, rect?: { x: number; y: number; w: number; h: number }) => {
      const open = dlg.open;
      const r = rect ?? (() => { const b = dlg.getBoundingClientRect(); return { x: b.left, y: b.top, w: b.width, h: b.height }; })();
      if (open) dlg.close();
      dlg.classList.toggle('pop-free', on);
      if (on) { setFull(false); place(r); } else for (const k of ['left', 'top', 'width', 'height'] as const) dlg.style[k] = '';
      free.setAttribute('aria-pressed', String(on));
      free.innerHTML = on ? ICON.dock : ICON.free;
      free.title = on ? 'Dock: back to a dialog (Alt+D)' : 'Detach into a movable panel (Alt+D)';
      keep({ free: on, rect: on ? r : kept().rect });
      if (open) on ? dlg.show() : dlg.showModal();
    };
    full.onclick = () => { if (dlg.classList.contains('pop-free')) setFree(false); setFull(!dlg.classList.contains('pop-full')); };
    free.onclick = () => setFree(!dlg.classList.contains('pop-free'));
    const peeking = (on: boolean) => { dlg.classList.toggle('pop-peek', on); peek.setAttribute('aria-pressed', String(on)); };
    peek.addEventListener('pointerdown', (e) => { e.preventDefault(); peek.setPointerCapture(e.pointerId); peeking(true); });
    for (const ev of ['pointerup', 'pointercancel']) peek.addEventListener(ev, () => peeking(false));
    peek.addEventListener('click', (e) => { if (e.detail === 0) peeking(!dlg.classList.contains('pop-peek')); });
    // a peek ends with any key or click on the page
    addEventListener('pointerdown', () => { if (dlg.classList.contains('pop-peek') && !peek.matches(':active')) peeking(false); }, true);
    dlg.addEventListener('keydown', (e) => {
      if (!e.altKey || e.ctrlKey || e.metaKey) return;
      const k = e.key.toLowerCase();
      if (k === 'p') { e.preventDefault(); peeking(!dlg.classList.contains('pop-peek')); }
      else if (k === 'd') { e.preventDefault(); setFree(!dlg.classList.contains('pop-free')); }
      else if (k === 'f') { e.preventDefault(); full.click(); }
    });
    addEventListener('keydown', (e) => { if (dlg.classList.contains('pop-peek') && !(e.altKey && e.key.toLowerCase() === 'p')) peeking(false); }, true);
    // a detached panel moves by its header and keeps its size
    header.addEventListener('pointerdown', (e) => {
      if (!dlg.classList.contains('pop-free') || e.button || (e.target as Element).closest('button, input, select, label, a')) return;
      e.preventDefault();
      const b = dlg.getBoundingClientRect(), x0 = e.clientX - b.left, y0 = e.clientY - b.top;
      const move = (m: PointerEvent) => place({ x: m.clientX - x0, y: m.clientY - y0, w: b.width, h: b.height });
      const up = () => { removeEventListener('pointermove', move); removeEventListener('pointerup', up); const r = dlg.getBoundingClientRect(); keep({ rect: { x: r.left, y: r.top, w: r.width, h: r.height } }); };
      addEventListener('pointermove', move);
      addEventListener('pointerup', up);
    });
    new ResizeObserver(() => {
      if (!dlg.classList.contains('pop-free') || !dlg.open) return;
      const r = dlg.getBoundingClientRect();
      if (r.width && r.height) keep({ rect: { x: r.left, y: r.top, w: r.width, h: r.height } });
    }).observe(dlg);
    // opened again: as it was left
    const restore = () => {
      const k = kept();
      if (k.full && !dlg.classList.contains('pop-full')) setFull(true);
      if (k.free && !dlg.classList.contains('pop-free') && k.rect) setFree(true, k.rect);
    };
    new MutationObserver(() => { if (dlg.open) restore(); }).observe(dlg, { attributes: true, attributeFilter: ['open'] });
    if (dlg.open) restore();
    return dlg;
  }

  /** The frame every widget shares: header (title, tools, close), body, footer with the primary action.
   * `opts`: {name, title, sub, main, side, foot, actions: [button…], parent, onClose}. */
  const ICONS = {
    undo: '<svg viewBox="0 0 16 16"><path d="M5.5 3 2.5 6l3 3"/><path d="M2.5 6h7a4 4 0 0 1 0 8H7"/></svg>',
    redo: '<svg viewBox="0 0 16 16"><path d="M10.5 3l3 3-3 3"/><path d="M13.5 6h-7a4 4 0 0 0 0 8H9"/></svg>',
    feedback: '<svg viewBox="0 0 16 16"><path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z"/><path d="M5.5 6.5h5M5.5 8.5h3"/></svg>',
  };
  /** The undo model every widget shares: one stack of state snapshots per session. `settle()` records
   * the state when it changed, `hold(true)` defers that over a gesture (typing a cell), `reset()` goes
   * back to how the widget opened (undoable too). `controls()` gives the header's undo and redo buttons,
   * `keys(root)` binds Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y at the root, before a focused child, outside text fields (those keep their own undo). */
  function history(get: () => unknown, set: (state: any) => void): History {
    const json = () => JSON.stringify(get()), first = json(), undo: string[] = [], redo: string[] = [], subs: (() => void)[] = [];
    let last = first, held = false;
    const told = () => subs.forEach((f) => f());
    const go = (from: string[], to: string[]) => { api.settle(); if (!from.length) return false; to.push(last); last = from.pop()!; set(JSON.parse(last)); told(); return true; };
    const api: History = {
      settle() { if (held) return; const now = json(); if (now !== last) { undo.push(last); if (undo.length > 200) undo.shift(); redo.length = 0; last = now; told(); } },
      hold(on: boolean) { held = on; if (!on) api.settle(); },
      undo: () => go(undo, redo),
      redo: () => go(redo, undo),
      reset() { api.settle(); if (last === first) return; undo.push(last); redo.length = 0; last = first; set(JSON.parse(first)); told(); },
      get changed() { return last !== first; },
      /** Whether the state now (settled or not) differs from how the widget opened. */
      get dirty() { return json() !== first; },
      controls() {
        const b = (k: 'undo' | 'redo', title: string, f: () => unknown) => h('button', { type: 'button', class: 'sd-icon', title, 'aria-label': k, html: ICONS[k], onclick: f });
        const u = b('undo', 'Undo (Ctrl+Z)', api.undo), r = b('redo', 'Redo (Ctrl+Shift+Z)', api.redo);
        const sync = () => { u.disabled = !undo.length; r.disabled = !redo.length; };
        subs.push(sync);
        sync();
        return [u, r];
      },
      keys(root: HTMLElement) {
        route(root, (e) => {
          if (!(e.ctrlKey || e.metaKey) || (e.target as Element).closest?.('input:not([type=range]):not([type=checkbox]), textarea, [contenteditable=true]')) return;
          const k = e.key.toLowerCase(), go = k === 'z' && !e.shiftKey ? api.undo : k === 'y' || (k === 'z' && e.shiftKey) ? api.redo : null;
          if (go) { e.preventDefault(); e.stopPropagation(); go(); }
        });
      },
    };
    return api;
  }
  /** What every widget's footer says about its keys. */
  const HINT = 'Ctrl+Z undoes, Ctrl+Enter applies, Esc cancels.';

  /** The footer every dialog and view shares: a note and the keys, then Reset, Cancel and Apply (each only when given).
   * The classes fit both styles (dialogs.js `.sd`, views.js `.tv`). */
  function footer({ note = '', hint = HINT, extra = [], reset, cancel, apply, label = 'Apply', tag = 'footer', cls = '' }: { note?: string; hint?: string; extra?: Node[]; reset?: (() => void) | null; cancel?: (() => unknown) | null; apply?: (() => void) | null; label?: string; tag?: 'footer' | 'div'; cls?: string }) {
    const b = (text: string, kind: string, title: string, f?: (() => unknown) | null) => f ? h('button', { type: 'button', class: `tv-button tv-${kind} sd-${kind}`, title, onclick: () => f() }, text) : null;
    const ok = b(label, 'primary', 'Write into the document (Ctrl+Enter)', apply);
    const el = h(tag, { class: cls }, h('span', { class: 'sd-muted tv-muted sd-foot-note', style: 'flex:1;min-width:10rem' }, note, note && hint ? h('span', { 'aria-hidden': 'true' }, ' · ') : null, hint), ...extra,
      b('Reset', 'ghost', 'Back to how the dialog opened', reset), b('Cancel', 'ghost', 'Close without writing (Esc)', cancel), ok);
    return { el, apply: ok as HTMLButtonElement };
  }
  /** Asks in `root` (a dialog's or view's own element) whether to drop the changes: true to discard. One ask at a time; Esc keeps editing. */
  function discard(root: HTMLElement): Promise<boolean> {
    style();
    if (root.satexAsk) return root.satexAsk;
    const done = (yes: boolean) => { bar.remove(); root.satexAsk = null; resolve(yes); };
    root.satexKeep = () => done(false);
    let resolve!: (yes: boolean) => void;
    const keep = h('button', { type: 'button', class: 'tv-button tv-ghost sd-ghost', onclick: () => done(false) }, 'Keep editing');
    const bar = h('div', { class: 'sd-row sd-confirm sd-discard', role: 'alertdialog', 'aria-label': 'unsaved changes' },
      h('span', { class: 'sd-muted' }, 'Your changes are not written yet.'),
      h('button', { type: 'button', class: 'tv-button tv-danger sd-danger', onclick: () => done(true) }, 'Discard'), keep);
    bar.addEventListener('keydown', (e) => { if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); done(false); } });
    root.insertBefore(bar, root.querySelector(':scope > footer, :scope > .tv-foot'));
    keep.focus();
    return (root.satexAsk = new Promise<boolean>((r) => { resolve = r; }));
  }

  function popup(opts: Rec) {
    style();
    const root = h('div', { class: 'sd' + (opts.parent ? ' sd-inline' : '') });
    let dlg: HTMLDialogElement | null = null;
    /** Closes, after asking when `opts.dirty()` says there is something unwritten; `force` skips the ask (after Apply). */
    const close = async (force = false) => {
      if (force !== true && opts.dirty?.() && !(await discard(root))) return false;
      if (dlg) { dlg.close(); dlg.remove(); }
      opts.onClose?.();
      return true;
    };
    const foot = footer({ note: opts.foot ?? '', extra: opts.extra, reset: opts.reset, cancel: opts.parent ? null : close, apply: opts.apply });
    root.append(
      h('header', {}, h('h3', {}, opts.title), opts.sub ? h('span', { class: 'sd-sub' }, opts.sub) : null,
        opts.tools ? h('span', { class: 'sd-tools' }, opts.tools) : null,
        opts.feedback ? h('button', { type: 'button', class: 'sd-icon sd-feedback', title: 'Send feedback on this widget', 'aria-label': 'feedback on this widget', html: ICONS.feedback, onclick: opts.feedback }) : null,
        opts.parent ? null : h('button', { type: 'button', class: 'sd-icon', 'aria-label': 'close', title: 'Close (Esc)', html: '<svg viewBox="0 0 16 16"><path d="M4 4l8 8M12 4l-8 8"/></svg>', onclick: () => close() })),
      h('div', { class: 'sd-body' }, opts.main, opts.side ? h('div', { class: 'sd-side' }, opts.side) : null),
      foot.el,
    );
    if (opts.parent) opts.parent.replaceChildren(root);
    else {
      // one dialog per widget: opening it again replaces the open one
      for (const old of document.querySelectorAll(`dialog.sd-dialog[data-pop="${opts.name ?? 'widget'}"]`)) { old.close(); old.remove(); }
      dlg = h('dialog', { class: 'sd-dialog', 'aria-label': opts.title }, root);
      document.body.append(dlg);
      enhance(dlg, opts.name ?? 'widget');
      dlg.addEventListener('cancel', (e) => { e.preventDefault(); if (root.satexAsk) root.satexKeep!(); else close(); });
      dlg.showModal();
    }
    return { root, close, dialog: dlg, apply: foot.apply };
  }

  /** The Apply lifecycle every widget shares (no DOM: views.test.mjs). `ask(op)` gets the server's
   * `{edits, notes, state?}` for the state `get()` returns now; `change(op)` asks again after `delay` ms
   * (an `op` at once), so typing asks once. `apply()` commits the answer to the latest state: at once
   * when it is in, else when it lands (a pending ask flushed first), so Apply works before the first
   * answer and never writes an older state. `told()` follows every step: `waiting`, `wanted`, `error`. */
  function applier({ get, set, ask, commit, delay = 120, told = () => {} }: { get: () => unknown; set: (state: any) => void; ask: (op?: unknown) => Promise<Rec | undefined> | Rec | undefined; commit: (edits: unknown) => void; delay?: number; told?: (a: Applier) => void }): Applier {
    const key = () => JSON.stringify(get());
    let result: Rec | null = null, answered: string | null = null, timer: ReturnType<typeof setTimeout> | 0 = 0, queued: unknown = undefined, seq = 0, open = 0;
    const api: Applier = {
      wanted: false, error: '',
      get result() { return result; },
      get waiting() { return !!timer || open > 0 || !result || answered !== key(); },
      change(op?: unknown) {
        clearTimeout(timer || undefined);
        queued = op;
        return new Promise<boolean>((done) => { timer = setTimeout(() => run(op).then(done), op ? 0 : delay); });
      },
      apply() {
        api.wanted = true;
        if (timer) { clearTimeout(timer); run(queued); } else if (!open && api.waiting) run();
        settle();
      },
      cancel() { api.wanted = false; clearTimeout(timer); timer = 0; seq++; told(api); },
    };
    const settle = () => {
      if (api.wanted && !api.waiting) { api.wanted = false; commit(result!.edits); } else if (api.wanted && !timer && !open) run();
      told(api);
    };
    async function run(op?: unknown): Promise<boolean> {
      timer = 0;
      const mine = ++seq, asked = key();
      open++;
      told(api);
      const r = await Promise.resolve().then(() => ask(op)).catch((e: Error) => ({ error: e.message } as Rec));
      open--;
      if (mine !== seq) { told(api); return false; }
      // no answer (the server busy with a run): the last result stays, a wanted Apply says why it waits no more
      if (!r || r.error) { api.error = r?.error ?? 'no answer from the analysis: Apply again'; api.wanted = false; told(api); return false; }
      api.error = '';
      result = r;
      if (r.state) set(r.state);
      answered = r.state ? key() : asked;
      settle();
      return true;
    }
    return api;
  }

  /** The frame of a dialog that writes TeX: Apply writes the server's edits of the latest state; `render.main/side` fill it. */
  function frame(view: DlgView, host: DlgHost, title: string, render: Rec) {
    const tex = h('pre', { class: 'sd-tex', 'aria-label': 'the TeX written' });
    const notes = h('div', { class: 'sd-note', role: 'status' });
    const hist = history(() => view.state, (st: Rec) => { view.state = st; render.redraw(); refresh(); });
    const life = applier({
      get: () => view.state,
      set: (st: Rec) => { view.state = st; render.redraw(); },
      ask: (op?: unknown) => host.apply(view.state, op),
      commit: (edits: unknown) => { host.commit(edits); p.close(true); },
      told: (l: Applier) => {
        if (l.result) tex.textContent = l.result.text ?? '';
        else if (!tex.textContent) tex.textContent = 'waiting for the analysis…';
        tex.classList.toggle('sd-stale', l.waiting);
        notes.textContent = l.error || (l.wanted ? 'waiting for the analysis… Apply writes when it answers' : [...(l.result?.notes ?? []), ...(render.lint?.() ?? [])].join('; '));
        p?.apply.setAttribute('aria-busy', String(l.wanted));
        if (!l.waiting) hist.settle();
      },
    });
    const refresh = (op?: unknown) => life.change(op);
    const p = popup({
      name: view.kind, title, sub: render.sub, main: render.main, side: [render.side, h('h4', {}, 'Written as'), tex, notes], foot: render.foot,
      tools: hist.controls(), feedback: host.feedback, reset: () => hist.reset(), apply: () => life.apply(), parent: host.parent,
      dirty: () => hist.dirty, onClose: () => { life.cancel(); host.closed?.(); },
    });
    keys(p.root, hist, () => life.apply());
    return { refresh, history: hist, root: p.root, close: p.close, apply: () => life.apply() };
  }
  /** The keys every widget routes at its root, before a focused child sees them: Ctrl+Enter applies, Ctrl+Z / Ctrl+Shift+Z undo and redo. */
  function keys(root: HTMLElement, hist: History | undefined, apply: () => void) {
    route(root, (e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); e.stopPropagation(); apply(); } });
    hist?.keys(root);
  }
  /** `on(keydown)` at `root` before its children, and for keys with no focus (a redrawn control drops it to the body)
   * while `root` sits in the topmost open dialog; gone with `root`. */
  function route(root: HTMLElement, on: (e: KeyboardEvent) => void) {
    root.addEventListener('keydown', on as EventListener, true);
    const stray = (e: KeyboardEvent) => {
      if (!root.isConnected) return removeEventListener('keydown', stray, true);
      if (e.target === document.body && [...document.querySelectorAll('dialog[open]')].pop()?.contains(root)) on(e);
    };
    addEventListener('keydown', stray, true);
  }

  // ------------------------------------------------------------ properties: a key=value list (TikZ options, image options)

  /** A key list split at its top-level commas (braces kept). */
  const splitKeys = (s = '') => {
    const out: string[] = [];
    let depth = 0, from = 0;
    for (let i = 0; i <= s.length; i++) {
      if (s[i] === '{') depth++;
      else if (s[i] === '}') depth--;
      else if (i === s.length || (s[i] === ',' && !depth)) { out.push(s.slice(from, i).trim()); from = i + 1; }
    }
    return out.filter(Boolean);
  };
  /** `key=value` as `[key, value]`, the value's outer braces dropped; a bare key has value null. */
  const keyOf = (o: string): [string, string | null] => { const i = o.indexOf('='); return i < 0 ? [o.trim(), null] : [o.slice(0, i).trim(), o.slice(i + 1).trim().replace(/^\{([^]*)\}$/, '$1')]; };
  /** An option's text again: a value with a comma or `=` in braces, no value a bare key. */
  const keyText = (k: string, v: string | null) => v == null ? k : `${k}=${/[,=]/.test(v) && !/^\{[^]*\}$/.test(v) ? `{${v}}` : v}`;
  /**
   * The property editor every widget shares: one row per option (`key | value | ↑ ↓ ×`) and "Add
   * property", completed from `keys` (`{name, type, choices?, default?}`, the run's). The value's
   * widget follows the key's type: a colour (names of `colors`, a picker), a length (number and
   * unit), a choice, a truth value, a node (`nodes`), none for a flag, text otherwise. `get()` gives
   * the options now, `set(list)` takes them after each edit, `commit()` ends a gesture (an undo
   * step). `skip(o)` hides options other fields show: they keep their places. A key added without
   * its required value waits in the editor until it has one.
   */
  function properties({ get, set, commit = () => {}, keys = [], colors = [], nodes = [], skip = () => false, label = 'Add property', css = () => null }: { get: () => string[]; set: (l: string[]) => void; commit?: () => void; keys?: PropKey[]; colors?: string[]; nodes?: string[]; skip?: (o: string) => unknown; label?: string; css?: (name: string) => string | null | undefined }) {
    style();
    const byName = new Map(keys.map((k): [string, PropKey] => [k.name, k]));
    const box = h('div', { class: 'sd-props' });
    let waiting: string[] = [];
    const uid = `sd-p-${Math.random().toString(36).slice(2, 8)}`;
    const keyList = h('datalist', { id: `${uid}-k` }, keys.map((k) => h('option', { value: k.name }, k.type === 'flag' ? '' : k.type)));
    const colorList = h('datalist', { id: `${uid}-c` }, colors.map((c) => h('option', { value: c })));
    const nodeList = h('datalist', { id: `${uid}-n` }, nodes.map((c) => h('option', { value: c })));
    // the options shown, each with its index in all of them
    const rowsNow = () => get().map((o, i) => ({ o, i })).filter(({ o }) => !skip(o));
    const put = (i: number, text: string | null) => { const l = [...get()]; if (text == null) l.splice(i, 1); else l[i] = text; set(l); };
    const swap = (i: number, j: number) => { const l = [...get()]; [l[i], l[j]] = [l[j], l[i]]; set(l); commit(); draw(); };
    const input = (value: string | null | undefined, attrs: Record<string, any>, on: (v: string, e: HTMLInputElement) => void) => {
      const e = h('input', { class: 'tv-input', value: value ?? '', spellcheck: 'false', ...attrs });
      e.addEventListener('input', () => on(e.value, e));
      e.addEventListener('change', () => commit());
      return e;
    };
    const balanced = (t: string) => { let d = 0; for (const m of t.replace(/\\./g, '').matchAll(/[{}]/g)) if ((d += m[0] === '{' ? 1 : -1) < 0) return false; return d === 0; };
    /** The value's widget for key `k`: `change(v)` with the new value (null: none). */
    function widget(k: string, v: string | null, change: (v: string | null) => void): HTMLElement[] {
      const type = byName.get(k)?.type ?? (v == null ? 'flag' : 'text');
      const text = (val: string | null, attrs: Record<string, any> = {}) => input(val, { 'aria-label': `${k} value`, ...attrs }, (t, e) => { e.classList.toggle('sd-bad', !balanced(t)); if (balanced(t)) change(t.trim() === '' ? null : t.trim()); });
      if (type === 'flag' && v == null) return [h('span', { class: 'sd-prop-on' }, 'on')];
      if (type === 'choice' || type === 'bool') {
        const cs = type === 'bool' ? ['true', 'false'] : byName.get(k)!.choices ?? [];
        const all = [...new Set([...(v == null ? [''] : []), ...cs, ...(v != null && !cs.includes(v) ? [v] : [])])];
        const s = h('select', { class: 'tv-input', 'aria-label': `${k} value` }, all.map((c) => h('option', { value: c, selected: c === (v ?? '') }, c || (byName.get(k)?.default != null ? `default (${byName.get(k)!.default})` : 'choose…'))));
        s.addEventListener('change', () => { change(s.value || null); commit(); });
        return [s];
      }
      if (type === 'length') {
        // an expression (`1cm+2pt`) already there stays plain text
        if (v != null && v.trim() && self.satexLengths.check(v) && !/^[-+]?[\d.]+$/.test(v.trim())) return [text(v)];
        const d = byName.get(k)?.default;
        return [self.satexLengths.field({ value: v ?? '', label: `${k} value`, bare: true, placeholder: d != null ? `default: ${d}` : 'a length: 2cm, 1.5in, 0.5\\linewidth', onvalue: (t: string) => change(t || null), oncommit: commit })];
      }
      if (type === 'color') {
        const t = text(v, { list: `${uid}-c`, placeholder: 'a colour: red, blue!30' });
        const pick = h('input', { type: 'color', title: 'Pick any colour: written as {rgb,255:red,…}', 'aria-label': `${k} colour picker` });
        const show = (val: string | null) => { const c = css(val ?? ''); if (/^#[0-9a-f]{6}$/i.test(c ?? '')) pick.value = c!; };
        show(v);
        t.addEventListener('input', () => show(t.value));
        pick.addEventListener('input', () => { const [r, g, b] = [1, 3, 5].map((i) => parseInt(pick.value.slice(i, i + 2), 16)); t.value = `{rgb,255:red,${r};green,${g};blue,${b}}`; change(t.value); });
        pick.addEventListener('change', () => commit());
        return [pick, t];
      }
      if (type === 'node') return [text(v, { list: `${uid}-n` })];
      return [text(v, { placeholder: type === 'flag' ? 'no value' : byName.get(k)?.default != null ? `default: ${byName.get(k)!.default}` : 'value' })];
    }
    const btn = (label: string, title: string, f: () => void, disabled = false) => h('button', { type: 'button', title, 'aria-label': title, disabled, onclick: f }, label);
    function row({ o, i }: { o: string; i: number }, prev?: number, next?: number) {
      const [k, v] = keyOf(o);
      const val = h('div', { class: 'sd-prop-val' }, widget(k, v, (nv: string | null) => { put(i, keyText(k, nv)); }));
      return h('div', { class: 'sd-prop', 'data-key': k }, h('code', { title: o }, k), val,
        h('span', { class: 'sd-prop-btns' },
          btn('↑', `Move ${k} up`, () => swap(i, prev!), prev == null),
          btn('↓', `Move ${k} down`, () => swap(i, next!), next == null),
          btn('×', `Remove ${k}`, () => { put(i, null); commit(); draw(); })));
    }
    // a key added that needs a value first: written with its first value, among the others once the row loses focus
    function pendingRow(k: string) {
      let at: number | null = null;
      const val = h('div', { class: 'sd-prop-val' }, widget(k, '', (nv: string | null) => {
        if (at != null) return put(at, keyText(k, nv));
        if (nv == null) return;
        waiting = waiting.filter((w) => w !== k);
        at = get().length;
        set([...get(), keyText(k, nv)]);
      }));
      const r = h('div', { class: 'sd-prop sd-prop-pending', 'data-key': k }, h('code', { title: `${k}: give it a value` }, k), val,
        h('span', { class: 'sd-prop-btns' }, btn('×', `Drop ${k}`, () => { if (at != null) put(at, null); waiting = waiting.filter((w) => w !== k); draw(); })));
      r.addEventListener('focusout', (e) => { if (at != null && !r.contains(e.relatedTarget as Node)) draw(); });
      return r;
    }
    const add = input('', { list: `${uid}-k`, placeholder: `${label}: a key…`, 'aria-label': label }, () => add.classList.remove('sd-bad'));
    const take = () => {
      const k = add.value.trim().replace(/=$/, '');
      if (!k) return;
      if (k.includes('=')) { const [kk, vv] = keyOf(k); set([...get(), keyText(kk, vv)]); }
      else {
        const key = byName.get(k);
        // a flag, or a key with a default, is set by its name alone
        if (!key || key.type === 'flag' || key.default != null) set([...get(), k]);
        else waiting.push(k);
      }
      add.value = '';
      commit();
      draw();
      box.querySelector('.sd-prop-pending:last-of-type input, .sd-prop-pending:last-of-type select')?.focus();
    };
    add.addEventListener('keydown', (e) => { if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) { e.preventDefault(); take(); } });
    const addRow = h('div', { class: 'sd-prop-add' }, add, h('button', { type: 'button', class: 'tv-button sd-ghost', onclick: take, title: 'Add the key (Enter)' }, 'Add'));
    function draw() {
      const rows = rowsNow();
      box.replaceChildren(keyList, colorList, nodeList,
        ...rows.map((r, j) => row(r, rows[j - 1]?.i, rows[j + 1]?.i)),
        ...waiting.map(pendingRow), addRow,
        ...(keys.length ? [] : [h('span', { class: 'sd-prop-hint' }, 'No keys from the run yet: type any key=value.')]));
    }
    draw();
    return Object.assign(box, { refresh: draw });
  }

  // ------------------------------------------------------------ escaping and validation (no DOM: views.test.mjs)

  /** Plain text as TeX: every special escaped (`50% & co` is `50\% \& co`). */
  const texText = (text: unknown) => String(text).replace(/[\\&%$#_{}~^]/g, (c) => ({ '\\': '\\textbackslash{}', '~': '\\textasciitilde{}', '^': '\\textasciicircum{}' })[c] ?? `\\${c}`);
  /** The pieces of `text` outside `$…$` (even) and inside (odd); escaped characters stay where they are. */
  const mathSplit = (text: unknown) => String(text).split(/((?<!\\)\$(?:\\.|[^$\\])*\$)/s);
  /** The first thing that breaks `text` as TeX in running text (a field the user types TeX in), else ''. */
  function texIssue(text: string) {
    if (!balanced(text)) return 'the braces { } do not match';
    const bare = String(text).replace(/\\./gs, '');
    if (bare.includes('%')) return '% starts a comment: the rest of the line is lost';
    if (bare.includes('#')) return '# is a macro parameter here';
    if (bare.includes('&')) return '& separates table cells';
    if ((bare.match(/\$/g) ?? []).length % 2) return 'a $ opens math that never closes';
    if (/[_^]/.test(mathSplit(text).filter((_, k) => k % 2 === 0).join('').replace(/\\./gs, ''))) return '_ and ^ work in math only';
    return '';
  }
  /** `text` with what `texIssue` finds escaped: % # & _ ^ and a lone $ outside math; math and commands kept. */
  function texSafe(text: string) {
    const parts = mathSplit(text);
    return parts.map((p, k) => k % 2 ? p : p.replace(/(\\.)|([%#&_$])|\^/gs, (m, cs, c) => cs ?? (c ? `\\${c}` : '\\^{}'))).join('');
  }
  /** The checks a widget's field names (a server's field `check: [...]`); each says what is wrong, else ''. `taken`: the keys in use. */
  const CHECKS: Record<string, (v: any, taken?: string[]) => string> = {
    required: (v) => String(v).trim() ? '' : 'needed',
    balanced: (v) => balanced(v) ? '' : 'the braces { } do not match',
    tex: texIssue,
    key: (v) => /^[^\s,{}%#\\]*$/.test(v) ? '' : 'a key has no spaces, commas, braces, %, # or \\',
    bibkey: (v) => /^[A-Za-z0-9_:.+\-/]*$/.test(v) ? '' : 'a BibTeX key takes letters, digits and _ : . + - / only',
    // an index level: makeindex reads ! @ | as syntax unless `"` quotes them
    index: (v) => /(^|[^"])[!@|]/.test(String(v).replace(/\{[^{}]*\}/g, '')) ? 'makeindex reads ! @ | as levels, sort key and format: write "! "@ "|' : '',
    unique: (v, taken = []) => taken.includes(String(v).trim()) ? `${String(v).trim()} exists already` : '',
    length: (v) => self.satexLengths.check(v),
    integer: (v) => /^\s*-?\d+\s*$/.test(v) ? '' : 'a whole number',
    number: (v) => String(v).trim() && Number.isFinite(Number(v)) ? '' : 'a number',
    // a date (ISO, `2020-02-30` is none): the day it names exists
    date: (v) => { const d = String(v).match(/^\s*(\d{4}-\d{2}-\d{2})/)?.[1]; const t = new Date(`${d}T00:00:00Z`); return !d || t.getTime() === t.getTime() && t.toISOString().startsWith(d) ? '' : `${d} is no day`; },
    // bibliography fields, by the data model's datatypes (src/lsp/plugins/bibliography.rs `checks`)
    names: (v) => { const n = String(v).split(/\s+and(?=\s|$)/i); return n.some((x) => !x.trim()) ? 'a name is empty: names are joined by "and"' : n.find((x) => (x.replace(/\{[^{}]*\}/g, '').match(/,/g) ?? []).length > 2) ? 'a name has at most two commas: Last, Jr, First' : ''; },
    bibdate: (v) => { const p = '\\d{4}(-\\d{2}(-\\d{2})?)?[?~%]?'; if (!new RegExp(`^\\s*(${p})?(/(${p}|\\.\\.)?)?\\s*$`).test(v) || !String(v).trim()) return 'a date: 2020, 2020-05, 2020-05-17, or a range 2020/2021'; for (const d of String(v).match(/\d{4}-\d{2}-\d{2}/g) ?? []) { const t = new Date(`${d}T00:00:00Z`); if (!(t.getTime() === t.getTime() && t.toISOString().startsWith(d))) return `${d} is no day`; } return ''; },
    datepart: (v) => /^\s*(\d+|[A-Za-z][\w-]*)\s*$/.test(v) ? '' : 'a number, or a constant such as jan',
    pages: (v) => /^\s*[\w.:]+(\s*(--?|–)\s*[\w.:]*)?(\s*,\s*[\w.:]+(\s*(--?|–)\s*[\w.:]*)?)*\s*$/.test(v) ? '' : 'pages: 12--34, 12, e123, or a list 1--4, 7',
    url: (v) => /^\s*(https?|ftp):\/\/[^\s{}]+\s*$/.test(v) ? '' : 'a URL starts with https:// and has no spaces',
    doi: (v) => /^\s*https?:\/\/(dx\.)?doi\.org\//i.test(v) ? 'the DOI alone: without https://doi.org/' : /^\s*10\.\d{4,9}\/\S+\s*$/.test(v) ? '' : 'a DOI: 10.1234/name',
    isbn: (v) => { const d = String(v).replace(/[-\s]/g, '').toUpperCase(); const sum = d.length === 10 ? [...d].reduce((a, c, i) => a + (c === 'X' ? 10 : +c) * (10 - i), 0) % 11 : d.length === 13 ? [...d].reduce((a, c, i) => a + +c * (i % 2 ? 3 : 1), 0) % 10 : -1; return /^(\d{9}[\dX]|\d{13})$/.test(d) && sum === 0 ? '' : 'an ISBN: 10 or 13 digits whose check digit fits'; },
    issn: (v) => { const d = String(v).replace(/[-\s]/g, '').toUpperCase(); return /^\d{7}[\dX]$/.test(d) && [...d].reduce((a, c, i) => a + (c === 'X' ? 10 : +c) * (8 - i), 0) % 11 === 0 ? '' : 'an ISSN: 1234-567X whose check digit fits'; },
    bibkeyref: (v, taken = []) => { const k = String(v).split(',').map((x) => x.trim()).find((x) => x && !taken.includes(x)); return k ? `no entry ${k}` : ''; },
  };
  /** The first message of `rules` (names, `min:N`, `max:N`) on `value`, else ''. */
  function check(value: unknown, rules: string[] = [], taken: string[] = []) {
    for (const r of rules) {
      const [name, arg] = String(r).split(':');
      const m = name === 'min' ? (Number(value) < Number(arg) ? `at least ${arg}` : '') : name === 'max' ? (Number(value) > Number(arg) ? `at most ${arg}` : '')
        : CHECKS[name]?.(value ?? '', taken) ?? '';
      if (m) return m;
    }
    return '';
  }
  /** Checks `input` as it changes and shows the message under it; for a TeX slip, an Escape button that fixes it.
   * `ok(all)`: whether it passes, showing also what only a submit shows (`needed`). */
  function guard(input: HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement, rules: string[], taken: string[] = []) {
    style();
    const say = h('span');
    const fix = h('button', { type: 'button', class: 'tv-button sd-ghost tv-ghost sd-fix', title: 'Write the specials escaped (\\% \\& \\_ …)', hidden: true,
      onclick: () => { input.value = texSafe(input.value); input.dispatchEvent(new Event('input', { bubbles: true })); input.focus(); } }, 'Escape');
    const msg = h('div', { class: 'sd-err', role: 'alert' }, say, fix);
    const show = (all: boolean) => {
      const m = check(input.value, all ? rules : rules.filter((r) => r !== 'required'), taken);
      say.textContent = m;
      fix.hidden = !m || !rules.includes('tex') || !!texIssue(texSafe(input.value));
      msg.classList.toggle('on', !!m);
      input.classList.toggle('tv-bad', !!m);
      input.setAttribute('aria-invalid', String(!!m));
      return !m;
    };
    input.addEventListener('input', () => show(false));
    input.after(msg);
    show(false);
    return { msg, ok: (all = true) => show(all) };
  }

  // ------------------------------------------------------------ table model (no DOM: views.test.mjs)

  const RULES = ['\\hline', '\\toprule', '\\midrule', '\\bottomrule'];
  /** The text array's `>{a}>{b}` (or `<{…}`) injects: the groups' insides, joined. */
  const injected = (spec = '') => [...spec.matchAll(/[<>]\{((?:[^{}]|\{[^{}]*\})*)\}/g)].map((m) => m[1]).join('');
  /** A column spec's alignment letter, its injects and argument left out (`>{\bf}r` is `r`, not the `b` of `\bf`). */
  const alignOf = (spec = '', letters = 'lcrSpmbX') => spec.replace(/[<>@!]\{(?:[^{}]|\{[^{}]*\})*\}|\{(?:[^{}]|\{[^{}]*\})*\}/g, '').replace(new RegExp(`[^${letters}]`, 'g'), '')[0] ?? 'l';
  /** The CSS of what an inject sets for each cell, as far as a preview shows it: font switches, sizes, colour, math, alignment. */
  const SWITCH: Record<string, string> = {
    bf: 'font-weight:bold', bfseries: 'font-weight:bold', textbf: 'font-weight:bold', boldmath: 'font-weight:bold',
    it: 'font-style:italic', itshape: 'font-style:italic', em: 'font-style:italic', sl: 'font-style:oblique', slshape: 'font-style:oblique', mathit: 'font-style:italic',
    tt: 'font-family:ui-monospace,monospace', ttfamily: 'font-family:ui-monospace,monospace', sf: 'font-family:sans-serif', sffamily: 'font-family:sans-serif',
    sc: 'font-variant:small-caps', scshape: 'font-variant:small-caps', upshape: 'font-style:normal', mdseries: 'font-weight:normal',
    tiny: 'font-size:.5em', scriptsize: 'font-size:.7em', footnotesize: 'font-size:.8em', small: 'font-size:.9em', normalsize: 'font-size:1em',
    large: 'font-size:1.2em', Large: 'font-size:1.44em', LARGE: 'font-size:1.73em', huge: 'font-size:2.07em', Huge: 'font-size:2.49em',
    centering: 'text-align:center', raggedleft: 'text-align:right', raggedright: 'text-align:left',
  };
  const injectStyle = (tex = ''): string => [...[...tex.matchAll(/\\([A-Za-z]+)/g)].map((m) => SWITCH[m[1]!]).filter(Boolean),
    ...[...tex.matchAll(/\\(?:color|textcolor)\{([A-Za-z]+)\}/g)].map((m) => `color:${m[1]}`), /\$/.test(tex) ? 'font-style:italic' : ''].filter(Boolean).join(';');
  const blank = (): TCell => ({ text: '', span: 1, spec: '' });
  const balanced = (t: string) => { let d = 0; for (const m of t.replace(/\\./g, '').matchAll(/[{}]/g)) if ((d += m[0] === '{' ? 1 : -1) < 0) return false; return d === 0; };
  const MR = /^\\multirow\s*\{(\d+)\}\s*\{([^{}]*)\}\s*\{([\s\S]*)\}$/;
  /** A cell's `\multirow{n}{width}{text}`: [n, width, text] when it is one. */
  const multirow = (c: TCell): [number, string, string] | null => { const m = c.text.trim().match(MR); return m && balanced(m[3]!) ? [+m[1]!, m[2]!, m[3]!] : null; };
  const rowsOf = (c: TCell) => multirow(c)?.[0] ?? 1;
  const inner = (c: TCell) => multirow(c)?.[2] ?? c.text;
  const setInner = (c: TCell, t: string) => { const m = multirow(c); c.text = m ? `\\multirow{${m[0]}}{${m[1]}}{${t}}` : t; };
  const setRows = (c: TCell, n: number) => { const m = multirow(c), t = inner(c); c.text = n > 1 ? `\\multirow{${n}}{${m?.[1] ?? '*'}}{${t}}` : t; };
  const width = (row: TRow) => row.cells.reduce((n, c) => n + Math.max(1, c.span), 0);
  /** The index of the cell covering column `col`, and the column it starts at. */
  function cellAt(row: TRow, col: number): [number, number] {
    let at = 0;
    for (let i = 0; i < row.cells.length; i++) {
      const s = Math.max(1, row.cells[i]!.span);
      if (col < at + s) return [i, at];
      at += s;
    }
    return [-1, at];
  }
  /** Per row and column: the cell there `{r, i, c, span, rows}` (a `\multirow` covers the rows below it). */
  function layout(s: Tbl): Slot[][] {
    const map: Slot[][] = s.rows.map(() => []);
    s.rows.forEach((row, r) => {
      let c = 0;
      row.cells.forEach((cell, i) => {
        const span = Math.max(1, cell.span), n = rowsOf(cell);
        for (let k = 0; k < span; k++) map[r]![c + k] ??= { r, i, c, span, rows: 1 };
        if (n > 1) {
          const owner = { r, i, c, span, rows: Math.min(n, s.rows.length - r) };
          for (let j = 0; j < owner.rows; j++) for (let k = 0; k < span; k++) map[r + j]![c + k] = owner;
        }
        c += span;
      });
    });
    return map;
  }
  /** `[r0, c0, r1, c1]` grown until it holds whole cells, merged ones included. */
  function normalize(s: Tbl, [r0, c0, r1, c1]: Rect): Rect {
    [r0, r1] = [Math.min(r0, r1), Math.max(r0, r1)];
    [c0, c1] = [Math.min(c0, c1), Math.max(c0, c1)];
    const map = layout(s);
    for (let grown = true; grown;) {
      grown = false;
      for (let r = r0; r <= r1; r++) for (let c = c0; c <= c1; c++) {
        const o = map[r]?.[c];
        if (!o) continue;
        const [a, b, x, y] = [Math.min(r0, o.r), Math.min(c0, o.c), Math.max(r1, o.r + o.rows - 1), Math.max(c1, o.c + o.span - 1)];
        if (a !== r0 || b !== c0 || x !== r1 || y !== c1) { [r0, c0, r1, c1] = [a, b, x, y]; grown = true; }
      }
    }
    return [r0, c0, r1, c1];
  }
  /** Pads `row` to `n` columns with blank cells. */
  const pad = (row: TRow, n: number) => { while (width(row) < n) row.cells.push(blank()); };
  /** Merges the cells of `row` over columns c0..c1 into one `\multicolumn`; returns it. */
  function mergeRow(s: Tbl, row: TRow, c0: number, c1: number): TCell {
    pad(row, c1 + 1);
    const [i0] = cellAt(row, c0), [i1] = cellAt(row, c1);
    if (i0 === i1) return row.cells[i0]!;
    const parts = row.cells.slice(i0, i1 + 1);
    const cell = { text: parts.map(inner).filter((t) => t.trim()).join(' '), span: parts.reduce((n, c) => n + Math.max(1, c.span), 0), spec: parts[0]!.spec || 'c' };
    row.cells.splice(i0, i1 - i0 + 1, cell);
    return cell;
  }
  const T = {
    injected, alignOf, injectStyle,
    multirow, rowsOf, inner, setInner, layout, normalize, cellAt, width, RULES,
    /** Merges the block: a row span is a `\multicolumn`, a column span a `\multirow`, a block both. */
    merge(s: Tbl, rect: Rect) {
      const [r0, c0, r1, c1] = normalize(s, rect);
      if (r0 === r1 && c0 === c1) return false;
      const texts: string[] = [];
      for (let r = r0; r <= r1; r++) {
        pad(s.rows[r]!, c1 + 1);
        for (const c of s.rows[r]!.cells.slice(cellAt(s.rows[r]!, c0)[0], cellAt(s.rows[r]!, c1)[0] + 1)) if (inner(c).trim()) texts.push(inner(c).trim());
      }
      for (let r = r0; r <= r1; r++) {
        const cell = c1 > c0 ? mergeRow(s, s.rows[r]!, c0, c1) : s.rows[r]!.cells[cellAt(s.rows[r]!, c0)[0]]!;
        cell.text = r === r0 ? texts.join(' ') : '';
        if (r === r0) setRows(cell, r1 - r0 + 1);
      }
      return true;
    },
    /** Splits every merged cell in the block. */
    split(s: Tbl, rect: Rect) {
      const [r0, c0, r1, c1] = normalize(s, rect);
      for (let r = r0; r <= r1; r++) {
        const row = s.rows[r]!;
        for (let i = cellAt(row, c1)[0]; i >= 0 && i >= cellAt(row, c0)[0]; i--) {
          const c = row.cells[i]!;
          if (rowsOf(c) > 1) setRows(c, 1);
          if (c.span > 1) { const n = c.span; c.span = 1; c.spec = ''; row.cells.splice(i + 1, 0, ...Array.from({ length: n - 1 }, blank)); }
        }
      }
    },
    /** Aligns the block: whole columns by their spec, else each cell by a `\multicolumn{n}{a}`. */
    align(s: Tbl, rect: Rect, a: string) {
      const [r0, c0, r1, c1] = normalize(s, rect);
      if (r0 === 0 && r1 === s.rows.length - 1) {
        for (let c = c0; c <= c1; c++) { const col = s.cols[c]!; if (['l', 'c', 'r', 'S'].includes(col.align) || !col.align) { col.align = a; col.arg = ''; } }
        for (let r = r0; r <= r1; r++) for (const cell of s.rows[r]!.cells) if (cell.span === 1 && /^[lcr]$/.test(cell.spec)) cell.spec = '';
        return;
      }
      for (let r = r0; r <= r1; r++) {
        const row = s.rows[r]!;
        for (let i = cellAt(row, c0)[0]; i >= 0 && i <= cellAt(row, c1)[0]; i++) {
          const cell = row.cells[i]!;
          cell.spec = cell.spec ? cell.spec.replace(/[lcr]/, a) : a;
          if (cell.span === 1 && cell.spec === s.cols[cellAt(row, c0)[1]]?.align) cell.spec = '';
        }
      }
    },
    clear(s: Tbl, rect: Rect) {
      const [r0, c0, r1, c1] = normalize(s, rect);
      for (let r = r0; r <= r1; r++) { const row = s.rows[r]!; for (let i = cellAt(row, c0)[0]; i >= 0 && i <= cellAt(row, c1)[0]; i++) setInner(row.cells[i]!, ''); }
    },
    insertRow(s: Tbl, at: number): void {
      const rule = at === s.rows.length && s.rows.length && !s.rows.at(-1)!.end;
      if (rule) s.rows.at(-1)!.end = '\\\\';
      s.rows.splice(at, 0, { before: [], inline: false, cells: s.cols.map(blank), end: '\\\\', comment: '', raw: '' });
      // a rule that opened the table stays on top
      if (at === 0 && s.rows[1]?.before.length) { s.rows[0].before = s.rows[1].before.filter((x) => x === '\\toprule' || x === '\\hline'); s.rows[1].before = s.rows[1].before.filter((x) => !s.rows[0].before.includes(x)); }
    },
    deleteRows(s: Tbl, r0: number, r1: number) {
      if (r1 - r0 + 1 >= s.rows.length) return false;
      // a \multirow from above reaching in shrinks
      const map = layout(s);
      for (let c = 0; c < s.cols.length; c++) {
        const o = map[r0]?.[c];
        if (o && o.r < r0 && o.c === c) setRows(s.rows[o.r]!.cells[o.i]!, Math.max(1, o.rows - (Math.min(r1, o.r + o.rows - 1) - r0 + 1)));
      }
      const gone = s.rows.splice(r0, r1 - r0 + 1);
      if (s.rows[r0]) s.rows[r0]!.before = [...gone[0]!.before, ...s.rows[r0]!.before.filter((x) => !gone[0]!.before.includes(x))];
      else s.after = [...gone.at(-1)!.before.filter((x) => !RULES.includes(x)), ...s.after];
      return true;
    },
    insertCol(s: Tbl, at: number): void {
      s.cols.splice(at, 0, { align: 'l', arg: '', pre: '', post: '' });
      s.seps.splice(Math.max(1, Math.min(at, s.cols.length - 1)), 0, '');
      for (const r of s.rows) {
        const [i, start] = cellAt(r, at);
        if (i < 0) { if (width(r) === at || at === 0) r.cells.splice(at === 0 ? 0 : r.cells.length, 0, blank()); }
        else if (start === at) r.cells.splice(i, 0, blank());
        else r.cells[i]!.span++;
      }
    },
    deleteCols(s: Tbl, c0: number, c1: number) {
      if (c1 - c0 + 1 >= s.cols.length) return false;
      for (let c = c1; c >= c0; c--) {
        s.cols.splice(c, 1);
        s.seps.splice(c === s.cols.length ? c : c + 1, 1);
        for (const r of s.rows) {
          const [i] = cellAt(r, c);
          if (i < 0) continue;
          if (r.cells[i]!.span > 1) r.cells[i]!.span--; else r.cells.splice(i, 1);
        }
      }
      return true;
    },
    moveRow(s: Tbl, r: number, d: number) {
      const b = r + d;
      if (b < 0 || b >= s.rows.length) return false;
      for (const k of ['cells', 'raw', 'comment'] as const) [s.rows[r]![k], s.rows[b]![k]] = [s.rows[b]![k], s.rows[r]![k]] as [never, never];
      return true;
    },
    moveCol(s: Tbl, a: number, d: number) {
      const b = a + d;
      if (b < 0 || b >= s.cols.length) return false;
      const pairs = s.rows.map((r) => [cellAt(r, a), cellAt(r, b)]);
      if (pairs.some(([[i], [j]], k) => i >= 0 && j >= 0 && (s.rows[k]!.cells[i]!.span > 1 || s.rows[k]!.cells[j]!.span > 1))) return false;
      [s.cols[a], s.cols[b]] = [s.cols[b]!, s.cols[a]!];
      s.rows.forEach((r, k) => { const [[i], [j]] = pairs[k]!; if (i >= 0 && j >= 0) [r.cells[i], r.cells[j]] = [r.cells[j]!, r.cells[i]!]; });
      return true;
    },
    /** 'format': `&`s lined up, one space around; 'compact': no padding. Every row is written anew. */
    setStyle(s: Tbl, mode: string) {
      s.style.aligned = mode === 'format';
      s.style.spaced = mode === 'format';
      for (const r of s.rows) r.raw = '';
    },
    styleOf: (s: Tbl) => s.style.aligned ? 'format' : !s.style.spaced ? 'compact' : '',
    /** The rows before the first rule below the top: the header. */
    header(s: Tbl): number { const k = s.rows.findIndex((r, i) => i > 0 && r.before.some((x) => RULES.includes(x.trim()))); return k > 0 && k <= 3 ? k : 0; },
    /** TeX to the text it shows: specials unescaped, font commands and math shifts dropped. */
    plain(tex: unknown): string {
      let t = String(tex).replace(/\\textbackslash(\{\})?/g, '\u0001').replace(/\\textasciitilde(\{\})?/g, '\u0002').replace(/\\textasciicircum(\{\})?/g, '\u0003');
      for (let k = 0; k < 4; k++) t = t.replace(/\\(?:textbf|textit|emph|texttt|textrm|textsf|textsc|underline|mathrm|mathbf|text|mbox)\s*\{([^{}]*)\}/g, '$1');
      t = t.replace(/\\([&%$#_{}])/g, '\u0004$1').replace(/~/g, ' ').replace(/(?<!\u0004)[{}$]/g, '').replace(/\u0004/g, '');
      return t.replace(/\\,/g, ' ').replace(/\u0001/g, '\\').replace(/\u0002/g, '~').replace(/\u0003/g, '^').replace(/ /g, ' ').trim();
    },
    /** Text to TeX: the specials escaped. */
    tex: texText as (text: unknown) => string,
    /** CSV (or TSV with `sep` '\t') of the cells' text; a merged cell's text in its first column. */
    toCSV(s: Tbl, sep = ',', rect: Rect | null = null, raw = false): string {
      const [r0, c0, r1, c1] = rect ?? [0, 0, s.rows.length - 1, s.cols.length - 1];
      const map = layout(s), q = (v: string) => /["\n\r,;\t]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v;
      const lines: string[] = [];
      for (let r = r0; r <= r1; r++) {
        const out: string[] = [];
        for (let c = c0; c <= c1; c++) {
          const o = map[r]![c], own = o && o.r === r && o.c === c;
          const t = own ? inner(s.rows[r]!.cells[o.i]!) : '';
          out.push(q(raw ? t : T.plain(t)));
        }
        lines.push(out.join(sep));
      }
      return lines.join('\n');
    },
    /** Rows of fields from CSV, TSV or `;`-separated text (the separator guessed from the first line). */
    parseCSV(text: string): string[][] {
      const first = text.split('\n')[0]!;
      const sep = first.includes('\t') ? '\t' : (first.split(';').length > first.split(',').length ? ';' : ',');
      const rows: string[][] = [[]];
      let f = '', quoted = false;
      for (let i = 0; i < text.length; i++) {
        const ch = text[i];
        if (quoted) { if (ch === '"' && text[i + 1] === '"') { f += '"'; i++; } else if (ch === '"') quoted = false; else f += ch; }
        else if (ch === '"' && f === '') quoted = true;
        else if (ch === sep) { rows.at(-1)!.push(f); f = ''; }
        else if (ch === '\n' || ch === '\r') { if (ch === '\r' && text[i + 1] === '\n') i++; rows.at(-1)!.push(f); f = ''; rows.push([]); }
        else f += ch;
      }
      rows.at(-1)!.push(f);
      while (rows.length > 1 && rows.at(-1)!.every((x) => x === '')) rows.pop();
      return rows;
    },
    /** The table as JSON: columns, header rows, each cell's text and TeX (with its span). */
    toJSON(s: Tbl) {
      return {
        format: 'satex-table', columns: s.cols.map((c) => ({ align: c.align, ...(c.arg ? { arg: c.arg } : {}) })), header: T.header(s),
        rows: s.rows.map((r) => r.cells.map((c) => ({ text: T.plain(inner(c)), tex: c.text, ...(c.span > 1 ? { span: c.span } : {}), ...(c.spec ? { spec: c.spec } : {}) }))),
      };
    },
    /** Replaces the rows (and columns, from JSON) by `rows` of cells; rules stay where they stood. */
    fill(s: Tbl, rows: TCell[][], cols: Partial<TCol>[] | null = null): void {
      const n = Math.max(1, cols?.length ?? 0, ...rows.map((r) => r.reduce((k, c) => k + Math.max(1, c.span ?? 1), 0)));
      const old = s.rows, lastEnd = old.at(-1)?.end ?? '\\\\';
      s.rows = rows.map((cells, k) => ({ before: old[k]?.before ?? [], inline: old[k]?.inline ?? false, cells, end: k < rows.length - 1 ? '\\\\' : lastEnd || '\\\\', comment: '', raw: '' }));
      for (const r of s.rows) pad(r, n);
      const kept = s.cols;
      s.cols = Array.from({ length: n }, (_, k) => cols?.[k] ? { align: cols[k].align ?? 'l', arg: cols[k].arg ?? '', pre: kept[k]?.pre ?? '', post: kept[k]?.post ?? '' } : kept[k] ?? { align: 'l', arg: '', pre: '', post: '' });
      const outer = [s.seps[0] ?? '', s.seps.at(-1) ?? ''];
      s.seps = Array.from({ length: n + 1 }, (_, k) => k === 0 ? outer[0] : k === n ? outer[1] : s.seps[k] ?? '');
    },
    /** Imports CSV/TSV (text, escaped) or this dialog's JSON (TeX as written); false when unreadable. */
    importText(s: Tbl, text: string) {
      const t = text.trim();
      if (t.startsWith('{')) {
        let j: Rec;
        try { j = JSON.parse(t); } catch { return false; }
        if (!Array.isArray(j.rows)) return false;
        T.fill(s, j.rows.map((r: any[]) => r.map((c: any) => typeof c === 'string' ? { text: T.tex(c), span: 1, spec: '' } : { text: c.tex ?? T.tex(c.text ?? ''), span: c.span ?? 1, spec: c.spec ?? '' })), j.columns);
        return true;
      }
      const rows = T.parseCSV(t);
      if (!rows.length || !rows[0]!.length) return false;
      T.fill(s, rows.map((r) => r.map((v) => ({ text: T.tex(v.trim()), span: 1, spec: '' }))));
      return true;
    },
    /** Pastes `grid` (rows of TeX) with its top left at (r, c), growing the table as needed. */
    paste(s: Tbl, r: number, c: number, grid: string[][]) {
      while (s.rows.length < r + grid.length) T.insertRow(s, s.rows.length);
      const w = Math.max(...grid.map((g) => g.length));
      while (s.cols.length < c + w) T.insertCol(s, s.cols.length);
      grid.forEach((g, i) => g.forEach((v, j) => {
        const row = s.rows[r + i]!;
        pad(row, c + j + 1);
        const [k, start] = cellAt(row, c + j);
        if (k >= 0 && start === c + j) setInner(row.cells[k]!, v);
      }));
    },
    numeric: (t: string) => /^\s*[+\-−]?(\d+([.,]\d*)?|[.,]\d+)([eE][+-]?\d+)?\s*$/.test(t),
  };

  // ------------------------------------------------------------ table dialog

  /** Column types: the ones the packages loaded give, the rest offered with the package they add. */
  const COLS = [
    ['l', 'left', null], ['c', 'center', null], ['r', 'right', null], ['p', 'paragraph p{width}', null],
    ['m', 'middle m{width}', 'array'], ['b', 'bottom b{width}', 'array'], ['X', 'fill X', 'tabularx'], ['S', 'number S', 'siunitx'],
  ];
  const WIDTH = ['p', 'm', 'b'];
  const SVG = (d: string) => `<svg viewBox="0 0 16 16">${d}</svg>`;
  const AL: Record<string, string> = { l: SVG('<path d="M2.5 4h11M2.5 8h7M2.5 12h9"/>'), c: SVG('<path d="M2.5 4h11M4.5 8h7M3.5 12h9"/>'), r: SVG('<path d="M2.5 4h11M6.5 8h7M4.5 12h9"/>') };

  function table(view: DlgView, host: DlgHost) {
    const t = (): Tbl => view.state as Tbl;
    const ctx = view.context ?? {}, has = (p: string) => (ctx.loaded ?? []).includes(p);
    const main = h('div', { class: 'sd-tab' });
    const side = h('div');
    let anchor: number[] = [0, 0], focus: number[] = [0, 0], editing: { input: HTMLInputElement; done: boolean } | null = null;
    const f = frame(view, host, 'Edit table', {
      main, side, sub: `${t().env} · ${t().rows.length} × ${t().cols.length}`, redraw: () => draw(),
      foot: 'Drag or Shift+arrows to select. Type to edit a cell.',
      // a cell's text that breaks the table (`50%`, `R&D`); what a group holds is the cell's own
      lint: () => t().rows.flatMap((r, i) => r.cells.map((c, j): [number, string] => [j, texIssue(c.text.replace(/\{(?:[^{}]|\{[^{}]*\})*\}/g, '{}'))]).filter(([, m]) => m)
        .map(([j, m]) => `cell ${i + 1},${j + 1}: ${m}, write it escaped (\\% \\& \\_)`)).slice(0, 3),
    });
    /** Every edit: undoable, drawn, written. */
    const change = <R,>(fn: (s: Tbl) => R, op?: unknown): R => { const ok = fn(t()); draw(); f.refresh(op); return ok; };
    const rect = () => T.normalize(t(), [anchor[0]!, anchor[1]!, focus[0]!, focus[1]!]);
    const note = (m: string) => { msg.textContent = m; setTimeout(() => { if (msg.textContent === m) msg.textContent = ''; }, 4000); };
    const pkg = (name: string) => has(name) ? '' : ` (adds \\usepackage{${name}})`;

    // the toolbar: output style and rewrites; the selection's actions float beside it
    const seg = (mode: string, label: string, title: string) => h('button', { type: 'button', 'data-style': mode, title, onclick: () => change((s) => T.setStyle(s, mode)) }, label);
    const styleSeg = h('span', { class: 'sd-seg', role: 'group', 'aria-label': 'how the rows are written' },
      seg('format', 'Format', 'Write one row per line with the & lined up'), seg('compact', 'Compact', 'Write one row per line without padding'));
    const msg = h('span', { class: 'sd-note', role: 'status' });
    main.append(h('div', { class: 'sd-bar' },
      styleSeg, h('span', { class: 'sd-sep' }),
      h('button', { type: 'button', title: `Use booktabs rules (\\toprule, \\midrule, \\bottomrule) and drop vertical rules${pkg('booktabs')}`, onclick: () => f.refresh('booktabs') }, 'booktabs'),
      h('button', { type: 'button', title: `Align numeric columns at the decimal mark with siunitx S columns${pkg('siunitx')}`, onclick: () => f.refresh('siunitx') }, 'Numbers → S'),
      h('span', { class: 'sd-sep' }),
      msg));
    const grid = h('table', { class: 'sd-grid', role: 'grid', 'aria-label': 'the table\'s cells', 'aria-multiselectable': 'true' });
    const bar = h('div', { class: 'sd-selbar', hidden: true, role: 'toolbar', 'aria-label': 'the selection' });
    const wrap = h('div', { class: 'sd-wrap' }, grid, bar);
    main.append(wrap);

    // the side: typeset preview, the data in and out
    const prev = h('div', { class: 'sd-prev', 'aria-label': 'the table as typeset' });
    const importBox = h('textarea', { placeholder: 'Paste CSV, TSV from a spreadsheet, or JSON exported here', 'aria-label': 'data to import', spellcheck: 'false' });
    const download = (name: string, text: string, type: string) => { const a = h('a', { href: URL.createObjectURL(new Blob([text], { type })), download: name }); document.body.append(a); a.click(); a.remove(); };
    const copy = (text: string, what: string) => navigator.clipboard?.writeText(text).then(() => note(`${what} copied`), () => note('The browser blocked the clipboard'));
    const doImport = (text: string) => {
      const ok = change((s) => T.importText(s, text));
      if (!ok) return note('This is neither CSV nor JSON from this dialog');
      importBox.value = '';
      pasteBox.hidden = true;
      // numbers become S columns when siunitx is loaded
      const s = t(), nums = s.cols.some((_, c) => s.rows.slice(T.header(s) || 1).every((r) => { const [i, at] = T.cellAt(r, c); return i >= 0 && at === c && (T.numeric(T.plain(r.cells[i].text)) || !r.cells[i].text.trim()); }));
      if (has('siunitx') && nums && /^[\s\d.,+\-]*$/.test(text) === false && !text.trim().startsWith('{')) f.refresh('siunitx');
      note(`Imported ${s.rows.length} rows and ${s.cols.length} columns`);
    };
    // an import that replaces cells with text asks first, in place
    const ask = h('div', { class: 'sd-row sd-confirm', hidden: true });
    const importing = (text: string) => {
      if (!text.trim()) return;
      const filled = t().rows.some((r) => r.cells.some((c) => c.text.trim()));
      if (!filled) return doImport(text);
      ask.replaceChildren(h('span', { class: 'sd-muted' }, `Replace the ${t().rows.length} rows with the imported data?`),
        h('button', { type: 'button', class: 'sd-primary', onclick: () => { ask.hidden = true; doImport(text); } }, 'Replace'),
        h('button', { type: 'button', class: 'sd-ghost', onclick: () => { ask.hidden = true; } }, 'Cancel'));
      ask.hidden = false;
    };
    const fileIn = h('input', { type: 'file', accept: '.csv,.tsv,.txt,.json,text/csv,application/json', hidden: true, onchange: async () => { const fl = fileIn.files?.[0]; fileIn.value = ''; if (fl) importing(await fl.text()); } });
    // the clipboard when the browser allows it, else a box to paste into
    const pasteBox = h('div', { class: 'sd-import', hidden: true }, importBox, h('div', { class: 'sd-row' }, h('button', { type: 'button', class: 'sd-primary', onclick: () => importing(importBox.value) }, 'Import')));
    const paste = () => (navigator.clipboard?.readText?.() ?? Promise.reject()).then((text) => text.trim() ? importing(text) : Promise.reject(), () => { pasteBox.hidden = false; importBox.focus(); });
    // a file dropped on the grid imports too
    wrap.addEventListener('dragover', (e) => { if (e.dataTransfer?.types?.includes('Files')) { e.preventDefault(); wrap.classList.add('sd-over'); } });
    wrap.addEventListener('dragleave', () => wrap.classList.remove('sd-over'));
    wrap.addEventListener('drop', async (e) => { const fl = e.dataTransfer?.files?.[0]; if (!fl) return; e.preventDefault(); wrap.classList.remove('sd-over'); importing(await fl.text()); });
    const group = (label: string, ...buttons: HTMLElement[]) => h('div', { class: 'sd-row sd-data' }, h('span', { class: 'sd-data-label' }, label), h('span', { class: 'sd-seg', role: 'group', 'aria-label': label }, ...buttons));
    side.append(
      h('h4', {}, 'Preview'), prev,
      h('h4', {}, 'Data'),
      group('Export',
        h('button', { type: 'button', title: 'Copy the cells as CSV, TeX unescaped', onclick: () => copy(T.toCSV(t()), 'CSV') }, 'Copy CSV'),
        h('button', { type: 'button', title: 'Download the cells as CSV', onclick: () => download('table.csv', T.toCSV(t()), 'text/csv') }, 'CSV ↓'),
        h('button', { type: 'button', title: 'Download each cell\'s text and TeX as JSON, header rows marked', onclick: () => download('table.json', JSON.stringify(T.toJSON(t()), null, 2), 'application/json') }, 'JSON ↓')),
      group('Import',
        h('button', { type: 'button', title: 'Import CSV or TSV from the clipboard', onclick: paste }, 'Paste CSV/TSV'),
        h('button', { type: 'button', title: 'Import a CSV, TSV or JSON file. You can also drop it on the grid', onclick: () => fileIn.click() }, 'Load file…')),
      fileIn, ask, pasteBox,
      h('p', { class: 'sd-muted' }, `Imported text is escaped for TeX (& % $ # _ { } ~ ^ \\). ${has('siunitx') ? 'Numeric columns become siunitx S columns. ' : ''}Rules stay where they were.`),
    );

    /** The rule before row `k` (after the last, `k === rows.length`): a compact select for the row's gutter. */
    function ruleSel(k: number) {
      const s = t(), items = k < s.rows.length ? s.rows[k].before : s.after;
      const main = items.find((x) => RULES.includes(x)) ?? '';
      const choices = ['', '\\hline', ...(has('booktabs') || items.some((x) => /rule/.test(x)) ? ['\\toprule', '\\midrule', '\\bottomrule'] : [])];
      const where = k === s.rows.length ? 'below the last row' : `above row ${k + 1}`;
      const sel = h('select', { class: 'sd-rsel' + (main ? '' : ' none'), 'aria-label': `rule ${where}`, title: `the rule ${where}: ${main || 'none'}`, onchange: () => change(() => {
        const i = items.findIndex((x) => RULES.includes(x));
        if (i >= 0) { if (sel.value) items[i] = sel.value; else items.splice(i, 1); } else if (sel.value) items.unshift(sel.value);
      }) }, choices.map((c) => h('option', { value: c, selected: c === main }, c ? c.replace(/^\\|rule$/g, '') : '—')));
      return sel;
    }
    /** The line between rows `k - 1` and `k`: its rule drawn, a `+` that inserts a row here. */
    function ruleRow(k: number, taken: Set<number>) {
      const s = t(), items = k < s.rows.length ? s.rows[k].before : s.after;
      const main = items.find((x) => RULES.includes(x)) ?? '';
      const w = main === '\\toprule' || main === '\\bottomrule' ? '--w:2px' : main === '\\midrule' ? '--w:1.2px' : '';
      const tds: HTMLElement[] = [];
      for (let g = 0; g < 2 * s.cols.length + 1; g++) if (!taken.has(g)) tds.push(h('td', { class: main ? 'sd-ruled' : '', style: w }));
      const label = k === 0 ? 'Insert a row at the top' : k === s.rows.length ? 'Insert a row at the bottom' : `Insert a row between ${k} and ${k + 1}`;
      return h('tr', { class: 'sd-rule' }, h('td', { class: 'sd-rh' }, h('button', { type: 'button', class: 'sd-ins', title: label, 'aria-label': label, onclick: () => { change((s) => T.insertRow(s, k)); select([k, focus[1]]); } }, '+')), tds);
    }
    function draw() {
      const s = t(), map = T.layout(s), n = s.cols.length;
      for (const b of styleSeg.children) b.setAttribute('aria-pressed', String(b.dataset.style === T.styleOf(s)));
      const vbar = (k: number, tag: 'th' | 'td' = 'th') => {
        const label = k === 0 ? 'Insert a column at the start' : k === n ? 'Insert a column at the end' : `Insert a column between ${k} and ${k + 1}`;
        const ins = tag === 'th' ? h('button', { type: 'button', class: 'sd-ins', title: label, 'aria-label': label, onclick: (e: Event) => { e.stopPropagation(); change((s) => T.insertCol(s, k)); select([focus[0], k]); } }, '+') : null;
        return h(tag, { class: 'sd-vcol' + (s.seps[k]?.includes('|') ? ' on' : ''), title: 'Click to add or remove a vertical rule', onclick: () => change((s) => { s.seps[k] = s.seps[k].includes('|') ? s.seps[k].replace(/\|/g, '') : s.seps[k] + '|'; }) }, ins);
      };
      const head = h('tr', {}, h('th', { class: 'sd-rh' }), s.cols.flatMap((c, k) => {
        const loaded = COLS.filter(([, , p]) => !p || has(p)), more = COLS.filter(([, , p]) => p && !has(p));
        const known = COLS.some(([a]) => a === c.align);
        const sel = h('select', { 'aria-label': `column ${k + 1} type`, title: 'the column type', onchange: () => change(() => {
          c.align = sel.value;
          // the menu's package: loaded when the run lacks the letter
          c.package = COLS.find(([a]) => a === c.align)?.[2] ?? null;
          if (WIDTH.includes(c.align) && !c.arg) c.arg = '3cm';
          if (!WIDTH.includes(c.align) && c.align !== 'S') c.arg = '';
        }) },
        loaded.map(([a, nm]) => h('option', { value: a, selected: a === c.align }, nm)),
        more.length ? h('optgroup', { label: 'adds a package' }, more.map(([a, nm, p]) => h('option', { value: a, selected: a === c.align }, `${nm} · ${p}`))) : null,
        known ? null : h('option', { value: c.align, selected: true }, c.align));
        const arg = WIDTH.includes(c.align) || c.align === 'S' ? h('input', { value: c.arg, 'aria-label': `column ${k + 1} ${c.align === 'S' ? 'options' : 'width'}`, placeholder: c.align === 'S' ? 'table-format=…' : 'width', oninput: (e: Event) => { c.arg = (e.target as HTMLInputElement).value; f.refresh(); } }) : null;
        // array's injections: `>{…}` before and `<{…}` after each cell of the column
        const inject = (key: 'pre' | 'post', open: string, label: string) => texField(h('input', { value: T.injected(c[key]), class: 'sd-inject', 'aria-label': `column ${k + 1}: ${label.toLowerCase()}`, title: `${label}…${pkg('array')}`, placeholder: label === 'Insert before each cell' ? '>{…}' : '<{…}',
          oninput: (e: Event) => { const v = (e.target as HTMLInputElement).value; c[key] = v ? `${open}{${v}}` : ''; draw(); f.refresh(); } }));
        const del = n > 1 ? h('button', { type: 'button', class: 'sd-del', title: `Remove column ${k + 1}`, 'aria-label': `remove column ${k + 1}`, onclick: () => { change((s) => T.deleteCols(s, k, k)); select([focus[0], Math.min(k, t().cols.length - 1)]); } }, '−') : null;
        return [vbar(k), h('th', { class: 'sd-colh' }, sel, arg && c.align !== 'S' ? texField(arg) : arg,
          h('div', { class: 'sd-injects' }, inject('pre', '>', 'Insert before each cell'), inject('post', '<', 'Insert after each cell')), del)];
      }), vbar(n));
      const rows: HTMLElement[] = [];
      // grid columns a clean \multirow takes in the rule rows below its first
      const taken = s.rows.map(() => new Set<number>());
      const clean = (o: Slot) => { for (let j = 1; j < o.rows; j++) { const r = s.rows[o.r + j], [i, at] = T.cellAt(r, o.c); if (i < 0 || at !== o.c || Math.max(1, r.cells[i].span) !== o.span || r.cells[i].text.trim()) return false; } return true; };
      const skip = s.rows.map(() => new Set<number>());
      map.forEach((line, k) => line.forEach((o, c) => {
        if (!o || o.r !== k || o.c !== c || o.rows < 2 || !clean(o)) return;
        o.clean = true;
        for (let j = 1; j < o.rows; j++) { skip[k + j].add(o.c); for (let g = 2 * o.c; g < 2 * (o.c + o.span); g++) taken[k + j].add(g); }
      }));
      const [r0, c0, r1, c1] = rect();
      s.rows.forEach((r, k) => {
        rows.push(ruleRow(k, taken[k]));
        let col = 0;
        const tds = [];
        r.cells.forEach((c, i) => {
          const span = Math.max(1, c.span), at = col;
          col += span;
          if (skip[k].has(at)) return;
          const o = map[k][at], n = o && o.r === k && o.clean ? o.rows : 1;
          const align = alignOf(c.spec || s.cols[at]?.align || 'l', 'lcrS');
          const text = T.inner(c);
          const sel = !(k + n - 1 < r0 || k > r1 || at + span - 1 < c0 || at > c1);
          const merged = span > 1 || T.rowsOf(c) > 1;
          tds.push(h('td', {
            class: 'sd-cell' + (sel ? ' sel' : '') + (k === focus[0] && at <= focus[1] && focus[1] < at + span ? ' cur' : '') + (merged ? ' merged' : ''),
            colspan: 2 * span, rowspan: n > 1 ? 2 * n - 1 : null, tabindex: -1, role: 'gridcell', 'aria-selected': String(sel),
            'data-r': k, 'data-c': at, 'data-align': align, title: merged ? `merged: ${span > 1 ? `\\multicolumn{${span}}{${c.spec}}` : ''}${T.rowsOf(c) > 1 ? ` \\multirow{${T.rowsOf(c)}}` : ''}` : null,
          }, span === 1 && s.cols[at]?.pre ? h('span', { class: 'sd-inj', title: `set before each cell of this column: >{${T.injected(s.cols[at].pre)}}` }, T.injected(s.cols[at].pre)) : null, text ? text : h('span', { class: 'sd-ph' }, '·'),
          span === 1 && s.cols[at]?.post ? h('span', { class: 'sd-inj sd-inj-post', title: `set after each cell of this column: <{${T.injected(s.cols[at].post)}}` }, T.injected(s.cols[at].post)) : null));
        });
        const short = n - T.width(r);
        if (short > 0) tds.push(h('td', { colspan: 2 * short, class: 'sd-muted' }));
        const del = s.rows.length > 1 ? h('button', { type: 'button', class: 'sd-del', title: `Remove row ${k + 1}`, 'aria-label': `remove row ${k + 1}`, onclick: () => { change((s) => T.deleteRows(s, k, k)); select([Math.min(k, t().rows.length - 1), focus[1]]); } }, '−') : null;
        const others = r.before.filter((x) => !RULES.includes(x)).map((x) => h('span', { class: 'sd-chip', title: x }, x.length > 24 ? x.slice(0, 22) + '…' : x));
        rows.push(h('tr', { class: 'sd-data' }, h('td', { class: 'sd-rh' }, h('span', { class: 'sd-gut' }, ruleSel(k), h('span', { class: 'sd-n' }, String(k + 1)), del ?? h('span'))), tds, vbar(n, 'td'), h('td', { class: 'sd-chips' }, others)));
      });
      const after = s.after.filter((x) => !RULES.includes(x)).map((x) => h('span', { class: 'sd-chip', title: x }, x));
      rows.push(ruleRow(s.rows.length, new Set()), h('tr', { class: 'sd-end' }, h('td', { class: 'sd-rh' }, h('span', { class: 'sd-gut' }, ruleSel(s.rows.length))), h('td', { colspan: 2 * n + 1, class: 'sd-muted sd-endtxt' }, after.length ? after : ''), h('td')));
      grid.replaceChildren(h('thead', {}, head), h('tbody', {}, rows));
      preview();
      placeBar();
    }
    const cellEl = ([r, c]: number[]) => grid.querySelector(`td.sd-cell[data-r="${r}"][data-c="${c}"]`) ?? (() => { const o = T.layout(t())[r]?.[c]; return o && grid.querySelector(`td.sd-cell[data-r="${o.r}"][data-c="${o.c}"]`); })();
    /** Moves the selection: `to` the new focus, the anchor stays when `extend`. */
    function select(to: number[], extend = false) {
      const s = t();
      to = [Math.max(0, Math.min(s.rows.length - 1, to[0])), Math.max(0, Math.min(s.cols.length - 1, to[1]))];
      focus = to;
      if (!extend) anchor = to;
      draw();
      cellEl(focus)?.focus({ preventScroll: false });
    }
    // the selection's actions, floating at its top right
    function placeBar() {
      const s = t(), [r0, c0, r1, c1] = rect(), one = r0 === r1 && c0 === c1;
      const first = cellEl([r0, c0]), last = cellEl([r1, c1]);
      if (!first || !last || editing || !wrap.contains(document.activeElement) && !wrap.matches(':hover')) { bar.hidden = true; return; }
      const map = T.layout(s);
      let merged = false;
      for (let r = r0; r <= r1; r++) for (let c = c0; c <= c1; c++) { const o = map[r][c]; if (o && (o.span > 1 || o.rows > 1 || T.rowsOf(s.rows[o.r].cells[o.i]) > 1)) merged = true; }
      const what = r0 === r1 ? '\\multicolumn' : c0 === c1 ? '\\multirow' : '\\multicolumn + \\multirow';
      const b = (label: string, title: string, fn: () => void, disabled = false) => h('button', { type: 'button', title, 'aria-label': title, disabled, html: label, onclick: fn });
      const rows = r1 - r0 + 1, cols = c1 - c0 + 1;
      bar.replaceChildren(
        b('Merge', `Merge ${rows * cols} cells (${what})${r1 > r0 ? pkg('multirow') : ''}`, () => { change((s) => T.merge(s, [r0, c0, r1, c1])); anchor = focus = [r0, c0]; draw(); cellEl(focus)?.focus(); }, one),
        b('Split', 'Split the merged cells', () => change((s) => T.split(s, [r0, c0, r1, c1])), !merged),
        h('span', { class: 'sd-sep' }),
        ...['l', 'c', 'r'].map((a) => b(AL[a], `Align ${{ l: 'left', c: 'center', r: 'right' }[a]}${r0 === 0 && r1 === s.rows.length - 1 ? ' (the column type)' : ' (\\multicolumn{1})'}`, () => change((s) => T.align(s, [r0, c0, r1, c1], a)))),
        h('span', { class: 'sd-sep' }),
        b(SVG('<path d="M3 5h10M3 11h10"/><path d="M6 8h4" stroke-width="2"/>'), `Delete ${rows > 1 ? `rows ${r0 + 1}–${r1 + 1}` : `row ${r0 + 1}`}`, () => { change((s) => T.deleteRows(s, r0, r1)) || note('A table needs at least one row'); select([Math.min(r0, t().rows.length - 1), c0]); }, rows === s.rows.length),
        b(SVG('<path d="M5 3v10M11 3v10"/><path d="M8 6v4" stroke-width="2"/>'), `Delete ${cols > 1 ? `columns ${c0 + 1}–${c1 + 1}` : `column ${c0 + 1}`}`, () => { change((s) => T.deleteCols(s, c0, c1)) || note('A table needs at least one column'); select([r0, Math.min(c0, t().cols.length - 1)]); }, cols === s.cols.length),
        b(SVG('<path d="M4 4l8 8M12 4l-8 8"/>'), 'Clear the cells (Delete)', () => change((s) => T.clear(s, [r0, c0, r1, c1]))),
      );
      // above the selection; below it where it would cover the column header
      const wr = wrap.getBoundingClientRect(), a = first.getBoundingClientRect(), z = last.getBoundingClientRect();
      const y0 = wr.top - wrap.scrollTop, headBottom = grid.tHead!.getBoundingClientRect().bottom - y0, above = a.top - y0 - bar.offsetHeight - 6;
      bar.style.top = `${above >= headBottom ? above : z.bottom - y0 + 6}px`;
      bar.style.left = `${Math.max(0, Math.min(a.left - wr.left + wrap.scrollLeft, wrap.scrollWidth - bar.offsetWidth - 4))}px`;
      bar.hidden = false;
    }
    wrap.addEventListener('focusout', () => setTimeout(placeBar, 0));
    wrap.addEventListener('focusin', () => placeBar());

    // editing in place: Enter, F2, a double click or typing opens the cell's TeX
    function edit(r: number, c: number, seed: string | null = null) {
      const s = t(), o = T.layout(s)[r]?.[c];
      if (!o) return;
      const td = cellEl([o.r, o.c]), cell = s.rows[o.r].cells[o.i], was = T.inner(cell);
      f.history.hold(true);
      const input = h('input', { value: seed ?? was, 'aria-label': `row ${o.r + 1} column ${o.c + 1}`, spellcheck: 'false' });
      editing = { input, done: false };
      bar.hidden = true;
      td!.replaceChildren(input);
      input.focus();
      if (seed == null) input.select(); else input.setSelectionRange(input.value.length, input.value.length);
      const write = () => { T.setInner(cell, input.value); f.refresh(); };
      if (seed != null) write();
      input.oninput = write;
      const finish = (keep: boolean, move?: string) => {
        if (editing?.done) return;
        editing!.done = true;
        if (!keep) { T.setInner(cell, was); f.refresh(); }
        f.history.hold(false);
        editing = null;
        anchor = focus = [o.r, o.c];
        if (move) { const nx = move === 'down' ? [o.r + T.rowsOf(cell), o.c] : move === 'right' ? [o.r, o.c + Math.max(1, cell.span)] : [o.r, o.c - 1]; select(nx); }
        else { draw(); cellEl(focus)?.focus(); }
      };
      input.onkeydown = (e) => {
        if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) { e.preventDefault(); finish(true, 'down'); }
        else if (e.key === 'Tab') { e.preventDefault(); finish(true, e.shiftKey ? 'left' : 'right'); }
        else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); finish(false); }
      };
      input.onblur = () => { if (editing && !editing.done) finish(true); };
    }
    const at = (e: Event): [number, number] | null => { const td = (e.target as Element).closest?.<HTMLElement>('td.sd-cell'); return td ? [+td.dataset.r!, +td.dataset.c!] : null; };
    let dragging = false;
    grid.addEventListener('pointerdown', (e) => {
      const p = at(e);
      if (!p || e.button || editing && (e.target as Element).closest('input')) return;
      e.preventDefault();
      dragging = true;
      select(p, e.shiftKey);
    });
    grid.addEventListener('pointerover', (e) => { const p = at(e); if (dragging && p && (p[0] !== focus[0] || p[1] !== focus[1])) select(p, true); });
    addEventListener('pointerup', () => { dragging = false; });
    grid.addEventListener('dblclick', (e) => { const p = at(e); if (p) edit(...p); });
    grid.addEventListener('keydown', (e) => {
      if (editing || !(e.target as Element).closest('td.sd-cell')) return;
      const s = t(), mod = e.ctrlKey || e.metaKey, o = T.layout(s)[focus[0]][focus[1]];
      const step = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] }[e.key];
      if (step && e.altKey) {
        e.preventDefault();
        const ok = step[0] ? change((s) => T.moveRow(s, focus[0], step[0])) : change((s) => T.moveCol(s, focus[1], step[1]));
        if (ok === false) return note(step[0] ? 'No row there' : 'A merged cell spans the columns: split it first');
        anchor = focus = [focus[0] + step[0], focus[1] + step[1]];
        return select(focus);
      }
      if (step) {
        e.preventDefault();
        const from = e.shiftKey ? focus : [o.r, o.c];
        const nx = [from[0] + (step[0] > 0 ? (e.shiftKey ? 1 : o.rows) : step[0]), from[1] + (step[1] > 0 ? (e.shiftKey ? 1 : o.span) : step[1])];
        return select(nx, e.shiftKey);
      }
      if (e.key === 'Tab') { e.preventDefault(); return select([focus[0], focus[1] + (e.shiftKey ? -1 : o.span)]); }
      if (e.key === 'Enter' || e.key === 'F2') { e.preventDefault(); return edit(focus[0]!, focus[1]!); }
      if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); return change((s) => T.clear(s, rect())); }
      if (mod && e.key.toLowerCase() === 'a') { e.preventDefault(); anchor = [0, 0]; focus = [s.rows.length - 1, s.cols.length - 1]; draw(); cellEl(focus)?.focus(); return; }
      if (e.key.length === 1 && !mod && !e.altKey) { e.preventDefault(); return edit(focus[0]!, focus[1]!, e.key); }
    });
    // copy and paste: TSV, so a spreadsheet's cells paste in; our own copy keeps the TeX
    grid.addEventListener('copy', (e) => {
      if (editing) return;
      e.preventDefault();
      e.clipboardData!.setData('text/plain', T.toCSV(t(), '\t', rect()));
      e.clipboardData!.setData('text/x-satex-cells', T.toCSV(t(), '\t', rect(), true));
    });
    grid.addEventListener('cut', (e) => { if (editing) return; grid.dispatchEvent(new ClipboardEvent('copy', { clipboardData: e.clipboardData })); e.preventDefault(); e.clipboardData!.setData('text/plain', T.toCSV(t(), '\t', rect())); change((s) => T.clear(s, rect())); });
    grid.addEventListener('paste', (e) => {
      if (editing) return;
      e.preventDefault();
      const own = e.clipboardData!.getData('text/x-satex-cells'), text = own || e.clipboardData!.getData('text/plain');
      if (!text) return;
      const rows = T.parseCSV(text.replace(/\n$/, '')).map((r) => r.map((v) => own ? v : T.tex(v)));
      const [r0, c0] = rect();
      change((s) => T.paste(s, r0, c0, rows));
      anchor = [r0, c0]; focus = [r0 + rows.length - 1, c0 + Math.max(...rows.map((r) => r.length)) - 1];
      draw(); cellEl(focus)?.focus();
    });

    /** The table as LaTeX sets it: alignment, spans, rules (booktabs' weights), vertical rules. */
    function preview() {
      const s = t(), map = T.layout(s), n = s.cols.length, html = (x: string) => esc(T.plain(x)).replace(/\^\{?(\w+)\}?/g, '<sup>$1</sup>');
      const rule = (items: string[]) => { const m = items.find((x) => RULES.includes(x)); return m === '\\toprule' || m === '\\bottomrule' ? '1.2px' : m === '\\midrule' ? '.7px' : m === '\\hline' ? '.5px' : ''; };
      const cl = (items: string[]) => { const out = new Set(); for (const x of items) { const m = x.match(/^\\c(?:mid)?rule(?:\([^)]*\))?\{(\d+)-(\d+)\}/); if (m) for (let c = +m[1] - 1; c < +m[2]; c++) out.add(c); } return out; };
      const vb = (k: number) => s.seps[k]?.includes('|') ? '.5px solid' : '';
      const body = s.rows.map((r, k) => {
        const top = rule(r.before), part = cl(r.before);
        let col = 0;
        const tds = r.cells.map((c) => {
          const span = Math.max(1, c.span), at = col;
          col += span;
          const o = map[k][at];
          if (o && o.r < k) return '';
          const a = alignOf(c.spec || s.cols[at]?.align || 'l');
          const align = { c: 'center', r: 'right', S: 'center' }[a] ?? 'left';
          const wide = ['p', 'm', 'b'].includes(a) && span === 1 ? `width:${/^\d/.test(s.cols[at].arg) ? +s.cols[at]!.arg.replace(/cm$/, '') * 37.8 + 'px' : '8em'};white-space:normal;` : a === 'X' ? 'width:100%;' : '';
          // the column's injects (a \multicolumn's own spec instead), after its alignment so `\centering` wins as in LaTeX
          const look = injectStyle(span > 1 || c.spec ? injected(c.spec) : `${injected(s.cols[at]?.pre)}${injected(s.cols[at]?.post)}`);
          const st = [`text-align:${align}`, look, top || part.has(at) ? `border-top:${top || '.5px'} solid` : '', k === s.rows.length - 1 && rule(s.after) ? `border-bottom:${rule(s.after)} solid` : '', at === 0 && vb(0) ? `border-left:${vb(0)}` : '', vb(at + span) ? `border-right:${vb(at + span)}` : '', wide].filter(Boolean).join(';');
          const rs = o && o.r === k && o.rows > 1 ? ` rowspan="${o.rows}" class="mr"` : '';
          return `<td colspan="${span}"${rs} style="${st}">${html(T.inner(c)) || '&nbsp;'}</td>`;
        }).join('');
        return `<tr>${tds}${col < n ? `<td colspan="${n - col}"></td>` : ''}</tr>`;
      }).join('');
      prev.innerHTML = `<table${t().env === 'tabularx' ? ' style="width:100%"' : ''}>${body}</table>`;
    }
    draw();
    f.refresh();
    setTimeout(() => cellEl(focus)?.focus(), 0);
    return f;
  }


  const fmt = (x: number) => String(Math.round(x * 100) / 100);
  /** The content's box `[x0, y0, x1, y1]` (pixels, x1/y1 exclusive) of RGBA `data` (w × h): the pixels
   * neither transparent nor within `tol` of the background, read from the corners. Null when blank. */
  function contentBox(data: ArrayLike<number>, w: number, hh: number, tol = 24): number[] | null {
    const px = (i: number) => [data[i], data[i + 1], data[i + 2], data[i + 3]];
    const corners = [0, w - 1, (hh - 1) * w, hh * w - 1].map((k) => px(k * 4));
    const key = (c: number[]) => c[3]! < 16 ? 'clear' : c.slice(0, 3).join();
    const votes = new Map<string, number[][]>();
    for (const c of corners) votes.set(key(c), [...votes.get(key(c)) ?? [], c]);
    const bg = [...votes.values()].sort((a, b) => b.length - a.length)[0][0];
    const blank = (i: number) => data[i + 3]! < 16 || (bg[3] >= 16 && Math.abs(data[i] - bg[0]) <= tol && Math.abs(data[i + 1] - bg[1]) <= tol && Math.abs(data[i + 2] - bg[2]) <= tol);
    let x0 = w, y0 = hh, x1 = 0, y1 = 0;
    for (let y = 0; y < hh; y++) for (let x = 0; x < w; x++) {
      if (blank((y * w + x) * 4)) continue;
      if (x < x0) x0 = x;
      if (x >= x1) x1 = x + 1;
      if (y < y0) y0 = y;
      if (y >= y1) y1 = y + 1;
    }
    return x1 > x0 ? [x0, y0, x1, y1] : null;
  }
  /** `\includegraphics`' trim `[l, b, r, t]` in bp keeping `box` (pixels of a w × h raster of `nat` bp) and `pad` bp around it (negative: into it). */
  const trimTo = ([x0, y0, x1, y1]: number[], w: number, hh: number, nat: number[], pad = 0) => {
    const kx = nat[0]! / w, ky = nat[1]! / hh, r = (v: number) => Math.max(0, Math.round(v * 100) / 100);
    return [r(x0 * kx - pad), r((hh - y1) * ky - pad), r((w - x1) * kx - pad), r(y0 * ky - pad)];
  };
  /** The colour channels `decodearray` maps of a raster's bytes: `gray`, `rgb`, `indexed`, `cmyk`, else null. */
  function colorModel(b?: Uint8Array | null): string | null {
    if (b?.[0] === 0x89 && b[1] === 0x50) return ({ 0: 'gray', 4: 'gray', 2: 'rgb', 6: 'rgb', 3: 'indexed' } as Record<number, string>)[b[25]!] ?? null;
    if (b?.[0] !== 0xff || b[1] !== 0xd8) return null;
    for (let i = 2; i + 9 < b.length && b[i] === 0xff;) {
      const m = b[i + 1]!;
      if (m >= 0xc0 && m <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(m)) return ({ 1: 'gray', 3: 'rgb', 4: 'cmyk' } as Record<number, string>)[b[i + 9]!] ?? null;
      i += 2 + (b[i + 2]! << 8 | b[i + 3]!);
    }
    return null;
  }
  /** `decodearray` (n values, two per channel) maps channel value v to Dmin + v·(Dmax − Dmin) (PDF reference § 8.9.5.2): `tint` takes
   * white to `rgb` by `strength`, `invert` swaps each pair; a colour of a gray image is its own luminance. */
  const DECODE = {
    invert: (n: number) => Array(n / 2).fill([1, 0]).flat(),
    tint: (n: number, rgb: number[], strength: number) => (n === 2 ? [0.2126 * rgb[0]! + 0.7152 * rgb[1]! + 0.0722 * rgb[2]!] : rgb).flatMap((c) => [0, Math.round((1 - strength * (1 - c)) * 100) / 100]),
    /** The preset `d` is, with its colour: invert, tint, custom or none. */
    of: (d?: number[] | null): [string, number[]?] => !d?.length ? ['none'] : d.every((x, i) => x === (i % 2 ? 0 : 1)) ? ['invert'] : d.every((x, i) => i % 2 || x === 0) ? ['tint', d.length === 6 ? [d[1], d[3], d[5]] : [d[1], d[1], d[1]]] : ['custom'],
  };
  /** A styled file picker in place of the browser's: a button and a drop zone that name the chosen file. */
  function picker({ accept, label, title, onfile }: { accept: string; label: string; title: string; onfile: (f: File) => unknown }) {
    const input = h('input', { type: 'file', accept, hidden: true, onchange: () => { const fl = input.files?.[0]; input.value = ''; if (fl) pick(fl); } });
    const name = h('span', { class: 'sd-drop-name' }, 'or drop a file here');
    const pick = (fl: File) => { name.textContent = fl.name; onfile(fl); };
    const zone = h('div', { class: 'sd-drop', title },
      h('button', { type: 'button', onclick: () => input.click() }, label), name, input);
    zone.addEventListener('dragover', (e) => { e.preventDefault(); zone.classList.add('over'); });
    zone.addEventListener('dragleave', () => zone.classList.remove('over'));
    zone.addEventListener('drop', (e) => { e.preventDefault(); zone.classList.remove('over'); const fl = e.dataTransfer?.files?.[0]; if (fl) pick(fl); });
    return zone;
  }
  /** A range input whose track fills up to its value. */
  const slider = (attrs: Record<string, any>) => {
    const r = h('input', { type: 'range', class: 'sd-range', ...attrs });
    const fill = () => r.style.setProperty('--p', `${(+r.value - +r.min) / (+r.max - +r.min) * 100}%`);
    r.addEventListener('input', fill);
    return Object.assign(r, { fill });
  };
  let filters = 0;
  function image(view: DlgView, host: DlgHost) {
    const s = () => view.state as ImgState;
    const ctx = view.context ?? {};
    const main = h('div');
    const side = h('div');
    const f = frame(view, host, 'Edit image', { main, side, sub: s().file, redraw: () => sync(), foot: 'Your crop, rotation, size and tint go into the \\includegraphics options.' });
    let nat = s().natural; // [w, h] bp
    let model: string | null = null; // the raster's channels, null for a PDF or an unread file
    const vector = /\.(pdf|eps|ps|mps)$/i.test(ctx.path ?? s().file);
    const stage = h('div', { class: 'sd-stage' });
    const crop = h('div', { class: 'sd-crop' });
    const handles = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'].map((d) => h('div', { class: 'sd-h', 'data-d': d, style: `cursor:${d}-resize` }));
    crop.append(...handles);
    const prevBox = h('div', { class: 'sd-prev-box' });
    // the tint's live preview: decodearray's linear map per channel as an SVG filter
    const fid = `sd-decode-${++filters}`;
    const funcs = ['R', 'G', 'B'].map((c) => document.createElementNS('http://www.w3.org/2000/svg', `feFunc${c}`));
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('class', 'sd-defs');
    svg.innerHTML = `<filter id="${fid}" color-interpolation-filters="sRGB"><feComponentTransfer/></filter>`;
    svg.querySelector('feComponentTransfer')!.append(...funcs);
    main.append(svg, stage, h('h4', {}, 'Result'), prevBox);
    let picture: HTMLImageElement | HTMLElement | null = null;
    const note = h('p', { class: 'sd-note', role: 'status' });
    const say = (m: string) => { note.textContent = m; };
    const num = (i: number, label: string) => [h('label', { for: `sd-trim-${i}-${fid}` }, label), h('input', { id: `sd-trim-${i}-${fid}`, type: 'number', step: '1', class: 'sd-num', 'data-k': 'trim', 'data-i': i, oninput: (e: Event) => { const st = s(); st.trim ??= [0, 0, 0, 0]; st.trim[i] = Number.isFinite(+(e.target as HTMLInputElement).value) ? +(e.target as HTMLInputElement).value : 0; st.clip = true; sync(); } })];
    const angle = slider({ min: -180, max: 180, step: 1, 'aria-label': 'angle', oninput: (e: Event) => setAngle(+(e.target as HTMLInputElement).value) });
    const angleNum = h('input', { type: 'number', step: 'any', class: 'sd-num', 'aria-label': 'angle in degrees', oninput: (e: Event) => setAngle(+(e.target as HTMLInputElement).value) });
    const setAngle = (a: number) => { s().angle = a ? ((a % 360) + 540) % 360 - 180 : null; sync(); };
    const lenInput = (key: 'width' | 'height', presets: string[]) => {
      const input = h('input', { class: 'sd-len', id: `sd-${key}-${fid}`, list: `sd-${key}-l-${fid}`, 'aria-label': key, placeholder: 'natural', oninput: (e: Event) => { s()[key] = (e.target as HTMLInputElement).value || null; sync(); } });
      return [input, h('datalist', { id: `sd-${key}-l-${fid}` }, presets.map((p) => h('option', { value: p }))), texField(input)];
    };
    const [wIn, wList, wBox] = lenInput('width', ['\\linewidth', '0.8\\linewidth', '0.5\\linewidth', '\\textwidth', '5cm']);
    const [hIn, hList, hBox] = lenInput('height', ['\\textheight', '0.3\\textheight', '4cm']);
    const scale = h('input', { type: 'number', step: '0.05', min: '0', class: 'sd-num', id: `sd-scale-${fid}`, placeholder: '1', oninput: (e: Event) => { const v = (e.target as HTMLInputElement).value; s().scale = v ? +v : null; sync(); } });
    const keep = h('input', { type: 'checkbox', role: 'switch', onchange: (e: Event) => { s().keepaspectratio = (e.target as HTMLInputElement).checked; sync(); } });
    const clip = h('input', { type: 'checkbox', role: 'switch', onchange: (e: Event) => { s().clip = (e.target as HTMLInputElement).checked; sync(); } });
    const order = h('select', { id: `sd-order-${fid}`, onchange: (e: Event) => { s().size_first = (e.target as HTMLSelectElement).value === 'upright'; sync(); } },
      h('option', { value: 'rotated' }, 'the rotated figure (angle, then width)'), h('option', { value: 'upright' }, 'the upright figure (width, then angle)'));
    const unread = h('p', { class: 'sd-note' });
    const pad = h('input', { type: 'number', step: '1', value: '0', class: 'sd-num', 'aria-label': 'padding in bp', title: 'Space to keep around the content, in bp. A negative value cuts into it.' });
    // crop to content: the box of the pixels that differ from the background, read from the rendered figure
    const fit = h('button', { type: 'button', title: 'Trim the empty margins of the rendered figure', onclick: () => {
      const pic = picture as HTMLImageElement;
      if (pic?.tagName !== 'IMG' || !nat) return;
      const n = Math.min(1, 1600 / Math.max(pic.naturalWidth, pic.naturalHeight)), w = Math.max(1, Math.round(pic.naturalWidth * n)), hh = Math.max(1, Math.round(pic.naturalHeight * n));
      const c = Object.assign(document.createElement('canvas'), { width: w, height: hh }), g = c.getContext('2d', { willReadFrequently: true })!;
      g.drawImage(pic, 0, 0, w, hh);
      const box = contentBox(g.getImageData(0, 0, w, hh).data, w, hh);
      if (!box) return say('The figure looks empty, so there is nothing to crop to.');
      const p = +pad.value || 0, most = Math.min((box[2] - box[0]) * nat[0] / w, (box[3] - box[1]) * nat[1] / hh) / 2;
      if (-p >= most) return say(`A padding of ${fmt(p)} bp cuts away all content. Use more than −${fmt(most)} bp.`);
      say('');
      s().trim = trimTo(box, w, hh, nat, p);
      s().clip = true;
      sync();
    } }, 'Crop to content');
    // tint: decodearray, for a raster the driver reads it of
    const tintColor = h('input', { type: 'color', value: '#1f6feb', 'aria-label': 'tint colour', oninput: () => setTint('tint') });
    const strength = slider({ min: 0, max: 100, step: 1, value: 60, 'aria-label': 'tint strength', oninput: () => setTint('tint') });
    const strengthOut = h('output', { class: 'sd-out' });
    const tintWhy = h('p', { class: 'sd-muted sd-why' });
    const preset = (mode: string, label: string, title: string) => h('button', { type: 'button', 'data-tint': mode, title, onclick: () => setTint(mode) }, label);
    // a blocked tint says why where the buttons are, not only in the note below
    const tintSeg = h('span', { class: 'sd-seg', role: 'group', 'aria-label': 'tint' },
      preset('none', 'None', 'Show the colours as they are in the file'), preset('tint', 'Tint', 'Turn white into the colour and keep black'),
      preset('invert', 'Invert', 'Invert each channel (decodearray 1 0)'),
      h('button', { type: 'button', disabled: true, title: 'Not possible: decodearray maps each channel on its own and cannot mix red, green and blue into gray' }, 'Grayscale'));
    const channels = () => (({ gray: 2, rgb: 6 }) as Record<string, number>)[model ?? ''] ?? 0;
    /** Why decodearray cannot apply here, or null. */
    const tintBlocked = () => vector ? 'PDF and EPS figures are drawn, not sampled: the driver drops decodearray for them (pdftex.def, xetex.def, dvipdfmx.def).'
      : !ctx.decode ? `The graphics driver${ctx.driver ? ` ${ctx.driver}` : ''} of this run does not read decodearray: pdfTeX, LuaTeX, XeTeX and dvipdfmx do, dvips does not.`
      : model === 'indexed' ? 'A palette PNG: decodearray would remap the palette\'s indices, not its colours.'
      : model === 'cmyk' ? 'A CMYK JPEG: its four channels are not offered here.'
      : !model ? 'The file\'s colour channels are not read yet.' : null;
    const rgbOf = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    const hexOf = (rgb: number[]) => '#' + rgb.map((c) => Math.round(Math.max(0, Math.min(1, c)) * 255).toString(16).padStart(2, '0')).join('');
    function setTint(mode: string) {
      const n = channels();
      if (mode !== 'none' && (!n || tintBlocked())) return;
      s().decode = mode === 'none' ? null : mode === 'invert' ? DECODE.invert(n) : DECODE.tint(n, rgbOf(tintColor.value), +strength.value / 100);
      sync();
    }
    // the file: replaced by an upload (written to the project, its path into the text), or downloaded
    const replace = host.upload && picker({ accept: '.png,.jpg,.jpeg,.pdf,.eps,image/png,image/jpeg,application/pdf', label: 'Replace…', title: 'A new PNG, JPEG, PDF or EPS in place of this figure', onfile: async (fl) => {
      if (!/\.(png|jpe?g|pdf|eps)$/i.test(fl.name)) return say(`${fl.name}: graphicx reads PNG, JPEG, PDF or EPS here.`);
      if (fl.size > 50 * 2 ** 20) return say(`${fl.name} is larger than 50 MiB.`);
      try {
        const got = await host.upload!(ctx.path, fl.name, new Uint8Array(await fl.arrayBuffer()));
        const st = s(), at = st.file.lastIndexOf('/') + 1, old = st.file.slice(at), stem = fl.name.replace(/\.\w+$/, '');
        st.file = got?.beside === false ? fl.name : st.file.slice(0, at) + (!/\.\w+$/.test(old) && old === stem ? old : fl.name);
        Object.assign(st, { trim: null, viewport: false, clip: false, decode: null, natural: null });
        if (await f.refresh()) { f.apply(); host.reopen?.(); }
      } catch (e) { say(`Upload: ${(e as Error).message}`); }
    } });
    const download = h('button', { type: 'button', title: 'Download the figure\'s file', onclick: async () => {
      const got: { saved?: boolean; url?: string; name?: string } | null | undefined = host.download ? await host.download(ctx.path) : vector ? null : await host.image(ctx.path);
      // an editor's host saves it itself
      if (got?.saved) return;
      if (!got?.url) return say('The file is not available here.');
      const a = h('a', { href: got.url, download: got.name ?? (ctx.path ?? s().file).split('/').pop() });
      document.body.append(a); a.click(); a.remove();
    } }, 'Download');
    const row = (...kids: unknown[]) => h('div', { class: 'sd-row' }, ...kids);
    const field = (label: string, input: HTMLElement, id = input.id) => [h('label', { for: id }, label), input];
    side.append(
      h('h4', {}, 'File'),
      // the name as written, and the format the run found for it (`example-image` is a PDF)
      row(h('span', { class: 'sd-chip sd-file', title: ctx.path ?? '' }, s().file, ...[(ctx.path ?? s().file).match(/\.(\w+)$/)?.[1]].filter(Boolean).map((x) => h('span', { class: 'sd-fmt', title: `the file the run reads is a ${x.toUpperCase()}` }, x.toUpperCase()))), ctx.path ? download : null),
      ...(replace ? [replace] : []),
      h('h4', {}, 'Crop ', h('span', { class: 'sd-unit' }, 'trim, bp')),
      h('div', { class: 'sd-quad' }, ...num(0, 'left'), ...num(2, 'right'), ...num(3, 'top'), ...num(1, 'bottom')),
      row(fit, h('label', { class: 'sd-inl', title: 'Space to keep around the content, in bp. A negative value cuts into it.' }, 'padding', pad)),
      row(h('label', { class: 'sd-inl' }, clip, 'clip'), h('button', { type: 'button', class: 'sd-ghost', style: 'margin-left:auto', onclick: () => { s().trim = null; s().clip = false; say(''); sync(); } }, 'Reset crop')),
      h('h4', {}, 'Rotate ', h('span', { class: 'sd-unit' }, 'degrees, counterclockwise')),
      row(h('button', { type: 'button', title: 'Rotate 90° counterclockwise', onclick: () => setAngle((s().angle ?? 0) + 90) }, '⟲ 90°'), h('button', { type: 'button', title: 'Rotate 90° clockwise', onclick: () => setAngle((s().angle ?? 0) - 90) }, '⟳ 90°'), angleNum),
      angle,
      h('h4', {}, 'Size'),
      h('div', { class: 'sd-form' }, ...field('width', wBox, wIn.id), ...field('height', hBox, hIn.id), ...field('scale', scale), ...field('applies to', order)),
      wList, hList,
      h('label', { class: 'sd-inl', title: 'Fit inside both width and height and keep the proportions' }, keep, 'keep aspect ratio'),
      h('p', { class: 'sd-muted' }, 'Key order matters. angle=90,width=3cm makes the rotated figure 3cm wide. width=3cm,angle=90 makes it 3cm high.'),
      h('h4', {}, 'Tint ', h('span', { class: 'sd-unit' }, 'decodearray')),
      tintSeg,
      h('div', { class: 'sd-form sd-tint' }, h('label', {}, 'colour'), tintColor, h('label', {}, 'strength'), h('div', { class: 'sd-row', style: 'margin:0;flex-wrap:nowrap' }, strength, strengthOut)),
      tintWhy,
      note, unread,
    );
    // the figure: a raster as it is, a PDF's first page as the host renders it
    (async () => {
      const got = ctx.path ? await host.image(ctx.path).catch(() => null) : null;
      if (got?.url && got.kind !== null) {
        picture = h('img', { src: got.url, alt: s().file, draggable: 'false' });
        picture.addEventListener('load', () => { nat ??= [(picture as HTMLImageElement).naturalWidth, (picture as HTMLImageElement).naturalHeight]; sync(); }, { once: true });
        if (!vector) model = colorModel(new Uint8Array(await fetch(got.url).then((r) => r.arrayBuffer()).catch(() => new ArrayBuffer(0))));
      } else {
        const [w, hh] = nat ?? [400, 300];
        const k = Math.min(480 / w, 320 / hh);
        picture = h('div', { class: 'sd-ph', style: `width:${w * k}px;height:${hh * k}px` }, h('div', {}, h('b', {}, s().file), h('br'), got?.kind === 'pdf' ? 'PDF: no renderer here, the crop works on its box' : 'no preview', h('br'), nat ? `${fmt(nat[0])} × ${fmt(nat[1])} bp` : ''));
      }
      fit.disabled = picture.tagName !== 'IMG';
      if (fit.disabled) fit.title = 'Needs the rendered figure';
      stage.replaceChildren(picture, crop);
      sync();
    })();
    /** Display pixels per bp. */
    const k = () => (picture?.getBoundingClientRect().width || 1) / (nat?.[0] || 1);
    function sync() {
      const st = s(), [l, b, r, tp] = st.trim ?? [0, 0, 0, 0];
      for (const i of side.querySelectorAll('input[data-k=trim]')) if (document.activeElement !== i) i.value = fmt((st.trim ?? [0, 0, 0, 0])[+i.dataset.i]);
      if (document.activeElement !== angleNum) angleNum.value = fmt(st.angle ?? 0);
      angle.value = String(st.angle ?? 0);
      angle.fill();
      if (document.activeElement !== wIn) wIn.value = st.width ?? '';
      if (document.activeElement !== hIn) hIn.value = st.height ?? '';
      if (document.activeElement !== scale) scale.value = String(st.scale ?? '');
      keep.checked = !!st.keepaspectratio;
      clip.checked = !!st.clip;
      order.value = st.size_first ? 'upright' : 'rotated';
      unread.textContent = st.unread?.length ? `Kept as written, not shown here: ${st.unread.join(', ')}.` : '';
      // tint: the preset the array is, the reason it cannot apply, the preview's filter
      const [mode, rgb] = DECODE.of(st.decode), blocked = tintBlocked();
      const ui = DECODE.tint(channels() || 6, rgbOf(tintColor.value), +strength.value / 100);
      if (mode === 'tint' && ![tintColor, strength].includes(document.activeElement as HTMLInputElement) && ui.join() !== st.decode!.join()) { tintColor.value = hexOf(rgb!); strength.value = '100'; }
      strength.fill();
      strengthOut.textContent = `${strength.value}%`;
      for (const b of tintSeg.querySelectorAll('[data-tint]')) { b.setAttribute('aria-pressed', String(b.dataset.tint === mode)); b.disabled = b.dataset.tint !== 'none' && !!blocked; if (b.disabled) b.title = blocked; }
      tintWhy.classList.toggle('sd-blocked', !!blocked);
      tintColor.disabled = strength.disabled = !!blocked;
      tintWhy.textContent = blocked ?? (mode === 'custom' ? `A decodearray of its own: ${st.decode!.join(' ')}.` : model === 'gray' ? 'A gray image has one channel: a tint darkens it by the colour\'s luminance.' : 'decodearray maps each channel from [0, 1] to [Dmin, Dmax]; the preview applies the same map.');
      const d = st.decode?.length && st.decode.length === channels() ? st.decode : null;
      funcs.forEach((fn, c) => { const [lo, hi] = d ? (d.length === 2 ? d : d.slice(2 * c, 2 * c + 2)) : [0, 1]; fn.setAttribute('type', 'linear'); fn.setAttribute('slope', String(hi - lo)); fn.setAttribute('intercept', String(lo)); });
      const look = d && !blocked ? `url(#${fid})` : '';
      if (picture?.tagName === 'IMG') picture.style.filter = look;
      if (picture && nat) {
        const q = k(), W = nat[0] * q, H = nat[1] * q;
        Object.assign(crop.style, { left: `${l * q}px`, top: `${tp * q}px`, width: `${Math.max(0, W - (l + r) * q)}px`, height: `${Math.max(0, H - (b + tp) * q)}px` });
        const pos: Record<string, number[]> = { nw: [0, 0], n: [.5, 0], ne: [1, 0], e: [1, .5], se: [1, 1], s: [.5, 1], sw: [0, 1], w: [0, .5] };
        for (const hd of handles) { const [x, y] = pos[hd.dataset.d!]!; Object.assign(hd.style, { left: `${x * 100}%`, top: `${y * 100}%` }); }
        preview(l, b, r, tp, look);
      }
      f.refresh();
    }
    function preview(l: number, b: number, r: number, tp: number, look: string) {
      const st = s(), cw = nat![0]! - l - r, ch = nat![1]! - b - tp;
      if (cw <= 0 || ch <= 0) return prevBox.replaceChildren(h('span', { class: 'sd-note' }, 'The crop leaves nothing.'));
      const a = (st.angle ?? 0) * Math.PI / 180, rw = Math.abs(cw * Math.cos(a)) + Math.abs(ch * Math.sin(a)), rh = Math.abs(cw * Math.sin(a)) + Math.abs(ch * Math.cos(a));
      const q = Math.min(300 / rw, 180 / rh);
      const inner = picture!.tagName === 'IMG' ? h('img', { src: (picture as HTMLImageElement).src, alt: '' }) : h('div', { class: 'sd-ph' }, st.file);
      Object.assign(inner.style, { width: `${nat![0]! * q}px`, height: `${nat![1]! * q}px`, left: `${-l * q}px`, top: `${-tp * q}px`, filter: look });
      const box = h('div', { class: 'sd-prev-crop', style: `width:${cw * q}px;height:${ch * q}px;transform:rotate(${-(st.angle ?? 0)}deg)` }, inner);
      if (!st.clip && st.trim) box.style.overflow = 'visible';
      const holder = h('div', { style: `width:${rw * q}px;height:${rh * q}px;display:grid;place-items:center` }, box);
      prevBox.replaceChildren(holder, h('div', { class: 'sd-muted', style: 'margin-top:.3rem' }, `${fmt(rw)} × ${fmt(rh)} bp before the size keys`));
    }
    // dragging: the frame moves, a handle moves its edges; the state's trim is in bp
    let drag: { d: string; x: number; y: number; t: number[] } | null = null;
    stage.addEventListener('pointerdown', (e) => {
      if (!nat || !picture) return;
      const d = (e.target as Element).closest<HTMLElement>('.sd-h')?.dataset.d ?? (e.target === crop ? 'move' : null);
      if (!d) return;
      e.preventDefault();
      stage.setPointerCapture(e.pointerId);
      drag = { d, x: e.clientX, y: e.clientY, t: [...(s().trim ?? [0, 0, 0, 0])] };
    });
    stage.addEventListener('pointermove', (e) => {
      if (!drag) return;
      const q = k(), dx = (e.clientX - drag.x) / q, dy = (e.clientY - drag.y) / q, [W, H] = nat as [number, number];
      let [l, b, r, tp] = drag.t;
      const d = drag.d;
      if (d === 'move') { const mx = Math.max(-l, Math.min(r, dx)), my = Math.max(-tp, Math.min(b, dy)); l += mx; r -= mx; tp += my; b -= my; }
      if (d.includes('w')) l = Math.min(Math.max(0, l + dx), W - r - 1);
      if (d.includes('e')) r = Math.min(Math.max(0, r - dx), W - l - 1);
      if (d.includes('n')) tp = Math.min(Math.max(0, tp + dy), H - b - 1);
      if (d.includes('s')) b = Math.min(Math.max(0, b - dy), H - tp - 1);
      s().trim = [l, b, r, tp].map((x) => Math.round(x));
      s().clip = true;
      sync();
    });
    const end = () => { drag = null; };
    stage.addEventListener('pointerup', end);
    stage.addEventListener('pointercancel', end);
    sync();
    return f;
  }
  /** A float's placement: a toggle per letter the run's LaTeX tests, the written order kept. */
  function placement(view: DlgView, host: DlgHost) {
    const ctx = view.context ?? {}, letters = ctx.letters ?? [];
    const main = h('div', { class: 'sd-place' }), side = h('div');
    const f = frame(view, host, 'Edit placement', { main, side, sub: `\\begin{${view.state.env}}`, redraw: () => draw(), foot: 'Turn every letter off to remove the brackets. LaTeX then uses its default.' });
    const spec = () => view.state.spec ?? '';
    const toggle = (c: string) => {
      const now = spec();
      let next = now.includes(c) ? now.replace(c, '') : now + c;
      // H stands alone (float.sty): it never floats
      if (c === 'H' && !now.includes('H')) next = 'H';
      else if (c !== 'H') next = next.replace('H', '');
      view.state.spec = next;
      draw();
      f.refresh();
    };
    // a chip names a command, else a counter (`topnumber` is `\c@topnumber`): its card shows when the run knows it
    const md = (t: string) => esc(t).replace(/`([^`]+)`/g, (_, c) => `<code class="cs-tip" data-cs="${c[0] === '\\' ? c : `\\c@${c}`}">${c}</code>`);
    function draw() {
      const on = spec();
      main.replaceChildren(
        h('div', { class: 'sd-chips', role: 'group', 'aria-label': 'placement letters' }, letters.map(([c, what]: string[]) => h('button', {
          type: 'button', class: 'sd-letter', 'aria-pressed': String(on.includes(c)), onclick: () => toggle(c),
        }, h('b', {}, c), h('span', { html: md(what) })))),
        h('p', { class: 'sd-muted' }, on ? `Writes [${on}]. The order of the letters does not matter. LaTeX always tries here, top, bottom, then a float page.`
          : `No letters set. LaTeX uses ${ctx.default ? `the default [${ctx.default}]` : 'its default'} for this ${view.state.env}.`),
      );
    }
    draw();
    f.refresh();
    return f;
  }

  const api = {
    open(view: DlgView | undefined, host: DlgHost) {
      if (view?.kind === 'table') return table(view, host);
      if (view?.kind === 'image') return image(view, host);
      if (view?.kind === 'float') return placement(view, host);
      return null;
    },
    popup, enhance, store, picker, footer, discard, applier, keys, texText, texIssue, texSafe, check, guard, table: T, image: { contentBox, trimTo, colorModel, DECODE },
    texTokens, history, HINT, properties, keyval: { split: splitKeys, of: keyOf, text: keyText },
  };
  return api;
})();
self.satexDialogs = satexDialogs;
self.satexStore = satexDialogs.store;
