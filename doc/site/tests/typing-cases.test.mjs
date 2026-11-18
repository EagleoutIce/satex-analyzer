// Type mode (pdfedit.ts `key`): the examples fuzz and the spots that broke. Moved from views.test.mjs; the runner is tests/run.mjs.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { test, load } from './harness.mjs';

/** The site's directory: the fixtures' URLs are relative to it. */
const SITE = new URL('../', import.meta.url);
// Type mode keeps the source's structure: a property test over the site's examples and synthetic documents. From
// thousands of carets where a page word can put one (plain text or math), random keys (letters, space, Backspace,
// Delete, Enter, `{`, `}`, `$`, `\`, `_`, `^`, `%`, `&`) go through E.key, the one function the playground uses;
// after each edit an independent reader checks: braces balanced, no group closed by typing, every command name,
// environment, comment and math pair as before (commands typed since the caret was put may change), no new paragraph
// inside a group, no new script outside math, no new `&` but by Enter. Pins `\section{… an }` left by typing into a
// section title (a paragraph break inside the argument), a `$` inside `\(…\)`, `\` before a word renaming it.
test("Type mode keeps the source's structure", async () => {
  const E = self.satexPdfEdit;
  const dir = new URL('./examples/', SITE);
  const docs = readdirSync(dir).filter((f) => f.endsWith('.tex')).map((f) => readFileSync(new URL(f, dir), 'utf8'));
  docs.push(String.raw`\documentclass{article}
\newcommand{\greet}[1]{Hello, #1!}
\begin{document}
\section[Short]{Long title with \emph{words}}\label{sec:a}
Text with a footnote\footnote{Some \textbf{\emph{nested} words} here.} and \TeX{} words, \LaTeX\ too.
\caption[Short cap]{A long caption, see Section~\ref{sec:a}.}
\begin{tabular}{l|c}
\hline
one & two \\
\multicolumn{2}{c}{three words} \\
four & $x^2$ \\
\end{tabular}
Math $a+b=c$ and \(x_i\) and \[\int_0^1 f\] and
\begin{align*}
  y &= mx + b \\
  z &= 2
\end{align*}
\begin{itemize}
  \item[label] first item
  \item second {\bfseries item}
\end{itemize}
\verb|a{b| and \lstinline{c_d} and
\begin{verbatim}
raw { text % here
\end{verbatim}
\tikz \node[draw] (a) at (0,0) {node text};
\greet{World} % a comment
\end{document}
`);
  // the independent reader: braces, commands, environments, comments, math, blank lines in groups, scripts, tabs
  const verbs = (t) => E.verbatim(t);
  const read = (t, mark = Infinity, end = t.length) => {
    const r = { depth: 0, low: 0, after: Infinity, cs: [], env: [], com: [], math: '', bad: 0, parIn: 0, script: 0, amp: 0, opens: [] };
    const vs = verbs(t);
    let k = 0;
    for (let i = 0; i < end; i++) {
      while (vs[k] && vs[k].to < i) k++;
      if (vs[k] && vs[k].from <= i) { i = vs[k].to - 1; k++; continue; }
      const c = t[i];
      if (c === '%') { const e = t.indexOf('\n', i); r.com.push(t.slice(i, e < 0 ? t.length : e)); i = e < 0 ? t.length : e - 1; continue; }
      if (c === '\\') {
        const m = /^\\([A-Za-z@]+|[\s\S]?)/.exec(t.slice(i, i + 64)), n = m[1];
        if (/^[A-Za-z@]/.test(n)) {
          r.cs.push([i, n]);
          const env = (n === 'begin' || n === 'end') && /^\{([^{}]*)\}/.exec(t.slice(i + m[0].length));
          if (env) r.env.push(`${n}:${env[1]}`);
          if (env && /^(align|equation)\*?$/.test(env[1])) r.math = n === 'begin' ? (r.math ? (r.bad++, r.math) : env[1]) : (r.math === env[1] ? '' : (r.bad++, r.math));
        } else if (n === '(' || n === '[') { if (r.math) r.bad++; r.math = n; } else if (n === ')' || n === ']') { if (r.math !== (n === ')' ? '(' : '[')) r.bad++; r.math = ''; }
        i += m[0].length - 1;
        continue;
      }
      if (c === '$') { const two = t[i + 1] === '$' && (r.math === '$$' || !r.math), d = two ? '$$' : '$'; if (!r.math) r.math = d; else if (r.math === d) r.math = ''; else r.bad++; i += d.length - 1; continue; }
      if (c === '{') { r.depth++; r.opens.push(i); }
      else if (c === '}') { r.depth--; r.opens.pop(); r.low = Math.min(r.low, r.depth); if (i >= mark) r.after = Math.min(r.after, r.depth); }
      else if (c === '\n' && /^\n[ \t]*\n/.test(t.slice(i, i + 64)) && r.depth > 0) r.parIn++;
      else if ((c === '_' || c === '^') && !r.math) r.script++;
      else if (c === '&') r.amp++;
    }
    if (r.math) r.bad++;
    return r;
  };
  // a page word's caret: in or at the edge of a letter run not after a backslash, where Type mode puts one
  let seed = 0x5a7e;
  const rand = (n) => { seed = (seed * 1103515245 + 12345) >>> 0; return (seed >>> 8) % n; };
  const KEYS = [...'abcdefghijklmnopqrstuvwxyz', ' ', ' ', 'Backspace', 'Backspace', 'Delete', 'Ctrl+Backspace', 'Ctrl+Delete', 'Enter', 'Shift+Enter', '{', '}', '$', '\\', '\\', '_', '^', '%', '&'];
  const ESCW = /^(textbackslash|textasciitilde|textasciicircum)$/;
  let edits = 0, refused = 0;
  const fails = [];
  for (const doc of docs) {
    const base = read(doc), carets = [];
    for (const m of doc.matchAll(/(?<![\\A-Za-z@])\p{L}+/gu)) for (let p = m.index; p <= m.index + m[0].length; p++) if (/^$|math/.test(E.editable(doc, p, p, []) ?? '')) carets.push(p);
    for (let run = 0; run < 2400 / docs.length * 4 && carets.length; run++) {
      let t = doc, at = carets[rand(carets.length)], f = null, fresh = new Array(t.length).fill(false);
      for (let n = 1 + rand(12); n--;) {
        const k = KEYS[rand(KEYS.length)], e = E.key(t, at, k, { fresh: f, inline: rand(2) ? '\\(' : '$', smart: !!rand(2) });
        if (typeof e === 'string') { refused++; continue; }
        const [a, b, body, step] = e, was = t;
        t = t.slice(0, a) + body + t.slice(b);
        fresh = [...fresh.slice(0, a), ...Array(body.length).fill(true), ...fresh.slice(b)];
        f = E.grow(f, a, b, body.length);
        at = a + step;
        edits++;
        const r = read(t), why = [];
        if (r.depth !== base.depth || r.low < base.low) why.push('braces');
        const own = (cs) => cs.filter(([i, nm]) => !ESCW.test(nm) && !fresh.slice(i, i + nm.length + 1).every(Boolean)).map(([, nm]) => nm).join(' ');
        const own0 = base.cs.filter(([, nm]) => !ESCW.test(nm)).map(([, nm]) => nm).join(' ');
        if (own(r.cs) !== own0) why.push('commands');
        if (r.env.join() !== base.env.join()) why.push('environments');
        if (r.com.join('\n') !== base.com.join('\n')) why.push('comments');
        if (r.bad > base.bad) why.push('math');
        if (r.parIn > base.parIn) why.push('paragraph in a group');
        if (r.script > base.script) why.push('script outside math');
        if (r.amp > read(was).amp && k !== 'Enter') why.push('alignment tab');
        // what was typed closes no group it did not open
        if (body && read(t, a, a + body.length).after < read(was, Infinity, a).depth) why.push('typed text left its group');
        if (why.length) { fails.push({ why, k, at: a, before: was.slice(Math.max(0, a - 30), b + 30), after: t.slice(Math.max(0, a - 30), a + body.length + 30) }); t = was; at = Math.min(at, t.length); break; }
      }
    }
  }
  assert.ok(edits > 2000, `the fuzz made ${edits} edits`);
  assert.deepEqual(fails.slice(0, 3), [], `${fails.length} Type-mode edits broke the source's structure`);
});
// Type mode's rules at the spots that broke: Enter in a section title (the bug: `\section{… an }` and a paragraph
// break inside the argument, a runaway), `\` before a word (it renamed it `\Background`), `$` inside `\(…\)`, Backspace
// at a group's edge, a comment's line end.
test("Type mode's rules at the spots that broke", async () => {
  const E = self.satexPdfEdit;
  const t = '\\section{Background}\\label{sec:b}\nText % note\nmore \\(x\\) end.';
  const at = (s, d = 0) => t.indexOf(s) + d;
  assert.match(E.key(t, at('ground'), 'Enter'), /paragraph cannot end inside \\section/, 'Enter in a title: refused');
  assert.deepEqual(E.key(t, at('ground'), 'Shift+Enter'), [at('ground'), at('ground'), '\n', 1], 'Shift+Enter: a line break, a space');
  assert.deepEqual(E.key(t, at('Back'), '\\'), [at('Back'), at('Back'), '\\ ', 1], '`\\` keeps off the word after it');
  assert.equal(typeof E.key(t, at('Back'), 'Backspace'), 'string', "Backspace at a group's start keeps the brace");
  // Ctrl+Backspace takes the word before the caret and the blanks after it, never a command's name; pins one character taken
  assert.deepEqual(E.key('one teh  x', 8, 'Ctrl+Backspace'), [4, 8, '', 0], 'Ctrl+Backspace: a word and its blanks');
  assert.deepEqual(E.key('one teh', 0, 'Ctrl+Delete'), [0, 3, '', 0], 'Ctrl+Delete: the word after');
  assert.deepEqual(E.key('a \\emph', 7, 'Ctrl+Backspace'), E.key('a \\emph', 7, 'Backspace'), 'a name: as Backspace');
  assert.equal(typeof E.key(t, at('note'), 'a'), 'string', 'a comment is no text');
  assert.equal(typeof E.key(t, at('note', 4), 'Delete'), 'string', "a comment's line end stays");
  assert.deepEqual(E.key(t, at('x\\)', 1), '$'), [at('x\\)', 1), at('x\\)', 1), '', 2], '`$` in \\(…\\) steps over its closer');
  assert.deepEqual(E.key(t, at('x\\)'), '$'), [at('x\\)'), at('x\\)'), '', 3], '`$` elsewhere in it leaves the formula');
  assert.deepEqual(E.key(t, at('x\\)'), '%'), [at('x\\)'), at('x\\)'), '\\%', 2], '`%` in math is escaped');
  assert.equal(typeof E.key(t, at('sec:b', 2), 'a'), 'string', 'a label is a key');
  // keys in a row, each on the text the one before left (the playground's order): Backspace right after a formula's
  // `\)` takes its last character, and a formula of one character whole; Delete before `\(` the same from the front.
  // Pins Backspace taking the `\)` alone (refused by the gate, so it did nothing) and a lone delimiter left behind.
  const typeKeys = (text, at, keys) => { let f = null; for (const k of keys) { const e = E.key(text, at, k, { fresh: f }); if (typeof e === 'string') continue; const [a, b, body, step] = e; text = text.slice(0, a) + body + text.slice(b); f = E.grow(f, a, b, body.length); at = a + step; } return [text, at]; };
  assert.equal(typeKeys(t, at('}'), [...' new', 'Enter', '}', '{', '$', 'x', '$', 'Backspace'])[0].split('\n')[0], '\\section{Background new\\}\\{}\\label{sec:b}');
  const xy = 'a \\(xy\\) b', close = xy.indexOf('\\)') + 2, open = xy.indexOf('\\(');
  assert.equal(typeKeys(xy, close, ['Backspace'])[0], 'a \\(x\\) b', 'Backspace after `\\)`: the last character inside');
  assert.equal(typeKeys(xy, close, ['Backspace', 'Backspace'])[0], 'a  b', 'then the formula of one character whole');
  assert.equal(typeKeys(xy, open, ['Delete'])[0], 'a \\(y\\) b', 'Delete before `\\(`: the first character inside');
  // `\` then `[` (or `(`) outside math opens a formula with its closer, the caret inside: the keys of a display typed
  // by hand. Pins the `[` refused as a lone `\[` (the source kept `so \y=2`, the formula lost).
  const para = 'Some text so\n\nMore.', end = para.indexOf('\n');
  assert.deepEqual(typeKeys(para, end, [...' \\[y=2']), ['Some text so \\[y=2\\]\n\nMore.', end + 6], 'a display formula typed: \\[y=2\\], the caret before \\]');
  assert.equal(typeKeys(para, end, [...' \\(a'])[0], 'Some text so \\(a\\)\n\nMore.', 'inline the same');
  assert.equal(typeKeys('a \\[z\\] b', 'a \\[z\\]'.length, ['Backspace'])[0], 'a  b', 'a display `\\[z\\]` the same');
  assert.equal(typeKeys(t, at('}'), [...'$x', 'Backspace'])[0].split('\n')[0], '\\section{Background}\\label{sec:b}', 'the last character of a formula takes it');
  assert.equal(typeKeys('a pipeline. b', 'a pipeline'.length, [...'\\emph{q}'])[0], 'a pipeline\\emph{q}. b', '`\\` before `.`: its space goes once `{` ends the name');
  assert.equal(typeKeys('Some text', 4, [...'\\zz{a}'])[0], 'Some\\zz{a} text', '`\\` before a space: its own space, the text\'s kept. Pins `\\zz` refused as renaming `\\ `');
  // a contents entry is written where its short title is, else its title; a starred section, a float-less caption none
  const texts = { 'a.tex': '\\section[Short]{Long}\\input{b}\\section*{No}\\caption{Lone}', 'b.tex': '\\begin{figure}\\caption[F]{Fig}\\end{figure}\\subsection{Sub}\n\\verb|\\section{v}|' };
  const got = (k) => E.entries(texts, 'a.tex', k).map(([f, a, b]) => `${f}:${texts[f].slice(a, b)}`);
  assert.deepEqual(got('toc'), ['a.tex:Short', 'b.tex:Sub']);
  assert.deepEqual(got('lof'), ['b.tex:F']);
  assert.deepEqual(got('lot'), []);
  assert.equal(E.key(texts['a.tex'], 'section[Sh'.length + 1, 'x')[2], 'x', 'a short title takes typing');
  // typing into a contents entry whose heading has no short title writes one: the title copied into `[…]`, the
  // caret at the clicked character of the copy. Pins the heading's own title staying as it was.
  const ed = (t, e) => t.slice(0, e[0]) + e[2] + t.slice(e[1]);
  const sub = 'x\\subsection{Pictures}\\label{p}', head = sub.indexOf('\\sub'), st = E.shortTitle(sub, head, 3);
  assert.equal(ed(sub, st), 'x\\subsection[Pictures]{Pictures}\\label{p}');
  assert.equal(ed(sub, st).slice(st[0] + st[3] - 3, st[0] + st[3]), 'Pic', 'the caret after the clicked characters');
  assert.equal(E.shortTitle(ed(sub, st), head, 0), null, 'a short title written: none more');
  assert.equal(ed('\\caption{A [b] c}', E.shortTitle('\\caption{A [b] c}', 0, 0)), '\\caption[{A [b] c}]{A [b] c}', 'a `]` in the title: the copy in braces');
  assert.equal(ed('\\section[Short]{Long}', E.bothTitles('\\section[Short]{Long}', 0)), '\\section{Short}', 'the heading takes the short title');
  assert.equal(ed('\\caption[{A [b]}] {L}', E.bothTitles('\\caption[{A [b]}] {L}', 0)), '\\caption{A [b]}', "the copy's braces go");
  // generated text renamed: a line before \begin{document}, or the line written before; under babel in \captions<lang>
  const pre = '\\documentclass{article}\n\\begin{document}\nx\n\\end{document}';
  const r1 = E.renamed(pre, '\\contentsname', 'Inhalt');
  assert.equal(ed(pre, r1).split('\n')[1], '\\renewcommand{\\contentsname}{Inhalt}');
  assert.equal(ed(pre, r1).slice(r1[3], r1[3] + 6), 'Inhalt', 'the value where the caret goes');
  const again = ed(pre, r1), r2 = E.renamed(again, '\\contentsname', 'Index');
  assert.equal(ed(again, r2).split('\n')[1], '\\renewcommand{\\contentsname}{Index}', 'the line written before takes the new value');
  assert.equal(ed('\\renewcommand\\listfigurename{F}\n' + pre, E.renamed('\\renewcommand\\listfigurename{F}\n' + pre, '\\listfigurename', 'G')).split('\n')[0], '\\renewcommand\\listfigurename{G}', 'the brace-less form too');
  assert.equal(ed('% \\renewcommand{\\contentsname}{C}\n' + pre, E.renamed('% \\renewcommand{\\contentsname}{C}\n' + pre, '\\contentsname', 'D')).split('\n')[2], '\\renewcommand{\\contentsname}{D}', 'a commented line is none');
  const bab = '\\documentclass{article}\n\\usepackage[ngerman]{babel}\n\\renewcommand{\\figurename}{Bild}\n\\begin{document}\n\\end{document}', r3 = E.renamed(bab, '\\figurename', 'Abb.', 'ngerman');
  assert.equal(ed(bab, r3).split('\n')[3], '\\addto\\captionsngerman{\\renewcommand{\\figurename}{Abb.}}', 'babel: in \\captions<lang>, the plain line is not the one in force');
  assert.equal(ed(ed(bab, r3), E.renamed(ed(bab, r3), '\\figurename', 'Abbildung', 'ngerman')).split('\n')[3], '\\addto\\captionsngerman{\\renewcommand{\\figurename}{Abbildung}}', 'babel: the captions line rewritten');
  assert.equal(E.renamed('no document', '\\contentsname', 'x'), null);
  // dot leaders: the kernel's \@dotsep in the list's own file, tocloft's per entry kind; one line a setting
  const l1 = E.leaders(pre, 'lof', 'none', false);
  assert.equal(ed(pre, l1).split('\n')[2], '\\makeatletter\\addtocontents{lof}{\\protect\\renewcommand\\protect\\@dotsep{10000}}\\makeatother', 'after \\begin{document}: \\addtocontents writes the .aux');
  assert.equal(ed(ed(pre, l1), E.leaders(ed(pre, l1), 'lof', '8', false)).split('\n').filter((l) => /dotsep/.test(l)).join('|'), '\\makeatletter\\addtocontents{lof}{\\protect\\renewcommand\\protect\\@dotsep{8}}\\makeatother', 'the spacing rewrites the line');
  assert.equal(ed(pre, E.leaders(pre, 'toc', 'none', true)).split('\n')[1], '\\renewcommand{\\cftsecdotsep}{\\cftnodots}\\renewcommand{\\cftsubsecdotsep}{\\cftnodots}\\renewcommand{\\cftsubsubsecdotsep}{\\cftnodots}');
  assert.equal(ed(pre, E.leaders(pre, 'lot', 'nopage', false)).split('\n')[1], '\\cftpagenumbersoff{tab}', 'page numbers: tocloft, in the preamble');
  assert.equal(E.leaders(pre, 'lof', 'wide', false), null);
});
