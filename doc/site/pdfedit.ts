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
/** A piece of a paragraph for the echo's line breaking: its width `n` and the space before it `gap` (bp); `par` starts
 * a new paragraph (Enter). Anything else it carries (its text, the page word it is) rides along. */
/** `disp`: display math, centered on a row of its own, that much more space above it (bp), `dshort` when the row
 * before ends left of it, `below` after it. */
type Piece = { gap: number; n: number; par?: boolean; disp?: number; dshort?: number; below?: number; st?: number; sh?: number; hy?: boolean; hw?: number };
/** The lines a paragraph may take: its right edge, its left margin, the indent and gap of a new paragraph, the rows the
 * page set after the first (each its left edge and its own words, gaps set), whether a row may take words up (`pull`). */
type Line<P> = { right: number; left: number; indent: number; skip: number; parskip: number; rows: { x: number; pieces: P[] }[]; pull?: boolean; shrink?: number; justify?: boolean; fit?: boolean; breaks?: { breaks: number[]; ratios: number[] } | null };
/** Rows laid out: each its left edge, its pieces placed (`x`), the page row it takes (`from`: -2 the first, -1 a new
 * one) and a new row's baseline shift from the row before (`dy`); `freed` the page rows whose words all went up. */
type Flow<P> = { rows: { x: number; from: number; dy: number; par: boolean; out: (P & { x: number })[] }[]; freed: number[] };
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
      treeNodes(text: string, at: number): [number, number][];
      editable(text: string, from: number, to: number, keys: [number, number][]): string | null;
      spans(line: string, word: string): [number, number][];
      sender<V>(send: (v: V) => Promise<boolean>): { put(v: V): void; flush(): Promise<void> };
      typed(text: string, at: number, c: string, inline?: string, smart?: boolean, fresh?: [number, number] | null): [number, number, string, number];
      erased(text: string, at: number, back: boolean, fresh?: [number, number] | null): [number, number];
      checked(text: string, edit: [number, number, string, number?], o?: object): string | null;
      naming(text: string, at: number, fresh?: [number, number] | null): boolean;
      entries(texts: Record<string, string>, root: string, kind: string): [string, number, number, number][];
      shortTitle(text: string, head: number, k: number): [number, number, string, number] | null;
      bothTitles(text: string, head: number): [number, number, string] | null;
      renamed(text: string, name: string, value: string, lang?: string): [number, number, string, number] | null;
      leaders(text: string, kind: string, what: string, tocloft: boolean, chapters?: boolean): [number, number, string] | null;
      grow(f: [number, number] | null | undefined, a: number, b: number, n: number): [number, number] | null;
      key(text: string, at: number, k: string, o?: object): [number, number, string, number] | string;
      broken(text: string, at: number, shift: boolean): [number, number, string, number] | string;
      scan(text: string, from: number, to: number, st: object, out?: { s: number; e: number; t: string }[]): { math: string; groups: [number, string][] };
      start(): object;
      mathAt(text: string, at: number, kind: string, inline?: string): [number, number, string, number];
      displayEnd(text: string, at: number): boolean;
      onRow(v: Word, r: Word): boolean;
      run(line: Word[], w: Word, back?: boolean): Word[];
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
      afterCall(text: string, at: number): number | null;
      apart<B extends Flat>(boxes: B[]): B[];
      relaid(row: [number, number, string][], k: number, now: string, width: (s: string) => number): [number, number, string][];
      reflow<P extends Piece>(pieces: P[], x0: number, line: Line<P>): Flow<P>;
      fit(pieces: Piece[], first: number, width: number, tolerance?: number): { breaks: number[]; ratios: number[] } | null;
      glue(before: string, space: number): { gap: number; st: number; sh: number };
      centered(box: [number, number], row: [number, number], growth: number): number;
      tokens(text: string, from: number, to: number): Tok[];
      unset(text: string, from: number, to: number): string;
      aligned(toks: string[], words: string[]): number[];
      changed(was: string, now: string, by?: number): [number, number];
      offsetAt(text: string, p: { line: number; character: number }): number;
      units(text: string, math: (i: number) => boolean): (string[] & { index: number })[];
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
  /** A structural token of the source: `{`, `}`, a command's option `[` and `]`, `$`, `$$`, `&`, `#`, `_`, `^`, `par` (a blank line), `%` (a comment),
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
        // a forest's `[` opens its tree, no option: the node text is text
        st.cmd = t === '\\begin{forest}' ? '' : m[1] ?? '';
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
      else if (c === '[' && st.cmd && st.opt == null) { put(i, i + 1, c); st.opt = st.cmd; st.optAt = st.groups.length; st.cmd = ''; continue; }
      else if (c === ']' && st.opt != null && st.groups.length === st.optAt) { put(i, i + 1, c); st.cmd = st.opt; st.opt = null; continue; }
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
    const m = /(?:\\[A-Za-z@]+\*?|\\[^A-Za-z@]|[\]}_^])[ \t]*(?:\n[ \t]*)?$/.exec(text.slice(Math.max(0, at - 64), at));
    return m ? [at - m[0].length, m[0].trimEnd()] : null;
  }
  /** Control symbols and words that are escaped characters, deleted whole as the character they show. */
  const ESC = /^\\([^A-Za-z@()[\]\\]|textbackslash|textasciitilde|textasciicircum)$/;
  /** Whether the tokens `ms`, in order, open and close as many groups, formulas and environments as they close. */
  function balanced(ms: Mark[]): boolean {
    let br = 0, dl = 0, dd = 0, p = 0, q = 0, o = 0;
    const env: string[] = [];
    for (const m of ms) {
      const t = m.t;
      if (t === '{') br++; else if (t === '}' && --br < 0) return false;
      else if (t === '[') o++; else if (t === ']' && --o < 0) return false;
      else if (t === '$') dl ^= 1; else if (t === '$$') dd ^= 1;
      else if (t === '\\(') p++; else if (t === '\\)' && --p < 0) return false;
      else if (t === '\\[') q++; else if (t === '\\]' && --q < 0) return false;
      else if (t.startsWith('\\begin{')) env.push(t.slice(7));
      else if (t.startsWith('\\end{') && env.pop() !== t.slice(5)) return false;
    }
    return !br && !dl && !dd && !p && !q && !o && !env.length;
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
    const g = ctx.groups.at(-1)?.[1] ?? '', gAt = ctx.groups.at(-1)?.[0] ?? -1;
    // an environment's name being typed (its `\\begin{` fresh) takes letters: Enter completes the pair (broken)
    const naming = g === 'begin' && !!f && f[0] <= gAt && gAt < f[1] && /^[A-Za-z*@]*$/.test(body);
    if (!naming && (KEYS.test(g) || /^(begin|end)$/.test(g))) return g === 'begin' || g === 'end' ? "it is an environment's argument" : 'it is a key (a label, a reference, a citation)';
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
    // an environment being typed (its `\\begin{…` all typed since the caret was put) counts once Enter writes its pair
    const typing = (m: Mark, own: (m: Mark) => boolean) => m.t.startsWith('\\begin{') && own(m);
    if (!balanced(gone) && !balanced(gone.filter((m) => !typing(m, freshWas)))) return 'it would unbalance the braces or the math around it';
    if (!balanced(added) && !balanced(added.filter((m) => !typing(m, ownNow)))) return 'it would unbalance the braces or the math';
    for (const m of gone) {
      if (m.t === 'par' || freshWas(m)) continue;
      if (!ownWas(m)) return m.t[0] === '\\' ? `it would change the command ${m.t}` : 'it would change the markup around it';
      // a formula's delimiters go only in pairs (balanced above): a whole formula may go
      if (m.t[0] === '\\' && !ESC.test(m.t) && !/^\\[()[\]]$/.test(m.t)) return `it would delete the command ${m.t}`;
      if (/^(%|verb|&|#)$/.test(m.t)) return 'it would delete markup';
      // an argument's braces go only with their command
      const arg = m.t === '{' ? argOf(text, m.s) : null;
      if (arg && !(a <= arg[0] && arg[0] < b) && !fr(arg[0])) return `it would delete an argument of ${arg[1]}`;
    }
    for (const m of added) {
      // a blank line joined to the line break beside it (Enter at a line's end) is a new one, its spaces no markup
      if (m.t === 'par' && !o.free && (ctx.groups.length || ctx.math)) return ctx.math ? 'a paragraph cannot end in math' : `a paragraph cannot end inside ${g ? `\\${g}{…}` : 'a group'}: Shift+Enter breaks the line`;
      if (m.t === 'par') continue;
      if (!ownNow(m)) return m.t[0] === '\\' ? `it would change a command's name (${m.t})` : 'it would change the markup around it';
      if (m.t === '%') return 'it would start a comment';
      if (m.t === 'verb') return 'it would start verbatim text';
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
  /** The script groups whose `{` was typed (`_{`), each as the text up to its brace: typed's only memory. */
  let explicitGroups: string[] = [];
  function typed(text: string, at: number, c: string, inline = '\\(', smart = false, fresh: [number, number] | null = null): [number, number, string, number] {
    // verbatim takes the character as it is; an inline body never its closer or a line break
    const v = verbatimAt(text, at);
    if (v) return !v.block && (c === v.close || c === '\n') ? [at, at, '', 0] : [at, at, c, c.length];
    const fr = (p: number) => !!fresh && fresh[0] <= p && p < fresh[1], letter = /^[A-Za-z@]$/.test(c);
    // the space typed after a `\` (to keep it off the next word) goes once the name is a control symbol
    // `\[` or `\(` outside math opens a formula: its closer comes with it, the caret inside (never a lone opener)
    if (lone(text, at) && (c === '[' || c === '(') && !scan(text, 0, at - 1, start()).math) return [at, at + (text[at] === ' ' && fr(at) ? 1 : 0), c === '[' ? '[\\]' : '(\\)', 1];
    if (lone(text, at)) return letter ? [at, at, c, 1] : [at, at + (text[at] === ' ' && fr(at) ? 1 : 0), c, 1];
    // its own space, never one of the text's: a letter typed then renames only what was typed
    if (c === '\\') return [at, at, '\\ ', 1];
    const cs = nameEnd(text, at), own = cs >= 0 && fr(cs);
    if (cs >= 0 && letter) return own ? [at, at, c, 1] : [at, at, `{}${c}`, 3];
    // its argument ends the name: the space that kept it off the next word goes
    if (cs >= 0 && c === '{' && own) return [at, at + (text[at] === ' ' && fr(at) ? 1 : 0), '{}', 1];
    if (cs >= 0 && c === ' ' && !own) return [at, at, '{} ', 3];
    if (c === '}' && text[at] === '}' && fr(at)) return [at, at, '', 1];
    // past the commands' own spaces too (`\\to\\infty  }`): they go, the group closes after the name
    const blanks = (/^( +)\}/.exec(text.slice(at, at + 64))?.[1].length ?? 0);
    if (c === '}' && blanks && fr(at) && fr(at + blanks)) return [at, at + blanks, '', 1];
    const st = scan(text, 0, at, start());
    if (!st.math) {
      if (c !== '$') { const body = escaped(c); return [at, at, body, body.length]; }
      const open = inline === '$' || inline === '\\(' ? inline : inlineOf(text);
      // `$ $`, not `$$`: TeX reads `$$` as display math; the space goes with the first character typed
      return open === '$' ? [at, at, '$ $', 1] : [at, at, '\\(\\)', 2];
    }
    // smart scripts: `_`/`^` open a group, a space, an operator, the other script (`x_i^2`) or the formula's end typed
    // at its end leave it; `script` the `_` or `^` of the group the caret ends, past groups closed in it (`_{{i=1}`)
    // a `{` typed into the empty group a script opened takes that group as its own (`_{i=1}` stays one group): it is
    // remembered, by the text before it, as a group no space or operator leaves
    if (smart && c === '{' && text[at] === '}' && text[at - 1] === '{' && /[_^]/.test(text[at - 2] ?? '') && fr(at)) {
      explicitGroups = [...explicitGroups.slice(-7), text.slice(Math.max(0, at - 17), at)];
      return [at, at, '', 0];
    }
    let script = '';
    for (let j = at - 1, n = 0; smart && text[at] === '}' && j >= Math.max(0, at - 200); j--) {
      if (text[j - 1] === '\\') continue;
      if (text[j] === '}') n++;
      else if (text[j] === '{' && n) n--;
      else if (text[j] === '{') { script = /[_^]/.test(text[j - 1] ?? '') && !explicitGroups.includes(text.slice(Math.max(0, j - 16), j + 1)) ? text[j - 1]! : ''; break; }
    }
    if (script && (/[\s$+\-=<>,;:|/*!?()[\]]/.test(c) || /[_^]/.test(c) && c !== script)) {
      // past the `}` first, then the character as typed there (a space only leaves)
      return /\s/.test(c) ? [at, at, '', 1] : typed(text, at + 1, c, inline, smart, fresh);
    }
    if (smart && (c === '_' || c === '^')) {
      // a new group opened here is a script's own until a `{` is typed into it
      const key = (text.slice(0, at) + c + '{').slice(-17);
      explicitGroups = explicitGroups.filter((g) => g !== key);
      return [at, at, `${c}{}`, 2];
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
  /** What Ctrl+Backspace (`back`) or Ctrl+Delete takes: the blanks next to the caret and the word past them, as far as
   * the text is plain letters; markup next to the caret goes as one Backspace takes it. */
  function wordErased(text: string, at: number, back: boolean, fresh: [number, number] | null = null): [number, number] {
    const run = back ? /[\p{L}\p{N}]*[ \t]*$/u.exec(text.slice(Math.max(0, at - 200), at))![0].length : /^[ \t]*[\p{L}\p{N}]*/u.exec(text.slice(at, at + 200))![0].length;
    // a letter after a `\` is a command's name, not a word
    const from = back ? at - run : at, to = back ? at : at + run;
    if (!run || verbatimAt(text, at) || /\\[A-Za-z@]*$/.test(text.slice(Math.max(0, from - 40), from + 1)) && /[A-Za-z@]/.test(text[from] ?? '')) return erased(text, at, back, fresh);
    return [from, to];
  }
  /** What Backspace (`back`) or Delete takes at `at`: `[from, to]`, an escape or an empty pair (`$ $`, `{}`, `\(\)`)
   * whole, a formula whose last character goes with it, at a formula's `\)` (`\(` for Delete) its character inside,
   * a `\` typed with the space that kept it off a word. */
  function erased(text: string, at: number, back: boolean, fresh: [number, number] | null = null): [number, number] {
    const v = verbatimAt(text, at);
    if (v) return back ? (at > v.from ? [at - 1, at] : [at, at]) : (at < v.to ? [at, at + 1] : [at, at]);
    for (const pair of ['{}', '\\(\\)', '\\[\\]']) {
      const h = pair.length / 2;
      if (text.slice(at - h, at + h) === pair) return [at - h, at + h];
    }
    for (const k of [1, 2]) if (text.slice(at - k, at - k + 3) === '$ $') return [at - k, at - k + 3];
    // next to a formula's delimiter (`\)` before, `\(` after): its character inside goes, never a lone delimiter
    const odd = (i: number) => { let n = 0; while (text[i - n] === '\\') n++; return n % 2 === 1; };
    if (back && /^\\[)\]]$/.test(text.slice(at - 2, at)) && odd(at - 2)) return /^\\[(\[]$/.test(text.slice(at - 4, at - 2)) ? [at - 4, at] : erased(text, at - 2, true, fresh);
    if (!back && /^\\[(\[]$/.test(text.slice(at, at + 2)) && odd(at)) return /^\\[)\]]$/.test(text.slice(at + 2, at + 4)) ? [at, at + 4] : erased(text, at + 2, false, fresh);
    // the same at an inline `$`: its closer before the caret (Backspace), its opener after it (Delete)
    if (back && text[at - 1] === '$' && text[at - 2] !== '$' && !odd(at - 2) && scan(text, 0, at - 1, start()).math === '$') return erased(text, at - 1, true, fresh);
    if (!back && text[at] === '$' && text[at + 1] !== '$' && !odd(at - 1) && !scan(text, 0, at, start()).math) return erased(text, at + 1, false, fresh);
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
      for (const [o, c] of [['\\(', '\\)'], ['\\[', '\\]']]) if (text.slice(p - 2, p) === o && text.startsWith(c, q)) return [p - 2, q + 2];
    }
    return [Math.max(0, p), Math.min(text.length, q)];
  }
  /** Enter (`shift`: Shift+Enter) at `at`: a line in verbatim, a row in a table, a line break in math, a paragraph
   * in text (a line break with Shift); the reason where none may go. */
  function broken(text: string, at: number, shift: boolean, fresh: [number, number] | null = null): [number, number, string, number] | string {
    // Enter after an environment's `\\begin{name}` just typed: its `\\end{name}` comes with it, the caret on the line
    // between (the pair written whole, from the `\\begin` on: never a lone \\begin or \\end)
    const env = /\\begin\{([A-Za-z*@]+)\}$/.exec(text.slice(Math.max(0, at - 80), at));
    if (env && !shift && fresh && fresh[0] <= at - env[0].length) { const b = at - env[0].length, o = `${env[0]}\n`; return [b, at, `${o}\n\\end{${env[1]}}`, o.length]; }
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
      : k === 'Ctrl+Backspace' || k === 'Ctrl+Delete' ? [...wordErased(text, at, k === 'Ctrl+Backspace', o.fresh ?? null), '', 0] as [number, number, string, number]
      : k === 'Enter' || k === 'Shift+Enter' ? broken(text, at, k === 'Shift+Enter', o.fresh ?? null)
        : typed(text, at, k, o.inline, o.smart, o.fresh ?? null);
    if (typeof e === 'string') return e;
    return checked(text, e, { ...o, row: k === 'Enter' }) ?? e;
  }
  /** The entries of a table of contents (`toc`), list of figures (`lof`) or of tables (`lot`), in the order the
   * document sets them from `root` (`\input`, `\include` followed in `texts`): where each entry's text is written,
   * `[file, from, to]`: a sectioning command's or caption's short title (`\section[Short]{Long}`) if it has one, else
   * its title. Starred sections and captions outside their float make none. */
  function entries(texts: Record<string, string>, root: string, kind: string): [string, number, number, number][] {
    const out: [string, number, number, number][] = [], seen = new Set<string>();
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
        if (t[i] === '[') { const e = closing(t, i, '[', ']'); if (e < 0) continue; short = [i + 1, e]; i = e + 1; while (/\s/.test(t[i] ?? '')) i++; }
        if (t[i] !== '{') continue;
        const e = closing(t, i, '{', '}');
        if (e < 0) continue;
        out.push([file, ...(short ?? [i + 1, e]), m.index]);
        re.lastIndex = e + 1;
      }
    };
    walk(root);
    return out;
  }
  /** Where the group opened at `i` (`open`, `close`) closes, -1 if it does not; an option's `]` inside braces is text. */
  function closing(t: string, i: number, open: string, close: string): number {
    let d = 0, g = 0;
    for (let j = i; j < t.length; j++) {
      if (t[j] === '\\') { j++; continue; }
      if (open !== '{' && (t[j] === '{' || t[j] === '}')) { g += t[j] === '{' ? 1 : -1; continue; }
      if (g > 0) continue;
      if (t[j] === open) d++; else if (t[j] === close && --d === 0) return j;
    }
    return -1;
  }
  const HEAD = /^\\(?:part|chapter|section|subsection|subsubsection|paragraph|caption)\s*/;
  /** A short title for the sectioning command or caption at `head` (its `\\`) that has none: its title copied into
   * `[…]` before it, `[from, to, body, caret]` with the caret at `k` in the copy; null when it has one. A `]` outside
   * braces would end the option early: the copy goes in braces then. */
  function shortTitle(text: string, head: number, k: number): [number, number, string, number] | null {
    const m = HEAD.exec(text.slice(head)), at = head + (m?.[0].length ?? 0), e = m && text[at] === '{' ? closing(text, at, '{', '}') : -1;
    if (e < 0) return null;
    const t = text.slice(at + 1, e), wrap = /\]/.test(t.replace(/\{[^{}]*\}/g, ''));
    return [at, at, wrap ? `[{${t}}]` : `[${t}]`, (wrap ? 2 : 1) + Math.min(Math.max(k, 0), t.length)];
  }
  /** `\\section[S]{L}` at `head` written `\\section{S}`: the heading takes the short title. `[from, to, body]`. */
  function bothTitles(text: string, head: number): [number, number, string] | null {
    const m = HEAD.exec(text.slice(head)), at = head + (m?.[0].length ?? 0), e = m && text[at] === '[' ? closing(text, at, '[', ']') : -1;
    let i = e + 1;
    while (e >= 0 && /\s/.test(text[i] ?? '')) i++;
    const f = e >= 0 && text[i] === '{' ? closing(text, i, '{', '}') : -1;
    if (f < 0) return null;
    // the braces a copy got for its `]` (shortTitle's wrap) go, any other braces stay
    const s = text.slice(at + 1, e), bare = /^\{.*\}$/s.test(s) && closing(s, 0, '{', '}') === s.length - 1 && /\]/.test(s.slice(1, -1).replace(/\{[^{}]*\}/g, '')) ? s.slice(1, -1) : s;
    return [at, f + 1, `{${bare}}`];
  }
  /** Offsets of `text` inside a comment. */
  const commented = (text: string, at: number) => /(?<!\\)%/.test(text.slice(text.lastIndexOf('\n', at - 1) + 1, at));
  /** Where a line of the preamble goes: the start of `\\begin{document}`'s line, -1 without one. */
  const preambleEnd = (text: string) => { const d = /\\begin\s*\{document\}/.exec(text); return d ? text.lastIndexOf('\n', d.index - 1) + 1 : -1; };
  /** Generated text renamed (`\\contentsname` set to `value`): the `\\renewcommand` of `name` already written gets the
   * value, else a line of its own before `\\begin{document}`. Under babel (`lang`, its language) the names are set
   * anew at `\\begin{document}`: the line goes into `\\addto\\captions<lang>{…}`, a plain one is not the one in force.
   * `[from, to, body, value]`: `value` where the value starts after the edit; null without a preamble. */
  function renamed(text: string, name: string, value: string, lang = ''): [number, number, string, number] | null {
    const n = name.replace(/[\\@]/g, '\\$&'), re = new RegExp(`\\\\renewcommand\\*?\\s*(?:\\{\\s*${n}\\s*\\}|${n}(?![A-Za-z@]))\\s*\\{`, 'g');
    for (const m of text.matchAll(re)) {
      const open = m.index + m[0].length - 1, e = closing(text, open, '{', '}');
      if (e < 0 || commented(text, m.index) || !lang !== !text.slice(Math.max(0, m.index - 80), m.index).includes(`\\captions${lang}`)) continue;
      return [open + 1, e, value, open + 1];
    }
    const at = preambleEnd(text);
    if (at < 0) return null;
    const head = `${lang ? `\\addto\\captions${lang}{` : ''}\\renewcommand{${name}}{`;
    return [at, at, `${head}${value}}${lang ? '}' : ''}\n`, at + head.length];
  }
  /** A contents list's (`toc`, `lof`, `lot`) leaders or page numbers changed: `what` `none` (no dots), a number (the
   * dots' spacing in mu, 4.5 the kernel's), `nopage` (no page numbers, tocloft's). With tocloft (`tocloft`) per
   * entry kind in the preamble; else the kernel's `\\@dotsep`, set in the list's own file (`\\addtocontents` read
   * in the list's group) after `\\begin{document}`. A line written before for the same setting is rewritten. */
  function leaders(text: string, kind: string, what: string, tocloft: boolean, chapters = false): [number, number, string] | null {
    const page = what === 'nopage', kinds = { toc: [...chapters ? ['chap'] : [], 'sec', 'subsec', 'subsubsec'], lof: ['fig'], lot: ['tab'] }[kind];
    if (!kinds || !page && !(what === 'none' || +what > 0)) return null;
    const v = what === 'none' ? (tocloft ? '\\cftnodots' : '10000') : what;
    const line = page ? kinds.map((k) => `\\cftpagenumbersoff{${k}}`).join('')
      : tocloft ? kinds.map((k) => `\\renewcommand{\\cft${k}dotsep}{${v}}`).join('')
        : `\\makeatletter\\addtocontents{${kind}}{\\protect\\renewcommand\\protect\\@dotsep{${v}}}\\makeatother`;
    // the same setting written before: its line, values aside
    const key = (l: string) => l.trim().replace(/\{(?:\\cftnodots|[\d.]+)\}/g, '{}');
    let a = 0;
    for (const l of text.split('\n')) { if (key(l) === key(line)) return [a, a + l.length, line]; a += l.length + 1; }
    const end = preambleEnd(text);
    if (end < 0) return null;
    const after = text.indexOf('\n', end) + 1;
    return page || tocloft ? [end, end, `${line}\n`] : after > 0 ? [after, after, `${line}\n`] : null;
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
  /** Where the call whose argument closes at `at` (a `}`) ends: past that `}` and the brace or bracket arguments right
   * after it. A glyph set past an argument's last character and not spelled after it (`\\greet{world}` setting
   * `world!`) is the macro body's: typing after it goes there, after the call. Null where `at` closes no argument
   * (no `}`, or a group no `\\name` opens: `{\\bf world}`). */
  function afterCall(text: string, at: number): number | null {
    if (text[at] !== '}') return null;
    // the `{` that `}` closes, then the arguments before it back to the macro's name
    const open = (end: number, o: string, c: string) => { for (let d = 0, i = end; i >= 0; i--) { if (text[i] === c && text[i - 1] !== '\\') d++; else if (text[i] === o && text[i - 1] !== '\\' && --d === 0) return i; } return -1; };
    let a = open(at, '{', '}');
    while (a > 0 && /[}\]]/.test(text[a - 1]!)) a = text[a - 1] === '}' ? open(a - 1, '{', '}') : open(a - 1, '[', ']');
    if (a <= 0 || !/\\[A-Za-z@]+\*?$/.test(text.slice(Math.max(0, a - 40), a))) return null;
    // the arguments after it: `{…}` or `[…]` groups with no space between
    let e = at + 1;
    for (;;) {
      const c = text[e] === '{' ? '}' : text[e] === '[' ? ']' : null;
      if (!c) return e;
      let d = 0, i = e;
      for (; i < text.length; i++) { if (text[i] === text[e] && text[i - 1] !== '\\') d++; else if (text[i] === c && text[i - 1] !== '\\' && --d === 0) break; }
      if (i >= text.length) return e;
      e = i + 1;
    }
  }
  /** Where the first argument of a use of macro `name` (`\greet`) starts, on line `line` (1-based) or a neighbour.
   * None for a macro the text defines without parameters: the `{}` of `\model{}` is an empty group, no argument. */
  function argAt(text: string, name: string, line: number): number | null {
    const q = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const def = new RegExp(`\\\\(?:re|provide)?newcommand\\*?\\s*\\{?\\s*${q}(?![A-Za-z@])\\s*\\}?\\s*(?:\\[\\s*(\\d))?|\\\\[gex]?def\\s*${q}(?![A-Za-z@])([^{]*)\\{|\\\\(?:New|Renew|Provide|Declare)DocumentCommand\\s*\\{?\\s*${q}(?![A-Za-z@])\\s*\\}?\\s*\\{([^}]*)\\}`).exec(text);
    if (def && !(+(def[1] ?? 0) > 0) && !def[2]?.includes('#') && !def[3]?.trim()) return null;
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
  }  /** The macro called at offset `at` (a site inside `\name`) and the definitions of it whose body spells `word`:
   * text a parameterless macro sets (`\def\tleapy{not }`), typed into where it is written. In source order,
   * each as [the macro, the word's offset, its 1-based line]; none when `at` is no call. */
  function defining(text: string, at: number, word: string): [string, number, number][] {
    let a = at;
    while (a > 0 && /[A-Za-z@]/.test(text[a] ?? '') && text[a - 1] !== '\\') a--;
    if (text[a - 1] === '\\') a--;
    const name = /^\\[A-Za-z@]+/.exec(text.slice(a))?.[0];
    if (!name || at > a + name.length) return [];
    const q = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), re = new RegExp(`\\\\[gex]?def\\s*${q}(?![A-Za-z@])\\s*\\{|\\\\(?:re)?newcommand\\*?\\s*\\{?\\s*${q}\\s*\\}?\\s*\\{`, 'g'), out: [string, number, number][] = [];
    for (const m of text.matchAll(re)) {
      const from = m.index + m[0].length;
      let depth = 1, to = from;
      for (; to < text.length && depth; to++) if (text[to] === '{' && text[to - 1] !== '\\') depth++; else if (text[to] === '}' && text[to - 1] !== '\\') depth--;
      for (const [p] of spans(text.slice(from, to - 1), word)) out.push([name, from + p, text.slice(0, from + p).split('\n').length]);
    }
    return out;
  }

  /** A row's words (`[left, right, text]`) after word `k` became `now`: it takes `width(now)` scaled as the old
   * word was set, the words after it move by the difference, the gaps stay. */
  /** Greedy line breaking of `pieces` from `x0` on the first row (TeX without hyphenation): a piece that would pass the
   * right edge starts the next row, the paragraph's next page row (its words follow, so overflow runs on row by row),
   * past the last one a new row a line below; a `par` piece a new paragraph's row, indented, `parskip` lower. A piece
   * wider than a line stands alone on its row. With `pull` a row takes up the next row's words while they fit (a
   * deletion), its row then freed if all went. A `disp` piece stands centered on a new row, what follows it on the
   * next; past it no page row is taken (the page below is pushed down). `shrink`: the share of each space a full row
   * may give up (TeX's shrink, a third for Computer Modern), the row then set to the edge; with `justify` a row broken
   * for the next ends at the edge as TeX sets it (not the first, whose words before `x0` are the page's). Pure:
   * positions only, in bp. */
  /** TeX's total-fit line breaking (Knuth and Plass) of one paragraph's `pieces`: a break before a piece with a space
   * (`gap`, its `st` stretch and `sh` shrink, else half and a third of it, Computer Modern's) or after a hyphen
   * (`hy`, penalty 50); the first line `first` wide, the rest `width`; each line's badness 100·r³ for its glue's
   * ratio r, at most `tolerance` (TeX's 200), demerits (10 + badness)² (+ 50² at a hyphen), the last line filled
   * (`\\parfillskip`). The breaks with least total demerits: the pieces starting each line after the first, and each
   * line's glue ratio (0 for the last). Null when no break sequence keeps every line within the tolerance. */
  function fit(pieces: Piece[], first: number, width: number, tolerance = 200): { breaks: number[]; ratios: number[] } | null {
    const n = pieces.length, st = (q: Piece) => q.st ?? q.gap / 2, sh = (q: Piece) => q.sh ?? q.gap / 3;
    // prefix sums of widths, stretch and shrink, the glue before piece j counted from j on
    const W = [0], S = [0], H = [0];
    pieces.forEach((q, j) => { W.push(W[j]! + q.n + (j ? q.gap : 0)); S.push(S[j]! + (j ? st(q) : 0)); H.push(H[j]! + (j ? sh(q) : 0)); });
    const can = (j: number) => j > 0 && (pieces[j]!.gap > 0 || !!pieces[j - 1]!.hy);
    // best[j]: least demerits of lines covering pieces 0..j-1 with a break before j (j = n: the end)
    const best: number[] = Array(n + 1).fill(Infinity), from: number[] = Array(n + 1).fill(-1), ratio: number[] = Array(n + 1).fill(0);
    best[0] = 0;
    for (let j = 1; j <= n; j++) {
      if (j < n && !can(j)) continue;
      for (let i = j - 1; i >= 0; i--) {
        if (best[i] === Infinity || (i && !can(i))) continue;
        // the line i..j-1: its glue from i+1 on (the space before i is dropped at a break)
        // a line ending at a hyphenation point carries the hyphen (`hw` wide)
        const L = i ? width : first, w = W[j]! - W[i]! - (i ? pieces[i]!.gap : 0) + (j < n && pieces[j - 1]!.hy ? pieces[j - 1]!.hw ?? 0 : 0), str = S[j]! - S[i + 1]!, shr = H[j]! - H[i + 1]!;
        if (w - shr > Math.max(first, width) + 0.01) break; // a line from further left is longer still: none fits
        if (w - shr > L + 0.01) continue;
        const last = j === n, r = last ? (w <= L ? 0 : (L - w) / shr) : w < L ? (str > 0 ? (L - w) / str : Infinity) : w > L ? (L - w) / shr : 0;
        if (r < -1) continue;
        const bad = Math.min(10000, 100 * Math.abs(r) ** 3);
        if (bad > tolerance) continue;
        const d = best[i]! + (10 + bad) ** 2 + (pieces[j - 1]?.hy && !last ? 50 ** 2 : 0);
        if (d < best[j]!) { best[j] = d; from[j] = i; ratio[j] = r; }
      }
    }
    if (best[n] === Infinity) return null;
    const breaks: number[] = [], ratios: number[] = [];
    for (let j = n; j > 0; j = from[j]!) { ratios.unshift(ratio[j]!); if (from[j]! > 0) breaks.unshift(from[j]!); }
    return { breaks, ratios };
  }
  /** The interword glue after word `before` for a font whose space is `space` bp, as TeX's \\nonfrenchspacing sets
   * it with Computer Modern's proportions (stretch half the space, shrink a third, extra space a third): scaled by the
   * space factor of the word's last character (3000 after `.?!`, 2000 `:`, 1500 `;`, 1250 `,`; a closing bracket or
   * quote keeps the one before it; a capital before the stop 999), the extra space from 2000 on. */
  function glue(before: string, space: number): { gap: number; st: number; sh: number } {
    const t = before.replace(/[)\]'’”"]+$/, ''), c = t.at(-1) ?? '';
    const sf = /[.?!]/.test(c) && /\p{Lu}/u.test(t.at(-2) ?? '') ? 999 : ({ '.': 3000, '?': 3000, '!': 3000, ':': 2000, ';': 1500, ',': 1250 } as Record<string, number>)[c] ?? 1000;
    return { gap: space + (sf >= 2000 ? space / 3 : 0), st: space / 2 * sf / 1000, sh: space / 3 * 1000 / sf };
  }
  /** The glue ratio that sets pieces `a..b` (a line, the hyphen after a `hy` end) `L` wide: stretch below, shrink
   * above, the last line (`last`) natural unless it is too wide. */
  function setRatio(all: Piece[], a: number, b: number, L: number, last: boolean): number {
    let w = 0, str = 0, shr = 0;
    for (let j = a; j < b; j++) {
      const q = all[j]!;
      if (j > a) { w += q.gap; str += q.st ?? q.gap / 2; shr += q.sh ?? q.gap / 3; }
      w += q.n;
    }
    if (!last && all[b - 1]?.hy) w += all[b - 1]!.hw ?? 0;
    if (w > L) return shr > 0 ? (L - w) / shr : 0;
    return last || str <= 0 ? 0 : (L - w) / str;
  }
  function reflow<P extends Piece>(pieces: P[], x0: number, line: Line<P>): Flow<P> {
    // `fit`: TeX's total fit over the typed pieces and every page row after them, each line but the last justified;
    // greedy below where nothing fits within the tolerance, or a new paragraph or display is typed
    const all = line.fit && !pieces.some((q) => q.par || q.disp != null) ? [...pieces, ...line.rows.flatMap((r) => r.pieces)] : null;
    // `breaks`: the lines TeX's own breaker gave for these pieces (`satex/break`), else the echo's total fit
    const given = !!all && !!line.breaks && line.breaks.breaks.every((b) => b > 0 && b < all.length);
    const k = all && (given ? line.breaks : fit(all, line.right - x0, line.right - line.left));
    if (all && k) {
      const starts = [0, ...k.breaks], rows: Flow<P>['rows'] = starts.map((a, li) => {
        const b = starts[li + 1] ?? all.length, slot = li ? line.rows[li - 1] : null, out: (P & { x: number })[] = [];
        let x = li ? slot?.x ?? line.left : x0;
        // given breaks (TeX's, from its own widths) set with these pieces' widths: each line but the last justified to
        // the edge, so a piece measured wider than TeX's never runs past it
        const r = given ? setRatio(all, a, b, line.right - x, b === all.length) : k.ratios[li]!;
        for (let j = a; j < b; j++) {
          const q = all[j] as P;
          if (j > a) x += q.gap + r * (r > 0 ? q.st ?? q.gap / 2 : q.sh ?? q.gap / 3);
          out.push({ ...q, x });
          x += q.n;
        }
        // a line broken at a hyphenation point ends with the hyphen
        const end = all[b - 1];
        if (b < all.length && end?.hy) out.push({ gap: 0, n: end.hw ?? 0, t: '-', x } as unknown as P & { x: number });
        return { x: li ? slot?.x ?? line.left : x0, from: li ? (slot ? li - 1 : -1) : -2, dy: li && !slot ? line.skip : 0, par: false, out };
      });
      return { rows, freed: line.rows.map((_, i) => i).filter((i) => i >= starts.length - 1) };
    }
    const queue = [...pieces], rows: Flow<P>['rows'] = [{ x: x0, from: -2, dy: 0, par: false, out: [] }], slots: { x: number; from: number }[] = [];
    let ri = 0, x = x0, i = 0, after: number | null = null, sunk = false, gaps = 0;
    const shrink = line.shrink ?? 0;
    const take = () => { const r = line.rows[ri]!; queue.push(...r.pieces); return { x: r.x, from: ri++ }; };
    const lay = () => {
      for (; i < queue.length; i++) {
        const q = queue[i]!;
        let r = rows.at(-1)!;
        const own = q.disp != null || after != null;
        if (own || r.out.length && (q.par || q.n > 0 && x + q.gap + q.n - shrink * (gaps + q.gap) > line.right + 0.5)) {
          sunk ||= own;
          const s = !q.par && !sunk && (slots.shift() ?? (ri < line.rows.length ? take() : null));
          const at = q.disp != null ? (line.left + line.right - q.n) / 2 : line.left + (q.par ? line.indent : 0);
          // TeX's short skip above a display whose row before ends left of it (an empty row after Enter): no
          // \abovedisplayskip, only what a tall formula adds
          const dy = line.skip + (q.disp != null && r.out.every((o) => o.x + o.n <= at) ? q.dshort ?? q.disp : q.disp ?? after ?? (q.par ? line.parskip : 0));
          rows.push(r = s ? { x: s.x, from: s.from, dy: 0, par: false, out: [] } : { x: at, from: -1, dy, par: !!q.par, out: [] });
          x = r.x - (own ? 0 : q.gap);
          gaps = own ? 0 : -q.gap;
        }
        after = q.disp != null ? q.below ?? 0 : null;
        if (own) { r.out.push({ ...q, gap: 0, x }); x += q.n; continue; }
        x += q.gap;
        if (r.out.length) gaps += q.gap;
        r.out.push({ ...q, x });
        x += q.n;
      }
    };
    lay();
    // a row with room takes up the next row's words (one deleted, the rest shorter): their row then waits for overflow
    while (line.pull && !sunk && ri < line.rows.length) {
      const first = line.rows[ri]!.pieces[0];
      if (first && x + first.gap + first.n > line.right + 0.5) break;
      slots.push(take());
      lay();
    }
    // a row past the edge (its spaces shrunk) or broken for the next (justified) ends at the edge: its spaces share it
    rows.forEach((r, j) => {
      const last = r.out.at(-1), next = rows[j + 1];
      if (!last || r.out.length < 2) return;
      const d = line.right - (last.x + last.n), room = r.out.slice(1).reduce((n, q) => n + q.gap, 0);
      if (!room || !(shrink && d < -0.01 || line.justify && j > 0 && next && !next.par && next.out[0]?.disp == null && d > 0)) return;
      let acc = 0;
      for (const q of r.out.slice(1)) { acc += q.gap; q.x += d * acc / room; }
    });
    return { rows, freed: slots.map((s) => s.from) };
  }
  /** How far a centered row moves when it grows by `growth`: half of it, for a row as far from both edges of its line
   * box `box` (more than 4 bp, within 2 bp of each other); else 0. */
  function centered(box: [number, number], row: [number, number], growth: number): number {
    const l = row[0] - box[0], r = box[1] - row[1];
    return l > 4 && Math.abs(l - r) < 2 ? growth / 2 : 0;
  }
  function relaid(row: [number, number, string][], k: number, now: string, width: (s: string) => number): [number, number, string][] {
    const [l, r, was] = row[k]!, scale = width(was) > 0 ? (r - l) / width(was) : 0, d = width(now) * scale - (r - l);
    return row.map(([a, b, t], i) => i < k ? [a, b, t] : i === k ? [a, b + d, now] : [a + d, b + d, t]);
  }
  /** `text` with what `from..to` writes but sets elsewhere or nowhere blanked (offsets kept): a sectioning command's or
   * caption's short title (`\\section[Short]{…}`, set in the contents) and the argument of `\\label` and `\\index`. */
  function unset(text: string, from: number, to: number): string {
    const re = /\\(?:(part|chapter|section|subsection|subsubsection|paragraph|caption)\*?\s*\[|(label|index)\s*\{)/g, cut: [number, number][] = [];
    re.lastIndex = from;
    for (let m; (m = re.exec(text)) && m.index < to;) {
      const open = re.lastIndex - 1, close = text[open] === '[' ? ']' : '}';
      let d = 0, e = -1;
      for (let j = open; j < text.length && j < to + 4096; j++) { if (text[j] === '\\') { j++; continue; } if (text[j] === text[open]) d++; else if (text[j] === close && --d === 0) { e = j; break; } }
      if (e < 0) continue;
      cut.push([m[1] ? open : m.index, e + 1]);
      re.lastIndex = e + 1;
    }
    let out = '', at = 0;
    for (const [a, b] of cut) { out += text.slice(at, a) + ' '.repeat(b - a); at = b; }
    return out + text.slice(at);
  }
  /** The words a paragraph's source `from..to` sets, by offset: runs between spaces (`~` too), commands, braces and
   * math shifts left out; a comment is no word. A word spelled by markup alone (`\\par`, `{}`) sets nothing. */
  function tokens(text: string, from: number, to: number): Tok[] {
    const out: Tok[] = [], re = /(?<!\\)%[^\n]*|(?:\\%|[^\s~%])+/g, src = unset(text, from, to);
    re.lastIndex = from;
    for (let m; (m = re.exec(src)) && m.index < to;) {
      if (m[0][0] === '%') continue;
      const t = m[0].replace(/\\\\|\\[A-Za-z@]+\*?|\\(?![A-Za-z@])|[{}$^_]/g, '');
      if (t) out.push({ s: m.index, e: m.index + m[0].length, t });
    }
    return out;
  }
  /** Source words `toks` matched to the page's words `words` (both in reading order): per word of the source, the
   * index of the page word that shows it, -1 where none does. Equal spellings pair up in order; one missing or extra
   * on either side (a macro's text, a dropped command) is skipped by looking a few words ahead; a word the page
   * hyphenated across rows (`consec-` `tetur`) is its first part; one that starts the other (`here.\footnote{A` is
   * `here.A`, the page sets `here.` and the `A` in the footnote) pairs with it before looking ahead. */
  function aligned(toks: string[], words: string[]): number[] {
    const n = (t: string) => t.normalize('NFKC').replace(/[\u2010-\u2014]/g, '-');
    const walk = (A: string[], B: string[], hyph: boolean) => {
      const out = A.map(() => -1), eq = (i: number, j: number) => A[i] === B[j] || hyph && B[j]!.endsWith('-') && j + 1 < B.length && A[i] === B[j]!.slice(0, -1) + B[j + 1];
      const LOOK = 4;
      for (let i = 0, j = 0; i < A.length && j < B.length;) {
        if (eq(i, j)) { out[i] = j; j += A[i] === B[j] ? 1 : 2; i++; continue; }
        const a = A[i]!, b = B[j]!;
        if (Math.min(a.length, b.length) >= 3 && (a.startsWith(b) || b.startsWith(a))) { out[i++] = j++; continue; }
        let step: [number, number] | null = null;
        for (let k = 1; k <= LOOK && !step; k++) step = j + k < B.length && eq(i, j + k) ? [0, k] : i + k < A.length && eq(i + k, j) ? [k, 0] : null;
        if (step) { i += step[0]; j += step[1]; } else { i++; j++; }
      }
      return out;
    };
    const A = toks.map(n), B = words.map(n), out = walk(A, B, true);
    // the same walk from the end: words after a stretch the page sets apart (a display's glyphs, more words than the
    // source has) pair from their side, where the walk from the start lost step (`\] ok`: "ok" paired with nothing)
    const back = walk([...A].reverse(), [...B].reverse(), false).reverse().map((j) => j < 0 ? -1 : B.length - 1 - j);
    for (let i = A.length - 1, hi = B.length; i >= 0; i--) {
      if (out[i]! < 0 && back[i]! >= 0 && back[i]! < hi && !out.slice(0, i).some((j) => j >= back[i]!)) out[i] = back[i]!;
      if (out[i]! >= 0) hi = out[i]!;
    }
    return out;
  }
  /** The offset of LSP position `p` (0-based line, UTF-16 character) in `text`: a character past its line's end is the
   * line's end (LSP's rule), a line past the text its end. */
  function offsetAt(text: string, p: { line: number; character: number }): number {
    let i = 0;
    for (let l = 0; l < p.line; l++) { const n = text.indexOf('\n', i); if (n < 0) return text.length; i = n + 1; }
    const e = text.indexOf('\n', i);
    return Math.min(i + Math.max(0, p.character), e < 0 ? text.length : e);
  }
  /** The words of a paragraph's text in the view, split where math meets text (`$x_{42}$Generally`: `x42`, `Generally`):
   * `math(i)` says whether character `i` is set in math. Each a match-like `[word]` with its `index`. */
  function units(text: string, math: (i: number) => boolean): (string[] & { index: number })[] {
    return [...text.matchAll(/[\p{L}\p{N}]+/gu)].flatMap((m) => {
      const out: (string[] & { index: number })[] = [];
      for (let s = 0, i = 1; i <= m[0].length; i++) if (i === m[0].length || math(m.index + i) !== math(m.index + i - 1)) { out.push(Object.assign([m[0].slice(s, i)], { index: m.index + s })); s = i; }
      return out;
    });
  }
  /** Where `now` differs from `was`: the length of their common start (`by` at most, where a change is known to start
   * at the latest), and of their common end after it. */
  function changed(was: string, now: string, by = Infinity): [number, number] {
    let p = 0, q = 0;
    const m = Math.min(was.length, now.length);
    while (p < m && p < by && was[p] === now[p]) p++;
    while (q < m - p && was[was.length - 1 - q] === now[now.length - 1 - q]) q++;
    return [p, q];
  }
  /** The paragraph around offset `at`: from after the blank line before it to the blank line after it, and within the
   * lines between environment edges (a line with `\\begin{` or `\\end{` other than `at`'s own): a float's caption is
   * set apart from the text above the float. */
  function paragraph(text: string, at: number): [number, number] {
    const before = text.slice(0, at).search(/\n[ \t]*\n(?![\s\S]*\n[ \t]*\n)/), after = /\n[ \t]*\n/.exec(text.slice(at));
    let p0 = before < 0 ? 0 : text.indexOf('\n', before + 1) + 1, p1 = after ? at + after.index : text.length;
    // an environment's edge or a heading's line bounds it too: TeX sets a heading apart from the text after it
    const edge = /\\(begin|end)\s*\{|\\(part|chapter|(sub)*section|paragraph)\*?\s*[[{]/, l0 = text.lastIndexOf('\n', at - 1) + 1, e0 = text.indexOf('\n', at);
    for (let e = l0 - 1; e > p0;) { const s = text.lastIndexOf('\n', e - 1) + 1; if (edge.test(text.slice(s, e))) { p0 = e + 1; break; } e = s - 1; }
    for (let s = e0 < 0 ? p1 : e0 + 1; s < p1;) { const e = text.indexOf('\n', s), end = e < 0 || e > p1 ? p1 : e; if (edge.test(text.slice(s, end))) { p1 = Math.max(s - 1, at); break; } s = end + 1; }
    // a heading's own line ends it: the text after the heading is the next paragraph
    if (e0 >= 0 && e0 < p1 && /\\(part|chapter|(sub)*section|paragraph)\*?\s*[[{]/.test(text.slice(l0, e0))) p1 = e0;
    return [p0, p1];
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
  /** The node contents of the forest tree around offset `at` (its `\begin` to its `\end` line), in reading order
   * (a level per row, left to right): each `[from, to)` the text after a `[` up to its first `[`, `]` or comma at
   * brace level 0, white space and one pair of outer braces off. Forest sets them all at its `\end`, so SyncTeX
   * names that line for each: the n-th same word on the page is the n-th node spelling it. */
  function treeNodes(text: string, at: number): [number, number][] {
    const b = text.lastIndexOf('\\begin{forest}', at), e = text.indexOf('\\end{forest}', b);
    if (b < 0 || e < 0 || at > e) return [];
    const out: [number, number, number][] = [];
    let depth = 0, group = 0;
    for (let i = b + 14; i < e; i++) {
      const c = text[i];
      if (c === '\\') { i++; continue; }
      if (c === '%') { while (i < e && text[i] !== '\n') i++; continue; }
      if (c === '{') group++; else if (c === '}') group--;
      else if (c === ']' && !group) depth--;
      else if (c === '[' && !group) {
        let j = i + 1, g = 0;
        for (; j < e; j++) { const d = text[j]; if (d === '\\') j++; else if (d === '{') g++; else if (d === '}') g--; else if (!g && (d === '[' || d === ']' || d === ',')) break; }
        let s0 = i + 1, s1 = j;
        while (s0 < s1 && /\s/.test(text[s0]!)) s0++;
        while (s1 > s0 && /\s/.test(text[s1 - 1]!)) s1--;
        if (text[s0] === '{' && text[s1 - 1] === '}') { s0++; s1--; }
        if (s1 > s0) out.push([s0, s1, depth]);
        depth++;
      }
    }
    return out.map((n, k) => [n, k] as const).sort((p, q) => p[0][2] - q[0][2] || p[1] - q[1]).map(([n]) => [n[0], n[1]]);
  }
  /** `text`'s characters `from..to` as runs of prose and math: `kind` '' (prose), 'inline' or 'display', the math's
   * body `b..be` inside its delimiters (to `to` while its closer is not typed yet). Read as scan reads it, from the
   * paragraph's start (no formula spans a blank line): per key, never the whole file. */
  function mathRuns(text: string, from: number, to: number): { s: number; e: number; kind: string; b: number; be: number }[] {
    const marks: Mark[] = [], out: { s: number; e: number; kind: string; b: number; be: number }[] = [];
    let open = scan(text, Math.max(0, text.lastIndexOf('\n\n', from - 1)), from, start()).math, s = from, b = from;
    scan(text, from, to, start(), marks);
    const kind = (m: string) => /^(\$|\\\(|math)$/.test(m) ? 'inline' : 'display';
    const run = (e: number, be: number) => { if (e > s) out.push({ s, e, kind: open ? kind(open) : '', b, be }); s = e; };
    for (const m of marks) {
      const env = /^\\(begin|end)\{(.*)\}$/.exec(m.t);
      if (!open && (/^(\$\$?|\\[([])$/.test(m.t) || env?.[1] === 'begin' && MATH.test(env[2]!))) { run(m.s, m.s); open = env ? env[2]! : m.t; b = m.e; }
      else if (open && (m.t === open && m.t[0] === '$' || m.t === { '\\(': '\\)', '\\[': '\\]' }[open] || env?.[1] === 'end' && env[2] === open)) { run(m.e, m.s); open = ''; b = s; }
    }
    run(to, to);
    return out;
  }
  // the footer typed in the PDF (playground `pdfFootAt`): the call that sets a cell, the page number in it
  /** The text of the last call that sets footer cell `letter` on odd pages (fancyhdr's `\fancyfoot[C]{…}`, `\cfoot{…}`
   * for L, C, R; scrlayer-scrpage's `\cfoot[…]{…}` for i, c, o): `[from, to]` inside its `{…}`, null where none does. */
  function footArg(text: string, pkg: string, letter: string): [number, number] | null {
    const koma = pkg === 'scrlayer-scrpage', re = koma ? new RegExp(`\\\\${letter}foot(?![A-Za-z@])\\*?\\s*(?:\\[[^\\]]*\\])?\\s*\\{`, 'g')
      : /\\(?:fancyfoot|([lcr])foot)(?![A-Za-z@])\s*(?:\[([^\]]*)\])?\s*\{/g;
    // a selector names the cell (or no column) and odd pages (or no side)
    const sel = (t: string) => t.split(',').some((x) => { const u = x.trim().toUpperCase(); return (!/[LCR]/.test(u) || u.includes(letter)) && (!/[EO]/.test(u) || u.includes('O')); });
    let got: [number, number] | null = null;
    for (const m of text.matchAll(re)) {
      if (commented(text, m.index) || !koma && (m[1] ? m[1] !== letter.toLowerCase() : m[2] != null && !sel(m[2]))) continue;
      const open = m.index + m[0].length - 1, e = closing(text, open, '{', '}');
      if (e >= 0) got = [open + 1, e];
    }
    return got;
  }
  /** What Backspace (`back`) or Delete takes in a footer cell at `at`: the page number's command whole (`\thepage`,
   * `\pagemark`, with its `{}` and the space after it), null where none is next to the caret. */
  function footBack(text: string, at: number, back: boolean): [number, number] | null {
    const m = back ? /\\(?:thepage|pagemark)(?:\{\}| ?)$/.exec(text.slice(Math.max(0, at - 16), at)) : /^ ?\\(?:thepage|pagemark)(?![A-Za-z@])(?:\{\}| ?)/.exec(text.slice(at, at + 16));
    if (!m || back && /[A-Za-z@]/.test(text[at] ?? '') && !m[0].endsWith('}') && !m[0].endsWith(' ')) return null;
    return back ? [at - m[0].length, at] : [at, at + m[0].length];
  }
  /** A footer cell's text `t` as set on page `page`: the page number for `\thepage` (`\pagemark`), other commands dropped. */
  const footShown = (t: string, page: string) => t.replace(/\\(?:thepage|pagemark)(?:\{\}|(?![A-Za-z@])\s*)/g, '\u0001').replace(/\\([^A-Za-z@])/g, '$1')
    .replace(/\\[A-Za-z@]+\*?\s*/g, '').replace(/[{}]/g, '').replace(/~/g, ' ').replace(/\u0001/g, page);
  /** Where a click on empty page space puts the caret: `rows` the page's lines (`t`, `d`, `base`, `l`, `r` in bp, `line`
   * the 1-based line of `text` SyncTeX sets each from), `(x, y)` the click. `{kind: 'end' | 'start' | 'near', row}` on
   * or between a paragraph's lines (its end, its start, the nearest word); `{kind: 'new', at, body, caret, ghost}` a
   * new paragraph the first key writes (`body` at `at`, the caret at `caret`): after the paragraph above (a heading's
   * line, a float's `\end`), before the page's first one; `ghost` where its first line starts (`x`, `base`, `h`). */
  function blankPlace(text: string, rows: { t: number; d: number; base: number; l: number; r: number; line: number }[], x: number, y: number) {
    const rs = [...rows].sort((a, b) => a.base - b.base);
    if (!rs.length) return null;
    const starts = [0];
    for (let i = text.indexOf('\n'); i >= 0; i = text.indexOf('\n', i + 1)) starts.push(i + 1);
    const off = (n: number) => starts[Math.min(Math.max(n, 1), starts.length) - 1]!, lineOf = (n: number) => text.slice(off(n), (starts[n] ?? text.length + 1) - 1);
    // SyncTeX names the line where TeX ended the paragraph: the blank line after it, `\end{document}`: the text's line before
    const src = (n: number) => { while (n > 1 && (!lineOf(n).trim() || /^\s*\\end\s*\{document\}/.test(lineOf(n)))) n--; return n; };
    const heading = (r: { line: number }) => /^\s*\\(part|chapter|(sub)*section|paragraph)\*?\s*[[{]/.test(lineOf(src(r.line)));
    // a float the row is set in: its `\end` line's end
    const floatEnd = (r: { line: number }) => {
      const at = off(src(r.line)), re = /\\(begin|end)\s*\{(figure|table|wrapfigure|wraptable|algorithm)\*?\}/g, open: number[] = [];
      for (const m of text.matchAll(re)) {
        if (m[1] === 'begin') { if (m.index > at) break; open.push(m.index); continue; }
        if (open.pop() != null && m.index >= at && !open.length) { const e = text.indexOf('\n', m.index); return e < 0 ? text.length : e; }
      }
      return -1;
    };
    const para = (r: { line: number }) => paragraph(text, off(src(r.line)));
    const med = (xs: number[], d: number) => xs.length ? xs.sort((p, q) => p - q)[xs.length >> 1]! : d;
    const left = Math.min(...rs.map((r) => r.l)), skip = med(rs.slice(1).map((r, i) => r.base - rs[i]!.base).filter((d) => d > 4 && d < 30), 12);
    const indent = med(rs.map((r) => r.l - left).filter((d) => d > 3 && d < 40), 15);
    const on = rs.find((r) => y >= r.t && y <= r.d);
    if (on) return { kind: x > on.r ? 'end' : x < on.l ? 'start' : 'near', row: rows.indexOf(on) };
    const i = rs.findLastIndex((r) => r.d < y), above = rs[i], below = rs[i + 1];
    const near = (r: typeof rs[0]) => ({ kind: 'near', row: rows.indexOf(r) });
    if (!above) {
      // above the page's first line: a new paragraph before it where one starts there (a heading, an indented line)
      if (!(heading(below!) || below!.l - left > 3)) return near(below!);
      const p0 = para(below!)[0];
      return { kind: 'new', at: p0, body: '\n\n', caret: p0, ghost: { x: left + indent, base: below!.base - skip, h: below!.d - below!.t } };
    }
    // between two lines of one paragraph: its nearest line
    const f = floatEnd(above), end = f >= 0 ? f : para(above)[1];
    if (below && f < 0 && !heading(above) && para(below)[0] === para(above)[0]) return near(y - above.d < below.t - y ? above : below);
    // a text line right after it gets a blank line of its own: the new paragraph never runs into it
    const next = /^\n([^\n]*)/.exec(text.slice(end))?.[1] ?? '', body = next.trim() && !/^\s*\\end\b/.test(next) ? '\n\n\n' : '\n\n';
    return { kind: 'new', at: end, body, caret: end + 2, ghost: { x: left + (heading(above) ? 0 : indent), base: above.base + skip, h: above.d - above.t } };
  }
  /** Whether page word `v` stands on the row of word `r`: its baseline; raised within its height and narrower than it
   * is tall (a \\thanks ∗, a footnote's number); or dipped below it by less than a third of the row's height, its top
   * above the baseline (the \\TeX logo's lowered E: `TEXbook` stood on no row and the echo dropped the row's rest). */
  function onRow(v: Word, r: Word): boolean {
    const h = r[4] - r[2];
    return Math.abs(v[4] - r[4]) < 1 || v[4] < r[4] && v[4] > r[2] && v[3] - v[1] < h || v[4] > r[4] && v[4] - r[4] < h / 3 && v[2] < r[4] - h / 2;
  }
  /** The words of row `line` (in order) from word `w` on, while they follow each other (a gap under 3 em of their
   * height: never another column's or node's words on the same baseline); `back`: from the first of them before `w`. */
  function run(line: Word[], w: Word, back = false): Word[] {
    let i = line.findIndex((v) => v[1] >= w[1] - 0.5);
    const em = 3 * (w[4] - w[2]);
    while (back && i > 0 && line[i]![1] - line[i - 1]![3] < em) i--;
    const out = i < 0 ? [] : [line[i]!];
    for (let j = i + 1; i >= 0 && j < line.length && line[j]![1] - line[j - 1]![3] < em; j++) out.push(line[j]!);
    return out;
  }
  /** Whether `at` stands right after a display formula's closer (`\\]`, `$$`, `\\end{equation*}` and the like). */
  function displayEnd(text: string, at: number): boolean {
    const m = /(\\\]|\$\$|\\end\{([A-Za-z]+)\*?\})[ \t]*$/.exec(text.slice(Math.max(0, at - 40), at));
    return !!m && (!m[2] || MATH.test(m[2]) && m[2] !== 'math');
  }
  self.satexPdfEdit = { onRow, run, displayEnd, offsetAt, units, treeNodes, naming, entries, shortTitle, bothTitles, renamed, leaders, checked, grow, key, broken, scan, start, cells, cellStep, rowAfter, verbatim, verbatimAt, nearest, bands, joined, place, extent, room, form, measure, respec, widthBp, snapWidth, resized, stepped, plain, editable, spans, sender, typed, erased, mathAt, boundaries, boundaryAt, inkSnap, locate, columns, wordAt, argAt, afterCall, defining, relaid, reflow, fit, glue, centered, apart, tokens, unset, aligned, changed, paragraph, mathRuns, footArg, footBack, footShown, blankPlace };
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
    /** Its own options. */
    opts: string[];
  }
  /** A path (`\draw (a) -- (b);`): its statement, its options' place, each segment between two points and where a label goes in it. */
  interface TikzEdge { from: number; to: number; line: number; opts: string[]; segs: TikzSeg[]; pic: TikzPic }
  /** A path's segment from point `a` to `b`: its operation (`--`, `to`, `|-`, `-|`) at `op`, the keys of a `to[…]`, and `at` where a label goes. */
  type TikzSeg = { a: string; b: string; at: number; op: [number, number]; kind: string; keys: string[] };
  /** A picture's scale and units in cm, and the keys its `every node/.style` gives each node. */
  type TikzPic = { scale: number; x: number; y: number; node?: string[]; at?: number; dist?: [number, number] };
  /** A move: by a distance (`by`), to a place (`to`, from the origin), or to `pos`. Lengths land on the 0.5 mm grid, `grid`
   * cm an axis where given (0: as is), off it with `free`. */
  type TikzMove = { by?: [number, number]; to?: [number, number]; pos?: number; free?: boolean; grid?: [number, number] };
  /** A box on a page, in bp, y down. */
  type TikzBox = { l: number; t: number; r: number; d: number };
  /** A guide line at `at` across axis `ax` (0: a vertical line at x) from `from` to `to`; a gap: equal spacings along `ax`, each [from, to, cross]. */
  type TikzSnap = { d: [number, number]; guides: { ax: number; at: number; from: number; to: number }[]; gaps: { ax: number; segs: [number, number, number][] }[] };
  /** A shape's name as TikZ spells it (`regular polygon`), `rounded` for `rounded corners`. */
  type TikzShape = string;
  interface Window {
    satexTikzMove: {
      nodes(text: string): TikzNode[]; move(stmt: string, i: number, pic: TikzPic, m: TikzMove): { text: string; label: string; by: [number, number] } | null;
      len(s: string): TikzLen | null; cm(l: TikzLen, unit: number): number; posOn(a: [number, number], b: [number, number], p: [number, number]): number;
      clipped(a: [number, number], ah: [number, number], b: [number, number], bh: [number, number]): [[number, number], [number, number]];
      snap(b: TikzBox, d: [number, number], others: TikzBox[], tol: number): TikzSnap;
      edges(text: string): TikzEdge[];
      shapeOf(n: TikzNode): TikzShape; shape(stmt: string, i: number, pic: TikzPic, to: TikzShape, extra?: string[]): { text: string; library: string | null } | null;
      looks(n: TikzNode): { draw: boolean; fill: boolean }; flip(stmt: string, i: number, pic: TikzPic, key: 'draw' | 'fill'): string | null;
      retext(stmt: string, i: number, text: string): string | null;
      tipOf(e: TikzEdge): string; tip(stmt: string, tip: string): string | null; label(stmt: string, seg: number, text: string, opts?: string[]): { text: string; at: number } | null;
      SHAPES: [TikzShape, string][]; library(shape: TikzShape): string | null;
      keysAt(stmt: string, i: number): string[] | null; current(stmt: string, i: number, group: string, names?: string[]): string[];
      set(stmt: string, i: number, group: string, add: string[], names?: string[]): string | null;
      segOf(stmt: string, seg: number): TikzSeg | null; route(stmt: string, seg: number, kind: string): string | null;
      bend(stmt: string, seg: number, side: string | null, angle?: number): string | null; inout(stmt: string, seg: number, out: number, into: number): string | null;
      looseness(stmt: string, seg: number, v: number): string | null; segPath(a: [number, number], b: [number, number], g: TikzSeg): [number, number][];
      addEdge(text: string, picAt: number, a: string, b: string, opts?: string): { at: number; text: string } | null;
      named(stmt: string, i: number, name: string): string | null; fresh(text: string, picAt: number, base: string): string;
      picOpts(text: string, picAt: number): { from: number; to: number; keys: string[] } | null; picSet(text: string, picAt: number, group: RegExp, add: string[]): { from: number; to: number; was: string; now: string } | null;
      dependents(text: string, n: TikzNode): { direct: TikzNode[]; all: TikzNode[] };
      relation(b: TikzBox, ref: TikzBox, tol: number, dist: [number, number] | undefined, name: string): string | null; place(stmt: string, i: number, rel: string): string | null;
      snapAngle(deg: number, step: number): number; angle(stmt: string, i: number): number; unlabel(stmt: string, i: number): string | null;
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
  const GRID = 0.05;
  /** `c` cm in the unit of `l` (bare: `unit` cm), on a grid of `step` cm (0: as is, to 0.01 mm), with the digits it needs, at least as written. */
  function write(l: TikzLen, c: number, unit: number, step: number): string {
    const per = l.unit ? UNIT[l.unit] : unit, mm = step ? Math.round(c / step) * step : c;
    let need = l.dec;
    while (need < 5 && Math.abs(+(mm / per).toFixed(need) * per - mm) > (step || 0.01) / 10) need++;
    let s = (mm / per).toFixed(need);
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
      if (s.startsWith('--', k) || s.startsWith('|-', k) || s.startsWith('-|', k)) { op = true; k += 2; continue; }
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
    let scale = 1, x = 1, y = 1, node: string[] = [], dist: [number, number] = [1, 1];
    for (const kv of keys(opts)) {
      const [k, v = ''] = kv.split('=').map((t) => t.trim()), l = len(v);
      if (k === 'scale' && +v) scale = +v;
      if ((k === 'x' || k === 'y') && l) { if (k === 'x') x = cm(l, 1); else y = cm(l, 1); }
      if (k === 'every node/.style') node = keys(kv.slice(kv.indexOf('=') + 1).trim().replace(/^\{|\}$/g, ''));
      // `1cm`, or `1cm and 2cm`: vertical, then horizontal
      if (k === 'node distance') { const d = v.split(/\s+and\s+/).map(len); if (d.every(Boolean)) dist = [cm(d[0]!, 1), cm((d[1] ?? d[0])!, 1)]; }
    }
    return { scale, x, y, node, dist };
  }
  /** Each statement of the text's pictures, comments blanked: its text `s` from offset `at`, its line, its picture. */
  function stmts(text: string): { s: string; at: number; line: (i: number) => number; pic: TikzPic }[] {
    const t = text.replace(/(?<!\\)%.*$/gm, (c) => ' '.repeat(c.length)), out: { s: string; at: number; line: (i: number) => number; pic: TikzPic }[] = [];
    const line = (i: number) => { let n = 1; for (let k = text.indexOf('\n'); k >= 0 && k < i; k = text.indexOf('\n', k + 1)) n++; return n; };
    for (const m of t.matchAll(/\\begin\{tikzpicture\}/g)) {
      let k = m.index! + m[0].length;
      const end = t.indexOf('\\end{tikzpicture}', k);
      if (end < 0) continue;
      let pic = picOf('');
      while (/\s/.test(t[k])) k++;
      if (t[k] === '[') { const e = close(t, k); pic = picOf(t.slice(k + 1, e)); k = e + 1; }
      pic.at = m.index;
      // statements end at `;` outside groups; a loop's statements stand for many nodes, skipped whole
      while (k < end) {
        let e = k, depth = 0;
        for (; e < end; e++) { const c = t[e]; if (c === '\\') { e++; continue; } if ('{[('.includes(c)) depth++; else if ('}])'.includes(c)) depth--; else if (c === ';' && depth <= 0) break; }
        const s = t.slice(k, e + 1), lead = s.length - s.trimStart().length;
        if (!/^\s*\\foreach\b/.test(s)) out.push({ s: s.slice(lead), at: k + lead, line, pic });
        k = e + 1;
      }
    }
    return out;
  }
  /** Each node and path label of the text's pictures, comments blanked. */
  function nodes(text: string): TikzNode[] {
    return stmts(text).flatMap(({ s, at: k, line, pic }) => parts(s).map((p, i) => {
      const o = p.o ? keys(s.slice(p.o[0] + 1, p.o[1])) : [], xy = p.at ? s.slice(p.at[0] + 1, p.at[1]).split(',') : [];
      const at = xy.length === 2 && len(xy[0]) && len(xy[1]) ? [len(xy[0])!, len(xy[1])!] as [TikzLen, TikzLen] : null;
      const along = o.map((x) => x.startsWith('pos') ? +x.split('=')[1] : ALONG[x]).find((v) => v != null && !Number.isNaN(v));
      return { kind: p.kind, from: k, to: k + s.length, i, line: line(k + p.t[0]), name: p.name, text: s.slice(p.t[0] + 1, p.t[1]), at, placed: o.some((x) => PLACED.test(x)),
        plain: !o.some((x) => OFFSET.test(x)) && (!!at || !p.at), seg: p.seg, pos: p.kind === 'label' ? along ?? (p.after ? 1 : 0.5) : null, pic, opts: o };
    }));
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
    const free = !!m.free, step = (j: number) => (free ? 0 : m.grid?.[j] ?? GRID), opts = p.o ? stmt.slice(p.o[0] + 1, p.o[1]) : '';
    /** The statement with options `o` (and `at` text `at`, null to keep). */
    const out = (o: string, at: string | null, label: string, by: [number, number]) => {
      let s = stmt;
      if (at != null) s = p.at ? s.slice(0, p.at[0]) + at + s.slice(p.at[1] + 1) : s.slice(0, p.t[0]) + `at ${at} ` + s.slice(p.t[0]);
      s = p.o ? (o ? s.slice(0, p.o[0] + 1) + o + s.slice(p.o[1]) : s.slice(0, p.o[0]).trimEnd() + s.slice(p.o[1] + 1)) : o ? s.slice(0, p.oAt) + `[${o}]` + s.slice(p.oAt) : s;
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
      const c = m.to.map((v, j) => write(l[j], v / pic.scale, u[j], step(j))), o = setOpts(ks.filter((k) => !PLACED.test(k) && !SHIFTS.test(k)).join(', '), /$^/, []);
      return out(o, `(${c.join(',')})`, `at (${c.join(',')})`, [len(c[0])!, len(c[1])!].map((a, j) => cm(a, u[j])) as [number, number]);
    }
    const by = m.by!.map((v) => v / pic.scale);
    if (at) {
      const c = at.map((a, j) => write(a, cm(a, units[j]) + by[j], units[j], step(j)));
      return out(opts, `(${c.join(',')})`, `at (${c.join(',')})`, c.map((s, j) => cm(len(s)!, units[j]) - cm(at[j], units[j])) as [number, number]);
    }
    // a shift as written (a bare one is in pt), else a new one in mm; a shift moved to nothing goes
    const set: [string, string][] = [], gone: string[] = [], said: string[] = [], moved: number[] = [];
    ['xshift', 'yshift'].forEach((n, j) => {
      const k = ks.find((x) => x.split('=')[0].trim() === n), l = (k && len(k.split('=')[1])) || { v: 0, unit: 'mm', dec: 0 };
      const was = cm(l, UNIT.pt), now = was + by[j], v = write(l.unit ? l : { ...l, unit: 'pt' }, now, 1, step(j));
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
  /** Box `b` moved by `d` snapped to the `others`: its centre or an edge onto theirs, or its gap to a neighbour onto a gap
   * between two of them, each axis to the nearest within `tol`. The guides and equal gaps (bars 4 bp past the boxes) shown while snapped. */
  function snap(b: TikzBox, d: [number, number], others: TikzBox[], tol: number): TikzSnap {
    const out: [number, number] = [d[0], d[1]], lo = (ax: number, o: TikzBox) => (ax ? o.t : o.l), hi = (ax: number, o: TikzBox) => (ax ? o.d : o.r);
    const at = (ax: number, o: TikzBox) => [lo(ax, o), (lo(ax, o) + hi(ax, o)) / 2, hi(ax, o)];
    const moved = (): TikzBox => ({ l: b.l + out[0], r: b.r + out[0], t: b.t + out[1], d: b.d + out[1] });
    const guides: TikzSnap['guides'] = [], gaps: TikzSnap['gaps'] = [];
    for (const ax of [0, 1]) {
      const m = moved(), x = 1 - ax, cross = (o: TikzBox) => lo(x, o) < hi(x, m) && lo(x, m) < hi(x, o);
      let best: { diff: number; gap?: [number, number, number][] } | null = null;
      // centres and equal gaps before edges: an edge wins only when nearer by a third of the tolerance
      let rank = Infinity;
      const take = (diff: number, gap?: [number, number, number][], edge = false) => { const r = Math.abs(diff) + (edge ? tol / 3 : 0); if (Math.abs(diff) <= tol && r < rank - 1e-6) { best = { diff, gap }; rank = r; } };
      for (const o of others) at(ax, o).forEach((v, i) => at(ax, m).forEach((w, j) => take(v - w, undefined, i !== 1 || j !== 1)));
      // equal spacing: the neighbours in line on this axis, their gaps, and the moved box's gap to its nearest one
      const row = others.filter(cross).sort((p, q) => lo(ax, p) - lo(ax, q)), mid = (o: TikzBox, q: TikzBox) => Math.max(hi(x, o), hi(x, q)) + 4;
      const pairs = row.slice(1).map((q, k) => [row[k], q]).filter(([p, q]) => lo(ax, q) - hi(ax, p) > 0);
      const before = row.filter((o) => hi(ax, o) <= lo(ax, m) + tol).at(-1), after = row.find((o) => lo(ax, o) >= hi(ax, m) - tol);
      for (const [p, q] of pairs) {
        const g = lo(ax, q) - hi(ax, p), seg: [number, number, number] = [hi(ax, p), lo(ax, q), mid(p, q)];
        if (before) take(hi(ax, before) + g - lo(ax, m), [seg, [hi(ax, before), hi(ax, before) + g, mid(before, m)]]);
        if (after) take(lo(ax, after) - g - hi(ax, m), [seg, [lo(ax, after) - g, lo(ax, after), mid(after, m)]]);
      }
      if (!best) continue;
      out[ax] += (best as { diff: number }).diff;
      const gap = (best as { gap?: [number, number, number][] }).gap;
      if (gap) { gaps.push({ ax, segs: gap }); continue; }
      const now = moved();
      for (const w of at(ax, now)) {
        const with_ = others.filter((o) => at(ax, o).some((v) => Math.abs(v - w) < 0.01));
        if (with_.length) guides.push({ ax, at: w, from: Math.min(lo(x, now), ...with_.map((o) => lo(x, o))), to: Math.max(hi(x, now), ...with_.map((o) => hi(x, o))) });
      }
    }
    return { d: out, guides, gaps };
  }
  /** The paths of the text's pictures (`\draw`, `\path` …): their options and each segment from point to point, with where
   * a label (`node[…] {…}`) goes in it, right after its `--` or `to[…]`. */
  function edges(text: string): TikzEdge[] {
    const out: TikzEdge[] = [];
    for (const { s, at, line, pic } of stmts(text)) {
      const e = edgeOf(s);
      if (e?.segs.length) out.push({ from: at, to: at + s.length, line: line(at), opts: e.o ? keys(s.slice(e.o[0] + 1, e.o[1])) : [], segs: e.segs, pic });
    }
    return out;
  }
  /** A path statement's options and segments. */
  function edgeOf(s: string) {
    const head = /^\s*\\(draw|path|fill|filldraw)\b/.exec(s);
    if (!head) return null;
    let k = head[0].length, o: [number, number] | null = null, last = '', op: number | null = null, opAt: [number, number] = [0, 0], kind = '', tkeys: string[] = [];
    const oAt = k, segs: TikzSeg[] = [], ws = () => { while (/\s/.test(s[k] ?? '')) k++; };
    ws();
    if (s[k] === '[') { const e = close(s, k); if (e < 0) return null; o = [k, e]; k = e + 1; }
    while (k > 0 && k < s.length) {
      ws();
      const two = s.slice(k, k + 2);
      if (two === '--' || two === '|-' || two === '-|') { opAt = [k, k + 2]; kind = two; tkeys = []; k += 2; op = k; continue; }
      if (s.startsWith('to', k) && !/[a-z]/i.test(s[k + 2] ?? '')) {
        const a = k; k += 2; ws(); tkeys = [];
        if (s[k] === '[') { const e = close(s, k); if (e < 0) return null; tkeys = keys(s.slice(k + 1, e)); k = e + 1; }
        opAt = [a, k]; kind = 'to'; op = k; continue;
      }
      if (s.startsWith('node', k) && !/[a-z]/i.test(s[k + 4] ?? '')) {
        // past its options, name, `at` and text; a label on the segment goes after it
        k += 4;
        for (;;) { ws(); if (s.startsWith('at', k) && /\s|\(/.test(s[k + 2] ?? '')) { k += 2; ws(); } if (!'[({'.includes(s[k] ?? 'x')) break; const c = s[k]; k = close(s, k) + 1; if (k <= 0) return null; if (c === '{') break; }
        if (op != null) op = k;
        continue;
      }
      if (s[k] === '(') { const e = close(s, k), pt = s.slice(k + 1, e).trim(); if (e < 0) return null; if (op != null && last) segs.push({ a: last, b: pt, at: op, op: opAt, kind, keys: tkeys }); last = pt; op = null; k = e + 1; continue; }
      if (s[k] === ';') break;
      // `+(…)`, `|-`, `circle` …: no point known after it
      last = ''; op = null; k++;
      while (k < s.length && !/[\s(;]/.test(s[k])) k++;
    }
    return { o, oAt, segs };
  }
  /** `stmt` with option list `o` for the part whose options stand at `range`, else go at `oAt`; an empty list goes. */
  const withOpts = (stmt: string, range: [number, number] | null, oAt: number, o: string[]) => {
    const t = o.join(', ');
    return range ? stmt.slice(0, range[0]) + (t ? `[${t}]` : '') + stmt.slice(range[1] + 1) : t ? stmt.slice(0, oAt) + `[${t}]` + stmt.slice(oAt) : stmt;
  };
  /** The shapes the page menu offers, each with its label; `rounded` is `rounded corners` on a rectangle. */
  const SHAPE_LIST: [TikzShape, string][] = [['rectangle', 'Rectangle'], ['rounded', 'Rounded rectangle'], ['circle', 'Circle'], ['ellipse', 'Ellipse'], ['diamond', 'Diamond'],
    ['trapezium', 'Trapezium'], ['semicircle', 'Semicircle'], ['regular polygon', 'Regular polygon'], ['star', 'Star'], ['cylinder', 'Cylinder'], ['rectangle split', 'Rectangle split'],
    ['cloud', 'Cloud'], ['signal', 'Signal'], ['single arrow', 'Single arrow'], ['double arrow', 'Double arrow']];
  /** The TikZ library that declares a shape, when the run has not said (its `load` wins): pgf's manual, Part V. */
  const SHAPE_LIB: Record<string, string> = { ellipse: 'shapes.geometric', diamond: 'shapes.geometric', trapezium: 'shapes.geometric', semicircle: 'shapes.geometric',
    'regular polygon': 'shapes.geometric', star: 'shapes.geometric', cylinder: 'shapes.geometric', 'rectangle split': 'shapes.multipart', cloud: 'shapes.symbols',
    signal: 'shapes.symbols', 'single arrow': 'shapes.arrows', 'double arrow': 'shapes.arrows' };
  const library = (shape: TikzShape) => SHAPE_LIB[shape] ?? null;
  const NAMED = SHAPE_LIST.map(([n]) => n).filter((n) => n !== 'rounded').sort((x, y) => y.length - x.length).join('|');
  const SHAPES = new RegExp(`^(${NAMED}|shape\\s*=.*|rounded corners(\\s*=.*)?|sharp corners)$`);
  /** The keys one shape reads (`regular polygon sides`): they go with it when the shape changes. */
  const SHAPE_KEYS = /^(regular polygon sides|star points|star point (ratio|height)|rectangle split (parts|horizontal)|trapezium (left|right)? ?angle|trapezium stretches( body)?|cylinder uses custom fill|aspect|cloud puffs|cloud puff arc|signal (to|from|pointer angle)|single arrow (head extend|tip angle|head indent)|double arrow (head extend|tip angle|head indent)|shape border rotate)\s*(=|$)/;
  /** A node's shape as its keys and its picture's `every node` say: the last such key wins. */
  function shapeOf(n: TikzNode): TikzShape {
    const k = [...(n.pic.node ?? []), ...n.opts].filter((x) => SHAPES.test(x)).at(-1) ?? '', name = k.replace(/^shape\s*=\s*/, '');
    return name !== 'rectangle' && SHAPE_LIST.some(([s]) => s === name) ? name : /^rounded/.test(k) ? 'rounded' : 'rectangle';
  }
  /** Statement `stmt` with node `i` in shape `to` (`extra`: the keys it reads, `regular polygon sides=5`), and the library it needs, else null. */
  function shape(stmt: string, i: number, pic: TikzPic, to: TikzShape, extra: string[] = []) {
    const p = parts(stmt)[i];
    if (!p) return null;
    const own = (p.o ? keys(stmt.slice(p.o[0] + 1, p.o[1])) : []).filter((k) => !SHAPES.test(k) && !SHAPE_KEYS.test(k)), round = (pic.node ?? []).some((k) => /^rounded corners/.test(k));
    const add = to === 'rectangle' ? (round ? ['sharp corners'] : []) : to === 'rounded' ? (round ? [] : ['rounded corners']) : [to, ...extra];
    // a shape key goes first, as people write it
    return { text: withOpts(stmt, p.o, p.oAt, to === 'rectangle' || to === 'rounded' ? [...own, ...add] : [...add, ...own]), library: library(to) };
  }
  /** Whether a key list draws (`draw`) or fills (`fill`): the last say wins, `none` says no. */
  const says = (ks: string[], key: string) => ks.filter((k) => k.split('=')[0].trim() === key).map((k) => !/=\s*none\s*$/.test(k)).at(-1);
  const looks = (n: TikzNode) => ({ draw: says(n.opts, 'draw') ?? says(n.pic.node ?? [], 'draw') ?? false, fill: says(n.opts, 'fill') ?? says(n.pic.node ?? [], 'fill') ?? false });
  /** Statement `stmt` with node `i`'s frame (`draw`) or fill switched: its own key goes, `key=none` where the picture's style still gives it. */
  function flip(stmt: string, i: number, pic: TikzPic, key: 'draw' | 'fill') {
    const p = parts(stmt)[i];
    if (!p) return null;
    const ks = p.o ? keys(stmt.slice(p.o[0] + 1, p.o[1])) : [], style = says(pic.node ?? [], key) ?? false, on = says(ks, key) ?? style;
    const rest = ks.filter((k) => k.split('=')[0].trim() !== key), want = !on;
    return withOpts(stmt, p.o, p.oAt, want === style ? rest : [...rest, want ? (key === 'fill' ? 'fill=black!10' : 'draw') : `${key}=none`]);
  }
  /** Statement `stmt` with node `i`'s text `text`. */
  function retext(stmt: string, i: number, text: string) {
    const p = parts(stmt)[i];
    return p ? stmt.slice(0, p.t[0] + 1) + text + stmt.slice(p.t[1]) : null;
  }
  /** An arrow tip key: `->`, `<->`, `-Stealth`, `{Latex}-{Latex}`, never a key with a value. */
  const TIP = /^[^=\s]*-[^=\s]*$/;
  const tipOf = (e: TikzEdge) => e.opts.filter((k) => TIP.test(k)).at(-1) ?? '';
  /** Path statement `stmt` with arrow tip `tip` (`''`: none, written `-` where a style may give one). */
  function tip(stmt: string, tip: string) {
    const e = edgeOf(stmt);
    if (!e) return null;
    const ks = e.o ? keys(stmt.slice(e.o[0] + 1, e.o[1])) : [], rest = ks.filter((k) => !TIP.test(k));
    const styled = rest.some((k) => !/^(thick|thin|very thick|ultra thick|semithick|dashed|dotted|densely dashed|loosely dashed|draw|rounded corners)$/.test(k) && !k.includes('='));
    return withOpts(stmt, e.o, e.oAt, tip ? [tip, ...rest] : styled ? ['-', ...rest] : rest);
  }
  /** Path statement `stmt` with a label `text` on segment `seg` (`node[midway, above] {…}`): the text and the offset of the label's text in it. */
  function label(stmt: string, seg: number, text: string, opts = ['midway', 'above']) {
    const g = edgeOf(stmt)?.segs[seg];
    if (!g) return null;
    const put = ` node${opts.length ? `[${opts.join(', ')}]` : ''} {${text}}`;
    return { text: stmt.slice(0, g.at) + put + stmt.slice(g.at), at: g.at + put.length - 1 - text.length };
  }
  // ---- the page actions' rewrites (actions.ts): one option group of a node, a path or a segment at a time
  /** The keys each option group owns: setting a group drops all of them first. `names`: bare color names (`red`), a color group's too. */
  const GROUP: Record<string, RegExp> = {
    draw: /^(draw|color)\s*=/, fill: /^fill\s*=/, text: /^text\s*=/, font: /^font\s*=/, rotate: /^rotate\s*=/, opacity: /^opacity\s*=/,
    width: /^(ultra thin|very thin|thin|semithick|thick|very thick|ultra thick|line width\s*=.*)$/,
    corners: /^(rounded corners(\s*=.*)?|sharp corners)$/,
    dash: /^(solid|dashed|dotted|densely dashed|loosely dashed|densely dotted|loosely dotted|dash dot|dash dot dot|dash pattern\s*=.*)$/,
    'inner sep': /^inner sep\s*=/, 'minimum width': /^minimum width\s*=/, 'minimum height': /^minimum height\s*=/,
    pos: /^(midway|near start|near end|very near start|very near end|at start|at end|pos\s*=.*)$/,
    // a side, never a placement (`right=of a`)
    side: /^(above|below|left|right)( (left|right))?(\s*=(?!.*\bof\b).*)?$/, sloped: /^sloped$/, auto: /^(auto(\s*=.*)?|swap|')$/,
  };
  /** Groups that exclude each other: a side and `auto` both place a label. */
  const EXCLUDES: Record<string, string[]> = { side: ['auto'], auto: ['side'] };
  /** The option list of part `i` of `stmt` (a node or a label; -1: the path's own) as a range and where a new one goes. */
  function optsOf(stmt: string, i: number): { o: [number, number] | null; oAt: number } | null {
    if (i < 0) { const e = edgeOf(stmt); return e && { o: e.o, oAt: e.oAt }; }
    const p = parts(stmt)[i];
    return p ? { o: p.o, oAt: p.oAt } : null;
  }
  const keysAt = (stmt: string, i: number) => { const r = optsOf(stmt, i); return r ? (r.o ? keys(stmt.slice(r.o[0] + 1, r.o[1])) : []) : null; };
  const inGroup = (k: string, group: string, names: string[] = []) => GROUP[group]?.test(k) || (group === 'draw' && names.includes(k));
  /** The keys of part `i` that group `group` owns, as written. */
  const current = (stmt: string, i: number, group: string, names: string[] = []) => (keysAt(stmt, i) ?? []).filter((k) => inGroup(k, group, names));
  /** `stmt` with part `i`'s keys of `group` (and those it excludes) replaced by `add` (none: they go), else null. */
  function set(stmt: string, i: number, group: string, add: string[], names: string[] = []) {
    const r = optsOf(stmt, i), ks = keysAt(stmt, i);
    if (!r || !ks) return null;
    const drop = [group, ...(EXCLUDES[group] ?? [])], rest = ks.filter((k) => !drop.some((g) => inGroup(k, g, names)));
    return withOpts(stmt, r.o, r.oAt, [...rest, ...add]);
  }
  /** Segment `seg` of path statement `stmt`. */
  const segOf = (stmt: string, seg: number) => edgeOf(stmt)?.segs[seg] ?? null;
  /** `stmt` with segment `seg`'s operation written `op`. */
  const opSet = (stmt: string, g: TikzSeg, op: string) => stmt.slice(0, g.op[0]) + op + stmt.slice(g.op[1]);
  /** `stmt` with segment `seg` as a straight line (`--`) or a right-angle one (`|-` up or down first, `-|` across first). */
  function route(stmt: string, seg: number, kind: string) {
    const g = segOf(stmt, seg);
    return g && (g.kind === kind && !g.keys.length ? stmt : opSet(stmt, g, kind));
  }
  const CURVE = /^(bend (left|right)(\s*=.*)?|in\s*=.*|out\s*=.*|relative(\s*=.*)?)$/;
  /** `stmt` with segment `seg`'s `to[…]` keys passed through `f`: none left is a straight `--`. */
  function toKeys(stmt: string, seg: number, f: (ks: string[]) => string[]) {
    const g = segOf(stmt, seg);
    if (!g) return null;
    const ks = f(g.kind === 'to' ? g.keys : []);
    return opSet(stmt, g, ks.length ? `to[${ks.join(', ')}]` : g.kind === 'to' && g.keys.length ? '--' : g.kind);
  }
  /** `stmt` with segment `seg` bent to `side` (`left`, `right`; null: straight) by `angle` degrees (30, TikZ's default, unwritten). */
  const bend = (stmt: string, seg: number, side: string | null, angle = 30) =>
    toKeys(stmt, seg, (ks) => [...ks.filter((k) => !CURVE.test(k) && !(side == null && /^looseness\s*=/.test(k))), ...(side ? [angle === 30 ? `bend ${side}` : `bend ${side}=${angle}`] : [])]);
  /** `stmt` with segment `seg` leaving at `out` and entering at `into` degrees. */
  const inout = (stmt: string, seg: number, out: number, into: number) => toKeys(stmt, seg, (ks) => [...ks.filter((k) => !CURVE.test(k)), `out=${out}`, `in=${into}`]);
  /** `stmt` with segment `seg`'s looseness `v` (1, the default, unwritten). */
  const looseness = (stmt: string, seg: number, v: number) => toKeys(stmt, seg, (ks) => [...ks.filter((k) => !/^looseness\s*=/.test(k)), ...(v === 1 ? [] : [`looseness=${v}`])]);
  /** The points segment `g` draws from `a` to `b` (page bp, y down), for a hit test: a corner for `|-`/`-|`, a curve sampled for a bend or angles. */
  function segPath(a: [number, number], b: [number, number], g: TikzSeg): [number, number][] {
    if (g.kind === '-|') return [a, [b[0], a[1]], b];
    if (g.kind === '|-') return [a, [a[0], b[1]], b];
    const kv = (n: string) => g.keys.map((k) => k.split('=').map((x) => x.trim())).find(([k]) => k === n), bent = g.keys.map((k) => /^bend (left|right)(?:\s*=\s*(.*))?$/.exec(k)).find(Boolean);
    const out = kv('out'), into = kv('in');
    if (!bent && !(out && into)) return [a, b];
    // in TikZ's frame, y up: `to` leaves at `out`, enters at `in`, its controls 0.3915 × the distance × looseness away
    const d = [b[0] - a[0], a[1] - b[1]], dir = Math.atan2(d[1], d[0]) * 180 / Math.PI, th = bent ? +(bent[2] ?? 30) : 0;
    const o = bent ? dir + (bent[1] === 'left' ? th : -th) : +out![1], i = bent ? dir + 180 - (bent[1] === 'left' ? th : -th) : +into![1];
    const L = 0.3915 * Math.hypot(d[0], d[1]) * (+(kv('looseness')?.[1] ?? 1) || 1), r = (deg: number) => deg * Math.PI / 180;
    const c1 = [a[0] + L * Math.cos(r(o)), a[1] - L * Math.sin(r(o))], c2 = [b[0] + L * Math.cos(r(i)), b[1] - L * Math.sin(r(i))];
    return Array.from({ length: 17 }, (_, k) => { const t = k / 16, u = 1 - t; return [0, 1].map((j) => u * u * u * a[j] + 3 * u * u * t * c1[j] + 3 * u * t * t * c2[j] + t * t * t * b[j]) as [number, number]; });
  }
  /** Where `\draw[opts] (a) -- (b);` goes in the picture at `picAt`: its own line before `\end{tikzpicture}`, indented as the statement before. */
  function addEdge(text: string, picAt: number, a: string, b: string, opts = '->') {
    const end = text.indexOf('\\end{tikzpicture}', picAt);
    if (end < 0) return null;
    const stmt = `\\draw${opts ? `[${opts}]` : ''} (${a}) -- (${b});`, ls = text.lastIndexOf('\n', end - 1) + 1;
    if (text.slice(ls, end).trim()) return { at: end, text: `${stmt} ` };
    const prev = text.slice(text.lastIndexOf('\n', ls - 2) + 1, ls - 1), lead = /^\s*/.exec(prev)![0];
    return { at: ls, text: `${/\\begin\{tikzpicture\}/.test(prev) ? `${lead}  ` : lead}${stmt}\n` };
  }
  /** `stmt` with node `i` named `name` (a path end needs one), as it was when it has a name. */
  function named(stmt: string, i: number, name: string) {
    const p = parts(stmt)[i];
    if (!p) return null;
    if (p.name) return stmt;
    const at = p.o ? p.o[1] + 1 : p.oAt;
    return `${stmt.slice(0, at)} (${name})${stmt.slice(at)}`;
  }
  /** A node name from `base` (its text) not yet used in the picture at `picAt`. */
  function fresh(text: string, picAt: number, base: string) {
    const used = new Set(nodes(text).filter((n) => n.pic.at === picAt && n.name).map((n) => n.name)), stem = base.replace(/\\[A-Za-z@]+|[^A-Za-z]/g, '').toLowerCase().slice(0, 8) || 'n';
    for (let k = 1; ; k++) { const n = k === 1 ? stem : `${stem}${k}`; if (!used.has(n)) return n; }
  }
  /** The options of the picture at `picAt`: their range (`[`..`]`, or the empty place after `\begin{tikzpicture}`) and keys. */
  function picOpts(text: string, picAt: number) {
    const m = /^\\begin\{tikzpicture\}\s*/.exec(text.slice(picAt));
    if (!m) return null;
    const k = picAt + m[0].length, at = picAt + '\\begin{tikzpicture}'.length;
    if (text[k] !== '[') return { from: at, to: at, keys: [] };
    const e = close(text, k);
    return e < 0 ? null : { from: k, to: e + 1, keys: keys(text.slice(k + 1, e)) };
  }
  /** The picture's options with the keys `group` matches replaced by `add`: the edit `from`..`to` of `was` to `now`. */
  function picSet(text: string, picAt: number, group: RegExp, add: string[]) {
    const o = picOpts(text, picAt);
    if (!o) return null;
    const ks = [...o.keys.filter((k) => !group.test(k)), ...add];
    return { from: o.from, to: o.to, was: text.slice(o.from, o.to), now: ks.length ? `[${ks.join(', ')}]` : '' };
  }
  /** `deg` on the nearest multiple of `step`, in -180..180. */
  const snapAngle = (deg: number, step: number) => { const v = Math.round(deg / step) * step, w = ((v % 360) + 540) % 360 - 180; return w === -180 ? 180 : w || 0; };
  /** Node `i`'s own rotation in degrees. */
  const angle = (stmt: string, i: number) => +(current(stmt, i, 'rotate')[0]?.split('=')[1] ?? 0) || 0;
  /** Where frame `b` stands beside frame `ref` (page bp, y down) as positioning writes it: a side when their centres line up
   * within `tol`, else a corner (`above right=⟨vertical⟩ and ⟨horizontal⟩ of a`), each gap between their borders in cm on
   * the 0.5 mm grid; gaps equal to the picture's node distance `dist` (vertical, horizontal) unwritten. Null: the same centre. */
  function relation(b: TikzBox, ref: TikzBox, tol: number, dist: [number, number] = [1, 1], name: string) {
    const bp = 72 / 2.54, mid = (o: TikzBox, ax: number) => (ax ? o.t + o.d : o.l + o.r) / 2;
    const dx = mid(b, 0) - mid(ref, 0), dy = mid(b, 1) - mid(ref, 1);
    const h = Math.abs(dx) <= tol ? '' : dx > 0 ? 'right' : 'left', v = Math.abs(dy) <= tol ? '' : dy > 0 ? 'below' : 'above';
    if (!h && !v) return null;
    const gx = (h === 'right' ? b.l - ref.r : ref.l - b.r) / bp, gy = (v === 'below' ? b.t - ref.d : ref.t - b.d) / bp;
    const fmt = (c: number) => `${+(Math.round(c / GRID) * GRID).toFixed(2)}cm`, same = (g: number, d: number) => Math.abs(g - d) < GRID / 2;
    // frames overlapping across a corner: the side they stand apart on, shifted by the centres' offset along the other axis
    if (h && v && (gx < 0 || gy < 0)) {
      const vert = gy >= gx, d = vert ? dist[0] : dist[1], g = vert ? gy : gx;
      return `${vert ? v : h}=${same(g, d) ? '' : `${fmt(g)} `}of ${name}, ${vert ? `xshift=${fmt(dx / bp)}` : `yshift=${fmt(-dy / bp)}`}`;
    }
    const gaps: [number, number][] = [...(v ? [[gy, dist[0]] as [number, number]] : []), ...(h ? [[gx, dist[1]] as [number, number]] : [])];
    const val = gaps.every(([g, d]) => same(g, d)) ? '' : `${gaps.map(([g]) => fmt(g)).join(' and ')} `;
    return `${[v, h].filter(Boolean).join(' ')}=${val}of ${name}`;
  }
  /** `stmt` with node `i` placed by positioning keys `rel` (`right=of a`, a shift after it): its placement, shifts and `at` before go. */
  function place(stmt: string, i: number, rel: string) {
    const p = parts(stmt)[i];
    if (!p) return null;
    const LEGACY = /^(above|below|left|right)( (left|right))? of\s*=/;
    const ks = (p.o ? keys(stmt.slice(p.o[0] + 1, p.o[1])) : []).filter((k) => !PLACED.test(k) && !SHIFTS.test(k) && !LEGACY.test(k));
    let s = withOpts(stmt, p.o, p.oAt, [...ks, ...keys(rel)]);
    const q = parts(s)[i]!;
    if (q.at) s = s.slice(0, q.at[0]).replace(/\s*\bat\s*$/, '') + s.slice(q.at[1] + 1);
    return s;
  }
  /** The nodes placed from node `n` (`right=of n`, `at (n.east)`), in its picture: `direct` those naming it, `all` with the
   * chains placed from them. A node of a path statement counts by its own options only. */
  function dependents(text: string, n: TikzNode) {
    const ns = nodes(text).filter((m) => m.kind === 'node' && m.pic.at === n.pic.at && m.from > n.from);
    const uses = (m: TikzNode, name: string) => {
      const q = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), re = new RegExp(`\\(\\s*${q}\\s*(\\.[^)]*)?\\)|\\bof\\s*=?\\s*${q}(?![\\w])`);
      const own = /^\s*\\node\b/.test(text.slice(m.from, m.to)) ? text.slice(m.from, m.to).replace(/\{[^{}]*\}\s*;?\s*$/, '') : m.opts.join(', ');
      return re.test(m.name ? own.replace(new RegExp(`\\(\\s*${m.name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\)`), '') : own);
    };
    const direct = n.name ? ns.filter((m) => uses(m, n.name!)) : [], all = [...direct];
    for (let k = 0; k < all.length; k++) for (const m of ns) if (all[k]!.name && !all.includes(m) && uses(m, all[k]!.name!)) all.push(m);
    return { direct, all };
  }
  /** `stmt` without its label `i` (`node[…] {…}` on a path). */
  function unlabel(stmt: string, i: number) {
    const p = parts(stmt)[i];
    if (!p || p.kind !== 'label') return null;
    let a = p.oAt - 4;
    while (a > 0 && /\s/.test(stmt[a - 1]!)) a--;
    return stmt.slice(0, a) + stmt.slice(p.t[1] + 1);
  }
  self.satexTikzMove = { nodes, move, len, cm, posOn, clipped, snap, edges, shapeOf, shape, looks, flip, retext, tipOf, tip, label,
    SHAPES: SHAPE_LIST, library, keysAt, current, set, segOf, route, bend, inout, looseness, segPath, addEdge, named, fresh, picOpts, picSet, snapAngle, angle, unlabel, relation, place, dependents };
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
