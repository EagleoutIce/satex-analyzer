// Typing into the page footer (a footer package set up on the first key) and clicks on empty page space (a new
// paragraph there). kinds/foot.mjs.
const PLAIN = '\\documentclass{article}\n\\begin{document}\n\\section{Intro}\nFirst paragraph has some words in it.\n\nSecond paragraph follows here.\n\\end{document}\n';
const KOMA = PLAIN.replace('{article}', '{scrartcl}');
const FANCY = PLAIN.replace('\\begin{document}', '\\usepackage{fancyhdr}\n\\pagestyle{fancy}\n\\fancyhf{}\n\\fancyfoot[C]{\\thepage}\n\\begin{document}');
const TITLE = '\\documentclass{article}\n\\title{T}\n\\author{A}\n\\begin{document}\n\\maketitle\nBody text.\n\\newpage\nMore text.\n\\end{document}\n';

// `click`: where, as a function of the page's words (bp) → [page, x, y]; `keys`; `want` regexes on the source;
// `not`: regexes the source must not match; `pdf`: regexes on the built page's words joined; `note`: on the note.
const num = `(ws) => { const w = ws.filter((w) => /^\\d+$/.test(w[5])).at(-1); return w; }`;
export default [
  { kind: 'foot', name: 'plain-before-number', source: PLAIN, click: `(ws) => { const w = (${num})(ws); return [w[0], w[1] + 0.3, (w[2] + w[4]) / 2]; }`, keys: ['Page '],
    want: [/\\usepackage\{fancyhdr\}/, /\\pagestyle\{fancy\}/, /\\fancyhf\{\}/, /\\fancyfoot\[C\]\{Page \\thepage\}/, /headrulewidth\}\{0pt\}/], pdf: [/Page 1$/], note: /footer[^]*fancyhdr/i, echo: /Page 1/ },
  { kind: 'foot', name: 'plain-right-cell', source: PLAIN, click: `(ws) => { const w = (${num})(ws); return [w[0], 480, (w[2] + w[4]) / 2]; }`, keys: ['Draft'],
    want: [/\\fancyfoot\[C\]\{\\thepage\}/, /\\fancyfoot\[R\]\{Draft\}/], pdf: [/1 Draft$/] },
  { kind: 'foot', name: 'plain-backspace-number', source: PLAIN, click: `(ws) => { const w = (${num})(ws); return [w[0], w[3] - 0.2, (w[2] + w[4]) / 2]; }`, keys: ['Backspace', 'X'],
    want: [/\\fancyfoot\[C\]\{X\}/], not: [/\\thepage/], pdf: [/X$/] },
  { kind: 'foot', name: 'koma', source: KOMA, click: `(ws) => { const w = (${num})(ws); return [w[0], w[1] + 0.3, (w[2] + w[4]) / 2]; }`, keys: ['S. '],
    want: [/\\usepackage\{scrlayer-scrpage\}/, /\\clearpairofpagestyles/, /\\cfoot\[\\pagemark\]\{S\. \\pagemark\}/], not: [/fancyhdr/], pdf: [/S\. 1$/], note: /scrlayer-scrpage/ },
  { kind: 'foot', name: 'fancy-existing', source: FANCY, click: `(ws) => { const w = (${num})(ws); return [w[0], w[3] - 0.2, (w[2] + w[4]) / 2]; }`, keys: [' end'],
    want: [/\\fancyfoot\[C\]\{\\thepage\{\} end\}/], not: [/usepackage\{fancyhdr\}[\s\S]*usepackage\{fancyhdr\}/], pdf: [/1 end$/] },
  { kind: 'foot', name: 'title-page-kept', source: TITLE, page: 2, click: `(ws) => { const w = ws.filter((w) => w[0] === 2 && /^\\d+$/.test(w[5])).at(-1); return [w[0], w[1] + 0.3, (w[2] + w[4]) / 2]; }`, keys: ['p. '],
    want: [/\\fancyfoot\[C\]\{p\. \\thepage\}/, /\\maketitle/], pdf: [/^1$/m] },
  // empty page space
  { kind: 'foot', name: 'blank-between', source: PLAIN.replace('\\begin{document}', '\\setlength{\\parskip}{12pt}\n\\begin{document}'), click: `(ws) => { const a = ws.find((w) => w[5] === 'First'), b = ws.find((w) => w[5] === 'Second'); return [a[0], 200, (a[4] + b[2]) / 2]; }`, keys: ['New one.'],
    want: [/in it\.\n\nNew one\.\n\nSecond/], ghost: true },
  { kind: 'foot', name: 'blank-below', source: PLAIN, click: `(ws) => { const b = ws.find((w) => w[5] === 'Second'); return [b[0], 200, b[4] + 40]; }`, keys: ['Last.'],
    want: [/follows here\.\n\nLast\.\n\\end\{document\}/] },
  { kind: 'foot', name: 'blank-line-end', source: PLAIN, click: `(ws) => { const b = ws.find((w) => w[5] === 'here.'); return [b[0], b[3] + 60, (b[2] + b[4]) / 2]; }`, keys: [' Z'],
    want: [/follows here\. Z\n/] },
  { kind: 'foot', name: 'blank-under-heading', source: PLAIN, click: `(ws) => { const h = ws.find((w) => w[5] === 'Intro'), a = ws.find((w) => w[5] === 'First'); return [h[0], 200, (h[4] + a[2]) / 2]; }`, keys: ['Lead.'],
    want: [/\\section\{Intro\}\n\nLead\.\n\nFirst/] },
];
