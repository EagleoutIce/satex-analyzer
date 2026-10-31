// Draw a symbol, get the commands that set it: the matcher of the playground's Draw dialog and of the
// standalone page draw/index.html. `satexDraw({ root, base, symbols, inForce, pick, label })` wires the
// pad, the filters and the list found under `root`; `symbols` is build.py's symbols.json, under the site's root `base`, `inForce` the
// names in force as [name, package] pairs, `pick(text, package)` what a result's button does; with
// `usepackage`, a second button picks the `\\usepackage{…}` a name not in the kernel needs.
self.satexDraw = ({ root, base, symbols, inForce = async () => [], pick = () => {}, label = 'Insert', usepackage = false }) => {
  const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  const plural = (n, w) => `${n} ${w}${n === 1 ? '' : /(s|ch)$/.test(w) ? 'es' : 's'}`;
  // The candidates (build.py `symbols`) are unicode-math's table and the icon packages' commands, each
  // drawn in the font that sets it. The drawing and each glyph are cut to their ink, scaled into one
  // square with their aspect kept, and reduced to their outline: a filled icon and a drawn outline
  // compare alike. Two outlines are as far apart as the mean distance of each one's points to the
  // other (chamfer distance), plus how much their aspects and their numbers of dots differ. Strokes
  // drawn apart are no dots: a `⇒` drawn in three strokes is one shape, `÷` has two dots.
  const DRAW = { side: 32, glyph: 96, pen: 9, shown: 14, foreign: 1.12, aspect: 0.6, dots: 0.5, dot: 6 };
  const $ = (id) => root.querySelector(`#${id}`), escape = esc;
  const pad = $('draw-pad'), pen = pad.getContext('2d', { willReadFrequently: true });
  let drawTable = null, drawn = false;
  const drawFilter = { from: 'all', kind: '', mode: '' };
  /** The ink of `canvas` as [outline points, distance map, aspect, dots], or null without ink. */
  function features(canvas) {
    const { width: w, height: h } = canvas, data = canvas.getContext('2d', { willReadFrequently: true }).getImageData(0, 0, w, h).data;
    let x0 = w, y0 = h, x1 = -1, y1 = -1;
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) if (data[(y * w + x) * 4 + 3] > 40) { x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y); }
    if (x1 < 0) return null;
    const n = DRAW.side, small = Object.assign(document.createElement('canvas'), { width: n, height: n }), c = small.getContext('2d', { willReadFrequently: true });
    const bw = x1 - x0 + 1, bh = y1 - y0 + 1, k = (n - 4) / Math.max(bw, bh);
    c.drawImage(canvas, x0, y0, bw, bh, (n - bw * k) / 2, (n - bh * k) / 2, bw * k, bh * k);
    const px = c.getImageData(0, 0, n, n).data, ink = new Uint8Array(n * n);
    for (let i = 0; i < n * n; i++) ink[i] = px[i * 4 + 3] > 64 ? 1 : 0;
    const on = (x, y) => x >= 0 && y >= 0 && x < n && y < n && ink[y * n + x];
    const points = [];
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (on(x, y) && !(on(x - 1, y) && on(x + 1, y) && on(x, y - 1) && on(x, y + 1))) points.push(y * n + x);
    return points.length ? [Uint16Array.from(points), distances(points, n), Math.log(bw / bh), dots(ink, n)] : null;
  }
  /** Each cell's distance to the nearest point, by two chamfer passes (3 along, 4 across). */
  function distances(points, n) {
    const d = new Uint16Array(n * n).fill(9999);
    for (const p of points) d[p] = 0;
    const relax = (i, j, cost) => { if (d[j] + cost < d[i]) d[i] = d[j] + cost; };
    for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) {
      const i = y * n + x;
      if (x > 0) relax(i, i - 1, 3);
      if (y > 0) { relax(i, i - n, 3); if (x > 0) relax(i, i - n - 1, 4); if (x < n - 1) relax(i, i - n + 1, 4); }
    }
    for (let y = n - 1; y >= 0; y--) for (let x = n - 1; x >= 0; x--) {
      const i = y * n + x;
      if (x < n - 1) relax(i, i + 1, 3);
      if (y < n - 1) { relax(i, i + n, 3); if (x < n - 1) relax(i, i + n + 1, 4); if (x > 0) relax(i, i + n - 1, 4); }
    }
    return d;
  }
  /** The ink's pieces no wider or taller than `DRAW.dot` cells: an `x` with two dots has two. */
  function dots(ink, n) {
    const seen = new Uint8Array(n * n);
    let count = 0;
    for (let s = 0; s < n * n; s++) {
      if (!ink[s] || seen[s]) continue;
      const stack = [s], box = [n, n, 0, 0];
      seen[s] = 1;
      while (stack.length) {
        const i = stack.pop(), x = i % n, y = (i - x) / n;
        box[0] = Math.min(box[0], x); box[1] = Math.min(box[1], y); box[2] = Math.max(box[2], x); box[3] = Math.max(box[3], y);
        for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
          const u = x + dx, v = y + dy, j = v * n + u;
          if (u >= 0 && v >= 0 && u < n && v < n && ink[j] && !seen[j]) { seen[j] = 1; stack.push(j); }
        }
      }
      if (box[2] - box[0] < DRAW.dot && box[3] - box[1] < DRAW.dot) count++;
    }
    return count;
  }
  /** How far apart two shapes' features are: 0 for the same outline. */
  function apart([pa, da, aa, ka], [pb, db, ab, kb]) {
    let ab_ = 0, ba = 0;
    for (const p of pa) ab_ += db[p];
    for (const p of pb) ba += da[p];
    return (ab_ / pa.length + ba / pb.length) / 3 + DRAW.aspect * Math.abs(aa - ab) + DRAW.dots * Math.min(3, Math.abs(ka - kb));
  }
  /** The symbol table, its fonts loaded only once a candidate drawn in them is wanted. */
  async function symbolsLoad() {
    drawTable ??= fetch(`${base}${symbols}`).then((r) => r.json()).then(({ fonts, rows }) => ({
      fonts: fonts.map((url, i) => ({ family: `satex-sym-${i}`, url, ready: null })),
      rows: rows.map(([code, name, cls, about, font, pkg]) => ({ ch: String.fromCodePoint(code), code, name, cls, about, font, pkg, f: undefined })),
    })).catch((e) => ({ fonts: [], rows: [], error: String(e?.message ?? e) }));
    return drawTable;
  }
  /** A font's glyphs drawn and measured, once: a character the font has no glyph for is left out. */
  function fontReady(table, i) {
    const font = table.fonts[i];
    font.ready ??= (async () => {
      const face = new FontFace(font.family, `url(${base}${font.url})`);
      document.fonts.add(await face.load());
      const g = Object.assign(document.createElement('canvas'), { width: DRAW.glyph, height: DRAW.glyph }), c = g.getContext('2d', { willReadFrequently: true });
      c.font = `${DRAW.glyph * 0.7}px "${font.family}"`;
      c.textAlign = 'center';
      c.textBaseline = 'middle';
      const draw = (ch) => { c.clearRect(0, 0, g.width, g.height); c.fillText(ch, g.width / 2, g.height / 2); return features(g); };
      for (const s of table.rows) if (s.font === i) s.f = draw(s.ch);
    })().catch(() => { for (const s of table.rows) if (s.font === i) s.f = null; });
    return font.ready;
  }
  const drawKind = (s) => (s.cls === 'icon' ? 'symbol' : s.cls === 'alpha' ? 'letter' : 'symbol');
  const drawMode = (s) => (s.cls === 'icon' ? 'text' : 'math');
  async function drawMatch() {
    const list = $('draw-list'), f = features(pad);
    if (!f) { list.innerHTML = ''; return; }
    const [table, names] = await Promise.all([symbolsLoad(), inForce()]);
    // "this document": the names in force in its run, nothing a package it does not load defines
    const source = new Map(names), force = new Set(source.keys());
    const wanted = table.rows.filter((s) => (drawFilter.from === 'all' || force.has(s.name))
      && (!drawFilter.kind || drawKind(s) === drawFilter.kind) && (!drawFilter.mode || drawMode(s) === drawFilter.mode));
    if (!table.rows.length) { list.innerHTML = `<li class="muted">The symbols did not load${table.error ? `: ${escape(table.error)}` : ''}. Reload the page to try again.</li>`; return; }
    await Promise.all([...new Set(wanted.map((s) => s.font))].map((i) => fontReady(table, i)));
    const best = wanted.filter((s) => s.f).map((s) => [s, apart(f, s.f) * (force.has(s.name) ? 1 : DRAW.foreign)]).sort((a, b) => a[1] - b[1]);
    // the package a name needs: where the run has it from when in force, else the table's
    const from = (x) => source.get(x.name) ?? x.pkg;
    const chip = (x) => force.has(x.name) ? `<code class="cs-ref" data-cs="${escape(x.name)}" tabindex="0">${escape(x.name)}</code>`
      : `<code class="draw-off" data-sym="${table.rows.indexOf(x)}" tabindex="0">${escape(x.name)}</code>`;
    const seen = new Set(), rows = [];
    for (const [s] of best) {
      if (rows.length >= DRAW.shown) break;
      const key = s.ch;
      if (seen.has(key)) continue;
      seen.add(key);
      // one row per glyph: its first command, in force first; every other one that sets it folded under "aliases"
      // one glyph however many packages set it: a codepoint is one symbol
      const all = wanted.filter((x) => x.ch === s.ch).sort((a, b) => force.has(b.name) - force.has(a.name));
      const [main, ...more] = all, have = force.has(main.name);
      rows.push(`<li><span class="draw-glyph" style="font-family: '${table.fonts[s.font].family}'">${escape(s.ch)}</span><span class="draw-names">${chip(main)}` +
        (more.length ? `<details class="draw-aliases"><summary>${plural(more.length, 'alias')}</summary><ul>${more.map((x) => `<li>${chip(x)} <span class="lib-pkg${force.has(x.name) ? '' : ' draw-off'}">${escape(from(x))}${force.has(x.name) ? '' : ', not loaded'}</span></li>`).join('')}</ul></details>` : '') +
        `</span><span class="lib-pkg${have ? '' : ' draw-off'}" title="${have ? 'in force in this document' : 'not loaded in this document'}">${escape(from(main))}</span>` +
        `<button type="button" class="small ghost draw-put" data-name="${escape(main.name)}" data-pkg="${escape(from(main))}" title="${escape(label)} ${escape(main.name)}">${escape(label)}</button>` +
        (usepackage && from(main) !== 'kernel'
          ? `<button type="button" class="small ghost draw-put" data-name="\\usepackage{${escape(from(main).replace(/\.cls$/, ''))}}" data-pkg="${escape(from(main))}" title="${escape(label)} the package line">\\usepackage</button>` : '') + '</li>');
    }
    list.innerHTML = rows.join('') || `<li class="muted">${drawFilter.from === 'loaded' ? 'No symbol of this document matches: try every package.' : 'No symbol matches.'}</li>`;
  }
  /** A name not in force: its card from the table, with the package that defines it. */
  $('draw-list').addEventListener('mouseover', async (e) => {
    const el = e.target.closest('[data-sym]');
    if (!el) return;
    const s = (await symbolsLoad()).rows[+el.dataset.sym];
    const does = [s.about, `U+${s.code.toString(16).toUpperCase().padStart(4, '0')}`].filter(Boolean).join(' · ');
    if (self.satexTip && self.satexCards) self.satexTip.card.show(el, self.satexCards.explain({ name: s.name, tag: s.cls === 'icon' ? 'icon' : `math ${s.cls}`, does, package: s.pkg }));
    else el.title = `${s.name}: ${does}, ${s.pkg}`;
  });
  $('draw-list').addEventListener('mouseout', (e) => { if (e.target.closest('[data-sym]')) self.satexTip?.card.hide(150); });
  {
    const clear = () => { pen.clearRect(0, 0, pad.width, pad.height); drawn = false; $('draw-list').innerHTML = ''; };
    let last = null;
    const at = (e) => { const b = pad.getBoundingClientRect(); return [(e.clientX - b.left) * pad.width / b.width, (e.clientY - b.top) * pad.height / b.height]; };
    pad.addEventListener('pointerdown', (e) => { pad.setPointerCapture(e.pointerId); last = at(e); drawn = true; });
    pad.addEventListener('pointermove', (e) => {
      if (!last) return;
      const p = at(e);
      Object.assign(pen, { lineWidth: DRAW.pen, lineCap: 'round', lineJoin: 'round', strokeStyle: getComputedStyle(pad).color });
      pen.beginPath(); pen.moveTo(...last); pen.lineTo(...p); pen.stroke();
      last = p;
    });
    pad.addEventListener('pointerup', () => { last = null; if (drawn) drawMatch(); });
    $('draw-clear').onclick = clear;
    $('draw-filters').addEventListener('click', (e) => {
      const b = e.target.closest('[data-f]');
      if (!b) return;
      drawFilter[b.dataset.f] = b.dataset.v;
      for (const x of $('draw-filters').querySelectorAll(`[data-f="${b.dataset.f}"]`)) x.setAttribute('aria-pressed', String(x === b));
      if (drawn) drawMatch();
    });
    $('draw-list').addEventListener('click', (e) => {
      const b = e.target.closest('.draw-put');
      if (!b) return;
      pick(b.dataset.name, b.dataset.pkg);
    });
  }
  return { load: symbolsLoad, match: drawMatch, filter: drawFilter };
};
