// The playground's page side: a CodeMirror editor, satex's language server for
// findings, fixes, completion, hover, definitions and rename, and the `satex`
// command line for runs. satex itself runs in worker.js. Without the CDN the
// editor is a textarea. A classic script, so the page also works from disk.
(() => {
  const $ = (id) => document.getElementById(id);
  const manifest = JSON.parse($('satex-play').textContent);
  const base = new URL(manifest.root, document.baseURI).href;
  const [tabs, args, button, status, output, lspStatus, list, count, cmd, kernelBox, rootBox, toastBox, ctxMenu] =
    ['play-tabs', 'play-args', 'play-run', 'play-status', 'play-output', 'lsp-status', 'lsp-diagnostics', 'lsp-count',
      'play-cmd', 'play-kernel', 'play-root', 'play-toast', 'play-ctx'].map($);
  {
    const divider = $('play-divider'), split = divider.parentElement;
    const set = (px) => {
      const w = split.getBoundingClientRect().width;
      const clamped = Math.min(Math.max(px, 240), w - 240);
      split.style.setProperty('--split', `${clamped}px`);
      try { localStorage.setItem('satex-split', String(clamped / w)); } catch {}
    };
    try { const f = +localStorage.getItem('satex-split'); if (f > 0 && f < 1) split.style.setProperty('--split', `${f * 100}%`); } catch {}
    divider.addEventListener('pointerdown', (e) => {
      divider.setPointerCapture(e.pointerId);
      divider.classList.add('drag');
      const left = split.getBoundingClientRect().left;
      const move = (m) => set(m.clientX - left);
      const up = () => { divider.classList.remove('drag'); divider.removeEventListener('pointermove', move); divider.removeEventListener('pointerup', up); };
      divider.addEventListener('pointermove', move);
      divider.addEventListener('pointerup', up);
    });
    divider.addEventListener('keydown', (e) => {
      const step = { ArrowLeft: -32, ArrowRight: 32 }[e.key];
      if (step) { e.preventDefault(); set(split.firstElementChild.getBoundingClientRect().width + step); }
    });
    divider.addEventListener('dblclick', () => { split.style.removeProperty('--split'); try { localStorage.removeItem('satex-split'); } catch {} });
  }
  // The worker's code comes with the page: file:// pages cannot start a worker from a file.
  const worker = new Worker(URL.createObjectURL(new Blob([manifest.worker], { type: 'text/javascript' })));
  const escape = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  const SEVERITY = ['', 'error', 'warning', 'info', 'hint'];
  const sev = (d) => SEVERITY[d.severity] || 'info';
  const secs = (ms) => `${(ms / 1000).toFixed(2)} s`;
  const size = (n) => n < 2 ** 20 ? `${Math.ceil(n / 1024)} KiB` : `${(n / 2 ** 20).toFixed(1)} MiB`;
  const started = performance.now();

  // The project: its directory, its files' texts, the open file and the root a run reads.
  // The samples are one project whose every file is its own root; an upload has one root.
  // Read-only files (`ro`) are kernel or package files opened by a definition: never sent or run.
  const state = { dir: '/work', texts: {}, original: {}, file: manifest.sample, root: null, kernel: 0, loaded: new Set([0]), upload: false, ro: {} };
  const path = (name) => `${state.dir}/${name}`;
  const uri = (name = state.file) => `file://${path(name)}`;

  let toastTimer;
  function toast(text) {
    toastBox.textContent = text;
    toastBox.classList.add('on');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toastBox.classList.remove('on'), 2600);
  }
  const timing = (id, text, title = '') => Object.assign($(id), { textContent: text, title });

  /** SGR codes to the classes build.py's ansi() writes; OSC hyperlinks and cursor moves dropped. */
  function ansi(text) {
    text = text.replace(/\x1b\]8;[^\x1b\x07]*(\x1b\\|\x07)/g, '').replace(/\x1b\[[0-9;]*[A-HJKST]/g, '');
    let classes = new Set(), html = '';
    for (const part of text.split(/(\x1b\[[0-9;]*m)/)) {
      const m = part.match(/^\x1b\[([0-9;]*)m$/);
      if (!m) { html += escape(part); continue; }
      if (classes.size) html += '</span>';
      for (const code of m[1].split(';').map(Number)) {
        const color = (code >= 30 && code <= 39) || (code >= 90 && code <= 97);
        if (code === 0) classes.clear();
        else if (code === 22) { classes.delete('b'); classes.delete('d'); }
        else if (color) for (const c of classes) if (c[0] === 'c') classes.delete(c);
        const add = { 1: 'b', 2: 'd', 3: 'i', 4: 'u' }[code] ?? (color && code % 10 !== 9 ? `c${code % 10}` : null);
        if (add) classes.add(add);
      }
      if (classes.size) html += `<span class="${[...classes].sort().join(' ')}">`;
    }
    return html + (classes.size ? '</span>' : '');
  }
  const plain = (text) => text.replace(/\x1b\[[0-9;]*[A-Za-z]|\x1b\]8;[^\x1b\x07]*(\x1b\\|\x07)/g, '');
  /** stderr without the progress bars, which redraw themselves in place. */
  const quiet = (text) => text.replace(/\x1b\[1A/g, '').split('\x1b[J')
    .filter((piece) => !/\[[#-]{20}\]/.test(plain(piece))).join('').trim();
  const words = (line) => (line.match(/"[^"]*"|'[^']*'|\S+/g) || []).map((w) => w.replace(/^(["'])(.*)\1$/, '$2'));

  /** The Markdown satex writes, for hovers and rule explanations: headings, paragraphs, lists,
   * code, links, emphasis. A `file:line` in code is a place that opens (site.js). */
  const cards = self.satexCards;
  const inline = (t) => cards.inline(t).replace(/(^|[\s(])\*([^*\s][^*]*)\*/g, '$1<i>$2</i>');
  const markdown = (text) => text.split(/^```\w*\n([\s\S]*?)^```\s*$/m).map((part, i) => i % 2 ? `<pre><code>${escape(part)}</code></pre>`
    : part.split(/\n{2,}/).map((b) => b.trim()).filter(Boolean).map((b) => /^#+ /.test(b) ? `<h4>${inline(b.replace(/^#+ /, ''))}</h4>`
      : /^[-*] /.test(b) ? `<ul>${b.split(/\n(?=[-*] )/).map((li) => `<li>${inline(li.slice(2))}</li>`).join('')}</ul>` : `<p>${inline(b)}</p>`).join('')).join('');
  /** A rule's explanation under one of its findings: the finding already shows its severity
   * and message, so only the category, the rule's title if it says more, the text and links. */
  function explanation(d) {
    const r = manifest.rules[d.code];
    if (!r) return '';
    const title = r.summary ?? r.message;
    const refs = (r.references ?? []).map((x) => x.url ? `<a href="${escape(x.url)}" target="_blank" rel="noopener">${escape(x.cite)}</a>` : escape(x.cite));
    return `<div class="explain"><p><span class="cat">${escape(r.category)}</span>${title && title !== d.message ? ` <b>${inline(title)}</b>` : ''}</p>` +
      `${markdown(r.explanation ?? '')}<p class="refs">${[...refs, `<a href="${base}wiki/lints.html#${encodeURIComponent(d.code)}">the rule's entry</a>`].join(' · ')}</p></div>`;
  }
  /** A hover's Markdown; its last paragraph, the places and links, is a muted footer. */
  const card = (value) => cards.slots(markdown(value)).replace(/<p>((?:\s|·|<a [^>]*>[^<]*<\/a>)+)<\/p>$/, '<p class="foot">$1</p>');

  // ---------------------------------------------------------------- worker requests

  const waiting = {}; // reply type -> resolver; runs, reads and kernel loads go one at a time, in order
  const REPLY = { run: 'result', kernel: 'kernel', init: 'ready', read: 'file' };
  let line_ = Promise.resolve();
  const ask = (msg, transfer = []) => {
    const asked = line_.then(() => new Promise((resolve, reject) => {
      waiting[REPLY[msg.type]] = { resolve, reject };
      worker.postMessage(msg, transfer);
    }));
    line_ = asked.catch(() => {});
    return asked;
  };

  // ---------------------------------------------------------------- language server

  let nextId = 1;
  const pending = new Map();
  function lsp(method, params, notify = false) {
    const message = { jsonrpc: '2.0', method, params, ...(notify ? {} : { id: nextId++ }) };
    worker.postMessage({ type: 'lsp', message });
    return notify ? null : new Promise((resolve, reject) => pending.set(message.id, { resolve, reject }));
  }
  const at = (doc, p) => Math.min(doc.line(Math.min(p.line + 1, doc.lines)).from + p.character, doc.length);
  const position = (doc, offset) => { const l = doc.lineAt(offset); return { line: l.number - 1, character: offset - l.from }; };
  /** `edits` (LSP TextEdits) applied to a plain text, for files not in the editor. */
  function edited(text, edits) {
    const starts = [0];
    for (let i = 0; i < text.length; i++) if (text[i] === '\n') starts.push(i + 1);
    const off = (p) => Math.min((starts[p.line] ?? text.length) + p.character, text.length);
    for (const e of [...edits].sort((a, b) => off(b.range.start) - off(a.range.start)))
      text = text.slice(0, off(e.range.start)) + e.newText + text.slice(off(e.range.end));
    return text;
  }
  /** A WorkspaceEdit: the open file through the editor, the others in the project's texts. */
  function applyEdit(edit) {
    const changes = { ...edit?.changes };
    for (const d of edit?.documentChanges ?? []) if (d.edits) changes[d.textDocument.uri] = d.edits;
    let n = 0;
    for (const [target, edits] of Object.entries(changes)) {
      n += edits.length;
      if (target === uri()) editor.apply(edits);
      else {
        const name = Object.keys(state.texts).find((f) => uri(f) === target);
        if (name) { state.texts[name] = edited(state.texts[name], edits); if (opened.has(target)) lsp('textDocument/didChange', { textDocument: { uri: target, version: ++version }, contentChanges: [{ text: state.texts[name] }] }, true); }
      }
    }
    return n;
  }

  // ---------------------------------------------------------------- editor

  let editor; // { text(), set(text), diagnostics(list), apply(edits), cursor(), select(c), word() }
  let version = 0, synced = 0, timer = null, checkStart = 0;
  function changed() {
    version++;
    if (refs && refs.file === state.file) clearRefs();
    clearTimeout(timer);
    timer = setTimeout(sync, 400);
  }
  /** Sends the buffer to the server unless it already has it. */
  function sync() {
    clearTimeout(timer);
    if (state.file in state.ro) return;
    state.texts[state.file] = editor.text();
    if (synced === version || !opened.has(uri())) return;
    synced = version;
    checking();
    lsp('textDocument/didChange', { textDocument: { uri: uri(), version }, contentChanges: [{ text: editor.text() }] }, true);
  }
  function checking() { lspStatus.textContent = 'checking…'; checkStart = performance.now(); }

  /** Completion order: the project's own commands, then the documented ones, internals
   * (`@`, `_`, `:` in the name) last; within each, prefix matches, then shorter names. */
  const tier = (i) => /\(document\)$/.test(i.detail ?? '') ? 0 : /[@_:]/.test(i.label) ? 2 : 1;
  const rank = (items, typed) => items.filter((i) => i.label.toLowerCase().includes(typed.toLowerCase()))
    .map((i) => [i, tier(i), i.label.startsWith(typed) ? 0 : 1])
    .sort((a, b) => a[1] - b[1] || a[2] - b[2] || a[0].label.length - b[0].label.length || a[0].label.localeCompare(b[0].label)).map((x) => x[0]);
  /** The language server's completion items at `position`, kept per buffer version. */
  const asked = new Map();
  function completions(position) {
    const k = `${uri()}|${version}|${position.line}:${position.character}`;
    if (!asked.has(k)) {
      asked.clear();
      asked.set(k, lsp('textDocument/completion', { textDocument: { uri: uri() }, position }).then((r) => r ?? []).catch(() => []));
    }
    return asked.get(k);
  }
  /** The command (or key) at offset `col` of `line`: [from, to, name, isCommand]. */
  function wordAt(line, col) {
    for (const m of line.matchAll(/\\([A-Za-z@]+|.)|[A-Za-z0-9_:.+*-]+/g))
      if (m.index <= col && col <= m.index + m[0].length) return [m.index, m.index + m[0].length, m[1] ?? m[0], m[0][0] === '\\'];
    return null;
  }

  async function codemirror(parent, text) {
    const cdn = 'https://esm.sh/';
    const [{ EditorView, basicSetup }, language, lint, ac, view, st, { oneDark }, tex] = await Promise.all(
      ['codemirror@6.0.2', '@codemirror/language@6', '@codemirror/lint@6', '@codemirror/autocomplete@6', '@codemirror/view@6',
        '@codemirror/state@6', '@codemirror/theme-one-dark@6'].map((m) => import(cdn + m)).concat(self.siteTex()));
    const KIND = { 3: 'function', 14: 'keyword', 9: 'namespace', 18: 'variable' };
    async function complete(ctx) {
      const word = ctx.matchBefore(/[A-Za-z@*_:]*/);
      const before = ctx.state.sliceDoc(word.from - 1, word.from);
      if (!ctx.explicit && before !== '\\' && before !== '{') return null;
      sync();
      const items = await completions(position(ctx.state.doc, word.from));
      return {
        from: word.from, filter: false,
        options: rank(items, word.text).slice(0, 200).map((i) => ({
          label: i.label, detail: i.detail, type: KIND[i.kind] ?? 'text',
          apply: i.insertTextFormat === 2 ? ac.snippet(i.insertText.replace(/\$\{?(\d+)\}?/g, '#{$1}')) : i.insertText ?? i.label,
        })),
      };
    }
    const hover = view.hoverTooltip(async (v, pos) => {
      // Asked again while the text changed under the request: never a stale card.
      let result, asked;
      for (let tries = 0; tries < 3 && asked !== version; tries++) {
        sync();
        asked = version;
        result = await lsp('textDocument/hover', { textDocument: { uri: uri() }, position: position(v.state.doc, pos) }).catch(() => null);
      }
      if (asked !== version) return null;
      const c = result?.contents;
      const value = typeof c === 'string' ? c : c?.value ?? (Array.isArray(c) ? c.map((x) => x.value ?? x).join('\n') : '');
      if (!value) return null;
      const range = result.range;
      return {
        pos: range ? at(v.state.doc, range.start) : pos, end: range ? at(v.state.doc, range.end) : pos,
        create: () => { const dom = document.createElement('div'); dom.className = 'hover'; dom.innerHTML = card(value); return { dom }; },
      };
    }, { hoverTime: 350, hideOnChange: true });
    // Ctrl (Cmd) held over a command underlines it as a link to its definition.
    const link = st.StateEffect.define();
    const linkField = st.StateField.define({
      create: () => view.Decoration.none,
      update: (deco, tr) => tr.effects.reduce((d, e) => e.is(link) ? (e.value ? view.Decoration.set([view.Decoration.mark({ class: 'cm-deflink' }).range(...e.value)]) : view.Decoration.none) : d, deco.map(tr.changes)),
      provide: (f) => view.EditorView.decorations.from(f),
    });
    // The places Find references lists, marked in the open file.
    const marks = st.StateEffect.define();
    const markField = st.StateField.define({
      create: () => view.Decoration.none,
      update: (deco, tr) => tr.effects.reduce((d, e) => e.is(marks) ? view.Decoration.set(e.value.map(([f, t]) => view.Decoration.mark({ class: 'cm-refmark' }).range(f, t)), true) : d, deco.map(tr.changes)),
      provide: (f) => view.EditorView.decorations.from(f),
    });
    const theme = new st.Compartment(), lock = new st.Compartment();
    const ev = new EditorView({
      parent, doc: text,
      extensions: [basicSetup, tex.language, language.syntaxHighlighting(tex.highlighter), lint.lintGutter(), ac.autocompletion({ override: [complete] }), hover,
        linkField, markField, view.keymap.of([{ key: 'F2', run: () => { renameBox(); return true; } }]),
        EditorView.updateListener.of((u) => { if (u.docChanged) changed(); if (u.selectionSet || u.docChanged) explainSoon(); }), theme.of([]), lock.of([])],
    });
    // the site's light/dark choice (site.js) picks the editor's theme
    const scheme = () => ev.dispatch({ effects: theme.reconfigure(self.siteDark?.() ? oneDark : []) });
    new MutationObserver(scheme).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', scheme);
    scheme();
    const doc = () => ev.state.doc;
    /** The command under the mouse: its range in the document, or null. */
    const under = (e) => {
      const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos == null) return null;
      const l = doc().lineAt(pos), w = wordAt(l.text, pos - l.from);
      return w && w[3] ? [l.from + w[0], l.from + w[1]] : null;
    };
    let linked = null;
    const underline = (r) => { if (String(r) !== String(linked)) { linked = r; ev.dispatch({ effects: link.of(r) }); } };
    parent.addEventListener('mousemove', (e) => { underline(e.ctrlKey || e.metaKey ? under(e) : null); const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY }); if (pos != null) explainSoon(position(doc(), pos)); });
    parent.addEventListener('mouseleave', () => underline(null));
    addEventListener('keyup', (e) => (e.key === 'Control' || e.key === 'Meta') && underline(null));
    parent.addEventListener('mousedown', (e) => {
      if (!(e.ctrlKey || e.metaKey) || e.button !== 0 || !under(e)) return;
      e.preventDefault();
      const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY });
      ev.dispatch({ selection: { anchor: pos } });
      definition();
    }, true);
    parent.addEventListener('contextmenu', (e) => {
      const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos == null) return;
      if (ev.state.selection.main.empty || pos < ev.state.selection.main.from || pos > ev.state.selection.main.to) ev.dispatch({ selection: { anchor: pos } });
      if (menuAt(e.clientX, e.clientY)) e.preventDefault();
    });
    parent.addEventListener('click', (e) => {
      const marker = e.target.closest('.cm-lint-marker');
      if (!marker && !e.target.closest('[class*=cm-lintRange]')) return;
      const pos = marker ? ev.lineBlockAtHeight(e.clientY - ev.documentTop).from : ev.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos != null) pick(ev.state.doc.lineAt(pos).number - 1, marker ? null : position(ev.state.doc, pos).character);
    });
    const span = (r) => [at(doc(), r.start), Math.max(at(doc(), r.end), at(doc(), r.start))];
    return {
      text: () => doc().toString(),
      set: (t, readOnly = false) => ev.dispatch({ changes: { from: 0, to: doc().length, insert: t }, effects: lock.reconfigure(readOnly ? [st.EditorState.readOnly.of(true)] : []) }),
      apply: (edits) => ev.dispatch({ changes: edits.map((e) => { const [from, to] = span(e.range); return { from, to, insert: e.newText }; }) }),
      diagnostics: (ds) => ev.dispatch(lint.setDiagnostics(ev.state, ds.map((d, i) => {
        const [from, to] = span(d.range);
        return {
          from, to: Math.min(Math.max(to, from + 1), doc().length), severity: sev(d) === 'hint' ? 'info' : sev(d),
          message: d.message, source: d.code,
          renderMessage: () => Object.assign(document.createElement('span'), { innerHTML: inline(d.message) }),
          actions: [...d.fixes.map((f) => ({ name: `Fix (${f.safety})`, apply: () => fix(f) })), { name: 'Explain rule', apply: () => unfold(i, false) }],
        };
      }))),
      cursor: () => [ev.state.selection.main.anchor, ev.state.selection.main.head],
      select: ([anchor, head] = [0, 0]) => { const n = doc().length; ev.dispatch({ selection: { anchor: Math.min(anchor, n), head: Math.min(head, n) }, scrollIntoView: true }); },
      position: () => position(doc(), ev.state.selection.main.head),
      word: () => { const l = doc().lineAt(ev.state.selection.main.head); return wordAt(l.text, ev.state.selection.main.head - l.from); },
      coords: () => ev.coordsAtPos(ev.state.selection.main.head),
      mark: (ranges) => ev.dispatch({ effects: marks.of(ranges.map((r) => span(r)).filter(([f, t]) => t > f)) }),
      line: () => { const l = doc().lineAt(ev.state.selection.main.head); return [l.from, l.to]; },
      focus: (r) => { const [from, to] = span(r); ev.dispatch({ selection: { anchor: from, head: to }, scrollIntoView: true }); ev.focus(); },
    };
  }

  function textarea(area) {
    area.addEventListener('input', changed);
    const pos = () => { const before = area.value.slice(0, area.selectionStart).split('\n'); return { line: before.length - 1, character: before.at(-1).length }; };
    return {
      text: () => area.value, set: (t, readOnly = false) => { area.value = t; area.readOnly = readOnly; changed(); }, diagnostics: () => {},
      apply: (edits) => { area.value = edited(area.value, edits); changed(); },
      cursor: () => [area.selectionStart, area.selectionEnd], select: ([a, h] = [0, 0]) => area.setSelectionRange(Math.min(a, h), Math.max(a, h)),
      mark: () => {}, line: () => { const s = area.value.lastIndexOf('\n', area.selectionStart - 1) + 1, e = area.value.indexOf('\n', area.selectionStart); return [s, e < 0 ? area.value.length : e]; },
      position: pos, word: () => { const p = pos(); return wordAt(area.value.split('\n')[p.line], p.character); }, focus: () => area.focus(),
      coords: () => { const r = area.getBoundingClientRect(); return { left: r.left + 8, bottom: r.top + 24 }; },
    };
  }

  // ---------------------------------------------------------------- findings and fixes

  const diagnostics = {}; // uri -> the last list, each with its fixes
  const key = (d) => `${d.code}|${d.range.start.line}:${d.range.start.character}|${d.message}`;
  async function show(target, ds) {
    const actions = await lsp('textDocument/codeAction', { textDocument: { uri: target },
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, context: { diagnostics: [] } }).catch(() => []);
    const by = new Map();
    for (const a of actions ?? []) for (const d of a.diagnostics ?? []) {
      const safety = d.data?.applicability ?? (a.isPreferred ? 'safe' : 'unsafe');
      by.set(key(d), [...(by.get(key(d)) ?? []), { ...a, safety }]);
    }
    diagnostics[target] = ds.map((d) => ({ ...d, fixes: by.get(key(d)) ?? [] }));
    if (target === uri()) render();
  }
  /** Each finding: its severity, place, rule and message once. */
  function render() {
    const ds = diagnostics[uri()] ?? [];
    editor.diagnostics(ds);
    count.textContent = ds.length;
    if (checkStart) timing('t-check', `check ${secs(performance.now() - checkStart)}`, 'the language server analyzed the open file');
    checkStart = 0;
    lspStatus.textContent = state.file in state.ro ? 'read-only file' : ds.length ? '' : 'none';
    const fixButton = (f, j, cls) => `<button class="small apply${cls}" data-j="${j}" title="${escape(f.title)}">Fix <span class="fix fix-${f.safety}">${f.safety}</span></button>`;
    list.innerHTML = ds.map((d, i) => `<li class="f-${sev(d)}${d.fixes.length ? ' fixable' : ''}${unfolded.has(key(d)) ? ' open' : ''}" data-i="${i}" tabindex="0">` +
      `<span class="badge"><span class="sev sev-${sev(d)}">${sev(d)}</span>${d.fixes.length ? fixButton(d.fixes[0], 0, ' as-badge') : ''}</span> <b>${d.range.start.line + 1}:${d.range.start.character + 1}</b> ` +
      `<code>${escape(d.code ?? '')}</code> <span class="msg">${inline(d.message)}</span><span class="acts">` +
      d.fixes.slice(1).map((f, j) => fixButton(f, j + 1, '')).join('') +
      `<button class="small ghost why">${unfolded.has(key(d)) ? 'Hide rule' : 'Explain rule'}</button></span>` +
      (unfolded.has(key(d)) ? explanation(d) : '') + '</li>').join('');
  }
  const unfolded = new Set();
  function unfold(i, toggle = true) {
    const d = diagnostics[uri()]?.[i];
    if (!d) return;
    if (toggle && unfolded.has(key(d))) unfolded.delete(key(d)); else unfolded.add(key(d));
    render();
    list.children[i]?.scrollIntoView({ block: 'nearest', behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
  }
  /** The findings at a place: line, and column unless a gutter marker was clicked. */
  const findingsAt = (line, character) => (diagnostics[uri()] ?? []).map((d, i) => [d, i]).filter(([d]) => d.range.start.line <= line && line <= d.range.end.line &&
    (character == null || ((d.range.start.line < line || d.range.start.character <= character) && (d.range.end.line > line || character <= Math.max(d.range.end.character, d.range.start.character + 1)))));
  function pick(line, character) {
    const [hit] = findingsAt(line, character);
    if (hit) unfold(hit[1], false);
  }
  list.onclick = (e) => {
    const li = e.target.closest('li');
    if (!li) return;
    const d = diagnostics[uri()][li.dataset.i];
    const apply = e.target.closest('.apply');
    if (apply) return fix(d.fixes[apply.dataset.j]);
    if (e.target.closest('.explain a')) return;
    if (!e.target.closest('.why')) editor.focus(d.range);
    unfold(+li.dataset.i);
  };
  function fix(action) {
    applyEdit(action.edit);
    toast(`Applied: ${action.title}`);
  }
  /** Every safe fix of the open file, of one rule if given, in one edit; overlapping ones wait for the next check. */
  function fixAll(code, unsafe = false) {
    sync();
    const edits = [];
    for (const d of diagnostics[uri()] ?? []) if (!code || d.code === code) for (const f of d.fixes.filter((f) => unsafe || f.safety === 'safe').slice(0, 1)) edits.push(...(f.edit?.changes?.[uri()] ?? []));
    const pos = (p) => p.line * 1e6 + p.character;
    edits.sort((a, b) => pos(a.range.start) - pos(b.range.start));
    const kept = edits.filter((e, i) => !edits.slice(0, i).some((o) => pos(o.range.end) > pos(e.range.start) || pos(o.range.start) === pos(e.range.start)));
    if (kept.length) editor.apply(kept);
    const what = unsafe ? 'fix' : 'safe fix';
    toast(kept.length ? `Applied ${kept.length} ${what}${kept.length > 1 ? 'es' : ''}` : `No ${what}es${code ? ` for ${code}` : ''} in this file`);
  }

  // ---------------------------------------------------------------- language server actions: definition, rename, the context menu

  /** The definition of the command at the cursor: a project file's tab, or a bundle file read-only. */
  async function definition() {
    sync();
    const r = await lsp('textDocument/definition', { textDocument: { uri: uri() }, position: editor.position() }).catch(() => null);
    const loc = Array.isArray(r) ? r[0] : r;
    if (!loc) return toast('No definition found');
    await openPlace(decodeURIComponent(new URL(loc.uri ?? loc.targetUri).pathname), (loc.range ?? loc.targetSelectionRange).start);
  }
  /** `file` (a path or a bare name) at `start`; a file outside the project opens read-only. */
  async function openPlace(file, start = { line: 0, character: 0 }) {
    const name = file.startsWith(`${state.dir}/`) ? file.slice(state.dir.length + 1) : Object.keys(state.texts).find((n) => n === file) ?? null;
    let target = name in state.texts ? name : null;
    if (!target) {
      const got = await ask({ type: 'read', path: file }).catch(() => null);
      if (!got?.data) return toast(`${file.split('/').pop()}:${start.line + 1}`);
      target = got.path.split('/').pop();
      state.ro[target] = got.path;
      state.texts[target] = new TextDecoder().decode(got.data);
    }
    choose(target);
    editor.focus({ start, end: start });
    editor.mark([{ start: { line: start.line, character: 0 }, end: { line: start.line + 1, character: 0 } }]);
  }
  // A place (`file:line`) in a card, a finding or a run opens; a command chip is explained.
  document.addEventListener('click', (e) => {
    const a = e.target.closest('a.place, a.cs');
    if (!a || !editor) return;
    e.preventDefault();
    if (a.classList.contains('cs')) return explainName(a.dataset.cs);
    const file = a.dataset.path;
    const name = Object.keys(state.texts).find((n) => !(n in state.ro) && (path(n) === file || n === file || file.endsWith('/' + n)));
    openPlace(name ?? file, { line: Number(a.dataset.line) - 1, character: 0 });
  });
  async function renameTo(newName, position = editor.position()) {
    try {
      const n = applyEdit(await lsp('textDocument/rename', { textDocument: { uri: uri() }, position, newName }));
      toast(n ? `Renamed ${n} place${n > 1 ? 's' : ''}` : 'Nothing to rename');
    } catch (err) { toast(err.message); }
  }
  /** F2: a box at the cursor, filled with the current name; Enter renames, Escape cancels. */
  async function renameBox() {
    if (state.file in state.ro) return;
    sync();
    const position = editor.position();
    const where = await lsp('textDocument/prepareRename', { textDocument: { uri: uri() }, position }).catch(() => null);
    if (!where) return toast('Put the cursor on a command or label to rename it');
    const c = editor.coords() ?? { left: 40, bottom: 80 };
    const box = Object.assign(document.createElement('input'), { className: 'renamebox', value: where.placeholder ?? editor.word()?.[2] ?? '', spellcheck: false });
    box.setAttribute('aria-label', 'new name');
    Object.assign(box.style, { left: `${c.left}px`, top: `${c.bottom + 4}px` });
    document.body.append(box);
    box.select();
    const close = () => { box.onblur = null; box.remove(); };
    box.onkeydown = (e) => {
      if (e.key === 'Enter') { e.preventDefault(); close(); renameTo(box.value, position); }
      else if (e.key === 'Escape') { e.preventDefault(); close(); }
    };
    box.onblur = close;
  }
  /** Find references: the places in the findings panel, and marked in the open file. */
  let refs = null;
  async function references() {
    sync();
    const w = editor.word();
    const at = { textDocument: { uri: uri() }, position: editor.position() };
    const [r, spans] = await Promise.all([lsp('textDocument/references', { ...at, context: { includeDeclaration: true } }), lsp('textDocument/documentHighlight', at)].map((q) => q.catch(() => null)));
    if (!r?.length) { clearRefs(); return toast('No references found'); }
    // The list from references, the marks from the highlights: they span the name.
    const lines = editor.text().split('\n');
    refs = { name: w ? `${w[3] ? '\\' : ''}${w[2]}` : 'symbol', file: state.file, locs: r, marks: (spans ?? []).map((h) => h.range),
      text: (l) => l.uri === uri() ? (lines[l.range.start.line] ?? '').trim() : '' };
    drawRefs();
    refsBox.scrollIntoView({ block: 'nearest' });
  }
  function clearRefs() { refs = null; drawRefs(); }
  function drawRefs() {
    editor.mark(refs && refs.file === state.file ? refs.marks : []);
    refsBox.hidden = !refs;
    if (!refs) return;
    const name = (l) => decodeURIComponent(new URL(l.uri).pathname).split('/').pop();
    refsBox.innerHTML = `<div class="refs-head">References to <code>${escape(refs.name)}</code> <b>${refs.locs.length}</b><button class="small ghost close" title="close">×</button></div>` +
      `<ul>${refs.locs.map((l, i) => `<li data-i="${i}" tabindex="0"><span class="chip">${escape(name(l))}:${l.range.start.line + 1}</span> <code>${escape(refs.text(l).slice(0, 80))}</code></li>`).join('')}</ul>`;
  }
  const refsBox = $('play-refs');
  refsBox.onclick = (e) => {
    if (e.target.closest('.close')) return clearRefs();
    const li = e.target.closest('li');
    if (li) { const l = refs.locs[li.dataset.i]; openPlace(decodeURIComponent(new URL(l.uri).pathname), l.range.start); }
  };
  /** The command's expansions as a run: where and how often it was used. */
  const expansions = (w) => execute(`query expansions --filter name=\\${w[2]}`);
  /** A link that opens this file with the cursor's line selected. */
  async function lineLink() {
    const c = editor.cursor();
    editor.select(editor.line());
    const link = await permalink();
    editor.select(c);
    try { await navigator.clipboard.writeText(link); } catch {}
    toast(`Link to line ${editor.position().line + 1} copied`);
    return link;
  }
  /** The editor's context menu at the cursor: the symbol's actions, the fixes there, a link to the line. */
  function menuAt(x, y) {
    const w = editor.word(), p = editor.position();
    const found = findingsAt(p.line, p.character).map(([d]) => d);
    const fixes = found.flatMap((d) => d.fixes);
    const rules = [...new Set(found.filter((d) => d.fixes.length).map((d) => d.code))];
    const editable = !(state.file in state.ro);
    const items = [
      ...(w?.[3] ? [['Go to definition', 'Ctrl+click', definition]] : []),
      ...(w && editable ? [['Find references', '', references], ['Rename symbol', 'F2', renameBox]] : []),
      ...(w?.[3] ? [['Explain', '', focusExplain], ['Show expansions', '', () => expansions(w)]] : []),
      ...found.map((d) => [`Explain rule ${d.code}`, '', () => unfold(diagnostics[uri()].indexOf(d), false)]),
      ...(editable ? fixes.map((f) => [`Fix: ${f.title}`, f.safety, () => fix(f)]) : []),
      ...(editable ? rules.map((c) => [`Fix all ${c}`, `${(diagnostics[uri()] ?? []).filter((d) => d.code === c && d.fixes.length).length}×`, () => fixAll(c, true)]) : []),
      ['Copy link to this line', '', lineLink],
    ];
    if (!items.length) return false;
    ctxMenu.innerHTML = items.map(([label, hint], i) => `<li role="menuitem" tabindex="-1" data-i="${i}">${escape(label)}<span class="muted">${escape(hint)}</span></li>`).join('');
    ctxMenu.hidden = false;
    Object.assign(ctxMenu.style, { left: `${Math.min(x, innerWidth - ctxMenu.offsetWidth - 8)}px`, top: `${Math.min(y, innerHeight - ctxMenu.offsetHeight - 8)}px` });
    ctxMenu.onclick = (e) => { const li = e.target.closest('li'); if (li) { ctxMenu.hidden = true; items[li.dataset.i][2](); } };
    return true;
  }
  addEventListener('mousedown', (e) => { if (!e.target.closest('.ctx')) for (const m of document.querySelectorAll('.ctx')) m.hidden = true; });
  addEventListener('keydown', (e) => { if (e.key === 'Escape') { for (const m of document.querySelectorAll('.ctx')) m.hidden = true; drop.hidden = true; } });

  // ---------------------------------------------------------------- the Explain macro panel

  /** The command at the caret or under the mouse, explained live: what `satex explain`
   * says of it in the current text, run in the worker and kept per name and text version.
   * The language server's hover fills in until the run is back. */
  const explainBox = $('play-explain'), explainBody = $('explain-body'), explainLabel = $('explain-name');
  var explainTimer = 0, explained = '';
  const explanations = new Map(); // `name|file|version|kernel` -> records
  function explainSoon(at) {
    clearTimeout(explainTimer);
    explainTimer = setTimeout(() => explainAt(at ?? editor?.position()), 200);
  }
  async function explainAt(at) {
    if (!editor || !at) return;
    const line = editor.text().split('\n')[at.line] ?? '', w = wordAt(line, at.character);
    if (!w?.[3]) return;
    return explainName(`\\${w[2]}`, at);
  }
  /** Runs `satex explain name` unless the same text was explained already. */
  async function explainRecords(name) {
    sync();
    const target = state.root ?? (state.file in state.ro ? manifest.sample : state.file);
    const k = `${name}|${target}|${version}|${state.kernel}`;
    if (!explanations.has(k)) {
      const edits = Object.fromEntries(Object.entries(state.texts).filter(([n]) => !(n in state.ro)).map(([n, text]) => [path(n), text]));
      explanations.set(k, ask({ type: 'run', args: [...flags(), 'explain', name, '--format', 'json', '-f', target], edits, cwd: state.dir })
        .then((data) => JSON.parse(data.lines.filter(([kind]) => kind === 'out').map(([, t]) => plain(t)).join('\n') || '[]')).catch(() => []));
    }
    return explanations.get(k);
  }
  /** The Explain macro panel shows `name`: its full record, or the hover until that is back. */
  async function explainName(name, at = null, records = null) {
    const k = `${name}|${version}|${state.file}`;
    if (k === explained && !records) return;
    explained = k;
    explainLabel.textContent = name;
    explainBox.open = true;
    if (!records && at) {
      lsp('textDocument/hover', { textDocument: { uri: uri() }, position: at }).then((result) => {
        const c = result?.contents, value = typeof c === 'string' ? c : c?.value ?? '';
        if (value && explained === k && !explainBody.querySelector('.xcard')) explainBody.innerHTML = card(value);
      }).catch(() => {});
    }
    explainBody.classList.add('stale');
    const rs = records ?? await explainRecords(name);
    if (explained !== k) return;
    explainBody.classList.remove('stale');
    const shown = rs.filter((r) => r.name === name);
    if (shown.length) explainBody.innerHTML = shown.map(cards.explain).join('');
    else if (!explainBody.querySelector('.hover')) explainBody.innerHTML = `<p class="muted">satex knows no ${escape(name)} here.</p>`;
  }
  function focusExplain() {
    explainBox.open = true;
    explained = '';
    explainAt(editor.position());
    explainBox.scrollIntoView({ block: 'nearest' });
  }

  // ---------------------------------------------------------------- runs

  /** The kernel's flags; an upload, outside /work, reads the playground's satex.yaml by name. */
  const flags = () => [...manifest.kernels[state.kernel].args, ...(state.upload ? ['--config', '/work/satex.yaml'] : [])];
  const command = () => words(args.value).filter((x) => x !== 'satex' && !x.startsWith('-'))[0];
  /** Plain `lint` repeats what the language server shows: it runs only when asked to. */
  const plainLint = () => words(args.value).filter((x) => x !== 'satex').join(' ') === 'lint';
  /** Records a run printed as JSON, as cards (one) or a table (many), each place a chip that opens it.
   * The text output stays behind a collapsed "raw" toggle. */
  const HIDDEN_COLS = new Set(['path', 'node', 'file', 'line', 'col', 'expanded', 'certain', 'definition']);
  const cell = cards.value;
  const chip = (r) => r.file && r.line ? cards.placeLink(r.path ?? r.file, r.line, r.file) : '';
  function structured(out, raw) {
    let rows;
    try { rows = JSON.parse(out); } catch { return `<pre class="raw">${raw}</pre>`; }
    if (!Array.isArray(rows)) rows = [rows];
    const rawBox = `<details class="raw"><summary>raw</summary><pre>${raw}</pre></details>`;
    if (!rows.length) return `<p class="muted">no results</p>${rawBox}`;
    if (command() === 'explain' && rows.every((r) => r.name)) return rows.map(cards.explain).join('') + rawBox;
    if (rows.length === 1) {
      const r = rows[0];
      return `<div class="card"><p>${cell(r.name ?? r.key ?? '')} ${chip(r)}</p><dl>` +
        Object.entries(r).filter(([k, v]) => !HIDDEN_COLS.has(k) && k !== 'name' && v != null && v !== '').map(([k, v]) => `<dt>${escape(k)}</dt><dd>${cell(v)}</dd>`).join('') + `</dl></div>${rawBox}`;
    }
    const cols = [...new Set(rows.flatMap((r) => Object.keys(r)))].filter((k) => !HIDDEN_COLS.has(k));
    return `<table class="rows"><thead><tr>${cols.map((c) => `<th>${escape(c)}</th>`).join('')}<th>where</th></tr></thead><tbody>` +
      rows.slice(0, 500).map((r) => `<tr>${cols.map((c) => `<td>${cell(r[c])}</td>`).join('')}<td>${chip(r)}</td></tr>`).join('') + `</tbody></table>${rawBox}`;
  }
  /** The command line as satex's argv: `-f` the root, the kernel's flags, colors and --stats. */
  function argv() {
    let a = words(args.value);
    if (a[0] === 'satex') a = a.slice(1);
    // Records render as tables and cards rather than as text.
    if (['query', 'explain', 'scope', 'controls'].includes(a[0]) && !a.some((x) => x.startsWith('--format'))) a.push('--format', 'json');
    if (!a.some((x) => x === '-f' || x.startsWith('--file'))) a.push('-f', state.root ?? (state.file in state.ro ? manifest.sample : state.file));
    return [...flags(), '--stats', ...(a.some((x) => x.startsWith('--color')) ? [] : ['--color', 'always']), ...a];
  }
  const outBox = $('play-out');
  async function start() {
    sync();
    const a = argv();
    button.disabled = true;
    status.textContent = 'running…';
    delete status.dataset.done;
    cmd.textContent = `satex ${words(args.value).filter((x) => x !== 'satex').join(' ')}`;
    const edits = Object.fromEntries(Object.entries(state.texts).filter(([name]) => !(name in state.ro)).map(([name, text]) => [path(name), text]));
    try {
      const data = await ask({ type: 'run', args: a, edits, cwd: state.dir });
      let stats = '', summary = '';
      const raw = data.lines.map(([kind, text]) => {
        if (kind === 'out') { if (/\d+ errors?, \d+ warnings?/.test(plain(text))) summary = plain(text).trim(); return ansi(text); }
        if (/^\s*[\d.]+s\s.*\|.* tokens/.test(plain(text))) { stats = plain(text).trim(); return null; }
        if (/^ {2}hot /.test(plain(text))) return null;
        return (text = quiet(text)) ? `<span class="err">${ansi(text)}</span>` : null;
      }).filter((line) => line !== null).join('\n');
      const printed = data.lines.filter(([kind]) => kind === 'out').map(([, t]) => plain(t)).join('\n');
      output.innerHTML = structured(printed, raw);
      // `explain \x` fills the Explain macro panel: one card, not a second one under Output
      let records = null;
      try { records = command() === 'explain' && !a.includes('--at') ? JSON.parse(printed) : null; } catch {}
      const explainedHere = Array.isArray(records) && records.length > 0;
      if (explainedHere) { explained = ''; explainName(records[0].name, null, records); explainBox.scrollIntoView({ block: 'nearest' }); }
      const [time, peak, files, tokens] = [/^(\S+?)s/, /([\d.]+ \w+) peak/, /([\d.k]+) files/, /([\d.kM]+) tokens/].map((r) => stats.match(r)?.[1]);
      timing('t-stats', stats ? [`${time} s`, peak && `${peak} peak`, `${files} files`, `${tokens} tokens`].filter(Boolean).join(' · ') : '', stats);
      const failed = typeof data.status !== 'number' || data.status > 1;
      status.textContent = `${command() ?? 'run'} ${secs(data.ms)} · exit ${data.status}${plainLint() && summary ? ` · ${summary}` : ''}`;
      // plain lint's findings are the panel's: its output would repeat them
      outBox.hidden = (plainLint() || explainedHere) && !failed;
      outBox.open = !outBox.hidden;
      status.dataset.done = data.status;
    } catch (e) {
      status.textContent = `the run failed: ${e.message}`;
      status.dataset.done = 'error';
      outBox.hidden = false;
    }
    button.disabled = false;
  }

  // ---------------------------------------------------------------- the command line

  const past = []; // the command line's history
  let back = 0;
  /** The playground's own commands, next to satex's: language server edits in the editor. */
  const LOCAL = {
    rename: { about: 'rename the command or label at the cursor, in every file', run: (rest) => rest[0] ? renameTo(rest[0].replace(/^\\/, '')) : renameBox() },
    fix: { about: 'apply the safe fixes, of one rule if given', run: (rest) => fixAll(rest[0]) },
  };
  /** Runs `line` (or the command line): a playground command, or satex. */
  function execute(line = args.value) {
    if (!editor) return;
    args.value = line;
    menu.hidden = true;
    if (past.at(-1) !== line) past.push(line);
    back = past.length;
    const a = words(line).filter((x) => x !== 'satex');
    if (a[0] in LOCAL) return LOCAL[a[0]].run(a.slice(1));
    if (a[0] === 'explain' && a.length === 1) {
      const w = editor.word();
      if (!w?.[3]) return toast('Put the cursor on a command, or name one: explain \\cmd');
      args.value = `explain \\${w[2]}`;
    }
    start();
  }

  // From `satex completions json`: the command tree, the config keys, the rule codes.
  // Commands that make no sense in a browser stay out of the list.
  const tree = manifest.completion, menu = $('play-complete');
  const HIDE = new Set(['lsp', 'completions', 'install', 'cache', 'help']);
  const top = { ...tree.command, subcommands: [...tree.command.subcommands.filter((c) => !HIDE.has(c.name)),
    ...Object.entries(LOCAL).map(([name, c]) => ({ name, about: c.about, args: [], subcommands: [] }))] };
  let choices = [], chosen = 0, asking = 0;
  async function suggest() {
    const ticket = ++asking;
    const before = args.value.slice(0, args.selectionStart);
    const toks = words(before), cur = !before || /\s$/.test(before) ? '' : toks.pop();
    let node = top;
    for (const t of toks) node = node.subcommands.find((c) => c.name === t) ?? node;
    const flag = toks.at(-1)?.startsWith('-') && node.args.find((f) => `--${f.long}` === toks.at(-1) || `-${f.short}` === toks.at(-1));
    let items = [];
    if (flag?.takes_value && flag.long === 'set') {
      const [k, eq] = [cur.split('=')[0], cur.includes('=')];
      items = !eq ? Object.entries(tree.config_keys).map(([key, v]) => [`${key}=`, JSON.stringify(v)])
        : Array.isArray(tree.config_keys[k]) && /lint|rule/.test(k) ? tree.rules.map((r) => [`${k}=[${r}]`, manifest.rules[r]?.message]) : [];
    } else if (flag?.takes_value && flag.long === 'file') items = Object.keys(state.texts).filter((n) => !(n in state.ro)).map((n) => [n, 'file']);
    else if (flag?.takes_value && flag.long === 'explain') items = tree.rules.map((r) => [r, manifest.rules[r]?.message]);
    else if (flag?.takes_value && flag.long === 'at') items = atChoices(node.name);
    else if (flag?.takes_value) items = [];
    else if (cur.startsWith('-')) items = node.args.filter((f) => f.long).map((f) => [`--${f.long}`, f.help]);
    else if (node.name === 'explain') {
      // the commands in scope at the end of the open file, from the language server
      const doc = editor.text().split('\n');
      const found = await completions({ line: doc.length - 1, character: doc.at(-1).length });
      if (ticket !== asking) return;
      items = rank(found, cur.replace(/^\\/, '')).map((i) => [`\\${i.label}`, i.detail]);
    } else if (node.name === 'fix') items = [...new Set((diagnostics[uri()] ?? []).filter((d) => d.fixes.some((f) => f.safety === 'safe')).map((d) => d.code))].map((c) => [c, manifest.rules[c]?.message]);
    else if (node.name === 'rename') items = [];
    else if (node.name === 'query' && !toks.some((t) => manifest.queries.includes(t))) items = manifest.queries.map((q) => [q, 'query']);
    else items = node.subcommands.map((c) => [c.name, c.about]);
    const seen = new Set();
    choices = items.filter(([l]) => l.startsWith(cur) && l !== cur && !seen.has(l) && seen.add(l)).slice(0, 50).map(([l, d]) => [l, d, cur]);
    chosen = 0;
    draw();
  }
  /** `--at`'s values: for explain the document's halves, after each package it loads and the
   * caret's line; for a position the caret's and the project's files. */
  function atChoices(command) {
    const p = editor.position(), file = state.file in state.ro ? manifest.sample : state.file;
    if (command === 'explain') {
      const text = editor.text().replace(/%.*$/gm, '');
      const packages = [...new Set([...text.matchAll(/\\(?:usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}/g)].flatMap((m) => m[1].split(',').map((x) => x.trim()).filter(Boolean)))];
      return [['preamble', 'as at \\begin{document}'], ['document', 'as at the end of the document'],
        ...packages.map((pkg) => [`after:${pkg}`, `right after \\usepackage{${pkg}}`]),
        [`${p.line + 1}`, 'the caret\'s line'], [`${file}:${p.line + 1}`, 'the caret\'s line, in this file']];
    }
    return [[`${p.line + 1}:${p.character + 1}`, 'the caret'], [`${file}:${p.line + 1}:${p.character + 1}`, 'the caret, in this file'],
      ...Object.keys(state.texts).filter((n) => !(n in state.ro)).map((n) => [`${n}:`, 'a file of the project, then LINE:COL'])];
  }
  function draw() {
    menu.hidden = !choices.length;
    menu.innerHTML = choices.map(([l, d], i) => `<li role="option" aria-selected="${i === chosen}" data-i="${i}"><code>${escape(l)}</code>` +
      `${d ? ` <span class="muted">${escape(String(d).split('\n')[0])}</span>` : ''}</li>`).join('');
    menu.children[chosen]?.scrollIntoView({ block: 'nearest' });
  }
  function accept(i) {
    const [label, , cur] = choices[i];
    const at = args.selectionStart, from = at - cur.length;
    const glue = /[=:]$/.test(label) ? '' : ' ';
    args.value = args.value.slice(0, from) + label + glue + args.value.slice(at).replace(/^\s+/, '');
    args.selectionStart = args.selectionEnd = from + label.length + glue.length;
    suggest();
  }
  args.oninput = suggest;
  args.onblur = () => setTimeout(() => { menu.hidden = true; }, 120);
  menu.onmousedown = (e) => { e.preventDefault(); const li = e.target.closest('li'); if (li) accept(+li.dataset.i); };
  args.onkeydown = (e) => {
    const open = !menu.hidden && choices.length;
    if (open && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault();
      chosen = (chosen + (e.key === 'ArrowDown' ? 1 : choices.length - 1)) % choices.length;
      menu.dataset.moved = 1;
      return draw();
    }
    if (open && (e.key === 'Tab' || (e.key === 'Enter' && menu.dataset.moved))) { e.preventDefault(); delete menu.dataset.moved; return accept(chosen); }
    delete menu.dataset.moved;
    if (open && e.key === 'Escape') menu.hidden = true;
    else if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && past.length) {
      e.preventDefault();
      back = Math.max(0, Math.min(past.length, back + (e.key === 'ArrowUp' ? -1 : 1)));
      args.value = past[back] ?? '';
    } else if (e.key === 'Enter' && !button.disabled) { e.preventDefault(); execute(); }
  };
  button.onclick = () => execute();
  $('play-editor').addEventListener('keydown', (e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && !button.disabled) execute(); });

  // ---------------------------------------------------------------- files, projects, kernels

  const opened = new Set();
  function open(name) {
    if (opened.has(uri(name)) || name in state.ro) return;
    opened.add(uri(name));
    checking();
    lsp('textDocument/didOpen', { textDocument: { uri: uri(name), languageId: 'latex', version, text: state.texts[name] } }, true);
  }
  /** Starts the language server again: another kernel or another project. */
  async function restart() {
    sync();
    opened.clear();
    asked.clear();
    for (const p of pending.values()) p.resolve(null);
    pending.clear();
    const root = state.root ? ['-f', state.root] : [];
    worker.postMessage({ type: 'reset', args: [...flags(), ...root], cwd: state.dir });
    lspStatus.textContent = 'language server: starting…';
    await lsp('initialize', { processId: null, rootUri: `file://${state.dir}`, capabilities: {} });
    lsp('initialized', {}, true);
    if (state.root) open(state.root);
    open(state.file);
    synced = version;
  }
  function drawTabs() {
    const names = Object.keys(state.texts);
    tabs.innerHTML = (state.upload ? '<button role="tab" class="back" data-back="1" title="back to the samples">← samples</button>' : '') +
      names.map((n) => `<button role="tab" aria-selected="${n === state.file}" data-name="${escape(n)}"${n in state.ro ? ` class="ro" title="${escape(state.ro[n])}, read-only"` : ''}>${escape(n)}</button>`).join('');
    $('play-rootbox').hidden = !state.upload;
  }
  /** The command a sample comes with: plain lint unless the manifest says otherwise. */
  const commandOf = (name) => manifest.commands?.[name] ?? 'lint';
  /** Shows `name` in the editor; the language server gets it if it does not have it. */
  function choose(name) {
    if (!editor || !(name in state.texts) || name === state.file) return;
    sync();
    state.file = name;
    drawTabs();
    editor.set(state.texts[name], name in state.ro);
    drawRefs();
    if (name in state.ro) render();
    else if (opened.has(uri())) { synced = version; lsp('textDocument/didChange', { textDocument: { uri: uri(), version }, contentChanges: [{ text: state.texts[name] }] }, true); render(); }
    else open(name);
    if (!state.upload && !(name in state.ro) && name in sampleTexts) {
      history.replaceState(null, '', `#${name}`);
      args.value = commandOf(name);
      if (!plainLint()) start();
    }
  }
  tabs.onclick = (e) => {
    const tab = e.target.closest('button');
    if (tab?.dataset.back) samples();
    else if (tab) choose(tab.dataset.name);
  };
  addEventListener('hashchange', () => {
    const h = decodeURIComponent(location.hash.slice(1)), opening = h.match(/^open=(.+):(\d+)$/);
    if (opening) openPlace(opening[1], { line: Number(opening[2]) - 1, character: 0 });
    else if (!h.startsWith('s=')) choose(h);
  });

  let sampleTexts;
  async function samples(file = manifest.sample) {
    Object.assign(state, { dir: '/work', texts: { ...sampleTexts }, original: sampleTexts, file, root: null, upload: false, ro: {} });
    drawTabs();
    editor.set(state.texts[file]);
    await restart();
    args.value = commandOf(file);
    if (!plainLint()) start();
  }

  /** A kernel's bundle, fetched once, then the server and the run start over with it. */
  async function kernel(i) {
    const entry = manifest.kernels[i];
    if (!state.loaded.has(i)) {
      status.textContent = `loading the ${entry.label} kernel (${size(entry.size)})…`;
      const t = performance.now();
      const given = location.protocol === 'file:' ? await local(entry.bundle) : undefined;
      await ask({ type: 'kernel', base, entry, given }, given ? [given.buffer] : []);
      state.loaded.add(i);
      timing('t-init', `${$('t-init').textContent} · ${entry.label} ${secs(performance.now() - t)}`, 'time to fetch and set up each kernel');
      status.textContent = `${entry.label} ready`;
    }
    state.kernel = i;
    kernelBox.value = i;
  }
  kernelBox.innerHTML = manifest.kernels.map((k, i) => `<option value="${i}" title="download ${size(k.size)}">${k.label} · ${size(k.size)}</option>`).join('') +
    (manifest.unavailable ?? []).map(([label, why]) => `<option disabled title="${escape(why)}">${label} · not available</option>`).join('');
  kernelBox.onchange = async () => {
    kernelBox.disabled = true;
    try { await kernel(+kernelBox.value); await restart(); if (!plainLint()) await start(); } catch (e) { toast(`the kernel did not load: ${e.message}`); kernelBox.value = state.kernel; }
    kernelBox.disabled = false;
  };

  // ---------------------------------------------------------------- uploads

  const LIMIT = 50 * 2 ** 20;
  const TEXT = /\.(tex|ltx|sty|cls|bib|bst|cfg|def|clo|fd|dtx|ins|bbx|cbx|lbx|yaml|yml|txt|md|latexmain)$|(^|\/)(latexmkrc|\.latexmkrc|Makefile)$/i;
  const inflate = async (data) => new Uint8Array(await new Response(new Blob([data]).stream().pipeThrough(new DecompressionStream('deflate-raw'))).arrayBuffer());
  /** A zip file's entries, read from its central directory: stored or deflated, nothing else. */
  async function unzip(buf) {
    const v = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
    let end = buf.length - 22;
    while (end >= 0 && v.getUint32(end, true) !== 0x06054b50) end--;
    if (end < 0) throw new Error('this is not a zip file');
    const out = {};
    let p = v.getUint32(end + 16, true), total = 0;
    for (let i = v.getUint16(end + 10, true); i > 0; i--) {
      const method = v.getUint16(p + 10, true), packed = v.getUint32(p + 20, true), size = v.getUint32(p + 24, true);
      const n = v.getUint16(p + 28, true), local = v.getUint32(p + 42, true);
      const name = new TextDecoder().decode(buf.subarray(p + 46, p + 46 + n));
      p += 46 + n + v.getUint16(p + 30, true) + v.getUint16(p + 32, true);
      if (name.endsWith('/') || /(^|\/)(__MACOSX|\.git)\//.test(name)) continue;
      if ((total += size) > LIMIT) throw new Error(`the project unpacks to more than ${LIMIT / 2 ** 20} MiB`);
      const from = local + 30 + v.getUint16(local + 26, true) + v.getUint16(local + 28, true);
      const data = buf.subarray(from, from + packed);
      if (method !== 0 && method !== 8) throw new Error(`${name} uses an unsupported compression`);
      out[name] = method === 8 ? await inflate(data) : data;
    }
    const names = Object.keys(out), top = names[0]?.split('/')[0] + '/';
    return names.length && names.every((n) => n.startsWith(top)) ? Object.fromEntries(names.map((n) => [n.slice(top.length), out[n]])) : out;
  }
  /** The root documents: a .latexmain or latexmkrc names one, else every file with \documentclass and \begin{document}. */
  function roots(texts) {
    const named = Object.keys(texts).filter((n) => n.endsWith('.latexmain')).map((n) => n.slice(0, -'.latexmain'.length))
      .concat([...Object.entries(texts).filter(([n]) => /latexmkrc$/.test(n)).flatMap(([, t]) => [...t.matchAll(/@default_files\s*=\s*\(([^)]*)\)/g)])]
        .flatMap((m) => [...m[1].matchAll(/['"]([^'"]+)['"]/g)].map((x) => x[1].replace(/^(?!.*\.tex$)(.*)$/, '$1.tex'))))
      .filter((n) => n in texts);
    return named.length ? named : Object.keys(texts).filter((n) => /\\documentclass/.test(texts[n]) && /\\begin\s*\{document\}/.test(texts[n]));
  }
  /** A new project: a .zip or a folder, its root found, the samples set aside. */
  async function project(files, label) {
    const decoder = new TextDecoder();
    const texts = Object.fromEntries(Object.entries(files).filter(([n]) => TEXT.test(n)).sort(([a], [b]) => a.localeCompare(b)).map(([n, d]) => [n, decoder.decode(d)]));
    const found = roots(texts);
    if (!found.length) throw new Error('no root document: no file has \\documentclass and \\begin{document}');
    Object.assign(state, { dir: '/work/upload', texts, original: { ...texts }, root: found[0], file: found[0], upload: true, ro: {} });
    rootBox.innerHTML = found.map((n) => `<option>${escape(n)}</option>`).join('');
    worker.postMessage({ type: 'project', dir: state.dir, files, replace: true });
    drawTabs();
    editor.set(texts[state.file]);
    history.replaceState(null, '', location.pathname);
    toast(`${label}: ${Object.keys(files).length} files, root ${state.root}${found.length > 1 ? ` (${found.length} candidates: choose under root)` : ''}`);
    await restart();
    args.value = 'lint';
  }
  /** Single files join the open project as tabs; images and the like only its file system. */
  async function add(files) {
    const decoder = new TextDecoder();
    const texts = Object.entries(files).filter(([n]) => TEXT.test(n)).map(([n, d]) => [n, decoder.decode(d)]);
    Object.assign(state.texts, Object.fromEntries(texts));
    worker.postMessage({ type: 'project', dir: state.dir, files });
    if (state.upload) {
      const all = roots(Object.fromEntries(Object.entries(state.texts).filter(([n]) => !(n in state.ro))));
      rootBox.innerHTML = all.map((n) => `<option${n === state.root ? ' selected' : ''}>${escape(n)}</option>`).join('');
    }
    await restart();
    drawTabs();
    if (texts.length) choose(texts[0][0]);
    const n = Object.keys(files).length;
    toast(`Added ${n === 1 ? Object.keys(files)[0] : `${n} files`}`);
  }
  /** Whatever was picked or dropped, as [path, File] pairs: a folder keeps its paths. */
  async function upload(entries) {
    if (!editor || !entries.length) return;
    try {
      const total = entries.reduce((n, [, f]) => n + f.size, 0);
      if (total > LIMIT) throw new Error(`more than ${LIMIT / 2 ** 20} MiB`);
      const read = async (f) => new Uint8Array(await f.arrayBuffer());
      if (entries.length === 1 && /\.zip$/i.test(entries[0][0])) return await project(await unzip(await read(entries[0][1])), entries[0][0]);
      const files = {};
      for (const [p, f] of entries) if (!/(^|\/)(__MACOSX|\.git)\//.test(p)) files[p] = await read(f);
      const names = Object.keys(files), top = names[0].split('/')[0] + '/';
      if (names.every((n) => n.startsWith(top))) return await project(Object.fromEntries(names.map((n) => [n.slice(top.length), files[n]])), top.slice(0, -1));
      await add(files);
    } catch (e) { toast(`Upload: ${e.message}`); }
  }
  /** A dropped item's files, folders walked. */
  async function walk(entry, into) {
    if (entry.isFile) into.push([entry.fullPath.replace(/^\//, ''), await new Promise((res, rej) => entry.file(res, rej))]);
    else if (entry.isDirectory) {
      const reader = entry.createReader();
      for (let batch; (batch = await new Promise((res, rej) => reader.readEntries(res, rej))).length;) for (const e of batch) await walk(e, into);
    }
    return into;
  }
  rootBox.onchange = async () => { state.root = rootBox.value; await restart(); };
  const picked = (e) => { upload([...e.target.files].map((f) => [f.webkitRelativePath || f.name, f])); e.target.value = ''; };
  $('play-files').onchange = $('play-folder').onchange = picked;
  const upMenu = $('play-upmenu');
  $('play-upload').onclick = () => { upMenu.hidden = !upMenu.hidden; };
  upMenu.onclick = (e) => { const b = e.target.closest('[data-pick]'); if (b) { upMenu.hidden = true; $(`play-${b.dataset.pick}`).click(); } };
  // Files dragged over the page, the editor too: the drop zone covers it until the drop,
  // Escape, a click, or the drag leaving the window.
  const drop = $('play-drop');
  const dragging = (e) => e.dataTransfer?.types.includes('Files');
  addEventListener('dragover', (e) => { if (dragging(e)) { e.preventDefault(); e.stopPropagation(); drop.hidden = false; } }, true);
  addEventListener('dragleave', (e) => { if (!e.relatedTarget || e.target === drop) drop.hidden = true; }, true);
  addEventListener('dragend', () => { drop.hidden = true; });
  drop.onclick = () => { drop.hidden = true; };
  addEventListener('drop', async (e) => {
    if (!dragging(e)) return;
    e.preventDefault();
    e.stopPropagation();
    drop.hidden = true;
    const items = [...e.dataTransfer.items].map((i) => i.webkitGetAsEntry?.()).filter(Boolean);
    upload(items.length ? (await Promise.all(items.map((i) => walk(i, [])))).flat() : [...e.dataTransfer.files].map((f) => [f.name, f]));
  }, true);

  // ---------------------------------------------------------------- permalinks

  const b64 = (bytes) => btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  const unb64 = (s) => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));
  const through = async (bytes, stream) => new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(stream)).arrayBuffer());
  async function permalink() {
    sync();
    const data = { f: state.file, t: editor.text(), a: args.value, k: manifest.kernels[state.kernel].id, c: editor.cursor() };
    const packed = await through(new TextEncoder().encode(JSON.stringify(data)), new CompressionStream('deflate-raw'));
    let s = '';
    for (let i = 0; i < packed.length; i += 0x8000) s += String.fromCharCode(...packed.subarray(i, i + 0x8000));
    return `${location.href.split('#')[0]}#s=${btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')}`;
  }
  async function restore(hash) {
    try { return JSON.parse(new TextDecoder().decode(await through(unb64(hash), new DecompressionStream('deflate-raw')))); } catch { return null; }
  }
  $('play-link').onclick = async () => {
    const link = await permalink();
    history.replaceState(null, '', link);
    try { await navigator.clipboard.writeText(link); } catch {}
    toast(state.upload ? 'Link copied. It holds only the open file, not the whole project.' : 'Link copied');
  };

  // ---------------------------------------------------------------- start

  worker.onmessage = async ({ data }) => {
    if (data.type === 'lsp') {
      for (const m of data.out) {
        if (m.id !== undefined && pending.has(m.id)) {
          const p = pending.get(m.id);
          pending.delete(m.id);
          if (m.error) p.reject(new Error(m.error.message.replace(/^[\w/]+: /, ''))); else p.resolve(m.result ?? null);
        } else if (m.method === 'textDocument/publishDiagnostics' && m.params.uri.startsWith(`file://${state.dir}/`)) show(m.params.uri, m.params.diagnostics);
      }
    } else if (data.type === 'error' && data.of === 'lsp') {
      lspStatus.textContent = `language server failed: ${data.message.split('\n')[0]}`;
      for (const p of pending.values()) p.resolve(null);
      pending.clear();
    } else if (data.type === 'error') {
      waiting[REPLY[data.of]]?.reject(new Error(data.message.split('\n')[0]));
    } else {
      waiting[data.type]?.resolve(data);
    }
  };

  /** A build file's bytes on file://, where fetch fails: build.py's `.js` copy sets them as base64. */
  async function local(file) {
    try { return new Uint8Array(await (await fetch(base + file)).arrayBuffer()); } catch {}
    await new Promise((resolve, reject) => document.head.append(Object.assign(document.createElement('script'),
      { src: base + file + '.js', onload: resolve, onerror: () => reject(new Error(`${file}.js did not load`)) })));
    const text = self.SATEX_FILES[file];
    delete self.SATEX_FILES[file];
    return Uint8Array.fromBase64?.(text) ?? Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
  }

  (async () => {
    let given;
    if (location.protocol === 'file:') {
      const [js, wasm, bundle] = await Promise.all([manifest.js, manifest.wasm, manifest.kernels[0].bundle].map(local));
      given = { js, wasm, bundle };
    }
    const ready = await ask({ type: 'init', base, manifest, given }, given ? Object.values(given).map((b) => b.buffer) : []);
    timing('t-init', `init ${secs(performance.now() - started)}`, 'time to fetch satex and the pdfTeX kernel and set them up');
    sampleTexts = ready.samples;
    const hash = decodeURIComponent(location.hash.slice(1));
    const link = hash.startsWith('s=') ? await restore(hash.slice(2)) : null;
    if (link && !(link.f in sampleTexts)) sampleTexts[link.f] = '';
    const file = link?.f ?? (hash in sampleTexts ? hash : manifest.sample);
    if (hash.startsWith('open=')) history.replaceState(null, '', location.pathname);
    Object.assign(state, { texts: { ...sampleTexts, ...(link ? { [link.f]: link.t } : {}) }, original: sampleTexts, file });
    args.value = link ? link.a : commandOf(file);
    const k = manifest.kernels.findIndex((x) => x.id === link?.k);
    if (k > 0) await kernel(k);
    kernelBox.value = state.kernel;
    try {
      editor = await codemirror($('play-editor'), state.texts[file]);
      $('play-source').remove();
    } catch (e) {
      console.warn('CodeMirror did not load, the editor is a textarea', e);
      editor = textarea($('play-source'));
      editor.set(state.texts[file]);
    }
    if (link?.c) editor.select(link.c);
    drawTabs();
    button.textContent = 'Run';
    button.disabled = false;
    await restart();
    const opening = hash.match(/^open=(.+):(\d+)$/);
    if (opening) await openPlace(opening[1], { line: Number(opening[2]) - 1, character: 0 });
    if (!plainLint()) return start();
    status.textContent = 'Ready. Findings update as you type; Enter runs the command.';
    status.dataset.done = 'ready';
  })().catch((e) => { status.textContent = `the playground failed: ${e.message}`; status.dataset.done = 'error'; });
})();
