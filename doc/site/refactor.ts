// Refactor plugins (actions.ts registry, category `refactor`): rewrites of the source a right click offers in the
// editor, the PDF and the Typeset view. A float around a picture, a table or an image (and back), math from one
// display to another, a selection to a `\newcommand`. Insert (plugin `insert`): new content at the place clicked or at the caret. Each rewrite is a pure function of the text here (unit-tested in
// tests/refactor.test.mjs); a menu row writes its edits through `ctx.write`: one undo step through the snapshot printer.
export {};
declare global {
  /** A span of the source: `[from, to)` offsets; `env` an environment's name, `head` the end of its `\begin{..}[..]`. */
  type RfSpan = { kind: string; from: number; to: number; env?: string; head?: number; foot?: number };
  type RfMath = RfSpan & { open: string; close: string; body: string };
  /** A thing Insert writes: its text (`\x01` the caret), the float it goes into, the widget opened on it; `flow` a mark in the running text, `tight` a display that may stand inside a paragraph, `before` a mark
   * set before the sentence's end. */
  type InsItem = { kind: string; label: string; group: string; hint?: string; body: string | ((text: string, at: number) => string); wrap?: boolean; widget?: string; flow?: boolean; tight?: boolean; before?: boolean };
  /** Where Insert goes: the caret (`caret`), a blank place between paragraphs (`place`, PE.blankPlace), the block clicked (its last line `to`). */
  type InsTarget = { caret?: number | null; place?: number | null; block?: { to: number; what: string } | null };
  /** `caret` at the caret; `after` a block of its own after the line ending at `at`; `end` a mark at a paragraph's end. */
  type InsMode = 'caret' | 'after' | 'end';
  /** One undo step's edit; `caret` and `open` (the widget's place) are offsets in the text after it. */
  type InsPlan = { edits: PaEdit[]; caret: number; open: number; widget: string | null };
  interface Window {
    satexRefactor: {
      blank(text: string): string; envs(text: string): RfSpan[]; group(text: string, at: number): number;
      wrapTarget(text: string, at: number, end?: number): (RfSpan & { float: RfSpan | null }) | null;
      wrap(text: string, t: RfSpan, labels: string, alt?: string | null): { edit: PaEdit; caret: number };
      unwrap(text: string, f: RfSpan): { edit: PaEdit; dropped: string[] };
      slug(s: string): string; firstWords(text: string, t: RfSpan): string;
      mathAt(text: string, at: number, end?: number): RfMath | null; rows(body: string): string[]; targets(m: RfMath): string[];
      mathTo(text: string, m: RfMath, to: string): string; mathEdit(text: string, m: RfMath, to: string): PaEdit; needsAms(to: string): boolean;
      pkgEdit(text: string, all: string, pkg: string): { have: true } | { edit: PaEdit } | { line: string };
      matrixAt(text: string, at: number, end?: number): (RfSpan & { grid: boolean; body: string }) | null; matrixTo(text: string, m: RfSpan & { grid: boolean; body: string }, to: string): string;
      extract(text: string, all: string, a: number, b: number, name: string, every: boolean): { edits: PaEdit[] } | { error: string };
      occurrences(text: string, body: string): number[];
      inserts: InsItem[];
      insertSpot(text: string, t: InsTarget, flow: boolean, float?: boolean): { at: number; mode: InsMode } | null;
      insertPlan(text: string, all: string, kind: string, at: number, mode: InsMode): InsPlan | null;
      lineHabit(text: string): number; wrapLine(text: string, at: number, max: number, indent?: boolean): PaEdit | null;
      formatToggle(text: string, a: number, b: number, cmd: string): { edit: PaEdit; caret: number };
    };
  }
}
(() => {
  // ---------------------------------------------------------------- reading the source
  /** `text` with its comments blanked (same length): a `%` not escaped to the line's end. */
  const blank = (text: string) => text.replace(/(?<!\\)((?:\\\\)*)%[^\n]*/g, (m, bs: string) => bs + ' '.repeat(m.length - bs.length));
  /** The offset after the group (`{…}` or `[…]`) that opens at `at`, `at` when none opens there. */
  function group(text: string, at: number) {
    const o = text[at], c = o === '{' ? '}' : o === '[' ? ']' : null;
    if (!c) return at;
    let d = 0;
    for (let i = at; i < text.length; i++) {
      if (text[i] === '\\') { i++; continue; }
      if (text[i] === o) d++;
      else if (text[i] === c && --d === 0) return i + 1;
    }
    return at;
  }
  /** Where the braces open at `at` (counted from its paragraph's start) close again: `at` when none is open, null
   * when one never closes. */
  function groupEnd(text: string, at: number): number | null {
    const t = blank(text);
    let d = 0;
    for (let i = t.lastIndexOf('\n\n', at) + 1; i < at; i++) { if (t[i] === '\\') i++; else if (t[i] === '{') d++; else if (t[i] === '}') d--; }
    for (let i = at; d > 0 && i < t.length; i++) { if (t[i] === '\\') i++; else if (t[i] === '{') d++; else if (t[i] === '}' && --d === 0) return i + 1; }
    return d > 0 ? null : at;
  }
  /** Every `\begin{e}…\end{e}` pair, inner ones first: `head` after the `\begin{e}` and its options, `foot` at `\end`. */
  function envs(text: string): RfSpan[] {
    const t = blank(text), open: [string, number, number][] = [], out: RfSpan[] = [];
    for (const m of t.matchAll(/\\(begin|end)\s*\{([^}]*)\}/g)) {
      if (m[1] === 'begin') {
        let h = m.index! + m[0].length;
        if (t[h] === '[') h = group(t, h);
        open.push([m[2]!, m.index!, h]);
        continue;
      }
      const k = open.map(([e]) => e).lastIndexOf(m[2]!);
      if (k < 0) continue;
      const [env, from, head] = open.splice(k)[0]!;
      out.push({ kind: 'env', env, from, head, foot: m.index!, to: m.index! + m[0].length });
    }
    return out;
  }
  const lineStart = (text: string, at: number) => text.lastIndexOf('\n', at - 1) + 1;
  const lineEnd = (text: string, at: number) => { const e = text.indexOf('\n', at); return e < 0 ? text.length : e; };
  const indentAt = (text: string, at: number) => /^[ \t]*/.exec(text.slice(lineStart(text, at)))![0];
  /** The innermost of `spans` around `at`, else the first that starts in `[at, end]`. */
  const pick = <T extends RfSpan>(spans: T[], at: number, end: number) =>
    spans.filter((s) => s.from <= at && at < s.to).sort((a, b) => (a.to - a.from) - (b.to - b.from))[0] ?? spans.filter((s) => s.from >= at && s.from <= end).sort((a, b) => a.from - b.from)[0] ?? null;

  // ---------------------------------------------------------------- floats
  const WRAP = /^(?:tikzpicture|tabular\*?|tabularx|tabulary|forest)$/, FLOAT = /^(?:figure|table)\*?$/;
  /** What a float would hold at `at` (or first in `[at, end]`): a picture, a tabular, a forest tree, an
   * `\includegraphics`; `kind` the float it goes into, `float` the one around it already. */
  function wrapTarget(text: string, at: number, end = at) {
    const all = envs(text), t = blank(text);
    const gfx: RfSpan[] = [...t.matchAll(/\\includegraphics\s*(?=[[{])/g)].map((m) => { let e = m.index! + m[0].length; if (t[e] === '[') e = group(t, e); return { kind: 'figure', env: 'includegraphics', from: m.index!, to: group(t, e) }; });
    const own = pick([...all.filter((e) => WRAP.test(e.env!)).map((e) => ({ ...e, kind: /^tab/.test(e.env!) ? 'table' : 'figure' })), ...gfx], at, end);
    // the picture inside a float is the float's own, never a new float's
    const float = (own ? all.filter((f) => FLOAT.test(f.env!) && f.from <= own.from && own.to <= f.to) : all.filter((f) => FLOAT.test(f.env!) && f.from <= at && at < f.to)).sort((a, b) => (a.to - a.from) - (b.to - b.from))[0] ?? null;
    if (!own) return float && { kind: float.env!.replace(/\*$/, ''), from: float.from, to: float.to, float };
    return { ...own, float };
  }
  /** A label's slug: the first three words, lowercase, ASCII letters and digits joined by `-`. */
  const slug = (s: string) => s.replace(/\\[A-Za-z@]+\*?/g, ' ').normalize('NFKD').replace(/[^\w\s-]|_/g, '').toLowerCase().split(/[\s-]+/).filter(Boolean).slice(0, 3).join('-');
  /** The words a float's label and alt text come from: a node's text, a table's first cell, a tree's root, an image's file name. */
  function firstWords(text: string, t: RfSpan) {
    const s = text.slice(t.head ?? t.from, t.foot ?? t.to);
    if (t.env === 'includegraphics') return (/\{([^{}]*)\}\s*$/.exec(s)?.[1] ?? '').split('/').pop()!.replace(/\.\w+$/, '').replace(/[_-]+/g, ' ');
    if (t.env === 'tikzpicture') return /\\node\b[^;]*?\{((?:[^{}]|\{[^{}]*\})*)\}\s*;/.exec(s)?.[1] ?? '';
    if (t.env === 'forest') return /\[\s*([^[\]{,]*)/.exec(s)?.[1] ?? '';
    return s.replace(/^\s*\{[^}]*\}/, '').split(/&|\\\\/)[0] ?? '';
  }
  /** Lines `[a, b)` of the span when nothing else stands on them, else the span alone. */
  const ownLines = (text: string, from: number, to: number) => {
    const a = lineStart(text, from), b = lineEnd(text, to);
    return text.slice(a, from).trim() || text.slice(to, b).trim() ? null : [a, b] as const;
  };
  /** `t` in a float (`figure`, `table` for a tabular): centred, the caption empty with the caret in it, a label from
   * its first words not among `labels` (the document's text). An image gets `alt=` (tagging) when it has none. */
  function wrap(text: string, t: RfSpan, labels: string, alt: string | null = null) {
    const lines = ownLines(text, t.from, t.to), ind = indentAt(text, t.from), env = t.kind === 'table' ? 'table' : 'figure';
    let body = lines ? text.slice(lines[0], lines[1]) : text.slice(t.from, t.to);
    if (t.env === 'includegraphics' && alt != null && !/\balt\s*=/.test(body)) {
      const k = body.indexOf('\\includegraphics') + 16, opt = /^\s*\[/.test(body.slice(k)) ? body.indexOf('[', k) : -1, a = `alt={${alt.replace(/[{}]/g, '')}}`;
      body = opt >= 0 ? `${body.slice(0, group(body, opt) - 1).replace(/\s*$/, '')}, ${a}${body.slice(group(body, opt) - 1)}` : `${body.slice(0, k)}[${a}]${body.slice(k)}`;
    }
    const own = (lines ? body : `${ind}${body}`).split('\n').map((l) => l.trim() ? `  ${l}` : l).join('\n');
    const base = `${env === 'table' ? 'tab' : 'fig'}:${slug(firstWords(text, t)) || (t.env === 'tikzpicture' ? 'picture' : env)}`;
    let label = base;
    for (let k = 2; new RegExp(`\\\\label\\s*\\{${label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\}`).test(labels); k++) label = `${base}-${k}`;
    const cap = `${ind}  \\caption{`, tail = `}\n${ind}  \\label{${label}}`;
    const head = `${ind}\\begin{${env}}[htbp]\n${ind}  \\centering\n`;
    const now = env === 'table' ? `${head}${cap}${tail}\n${own}\n${ind}\\end{${env}}` : `${head}${own}\n${cap}${tail}\n${ind}\\end{${env}}`;
    const from = lines ? lines[0] : t.from, to = lines ? lines[1] : t.to;
    // a float that shares its line with text stands on lines of its own
    const pre = lines || !text.slice(lineStart(text, from), from).trim() ? '' : '\n', post = lines || !text.slice(to, lineEnd(text, to)).trim() ? '' : '\n';
    const out = `${pre}${now}${post}`;
    // the spaces at the line break go with it
    const a = pre ? from - /[ \t]*$/.exec(text.slice(0, from))![0].length : from, b = post ? to + /^[ \t]*/.exec(text.slice(to))![0].length : to;
    return { edit: { from: a, to: b, now: out }, caret: a + out.indexOf(cap) + cap.length };
  }
  /** Float `f` removed, its content kept one level out: its `\centering`, `\caption` and `\label` go (`dropped` names them). */
  function unwrap(text: string, f: RfSpan) {
    const b = blank(text), dropped: string[] = [], cuts: [number, number][] = [];
    // the float's own commands: those not inside an environment of its content
    const inner = envs(text).filter((e) => e.from > f.from && e.to < f.to);
    const top = (i: number) => !inner.some((e) => e.from < i && i < e.to);
    const sp = (e: number) => e + /^[ \t]*/.exec(b.slice(e))![0].length;
    for (const m of b.slice(0, f.foot).matchAll(/\\(centering|caption|label)(?![A-Za-z@])/g)) {
      if (m.index! < f.head! || !top(m.index!)) continue;
      let e = sp(m.index! + m[0].length);
      if (m[1] !== 'centering') { if (b[e] === '[') e = sp(group(b, e)); e = group(b, e); }
      if (m[1] !== 'centering') dropped.push(text.slice(m.index!, e).trim());
      cuts.push([m.index!, e]);
    }
    let body = '', at = f.head!;
    for (const [x, y] of cuts) { body += text.slice(at, x); at = y; }
    body += text.slice(at, f.foot);
    const ind = indentAt(text, f.from), lines = body.split('\n').filter((l, i, all) => l.trim() || (i > 0 && i < all.length - 1 && all[i - 1]!.trim() && all[i + 1]?.trim()));
    const own = lines.map((l) => l.startsWith(`${ind}  `) ? ind + l.slice(ind.length + 2) : l).join('\n');
    const span = ownLines(text, f.from, f.to);
    return { edit: span ? { from: span[0], to: span[1], now: own } : { from: f.from, to: f.to, now: own.trim() }, dropped };
  }

  // ---------------------------------------------------------------- math
  const MATH = /^(?:equation|align|gather|multline|flalign|displaymath)\*?$/, DELIMS: Record<string, string> = { '\\(': '\\)', '\\[': '\\]', $$: '$$', $: '$' };
  /** The math around `at` (or first in `[at, end]`): `\(…\)`, `$…$`, `\[…\]`, `$$…$$` or a display environment. */
  function mathAt(text: string, at: number, end = at): RfMath | null {
    const t = blank(text), spans: RfMath[] = [];
    for (const e of envs(text)) if (MATH.test(e.env!)) spans.push({ ...e, open: text.slice(e.from, e.head), close: text.slice(e.foot, e.to), body: text.slice(e.head, e.foot) });
    let open: [string, number] | null = null;
    for (const m of t.matchAll(/\\begin\s*\{([^}]*)\}|\\end\s*\{([^}]*)\}|\\[[\]()]|\\.|\$\$|\$/g)) {
      const s = m[0];
      if (s.length === 2 && s[0] === '\\' && !'[]()'.includes(s[1]!)) continue;
      if (s[1] === 'b' || s[1] === 'e') continue;
      if (open && DELIMS[open[0]] === s) { spans.push({ kind: open[0] === '\\(' || open[0] === '$' ? 'inline' : 'display', from: open[1], to: m.index! + s.length, open: open[0], close: s, body: text.slice(open[1] + open[0].length, m.index!) }); open = null; }
      else if (!open && s in DELIMS) open = [s, m.index!];
    }
    return pick(spans, at, end);
  }
  /** Where a body splits at the top level (outside groups and environments), by `sep` (`\\` or `&`): the offsets. */
  function tops(body: string, sep: RegExp) {
    const b = blank(body), at: [number, number][] = [];
    let d = 0;
    const whole = new RegExp(`^(?:${sep.source})$`);
    // the separator before `\\.`: a relation `\le` is one, an escaped `\&` none
    for (const m of b.matchAll(new RegExp(`\\\\begin\\s*\\{[^}]*\\}|\\\\end\\s*\\{[^}]*\\}|${sep.source}|\\\\\\\\|\\\\.|[{}]`, 'g'))) {
      const s = m[0];
      if (s === '{' || s.startsWith('\\begin')) d++;
      else if (s === '}' || s.startsWith('\\end')) d--;
      else if (d === 0 && whole.test(s)) at.push([m.index!, m.index! + s.length]);
    }
    return at;
  }
  const ROW = /\\\\(?:\s*\[[^\]]*\])?/;
  /** A body's rows: split at its top-level `\\`. */
  const rows = (body: string) => { const out: string[] = []; let at = 0; for (const [a, b] of tops(body, ROW)) { out.push(body.slice(at, a)); at = b; } out.push(body.slice(at)); return out.filter((r, i, all) => i < all.length - 1 || r.trim()); };
  const REL = /=|<|>|\\(?:le|leq|ge|geq|ne|neq|approx|equiv|sim|simeq|cong|propto|leqslant|geqslant|ll|gg|subset|subseteq|supset|supseteq|in|to|mapsto|Rightarrow|Leftarrow|iff|implies|coloneqq|eqqcolon)(?![A-Za-z])/;
  const ALIGNED = /^(?:align|flalign)\*?$/, ONE = /^(?:inline|display|equation\*?|displaymath)$/;
  const kindOf = (m: RfMath) => m.kind === 'env' ? m.env! : m.kind;
  /** The forms math `m` can take: one row also inline and as an equation, several rows the environments of rows; an
   * unnumbered form only without a `\label` (its references would go). */
  function targets(m: RfMath) {
    const n = rows(m.body).length, labelled = /\\label\s*\{/.test(m.body);
    const all = n > 1 ? ['align', 'align*', 'gather', 'gather*', 'multline', 'multline*'] : ['inline', 'display', 'equation', 'equation*', 'align', 'align*'];
    return all.filter((t) => t !== kindOf(m) && !(labelled && (t.endsWith('*') || t === 'inline' || t === 'display')) && !(t === 'inline' && /\n\s*\n/.test(m.body)));
  }
  /** The edit that writes math `m` as form `to`: an environment out of a line of text takes the spaces around it. */
  function mathEdit(text: string, m: RfMath, to: string): PaEdit {
    const now = mathTo(text, m, to), a = now.startsWith('\n') ? m.from - /[ \t]*$/.exec(text.slice(0, m.from))![0].length : m.from;
    const b = now.endsWith('\n' + indentAt(text, m.from)) && m.kind === 'inline' ? m.to + /^[ \t]*/.exec(text.slice(m.to))![0].length : m.to;
    return { from: a, to: b, now };
  }
  const needsAms = (to: string) => /^(?:align|gather|multline|flalign)\*?$|^equation\*$|^[pbBvV]matrix$|^cases$/.test(to);
  /** Math `m` as form `to`: the body kept, `&` before each row's first top-level relation into `align`, the `&` out of it. */
  function mathTo(text: string, m: RfMath, to: string) {
    let body = m.body;
    const amp = (s: string) => tops(s, /&/);
    if (ALIGNED.test(to) && !ALIGNED.test(kindOf(m))) {
      let out = '', at = 0;
      for (const [a, b] of [...tops(body, ROW), [body.length, body.length] as [number, number]]) {
        const row = body.slice(at, a);
        const rel = amp(row).length ? null : tops(row, REL)[0];
        out += rel ? `${row.slice(0, rel[0])}&${row.slice(rel[0])}` : row;
        out += body.slice(a, b);
        at = b;
      }
      body = out;
    } else if (!ALIGNED.test(to) && ALIGNED.test(kindOf(m))) {
      for (const [a, b] of amp(body).reverse()) body = body.slice(0, a) + body.slice(b);
    }
    const ind = indentAt(text, m.from);
    if (to === 'inline') return `\\(${body.trim().replace(/\s+/g, ' ')}\\)`;
    if (to === 'display') return `\\[${body}\\]`;
    const own = /\n[ \t]*$/.test(body) ? body : `\n${ind}  ${body.trim()}\n${ind}`;
    const pre = m.kind === 'inline' && text.slice(lineStart(text, m.from), m.from).trim() ? `\n${ind}` : '';
    const post = m.kind === 'inline' && text.slice(m.to, lineEnd(text, m.to)).trim() ? `\n${ind}` : '';
    return `${pre}\\begin{${to}}${own}\\end{${to}}${post}`;
  }
  /** How to have package `pkg` (amsmath; mathtools loads it): loaded in the document, else its `\usepackage` after this
   * file's last one (or its `\documentclass`), else the line to add by hand. */
  function pkgEdit(text: string, all: string, pkg: string): { have: true } | { edit: PaEdit } | { line: string } {
    const with_ = pkg === 'amsmath' ? '(?:amsmath|mathtools)' : pkg;
    if (new RegExp(`\\\\(?:usepackage|RequirePackage)\\s*(?:\\[[^\\]]*\\])?\\s*\\{[^}]*\\b${with_}\\b`).test(blank(all))) return { have: true };
    const end = /\\begin\s*\{document\}/.exec(text)?.index ?? -1, line = `\\usepackage{${pkg}}`;
    if (end < 0) return { line };
    const b = blank(text.slice(0, end)), last = [...b.matchAll(/^[^\n]*\\(?:usepackage|documentclass)\b[^\n]*\n/gm)].at(-1);
    return last ? { edit: { from: last.index! + last[0].length, to: last.index! + last[0].length, now: `${line}\n` } } : { line };
  }
  const MATRIX = /^(?:[pbBvV]?matrix|array|cases)$/;
  /** The matrix around `at`: an environment of rows, or a display's body of rows and columns without one (`grid`). */
  function matrixAt(text: string, at: number, end = at) {
    const own = pick(envs(text).filter((e) => MATRIX.test(e.env!)), at, end);
    if (own) { let h = own.head!; if (own.env === 'array') h = group(text, text.slice(h).match(/^\s*/)![0].length + h); return { ...own, head: h, grid: false, body: text.slice(h, own.foot) }; }
    const m = mathAt(text, at, end);
    if (!m || m.kind === 'inline' || ALIGNED.test(kindOf(m)) || rows(m.body).length < 2 || !tops(m.body, /&/).length) return null;
    return { kind: 'grid', from: m.from + m.open.length, to: m.to - m.close.length, head: m.from + m.open.length, foot: m.to - m.close.length, grid: true, body: m.body };
  }
  /** Matrix `m` as environment `to`; `array` gets one centred column per column of its widest row. */
  function matrixTo(text: string, m: RfSpan & { grid: boolean; body: string }, to: string) {
    const cols = Math.max(...rows(m.body).map((r) => tops(r, /&/).length + 1));
    const open = `\\begin{${to}}${to === 'array' ? `{${'c'.repeat(cols)}}` : ''}`;
    if (!m.grid) return `${open}${m.body}\\end{${to}}`;
    const ind = indentAt(text, m.from), body = /^\s*\n/.test(m.body) ? m.body.replace(/\s*$/, '') : ` ${m.body.trim()}`;
    return `\n${ind}  ${open}${body}\n${ind}  \\end{${to}}\n${ind}`;
  }

  // ---------------------------------------------------------------- extract to \newcommand
  /** The offsets where `body` stands in `text` outside comments. */
  const occurrences = (text: string, body: string) => { const b = blank(text), out: number[] = []; if (!body) return out; for (let i = b.indexOf(body); i >= 0; i = b.indexOf(body, i + body.length)) if (text.startsWith(body, i)) out.push(i); return out; };
  /** `\newcommand{\name}{…}` of the text at `[a, b)`, in the preamble after its last definition or package (a file without
   * one: at its top), the selection (`every`: each place the text stands) written as `\name`. */
  function extract(text: string, all: string, a: number, b: number, name: string, every: boolean) {
    const body = text.slice(a, b);
    if (!/^[A-Za-z]+$/.test(name)) return { error: 'A command name is letters only' };
    if (new RegExp(`\\\\${name}(?![A-Za-z])`).test(blank(all)) || ['begin', 'end', 'par', 'relax', 'def', 'item', 'label', 'ref', 'cite', 'section', 'text', 'emph'].includes(name)) return { error: `\\${name} is used already: pick another name` };
    if (!body.trim()) return { error: 'Select the text the command stands for' };
    if (group(`{${body}}`, 0) !== body.length + 2 || /(?<!\\)#/.test(body)) return { error: 'The selection must be balanced braces without #' };
    const end = /\\begin\s*\{document\}/.exec(text)?.index ?? -1, pre = blank(end < 0 ? '' : text.slice(0, end));
    const last = [...pre.matchAll(/^[^\n]*\\(?:usepackage|documentclass|(?:re)?newcommand|providecommand|DeclareMathOperator|NewDocumentCommand|def)\b[^\n]*\n/gm)].at(-1);
    const top = end < 0 ? (/^(?:[ \t]*%[^\n]*\n)*/.exec(text)![0].length) : last ? last.index! + last[0].length : end;
    const use = (at: number) => `\\${name}${/^[A-Za-z\s]/.test(text.slice(at, at + 1)) ? '{}' : ''}`;
    const sites = every ? occurrences(text, body).filter((i) => i >= top || end < 0) : [a];
    if (!sites.includes(a)) sites.push(a);
    return { edits: [{ from: top, to: top, now: `\\newcommand{\\${name}}{${body}}\n` }, ...sites.map((i) => ({ from: i, to: i + body.length, now: use(i + body.length) }))] };
  }
  // ---------------------------------------------------------------- smart wrap
  /** The document's own line length: the 95th percentile of its wrapped lines (a line the next one continues), 80
   * when it wraps too few lines to tell (one paragraph per line). */
  function lineHabit(text: string) {
    const L = text.split('\n'), wrapped = L.filter((l, i) => l.trim() && L[i + 1]?.trim() && !/^\s*[%\\]/.test(l)).map((l) => l.length).sort((a, b) => a - b);
    if (wrapped.length < 8) return 80;
    const p = wrapped[Math.floor(wrapped.length * 0.95)]!;
    return p >= 40 && p <= 160 ? p : 80;
  }
  const VERB = /^(?:verbatim\*?|Verbatim\*?|lstlisting|minted|comment|filecontents\*?|alltt)$/;
  /** The edit that breaks the line around `at` when it is longer than `max`: one space, at or before `max`, becomes a
   * line break (with the line's indentation when `indent`), the first one past it when none fits. The best space first: after a sentence's end, after an
   * argument's closing brace, after `,;:`, else the last. Never in math, an option list `[…]`, a `\verb`, a comment,
   * after `\` (a control space) or in a verbatim environment. Null when the line fits or has no such space. */
  function wrapLine(text: string, at: number, max: number, indent = true): PaEdit | null {
    const a = lineStart(text, at), b = lineEnd(text, at), line = text.slice(a, b);
    if (line.length <= max || envs(text).some((e) => VERB.test(e.env!) && e.head! <= a && b <= e.foot!)) return null;
    const ind = /^[ \t]*/.exec(line)![0], spaces: [number, number][] = [];
    let math: string | null = null, sq = 0, d = 0;
    for (let i = 0; i < line.length; i++) {
      const c = line[i]!;
      if (c === '\\') {
        const m = /^\\(?:verb\*?(.)|[A-Za-z@]+|.)/.exec(line.slice(i))!;
        if (m[1]) { const e = line.indexOf(m[1], i + m[0].length); if (e < 0) break; i = e; continue; }
        const s = m[0];
        if (s === '\\(' || s === '\\[') math = s === '\\(' ? '\\)' : '\\]';
        else if (s === math) math = null;
        i += s.length - 1;
        continue;
      }
      if (c === '%') break;
      if (c === '$') { if (math === '$' || math === '$$') { math = null; if (line[i + 1] === '$') i++; } else if (!math) { math = line[i + 1] === '$' ? '$$' : '$'; if (math === '$$') i++; } continue; }
      if (math) continue;
      if (c === '{') d++;
      else if (c === '}') d--;
      else if (c === '[' && /[\]A-Za-z@*}]$/.test(line.slice(0, i))) sq++;
      else if (c === ']' && sq) sq--;
      else if (c === ' ' && !sq && i > ind.length && line[i - 1] !== ' ' && line[i + 1] && line[i + 1] !== ' ') {
        const before = line.slice(0, i);
        // a sentence ends at `.!?` (a closing quote or bracket after it), not after one capital (`A. Turing`)
        const rank = /(?<![A-Z]|\b[a-z])[.!?]['")\]]*$/.test(before) && d === 0 ? 4 : /\}$/.test(before) && d === 0 ? 3 : /[,;:]$/.test(before) ? 2 : 1;
        spaces.push([i, rank]);
      }
    }
    // the best rank; within it the last, so the line stays as full as it can
    // no space that fits: the first one past the length, so the line is as short as it can be
    const best = spaces.filter(([i]) => i <= max).reduce<[number, number] | null>((x, y) => !x || y[1] > x[1] || y[1] === x[1] && y[0] > x[0] ? y : x, null) ?? spaces[0] ?? null;
    return best ? { from: a + best[0], to: a + best[0] + 1, now: `\n${indent ? ind : ''}` } : null;
  }

  // ---------------------------------------------------------------- insert
  const TIKZ = '\\begin{tikzpicture}\n  \\node[draw] (a) at (0,0) {A};\n  \\node[draw] (b) at (3,0) {B};\n  \\draw[->] (a) -- (b);\n\\end{tikzpicture}';
  /** The heading level of the last heading before `at` (a new one is its sibling), `section` when none. */
  const level = (text: string, at: number) => [...blank(text.slice(0, at)).matchAll(/\\(part|chapter|(?:sub)*section|(?:sub)?paragraph)\*?\s*[[{]/g)].at(-1)?.[1] ?? 'section';
  /** What Insert offers, in menu order. Figures and tables go into a float by `wrap` (caption, label, the caret in the caption). */
  const inserts: InsItem[] = [
    { kind: 'figure-image', label: 'Figure with an image', group: 'Floats', hint: '\\begin{figure} \\includegraphics', body: '\\includegraphics[width=0.8\\linewidth]{example-image}', wrap: true, widget: 'image' },
    { kind: 'figure-drawing', label: 'Figure with a drawing', group: 'Floats', hint: '\\begin{figure} tikzpicture', body: TIKZ, wrap: true, widget: 'tikz' },
    { kind: 'table', label: 'Table', group: 'Floats', hint: '\\begin{table} tabular', body: '\\begin{tabular}{ll}\n  Name & Value \\\\\n  \\hline\n  a & 1 \\\\\n\\end{tabular}', wrap: true, widget: 'table' },
    { kind: 'itemize', label: 'Bulleted list', group: 'Lists', hint: '\\begin{itemize}', body: '\\begin{itemize}\n  \\item \x01\n\\end{itemize}', tight: true },
    { kind: 'enumerate', label: 'Numbered list', group: 'Lists', hint: '\\begin{enumerate}', body: '\\begin{enumerate}\n  \\item \x01\n\\end{enumerate}', tight: true },
    { kind: 'equation', label: 'Equation', group: 'Math', hint: '\\begin{equation}', body: '\\begin{equation}\n  \x01\n\\end{equation}', tight: true },
    { kind: 'align', label: 'Aligned equations', group: 'Math', hint: '\\begin{align}', body: '\\begin{align}\n  \x01\n\\end{align}', tight: true },
    { kind: 'heading', label: 'Section heading', group: 'Structure', hint: '\\section{…}', body: (text, at) => `\\${level(text, at)}{\x01}` },
    { kind: 'footnote', label: 'Footnote', group: 'Structure', hint: '\\footnote{…}', body: '\\footnote{\x01}', flow: true },
    { kind: 'cite', label: 'Citation', group: 'References', hint: '~\\cite{…}', body: '~\\cite{\x01}', flow: true, before: true },
    { kind: 'ref', label: 'Reference', group: 'References', hint: '~\\ref{…}', body: '~\\ref{\x01}', flow: true, before: true },
    { kind: 'tikz', label: 'TikZ picture', group: 'Pictures', hint: '\\begin{tikzpicture}', body: TIKZ, widget: 'tikz' },
    { kind: 'forest', label: 'Forest tree', group: 'Pictures', hint: '\\begin{forest}', body: '\\begin{forest}\n  [Root\n    [Child]\n    [Child]\n  ]\n\\end{forest}', widget: 'forest' },
  ];
  /** The innermost box around `at` that holds no paragraph: math, a picture or tabular (WRAP), an alignment (a body
   * with a top-level `&`, which TeX takes only in one, tex.web § 1128); a float too for a `float`, which needs outer
   * paragraph mode (latex.ltx `\@xfloat`). Null in running text; a stray `&` in the document's body is its error, no box. */
  function boxAt(text: string, at: number, float: boolean): RfSpan | null {
    const inside = (s: RfSpan) => s.from < at && at < s.to;
    const all = envs(text).filter((e) => inside(e) && (WRAP.test(e.env!) || float && FLOAT.test(e.env!) || e.env !== 'document' && tops(text.slice(e.head, e.foot), /&/).length > 0));
    const m = mathAt(text, at);
    return [...all, ...m && inside(m) ? [m] : []].sort((a, b) => (a.to - a.from) - (b.to - b.from))[0] ?? null;
  }
  /** Where an insert goes: at the caret; at a blank place between paragraphs; after the block clicked (a mark of the
   * running text at its paragraph's last character). Null for a mark where no paragraph is. */
  function insertSpot(text: string, t: InsTarget, flow: boolean, float = false): { at: number; mode: InsMode } | null {
    if (t.caret != null) {
      // a block never splits a group (a macro's argument, `\greet{wo|rld}`) nor stands in a box: it goes after the
      // line the group or box closes on; a box in a group and a group in a box, each left in turn
      let out = t.caret;
      for (let was = -1; !flow && was !== out;) { was = out; out = groupEnd(text, out) ?? out; out = boxAt(text, out, float)?.to ?? out; }
      return out === t.caret ? { at: out, mode: 'caret' } : { at: lineEnd(text, out), mode: 'after' };
    }
    if (t.place != null) return flow ? null : { at: t.place, mode: 'after' };
    if (!t.block) return null;
    let end = 0;
    for (let n = 1; n < t.block.to && text.indexOf('\n', end) >= 0; n++) end = text.indexOf('\n', end) + 1;
    end = lineEnd(text, end);
    if (!flow) return { at: end, mode: 'after' };
    if (t.block.what !== 'paragraph') return null;
    while (end > 0 && /\s/.test(text[end - 1]!)) end--;
    return { at: end, mode: 'end' };
  }
  /** The edits that write insert `kind` at `at`: a block on lines of its own (blank lines around it, a display inside a
   * paragraph at the caret only on its own lines), a mark in the text (`~` after a word); a float by `wrap`. The
   * package a new name needs is the snapshot printer's: it loads what the run does not define, in the same write. */
  function insertPlan(text: string, all: string, kind: string, at: number, mode: InsMode): InsPlan | null {
    const it = inserts.find((k) => k.kind === kind);
    if (!it) return null;
    let body = typeof it.body === 'function' ? it.body(text, at) : it.body, pre = '', post = '';
    if (it.flow) {
      if (it.before && mode === 'end') while (/[.,;:!?]/.test(text[at - 1] ?? '')) at--;
      if (body[0] === '~' && (at === 0 || /\s/.test(text[at - 1]!))) body = body.slice(1);
    } else {
      const sep = it.tight && mode === 'caret' ? '\n' : '\n\n', a = lineStart(text, at), e = lineEnd(text, at), ind = indentAt(text, at);
      const before = text.slice(a, at), next = e < text.length ? text.slice(e + 1, lineEnd(text, e + 1)) : '', prev = a ? text.slice(lineStart(text, a - 1), a - 1) : '';
      pre = before.trim() ? sep + ind : before ? '' : prev.trim() ? sep.slice(1) : '';
      post = text.slice(at, e).trim() ? sep + ind : next.trim() && !/^\s*\\end\b/.test(next) ? sep.slice(1) : '';
      body = body.split('\n').map((l, i) => (i && l ? ind + l : l)).join('\n');
    }
    // a line of blanks is an empty line, a \par to TeX ("Paragraph ended before \align was complete"): a caret line with
    // nothing else on it takes the next line, so the insert builds as written before anything is typed
    body = body.replace(/\n([ \t]*)\x01\n[ \t]*/, '\n$1\x01');
    const k = body.indexOf('\x01'), clean = body.replace('\x01', ''), start = at + pre.length;
    let now = text.slice(0, at) + pre + clean + post + text.slice(at), caret = k >= 0 ? start + k : start + clean.length, open = start;
    if (it.wrap) {
      const r = wrap(now, wrapTarget(now, start)!, all);
      now = now.slice(0, r.edit.from) + r.edit.now + now.slice(r.edit.to);
      caret = r.caret;
      open = now.indexOf(clean.split('\n')[0]!, r.edit.from);
    }
    // one edit: what differs between the text and the text with the insert
    let p = 0, s = 0;
    while (p < text.length && text[p] === now[p]) p++;
    while (s < text.length - p && text[text.length - 1 - s] === now[now.length - 1 - s]) s++;
    const edits: PaEdit[] = [{ from: p, to: text.length - s, now: now.slice(p, now.length - s) }];
    return { edits, caret, open, widget: it.widget ?? null };
  }
  self.satexRefactor = { formatToggle, inserts, insertSpot, insertPlan, lineHabit, wrapLine, blank, envs, group, wrapTarget, wrap, unwrap, slug, firstWords, mathAt, rows, targets, mathTo, mathEdit, needsAms, pkgEdit, matrixAt, matrixTo, extract, occurrences };

  // ---------------------------------------------------------------- the plugins
  const PA = self.satexPageActions, R = self.satexRefactor, TARGETS = ['source', 'block', 'tikz-node', 'tikz-label', 'tikz-edge', 'forest-node', 'table-column'];
  const LABELS: Record<string, string> = { inline: 'Inline \\(…\\)', display: 'Display \\[…\\]' };
  /** amsmath with the change, in the same undo step; null (and a toast with the line) when this file has no preamble. */
  function ams(ctx: PaCtx, to: string): PaEdit[] {
    if (!R.needsAms(to)) return [];
    const r = R.pkgEdit(ctx.text, ctx.all, 'amsmath');
    if ('edit' in r) return [r.edit];
    if ('line' in r) ctx.toast(`${to} needs ${r.line} in the preamble: add that line there`);
    return [];
  }
  PA.register({
    name: 'refactor-float', title: 'Wrap in a float', category: 'refactor', type: 'refactor', targets: TARGETS,
    about: 'A picture, tabular, forest tree or image wrapped in a figure or table (centred, caption, label, alt text), or a float unwrapped',
    groups(t, ctx) {
      const w = R.wrapTarget(ctx.text, t.at ?? 0, t.end ?? t.at ?? 0);
      if (t.at == null || !w) return [];
      const items: PaItem[] = [];
      if (!w.float) {
        const go = (alt: string | null) => { const r = R.wrap(ctx.text, w, ctx.all, alt); ctx.write([r.edit])?.then?.((ok: boolean) => { if (ok) ctx.caret?.(r.caret); }); };
        items.push(w.env === 'includegraphics'
          ? { label: 'Wrap in a figure…', hint: 'alt text', run: () => PA.ask(ctx.x, ctx.y, R.firstWords(ctx.text, w), 'the image\'s alt text, read aloud in its place', (v) => go(v), true) }
          : { label: `Wrap in a ${w.kind}`, hint: w.env, run: () => go(null) });
      } else {
        items.push({ label: `Unwrap the ${w.float.env!.replace(/\*$/, '')}`, hint: 'keeps its content', run: () => {
          const r = R.unwrap(ctx.text, w.float!);
          ctx.write([r.edit]);
          if (r.dropped.length) ctx.toast(`Removed ${r.dropped.join(' ')}`);
        } });
      }
      return [{ title: 'Refactor', items }];
    },
  });
  PA.register({
    name: 'refactor-math', title: 'Math conversions', category: 'refactor', type: 'refactor', targets: ['source', 'block'],
    about: 'Inline and display math into each other and into equation, align, gather or multline (& at each row\'s relation), a grid into a matrix or cases; loads amsmath',
    groups(t, ctx) {
      if (t.at == null) return [];
      const m = R.mathAt(ctx.text, t.at, t.end ?? t.at), x = R.matrixAt(ctx.text, t.at, t.end ?? t.at), items: PaItem[] = [];
      if (m) for (const to of R.targets(m)) items.push({ label: `To ${LABELS[to] ?? to}`, run: () => ctx.write([...ams(ctx, to), R.mathEdit(ctx.text, m, to)]) });
      if (x) {
        const now = x.grid ? null : x.env, cols = Math.max(...R.rows(x.body).map((r) => r.split('&').length));
        const kinds = ['pmatrix', 'bmatrix', 'vmatrix', 'array', ...(cols <= 2 ? ['cases'] : [])].filter((k) => k !== now);
        items.push({ label: x.grid ? 'Rows into a matrix' : 'Matrix as', items: kinds.map((k) => ({ label: k, run: () => ctx.write([...ams(ctx, k), { from: x.from, to: x.to, now: R.matrixTo(ctx.text, x, k) }]) })) });
      }
      return items.length ? [{ title: 'Refactor', items: [{ label: 'Math', items }] }] : [];
    },
  });
  PA.register({
    name: 'refactor-extract', title: 'Extract to \\newcommand', category: 'refactor', type: 'refactor', targets: ['source'],
    about: 'The selected text into a \\newcommand in the preamble, named at a prompt; there, or each place the same text stands',
    groups(t, ctx) {
      const s: [number, number] | null = t.sel;
      if (!s || s[0] === s[1]) return [];
      const body = ctx.text.slice(s[0], s[1]), n = R.occurrences(ctx.text, body).length;
      const go = (every: boolean) => PA.ask(ctx.x, ctx.y, R.slug(body).replace(/-/g, '').replace(/[^a-z]/g, '').slice(0, 12) || 'mycmd', 'the new command\'s name, letters only', (v) => {
        const r = R.extract(ctx.text, ctx.all, s[0], s[1], v.trim().replace(/^\\/, ''), every);
        if ('error' in r) ctx.toast(r.error); else ctx.write(r.edits);
      }, true);
      return [{ title: 'Refactor', items: [{ label: 'Extract to \\newcommand…', hint: 'here', run: () => go(false) },
        ...(n > 1 ? [{ label: `Extract to \\newcommand, ${n} places…`, hint: 'each', run: () => go(true) }] : [])] }];
    },
  });
  /** A selection's format commands, text or math (the run's mode at it, as the source spells it): each wraps `\cmd{…}`.
   * Bold in math is `\bm` with bm loaded, `\boldsymbol` with amsmath, else `\mathbf`; Underline needs ulem (`\uline`) or
   * soul (`\ul`). A selection inside or around one of them unwraps it (Remove formatting). */
  const TEXT_FMT: [string, string][] = [['Bold', 'textbf'], ['Italic', 'textit'], ['Emphasis', 'emph'], ['Monospace', 'texttt'], ['Small caps', 'textsc']];
  const FMT_CMDS = /^(textbf|textit|emph|texttt|textsc|textrm|textsf|uline|ul|underline|mathbf|bm|boldsymbol|mathrm|mathsf|mathtt|textcolor|mathcolor|tiny|scriptsize|footnotesize|small|normalsize|large|Large|LARGE|huge|Huge)$/;
  /** The format command whose argument (or `{\size …}` group) is exactly `a`..`b` or holds it: [from, to, inner from, inner to]. */
  function formatAround(text: string, a: number, b: number): [number, number, number, number] | null {
    let best: [number, number, number, number] | null = null;
    for (const m of blank(text).matchAll(/\\([A-Za-z]+)(\{[^{}]*\})?\{|\{\\(tiny|scriptsize|footnotesize|small|normalsize|large|Large|LARGE|huge|Huge)\s/g)) {
      const name = m[1] ?? m[3]!;
      if (!FMT_CMDS.test(name) || m.index! > a) continue;
      // `group` gives the offset after the closing brace
      const open = m[3] ? m.index! : m.index! + m[0].length - 1, end = group(text, open);
      if (end === open || end - 1 < b) continue;
      const inner: [number, number] = [m[3] ? m.index! + m[0].length : open + 1, end - 1];
      if (inner[0] > a) continue;
      if (!best || m.index! > best[0]) best = [m.index!, end, inner[0], inner[1]];
    }
    return best;
  }
  /** Ctrl/Cmd+B or +I over a selection `[a, b]` (`cmd` textbf, textit): unwraps the call of `cmd` whose argument is the
   * selection (blanks aside), else wraps it. The edit and the caret after it, at the argument's end inside the call
   * (Right then leaves it, as at a typed one). */
  function formatToggle(text: string, a: number, b: number, cmd: string): { edit: PaEdit; caret: number } {
    const around = formatAround(text, a, b), name = around && /^\\([A-Za-z]+)/.exec(text.slice(around[0]))?.[1];
    if (around && name === cmd && !text.slice(around[2], a).trim() && !text.slice(b, around[3]).trim()) {
      const inner = text.slice(around[2], around[3]);
      return { edit: { from: around[0], to: around[1], now: inner }, caret: around[0] + inner.length };
    }
    return { edit: { from: a, to: b, now: `\\${cmd}{${text.slice(a, b)}}` }, caret: b + cmd.length + 2 };
  }
  function formatMenu(t: PaTarget, ctx: PaCtx): PaGroup[] {
    const s: [number, number] | null = t.sel;
    if (!s || s[0] === s[1]) return [];
    const [a, b] = s, body = ctx.text.slice(a, b), all = blank(ctx.all), has = (p: string) => new RegExp(`\\\\(?:usepackage|RequirePackage)\\s*(?:\\[[^\\]]*\\])?\\s*\\{[^}]*\\b${p}\\b`).test(all);
    const math = !!R.mathAt(ctx.text, a, b);
    const wrap = (pre: string, post = '}', pkg: string | null = null) => () => {
      const load = pkg ? pkgEdit(ctx.text, ctx.all, pkg) : { have: true as const };
      if ('line' in load) ctx.toast(`Needs ${load.line} in the preamble`);
      ctx.write([...'edit' in load ? [load.edit] : [], { from: a, to: b, now: `${pre}${body}${post}` }]);
    };
    const fmt: PaItem[] = math
      ? [{ label: 'Bold', hint: has('bm') ? '\\bm' : has('(?:amsmath|amsbsy|mathtools)') ? '\\boldsymbol' : '\\mathbf' }, { label: 'Roman', hint: '\\mathrm' }, { label: 'Sans serif', hint: '\\mathsf' }, { label: 'Monospace', hint: '\\mathtt' }]
        .map((r) => ({ ...r, run: wrap(`${r.hint}{`) }))
      : [...TEXT_FMT.map(([label, c]) => ({ label, hint: `\\${c}`, run: wrap(`\\${c}{`) })),
        ...has('ulem') ? [{ label: 'Underline', hint: '\\uline', run: wrap('\\uline{') }] : has('soul') ? [{ label: 'Underline', hint: '\\ul', run: wrap('\\ul{') }] : []];
    const around = formatAround(ctx.text, a, b);
    if (around) fmt.push({ label: 'Remove formatting', hint: ctx.text.slice(around[0], around[2]).trim(), run: () => ctx.write([{ from: around[0], to: around[1], now: ctx.text.slice(around[2], around[3]) }]) });
    const c = PA.colors(ctx.all, ctx.facts);
    // color in math: \mathcolor where the kernel has it (2022-06 on), else a group, which every kernel takes
    const mc = (ctx.kernel ?? '') >= '2022-06';
    const color = [...c.doc, ...c.base].map(([n, css]): PaItem => ({ label: n, swatch: css, run: math ? wrap(mc ? `\\mathcolor{${n}}{` : `{\\color{${n}}`, '}', 'xcolor') : wrap(`\\textcolor{${n}}{`, '}', 'xcolor') }));
    const font: PaItem[] = math ? [] : [['Roman', 'textrm'], ['Sans serif', 'textsf'], ['Typewriter', 'texttt']].map(([label, cmd]) => ({ label: label!, hint: `\\${cmd}`, run: wrap(`\\${cmd}{`) }));
    const size: PaItem[] = math ? [] : ['tiny', 'scriptsize', 'footnotesize', 'small', 'normalsize', 'large', 'Large', 'LARGE', 'huge', 'Huge'].map((z) => ({ label: z, hint: `{\\${z} …}`, run: wrap(`{\\${z} `) }));
    return [{ title: 'Format', items: fmt }, { title: 'Color', items: color }, { title: 'Font', items: font }, { title: 'Size', items: size }].filter((g) => g.items.length);
  }
  PA.register({
    name: 'format', title: 'Format', category: 'refactor', type: 'refactor', targets: ['source'],
    about: 'The selection bold, italic, emphasized, monospace, small caps or underlined (with ulem or soul), in a color (xcolor loaded), a font family or a size; in math \\bm, \\boldsymbol or \\mathbf, \\mathrm, \\mathsf, \\mathtt and \\mathcolor. Remove formatting unwraps the command around it',
    groups: formatMenu,
  });
  /** Insert's rows by group (Math ▸, Floats ▸, Lists ▸, Structure ▸, References ▸, Pictures ▸), each hinted with the LaTeX
   * it writes; Math starts with the Type caret's inline, display and align* math when the caret is there (`ctx.math`). */
  function insertMenu(t: PaTarget, ctx: PaCtx, para: boolean): PaItem[] {
    const at = t.caret ?? t.place ?? t.at ?? 0;
    const math: PaItem[] = ctx.math && t.kind === 'caret' ? [['inline', 'Inline math', ctx.mathInline ?? '\\(…\\)'], ['display', 'Display math', '\\[…\\]'], ['align', 'Aligned', 'align*']]
      .map(([k, label, hint]) => ({ label: label!, hint, run: () => ctx.math!(k!) })) : [];
    const rows = R.inserts.filter((k) => para || !k.flow).map((k) => ({ group: k.group, label: k.label, hint: k.kind === 'heading' ? `\\${level(ctx.text, at)}{…}` : k.hint, run: () => ctx.insert!(k.kind, t) }));
    return ['Math', 'Floats', 'Lists', 'Structure', 'References', 'Pictures'].map((g) => ({ label: g, items: [...g === 'Math' ? math : [], ...rows.filter((r) => r.group === g).map(({ group: _, ...r }) => r)] }))
      .filter((g) => g.items.length);
  }
  PA.register({
    name: 'insert', title: 'Insert', type: 'insert', targets: ['block', 'place', 'caret'],
    about: 'New content where the PDF or the Typeset view is right-clicked (after the paragraph, or between paragraphs) and at the caret from the Type bar\'s Insert menu: a figure with an image or a drawing, a table, a list, an equation, a heading, a footnote, a citation or reference, a TikZ picture, a forest tree. Its widget opens at once, the package it needs is loaded, one undo step',
    groups(t, ctx) {
      if (!ctx.insert) return [];
      // a mark of the running text (a footnote, a citation) only where a paragraph is
      const para = t.kind === 'caret' || t.place == null && t.block?.what === 'paragraph';
      return [{ title: 'Insert', items: insertMenu(t, ctx, para) }];
    },
  });
})();
