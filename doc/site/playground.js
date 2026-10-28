// The playground's page side: a CodeMirror editor, satex's language server for
// findings, fixes, completion, hover, definitions and rename, and the `satex`
// command line for runs. satex itself runs in worker.js. Without the CDN the
// editor is a textarea. A classic script, so the page also works from disk.
(() => {
  const $ = (id) => document.getElementById(id);
  const manifest = JSON.parse($('satex-play').textContent);
  const base = new URL(manifest.root, document.baseURI).href;
  const [tabs, args, button, status, output, lspStatus, list, count, cmd, kernelBox, toastBox, ctxMenu, treeBox, examplesBox] =
    ['play-tabs', 'play-args', 'play-run', 'play-status', 'play-output', 'lsp-status', 'lsp-diagnostics', 'lsp-count',
      'play-cmd', 'play-kernel', 'play-toast', 'play-ctx', 'play-files-tree', 'play-examples'].map($);
  // Panes snap, as in flowR's playground: a splitter dragged into a pane's last half
  // minimum folds that pane away, a click on the folded splitter brings it back; near a
  // third, half or two thirds it rests there. A section's grip sets its height, dragged
  // under half the minimum it closes the section. Sizes are kept per browser.
  let foldConsole; // the satex console's fold, set below
  const PANES = { min: 15, snap: [1 / 3, 1 / 2, 2 / 3], within: 1, minSection: 4 }; // rem, fractions
  {
    const rem = () => parseFloat(getComputedStyle(document.documentElement).fontSize);
    const clamp = (x, lo, hi) => Math.min(Math.max(x, lo), hi);
    let kept = {};
    try { kept = JSON.parse(localStorage.getItem('satex-panes')) ?? {}; } catch {}
    const keep = () => { try { localStorage.setItem('satex-panes', JSON.stringify(kept)); } catch {} };
    /** Drags `grip`: `to(move, down, from())` sizes while it moves, `done(moved)` ends it. */
    const drag = (grip, from, to, done) => grip.addEventListener('pointerdown', (e) => {
      if (e.button) return;
      e.preventDefault();
      grip.setPointerCapture(e.pointerId);
      grip.classList.add('drag');
      const start = from();
      let moved = false;
      const move = (m) => { moved ||= Math.abs(m.clientX - e.clientX) + Math.abs(m.clientY - e.clientY) > 3; if (moved) to(m, e, start); };
      const up = () => { grip.classList.remove('drag'); grip.removeEventListener('pointermove', move); grip.removeEventListener('lostpointercapture', up); done(moved); };
      grip.addEventListener('pointermove', move);
      grip.addEventListener('lostpointercapture', up);
    });

    const divider = $('play-divider'), split = divider.parentElement;
    let open = kept.open ?? '50%'; // the split a folded pane comes back to
    const place = (v) => {
      split.style.setProperty('--split', v);
      const f = parseFloat(v);
      if (f <= 0 || f >= 100) split.dataset.fold = f <= 0 ? 'editor' : 'side';
      else { delete split.dataset.fold; open = v; }
      divider.setAttribute('aria-valuenow', Math.round(f));
      Object.assign(kept, { split: v, open });
    };
    const at = (x) => {
      const b = split.getBoundingClientRect(), min = PANES.min * rem(), px = x - b.left;
      if (px < min / 2) return '0%';
      if (b.width - px < min / 2) return '100%';
      const f = clamp(px, min, b.width - min) / b.width;
      const snapped = PANES.snap.find((s) => Math.abs(s - f) * b.width < PANES.within * rem());
      return `${((snapped ?? f) * 100).toFixed(1)}%`;
    };
    if (/^[\d.]+%$/.test(kept.split ?? '')) place(kept.split);
    drag(divider, () => 0, (m) => place(at(m.clientX)), (moved) => { if (!moved && split.dataset.fold) place(open); keep(); });
    divider.addEventListener('dblclick', () => { place('50%'); keep(); });
    divider.addEventListener('keydown', (e) => {
      const f = parseFloat(split.style.getPropertyValue('--split')) || 50;
      const to = { ArrowLeft: `${clamp(f - 5, 0, 100)}%`, ArrowRight: `${clamp(f + 5, 0, 100)}%`, Home: '0%', End: '100%', Enter: split.dataset.fold ? open : '50%' }[e.key];
      if (to) { e.preventDefault(); place(to); keep(); }
    });

    // each section of the side pane: a grip under its body
    kept.heights ??= {};
    for (const sect of document.querySelectorAll('.side-pane > details.sect')) {
      const body = sect.querySelector('.grow');
      if (!body) continue;
      const grip = Object.assign(document.createElement('div'), { className: 'grip', title: 'drag to resize, down to nothing to close; double-click: fit' });
      grip.setAttribute('role', 'separator');
      grip.setAttribute('aria-orientation', 'horizontal');
      body.after(grip);
      const height = (px) => { body.style.setProperty('--h', px == null ? '' : `${px}px`); if (px == null) delete kept.heights[sect.id]; else kept.heights[sect.id] = px; };
      if (kept.heights[sect.id]) height(kept.heights[sect.id]);
      drag(grip, () => body.getBoundingClientRect().height, (m, e, h) => {
        const min = PANES.minSection * rem(), want = h + m.clientY - e.clientY;
        if (want < min / 2) { sect.open = false; height(h); return; }
        height(Math.round(clamp(want, min, innerHeight * 0.85)));
      }, keep);
      grip.addEventListener('dblclick', () => { height(null); keep(); });
    }

    // the file tree: a toggle in the tab row, a grip for its width; folded on narrow screens
    const tree = $('play-tree'), toggle = $('play-treetoggle'), work = tree.parentElement;
    const showTree = (on) => { tree.hidden = !on; toggle.setAttribute('aria-expanded', on); };
    showTree(kept.tree ?? !matchMedia('(max-width: 52rem)').matches);
    toggle.onclick = () => { showTree(tree.hidden); kept.tree = !tree.hidden; keep(); };
    if (kept.treeWidth) work.style.setProperty('--tree', `${kept.treeWidth}px`);
    drag($('play-treegrip'), () => tree.getBoundingClientRect().width, (m, e, w) => {
      const want = w + m.clientX - e.clientX;
      if (want < PANES.min * rem() / 3) return showTree(false);
      kept.treeWidth = Math.round(clamp(want, 7 * rem(), work.getBoundingClientRect().width / 2));
      work.style.setProperty('--tree', `${kept.treeWidth}px`);
    }, keep);
    // the satex console folds away; the state is remembered
    foldConsole = (folded, remember = true) => {
      $('play-termbody').hidden = folded;
      $('play-termtoggle').setAttribute('aria-expanded', !folded);
      $('play-term').classList.toggle('folded', folded);
      if (remember) { kept.console = folded; keep(); }
    };
    $('play-termtoggle').onclick = () => foldConsole(!$('play-termbody').hidden);
    foldConsole(!!kept.console, false);
    // the run output over the command line: its grip sets its height
    const out = $('play-out');
    if (kept.term) out.style.setProperty('--h', `${kept.term}px`);
    drag($('play-termgrip'), () => out.getBoundingClientRect().height, (m, e, h) => {
      kept.term = Math.round(clamp(h - (m.clientY - e.clientY), 4 * rem(), innerHeight * 0.75));
      out.style.setProperty('--h', `${kept.term}px`);
    }, keep);
  }
  // The worker's code comes with the page: file:// pages cannot start a worker from a file.
  const worker = new Worker(URL.createObjectURL(new Blob([manifest.worker], { type: 'text/javascript' })));
  const escape = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  const SEVERITY = ['', 'error', 'warning', 'info', 'hint'];
  const sev = (d) => SEVERITY[d.severity] || 'info';
  const secs = (ms) => `${(ms / 1000).toFixed(2)} s`;
  const size = (n) => n < 2 ** 20 ? `${Math.ceil(n / 1024)} KiB` : `${(n / 2 ** 20).toFixed(1)} MiB`;
  const started = performance.now();

  // The project, as in flowR's playground: its files in the tree, the open ones as tabs, one
  // root that runs and the language server analyze, its \input files with it. An example
  // (`manifest.projects` names the files it reads besides itself) or an upload is a project.
  // `texts` holds every file under `dir`: an example's edits stay while another one is open.
  // Read-only files (`ro`) are kernel or package files opened by a definition: never sent or run.
  const state = { dir: '/work', texts: {}, file: null, root: manifest.sample, kernel: 0, loaded: new Set([0]), upload: false, ro: {},
    example: manifest.sample, files: [], open: [], lists: {} };
  const projects = manifest.projects ?? {};
  const shared = new Set(Object.values(projects).flat());
  const projectOf = (name) => name in projects ? name : Object.keys(projects).find((r) => projects[r].includes(name)) ?? null;
  /** Data files (a database, a style) go to the file system, not to the language server as TeX. */
  const data = (name) => /\.(bib|bst|bbl|txt|md|ya?ml)$/i.test(name);
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
  // The status bar: one time, a pill per non-empty severity; the rest in a pop-up on the time.
  const shownTime = (text) => { $('t-time').hidden = false; $('t-time-text').textContent = text; };
  $('t-time').onclick = () => { const pop = $('t-detail'); pop.hidden = !pop.hidden; $('t-time').setAttribute('aria-expanded', !pop.hidden); };
  addEventListener('mousedown', (e) => { if (!e.target.closest('.statdetail')) { $('t-detail').hidden = true; $('t-time').setAttribute('aria-expanded', false); } });
  function pills(ds) {
    const n = (s) => ds.filter((d) => (sev(d) === 'hint' ? 'info' : sev(d)) === s).length;
    const shown = [['error', 'error'], ['warning', 'warning'], ['info', 'suggestion']].map(([s, w]) => [s, w, n(s)]).filter(([, , c]) => c);
    $('play-pills').innerHTML = shown.map(([s, w, c]) => `<span class="pill-sev sev-${s}" title="${c} ${w}${c > 1 ? 's' : ''} in the open file">${c} ${w}${c > 1 ? 's' : ''}</span>`).join('');
  }

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
  const unescape = (s) => s.replace(/&(lt|gt|quot|amp);/g, (m, e) => ({ lt: '<', gt: '>', quot: '"', amp: '&' })[e]);
  /** A hover's Markdown: its usage lines colored, each ⟨argN⟩ elsewhere in the kind its usage gave it. */
  const card = (value) => {
    // each definition is its own section: its types belong to its own usage line
    const section = (md) => {
      const kinds = {};
      // the argument types the section lists (`- `⟨arg1⟩` glue`, roles::Ty), shown in the usage line too
      const types = Object.fromEntries([...md.matchAll(/^- `⟨(arg\d+)⟩` ((?:\w+ )?(?:register|name)|number|dimen|glue|muglue|quantity|box|key|file|counter|control sequence|options|any)$/gm)].map((m) => [m[1], m[2]]));
      const html = markdown(md).replace(/<code>([^<]*⟨arg\d+⟩[^<]*)<\/code>/g, (m, code) => {
        if (/^⟨arg\d+⟩$/.test(code)) return m;
        const u = cards.usage(unescape(code), types);
        Object.assign(kinds, u.kinds);
        return `<code class="usage">${u.html}</code>`;
      });
      return cards.slots(html, kinds).replace(/<p>((?:\s|·|<a [^>]*>[^<]*<\/a>)+)<\/p>$/, '<p class="foot">$1</p>');
    };
    const sections = (md) => md.split(/^(?=### )/m).map(section).join('');
    // the definitions that do not hold at the cursor fold away (lsp::handlers::MORE_DEFINITIONS)
    const [main, more] = value.split(/^#### More definitions\n+/m);
    return sections(main) + (more ? `<details class="more"><summary>More definitions</summary>${sections(more)}</details>` : '');
  };

  // ---------------------------------------------------------------- worker requests

  const waiting = {}; // reply type -> resolver; runs, reads and kernel loads go one at a time, in order
  const REPLY = { run: 'result', bundle: 'bundle', bundles: 'bundle', init: 'ready', read: 'file' };
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
  // The worker answers each message in turn: those that run an analysis, not
  // yet answered, are `running`, and what it answered before stands in meanwhile.
  const order = [];
  let running = 0;
  function lsp(method, params, notify = false) {
    const message = { jsonrpc: '2.0', method, params, ...(notify ? {} : { id: nextId++ }) };
    const runs = method === 'satex/analyze' || method === 'textDocument/didOpen';
    order.push(runs);
    running += runs;
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
        if (name) { state.texts[name] = edited(state.texts[name], edits); if (opened.has(target)) { lsp('textDocument/didChange', { textDocument: { uri: target, version: ++version }, contentChanges: [{ text: state.texts[name] }] }, true); analyze(); } }
      }
    }
    return n;
  }

  // The editor's modules, fetched while satex loads rather than after it.
  const EDITOR = ['codemirror@6.0.2', '@codemirror/language@6', '@codemirror/lint@6', '@codemirror/autocomplete@6', '@codemirror/view@6',
    '@codemirror/state@6', '@codemirror/theme-one-dark@6'].map((m) => import(`https://esm.sh/${m}`));
  for (const loading of EDITOR) loading.catch(() => {}); // a failure shows when the editor is built

  // ---------------------------------------------------------------- editor

  let editor; // { text(), set(text), diagnostics(list), apply(edits), cursor(), select(c), word() }
  let version = 0, synced = 0, timer = null, checkStart = 0;
  function changed() {
    version++;
    if (refs && refs.file === state.file) clearRefs();
    clearTimeout(timer);
    timer = setTimeout(sync, 400);
  }
  /** Sends the buffer to the server unless it already has it: the text
   * only, so what is asked meanwhile is answered from the last analysis. */
  function push() {
    if (!state.file || state.file in state.ro) return false;
    state.texts[state.file] = editor.text();
    if (data(state.file) && synced !== version) {
      synced = version;
      worker.postMessage({ type: 'project', dir: state.dir, files: { [state.file]: new TextEncoder().encode(editor.text()) } });
      return true;
    }
    if (synced === version || !opened.has(uri())) return false;
    synced = version;
    lsp('textDocument/didChange', { textDocument: { uri: uri(), version }, contentChanges: [{ text: editor.text() }] }, true);
    return true;
  }
  /** Once typing pauses: the buffer, and an analysis of it. */
  function sync() {
    clearTimeout(timer);
    if (push()) {
      // the root's text may now ask for another engine
      if (state.file === state.root && wanted() !== state.kernel && !manualKernel && !following) { following = true; restart().finally(() => { following = false; }); return; }
      analyze(); explainSoon();
    }
  }
  function analyze() { checking(); lsp('satex/analyze', {}, true); }
  const checkBox = $('lsp-check');
  /** The indeterminate bar of a running check, with the phase it is in; the worker reports none, the waits for packages are the phases. */
  const phase = (text, done = null) => {
    checkBox.hidden = !text;
    checkBox.lastElementChild.textContent = text ?? '';
    lspStatus.textContent = text ?? '';
    // how much of the file is read, once the engine says; until then the bar sweeps
    const bar = checkBox.firstElementChild;
    bar.classList.toggle('known', done !== null);
    bar.style.setProperty('--done', done ?? 0);
  };
  function checking() { phase('checking…'); checkStart = performance.now(); }

  /** Completion order: the project's own commands, then the documented ones, internals
   * (`@`, `_`, `:` in the name) last; within each, prefix matches, then shorter names. */
  const tier = (i) => /\(document\)$/.test(i.detail ?? '') ? 0 : /[@_:]/.test(i.label) ? 2 : 1;
  const rank = (items, typed) => items.filter((i) => i.label.toLowerCase().includes(typed.toLowerCase()))
    .map((i) => [i, tier(i), i.label.startsWith(typed) ? 0 : 1])
    .sort((a, b) => a[1] - b[1] || a[2] - b[2] || a[0].label.length - b[0].label.length || a[0].label.localeCompare(b[0].label)).map((x) => x[0]);
  /** The language server's completion items at `position`, kept per buffer version. */
  const asked = new Map();
  let lastItems = null;
  function completions(position) {
    const k = `${uri()}|${version}|${position.line}:${position.character}`;
    if (!asked.has(k)) {
      asked.clear();
      asked.set(k, lsp('textDocument/completion', { textDocument: { uri: uri() }, position }).then((r) => (lastItems = (Array.isArray(r) ? r : r?.items) ?? [])).catch(() => []));
    }
    return running && lastItems ? lastItems : asked.get(k);
  }
  /** The last hover of each command, by file: shown at once while a run goes on. */
  const keptHovers = new Map();
  /** The command (or key) at offset `col` of `line`: [from, to, name, isCommand]. */
  function wordAt(line, col) {
    for (const m of line.matchAll(/\\([A-Za-z@]+|.)|[A-Za-z0-9_:.+*-]+/g))
      if (m.index <= col && col <= m.index + m[0].length) return [m.index, m.index + m[0].length, m[1] ?? m[0], m[0][0] === '\\'];
    return null;
  }

  async function codemirror(parent, text) {
    const [{ EditorView, basicSetup }, language, lint, ac, view, st, { oneDark }, tex] = await Promise.all(EDITOR.concat(self.siteTex()));
    /** Words (file names in arguments) the server resolves a definition for, by start offset. */
    const resolves = new Map();
    const KIND = { 3: 'function', 14: 'keyword', 9: 'namespace', 18: 'variable' };
    async function complete(ctx) {
      // a command's name, or a key or value in a list (`/demo/mode=fa`)
      const word = ctx.matchBefore(/[A-Za-z@*_:]*/), key = ctx.matchBefore(/[\w@*:./-]*/);
      const before = ctx.state.sliceDoc(word.from - 1, word.from), line = ctx.state.doc.lineAt(ctx.pos);
      // inside a {…} or […] the cursor's list item starts after `{`, `[`, `,` or `=`
      const open = [...line.text.slice(0, key.from - line.from)].reduce((d, c) => d + ('{['.includes(c)) - ('}]'.includes(c)), 0) > 0;
      const item = open && /[{[,=]\s*$/.test(line.text.slice(0, key.from - line.from));
      if (!ctx.explicit && before !== '\\' && before !== '{' && !item) return null;
      push();
      const items = await completions(position(ctx.state.doc, ctx.pos));
      // an item that replaces a range (a key path) starts where that range does
      const edit = items.find((i) => i.textEdit)?.textEdit;
      const from = edit ? at(ctx.state.doc, edit.range.start) : before === '\\' ? word.from : key.from;
      const typed = ctx.state.sliceDoc(from, ctx.pos);
      return {
        from, to: edit ? Math.max(at(ctx.state.doc, edit.range.end), ctx.pos) : ctx.pos, filter: false,
        options: rank(items, typed).slice(0, 200).map((i) => ({
          label: i.label, usage: i.detail, type: KIND[i.kind] ?? 'text',
          apply: i.insertTextFormat === 2 ? ac.snippet((i.insertText ?? i.textEdit?.newText ?? i.label).replace(/\$\{?(\d+)\}?/g, '#{$1}')) : i.textEdit?.newText ?? i.insertText ?? i.label,
        })),
      };
    }
    const hover = view.hoverTooltip(async (v, pos) => {
      // The server has the text; it answers from its last analysis at once.
      push();
      const line = v.state.doc.lineAt(pos), word = wordAt(line.text, pos - line.from);
      const key = word?.[3] ? `${uri()}|${word[2]}` : null;
      const asking = lsp('textDocument/hover', { textDocument: { uri: uri() }, position: position(v.state.doc, pos) }).catch(() => null)
        .then((r) => { if (key) keptHovers.set(key, r); return r; });
      const result = running && keptHovers.has(key) ? keptHovers.get(key) : await asking;
      const c = result?.contents;
      const value = typeof c === 'string' ? c : c?.value ?? (Array.isArray(c) ? c.map((x) => x.value ?? x).join('\n') : '');
      if (!value) return null;
      const range = result.range;
      return {
        pos: range ? at(v.state.doc, range.start) : pos, end: range ? at(v.state.doc, range.end) : pos,
        create: () => { const dom = document.createElement('div'); dom.className = 'hover'; dom.innerHTML = card(value); return { dom }; },
      };
    }, { hoverTime: 250, hideOnChange: true });
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
      extensions: [basicSetup, tex.language, language.syntaxHighlighting(tex.highlighter), lint.lintGutter(), ac.autocompletion({ override: [complete], addToOptions: [{ position: 80, render: (c) => {
        // the detail: a usage line colored like the hover's, then where it comes from
        const span = Object.assign(document.createElement('span'), { className: 'cm-completionDetail' });
        const [, use, from] = (c.usage ?? '').match(/^(.*?)(?:\s+(\([^)]*\)))?$/) ?? [];
        span.innerHTML = (use ? cards.usage(use).html : '') + (from ? ` <span class="muted">${escape(from)}</span>` : '');
        return span;
      } }] }), hover,
        linkField, markField, view.keymap.of([{ key: 'F2', run: () => { renameBox(); return true; } }]),
        EditorView.updateListener.of((u) => { if (u.docChanged) { resolves.clear(); changed(); } else if (u.selectionSet) explainSoon(); }), theme.of([]), lock.of([])],
    });
    // the site's light/dark choice (site.js) picks the editor's theme
    const scheme = () => ev.dispatch({ effects: theme.reconfigure(self.siteDark?.() ? oneDark : []) });
    new MutationObserver(scheme).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', scheme);
    scheme();
    const doc = () => ev.state.doc;
    /** The command, or a word that resolves to a file, under the mouse: its range, or null. */
    const under = (e) => {
      const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos == null) return null;
      const l = doc().lineAt(pos), w = wordAt(l.text, pos - l.from);
      return w && (w[3] || resolves.get(l.from + w[0])) ? [l.from + w[0], l.from + w[1]] : null;
    };
    function resolve(e) {
      const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos == null) return;
      const l = doc().lineAt(pos), w = wordAt(l.text, pos - l.from);
      if (!w || w[3] || resolves.has(l.from + w[0])) return;
      resolves.set(l.from + w[0], false);
      lsp('textDocument/definition', { textDocument: { uri: uri() }, position: position(doc(), pos) }).catch(() => null).then((r) => {
        if (!(Array.isArray(r) ? r.length : r)) return;
        resolves.set(l.from + w[0], true);
        underline(under(e));
      });
    }
    let linked = null;
    const underline = (r) => { if (String(r) !== String(linked)) { linked = r; ev.dispatch({ effects: link.of(r) }); } };
    parent.addEventListener('mousemove', (e) => { const ctrl = e.ctrlKey || e.metaKey; if (ctrl) resolve(e); underline(ctrl ? under(e) : null); const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY }); if (pos != null) explainSoon(position(doc(), pos)); });
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
  /** What the open file's paragraphs set, from the language server's last
   * analysis (`satex query typeset`); a click goes to the paragraph. */
  const typesetBody = $('typeset-body');
  /** satex's notation in typeset text (`\uE000…\uE001`): `⟦…⟧` is text set in a box, on the page
   * only if the box is used; `·` a place the line breaker may hyphenate, not shown. */
  function notation(marked) {
    let open = 0;
    const html = marked.split(/(\uE000[^\uE001]*\uE001)/).map((part) => {
      const note = /^\uE000([^\uE001]*)\uE001$/.exec(part)?.[1];
      if (note === undefined) return escape(part);
      if (note === '⟦') { open++; return '<span class="ts-box" title="set in a box: on the page only if the box is used">'; }
      if (note === '⟧') { if (!open) return ''; open--; return '</span>'; }
      return note === '·' ? '' : `<span class="ts-note">${escape(note)}</span>`;
    }).join('');
    return html + '</span>'.repeat(open);
  }
  const boxesToggle = $('typeset-boxes');
  try { boxesToggle.checked = localStorage.getItem('satex-typeset-boxes') === '1'; } catch {}
  const showBoxes = () => typesetBody.classList.toggle('show-boxes', boxesToggle.checked);
  boxesToggle.onchange = () => { showBoxes(); try { localStorage.setItem('satex-typeset-boxes', boxesToggle.checked ? '1' : '0'); } catch {} };
  showBoxes();
  async function typeset(target) {
    if (target !== uri()) return;
    const rows = await lsp('satex/typeset', { textDocument: { uri: target } }).catch(() => null);
    if (!rows || target !== uri()) return;
    const paragraphs = rows.filter((r) => r.kind === 'paragraph');
    const row = (r) => `<p class="ts${r.heading ? ` ts-h ts-${escape(r.heading)}` : ''}" data-path="${escape(r.path ?? '')}" data-line="${r.line}" data-col="${r.col}"><span class="ts-at">${r.line}</span><span class="ts-text">${notation(r.marked ?? r.detail ?? '')}</span></p>`;
    // the whole document in order: this file's paragraphs as they are, a run from another file folded into one line
    const html = [];
    for (let i = 0; i < paragraphs.length;) {
      const r = paragraphs[i];
      if (r.path?.endsWith(state.file)) { html.push(row(r)); i++; continue; }
      let j = i;
      while (j < paragraphs.length && paragraphs[j].path === r.path) j++;
      const run = paragraphs.slice(i, j), name = (r.path ?? '').split('/').pop();
      html.push(`<details class="ts-other"><summary>⟨${escape(name)}⟩ · ${run.length} paragraph${run.length > 1 ? 's' : ''}</summary>${run.map(row).join('')}</details>`);
      i = j;
    }
    typesetBody.innerHTML = html.length ? html.join('') : '<p class="muted">The document sets no paragraph.</p>';
  }
  typesetBody.addEventListener('click', (e) => {
    const p = e.target.closest('.ts');
    if (!p) return;
    const at = { line: +p.dataset.line - 1, character: Math.max(+p.dataset.col - 1, 0) };
    if (p.dataset.path && !p.dataset.path.endsWith(state.file)) {
      const name = Object.keys(state.texts).find((n) => !(n in state.ro) && p.dataset.path.endsWith(n));
      openPlace(name ?? p.dataset.path, at);
    } else editor.focus({ start: at, end: at });
  });

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
    drawTree();
  }
  /** Each finding: its severity, place, rule and message once. */
  function render() {
    const ds = diagnostics[uri()] ?? [];
    editor.diagnostics(ds);
    count.textContent = ds.length;
    pills(ds);
    if (checkStart) { timing('t-check', secs(performance.now() - checkStart), 'the language server analyzed the open file'); shownTime(secs(performance.now() - checkStart)); }
    checkStart = 0;
    phase();
    lspStatus.textContent = state.file in state.ro ? 'read-only file' : ds.length ? '' : 'none';
    const fixButton = (f, j, cls) => `<button class="small apply${cls}" data-j="${j}" title="${escape(f.title)}">Fix <span class="fix fix-${f.safety}">${f.safety}</span></button>`;
    const item = (d, i) => `<li class="f-${sev(d)}${d.fixes.length ? ' fixable' : ''}${unfolded.has(key(d)) ? ' open' : ''}" data-i="${i}" tabindex="0">` +
      `<div class="f-head"><span class="msg"><b class="rn">${escape(d.code ?? '')}</b>: ${inline(d.message)}</span><span class="badge"><span class="sev sev-${sev(d)}" title="${sev(d)}" aria-label="${sev(d)}"></span>${d.fixes.length ? fixButton(d.fixes[0], 0, ' as-badge') : ''}</span>` +
      `<b class="loc">${d.range.start.line + 1}:${d.range.start.character + 1}</b></div><span class="acts">` +
      d.fixes.slice(1).map((f, j) => fixButton(f, j + 1, '')).join('') +
      `<button class="small ghost why">${unfolded.has(key(d)) ? 'Hide rule' : 'Explain rule'}</button></span>` +
      (unfolded.has(key(d)) ? explanation(d) : '') + '</li>';
    /** Findings of one object (`group`, "table@12:1") fold together under a header; a lone one stays flat. */
    const fold = (id, cls, head, rows, inner) => `<li class="${cls}"><details${folded.has(id) ? '' : ' open'} data-fold="${escape(id)}"><summary class="cat-head" aria-label="${escape(head)}">${inner}</summary>` +
      `<ul class="cat-items">${rows}</ul></details></li>`;
    const grouped = (cat, rows) => {
      const by = Map.groupBy(rows, ([d]) => groupOf(d) ?? Symbol());
      return [...by].map(([g, rs]) => {
        if (typeof g !== 'string' || rs.length < 2) return rs.map(([d, i]) => item(d, i)).join('');
        const [kind, at] = g.split('@');
        const label = `${kind[0].toUpperCase()}${kind.slice(1)}${at ? ` at ${at}` : ''}`;
        return fold(`${cat}|${g}`, `obj-group og-${['error', 'warning'].find((s) => rs.some(([d]) => sev(d) === s)) ?? 'info'}`, `${label}: ${rs.length} findings`,
          rs.map(([d, i]) => item(d, i)).join(''), `<span class="cat-name">${escape(label)}</span><span class="cat-count">${rs.length}</span>`);
      }).join('');
    };
    // grouped by the rule's category, as the command line prints them: a header with
    // the category's icon, its count and severities, folding its findings
    const groups = new Map(CATEGORIES.map(([cat]) => [cat, []]));
    ds.forEach((d, i) => {
      const cat = manifest.rules[d.code]?.category ?? 'other';
      groups.set(cat, [...(groups.get(cat) ?? []), [d, i]]);
    });
    list.innerHTML = [...groups].filter(([, rows]) => rows.length).map(([cat, rows]) => {
      const sevs = ['error', 'warning', 'info'].map((s) => [s, rows.filter(([d]) => sev(d) === s || (s === 'info' && sev(d) === 'hint')).length]).filter(([, n]) => n);
      const label = `${cat}: ${rows.length} finding${rows.length > 1 ? 's' : ''}`;
      return fold(cat, `cat-group cat-${escape(cat)}`, label, grouped(cat, rows), `<span class="cat-icon" aria-hidden="true">${catIcon(cat)}</span>` +
        `<span class="cat-name">${escape(cat)}</span><span class="cat-count">${rows.length}</span>` +
        `<span class="cat-sevs" aria-hidden="true">${sevs.map(([s, n]) => `<span class="cat-sev sev-${s}" title="${n} ${s}">${n}</span>`).join('')}</span>`);
    }).join('');
  }
  /** The lint categories in the command line's order, each with its icon (24-unit strokes). */
  const CATEGORIES = [
    ['correctness', '<path d="M12 3l7 3v5c0 4.5-3 8.3-7 10-4-1.7-7-5.5-7-10V6z"/><path d="M9 12l2 2 4-4"/>'],
    ['security', '<rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>'],
    ['style', '<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M13 7l4 4"/>'],
    ['performance', '<path d="M13 3L5 14h6l-1 7 8-11h-6z"/>'],
    ['precision', '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3"/>'],
  ];
  const catIcon = (cat) => `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${(CATEGORIES.find(([c]) => c === cat) ?? [, '<circle cx="12" cy="12" r="4"/>'])[1]}</svg>`;
  /** The categories folded away, kept across checks. */
  const folded = new Set();
  const groupOf = (d) => d.group ?? d.data?.group;
  list.addEventListener('toggle', (e) => {
    const id = e.target.dataset?.fold;
    if (id) { if (e.target.open) folded.delete(id); else folded.add(id); }
  }, true);
  const unfolded = new Set();
  function unfold(i, toggle = true) {
    const d = diagnostics[uri()]?.[i];
    if (!d) return;
    if (toggle && unfolded.has(key(d))) unfolded.delete(key(d)); else unfolded.add(key(d));
    const cat = manifest.rules[d.code]?.category ?? 'other';
    folded.delete(cat); folded.delete(`${cat}|${groupOf(d)}`);
    render();
    list.querySelector(`li[data-i="${i}"]`)?.scrollIntoView({ block: 'nearest', behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
  }
  /** The findings at a place: line, and column unless a gutter marker was clicked. */
  const findingsAt = (line, character) => (diagnostics[uri()] ?? []).map((d, i) => [d, i]).filter(([d]) => d.range.start.line <= line && line <= d.range.end.line &&
    (character == null || ((d.range.start.line < line || d.range.start.character <= character) && (d.range.end.line > line || character <= Math.max(d.range.end.character, d.range.start.character + 1)))));
  function pick(line, character) {
    const [hit] = findingsAt(line, character);
    if (hit) unfold(hit[1], false);
  }
  list.onclick = (e) => {
    const li = e.target.closest('li[data-i]');
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
    if (a.classList.contains('cs') && !(e.ctrlKey || e.metaKey)) return explainName(a.dataset.cs);
    const go = (file, line) => {
      const name = Object.keys(state.texts).find((n) => !(n in state.ro) && (path(n) === file || n === file || file.endsWith('/' + n)));
      return openPlace(name ?? file, { line: line - 1, character: 0 });
    };
    // Ctrl (Cmd) on a command in the Explain panel goes to its definition, as in the editor.
    if (a.classList.contains('cs')) {
      explainRecords(a.dataset.cs).then((rs) => {
        const r = rs.find((x) => x.name === a.dataset.cs && x.path && x.line);
        if (r) go(r.path, r.line); else toast(`No definition found for ${a.dataset.cs}`);
      }).catch(() => toast('No definition found'));
      return;
    }
    go(a.dataset.path, Number(a.dataset.line));
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
    showMenu(items, x, y);
    return true;
  }
  /** The page's one context menu: [label, hint, action] rows at (x, y). */
  function showMenu(items, x, y) {
    ctxMenu.innerHTML = items.map(([label, hint], i) => `<li role="menuitem" tabindex="-1" data-i="${i}">${escape(label)}<span class="muted">${escape(hint)}</span></li>`).join('');
    ctxMenu.hidden = false;
    Object.assign(ctxMenu.style, { left: `${Math.max(4, Math.min(x, innerWidth - ctxMenu.offsetWidth - 8))}px`, top: `${Math.max(4, Math.min(y, innerHeight - ctxMenu.offsetHeight - 8))}px` });
    ctxMenu.onclick = (e) => { const li = e.target.closest('li'); if (li) { ctxMenu.hidden = true; items[li.dataset.i][2](); } };
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
  /** `satex explain name`, answered by the language server from the analysis it holds,
   *  unless the same text was explained already. */
  async function explainRecords(name) {
    sync();
    const target = state.root ?? (state.file in state.ro ? manifest.sample : state.file);
    const k = `${name}|${target}|${version}|${state.kernel}`;
    if (!explanations.has(k)) {
      const request = { file: path(target), queries: [{ type: 'explain', names: [name] }] };
      explanations.set(k, lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] })
        .then((r) => r?.[0]?.records ?? []).catch(() => []));
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
    // the definition in force at the caret first, the others folded
    const text = editor.text(), begin = text.split('\n').findIndex((l) => /\\begin\s*\{document\}/.test(l));
    const now = at && shown.length > 1 ? (begin >= 0 ? at.line < begin : /\\documentclass/.test(text)) ? 'preamble' : 'document' : null;
    let first = shown.filter((r) => !now || r.when === now), rest = shown.filter((r) => !first.includes(r));
    if (!first.length) [first, rest] = [shown.slice(0, 1), shown.slice(1)];
    if (shown.length) explainBody.innerHTML = first.map(cards.explain).join('') +
      (rest.length ? `<details class="more"><summary>More definitions</summary>${rest.map(cards.explain).join('')}</details>` : '');
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
  /** Records a run printed as JSON: one table per record shape (the keys a record has),
   * a single record as a card, each place a chip that opens it. The text output stays
   * behind a collapsed "raw" toggle. */
  const HIDDEN_COLS = new Set(['path', 'node', 'file', 'line', 'col', 'expanded', 'certain', 'definition']);
  const ROW_LIMIT = 500;
  const cell = cards.value;
  const chip = (r) => r.file && r.line ? cards.placeLink(r.path ?? r.file, r.line, r.file) : '';
  const filled = (v) => v != null && v !== '' && !(Array.isArray(v) && !v.length);
  function records(rows) {
    if (command() === 'explain' && rows.every((r) => r.name)) return rows.map(cards.explain).join('');
    const shapes = new Map();
    for (const r of rows) {
      const k = Object.keys(r).filter((c) => !HIDDEN_COLS.has(c)).join('\0');
      shapes.set(k, [...(shapes.get(k) ?? []), r]);
    }
    return [...shapes.values()].map((group) => {
      const cols = [...new Set(group.flatMap(Object.keys))].filter((c) => !HIDDEN_COLS.has(c) && group.some((r) => filled(r[c]))), title = group[0].tag ?? group[0].kind;
      if (group.length === 1) {
        const r = group[0], head = r.name ?? r.key, rest = cols.filter((k) => k !== 'name' && !(k === 'key' && !r.name));
        return `<div class="card">${head || chip(r) ? `<p>${cell(head ?? '')} ${chip(r)}</p>` : ''}<dl>${rest.map((k) => `<dt>${escape(k)}</dt><dd>${cell(r[k])}</dd>`).join('')}</dl></div>`;
      }
      const where = group.some(chip);
      return `<table class="rows"><caption>${title ? `${escape(title)} · ` : ''}${group.length} rows${group.length > ROW_LIMIT ? `, the first ${ROW_LIMIT} shown` : ''}</caption>` +
        `<thead><tr>${cols.map((c) => `<th>${escape(c)}</th>`).join('')}${where ? '<th>where</th>' : ''}</tr></thead><tbody>` +
        group.slice(0, ROW_LIMIT).map((r) => `<tr>${cols.map((c) => `<td>${cell(r[c])}</td>`).join('')}${where ? `<td>${chip(r)}</td>` : ''}</tr>`).join('') + '</tbody></table>';
    }).join('');
  }
  function structured(out, raw) {
    let rows;
    try { rows = JSON.parse(out); } catch { return `<pre class="raw">${raw}</pre>`; }
    if (!Array.isArray(rows)) rows = [rows];
    const rawBox = `<details class="raw"><summary>raw</summary><pre>${raw}</pre></details>`;
    return (rows.length ? records(rows) : '<p class="muted">no results</p>') + rawBox;
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
  $('play-outclose').onclick = () => { outBox.hidden = true; };
  const edits = () => Object.fromEntries(Object.entries(state.texts).filter(([name]) => !(name in state.ro)).map(([name, text]) => [path(name), text]));
  async function start() {
    sync();
    const a = argv();
    button.disabled = true;
    status.textContent = 'running…';
    delete status.dataset.done;
    cmd.textContent = `satex ${words(args.value).filter((x) => x !== 'satex').join(' ')}`;
    try {
      const data = await ask({ type: 'run', args: a, edits: edits(), cwd: state.dir });
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
      shownTime(`${secs(data.ms)}`);
      const failed = typeof data.status !== 'number' || data.status > 1;
      status.textContent = `${command() ?? 'run'} · exit ${data.status}`;
      status.title = summary;
      const cached = cacheReport(data.lines);
      if (cached) status.textContent += ` · ${cached}`;
      // plain lint's findings are the panel's: its output would repeat them
      outBox.hidden = (plainLint() || explainedHere) && !failed;
      if (!outBox.hidden) foldConsole(false, false);
      status.dataset.done = data.status;
      if (missing([printed]).length) fetchMissing([printed]);
    } catch (e) {
      status.textContent = `the run failed: ${e.message}`;
      status.dataset.done = 'error';
      outBox.hidden = false;
      foldConsole(false, false);
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
    } else if (flag?.takes_value && flag.long === 'file') items = state.files.map((n) => [n, 'file']);
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
      ...state.files.map((n) => [`${n}:`, 'a file of the project, then LINE:COL'])];
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

  let sampleTexts;
  const opened = new Set();
  function open(name) {
    if (opened.has(uri(name)) || name in state.ro || data(name)) return;
    opened.add(uri(name));
    checking();
    lsp('textDocument/didOpen', { textDocument: { uri: uri(name), languageId: 'latex', version, text: state.texts[name] } }, true);
  }
  function close(name) {
    if (!opened.delete(uri(name))) return;
    lsp('textDocument/didClose', { textDocument: { uri: uri(name) } }, true);
    delete diagnostics[uri(name)];
  }
  const encode = (text) => new TextEncoder().encode(text);
  /** The project's texts into the worker's file system, which a new language server boots from. */
  const store = (names = state.files, removed = []) => worker.postMessage({ type: 'project', dir: state.dir, removed,
    files: Object.fromEntries(names.filter((n) => n in state.texts && !(n in state.ro)).map((n) => [n, encode(state.texts[n])])) });
  const OPEN_AT_START = 16;
  // The engine follows the root: a `% !TEX program =` comment or a package only XeTeX has, else pdfTeX
  // (plain TeX for a `\bye` file without a class). Choosing in the selector overrides it until the root changes.
  let manualKernel = false, following = false;
  const ENGINES = { xelatex: 'xetex', xetex: 'xetex', pdflatex: 'pdftex', pdftex: 'pdftex', latex: 'pdftex', tex: 'plain', plain: 'plain' };
  function engineOf(text = '') {
    const magic = text.match(/^%\s*!T[eE]X\s+(?:TS-)?program\s*=\s*(\w+)/m);
    if (magic && ENGINES[magic[1].toLowerCase()]) return ENGINES[magic[1].toLowerCase()];
    if (/\\(?:usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{[^}]*\b(?:fontspec|unicode-math|polyglossia|xltxtra|xunicode)\b/.test(text)) return 'xetex';
    return /\\bye\b/.test(text) && !/\\documentclass/.test(text) ? 'plain' : 'pdftex';
  }
  const wanted = () => Math.max(0, manifest.kernels.findIndex((k) => k.id === engineOf(state.texts[state.root])));
  /** Loads and selects the engine the root asks for, unless the selector was used; true if it changed. */
  async function followRoot() {
    const want = wanted();
    if (manualKernel || want === state.kernel) return false;
    try { await kernel(want); } catch (e) { toast(`the ${manifest.kernels[want].label} kernel did not load: ${e.message}`); return false; }
    toast(`${manifest.kernels[want].label} selected for ${state.root}`);
    return true;
  }
  /** Starts the language server again: another kernel, project or root. */
  async function restart() {
    following = true;
    sync();
    await followRoot();
    await fetchPackages(wanted_(named(Object.entries(state.texts).filter(([n]) => !(n in state.ro)).map(([, t]) => t))));
    store();
    opened.clear();
    asked.clear();
    for (const p of pending.values()) p.resolve(null);
    pending.clear();
    worker.postMessage({ type: 'reset', args: [...flags(), '-f', state.root], cwd: state.dir });
    lspStatus.textContent = 'language server: starting…';
    await lsp('initialize', { processId: null, rootUri: `file://${state.dir}`, capabilities: {} });
    lsp('initialized', {}, true);
    open(state.root);
    open(state.file);
    // the server publishes findings of open files only: the project's own, up to a few, for the tree's counts
    for (const n of state.files.slice(0, OPEN_AT_START)) open(n);
    synced = version;
    following = false;
  }

  // ---------------------------------------------------------------- the file tree and the tabs

  const KINDS = { tex: 'c4', ltx: 'c4', bib: 'c5', bst: 'c5', sty: 'c6', cls: 'c6', cfg: 'c6', def: 'c6' };
  const fileIcon = (name) => `<svg class="ficon" viewBox="0 0 16 16" aria-hidden="true" style="--k: var(--${KINDS[name.split('.').pop()] ?? 'c0'})"><path d="M4 1.5h5.5L13 5v9.5H4z"/><path d="M9.5 1.5V5H13"/></svg>`;
  /** A file's findings as a count in its worst severity's color. */
  function tally(name) {
    const ds = diagnostics[uri(name)] ?? [];
    if (!ds.length) return '';
    const worst = ['error', 'warning'].find((s) => ds.some((d) => sev(d) === s)) ?? 'info';
    return `<span class="tcount sev-${worst}" title="${ds.length} finding${ds.length > 1 ? 's' : ''}">${ds.length}</span>`;
  }
  /** A root candidate: a TeX file that starts a document. */
  const rootable = (name) => /\.(tex|ltx)$/.test(name) && /\\documentclass|\\begin\s*\{document\}|\\bye\b/.test(state.texts[name] ?? '');
  const foldedDirs = new Set();
  function drawTree() {
    const names = [...state.files].sort((a, b) => a.localeCompare(b));
    const rows = [], seen = new Set();
    for (const n of names) {
      const parts = n.split('/');
      for (let d = 1; d < parts.length; d++) {
        const dir = parts.slice(0, d).join('/');
        if (!seen.has(dir)) { seen.add(dir); if (![...foldedDirs].some((f) => dir.startsWith(f + '/'))) rows.push(`<div class="folder${foldedDirs.has(dir) ? ' folded' : ''}" role="treeitem" tabindex="0" data-folder="${escape(dir)}" style="--depth: ${d - 1}"><span class="tname">${escape(parts[d - 1])}</span></div>`); }
      }
      if ([...foldedDirs].some((f) => n.startsWith(f + '/'))) continue;
      const root = n === state.root;
      rows.push(`<div class="file${root ? ' root' : ''}" role="treeitem" tabindex="0" data-name="${escape(n)}" style="--depth: ${parts.length - 1}"${n === state.file ? ' aria-current="true"' : ''} title="${escape(n)}">` +
        `${fileIcon(n)}<span class="tname">${escape(parts.at(-1))}</span>${root ? '<span class="rootmark" title="the root: runs and the language server read the project from here">root</span>' : ''}${tally(n)}` +
        `<span class="tacts">${!root && rootable(n) ? '<button type="button" class="tact" data-act="root" title="make this the root">◎</button>' : ''}` +
        `<button type="button" class="tact" data-act="rename" title="rename ${escape(n)}">✎</button><button type="button" class="tact" data-act="delete" title="delete ${escape(n)}">×</button></span></div>`);
    }
    treeBox.innerHTML = rows.join('');
    const examples = Object.keys(sampleTexts ?? {}).filter((n) => !shared.has(n));
    examplesBox.innerHTML = examples.map((n) => {
      const more = projects[n]?.length;
      return `<div class="file ex" role="listitem" tabindex="0" data-example="${escape(n)}"${!state.upload && n === state.example ? ' aria-current="true"' : ''} title="${escape(n)}${more ? ` and ${projects[n].join(', ')}` : ''}, in place of the project's files">` +
        `${fileIcon(n)}<span class="tname">${escape(n)}</span>${more ? `<span class="tmore">+${more}</span>` : ''}</div>`;
    }).join('');
  }
  function drawTabs() {
    tabs.innerHTML = state.open.map((n) => `<button role="tab" aria-selected="${n === state.file}" data-name="${escape(n)}"${n in state.ro ? ` class="ro" title="${escape(state.ro[n])}, read-only"` : ` title="${escape(n)}${n === state.root ? ', the root' : ''}"`}>` +
      `${escape(n)}<span class="x" role="button" aria-label="close ${escape(n)}" title="close"></span></button>`).join('');
    tabs.querySelector('[aria-selected=true]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }
  const redraw = () => { drawTabs(); drawTree(); };
  /** The command a sample comes with: plain lint unless the manifest says otherwise. */
  const commandOf = (name) => manifest.commands?.[name] ?? 'lint';
  /** Shows `name` in the editor, as a tab; the language server gets it if it does not have it. */
  function choose(name) {
    if (!editor || !(name in state.texts)) return;
    if (!state.open.includes(name)) state.open.push(name);
    if (name === state.file) return redraw();
    sync();
    state.file = name;
    redraw();
    editor.set(state.texts[name], name in state.ro);
    drawRefs();
    open(name);
    synced = version; // the server has this text: opened with it, or kept in sync with it
    render();
    typeset(uri());
  }
  /** Closes a tab; the last one closed leaves the root open. */
  function closeTab(name) {
    const i = state.open.indexOf(name);
    if (i < 0) return;
    state.open.splice(i, 1);
    if (name in state.ro) { delete state.ro[name]; delete state.texts[name]; }
    if (!state.open.length) state.open.push(state.root);
    if (name === state.file) { state.file = null; choose(state.open[Math.min(i, state.open.length - 1)]); } else redraw();
  }
  tabs.onclick = (e) => {
    const tab = e.target.closest('button');
    if (!tab) return;
    if (e.target.closest('.x')) closeTab(tab.dataset.name); else choose(tab.dataset.name);
  };
  tabs.onauxclick = (e) => { const tab = e.target.closest('button'); if (tab && e.button === 1) closeTab(tab.dataset.name); };
  tabs.ondblclick = (e) => { const n = e.target.closest('button')?.dataset.name; if (n && !(n in state.ro)) renameFile(n); };

  /** An example in place of the project: its files (as last edited), its root, its command. */
  async function example(ex, { file = ex, list = null, root = ex, command = null } = {}) {
    if (!editor || !(ex in sampleTexts)) return;
    sync();
    if (state.upload) Object.assign(state, { dir: '/work', texts: { ...sampleTexts }, upload: false, lists: {} });
    for (const n of Object.keys(state.ro)) delete state.texts[n];
    state.ro = {};
    state.files = state.lists[ex] ??= list ?? [ex, ...(projects[ex] ?? [])];
    manualKernel = false;
    Object.assign(state, { example: ex, root: state.files.includes(root) ? root : ex, open: [...state.files], file: state.files.includes(file) ? file : ex });
    editor.set(state.texts[state.file]);
    history.replaceState(null, '', `#${encodeURIComponent(ex)}`);
    redraw();
    await restart();
    args.value = command ?? commandOf(ex);
    outBox.hidden = true;
    if (!plainLint()) start();
  }
  /** Another root: the server starts over from it. */
  async function makeRoot(name) {
    state.root = name;
    manualKernel = false;
    redraw();
    await restart();
    toast(`${name} is the root`);
  }
  examplesBox.onclick = (e) => { const row = e.target.closest('[data-example]'); if (row) example(row.dataset.example); };
  treeBox.onclick = (e) => {
    const folder = e.target.closest('[data-folder]');
    if (folder) { const f = folder.dataset.folder; if (foldedDirs.has(f)) foldedDirs.delete(f); else foldedDirs.add(f); return drawTree(); }
    const row = e.target.closest('[data-name]');
    if (!row || e.target.closest('.renaming')) return;
    const name = row.dataset.name, act = e.target.closest('.tact')?.dataset.act;
    if (act === 'root') makeRoot(name);
    else if (act === 'rename') renameFile(name);
    else if (act === 'delete') deleteFile(name, e.target.closest('.tact'));
    else choose(name);
  };
  treeBox.oncontextmenu = (e) => {
    const row = e.target.closest('.file[data-name]');
    if (!row || e.target.closest('.renaming')) return;
    e.preventDefault();
    const name = row.dataset.name, ro = name in state.ro;
    showMenu([
      ...(/\.(tex|ltx)$/.test(name) && name !== state.root ? [['Use as root', '', () => makeRoot(name)]] : []),
      ['Open', '', () => choose(name)],
      ...(ro ? [] : [['Rename…', 'F2', () => renameFile(name)], ['Delete', '', () => deleteFile(name)]]),
    ], e.clientX, e.clientY);
  };
  treeBox.addEventListener('keydown', (e) => { const row = e.target.closest?.('.file[data-name]'); if (e.key === 'F2' && row && !e.target.closest('.renaming')) { e.preventDefault(); renameFile(row.dataset.name); } });
  for (const box of [treeBox, examplesBox]) box.onkeydown = (e) => { if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('[data-name], [data-example], [data-folder]')) { e.preventDefault(); e.target.click(); } };
  addEventListener('hashchange', () => {
    const h = decodeURIComponent(location.hash.slice(1)), opening = h.match(/^open=(.+):(\d+)$/);
    if (opening) openPlace(opening[1], { line: Number(opening[2]) - 1, character: 0 });
    else if (!h.startsWith('s=') && h in sampleTexts && !shared.has(h)) example(h);
  });

  /** A name box in the tree, at `row` or at the end: Enter takes the name, Escape cancels. */
  function nameBox(value, row, done) {
    const box = Object.assign(document.createElement('input'), { className: 'tnamebox', value, spellcheck: false });
    box.setAttribute('aria-label', 'file name');
    const holder = Object.assign(document.createElement('div'), { className: 'file renaming' });
    holder.style.setProperty('--depth', row?.style.getPropertyValue('--depth') || 0);
    holder.append(box);
    if (row) row.replaceWith(holder); else treeBox.append(holder);
    box.focus();
    box.setSelectionRange(0, value.lastIndexOf('.') > 0 ? value.lastIndexOf('.') : value.length);
    let finished = false;
    const finish = (name) => { if (finished) return; finished = true; drawTree(); if (name != null) done(name.trim()); };
    box.onkeydown = (e) => {
      if (e.key === 'Enter') { e.preventDefault(); finish(box.value); } else if (e.key === 'Escape') { e.preventDefault(); finish(null); }
    };
    box.onblur = () => finish(box.value === value ? null : box.value);
  }
  const valid = (name, old = null) => {
    if (!name || name === old) return false;
    if (/^\/|(^|\/)\.\.?(\/|$)|\/$/.test(name)) return toast('A name inside the project: no leading /, no . or .. parts'), false;
    if (name in state.texts && !(name in state.ro)) return toast(`${name} is already in the project`), false;
    return true;
  };
  /** A new, empty file in the project, opened. */
  function newFile() {
    let n = 1;
    while (`section${n}.tex` in state.texts) n++;
    $('play-tree').hidden = false;
    nameBox(`section${n}.tex`, null, (name) => {
      if (!valid(name)) return;
      state.texts[name] = '';
      state.files.push(name);
      store([name]);
      choose(name);
      const stem = name.replace(/\.tex$/, '');
      toast(/\.tex$/.test(name) ? `${name} added: \\input{${stem}} in ${state.root} reads it` : `${name} added`);
    });
  }
  /** The project's references to a file (`\input{chapter}`, `\bibliography{refs}`), renamed with it. */
  function relink(from, to) {
    const strip = (n) => n.replace(/\.(tex|bib)$/, '');
    const same = (arg) => arg === from || (strip(arg) === strip(from) && strip(from) !== from);
    let n = 0;
    for (const name of state.files) {
      if (data(name) || !(name in state.texts)) continue;
      const text = state.texts[name].replace(/(\\(?:input|include|includeonly|subfile|bibliography|addbibresource|InputIfFileExists)\s*\{)([^}]*)\}/g, (m, cs, list) =>
        cs + list.split(',').map((arg) => { const a = arg.trim(); if (!same(a)) return arg; n++; return arg.replace(a, a === from ? to : strip(to)); }).join(',') + '}');
      if (text === state.texts[name]) continue;
      if (name === state.file) editor.set(text); else state.texts[name] = text;
      if (opened.has(uri(name)) && name !== state.file) lsp('textDocument/didChange', { textDocument: { uri: uri(name), version: ++version }, contentChanges: [{ text }] }, true);
    }
    return n;
  }
  function renameFile(name) {
    const row = treeBox.querySelector(`[data-name="${CSS.escape(name)}"]`);
    if (!row) { $('play-tree').hidden = false; drawTree(); }
    nameBox(name, treeBox.querySelector(`[data-name="${CSS.escape(name)}"]`), async (to) => {
      if (!valid(to, name)) return;
      sync();
      state.texts[to] = state.texts[name];
      delete state.texts[name];
      for (const list of [state.files, state.open]) { const i = list.indexOf(name); if (i >= 0) list[i] = to; }
      if (state.file === name) state.file = to;
      close(name);
      store([to], [name]);
      const links = relink(name, to);
      toast(`Renamed to ${to}${links ? `, ${links} reference${links > 1 ? 's' : ''} updated` : ''}`);
      if (state.root === name) { state.root = to; redraw(); await restart(); } else { redraw(); open(state.file); analyze(); }
    });
  }
  /** Deletes a file: the first click asks, the second within a few seconds deletes. */
  async function deleteFile(name, button) {
    if (state.files.length < 2) return toast('A project keeps at least one file');
    if (button && !button.classList.contains('sure')) {
      button.classList.add('sure');
      button.textContent = 'delete?';
      setTimeout(() => button.isConnected && drawTree(), 3000);
      return;
    }
    state.files.splice(state.files.indexOf(name), 1);
    const tab = state.open.indexOf(name);
    if (tab >= 0) state.open.splice(tab, 1);
    close(name);
    delete state.texts[name];
    store([], [name]);
    toast(`Deleted ${name}`);
    const rootGone = state.root === name;
    if (rootGone) state.root = state.files.find(rootable) ?? state.files[0];
    if (!state.open.length) state.open.push(state.root);
    if (state.file === name) { state.file = null; choose(state.open[Math.min(Math.max(tab, 0), state.open.length - 1)]); } else redraw();
    if (rootGone) await restart(); else analyze();
  }

  /** A kernel's bundle, fetched once, then the server and the run start over with it. */
  async function kernel(i) {
    const entry = manifest.kernels[i];
    if (!state.loaded.has(i)) {
      status.textContent = `loading the ${entry.label} kernel (${size(entry.size)})…`;
      const t = performance.now();
      const given = location.protocol === 'file:' ? await local(entry.bundle) : undefined;
      await ask({ type: 'bundle', base, entry, given }, given ? [given.buffer] : []);
      state.loaded.add(i);
      timing('t-init', `${$('t-init').textContent} · ${entry.label} ${secs(performance.now() - t)}`, 'time to fetch and set up each kernel');
      bar.hidden = true;
      loads.clear();
      status.textContent = `${entry.label} ready`;
    }
    state.kernel = i;
    kernelBox.value = i;
  }

  /** Packages beyond the first download: a bundle each per kernel (the files and cache entries a
   * document loading it adds), with the bundles it needs. Fetched before a project's first run for
   * what its files name, and again whenever a finding or a run says one is not in this installation. */
  const PACKAGES = new Map((manifest.packages ?? []).map((p) => [p.id, p]));
  const fetchedPackages = new Map(); // id -> its load
  const MISSING = /`([^`\s]+)` is not in this installation/g;
  const NAMED = /\\(?:documentclass|usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}/g;
  /** The bundles of the current kernel for `names` (a class too), and what those need, not yet asked for. */
  function wanted_(names) {
    const kernel = manifest.kernels[state.kernel].id;
    const todo = new Set();
    const add = (id) => {
      const entry = PACKAGES.get(id);
      if (!entry || todo.has(id)) return;
      todo.add(id);
      entry.needs.forEach((n) => add(`${kernel}/${n}`));
    };
    for (const n of names) { add(`${kernel}/${n}`); add(`${kernel}/${n}.cls`); }
    return [...todo].filter((id) => !fetchedPackages.has(id));
  }
  /** The packages and classes the project's own files name. */
  const named = (texts) => [...new Set(texts.flatMap((t) => [...String(t).matchAll(NAMED)].flatMap((m) => m[1].split(',').map((x) => x.trim()).filter(Boolean))))];
  const missing = (texts) => wanted_([...new Set(texts.flatMap((t) => [...String(t).matchAll(MISSING)].map((m) => m[1].replace(/\.(sty|cls)$/, ''))))]);
  const loadedList = [], inFlight = new Set();
  async function fetchPackages(ids) {
    if (!ids.length) return false;
    const entries = ids.map((id) => PACKAGES.get(id));
    const label = entries.map((e) => e.label).join(', ');
    status.textContent = `fetching ${entries.map((e) => `${e.label} ${size(e.size)}`).join(', ')} …`;
    const t = performance.now();
    const given = location.protocol === 'file:' ? await Promise.all(entries.map((e) => local(e.bundle))) : undefined;
    const load = ask({ type: 'bundles', base, entries, given }, given ? given.map((g) => g.buffer) : []);
    ids.forEach((id) => { fetchedPackages.set(id, load); inFlight.add(id); });
    try { await load; } catch (e) { ids.forEach((id) => fetchedPackages.delete(id)); toast(`${label} did not load: ${e.message}`); return false; } finally { ids.forEach((id) => inFlight.delete(id)); }
    bar.hidden = true;
    loads.clear();
    status.textContent = `${label} fetched in ${secs(performance.now() - t)}`;
    status.dataset.packages = [...fetchedPackages.keys()].join(' ');
    loadedList.push(...entries.map((e) => `${e.label} ${size(e.size)}`));
    timing('t-load', `${loadedList.join(', ')} · last ${secs(performance.now() - t)}`, 'packages fetched after the first download');
    return true;
  }
  /** What a run's cache report says: the packages and classes served from the caches, the ones analysed anew. */
  function cacheReport(lines) {
    const reused = [], fresh = [];
    let segments = 0;
    for (const [, text] of lines) {
      const m = plain(text).match(/cache (?:kernel|class|package) (\S+): (reused|rebuilt \((.*)\))/);
      if (m) (m[2] === 'reused' ? reused : fresh).push(m[3] ? `${m[1]} (${m[3]})` : m[1]);
      const s = plain(text).match(/cache package segments: (\d+) reused/);
      if (s) segments = Number(s[1]);
    }
    if (!reused.length && !fresh.length && !segments) return '';
    const detail = [reused.length && `cached: ${reused.join(', ')}`, segments && `${segments} package segment${segments > 1 ? 's' : ''} cached`, fresh.length && `analysed anew: ${fresh.join(', ')}`].filter(Boolean).join(' · ');
    timing('t-cache', detail, 'what the last run reused from the caches and what it analysed again');
    return `${reused.length + segments} cached${fresh.length ? ` · fresh: ${fresh.map((f) => f.split(' ')[0]).join(', ')}` : ''}`;
  }
  /** After findings or a run name missing packages: fetch them, start over. */
  async function fetchMissing(texts) {
    if (!await fetchPackages(missing(texts))) return false;
    await restart();
    if (!plainLint()) await start();
    return true;
  }
  // Closed, the selector shows the engine's name; its sizes only while the list is open.
  kernelBox.innerHTML = manifest.kernels.map((k, i) => `<option value="${i}" title="download ${size(k.size)}" data-name="${k.label}" data-more="${size(k.size)}"></option>`).join('') +
    (manifest.unavailable ?? []).map(([label, why]) => `<option disabled title="${escape(why)}" data-name="${label}" data-more="not available"></option>`).join('');
  const kernelList = (sizes) => { for (const o of kernelBox.options) o.textContent = sizes ? `${o.dataset.name} · ${o.dataset.more}` : o.dataset.name; };
  kernelList(false);
  kernelBox.addEventListener('mousedown', () => kernelList(true));
  kernelBox.addEventListener('blur', () => kernelList(false));
  kernelBox.addEventListener('change', () => kernelList(false));
  kernelBox.onchange = async () => {
    manualKernel = true;
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
  /** A new project: a .zip or a folder, its root found, the examples set aside. */
  async function project(files, label, root = null) {
    sync();
    const decoder = new TextDecoder();
    const texts = Object.fromEntries(Object.entries(files).filter(([n]) => TEXT.test(n)).sort(([a], [b]) => a.localeCompare(b)).map(([n, d]) => [n, decoder.decode(d)]));
    const found = roots(texts);
    if (!found.length) throw new Error('no root document: no file has \\documentclass and \\begin{document}');
    const first = root in texts ? root : found[0];
    manualKernel = false;
    Object.assign(state, { dir: '/work/upload', texts, root: first, file: first, open: [first], files: Object.keys(texts), example: null, upload: true, ro: {} });
    worker.postMessage({ type: 'project', dir: state.dir, files, replace: true });
    redraw();
    editor.set(texts[state.file]);
    history.replaceState(null, '', location.pathname);
    toast(`${label}: ${Object.keys(files).length} files, root ${state.root}${found.length > 1 ? ` (${found.length} candidates: ◎ in the tree makes another the root)` : ''}`);
    await restart();
    args.value = 'lint';
    outBox.hidden = true;
  }
  /** Single files join the open project; images and the like only its file system. */
  async function add(files) {
    const decoder = new TextDecoder();
    const texts = Object.entries(files).filter(([n]) => TEXT.test(n)).map(([n, d]) => [n, decoder.decode(d)]);
    Object.assign(state.texts, Object.fromEntries(texts));
    for (const [n] of texts) if (!state.files.includes(n)) state.files.push(n);
    worker.postMessage({ type: 'project', dir: state.dir, files });
    if (texts.length) choose(texts[0][0]); else redraw();
    analyze();
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
  const picked = (e) => { upload([...e.target.files].map((f) => [f.webkitRelativePath || f.name, f])); e.target.value = ''; };
  $('play-files').onchange = $('play-folder').onchange = picked;
  const upMenu = $('play-upmenu');
  // fixed, placed under its button: the tree clips and the editor's layers would cover an inner one
  $('play-upload').onclick = (e) => {
    upMenu.hidden = !upMenu.hidden;
    if (upMenu.hidden) return;
    const r = e.currentTarget.getBoundingClientRect();
    Object.assign(upMenu.style, { left: `${Math.max(4, Math.min(r.left, innerWidth - upMenu.offsetWidth - 8))}px`, top: `${r.bottom + 4}px` });
  };
  upMenu.onclick = (e) => { const b = e.target.closest('[data-pick]'); if (!b) return; upMenu.hidden = true; if (b.dataset.pick === 'new') newFile(); else $(`play-${b.dataset.pick}`).click(); };
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
    // the project: an example's changed files, or all of an upload's text files
    const files = state.files.filter((n) => n in state.texts && (state.upload || state.texts[n] !== sampleTexts[n]));
    const data = { x: state.example, r: state.root, l: state.files, p: Object.fromEntries(files.map((n) => [n, state.texts[n]])),
      f: state.file in state.ro ? state.root : state.file, a: args.value, k: manifest.kernels[state.kernel].id, c: editor.cursor() };
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
    toast(state.upload ? 'Link copied. It holds the project\'s text files, not its images.' : 'Link copied');
  };

  // ---------------------------------------------------------------- start

  /** What loading has come to: per download its bytes, else the phase it is in. */
  const bar = $('play-progress'), loads = new Map();
  const PHASE = { unpacking: 'Unpacking the TeX files…', importing: 'Importing the kernel caches…' };
  function progressed({ label, loaded, total }) {
    bar.hidden = false;
    if (PHASE[label]) {
      bar.removeAttribute('value');
      status.textContent = PHASE[label];
      return;
    }
    loads.set(label, [loaded, total]);
    const [got, all] = [...loads.values()].reduce(([g, a], [l, t]) => [g + l, a + t], [0, 0]);
    if (all) bar.value = Math.min(got / all, 1);
    const mb = (n) => (n / 2 ** 20).toFixed(1);
    status.textContent = `Fetching ${[...loads.keys()].join(' and ')}: ${mb(got)} of ${mb(all)} MiB`;
  }

  worker.onmessage = async ({ data }) => {
    if (data.type === 'lsp') {
      running -= order.shift() ? 1 : 0;
      for (const m of data.out) {
        if (m.id !== undefined && pending.has(m.id)) {
          const p = pending.get(m.id);
          pending.delete(m.id);
          if (m.error) p.reject(new Error(m.error.message.replace(/^[\w/]+: /, ''))); else p.resolve(m.result ?? null);
        } else if (m.method === 'textDocument/publishDiagnostics' && m.params.uri.startsWith(`file://${state.dir}/`)) {
          // Findings that come from a missing package wait for it: shown once its bundle is in.
          const unresolved = m.params.diagnostics.filter((d) => d.code === 'unresolved-file').map((d) => d.message);
          const waiting = missing(unresolved);
          if ((waiting.length || inFlight.size) && m.params.uri === uri()) {
            const names = [...new Set([...waiting, ...inFlight])].map((id) => PACKAGES.get(id).label).join(', ');
            status.textContent = `loading ${names}…`;
            phase(`loading ${names}…`);
            if (waiting.length) fetchMissing(unresolved).then((ok) => { if (!ok) { show(m.params.uri, m.params.diagnostics); typeset(m.params.uri); } });
            continue;
          }
          show(m.params.uri, m.params.diagnostics);
          typeset(m.params.uri);
          if (m.params.uri === uri()) lsp('satex/caches', { textDocument: { uri: m.params.uri } }).then((r) => cacheReport((r ?? []).map((l) => ['err', l]))).catch(() => {});
          fetchMissing(unresolved);
        }
      }
    } else if (data.type === 'status') {
      if (data.text && !checkBox.hidden) phase(data.text, data.total ? Math.min(data.reached / data.total, 1) : null);
    } else if (data.type === 'progress') {
      progressed(data);
    } else if (data.type === 'error' && data.of === 'lsp') {
      lspStatus.textContent = `language server failed: ${data.message.split('\n')[0]}`;
      running -= order.shift() ? 1 : 0;
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
    timing('t-init', secs(performance.now() - started), 'time to fetch satex and the pdfTeX kernel and set them up');
    bar.hidden = true;
    sampleTexts = ready.samples;
    const hash = decodeURIComponent(location.hash.slice(1));
    let link = hash.startsWith('s=') ? await restore(hash.slice(2)) : null;
    // a link from before projects: one file and its text
    if (link && !link.p) link = { ...link, x: projectOf(link.f) ?? link.f, p: { [link.f]: link.t } };
    if (link?.x && !(link.x in sampleTexts)) sampleTexts[link.x] = '';
    if (hash.startsWith('open=')) history.replaceState(null, '', location.pathname);
    const ex = link ? link.x : hash in sampleTexts && !shared.has(hash) ? hash : projectOf(hash) ?? manifest.sample;
    state.texts = { ...sampleTexts, ...(link?.x ? link.p : {}) };
    const k = manifest.kernels.findIndex((x) => x.id === link?.k);
    if (k >= 0) { manualKernel = true; if (k > 0) await kernel(k); }
    kernelBox.value = state.kernel;
    try {
      editor = await codemirror($('play-editor'), '');
      $('play-source').remove();
    } catch (e) {
      console.warn('CodeMirror did not load, the editor is a textarea', e);
      editor = textarea($('play-source'));
    }
    button.textContent = 'Run';
    button.disabled = false;
    if (link && !link.x) {
      await project(Object.fromEntries(Object.entries(link.p).map(([n, t]) => [n, encode(t)])), 'From the link', link.r);
      if (link.f in state.texts) choose(link.f);
      args.value = link.a;
    } else await example(ex, { file: link?.f ?? (shared.has(hash) ? hash : ex), list: link?.l, root: link?.r, command: link?.a });
    if (link?.c) editor.select(link.c);
    const opening = hash.match(/^open=(.+):(\d+)$/);
    if (opening) await openPlace(opening[1], { line: Number(opening[2]) - 1, character: 0 });
    if (!plainLint()) return link && !link.x ? start() : undefined;
    status.textContent = 'Ready. Findings update as you type; Enter runs the command.';
    status.dataset.done = 'ready';
  })().catch((e) => { status.textContent = `the playground failed: ${e.message}`; status.dataset.done = 'error'; });
})();
