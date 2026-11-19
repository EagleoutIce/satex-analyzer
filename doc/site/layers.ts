// The layers a viewer draws over its text (Typeset) or its pages (PDF): one catalogue, one grouped
// chip renderer and one kept state per viewer, so both name, group and keep a layer alike.
export {};
type Store = { get(k: string): any; set(k: string, v: unknown): void } | undefined;
type On = Record<string, boolean>;
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
    origins: ['origins', 'analysis'],
    bad: ['bad boxes', 'analysis'],
  };
  /** The Typeset view's Show menu: what it renders as TeX does, each [label, title, on by default, ready]. A switch on
   * puts `ts-show-<id>` on the view (style.css draws it); one the run does not report yet (`ready` false) is shown off. */
  const SHOW: Record<string, [string, string, boolean, boolean]> = {
    indent: ['paragraph indent', '\\parindent before a paragraph\'s first line, none after a heading', true, false],
    parskip: ['paragraph spacing', '\\parskip between paragraphs, as the run set it', true, true],
    baselineskip: ['line spacing', '\\baselineskip between lines, 1.2 times the font size as LaTeX\'s sizes set it', true, true],
    colors: ['text colors', '\\color and \\textcolor', true, true],
    fonts: ['font shapes', 'bold, italic, slanted, small caps, sans serif and typewriter', true, true],
    math: ['math layout', 'scripts raised and lowered, stacked', true, true],
    pages: ['page breaks', 'page breaks, headers and footers', true, true],
    footnotes: ['footnotes at page end', 'footnotes at the page\'s end, else where their mark stands', true, false],
    hyphens: ['hyphenation points', 'where TeX may break a word', false, false],
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
    const draw = () => { const n = Object.values(on).filter(Boolean).length; summary.innerHTML = `&#x25A4; <span>${n ? `layers ${n}` : 'layers'}</span>`; };
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
  self.satexLayers = { GROUPS, LAYERS, groups, html, state, wire, count, menu, length, SHOW, showClasses, showDefaults, showHtml };
})();
