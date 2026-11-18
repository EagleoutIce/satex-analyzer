// Editing in the PDF: what a column-width handle and a caret on the pages map to in the source.
// Pure: SyncTeX records and the server's tables in, places and new values out (playground.js draws and writes).
export {};
/** A SyncTeX record as playground.js parses it, in bp from the page's top left. */
type Box = { page: number; file: string; line: number; kind: string; x: number; y: number; w: number; h: number; d: number };
/** A table of `satex/dialogApply` kind `columns` (src/lsp/dialogs.rs). */
type Table = { env: string; lines: [number, number]; width: unknown; cols: { align: string; width: string; node: SnapNode | null; spec?: SnapNode }[] };
/** A column found on a page: its index, where its cells' boxes stand, how wide they are, in bp. */
type Placed = { k: number; page: number; x: number; w: number; t: number; d: number; n: number };
/** A width as written that a drag may rewrite: a length (`3cm`) or a share of a width (`.3\linewidth`). */
/** A box on a page, in bp: left, top, right, bottom. */
/** A word on a page as the words file lists it: page, left, top, right, baseline, text, in bp. */
type Word = [number, number, number, number, number, string];
/** A verbatim body: `from..to` of the text, a block (`verbatim`, `lstlisting`, `minted`) or inline (`\verb|…|`), its closer. */
type Verb = { from: number; to: number; block: boolean; close: string };
/** A source word: its offsets `s..e` in the text and what the page shows of it. */
type Tok = { s: number; e: number; t: string };
type Flat = { page: number; l: number; t: number; r: number; d: number };
type Form = { kind: 'len'; num: number; unit: string } | { kind: 'share'; num: number; name: string; lead: boolean };
declare global {
  interface Window {
    satexPdfEdit: {
      place(boxes: Box[], file: string, table: Table, tables: Table[]): Placed[];
      extent(boxes: Box[], file: string, table: Table): Box | null;
      form(text: string): Form | null;
      resized(text: string, was: number, now: number, fine: boolean): string | null;
      stepped(text: string, dir: number, fine: boolean): string | null;
      room(boxes: Box[], file: string, table: Table): number | null;
      plain(text: string): boolean;
      editable(text: string, from: number, to: number, keys: [number, number][]): string | null;
      spans(line: string, word: string): [number, number][];
      sender<V>(send: (v: V) => Promise<boolean>): { put(v: V): void; flush(): Promise<void> };
      typed(text: string, at: number, c: string, inline?: string, smart?: boolean, fresh?: [number, number] | null): [number, number, string, number];
      erased(text: string, at: number, back: boolean, fresh?: [number, number] | null): [number, number];
      checked(text: string, edit: [number, number, string, number?], o?: object): string | null;
      naming(text: string, at: number, fresh?: [number, number] | null): boolean;
      entries(texts: Record<string, string>, root: string, kind: string): [string, number, number][];
      grow(f: [number, number] | null | undefined, a: number, b: number, n: number): [number, number] | null;
      key(text: string, at: number, k: string, o?: object): [number, number, string, number] | string;
      broken(text: string, at: number, shift: boolean): [number, number, string, number] | string;
      scan(text: string, from: number, to: number, st: object, out?: { s: number; e: number; t: string }[]): { math: string; groups: [number, string][] };
      start(): object;
      mathAt(text: string, at: number, kind: string, inline?: string): [number, number, string, number];
      boundaries(text: string, l: number, r: number, width: (s: string) => number): number[];
      boundaryAt(xs: number[], x: number): number;
      inkSnap(x: number, ink: number[], reach: number): number;
      locate(texts: string[], row: string, a: number, b: number): [number, number, number][] | null;
      verbatim(text: string): Verb[];
      cells(text: string, at: number): { rows: [number, number][][]; r: number; c: number } | null;
      cellStep(text: string, at: number, dir: number): number | null;
      rowAfter(text: string, at: number): [number, number, string, number] | null;
      verbatimAt(text: string, at: number): Verb | null;
      columns(boxes: Box[], file: string, table: Table, tables: Table[], words?: Word[]): (Placed & { approx?: boolean; measured?: boolean })[];
      measure(words: Word[], box: Box, n: number): ([number, number] | null)[];
      snapWidth(text: string | null, was: number, now: number, line: number, fine: boolean): string | null;
      widthBp(text: string, line: number): number | null;
      respec(align: string, width: string): { body: string; array: boolean } | null;
      wordAt(text: string, at: number): [number, string] | null;
      argAt(text: string, name: string, line: number): number | null;
      apart<B extends Flat>(boxes: B[]): B[];
      relaid(row: [number, number, string][], k: number, now: string, width: (s: string) => number): [number, number, string][];
      tokens(text: string, from: number, to: number): Tok[];
      aligned(toks: string[], words: string[]): number[];
      changed(was: string, now: string): [number, number];
      paragraph(text: string, at: number): [number, number];
      nearest(words: Word[], x: number, y: number): number;
      bands(words: Word[]): Flat[];
      joined(words: Word[]): string;
    };
  }
}
(() => {
  const BP = 72 / 72.27;
  /** The columns of `table` with a width on the pages: the boxes SyncTeX records for its cells (a `p` cell is a
   * vbox as wide as the column), grouped by place and width. A column written as a length takes the group as
   * wide; one computed (`X`, `\dimexpr`) or rewritten since the build the fullest group between its neighbours. Only boxes
   * inside the table's own, none inside a nested table's. */
  function place(boxes: Box[], file: string, table: Table, tables: Table[]): Placed[] {
    const [b, e] = table.lines, inner = tables.filter((o) => o !== table && o.lines[0] >= b && o.lines[1] <= e);
    // a column's cells stand inside the table's box
    const box = extent(boxes, file, table);
    if (!box) return [];
    const within = (r: Box, o: Box) => r.y - r.h >= o.y - o.h - 1 && r.y + r.d <= o.y + o.d + 1 && r.x >= o.x - 1 && r.x + r.w <= o.x + o.w + 1;
    // a nested table's own cells: strictly narrower than its box, inside it (its box may share a line with the outer row)
    const nested = inner.map((o) => extent(boxes, file, o)).filter((o): o is Box => !!o);
    const groups = new Map<string, Placed>();
    for (const r of boxes) {
      if (r.kind !== '[' || r.file !== file || r.line < b || r.line > e || r.w <= 0 || r.w >= box.w - 1 || !within(r, box)) continue;
      if (inner.some((o) => r.line > o.lines[0] && r.line < o.lines[1]) || nested.some((o) => r.w < o.w - 0.5 && within(r, o))) continue;
      const key = `${r.page}|${Math.round(r.x * 2)}|${Math.round(r.w * 2)}`, g = groups.get(key);
      if (g) { g.n++; g.t = Math.min(g.t, r.y - r.h); g.d = Math.max(g.d, r.y + r.d); } else groups.set(key, { k: -1, page: r.page, x: r.x, w: r.w, t: r.y - r.h, d: r.y + r.d, n: 1 });
    }
    const all = [...groups.values()], best = (gs: Placed[]) => gs.sort((p, q) => q.n - p.n || p.x - q.x)[0];
    const found: (Placed | undefined)[] = table.cols.map(() => undefined);
    const pt = (c: Table['cols'][number]) => { const f = form(c.width); return f?.kind === 'len' ? self.satexLayers.length(`${f.num}${f.unit}`)?.pt : undefined; };
    // the lengths first, in order, then the computed ones between them
    let after = -Infinity;
    table.cols.forEach((c, k) => {
      const want = pt(c);
      if (want == null) return;
      const g = best(all.filter((g) => g.x > after - 0.5 && Math.abs(g.w - want * BP) < 0.6));
      if (g) { found[k] = { ...g, k }; after = g.x + g.w; }
    });
    table.cols.forEach((c, k) => {
      if (found[k] || !(c.align === 'X' || c.width)) return;
      const lo = found.slice(0, k).reduce((m, g) => g ? g.x + g.w : m, -Infinity), hi = found.slice(k + 1).find(Boolean)?.x ?? Infinity;
      const g = best(all.filter((g) => g.x > lo - 0.5 && g.x + g.w < hi + 0.5 && !found.some((f) => f && f.x === g.x && f.w === g.w)));
      if (g) found[k] = { ...g, k };
    });
    return found.filter((g): g is Placed => !!g);
  }
  /** Every column of `table` on the pages: placed ones as `place` finds them, the others (`l`, `c`, `r`, whose cells
   * SyncTeX records no box for) approximate, `approx`: the room between their placed neighbours (else the table's
   * edges) shared evenly. A lie where cells differ in width, good enough to point at and to retype a column's letter. */
  function columns(boxes: Box[], file: string, table: Table, tables: Table[], words: Word[] = []): (Placed & { approx?: boolean; measured?: boolean })[] {
    const box = extent(boxes, file, table);
    if (!box) return [];
    const got = new Map(place(boxes, file, table, tables).map((g) => [g.k, g]));
    const ink = measure(words, box, table.cols.length);
    const out: (Placed & { approx?: boolean; measured?: boolean })[] = [];
    for (let k = 0; k < table.cols.length;) {
      if (got.has(k)) { out.push(got.get(k)!); k++; continue; }
      const m = ink[k];
      if (m) { out.push({ k, page: box.page, x: m[0], w: m[1] - m[0], t: box.y - box.h, d: box.y + box.d, n: 0, measured: true }); k++; continue; }
      let e = k;
      while (e < table.cols.length && !got.has(e) && !ink[e]) e++;
      const lo = got.get(k - 1), hi = got.get(e), l = lo ? lo.x + lo.w : box.x, r = hi ? hi.x : box.x + box.w, w = (r - l) / (e - k);
      for (let j = k; j < e; j++) out.push({ k: j, page: box.page, x: l + (j - k) * w, w, t: box.y - box.h, d: box.y + box.d, n: 0, approx: true });
      k = e;
    }
    return out;
  }
  /** Each column's ink on the page, left and right in bp, from the words inside `box`: the rows (one baseline) whose
   * words fall into exactly `n` runs apart by more than a word space give each run's extent to its column; a row with
   * fewer runs (a `\multicolumn`, an empty cell) tells nothing. Null where no row tells. */
  function measure(words: Word[], box: Box, n: number): ([number, number] | null)[] {
    const out: ([number, number] | null)[] = Array.from({ length: n }, () => null), rows = new Map<number, Word[]>();
    const inside = words.filter((w) => w[0] === box.page && w[1] >= box.x - 1 && w[3] <= box.x + box.w + 1 && w[4] >= box.y - box.h - 1 && w[4] <= box.y + box.d + 1);
    for (const w of inside) { const key = [...rows.keys()].find((y) => Math.abs(y - w[4]) < 2) ?? w[4]; rows.set(key, [...(rows.get(key) ?? []), w]); }
    // a word space is some 3.3bp, the gap between two cells at least 2\tabcolsep less what the cells' alignment leaves
    const GAP = 7;
    for (const row of rows.values()) {
      row.sort((a, b) => a[1] - b[1]);
      const runs: [number, number][] = [];
      for (const w of row) { const last = runs.at(-1); if (last && w[1] - last[1] <= GAP) last[1] = Math.max(last[1], w[3]); else runs.push([w[1], w[3]]); }
      if (runs.length !== n) continue;
      runs.forEach(([l, r], k) => { const o = out[k]; out[k] = o ? [Math.min(o[0], l), Math.max(o[1], r)] : [l, r]; });
    }
    return out;
  }
  /** A column's letter with a width: `p` keeps its own (`p`, `m`, `b`), `c` and `r` keep their alignment through
   * array's `>{…}`; `array`: whether the text needs that package. Null for a letter that takes no width (`X`, `S`). */
  function respec(align: string, width: string): { body: string; array: boolean } | null {
    if ('pmb'.includes(align) && align) return { body: `${align}{${width}}`, array: align === 'm' || align === 'b' };
    const pre = ({ l: '', c: '>{\\centering\\arraybackslash}', r: '>{\\raggedleft\\arraybackslash}' } as Record<string, string>)[align];
    return pre == null ? null : { body: `${pre}p{${width}}`, array: !!pre };
  }
  /** The width in bp a width as written stands for, `line` the bp of `\linewidth`; null for a form not read. */
  function widthBp(text: string, line: number): number | null {
    const f = form(text);
    if (!f) return null;
    if (f.kind === 'share') return f.num * line;
    const len = self.satexLayers.length(`${f.num}${f.unit}`);
    return len ? len.pt * BP : null;
  }
  /** The width a drag to `now` bp writes, `text` the width as written (null: a column with none yet, `was` bp wide on
   * the page) and `line` the bp of `\linewidth`. A share stays a share; near a twentieth of the line it becomes one
   * (`.25\linewidth`); else 0.5 mm steps (Shift: 0.1 mm) in cm, or mm where written so; another unit keeps its own. */
  function snapWidth(text: string | null, was: number, now: number, line: number, fine: boolean): string | null {
    const f = text ? form(text) : null;
    if (text && !f || now <= 0) return null;
    if (f?.kind === 'share') return resized(text!, was, now, fine);
    const MM = 72 / 25.4;
    if (!fine && line > 0) {
      const share = Math.round(now / line * 20) / 20;
      if (share > 0 && share <= 1 && Math.abs(share * line - now) <= 0.5 * MM) return share === 1 ? '\\linewidth' : `${String(+share.toFixed(2)).replace(/^0\./, '.')}\\linewidth`;
    }
    if (f && !/^(cm|mm)$/i.test(f.unit)) return resized(text!, was, now, fine);
    const mm = Math.max(fine ? 0.1 : 0.5, Math.round(now / MM / (fine ? 0.1 : 0.5)) * (fine ? 0.1 : 0.5));
    return f?.unit.toLowerCase() === 'mm' ? `${+mm.toFixed(1)}mm` : `${+(mm / 10).toFixed(fine ? 2 : 3)}cm`;
  }
  /** The box of the whole table: the widest SyncTeX records on its `\end` line. */
  const extent = (boxes: Box[], file: string, table: Table) => boxes.filter((r) => r.file === file && r.line === table.lines[1] && '[('.includes(r.kind) && r.w > 0 && r.h + r.d > 0).sort((p, q) => q.w - p.w)[0] ?? null;
  /** A width a drag may rewrite: `3.26cm`, `0.3\linewidth`, `\linewidth`; null for anything else (`\dimexpr`…). */
  function form(text: string): Form | null {
    const len = /^\s*(\d*\.?\d+)\s*(pt|pc|in|bp|cm|mm|dd|cc|em|ex)\s*$/i.exec(text ?? '');
    if (len) return { kind: 'len', num: +len[1], unit: len[2] };
    const share = /^\s*(\d*\.?\d+)?\s*\\(linewidth|textwidth|columnwidth|hsize)\s*$/.exec(text ?? '');
    return share ? { kind: 'share', num: share[1] ? +share[1] : 1, name: share[2], lead: !share[1] || !share[1].startsWith('.') } : null;
  }
  /** The steps a drag snaps to, per unit (`fine`: Shift held). */
  const STEP: Record<string, [number, number]> = { cm: [0.1, 0.01], mm: [1, 0.1], in: [0.05, 0.01], pt: [1, 0.1], bp: [1, 0.1], pc: [0.5, 0.1], dd: [1, 0.1], cc: [0.1, 0.01], em: [0.5, 0.1], ex: [0.5, 0.1], share: [0.01, 0.001] };
  /** The width `text` (`was` bp wide on the page) rewritten as `now` bp in its own form and unit, snapped; null when it has no form to keep. */
  function resized(text: string, was: number, now: number, fine: boolean): string | null {
    const f = form(text);
    if (!f || was <= 0 || now <= 0) return null;
    const [coarse, small] = STEP[f.kind === 'len' ? f.unit.toLowerCase() : 'share'], step = fine ? small : coarse;
    // what one of its units is on this page: measured, so `em` and `\linewidth` need no font or register
    const v = Math.max(step, Math.round(f.num * now / was / step) * step), digits = Math.max(0, -Math.floor(Math.log10(step) + 1e-9));
    const num = String(+v.toFixed(digits));
    return f.kind === 'len' ? `${num}${f.unit}` : `${f.lead || num.startsWith('1') ? num : num.replace(/^0\./, '.')}\\${f.name}`;
  }
  /** The width `text` one snap step wider (`dir` 1) or narrower (-1), never below one step: a key on a handle. */
  function stepped(text: string, dir: number, fine: boolean): string | null {
    const f = form(text);
    if (!f) return null;
    const step = STEP[f.kind === 'len' ? f.unit.toLowerCase() : 'share'][fine ? 1 : 0];
    return resized(text, f.num, Math.max(f.num + dir * step, step), fine);
  }
  /** The width a table may take, in bp: the line its box sits on (a minipage's, a list item's), else null. */
  function room(boxes: Box[], file: string, table: Table): number | null {
    const t = extent(boxes, file, table);
    const line = t && boxes.filter((r) => r.kind === '(' && r.page === t.page && r.w > t.w + 0.5 && r.x <= t.x + 0.5 && r.x + r.w >= t.x + t.w - 0.5
      && r.y - r.h <= t.y - t.h + 0.5 && r.y + r.d >= t.y + t.d - 0.5).sort((p, q) => p.w - q.w)[0];
    return line ? line.w : null;
  }
  /** Whether typed text is plain: no character TeX reads as markup. */
  const plain = (text: string) => !/[\\{}$&#^_~%\r\n]/.test(text);
  /** The math environments of LaTeX and amsmath: their body is math, not text. */
  const MATH = /^(equation|align|gather|multline|flalign|alignat|eqnarray|math|displaymath|dmath|split)\*?$/;
  /** The environments whose body TeX reads as characters, not markup. */
  const VERB_ENV = /^(verbatim|Verbatim|BVerbatim|LVerbatim|lstlisting|minted|filecontents|comment)\*?$/;
  /** The verbatim bodies of `text`, in order: environments (`\begin{lstlisting}[…]`, `\begin{minted}{lang}`) and inline
   * `\verb|…|`, `\verb*|…|`, `\lstinline[…]|…|` or `{…}`, `\mintinline{lang}|…|`. Comments are skipped. Once per text. */
  let verbs: { of: string; spans: Verb[] } = { of: '', spans: [] };
  function verbatim(text: string): Verb[] {
    if (verbs.of === text) return verbs.spans;
    const spans: Verb[] = [], re = /%|\\begin\s*\{([^}]*)\}|\\(verb\*?|lstinline|mintinline)(?![A-Za-z@])/g;
    for (let m: RegExpExecArray | null; (m = re.exec(text));) {
      if (m[0] === '%') { if (text[m.index - 1] === '\\') continue; const nl = text.indexOf('\n', m.index); re.lastIndex = nl < 0 ? text.length : nl; continue; }
      let i = re.lastIndex;
      if (m[1] != null) {
        if (!VERB_ENV.test(m[1])) continue;
        // its options and minted's language
        const args = /^(\s*\[[^\]\n]*\])?(\s*\{[^}\n]*\})?/.exec(text.slice(i))!;
        i += m[1].startsWith('minted') ? args[0].length : (args[1]?.length ?? 0);
        const end = text.indexOf(`\\end{${m[1]}}`, i), to = end < 0 ? text.length : end;
        spans.push({ from: i, to, block: true, close: `\\end{${m[1]}}` });
        re.lastIndex = to;
        continue;
      }
      const opt = m[2] === 'verb*' || m[2] === 'verb' ? null : /^\s*\[[^\]\n]*\]/.exec(text.slice(i));
      if (opt) i += opt[0].length;
      if (m[2] === 'mintinline') { const lang = /^\{[^}\n]*\}/.exec(text.slice(i)); if (lang) i += lang[0].length; }
      const d = text[i];
      if (!d || /[\s*]/.test(d) && m[2].startsWith('verb')) continue;
      const close = d === '{' && !m[2].startsWith('verb') ? '}' : d, end = text.indexOf(close, i + 1), nl = text.indexOf('\n', i + 1);
      if (end < 0 || nl >= 0 && nl < end) continue;
      spans.push({ from: i + 1, to: end, block: false, close });
      re.lastIndex = end + 1;
    }
    verbs = { of: text, spans };
    return spans;
  }
  /** The verbatim body `at` stands in (its ends included), null outside one. */
  const verbatimAt = (text: string, at: number) => verbatim(text).find((v) => v.from <= at && at <= v.to) ?? null;
  /** The table `at` stands in: its rows of cells, each cell's text `[from, to]` without the spaces and rules around
   * it, and the row and cell of `at`. Cells part at `&`, rows at `\\`, both at the body's own level (a `{…}`
   * holds its own). Null outside a tabular, array, tabularx, longtable or tabulary body. */
  function cells(text: string, at: number) {
    const re = /\\begin\s*\{(tabular\*?|tabularx|tabulary|longtable|array)\}/g;
    let body: [number, number] | null = null;
    for (let m: RegExpExecArray | null; (m = re.exec(text)) && m.index < at;) {
      // the width (tabular*, tabularx) and the column spec: braced groups after the name, an optional position between
      let i = re.lastIndex;
      const group = () => { let d = 0, j = i; for (; j < text.length; j++) { if (text[j] === '{') d++; else if (text[j] === '}' && --d === 0) return j + 1; else if (d === 0 && !/\s/.test(text[j]!)) return i; } return i; };
      if (/\*|x|y/.test(m[1]!.slice(-1)) && m[1] !== 'array') i = group();
      const pos = /^\s*\[[^\]]*\]/.exec(text.slice(i)); if (pos) i += pos[0].length;
      i = group();
      const end = text.indexOf(`\\end{${m[1]}}`, i);
      if (end >= at && i <= at) body = [i, end];
    }
    if (!body) return null;
    const rows: [number, number][][] = [[]], raw: [number, number, number, number][] = [];
    const RULE = /^(\s|\\(hline|toprule|midrule|bottomrule|cline\s*\{[^}]*\}|cmidrule\s*(\([^)]*\))?\s*\{[^}]*\}|addlinespace|endhead|endfirsthead|endfoot|endlastfoot)(?![A-Za-z])|%[^\n]*\n)+/;
    let from = body[0], depth = 0;
    const put = (to: number) => {
      const lead = RULE.exec(text.slice(from, to))?.[0].length ?? 0, a = Math.min(to, from + lead), b = a + text.slice(a, to).trimEnd().length;
      raw.push([from, to, rows.length - 1, rows.at(-1)!.length]);
      rows.at(-1)!.push([a, b]);
    };
    for (let i = body[0]; i < body[1]; i++) {
      const c = text[i];
      if (c === '%') { while (i < body[1] && text[i] !== '\n') i++; continue; }
      if (c === '\\') {
        if (depth === 0 && text[i + 1] === '\\') { put(i); rows.push([]); i++; const opt = /^\*?\s*\[[^\]]*\]/.exec(text.slice(i + 1)); if (opt) i += opt[0].length; from = i + 1; } else i++;
        continue;
      }
      if (c === '{') depth++; else if (c === '}') depth--;
      else if (c === '&' && depth === 0) { put(i); from = i + 1; }
    }
    put(body[1]);
    // a last row of rules alone is no row
    const last = rows.at(-1)!;
    if (rows.length > 1 && last.length === 1 && last[0]![0] === last[0]![1]) { rows.pop(); raw.pop(); }
    const hit = raw.find(([a, b]) => a <= at && at <= b) ?? raw.at(-1)!, [, , r, c] = hit;
    return { rows, r, c };
  }
  /** Tab (`dir` 1) and Shift+Tab (-1) in a table: the end of the next or previous cell's text, row over row; null past either end. */
  function cellStep(text: string, at: number, dir: number): number | null {
    const t = cells(text, at);
    if (!t) return null;
    const flat = t.rows.flat(), k = flat.indexOf(t.rows[t.r]![t.c]!) + dir;
    return flat[k] ? flat[k]![1] : null;
  }
  /** Enter in a table: a new row after the caret's, as many cells as it has, the caret in its first. */
  function rowAfter(text: string, at: number): [number, number, string, number] | null {
    const t = cells(text, at);
    if (!t) return null;
    const row = t.rows[t.r]!, end = row.at(-1)![1], amp = ' &'.repeat(row.length - 1);
    // the row's own `\\`, if it has one: the new row goes on the line after it
    const sep = /^\s*\\\\(\*?\s*\[[^\]]*\])?/.exec(text.slice(end));
    if (sep) { const at2 = end + sep[0].length; return [at2, at2, `\n${amp} \\\\`, 1]; }
    return [end, end, ` \\\\\n${amp}`, 4];
  }
  /** A structural token of the source: `{`, `}`, `$`, `$$`, `&`, `#`, `_`, `^`, `par` (a blank line), `%` (a comment),
   * `verb` (a verbatim body), `\begin{env}`/`\end{env}` with its name, or a control sequence as written (`\emph`, `\%`). */
  type Mark = { s: number; e: number; t: string };
  /** Where a scan stands: the open math (its opener or environment, '' outside), the open groups (the `{`'s offset
   * and the command before it), the command whose `[…]` option is open, the command just read, the last token. */
  type Scan = { math: string; groups: [number, string][]; opt: string | null; optAt: number; cmd: string; last: Mark | null };
  const start = (): Scan => ({ math: '', groups: [], opt: null, optAt: 0, cmd: '', last: null });
  const copy = (s: Scan): Scan => ({ ...s, groups: [...s.groups] });
  /** Reads `text` from `from` to `to` the way TeX tokenizes it, from state `st` (changed in place, returned); the
   * tokens go to `out`. A token begun before `to` is read whole. */
  function scan(text: string, from: number, to: number, st: Scan, out?: Mark[]): Scan {
    const skip = verbatim(text);
    const put = (s: number, e: number, t: string) => { st.last = { s, e, t }; out?.push(st.last); };
    let k = 0;
    for (let i = from; i < to; i++) {
      while (skip[k] && skip[k]!.to < i) k++;
      const v = skip[k];
      if (v && v.from <= i) { put(v.from, v.to, 'verb'); i = v.to - 1; k++; st.cmd = ''; continue; }
      const c = text[i]!;
      if (c === '%') { const nl = text.indexOf('\n', i), e = nl < 0 ? text.length : nl; put(i, e, '%'); i = e - 1; continue; }
      if (c === '\\') {
        const m = /^\\(?:([A-Za-z@]+)|([\s\S]))?/.exec(text.slice(i, i + 64))!;
        let e = i + m[0].length, t = m[0];
        if (m[1] === 'begin' || m[1] === 'end') {
          const env = /^\s*\{([^{}\n]*)\}/.exec(text.slice(e, e + 80));
          if (env) { e += env[0].length; t = `\\${m[1]}{${env[1]}}`; if (MATH.test(env[1]!) && (m[1] === 'begin' ? !st.math : st.math === env[1])) st.math = m[1] === 'begin' ? env[1]! : ''; }
        } else if ((m[2] === '(' || m[2] === '[') && !st.math) st.math = t;
        else if ((m[2] === ')' || m[2] === ']') && st.math === `\\${m[2] === ')' ? '(' : '['}`) st.math = '';
        put(i, e, t);
        st.cmd = m[1] ?? '';
        i = e - 1;
        continue;
      }
      if (c === '$') {
        // `$$` opens display math where no math is open, closes it in display math; inline math ends at one `$`
        const two = text[i + 1] === '$' && (st.math === '$$' || !st.math), t = two ? '$$' : '$';
        if (!st.math) st.math = t; else if (st.math === t) st.math = '';
        put(i, i + t.length, t); i += t.length - 1; st.cmd = '';
        continue;
      }
      if (c === '\n') { const p = /^\n[ \t]*\n(?:[ \t]*\n)*/.exec(text.slice(i, i + 256)); if (p) { put(i, i + p[0].length, 'par'); i += p[0].length - 1; st.cmd = ''; } continue; }
      if (/\s/.test(c) || c === '*' && st.cmd) continue;
      if (c === '{') { put(i, i + 1, c); st.groups.push([i, st.cmd]); }
      else if (c === '}') { put(i, i + 1, c); st.groups.pop(); }
      else if (/[&#_^]/.test(c)) put(i, i + 1, c);
      else if (c === '[' && st.cmd && st.opt == null) { st.opt = st.cmd; st.optAt = st.groups.length; st.cmd = ''; continue; }
      else if (c === ']' && st.opt != null && st.groups.length === st.optAt) { st.cmd = st.opt; st.opt = null; continue; }
      st.cmd = '';
    }
    return st;
  }
  /** Groups whose text is a name, not output: keys, files, packages, environments' arguments. */
  const KEYS = /^(label|ref|eqref|pageref|autoref|[cC]ref|nameref|cite\w*|nocite|input|include|includegraphics|usepackage|documentclass|bibliography(style)?|url)$/;
  /** Why `text`'s characters `from..to` may not be edited as plain text, null when they may. The file is read up to
   * them: math (`$`, `$$`, `\(`, `\[`, a math environment), a command's name or option, an environment's name, a
   * comment, a key (`keys`: the run's label, reference and citation spans; else a `\label`, `\ref`, `\cite`… argument). */
  function editable(text: string, from: number, to: number, keys: [number, number][]): string | null {
    if (from < 0 || to > text.length || from > to) return 'it is outside the text';
    // verbatim: every character is text; an inline body holds one line and not its closer
    const v = verbatimAt(text, from);
    if (v && to <= v.to) return v.block || !/[\n]/.test(text.slice(from, to)) && !text.slice(from, to).includes(v.close) ? null : 'it ends the verbatim text';
    if (!plain(text.slice(from, to))) return 'the text there holds markup';
    if (keys.some(([a, b]) => a < to && from < b || a === from && b === to)) return 'it is a key (a label, a reference, a citation)';
    const st = scan(text, 0, from, start()), l = st.last;
    if (l && l.s < from && (l.t === '%' ? l.e >= from : l.e > from)) return l.t === '%' ? 'it is a comment' : /^\\(begin|end)\{/.test(l.t) ? "it is an environment's name" : "it is a command's name";
    if (st.math) return 'it is math';
    const g = st.groups.at(-1)?.[1] ?? '';
    if (/^(begin|end)$/.test(g)) return "it is an environment's argument";
    if (KEYS.test(g)) return 'it is a key (a label, a reference, a citation)';
    if (st.opt != null && (KEYS.test(st.opt) || /^(begin|end)$/.test(st.opt))) return "it is a command's option";
    return null;
  }
  /** What a `{` at `at` follows, with only spaces between: a command, a `]`, `}`, `_` or `^` (it is an argument),
   * as `[offset, what]`; null where it opens a plain group. */
  function argOf(text: string, at: number): [number, string] | null {
    const m = /(?:\\[A-Za-z@]+\*?|\\[^A-Za-z@]|[\]}_^])\s*$/.exec(text.slice(Math.max(0, at - 64), at));
    return m ? [at - m[0].length, m[0].trimEnd()] : null;
  }
  /** Control symbols and words that are escaped characters, deleted whole as the character they show. */
  const ESC = /^\\([^A-Za-z@()[\]\\]|textbackslash|textasciitilde|textasciicircum)$/;
  /** Whether the tokens `ms`, in order, open and close as many groups, formulas and environments as they close. */
  function balanced(ms: Mark[]): boolean {
    let br = 0, dl = 0, dd = 0, p = 0, q = 0;
    const env: string[] = [];
    for (const m of ms) {
      const t = m.t;
      if (t === '{') br++; else if (t === '}' && --br < 0) return false;
      else if (t === '$') dl ^= 1; else if (t === '$$') dd ^= 1;
      else if (t === '\\(') p++; else if (t === '\\)' && --p < 0) return false;
      else if (t === '\\[') q++; else if (t === '\\]' && --q < 0) return false;
      else if (t.startsWith('\\begin{')) env.push(t.slice(7));
      else if (t.startsWith('\\end{') && env.pop() !== t.slice(5)) return false;
    }
    return !br && !dl && !dd && !p && !q && !env.length;
  }
  /** What a caret edit may know: the text typed since the caret was put (`fresh`, `[from, to)`: commands typed there
   * may still change), the run's key spans, whether it adds a table row (`row`) or is a snippet (`free`). */
  type Opts = { fresh?: [number, number] | null; keys?: [number, number][]; row?: boolean; free?: boolean; inline?: string; smart?: boolean };
  /** Why edit `[from, to, body]` of `text` would change its structure, null when it keeps it. Kept: every brace,
   * command name, environment, math delimiter, comment and verbatim body around it, each argument with its command;
   * what it removes or adds is balanced, removes only escapes and text, adds no comment, no table cell outside a row
   * edit, no script outside math, no paragraph inside an argument. Commands typed since the caret was put (`o.fresh`)
   * may change. The one gate of every caret edit: the drawn text and the written one pass it alike. */
  function checked(text: string, edit: [number, number, string, number?], o: Opts = {}): string | null {
    const [a, b, body] = edit, n = body.length, d = n - (b - a), f = o.fresh ?? null;
    if (a === b && !n) return null;
    if (a < 0 || b > text.length || a > b) return 'it is outside the text';
    const now = text.slice(0, a) + body + text.slice(b);
    const v = verbatimAt(text, a);
    if (v) {
      const ok = b <= v.to && verbatim(now).length === verbatim(text).length && verbatimAt(now, a)?.to === v.to + d && (v.block || !/\n/.test(body));
      return ok ? null : 'it ends the verbatim text';
    }
    if (o.keys?.some(([p, q]) => p < b && a < q || p < a && a < q)) return 'it is a key (a label, a reference, a citation)';
    // the lines around the edit, blank lines beside it too (a blank line, a comment's end may join it)
    let p0 = a, q0 = b;
    while (p0 > 0 && /\s/.test(text[p0 - 1]!)) p0--;
    while (q0 < text.length && /\s/.test(text[q0]!)) q0++;
    const L = text.lastIndexOf('\n', p0 - 1) + 1, nl = text.indexOf('\n', q0), R = nl < 0 ? text.length : nl;
    const head = scan(text, 0, L, start()), ctx = scan(text, L, a, copy(head)), l = ctx.last;
    if (l && l.t === '%' && l.s < a && l.e >= a) return 'it is a comment';
    const g = ctx.groups.at(-1)?.[1] ?? '';
    if (KEYS.test(g) || /^(begin|end)$/.test(g)) return g === 'begin' || g === 'end' ? "it is an environment's argument" : 'it is a key (a label, a reference, a citation)';
    if (ctx.opt != null && (KEYS.test(ctx.opt) || /^(begin|end)$/.test(ctx.opt))) return "it is a command's option";
    const m1: Mark[] = [], m2: Mark[] = [];
    scan(text, L, R, copy(head), m1);
    scan(now, L, R + d, copy(head), m2);
    // a token of the text before the edit, where it stands after it; -1 where the edit cuts it
    const moved = (m: Mark) => m.e <= a ? m.s : m.s >= b ? m.s + d : -1, id = (s: number, m: Mark) => `${s}:${m.e - m.s}:${m.t}`;
    const was = new Set(m1.map((m) => id(moved(m), m))), is = new Set(m2.map((m) => id(m.s, m)));
    const gone = m1.filter((m) => moved(m) < 0 || !is.has(id(moved(m), m))), added = m2.filter((m) => !was.has(id(m.s, m)));
    const fr = (p: number) => !!f && f[0] <= p && p < f[1];
    // each character of a token removed (added) is one the edit removes (adds) or one typed since the caret was put
    const ownWas = (m: Mark) => { for (let p = m.s; p < m.e; p++) if (!(a <= p && p < b) && !fr(p)) return false; return true; };
    const ownNow = (m: Mark) => { for (let p = m.s; p < m.e; p++) if (!(a <= p && p < a + n) && !fr(p < a ? p : p - d)) return false; return true; };
    const freshWas = (m: Mark) => !!f && f[0] <= m.s && m.e <= f[1];
    // typed or not, what goes is balanced: a `\)` typed before goes only with its `\(`
    if (!balanced(gone)) return 'it would unbalance the braces or the math around it';
    if (!balanced(added)) return 'it would unbalance the braces or the math';
    for (const m of gone) {
      if (m.t === 'par' || freshWas(m)) continue;
      if (!ownWas(m)) return m.t[0] === '\\' ? `it would change the command ${m.t}` : 'it would change the markup around it';
      if (m.t[0] === '\\' && !ESC.test(m.t)) return `it would delete the command ${m.t}`;
      if (/^(%|verb|&|#)$/.test(m.t)) return 'it would delete markup';
      // an argument's braces go only with their command
      const arg = m.t === '{' ? argOf(text, m.s) : null;
      if (arg && !(a <= arg[0] && arg[0] < b) && !fr(arg[0])) return `it would delete an argument of ${arg[1]}`;
    }
    for (const m of added) {
      if (!ownNow(m)) return m.t[0] === '\\' ? `it would change a command's name (${m.t})` : 'it would change the markup around it';
      if (m.t === '%') return 'it would start a comment';
      if (m.t === 'verb') return 'it would start verbatim text';
      if (m.t === 'par' && !o.free && (ctx.groups.length || ctx.math)) return ctx.math ? 'a paragraph cannot end in math' : `a paragraph cannot end inside ${g ? `\\${g}{…}` : 'a group'}: Shift+Enter breaks the line`;
      if ((m.t === '&' || m.t === '\\\\' && cells(now, m.s)) && !o.row && !o.free) return 'it would add a table cell or row: Tab and Enter do';
      if (m.t === '#' && !o.free) return 'a # is a macro parameter';
      if ((m.t === '_' || m.t === '^') && !ctx.math && !o.free) return 'a script belongs in math';
      if (ctx.math && /^(\\[([]|\$\$?)$/.test(m.t)) return 'math cannot open inside math';
    }
    // a brace kept stays an argument of what it followed; one added after a command already there is its `{}` end
    for (const m of m2) {
      if (m.t !== '{') continue;
      const p = m.s < a ? m.s : m.s >= a + n ? m.s - d : -1, now1 = argOf(now, m.s);
      if (p >= 0 && was.has(id(m.s, m))) {
        const then = argOf(text, p);
        if ((then?.[1] ?? '') !== (now1?.[1] ?? '')) return then ? `it would part ${then[1]} from its argument` : 'it would make a group an argument';
      } else if (now1 && /^\\[A-Za-z@]/.test(now1[1]) && !(a <= now1[0] && now1[0] < a + n) && !fr(now1[0] < a ? now1[0] : now1[0] - d) && now[m.s + 1] !== '}') {
        return `it would give ${now1[1]} an argument`;
      }
    }
    return null;
  }
  /** The fresh span after edit `[from, to)` → `n` characters: grown by an edit in or at it, else the edit's own. */
  function grow(f: [number, number] | null | undefined, a: number, b: number, n: number): [number, number] | null {
    if (f && a >= f[0] && b <= f[1]) return [f[0], f[1] + n - (b - a)];
    if (n) return [a, a + n];
    return f && b <= f[0] ? [f[0] - (b - a), f[1] - (b - a)] : f && a >= f[1] ? f : null;
  }
  /** Where `word` (a word of the page, edge punctuation left out, ligatures `ﬁ` as letters) stands whole in a source line, each occurrence; none after a backslash. */
  function spans(line: string, word: string): [number, number][] {
    const bare = word.normalize('NFKC').replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, '');
    if (!bare) return [];
    const re = new RegExp(`(?<![\\p{L}\\p{N}\\\\])${bare.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\p{L}\\p{N}])`, 'gu');
    return [...line.matchAll(re)].map((m) => [m.index, m.index + bare.length]);
  }
  /** A character typed as text, written the LaTeX way so the output shows it: `_` as `\_`, `\` as `\textbackslash{}`. */
  const ESCAPED: Record<string, string> = { '\\': '\\textbackslash{}', '~': '\\textasciitilde{}', '^': '\\textasciicircum{}' };
  const escaped = (c: string) => /[\\&%#_{}~^]/.test(c) ? ESCAPED[c] ?? `\\${c}` : c;
  /** The inline math delimiters `text` uses most: `\(` where it has more `\(` than `$`, else `$`. */
  const inlineOf = (text: string) => (text.match(/\\\(/g)?.length ?? 0) > (text.match(/(?<![\\$])\$(?!\$)/g)?.length ?? 0) ? '\\(' : '$';
  /** A command name ending at `at` (an even run of backslashes before its `\`): its offset, else -1. */
  const nameEnd = (text: string, at: number) => { const m = /(?:^|[^\\])(?:\\\\)*(\\[A-Za-z@]+)$/.exec(text.slice(Math.max(0, at - 64), at)); return m ? at - m[1]!.length : -1; };
  /** A lone `\` just before `at` (an odd run of backslashes): a command being typed. */
  const lone = (text: string, at: number) => /(?:^|[^\\])(?:\\\\)*\\$/.test(text.slice(Math.max(0, at - 64), at));
  /** Whether a command's name is being typed at `at`: a lone `\` before it, or a name typed since the caret was put. */
  const naming = (text: string, at: number, fresh: [number, number] | null = null) => { const cs = nameEnd(text, at); return lone(text, at) || cs >= 0 && !!fresh && fresh[0] <= cs && cs < fresh[1]; };
  /** What typing `c` at `at` of `text` writes so the source stays compilable: `[from, to, body, caret]`, the text
   * `from..to` replaced by `body` and the caret `caret` past `from`. In text a special character is escaped and `$`
   * opens math with its closer (`inline`: `\(`, `$` or `auto`, the document's most used). In math a character goes
   * in as typed, `{` with its `}`, `%`, `#`, `&` escaped, a closer typed where it stands steps over it, `$` elsewhere
   * leaves the formula. `\` starts a command (its own space keeps it off what follows); letters name it, another
   * character makes a control symbol (`\%`). After a command typed since the caret was put (`fresh`) `{` opens its
   * argument and `}` steps out of one; after an older one a letter or space gets a `{}` first, so the name stays.
   * `smart`: `x_42` writes `x_{42}`. */
  function typed(text: string, at: number, c: string, inline = '\\(', smart = false, fresh: [number, number] | null = null): [number, number, string, number] {
    // verbatim takes the character as it is; an inline body never its closer or a line break
    const v = verbatimAt(text, at);
    if (v) return !v.block && (c === v.close || c === '\n') ? [at, at, '', 0] : [at, at, c, c.length];
    const fr = (p: number) => !!fresh && fresh[0] <= p && p < fresh[1], letter = /^[A-Za-z@]$/.test(c);
    // the space typed after a `\` (to keep it off the next word) goes once the name is a control symbol
    if (lone(text, at)) return letter ? [at, at, c, 1] : [at, at + (text[at] === ' ' && fr(at) ? 1 : 0), c, 1];
    // its own space, never one of the text's: a letter typed then renames only what was typed
    if (c === '\\') return [at, at, '\\ ', 1];
    const cs = nameEnd(text, at), own = cs >= 0 && fr(cs);
    if (cs >= 0 && letter) return own ? [at, at, c, 1] : [at, at, `{}${c}`, 3];
    // its argument ends the name: the space that kept it off the next word goes
    if (cs >= 0 && c === '{' && own) return [at, at + (text[at] === ' ' && fr(at) ? 1 : 0), '{}', 1];
    if (cs >= 0 && c === ' ' && !own) return [at, at, '{} ', 3];
    if (c === '}' && text[at] === '}' && fr(at)) return [at, at, '', 1];
    const st = scan(text, 0, at, start());
    if (!st.math) {
      if (c !== '$') { const body = escaped(c); return [at, at, body, body.length]; }
      const open = inline === '$' || inline === '\\(' ? inline : inlineOf(text);
      // `$ $`, not `$$`: TeX reads `$$` as display math; the space goes with the first character typed
      return open === '$' ? [at, at, '$ $', 1] : [at, at, '\\(\\)', 2];
    }
    // smart scripts: `_`/`^` open a group, a space, an operator or the formula's end typed at its end leave it
    if (smart && (c === '_' || c === '^')) return [at, at, `${c}{}`, 2];
    if (smart && text[at] === '}' && /[_^]\{[^{}]*$/.test(text.slice(Math.max(0, at - 64), at)) && /[\s$+\-=<>,;:|/*!?()[\]]/.test(c)) {
      // past the `}` first, then the character as typed there (a space only leaves)
      return /\s/.test(c) ? [at, at, '', 1] : typed(text, at + 1, c, inline, smart, fresh);
    }
    const empty = st.math === '$' && text[at - 1] === '$' && text.startsWith(' $', at);
    if (c === '$') {
      if (empty) return [at, at, '', 2];
      for (const close of ['$', '\\)', '\\]']) if (text.startsWith(close, at)) return [at, at, '', close.length];
      // elsewhere in inline math: past the formula's closer
      const end = st.math === '$' ? /(?<!\\)\$/.exec(text.slice(at)) : st.math === '\\(' ? /\\\)/.exec(text.slice(at)) : null;
      return end ? [at, at, '', end.index + end[0].length] : [at, at, '', 0];
    }
    const body = c === '{' ? '{}' : /[%#&}]/.test(c) ? `\\${c}` : c;
    return [at, at + (empty ? 1 : 0), body, c === '{' ? 1 : body.length];
  }
  /** The math control at `at`: `[from, to, body, caret]` like typed's. Inline math in the `inline` convention's
   * delimiters, display `\[…\]` or an `align*`, the caret inside; in math already nothing. */
  function mathAt(text: string, at: number, kind: string, inline = '\\('): [number, number, string, number] {
    if (scan(text, 0, at, start()).math) return [at, at, '', 0];
    if (kind === 'inline') return typed(text, at, '$', inline);
    const [o, c] = kind === 'align' ? ['\\begin{align*}\n', '\n\\end{align*}'] : ['\\[', '\\]'];
    return [at, at, o + c, o.length];
  }
  /** What Backspace (`back`) or Delete takes at `at`: `[from, to]`, an escape or an empty pair (`$ $`, `{}`, `\(\)`)
   * whole, a formula whose last character goes with it, a `\` typed with the space that kept it off a word. */
  function erased(text: string, at: number, back: boolean, fresh: [number, number] | null = null): [number, number] {
    const v = verbatimAt(text, at);
    if (v) return back ? (at > v.from ? [at - 1, at] : [at, at]) : (at < v.to ? [at, at + 1] : [at, at]);
    for (const pair of ['{}', '\\(\\)', '\\[\\]']) {
      const h = pair.length / 2;
      if (text.slice(at - h, at + h) === pair) return [at - h, at + h];
    }
    for (const k of [1, 2]) if (text.slice(at - k, at - k + 3) === '$ $') return [at - k, at - k + 3];
    let p = back ? at - 1 : at, q = back ? at : at + 1;
    const esc = /(?:\\(?:textbackslash|textasciitilde|textasciicircum)\{\}|\\[^A-Za-z@\\])/y;
    if (back) {
      const m = /(?:\\(?:textbackslash|textasciitilde|textasciicircum)\{\}|(?:^|[^\\])(?:\\\\)*\\[^A-Za-z@\\])$/.exec(text.slice(Math.max(0, at - 20), at));
      if (m) p = at - (m[0].includes('\\text') ? m[0].length : 2);
      else if (lone(text, at) && text[at] === ' ' && fresh && fresh[0] <= at && at < fresh[1]) q = at + 1;
    } else {
      esc.lastIndex = at;
      const m = !lone(text, at) && esc.exec(text);
      if (m) q = at + m[0].length;
    }
    // the only character of a formula takes the formula with it
    if (q - p === 1) {
      if (text[p - 1] === '$' && text[q] === '$' && text[p - 2] !== '\\' && scan(text, 0, p, start()).math === '$') return [p - 1, q + 1];
      if (text.slice(p - 2, p) === '\\(' && text.startsWith('\\)', q)) return [p - 2, q + 2];
    }
    return [Math.max(0, p), Math.min(text.length, q)];
  }
  /** Enter (`shift`: Shift+Enter) at `at`: a line in verbatim, a row in a table, a line break in math, a paragraph
   * in text (a line break with Shift); the reason where none may go. */
  function broken(text: string, at: number, shift: boolean): [number, number, string, number] | string {
    const v = verbatimAt(text, at);
    if (v) return v.block ? [at, at, '\n', 1] : 'inline verbatim holds one line';
    if (!shift && cells(text, at)) return rowAfter(text, at) ?? 'no row here';
    const br = shift || scan(text, 0, at, start()).math ? '\n' : '\n\n';
    return [at, at, br, br.length];
  }
  /** A key at the caret (`Backspace`, `Delete`, `Enter`, `Shift+Enter` or a character): the edit it makes, checked,
   * or why it makes none. */
  function key(text: string, at: number, k: string, o: Opts = {}): [number, number, string, number] | string {
    const e = k === 'Backspace' || k === 'Delete' ? [...erased(text, at, k === 'Backspace', o.fresh ?? null), '', 0] as [number, number, string, number]
      : k === 'Enter' || k === 'Shift+Enter' ? broken(text, at, k === 'Shift+Enter')
        : typed(text, at, k, o.inline, o.smart, o.fresh ?? null);
    if (typeof e === 'string') return e;
    return checked(text, e, { ...o, row: k === 'Enter' }) ?? e;
  }
  /** The entries of a table of contents (`toc`), list of figures (`lof`) or of tables (`lot`), in the order the
   * document sets them from `root` (`\input`, `\include` followed in `texts`): where each entry's text is written,
   * `[file, from, to]`: a sectioning command's or caption's short title (`\section[Short]{Long}`) if it has one, else
   * its title. Starred sections and captions outside their float make none. */
  function entries(texts: Record<string, string>, root: string, kind: string): [string, number, number][] {
    const out: [string, number, number][] = [], seen = new Set<string>();
    const group = (t: string, i: number, open: string, close: string): number => { let d = 0; for (let j = i; j < t.length; j++) { if (t[j] === '\\') { j++; continue; } if (t[j] === open) d++; else if (t[j] === close && --d === 0) return j; } return -1; };
    const walk = (name: string) => {
      const file = name in texts ? name : `${name}.tex` in texts ? `${name}.tex` : null;
      if (!file || seen.has(file)) return;
      seen.add(file);
      const t = texts[file]!, floats: string[] = [];
      const re = /(?<!\\)%[^\n]*|\\(input|include)\s*\{([^}]*)\}|\\(begin|end)\s*\{(figure|table)\*?\}|\\(part|chapter|section|subsection|subsubsection|caption)(?![A-Za-z@])(\*?)/g;
      for (let m: RegExpExecArray | null; (m = re.exec(t));) {
        if (m[0][0] === '%' || verbatim(t).some((v) => v.from <= m!.index && m!.index < v.to)) continue;
        if (m[1]) { walk(m[2]!.trim()); continue; }
        if (m[3]) { if (m[3] === 'begin') floats.push(m[4]!); else floats.pop(); continue; }
        if (m[6] || (m[5] === 'caption' ? kind !== { figure: 'lof', table: 'lot' }[floats.at(-1) ?? ''] : kind !== 'toc')) continue;
        let i = re.lastIndex;
        while (/\s/.test(t[i] ?? '')) i++;
        let short: [number, number] | null = null;
        if (t[i] === '[') { const e = group(t, i, '[', ']'); if (e < 0) continue; short = [i + 1, e]; i = e + 1; while (/\s/.test(t[i] ?? '')) i++; }
        if (t[i] !== '{') continue;
        const e = group(t, i, '{', '}');
        if (e < 0) continue;
        out.push([file, ...(short ?? [i + 1, e])]);
        re.lastIndex = e + 1;
      }
    };
    walk(root);
    return out;
  }
  /** Sends the newest value put, one at a time: a value put while one is under way waits and the latest wins.
   * A send that fails stops there (no retry loop): the next `put` or `flush` sends again. */
  function sender<V>(send: (v: V) => Promise<boolean>) {
    let busy: Promise<void> | null = null, next: { v: V } | null = null;
    const run = (): Promise<void> => {
      if (busy || !next) return busy ?? Promise.resolve();
      const { v } = next;
      next = null;
      busy = send(v).catch(() => false).then((ok) => { busy = null; if (ok && next) return run(); });
      return busy;
    };
    return { put(v: V) { next = { v }; run(); }, async flush() { while (busy) await busy; await run(); } };
  }
  /** The word around offset `at` of `text` (letters and digits, the caret at its edge counts): its start and itself. */
  function wordAt(text: string, at: number): [number, string] | null {
    let a = at, b = at;
    while (a > 0 && /[\p{L}\p{N}]/u.test(text[a - 1]!)) a--;
    while (b < text.length && /[\p{L}\p{N}]/u.test(text[b]!)) b++;
    return a < b ? [a, text.slice(a, b)] : null;
  }
  /** Where the first argument of a use of macro `name` (`\greet`) starts, on line `line` (1-based) or a neighbour. */
  function argAt(text: string, name: string, line: number): number | null {
    const starts = [0];
    for (let i = text.indexOf('\n'); i >= 0; i = text.indexOf('\n', i + 1)) starts.push(i + 1);
    const re = new RegExp(`${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![a-zA-Z@])\\s*\\{`, 'g');
    for (const n of [line, line + 1, line - 1]) {
      const a = starts[n - 1];
      if (a === undefined) continue;
      const m = re.exec(text.slice(a, starts[n] ?? text.length));
      re.lastIndex = 0;
      if (m) return a + m.index + m[0].length;
    }
    return null;
  }
  /** A row's words (`[left, right, text]`) after word `k` became `now`: it takes `width(now)` scaled as the old
   * word was set, the words after it move by the difference, the gaps stay. */
  function relaid(row: [number, number, string][], k: number, now: string, width: (s: string) => number): [number, number, string][] {
    const [l, r, was] = row[k]!, scale = width(was) > 0 ? (r - l) / width(was) : 0, d = width(now) * scale - (r - l);
    return row.map(([a, b, t], i) => i < k ? [a, b, t] : i === k ? [a, b + d, now] : [a + d, b + d, t]);
  }
  /** The words a paragraph's source `from..to` sets, by offset: runs between spaces (`~` too), commands, braces and
   * math shifts left out; a comment is no word. A word spelled by markup alone (`\\par`, `{}`) sets nothing. */
  function tokens(text: string, from: number, to: number): Tok[] {
    const out: Tok[] = [], re = /(?<!\\)%[^\n]*|(?:\\%|[^\s~%])+/g;
    re.lastIndex = from;
    for (let m; (m = re.exec(text)) && m.index < to;) {
      if (m[0][0] === '%') continue;
      const t = m[0].replace(/\\[A-Za-z@]+\*?|\\(?=[^A-Za-z@])|[{}$^_]/g, '');
      if (t) out.push({ s: m.index, e: m.index + m[0].length, t });
    }
    return out;
  }
  /** Source words `toks` matched to the page's words `words` (both in reading order): per word of the source, the
   * index of the page word that shows it, -1 where none does. Equal spellings pair up in order; one missing or extra
   * on either side (a macro's text, a dropped command) is skipped by looking a few words ahead; a word the page
   * hyphenated across rows (`consec-` `tetur`) is its first part. */
  function aligned(toks: string[], words: string[]): number[] {
    const n = (t: string) => t.normalize('NFKC').replace(/[\u2010-\u2014]/g, '-'), A = toks.map(n), B = words.map(n), out = A.map(() => -1);
    const eq = (i: number, j: number) => A[i] === B[j] || B[j]!.endsWith('-') && j + 1 < B.length && A[i] === B[j]!.slice(0, -1) + B[j + 1];
    const LOOK = 4;
    for (let i = 0, j = 0; i < A.length && j < B.length;) {
      if (eq(i, j)) { out[i] = j; j += A[i] === B[j] ? 1 : 2; i++; continue; }
      let step: [number, number] | null = null;
      for (let k = 1; k <= LOOK && !step; k++) step = j + k < B.length && eq(i, j + k) ? [0, k] : i + k < A.length && eq(i + k, j) ? [k, 0] : null;
      if (step) { i += step[0]; j += step[1]; } else { i++; j++; }
    }
    return out;
  }
  /** Where `now` differs from `was`: the length of their common start, and of their common end after it. */
  function changed(was: string, now: string): [number, number] {
    let p = 0, q = 0;
    const m = Math.min(was.length, now.length);
    while (p < m && was[p] === now[p]) p++;
    while (q < m - p && was[was.length - 1 - q] === now[now.length - 1 - q]) q++;
    return [p, q];
  }
  /** The paragraph around offset `at`: from after the blank line before it to the blank line after it. */
  function paragraph(text: string, at: number): [number, number] {
    const before = text.slice(0, at).search(/\n[ \t]*\n(?![\s\S]*\n[ \t]*\n)/), after = /\n[ \t]*\n/.exec(text.slice(at));
    return [before < 0 ? 0 : text.indexOf('\n', before + 1) + 1, after ? at + after.index : text.length];
  }
  /** Where a caret may stand in a word set from `l` to `r`: the boundary before each character and after the last,
   * at `width(prefix)` scaled to the box (`width`: the echo font's), evenly spaced when it measures nothing. */
  function boundaries(text: string, l: number, r: number, width: (s: string) => number): number[] {
    const all = width(text), n = Math.max(1, text.length);
    return [...Array(text.length + 1).keys()].map((i) => all > 0 ? l + (r - l) * width(text.slice(0, i)) / all : l + (r - l) * i / n);
  }
  /** The index of the boundary nearest `x`: a click between two characters goes there, never into a glyph. */
  const boundaryAt = (xs: number[], x: number) => xs.reduce((best, b, i) => Math.abs(b - x) < Math.abs(xs[best]! - x) ? i : best, 0);
  /** `x` (in pixel columns of a word, `ink` each column's darkness) moved to the gap between glyphs nearest it within
   * `reach` columns: the middle of the run of least ink. A font measure puts a boundary near its glyphs' gap, the
   * page's pixels put it there exactly. */
  function inkSnap(x: number, ink: number[], reach: number): number {
    const lo = Math.max(0, Math.ceil(x - reach)), hi = Math.min(ink.length - 1, Math.floor(x + reach));
    if (lo > hi) return x;
    const min = Math.min(...ink.slice(lo, hi + 1));
    let best = x, gap = Infinity;
    for (let c = lo; c <= hi; c++) {
      if (ink[c] !== min) continue;
      let e = c;
      while (e < hi && ink[e + 1] === min) e++;
      const mid = (c + e + 1) / 2;
      if (Math.abs(mid - x) < gap) { gap = Math.abs(mid - x); best = mid; }
      c = e;
    }
    return best;
  }
  /** Where characters `a..b` (inclusive) of a typeset row's text `row` (its spaces left out) stand among a page's
   * words `texts` (in reading order, spaces left out too): `[word, from, to]` per word they cover, by their place in
   * the row, never by their spelling alone. The run is found with as much of its row around it as the page spells
   * the same and only once (math the page writes otherwise, `−` for `-`, stays outside); null where none is unique. */
  function locate(texts: string[], row: string, a: number, b: number): [number, number, number][] | null {
    const all = texts.join(''), starts = texts.map((_, i) => texts.slice(0, i).join('').length);
    for (let reach = 16; reach >= 0; reach = reach ? reach >> 1 : -1) {
      for (const [l, r] of [[reach, reach], [reach, 0], [0, reach]] as const) {
        const lo = Math.max(0, a - l), probe = row.slice(lo, b + 1 + r), at = all.indexOf(probe);
        if (at < 0 || all.indexOf(probe, at + 1) >= 0) continue;
        const from = at + a - lo, to = from + b - a + 1, out: [number, number, number][] = [];
        texts.forEach((t, i) => { const p = Math.max(from, starts[i]!), q = Math.min(to, starts[i]! + t.length); if (p < q) out.push([i, p - starts[i]!, q - starts[i]!]); });
        return out;
      }
    }
    return null;
  }
  /** Boxes cut so that none overlaps another on its row: where two do, the narrower keeps the place and the wider
   * keeps what lies left and right of it. One highlight per spot: an argument's word inside its macro's text. */
  function apart<B extends Flat>(boxes: B[]): B[] {
    const byWidth = [...boxes].sort((a, b) => (a.r - a.l) - (b.r - b.l)), kept: B[] = [];
    for (const b of byWidth) {
      let parts = [b];
      for (const k of kept) {
        if (k.page !== b.page || k.d <= b.t || k.t >= b.d) continue;
        parts = parts.flatMap((p) => k.r <= p.l || k.l >= p.r ? [p] : [{ ...p, r: k.l }, { ...p, l: k.r }].filter((q) => q.r - q.l > 0.5));
      }
      kept.push(...parts);
    }
    return kept;
  }
  /** The index of the word nearest a point: distance across counts less than distance down (a row first). */
  function nearest(words: Word[], x: number, y: number): number {
    let best = -1, score = Infinity;
    words.forEach((w, i) => { const s = Math.max(0, w[1] - x, x - w[3]) + 4 * Math.max(0, w[2] - y, y - w[4]); if (s < score) { score = s; best = i; } });
    return best;
  }
  /** Selected words as one band per row: consecutive words on a baseline merged, the gaps between them covered. */
  function bands(words: Word[]): Flat[] {
    const out: Flat[] = [];
    for (const w of words) {
      const o = out.at(-1);
      if (o && o.page === w[0] && Math.abs(o.d - w[4]) < 1 && w[1] >= o.l) { o.r = Math.max(o.r, w[3]); o.t = Math.min(o.t, w[2]); }
      else out.push({ page: w[0], l: w[1], t: w[2], r: w[3], d: w[4] });
    }
    return out;
  }
  /** Words as the page reads: a space between words of a row, a line break between rows. */
  const joined = (words: Word[]) => bands(words).map((b) => words.filter((w) => w[0] === b.page && Math.abs(w[4] - b.d) < 1 && w[1] >= b.l && w[3] <= b.r).map((w) => w[5]).join(' ')).join('\n');
  self.satexPdfEdit = { naming, entries, checked, grow, key, broken, scan, start, cells, cellStep, rowAfter, verbatim, verbatimAt, nearest, bands, joined, place, extent, room, form, measure, respec, widthBp, snapWidth, resized, stepped, plain, editable, spans, sender, typed, erased, mathAt, boundaries, boundaryAt, inkSnap, locate, columns, wordAt, argAt, relaid, apart, tokens, aligned, changed, paragraph };
})();

// TikZ nodes moved by drag: where each `\node` and path label stands in the source, and the statement a drag
// rewrites. `at (x,y)` moves keeping its units and digits, a placed node (`right=of a`) gets `xshift`/`yshift`,
// a label on a path gets `pos=`. Lengths are cm in the picture's frame, y up.
declare global {
  /** A coordinate as written: its number, unit ('' is the picture's x or y unit), digits after the point. */
  type TikzLen = { v: number; unit: string; dec: number };
  /** A node of a picture: the statement it stands in (`from`..`to`), its `{…}` line, what places it. */
  interface TikzNode {
    kind: 'node' | 'label'; from: number; to: number; i: number; line: number; name: string | null; text: string;
    at: [TikzLen, TikzLen] | null; placed: boolean; plain: boolean; seg: [string, string] | null; pos: number | null; pic: TikzPic;
  }
  /** A picture's scale and units in cm. */
  type TikzPic = { scale: number; x: number; y: number };
  /** A move: by a distance (`by`, snapped to the mm grid unless `free`), to a place (`to`, from the origin), or to `pos`. */
  type TikzMove = { by?: [number, number]; to?: [number, number]; pos?: number; free?: boolean };
  interface Window {
    satexTikzMove: {
      nodes(text: string): TikzNode[]; move(stmt: string, i: number, pic: TikzPic, m: TikzMove): { text: string; label: string; by: [number, number] } | null;
      len(s: string): TikzLen | null; cm(l: TikzLen, unit: number): number; posOn(a: [number, number], b: [number, number], p: [number, number]): number;
      clipped(a: [number, number], ah: [number, number], b: [number, number], bh: [number, number]): [[number, number], [number, number]];
    };
  }
}
(() => {
  const UNIT: Record<string, number> = { cm: 1, mm: 0.1, in: 2.54, pt: 2.54 / 72.27, bp: 2.54 / 72 };
  /** `2.5cm`, `-3`, `.5in`: a length as written. */
  function len(s: string): TikzLen | null {
    const m = /^\s*([+-]?(?:\d+\.?\d*|\.\d+))\s*(cm|mm|in|pt|bp)?\s*$/.exec(s);
    return m ? { v: +m[1], unit: m[2] ?? '', dec: m[1].split('.')[1]?.length ?? 0 } : null;
  }
  /** A length in cm, `unit` the cm of a bare number. */
  const cm = (l: TikzLen, unit: number) => l.v * (l.unit ? UNIT[l.unit] : unit);
  /** `c` cm in the unit of `l` (bare: `unit` cm), on the mm grid unless `free`, with as many digits as needed, at least as written. */
  function write(l: TikzLen, c: number, unit: number, free: boolean): string {
    const per = l.unit ? UNIT[l.unit] : unit, mm = free ? c : Math.round(c * 10) / 10;
    const need = Math.max(0, Math.ceil(-Math.log10(0.1 / per) - 1e-9)) + (free ? 1 : 0);
    let s = (mm / per).toFixed(Math.max(l.dec, need));
    if (s.includes('.')) s = s.replace(/0+$/, '').replace(/\.$/, '');
    if (l.dec && !s.includes('.')) s += '.';
    while (l.dec && s.split('.')[1].length < l.dec) s += '0';
    return `${/^-0(\.0*)?$/.test(s) ? s.slice(1) : s}${l.unit}`;
  }
  /** The end of the group opened at `i` (`[`, `(`, `{`), braces counted, else -1. */
  function close(t: string, i: number): number {
    const want: Record<string, string> = { '[': ']', '(': ')', '{': '}' }, stack = [want[t[i]]];
    for (let k = i + 1; k < t.length; k++) {
      const c = t[k];
      if (c === '\\') { k++; continue; }
      if (c === '{' || (stack.at(-1) !== '}' && (c === '[' || c === '(') && c === t[i])) stack.push(want[c]);
      else if (c === stack.at(-1)) { stack.pop(); if (!stack.length) return k; }
    }
    return -1;
  }
  /** The comma-separated keys of an option list, braces kept together. */
  function keys(s: string): string[] {
    const out = [''];
    let depth = 0;
    for (const c of s) { if (c === '{') depth++; if (c === '}') depth--; if (c === ',' && !depth) out.push(''); else out[out.length - 1] += c; }
    return out.map((k) => k.trim()).filter(Boolean);
  }
  const PLACED = /^(above|below|left|right)(\s+(left|right))?\s*=.*\bof\b/, SHIFTS = /^[xy]shift\s*=/;
  const ALONG: Record<string, number> = { 'at start': 0, 'very near start': 0.125, 'near start': 0.25, midway: 0.5, 'near end': 0.75, 'very near end': 0.875, 'at end': 1 };
  /** Anything but a centred node at its point: an anchor, a placement, a shift, a transformation. */
  const OFFSET = /^(anchor|above|below|left|right|[xy]?shift|rotate|scale|[xy]scale|label|pin)\b/;
  /** The parts of statement `s`: each node with its options (`o`: `[`..`]`, else where they go), name, `at`, text. */
  type Part = { kind: 'node' | 'label'; o: [number, number] | null; oAt: number; name: string | null; at: [number, number] | null; t: [number, number]; seg: [string, string] | null; after: boolean };
  function parts(s: string): Part[] {
    const head = /^\s*\\(node|draw|path|fill|filldraw|shade|shadedraw)\b/.exec(s);
    if (!head) return [];
    const out: Part[] = [], points: string[] = [];
    let k = head[0].length, op = false;
    const ws = () => { while (/\s/.test(s[k] ?? '')) k++; };
    /** A node from `k` (past `node`): options, name, `at`, text. */
    const node = (kind: 'node' | 'label', oAt: number) => {
      let o: [number, number] | null = null, name: string | null = null, at: [number, number] | null = null;
      for (;;) {
        ws();
        if (s[k] === '[') { const e = close(s, k); if (e < 0) return null; o = [k, e]; k = e + 1; }
        else if (s[k] === '(') { const e = close(s, k); if (e < 0) return null; name = s.slice(k + 1, e).trim(); k = e + 1; }
        else if (s.startsWith('at', k) && /\s|\(/.test(s[k + 2] ?? '')) { k += 2; ws(); const e = close(s, k); if (e < 0) return null; at = [k, e]; k = e + 1; }
        else if (s[k] === '{') { const e = close(s, k); if (e < 0) return null; const t: [number, number] = [k, e]; k = e + 1; return { kind, o, oAt, name, at, t, seg: null, after: false } as Part; }
        else return null;
      }
    };
    if (head[1] === 'node') { const p = node('node', k); return p ? [p] : []; }
    ws();
    if (s[k] === '[') k = close(s, k) + 1;
    while (k > 0 && k < s.length) {
      ws();
      if (k >= s.length) break;
      if (s.startsWith('--', k)) { op = true; k += 2; continue; }
      if (s.startsWith('to', k) && !/[a-z]/i.test(s[k + 2] ?? '')) { op = true; k += 2; ws(); if (s[k] === '[') k = close(s, k) + 1; continue; }
      if (s.startsWith('node', k) && !/[a-z]/i.test(s[k + 4] ?? '')) {
        const oAt = k + 4; k = oAt;
        const p = node('label', oAt);
        if (!p) return out;
        // on the segment it stands in, else the one it follows
        p.after = !op;
        p.seg = op ? [points.at(-1) ?? '', ''] : points.length > 1 ? [points.at(-2)!, points.at(-1)!] : null;
        out.push(p);
        continue;
      }
      if (s[k] === '(' || s.startsWith('+(', k) || s.startsWith('++(', k)) {
        if (s[k] === '+') { op = false; points.push(''); k = s.indexOf('(', k); k = close(s, k) + 1; continue; }
        const e = close(s, k), pt = s.slice(k + 1, e).trim();
        for (const p of out) if (p.seg && p.seg[1] === '' && !p.after) p.seg[1] = pt;
        points.push(pt); op = false; k = e + 1; continue;
      }
      // another operation (`|-`, `circle`, `rectangle`, `..`): the labels before it keep their segments
      op = false; points.push(''); k++;
      while (k < s.length && !/[\s(]/.test(s[k])) k++;
    }
    return out.map((p) => (p.seg && (!p.seg[0] || !p.seg[1]) ? { ...p, seg: null } : p));
  }
  /** The picture options' scale and units. */
  function picOf(opts: string): TikzPic {
    let scale = 1, x = 1, y = 1;
    for (const kv of keys(opts)) {
      const [k, v = ''] = kv.split('=').map((t) => t.trim()), l = len(v);
      if (k === 'scale' && +v) scale = +v;
      if ((k === 'x' || k === 'y') && l) { if (k === 'x') x = cm(l, 1); else y = cm(l, 1); }
    }
    return { scale, x, y };
  }
  /** Each node and path label of the text's pictures, comments blanked. */
  function nodes(text: string): TikzNode[] {
    const t = text.replace(/(?<!\\)%.*$/gm, (c) => ' '.repeat(c.length)), out: TikzNode[] = [];
    const lineAt = (i: number) => { let n = 1; for (let k = text.indexOf('\n'); k >= 0 && k < i; k = text.indexOf('\n', k + 1)) n++; return n; };
    for (const m of t.matchAll(/\\begin\{tikzpicture\}/g)) {
      let k = m.index! + m[0].length;
      const end = t.indexOf('\\end{tikzpicture}', k);
      if (end < 0) continue;
      let pic = picOf('');
      while (/\s/.test(t[k])) k++;
      if (t[k] === '[') { const e = close(t, k); pic = picOf(t.slice(k + 1, e)); k = e + 1; }
      // statements end at `;` outside groups; a loop's statements stand for many nodes, skipped whole
      while (k < end) {
        let e = k, depth = 0;
        for (; e < end; e++) { const c = t[e]; if (c === '\\') { e++; continue; } if ('{[('.includes(c)) depth++; else if ('}])'.includes(c)) depth--; else if (c === ';' && depth <= 0) break; }
        const s = t.slice(k, e + 1), lead = s.length - s.trimStart().length;
        if (!/^\s*\\foreach\b/.test(s)) parts(s).forEach((p, i) => {
          const o = p.o ? keys(s.slice(p.o[0] + 1, p.o[1])) : [], xy = p.at ? s.slice(p.at[0] + 1, p.at[1]).split(',') : [];
          const at = xy.length === 2 && len(xy[0]) && len(xy[1]) ? [len(xy[0])!, len(xy[1])!] as [TikzLen, TikzLen] : null;
          const along = o.map((x) => x.startsWith('pos') ? +x.split('=')[1] : ALONG[x]).find((v) => v != null && !Number.isNaN(v));
          out.push({ kind: p.kind, from: k + lead, to: e + 1, i, line: lineAt(k + p.t[0]), name: p.name, text: s.slice(p.t[0] + 1, p.t[1]), at, placed: o.some((x) => PLACED.test(x)),
            plain: !o.some((x) => OFFSET.test(x)) && (!!at || !p.at), seg: p.seg, pos: p.kind === 'label' ? along ?? (p.after ? 1 : 0.5) : null, pic });
        });
        k = e + 1;
      }
    }
    return out;
  }
  /** Option list `o` (the text between `[`..`]`) with `drop` keys gone and `set` keys written over or added. */
  function setOpts(o: string, drop: RegExp, set: [string, string][]): string {
    const ks = keys(o).filter((k) => !drop.test(k)), names = (k: string) => k.split('=')[0].trim();
    for (const [n, v] of set) { const j = ks.findIndex((k) => names(k) === n); if (j >= 0) ks[j] = `${n}=${v}`; else ks.push(`${n}=${v}`); }
    return ks.join(', ');
  }
  /** Statement `stmt` with its `i`th node moved: its text, what the ghost says, `by` the move in the picture's cm (`to`: the place written). */
  function move(stmt: string, i: number, pic: TikzPic, m: TikzMove) {
    const p = parts(stmt)[i];
    if (!p) return null;
    const free = !!m.free, opts = p.o ? stmt.slice(p.o[0] + 1, p.o[1]) : '';
    /** The statement with options `o` (and `at` text `at`, null to keep). */
    const out = (o: string, at: string | null, label: string, by: [number, number]) => {
      let s = stmt;
      if (at != null) s = p.at ? s.slice(0, p.at[0]) + at + s.slice(p.at[1] + 1) : s.slice(0, p.t[0]) + `at ${at} ` + s.slice(p.t[0]);
      s = p.o ? s.slice(0, p.o[0] + 1) + o + s.slice(p.o[1]) : o ? s.slice(0, p.oAt) + `[${o}]` + s.slice(p.oAt) : s;
      return { text: s, label, by };
    };
    if (m.pos != null) {
      const v = +Math.min(1, Math.max(0, m.pos)).toFixed(free ? 3 : 2);
      return out(setOpts(opts, new RegExp(`^(pos\\s*=|${Object.keys(ALONG).join('|')})`), [['pos', String(v)]]), null, `pos=${v}`, [0, 0]);
    }
    const ks = keys(opts), xy = p.at ? stmt.slice(p.at[0] + 1, p.at[1]).split(',') : [];
    const at = xy.length === 2 && len(xy[0]) && len(xy[1]) ? [len(xy[0])!, len(xy[1])!] : null, units = [pic.x, pic.y];
    if (m.to) {
      // placed anew: the placement and its shifts dropped, `at` in the units written there, else cm
      const l = at ?? [{ v: 0, unit: '', dec: 0 }, { v: 0, unit: '', dec: 0 }], u = at ? units : [1, 1];
      const c = m.to.map((v, j) => write(l[j], v / pic.scale, u[j], free)), o = setOpts(ks.filter((k) => !PLACED.test(k) && !SHIFTS.test(k)).join(', '), /$^/, []);
      return out(o, `(${c.join(',')})`, `at (${c.join(',')})`, [len(c[0])!, len(c[1])!].map((a, j) => cm(a, u[j])) as [number, number]);
    }
    const by = m.by!.map((v) => v / pic.scale);
    if (at) {
      const c = at.map((a, j) => write(a, cm(a, units[j]) + by[j], units[j], free));
      return out(opts, `(${c.join(',')})`, `at (${c.join(',')})`, c.map((s, j) => cm(len(s)!, units[j]) - cm(at[j], units[j])) as [number, number]);
    }
    // a shift as written (a bare one is in pt), else a new one in mm; a shift moved to nothing goes
    const set: [string, string][] = [], gone: string[] = [], said: string[] = [], moved: number[] = [];
    ['xshift', 'yshift'].forEach((n, j) => {
      const k = ks.find((x) => x.split('=')[0].trim() === n), l = (k && len(k.split('=')[1])) || { v: 0, unit: 'mm', dec: 0 };
      const was = cm(l, UNIT.pt), now = was + by[j], v = write(l.unit ? l : { ...l, unit: 'pt' }, now, 1, free);
      moved.push(cm(len(v)!, 1) - was);
      if (cm(len(v)!, 1) === 0) gone.push(n); else { set.push([n, v]); said.push(`${n}=${v}`); }
    });
    return out(setOpts(opts, new RegExp(`^(${gone.join('|') || '$^'})\\s*=`), set), null, said.join(', ') || 'no shift', moved as [number, number]);
  }
  /** Where `p` projects onto `a`→`b`, as a part of its length. */
  const posOn = (a: [number, number], b: [number, number], p: [number, number]) => { const dx = b[0] - a[0], dy = b[1] - a[1], n = dx * dx + dy * dy; return n ? ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / n : 0; };
  /** The line between two centres cut where it leaves their boxes (half sizes `ah`, `bh`): the path TikZ draws between two nodes. */
  function clipped(a: [number, number], ah: [number, number], b: [number, number], bh: [number, number]): [[number, number], [number, number]] {
    const d = [b[0] - a[0], b[1] - a[1]], exit = (h: [number, number]) => Math.min(d[0] ? h[0] / Math.abs(d[0]) : Infinity, d[1] ? h[1] / Math.abs(d[1]) : Infinity);
    const s = Math.min(0.5, exit(ah)), e = Math.max(0.5, 1 - exit(bh));
    return [[a[0] + s * d[0], a[1] + s * d[1]], [a[0] + e * d[0], a[1] + e * d[1]]];
  }
  self.satexTikzMove = { nodes, move, len, cm, posOn, clipped };
})();

// References switched from the pages: a reference's printed number is no text to type into. Where its key stands in
// the command, whether a page word is such output, and a fuzzy rank for the list of keys to switch to.
declare global {
  interface Window {
    satexRefPick: { fuzzy(q: string, t: string): number; keyAt(line: string, col: number, key: string): [number, number] | null; unspelled(line: string, word: string): boolean };
  }
}
(() => {
  /** How well `q` matches `t` in order, case aside: higher is better, -1 none. Runs and word starts count more. */
  function fuzzy(q: string, t: string): number {
    const a = q.toLowerCase().replace(/\s+/g, ''), b = t.toLowerCase();
    if (!a) return 0;
    let score = 0, j = 0, run = 0;
    for (const c of a) {
      const k = b.indexOf(c, j);
      if (k < 0) return -1;
      run = k === j ? run + 1 : 1;
      score += run * 2 + (k === 0 || /[^a-z0-9]/.test(b[k - 1]) ? 3 : 0) - Math.min(k - j, 5) * 0.1;
      j = k + 1;
    }
    return score + (b.includes(a) ? 10 : 0);
  }
  /** Where `key` stands in the key list of the command at column `col` (from 1) of `line`: past its name and `[…]` options. */
  function keyAt(line: string, col: number, key: string): [number, number] | null {
    const m = /^\\[A-Za-z@]+\*?\s*(?:\[[^\]]*\]\s*)*\{/.exec(line.slice(col - 1));
    if (!m) return null;
    const open = col - 1 + m[0].length, end = line.indexOf('}', open);
    let at = open;
    for (const item of line.slice(open, end < 0 ? undefined : end).split(',')) {
      const lead = item.length - item.trimStart().length;
      if (item.trim() === key) return [at + lead, at + lead + key.length];
      at += item.length + 1;
    }
    return null;
  }
  /** Whether page word `word` is not written in source line `line` outside its commands: the output of one. */
  function unspelled(line: string, word: string): boolean {
    const core = word.normalize('NFKC').replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, '');
    if (!core) return false;
    const plain = line.replace(/(?<!\\)%.*$/, '').replace(/\\[A-Za-z@]+\*?(\s*\[[^\]]*\])*(\s*\{[^}]*\})*/g, ' ');
    return !new RegExp(`(?<![\\p{L}\\p{N}])${core.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\p{L}\\p{N}])`, 'u').test(plain);
  }
  self.satexRefPick = { fuzzy, keyAt, unspelled };
})();
