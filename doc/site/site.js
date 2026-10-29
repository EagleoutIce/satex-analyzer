// Every page: the light/dark toggle, the running table of contents, sortable
// and filterable long tables, the explain cards, and the landing page's examples.
(() => {
  const root = document.documentElement;
  const dark = () => (root.dataset.theme ?? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')) === 'dark';
  self.siteDark = dark; // playground.js themes its editor by it
  const button = document.getElementById('theme');
  const label = () => button.setAttribute('aria-pressed', dark());
  button.onclick = () => {
    root.dataset.theme = dark() ? 'light' : 'dark';
    try { localStorage.setItem('satex-theme', root.dataset.theme); } catch {}
    label();
  };
  label();

  // Tabs: a tablist's buttons show the panel each controls.
  for (const list of document.querySelectorAll('.examples > .tabs')) {
    list.onclick = (e) => {
      const tab = e.target.closest('[role=tab]');
      if (!tab) return;
      for (const t of list.children) {
        t.setAttribute('aria-selected', t === tab);
        document.getElementById(t.getAttribute('aria-controls')).hidden = t !== tab;
      }
    };
  }
  // The TeX tokenizer of the playground's editor, CodeMirror's stex mode. The landing
  // page's examples come colored from build.py.
  self.siteTex = () => (self.siteTex.loaded ??= (async () => {
    const [{ StreamLanguage }, { stex }, hl] = await Promise.all(['@codemirror/language@6', '@codemirror/legacy-modes@6/mode/stex',
      '@lezer/highlight@1'].map((m) => import('https://esm.sh/' + m)));
    const t = hl.tags;
    const highlighter = hl.tagHighlighter([[t.tagName, 'cmd'], [t.bracket, 'br'], [t.keyword, 'kw'], [t.comment, 'com'], [t.atom, 'atom'],
      [t.number, 'num'], [t.string, 'str'], [t.special(t.variableName), 'arg'], [t.variableName, 'arg']].map(([tag, c]) => ({ tag, class: `tok-${c}` })));
    // BibTeX: @type, key, field names, braces and values, in the TeX mode's classes
    const bib = StreamLanguage.define({
      startState: () => ({ depth: 0, key: false }),
      token(s, st) {
        if (st.depth === 0) {
          if (s.match(/@[A-Za-z]+/)) { st.key = true; return 'tag'; }
          if (st.key && s.match(/[{(]/)) { st.depth = 1; return 'bracket'; }
          if (s.eatSpace()) return null;
          s.skipTo('@') || s.skipToEnd(); return 'comment';
        }
        if (st.depth > 1) { // inside a braced value
          if (s.match(/[{]/)) { st.depth++; return 'bracket'; }
          if (s.match(/[}]/)) { st.depth--; return 'bracket'; }
          s.match(/[^{}]+/); return 'string';
        }
        if (s.eatSpace()) return null;
        if (st.key) { st.key = false; if (s.match(/[^,\s{}]+/)) return 'atom'; }
        if (s.match(/[{]/)) { st.depth = 2; return 'bracket'; }
        if (s.match(/[})]/)) { st.depth = 0; st.key = false; return 'bracket'; }
        if (s.match(/"[^"]*"?/)) return 'string';
        if (s.match(/\d+/)) return 'number';
        if (s.match(/[A-Za-z_-][\w:.-]*(?=\s*=)/)) return 'keyword';
        if (s.match(/[=#,]/)) return 'punctuation';
        s.next(); return 'variableName';
      },
    });
    return { language: StreamLanguage.define(stex), bib, highlighter };
  })());

  // ------------------------------------------------------------ explain cards
  // One renderer for `satex explain --format json`: the landing page's cards, the
  // playground's Explain panel, its run output and its hover cards.
  const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  /** TeX source colored like the editor: commands, braces, parameters, comments. */
  const tex = (text, link = false) => esc(text).replace(/(%[^\n]*)|(\\(?:[A-Za-z@_:]+|.))|([{}[\]])|(#+\d)/g, (m, com, cmd, br) =>
    `<span class="tok-${com ? 'com' : cmd ? 'cmd' : br ? 'br' : 'arg'}">${link && cmd && /^\\[A-Za-z@_:]+$/.test(m) ? `<a class="cs bl" href="#" data-cs="${m}" title="explain ${m}">${m}</a>` : m}</span>`);
  /** `code`, links and bold of satex's Markdown; a `file:line` is a place. */
  const inline = (t) => esc(t).split(/(`[^`]+`)/).map((p, i) => {
    if (!(i % 2)) return p.replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>').replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>');
    const code = p.slice(1, -1), at = code.match(/^([\w./-]+\.\w+):(\d+)$/);
    return at ? placeLink(at[1], +at[2]) : `<code>${code}</code>`;
  }).join('');

  // xparse's argument types (xparse manual, "Argument specifications"): a color
  // class, the shape a use writes, how the argument is read.
  const ARG = {
    m: ['mand', '{…}', 'mandatory argument: one token or a {…} group'],
    r: ['mand', (a) => `${a[0]}…${a[1]}`, (a) => `required argument between ${a[0]} and ${a[1]}`],
    R: ['mand', (a) => `${a[0]}…${a[1]}`, (a) => `required argument between ${a[0]} and ${a[1]}, ${a[2]} (with an error) when missing`],
    v: ['mand', '|…|', 'verbatim argument, between two equal characters or in braces'],
    b: ['mand', 'body', 'the environment\'s body, up to its \\end'],
    l: ['mand', '…{', 'everything up to the next {'],
    u: ['mand', (a) => `…${a[0]}`, (a) => `everything up to ${a[0]}`],
    o: ['opt', '[…]', 'optional argument in […]'],
    O: ['opt', (a) => `[${a[0]}]`, (a) => `optional argument in […], ${a[0] === '' ? 'empty' : a[0]} when left out`],
    d: ['opt', (a) => `${a[0]}…${a[1]}`, (a) => `optional argument between ${a[0]} and ${a[1]}`],
    D: ['opt', (a) => `${a[0]}${a[2]}${a[1]}`, (a) => `optional argument between ${a[0]} and ${a[1]}, ${a[2]} when left out`],
    g: ['opt', '{…}?', 'optional argument in {…}'],
    G: ['opt', (a) => `{${a[0]}}?`, (a) => `optional argument in {…}, ${a[0]} when left out`],
    s: ['flag', '*', 'optional star: true when a * follows'],
    t: ['flag', (a) => a[0], (a) => `optional ${a[0]}: true when it follows`],
    e: ['emb', (a) => a[0], (a) => `embellishments: each of ${a[0].split('').join(' ')} may follow, with an argument`],
    E: ['emb', (a) => a[0], (a) => `embellishments ${a[0].split('').join(' ')}, with defaults ${a[1]}`],
  };
  const PARAMS = { r: 2, R: 3, d: 2, D: 3, O: 1, G: 1, t: 1, e: 1, E: 2, u: 1 };
  // The placeholders of a primitive's syntax (The TeXbook, chapter 24).
  const SLOT = {
    'control sequence': 'a control sequence like \\foo, or an active character', cs: 'a control sequence like \\foo, or an active character',
    'parameter text': 'what a use looks like: #1#2… with any delimiters between them',
    'replacement text': 'the body: what a use expands to, its #1… replaced',
    'balanced text': 'tokens with balanced braces', 'general text': 'a {…} group',
    number: 'an integer: digits, a count register, \\numexpr …', dimen: 'a length: 3pt, \\parindent, \\dimexpr …', glue: 'a length that may stretch: 3pt plus 1fil',
    muglue: 'math glue: 3mu plus 1fill', 'file name': 'a file name, up to a space', equals: 'an optional =', 'optional spaces': 'spaces, skipped',
    filler: 'spaces and \\relax, skipped', token: 'one token', box: 'a box: \\hbox{…}, \\vbox{…}, \\box0 …', rule: 'a rule: \\hrule or \\vrule with its sizes',
    'math field': 'a math character or a {…} group', 'math character': 'a character in math', delim: 'a delimiter: (, \\langle …', font: 'a font identifier like \\tenrm',
  };
  const slot = (name, kind, ty) => {
    const tip = SLOT[name.toLowerCase()] ?? (/^arg\d+$/.test(name) ? `the ${name.slice(3)}. argument${kind ? `, ${KIND[kind]}` : ''}${ty ? `, read as ${ty}` : ''}` : `${name}, read by the command`);
    // the brackets as entities: a chip is never wrapped again
    const named = TY.has(name) ? tyLink(name) : esc(name);
    return `<span class="arg arg-${kind ?? 'slot'}" title="${esc(tip)}">&#x27E8;${named}${ty ? `<small>: ${tyLink(ty)}</small>` : ''}&#x27E9;</span>`;
  };
  /** The placeholders of primitive syntax that are types of the Ty lattice. */
  const TY = new Set(['number', 'dimen', 'glue', 'muglue', 'control sequence']);
  /** A type's name linking to the wiki's Ty lattice. */
  const tyLink = (t) => `<a class="ty" target="_blank" rel="noopener" href="${document.querySelector('a.brand')?.getAttribute('href')?.replace(/index\.html$/, '') ?? ''}wiki/domains.html#argument-types-the-ty-lattice" title="the type ${esc(t)} in the Ty lattice">${esc(t)}</a>`;
  /** explain's `types` (`#1 skip register`) by argument name: `{ arg1: 'skip register' }`. */
  const typesOf = (r) => Object.fromEntries((r.types ?? []).map((t) => t.match(/^#(\d+) (.+)$/)).filter(Boolean).map(([, n, t]) => [`arg${n}`, t]));
  const KIND = { mand: 'mandatory', opt: 'optional', emb: 'an embellishment' };
  /** The ⟨…⟩ placeholders of a syntax or usage line as chips that explain themselves;
   * `kinds` colors ⟨argN⟩ as `usage` found it. */
  const slots = (html, kinds = {}) => html.replace(/⟨([^⟩<]+)⟩/g, (m, n) => slot(n, kinds[n]));
  /** A usage line, as the hover writes it (`\cs*{⟨arg1⟩}[⟨arg2⟩]`) or as explain's
   * `effective` (`\cs{1}[2](3)?`), TeX-colored: each argument a ⟨argN⟩ chip in its
   * kind's color, read off its delimiters: […] or a trailing ? optional, c{…}? an
   * embellishment, else mandatory; a star after the name as the star's chip. Returns the HTML and the kinds by name. */
  function usage(text, types = {}) {
    const kinds = {};
    types = { ...types };
    let n = 0;
    const marked = String(text ?? '').replace(/([{[(<]?)(⟨arg(\d+)⟩|\d)(?:: ([^}\])>⟩]+))?([}\])>]?)(\??)/g, (m, open, arg, k, ty, close, opt, at, all) => {
      if (!k && (!open || !close || +arg !== n + 1)) return m;
      n = k ? +k : n + 1;
      if (ty) types[`arg${n}`] = ty;
      kinds[`arg${n}`] = open === '[' ? 'opt' : !opt ? 'mand' : open === '{' && /[^A-Za-z0-9\s\]})>]/.test(all[at - 1] ?? '') ? 'emb' : 'opt';
      return `${open}\uE000${n}\uE001${close}${opt}`;
    }).replace(/^(\\[A-Za-z@]+)\*\??/, '$1\uE002'); // a star after the name: the s argument
    const html = slots(tex(marked), kinds).replace(/\uE000(\d+)\uE001/g, (m, k) => slot(`arg${k}`, kinds[`arg${k}`], types[`arg${k}`]))
      .replace('\uE002', '<span class="arg arg-flag" title="the star: the starred form">*</span>');
    return { html, kinds };
  }
  /** An xparse-style signature (`smO{0}om`, `⟨cs⟩soo…`) as one chip per argument. */
  function signature(spec) {
    const out = [];
    let i = 0, prefix = '';
    const group = () => {
      if (spec[i] === '{') {
        let depth = 0, j = i;
        do { depth += spec[j] === '{' ? 1 : spec[j] === '}' ? -1 : 0; j++; } while (j < spec.length && depth);
        const g = spec.slice(i + 1, j - 1);
        i = j;
        return g;
      }
      if (spec[i] === '\\') { const m = spec.slice(i).match(/^\\(?:[A-Za-z@]+|.)/); i += m[0].length; return m[0]; }
      return spec[i++] ?? '';
    };
    while (i < spec.length) {
      const c = spec[i];
      if (/\s/.test(c)) { i++; continue; }
      if (c === '⟨') { const j = spec.indexOf('⟩', i); out.push({ key: spec.slice(i, j + 1), html: slot(spec.slice(i + 1, j)) }); i = j + 1; continue; }
      if (c === '…') { out.push({ html: '<span class="arg arg-more" title="satex saw no more: the command may read further arguments">…</span>' }); i++; continue; }
      if (c === '+' || c === '!') { prefix += c; i++; continue; }
      if (c === '>') { i++; group(); prefix += '>'; continue; }
      i++;
      const a = Array.from({ length: PARAMS[c] ?? 0 }, group);
      const key = `${prefix}${c}${a.map((x) => `{${x}}`).join('')}`;
      if (!ARG[c]) { out.push({ key, html: `<span class="arg arg-slot" title="argument type ${esc(c)}"><b>${esc(c)}</b></span>` }); prefix = ''; continue; }
      const [cls, shape, tip] = ARG[c].map((x) => typeof x === 'function' ? x(a) : x);
      const extra = (prefix.includes('+') ? ', may hold paragraphs (+)' : '') + (prefix.includes('!') ? ', no spaces before it (!)' : '') + (prefix.includes('>') ? ', processed first (>)' : '');
      const label = `${prefix.replace('>', '')}${c}${'ORDGE'.includes(c) ? `{${a.at(-1)}}` : ''}`;
      out.push({ key, html: `<span class="arg arg-${cls}" title="${esc(c)}: ${esc(tip + extra)}"><b>${esc(label)}</b><small>${esc(shape)}</small></span>` });
      prefix = '';
    }
    // a run of equal arguments, as \def's nine optional ones, is one chip with a count
    const runs = [];
    for (const x of out) { const last = runs.at(-1); if (last && x.key && last.key === x.key) last.n++; else runs.push({ ...x, n: 1 }); }
    return `<span class="sig">${runs.map((r) => r.n > 1 ? r.html.replace(/<\/span>$/, `<small class="times">×${r.n}</small></span>`) : r.html).join('')}</span>`;
  }
  const argumentCount = (t) => {
    const n = (k) => `${k} argument${k === 1 ? '' : 's'}`;
    return t.max == null ? `at least ${n(t.min ?? 0)}` : t.min === t.max ? (t.min ? n(t.min) : 'no arguments') : `${t.min}–${t.max} arguments`;
  };
  /** A distribution file's upstream sources: the kernel and the team's required packages in
   * latex2e, expl3 in latex3, other packages on CTAN. No line: a generated file's lines are
   * not its .dtx source's. */
  function upstream(path) {
    const m = String(path ?? '').match(/(?:^|\/)tex\/latex\/([^/]+)\/[^/]+$/);
    if (!m) return null;
    const dir = m[1];
    if (dir === 'base') return ['https://github.com/latex3/latex2e/tree/HEAD/base', 'latex2e'];
    if (['tools', 'amsmath', 'graphics', 'cyrillic', 'firstaid', 'latex-lab'].includes(dir)) return [`https://github.com/latex3/latex2e/tree/HEAD/required/${dir}`, 'latex2e'];
    if (/^l3/.test(dir)) return [`https://github.com/latex3/latex3/tree/HEAD/${dir}`, 'latex3'];
    return [`https://ctan.org/pkg/${dir}`, 'CTAN'];
  }
  /** `file:line` as a link: the playground opens it (a distribution file read-only),
   * other pages lead to the playground; plus the file's upstream sources. */
  function placeLink(path, line, file = String(path).split('/').pop()) {
    const play = document.body.classList.contains('play');
    const home = document.querySelector('a.brand')?.getAttribute('href')?.replace(/index\.html$/, '') ?? '';
    const href = play ? '#' : `${home}play/index.html#open=${encodeURIComponent(path)}:${line}`;
    const up = upstream(path);
    return `<a class="place" href="${esc(href)}" data-path="${esc(path)}" data-line="${line}" title="open ${esc(path)} at line ${line}">${esc(file)}:${line}</a>` +
      (up ? ` <a class="upstream" href="${esc(up[0])}" target="_blank" rel="noopener" title="the sources of ${esc(file)} in ${esc(up[1])}">source ↗</a>` : '');
  }
  const csChip = (name) => `<a class="cs" href="#" data-cs="${esc(name)}" title="explain ${esc(name)}">${esc(name)}</a>`;
  /** The one marker of a package, class or the kernel, as the cache dialog badges them. */
  const pkgChip = (kind, name) => `<span class="cache-kind k-${kind}" title="${kind === 'kernel' ? 'defined by the kernel' : `from the ${kind} ${esc(name)}`}">${esc(name)}</span>`;
  /** The playground's button that traces a call of the command. */
  const traceButton = (r) => document.body.classList.contains('play') && /^\\/.test(r.name) ? ` <button type="button" class="small ghost x-trace" data-trace="${esc(r.name)}" title="step through how a call of ${esc(r.name)} expands">Trace</button>` : '';
  /** One explain record as a card: signature, body, by, defined at, context, uses, expands. */
  function explain(r) {
    const rows = [];
    const primitive = r.kind === 'primitive';
    // what it does and where its documentation is, in one row
    const link = r.reference ? `<a href="${esc(r.reference)}" target="_blank" rel="noopener">${esc(r.documentation ?? 'documentation')} ↗</a>` : '';
    if (r.does || link) rows.push(['docs', [r.does && esc(r.does), link].filter(Boolean).join(' · ')]);
    if (r.signature) {
      const counted = (r.signature.match(/⟨[^⟩]*⟩|[mrRvbluoOdDgGste]/g) ?? []).length;
      const more = r.takes && !r.signature.endsWith('…') && !(r.takes.min <= counted && (r.takes.max ?? Infinity) <= counted);
      rows.push(['signature', signature(r.signature) + (more ? ` <span class="muted">reads ${argumentCount(r.takes)} with what it calls</span>` : '')]);
    } else if (r.takes) rows.push(['arguments', argumentCount(r.takes)]);
    const types = typesOf(r);
    if (r.syntax) rows.push(['usage', `<code class="usage">${esc(r.syntax)}</code>`]); // a primitive's TeXbook syntax
    else if (r.effective && r.effective !== r.name) rows.push(['usage', `<code class="usage">${usage(r.effective, types).html}</code>`]);
    if (r.types?.length) rows.push(['types', `<span class="chips">${Object.entries(types).map(([n, t]) => `<span class="x-type"><code>#${n.slice(3)}</code> ${tyLink(t)}</span>`).join('')}</span>`]);
    if (r.forms?.length) rows.push(['forms', r.forms.map((f) => `<code>${esc(f)}</code>`).join(' ')]);
    if (primitive) rows.push(['body', 'none: a TeX primitive']);
    else if (r.body != null) rows.push(['body', r.body === '' ? '<span class="muted">empty</span>' : `<pre class="x-body"><code>${tex(r.body, true)}</code></pre>`]);
    if (r.by) rows.push(['by', csChip(r.by)]);
    const owner = r.class ? pkgChip('class', r.class) : r.package ? (r.package === 'kernel' ? pkgChip('kernel', 'the kernel') : pkgChip('package', r.package)) : '';
    if (r.path && r.line) rows.push(['defined at', `${placeLink(r.path, r.line, r.file)}${owner ? ` ${owner}` : ''}`]);
    else if (owner) rows.push(['from', owner]);
    if (r.context) rows.push(['context', esc(r.context)]);
    if (r.uses != null) rows.push(['uses', r.uses ? `${r.uses} in this run` : 'none in this run']);
    for (const [key, label] of [['expands', 'always expands'], ['may_expand', 'may expand']]) if (r[key]?.length) rows.push([label, `<span class="chips">${r[key].map(csChip).join('')}</span>`]);
    // what the document's calls leave defined: the public names, the internal ones folded
    const made = r.creates ?? [], inner = r.creates_internal ?? [];
    if (made.length || inner.length) rows.push(['creates', `<span class="chips">${made.map(csChip).join('')}</span>` +
      (inner.length ? `<details class="x-more"><summary>${inner.length} internal</summary><span class="chips">${inner.map(csChip).join('')}</span></details>` : '')]);
    if (r.error) rows.push(['error', `<span class="x-err">${esc(r.error)}</span>`]);
    const tags = [primitive ? 'primitive' : r.tag, r.when && `in the ${r.when}`, r.certain === false && 'uncertain'].filter(Boolean);
    return `<div class="xcard"><div class="x-head"><code class="x-name">${esc(r.name)}</code>${tags.map((t) => ` <span class="x-kind">${esc(t)}</span>`).join('')}${traceButton(r)}</div>` +
      `<dl>${rows.map(([k, v]) => `<dt>${k}</dt><dd>${v}</dd>`).join('')}</dl></div>`;
  }
  /** The same record as a hover card: its name and kind, where it is
   * defined, how it is used, the error it raises and its documentation. */
  function brief(r) {
    const primitive = r.kind === 'primitive';
    const tags = [primitive ? 'primitive' : r.tag, r.when && `in the ${r.when}`].filter(Boolean);
    const use = r.signature ? signature(r.signature) : r.effective && r.effective !== r.name ? `<code class="usage">${usage(r.effective).html}</code>`
      : r.takes ? argumentCount(r.takes) : '';
    const where = r.path && r.line ? placeLink(r.path, r.line, r.file) : '';
    const docs = r.reference ? `<a href="${esc(r.reference)}" target="_blank" rel="noopener">${esc(r.documentation ?? 'docs')} ↗</a>` : '';
    return `<div class="xcard brief"><div class="x-head"><code class="x-name">${esc(r.name)}</code>${tags.map((t) => ` <span class="x-kind">${esc(t)}</span>`).join('')}` +
      `${where ? ` <span class="x-where">${where}</span>` : ''}${traceButton(r)}</div>` +
      (r.does ? `<div class="x-does">${esc(r.does)}</div>` : '') + (use ? `<div class="x-use">${use}</div>` : '') + (r.error ? `<div class="x-err">${esc(r.error)}</div>` : '') +
      (docs ? `<div class="x-docs">${docs}</div>` : '') + '</div>';
  }
  /** Any other record value for a table cell: lists as chips, never raw JSON. */
  const value = (v) => v == null ? '' : Array.isArray(v) ? v.map((x) => typeof x === 'object' ? value(x) : /^\\/.test(String(x)) ? csChip(String(x)) : `<code>${esc(x)}</code>`).join(' ')
    : typeof v === 'object' ? ('min' in v && 'max' in v ? argumentCount(v) : Object.entries(v).map(([k, x]) => `${esc(k)} ${value(x)}`).join(', '))
      : /^\\/.test(String(v)) ? csChip(String(v)) : /^[{}#]/.test(String(v)) ? `<code>${esc(v)}</code>` : esc(v);
  self.satexCards = { explain, brief, signature, slots, usage, tex, inline, placeLink, upstream, csChip, value, esc };

  // ------------------------------------------------------------ the landing page's examples
  // A command explains itself on hover, an underline shows its finding: a floating card,
  // so nothing on the page moves. The panel beside keeps the last command explained.
  const tip = Object.assign(document.createElement('div'), { className: 'tip', hidden: true });
  let tipTimer = 0;
  const hideTip = () => { clearTimeout(tipTimer); tipTimer = setTimeout(() => { tip.hidden = true; }, 250); };
  tip.onmouseenter = () => clearTimeout(tipTimer);
  tip.onmouseleave = hideTip;
  for (const fig of document.querySelectorAll('.example')) {
    if (!tip.isConnected) document.body.append(tip);
    const data = JSON.parse(fig.querySelector('.ex-data').textContent);
    const pane = fig.querySelector('.ex-explain');
    // A command before `\begin{document}` means what it means in the preamble.
    const src = fig.querySelector('pre.src');
    const lines = [...src.querySelectorAll('.ln')];
    const begins = lines.findIndex((l) => l.textContent.includes('\\begin{document}'));
    const when = (el) => el && begins >= 0 && lines.indexOf(el.closest('.ln')) < begins ? 'preamble' : 'document';
    const record = (name, el) => {
      const all = data.cs[name] ?? [];
      return all.find((r) => r.when && r.when === when(el)) ?? all.find((r) => !r.error) ?? all[0];
    };
    const show = (name, el) => {
      const r = record(name, el);
      if (!r) return false;
      pane.querySelector('.xname').textContent = name;
      pane.querySelector('.xbody').innerHTML = explain(r);
      return true;
    };
    const lintCard = (k) => {
      const d = data.lints[k];
      const diff = d.typo ? d.edits.map((e) => ` <span class="diff"><del>${esc(e.from) || '␣'}</del>→<ins>${esc(e.to) || '∅'}</ins></span>`).join('') : '';
      return `<div class="tip-lint f-${d.severity}"><p><span class="sev sev-${d.severity}">${d.severity}</span> <code>${esc(d.code)}</code> ${inline(d.message)}</p>` +
        (d.fix ? `<p class="tip-fix"><span class="fix fix-${d.applicability}">fix</span> ${inline(d.fix)}${diff}</p>` : '') + '</div>';
    };
    const lit = (ks, on) => { for (const k of ks) for (const x of fig.querySelectorAll(`[data-lint~="${k}"]`)) x.classList.toggle('lit', on); };
    const put = (el) => {
      const b = el.getBoundingClientRect();
      tip.hidden = false;
      const w = tip.offsetWidth, h = tip.offsetHeight;
      const top = b.bottom + 6 + h > innerHeight && b.top - 6 - h > 0 ? b.top - 6 - h : b.bottom + 6;
      Object.assign(tip.style, { left: `${Math.max(8, Math.min(b.left, innerWidth - w - 16))}px`, top: `${top}px` });
    };
    src.addEventListener('mouseover', (e) => {
      const el = e.target.closest('[data-cs], [data-lint]');
      if (!el) return;
      clearTimeout(tipTimer);
      const ks = el.dataset.lint ? el.dataset.lint.split(' ') : [];
      const cs = el.dataset.cs ? record(el.dataset.cs, el) : null;
      if (!ks.length && !cs) return hideTip();
      tip.innerHTML = ks.map(lintCard).join('') + (cs ? brief(cs) : '');
      tip.dataset.fig = fig.id;
      if (cs) show(el.dataset.cs, el);
      lit(ks, true);
      put(el);
    });
    src.addEventListener('mouseout', (e) => {
      const el = e.target.closest('[data-lint]');
      if (el) lit(el.dataset.lint.split(' '), false);
      hideTip();
    });
    const list = fig.querySelector('.ex-findings');
    list?.addEventListener('mouseover', (e) => { const li = e.target.closest('li[data-k]'); if (li) lit([li.dataset.k], true); });
    list?.addEventListener('mouseout', (e) => { const li = e.target.closest('li[data-k]'); if (li) lit([li.dataset.k], false); });
    const pick = (e) => { const a = e.target.closest('a.cs'); if (a && (e.currentTarget !== tip || tip.dataset.fig === fig.id) && show(a.dataset.cs)) e.preventDefault(); };
    fig.addEventListener('click', pick);
    tip.addEventListener('click', pick);
    show(data.initial);
  }

  // Long tables: a header click sorts; a text box and one select per badge kind filter.
  const RANK = { error: 0, warning: 1, info: 2, hint: 3 };
  const FACETS = { sev: 'severity', cat: 'category', fix: 'quick fix', from: 'origin' };
  const facet = (row, k) => [...row.querySelectorAll('.' + k)].map((e) => k === 'sev' ? e.title.split(' ').pop() : e.textContent.split(' ')[0]);
  const key = (cell) => { const sev = cell?.querySelector('.sev'); return (sev ? RANK[sev.title.split(' ').pop()] ?? 9 : '') + (cell?.textContent.trim() ?? ''); };
  for (const table of document.querySelectorAll('main table')) {
    const body = table.tBodies[0];
    const rows = [...(body?.rows ?? [])];
    if (rows.length < 20) continue;
    const bar = document.createElement('div');
    bar.className = 'filters';
    const text = Object.assign(document.createElement('input'), { type: 'search', placeholder: `filter ${rows.length} rows…` });
    const selects = Object.entries(FACETS).flatMap(([k, label]) => {
      const values = [...new Set(rows.flatMap((r) => facet(r, k)))].sort();
      if (!values.length) return [];
      const select = document.createElement('select');
      select.innerHTML = `<option value="">${label}: all</option>` + (k === 'sev' || k === 'cat' ? '' : '<option value="—">none</option>') +
        values.map((v) => `<option>${v}</option>`).join('');
      select.dataset.k = k;
      return [select];
    });
    const count = document.createElement('span');
    count.className = 'muted';
    bar.append(text, ...selects, count);
    table.before(bar);
    const apply = () => {
      const q = text.value.toLowerCase();
      let shown = 0;
      for (const row of rows) {
        const ok = row.textContent.toLowerCase().includes(q) && selects.every((s) => !s.value ||
          (s.value === '—' ? !facet(row, s.dataset.k).length : facet(row, s.dataset.k).includes(s.value)));
        row.hidden = !ok;
        shown += ok;
      }
      count.textContent = shown === rows.length ? '' : `${shown} of ${rows.length}`;
    };
    bar.oninput = apply;
    for (const [i, th] of [...table.tHead.rows[0].cells].entries()) {
      th.classList.add('sortable');
      th.tabIndex = 0;
      th.onclick = th.onkeydown = (e) => {
        if (e.key && e.key !== 'Enter') return;
        const dir = th.getAttribute('aria-sort') === 'ascending' ? -1 : 1;
        for (const h of th.parentNode.cells) h.removeAttribute('aria-sort');
        th.setAttribute('aria-sort', dir > 0 ? 'ascending' : 'descending');
        rows.sort((a, b) => dir * key(a.cells[i]).localeCompare(key(b.cells[i]), undefined, { numeric: true }));
        body.append(...rows);
      };
    }
  }

  // A lint rule's section (a heading with badges under it) is one card; every heading
  // has a link to itself, copied on click.
  for (const h of document.querySelectorAll('main h3[id]')) {
    if (!h.nextElementSibling?.classList.contains('badges')) continue;
    const card = document.createElement('section');
    card.className = 'rule';
    h.before(card);
    let n = h;
    do { const next = n.nextElementSibling; card.append(n); n = next; } while (n && !/^H[1-3]$/.test(n.tagName));
  }
  for (const h of document.querySelectorAll('body.doc main :is(h2, h3)[id]')) {
    const a = Object.assign(document.createElement('a'), { className: 'self', href: `#${h.id}`, textContent: '#', title: 'copy the link to this section' });
    a.onclick = () => { try { navigator.clipboard.writeText(new URL(`#${h.id}`, location.href).href); } catch {} };
    h.append(a);
  }
  // A link into a folded section unfolds it.
  const unfold = () => { const t = location.hash ? document.getElementById(decodeURIComponent(location.hash.slice(1))) : null; t?.closest('details')?.setAttribute('open', ''); t?.scrollIntoView(); };
  addEventListener('hashchange', unfold);
  unfold();

  // The headings of a documentation page, the one in view marked; a heading
  // in a folded group (the lint rules' categories) comes under the group's name.
  const toc = document.getElementById('toc');
  const heads = [...document.querySelectorAll('body.doc main :is(h2, h3)[id]')];
  if (!toc || heads.length < 2) return toc?.remove();
  let group = null;
  toc.innerHTML = '<b>On this page</b>' + heads.map((h) => {
    const g = h.closest('details')?.querySelector('summary')?.textContent ?? null;
    const label = g && g !== group ? `<span class="grp">${esc(g.split(/\s+[—-]\s+/)[0])}</span>` : '';
    group = g;
    return `${label}<a class="${h.localName}" href="#${h.id}">${esc(h.firstChild?.textContent ?? h.textContent)}</a>`;
  }).join('');
  const links = new Map([...toc.querySelectorAll('a')].map((a, i) => [heads[i], a]));
  const seen = new Set();
  const observer = new IntersectionObserver((entries) => {
    for (const e of entries) e.isIntersecting ? seen.add(e.target) : seen.delete(e.target);
    const first = heads.find((h) => seen.has(h));
    if (!first) return;
    for (const a of links.values()) a.classList.remove('on');
    links.get(first).classList.add('on');
  }, { rootMargin: '0px 0px -70% 0px' });
  heads.forEach((h) => observer.observe(h));
})();
