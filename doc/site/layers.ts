// The layers a viewer draws over its text (Typeset) or its pages (PDF): one catalogue, one grouped
// chip renderer and one kept state per viewer, so both name, group and keep a layer alike.
export {};
type Store = { get(k: string): any; set(k: string, v: unknown): void } | undefined;
type On = Record<string, boolean>;
type Box = { page: number; file: string; line: number; kind: string; x: number; y: number; w: number; h: number; d: number };
type Rect = { l: number; t: number; r: number; b: number };
declare global {
  interface Window {
    satexLayers: {
      GROUPS: string[][]; LAYERS: Record<string, string[]>; groups(ids: string[]): [string, string, string[]][];
      html(about: Record<string, string>, on: On, o: { prefix: string; facets?: Set<string> }): string;
      state(viewer: string, defaults: On, store: Store, legacy?: Partial<On>): On;
      wire(root: HTMLElement, viewer: string, on: On, store: Store, apply: (id: string) => void): void;
      count(root: ParentNode, id: string, n: number | boolean | undefined): void;
      menu(details: HTMLElement, list: HTMLElement, on: On, onFold?: () => void): () => void;
      length(text: unknown, font?: Record<string, number>): { pt: number; num: number; unit: string } | null;
      indentOf(r: { indented?: boolean; indent?: number; parindent?: number }): number | null;
      glueShort(spec: string): string;
      runsBy<T>(items: T[], key: (x: T) => string | null | undefined): [string, number, number][];
      mathSpans(boxes: Box[], displays?: [string, number, number][]): { page: number; l: number; t: number; r: number; d: number; file: string; line: number }[];
      fitLabels(labels: Rect[], fixed: Rect[], ink: Rect[]): (number | null)[];
      lineMap(from: string, to: string): ((n: number) => number) & { changed(n: number): boolean };
      SHOW: Record<string, [string, string, boolean, boolean]>;
      showClasses(on: On): string[]; showDefaults(): On; showHtml(on: On): string;
    };
  }
}
(() => {
  const esc = (s: unknown) => String(s).replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
  /** The groups, in the order they show. */
  const GROUPS: string[][] = [['structure', 'Structure'], ['layout', 'Layout'], ['analysis', 'Analysis']];
  /** Every layer: its name and group. A viewer shows the ones it draws, each with its own title. */
  const LAYERS: Record<string, string[]> = {
    envs: ['envs', 'structure'],
    keys: ['keys', 'structure'],
    boxes: ['boxes', 'layout'],
    glue: ['glue', 'layout'],
    ligs: ['ligs', 'layout'],
    geometry: ['text block', 'layout'],
    figures: ['bounding boxes', 'layout'],
    widths: ['column widths', 'layout'],
    grid: ['grid', 'layout'],
    findings: ['findings', 'analysis'],
    maybe: ['maybe', 'analysis'],
    math: ['math', 'structure'],
    macros: ['macros', 'analysis'],
    origins: ['origins', 'analysis'],
    bad: ['bad boxes', 'analysis'],
  };
  /** The Typeset view's Show menu: what it renders as TeX does, each [label, title, on by default, ready]. A switch on
   * puts `ts-show-<id>` on the view (style.css draws it); one the run does not report yet (`ready` false) is shown off. */
  const SHOW: Record<string, [string, string, boolean, boolean]> = {
    indent: ['paragraph indent', '\\parindent before a paragraph\'s first line, none after a heading', true, true],
    parskip: ['paragraph spacing', '\\parskip between paragraphs, as the run set it', true, true],
    baselineskip: ['line spacing', '\\baselineskip between lines, 1.2 times the font size as LaTeX\'s sizes set it', true, true],
    colors: ['text colors', '\\color and \\textcolor', true, true],
    fonts: ['font shapes', 'bold, italic, slanted, small caps, sans serif and typewriter', true, true],
    math: ['math layout', 'scripts raised and lowered, stacked', true, true],
    pages: ['page breaks', 'page breaks, headers and footers', true, true],
    footnotes: ['footnotes at page end', 'footnotes at the page\'s end, else where their mark stands', true, false],
    hyphens: ['hyphenation points', 'where TeX may break a word', false, false],
    linebreaks: ['TeX\'s lines', 'each paragraph broken into lines where the run\'s own line breaker (TeX\'s, with the fonts\' widths, kerns, ligatures and hyphenation) broke it, where all of it is known', false, true],
    justify: ['justified', 'lines set flush on both sides, as TeX justifies them (off: ragged right)', false, true],
    numbers: ['section numbers', 'the numbers of headings', true, false],
    refs: ['printed references', 'references and citations as printed (the keys layer shows their keys)', true, false],
    index: ['index marks', '\\index and glossary entries where they stand', false, false],
  };
  /** The Typeset view's classes for Show switches `on`: `ts-show-<id>` for each ready one on. */
  const showClasses = (on: On) => Object.keys(SHOW).filter((k) => SHOW[k]![3] && on[k]).map((k) => `ts-show-${k}`);
  /** The Show switches' defaults. */
  const showDefaults = (): On => Object.fromEntries(Object.entries(SHOW).map(([k, v]) => [k, v[2] && v[3]]));
  /** The Show menu's switches as HTML: one per item, one not ready disabled. */
  const showHtml = (on: On) => Object.entries(SHOW).map(([k, [label, title, , ready]]) => `<label class="layer${ready ? '' : ' layer-off'}" title="${esc(title)}${ready ? '' : ' (the run does not report it yet)'}"><input type="checkbox" role="switch" id="typeset-show-${k}" data-layer="${k}"${on[k] && ready ? ' checked' : ''}${ready ? '' : ' disabled'}><span>${esc(label)}</span></label>`).join('');
  /** A typeset row's first-line indent in pt, from the run's facts: the width it set before the first character
   * (`indent`, else `\parindent` when `indented`), 0 when it set none (`\noindent`, after a heading), null when not known. */
  const indentOf = (r: { indented?: boolean; indent?: number; parindent?: number }) =>
    r.indented === false ? 0 : r.indented === true ? (typeof r.indent === 'number' ? r.indent : typeof r.parindent === 'number' ? r.parindent : null) : null;
  /** `ids` by group, in catalogue order: [[group, label, ids]], empty groups left out. */
  const groups = (ids: string[]) => GROUPS.map(([g, label]) => [g, label, Object.keys(LAYERS).filter((k) => ids.includes(k) && LAYERS[k][1] === g)]).filter((x) => x[2].length) as [string, string, string[]][];
  /** The chips of `about` (id → title) as HTML: a labelled segmented group each. Each switch is
   * `prefix-id`; a layer in `facets` gets a fold of filters beside it. */
  function html(about: Record<string, string>, on: On, { prefix, facets = new Set<string>() }: { prefix: string; facets?: Set<string> }) {
    const label = (k: string) => `<label class="layer" title="${esc(about[k])}"><input type="checkbox" role="switch" id="${prefix}-${k}" data-layer="${k}"${on[k] ? ' checked' : ''}><i class="layer-swatch sw-${k}" aria-hidden="true"></i><span>${esc(LAYERS[k][0])}</span><b class="count layer-n"></b></label>`;
    // a layer with filters is one split button: the switch, then its fold's arrow
    const chip = (k: string) => facets.has(k) ? `<span class="layer-split">${label(k)}<button type="button" class="layer-sub-toggle" data-fold="${k}" aria-expanded="false" title="Filter ${esc(LAYERS[k][0])}" aria-label="filter ${esc(LAYERS[k][0])}"></button></span>` : label(k);
    return groups(Object.keys(about)).map(([g, label, ids]) => `<div class="layer-group" role="group" aria-label="${label}" data-group="${g}"><span class="layer-label">${label}</span><span class="layer-seg">${ids.map(chip).join('')}</span>` +
      ids.filter((k) => facets.has(k)).map((k) => `<div class="layer-sub" data-sub="${k}" hidden><div class="layer-sub-list"></div></div>`).join('') + '</div>').join('');
  }
  /** A viewer's switches: `defaults`, then what an older version kept (`legacy`), then what the store keeps as `layers-viewer`. */
  function state(viewer: string, defaults: On, store: Store, legacy: Partial<On> = {}): On {
    const kept = store?.get(`layers-${viewer}`) ?? {};
    return Object.fromEntries(Object.keys(defaults).map((k) => [k, !!(kept[k] ?? legacy[k] ?? defaults[k])]));
  }
  /** Wires `root`'s switches: a change is kept in `on` and the store, then `apply(id)` runs. */
  function wire(root: HTMLElement, viewer: string, on: On, store: Store, apply: (id: string) => void) {
    root.addEventListener('change', (e) => {
      const t = e.target as HTMLInputElement, k = t.dataset?.layer;
      if (!k || !(k in on)) return;
      on[k] = t.checked;
      store?.set(`layers-${viewer}`, { ...on });
      apply(k);
    });
  }
  /** Shows `n` beside layer `id`'s name, nothing while it is off or none. */
  const count = (root: ParentNode, id: string, n: number | boolean | undefined) => { const b = root.querySelector(`[data-layer="${id}"]`)?.closest('.layer')?.querySelector('.layer-n'); if (b) b.textContent = n ? String(n) : ''; };
  /** A viewer's layers menu (`details` holding `list`): double-click on its button (or a group's title) switches
   * every layer (of the group) off, the next one back as it was (all on when none was); a layer's filters fold
   * open under its group, then `onFold` fills them. Returns what redraws the button's count. */
  function menu(details: HTMLElement, list: HTMLElement, on: On, onFold?: () => void) {
    const summary = details.querySelector('summary') as HTMLElement;
    let were: { scope: Element; on: string[] } | null = null;
    const all = (scope: Element) => {
      const boxes = [...scope.querySelectorAll('input[data-layer]')] as HTMLInputElement[], lit = boxes.filter((b) => b.checked);
      const was = were?.scope === scope && !lit.length ? were.on : null;
      if (lit.length) were = { scope, on: lit.map((b) => b.dataset.layer!) };
      for (const b of boxes) { const want = was ? was.includes(b.dataset.layer!) : !lit.length; if (b.checked !== want) { b.checked = want; b.dispatchEvent(new Event('change', { bubbles: true })); } }
    };
    summary.title += ' · double-click: all layers off, again: back on';
    list.querySelectorAll('.layer-label').forEach((l) => { (l as HTMLElement).title = 'Double-click: this group off, again: back on'; });
    summary.addEventListener('dblclick', (e) => { e.preventDefault(); all(list); });
    list.addEventListener('dblclick', (e) => { const t = e.target as Element, g = t.closest('.layer-group'); if (!g || t.closest('input, label, button')) return; e.preventDefault(); all(g); });
    list.addEventListener('click', (e) => {
      const t = (e.target as Element).closest('.layer-sub-toggle') as HTMLElement | null;
      if (!t) return;
      e.preventDefault();
      const sub = list.querySelector(`.layer-sub[data-sub="${t.dataset.fold}"]`) as HTMLElement, open = sub.hidden;
      sub.hidden = !open;
      t.setAttribute('aria-expanded', String(open));
      if (open) onFold?.();
    });
    const draw = () => { const n = Object.values(on).filter(Boolean).length; summary.innerHTML = `&#x25A4; <span>View</span>${n ? ` <b class="count" title="${n} layers on">${n}</b>` : ''}`; };
    list.addEventListener('change', draw);
    draw();
    return draw;
  }
  /** Points per unit, as TeX counts them (tex.web § 458); `px` is pdfTeX's default \pdfpxdimen, 1bp. */
  const PT: Record<string, number> = { pt: 1, pc: 12, in: 72.27, bp: 72.27 / 72, px: 72.27 / 72, cm: 72.27 / 2.54, mm: 72.27 / 25.4, dd: 1238 / 1157, cc: 14856 / 1157, sp: 1 / 65536 };
  /** A TeX length (`1cm`, `.5in`, `12 pt`, `2em` given `font` {em, ex} in pt) as {pt, num, unit}; null when it is none or not above 0. */
  function length(text: unknown, font: Record<string, number> = {}) {
    const m = /^\s*(\d*\.?\d+)\s*(pt|pc|in|bp|px|cm|mm|dd|cc|sp|em|ex)\s*$/i.exec(String(text ?? ''));
    const unit = m?.[2].toLowerCase(), per = unit && (PT[unit] ?? font[unit]);
    return m && unit && per && +m[1] > 0 ? { pt: +m[1] * per, num: +m[1], unit } : null;
  }
  /** A number as a label shows it: at most two decimals, no trailing zeros. */
  const num = (x: string) => String(+(+x).toFixed(2));
  /** A glue spec as its label: `3.33333pt plus 1.66667pt minus 1.11111pt` as `3.33+1.67−1.11pt`, the unit said once when
   * all parts share it; any other text (`end · 253.6pt`) keeps its words, its numbers trimmed. The full spec stays on hover. */
  function glueShort(spec: string) {
    const m = /^\s*(-?[\d.]+)(\w+)(?:\s+plus\s+(-?[\d.]+)(\w+))?(?:\s+minus\s+(-?[\d.]+)(\w+))?\s*$/.exec(spec);
    if (!m) return spec.replace(/-?\d+\.\d+/g, num);
    const parts = [[m[1], m[2], ''], [m[3], m[4], '+'], [m[5], m[6], '\u2212']].filter((p) => p[0] != null) as string[][];
    const one = parts.every((p) => p[1] === parts.at(-1)![1]);
    return parts.map(([n, u, sign], i) => sign + num(n) + (one && i < parts.length - 1 ? '' : u)).join('');
  }
  const meets = (a: Rect, b: Rect) => a.l < b.r && a.r > b.l && a.t < b.b && a.b > b.t;
  const area = (a: Rect, b: Rect) => Math.max(0, Math.min(a.r, b.r) - Math.max(a.l, b.l)) * Math.max(0, Math.min(a.b, b.b) - Math.max(a.t, b.t));
  /** Labels placed in order (the first wins its spot): each its vertical shift, null when elided. A label never overlaps a
   * `fixed` one or one placed before; of its spots (in place, a height below, above, two below) it takes the one covering
   * the least `ink`, the first on a tie. One with no free spot is elided: its value stays on hover. */
  function fitLabels(labels: Rect[], fixed: Rect[], ink: Rect[]) {
    const placed = [...fixed];
    return labels.map((a) => {
      const h = a.b - a.t;
      let best: number | null = null, cost = Infinity;
      for (const dy of [0, h, -h, 2 * h]) {
        const q = { l: a.l, r: a.r, t: a.t + dy, b: a.b + dy };
        if (placed.some((p) => meets(p, q))) continue;
        const c = ink.reduce((s, w) => s + area(w, q), 0);
        if (c < cost) { cost = c; best = dy; }
        if (!c) break;
      }
      if (best != null) placed.push({ l: a.l, r: a.r, t: a.t + best, b: a.b + best });
      return best;
    });
  }
  /** Inline math as SyncTeX records it: a `$` record where math begins and one where it ends, in the order TeX shipped
   * them. One span per pair on a line, from the first to the second; math broken over lines runs to the first line's end
   * and from the next line's start. Its height is the line box's, a 7bp ascent and 2bp descent where none is found. */
  function mathSpans(boxes: Box[], displays: [string, number, number][] = []) {
    // a mark's line is on its page: looked for among that page's lines (all lines per mark is quadratic in the document)
    const lines = Map.groupBy(boxes.filter((b) => b.kind === '(' && b.w > 0), (b) => b.page), out: ReturnType<Window['satexLayers']['mathSpans']> = [];
    const lineOf = (b: Box) => lines.get(b.page)?.find((o) => Math.abs(o.y - b.y) < 0.5 && o.x - 0.5 <= b.x && b.x <= o.x + o.w + 0.5);
    const put = (b: Box, l: number, r: number) => { const o = lineOf(b); if (r > l) out.push({ page: b.page, l, r, t: b.y - (o?.h || 7), d: b.y + (o?.d || 2), file: b.file, line: b.line }); };
    const marks = boxes.flatMap((b, at) => b.kind === '$' ? [{ b, at }] : []);
    for (let i = 0; i + 1 < marks.length; i += 2) {
      const [{ b: a, at: from }, { b, at: to }] = [marks[i]!, marks[i + 1]!];
      if (a.page !== b.page) continue;
      // a box filling the math is text set in math mode, not a formula: `tabular` is built in `$…$` (`\@tabular`),
      // so `\maketitle`'s author block is no math
      if (boxes.slice(from + 1, to).some((o) => o.kind === '(' && o.w >= 0.9 * Math.abs(b.x - a.x) && o.w > 0)) continue;
      if (Math.abs(a.y - b.y) < 0.5) { put(a, a.x, b.x); continue; }
      const la = lineOf(a), lb = lineOf(b);
      put(a, a.x, la ? la.x + la.w : a.x);
      put(b, lb ? lb.x : b.x, b.x);
    }
    // a displayed formula leaves no `$` record: its boxes from its source lines (`displays`: file, first and last line),
    // per page, without the widest where others stand in it (the display's line, out to its equation number)
    for (const [file, from, to] of displays ?? []) {
      const own = boxes.filter((b) => b.kind === '(' && b.w > 0 && b.line >= from && b.line <= to && (b.file === file || b.file.endsWith(`/${file}`) || file.endsWith(`/${b.file}`)));
      for (const [page, bs] of Map.groupBy(own, (b) => b.page)) {
        const wide = bs.length > 1 ? bs.reduce((m, b) => b.w > m.w ? b : m) : null, kept = bs.filter((b) => b !== wide);
        out.push({ page, l: Math.min(...kept.map((b) => b.x)), r: Math.max(...kept.map((b) => b.x + b.w)), t: Math.min(...kept.map((b) => b.y - b.h)), d: Math.max(...kept.map((b) => b.y + b.d)), file, line: from });
      }
    }
    return out;
  }
  /** Runs of `items` with one key: [key, first, last] for each stretch whose `key` is the same and set. */
  function runsBy<T>(items: T[], key: (x: T) => string | null | undefined) {
    const out: [string, number, number][] = [];
    items.forEach((x, i) => { const k = key(x), o = out.at(-1); if (!k) return; if (o && o[0] === k && o[2] === i - 1) o[2] = i; else out.push([k, i, i]); });
    return out;
  }
  /** Where the lines of the text `from` stand in the text `to` (1-based): the same line while the two agree from the
   * top, the same counted from the end while they agree from the bottom, and between (the edit) the nearest line of
   * the edit in `to`; `changed(n)`: line `n` is of the edit. A layer places a run's facts on a build's pages with
   * it: the run's text and the built one differ while a build lands under the keys, or a run before its build. */
  function lineMap(from: string, to: string) {
    const a = from === to ? [] : from.split('\n'), b = from === to ? [] : to.split('\n');
    let top = 0, end = 0;
    while (top < a.length && top < b.length && a[top] === b[top]) top++;
    while (end < a.length - top && end < b.length - top && a[a.length - 1 - end] === b[b.length - 1 - end]) end++;
    const changed = (n: number) => from !== to && n > top && n <= a.length - end;
    return Object.assign((n: number) => from === to || n <= top ? n : n > a.length - end ? n + b.length - a.length : Math.min(n, Math.max(top + 1, b.length - end)), { changed });
  }
  self.satexLayers = { GROUPS, LAYERS, groups, html, state, wire, count, menu, length, SHOW, showClasses, showDefaults, showHtml, indentOf, glueShort, fitLabels, mathSpans, runsBy, lineMap };
})();
