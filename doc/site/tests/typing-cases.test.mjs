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
  // (Enter at the heading's end starts a paragraph after it; the keys after go there)
  const keyed = typeKeys(t, at('}'), [...' new', 'Enter', '}', '{', '$', 'x', '$', 'Backspace'])[0].split('\n');
  assert.deepEqual([keyed[0], keyed[1], keyed[2]], ['\\section{Background new}', '', '\\}\\{\\label{sec:b}']);
  const xy = 'a \\(xy\\) b', close = xy.indexOf('\\)') + 2, open = xy.indexOf('\\(');
  assert.equal(typeKeys(xy, close, ['Backspace'])[0], 'a \\(x\\) b', 'Backspace after `\\)`: the last character inside');
  assert.equal(typeKeys(xy, close, ['Backspace', 'Backspace'])[0], 'a  b', 'then the formula of one character whole');
  assert.equal(typeKeys(xy, open, ['Delete'])[0], 'a \\(y\\) b', 'Delete before `\\(`: the first character inside');
  // `\` then `[` (or `(`) outside math opens a formula with its closer, the caret inside: the keys of a display typed
  // by hand. Pins the `[` refused as a lone `\[` (the source kept `so \y=2`, the formula lost).
  const para = 'Some text so\n\nMore.', end = para.indexOf('\n');
  assert.deepEqual(typeKeys(para, end, [...' \\[y=2']), ['Some text so \\[y=2\\]\n\nMore.', end + 6], 'a display formula typed: \\[y=2\\], the caret before \\]');
  assert.equal(typeKeys(para, end, [...' \\(a'])[0], 'Some text so \\(a\\)\n\nMore.', 'inline the same');
  // an environment typed by hand: its name goes into a fresh \\begin{…}, Enter writes the \\end{name} with it and puts
  // the caret on the line between. Pins the `{` refused (`\\beginitemize\\}`) and a lone \\begin left behind.
  const envd = typeKeys('Some text.\n\nMore.', 10, ['Enter', ...'\\begin{itemize}', 'Enter']);
  assert.deepEqual(envd, ['Some text.\n\n\\begin{itemize}\n\n\\end{itemize}\n\nMore.', 28], 'the pair, the caret inside');
  assert.equal(typeof E.key('a \\begin{x} b \\end{x}', 9, 'y'), 'string', 'a name not being typed stays a name');
  // a reference typed by hand: its key goes into the fresh \\ref{…} (and a citation's list into \\cite{…}); a key
  // not typed since the caret was put stays a key. Pins the key refused (`\\ref{}`, preview wish 4, 2026-11-21).
  assert.equal(typeKeys('See it.', 3, [...' \\ref{sec:a}'])[0], 'See \\ref{sec:a} it.', 'a fresh key takes its name');
  assert.equal(typeKeys('See it.', 3, [...' \\cite{a, b}'])[0], 'See \\cite{a, b} it.', 'a citation list');
  assert.equal(typeof E.key('a \\ref{x} b', 8, 'y'), 'string', 'a key not being typed stays a key');
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

// Intuition: a footer cell typed in the PDF is the argument of the last call setting it on odd pages; Backspace next to
// the page number takes its command whole, never one letter of `\thepage`. Pins typing after the number renaming it.
test('footer cells: the call typed into, the page number taken whole', () => {
  const E = self.satexPdfEdit;
  const t = '\\usepackage{fancyhdr}\n\\fancyhf{}\n\\fancyfoot[LE,RO]{x}\n\\fancyfoot[C]{\\thepage}\n%\\cfoot{no}\n\\rfoot{r}\n';
  const arg = (pkg, l) => { const a = E.footArg(t, pkg, l); return a && t.slice(...a); };
  assert.deepEqual(['L', 'C', 'R'].map((l) => arg('fancyhdr', l)), [null, '\\thepage', 'r'], 'LE is even pages; a comment sets nothing; the last call wins');
  const k = '\\cfoot[\\pagemark]{Page \\pagemark}\n\\ofoot*{o}\n';
  assert.deepEqual(['i', 'c', 'o'].map((l) => { const a = E.footArg(k, 'scrlayer-scrpage', l); return a && k.slice(...a); }), [null, 'Page \\pagemark', 'o']);
  const back = (s, at, b) => { const r = E.footBack(s, at, b); return r && s.slice(0, r[0]) + s.slice(r[1]); };
  assert.equal(back('{a \\thepage}', 11, true), '{a }');
  assert.equal(back('{\\thepage{} of}', 11, true), '{ of}');
  assert.equal(back('{\\thepage}', 1, false), '{}');
  assert.equal(back('{\\thepage x}', 1, false), '{x}');
  assert.equal(back('{ab}', 3, true), null, 'plain text: the usual Backspace');
  assert.equal(E.footShown('Page~\\thepage{} of \\textbf{2}', '7'), 'Page 7 of 2');
  assert.equal(E.footShown('\\thepage\\ x', '3'), '3 x');
});

// Intuition: a click on empty page space is a place in the source: past a line's end that paragraph's end, between
// paragraphs a new one after the one above (a heading's line, a float's \end), between two lines of one paragraph its
// nearest line. Pins Type mode answering "No text here" for every click off the words.
test('blankPlace: empty page space mapped to the source', () => {
  const E = self.satexPdfEdit;
  const t = ['\\begin{document}', '\\section{Intro}', 'First para line one', 'line two.', '', 'Second para.', '\\begin{figure}', '\\caption{Cap}', '\\end{figure}', 'Third.', '\\end{document}'].join('\n');
  const row = (line, base, l = 72, r = 500) => ({ t: base - 8, d: base + 2, base, l, r, line });
  const rows = [row(2, 100, 72, 200), row(3, 130, 87), row(4, 142, 72, 300), row(6, 160, 87, 250), row(8, 200, 200, 300), row(10, 230, 87, 120)];
  const off = (s) => t.indexOf(s);
  assert.deepEqual(E.blankPlace(t, rows, 400, 141), { kind: 'end', row: 2 }, 'right of a line: its end');
  assert.deepEqual(E.blankPlace(t, rows, 50, 141), { kind: 'start', row: 2 });
  assert.deepEqual(E.blankPlace(t, rows, 100, 136), { kind: 'near', row: 2 }, 'between lines of one paragraph: the nearer');
  const a = E.blankPlace(t, rows, 100, 115);
  assert.deepEqual([a.kind, a.at, a.caret], ['new', off('\nFirst'), off('\nFirst') + 2], 'under a heading: right after its line');
  assert.equal(a.ghost.x, 72, 'no indent after a heading');
  assert.equal(t.slice(0, a.at) + a.body.slice(0, 2) + 'Lead.' + a.body.slice(2) + t.slice(a.at), t.replace('Intro}\nFirst', 'Intro}\n\nLead.\n\nFirst'), 'a blank line before the next text too');
  const b = E.blankPlace(t, rows, 100, 148);
  assert.deepEqual([b.kind, b.at, b.ghost.x], ['new', off('.\n\nSecond') + 1, 87], 'between paragraphs: after the one above, indented');
  assert.equal(E.blankPlace(t, rows, 100, 215).at, off('\\end{figure}') + '\\end{figure}'.length, "after a float: past its \\end");
  assert.equal(E.blankPlace(t, rows, 100, 300).at, off('Third.') + 6, 'below the last line: after its paragraph');
  const c = E.blankPlace(t, rows.slice(1), 100, 60);
  assert.deepEqual([c.kind, c.at, c.caret], ['new', off('First'), off('First')], 'above an indented first line: before it');
  assert.deepEqual(E.blankPlace(t, rows.slice(2), 100, 60), { kind: 'near', row: 0 }, 'above a continued paragraph: its line');
  // SyncTeX's line of a paragraph is often the blank line ending it, or \end{document}: the paragraph before
  assert.equal(E.blankPlace(t, rows.map((r) => r.line === 6 ? { ...r, line: 5 } : r.line === 10 ? { ...r, line: 11 } : r), 100, 300).at, off('Third.') + 6);
  assert.equal(E.blankPlace(t, [row(5, 130, 87), row(6, 160, 87)], 100, 150).at, off('line two.') + 9, 'the blank line after the first paragraph names it');
  // written where shown: the text with the body in place keeps the paragraphs apart by blank lines
  const w = t.slice(0, b.at) + b.body + 'New' + t.slice(b.at);
  assert.ok(w.includes('line two.\n\nNew\n\nSecond'));
});
// Intuition: Enter in the page is a new paragraph as TeX sets one (a blank line in the source). At the end of a command's
// argument (`\textbf{world|}`) the paragraph starts after the command, never inside its argument (refused there: the
// key did nothing and the next text went into the bold); on an item's last line of a list the next `\item`; in math a
// line of source only. Pins user report 2026-11-21 13:15 (Enter "does a newline in the code editor").
test('Enter: a paragraph where TeX takes one', () => {
  const E = self.satexPdfEdit, ed = (t, e) => t.slice(0, e[0]) + e[2] + t.slice(e[1]);
  const bold = 'a \\textbf{world}\n';
  const e1 = E.key(bold, bold.indexOf('}'), 'Enter');
  assert.equal(ed(bold, e1), 'a \\textbf{world}\n\n\n', 'after the command');
  assert.equal(e1[0] + e1[3], bold.indexOf('}') + 3, 'the caret on the new paragraph');
  const list = '\\begin{itemize}\n\\item one\n\\end{itemize}', at = list.indexOf('one') + 3;
  assert.equal(ed(list, E.key(list, at, 'Enter')), '\\begin{itemize}\n\\item one\n\\item \n\\end{itemize}', 'the next item');
  const mid = 'Hello there friend.';
  assert.equal(ed(mid, E.key(mid, 5, 'Enter')), 'Hello\n\n there friend.', 'prose: a blank line');
  assert.equal(typeof E.key('a \\textbf{wor ld}', 9, 'Enter'), 'string', 'inside an argument: refused');
});

// Intuition: a footnote is set at the page's foot but written inside its body paragraph. A click below the footnote's
// text goes to the end of the note (its last row's end), never to a new body paragraph after the `\footnote{…}`.
// A click below the body text, above the note, still starts a new paragraph after the body's.
// Pins "Generally" typed below "Foobar chicken!!" landing as a new body paragraph (user 2026-11-22 18:54).
test('blankPlace: below a footnote, the note\'s end', () => {
  const E = self.satexPdfEdit;
  const t = ['\\begin{document}', 'There is merit to this, you know?!', '\\footnote{Foobar chicken!!}', '', '\\end{document}'].join('\n');
  const word = (s, l, base) => [1, l, base - 8, l + 30, base, s];
  const body = { t: 92, d: 102, base: 100, l: 72, r: 300, line: 3, words: [word('There', 72, 100), word('know?!1', 250, 100)] };
  const note = { t: 692, d: 700, base: 698, l: 80, r: 200, line: 3, words: [word('1Foobar', 80, 698), word('chicken!!', 130, 698)] };
  assert.deepEqual(E.blankPlace(t, [body, note], 150, 720), { kind: 'end', row: 1 }, 'below the note: its end');
  const p = E.blankPlace(t, [body, note], 150, 300);
  assert.equal(p.kind, 'new', 'below the body text, above the note: a new paragraph');
  assert.equal(E.blankPlace(t, [body, { ...note, words: [word('Other', 80, 698)] }], 150, 720).kind, 'new', 'a row of other text: no note');
  const two = { ...note, t: 704, d: 712, base: 710, words: [word('more', 80, 710)] }, u = t.replace('chicken!!', 'chicken!! and more');
  assert.equal(E.blankPlace(u, [body, { ...note, words: [...note.words, word('and', 170, 698)] }, two], 150, 703).kind, 'end', 'between two rows of a note: the upper row\'s end');
});

// Intuition: a footnote is set apart from the paragraph it is written in, so the Type caret's paragraph inside one is
// the note's text, and its page rows are those that spell that text (the mark aside). Pins the echo of text typed in
// a footnote drawn on the body row, then the caret jumping to the foot when the build landed (e2e footnote).
test('notes: the note around the caret and its rows', () => {
  const E = self.satexPdfEdit, t = 'Body here.\\footnote{A footnote text goes here.} More.';
  const arg = [t.indexOf('A footnote'), t.indexOf('.}') + 1];
  assert.deepEqual(E.noteAt(t, t.indexOf('text')), arg, 'inside: its text span');
  assert.equal(E.noteAt(t, t.indexOf('More')), null, 'outside: none');
  assert.deepEqual(E.notes(t, 0, t.length), [arg]);
  assert.equal(E.noteRow(t.slice(...arg), ['1A', 'footnote', 'text']), true, 'its row, the mark aside');
  assert.equal(E.noteRow(t.slice(...arg), ['Body', 'here.1']), false, 'the body row');
});
