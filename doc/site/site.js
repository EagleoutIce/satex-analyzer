// Every page: the light/dark toggle, the running table of contents, sortable
// and filterable long tables, the explain cards, and the landing page's examples.
(() => {
  const root = document.documentElement;
  const dark = () => (root.dataset.theme ?? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')) === 'dark';
  self.siteDark = dark; // playground.js themes its editor by it
  const button = document.getElementById('theme');
  const label = () => button.setAttribute('aria-pressed', String(dark()));
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
        t.setAttribute('aria-selected', String(t === tab));
        document.getElementById(t.getAttribute('aria-controls')).hidden = t !== tab;
      }
    };
  }
  // The TeX tokenizer of the playground's editor, CodeMirror's stex mode. The landing
  // page's examples come colored from build.py.
  // CodeMirror, vendored (codemirror.js): the page names its hashed file, a dev server the plain one beside site.js
  const cmAt = document.querySelector('meta[name=satex-cm]'), cmUrl = new URL(cmAt?.content ?? 'codemirror.js', cmAt ? location.href : document.currentScript.src).href;
  self.siteCM = () => (self.siteCM.loaded ??= import(cmUrl));
  self.siteTex = () => (self.siteTex.loaded ??= (async () => {
    const { language: { StreamLanguage }, stex: { stex }, highlight: hl } = await self.siteCM();
    const t = hl.tags;
    const highlighter = hl.tagHighlighter([[t.tagName, 'cmd'], [t.bracket, 'br'], [t.keyword, 'kw'], [t.comment, 'com'], [t.atom, 'atom'],
      [t.number, 'num'], [t.string, 'str'], [t.definition(t.variableName), 'def'], [t.special(t.variableName), 'arg'], [t.variableName, 'arg'], [t.operator, 'br']].map(([tag, c]) => ({ tag, class: `tok-${c}` })));
    // BibTeX (btxdoc § 2): `@type{key, field = value # value}`; `@string`, `@preamble` and `@comment`
    // are commands, not entries. A braced value is TeX: commands, braces and math in the TeX mode's classes.
    const COMMANDS = /^@(string|preamble|comment)\b/i;
    const bib = StreamLanguage.define({
      startState: () => ({ depth: 0, key: false, command: '', field: true }),
      token(s, st) {
        if (st.depth === 0) {
          const command = s.match(COMMANDS);
          if (command) { st.command = command[1].toLowerCase(); st.key = false; return 'keyword'; }
          if (s.match(/@[A-Za-z]+/)) { st.command = ''; st.key = true; return 'tag'; }
          if ((st.key || st.command) && s.match(/[{(]/)) { st.depth = 1; st.field = true; return 'bracket'; }
          if (s.eatSpace()) return null;
          s.skipTo('@') || s.skipToEnd(); return 'comment';
        }
        if (st.command === 'comment') {
          if (s.match(/[{(]/)) { st.depth++; return 'comment'; }
          if (s.match(/[})]/)) { if (--st.depth === 0) st.command = ''; return 'comment'; }
          s.match(/[^{}()]+/) || s.next(); return 'comment';
        }
        if (st.depth > 1) { // inside a braced value: TeX
          if (s.match(/\\(?:[A-Za-z@]+|.)/)) return 'tagName';
          if (s.match(/\$/)) return 'atom';
          if (s.match(/[{]/)) { st.depth++; return 'bracket'; }
          if (s.match(/[}]/)) { st.depth--; return 'bracket'; }
          s.match(/[^{}\\$]+/) || s.next(); return 'string';
        }
        if (s.eatSpace()) return null;
        if (st.key) { st.key = false; if (s.match(/[^,\s{}]+/)) return 'atom'; }
        if (s.match(/[{]/)) { st.depth = 2; return 'bracket'; }
        if (s.match(/[})]/)) { st.depth = 0; st.key = false; st.command = ''; return 'bracket'; }
        if (s.match(/"(?:[^"\\]|\\.)*"?/)) return 'string';
        if (s.match(/\d+/)) return 'number';
        if (s.match(/#/)) return 'operator';
        if (s.match(/,/)) { st.field = true; return 'punctuation'; }
        if (s.match(/=/)) { st.field = false; return 'punctuation'; }
        // before `=`: a field name, or the name a @string defines; after it: a @string name used
        if (s.match(/[A-Za-z_][\w:.+/-]*/)) return st.field ? (st.command === 'string' ? 'variableName.definition' : 'keyword') : 'variableName.special';
        s.next(); return null;
      },
    });
    // a control symbol (`\ `, `\-`, `\'`) is one token, as for TeX; stex leaves some of them split
    const tex = { ...stex, token: (s, st) => (s.match(/^\\[^A-Za-z@\xc0-\uffff]/) ? 'tag' : stex.token(s, st)) };
    return { language: StreamLanguage.define(tex), bib, highlighter };
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
    if (!(i % 2)) return p.replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>').replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>')
      .replace(/\*\(([^*]+)\)\*/g, '<span class="x-from">($1)</span>').replace(/\*([^*\s][^*]*)\*/g, '<em>$1</em>');
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
    return `<span class="arg arg-${kind ?? 'slot'}" data-tip="${esc(tip)}">&#x27E8;${named}${ty ? `<small>: ${tyLink(ty)}</small>` : ''}&#x27E9;</span>`;
  };
  /** The placeholders of primitive syntax that are types of the Ty lattice. */
  const TY = new Set(['number', 'dimen', 'glue', 'muglue', 'control sequence']);
  /** A type's name linking to the wiki's Ty lattice; hovered, it shows its card ([`tyCard`]). */
  const tyLink = (t) => `<a class="ty" target="_blank" rel="noopener" href="${document.querySelector('a.brand')?.getAttribute('href')?.replace(/index\.html$/, '') ?? ''}wiki/domains.html#argument-types-the-ty-lattice" data-ty="${esc(t)}" data-fam="${esc(TYPES().get(t)?.family ?? '')}"${TYPES().has(t) ? '' : ` title="the type ${esc(t)} in the Ty lattice"`}>${esc(t)}</a>`;
  /** The lattice by name (`satex completions json`'s `types`), when the page has it. */
  const TYPES = () => (window.satexTypes ??= new Map((window.satexTypeList ?? []).map((t) => [t.name, t])));
  /** A type's card: its chain up to `any`, what it is, the types right below it. */
  function tyCard(name) {
    const types = TYPES(), t = types.get(name);
    if (!t) return '';
    const up = [];
    // `any` tops every chain: left implied
    for (let x = t; x && x.name !== 'any' && !up.includes(x.name); x = types.get(x.parent)) up.unshift(x.name);
    const below = [...types.values()].filter((x) => x.parent === name && x.name !== name).map((x) => x.name);
    const chain = up.map((n) => (n === name ? `<b>${esc(n)}</b>` : esc(n))).join(' <span class="ty-sep">›</span> ');
    const badge = t.family === 'expl3' ? ' <span class="ty-badge">expl3</span>' : '';
    return `<div class="ty-chain">${chain}${badge}</div><p>${esc(t.about).replace(/`([^`]+)`/g, '<code>$1</code>')}</p>` +
      (below.length ? `<div class="ty-below"><span>below</span> ${below.map((n) => `<code>${esc(n)}</code>`).join(' ')}</div>` : '') +
      '<div class="ty-foot">click: the lattice in the wiki</div>';
  }
  // ------------------------------------------------------------ tooltips
  // Every tooltip of the site is one of two floating cards made here: `satexTip.card` for a
  // command's card, a finding or a name in a trace, `satexTip.note` for short text (a `title`,
  // a type, an argument chip), which may stand beside a card.
  const clamp = (x, lo, hi) => Math.min(Math.max(x, lo), hi);
  function floating(cls) {
    const el = Object.assign(document.createElement('div'), { className: `tip ${cls}`, hidden: true, role: 'tooltip' });
    let timer = 0;
    const later = (f, ms) => { clearTimeout(timer); timer = setTimeout(f, ms); };
    const hide = (ms = 0) => later(() => { el.hidden = true; delete el.dataset.pin; }, ms);
    el.addEventListener('mouseenter', () => clearTimeout(timer));
    el.addEventListener('mouseleave', () => { if (!el.dataset.pin) hide(120); });
    /** Below `at` (an element or a rect), else above it; beside `host` when that is a card it would cover. */
    const show = (at, html, host = null) => {
      clearTimeout(timer);
      // a modal dialog's top layer hides anything outside it
      const layer = at.closest?.('dialog[open]') ?? document.querySelector('dialog[open]') ?? document.body;
      if (el.parentElement !== layer) layer.append(el);
      el.innerHTML = html;
      el.hidden = false;
      const a = at.getBoundingClientRect?.() ?? at, h = host?.getBoundingClientRect() ?? a, w = el.offsetWidth, ht = el.offsetHeight;
      let left = clamp(a.left, 8, innerWidth - w - 8), top = h.bottom + 6 + ht <= innerHeight ? h.bottom + 6 : h.top - ht - 6;
      if (top < 8) { top = clamp(a.top, 8, innerHeight - ht - 8); left = h.right + 6 + w <= innerWidth ? h.right + 6 : Math.max(8, h.left - w - 6); }
      Object.assign(el.style, { left: `${left}px`, top: `${top}px` });
    };
    return { el, show, hide, later };
  }
  const tips = self.satexTip = { card: floating('tip-card'), note: floating('tip-note') };
  {
    const { note } = tips;
    // a native `title` would bring the browser's own tooltip: lent to the note while hovered
    let titled = null;
    const untitle = () => { if (titled) { titled.title = titled.dataset.title; delete titled.dataset.title; titled = null; } };
    document.addEventListener('mouseover', (e) => {
      const a = e.target.closest?.('a.ty[data-ty]'), chip = e.target.closest?.('.arg[data-tip]');
      const ty = a?.dataset.ty ?? chip?.querySelector('a.ty[data-ty]')?.dataset.ty;
      const html = (chip ? `<div class="ty-what">${esc(chip.dataset.tip)}</div>` : '') + (ty ? tyCard(ty) : '');
      const t = !html && (e.target.closest?.('[title]') ?? (titled?.contains(e.target) ? titled : null));
      if (t) {
        if (t === titled) return;
        untitle();
        if (!t.title.trim()) return;
        titled = t;
        t.dataset.title = t.title;
        t.removeAttribute('title');
        note.later(() => { if (titled === t) note.show(t, esc(t.dataset.title), t.closest('.tip')); }, 450);
        return;
      }
      untitle();
      if (!html) { if (!e.target.closest?.('.tip-note')) note.hide(); return; }
      note.show(chip ?? a, html, (chip ?? a).closest('.tip, .cm-tooltip'));
    });
    document.addEventListener('pointerdown', (e) => { untitle(); note.hide(); if (!e.target.closest?.('.tip-card')) tips.card.hide(); }, true);
  }
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
      // required text, as written (`‹key:›`)
      if (c === '‹') { const j = spec.indexOf('›', i); const lit = spec.slice(i + 1, j < 0 ? spec.length : j); out.push({ key: lit, html: `<span class="arg arg-lit" title="text the command requires here">${esc(lit)}</span>` }); i = j < 0 ? spec.length : j + 1; continue; }
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
  /** A command's name as shown: a control space `\ ` keeps its space visible, as one token. */
  const csName = (name) => esc(String(name ?? '').replace(/^\\ $/, '\\␣'));
  const csChip = (name) => `<a class="cs" href="#" data-cs="${esc(name)}" title="explain ${csName(name)}">${csName(name)}</a>`;
  /** The one marker of a package, class or the kernel, as the cache dialog badges them. */
  const pkgChip = (kind, name) => `<span class="cache-kind k-${kind}" title="${kind === 'kernel' ? 'defined by the kernel' : `from the ${kind} ${esc(name)}`}">${esc(name)}</span>`;
  /** The playground's button that traces a call of the command. */
  /** Trace where the project calls the name (the playground's `satexCalled`), else Try a sample call in a scratch run. */
  const traceButton = (r) => !document.body.classList.contains('play') || !/^\\/.test(r.name) ? ''
    : r.uses !== 0 && (self.satexCalled?.(r.name) ?? true) ? ` <button type="button" class="small ghost x-trace" data-trace="${esc(r.name)}" title="step through how a call of ${csName(r.name)} expands">Trace</button>`
      : ` <button type="button" class="small ghost x-trace" data-try="${esc(r.effective ?? r.name)}" title="the project never calls ${csName(r.name)}: try a call in a scratch run, its arguments yours to fill in">Try</button>`;
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
    if (r.syntax) rows.push(['usage', `<code class="usage">${slots(esc(r.syntax))}</code>`]); // a primitive's TeXbook syntax
    else if (r.effective && r.effective !== r.name) rows.push(['usage', `<code class="usage">${usage(r.effective, types).html}</code>`]);
    if (r.types?.length) rows.push(['types', `<span class="chips">${Object.entries(types).map(([n, t]) => `<span class="x-type"><code>#${n.slice(3)}</code> ${tyLink(t)}</span>`).join('')}</span>`]);
    if (r.forms?.length) rows.push(['forms', r.forms.map((f) => `<code>${esc(f)}</code>`).join(' ')]);
    if (primitive) rows.push(['body', 'none: a TeX primitive']);
    else if (r.body != null) rows.push(['body', r.body === '' ? '<span class="muted">empty</span>' : `<pre class="x-body"><code>${tex(r.body, true)}</code></pre>`]);
    if (r.by) rows.push(['by', csChip(r.by)]);
    const owner = r.class ? pkgChip('class', r.class) : r.package ? (r.package === 'kernel' ? pkgChip('kernel', 'the kernel') : pkgChip('package', r.package)) : '';
    if (r.path && r.line) rows.push(['defined at', `${placeLink(r.path, r.line, r.file)}${owner ? ` ${owner}` : ''}`]);
    else if (owner) rows.push(['from', owner]);
    // `in environments via \tikz@installcommands: tikzpicture, scope`: each name a link to its explanation
    if (r.context) rows.push(['context', esc(r.context)
      .replace(/: ([^:]+?)(?=, in |, outside|$)/g, (m, list) => `: ${list.split(', ').map((e) => `<code class="cs-ref env-ref" data-cs="\\${e}" tabindex="0">${e}</code>`).join(' ')}`)
      .replace(/(^|[\s(])(\\[A-Za-z@]+)/g, (m, pre, cs) => `${pre}<code class="cs-ref" data-cs="${cs}" tabindex="0">${cs}</code>`)]);
    if (r.uses != null) rows.push(['uses', r.uses ? `${r.uses} in this run` : 'none in this run']);
    for (const [key, label] of [['expands', 'always expands'], ['may_expand', 'may expand']]) if (r[key]?.length) rows.push([label, `<span class="chips">${r[key].map(csChip).join('')}</span>`]);
    // what the document's calls leave defined: the public names, the internal ones folded
    const made = r.creates ?? [], inner = r.creates_internal ?? [];
    if (made.length || inner.length) rows.push(['creates', `<span class="chips">${made.map(csChip).join('')}</span>` +
      (inner.length ? `<details class="x-more"><summary>${inner.length} internal</summary><span class="chips">${inner.map(csChip).join('')}</span></details>` : '')]);
    if (r.error) rows.push(['error', `<span class="x-err">${esc(r.error)}</span>`]);
    const tags = [primitive ? 'primitive' : r.tag, r.when && `in the ${r.when}`, r.certain === false && 'uncertain'].filter(Boolean);
    return `<div class="xcard"><div class="x-head"><code class="x-name">${csName(r.name)}</code>${tags.map((t) => ` <span class="x-kind">${esc(t)}</span>`).join('')}${traceButton(r)}</div>` +
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
    return `<div class="xcard brief"><div class="x-head"><code class="x-name">${csName(r.name)}</code>${tags.map((t) => ` <span class="x-kind">${esc(t)}</span>`).join('')}` +
      `${where ? ` <span class="x-where">${where}</span>` : ''}${traceButton(r)}</div>` +
      (r.does ? `<div class="x-does">${esc(r.does)}</div>` : '') + (use ? `<div class="x-use">${use}</div>` : '') + (r.error ? `<div class="x-err">${esc(r.error)}</div>` : '') +
      (docs ? `<div class="x-docs">${docs}</div>` : '') + '</div>';
  }
  /** Any other record value for a table cell: lists as chips, never raw JSON. */
  const value = (v) => v == null ? '' : Array.isArray(v) ? v.map((x) => typeof x === 'object' ? value(x) : /^\\/.test(String(x)) ? csChip(String(x)) : `<code>${esc(x)}</code>`).join(' ')
    : typeof v === 'object' ? ('min' in v && 'max' in v ? argumentCount(v) : Object.entries(v).map(([k, x]) => `${esc(k)} ${value(x)}`).join(', '))
      : /^\\/.test(String(v)) ? csChip(String(v)) : /^[{}#]/.test(String(v)) ? `<code>${esc(v)}</code>` : esc(v);
  /** satex's notation in typeset text (`\uE000…\uE001`): `⟦…⟧` is text set in a box, on the page
   * only if the box is used; `·` a place the line breaker may hyphenate, not shown; `│`, `―`, `▬`
   * a table's `|`, an `\hline` and a `\rule`, drawn. An alignment's parts (`\u0003begin`, `cell`, `row`,
   * `end`) make a grid: a row opens at its first character, so an `\hline` before it rules its top. */
  const RULES = { '│': ['ts-vrule', 'a \\vrule: a table\'s |'], '―': ['ts-hrule', 'an \\hrule: \\hline'], '▬': ['ts-bar', 'a \\rule'] };
  /** `marked` with each key and font change as a note of its own, where it stands. `at` counts every
   * character but the note brackets, a note's own (a hyphen's `·`) too, and a key never parts a note. */
  function withKeys(marked, keys = [], fonts = [], align = []) {
    const at = [...(keys ?? []).filter((k) => k?.key != null), ...(fonts ?? []), ...(align ?? [])].sort((a, b) => a.at - b.at);
    if (!at.length) return marked;
    let out = '', seen = 0, i = 0, note = false;
    const mark = (k) => k.style != null ? `\u0002${k.style}` : k.mark != null ? `\u0003${k.mark}` : `\u0001${k.kind ?? 'key'}\u0001${k.key}`;
    const put = (n) => { while (i < at.length && at[i].at <= n) out += `\uE000${mark(at[i++])}\uE001`; };
    for (const c of marked) {
      if (c === '\uE000') { put(seen); note = true; } else if (c === '\uE001') note = false;
      else { if (!note) put(seen); seen++; }
      out += c;
    }
    put(Infinity);
    return out;
  }
  function notation(marked) {
    // row: false while a row waits for its first character, true if an \hline rules it, null when one is open
    let open = 0, font = '', table = 0, row = null;
    const opened = () => {
      if (row === null) return '';
      const tr = `<span class="ts-tr${row ? ' ts-ruled' : ''}"><span class="ts-td">`;
      row = null;
      return tr;
    };
    const grid = (mark) => {
      if (mark === 'begin') { table++; row = false; return '<span class="ts-table">'; }
      if (!table) return '';
      if (mark === 'cell') return `${opened()}</span><span class="ts-td">`;
      const shut = row === null ? '</span></span>' : row ? '<span class="ts-tr ts-ruled"></span>' : '';
      if (mark === 'row') { row = false; return shut; }
      table--;
      row = null;
      return `${shut}</span>`;
    };
    const html = marked.split(/(\uE000[^\uE001]*\uE001)/).map((part) => {
      const note = /^\uE000([^\uE001]*)\uE001$/.exec(part)?.[1];
      if (note === undefined) {
        if (row !== null && !part.trim()) return '';
        const tr = table ? opened() : '';
        return tr + (font && part ? `<span class="${font}">${esc(part)}</span>` : esc(part));
      }
      if (note[0] === '\u0003') return grid(note.slice(1));
      if (note === '―' && table && row !== null) { row = true; return ''; }
      if (note[0] === '\u0002') { font = note.slice(1).split(' ').filter(Boolean).map((x) => `ts-${x}`).join(' '); return ''; }
      if (note === '·') return '';
      const tr = table ? opened() : '';
      if (note === '⟦') { open++; return `${tr}<span class="ts-box" title="set in a box: on the page only if the box is used">`; }
      if (note === '⟧') { if (!open) return tr; open--; return `${tr}</span>`; }
      if (note[0] === '\u0001') { const [, kind, key] = note.split('\u0001'); return `${tr}<span class="ts-key ts-key-${esc(kind)}" title="${esc(kind)}">${esc(key)}</span>`; }
      if (RULES[note]) return `${tr}<span class="${RULES[note][0]}" title="${RULES[note][1]}"></span>`;
      return `${tr}<span class="ts-note">${esc(note)}</span>`;
    }).join('');
    return html + '</span>'.repeat(open + 2 * table);
  }
  /** The Markdown satex writes, for hovers and rule explanations: headings, paragraphs, lists,
   * fenced or indented code, links, emphasis. */
  const indented = (b) => b.split('\n').every((l) => !l.trim() || /^ {4}/.test(l));
  const markdown = (text) => text.split(/^```\w*\n([\s\S]*?)^```\s*$/m).map((part, i) => i % 2 ? `<pre><code>${esc(part)}</code></pre>`
    : part.split(/\n{2,}/).filter((b) => b.trim()).map((b) => indented(b) ? `<pre><code>${esc(b.replace(/^ {4}/gm, ''))}</code></pre>`
      : /^#+ /.test(b.trim()) ? `<h4>${inline(b.trim().replace(/^#+ /, ''))}</h4>`
        : /^[-*] /.test(b.trim()) ? `<ul>${b.trim().split(/\n(?=[-*] )/).map((li) => `<li>${inline(li.slice(2))}</li>`).join('')}</ul>`
          : `<p>${inline(b.trim())}</p>`).join('')).join('');
  /** A lint rule's card, the parts and markup of its section on the rules page: name, number,
   * severity, category, engines, fixes, summary, text and references. */
  const ruleCard = (r, href = '') => {
    const refs = (r.references ?? []).map((x) => `<a href="${esc(x.url)}" target="_blank" rel="noopener">${esc(x.cite)}</a>`).join(', ');
    const engines = (r.engines ?? []).join(', ');
    const badges = [`<code class="num" title="rule number">${esc(r.number ?? '')}</code>`,
      `<span class="sev sev-${esc(r.severity)}" title="default: ${esc(r.severity)}">default: ${esc(r.severity)}</span>`,
      `<span class="cat cat-${esc(r.category)}">${esc(r.category)}</span>`,
      ...(engines ? [`<span class="eng" title="only under ${esc(engines)}">${esc(engines)}</span>`] : []),
      ...(r.fixes ?? []).map((f) => `<span class="fix fix-${esc(f)}">${esc(f)}</span>`)];
    const name = href ? `<a href="${esc(href)}">${esc(r.code)}</a>` : esc(r.code);
    return `<section class="rule rule-tip"><h4>${name}</h4><p class="badges">${badges.join(' ')}</p>` +
      `<p><b>${inline(r.summary ?? r.message ?? '')}</b></p>${markdown(r.explanation ?? '')}${refs ? `<p>See ${refs}.</p>` : ''}</section>`;
  };
  self.satexCards = { explain, brief, signature, slots, usage, tex, inline, markdown, ruleCard, placeLink, upstream, csChip, csName, value, esc, notation, withKeys };
  // A rule named anywhere on the site, as a link into the rules page or its code or number in a
  // code span, shows its card. The cards' data is the rules the build wrote, fetched once.
  const RULES_URL = new URL('rules.json', document.currentScript?.src ?? location.href);
  let ruleData = null;
  const ruleNamed = async (name) => {
    const all = await (ruleData ??= fetch(RULES_URL).then((r) => r.ok ? r.json() : {}).catch(() => ({})));
    return all[name] ?? Object.values(all).find((r) => r.number === name);
  };
  const ruleLink = (code) => new URL(`wiki/lints.html#${encodeURIComponent(code)}`, RULES_URL).href;
  self.satexRule = { named: ruleNamed, link: ruleLink };
  const RULE_NAME = /^(?:[a-z0-9]+(?:-[a-z0-9]+)+|[CXSPA]\d{3})$/;
  document.addEventListener('mouseover', async (e) => {
    const el = e.target.closest?.('a[href*="lints.html#"], code');
    if (!el || el.closest('.tip, section.rule, .cm-editor')) return;
    const name = el.localName === 'a' ? decodeURIComponent(el.hash.slice(1)) : el.textContent.trim();
    if (!RULE_NAME.test(name)) return;
    const rule = await ruleNamed(name);
    if (rule && el.matches(':hover')) tips.card.show(el, ruleCard(rule, ruleLink(rule.code)));
  });
  document.addEventListener('mouseout', (e) => {
    if (e.target.closest?.('a[href*="lints.html#"], code') && tips.card.el.querySelector('.rule-tip')) tips.card.hide(200);
  });

  // ------------------------------------------------------------ the landing page's examples
  // Each example: its source, findings underlined with a marker at the line's end, and what it
  // typesets beside it. A command explains itself on hover, a finding shows its rule: one floating
  // card, so nothing on the page moves; a click on a command pins its full card.
  const { card } = tips, tip = card.el;
  /** Typeset paragraphs as the playground's Typeset panel draws them. */
  const paragraphs = (rows) => rows.map((r) => `<p class="ts${r.heading ? ` ts-h ts-${esc(r.heading)}` : ''}"><span class="ts-text">${notation(withKeys(r.marked ?? r.detail ?? '', r.keys, r.fonts, r.align))}</span></p>`).join('')
    || '<p class="muted">It sets no paragraph.</p>';
  const lintCard = (d) => {
    const diff = d.typo ? (d.edits ?? []).map((e) => ` <span class="diff"><del>${esc(e.from) || '␣'}</del>→<ins>${esc(e.to) || '∅'}</ins></span>`).join('') : '';
    return `<div class="tip-lint f-${d.severity}"><p><span class="sev sev-${d.severity}">${d.severity}</span> <code>${esc(d.code)}</code> ${inline(d.message)}</p>` +
      (d.fix ? `<p class="tip-fix"><span class="fix fix-${d.applicability}">fix</span> ${inline(d.fix)}${diff}</p>` : '') + '</div>';
  };
  /** The findings' cards, then their rules' cards. */
  const findingCards = async (ds) => ds.map(lintCard).join('') + (await Promise.all([...new Set(ds.map((d) => d.code))].map(ruleNamed)))
    .filter(Boolean).map((r) => ruleCard(r, ruleLink(r.code))).join('');
  for (const fig of document.querySelectorAll('.example:not(.live)')) {
    const data = JSON.parse(fig.querySelector('.ex-data').textContent);
    fig.querySelector('.ex-side').innerHTML = paragraphs(data.ts ?? []);
    // A command before `\begin{document}` means what it means in the preamble.
    const src = fig.querySelector('pre.src');
    const lines = [...src.querySelectorAll('.ln')];
    const begins = lines.findIndex((l) => l.textContent.includes('\\begin{document}'));
    const when = (el) => el && begins >= 0 && lines.indexOf(el.closest('.ln')) < begins ? 'preamble' : 'document';
    const record = (name, el) => {
      const all = data.cs[name] ?? [];
      return all.find((r) => r.when && r.when === when(el)) ?? all.find((r) => !r.error) ?? all[0];
    };
    const lit = (ks, on) => { for (const k of ks) for (const x of fig.querySelectorAll(`[data-lint~="${k}"]`)) x.classList.toggle('lit', on); };
    src.addEventListener('mouseover', async (e) => {
      const el = e.target.closest('[data-cs], [data-lint]');
      if (!el || tip.dataset.pin) return;
      const ks = el.dataset.lint ? el.dataset.lint.split(' ') : [];
      const cs = el.dataset.cs ? record(el.dataset.cs, el) : null;
      if (!ks.length && !cs) return card.hide(250);
      lit(ks, true);
      const html = (ks.length ? await findingCards(ks.map((k) => data.lints[k])) : '') + (cs ? brief(cs) : '');
      if (!el.matches(':hover')) return;
      card.show(el, html);
      tip.dataset.fig = fig.id;
    });
    src.addEventListener('mouseout', (e) => {
      const el = e.target.closest('[data-lint]');
      if (el) lit(el.dataset.lint.split(' '), false);
      if (!tip.dataset.pin) card.hide(250);
    });
    // a click on a command, here or in its card, pins its full card
    const pin = (e) => {
      const a = e.target.closest('[data-cs]');
      const r = a && (e.currentTarget !== tip || tip.dataset.fig === fig.id) && record(a.dataset.cs, a);
      if (!r) return;
      e.preventDefault();
      card.show(a, explain(r));
      tip.dataset.fig = fig.id;
      tip.dataset.pin = '1';
    };
    src.addEventListener('click', pin);
    tip.addEventListener('click', pin);
  }

  // The last tab: the reader's own text in the playground's editor and engine (worker and manifest
  // from play/index.html), started once the tab opens; each edit, debounced, shows what it typesets
  // and its findings, underlined like the examples'.
  const live = document.querySelector('.example.live');
  if (live) {
    const out = live.querySelector('.live-ts'), status = live.querySelector('.live-status'), host = live.querySelector('.live-src');
    const MiB = (n) => (n / 2 ** 20).toFixed(1), SEV = ['', 'error', 'warning', 'info', 'info'], uri = 'file:///work/try.tex';
    const NAMED = /\\(?:documentclass|usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}/g;
    const FIRST = '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, \\emph{#1}!}\n\\begin{document}\n\\section{Welcome}\n\\greet{world} Pages 10-12 come next.\nSee Section~\\ref{sec:none}.\n\\end{document}\n';
    let engine = null, version = 0, timer = 0, editor = null;
    const say = (html, bad = false) => { status.innerHTML = html; status.classList.toggle('live-bad', bad); };
    async function boot() {
      say('Loading SaTeX…');
      const at = new URL('play/index.html', location.href);
      const manifest = JSON.parse((await (await fetch(at)).text()).match(/<script id="satex-play" type="application\/json">(.*?)<\/script>/s)[1]);
      const worker = new Worker(URL.createObjectURL(new Blob([manifest.worker], { type: 'text/javascript' })));
      let line = Promise.resolve(), waiting = null, id = 0, found = [];
      worker.onmessage = ({ data }) => {
        if (data.type === 'progress') return say(`Loading SaTeX <progress max="${data.total || 1}" value="${data.loaded}"></progress> ${MiB(data.loaded)} of ${MiB(data.total)} MiB`);
        if (data.type === 'status') return;
        if (data.type === 'lsp') for (const m of data.out) if (m.method === 'textDocument/publishDiagnostics' && m.params.uri === uri) found = m.params.diagnostics;
        const w = waiting;
        waiting = null;
        if (data.type === 'error') w?.reject(new Error(data.message.split('\n')[0])); else w?.resolve(data);
      };
      // one message at a time, each answered before the next goes; `project` and `reset` have no answer
      /** @type {(msg: object) => Promise<any>} */
      const ask = (msg) => (line = line.then(() => new Promise((resolve, reject) => {
        if (msg.type === 'project' || msg.type === 'reset') resolve(); else waiting = { resolve, reject };
        worker.postMessage(msg);
      })));
      const lsp = async (method, params, note = false) => {
        const n = ++id, r = await ask({ type: 'lsp', message: { jsonrpc: '2.0', method, params, ...(note ? {} : { id: n }) } });
        return r.out.find((m) => m.id === n)?.result;
      };
      await ask({ type: 'init', base: new URL(manifest.root, at).href, manifest });
      return { manifest, base: new URL(manifest.root, at).href, kernel: manifest.kernels[0], ask, lsp, have: new Set(), found: () => found, opened: false };
    }
    const start = () => (engine ??= boot().catch((e) => { say(`SaTeX did not load: ${esc(e.message)}`, true); throw e; }));
    /** The bundles the text names (a class or package) and those they need, not loaded yet. */
    function wanted(e, text) {
      const byId = new Map((e.manifest.packages ?? []).map((p) => [p.id, p])), picked = new Map();
      const pick = (id) => { const p = byId.get(id); if (p && !picked.has(id) && !e.have.has(id)) { picked.set(id, p); p.needs.forEach((n) => pick(`${e.kernel.id}/${n}`)); } };
      for (const m of text.matchAll(NAMED)) for (const n of m[1].split(',').map((x) => x.trim())) { pick(`${e.kernel.id}/${n}`); pick(`${e.kernel.id}/${n}.cls`); }
      return [...picked.values()];
    }
    const text = () => editor?.state.doc.toString() ?? FIRST;
    async function analyze() {
      const mine = ++version, e = await start(), now = text(), t0 = performance.now();
      const more = wanted(e, now);
      if (more.length) { say(`Loading ${esc(more.map((p) => p.label).join(', '))}…`); await e.ask({ type: 'bundles', base: e.base, entries: more }); more.forEach((p) => e.have.add(p.id)); }
      if (mine !== version) return;
      say('Analyzing…');
      live.classList.add('busy');
      e.ask({ type: 'project', dir: '/work', files: { 'try.tex': new TextEncoder().encode(now) } });
      if (!e.opened) {
        e.opened = true;
        e.ask({ type: 'reset', args: [...e.kernel.args, '-f', 'try.tex'], cwd: '/work' });
        await e.lsp('initialize', { processId: null, rootUri: 'file:///work', capabilities: {} });
        await e.lsp('initialized', {}, true);
        await e.lsp('textDocument/didOpen', { textDocument: { uri, languageId: 'latex', version: mine, text: now } }, true);
      } else {
        await e.lsp('textDocument/didChange', { textDocument: { uri, version: mine }, contentChanges: [{ text: now }] }, true);
        await e.lsp('satex/analyze', {}, true);
      }
      if (mine !== version) return; // a newer text is on its way
      const rows = (await e.lsp('satex/typeset', { textDocument: { uri } })) ?? [];
      if (mine !== version) return;
      live.classList.remove('busy');
      out.innerHTML = paragraphs(rows.filter((r) => r.kind === 'paragraph' && String(r.path ?? '').endsWith('try.tex')));
      underline(e.found());
      say(`Analyzed in ${Math.round(performance.now() - t0)} ms. <a href="play/index.html">The playground</a> has traces, the library and more.`);
    }
    /** The editor: the playground's tokenizer, completion and hover from the engine's language server. */
    let underline = () => {};
    async function edit() {
      const [{ codemirror: { EditorView, basicSetup }, autocomplete: ac, lint, language, oneDark: { oneDark }, state: st }, tex] = await Promise.all([self.siteCM(), self.siteTex()]);
      const pos = (doc, p) => { const l = doc.lineAt(p); return { line: l.number - 1, character: p - l.from }; };
      const off = (doc, p) => Math.min(doc.line(Math.min(p.line + 1, doc.lines)).from + p.character, doc.length);
      async function complete(ctx) {
        const word = ctx.matchBefore(/\\[A-Za-z@]*/);
        if (!word || !engine) return null;
        const e = await engine, r = await e.lsp('textDocument/completion', { textDocument: { uri }, position: pos(ctx.state.doc, ctx.pos) });
        const items = (Array.isArray(r) ? r : r?.items) ?? [];
        return { from: word.from + 1, options: items.slice(0, 200).map((i) => ({ label: i.label.replace(/^\\/, ''), detail: i.detail, apply: i.insertText?.replace(/^\\/, '') ?? i.label.replace(/^\\/, '') })) };
      }
      const theme = new st.Compartment();
      editor = new EditorView({
        parent: host, doc: FIRST,
        extensions: [basicSetup, language.syntaxHighlighting(tex.highlighter), tex.language, lint.linter(null), lint.lintGutter(), ac.autocompletion({ override: [complete] }), theme.of(dark() ? oneDark : []),
          EditorView.updateListener.of((u) => { if (u.docChanged) { clearTimeout(timer); timer = setTimeout(analyze, 350); } })],
      });
      new MutationObserver(() => editor.dispatch({ effects: theme.reconfigure(dark() ? oneDark : []) })).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
      underline = (ds) => editor.dispatch(lint.setDiagnostics(editor.state, ds.map((d) => {
        const from = off(editor.state.doc, d.range.start), to = off(editor.state.doc, d.range.end);
        return { from, to: Math.min(Math.max(to, from + 1), editor.state.doc.length), severity: SEV[d.severity] || 'info', message: d.message, source: d.data?.rule ?? d.code };
      })));
      // hover: the language server's card, as in the playground
      let shown = null;
      host.addEventListener('mousemove', (ev) => {
        const p = editor.posAtCoords({ x: ev.clientX, y: ev.clientY });
        if (p == null || !engine) return;
        clearTimeout(shown);
        shown = setTimeout(async () => {
          const e = await engine, r = await e.lsp('textDocument/hover', { textDocument: { uri }, position: pos(editor.state.doc, p) }).catch(() => null);
          const c = r?.contents, value = typeof c === 'string' ? c : c?.value ?? '';
          const at = editor.coordsAtPos(p);
          if (value && at && host.matches(':hover')) card.show({ left: at.left, top: at.top, right: at.right, bottom: at.bottom }, `<div class="hover">${markdown(value)}</div>`);
        }, 300);
      });
      host.addEventListener('mouseleave', () => { clearTimeout(shown); card.hide(150); });
    }
    const open = () => { if (!editor) edit().then(analyze); };
    document.getElementById(live.getAttribute('aria-labelledby'))?.addEventListener('click', open);
  }

  // Long tables: a header click sorts, a text box and one select per badge kind filter, pages
  // of a chosen size, and a columns menu on the header that hides any column.
  const RANK = { error: 0, warning: 1, info: 2, hint: 3 };
  const FACETS = { sev: 'severity', cat: 'category', eng: 'engine', fix: 'quick fix', from: 'origin' };
  const facet = (row, k) => [...row.querySelectorAll('.' + k)].flatMap((e) => k === 'sev' ? [e.title.split(' ').pop()]
    : k === 'eng' ? e.textContent.split(', ') : [e.textContent.split(' ')[0]]);
  const key = (cell) => { const sev = cell?.querySelector('.sev'); return (sev ? RANK[sev.title.split(' ').pop()] ?? 9 : '') + (cell?.textContent.trim() ?? ''); };
  const SIZES = [25, 50, 100, 0];
  const remember = (name, fallback) => { try { return JSON.parse(localStorage.getItem(name) ?? 'null') ?? fallback; } catch { return fallback; } };
  const keep = (name, value) => { try { localStorage.setItem(name, JSON.stringify(value)); } catch {} };
  for (const table of document.querySelectorAll('main table')) {
    const body = table.tBodies[0];
    const rows = [...(body?.rows ?? [])];
    if (rows.length < 20) continue;
    const heads = [...table.tHead.rows[0].cells];
    const names = heads.map((h) => h.textContent.trim());
    // the summary takes the width the other columns leave, cut to one line
    const grow = Math.max(names.indexOf('summary'), 0) || heads.length - 1;
    for (const row of [table.tHead.rows[0], ...rows]) row.cells[grow]?.classList.add('fill');
    for (const row of rows) if (row.cells[grow]) row.cells[grow].title = row.cells[grow].textContent.trim();
    const bar = Object.assign(document.createElement('div'), { className: 'filters' });
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
    const stored = `satex-table:${location.pathname}:${names.join('|')}`;
    const saved = remember(stored, {});
    const menu = Object.assign(document.createElement('details'), { className: 'cols',
      innerHTML: '<summary title="columns" aria-label="columns"><svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M2 3h3v10H2zM6.5 3h3v10h-3zM11 3h3v10h-3z" fill="currentColor"/></svg></summary><div></div>' });
    const toggle = (name, on, set) => {
      const box = Object.assign(document.createElement('input'), { type: 'checkbox', checked: on });
      box.onchange = () => { set(box.checked); saved[name] = box.checked; keep(stored, saved); };
      const label = document.createElement('label');
      label.append(box, ` ${name}`);
      menu.lastChild.append(label);
      set(on);
    };
    names.forEach((name, i) => toggle(name, saved[name] ?? true, (on) => { for (const row of table.rows) if (row.cells[i]) row.cells[i].hidden = !on; }));
    toggle('wrap text', saved['wrap text'] ?? false, (on) => table.classList.toggle('wrap', on));
    menu.onclick = menu.onkeydown = (e) => e.stopPropagation();
    document.addEventListener('click', (e) => { if (!menu.contains(e.target)) menu.open = false; });
    heads.at(-1).append(menu);
    table.classList.add('long');
    const scroll = Object.assign(document.createElement('div'), { className: 'table-scroll' });
    table.before(scroll);
    scroll.append(table);
    const pager = Object.assign(document.createElement('div'), { className: 'pager' });
    const size = Object.assign(document.createElement('select'), { title: 'rows per page',
      innerHTML: SIZES.map((n) => `<option value="${n}">${n ? `${n} per page` : 'all rows'}</option>`).join('') });
    size.value = String(saved['page size'] ?? SIZES[0]);
    const [prev, next] = ['‹', '›'].map((t) => Object.assign(document.createElement('button'), { type: 'button', textContent: t }));
    const where = Object.assign(document.createElement('span'), { className: 'muted' });
    pager.append(prev, where, next, size);
    scroll.after(pager);
    bar.append(text, ...selects);
    scroll.before(bar);
    let page = 0;
    const apply = () => {
      const q = text.value.toLowerCase();
      const ok = rows.filter((row) => row.textContent.toLowerCase().includes(q) && selects.every((s) => !s.value ||
        (s.value === '—' ? !facet(row, s.dataset.k).length : facet(row, s.dataset.k).includes(s.value))));
      const per = +size.value || ok.length || 1, pages = Math.max(1, Math.ceil(ok.length / per));
      page = Math.min(page, pages - 1);
      const shown = new Set(ok.slice(page * per, (page + 1) * per));
      for (const row of rows) row.hidden = !shown.has(row);
      where.textContent = `${ok.length ? page * per + 1 : 0}–${Math.min((page + 1) * per, ok.length)} of ${ok.length}${ok.length < rows.length ? ` (${rows.length} in all)` : ''}`;
      prev.disabled = page === 0;
      next.disabled = page >= pages - 1;
    };
    bar.oninput = () => { page = 0; apply(); };
    size.onchange = () => { saved['page size'] = +size.value; keep(stored, saved); page = 0; apply(); };
    prev.onclick = () => { page -= 1; apply(); };
    next.onclick = () => { page += 1; apply(); };
    table.satexFilter = (k, value) => {
      const select = selects.find((s) => s.dataset.k === k);
      if (select) select.value = value;
      page = 0;
      apply();
    };
    for (const [i, th] of heads.entries()) {
      th.classList.add('sortable');
      th.tabIndex = 0;
      th.onclick = th.onkeydown = (e) => {
        if (e.key && e.key !== 'Enter') return;
        const dir = th.getAttribute('aria-sort') === 'ascending' ? -1 : 1;
        for (const h of heads) h.removeAttribute('aria-sort');
        th.setAttribute('aria-sort', dir > 0 ? 'ascending' : 'descending');
        rows.sort((a, b) => dir * key(a.cells[i]).localeCompare(key(b.cells[i]), undefined, { numeric: true }));
        body.append(...rows);
        apply();
      };
    }
    apply();
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

  // The landing page: the claim turns through what a run knows; tab rows show one case at a time.
  const knows = [...document.querySelectorAll('[data-knows]')];
  const dots = document.querySelector('.claim-dots');
  if (knows.length > 1 && dots) {
    // typed out and erased like an editor, code parts marked as a linter would once a claim stands
    const parts = knows.map((k) => [...k.childNodes].map((n) => ({ text: n.textContent, code: n.nodeName === 'CODE' })));
    const typed = (i, n) => {
      let left = n;
      return parts[i].map(({ text, code }) => {
        const shown = text.slice(0, Math.max(0, left));
        left -= text.length;
        return shown && (code ? `<code>${esc(shown)}</code>` : esc(shown));
      }).join('');
    };
    const length = (i) => parts[i].reduce((sum, p) => sum + p.text.length, 0);
    const line = knows[0];
    knows.slice(1).forEach((k) => k.remove());
    line.classList.add('typing');
    let shown = 0, paused = false, timer = 0;
    dots.innerHTML = knows.map((k, i) => `<button type="button" role="tab" aria-selected="${!i}" aria-label="${parts[i].map((p) => p.text).join('')}"></button>`).join('');
    const run = (i, n, step, then) => {
      line.innerHTML = typed(i, n);
      if (step > 0 ? n >= length(i) : n <= 0) return then();
      timer = setTimeout(() => run(i, n + step, step, then), step > 0 ? 28 + Math.random() * 40 : 12);
    };
    const settle = () => { line.classList.add('linted'); timer = setTimeout(() => paused || show((shown + 1) % knows.length), 3200); };
    const show = (i) => {
      clearTimeout(timer);
      line.classList.remove('linted');
      [...dots.children].forEach((d, j) => d.setAttribute('aria-selected', String(j === i)));
      run(shown, length(shown), -1, () => { shown = i; run(i, 0, 1, settle); });
    };
    dots.onclick = (e) => { const b = e.target.closest('button'); if (b) show([...dots.children].indexOf(b)); };
    const claim = line.closest('.claim');
    for (const el of [claim, dots]) { el.onmouseenter = () => (paused = true); el.onmouseleave = () => { paused = false; if (line.classList.contains('linted')) settle(); }; }
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) line.innerHTML = typed(0, length(0));
    else settle();
  }
  for (const [tabs, key] of [['.contrast', 'case'], ['.install-tabs', 'way']]) {
    const row = document.querySelector(tabs);
    row?.addEventListener('click', (e) => {
      const b = e.target.closest('button');
      if (!b) return;
      for (const t of row.querySelectorAll('button')) t.setAttribute('aria-selected', String(t === b));
      for (const c of row.parentElement.querySelectorAll(`[data-${key}]:not(button)`)) c.hidden = c.dataset[key] !== b.dataset[key];
    });
  }

  // "category 12" on a documentation page links to that code's row
  const catcodes = document.querySelector('a.brand')?.getAttribute('href')?.replace(/index\.html$/, '') ?? '';
  for (const root of document.querySelectorAll('body.doc main')) {
    const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, { acceptNode: (n) => n.parentElement.closest('a, pre, code, h1, h2, h3') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT });
    const found = [];
    while (walk.nextNode()) if (/\bcategor(?:y|ies) \d/.test(walk.currentNode.data)) found.push(walk.currentNode);
    for (const n of found) {
      const span = document.createElement('span');
      span.innerHTML = esc(n.data).replace(/\b(categor(?:y|ies) )(1[0-5]|\d)\b/g, (m, w, k) => `${w}<a class="catcode" href="${catcodes}wiki/domains.html#catcode-${k}">${k}</a>`);
      n.replaceWith(...span.childNodes);
    }
  }

  // The headings of a documentation page, the one in view marked; a heading
  // in a folded group (the lint rules' categories) comes under the group's name.
  const toc = document.getElementById('toc');
  const heads = [...document.querySelectorAll('body.doc main :is(h2, h3)[id]')];
  if (!toc || heads.length < 2) return toc?.remove();
  // A folded group (the lint rules' categories) is a link that filters the rule table to it,
  // and unfolds to list its headings.
  let group = null;
  const blocks = heads.map((h) => {
    const g = h.closest('details')?.querySelector('summary')?.textContent ?? null;
    const name = g?.split(/\s+[(—-]/)[0] ?? '';
    const open = g && g !== group ? `<div class="grp-block"><span class="grp-row"><a class="grp" href="#rules" data-cat="${esc(name)}">${esc(g)}</a><button type="button" class="grp-more" aria-expanded="false" title="list its rules">▸</button></span><div class="grp-items" hidden>` : '';
    const close = group && g !== group ? '</div></div>' : '';
    group = g;
    // the heading's name, not its gloss: "Meaning sets: what … may mean" lists as "Meaning sets"
    const text = [...h.childNodes].filter((n) => !n.classList?.contains('self')).map((n) => n.textContent).join('').trim(), short = text.split(': ')[0];
    return `${close}${open}<a class="${h.localName}" href="#${h.id}" title="${esc(text)}">${esc(short)}</a>`;
  });
  toc.innerHTML = '<b>On this page</b>' + blocks.join('') + (group ? '</div></div>' : '');
  toc.addEventListener('click', (e) => {
    const more = e.target.closest('.grp-more');
    if (more) {
      const items = more.closest('.grp-block').querySelector('.grp-items');
      items.hidden = !items.hidden;
      more.setAttribute('aria-expanded', String(!items.hidden));
      more.textContent = items.hidden ? '▸' : '▾';
      return;
    }
    const grp = e.target.closest('a.grp');
    document.querySelector('main table.long')?.satexFilter?.('cat', grp?.dataset.cat ?? '');
  });
  const links = new Map([...toc.querySelectorAll('a.h2, a.h3')].map((a, i) => [heads[i], a]));
  const seen = new Set();
  const observer = new IntersectionObserver((entries) => {
    for (const e of entries) e.isIntersecting ? seen.add(e.target) : seen.delete(e.target);
    const first = heads.find((h) => seen.has(h));
    if (!first) return;
    for (const a of links.values()) a.classList.remove('on');
    links.get(first).classList.add('on');
    const top = first.localName === 'h2' ? first : heads.slice(0, heads.indexOf(first)).findLast((h) => h.localName === 'h2');
    let open = false;
    for (const h of heads) {
      if (h.localName === 'h2') open = h === top;
      else links.get(h).classList.toggle('open', open);
    }
  }, { rootMargin: '0px 0px -70% 0px' });
  heads.forEach((h) => observer.observe(h));
})();
