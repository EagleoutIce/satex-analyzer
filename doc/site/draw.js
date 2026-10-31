// Draw a symbol, get the commands that set it: the matcher of the playground's Draw dialog and of the
// standalone page draw/index.html. `satexDraw({ root, base, symbols, inForce, pick, label })` wires the
// pad, the filters and the list found under `root`; `symbols` is build.py's symbols.json, under the site's root `base`, `inForce` the
// names in force as [name, package] pairs, `pick(name, package, use)` what a result's button does; with
// `usepackage`, a second button also hands it the `\\usepackage{…}` line (`use`) a name not loaded needs.
// Without a `document` (the page alone) nothing is "not loaded".
self.satexDraw = ({ root, base, symbols, inForce = async () => [], pick = () => {}, label = 'Insert', usepackage = false, document: doc = true }) => {
  const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  const plural = (n, w) => `${n} ${w}${n === 1 ? '' : /(s|ch)$/.test(w) ? 'es' : 's'}`;
  // The candidates (build.py `symbols`) are unicode-math's table and the icon packages' commands, each
  // drawn in the font that sets it. The drawing and each glyph are cut to their ink, scaled into one
  // square with their aspect kept, and reduced to their outline: a filled icon and a drawn outline
  // compare alike. Two outlines are as far apart as the mean distance of each one's points to the
  // other (chamfer distance), plus how much their aspects and their numbers of dots differ. Strokes
  // drawn apart are no dots: a `⇒` drawn in three strokes is one shape, `÷` has two dots.
  const DRAW = { side: 32, glyph: 96, pen: 9, shown: 14, foreign: 1.12, aspect: 0.6, dots: 0.8, dot: 4, batch: 300, again: 120, more: 30 };
  const $ = (id) => root.querySelector(`#${id}`), escape = esc;
  const pad = $('draw-pad'), pen = pad.getContext('2d', { willReadFrequently: true });
  let drawTable = null, drawn = false;
  const drawFilter = { from: 'all', kind: '', mode: '' };
  // the packages left out, as this viewer last chose them
  let skipped = new Set();
  try { skipped = new Set(JSON.parse(localStorage.getItem('satex-draw-skip') ?? '[]')); } catch {}
  const keepSkipped = () => { try { localStorage.setItem('satex-draw-skip', JSON.stringify([...skipped])); } catch {} };
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
    // dots by their size on the whole canvas, not after the cut: a glyph of two dots (`¨`) keeps them
    const whole = Object.assign(document.createElement('canvas'), { width: n, height: n }), wc = whole.getContext('2d', { willReadFrequently: true });
    wc.drawImage(canvas, 0, 0, w, h, 0, 0, n, n);
    const wp = wc.getImageData(0, 0, n, n).data, wink = new Uint8Array(n * n);
    for (let i = 0; i < n * n; i++) wink[i] = wp[i * 4 + 3] > 40 ? 1 : 0;
    return points.length ? [Uint16Array.from(points), distances(points, n), Math.log(bw / bh), dots(wink, n)] : null;
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
  /** The ink's pieces no wider or taller than `DRAW.dot` cells of the whole canvas: an `x` with two dots has two. */
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
  /** The symbol table; each font's glyphs are measured once a candidate in it is wanted. */
  async function symbolsLoad() {
    drawTable ??= fetch(`${base}${symbols}`).then((r) => r.json()).then(({ fonts, rows }) => ({
      fonts: fonts.map((url, i) => ({ family: `satex-sym-${i}`, url, ready: null })),
      rows: rows.map(([code, name, cls, about, font, pkg]) => ({
        ch: String.fromCodePoint(code), code, name, cls, about, font, pkg, f: undefined,
        text: `${name.slice(1)} ${about ?? ''}`.toLowerCase().replace(/[-_]/g, ' '),
      })),
    })).then((t) => { totals(t); return t; }).catch((e) => ({ fonts: [], rows: [], error: String(e?.message ?? e) }));
    return drawTable;
  }
  const idle = () => new Promise((done) => (self.requestIdleCallback ? requestIdleCallback(() => done(), { timeout: 30 }) : setTimeout(done, 0)));
  /** A font's glyphs drawn and measured, a few hundred at a time between frames, `more()` after each batch;
   * a character the font has no glyph for is left out. */
  /** A font's face, loaded once: rows drawn in it show their glyphs as soon as it is in. */
  function faceReady(table, i) {
    const font = table.fonts[i];
    font.face ??= new FontFace(font.family, `url(${base}${font.url})`).load().then((face) => { document.fonts.add(face); return face; });
    return font.face;
  }
  function fontReady(table, i, more) {
    const font = table.fonts[i];
    font.ready ??= (async () => {
      await faceReady(table, i);
      const g = Object.assign(document.createElement('canvas'), { width: DRAW.glyph, height: DRAW.glyph }), c = g.getContext('2d', { willReadFrequently: true });
      c.font = `${DRAW.glyph * 0.7}px "${font.family}"`;
      c.textAlign = 'center';
      c.textBaseline = 'middle';
      const draw = (ch) => { c.clearRect(0, 0, g.width, g.height); c.fillText(ch, g.width / 2, g.height / 2); return features(g); };
      const mine = table.rows.filter((s) => s.font === i);
      for (let k = 0; k < mine.length; k += DRAW.batch) {
        for (const s of mine.slice(k, k + DRAW.batch)) s.f = draw(s.ch);
        font.more?.();
        await idle();
      }
    })().catch(() => { for (const s of table.rows) if (s.font === i) s.f = null; });
    font.more = more;
    return font.ready;
  }
  const drawKind = (s) => (s.cls === 'icon' ? 'symbol' : s.cls === 'alpha' ? 'letter' : 'symbol');
  const drawMode = (s) => (s.cls === 'icon' ? 'text' : 'math');
  /** How well the search `q` names `s`: every word found in its command, glyph name or description; a
   * command starting with it first, then a word starting with it, then anywhere; null when not. */
  const named = (s, words) => {
    let rank = 0;
    for (const w of words) {
      const at = s.text.indexOf(w);
      if (at < 0) return null;
      rank += at === 0 ? 0 : s.text[at - 1] === ' ' ? 1 : 2;
    }
    return rank;
  };
  /** The packages' list: each with its count of symbols, a box to leave it out; all or none at once. */
  function packagesShow(table) {
    const counts = new Map();
    for (const s of table.rows) counts.set(s.pkg, (counts.get(s.pkg) ?? 0) + 1);
    const list = [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
    $('draw-pkgs').textContent = `packages ${list.length - [...skipped].filter((p) => counts.has(p)).length}/${list.length}`;
    $('draw-pkg-list').innerHTML = `<div class="draw-pkg-all"><button type="button" class="ghost small" data-all="1">all</button><button type="button" class="ghost small" data-all="0">none</button></div>` +
      list.map(([p, n]) => `<label><input type="checkbox" data-pkg="${escape(p)}"${skipped.has(p) ? '' : ' checked'}> ${escape(p)} <b>${n.toLocaleString('en')}</b></label>`).join('');
  }
  /** The table's size, and how many symbols the filters leave. */
  function totals(table, shown = null) {
    packagesShow(table);
    const pkgs = new Set(table.rows.map((s) => s.pkg)).size;
    $('draw-total').textContent = `${table.rows.length.toLocaleString('en')} symbols from ${pkgs} packages${shown == null ? '' : ` · ${shown.toLocaleString('en')} match`}`;
  }
  let matching = 0, started = 0, redraw = 0, limit = DRAW.shown;
  /** The candidates for the drawing and the search, best first: shown from the glyphs measured so far,
   * ranked again as more fonts are measured. */
  async function drawMatch(fresh = true) {
    const mine = fresh ? ++matching : matching, list = $('draw-list'), f = features(pad);
    const words = $('draw-search').value.trim().toLowerCase().replace(/^\\/, '').split(/[\s\-_]+/).filter(Boolean);
    if (fresh) { started = performance.now(); limit = DRAW.more; }
    if (!f && !words.length) { list.innerHTML = ''; return; }
    const [table, names] = await Promise.all([symbolsLoad(), inForce()]);
    if (mine !== matching) return;
    if (!table.rows.length) { list.innerHTML = `<li class="muted">The symbols did not load${table.error ? `: ${escape(table.error)}` : ''}. Reload the page to try again.</li>`; return; }
    // "this document": the names in force in its run, nothing a package it does not load defines
    const source = new Map(names), force = new Set(source.keys());
    const ranked = new Map();
    const wanted = table.rows.filter((s) => {
      if (drawFilter.from !== 'all' && !force.has(s.name)) return false;
      if (skipped.has(s.pkg)) return false;
      if ((drawFilter.kind && drawKind(s) !== drawFilter.kind) || (drawFilter.mode && drawMode(s) !== drawFilter.mode)) return false;
      const r = words.length ? named(s, words) : 0;
      if (r == null) return false;
      ranked.set(s, r);
      return true;
    });
    totals(table, wanted.length);
    // the fonts measured in the background: each batch ranks again, at most every DRAW.again ms
    const again = () => { if (mine === matching && !redraw) redraw = setTimeout(() => { redraw = 0; drawMatch(false); }, DRAW.again); };
    if (f) for (const i of new Set(wanted.map((s) => s.font))) fontReady(table, i, again);
    const score = (s) => (f ? apart(f, s.f) * (force.has(s.name) ? 1 : DRAW.foreign) + ranked.get(s) * 0.05 : ranked.get(s) + s.name.length / 100);
    const best = wanted.filter((s) => !f || s.f).map((s) => [s, score(s)]).sort((a, b) => a[1] - b[1]);
    const from = (x) => source.get(x.name) ?? x.pkg;
    const off = (x) => (doc && !force.has(x.name) ? ' draw-off' : '');
    const chip = (x) => force.has(x.name) ? `<code class="cs-ref" data-cs="${escape(x.name)}" tabindex="0">${escape(x.name)}</code>`
      : `<code class="draw-name${off(x)}" data-sym="${table.rows.indexOf(x)}" tabindex="0">${escape(x.name)}</code>`;
    const loaded = (pkg) => [...source.values()].includes(pkg) || pkg === 'kernel';
    // a name's packages: the first three, then how many more
    const pkgs = (list) => { const p = [...new Set(list.map(from))]; return escape(p.slice(0, 3).join(', ')) + (p.length > 3 ? ` +${p.length - 3}` : ''); };
    const seen = new Set(), rows = [];
    let glyphs = 0;
    for (const [s] of best) {
      if (seen.has(s.ch)) continue;
      seen.add(s.ch);
      if (glyphs++ >= limit) continue;
      // one row per glyph however many packages set it: its first command, in force first, with every package
      // that defines that name; other names for the glyph on request
      const all = wanted.filter((x) => x.ch === s.ch).sort((a, b) => force.has(b.name) - force.has(a.name));
      const byName = [...Map.groupBy(all, (x) => x.name).values()];
      const [[main, ...same], ...others] = byName, have = force.has(main.name), pkg = from(main);
      const use = `\\usepackage{${pkg.replace(/\.cls$/, '')}}`;
      const act = (title, icon, extra = '') => `<button type="button" class="pill draw-put" data-name="${escape(main.name)}" data-pkg="${escape(pkg)}"${extra} title="${escape(title)}" aria-label="${escape(title)}">${icon}</button>`;
      rows.push(`<li class="draw-row"><span class="draw-glyph" style="font-family: '${table.fonts[s.font].family}'">${escape(s.ch)}</span>` +
        `<span class="draw-what">${chip(main)}<span class="lib-pkg${doc && !have ? ' draw-off' : ''}" title="${doc && !have ? `not loaded: ${escape(use)}` : escape([main, ...same].map(from).join(', '))}">${pkgs([main, ...same])}</span>` +
        (others.length ? `<button type="button" class="draw-more-pill" aria-expanded="false" title="other commands that set it">+${plural(others.length, 'alias')}</button>` : '') + '</span>' +
        `<span class="draw-acts">${act(`${label} ${main.name}`, ICON[label] ?? ICON.Insert)}` +
        (usepackage && !have && !loaded(pkg) ? act(`${label} ${main.name} with ${use}`, ICON.package, ` data-use="${escape(use)}"`) : '') + '</span>' +
        (others.length ? `<ul class="draw-more" hidden>${others.map((xs) => `<li>${chip(xs[0])} <span class="lib-pkg${off(xs[0])}">${pkgs(xs)}${doc && !force.has(xs[0].name) ? ', not loaded' : ''}</span></li>`).join('')}</ul>` : '') + '</li>');
    }
    if (glyphs > limit) rows.push(`<li class="draw-row-more"><button type="button" class="ghost small" data-more="1">show ${Math.min(DRAW.more, glyphs - limit)} more of ${glyphs.toLocaleString('en')}</button></li>`);
    // the glyphs shown appear once their fonts are in, measured or not
    for (const i of new Set([...seen].map((ch) => wanted.find((x) => x.ch === ch)?.font).filter((i) => i != null))) faceReady(table, i).catch(() => {});
    list.dataset.strokes = String(mine);
    list.innerHTML = rows.join('') || `<li class="muted">${drawFilter.from === 'loaded' ? 'No symbol of this document matches: try every package.' : 'No symbol matches.'}</li>`;
    // how long the drawing took to rank: first results, then the full ranking once every font is measured
    const ms = Math.round(performance.now() - started), pending = f && [...new Set(wanted.map((s) => s.font))].some((i) => table.rows.some((s) => s.font === i && s.f === undefined));
    list.dataset[fresh ? 'first' : 'full'] = String(ms);
    if (!pending) list.dataset.full = String(ms);
    list.classList.toggle('refining', !!pending);
  }
  const ICON = {
    Insert: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2.5v8M4.5 7l3.5 3.5L11.5 7M3 13.5h10"/></svg>',
    Copy: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/><path d="M10.5 5.5V3.5a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2"/></svg>',
    package: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M2.5 5l5.5-2.5L13.5 5v6L8 13.5 2.5 11z"/><path d="M2.5 5L8 7.5 13.5 5M8 7.5v6"/></svg>',
  };
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
    const clear = () => { pen.clearRect(0, 0, pad.width, pad.height); drawn = false; drawMatch(); };
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
    $('draw-search').addEventListener('input', () => drawMatch());
    // how it matches and its credits: the site's card where the page has one, else a popover of the browser's
    $('draw-help').onclick = (e) => {
      const html = `<div class="draw-about">${$('draw-about').innerHTML}</div>`;
      if (self.satexTip) { self.satexTip.card.show(e.currentTarget, html); self.satexTip.card.el.dataset.pin = '1'; return; }
      let pop = root.querySelector('.draw-pop');
      if (!pop) { pop = Object.assign(document.createElement('div'), { className: 'draw-pop tip tip-card', popover: 'auto' }); root.append(pop); }
      pop.innerHTML = html;
      pop.togglePopover();
    };
    // the package filter: a button with a list under it
    $('draw-filters').insertAdjacentHTML('beforeend', '<span class="sep"></span><span class="draw-pkg-wrap"><button type="button" class="lib-kind" id="draw-pkgs" aria-expanded="false" aria-controls="draw-pkg-list">packages</button><div id="draw-pkg-list" class="draw-pkg-list" hidden></div></span>');
    $('draw-pkgs').addEventListener('click', () => { const l = $('draw-pkg-list'), open = l.hidden; l.hidden = !open; $('draw-pkgs').setAttribute('aria-expanded', String(open)); });
    $('draw-pkg-list').addEventListener('change', (e) => {
      const p = e.target.dataset.pkg;
      if (p == null) return;
      if (e.target.checked) skipped.delete(p); else skipped.add(p);
      keepSkipped();
      drawMatch();
    });
    $('draw-pkg-list').addEventListener('click', async (e) => {
      const b = e.target.closest('[data-all]');
      if (!b) return;
      const table = await symbolsLoad();
      skipped = b.dataset.all === '1' ? new Set() : new Set(table.rows.map((s) => s.pkg));
      keepSkipped();
      drawMatch();
      packagesShow(table);
    });
    $('draw-filters').addEventListener('click', (e) => {
      const b = e.target.closest('[data-f]');
      if (!b) return;
      drawFilter[b.dataset.f] = b.dataset.v;
      for (const x of $('draw-filters').querySelectorAll(`[data-f="${b.dataset.f}"]`)) x.setAttribute('aria-pressed', String(x === b));
      drawMatch();
    });
    $('draw-list').addEventListener('click', (e) => {
      if (e.target.closest('[data-more]')) { limit += DRAW.more; drawMatch(false); return; }
      const more = e.target.closest('.draw-more-pill');
      if (more) {
        const open = more.getAttribute('aria-expanded') !== 'true';
        more.setAttribute('aria-expanded', String(open));
        more.closest('li').querySelector('.draw-more').hidden = !open;
        return;
      }
      const b = e.target.closest('.draw-put');
      if (!b) return;
      pick(b.dataset.name, b.dataset.pkg, b.dataset.use ?? null);
      // a moment's ✓ on the button that did it
      b.classList.add('done');
      setTimeout(() => b.classList.remove('done'), 1200);
    });
  }
  symbolsLoad();
  return { load: symbolsLoad, match: drawMatch, filter: drawFilter };
};
