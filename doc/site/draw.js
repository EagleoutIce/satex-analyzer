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
  const DRAW = { side: 32, glyph: 96, pen: 9, bitmap: 0.6, outline: 1, stroke: 1.5, either: 0, grid: 8, bins: 4, orient: 96, shortlist: 1500, refine: 5000, off: ['emoji', 'twemojis', 'fontawesome', 'fontawesome6', 'fontawesome7'], trusted: 0.2, workers: 4, cut: 2.6, least: 12, on: ['worldflags'], pixel: 1.5, typed: 0.35, alias: 0.03, known: 0.2, plain: 0.1, call: 0.2, shown: 14, foreign: 1.12, variant: 0.4, icon: 0.1, aspect: 0.6, dots: 0.4, dot: 4, batch: 300, again: 120, more: 30 };
  const $ = (id) => root.querySelector(`#${id}`);
  const pad = $('draw-pad'), pen = pad.getContext('2d', { willReadFrequently: true });
  let drawTable = null, drawn = false;
  // the filters and the packages chosen apart from the default, kept in the address (`?kind=symbol&pkgs=-twemojis+emoji`)
  // so a link or a reload brings them back, and in this browser for the next visit
  const query = new URLSearchParams(location.search);
  const drawFilter = { from: 'all', kind: query.get('kind') ?? '', mode: query.get('mode') ?? '', shape: query.get('shape') ?? '' };
  let chosen = {};
  try { chosen = JSON.parse(localStorage.getItem('satex-draw-pkgs') ?? '{}'); } catch {}
  for (const m of (query.get('pkgs') ?? '').matchAll(/([+-])([^+-]+)/g)) chosen[m[2]] = m[1] === '+';
  // a package only pictures stand for starts off too (its fonts set nothing to match crisply), unless DRAW.on names it or it is relied on (DRAW.trusted)
  const pictured = new Set();
  const used = (p) => chosen[p] ?? (DRAW.on.includes(p) || !(DRAW.off.includes(p) || pictured.has(p)));
  const remember = () => {
    const q = new URLSearchParams(location.search);
    for (const k of ['kind', 'mode', 'shape']) if (drawFilter[k]) q.set(k, drawFilter[k]); else q.delete(k);
    const pkgs = Object.entries(chosen).map(([p, on]) => `${on ? '+' : '-'}${p}`).join('');
    if (pkgs) q.set('pkgs', pkgs); else q.delete('pkgs');
    if (root.classList.contains('draw-solo')) history.replaceState(null, '', `${location.pathname}${q.size ? `?${q}` : ''}${location.hash}`);
    try { localStorage.setItem('satex-draw-pkgs', JSON.stringify(chosen)); } catch {}
  };
  const choose = (ps, on) => { for (const p of ps) chosen[p] = on; remember(); };
  /** The matcher as a function of its surroundings, so one source runs in a Web Worker (this text, in a Blob)
   * or, where that cannot, on the page: `mk(w, h)` makes a canvas, `post(message, transfer)` answers,
   * `fontSet` takes the loaded fonts, `pause()` yields between batches, `now()` is the clock.
   * Messages in: `init` (the fonts, rows and bitmaps), `match` (a drawing's features and the rows to rank),
   * `tune` (new settings). Messages out: `ranked`, best first, again after each batch of glyphs measured. */
  const engine = (DRAW, mk, post, fontSet, pause, now) => {
    const scratch = {};
    /** The ink of `canvas` as [outline points, distance map, aspect, dots], or null without ink. */
    function features(canvas) {
      const { width: w, height: h } = canvas, data = canvas.getContext('2d', { willReadFrequently: true }).getImageData(0, 0, w, h).data;
      let x0 = w, y0 = h, x1 = -1, y1 = -1;
      for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) if (data[(y * w + x) * 4 + 3] > 40) { x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y); }
      if (x1 < 0) return null;
      const n = DRAW.side, small = (scratch.small ??= mk(n, n)), c = small.getContext('2d', { willReadFrequently: true });
      c.clearRect(0, 0, n, n);
      const bw = x1 - x0 + 1, bh = y1 - y0 + 1, k = (n - 4) / Math.max(bw, bh);
      c.drawImage(canvas, x0, y0, bw, bh, (n - bw * k) / 2, (n - bh * k) / 2, bw * k, bh * k);
      const px = c.getImageData(0, 0, n, n).data, ink = new Uint8Array(n * n);
      for (let i = 0; i < n * n; i++) ink[i] = px[i * 4 + 3] > 64 ? 1 : 0;
      const on = (x, y) => x >= 0 && y >= 0 && x < n && y < n && ink[y * n + x];
      const points = [];
      for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (on(x, y) && !(on(x - 1, y) && on(x + 1, y) && on(x, y - 1) && on(x, y + 1))) points.push(y * n + x);
      // dots by their size on the whole canvas, not after the cut: a glyph of two dots (`¨`) keeps them
      const whole = (scratch.whole ??= mk(n, n)), wc = whole.getContext('2d', { willReadFrequently: true });
      wc.clearRect(0, 0, n, n);
      wc.drawImage(canvas, 0, 0, w, h, 0, 0, n, n);
      const wp = wc.getImageData(0, 0, n, n).data, wink = new Uint8Array(n * n);
      for (let i = 0; i < n * n; i++) wink[i] = wp[i * 4 + 3] > 40 ? 1 : 0;
      if (!points.length) return null;
      const thinned = thin(ink, n), bones = thinned.length ? thinned : Uint16Array.from(points);
      // the outline's hash: shapes of the same picture share it
      let hash = 2166136261;
      for (const p of points) hash = Math.imul(hash ^ p, 16777619);
      return [Uint16Array.from(points), distances(points, n), Math.log(bw / bh), dots(wink, n), bones, distances(bones, n), spread(bones, n), hash >>> 0];
    }
    /** The centre lines of `ink` (Zhang-Suen thinning) as cell indices: a thick stroke and a thin one have the same. */
    function thin(ink, n) {
      const a = Uint8Array.from(ink), drop = [];
      for (let again = true; again;) {
        again = false;
        for (let pass = 0; pass < 2; pass++) {
          drop.length = 0;
          for (let y = 1; y < n - 1; y++) for (let x = 1; x < n - 1; x++) {
            const i = y * n + x;
            if (!a[i]) continue;
            const q0 = a[i - n], q1 = a[i - n + 1], q2 = a[i + 1], q3 = a[i + n + 1], q4 = a[i + n], q5 = a[i + n - 1], q6 = a[i - 1], q7 = a[i - n - 1];
            const b = q0 + q1 + q2 + q3 + q4 + q5 + q6 + q7;
            const cross = (!q0 && q1) + (!q1 && q2) + (!q2 && q3) + (!q3 && q4) + (!q4 && q5) + (!q5 && q6) + (!q6 && q7) + (!q7 && q0);
            if (b < 2 || b > 6 || cross !== 1) continue;
            if (pass ? q0 * q2 * q6 === 0 && q0 * q4 * q6 === 0 : q0 * q2 * q4 === 0 && q2 * q4 * q6 === 0) drop.push(i);
          }
          for (const i of drop) a[i] = 0;
          if (drop.length) again = true;
        }
      }
      const out = [];
      for (let i = 0; i < n * n; i++) if (a[i]) out.push(i);
      return Uint16Array.from(out);
    }
    /** Where the centre lines lie and which way they run, coarsely: their cells spread bilinearly over a DRAW.grid square grid,
     * each into the DRAW.bins directions of a line (from its neighbours), as shares of the whole. */
    function spread(bones, n) {
      const g = DRAW.grid, t = DRAW.bins, d = new Float32Array(g * g * t), k = (g - 1) / (n - 1), has = new Uint8Array(n * n);
      for (const p of bones) has[p] = 1;
      for (const p of bones) {
        const px = p % n, py = (p - px) / n;
        let xx = 0, xy = 0, yy = 0;
        for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) if ((dx || dy) && px + dx >= 0 && px + dx < n && has[(py + dy) * n + px + dx]) { xx += dx * dx; xy += dx * dy; yy += dy * dy; }
        const turn = (((0.5 * Math.atan2(2 * xy, xx - yy)) % Math.PI) + Math.PI) % Math.PI / Math.PI * t, b0 = Math.floor(turn), fb = turn - b0, b = [b0 % t, (b0 + 1) % t];
        const x = px * k, y = py * k, x0 = Math.min(Math.floor(x), g - 2), y0 = Math.min(Math.floor(y), g - 2), fx = x - x0, fy = y - y0;
        for (const [cell, w] of [[y0 * g + x0, (1 - fx) * (1 - fy)], [y0 * g + x0 + 1, fx * (1 - fy)], [(y0 + 1) * g + x0, (1 - fx) * fy], [(y0 + 1) * g + x0 + 1, fx * fy]]) { d[cell * t + b[0]] += w * (1 - fb); d[cell * t + b[1]] += w * fb; }
      }
      for (let i = 0; i < d.length; i++) d[i] /= bones.length;
      return d;
    }
    /** Each cell's distance to the nearest point, by two chamfer passes (3 along, 4 across). */
    function distances(points, n) {
      const d = new Uint8Array(n * n).fill(255);
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
    /** How far apart two centre-line spreads are: half the sum of their differences, 0 to 1. */
    function shape(a, b) {
      let share = 0;
      for (let i = 0; i < a.length; i++) share += Math.abs(a[i] - b[i]);
      return share / 2;
    }
    /** A cheap bound on `apart`, to pick the shapes worth measuring in full: how far the spreads and the aspects are. */
    function roughly(a, b) {
      return shape(a[6], b[6]) + DRAW.aspect * Math.abs(a[2] - b[2]);
    }
    /** How far apart two shapes' features are: 0 for the same outline. */
    function apart([pa, da, aa, ka, sa, ea, ha], [pb, db, ab, kb, sb, eb, hb]) {
      let ab_ = 0, ba = 0, sab = 0, sba = 0;
      for (const p of pa) ab_ += db[p];
      for (const p of pb) ba += da[p];
      for (const p of sa) sab += eb[p];
      for (const p of sb) sba += ea[p];
      const edge = DRAW.outline * (ab_ / pa.length + ba / pb.length) / 3, bone = DRAW.stroke * (sab / sa.length + sba / sb.length) / 3;
      return (DRAW.either ? Math.min(edge, bone) : edge + bone) + DRAW.aspect * Math.abs(aa - ab) + DRAW.dots * Math.min(3, Math.abs(ka - kb)) + DRAW.orient * shape(ha, hb);
    }
    /** A bitmap symbol as a canvas: its pixels in the alpha channel, at its own size. */
    function bitmapCanvas({ w, h, bits }) {
      const c = mk(w, h), g = c.getContext('2d', { willReadFrequently: true }), img = g.createImageData(w, h);
      for (let i = 0; i < w * h; i++) img.data[i * 4 + 3] = (bits[i >> 3] >> (7 - (i & 7))) & 1 ? 255 : 0;
      g.putImageData(img, 0, 0);
      return c;
    }
    let fonts = [], rows = [], bitmaps = [], feats = [], req = null, deadline = 0, busy = false, sent = 0, spent = 0, part = [0, 1];
    const faces = [];
    const face = (i) => (faces[i] ??= fonts[i].url == null ? Promise.resolve() : new FontFace(fonts[i].family, `url(${fonts[i].url})`).load().then((f) => { fontSet.add(f); }));
    /** The glyphs of `list` (row numbers, all of one font) drawn and measured; a bitmap is set as big as a font's glyph is: its longer side at DRAW.bitmap of the canvas. */
    function measure(i, list) {
      const g = mk(DRAW.glyph, DRAW.glyph), c = g.getContext('2d', { willReadFrequently: true });
      c.font = `${DRAW.glyph * 0.7}px "${fonts[i].family}"`;
      c.textAlign = 'center';
      c.textBaseline = 'middle';
      for (const j of list) {
        const [, ch, bm] = rows[j], b = bitmaps[bm];
        c.clearRect(0, 0, g.width, g.height);
        if (b) { const k = DRAW.glyph * DRAW.bitmap / Math.max(b.w, b.h); c.drawImage(bitmapCanvas(b), (g.width - b.w * k) / 2, (g.height - b.h * k) / 2, b.w * k, b.h * k); } else c.fillText(ch, g.width / 2, g.height / 2);
        feats[j] = features(g);
      }
    }
    /** The wanted rows measured so far, ranked: `score` is the distance times the row's `mul` plus its `add`; a picture seen twice points at its first. */
    function rank(done) {
      const t = (sent = now());
      const { id, f, want, mul, add } = req;
      let pool = [];
      for (let k = 0; k < want.length; k++) if (feats[want[k]]) pool.push(k);
      if (pool.length > DRAW.shortlist) pool = pool.map((k) => [k, roughly(f, feats[want[k]])]).sort((a, b) => a[1] - b[1]).slice(0, DRAW.shortlist).map((a) => a[0]);
      const best = pool.map((k) => [want[k], apart(f, feats[want[k]]) * mul[k] + add[k]]).sort((a, b) => a[1] - b[1]);
      const first = new Map(), head = new Int32Array(best.length);
      best.forEach(([i], k) => { head[k] = first.get(feats[i][7]) ?? first.set(feats[i][7], i).get(feats[i][7]); });
      const idx = Int32Array.from(best, (b) => b[0]), score = Float32Array.from(best, (b) => b[1]);
      post({ type: 'ranked', id, idx, score, head, pending: !done && now() < deadline && want.some((i) => feats[i] === undefined) }, [idx.buffer, score.buffer, head.buffer]);
      spent = now() - t;
    }
    /** Measures what the request wants, a batch at a time, until all is or DRAW.refine has passed; every DRAW.again ms (or four times what a ranking took) it ranks again. */
    async function pump() {
      if (busy) return;
      busy = true;
      try {
        while (req && now() < deadline) {
          const todo = req.want.filter((i) => feats[i] === undefined);
          if (!todo.length) break;
          const i = rows[todo[0]][0], batch = todo.filter((j) => rows[j][0] === i).slice(0, DRAW.batch);
          try { await face(i); measure(i, batch); } catch { for (const j of batch) feats[j] = null; }
          if (now() - sent >= Math.max(DRAW.again, 4 * spent)) rank(false);
          await pause();
        }
      } finally { busy = false; }
      if (req) rank(true);
    }
    const on = (m) => {
      if (m.type === 'init') ({ fonts, rows, bitmaps } = m), (part = m.part ?? [0, 1]), (feats = new Array(rows.length));
      else if (m.type === 'tune') Object.assign(DRAW, m.draw);
      else if (m.type === 'match') {
        if (m.fresh || !req) deadline = now() + DRAW.refine;
        // this matcher's share of the rows: every n-th, so each font's glyphs are spread over the matchers
        const mine = [];
        m.want.forEach((j, k) => { if (j % part[1] === part[0]) mine.push(k); });
        req = { ...m, want: Int32Array.from(mine, (k) => m.want[k]), mul: Float32Array.from(mine, (k) => m.mul[k]), add: Float32Array.from(mine, (k) => m.add[k]) };
        for (const i of new Set(m.want.filter((j) => feats[j] === undefined).map((j) => rows[j][0]))) face(i).catch(() => {});
        rank(false);
        pump();
      }
    };
    return { on, features, apart, bitmapCanvas };
  };
  const mkDoc = (w, h) => Object.assign(document.createElement('canvas'), { width: w, height: h });
  const idle = () => new Promise((done) => (self.requestIdleCallback ? requestIdleCallback(() => done(), { timeout: 30 }) : setTimeout(done, 0)));
  const local = engine(DRAW, mkDoc, (m) => arrive(m), document.fonts, idle, () => performance.now());
  const { features, apart, bitmapCanvas } = local;
  // the matching runs in a worker; without one (no OffscreenCanvas, no fonts in workers) the same engine runs here, between frames
  // several workers share the glyphs, each its every n-th row; their rankings merge as they come
  const last = {};
  let workers = [];
  const send = (m) => {
    if (m.type !== 'tune') last[m.type] = m;
    if (workers.length) workers.forEach((w, k) => w.postMessage(m.type === 'init' ? { ...m, part: [k, workers.length] } : m));
    else local.on(m);
  };
  try {
    if (typeof OffscreenCanvas !== 'undefined' && self.Worker) {
      const code = `const e = (${engine})(${JSON.stringify(DRAW)}, (w, h) => new OffscreenCanvas(w, h), (m, t) => postMessage(m, t), self.fonts, () => new Promise((r) => setTimeout(r)), () => performance.now());\nonmessage = (v) => e.on(v.data);\npostMessage({ type: 'hello', ok: !!self.fonts && typeof FontFace === 'function' });`;
      const url = URL.createObjectURL(new Blob([code], { type: 'text/javascript' }));
      const n = Math.max(1, Math.min(DRAW.workers, (navigator.hardwareConcurrency ?? 2) - 1));
      const fail = () => { if (!workers.length) return; workers.forEach((w) => w.terminate()); workers = []; for (const m of Object.values(last)) local.on(m); };
      workers = Array.from({ length: n }, (_, k) => {
        const w = new Worker(url);
        w.onmessage = (e) => (e.data.type === 'hello' ? e.data.ok || fail() : arrive({ ...e.data, part: k }));
        w.onerror = fail;
        return w;
      });
    }
  } catch { workers = []; }
  /** The symbol table; each font's glyphs are measured once a candidate in it is wanted. */
  async function symbolsLoad() {
    drawTable ??= fetch(`${base}${symbols}`).then((r) => r.json()).then(async ({ fonts, rows, glyphs, weight }) => {
      // symbols no font sets are bitmaps (w, h, then 1 bit per pixel), one after the other in a gzipped file: the last "font"
      const bitmaps = [], bytes = new Uint8Array(await new Response((await fetch(`${base}${glyphs}`)).body.pipeThrough(new DecompressionStream('gzip'))).arrayBuffer());
      const end = (at) => at + 2 + Math.ceil(bytes[at] * bytes[at + 1] / 8);
      for (let at = 0; at < bytes.length; at = end(at)) bitmaps.push({ w: bytes[at], h: bytes[at + 1], bits: bytes.subarray(at + 2, end(at)) });
      return {
        weight, bitmaps,
        fonts: [...fonts.map((url, i) => ({ family: `satex-sym-${i}`, url, ready: null })), { bitmaps: true, ready: null }],
        rows: rows.map(([code, name, cls, about, font, pkg, bitmap], i) => ({
          ch: bitmap == null ? String.fromCodePoint(code) : `#${bitmap}`, code, name, cls, about, pkg, i, bm: bitmap ?? -1, bitmap: bitmaps[bitmap], font: bitmap == null ? font : fonts.length,
          // the name, its camel-case words (`faSkullCrossbones`: fa skull crossbones) and what it is
          text: `${name.replace(/^\\/, '')} ${name.replace(/^\\/, '').replace(/([a-z])([A-Z])/g, '$1 $2')} ${about ?? ''}`.toLowerCase().replace(/[-_{}]/g, ' '),
        })),
      };
    }).then((t) => {
      const fonted = new Set(t.rows.filter((s) => !s.bitmap).map((s) => s.pkg));
      for (const s of t.rows) if (!fonted.has(s.pkg) && (t.weight?.[s.pkg] ?? 0) < DRAW.trusted) pictured.add(s.pkg);
      totals(t, t.rows.length, false);
      send({ type: 'init', fonts: t.fonts.map((f) => ({ family: f.family, url: f.bitmaps ? null : new URL(`${base}${f.url}`, location.href).href })), rows: t.rows.map((s) => [s.font, s.ch, s.bm]), bitmaps: t.bitmaps });
      return t;
    }).catch((e) => ({ fonts: [], rows: [], error: String(e?.message ?? e) }));
    return drawTable;
  }
  /** A font's face, loaded once: rows drawn in it show their glyphs as soon as it is in. */
  function faceReady(table, i) {
    const font = table.fonts[i];
    if (font.bitmaps) return Promise.resolve();
    font.face ??= new FontFace(font.family, `url(${base}${font.url})`).load().then((face) => { document.fonts.add(face); return face; });
    return font.face;
  }
  /** A row's glyph: its character in its font, or its bitmap as the mask of a box in the text colour. */
  const glyph = (s, table) => {
    if (!s.bitmap) return `<span class="draw-glyph" style="font-family: '${table.fonts[s.font].family}'">${esc(s.ch)}</span>`;
    s.url ??= bitmapCanvas(s.bitmap).toDataURL();
    return `<span class="draw-glyph draw-bmp" style="--bmp: url(${s.url})" role="img" aria-label="${esc(s.name)}"></span>`;
  };
  /** Plain symbols over the styled copies of alphabets (math alphanumerics) and over icons. */
  // a math alphabet's variant (`\mbfsansalpha`) ranks after the letter itself (`\alpha`, set from the same block)
  // a math alphabet's letters (`\mbfsansf`, `\Bbbf`), named apart from the letter they restyle
  const ALPHABET = /^\\(?:m(?:up|it|bf|sans|tt|scr|frak|bb)+|Bbb)(?=[A-Za-z])/;
  // a letter or digit one can type ranks after the symbols: no one draws an `L` to find it
  const prior = (s) => (/^[A-Za-z0-9]$/.test(s.ch) ? DRAW.typed : ALPHABET.test(s.name) ? DRAW.variant : s.cls === 'icon' ? DRAW.icon : s.bitmap ? DRAW.plain + (s.name.includes('{') ? DRAW.call : 0) : 0);
  const drawKind = (s) => (s.cls === 'alpha' ? 'letter' : 'symbol');
  const drawMode = (s) => (s.cls === 'icon' || s.cls === 'text' || s.cls === 'other' ? 'text' : 'math');
  /** Whether `a` and `b` are at most one edit apart. */
  const near = (a, b) => {
    if (Math.abs(a.length - b.length) > 1) return false;
    let i = 0, j = 0, edits = 0;
    while (i < a.length && j < b.length) {
      if (a[i] === b[j]) { i++; j++; continue; }
      if (++edits > 1) return false;
      if (a.length > b.length) i++; else if (b.length > a.length) j++; else { i++; j++; }
    }
    return edits + (a.length - i) + (b.length - j) <= 1;
  };
  /** How well the search words name a symbol, lower better, or null: the name itself first, then a word
   * the name or description starts, then one inside a word, then one a letter off (a typo). */
  const named = (s, words) => {
    const bare = s.name.replace(/^\\/, '').toLowerCase();
    if (words.length === 1 && bare === words[0]) return -2;
    let rank = 0;
    for (const w of words) {
      const at = s.text.indexOf(w);
      if (at >= 0) { rank += at === 0 ? 0 : s.text[at - 1] === ' ' ? 1 : 2; continue; }
      if (w.length < 4 || !s.text.split(' ').some((t) => near(t, w))) return null;
      rank += 3;
    }
    return rank;
  };
  /** Whether every word of the search names the package: inside its name, or (from four letters) spread over it. */
  const packageNamed = (pkg, words) => {
    const p = pkg.toLowerCase().replace(/\.cls$/, ''), spread = (w) => { let i = 0; for (const c of p) if (c === w[i]) i++; return i === w.length; };
    return words.every((w) => p.includes(w) || (w.length > 3 && spread(w)));
  };
  /** The packages' list: each with its count of symbols, a box to leave it out; all or none at once. */
  function packagesShow(table) {
    const counts = new Map();
    for (const s of table.rows) counts.set(s.pkg, (counts.get(s.pkg) ?? 0) + 1);
    const list = [...counts].sort((a, b) => a[0].localeCompare(b[0], 'en', { sensitivity: 'base' }));
    $('draw-pkgs').textContent = `packages ${list.filter(([p]) => used(p)).length}/${list.length}`;
    $('draw-pkg-list').innerHTML = `<div class="draw-pkg-all"><button type="button" class="ghost small" data-all="1">all</button><button type="button" class="ghost small" data-all="0">none</button></div>` +
      list.map(([p, n]) => `<label><input type="checkbox" data-pkg="${esc(p)}"${used(p) ? ' checked' : ''}> ${esc(p)} <b>${n.toLocaleString('en')}</b></label>`).join('');
  }
  // the table's size, matches and recognition time, beside the pad's buttons
  let meta = ['', ''], took = '';
  const total = $('draw-total');
  const metaShow = () => { total.textContent = [...meta, took].filter(Boolean).join(' · '); };
  /** The table's size, and how many symbols the filters leave. */
  function totals(table, shown, searched) {
    packagesShow(table);
    const pkgs = new Set(table.rows.map((s) => s.pkg)), on = [...pkgs].filter(used).length;
    const short = (n) => (n >= 1e4 ? `${Math.round(n / 1e3)}k` : n.toLocaleString('en'));
    const of = shown < table.rows.length ? `${short(shown)} of ${short(table.rows.length)}` : short(table.rows.length);
    meta = [`${of} symbols · ${on < pkgs.size ? `${on} of ${pkgs.size}` : pkgs.size} packages`, searched ? `${short(shown)} ${shown === 1 ? 'match' : 'matches'}` : ''];
    metaShow();
  }
  let matching = 0, started = 0, limit = DRAW.shown, current = null, result = null;
  /** The candidates for the drawing and the search: what the filters and the words leave is sent to the matcher,
   * which answers best first now and again as more glyphs are measured. */
  async function drawMatch(fresh = true) {
    const mine = fresh ? ++matching : matching, list = $('draw-list'), f = features(pad);
    const words = $('draw-search').value.trim().toLowerCase().replace(/^\\/, '').split(/[\s\-_]+/).filter(Boolean);
    if (fresh) { started = performance.now(); limit = DRAW.more; }
    const [table, names] = await Promise.all([symbolsLoad(), inForce()]);
    if (mine !== matching) return;
    if (!table.rows.length) { list.innerHTML = `<li class="muted">The symbols did not load${table.error ? `: ${esc(table.error)}` : ''}. Reload the page to try again.</li>`; return; }
    // "this document": the names in force in its run, nothing a package it does not load defines
    const source = new Map(names), force = new Set(source.keys());
    const ranked = new Map();
    const wanted = table.rows.filter((s) => {
      if (drawFilter.from !== 'all' && !force.has(s.name)) return false;
      if (!used(s.pkg)) return false;
      if ((drawFilter.kind && drawKind(s) !== drawFilter.kind) || (drawFilter.mode && drawMode(s) !== drawFilter.mode)) return false;
      if (drawFilter.shape && (drawFilter.shape === 'pixel') !== !!s.bitmap) return false;
      // within a rank the more relied-on package first; browsing puts plain characters and pictures last
      const known = table.weight?.[s.pkg] ?? 0, plain = (s.code > 32 && s.code < 128 ? 1 : 0) + (s.bitmap ? 0.6 : 0);
      const hit = words.length ? named(s, words) ?? (packageNamed(s.pkg, words) ? 3 : null) : null;
      const r = words.length ? (hit == null ? null : hit - known / 2) : f ? 0 : 1 - known + plain;
      if (r == null) return false;
      ranked.set(s, r);
      return true;
    });
    totals(table, wanted.length, words.length > 0);
    current = { id: mine, table, wanted, ranked, source, force, words, f };
    result = null;
    if (!f) { render(); return; }
    // the more a package is relied on (build.py `established`), the lower its symbols score
    // the more packages and names set a glyph, the more it is used: a little lower for each, up to five
    const setters = (table.setters ??= table.rows.reduce((m, x) => m.set(x.ch, (m.get(x.ch) ?? 0) + 1), new Map()));
    const trust = (s) => (1 - DRAW.known * (table.weight?.[s.pkg] ?? 0)) * (1 - DRAW.alias * Math.min(5, (setters.get(s.ch) ?? 1) - 1));
    // the established packages' symbols are measured first, so they show at once
    const first = wanted.map((s, k) => [s, -(table.weight?.[s.pkg] ?? 0), k]).sort((a, b) => a[1] - b[1] || a[2] - b[2]).map((a) => a[0]);
    send({ type: 'match', id: mine, fresh, f, want: Int32Array.from(first, (s) => s.i),
      // a picture scores half again worse: a font's glyph of the shape wins
      mul: Float32Array.from(first, (s) => (force.has(s.name) ? 1 : DRAW.foreign) * (s.bitmap ? DRAW.pixel : 1) * trust(s)), add: Float32Array.from(first, (s) => (ranked.get(s) * 0.05 + prior(s)) * trust(s)) });
  }
  /** The matcher's answer for the latest request: shown unless it is older. */
  const shares = new Map();
  function arrive(m) {
    if (m.type !== 'ranked' || m.id !== current?.id) return;
    if (shares.get('id') !== m.id) { shares.clear(); shares.set('id', m.id); }
    shares.set(m.part ?? 0, m);
    // the workers' shares as one ranking, best first
    const all = [...shares.values()].filter((x) => typeof x === 'object');
    const order = all.flatMap((x) => Array.from(x.idx, (i, k) => [i, x.score[k], x.head[k]])).sort((a, b) => a[1] - b[1]);
    result = { id: m.id, idx: Int32Array.from(order, (o) => o[0]), score: Float32Array.from(order, (o) => o[1]), head: Int32Array.from(order, (o) => o[2]),
      pending: all.length < Math.max(1, workers.length) || all.some((x) => x.pending) };
    render();
  }
  /** The list ends at the window's bottom (or the widget's, if it ends sooner) and scrolls inside: "show more" stays in view. */
  const fit = () => {
    const list = $('draw-list'), top = list.getBoundingClientRect().top, bottom = Math.min(innerHeight, root.getBoundingClientRect().bottom);
    list.style.maxHeight = `${Math.max(DRAW.least * 8, bottom - top - 8)}px`;
  };
  addEventListener('resize', fit);
  /** The list: one row per picture, best first, from the latest ranking. */
  function render() {
    const { id: mine, table, wanted, ranked, source, force, words, f } = current, list = $('draw-list');
    let best;
    if (f) {
      // nothing measured yet: the list stays as it was
      if (!result || (!result.idx.length && result.pending)) return;
      // only what looks enough like the best match: within DRAW.cut of its distance
      const bound = result.score.length ? result.score[0] * DRAW.cut + 0.05 : Infinity;
      best = Array.from(result.idx, (i, k) => ({ s: table.rows[i], i, head: result.head[k], score: result.score[k] })).filter((b, k) => k < DRAW.least || b.score <= bound);
    } else best = wanted.map((s) => ({ s, i: s.i, head: s.i, score: ranked.get(s) + s.name.length / 100 })).sort((a, b) => a.score - b.score);
    // rows of one picture follow its first, and so do a family's later glyphs (`\varpropto`, `\leftpropto` after `\propto`)
    const twins = new Map(), family = new Map();
    const kin = (name) => name.replace(ALPHABET, '\\').replace(/^\\/, '').replace(/^(?:var|left|right|big|small|text|math)+(?=[a-z])/i, '').replace(/\{.*$/, '');
    best = best.map((b) => {
      if (b.head !== b.i) return b;
      const first = family.get(kin(b.s.name));
      if (first == null) { family.set(kin(b.s.name), b.i); return b; }
      return { ...b, head: first };
    });
    for (const { s, i, head } of best) if (head !== i) twins.set(head, [...twins.get(head) ?? [], s]);
    const from = (x) => source.get(x.name) ?? x.pkg;
    const off = (x) => (doc && !force.has(x.name) ? ' draw-off' : '');
    const chip = (x) => force.has(x.name) ? `<code class="cs-ref" data-cs="${esc(x.name)}" tabindex="0">${esc(x.name)}</code>`
      : `<code class="draw-name${off(x)}" data-sym="${x.i}" tabindex="0">${esc(x.name)}</code>`;
    const loaded = (pkg) => [...source.values()].includes(pkg) || pkg === 'kernel';
    // a name's packages: the first three, then how many more
    const pkgs = (list) => { const p = [...new Set(list.map(from))]; return esc(p.slice(0, 3).join(', ')) + (p.length > 3 ? ` +${p.length - 3}` : ''); };
    const seen = new Set(), rows = [], icon = ICON[label] ?? ICON.Insert;
    let glyphs = 0;
    for (const { s, i, head } of best) {
      if (head !== i || seen.has(s.ch)) continue;
      seen.add(s.ch);
      if (glyphs++ >= limit) continue;
      // one row per glyph with every package that sets it, led by the name in force, else the most relied-on one; other names on request
      const chs = new Set([s, ...twins.get(i) ?? []].map((x) => x.ch)), all = wanted.filter((x) => chs.has(x.ch))
        .sort((a, b) => force.has(b.name) - force.has(a.name) || (table.weight?.[b.pkg] ?? 0) - (table.weight?.[a.pkg] ?? 0) || a.name.length - b.name.length);
      const byName = [...Map.groupBy(all, (x) => x.name).values()];
      const [[main, ...same], ...others] = byName, have = force.has(main.name), pkg = from(main);
      const use = `\\usepackage{${pkg.replace(/\.cls$/, '')}}`;
      const act = (title, icon, extra = '', x = main, xp = pkg) => `<button type="button" class="pill draw-put" data-name="${esc(x.name)}" data-pkg="${esc(xp)}"${extra} title="${esc(title)}" aria-label="${esc(title)}">${icon}</button>`;
      rows.push(`<li class="draw-row">${glyph(s, table)}` +
        `<span class="draw-what">${chip(main)}${s.bitmap ? '<span class="draw-px" title="a picture from the symbol list: no font sets it">px</span>' : ''}<span class="lib-pkg${doc && !have ? ' draw-off' : ''}" title="${doc && !have ? `not loaded: ${esc(use)}` : esc([main, ...same].map(from).join(', '))}">${pkgs([main, ...same])}</span>` +
        (others.length ? `<button type="button" class="draw-more-pill" aria-expanded="false" title="${esc(others.map((xs) => `${xs[0].name} (${[...new Set(xs.map(from))].join(', ')})`).join('\n'))}">+${plural(others.length, 'alias')}</button>` : '') + '</span>' +
        `<span class="draw-acts">${act(`${label} ${main.name}`, icon)}` +
        (usepackage && !have && !loaded(pkg) ? act(`${label} ${main.name} with ${use}`, ICON.package, ` data-use="${esc(use)}"`) : '') + '</span>' +
        (others.length ? `<ul class="draw-more" hidden>${others.map((xs) => `<li>${chip(xs[0])} <span class="lib-pkg${off(xs[0])}">${pkgs(xs)}${doc && !force.has(xs[0].name) ? ', not loaded' : ''}</span><span class="draw-acts">${act(`${label} ${xs[0].name}`, icon, '', xs[0], from(xs[0]))}</span></li>`).join('')}</ul>` : '') + '</li>');
    }
    if (glyphs > limit) rows.push(`<li class="draw-row-more"><button type="button" class="ghost small" data-more="1">show ${Math.min(DRAW.more, glyphs - limit)} more${words.length ? ` of ${glyphs.toLocaleString('en')}` : ''}</button></li>`);
    // the glyphs shown appear once their fonts are in, measured or not
    for (const i of new Set([...seen].map((ch) => wanted.find((x) => x.ch === ch)?.font).filter((i) => i != null))) faceReady(table, i).catch(() => {});
    list.dataset.strokes = String(mine);
    list.innerHTML = rows.join('') || `<li class="muted">${drawFilter.from === 'loaded' ? 'No symbol of this document matches: try every package.' : 'No symbol or package matches.'}</li>`;
    // how long the drawing took to rank: first results, then the full ranking once every font is measured
    const ms = Math.round(performance.now() - started), pending = f && result.pending;
    if (!current.shown) { current.shown = true; list.dataset.first = String(ms); }
    if (!pending) list.dataset.full = String(ms);
    fit();
    list.classList.toggle('refining', !!pending);
    total.classList.toggle('refining', !!pending);
    took = f ? `${ms} ms` : '';
    metaShow();
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
    const does = [s.about, s.bitmap ? '' : `U+${s.code.toString(16).toUpperCase().padStart(4, '0')}`].filter(Boolean).join(' · ');
    if (self.satexTip && self.satexCards) self.satexTip.card.show(el, self.satexCards.explain({ name: s.name, tag: s.cls === 'icon' ? 'icon' : s.cls === 'text' || s.cls === 'other' ? s.cls : `math ${s.cls}`, does, package: s.pkg }));
    else el.title = `${s.name}: ${does}, ${s.pkg}`;
  });
  $('draw-list').addEventListener('mouseout', (e) => { if (e.target.closest('[data-sym]')) self.satexTip?.card.hide(150); });
  // a glyph under the mouse, large in a card of its own, centred on it
  const zoom = Object.assign(document.createElement('div'), { className: 'draw-zoom', hidden: true });
  root.append(zoom);
  $('draw-list').addEventListener('mouseover', (e) => {
    const g = e.target.closest('.draw-glyph');
    if (!g) return;
    const b = g.getBoundingClientRect();
    zoom.replaceChildren(g.cloneNode(true));
    Object.assign(zoom.style, { left: `${b.left + b.width / 2}px`, top: `${b.top + b.height / 2}px` });
    zoom.hidden = false;
  });
  $('draw-list').addEventListener('mouseout', (e) => { if (e.target.closest('.draw-glyph')) zoom.hidden = true; });
  $('draw-list').addEventListener('scroll', () => { zoom.hidden = true; }, { passive: true });
  {
    // the strokes are kept: a change of theme draws them again in its ink
    let ink = [];
    const paint = () => {
      pen.clearRect(0, 0, pad.width, pad.height);
      Object.assign(pen, { lineWidth: DRAW.pen, lineCap: 'round', lineJoin: 'round', strokeStyle: getComputedStyle(pad).color });
      for (const st of ink) { pen.beginPath(); st.forEach((p, k) => (k ? pen.lineTo(...p) : pen.moveTo(...p))); pen.stroke(); }
    };
    const clear = () => { ink = []; paint(); drawn = false; drawMatch(); };
    new MutationObserver(paint).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    matchMedia('(prefers-color-scheme: dark)').addEventListener?.('change', paint);
    let last = null;
    const at = (e) => { const b = pad.getBoundingClientRect(); return [(e.clientX - b.left) * pad.width / b.width, (e.clientY - b.top) * pad.height / b.height]; };
    pad.addEventListener('pointerdown', (e) => { pad.setPointerCapture(e.pointerId); last = at(e); drawn = true; ink.push([last]); });
    pad.addEventListener('pointermove', (e) => {
      if (!last) return;
      const p = at(e);
      Object.assign(pen, { lineWidth: DRAW.pen, lineCap: 'round', lineJoin: 'round', strokeStyle: getComputedStyle(pad).color });
      pen.beginPath(); pen.moveTo(...last); pen.lineTo(...p); pen.stroke();
      ink.at(-1).push(p);
      last = p;
    });
    pad.addEventListener('pointerup', () => { last = null; if (drawn) drawMatch(); });
    $('draw-clear').onclick = clear;
    $('draw-search').addEventListener('input', () => drawMatch());
    // day or night: kept in the address (an embedding frame, a reload) and in the browser
    const theme = $('draw-theme'), html = document.documentElement;
    if (theme) {
      const dark = () => (html.dataset.theme ? html.dataset.theme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches);
      // the icon is the mode a press switches to: a sun at night, a moon by day
      const sun = '<circle cx="8" cy="8" r="2.8"/><path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1"/>', moon = '<path d="M13 9.5A5.5 5.5 0 0 1 6.5 3a5.5 5.5 0 1 0 6.5 6.5z"/>';
      const mark = () => { theme.setAttribute('aria-pressed', String(dark())); theme.querySelector('svg').innerHTML = dark() ? sun : moon; };
      mark();
      theme.onclick = () => {
        const t = dark() ? 'light' : 'dark';
        html.dataset.theme = t;
        try { const u = new URL(location.href); u.searchParams.set('theme', t); history.replaceState(null, '', u); } catch {}
        try { localStorage.setItem('satex-theme', t); } catch {}
        mark();
      };
    }
    // how it matches and its credits: the site's card where the page has one, else a popover of the browser's
    $('draw-help').onclick = (e) => {
      const html = `<div class="draw-about">${$('draw-about').innerHTML}</div>`;
      if (self.satexTip) { self.satexTip.card.show(e.currentTarget, html); self.satexTip.card.el.dataset.pin = '1'; return; }
      let pop = root.querySelector('.draw-pop');
      if (!pop) { pop = Object.assign(document.createElement('div'), { className: 'draw-pop tip tip-card', popover: 'auto' }); root.append(pop); }
      pop.innerHTML = html;
      pop.togglePopover();
    };
    // the filters as dropdowns in the bar beside the search: kind, mode, glyph source, then the packages
    const FILTERS = [['kind', 'any kind', [['symbol', 'symbols'], ['letter', 'letters']]], ['mode', 'any mode', [['math', 'math'], ['text', 'text']]],
      ['shape', 'any source', [['font', 'fonts'], ['pixel', 'pictures']]]];
    $('draw-filters').innerHTML = FILTERS.map(([f, all, options]) => `<select class="dropdown" data-f="${f}" aria-label="${all}"><option value="">${all}</option>` +
      options.map(([v, label]) => `<option value="${v}">${label}</option>`).join('') + '</select>').join('');
    for (const sel of $('draw-filters').querySelectorAll('select[data-f]')) sel.value = drawFilter[sel.dataset.f] ?? '';
    root.querySelector('.draw-top')?.append($('draw-filters'));
    // the package filter: a button with a list under it
    $('draw-filters').insertAdjacentHTML('beforeend', '<span class="draw-pkg-wrap"><button type="button" class="lib-kind" id="draw-pkgs" aria-expanded="false" aria-controls="draw-pkg-list">packages</button><div id="draw-pkg-list" class="draw-pkg-list" hidden></div></span>');
    const packagesOpen = (open) => { $('draw-pkg-list').hidden = !open; $('draw-pkgs').setAttribute('aria-expanded', String(open)); };
    $('draw-pkgs').addEventListener('click', () => packagesOpen($('draw-pkg-list').hidden));
    document.addEventListener('click', (e) => { if (!e.target.closest('.draw-pkg-wrap')) packagesOpen(false); });
    document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && !$('draw-pkg-list').hidden) { packagesOpen(false); $('draw-pkgs').focus(); } });
    $('draw-pkg-list').addEventListener('change', (e) => {
      const p = e.target.dataset.pkg;
      if (p == null) return;
      choose([p], e.target.checked);
      drawMatch();
    });
    $('draw-pkg-list').addEventListener('click', async (e) => {
      const b = e.target.closest('[data-all]');
      if (!b) return;
      const table = await symbolsLoad();
      choose(new Set(table.rows.map((s) => s.pkg)), b.dataset.all === '1');
      drawMatch();
      packagesShow(table);
    });
    $('draw-filters').addEventListener('change', (e) => {
      const f = e.target.dataset?.f;
      if (!f) return;
      drawFilter[f] = e.target.value;
      remember();
      drawMatch();
    });
    $('draw-list').addEventListener('click', (e) => {
      if (e.target.closest('[data-more]')) { limit += DRAW.more; if (current) render(); return; }
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
  const tune = (cfg) => { Object.assign(DRAW, cfg); send({ type: 'tune', draw: cfg }); };
  // the list is never empty: once loaded, it shows the symbols to browse
  return { load: () => symbolsLoad().then((t) => { drawMatch(); return t; }), match: drawMatch, filter: drawFilter, tune };
};
