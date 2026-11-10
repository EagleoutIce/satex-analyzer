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
    grid: ['grid', 'layout'],
    findings: ['findings', 'analysis'],
    maybe: ['maybe', 'analysis'],
    bad: ['bad boxes', 'analysis'],
  };
  /** `ids` by group, in catalogue order: [[group, label, ids]], empty groups left out. */
  const groups = (ids: string[]) => GROUPS.map(([g, label]) => [g, label, Object.keys(LAYERS).filter((k) => ids.includes(k) && LAYERS[k][1] === g)]).filter((x) => x[2].length) as [string, string, string[]][];
  /** The chips of `about` (id → title) as HTML: a labelled segmented group each. Each switch is
   * `prefix-id`; a layer in `facets` gets a fold of filters beside it. */
  function html(about: Record<string, string>, on: On, { prefix, facets = new Set<string>() }: { prefix: string; facets?: Set<string> }) {
    const chip = (k: string) => `<label class="layer" title="${esc(about[k])}"><input type="checkbox" role="switch" id="${prefix}-${k}" data-layer="${k}"${on[k] ? ' checked' : ''}><i class="layer-swatch sw-${k}" aria-hidden="true"></i><span>${esc(LAYERS[k][0])}</span><b class="count layer-n"></b></label>` +
      (facets.has(k) ? `<button type="button" class="layer-sub-toggle" data-fold="${k}" disabled aria-expanded="false" title="Filter ${esc(LAYERS[k][0])}" aria-label="filter ${esc(LAYERS[k][0])}"></button>` : '');
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
  /** Points per unit, as TeX counts them (tex.web § 458); `px` is pdfTeX's default \pdfpxdimen, 1bp. */
  const PT: Record<string, number> = { pt: 1, pc: 12, in: 72.27, bp: 72.27 / 72, px: 72.27 / 72, cm: 72.27 / 2.54, mm: 72.27 / 25.4, dd: 1238 / 1157, cc: 14856 / 1157, sp: 1 / 65536 };
  /** A TeX length (`1cm`, `.5in`, `12 pt`, `2em` given `font` {em, ex} in pt) as {pt, num, unit}; null when it is none or not above 0. */
  function length(text: unknown, font: Record<string, number> = {}) {
    const m = /^\s*(\d*\.?\d+)\s*(pt|pc|in|bp|px|cm|mm|dd|cc|sp|em|ex)\s*$/i.exec(String(text ?? ''));
    const unit = m?.[2].toLowerCase(), per = unit && (PT[unit] ?? font[unit]);
    return m && unit && per && +m[1] > 0 ? { pt: +m[1] * per, num: +m[1], unit } : null;
  }
  self.satexLayers = { GROUPS, LAYERS, groups, html, state, wire, count, length };
})();
