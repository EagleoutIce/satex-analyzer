// Editing in the PDF: what a column-width handle and a caret on the pages map to in the source.
// Pure: SyncTeX records and the server's tables in, places and new values out (playground.js draws and writes).
export {};
/** A SyncTeX record as playground.js parses it, in bp from the page's top left. */
type Box = { page: number; file: string; line: number; kind: string; x: number; y: number; w: number; h: number; d: number };
/** A table of `satex/dialogApply` kind `columns` (src/lsp/dialogs.rs). */
type Table = { env: string; lines: [number, number]; width: unknown; cols: { align: string; width: string; node: SnapNode | null }[] };
/** A column found on a page: its index, where its cells' boxes stand, how wide they are, in bp. */
type Placed = { k: number; page: number; x: number; w: number; t: number; d: number; n: number };
/** A width as written that a drag may rewrite: a length (`3cm`) or a share of a width (`.3\linewidth`). */
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
  /** Why `text`'s characters `from..to` may not be edited as plain text, null when they may. The file is read up to
   * them: math (`$`, `$$`, `\(`, `\[`, a math environment), an environment's name, a key (`keys`: the run's label,
   * reference and citation spans) or markup in them. Comments are skipped. */
  function editable(text: string, from: number, to: number, keys: [number, number][]): string | null {
    if (from < 0 || to > text.length || from > to) return 'it is outside the text';
    if (!plain(text.slice(from, to))) return 'the text there holds markup';
    if (keys.some(([a, b]) => a < to && from < b || a === from && b === to)) return 'it is a key (a label, a reference, a citation)';
    let math = 0, inline = false, display = false, cmd = '';
    const groups: string[] = [];
    for (let i = 0; i < from; i++) {
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
  self.satexPdfEdit = { place, extent, room, form, resized, stepped, plain, editable, spans, sender };
})();
