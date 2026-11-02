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
      [t.number, 'num'], [t.string, 'str'], [t.definition(t.variableName), 'def'], [t.special(t.variableName), 'arg'], [t.variableName, 'arg'], [t.operator, 'br'], [t.meta, 'magic'], [t.attributeValue, 'magicv']].map(([tag, c]) => ({ tag, class: `tok-${c}` })));
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
    // a `%\satex@assert{…}` directive is a keyword with TeX after it, not a comment
    // a magic comment's key (`% !TeX program =`, `% arara:`) and its value, apart from a plain comment
    const MAGIC = /^%\s*(?:!T[eE]X\s+(?:TS-)?[\w-]+\s*=?|arara:)/;
    // a verbatim body (lstlisting, minted, verbatim) is code in the language it names, not TeX:
    // its comments, strings and numbers by that language's comment marker, TeX by TeX's own rules
    const VERBATIM = /\\begin\{(lstlisting|verbatim\*?|Verbatim|BVerbatim|minted|comment)\}(?:\[([^\]]*)\])?(?:\{([^}]*)\})?/;
    const COMMENT = [[/^(python|ruby|r|perl|bash|sh|make|yaml|julia)$/i, '#'], [/^(tex|latex|matlab|octave|erlang|prolog)$/i, '%'], [/^(lua|sql|haskell|ada)$/i, '--'], [/./, '//']];
    const code = (s, lang) => {
      const mark = lang ? COMMENT.find(([re]) => re.test(lang))[1] : null;
      if (mark && s.match(mark)) { s.skipToEnd(); return 'comment'; }
      if (s.match(/^"(?:[^"\\]|\\.)*"?/) || s.match(/^'(?:[^'\\]|\\.)*'?/)) return 'string';
      if (s.match(/^\d[\d._]*(?:e[+-]?\d+)?/i)) return 'number';
      if (s.match(/^[A-Za-z_]\w*(?=\s*\()/)) return 'variableName.function';
      if (s.match(/^[A-Za-z_]\w*/)) return lang ? 'variableName' : null;
      s.next();
      return null;
    };
    const tex = {
      ...stex,
      startState: () => ({ ...stex.startState(), magic: false, verb: null, opening: null }),
      copyState: (st) => ({ ...stex.copyState(st), magic: st.magic, verb: st.verb, opening: st.opening }),
      token: (s, st) => {
        if (s.sol()) { st.magic = false; if (st.opening) [st.verb, st.opening] = [st.opening, null]; }
        if (st.verb) {
          if (s.sol() && s.match(new RegExp(`^\\s*\\\\end\\{${st.verb.env.replace('*', '\\*')}\\}`), false)) st.verb = null;
          else if (!/^(tex|latex)$/i.test(st.verb.lang ?? '')) return code(s, st.verb.lang);
        }
        if (!st.verb && s.sol()) {
          const open = s.string.match(VERBATIM);
          if (open) st.opening = { env: open[1], lang: open[1] === 'minted' ? open[3] : open[2]?.match(/language\s*=\s*(?:\[[^\]]*\])?\s*(\w+)/)?.[1] ?? null };
        }
        if (st.magic) { s.skipToEnd(); return 'attributeValue'; }
        if (s.sol() && s.match(MAGIC)) { st.magic = true; return 'meta'; }
        if (s.match(/^%\s*\\satex@[A-Za-z]+/)) return 'keyword';
        return s.match(/^\\[^A-Za-z@\xc0-\uffff]/) ? 'tag' : stex.token(s, st);
      },
    };
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
  // a code span between double backticks may hold one (`` `* ``)
  const inline = (t) => esc(t).split(/(``.+?``|`[^`]+`)/).map((p, i) => {
    if (!(i % 2)) return p.replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>').replace(/\*\*([^*]+)\*\*/g, '<b>$1</b>')
      .replace(/\*\(([^*]+)\)\*/g, '<span class="x-from">($1)</span>').replace(/\*([^*\s][^*]*)\*/g, '<em>$1</em>');
    const code = p.startsWith('``') ? p.slice(2, -2).trim() : p.slice(1, -1), at = code.match(/^([\w./-]+\.\w+):(\d+)$/);
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
  // a type with a card of its own is underlined as more to see; one the lattice lacks has only a note
  const tyLink = (t) => `<a class="ty${TYPES().has(t) ? '' : ' ty-plain'}" target="_blank" rel="noopener" href="${document.querySelector('a.brand')?.getAttribute('href')?.replace(/index\.html$/, '') ?? ''}wiki/domains.html#argument-types-the-ty-lattice" data-ty="${esc(t)}" data-fam="${esc(TYPES().get(t)?.family ?? '')}"${TYPES().has(t) ? '' : ` title="the type ${esc(t)} in the Ty lattice"`}>${esc(t)}</a>`;
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
    // each related type a chip that opens its own card; a long list shows its first few
    const chips = (names) => names.slice(0, 6).map((n) => `<a class="ty ty-chip" href="#" data-ty="${esc(n)}">${esc(n)}</a>`).join(' ') + (names.length > 6 ? ` <span class="muted">+${names.length - 6} more</span>` : '');
    const badge = t.family === 'expl3' ? ' <span class="ty-badge">expl3</span>' : '';
    return `<div class="ty-chain"><b>${esc(name)}</b>${badge}</div><p>${esc(t.about).replace(/`([^`]+)`/g, '<code>$1</code>')}</p>` +
      (up.length > 1 ? `<div class="ty-below"><span>more general:</span> ${chips(up.slice(0, -1).reverse())}</div>` : '') +
      (below.length ? `<div class="ty-below"><span>more specific:</span> ${chips(below)}</div>` : '') +
      '<div class="ty-foot">click: the lattice in the wiki</div>';
  }
  // ------------------------------------------------------------ tooltips
  // Every tooltip of the site is one of two floating cards made here: `satexTip.card` for a
  // command's card, a finding or a name in a trace, `satexTip.note` for short text (a `title`,
  // a type, an argument chip), which may stand beside a card.
  const clamp = (x, lo, hi) => Math.min(Math.max(x, lo), hi);
  /** Whether a menu, a completion list or a modal dialog the user opened lies over `at` (an element, or a
   * rect of the editor): what is under it shows no card or note. */
  const covered = (at) => {
    const inside = (o) => at.closest && o.contains(at);
    // a modal dialog covers the whole page around it; a menu or completion list what lies under it
    const dialog = document.querySelector('dialog[open]');
    if (dialog && !inside(dialog)) return true;
    const r = at.getBoundingClientRect?.() ?? at;
    return [...document.querySelectorAll('.ctx:not([hidden]), .cm-tooltip-autocomplete')].some((o) => {
      if (inside(o)) return false;
      const b = o.getBoundingClientRect();
      return r.left < b.right && b.left < r.right && r.top < b.bottom && b.top < r.bottom;
    });
  };
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
      if (covered(at)) return hide();
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
  // One progress bar for the site: a share when the work has a total (and the time left once it is
  // known), a moving shimmer when it has none. `satexProgress.html(done)` writes one, `.set(el, done)`
  // moves one, both with the role and values a screen reader reads.
  self.satexProgress = {
    html: (done = null, label = 'progress') => `<span class="sx-progress${done == null ? ' sx-busy' : ''}" role="progressbar" aria-label="${esc(label)}" aria-valuemin="0" aria-valuemax="100"${done == null ? '' : ` aria-valuenow="${Math.round(done * 100)}"`} style="--done: ${done ?? 0}"><span class="sx-fill"></span></span>`,
    set(el, done = null) {
      el.hidden = false;
      el.classList.toggle('sx-busy', done == null);
      el.style.setProperty('--done', String(done ?? 0));
      if (done == null) el.removeAttribute('aria-valuenow'); else el.setAttribute('aria-valuenow', String(Math.round(done * 100)));
    },
    /** The time left for a share `done` reached `ms` after the start, once enough of it is done to tell. */
    left: (done, ms) => (done > 0.05 && done < 1 ? Math.max(1, Math.round((ms * (1 - done)) / done / 1000)) : null),
  };
  {
    const { note } = tips;
    // a native `title` would bring the browser's own tooltip: lent to the note while hovered
    let titled = null;
    const untitle = () => { if (titled) { titled.title = titled.dataset.title; delete titled.dataset.title; titled = null; } };
    document.addEventListener('mouseover', (e) => {
      const a = e.target.closest?.('a.ty[data-ty]'), chip = e.target.closest?.('.arg[data-tip]');
      const ty = a?.dataset.ty ?? chip?.querySelector('a.ty[data-ty]')?.dataset.ty;
      // one heading and one description: the type's card when there is one, else the chip's own note
      const card = ty ? tyCard(ty) : '', html = card || (chip ? `<div class="ty-what">${esc(chip.dataset.tip)}</div>` : '');
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
    // a related type in a type card opens its own card in place
    document.addEventListener('click', (e) => {
      const chip = e.target.closest?.('.tip-note a.ty-chip');
      if (!chip) return;
      e.preventDefault();
      note.el.innerHTML = tyCard(chip.dataset.ty);
    });
    document.addEventListener('pointerdown', (e) => { if (e.target.closest?.('.tip-note .ty-chip')) return; untitle(); note.hide(); if (!e.target.closest?.('.tip-card')) tips.card.hide(); }, true);
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
    // at a use paths reach with different definitions: each text it may hold, with the lines that define it
    if (r.one_of?.length) {
      const by = new Map();
      for (const a of r.one_of) by.set(a.body ?? '', [...(by.get(a.body ?? '') ?? []), a.line]);
      rows.push(['one of, by path', [...by].map(([b, ls]) => `${b ? `<code>${esc(b)}</code>` : '<span class="muted">(empty)</span>'} <span class="muted">(line${ls.length > 1 ? 's' : ''} ${ls.join(', ')})</span>`).join(' / ')]);
    }
    if (r.together?.length) rows.push(['defined with', `<span class="chips">${r.together.map(csChip).join('')}</span>`]);
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
  /** A record's usage line, as its cards show it: the arguments as typed chips. */
  const useOf = (r) => (r.effective && r.effective !== r.name ? `<code class="usage">${usage(r.effective, typesOf(r)).html}</code>`
    : r.signature ? signature(r.signature) : r.syntax ? `<code class="usage">${slots(esc(r.syntax))}</code>` : '');
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
      (r.does ? `<div class="x-does">${esc(r.does)}</div>` : '') + (use ? `<div class="x-use">${use}</div>` : '') +
      (r.together?.length ? `<div class="x-with muted">defined together with <span class="chips">${r.together.slice(0, 6).map(csChip).join('')}</span>${r.together.length > 6 ? ` +${r.together.length - 6}` : ''}</div>` : '') + (r.error ? `<div class="x-err">${esc(r.error)}</div>` : '') +
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
  function withKeys(marked, keys = [], fonts = [], align = [], ligatures = [], lines = []) {
    const at = [...(keys ?? []).filter((k) => k?.key != null), ...(fonts ?? []), ...(align ?? []), ...(ligatures ?? []), ...(lines ?? [])].sort((a, b) => a.at - b.at);
    if (!at.length) return marked;
    let out = '', seen = 0, i = 0, note = false;
    const mark = (k) => k.style != null ? `\u0002${k.style}\u0001${k.color ?? ''}` : k.line != null ? `\u0007${k.line}` : k.mark != null ? `\u0003${k.mark}` : k.from != null ? `\u0005${k.from}\u0001${k.to}` : `\u0001${k.kind ?? 'key'}\u0001${k.key}`;
    const put = (n) => { while (i < at.length && at[i].at <= n) out += `\uE000${mark(at[i++])}\uE001`; };
    for (const c of marked) {
      if (c === '\uE000') { put(seen); note = true; } else if (c === '\uE001') note = false;
      else { if (!note) put(seen); seen++; }
      out += c;
    }
    put(Infinity);
    return out;
  }
  /** A picture's sketch (`satex typeset` records them in `sketches`) as an inline SVG: its nodes as labeled boxes, its paths as lines with their arrow tips. */
  let sketchCount = 0;
  const SKETCH_PX = 38, SKETCH_MAX = 520, SKETCH_PAD = 8;
  function sketchSvg(sk) {
    const xs = [...sk.nodes.flatMap((n) => [n.x - n.w / 2, n.x + n.w / 2]), ...sk.edges.flatMap((e) => [e.x1, e.x2])];
    const ys = [...sk.nodes.flatMap((n) => [n.y - n.h / 2, n.y + n.h / 2]), ...sk.edges.flatMap((e) => [e.y1, e.y2])];
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    const scale = Math.min(SKETCH_PX, (SKETCH_MAX - 2 * SKETCH_PAD) / Math.max(x1 - x0, 0.5));
    const X = (x) => ((x - x0) * scale + SKETCH_PAD).toFixed(1), Y = (y) => ((y1 - y) * scale + SKETCH_PAD).toFixed(1);
    const size = (v) => (v * scale).toFixed(1), font = Math.max(7, Math.min(12, scale * 0.3)).toFixed(1);
    const id = `sk${sketchCount++}`;
    const lines = sk.edges.map((e) => `<line x1="${X(e.x1)}" y1="${Y(e.y1)}" x2="${X(e.x2)}" y2="${Y(e.y2)}"${e.arrow === 'end' || e.arrow === 'both' ? ` marker-end="url(#${id})"` : ''}${e.arrow === 'start' || e.arrow === 'both' ? ` marker-start="url(#${id})"` : ''}/>`).join('');
    const boxes = sk.nodes.map((n) => {
      const shape = n.round ? `<ellipse cx="${X(n.x)}" cy="${Y(n.y)}" rx="${size(n.w / 2)}" ry="${size(n.h / 2)}"/>` : `<rect x="${X(n.x - n.w / 2)}" y="${Y(n.y + n.h / 2)}" width="${size(n.w)}" height="${size(n.h)}" rx="2"/>`;
      const fit = Math.min(font, Math.max(5, (n.w * scale - 4) / (Math.max(n.text.length, 1) * 0.62))).toFixed(1);
      return `${shape}<text x="${X(n.x)}" y="${Y(n.y)}" font-size="${fit}">${esc(n.text)}</text>`;
    }).join('');
    const width = ((x1 - x0) * scale + 2 * SKETCH_PAD).toFixed(0), height = ((y1 - y0) * scale + 2 * SKETCH_PAD).toFixed(0);
    return `<svg class="ts-sketch" role="img" aria-label="rough sketch of a TikZ picture" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">` +
      `<defs><marker id="${id}" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L8 4L0 8z"/></marker></defs>${lines}${boxes}</svg>`;
  }
  function notation(marked, sketches = [], picture = null) {
    let drawn = 0;
    // row: false while a row waits for its first character, true if an \hline rules it, null when one is open
    let open = 0, font = '', ink = '', table = 0, row = null, lig = null, math = '';
    // a math atom: `i` an italic letter, a class digit (2 binary, 3 relation, 6 punctuation) sets its spacing
    const MATH_CLASS = { i: 'ts-mi', 2: 'ts-mbin', 3: 'ts-mrel', 6: 'ts-mpunct' };
    const mathSpan = (html) => MATH_CLASS[math] ? `<span class="${MATH_CLASS[math]}">${html}</span>` : html;
    // a ligature: the characters the font joined, then the glyph it set (`.show-ligs` animates the join)
    const ligature = (from, to) => {
      const code = [...to].length === 1 ? ` (U+${to.codePointAt(0).toString(16).toUpperCase().padStart(4, '0')})` : '';
      return `Ligature: the font sets ${[...from].join(' + ')} as one glyph, ${to}${code}, as its ligature table says`;
    };
    const joined = (part) => {
      if (!lig) return esc(part);
      const [from, to] = lig, n = [...to].length, glyph = [...part].slice(0, n).join('');
      lig = null;
      return `<span class="ts-lig" data-from="${esc(from)}" title="${esc(ligature(from, to))}"><span class="ts-lig-to">${esc(glyph)}</span></span>${esc([...part].slice(n).join(''))}`;
    };
    const opened = () => {
      if (row === null) return '';
      const tr = `<span class="ts-tr${row ? ' ts-ruled' : ''}"><span class="ts-td">`;
      row = null;
      return tr;
    };
    // a box of a set width in the line (a contents line's number, its page) is set that wide; leaders fill the rest
    let boxes = 0;
    const grid = (mark) => {
      if (mark.startsWith('box ')) { boxes++; return `<span class="ts-boxto" style="min-width: ${parseFloat(mark.slice(4))}pt">`; }
      if (mark === 'boxend') { if (!boxes) return ''; boxes--; return '</span>\u0000'; }
      if (mark === 'leaders') return '<span class="ts-leaders" aria-hidden="true"></span>';
      if (mark === 'break') return '<br>';
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
        return tr + mathSpan((font || ink) && part ? `<span${font ? ` class="${font}"` : ''}${ink ? ` style="color: ${esc(ink)}"` : ''}>${joined(part)}</span>` : joined(part));
      }
      if (note[0] === '\u0006') { math = note.slice(1); return ''; }
      if (note[0] === '\u0003') return grid(note.slice(1));
      if (note[0] === '\u0005') { lig = note.slice(1).split('\u0001'); return ''; }
      if (note === '―' && table && row !== null) { row = true; return ''; }
      if (note[0] === '\u0002') { const [style, color = ''] = note.slice(1).split('\u0001'); font = style.split(' ').filter(Boolean).map((x) => `ts-${x}`).join(' '); ink = color; return ''; }
      if (note[0] === '\u0007') return `<span class="ts-ln" title="source line ${esc(note.slice(1))}">${esc(note.slice(1))}</span>`;
      if (note === '·') return '';
      const tr = table ? opened() : '';
      if (note === '⟦') { open++; return `${tr}<span class="ts-box" title="set in a box: on the page only if the box is used">`; }
      if (note === '⟧') { if (!open) return tr; open--; return `${tr}</span>`; }
      if (note[0] === '\u0001') { const [, kind, key] = note.split('\u0001'); return `${tr}<span class="ts-key ts-key-${esc(kind)}" title="${esc(kind)}">${esc(key)}</span>`; }
      if (note[0] === '▢') { const sk = sketches?.[drawn++]; return sk ? tr + sketchSvg(sk) : `${tr}<span class="ts-note">${esc(note)}</span>`; }
      if (RULES[note]) return `${tr}<span class="${RULES[note][0]}" title="${RULES[note][1]}"></span>`;
      return `${tr}<span class="ts-note">${esc(note)}</span>`;
    }).join('').replace(/<span class="ts-boxto" style="[^"]*"><\/span>\u0000/g, '');
    // set boxes at both ends (a contents line, a label) align the line as a row
    const aligned = html.startsWith('<span class="ts-boxto"') && html.endsWith('\u0000') || html.includes('ts-leaders');
    if (picture) return sketchSvg(picture);
    return (aligned ? '<i class="ts-row" hidden></i>' : '') + html.replaceAll('\u0000', '') + '</span>'.repeat(open + 2 * table + boxes);
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
  const ruleCard = (r, href = '', own = false) => {
    const refs = (r.references ?? []).map((x) => `<a href="${esc(x.url)}" target="_blank" rel="noopener">${esc(x.cite)}</a>`).join(', ');
    const engines = (r.engines ?? []).join(', ');
    const badges = [`<code class="num" title="rule number">${esc(r.number ?? '')}</code>`,
      // a finding's own card shows its severity once, its own, not the rule's default
      ...(own ? [] : [`<span class="sev sev-${esc(r.severity)}" title="default: ${esc(r.severity)}">default: ${esc(r.severity)}</span>`]),
      `<span class="cat cat-${esc(r.category)}">${esc(r.category)}</span>`,
      ...(engines ? [`<span class="eng" title="only under ${esc(engines)}">${esc(engines)}</span>`] : []),
      ...(r.fixes ?? []).map((f) => `<span class="fix fix-${esc(f)}" title="${f === 'safe' ? 'safe: keeps the meaning, --fix applies it' : 'unsafe: may change the meaning, only --unsafe-fixes applies it'}">${f === 'safe' ? 'S' : 'U'}</span>`)];
    const name = href ? `<a href="${esc(href)}">${esc(r.code)}</a>` : esc(r.code);
    return `<section class="rule rule-tip"><h4>${name}</h4><p class="badges">${badges.join(' ')}</p>` +
      `<p><b>${inline(r.summary ?? r.message ?? '')}</b></p>${markdown(r.explanation ?? '')}${refs ? `<p>See ${refs}.</p>` : ''}</section>`;
  };
  /** A hover's Markdown; its last paragraph, the places and links, is a muted footer. */
  const unescape = (s) => s.replace(/&(lt|gt|quot|amp);/g, (m, e) => ({ lt: '<', gt: '>', quot: '"', amp: '&' })[e]);
  /** A hover's Markdown: its usage lines colored, each ⟨argN⟩ elsewhere in the kind its usage gave it. */
  const hoverCard = (value) => {
    // each definition is its own section: its types belong to its own usage line
    const section = (md) => {
      const kinds = {};
      // the argument types the section lists (`- `⟨arg1⟩` glue`, roles::Ty), shown in the usage line too
      const types = Object.fromEntries([...md.matchAll(/^- `⟨(arg\d+)⟩` ([^\n`]+)$/gm)].map((m) => [m[1], m[2]]));
      const html = markdown(md).replace(/<code>([^<]*⟨arg\d+⟩[^<]*)<\/code>/g, (m, code) => {
        if (/^⟨arg\d+⟩$/.test(code)) return m;
        const u = usage(unescape(code), types);
        Object.assign(kinds, u.kinds);
        return `<code class="usage">${u.html}</code>`;
      });
      return slots(html, kinds).replace(/<p>((?:\s|·|<a [^>]*>[^<]*<\/a>)+)<\/p>/g, '<p class="foot">$1</p>');
    };
    const sections = (md) => md.split(/^(?=### )/m).map(section).join('');
    // the definitions that do not hold at the cursor fold away (lsp::handlers::MORE_DEFINITIONS)
    const [main, more] = value.split(/^#### More definitions\n+/m);
    return sections(main) + (more ? `<details class="more"><summary>More definitions</summary>${sections(more)}</details>` : '');
  };

  self.satexCards = { explain, brief, signature, slots, usage, tex, inline, markdown, ruleCard, placeLink, upstream, csChip, csName, value, esc, notation, withKeys, hover: hoverCard, useOf };
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
  const paragraphs = (rows) => rows.map((r) => `<p class="ts${r.heading ? ` ts-h ts-${esc(r.heading)}` : ''}${r.footnote ? ' ts-foot' : ''}"${typeof r.skip === 'number' && !r.heading ? ` style="--skip: ${Math.max(r.skip, 0)}pt"` : ''}${r.env ? ` data-env="${esc(r.env)}"` : ''}><span class="ts-text">${notation(withKeys(r.marked ?? r.detail ?? '', r.keys, r.fonts, r.align, r.ligatures, r.lines), r.sketches, r.sketch)}</span></p>`).join('')
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

  // Every example is editable: a click into its source puts the playground's editor in its place
  // (tokenizer, completion and hover from the in-page engine), and each edit, debounced, shows
  // what it typesets and its findings; "reset" brings the build's result back. The last tab, "your
  // own", starts in the editor. The engine (worker and manifest from play/index.html) loads on the
  // first edit or when that tab opens, and runs each file on the kernel its magic comment names.
  const MiB = (n) => (n / 2 ** 20).toFixed(1), SEV = ['', 'error', 'warning', 'info', 'info'];
  const NAMED = /\\(?:documentclass|usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}/g;
  const PLAIN = /^%\s*!T[eE]X\s+(?:TS-)?program\s*=\s*tex\b/m;
  let engine = null, say = () => {};
  const found = new Map(); // uri -> its last diagnostics
  async function boot() {
    say('Loading SaTeX…');
    const at = new URL('play/index.html', location.href);
    const manifest = JSON.parse((await (await fetch(at)).text()).match(/<script id="satex-play" type="application\/json">(.*?)<\/script>/s)[1]);
    const worker = new Worker(URL.createObjectURL(new Blob([manifest.worker], { type: 'text/javascript' })));
    let line = Promise.resolve(), waiting = null, id = 0;
    worker.onmessage = ({ data }) => {
      if (data.type === 'progress') return say(`${self.satexProgress.html(data.total ? data.loaded / data.total : null, 'loading SaTeX')} Loading SaTeX: ${MiB(data.loaded)} of ${MiB(data.total)} MiB`);
      if (data.type === 'status') return;
      if (data.type === 'lsp') for (const m of data.out) if (m.method === 'textDocument/publishDiagnostics') found.set(m.params.uri, m.params.diagnostics);
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
    const base = new URL(manifest.root, at).href;
    await ask({ type: 'init', base, manifest });
    return { manifest, base, ask, lsp, have: new Set([manifest.kernels[0].id]), kernel: null, open: new Set() };
  }
  const start = () => (engine ??= boot().catch((e) => { say(`SaTeX did not load: ${esc(e.message)}`, true); engine = null; throw e; }));
  /** The bundles the text names (a class or package) and those they need, not loaded yet. */
  function wanted(e, kernel, text) {
    const byId = new Map((e.manifest.packages ?? []).map((p) => [p.id, p])), picked = new Map();
    const pick = (id) => { const p = byId.get(id); if (p && !picked.has(id) && !e.have.has(id)) { picked.set(id, p); p.needs.forEach((n) => pick(`${kernel.id}/${n}`)); } };
    for (const m of text.matchAll(NAMED)) for (const n of m[1].split(',').map((x) => x.trim())) { pick(`${kernel.id}/${n}`); pick(`${kernel.id}/${n}.cls`); }
    return [...picked.values()];
  }
  /** The server on `kernel`, started anew when another kernel ran last: its documents open again. */
  async function onKernel(e, kernel, name) {
    if (e.kernel === kernel.id) return;
    if (!e.have.has(kernel.id)) { say(`Loading the ${esc(kernel.label)} kernel…`); await e.ask({ type: 'bundle', base: e.base, entry: kernel }); e.have.add(kernel.id); }
    e.ask({ type: 'reset', args: [...kernel.args, '-f', name], cwd: '/work' });
    await e.lsp('initialize', { processId: null, rootUri: 'file:///work', capabilities: {} });
    await e.lsp('initialized', {}, true);
    e.kernel = kernel.id;
    e.open.clear();
  }
  /** An editor in `host` for file `name` of `fig`, starting from `first`. */
  async function liveEditor(fig, host, name, first) {
    const out = fig.querySelector('.live-ts'), status = fig.querySelector('.live-status'), uri = `file:///work/${name}`;
    let version = 0, timer = 0, view = null, underline = () => {};
    const tell = (html, bad = false) => { status.innerHTML = html; status.classList.toggle('live-bad', bad); };
    async function analyze() {
      say = tell;
      const mine = ++version, e = await start(), now = view.state.doc.toString(), t0 = performance.now();
      const kernel = PLAIN.test(now) ? e.manifest.kernels.find((k) => k.id === 'plain') : e.manifest.kernels[0];
      const more = wanted(e, kernel, now);
      if (more.length) { tell(`Loading ${esc(more.map((p) => p.label).join(', '))}…`); await e.ask({ type: 'bundles', base: e.base, entries: more }); more.forEach((p) => e.have.add(p.id)); }
      if (mine !== version) return;
      tell('Analyzing…');
      fig.classList.add('busy');
      e.ask({ type: 'project', dir: '/work', files: { [name]: new TextEncoder().encode(now) } });
      await onKernel(e, kernel, name);
      if (!e.open.has(uri)) {
        e.open.add(uri);
        await e.lsp('textDocument/didOpen', { textDocument: { uri, languageId: 'latex', version: mine, text: now } }, true);
      } else {
        await e.lsp('textDocument/didChange', { textDocument: { uri, version: mine }, contentChanges: [{ text: now }] }, true);
      }
      await e.lsp('satex/analyze', {}, true);
      if (mine !== version) return; // a newer text is on its way
      const rows = (await e.lsp('satex/typeset', { textDocument: { uri } })) ?? [];
      if (mine !== version) return;
      fig.classList.remove('busy');
      out.innerHTML = paragraphs(rows.filter((r) => r.kind === 'paragraph' && !String(r.path ?? '').startsWith('/texmf')));
      underline(found.get(uri) ?? []);
      tell(`Analyzed in ${Math.round(performance.now() - t0)} ms. <a href="play/index.html">The playground</a> has traces, the library and more.`);
    }
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
    view = new EditorView({
      parent: host, doc: first,
      extensions: [basicSetup, EditorView.lineWrapping, language.syntaxHighlighting(tex.highlighter), tex.language, lint.linter(null), lint.lintGutter(), ac.autocompletion({ override: [complete] }), theme.of(dark() ? oneDark : []),
        EditorView.updateListener.of((u) => { if (u.docChanged) { clearTimeout(timer); timer = setTimeout(analyze, 350); } })],
    });
    new MutationObserver(() => view.dispatch({ effects: theme.reconfigure(dark() ? oneDark : []) })).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    underline = (ds) => view.dispatch(lint.setDiagnostics(view.state, ds.map((d) => {
      const from = off(view.state.doc, d.range.start), to = off(view.state.doc, d.range.end);
      return { from, to: Math.min(Math.max(to, from + 1), view.state.doc.length), severity: SEV[d.severity] || 'info', message: d.message, source: d.data?.rule ?? d.code };
    })));
    // hover: the language server's card, as in the playground
    let shown = null;
    host.addEventListener('mousemove', (ev) => {
      const p = view.posAtCoords({ x: ev.clientX, y: ev.clientY });
      if (p == null || !engine) return;
      clearTimeout(shown);
      shown = setTimeout(async () => {
        const e = await engine, r = await e.lsp('textDocument/hover', { textDocument: { uri }, position: pos(view.state.doc, p) }).catch(() => null);
        const c = r?.contents, value = typeof c === 'string' ? c : c?.value ?? '';
        const at = view.coordsAtPos(p);
        if (value && at && host.matches(':hover')) card.show({ left: at.left, top: at.top, right: at.right, bottom: at.bottom }, `<div class="hover">${hoverCard(value)}</div>`);
      }, 300);
    });
    host.addEventListener('mouseleave', () => { clearTimeout(shown); card.hide(150); });
    return { view, analyze, destroy: () => { clearTimeout(timer); version++; view.destroy(); } };
  }
  // an example: its source becomes an editor on a click that is no command's, "reset" brings the build's result back
  for (const fig of document.querySelectorAll('.example:not(.live)')) {
    const data = JSON.parse(fig.querySelector('.ex-data').textContent), pre = fig.querySelector('pre.src'), reset = fig.querySelector('.ex-reset');
    if (!reset) continue;
    const note = fig.querySelector('.live-status').innerHTML, side = fig.querySelector('.ex-side');
    let live = null;
    /** The line and column of the static source a click landed on: past a line's text, its end. */
    const placeOf = (e) => {
      const caret = document.caretPositionFromPoint?.(e.clientX, e.clientY) ?? (() => { const r = document.caretRangeFromPoint?.(e.clientX, e.clientY); return r && { offsetNode: r.startContainer, offset: r.startOffset }; })();
      const lines = [...pre.querySelectorAll('.ln')];
      const ln = caret?.offsetNode?.parentElement?.closest('.ln') ?? lines.find((l) => { const r = l.getBoundingClientRect(); return r.top <= e.clientY && e.clientY < r.bottom; });
      if (!ln) return null;
      const line = +ln.dataset.l - 1, length = data.source.split('\n')[line]?.length ?? 0;
      if (!ln.contains(caret?.offsetNode)) return { line, col: length };
      const span = document.createRange();
      span.setStart(ln, 0);
      span.setEnd(caret.offsetNode, caret.offset);
      return { line, col: Math.min(span.toString().length, length) };
    };
    const slide = (ms) => new Promise((done) => setTimeout(done, ms));
    pre.addEventListener('click', async (e) => {
      if (live || e.target.closest('[data-cs], [data-lint]') || !data.source) return;
      const place = placeOf(e);
      // the editor builds beside the source, unseen, at its size; the source then makes room for the gutter and the editor fades in
      const host = Object.assign(document.createElement('div'), { className: 'src live-src' });
      Object.assign(host.style, { position: 'absolute', visibility: 'hidden', left: `${pre.offsetLeft}px`, top: `${pre.offsetTop}px`, width: `${pre.offsetWidth}px`, minHeight: `${pre.offsetHeight}px` });
      pre.after(host);
      card.hide();
      live = await liveEditor(fig, host, data.name, data.source);
      const gutter = host.querySelector('.cm-gutters')?.offsetWidth ?? 0;
      pre.style.transition = 'padding-left .15s var(--ease)';
      pre.style.paddingLeft = `calc(${getComputedStyle(pre).paddingLeft} + ${gutter}px)`;
      await slide(160);
      host.removeAttribute('style');
      host.style.minHeight = `${pre.offsetHeight}px`;
      pre.replaceWith(host);
      host.animate([{ opacity: 0 }, { opacity: 1 }], 140);
      reset.hidden = false;
      const doc = live.view.state.doc;
      if (place) live.view.dispatch({ selection: { anchor: doc.line(Math.min(place.line + 1, doc.lines)).from + place.col } });
      live.view.focus();
    });
    // an edited example opens in the playground as it stands: its text travels in a share link, the playground's own format
    fig.querySelector('.ex-acts a').addEventListener('click', async (e) => {
      const text = live?.view.state.doc.toString();
      if (text == null || text === data.source) return;
      e.preventDefault();
      const base = e.currentTarget.href.split('#')[0];
      const packed = new Uint8Array(await new Response(new Blob([new TextEncoder().encode(JSON.stringify({ x: data.name, f: data.name, p: { [data.name]: text } }))]).stream().pipeThrough(new CompressionStream('deflate-raw'))).arrayBuffer());
      const bits = [...packed].map((b) => String.fromCharCode(b)).join('');
      location.href = `${base}#s=${btoa(bits).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')}`;
    });
    reset.addEventListener('click', () => {
      if (!live) return;
      const host = fig.querySelector('.live-src');
      live.destroy();
      live = null;
      host.replaceWith(pre);
      pre.style.paddingLeft = '';
      side.innerHTML = paragraphs(data.ts ?? []);
      fig.querySelector('.live-status').innerHTML = note;
      reset.hidden = true;
    });
  }
  const own = document.querySelector('.example.live');
  if (own) {
    const FIRST = '\\documentclass{article}\n\\newcommand{\\greet}[1]{Hello, \\emph{#1}!}\n\\begin{document}\n\\section{Welcome}\n\\greet{world} Pages 10-12 come next.\nSee Section~\\ref{sec:none}.\n\\end{document}\n';
    let live = null;
    const open = async () => { if (!live) { live = liveEditor(own, own.querySelector('.live-src'), 'try.tex', FIRST); (await live).analyze(); } };
    document.getElementById(own.getAttribute('aria-labelledby'))?.addEventListener('click', open);
  }

  // Long tables: a header click sorts, a text box and one select per badge kind filter, pages
  // of a chosen size, and a columns menu on the header that hides any column.
  const RANK = { error: 0, warning: 1, info: 2, hint: 3 };
  const FACETS = { sev: 'severity', cat: 'category', eng: 'engine', fix: 'quick fix', from: 'origin' };
  const facet = (row, k) => [...row.querySelectorAll('.' + k)].flatMap((e) => k === 'sev' ? [e.title.split(' ').pop()]
    : k === 'eng' ? e.textContent.split(', ') : k === 'fix' ? [e.className.replace(/.*fix-/, '')] : [e.textContent.split(' ')[0]]);
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
      select.innerHTML = `<option value="">${label}</option>` + (k === 'sev' || k === 'cat' ? '' : '<option value="—">none</option>') +
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
    // the engine and origin columns start hidden: the summary gets their room
    names.forEach((name, i) => toggle(name, saved[name] ?? !['engine', 'origin'].includes(name), (on) => { for (const row of table.rows) if (row.cells[i]) row.cells[i].hidden = !on; }));
    toggle('wrap text', saved['wrap text'] ?? false, (on) => table.classList.toggle('wrap', on));
    menu.onclick = menu.onkeydown = (e) => e.stopPropagation();
    document.addEventListener('click', (e) => { if (!menu.contains(e.target)) menu.open = false; });
    bar.append(menu);
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
  document.addEventListener('click', (e) => {
    const d = e.target.closest('main details[id^=category-] > summary')?.parentElement;
    if (d) setTimeout(() => { if (d.open) { history.replaceState(null, '', '#' + d.id); d.scrollIntoView({ behavior: 'smooth' }); } });
  });
  unfold();

  // The landing page: the claim turns through what a run knows; tab rows show one case at a time.
  const knows = [...document.querySelectorAll('[data-knows]')];
  const dots = document.querySelector('.claim-dots');
  if (knows.length > 1 && dots) {
    // typed out and erased like an editor; once a claim stands it shows what the run computes for it: its
    // `<template>` as a sketch under it, or its `data-effect` on its own words (completion, category codes, ligatures)
    const mocks = knows.map((k) => k.querySelector('template')?.innerHTML ?? null), effects = knows.map((k) => k.dataset.effect ?? '');
    const parts = knows.map((k) => [...k.childNodes].filter((n) => n.nodeName !== 'TEMPLATE')
      .map((n) => ({ text: n.textContent, code: n.nodeName === 'CODE', lint: n.nodeName === 'U', fix: n.dataset?.fix ?? null, href: n.nodeName === 'A' ? n.getAttribute('href') : null })));
    // TikZ is set as its logo is, with an italic k
    const tikz = (html) => html.replace(/Tik(?=Z|$)/g, 'Ti<i class="tk">k</i>');
    const typed = (i, n) => {
      let left = n;
      // a `<u>` is a spot a linter would mark: with `data-fix` it holds the wrong text and the right one, the first turning into the second
      return parts[i].map(({ text, code, lint, fix, href }) => {
        const shown = text.slice(0, Math.max(0, left));
        left -= text.length;
        if (!shown) return '';
        if (lint) return `<u class="claim-fix"><span>${esc(shown)}</span><span>${esc(shown === text ? fix ?? '' : '')}</span></u>`;
        return code ? `<code>${tikz(esc(shown))}</code>` : href ? `<a href="${esc(href)}">${tikz(esc(shown))}</a>` : tikz(esc(shown));
      }).join('');
    };
    const length = (i) => parts[i].reduce((sum, p) => sum + p.text.length, 0);
    const line = knows[0];
    knows.slice(1).forEach((k) => k.remove());
    line.classList.add('typing');
    let shown = 0, paused = false, timer = 0;
    dots.innerHTML = knows.map((k, i) => `<button type="button" role="tab" aria-selected="${!i}" aria-label="${parts[i].map((p) => p.text).join('')}"></button>`).join('');
    // a typist's rhythm: quick within a word, a beat after a space or a comma
    const text = (i) => parts[i].map((p) => p.text).join('');
    const run = (i, n, then) => {
      line.innerHTML = typed(i, n);
      if (n >= length(i)) return then();
      const c = text(i)[n - 1] ?? '';
      timer = setTimeout(() => run(i, n + 1, then), 18 + Math.random() * 28 + (c === ' ' ? 45 : c === ',' ? 160 : 0));
    };
    const erase = (then) => { line.innerHTML = ''; then(); };
    // a typed `\command` opens a sketch of the editor's completion: the examples' own commands, as their cards write them
    const own = [...document.querySelectorAll('.ex-data')].flatMap((s) => { try { const d = JSON.parse(s.textContent); return Object.values(d.cs).flat().filter((r) => r.path === d.name && r.effective && r.effective !== r.name); } catch { return []; } });
    const offered = [...new Map(own.map((r) => [r.name, r])).values()].slice(0, 3);
    // TeX's category codes of what a name is spelled with once `@` is a letter
    const CODES = { '\\': 0, '{': 1, '}': 2, '$': 3, '&': 4, '#': 6, '^': 7, '_': 8, ' ': 10, '%': 14 };
    const catcode = (c) => CODES[c] ?? (/[A-Za-z@]/.test(c) ? 11 : 12);
    /** Every character of the line with its category code under it, a word kept whole; the odd codes stand out. */
    const annotate = () => {
      const mark = (c) => `<span class="cc${[11, 12].includes(catcode(c)) ? '' : ' cc-hot'}" title="category ${catcode(c)}">${esc(c)}<sub>${catcode(c)}</sub></span>`;
      const word = (w) => `<span class="cc-w">${[...w].map(mark).join('')}</span>`;
      const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
      const texts = [];
      while (walker.nextNode()) texts.push(walker.currentNode);
      for (const t of texts) {
        const html = t.parentElement.tagName === 'CODE' ? word(t.data) : t.data.split(/( +)/).map((w, i) => (i % 2 ? ' ' : word(w))).join('');
        t.replaceWith(Object.assign(document.createElement('span'), { innerHTML: html }));
      }
      let n = 0;
      for (const c of line.querySelectorAll('.cc')) c.style.setProperty('--i', String(n++));
    };
    // each claim acts out its words in a few classes: the pip is the shared mark for "noticed", the rest is CSS
    const tag = (cls, mark) => (w) => `<span class="claim-tag ${cls}" data-tag="${mark}">${w}</span>`;
    const arm = (cls) => `<span class="claim-arm ${cls}" aria-hidden="true">branches</span>`;
    const peek = '<span class="claim-peek" aria-hidden="true"><code>Ti<i class="tk">k</i>Zpingus</code><span>written by the author of SaTeX</span></span>';
    const WRAPS = {
      peek: [[/(<a [^>]*>Ti<i class="tk">k<\/i>Zpingus<\/a>)/, `<span class="claim-host">$1${peek}</span>`]],
      branch: [[/branches/, `<span class="claim-fork">${arm('up')}<span class="claim-base">branches</span>${arm('down')}</span>`]],
      options: [[/options/, '<span class="claim-opt">options</span>']],
      runs: [[/runs/, tag('claim-count', '')]],
      diff: [[/yesterday/, tag('claim-ok', '✓')], [/today/, tag('claim-bad', '✗')]],
      ligatures: [[/fi(?=ne)/, '<span class="claim-lig" title="f and i set as one glyph, ﬁ"><span class="lig-f">f</span><span class="lig-i">i</span><span class="lig-fi">ﬁ</span></span>']],
    };
    // `satex trace` of `\def\twice#1{#1#1}\twice{ab}`: expand \twice, then its replacement `#1#1` with #1 = ab, then the tokens it hands on
    const STEPS = [['expand', ['\\twice', '{ab}']], ['replace', ['#1', '#1']], ['read', ['a', 'b', 'a', 'b']]];
    const stepper = () => {
      let at = 0;
      const rows = STEPS.map(([step, tokens]) => `<li style="--r:${at}"><span class="st-step">${step}</span>${tokens.map((t) => `<code class="st-tk" style="--i:${at++}">${esc(t)}</code>`).join('')}</li>`).join('');
      return `<ol class="claim-steps" aria-hidden="true"><li class="st-head"><span>satex trace</span><span>#1 = ab</span></li>${rows}</ol>`;
    };
    // the drawing pad's answer: a stroke draws itself, then the symbol it matches and its command; the card links to /draw
    const sketchPad = () => '<a class="claim-draw" href="draw/index.html" tabindex="-1" aria-hidden="true"><svg viewBox="0 0 24 24"><path pathLength="1" d="M8 22V9C8 3 20 3 20 10c0 6-9 6-12 2"/></svg><span class="dr-sym">ρ</span><code class="dr-cmd">\\rho</code></a>';
    const effect = () => {
      if (effects[shown] === 'draw') { line.insertAdjacentHTML('beforeend', sketchPad()); return true; }
      if (effects[shown] === 'stepper') { line.insertAdjacentHTML('beforeend', stepper()); return true; }
      const chip = line.querySelector('code');
      if (effects[shown] === 'catcodes') { annotate(); return true; }
      if (WRAPS[effects[shown]]) { for (const [re, wrap] of WRAPS[effects[shown]]) line.innerHTML = line.innerHTML.replace(re, wrap); return true; }
      const mock = effects[shown] === 'complete' && offered.length
        ? offered.map((r) => `<li><code class="usage">${usage(r.effective, typesOf(r)).html}</code><span>${esc(r.file ?? r.path ?? '')}</span></li>`).join('') : mocks[shown];
      if (!mock || !chip) return false;
      line.insertAdjacentHTML('beforeend', `<ul class="claim-complete" aria-hidden="true" style="margin-left:${chip.offsetLeft}px">${mock}</ul>`);
      return true;
    };
    // appear (typed), act (SHOWS_AFTER), settle (the hold): the same beat for every claim
    const SHOWS_AFTER = 700, HOLD = 3400;
    const fits = () => { for (const u of line.querySelectorAll('.claim-fix')) { const [from, to] = [...u.children].map((s) => s.getBoundingClientRect().width); u.style.setProperty('--from', `${from}px`); u.style.setProperty('--to', `${to}px`); } };
    const settle = () => {
      timer = setTimeout(() => {
        fits();
        if (effect() || line.querySelector('.claim-fix')) requestAnimationFrame(() => line.classList.add('shows'));
        timer = setTimeout(() => paused || show((shown + 1) % knows.length), HOLD);
      }, SHOWS_AFTER);
    };
    const show = (i) => {
      clearTimeout(timer);
      line.classList.remove('shows');
      [...dots.children].forEach((d, j) => d.setAttribute('aria-selected', String(j === i)));
      erase(() => { shown = i; run(i, 0, settle); });
    };
    dots.onclick = (e) => { const b = e.target.closest('button'); if (b) show([...dots.children].indexOf(b)); };
    const claim = line.closest('.claim');
    for (const el of [claim, dots]) { el.onmouseenter = () => (paused = true); el.onmouseleave = () => { paused = false; if (line.classList.contains('shows')) { clearTimeout(timer); timer = setTimeout(() => paused || show((shown + 1) % knows.length), 1500); } }; }
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) { line.innerHTML = typed(0, length(0)); line.classList.add('shows'); }
    else { line.innerHTML = ''; run(0, 0, settle); }
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
    const open = g && g !== group ? `<div class="grp-block"><span class="grp-row"><a class="grp" href="#category-${esc(name.toLowerCase())}" data-cat="${esc(name)}">${esc(g)}</a><button type="button" class="grp-more" aria-expanded="false" title="list its rules">▸</button></span><div class="grp-items" hidden>` : '';
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
