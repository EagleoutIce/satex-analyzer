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
      typed(text: string, at: number, c: string, inline?: string, smart?: boolean): [number, number, string, number];
      erased(text: string, at: number, back: boolean): [number, number];
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
  /** Why `text`'s characters `from..to` may not be edited as plain text, null when they may. The file is read up to
   * them: math (`$`, `$$`, `\(`, `\[`, a math environment), an environment's name, a key (`keys`: the run's label,
   * reference and citation spans) or markup in them. Comments are skipped. */
  function editable(text: string, from: number, to: number, keys: [number, number][]): string | null {
    if (from < 0 || to > text.length || from > to) return 'it is outside the text';
    // verbatim: every character is text; an inline body holds one line and not its closer
    const v = verbatimAt(text, from);
    if (v && to <= v.to) return v.block || !/[\n]/.test(text.slice(from, to)) && !text.slice(from, to).includes(v.close) ? null : 'it ends the verbatim text';
    if (!plain(text.slice(from, to))) return 'the text there holds markup';
    if (keys.some(([a, b]) => a < to && from < b || a === from && b === to)) return 'it is a key (a label, a reference, a citation)';
    let math = 0, inline = false, display = false, cmd = '';
    const groups: string[] = [], skip = verbatim(text);
    for (let i = 0, k = 0; i < from; i++) {
      // a verbatim body opens no math and no group
      while (skip[k] && skip[k]!.to < i) k++;
      if (skip[k] && skip[k]!.from <= i) { i = skip[k]!.to - 1; k++; cmd = ''; continue; }
      const c = text[i];
      if (c === '%') { while (i < from && text[i] !== '\n') i++; cmd = ''; continue; }
      if (c === '\\') {
        const m = /^\\([A-Za-z@]+|.)/s.exec(text.slice(i, i + 64));
        const name = m?.[1] ?? '';
        if (name === '(' || name === '[') math++; else if (name === ')' || name === ']') math = Math.max(0, math - 1);
        if (name === 'begin' || name === 'end') {
          const env = /^\s*\{([^}]*)\}/.exec(text.slice(i + 1 + name.length, i + 80))?.[1];
          if (env != null && MATH.test(env)) math = Math.max(0, math + (name === 'begin' ? 1 : -1));
        }
        cmd = /^[A-Za-z@]/.test(name) ? name : '';
        i += name.length;
        continue;
      }
      if (c === '$') { if (text[i + 1] === '$') { display = !display; i++; } else inline = !inline; cmd = ''; continue; }
      if (c === '{') groups.push(cmd); else if (c === '}') groups.pop();
      if (!/\s/.test(c)) cmd = '';
    }
    if (math || inline || display) return 'it is math';
    if (/^(begin|end)$/.test(groups.at(-1) ?? '')) return "it is an environment's name";
    return null;
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
  /** What typing `c` at `at` of `text` writes so the source stays compilable: `[from, to, body, caret]`, the text
   * `from..to` replaced by `body` and the caret `caret` past `from`. In text a special character is escaped (a brace
   * after a command's name opens its argument, one before a `}` steps over it) and `$`
   * opens math with its closer (`inline`: `\(`, `$` or `auto`, the document's most used). In math a character goes
   * in as typed, `{` with its `}`, and a closer typed where it stands steps over it. `smart`: `x_42` writes `x_{42}`. */
  function typed(text: string, at: number, c: string, inline = '\\(', smart = false): [number, number, string, number] {
    // verbatim takes the character as it is; an inline body never its closer or a line break
    const v = verbatimAt(text, at);
    if (v) return !v.block && (c === v.close || c === '\n') ? [at, at, '', 0] : [at, at, c, c.length];
    const math = /math/.test(editable(text, at, at, []) ?? '');
    if (!math) {
      // after a command's name `{` opens its argument, and `}` where one stands steps over it
      if (c === '{' && /\\[A-Za-z@]+$/.test(text.slice(Math.max(0, at - 64), at))) return [at, at, '{}', 1];
      if (c === '}' && text[at] === '}') return [at, at, '', 1];
      if (c !== '$') { const body = escaped(c); return [at, at, body, body.length]; }
      const open = inline === '$' || inline === '\\(' ? inline : inlineOf(text);
      return open === '$' ? [at, at, '$$', 1] : [at, at, '\\(\\)', 2];
    }
    // smart scripts: `_`/`^` open a group, a space, an operator or the formula's end typed at its end leave it
    if (smart && (c === '_' || c === '^')) return [at, at, `${c}{}`, 2];
    if (smart && text[at] === '}' && /[_^]\{[^{}]*$/.test(text.slice(Math.max(0, at - 64), at)) && /[\s$+\-=<>,;:|/*!?()[\]]/.test(c)) {
      // past the `}` first, then the character as typed there (a space only leaves)
      return /\s/.test(c) ? [at + 1, at + 1, '', 0] : typed(text, at + 1, c, inline, smart);
    }
    for (const close of c === '$' ? ['$', '\\)', '\\]'] : c === '}' ? ['}'] : []) if (text.startsWith(close, at)) return [at, at, '', close.length];
    return c === '{' ? [at, at, '{}', 1] : [at, at, c, c.length];
  }
  /** The math control at `at`: `[from, to, body, caret]` like typed's. Inline math in the `inline` convention's
   * delimiters, display `\[…\]` or an `align*`, the caret inside; in math already nothing. */
  function mathAt(text: string, at: number, kind: string, inline = '\\('): [number, number, string, number] {
    if (/math/.test(editable(text, at, at, []) ?? '')) return [at, at, '', 0];
    if (kind === 'inline') return typed(text, at, '$', inline);
    const [o, c] = kind === 'align' ? ['\\begin{align*}\n', '\n\\end{align*}'] : ['\\[', '\\]'];
    return [at, at, o + c, o.length];
  }
  /** What Backspace (`back`) or Delete takes at `at`: `[from, to]`, an escape or an empty pair (`$$`, `{}`, `\(\)`) whole. */
  function erased(text: string, at: number, back: boolean): [number, number] {
    const v = verbatimAt(text, at);
    if (v) return back ? (at > v.from ? [at - 1, at] : [at, at]) : (at < v.to ? [at, at + 1] : [at, at]);
    for (const pair of ['$$', '{}', '\\(\\)']) {
      const h = pair.length / 2;
      if (text.slice(at - h, at + h) === pair) return [at - h, at + h];
    }
    if (back) {
      const m = /\\(?:textbackslash|textasciitilde|textasciicircum)\{\}$|\\[&%#_{}$]$/.exec(text.slice(Math.max(0, at - 20), at));
      return m ? [at - m[0].length, at] : [at - 1, at];
    }
    const m = /^\\(?:textbackslash|textasciitilde|textasciicircum)\{\}|^\\[&%#_{}$]/.exec(text.slice(at, at + 20));
    return [at, at + (m ? m[0].length : 1)];
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
  self.satexPdfEdit = { cells, cellStep, rowAfter, verbatim, verbatimAt, nearest, bands, joined, place, extent, room, form, measure, respec, widthBp, snapWidth, resized, stepped, plain, editable, spans, sender, typed, erased, mathAt, boundaries, boundaryAt, inkSnap, locate, columns, wordAt, argAt, relaid, apart };
})();
