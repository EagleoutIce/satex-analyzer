// The layers a viewer draws over its text (Typeset) or its pages (PDF): one catalogue, one grouped
// chip renderer and one kept state per viewer, so both name, group and keep a layer alike.
(() => {
  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
  /** The groups, in the order they show. */
  const GROUPS = [['structure', 'Structure'], ['layout', 'Layout'], ['analysis', 'Analysis']];
  /** Every layer: its name and group. A viewer shows the ones it draws, each with its own title. */
  const LAYERS = {
    envs: ['envs', 'structure'],
    keys: ['keys', 'structure'],
    boxes: ['boxes', 'layout'],
    glue: ['glue', 'layout'],
    ligs: ['ligs', 'layout'],
    geometry: ['text block', 'layout'],
    figures: ['bounding boxes', 'layout'],
    findings: ['findings', 'analysis'],
    maybe: ['maybe', 'analysis'],
    bad: ['bad boxes', 'analysis'],
  };
  /** `ids` by group, in catalogue order: [[group, label, ids]], empty groups left out. */
  const groups = (ids) => GROUPS.map(([g, label]) => [g, label, Object.keys(LAYERS).filter((k) => ids.includes(k) && LAYERS[k][1] === g)]).filter((x) => x[2].length);
  /** The chips of `about` (id → title) as HTML: a labelled segmented group each. Each switch is
   * `prefix-id`; a layer in `facets` gets a fold of filters beside it. */
  function html(about, on, { prefix, facets = new Set() }) {
    const chip = (k) => `<label class="layer" title="${esc(about[k])}"><input type="checkbox" role="switch" id="${prefix}-${k}" data-layer="${k}"${on[k] ? ' checked' : ''}><i class="layer-swatch sw-${k}" aria-hidden="true"></i><span>${esc(LAYERS[k][0])}</span><b class="count layer-n"></b></label>` +
      (facets.has(k) ? `<button type="button" class="layer-sub-toggle" data-fold="${k}" disabled aria-expanded="false" title="Filter ${esc(LAYERS[k][0])}" aria-label="filter ${esc(LAYERS[k][0])}"></button>` : '');
    return groups(Object.keys(about)).map(([g, label, ids]) => `<div class="layer-group" role="group" aria-label="${label}" data-group="${g}"><span class="layer-label">${label}</span><span class="layer-seg">${ids.map(chip).join('')}</span>` +
      ids.filter((k) => facets.has(k)).map((k) => `<div class="layer-sub" data-sub="${k}" hidden><div class="layer-sub-list"></div></div>`).join('') + '</div>').join('');
  }
  /** A viewer's switches: `defaults`, then what an older version kept (`legacy`), then what the store keeps as `layers-viewer`. */
  function state(viewer, defaults, store, legacy = {}) {
    const kept = store?.get(`layers-${viewer}`) ?? {};
    return Object.fromEntries(Object.keys(defaults).map((k) => [k, !!(kept[k] ?? legacy[k] ?? defaults[k])]));
  }
  /** Wires `root`'s switches: a change is kept in `on` and the store, then `apply(id)` runs. */
  function wire(root, viewer, on, store, apply) {
    root.addEventListener('change', (e) => {
      const k = e.target.dataset?.layer;
      if (!k || !(k in on)) return;
      on[k] = e.target.checked;
      store?.set(`layers-${viewer}`, { ...on });
      apply(k);
    });
  }
  /** Shows `n` beside layer `id`'s name, nothing while it is off or none. */
  const count = (root, id, n) => { const b = root.querySelector(`[data-layer="${id}"]`)?.closest('.layer')?.querySelector('.layer-n'); if (b) b.textContent = n ? String(n) : ''; };
  self.satexLayers = { GROUPS, LAYERS, groups, html, state, wire, count };
})();
