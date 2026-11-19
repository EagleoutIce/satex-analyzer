// Refactor plugins (actions.ts registry, category `refactor`): rewrites of the source a right click offers in the
// editor, the PDF and the Typeset view. A float around a picture, a table or an image (and back), math from one
// display to another, a selection to a `\newcommand`. Each rewrite is a pure function of the text here (unit-tested in
// tests/refactor.test.mjs); a menu row writes its edits through `ctx.write`: one undo step through the snapshot printer.
export {};
declare global {
  /** A span of the source: `[from, to)` offsets; `env` an environment's name, `head` the end of its `\begin{..}[..]`. */
  type RfSpan = { kind: string; from: number; to: number; env?: string; head?: number; foot?: number };
  type RfMath = RfSpan & { open: string; close: string; body: string };
  interface Window {
    satexRefactor: {
      blank(text: string): string; envs(text: string): RfSpan[]; group(text: string, at: number): number;
      wrapTarget(text: string, at: number, end?: number): (RfSpan & { float: RfSpan | null }) | null;
      wrap(text: string, t: RfSpan, labels: string, alt?: string | null): { edit: PaEdit; caret: number };
      unwrap(text: string, f: RfSpan): { edit: PaEdit; dropped: string[] };
      slug(s: string): string; firstWords(text: string, t: RfSpan): string;
      mathAt(text: string, at: number, end?: number): RfMath | null; rows(body: string): string[]; targets(m: RfMath): string[];
      mathTo(text: string, m: RfMath, to: string): string; needsAms(to: string): boolean;
      pkgEdit(text: string, all: string, pkg: string): { have: true } | { edit: PaEdit } | { line: string };
      matrixAt(text: string, at: number, end?: number): (RfSpan & { grid: boolean; body: string }) | null; matrixTo(text: string, m: RfSpan & { grid: boolean; body: string }, to: string): string;
      extract(text: string, all: string, a: number, b: number, name: string, every: boolean): { edits: PaEdit[] } | { error: string };
      occurrences(text: string, body: string): number[];
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
    return { edit: { from, to, now: out }, caret: from + out.indexOf(cap) + cap.length };
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
    const own = /^\s*\n/.test(body) ? body : `\n${ind}  ${body.trim()}\n${ind}`;
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
  self.satexRefactor = { blank, envs, group, wrapTarget, wrap, unwrap, slug, firstWords, mathAt, rows, targets, mathTo, needsAms, pkgEdit, matrixAt, matrixTo, extract, occurrences };

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
      if (m) for (const to of R.targets(m)) items.push({ label: `To ${LABELS[to] ?? to}`, run: () => ctx.write([...ams(ctx, to), { from: m.from, to: m.to, now: R.mathTo(ctx.text, m, to) }]) });
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
})();
