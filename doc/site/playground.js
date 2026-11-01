// The playground's page side: a CodeMirror editor, satex's language server for
// findings, fixes, completion, hover, definitions and rename, and the `satex`
// command line for runs. satex itself runs in worker.js. Without the CDN the
// editor is a textarea. A classic script, so the page also works from disk.
(() => {
  const $ = (id) => document.getElementById(id);
  const manifest = JSON.parse($('satex-play').textContent);
  let booted = false;
  /** A start that fails on what an older page left (its stored layout and settings, its cached
   * bundles): those are dropped and the page loads once more, with a note; a second failure is shown. */
  const recover = (error) => {
    let again = false;
    try { again = sessionStorage.getItem('satex-recovered') === '1'; sessionStorage.setItem('satex-recovered', '1'); } catch {}
    if (again) { document.getElementById('play-status').textContent = `The playground did not start: ${error?.message ?? error}`; return; }
    try { for (const k of Object.keys(localStorage)) if (k.startsWith('satex')) localStorage.removeItem(k); } catch {}
    try { indexedDB.deleteDatabase('satex-playground'); } catch {}
    try { sessionStorage.setItem('satex-note', 'The stored layout, settings and caches did not fit this version of the playground: they were reset.'); } catch {}
    location.reload();
  };
  addEventListener('error', (e) => { if (!booted) recover(e.error ?? e.message); });
  addEventListener('unhandledrejection', (e) => { if (!booted) recover(e.reason); });
  try { const note = sessionStorage.getItem('satex-note'); if (note) { sessionStorage.removeItem('satex-note'); setTimeout(() => self.satexToast?.(note) ?? console.warn(note), 2000); } } catch {}
  window.satexTypeList = manifest.completion?.types;
  const base = new URL(manifest.root, document.baseURI).href;
  const [tabs, args, button, status, output, lspStatus, list, count, cmd, kernelBox, toastBox, ctxMenu, treeBox, examplesBox] =
    ['play-tabs', 'play-args', 'play-run', 'play-status', 'play-output', 'lsp-status', 'lsp-diagnostics', 'lsp-count',
      'play-cmd', 'play-kernel', 'play-toast', 'play-ctx', 'play-files-tree', 'play-examples'].map($);
  // Buttons give up their labels when their bar runs out of room, as in flowR's playground, and get
  // them back once the bar is as wide again as when they lost them: `.tight` on the bar.
  // Still too narrow with symbols alone, the buttons fold into one menu: `.tighter`.
  function collapsing(bar, overflows) {
    let lost = 0, lostMore = 0;
    const check = () => {
      const width = bar.clientWidth;
      if (bar.classList.contains('tighter') && width <= lostMore) return;
      bar.classList.remove('tighter');
      if (bar.classList.contains('tight') && width <= lost) { if (overflows()) { lostMore = width; bar.classList.add('tighter'); } return; }
      bar.classList.remove('tight');
      if (!overflows()) return;
      lost = width;
      bar.classList.add('tight');
      if (overflows()) { lostMore = width; bar.classList.add('tighter'); }
    };
    new ResizeObserver(check).observe(bar);
    return check;
  }
  {
    const filebar = tabs.closest('.filebar'), actions = filebar.querySelector('.actions'), tablist = filebar.querySelector('.tablist');
    // the tabs scroll, or the actions wrapped under them
    const check = collapsing(filebar, () => tabs.scrollWidth > tabs.clientWidth + 1 || actions.offsetTop > tablist.offsetTop + tablist.offsetHeight / 2);
    // the menu the buttons fold into: a click opens them as a column under it
    const more = Object.assign(document.createElement('button'), { type: 'button', className: 'pill more-actions', title: 'more actions', innerHTML: '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="3.5" cy="8" r="1.2"/><circle cx="8" cy="8" r="1.2"/><circle cx="12.5" cy="8" r="1.2"/></svg>' });
    more.setAttribute('aria-expanded', 'false');
    actions.append(more);
    more.onclick = (e) => { e.stopPropagation(); const open = !actions.classList.contains('menu-open'); actions.classList.toggle('menu-open', open); more.setAttribute('aria-expanded', String(open)); };
    document.addEventListener('click', (e) => { if (!more.contains(e.target)) { actions.classList.remove('menu-open'); more.setAttribute('aria-expanded', 'false'); } });
    new MutationObserver(check).observe(tabs, { childList: true });
    const ask = args.closest('.ask');
    collapsing(ask, () => args.clientWidth < 12 * parseFloat(getComputedStyle(args).fontSize));
    // the typeset switches, wrapped under their heading, say less
    const head = $('play-typeset').querySelector('summary'), switches = head.querySelector('.ts-switches');
    const title = document.createRange();
    title.selectNodeContents(head.firstChild);
    collapsing(head, () => {
      const t = title.getBoundingClientRect(), w = switches.getBoundingClientRect();
      return w.top >= t.bottom - 2 || w.height > 1.5 * switches.firstElementChild.offsetHeight;
    });
  }
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
    // a layout an older page stored in another shape is dropped, not read
    const record = (v) => v != null && typeof v === 'object' && !Array.isArray(v);
    if (!record(kept)) kept = {};
    for (const part of ['heights', 'open']) if (part in kept && !record(kept[part])) delete kept[part];
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
      divider.setAttribute('aria-valuenow', String(Math.round(f)));
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

    // each section of the side pane: a grip under its body, and open or folded as the reader left it
    kept.heights ??= {};
    kept.open ??= {};
    for (const sect of document.querySelectorAll('.side-pane > details.sect, .tree > details.sect')) {
      const body = sect.querySelector('.grow');
      if (!body) continue;
      // the Explain macro panel starts folded on every visit
      if (sect.id in kept.open && sect.id !== 'play-explain') sect.open = kept.open[sect.id];
      sect.addEventListener('toggle', () => { kept.open[sect.id] = sect.open; keep(); });
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
      $('play-termtoggle').setAttribute('aria-expanded', String(!folded));
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
  const compact = new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 });
  /** A count with its thousands set apart by thin spaces: 21 939. */
  const num = (n) => Number(n).toLocaleString('en').replace(/,/g, '\u2009');
  /** A count in a chip or badge: 22k, 1.4M; the exact one is `num`'s. */
  const short = (n) => compact.format(Number(n)).toLowerCase().replace('m', 'M');
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
  /** A database is data for the build and a document for the editor: the server hovers and completes in it. */
  const database = (name) => /\.bib$/i.test(name);
  const path = (name) => `${state.dir}/${name}`;
  const uri = (name = state.file) => `file://${path(name)}`;

  let toastTimer;
  /** A rule the project breaks often: asks whether that is its style. The server records the answer in satex.yaml. */
  const styleQueue = [];
  function askStyle(candidate) {
    if (document.querySelector('.style-ask')) return void styleQueue.push(candidate);
    const ask = Object.assign(document.createElement('div'), { className: 'style-ask', role: 'alertdialog' });
    ask.innerHTML = `<p>${inline(candidate.question)}</p><button data-a="off">Yes, turn it off</button><button>No</button><button data-a="keep">Never ask</button>`;
    ask.onclick = (e) => {
      const button = e.target.closest('button');
      if (!button) return;
      ask.remove();
      if (styleQueue.length) askStyle(styleQueue.shift());
      if (!button.dataset.a) return;
      lsp('workspace/executeCommand', { command: 'satex/style', arguments: [{ code: candidate.code, answer: button.dataset.a }] }).catch((err) => toast(`not recorded: ${err.message}`));
      // the settings keep the answer, so it holds after a restart and in a shared link
      const key = `${button.dataset.a === 'off' ? 'lint_off' : 'style_prompt.keep'}+`, had = configSet[key] ?? [];
      if (!had.includes(candidate.code)) configKeep({ ...configSet, [key]: [...had, candidate.code] });
    };
    toastBox.before(ask);
  }
  self.satexToast = (text) => toast(text);
  function toast(text) {
    toastBox.textContent = text;
    toastBox.classList.add('on');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toastBox.classList.remove('on'), 2600);
  }
  /** One row of the timing pop-up: a line of text, `items` folded under it. */
  const timing = (id, text, title = '', items = null) => {
    const dd = Object.assign($(id), { title });
    dd.innerHTML = escape(text) + (items?.length ? `<details><summary>${items.length} listed</summary><span class="t-chips">${items.map((i) => `<code>${escape(i)}</code>`).join('')}</span></details>` : '');
  };
  // The status bar: one time, a pill per non-empty severity; the rest in a pop-up on the time.
  const shownTime = (text) => { $('t-time').hidden = false; $('t-time-text').textContent = text; };
  $('t-time').onclick = () => { const pop = $('t-detail'); pop.hidden = !pop.hidden; $('t-time').setAttribute('aria-expanded', String(!pop.hidden)); };
  addEventListener('mousedown', (e) => { if (!e.target.closest('.statdetail')) { $('t-detail').hidden = true; $('t-time').setAttribute('aria-expanded', 'false'); } });
  function pills(ds) {
    const n = (s) => ds.filter((d) => (sev(d) === 'hint' ? 'info' : sev(d)) === s).length;
    const shown = [['error', 'error'], ['warning', 'warning'], ['info', 'suggestion']].map(([s, w]) => [s, w, n(s)]).filter(([, , c]) => c);
    $('play-pills').innerHTML = shown.map(([s, w, c]) => `<span class="pill-sev sev-${s}" title="${c} ${w}${c > 1 ? 's' : ''} in the open file">${c} ${w}${c > 1 ? 's' : ''}</span>`).join('');
  }

  /** SGR codes to the classes build.py's ansi() writes; OSC hyperlinks and every other control sequence dropped. */
  function ansi(text) {
    text = text.replace(/\x1b\]8;[^\x1b\x07]*(\x1b\\|\x07)/g, '').replace(/\x1b\[[0-?]*[ -\/]*[@-ln-~]/g, '');
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
  const plain = (text) => text.replace(/\x1b\[[0-?]*[ -\/]*[@-~]|\x1b\]8;[^\x1b\x07]*(\x1b\\|\x07)/g, '');
  /** stderr without the progress bars, which redraw themselves in place. */
  const quiet = (text) => text.replace(/\x1b\[1A/g, '').split('\x1b[J')
    .filter((piece) => !/\[[#-]{20}\]/.test(plain(piece))).join('').trim();
  const words = (line) => (line.match(/"[^"]*"|'[^']*'|\S+/g) || []).map((w) => w.replace(/^(["'])(.*)\1$/, '$2'));

  /** The Markdown satex writes, for hovers and rule explanations: headings, paragraphs, lists,
   * code, links, emphasis. A `file:line` in code is a place that opens (site.js). */
  const cards = self.satexCards, tips = self.satexTip;
  const inline = (t) => cards.inline(t).replace(/(^|[\s(])\*([^*\s][^*]*)\*/g, '$1<i>$2</i>');
  const findingCard = (d) => `<div class="tip-lint f-${sev(d)}"><p><span class="sev sev-${sev(d)}">${sev(d)}</span> <code>${escape(d.code)}</code> ${inline(d.message)}</p></div>`;
  /** A command in text, or alone in a code span, explains itself on hover (`tips.card`), as the editor's hover does. */
  const commandRefs = (html) => html.replace(/(<code>(\\[A-Za-z@_]+(?::[A-Za-z]*)?\*?)<\/code>)|(<code[^>]*>[\s\S]*?<\/code>|<a [^>]*>[\s\S]*?<\/a>|<[^>]+>)|(\\[A-Za-z@_]+(?::[A-Za-z]*)?\*?)/g,
    (m, span, c1, skip, c2) => skip ? m : `<code class="cs-ref" data-cs="${c1 ?? c2}" tabindex="0">${c1 ?? c2}</code>`);
  const markdown = (text) => text.split(/^```\w*\n([\s\S]*?)^```\s*$/m).map((part, i) => i % 2 ? `<pre><code>${escape(part)}</code></pre>`
    : part.split(/\n{2,}/).map((b) => b.trim()).filter(Boolean).map((b) => /^#+ /.test(b) ? `<h4>${inline(b.replace(/^#+ /, ''))}</h4>`
      : /^[-*] /.test(b) ? `<ul>${b.split(/\n(?=[-*] )/).map((li) => `<li>${inline(li.slice(2))}</li>`).join('')}</ul>` : `<p>${inline(b)}</p>`).join('')).join('');
  /** A finding's rule as its card, the same one the rules page and every rule hover show. */
  function explanation(d) {
    const r = manifest.rules[d.code];
    return r ? `<div class="explain">${commandRefs(cards.ruleCard(r, `${base}wiki/lints.html#${encodeURIComponent(d.code)}`, true))}</div>` : '';
  }
  const card = cards.hover;

  // ---------------------------------------------------------------- worker requests

  const waiting = {}; // reply type -> resolver; runs, reads and kernel loads go one at a time, in order
  const REPLY = { run: 'result', bundle: 'bundle', bundles: 'bundle', init: 'ready', read: 'file', caches: 'caches', uncache: 'caches' };
  let line_ = Promise.resolve();
  // a dead worker fails its waiters instead of hanging them
  worker.onerror = (e) => {
    e.preventDefault?.();
    const error = new Error(e.message || 'the worker stopped');
    for (const [k, w] of Object.entries(waiting)) { delete waiting[k]; w.reject(error); }
  };
  const ask = (msg, transfer = []) => {
    const asked = line_.then(() => new Promise((resolve, reject) => {
      waiting[REPLY[msg.type]] = { resolve, reject };
      post(msg, transfer);
    }));
    line_ = asked.catch(() => {});
    return asked;
  };
  // Console commands run in a worker of their own, so the language server never waits on one. It gets
  // the same kernel, bundles and files: each such message goes to both, replayed in order when it starts.
  const MIRROR = new Set(['init', 'bundle', 'bundles']);
  const mirrored = [], runnerFiles = { dir: '', files: {} };
  let runner = null, mirrorable = true;
  function post(msg, transfer = []) {
    // a file:// page hands its buffers over, so a second worker cannot get them: commands share the first
    if (msg.given) mirrorable = false;
    if (MIRROR.has(msg.type)) mirrored.push(msg);
    // the project's files as they are now, not every edit that made them
    if (msg.type === 'project') {
      if (msg.replace) runnerFiles.files = {};
      runnerFiles.dir = msg.dir;
      Object.assign(runnerFiles.files, msg.files ?? {});
      for (const n of msg.removed ?? []) delete runnerFiles.files[n];
    }
    if (MIRROR.has(msg.type) || msg.type === 'project') runner?.send(msg);
    worker.postMessage(msg, transfer);
  }
  function startRunner() {
    const w = new Worker(URL.createObjectURL(new Blob([manifest.worker], { type: 'text/javascript' })));
    const replies = {};
    let chain = Promise.resolve();
    w.onmessage = ({ data }) => (data.type === 'error' ? replies[REPLY[data.of]]?.reject(new Error(data.message.split('\n')[0])) : replies[data.type]?.resolve(data));
    // one message at a time, as the first worker gets them through `ask`: the next waits for this one's reply
    const send = (msg) => {
      const done = chain.then(() => new Promise((resolve, reject) => {
        const reply = REPLY[msg.type];
        if (reply) replies[reply] = { resolve, reject }; else resolve(null);
        w.postMessage(msg);
      }));
      chain = done.catch(() => {});
      return done;
    };
    for (const msg of mirrored) send(msg);
    send({ type: 'project', dir: runnerFiles.dir, files: runnerFiles.files, replace: true });
    return { send };
  }
  /** A console command: in the second worker where it can be, else in line with the first one's work. */
  function runCommand(msg) {
    if (!mirrorable) return ask(msg);
    runner ??= startRunner();
    return runner.send(msg);
  }

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
    running += +runs;
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
  const EDITOR = self.siteCM().then((m) => [m.codemirror, m.language, m.lint, m.autocomplete, m.view, m.state, m.oneDark]);
  EDITOR.catch(() => {}); // a failure shows when the editor is built

  // ---------------------------------------------------------------- editor

  let editor; // { text(), set(text), diagnostics(list), apply(edits), cursor(), select(c), word() }
  let version = 0, synced = 0, analyzedVersion = 0, timer = null, checkStart = 0, retryCompletion = false;
  let typedAt = 0;
  function changed() {
    version++;
    typedAt = performance.now();
    if (refs && refs.file === state.file) clearRefs();
    clearTimeout(timer);
    // A command name or an argument still being typed (`\dra`, `\cite{kn`) waits: the analysis
    // would block completion for seconds, and its findings would be about unfinished text.
    const line = editor.text().split('\n')[editor.position().line] ?? '', before = line.slice(0, editor.position().character);
    timer = setTimeout(sync, /\\[A-Za-z@]*$|\{[^{}\s]*$/.test(before) ? 1500 : 400);
  }
  /** After an analysis, the command list the server would offer after a `\\`, asked once at a command
   * of the file: the first `\\` typed then completes at once. */
  function prefetchCommands() {
    // every name in force: what a `\\` completes from at once, beyond the server's best few hundred
    namesInForce(false).then((records) => scopeItems.set(uri(), records.map((r) => ({ label: r.name.replace(/^\\/, ''), detail: r.effective ?? r.name, kind: 3 }))));
    const text = editor?.text() ?? '';
    for (const [re, place, skip] of [[/\\[A-Za-z@]/, '\\', 1], [/\\begin\{/, 'begin', 7]]) {
      const at = text.search(re);
      if (at < 0 || commandItems.has(`${uri()}|${place}`)) continue;
      const before = text.slice(0, at + skip).split('\n');
      completions({ line: before.length - 1, character: before.at(-1).length }, before.at(-1).length, place);
    }
  }
  /** Sends the buffer to the server unless it already has it: the text
   * only, so what is asked meanwhile is answered from the last analysis. */
  function push() {
    if (!state.file || state.file in state.ro) return false;
    state.texts[state.file] = editor.text();
    if (data(state.file) && synced !== version) {
      synced = version;
      post({ type: 'project', dir: state.dir, files: { [state.file]: new TextEncoder().encode(editor.text()) } });
      if (database(state.file) && opened.has(uri())) lsp('textDocument/didChange', { textDocument: { uri: uri(), version }, contentChanges: [{ text: editor.text() }] }, true);
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
    // a completion may have sent the text already: what counts is whether this version was analyzed
    const pushed = push();
    if (pushed || (analyzedVersion !== version && !!state.file && !(state.file in state.ro))) {
      analyzedVersion = version;
      prefetch();
      // the root's text may now ask for another engine
      if (state.file === state.root && wanted() !== state.kernel && !manualKernel && !following) { following = true; restart().finally(() => { following = false; }); return; }
      analyze(); explainSoon();
    }
  }
  function analyze() { checking(); lsp('satex/analyze', {}, true); }
  /** A running check shows in the status bar like any other work (`busy`); the worker reports how much of the file is read once the engine says. */
  const phase = (text, done = null) => text ? busy('Analyzing', text[0].toUpperCase() + text.slice(1), { done }) : idle();
  function checking() { phase(`Analyzing ${state.file}…`); checkStart = performance.now(); }

  /** Completion order: the project's own commands, then the documented ones, internals
   * (`@`, `_`, `:` in the name) last; within each, the closest matches first: a prefix of the
   * label, a part of it, a part of what the item is found by (a citation's title), then the typed
   * characters in order (`m/f` finds `mode/fast`); then the server's order and shorter names. */
  const tier = (i) => /\(document\)$/.test(i.detail ?? '') ? 0 : /[@_:]/.test(i.label) ? 2 : 1;
  const subsequence = (text, typed) => { let k = 0; for (const c of text) if (c === typed[k]) k++; return k === typed.length; };
  const closeness = (i, typed) => {
    const label = i.label.toLowerCase(), found = (i.filterText ?? '').toLowerCase();
    return label.startsWith(typed) ? 0 : label.includes(typed) ? 1 : found.includes(typed) ? 2
      : subsequence(label, typed) || subsequence(found, typed) ? 3 : null;
  };
  const rank = (items, typed) => items
    .map((i) => [i, tier(i), closeness(i, typed.toLowerCase())]).filter((x) => x[2] !== null)
    .sort((a, b) => a[1] - b[1] || a[2] - b[2] || (a[0].sortText ?? '').localeCompare(b[0].sortText ?? '') || a[0].label.length - b[0].label.length || a[0].label.localeCompare(b[0].label)).map((x) => x[0]);
  /** The language server's completion items at `position`, kept per buffer version. While a run
   * keeps the server busy, the last items stand in only where they were asked for the same word. */
  const asked = new Map();
  let lastItems = null, lastWord = '';
  /** What the server last offered in each kind of place, by file (after a `\\`, in `\begin{`, `\cite{`, …):
   * what the same kind of place completes from at once. */
  const commandItems = new Map(), scopeItems = new Map();
  /** The names in force at the end of the root's last run (`scope`), asked once per analysis. */
  const scopeAsked = new Map();
  function namesInForce(all) {
    const key = `${state.root ?? state.file}|${all}|${analyzedVersion}`;
    if (!scopeAsked.has(key)) {
      if (scopeAsked.size > 4) scopeAsked.clear();
      const request = { file: path(state.root ?? state.file), queries: [{ type: 'scope', all }] };
      scopeAsked.set(key, lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).then((r) => r?.[0]?.records ?? []).catch(() => []));
    }
    return scopeAsked.get(key);
  }
  /** How long a completion waits for the server before the kept commands answer (the server's answer still replaces them). */
  const INSTANT_MS = 50;
  function completions(position, start = position.character, place = null) {
    const k = `${uri()}|${version}|${position.line}:${position.character}`, word = `${uri()}|${position.line}:${start}`;
    if (!asked.has(k)) {
      asked.clear();
      asked.set(k, lsp('textDocument/completion', { textDocument: { uri: uri() }, position }).then((r) => {
        [lastItems, lastWord] = [(Array.isArray(r) ? r : r?.items) ?? [], word];
        if (place && lastItems.length) commandItems.set(`${uri()}|${place}`, lastItems);
        return lastItems;
      }).catch(() => []));
    }
    if (running && lastItems && lastWord === word) return lastItems;
    const served = place && commandItems.get(`${uri()}|${place}`), names = place === '\\' && scopeItems.get(uri());
    const kept = names ? [...(served ?? []), ...names.filter((n) => !served?.some((i) => i.label === n.label))] : served;
    if (!kept) return asked.get(k);
    // a run keeps the server busy for seconds: the kept commands answer now, the server's items replace them when they come
    let early = false;
    asked.get(k).then(() => { if (early && document.querySelector('.cm-tooltip-autocomplete') && k.endsWith(`|${version}|${position.line}:${position.character}`)) editor.complete?.(); });
    return Promise.race([asked.get(k), new Promise((done) => setTimeout(() => { early = true; done(kept); }, INSTANT_MS))]);
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
    const [{ EditorView, basicSetup }, language, lint, ac, view, st, { oneDark }, tex] = [...await EDITOR, await self.siteTex()];
    /** Words (file names in arguments) the server resolves a definition for, by start offset. */
    const resolves = new Map();
    const KIND = { 3: 'function', 14: 'keyword', 9: 'namespace', 18: 'variable' };
    /** In a .bib file, after `=` or `#`: the @string names the file defines. */
    function bibStrings(ctx) {
      const word = ctx.matchBefore(/[A-Za-z_][\w:.+/-]*/), from = word?.from ?? ctx.pos;
      if (!/[=#]\s*$/.test(ctx.state.sliceDoc(Math.max(0, from - 40), from)) && !ctx.explicit) return null;
      const names = [...ctx.state.doc.toString().matchAll(/@string\s*[{(]\s*([A-Za-z_][\w:.+/-]*)\s*=\s*("[^"]*"|\{[^}]*\}|[^,})]*)/gi)];
      return names.length ? { from, options: names.map((m) => ({ label: m[1], detail: m[2].slice(0, 40), type: 'constant' })) } : null;
    }
    async function complete(ctx) {
      const bib = /\.bib$/.test(state.file);
      if (bib) {
        const strings = bibStrings(ctx);
        if (strings) return strings;
        // after `@` the entry types, at the start of a line in an entry its fields, after `\` the commands
        const typed = ctx.matchBefore(/[\\@]?[A-Za-z]*/), line = ctx.state.doc.lineAt(ctx.pos);
        if (!ctx.explicit && !/^[\\@]/.test(typed.text) && !/^\s*[A-Za-z]+$/.test(line.text.slice(0, ctx.pos - line.from))) return null;
        if (typed.text.startsWith('@') || !typed.text.startsWith('\\')) {
          push();
          const lead = typed.text.startsWith('@') ? 1 : 0, items = await completions(position(ctx.state.doc, ctx.pos), typed.from + lead - line.from, typed.text.startsWith('@') ? '@' : 'field');
          return items.length ? { from: typed.from + lead, filter: false, options: rank(items, ctx.state.sliceDoc(typed.from + lead, ctx.pos)).map((i) => ({ label: i.label, type: 'keyword', info: i.detail })) } : null;
        }
      }
      // a command's name, or a key or value in a list (`/demo/mode=fa`)
      const word = ctx.matchBefore(/[A-Za-z@*_:]*/), key = ctx.matchBefore(/[\w@*:./-]*/);
      const before = ctx.state.sliceDoc(word.from - 1, word.from), line = ctx.state.doc.lineAt(ctx.pos);
      // inside a {…} or […] the cursor's list item starts after `{`, `[`, `,` or `=`
      const open = [...line.text.slice(0, key.from - line.from)].reduce((d, c) => d + +'{['.includes(c) - +'}]'.includes(c), 0) > 0;
      const item = open && /[{[,=]\s*$/.test(line.text.slice(0, key.from - line.from));
      if (!ctx.explicit && before !== '\\' && before !== '{' && !item) return null;
      push();
      const start = before === '\\' ? word.from : key.from;
      // the kind of place: a command name, or the argument of the command whose brace is open
      const place = before === '\\' ? '\\' : line.text.slice(0, key.from - line.from).match(/\\([A-Za-z@]+)\*?(?:\[[^\]]*\])*\{[^{}]*$/)?.[1] ?? null;
      const items = await completions(position(ctx.state.doc, ctx.pos), start - line.from, place);
      // typed before the server read it: analyze now, ask again when that run ends
      if (!items.length && !ctx.explicit) { retryCompletion = true; sync(); }
      // an item that replaces a range (a key path) starts where that range does
      const edit = items.find((i) => i.textEdit)?.textEdit;
      const from = edit ? at(ctx.state.doc, edit.range.start) : before === '\\' ? word.from : key.from;
      const typed = ctx.state.sliceDoc(from, ctx.pos);
      return {
        from, to: edit ? Math.max(at(ctx.state.doc, edit.range.end), ctx.pos) : ctx.pos, filter: false,
        options: rank(items, typed).slice(0, 200).map((i) => ({
          label: i.label, usage: i.detail, preview: i.data?.preview, type: KIND[i.kind] ?? 'text',
          apply: i.insertTextFormat === 2 ? ac.snippet((i.insertText ?? i.textEdit?.newText ?? i.label).replace(/\$\{?(\d+)\}?/g, '#{$1}')) : i.textEdit?.newText ?? i.insertText ?? i.label,
        })),
      };
    }
    // a brace typed before a command closes too (`\\cite{` before `\\end`); TeX has no quote pairs
    const closing = st.EditorState.languageData.of(() => [{ closeBrackets: { brackets: ['(', '[', '{'], before: ')]}:;>\\' } }]);
    async function hoverCard(v, pos) {
      // The server has the text; it answers from its last analysis at once.
      push();
      const line = v.state.doc.lineAt(pos), word = wordAt(line.text, pos - line.from);
      const key = word?.[3] ? `${uri()}|${word[2]}` : null;
      const asking = lsp('textDocument/hover', { textDocument: { uri: uri() }, position: position(v.state.doc, pos) }).catch(() => null)
        .then((r) => { if (key) keptHovers.set(key, r); return r; });
      const result = running && keptHovers.has(key) ? keptHovers.get(key) : await asking;
      const c = result?.contents;
      const value = typeof c === 'string' ? c : c?.value ?? (Array.isArray(c) ? c.map((x) => x.value ?? x).join('\n') : '');
      const p = position(v.state.doc, pos), found = findingsAt(p.line, p.character);
      if (!value && !found.length) return null;
      const range = result?.range, [from, to] = range ? [at(v.state.doc, range.start), at(v.state.doc, range.end)]
        : found.length ? span(found[0][0].range) : [pos, pos];
      return { from, to, html: found.map(([d]) => findingCard(d) + explanation(d)).join('') + (value ? `<div class="hover">${card(value)}</div>` : '') };
    }
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
    // A swatch before each color the server resolved (`textDocument/documentColor`), as editors show them.
    class Swatch extends view.WidgetType {
      constructor(css) { super(); this.css = css; }
      eq(o) { return o.css === this.css; }
      toDOM() { return Object.assign(document.createElement('span'), { className: 'cm-swatch', title: this.css, style: `background:${this.css}` }); }
    }
    const swatches = st.StateEffect.define();
    const swatchField = st.StateField.define({
      create: () => view.Decoration.none,
      update: (deco, tr) => tr.effects.reduce((d, e) => e.is(swatches) ? view.Decoration.set(e.value.map(([at, css]) => view.Decoration.widget({ widget: new Swatch(css), side: -1 }).range(at)), true) : d, deco.map(tr.changes)),
      provide: (f) => view.EditorView.decorations.from(f),
    });
    // A `\begin` name and its `\end` name, from the server's linked editing:
    // while the cursor is in one, what is typed there is typed in both.
    const linkedSet = st.StateEffect.define();
    const linkedField = st.StateField.define({
      create: () => null,
      update: (r, tr) => tr.effects.reduce((v, e) => e.is(linkedSet) ? e.value : v,
        r && tr.docChanged ? r.map(([f, t]) => [tr.changes.mapPos(f, -1), tr.changes.mapPos(t, 1)]) : r),
      provide: (f) => view.EditorView.decorations.from(f, (r) => view.Decoration.set((r ?? []).filter(([a, b]) => a < b)
        .map(([a, b]) => view.Decoration.mark({ class: 'cm-linked' }).range(a, b)), true)),
    });
    const mirror = st.EditorState.transactionFilter.of((tr) => {
      const r = tr.startState.field(linkedField);
      if (!r || !tr.docChanged || !(tr.isUserEvent('input') || tr.isUserEvent('delete'))
        || tr.startState.sliceDoc(...r[0]) !== tr.startState.sliceDoc(...r[1])) return tr;
      const twin = [];
      let inside = true;
      tr.changes.iterChanges((fromA, toA, _fromB, _toB, text) => {
        const i = r.findIndex(([f, t]) => fromA >= f && toA <= t);
        if (i < 0) { inside = false; return; }
        const shift = r[1 - i][0] - r[i][0];
        twin.push({ from: fromA + shift, to: toA + shift, insert: text });
      });
      return inside && twin.length ? [tr, { changes: twin, sequential: false }] : tr;
    });
    function linkAround(v) {
      const s = v.state, pos = s.selection.main.head, r = s.field(linkedField);
      if (r && r.some(([f, t]) => pos >= f && pos <= t)) return;
      const line = s.doc.lineAt(pos), col = pos - line.from;
      const inName = /\\(?:begin|end)\s*\{[^{}%]*$/.test(line.text.slice(0, col)) && /^[^{}%]*\}/.test(line.text.slice(col));
      if (!inName || state.file in state.ro) { if (r) queueMicrotask(() => v.dispatch({ effects: linkedSet.of(null) })); return; }
      const doc = s.doc;
      push();
      lsp('textDocument/linkedEditingRange', { textDocument: { uri: uri() }, position: position(doc, pos) }).catch(() => null).then((found) => {
        if (v.state.doc !== doc) return;
        const ranges = found?.ranges?.map((x) => [at(doc, x.start), at(doc, x.end)]);
        v.dispatch({ effects: linkedSet.of(ranges?.length === 2 ? ranges : null) });
      });
    }
    // The `}` of a `\begin{name}` the document does not close yet adds its
    // `\end{name}` on the next line; the cursor stays for the arguments.
    const closeEnvironment = view.EditorView.inputHandler.of((v, from, to, text) => {
      if (text !== '}' || from !== to || state.file in state.ro) return false;
      const s = v.state, line = s.doc.lineAt(from);
      const name = line.text.slice(0, from - line.from).match(/\\begin\s*\{([^{}%]*)$/)?.[1].trim();
      if (!name) return false;
      const code = s.doc.toString().replace(/(^|[^\\])%.*$/gm, '$1'), quoted = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const count = (kind) => code.match(new RegExp(`\\\\${kind}\\s*\\{\\s*${quoted}\\s*\\}`, 'g'))?.length ?? 0;
      // an editor-closed `}` already completes this `\begin`
      const closed = s.sliceDoc(from, from + 1) === '}';
      if (count('begin') + !closed <= count('end')) return false;
      const indent = line.text.match(/^\s*/)[0];
      v.dispatch({ changes: [{ from, to: from + closed, insert: '}' }, { from: line.to, insert: `\n${indent}\\end{${name}}` }],
        selection: { anchor: from + 1 }, userEvent: 'input.type' });
      return true;
    });
    // The main file's heat (`heatShow`): a strip in the gutter, darker where a line read more tokens.
    const heatSet = st.StateEffect.define();
    const heatField = st.StateField.define({ create: () => null, update: (h, tr) => tr.effects.reduce((v, e) => e.is(heatSet) ? e.value : v, tr.docChanged ? null : h) });
    class HeatMark extends view.GutterMarker {
      constructor(share, n) { super(); this.share = share; this.n = n; }
      eq(o) { return o.n === this.n && o.share === this.share; }
      toDOM() { return Object.assign(document.createElement('span'), { className: 'cm-heat-mark', title: `${num(this.n)} tokens read from this line`, style: `--heat: ${this.share}` }); }
    }
    const heatGutter = view.gutter({
      class: 'cm-heat',
      lineMarker: (v, line) => { const h = v.state.field(heatField), n = h?.lines.get(v.state.doc.lineAt(line.from).number); return n ? new HeatMark(Math.max(0.12, Math.log1p(n) / Math.log1p(h.most)).toFixed(2), n) : null; },
      lineMarkerChange: (u) => u.startState.field(heatField) !== u.state.field(heatField),
    });
    const theme = new st.Compartment(), lock = new st.Compartment(), lang = new st.Compartment();
    const ev = new EditorView({
      parent, doc: text,
      extensions: [basicSetup, closing, lang.of(/\.bib$/.test(state.file) ? tex.bib : tex.language), language.syntaxHighlighting(tex.highlighter), lint.lintGutter({ tooltipFilter: () => [] }), lint.linter(null, { tooltipFilter: () => [] }), ac.autocompletion({ override: [complete], activateOnTypingDelay: 40, addToOptions: [{ position: 80, render: (c) => {
        // the detail: a usage line colored like the hover's, then where it comes from
        const span = Object.assign(document.createElement('span'), { className: 'cm-completionDetail' });
        const [, use, from] = (c.usage ?? '').match(/^(.*?)(?:\s+(\([^)]*\)))?$/) ?? [];
        span.innerHTML = (use ? cards.usage(use).html : '') + (from ? ` <span class="muted">${escape(from)}</span>` : '');
        // what the item draws (a line width, dash pattern, arrow tip or pattern), from the server
        if (c.preview) span.prepend(Object.assign(document.createElement('img'), { className: 'cm-preview', src: `data:image/svg+xml;utf8,${encodeURIComponent(c.preview)}`, alt: '' }));
        return span;
      } }] }),
        linkField, markField, swatchField, linkedField, heatField, heatGutter, mirror, st.Prec.high(closeEnvironment), view.keymap.of([{ key: 'F2', run: () => { renameBox(); return true; } }]),
        EditorView.updateListener.of((u) => { if (u.docChanged) { resolves.clear(); changed(); } else if (u.selectionSet) explainSoon(); if (u.docChanged || u.selectionSet) linkAround(u.view); }), theme.of([]), lock.of([])],
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
    let hovered = null, hoverTimer = 0;
    const unhover = () => { clearTimeout(hoverTimer); if (hovered) { hovered = null; tips.card.hide(150); } };
    parent.addEventListener('mousemove', (e) => {
      const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY });
      if (hovered && pos != null && pos >= hovered.from && pos <= hovered.to) return;
      unhover();
      if (pos == null || e.buttons) return;
      const version = doc();
      hoverTimer = setTimeout(async () => {
        const c = await hoverCard(ev, pos);
        if (!c || doc() !== version || !parent.matches(':hover')) return;
        const a = ev.coordsAtPos(c.from), b = ev.coordsAtPos(c.to) ?? a;
        if (!a) return;
        hovered = c;
        tips.card.show({ left: a.left, top: a.top, right: b.right, bottom: Math.max(a.bottom, b.bottom) }, c.html);
      }, 250);
    });
    parent.addEventListener('mouseleave', unhover);
    // a modifier alone (a screenshot shortcut, ctrl for the link) keeps the card
    parent.addEventListener('keydown', (e) => ['Control', 'Meta', 'Shift', 'Alt', 'OS', 'Super'].includes(e.key) || unhover());
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
      e.preventDefault();
      menuAt(e.clientX, e.clientY);
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
      // another file's text starts at its top: the last file's scroll position may lie past its end
      set: (t, readOnly = false) => { ev.dispatch({ effects: lang.reconfigure(/\.bib$/.test(state.file) ? tex.bib : tex.language) }); ev.dispatch({ changes: { from: 0, to: doc().length, insert: t }, selection: { anchor: 0 }, effects: lock.reconfigure(readOnly ? [st.EditorState.readOnly.of(true)] : []) }); ev.scrollDOM.scrollTop = 0; },
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
      complete: () => ac.startCompletion(ev),
      colors: (list) => ev.dispatch({ effects: swatches.of(list.map(([p, css]) => [at(doc(), p), css]).filter(([a]) => a != null && a <= doc().length)) }),
      heat: (lines) => ev.dispatch({ effects: heatSet.of(lines?.size ? { lines, most: Math.max(...lines.values()) } : null) }),
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
  const key = (d) => `${d.data?.rule ?? d.code}|${d.range.start.line}:${d.range.start.character}|${d.message}`;
  /** What the open file's paragraphs set, from the language server's last
   * analysis (`satex query typeset`); a click goes to the paragraph. */
  const typesetBody = $('typeset-body');
  const notation = self.satexCards.notation;
  // in the section's header: a click flips a switch, never the section's fold; kept across visits
  for (const [id, cls, store] of [['typeset-boxes', 'show-boxes', 'satex-typeset-boxes'], ['typeset-keys', 'show-keys', 'satex-typeset-keys'], ['typeset-ligs', 'show-ligs', 'satex-typeset-ligs']]) {
    const box = $(id), apply = () => typesetBody.classList.toggle(cls, box.checked);
    // the switch itself flips natively; its text flips it here
    box.closest('label').addEventListener('click', (e) => { if (e.target === box) return; e.preventDefault(); box.checked = !box.checked; box.dispatchEvent(new Event('change')); });
    try { box.checked = localStorage.getItem(store) === '1'; } catch {}
    box.onchange = () => { apply(); try { localStorage.setItem(store, box.checked ? '1' : '0'); } catch {} };
    apply();
  }
  /** The document's outline from the root's run (`satex query occurrences`), in reading order: its
   * sectioning units indented by level, each with its labels, then the bibliography and its entries. */
  // the examples fold away; kept across visits
  const exSect = $('play-exsect');
  try { exSect.open = localStorage.getItem('satex-examples') === '1'; } catch {}
  exSect.addEventListener('toggle', () => { try { localStorage.setItem('satex-examples', exSect.open ? '1' : '0'); } catch {} });
  const outlineBody = $('outline-body'), outlineOpen = new Set();
  outlineBody.addEventListener('toggle', (e) => { const id = e.target.dataset?.id; if (id) { if (e.target.open) outlineOpen.add(id); else outlineOpen.delete(id); } }, true);
  const LEVELS = ['part', 'chapter', 'section', 'subsection', 'subsubsection', 'paragraph', 'subparagraph'];
  async function outline() {
    // an edit in an included file leaves the held analysis of the root stale: a config forces a fresh one
    const request = { file: path(state.root), queries: [{ type: 'occurrences' }], ...(state.file !== state.root ? { config: { trace: 'false' } } : {}) };
    const rs = (await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).catch(() => null))?.[0]?.records;
    if (!rs) return;
    // sections in the run's order; labels, citations and the rest come after them, each placed by its source position
    const own = rs.filter((r) => !r.origin || r.origin === 'document'), file = (r) => r.path ?? r.file;
    const before = (a, b) => a.line < b.line || a.line === b.line && (a.col ?? 0) <= (b.col ?? 0);
    const firstOf = new Map();
    own.forEach((r, i) => { if (!firstOf.has(file(r))) firstOf.set(file(r), i); });
    const heads = own.map((r, i) => [r, i]).filter(([r]) => r.kind === 'section');
    /** The section a record stands in: the last one before it in its file, else the last one before its file starts. */
    const owner = (r) => heads.findLast(([s]) => file(s) === file(r) && before(s, r))?.[1] ?? heads.findLast(([, i]) => i < firstOf.get(file(r)))?.[1] ?? -1;
    const sections = own.filter((r) => r.kind === 'section'), present = [...new Set(sections.map((r) => LEVELS.indexOf(r.detail)).filter((l) => l >= 0))].sort((x, y) => x - y);
    const lowest = present.includes(1) ? 1 : 2; // chapters count as the top level when there are some, else sections
    const at = (r) => `data-path="${escape(r.path ?? r.file ?? '')}" data-line="${r.line}" data-col="${r.col ?? 1}"`;
    // LaTeX's numbers: a counter per level from the top one, deeper ones reset, three levels deep; parts count in Roman
    // numerals on their own; a starred command (read off its source) gets none
    const count = [], roman = (n) => ['', 'I', 'II', 'III', 'IV', 'V', 'VI', 'VII', 'VIII', 'IX', 'X'][n] ?? String(n);
    let parts = 0;
    const numberOf = (r) => {
      const l = LEVELS.indexOf(r.detail), text = state.texts[Object.keys(state.texts).find((n) => String(r.path ?? r.file).endsWith(n))]?.split('\n')[r.line - 1] ?? '';
      if (l < 0 || l < lowest - 1 && l > 0 || l >= lowest + 3 || /^\\[a-z]+\*/.test(text.slice((r.col ?? 1) - 1))) return '';
      if (l === 0) return roman(++parts);
      count.length = l + 1;
      for (let i = lowest; i <= l; i++) count[i] = (count[i] ?? 0) + (i === l ? 1 : 0);
      return count.slice(lowest, l + 1).join('.');
    };
    const base = (p) => String(p ?? '').split('/').pop();
    const html = [];
    let open = null;
    // one line per part; labels, files and citations unfold under it
    const row = (id, depth, head, body) => `<li class="o-sec" style="--d:${depth}">` + (body
      ? `<details class="o-row" data-id="${escape(id)}"${outlineOpen.has(id) ? ' open' : ''}><summary>${head}</summary>${body}</details>` : `<div class="o-row o-flat">${head}</div>`) + '</li>';
    const chips = (list) => list.length ? `<span class="o-labels">${list.join('')}</span>` : '';
    const close = () => {
      if (!open) return;
      const files = [...open.files].filter((f) => /\.(tex|ltx)$/.test(f) && state.files.some((n) => String(f).endsWith(n)));
      const cites = [...open.cites.entries()].map(([k, c]) => `<a href="#" class="o-go o-label o-bib" ${at(c[0])} title="cited ${c.length}× in this part">${escape(k)}</a>`);
      const fileChips = files.length > 1 || (files.length && !String(files[0]).endsWith(state.root)) ? files.map((f) => `<span class="o-file" title="a file this part is set from">${escape(base(f))}</span>`) : [];
      html.push(row(open.id, open.depth, open.head + (open.first ? `<span class="o-first" title="label ${escape(open.first)}">${escape(open.first)}</span>` : ''), chips(open.labels) + chips(fileChips) + chips(cites)));
      open = null;
    };
    const placed = own.filter((r) => r.kind !== 'section').map((r) => [r, owner(r)]);
    const inOrder = [...placed.filter(([, o]) => o < 0).map(([r]) => r), ...heads.flatMap(([s, i]) => [s,
      ...placed.filter(([, o]) => o === i).map(([r]) => r).sort((a, b) => file(a) === file(b) ? (before(a, b) ? -1 : 1) : 0)])];
    for (const r of inOrder) {
      if (r.kind === 'section') {
        close();
        const depth = Math.max(0, present.indexOf(LEVELS.indexOf(r.detail))), num = numberOf(r);
        open = { first: null, labels: [], cites: new Map(), files: new Set([r.path ?? r.file]), id: `${r.path ?? r.file}:${r.line}`, depth,
          head: `<a href="#" class="o-go" ${at(r)} title="${escape(r.detail)} at ${escape((r.file ?? '') + ':' + r.line)}">${num ? `<span class="o-num">${num}</span>` : ''}${escape(r.key)}</a>` };
      } else if (open && r.kind !== 'bibitem') {
        open.files.add(r.path ?? r.file);
        if (r.kind === 'cite') open.cites.set(r.key, [...(open.cites.get(r.key) ?? []), r]);
      }
      if (r.kind === 'label') {
        const chip = `<a href="#" class="o-go o-label" ${at(r)} title="label at ${escape((r.file ?? '') + ':' + r.line)}">${escape(r.key)}</a>`;
        if (open) { open.labels.push(chip); open.first ??= r.key; } else html.push(`<li class="o-sec" style="--d:0"><span class="o-labels">${chip}</span></li>`);
      }
    }
    close();
    const bib = own.filter((r) => r.kind === 'bibitem'), where = own.find((r) => r.kind === 'bibliography');
    if (bib.length || where) html.push(row('bibliography', 0, `<a href="#" class="o-go" ${at(where ?? bib[0])}><span class="o-kind">bibliography</span>${bib.length} entr${bib.length === 1 ? 'y' : 'ies'}</a>`,
      chips(bib.map((r) => `<span class="o-label o-bib" title="${escape((r.file ?? '') + ':' + r.line)}">${escape(r.key)}</span>`))));
    $('outline-count').textContent = String(sections.length || '');
    outlineBody.innerHTML = html.length ? `<ul class="outline-list">${html.join('')}</ul>` : '<p class="muted">No sections, labels or bibliography yet.</p>';
  }
  outlineBody.addEventListener('click', (e) => {
    const a = e.target.closest('.o-go');
    if (!a) return;
    e.preventDefault();
    const name = Object.keys(state.texts).find((n) => !(n in state.ro) && a.dataset.path.endsWith(n));
    openPlace(name ?? a.dataset.path, { line: +a.dataset.line - 1, character: Math.max(+a.dataset.col - 1, 0) });
  });
  /** The groups of other files unfolded, kept across checks. */
  const tsClosed = new Set();
  typesetBody.addEventListener('toggle', (e) => { const id = e.target.dataset?.id; if (id) { if (e.target.open) tsClosed.delete(id); else tsClosed.add(id); } }, true);
  /** The paragraph the cursor stands in, marked; with "only at cursor" the others hidden. */
  function typesetFollow() {
    const at = editor?.position()?.line + 1, own = [...typesetBody.querySelectorAll(':scope > p.ts[data-line]')];
    const here = own.filter((p) => +p.dataset.line <= at).at(-1) ?? null;
    for (const p of typesetBody.querySelectorAll('p.ts')) p.classList.toggle('ts-current', p === here);
    const only = !!$('typeset-here')?.checked;
    typesetBody.classList.toggle('ts-only-here', only);
    // nothing set where the cursor stands: said so, not the whole document
    typesetBody.querySelector('.ts-nothing')?.remove();
    if (only && !here) typesetBody.insertAdjacentHTML('afterbegin', '<p class="muted ts-nothing">Nothing is set at the cursor.</p>');
  }
  $('typeset-here')?.addEventListener('change', typesetFollow);
  const withKeys = cards.withKeys;
  async function typeset(target) {
    if (target !== uri()) return;
    const rows = await lsp('satex/typeset', { textDocument: { uri: target } }).catch(() => null);
    if (!rows || target !== uri()) return;
    const paragraphs = rows.filter((r) => r.kind === 'paragraph');
    const row = (r) => `<p class="ts${r.heading ? ` ts-h ts-${escape(r.heading)}` : ''}"${r.env ? ` data-env="${escape(r.env)}" title="set in ${escape(r.env)}"` : ''} data-path="${escape(r.path ?? '')}" data-line="${r.line}" data-col="${r.col}"><span class="ts-at">${r.line}</span><span class="ts-text">${notation(withKeys(r.marked ?? r.detail ?? '', r.keys, r.fonts, r.align, r.ligatures))}</span></p>`;
    // the whole document in order: this file's paragraphs as they are, a run from another file folded into one line
    const html = [];
    for (let i = 0; i < paragraphs.length;) {
      const r = paragraphs[i];
      if (r.path?.endsWith(state.file)) { html.push(row(r)); i++; continue; }
      let j = i;
      while (j < paragraphs.length && paragraphs[j].path === r.path) j++;
      const run = paragraphs.slice(i, j), name = (r.path ?? '').split('/').pop();
      const id = `${r.path}|${r.line}`;
      html.push(`<details class="ts-other" data-id="${escape(id)}"${tsClosed.has(id) ? '' : ' open'}><summary><span class="ts-file">${escape(name)}</span> <span class="muted">${plural(run.length, 'paragraph')}</span></summary>${run.map(row).join('')}</details>`);
      i = j;
    }
    typesetBody.innerHTML = html.length ? html.join('') : '<p class="muted">The document sets no paragraph.</p>';
    typesetFollow();
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
    // A finding on the word under the cursor while it is typed (`\dra` on the way to `\draw`)
    // waits until typing moves on: it is about text that is not finished.
    const typing = target === uri() && performance.now() - typedAt < 4000 && editor?.position?.();
    const unfinished = (d) => typing && d.range.start.line === typing.line && d.range.start.character <= typing.character && typing.character <= d.range.end.character + 1;
    // a file the build writes (.aux, .toc …) is no source of the reader's: its lines are never marked
    const generated = writtenFiles.some((f) => target.endsWith(`/${f.name}`));
    diagnostics[target] = generated ? [] : ds.filter((d) => !unfinished(d)).map((d) => ({ ...d, fixes: by.get(key(d)) ?? [] }));
    if (target === uri()) render();
    drawTree();
  }
  /** Each finding: its severity, place, rule and message once. */
  /** The findings the panel lists: the open file's, then every other project file's, each with its file. */
  let listed = [];
  function render() {
    const ds = diagnostics[uri()] ?? [];
    editor.diagnostics(ds);
    const others = Object.entries(diagnostics).filter(([u]) => u !== uri()).flatMap(([u, rs]) => rs.map((d) => ({ ...d, file: u.split('/').pop(), target: u })));
    listed = [...ds, ...others];
    if (retryCompletion && !running) { retryCompletion = false; asked.clear(); editor.complete?.(); }
    count.textContent = listed.length;
    pills(ds);
    if (checkStart) { timing('t-check', secs(performance.now() - checkStart), 'the language server analyzed the open file'); shownTime(secs(performance.now() - checkStart)); }
    checkStart = 0;
    phase();
    lspStatus.textContent = state.file in state.ro ? 'read-only file' : listed.length ? '' : 'none';
    const fixButton = (f, j) => `<button class="small apply fix-${f.safety}" data-j="${j}" title="${escape(f.title)}">Fix (${f.safety})</button>`;
    const item = (d, i) => `<li class="f-${sev(d)}${unfolded.has(key(d)) ? ' open' : ''}" data-i="${i}" tabindex="0">` +
      `<span class="msg" title="${escape(d.message ?? '')}">${d.number ? `<code class="rno" title="rule number">${escape(d.number)}</code> ` : ''}<b class="rn">${escape(d.code ?? '')}</b>: ${commandRefs(inline(d.message))}</span>` +
      `<span class="where"><b class="loc">${d.file ? `<span class="loc-file">${escape(d.file)}:</span>` : ''}${d.range.start.line + 1}:${d.range.start.character + 1}</b><span class="shape" title="${sev(d)}" aria-label="${sev(d)}"></span></span>` +
      `<span class="acts">${d.fixes.map(fixButton).join('')}<button class="small ghost why">${unfolded.has(key(d)) ? 'Hide rule' : 'Explain rule'}</button></span>` +
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
    listed.forEach((d, i) => {
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
    const d = listed[i];
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
    const d = listed[li.dataset.i];
    const apply = e.target.closest('.apply');
    if (apply) return fix(d.fixes[apply.dataset.j]);
    if (e.target.closest('.explain a')) return;
    // another file's finding opens that file at its place
    if (d.target && !e.target.closest('.why')) return openPlace(Object.keys(state.texts).find((n) => d.target.endsWith(`/${n}`)) ?? d.file, d.range.start);
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
  async function menuAt(x, y) {
    const w = editor.word(), p = editor.position();
    sync(); // renaming is offered where the server says the name is the project's own
    const renamable = w && !(state.file in state.ro) && !!(await lsp('textDocument/prepareRename', { textDocument: { uri: uri() }, position: p }).catch(() => null));
    const found = findingsAt(p.line, p.character).map(([d]) => d);
    const fixes = found.flatMap((d) => d.fixes);
    const rules = [...new Set(found.filter((d) => d.fixes.length).map((d) => d.code))];
    const editable = !(state.file in state.ro);
    const items = [
      ...(w?.[3] ? [['Go to definition', 'Ctrl+click', definition]] : []),
      ...(w && editable ? [['Find references', '', references]] : []), ...(renamable ? [['Rename symbol', 'F2', renameBox]] : []),
      ...(w?.[3] ? [['Explain', '', focusExplain], ['Show expansions', '', () => expansions(w)], ['Trace expansion', '', () => traceCall(`\\${w[2]}`, { file: state.file, line: p.line + 1, col: w[0] + 1 })]] : []),
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
    tips.card.hide();
    tips.note.hide();
    Object.assign(ctxMenu.style, { left: `${Math.max(4, Math.min(x, innerWidth - ctxMenu.offsetWidth - 8))}px`, top: `${Math.max(4, Math.min(y, innerHeight - ctxMenu.offsetHeight - 8))}px` });
    ctxMenu.onclick = (e) => { const li = e.target.closest('li'); if (li) { ctxMenu.hidden = true; items[li.dataset.i][2](); } };
  }
  addEventListener('mousedown', (e) => { if (!e.target.closest('.ctx')) for (const m of document.querySelectorAll('.ctx')) m.hidden = true; });
  // Escape closes the open menu, else the open dialog, wherever the focus is: the browser's own close
  // of a dialog opened by a script can wait for a user gesture, and the keys after it would go nowhere
  addEventListener('keydown', (e) => {
    if (e.key !== 'Escape' || e.defaultPrevented) return;
    const menus = [...document.querySelectorAll('.ctx:not([hidden])')], dialog = [...document.querySelectorAll('dialog[open]')].pop();
    drop.hidden = true;
    if (menus.length) menus.forEach((m) => { m.hidden = true; });
    else if (dialog) { e.preventDefault(); dialog.close(); }
  });

  // ---------------------------------------------------------------- the Explain macro panel

  /** The command at the caret or under the mouse, explained live: what `satex explain`
   * says of it in the current text, run in the worker and kept per name and text version.
   * The language server's hover fills in until the run is back. */
  const explainBox = $('play-explain'), explainBody = $('explain-body'), explainLabel = $('explain-name');
  var explainTimer = 0, explained = '';
  const explanations = new Map(); // `name|file|version|kernel` -> records
  function explainSoon(at) {
    if (!at) typesetFollow();
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
  /** The same for many names in one query, filling the same cache. */
  async function explainMany(names) {
    sync();
    const target = state.root ?? (state.file in state.ro ? manifest.sample : state.file), key = (n) => `${n}|${target}|${version}|${state.kernel}`;
    const todo = names.filter((n) => !explanations.has(key(n)));
    if (todo.length) {
      const request = { file: path(target), queries: [{ type: 'explain', names: todo }] };
      const all = lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).then((r) => r?.[0]?.records ?? []).catch(() => []);
      for (const n of todo) explanations.set(key(n), all.then((rs) => rs.filter((r) => r.name === n)));
    }
    return Promise.all(names.map((n) => explanations.get(key(n))));
  }
  /** The Explain macro panel shows `name`: its full record, or the hover until that is back. */
  async function explainName(name, at = null, records = null) {
    const k = `${name}|${version}|${state.file}`;
    if (k === explained && !records) return;
    explained = k;
    explainLabel.textContent = name;
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
  document.addEventListener('mouseover', async (e) => {
    const ref = e.target.closest?.('.cs-ref');
    if (!ref) return;
    const name = ref.dataset.cs, rs = (await explainRecords(name)).filter((r) => r.name === name);
    if (!rs.length || !ref.matches(':hover')) return;
    tips.card.show(ref, cards.explain(rs[0]));
  });
  document.addEventListener('mouseout', (e) => { if (e.target.closest?.('.cs-ref') && !tips.card.el.dataset.pin) tips.card.hide(150); });
  document.addEventListener('click', (e) => { const ref = e.target.closest?.('.cs-ref'); if (ref) { tips.card.hide(); explainName(ref.dataset.cs); } });
  // ---------------------------------------------------------------- library: every name in force, and a scratch line to try them

  const libList = $('lib-list'), libSearch = $('lib-search'), libFrom = $('lib-from'), libInternal = $('lib-internal');
  let libNames = [], libFor = '', libUses = new Map(), libPick = '';
  /** Where a name in force comes from, as the package filter names it. */
  const libSource = (r) => (!r.package || r.package.endsWith('.ltx') ? 'kernel' : r.package); // a format file (latex.ltx) is the kernel
  /** How useful a name is to a document, best first: used in the project, defined by it, then
   * public names before internals, documented ones (`does`) first, macros before primitives,
   * names of letters before control symbols. */
  const libUseful = (r) => {
    const used = libUses.get(r.name) ?? 0;
    return [used ? 0 : 1, r.package === 'document' ? 0 : 1, /[@_:]/.test(r.name.slice(1)) ? 1 : 0, r.does ? 0 : 1,
      r.tag === 'primitive' ? 1 : 0, /^\\[A-Za-z]+$/.test(r.name) ? 0 : 1, -used];
  };
  const byUse = (a, b) => { const x = libUseful(a), y = libUseful(b); for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return x[i] - y[i]; return 0; };
  /** The names in force at the end of the root's last run (`scope`), refreshed once per run. */
  async function libraryLoad() {
    const key = `${state.root ?? state.file}|${libInternal.checked}|${analyzedVersion}`;
    if (key === libFor) return libraryShow();
    libFor = key;
    libSig.clear();
    const records = await namesInForce(libInternal.checked);
    // an environment once, by its name: \X stays as the environment, its \endX goes
    const names = new Set(records.map((r) => r.name));
    const paired = (n) => names.has(`\\end${n.slice(1)}`) && n !== '\\end';
    libNames = records.filter((r) => !(r.name.startsWith('\\end') && r.name !== '\\end' && names.has(`\\${r.name.slice(4)}`)))
      .map((r) => (paired(r.name) && r.tag !== 'primitive' ? { ...r, tag: 'environment' } : r));
    libUses = new Map();
    for (const [file, text] of Object.entries(state.texts)) if (state.files.includes(file) && !data(file))
      for (const m of String(text).matchAll(/\\begin\s*\{([^{}]+)\}|\\([A-Za-z@]+|.)/g)) { const n = `\\${m[1] ?? m[2]}`; libUses.set(n, (libUses.get(n) ?? 0) + 1); }
    libraryShow();
  }
  /** The first 200 names the search matches: a name starting with it first, then one containing it, then a usage that does. */
  /** How well `name` matches `q`: a prefix, a part after `@`, `_` or `:`, anywhere, the letters in order, else none. */
  const libRank = (name, q) => {
    const n = name.toLowerCase().replace(/^\\/, '');
    if (!q) return 0;
    if (n.startsWith(q)) return 0;
    if (n.split(/[@_:]/).some((part) => part.startsWith(q))) return 1;
    if (n.includes(q)) return 2;
    let k = 0;
    for (const c of n) if (c === q[k]) k++;
    return k === q.length ? 3 : null;
  };
  let libKind = '';
  function libraryShow() {
    const q = libSearch.value.trim().toLowerCase().replace(/^\\/, ''), from = libPick;
    const kind = (r) => (r.tag === 'environment' ? 'environment' : r.tag === 'primitive' ? 'primitive' : 'command');
    // a name, else its description or usage, else an argument's type (`label` finds \ref by its ⟨label⟩)
    const about = (r) => `${r.effective ?? ''} ${r.does ?? ''} ${(libSig.get(r.name)?.types ?? []).join(' ')}`.toLowerCase();
    const hits = libNames
      .map((r) => [r, libRank(r.name, q) ?? (q && about(r).includes(q) ? 4 : null)])
      .filter(([r, rank]) => rank !== null && (!from || libSource(r) === from) && (!libKind || kind(r) === libKind))
      .sort((a, b) => a[1] - b[1] || byUse(a[0], b[0]) || a[0].name.length - b[0].name.length || a[0].name.localeCompare(b[0].name))
      .map(([r]) => r);
    libSigned(hits.slice(0, LIB_SIGNED));
    $('lib-count').textContent = libNames.length ? short(libNames.length) : '';
    const counts = { '': libNames.length };
    for (const r of libNames) counts[kind(r)] = (counts[kind(r)] ?? 0) + 1;
    $('lib-kinds').innerHTML = [['', 'all'], ['command', 'commands'], ['environment', 'environments'], ['primitive', 'primitives']]
      .map(([k, label]) => `<button type="button" class="lib-kind" aria-pressed="${k === libKind}" data-kind="${k}">${label} <b title="${num(counts[k] ?? 0)}">${short(counts[k] ?? 0)}</b></button>`).join('');
    libList.innerHTML = hits.slice(0, 200).map((r, i) => {
      const env = kind(r) === 'environment', use = r.effective ?? r.name, args = use.startsWith(r.name) ? use.slice(r.name.length) : '';
      const name = env ? r.name.slice(1) : r.name, shown = env ? `\\begin{${name}}${args}` : use;
      const tried = env ? `\\begin{${name}}${args}\n  \n\\end{${name}}` : use;
      const signed = libSig.get(r.name), line = (signed && cards.useOf(signed)) || (shown !== name ? slotsOf(shown) : '');
      return `<li tabindex="-1" data-i="${i}" data-name="${escape(r.name)}"><code class="cs-ref" data-cs="${escape(r.name)}" tabindex="0">${cards.csName(name)}</code>` +
      `<span class="lib-use">${line}</span><span class="lib-does">${escape(r.does ?? signed?.does ?? '')}</span>` +
      `<button type="button" class="lib-try-it" data-try="${escape(tried)}" title="try it" aria-label="try it">▸</button>` +
      `<span class="lib-pkg">${escape(libSource(r))}</span></li>`; }).join('') +
      (hits.length > 200 ? `<li class="muted">${num(hits.length - 200)} more: narrow the search</li>` : '') ||
      `<li class="muted">${!libNames.length ? 'Names in force, after each check.' : from && !libNames.some((r) => libSource(r) === from)
        ? `${escape(from)} is not loaded in this document: <code>\\usepackage{${escape(from)}}</code> loads it.` : 'No name matches.'}</li>`;
  }
  // the package filter: a search over the sources of the names in force with their counts, then the packages the playground knows
  const libFromList = $('lib-from-list');
  function libFromDraw() {
    const counts = new Map();
    for (const r of libNames) counts.set(libSource(r), (counts.get(libSource(r)) ?? 0) + 1);
    const prefix = `${manifest.kernels[state.kernel]?.id}/`;
    const known = [...new Set((manifest.packages ?? []).filter((p) => p.id.startsWith(prefix)).map((p) => p.label.replace(/\.cls$/, '')))].filter((p) => !counts.has(p)).sort();
    const manual = new Set((manifest.packages ?? []).filter((p) => p.manual).map((p) => p.label.replace(/\.cls$/, '')));
    const first = ['document', 'kernel'].filter((p) => counts.has(p));
    const loaded = [...counts.keys()].filter((p) => !first.includes(p)).sort();
    const q = libFrom.value.trim().toLowerCase(), all = q === libPick.toLowerCase();
    const hit = (p) => all || p.toLowerCase().includes(q);
    const row = (p, label = p) => `<li role="option" data-pick="${escape(p)}" aria-selected="${p === libPick}">${escape(label)}${manual.has(p) ? ' <span class="manual" title="its manual is mined for descriptions">docs</span>' : ''}${counts.has(p) ? ` <b class="count" title="${num(counts.get(p))} names">${short(counts.get(p))}</b>` : ''}</li>`;
    const rest = known.filter(hit);
    libFromList.innerHTML = [...(all || !q ? [row('', 'every package')] : []), ...[...first, ...loaded].filter(hit).map((p) => row(p)),
      ...(rest.length ? [`<li class="muted" role="presentation">not loaded</li>`, ...rest.map((p) => row(p))] : [])].join('') || '<li class="muted">No package matches.</li>';
    libFromList.hidden = false;
    libFrom.setAttribute('aria-expanded', 'true');
  }
  const libFromClose = () => { libFromList.hidden = true; libFrom.setAttribute('aria-expanded', 'false'); libFrom.value = libPick; };
  const libFromTake = (p) => { libPick = p; libFromClose(); libraryShow(); };
  libFrom.addEventListener('focus', libFromDraw);
  libFrom.addEventListener('input', libFromDraw);
  libFrom.addEventListener('blur', () => setTimeout(libFromClose, 150));
  libFromList.addEventListener('mousedown', (e) => { const li = e.target.closest('[data-pick]'); if (li) { e.preventDefault(); libFromTake(li.dataset.pick); } });
  libFrom.addEventListener('keydown', (e) => {
    const rows = [...libFromList.querySelectorAll('[data-pick]')], at = rows.findIndex((r) => r.getAttribute('aria-selected') === 'true');
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (libFromList.hidden) return libFromDraw();
      const next = rows[Math.max(0, Math.min(rows.length - 1, at + (e.key === 'ArrowDown' ? 1 : -1)))];
      rows.forEach((r) => r.setAttribute('aria-selected', String(r === next)));
      next?.scrollIntoView({ block: 'nearest' });
    } else if (e.key === 'Enter') { e.preventDefault(); libFromTake(rows[Math.max(0, at)]?.dataset.pick ?? libPick); }
    else if (e.key === 'Escape' && !libFromList.hidden) { e.preventDefault(); libFromClose(); }
  });
  /** Each listed name's explain record, asked for the first rows shown: their usage lines with typed
   * argument chips, as the hover card draws them, and their argument types for the search. */
  const LIB_SIGNED = 60, LIB_BATCH = 12, libSig = new Map();
  async function libSigned(rows) {
    const todo = rows.map((r) => r.name).filter((n) => !libSig.has(n));
    if (!todo.length) return;
    todo.forEach((n) => libSig.set(n, null));
    // a few at a time, so the first rows show theirs soon
    for (let k = 0; k < todo.length; k += LIB_BATCH) {
      const part = todo.slice(k, k + LIB_BATCH), found = await explainMany(part);
      part.forEach((n, i) => libSig.set(n, found[i]?.find((r) => !r.error) ?? found[i]?.[0] ?? null));
      libSignedShow(part);
    }
  }
  function libSignedShow(names) {
    for (const li of libList.querySelectorAll('li[data-name]')) {
      const r = names.includes(li.dataset.name) && libSig.get(li.dataset.name), use = r && cards.useOf(r);
      if (!r) continue;
      if (use) li.querySelector('.lib-use').innerHTML = use + (r.together?.length
        ? ` <details class="lib-with"><summary title="defined together with these">+${r.together.length}</summary>${r.together.map(cards.csChip).join(' ')}</details>` : '');
      if (!li.querySelector('.lib-does').textContent && r.does) li.querySelector('.lib-does').textContent = r.does;
    }
  }
  /** A usage line with its ⟨…⟩ placeholders as the explain cards draw them. */
  const slotsOf = (usage) => (cards.slots ? cards.slots(escape(usage)) : escape(usage));
  $('lib-kinds').addEventListener('click', (e) => { const b = e.target.closest('.lib-kind'); if (b) { libKind = b.dataset.kind; libraryShow(); } });
  // the cursor lands in an environment's body, else at the end
  const tryIt = (t) => {
    const text = $('lib-text'), v = t.dataset.try.replace(/[⟨⟩]/g, ''), at = v.indexOf('\n  \n') + 3;
    $('play-library').close();
    $('try-open').click();
    text.value = v; grow(); text.setSelectionRange(at > 2 ? at : v.length, at > 2 ? at : v.length);
  };
  libList.addEventListener('click', (e) => { const t = e.target.closest('.lib-try-it'); if (t) tryIt(t); });
  // ↓ from the search goes into the list, ↑/↓ move, Enter tries
  libSearch.addEventListener('keydown', (e) => { if (e.key === 'ArrowDown') { e.preventDefault(); libList.querySelector('li[data-i]')?.focus(); } });
  libList.addEventListener('keydown', (e) => {
    const li = e.target.closest('li[data-i]');
    if (!li) return;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); (e.key === 'ArrowDown' ? li.nextElementSibling : li.previousElementSibling ?? libSearch)?.focus(); }
    if (e.key === 'Enter') { e.preventDefault(); tryIt(li.querySelector('.lib-try-it')); }
  });
  libSearch.oninput = libraryShow;
  libInternal.onchange = libraryLoad;
  $('lib-open').onclick = () => { $('play-library').showModal(); libraryLoad(); $('lib-search').focus(); };
  $('try-open').onclick = () => { tryNote(); $('play-try').showModal(); $('lib-text').focus(); };
  let tryTimer = 0;
  $('lib-text').addEventListener('input', () => { clearTimeout(tryTimer); if ($('try-auto').checked) tryTimer = setTimeout(libraryTry, 500); });
  for (const [box, x] of [['play-library', 'lib-close'], ['play-try', 'try-close'], ['play-draw', 'draw-close'], ['play-config', 'cfg-close']]) {
    $(x).onclick = () => $(box).close();
    $(box).addEventListener('click', (e) => { if (e.target === e.currentTarget) e.currentTarget.close(); });
  }
  /** The scratch text read right after the root's `\begin{document}`: what it sets, its findings and what it expanded to. */
  /** Where Try reads its text in the root: right after `\begin{document}`, else (plain TeX) before `\bye`, else at the end. */
  function tryPlace(root = state.root ?? state.file, text = root === state.file ? editor.text() : state.texts[root] ?? '') {
    const lines = text.split('\n');
    const begin = lines.findIndex((l) => /\\begin\s*\{document\}/.test(l));
    if (begin >= 0) {
      const at = lines[begin].search(/\\begin\s*\{document\}/);
      return { root, i: begin, col: at + lines[begin].slice(at).indexOf('}') + 2, where: 'right after <code>\\begin{document}</code>' };
    }
    const bye = lines.findIndex((l) => /\\bye\b/.test(l));
    if (bye >= 0) return { root, i: bye, col: lines[bye].search(/\\bye\b/) + 1, where: 'right before <code>\\bye</code>' };
    const last = lines.length - 1;
    return { root, i: last, col: lines[last].length + 1, where: 'at the end' };
  }
  /** Try's other places: a scratch document of its own, under its kernel, in place of this one. */
  const TRY_ROOTS = {
    article: { kernel: 'pdftex', text: '\\documentclass{article}\n\\begin{document}\n\\end{document}\n', label: 'an empty article' },
    plain: { kernel: 'plain', text: '\\bye\n', label: 'an empty plain TeX file' },
  };
  const tryRoot = () => TRY_ROOTS[$('try-engine').value];
  const tryNote = () => {
    const scratch = tryRoot();
    $('try-note').innerHTML = scratch ? `Read in ${scratch.label}, under its own kernel.` : `Read ${tryPlace().where} of this document. The file stays as it is.`;
  };
  $('try-engine').onchange = () => { tryNote(); if ($('lib-text').value.trim()) libraryTry(); };
  /** The scratch root of `scratch` in the worker's files, its kernel loaded: [path, the kernel's flags as settings]. */
  async function tryScratch(scratch) {
    const k = manifest.kernels.findIndex((x) => x.id === scratch.kernel), name = `try-${scratch.kernel}.tex`;
    // the language server's instance holds the files it started with: a kernel fetched now needs a new one
    const fresh = !state.loaded.has(k);
    await loadKernel(k);
    if (fresh) await restart();
    post({ type: 'project', dir: '/work/try', files: { [name]: encode(scratch.text) } });
    const args = manifest.kernels[k].args, config = {};
    for (let j = 0; j + 1 < args.length; j += 2) if (args[j] === '--set') { const [key, ...v] = args[j + 1].split('='); config[key] = v.join('='); }
    return [`/work/try/${name}`, config];
  }
  async function libraryTry() {
    const text = $('lib-text').value, out = $('lib-out');
    if (!text.trim()) return;
    const scratch = tryRoot(), t0 = performance.now();
    $('try-status').innerHTML = `${self.satexProgress.html(null, 'typesetting')} typesetting…`;
    sync();
    const [file, kernelConfig] = scratch ? await tryScratch(scratch) : [path(state.root ?? state.file), {}];
    const { i, col } = scratch ? tryPlace(file, scratch.text) : tryPlace(), root = file.split('/').pop();
    const request = { file, inject: { text: `\n${text}\n`, at: `${i + 1}:${col}` }, config: { ...kernelConfig, trace: 'true', trace_document_only: 'false' },
      queries: [{ type: 'typeset' }, { type: 'diagnostics' }, { type: 'trace' }] };
    const [ts, ds, tr] = (await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).catch((e) => [{ error: String(e?.message ?? e) }])) ?? [];
    if (ts?.error) { out.innerHTML = `<p class="muted">${escape(ts.error)}</p>`; $('try-status').textContent = ''; return; }
    const lo = i + 2, hi = lo + text.split('\n').length - 1, mine = (r) => r.line >= lo && r.line <= hi && String(r.path ?? r.file ?? '').endsWith(root);
    const set = (ts.records ?? []).filter((r) => r.kind === 'paragraph' && mine(r));
    const found = (ds?.records ?? []).filter(mine);
    const top = (tr?.records ?? []).filter((r) => mine(r) && r.inserts != null).slice(0, 12);
    $('try-status').textContent = `typeset in ${secs(performance.now() - t0)} · ${plural(set.length, 'paragraph')}${found.length ? ` · ${plural(found.length, 'finding')}` : ''}`;
    out.innerHTML = `<div class="typeset">${set.map((r) => `<p class="ts"><span class="ts-text">${notation(withKeys(r.marked ?? r.detail ?? '', [], r.fonts, r.align, r.ligatures))}</span></p>`).join('') || '<p class="muted">It sets no text.</p>'}</div>` +
      (found.length ? `<ul class="lib-found">${found.map((d) => `<li><b>${escape(d.severity)}</b> ${escape(d.message)}</li>`).join('')}</ul>` : '') +
      (top.length ? `<details class="lib-steps"><summary>${plural(top.length, 'expansion')}</summary><ul>${top.map((r) => `<li><code class="cs-ref" data-cs="${escape(r.name)}">${escape(r.name)}</code> → <code>${escape(String(r.inserts).slice(0, 160))}</code></li>`).join('')}</ul></details>` : '');
  }
  // the box grows with its lines, up to eight
  const grow = () => { const t = $('lib-text'); t.rows = Math.min(8, Math.max(2, t.value.split('\n').length)); };
  $('lib-text').addEventListener('input', grow);
  $('lib-run').onclick = libraryTry;
  $('lib-text').addEventListener('keydown', (e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); libraryTry(); } });

  // ---------------------------------------------------------------- settings: the satex.yaml keys of the playground's runs

  // The keys, their types, defaults and descriptions come from satex.schema.json; each changed one
  // reaches every run and the language server as `--set key=value`.
  let configSet = {};
  try { configSet = JSON.parse(localStorage.getItem('satex-config')) ?? {}; } catch {}
  const configKeep = (set) => { configSet = set; try { localStorage.setItem('satex-config', JSON.stringify(set)); } catch {} };
  const yamlValue = (v) => (typeof v === 'string' ? v : JSON.stringify(v));
  function configFlags(set = configSet) { return Object.entries(set).flatMap(([k, v]) => ['--set', `${k}=${yamlValue(v)}`]); }
  /** Whether satex takes the settings `set`: null, else its reason, from a run that only reads them. */
  async function configRefused(set) {
    if (!Object.keys(set).length) return null;
    const r = await ask({ type: 'run', args: [...configFlags(set), 'query', '--list'], edits: {}, cwd: '/work' }).catch((e) => ({ status: 1, lines: [['err', e.message]] }));
    if (r.status === 0) return null;
    return r.lines.filter(([k, t]) => k === 'err' && !/ignored: /.test(t)).map(([, t]) => plain(t).replace(/^satex: /, '')).pop() ?? 'satex does not take it';
  }
  let configKeys = null;
  /** Settings set aside at the start ({set, why}): shown in the dialog, never applied. */
  let configRejected = null;
  const leaves = (props, at = '') => Object.entries(props ?? {}).flatMap(([k, p]) => ((p.type ?? 'object') === 'object' && p.properties
    ? leaves(p.properties, `${at}${k}.`) : [{ key: at + k, type: [].concat(p.type ?? 'string')[0], options: p.enum, def: p.default, about: p.description ?? '',
      affects: [].concat(p['x-affects'] ?? []), example: p['x-example'], items: p.items, min: p.minimum, max: p.maximum,
      suggest: (p.oneOf ?? []).flatMap((o) => o.enum ?? []), pattern: (p.oneOf ?? []).find((o) => o.pattern)?.pattern }]));
  /** A key's group: its first part (`limits`), or `general` for a key of one part. */
  const groupOf_ = (k) => (k.key.includes('.') ? k.key.split('.')[0] : 'general');
  /** The plugins (build.py `plugins`) and the keys each one reads: its own section, `plugins.<name>` its switch. */
  const PLUGINS = manifest.plugins ?? [];
  const pluginOf = (k) => PLUGINS.find((p) => k.key === `plugins.${p.name}` || k.key.split('.')[0] === p.name)?.name ?? null;
  let cfgTab = 'core', cfgYaml = false;
  const configLoad = () => (configKeys ??= fetch(`${base}${manifest.schema}`).then((r) => r.json()).then((s) => leaves(s.properties)).catch(() => []));
  const sameValue = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  function configInput(k, value) {
    if (k.type === 'boolean') return `<input type="checkbox" role="switch" data-cfg="${escape(k.key)}"${value ? ' checked' : ''}>`;
    if (k.options) return `<select class="dropdown" data-cfg="${escape(k.key)}">${k.options.map((o) => `<option${sameValue(o, value) ? ' selected' : ''}>${escape(o)}</option>`).join('')}</select>`;
    const text = value == null ? '' : typeof value === 'string' ? value : JSON.stringify(value);
    // a value of a few names or a pattern (`clock`: now, unknown or a date) completes from the names and today
    const suggest = k.suggest?.length ? [...k.suggest, ...(k.pattern?.includes('\\d{4}') ? [new Date().toISOString().slice(0, 10)] : [])] : [];
    const list = suggest.length ? `<datalist id="cfg-dl-${escape(k.key)}">${suggest.map((o) => `<option value="${escape(o)}">`).join('')}</datalist>` : '';
    return `<input type="${['integer', 'number'].includes(k.type) ? 'number' : 'text'}" data-cfg="${escape(k.key)}" value="${escape(text)}" spellcheck="false"${list ? ` list="cfg-dl-${escape(k.key)}"` : ''}${k.pattern ? ` pattern="${escape((k.suggest?.length ? `(${k.suggest.join('|')})|${k.pattern.replace(/^\^|\$$/g, '')}` : k.pattern).replace(/-\]/g, '\\-]'))}"` : ''}>${list}`;
  }
  /** The search: words that a key or its description holds, and filters: `@changed`, `@group:limits`, `@affects:precision`. */
  function configMatch(k, q) {
    const words = q.split(/\s+/).filter(Boolean);
    return words.every((w) => w === '@changed' ? k.key in configSet || `${k.key}+` in configSet
      : w.startsWith('@group:') ? groupOf_(k) === w.slice(7) : w.startsWith('@affects:') ? k.affects.includes(w.slice(9))
        : k.key.toLowerCase().includes(w) || k.about.toLowerCase().includes(w));
  }
  /** The plugins tab: those with settings as groups, then the others in one compact list, one line each. */
  function configPlugins(hits, q, section, closed) {
    const own = PLUGINS.map((p) => [p, hits.filter((k) => pluginOf(k) === p.name)]);
    const set = own.filter(([, ks]) => ks.length).map(([p, ks]) => section(p.name, p.name, p.about, ks, !!q || !closed.has(p.name)));
    const plain = own.filter(([p, ks]) => !ks.length && !q && p.about)
      .map(([p]) => `<li class="cfg-plain" title="${escape(p.about.replaceAll('`', ''))}"><b>${escape(p.name)}</b><span>${inline(p.about)}</span></li>`);
    return [...set, ...(plain.length ? [`<li class="cfg-group"><p class="cfg-plain-head">Without settings, always on</p><ul>${plain.join('')}</ul></li>`] : [])];
  }
  /** A setting's value an example line spells: `- field: v` items of a list, or `name: v` with the key's own name. */
  function exampleValue(k, line) {
    const read = (v) => { try { return JSON.parse(v); } catch { return /^\[.*\]$/.test(v) ? v.slice(1, -1).split(',').map((x) => x.trim()).filter(Boolean) : v; } };
    const item = line.match(/^- (\w+): (.+)$/);
    if (item && k.type === 'array') return [{ [item[1]]: read(item[2]) }];
    const own = line.match(/^(\w+): (.+)$/);
    return own && own[1] === k.key.split('.').at(-1) ? read(own[2]) : undefined;
  }
  /** An example as code, with the effect of each switch (`on:`, `off:`) and the prose apart, and a button that sets a value it spells. */
  function configExample(k) {
    const lines = typeof k.example === 'string' ? k.example.split('\n') : [configYamlOf({ [k.key]: k.example })];
    return lines.map((line) => {
      const effect = line.match(/^(on|off): (.*)$/);
      if (effect) return `<p class="cfg-effect"><span class="cfg-aff">${effect[1]}</span> ${inline(effect[2])}</p>`;
      const value = exampleValue(k, line);
      if (value !== undefined) return `<p class="cfg-try"><code>${escape(configYamlOf({ [k.key]: value }).replace(/\n\s*/g, ' '))}</code><button type="button" class="ghost small" data-use="${escape(k.key)}" data-value="${escape(JSON.stringify(value))}">use</button></p>`;
      return /^[\\%]/.test(line) ? `<pre><code>${escape(line)}</code></pre>` : `<p>${inline(line)}</p>`;
    }).join('');
  }
  /** A list's editor: a row per item, an object's fields side by side, remove and add. */
  function configList(k, value) {
    const fields = k.items?.properties ? Object.keys(k.items.properties) : null, list = Array.isArray(value) ? value : [];
    const item = (v, i) => `<li data-item="${i}">${fields ? fields.map((f) => `<input type="text" data-field="${escape(f)}" placeholder="${escape(f)}" value="${escape(v?.[f] ?? '')}" spellcheck="false">`).join('')
      : `<input type="text" value="${escape(typeof v === 'string' ? v : JSON.stringify(v))}" spellcheck="false">`}<button type="button" class="ghost small" data-drop="${i}" aria-label="remove">×</button></li>`;
    return `<ul class="cfg-items" data-cfg="${escape(k.key)}" data-list="1">${list.map(item).join('')}<li><button type="button" class="ghost small" data-add="1">Add</button></li></ul>`;
  }
  /** The settings changed, as the satex.yaml that sets them. */
  const configYamlOf = (set) => {
    const tree = {};
    for (const [k, v] of Object.entries(set)) { const parts = k.split('.'); let at = tree; parts.slice(0, -1).forEach((p) => { at = at[p] ??= {}; }); at[parts.at(-1)] = v; }
    const out = (o, pad) => Object.entries(o).map(([k, v]) => v && typeof v === 'object' && !Array.isArray(v) ? `${pad}${k}:\n${out(v, `${pad}  `)}` : `${pad}${k}: ${yamlValue(v)}`).join('\n');
    return out(tree, '');
  };
  /** The YAML view's text as settings: nested keys by indentation, each value as JSON or else as text; null when it does not read. */
  const configFromYaml = (text) => {
    const set = {}, stack = [];
    for (const line of text.split('\n')) {
      if (!line.trim() || /^\s*#/.test(line)) continue;
      const m = line.match(/^(\s*)([\w.+-]+):\s*(.*)$/);
      if (!m) return null;
      const depth = m[1].length;
      while (stack.length && stack.at(-1)[0] >= depth) stack.pop();
      const key = [...stack.map((s) => s[1]), m[2]].join('.');
      if (m[3] === '') { stack.push([depth, m[2]]); continue; }
      try { set[key] = JSON.parse(m[3]); } catch { set[key] = m[3]; }
    }
    return set;
  };
  async function configShow() {
    const keys = await configLoad(), q = $('cfg-search').value.trim().toLowerCase();
    for (const t of $('cfg-tabs').querySelectorAll('[data-tab]')) t.setAttribute('aria-pressed', String(t.dataset.tab === (cfgYaml ? 'yaml' : cfgTab)));
    $('cfg-yaml').hidden = !cfgYaml;
    $('cfg-list').hidden = cfgYaml;
    if (cfgYaml) { if (document.activeElement !== $('cfg-yaml')) $('cfg-yaml').value = configYamlOf(configSet); }
    // the filters as chips under the search: what the keys of this tab can be narrowed by
    const tabKeys = keys.filter((k) => (cfgTab === 'plugins') === !!pluginOf(k));
    const groups = [...new Set(tabKeys.map(groupOf_))], affects = [...new Set(tabKeys.flatMap((k) => k.affects))];
    // the filters complete a word that starts with `@`
    const typing = $('cfg-search').value.match(/(?:^|\s)(@\S*)$/)?.[1]?.toLowerCase();
    const filters = ['@changed', ...affects.map((a) => `@affects:${a}`), ...groups.map((g) => `@group:${g}`)].filter((f) => typing != null && f.startsWith(typing) && f !== typing);
    $('cfg-filters').hidden = !filters.length;
    $('cfg-filters').innerHTML = filters.map((f) => `<button type="button" class="lib-kind" data-filter="${escape(f)}">${escape(f)}</button>`).join('');
    const row = (k) => {
      const added = configSet[`${k.key}+`], changed = k.key in configSet || !!added;
      const rejected = configRejected && k.key in configRejected.set && !(k.key in configSet);
      const value = rejected ? configRejected.set[k.key] : k.key in configSet ? configSet[k.key] : k.def;
      const badges = k.affects.map((a) => `<span class="cfg-aff" title="changing it changes the ${escape(a)}">${escape(a)}</span>`).join('');
      const example = k.example == null ? '' : `<details class="cfg-example"><summary>example</summary>${configExample(k)}</details>`;
      return `<li class="cfg-row${changed ? ' cfg-changed' : ''}${rejected ? ' cfg-bad' : ''}"><div class="cfg-what"${rejected ? ` data-why="not applied: ${escape(configRejected.why)}"` : ''}><span><code>${k.key.includes('.') ? `<span class="cfg-prefix">${escape(k.key.slice(0, k.key.lastIndexOf('.') + 1))}</span>` : ''}${escape(k.key.slice(k.key.lastIndexOf('.') + 1))}</code>${badges}</span><span class="muted">${escape(k.about)}</span>` +
        `${added ? `<span class="muted">adds ${added.map((x) => `<code>${escape(x)}</code>`).join(', ')}</span>` : ''}${example}</div>` +
        (k.type === 'array' ? `<div class="cfg-value">${configList(k, value)}</div>` : `<label class="${k.type === 'boolean' ? 'ts-toggle ' : ''}cfg-value">${configInput(k, value)}</label>`) +
        `<button type="button" class="ghost small" data-reset="${escape(k.key)}"${changed ? '' : ' hidden'} title="back to the default: ${escape(yamlValue(k.def ?? ''))}">Reset</button></li>`;
    };
    const changedIn = (ks) => ks.filter((k) => k.key in configSet || `${k.key}+` in configSet).length;
    const hits = tabKeys.filter((k) => configMatch(k, q));
    const section = (id, head, about, ks, open) => `<li class="cfg-group"><details${open ? ' open' : ''} data-group="${escape(id)}"><summary><b>${escape(head)}</b>` +
      `${about ? ` <span class="muted">${escape(about)}</span>` : ''} <span class="muted">${plural(ks.length, 'key')}${changedIn(ks) ? `, ${changedIn(ks)} changed` : ''}</span></summary><ul>${ks.map(row).join('')}</ul></details></li>`;
    $('cfg-list').innerHTML = (cfgTab === 'plugins'
      ? configPlugins(hits, q, section, cfgClosed)
      : groups.map((g) => [g, hits.filter((k) => groupOf_(k) === g)]).filter(([, ks]) => ks.length).map(([g, ks]) => section(g, g, '', ks, !!q || changedIn(ks) > 0 || !cfgClosed.has(g)))).join('')
      || '<li class="muted">No setting matches.</li>';
    const n = Object.keys(configSet).length;
    $('cfg-total').textContent = n ? `${plural(n, 'setting')} changed` : 'every setting at its default';
  }
  let configTimer = 0;
  const configApply = (set) => { configKeep(set); configShow(); clearTimeout(configTimer); configTimer = setTimeout(() => restart(), 600); };
  $('cfg-open').onclick = () => { $('play-config').showModal(); configShow(); $('cfg-search').focus(); };
  $('cfg-search').oninput = configShow;
  const cfgClosed = new Set();
  $('cfg-list').addEventListener('toggle', (e) => { const g = e.target.dataset?.group; if (g) { if (e.target.open) cfgClosed.delete(g); else cfgClosed.add(g); } }, true);
  $('cfg-tabs').addEventListener('click', (e) => { const t = e.target.closest('[data-tab]'); if (!t) return; cfgYaml = t.dataset.tab === 'yaml'; if (!cfgYaml) cfgTab = t.dataset.tab; configShow(); });
  $('cfg-filters').addEventListener('click', (e) => {
    const f = e.target.closest('[data-filter]')?.dataset.filter, s = $('cfg-search');
    if (!f) return;
    s.value = `${s.value.replace(/@\S*$/, '')}${f} `;
    s.focus();
    configShow();
  });
  // the YAML view: the whole of the changed settings as satex.yaml, read back and checked as it is typed
  let yamlTimer = 0;
  $('cfg-yaml').addEventListener('input', () => {
    clearTimeout(yamlTimer);
    yamlTimer = setTimeout(async () => {
      const box = $('cfg-yaml'), set = configFromYaml(box.value), why = set ? await configRefused(set) : 'it does not read as `key: value` lines';
      box.classList.toggle('cfg-yaml-bad', !!why);
      $('cfg-total').textContent = why ? `not applied: ${why}` : '';
      if (!why && !sameValue(set, configSet)) configApply(set);
    }, 500);
  });
  // a list's rows: an item added or removed, any field changed, is the key's new value
  $('cfg-list').addEventListener('click', (e) => {
    const list = e.target.closest('[data-list]');
    if (!list) return;
    if (e.target.closest('[data-add]')) { const li = document.createElement('li'); li.dataset.item = 'new'; li.innerHTML = list.querySelector('li[data-item]')?.innerHTML.replace(/value="[^"]*"/g, 'value=""') ?? '<input type="text" spellcheck="false"><button type="button" class="ghost small" data-drop aria-label="remove">×</button>'; list.lastElementChild.before(li); li.querySelector('input')?.focus(); }
    else if (e.target.closest('[data-drop]')) { e.target.closest('li').remove(); list.dispatchEvent(new Event('change', { bubbles: true })); }
  });
  $('cfg-list').addEventListener('change', async (e) => {
    const el = e.target.closest('[data-cfg]'), k = el && (await configLoad()).find((x) => x.key === el.dataset.cfg);
    if (!k) return;
    const listOf = (ul) => [...ul.querySelectorAll('li[data-item]')].map((li) => {
      const fields = [...li.querySelectorAll('[data-field]')];
      if (fields.length) return Object.fromEntries(fields.filter((f) => f.value.trim()).map((f) => [f.dataset.field, f.value.trim()]));
      const v = li.querySelector('input').value.trim();
      try { return JSON.parse(v); } catch { return v; }
    }).filter((v) => v !== '' && !(typeof v === 'object' && !Object.keys(v).length));
    let value = el.dataset.list ? listOf(el) : el.type === 'checkbox' ? el.checked : el.value;
    el.setCustomValidity?.('');
    el.closest('.cfg-row').classList.remove('cfg-bad');
    const refuse = (why) => { el.setCustomValidity?.(why); el.reportValidity?.(); el.closest('.cfg-row').classList.add('cfg-bad'); el.closest('.cfg-row').querySelector('.cfg-what').dataset.why = why; };
    if (['integer', 'number'].includes(k.type)) {
      value = Number(value);
      if (!Number.isFinite(value) || (k.type === 'integer' && !Number.isInteger(value))) return refuse(`a${k.type === 'integer' ? 'n integer' : ' number'}`);
      if (k.min != null && value < k.min) return refuse(`at least ${k.min}`);
      if (k.max != null && value > k.max) return refuse(`at most ${k.max}`);
    } else if (k.type === 'object' && !el.dataset.list) { try { value = JSON.parse(value); } catch { return refuse('JSON: {"key": "value"}'); } }
    const { [k.key]: _, ...rest } = configSet;
    const next = sameValue(value, k.def) ? rest : { ...rest, [k.key]: value };
    // a value satex refuses is shown, never applied: the language server would not start with it
    const why = await configRefused(next);
    if (why) return refuse(why);
    configApply(next);
  });
  $('cfg-list').addEventListener('click', (e) => { const u = e.target.closest('[data-use]'); if (u) { configApply({ ...configSet, [u.dataset.use]: JSON.parse(u.dataset.value) }); return; } });
  $('cfg-list').addEventListener('click', (e) => { const b = e.target.closest('[data-reset]'); if (b) { const { [b.dataset.reset]: _, [`${b.dataset.reset}+`]: __, ...rest } = configSet; configApply(rest); } });
  $('cfg-reset').onclick = () => configApply({});

  // ---------------------------------------------------------------- draw: a symbol drawn, the commands that set it

  // draw.js matches; the playground gives it the names in force and inserts what is picked
  const drawPicker = self.satexDraw({
    root: $('play-draw'), base, symbols: manifest.symbols,
    inForce: async () => (await namesInForce(true)).map((r) => [r.name, libSource(r)]),
    usepackage: true,
    // the name at the cursor; with its package, the \usepackage line too, before \begin{document} when it is not there yet
    pick: (name, pkg, use) => {
      const at = editor.position(), lines = editor.text().split('\n'), begin = lines.findIndex((l) => /\\begin\s*\{document\}/.test(l));
      const edits = [{ range: { start: at, end: at }, newText: name }];
      const adds = use && begin >= 0 && !editor.text().includes(use);
      if (adds) edits.unshift({ range: { start: { line: begin, character: 0 }, end: { line: begin, character: 0 } }, newText: `${use}\n` });
      editor.apply(edits);
      $('play-draw').close();
      toast(adds ? `${name} inserted, ${use} added` : `${name} inserted`);
    },
  });
  $('draw-open').onclick = () => { $('play-draw').showModal(); drawPicker.load(); };

  // ---------------------------------------------------------------- trace: how one call expands

  const traceBox = $('play-trace'), traceBody = $('trace-body'), traceInternal = $('trace-internal');
  const STEP_KIND = { expand: 'kernel', define: 'package', 'open-file': 'class' };
  let traceFor = null, traceRun = 0;
  let traceLive = null;
  /** Where to trace `name`: the caret's own call, else its first use in the open file, else in any file of the project. */
  self.satexCalled = (name) => callSite(name) != null;
  function callSite(name) {
    const w = editor.word(), p = editor.position();
    if (w?.[3] && `\\${w[2]}` === name) return { file: state.file, line: p.line + 1, col: w[0] + 1 };
    const esc = name.replace(/[\\^$.*+?()[\]{}|]/g, '\\$&'), re = new RegExp(`${esc}(?![A-Za-z@])`);
    for (const file of [state.file, ...state.files.filter((f) => f !== state.file)]) {
      const lines = (file === state.file ? editor.text() : state.texts[file] ?? '').split('\n');
      for (let i = 0; i < lines.length; i++) { const m = re.exec(lines[i]); if (m && !new RegExp(`(?:\\\\(?:re)?(?:new|provide)command|\\\\[gex]?def|\\\\let)\\*?\\s*\\{?\\s*${esc}(?![A-Za-z@])`).test(lines[i])) return { file, line: i + 1, col: m.index + 1 }; }
    }
    return null;
  }
  /** The step's name as the server spells it (a register as `\count188`). */
  const stepName = (n) => String(n ?? '');
  const OPENS = /^\\(?:if|unless)/, MIDS = /^\\(?:else|or):?$/, CLOSES = /^\\fi:?$/;
  /** The run's steps as a tree by depth. Library work called by a project step (or by none) folds into one
   * group per run of sibling library steps; its steps show, and step, only when asked. */
  function traceModel(steps, own, internal) {
    const parent = steps.map(() => -1), stack = [];
    for (let i = 0; i < steps.length; i++) {
      while (stack.length && steps[stack.at(-1)].depth >= steps[i].depth) stack.pop();
      parent[i] = stack.at(-1) ?? -1;
      stack.push(i);
    }
    const mine = steps.map(own), top = steps.map((_, i) => !mine[i] && (parent[i] < 0 || mine[parent[i]]));
    // gof: the group each library step belongs to, through its library callers
    const groups = [], gof = steps.map(() => -1), lastKid = new Map();
    for (let i = 0; i < steps.length; i++) {
      const p = parent[i], prev = lastKid.get(p);
      lastKid.set(p, i);
      if (mine[i]) continue;
      if (!top[i]) { gof[i] = gof[p]; groups[gof[i]].size++; continue; }
      if (prev != null && top[prev]) gof[i] = gof[prev];
      else { gof[i] = groups.length; groups.push({ parent: p, first: i, size: 0 }); }
      groups[gof[i]].size++;
    }
    const shown = steps.map((_, i) => mine[i] || internal);
    const vparent = steps.map(() => -1), kids = new Map([[-1, []]]);
    const put = (k, v) => { if (!kids.has(k)) kids.set(k, []); kids.get(k).push(v); };
    for (let i = 0; i < steps.length; i++) {
      if (top[i] && groups[gof[i]].first === i) put(groups[gof[i]].parent, `g${gof[i]}`);
      if (!shown[i]) continue;
      let p = parent[i];
      while (p >= 0 && !shown[p]) p = parent[p];
      vparent[i] = p;
      put(top[i] ? `g${gof[i]}` : p, i);
    }
    // `\if…` with its decision, `\else`/`\or` and `\fi`; `\begin` with its `\end`
    const pair = new Map(), conds = [], envs = [];
    const link = (a, b, last) => { if (a == null) return; pair.set(b, a); if (last || !pair.has(a)) pair.set(a, b); };
    steps.forEach((r, i) => {
      if (!shown[i]) return;
      const n = r.name ?? '';
      if (r.step === 'execute' && OPENS.test(n)) conds.push(i);
      else if (r.step === 'branch' || r.step === 'condition' || MIDS.test(n)) link(conds.at(-1), i, false);
      else if (CLOSES.test(n)) link(conds.pop(), i, true);
      else if (n === '\\begin') envs.push(i);
      else if (n === '\\end') link(envs.pop(), i, true);
    });
    const order = steps.map((_, i) => i).filter((i) => shown[i]);
    return { steps, parent, vparent, kids, pair, order, groups, gof, top, internal, cur: -1, does: new Map() };
  }
  let trace = null;
  const clip = (t) => (t.length > 160 ? `${t.slice(0, 160)}…` : t);
  /** Text with each control sequence in it hoverable. */
  const csText = (t) => {
    t = String(t);
    let out = '', k = 0;
    for (const m of t.matchAll(/\\(?:[^\\\s{}#]+(?= |$)|[^\s])/g)) { out += `${escape(t.slice(k, m.index))}<span class="tr-cs" data-cs="${escape(m[0])}">${escape(m[0])}</span>`; k = m.index + m[0].length; }
    return out + escape(t.slice(k));
  };
  const keyOf = (s) => (String(s)[0] === 'g' ? String(s) : +s);
  const groupLabel = (g) => {
    const name = stepName(trace.steps[g.first].name);
    return /everypar/.test(name) ? `paragraph starts (everypar hook, ${plural(g.size, 'step')})` : `library work from ${name} (${plural(g.size, 'step')})`;
  };
  /** How a call got its result, one row: the body, each argument, the result. */
  /** Text the step panel shows whole: its middle folds when long, the arguments' text marked where they were put. */
  function longText(t, marks = []) {
    const html = (x) => marks.filter(Boolean).reduce((parts, m) => parts.flatMap((p) => (typeof p === 'string' ? p.split(m).flatMap((q, i) => (i ? [{ m }, q] : [q])) : [p])), [x])
      .map((p) => (typeof p === 'string' ? csText(p) : `<mark>${csText(p.m)}</mark>`)).join('');
    if (t.length <= LONG.head + LONG.tail + LONG.slack) return html(t);
    return `<details class="tr-long"><summary>${html(t.slice(0, LONG.head))} <span class="tr-more">… ${num(t.length - LONG.head - LONG.tail)} characters</span> ${html(t.slice(-LONG.tail))}</summary>${html(t)}</details>`;
  }
  const LONG = { head: 220, tail: 60, slack: 40 };
  function howOf(r, cut = clip) {
    const whole = cut !== clip, text = (t, marks) => (whole ? longText(String(t), marks) : csText(cut(t)));
    const bits = r.body != null && r.arguments?.length
      ? [`<span class="tr-bit"><i>body</i> <code>${text(r.body)}</code></span>`, ...r.arguments.map((a, k) => `<span class="tr-bit"><code>#${k + 1}</code> = <code>${csText(cut(a))}</code></span>`)] : [];
    if (r.inserts != null) bits.push(`<span class="tr-bit tr-res">→ <code class="tr-out">${text(r.inserts, r.arguments) || '<i>empty</i>'}</code></span>`);
    else if (r.detail) bits.push(`<span class="tr-bit">${['assign', 'define'].includes(r.step) ? '= ' : ''}<code class="tr-out">${csText(cut(r.detail))}</code></span>`);
    return bits.length ? `<span class="tr-how">${bits.join('')}</span>` : '';
  }
  const regAlias = new Map();
  const aliasOf = (name) => (regAlias.has(name) ? `<span class="tr-alias" title="the name the document's packages gave this register">${regAlias.get(name).map((n) => cards.csName(n)).join(', ')}</span>` : '');
  const csCode = (name, extra = '') => (/^\\/.test(name) ? `<code class="tr-cs${extra}" data-cs="${escape(name)}" tabindex="0">${cards.csName(name)}</code>${aliasOf(name)}` : `<code class="${extra.trim()}">${escape(name)}</code>`);
  function stepHead(i) {
    const r = trace.steps[i], name = stepName(r.name);
    const sets = r.sets ? `<span class="tr-sets" title="text this step set">sets <code>${escape(clip(r.sets))}</code></span>` : '';
    const j = trace.pair.get(i), mate = j == null ? '' : stepName(trace.steps[j].name) + (trace.steps[j].detail ? ` ${trace.steps[j].detail}` : '');
    const link = j == null ? '' : `<a href="#" class="tr-pair" data-go="${j}" title="go to the matching ${escape(mate)}">↔ ${escape(mate)}</a>`;
    const does = trace.does.get(r.name) ? `<span class="tr-does">${escape(trace.does.get(r.name))}</span>` : '';
    return `${cacheBadge(STEP_KIND[r.step] ?? 'hashes', r.step)}${csCode(name)}${howOf(r)}${sets}${link}` +
      `<a href="#" class="tr-at o-go" data-path="${escape(r.path ?? r.file ?? '')}" data-line="${r.line}" data-col="${r.col ?? 1}">${escape(r.file ?? '')}:${r.line}</a>${does}`;
  }
  /** One step or library group; its deeper steps are drawn when it first opens, so a long trace costs only what is looked at. */
  function stepNode(k) {
    if (typeof k === 'string') {
      const label = `<span class="tr-glabel">${escape(groupLabel(trace.groups[+k.slice(1)]))}</span>`;
      return trace.internal ? `<li data-i="${k}"><details class="tr-node tr-group"><summary>${label}</summary><ul></ul></details></li>`
        : `<li data-i="${k}"><div class="tr-leaf tr-group" title="library code; turn on “show package and kernel steps” to open it">${label}</div></li>`;
    }
    return (trace.kids.get(k) ?? []).length ? `<li data-i="${k}"><details class="tr-node"><summary>${stepHead(k)}</summary><ul></ul></details></li>` : `<li data-i="${k}"><div class="tr-leaf">${stepHead(k)}</div></li>`;
  }
  function fill(li) {
    const ul = li.querySelector(':scope > details > ul');
    if (ul && !ul.childElementCount) ul.innerHTML = (trace.kids.get(keyOf(li.dataset.i)) ?? []).map(stepNode).join('');
  }
  const rowOf = (i) => traceBody.querySelector(`li[data-i="${i}"] > details > summary, li[data-i="${i}"] > .tr-leaf`);
  const openLi = (k) => { const li = traceBody.querySelector(`li[data-i="${k}"]`); if (!li) return false; fill(li); const d = li.querySelector(':scope > details'); if (d) d.open = true; return true; };
  /** Select step `i`: its callers open, it marked and in view, the counter on it. */
  function stepTo(i) {
    if (!trace || i == null || i < 0 || !trace.order.includes(i)) return;
    const chain = [];
    for (let p = trace.vparent[i]; p >= 0; p = trace.vparent[p]) chain.unshift(p);
    for (const a of [...chain, i]) {
      if (trace.top[a] && !openLi(`g${trace.gof[a]}`)) break;
      if (a !== i && !openLi(a)) break;
    }
    traceBody.querySelector('.tr-cur')?.classList.remove('tr-cur');
    const row = rowOf(i);
    row?.classList.add('tr-cur');
    row?.scrollIntoView({ block: 'nearest' });
    trace.cur = i;
    const at = trace.order.indexOf(i);
    for (const b of traceBody.querySelectorAll('[data-move]')) {
      const off = (['prev', 'out'].includes(b.dataset.move) && (b.dataset.move === 'prev' ? at <= 0 : trace.vparent[i] < 0)) || (['next', 'over'].includes(b.dataset.move) && at >= trace.order.length - 1);
      b.disabled = off; b.setAttribute('aria-disabled', String(off));
    }
    $('trace-step').textContent = `step ${at + 1} of ${trace.order.length}`;
    // the one step: where it sits, what it is, how it got its result, and what the input holds after it
    const r = trace.steps[i], name = stepName(r.name);
    traceStream(i);
    const crumbs = [];
    for (let p = trace.parent[i]; p >= 0; p = trace.parent[p]) if (trace.steps[p].step === 'expand') crumbs.unshift(p);
    const crumb = (p) => (trace.order.includes(p) ? `<a href="#" class="tr-crumb" data-go="${p}" title="go to this call">${escape(stepName(trace.steps[p].name))}</a>` : `<span class="tr-crumb" title="a library step: turn on “show package and kernel steps” to go there">${escape(stepName(trace.steps[p].name))}</span>`);
    const nav = `<nav class="tr-crumbs" aria-label="the calls this step is inside">${[...crumbs.map(crumb), `<b class="tr-crumb on" aria-current="step">${escape(name)}</b>`].join('<span class="tr-sep">›</span>')}</nav>`;
    $('trace-now').innerHTML = nav + `<div class="tr-now-head">${cacheBadge(STEP_KIND[r.step] ?? 'hashes', r.step)}${csCode(name)}` +
      `<a href="#" class="tr-at o-go" data-path="${escape(r.path ?? r.file ?? '')}" data-line="${r.line}" data-col="${r.col ?? 1}">${escape(r.file ?? '')}:${r.line}</a></div>` +
      (r.inserts != null || r.detail ? `<div class="tr-now-row"><b>${r.inserts != null ? 'result' : 'detail'}</b>${howOf(r, (t) => t)}</div>` : '') +
      (r.sets ? `<div class="tr-now-row"><b>sets</b><code>${escape(r.sets)}</code></div>` : '') + statePanel(r);
  }
  /** The meanings this step's input names, searchable, and every name in force on request. */
  function statePanel(r) {
    const ms = Object.entries(r.meanings ?? {});
    return `<details class="tr-state" data-step="${trace.steps.indexOf(r)}"${trace.stateOpen ? ' open' : ''}><summary>State at this step: ${plural(ms.length, 'name')} ahead</summary>` +
      `<input type="search" class="tr-filter" placeholder="filter names" aria-label="filter names" value="${escape(trace.filter ?? '')}">` +
      `<ul class="tr-means">${ms.map(([n, m]) => `<li data-n="${escape(n)}"${n.toLowerCase().includes((trace.filter ?? '').toLowerCase()) ? '' : ' hidden'}>${csCode(n)} <code class="tr-mean">${csText(m)}</code></li>`).join('') || '<li class="muted">The input ahead names no command.</li>'}</ul>` +
      `<button type="button" class="ghost small tr-allnames">all names at this step</button><ul class="tr-means tr-every"></ul></details>`;
  }
  /** Catcode names, read from the wiki's table once. */
  let catNames = null;
  const loadCatNames = () => (catNames ??= fetch(`${base}wiki/domains.html`).then((r) => r.text()).then((t) => {
    const doc = new DOMParser().parseFromString(t, 'text/html');
    return new Map([...doc.querySelectorAll('a[id^="catcode-"]')].map((a) => [+a.id.slice(8), a.closest('tr')?.cells[1]?.textContent.trim() ?? '']));
  }).catch(() => new Map()));
  let cats = new Map();
  /** A category by its name in the wiki's table (tex.web § 207) and its number: "begin group (1)". */
  const catTitle = (c) => (c === 16 ? 'control sequence' : cats.get(c) ? `${cats.get(c)} (${c})` : `catcode ${c}`);
  const STREAM = { head: 16, tail: 6 };
  const tokChip = (t, c, ins = false) => (c === 16 ? csCode(t, ` tr-tok k16${ins ? ' tr-put' : ''}`) : `<a class="tr-tok k${c}${ins ? ' tr-put' : ''}" href="${base}wiki/domains.html#catcode-${c}" target="_blank" title="${escape(catTitle(c))}">${t === ' ' ? '␣' : escape(t)}<sub class="tr-cc">${c}</sub></a>`);
  /** An expansion's result as tokens: its control sequences, then single characters. */
  const insTokens = (t) => [...String(t).matchAll(/(\\(?:[^\\\s{}#]+(?= |$)|[^]))( ?)|([^])/g)];
  const insChip = (m) => (m[1] ? csCode(m[1], ' tr-tok tr-ins') : `<span class="tr-tok tr-ins" title="put back by this step">${m[3] === ' ' ? '␣' : escape(m[3])}</span>`);
  /** Chips, the middle of a long run folded into a button that shows them all. */
  const foldChips = (chips) => (chips.length > STREAM.head + STREAM.tail && !trace.wholeStream
    ? [...chips.slice(0, STREAM.head), `<button type="button" class="tr-more" data-whole="1">… ${num(chips.length - STREAM.head - STREAM.tail)} tokens</button>`, ...chips.slice(-STREAM.tail)] : chips).join('');
  /** The file text after the queued tokens in `ahead`. */
  function fileRest(r) {
    const a = String(r.ahead ?? '');
    let k = 0;
    for (const [t, c] of r.tokens ?? []) {
      if (!a.startsWith(t, k)) return '';
      k += t.length;
      if (c === 16 && a[k] === ' ') k++;
    }
    return a.slice(k);
  }
  /** The input after step `i`: what it put back marked, then the tokens that were ahead, then the file. */
  function traceStream(i) {
    const box = $('trace-stream');
    if (!box) return;
    const r = trace.steps[i];
    // the queued tokens carry their catcodes; the first of them are the ones this step put back
    const putBack = r.step === 'expand' && r.inserts != null ? insTokens(r.inserts).length : 0, queued = r.tokens ?? [];
    const put = putBack && !queued.length ? foldChips(insTokens(r.inserts).map(insChip)) : '';
    const shown = queued.length > STREAM.head + STREAM.tail && !trace.wholeStream
      ? [...queued.slice(0, STREAM.head).map((t, j) => [t, j]), null, ...queued.slice(-STREAM.tail).map((t, j) => [t, queued.length - STREAM.tail + j])] : queued.map((t, j) => [t, j]);
    const toks = shown.map((x) => (x ? tokChip(x[0][0], x[0][1], x[1] < putBack) : `<button type="button" class="tr-more" data-whole="1">… ${num(queued.length - STREAM.head - STREAM.tail)} tokens</button>`)).join('')
      + (putBack > queued.length && queued.length ? `<span class="tr-more muted">… ${num(putBack - queued.length)} more put back</span>` : '');
    // the categories in this stream; all of them behind the toggle
    const seen = new Set([...(r.tokens ?? []).map(([, c]) => c), ...(put ? [16] : [])]);
    const legend = [...Array(17).keys()].filter((c) => trace.legend || seen.has(c)).map((c) => c === 16
      ? '<span class="tr-key"><b class="k16">control sequence</b></span>'
      : `<a class="tr-key" href="${base}wiki/domains.html#catcode-${c}" target="_blank"><b class="k${c}">${escape(catTitle(c))}</b></a>`).join('');
    box.innerHTML = `<div class="tr-stream-head"><b>input after this step</b><button type="button" class="ghost small tr-legend-btn" aria-expanded="${!!trace.legend}">${trace.legend ? 'fewer catcodes' : 'all catcodes'}</button></div>` +
      `<div class="tr-chips">${put}${toks}<span class="tr-rest">${escape(clip(r.tokens?.length ? fileRest(r) : String(r.ahead ?? '')))}</span></div>` +
      `<div class="tr-legend">${legend}</div>`;
    if (!cats.size) loadCatNames().then((m) => { if (m.size && !cats.size) { cats = m; if (trace?.cur === i) traceStream(i); } });
  }
  const by = (i, d) => trace.order[Math.max(0, Math.min(trace.order.length - 1, trace.order.indexOf(i) + d))];
  /** The first step after `i` and all it called: over a call. */
  function over(i) {
    let k = trace.order.indexOf(i) + 1;
    while (k < trace.order.length && trace.steps[trace.order[k]].depth > trace.steps[i].depth) k++;
    return trace.order[Math.min(k, trace.order.length - 1)];
  }
  const moves = { prev: (i) => by(i, -1), next: (i) => by(i, 1), over, out: (i) => (trace.vparent[i] >= 0 ? trace.vparent[i] : i) };
  const move = (m) => stepTo(moves[m](trace.cur < 0 ? trace.order[0] : trace.cur));
  function traceTree(name) {
    const top = trace.kids.get(-1) ?? [];
    const mine = top.filter((i) => typeof i === 'number' && stepName(trace.steps[i].name) === name);
    const rest = mine.length ? top.filter((i) => !mine.includes(i) && typeof i === 'number') : [];
    const html = (mine.length ? top.filter((i) => !rest.includes(i)) : top).map(stepNode).join('') +
      (rest.length ? `<li><details class="tr-node"><summary><span class="muted">${plural(rest.length, 'other step')} on this line</span></summary><ul>${rest.map(stepNode).join('')}</ul></details></li>` : '');
    const now = '<div class="tr-now" id="trace-now" aria-live="polite"></div>';
    const bar = '<div class="tr-bar" role="toolbar" aria-label="step through the trace">' +
      [['prev', '◀', 'previous step (←)'], ['next', '▶', 'next step (→)'], ['over', '⤼', 'over this call (⇧→)'], ['out', '⤴', 'out to the caller (↑)']]
        .map(([m, g, t]) => `<button type="button" class="ghost small" data-move="${m}" title="${t}" aria-label="${t}">${g}</button>`).join('') +
      '<span id="trace-step" class="muted"></span></div>';
    return `${bar}${now}<details class="tr-all" open><summary>All steps</summary><ul class="tr-tree">${html}</ul></details>`;
  }
  // ---- the card of a name in the trace: its meaning at the step, then what Explain says
  const trTip = tips.card.el;
  const atStep = new Map();
  /** One query of the run stopped at record `r`'s event, cached. */
  function stopped(r, queries) {
    const k = `${r.index}|${JSON.stringify(queries)}`;
    if (!atStep.has(k)) {
      const request = { file: path(state.root ?? state.file), config: { ...traceFor.config, trace_stop: String(r.index) }, queries };
      atStep.set(k, lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).then((rs) => rs?.[0]?.records ?? []).catch(() => []));
    }
    return atStep.get(k);
  }
  async function tipFor(ref) {
    const name = ref.dataset.cs, si = +(ref.closest('[data-step]')?.dataset.step ?? trace?.cur ?? -1), r = trace?.steps[si];
    const label = r ? `at step ${trace.order.includes(si) ? trace.order.indexOf(si) + 1 : `event ${r.index}`}` : '';
    let head = '', rec = null;
    if (r?.meanings?.[name] != null) head = `<p class="tr-tip-mean"><b>meaning ${label}</b> <code>${csText(r.meanings[name])}</code></p>`;
    else if (r) {
      rec = (await stopped(r, [{ type: 'explain', names: [name] }])).find((x) => x.name === name);
      head = `<p class="tr-tip-mean"><b>meaning ${label}</b> <code>${rec ? csText(rec.tag === 'macro' && rec.body != null ? `macro:${rec.parameters ?? ''}->${rec.body}` : rec.body ?? rec.effective ?? rec.tag ?? '') : 'undefined'}</code></p>`;
    }
    rec ??= (await explainRecords(name)).find((x) => x.name === name);
    return head + (rec ? cards.explain(rec) : '');
  }
  async function showTip(ref, pin = false) {
    const html = await tipFor(ref);
    if (!pin && (trTip.dataset.pin || !ref.matches(':hover'))) return;
    tips.card.show(ref, (pin ? '<p class="tr-tip-pin muted">pinned · click outside to close</p>' : '') + (html || `<p class="muted">satex knows no ${escape(ref.dataset.cs)} here.</p>`));
  }
  traceBox.addEventListener('mouseover', (e) => { const ref = e.target.closest?.('.tr-cs'); if (ref && !trTip.dataset.pin) showTip(ref); });
  traceBox.addEventListener('mouseout', (e) => { if (e.target.closest?.('.tr-cs') && !trTip.dataset.pin) tips.card.hide(150); });
  traceBody.addEventListener('toggle', (e) => {
    const li = e.target.parentElement;
    if (li?.dataset.i && e.target.open) fill(li);
    if (e.target.classList?.contains('tr-state') && trace) trace.stateOpen = e.target.open;
  }, true);
  traceBody.addEventListener('mouseover', (e) => {
    traceBody.querySelectorAll('.tr-mate').forEach((x) => x.classList.remove('tr-mate'));
    const li = e.target.closest?.('li[data-i]'), j = li && trace?.pair.get(+li.dataset.i);
    if (j != null) rowOf(j)?.classList.add('tr-mate');
  });
  traceBody.addEventListener('input', (e) => {
    if (!e.target.classList.contains('tr-filter') || !trace) return;
    const q = (trace.filter = e.target.value).toLowerCase();
    for (const li of e.target.parentElement.querySelectorAll('li[data-n]')) li.hidden = !li.dataset.n.toLowerCase().includes(q);
  });
  traceBody.addEventListener('click', async (e) => {
    if (!trace) return;
    const m = e.target.closest('[data-move]'), go = e.target.closest('[data-go]'), row = e.target.closest('li[data-i]'), cs = e.target.closest('.tr-cs');
    if (e.target.closest('.tr-legend-btn')) { trace.legend = !trace.legend; return traceStream(trace.cur); }
    if (e.target.closest('[data-whole]')) { trace.wholeStream = true; return traceStream(trace.cur); }
    if (e.target.closest('.tr-allnames')) {
      const box = e.target.closest('.tr-state'), r = trace.steps[+box.dataset.step], list = box.querySelector('.tr-every');
      e.target.disabled = true; list.innerHTML = '<li class="muted">running to this step…</li>';
      const all = await stopped(r, [{ type: 'scope' }]), q = (trace.filter ?? '').toLowerCase();
      list.innerHTML = all.map((x) => `<li data-n="${escape(x.name)}"${x.name.toLowerCase().includes(q) ? '' : ' hidden'}>${csCode(x.name)} <code class="tr-mean">${escape(x.tag ?? '')}</code></li>`).join('') || '<li class="muted">No names.</li>';
      e.target.textContent = `${plural(all.length, 'name')} at this step`;
      return;
    }
    if (m) return move(m.dataset.move);
    if (go) { e.preventDefault(); return stepTo(+go.dataset.go); }
    if (cs) { e.preventDefault(); delete trTip.dataset.pin; await showTip(cs, true); trTip.dataset.pin = '1'; return; }
    if (row && !e.target.closest('.tr-at, summary.tr-group, .tr-group, input')) stepTo(keyOf(row.dataset.i));
  });
  traceBox.addEventListener('click', (e) => { if (trTip.dataset.pin && !e.target.closest('.tip-card, .tr-cs')) tips.card.hide(); }, true);
  traceBox.addEventListener('close', () => tips.card.hide());
  traceBox.addEventListener('keydown', (e) => {
    const m = trace && !e.target.closest?.('input, textarea') && { ArrowLeft: 'prev', ArrowRight: e.shiftKey ? 'over' : 'next', ArrowUp: 'out' }[e.key];
    if (!m) return;
    e.preventDefault();
    move(m);
  });
  async function traceCall(name, site = null) {
    traceFor = { name, site: site ?? callSite(name), config: null, records: null, error: null, fork: null };
    $('trace-title').innerHTML = `Trace <code class="cs-ref" data-cs="${escape(name)}" tabindex="0">${escape(name)}</code>`;
    if (!traceBox.open) traceBox.showModal();
    const at = traceFor.site, mine = ++traceRun;
    $('trace-total').textContent = '';
    if (!at) { $('trace-note').textContent = ''; traceBody.innerHTML = `<p class="muted">${escape(name)} is not used in the project, so there is no call to trace.</p>`; return; }
    $('trace-note').textContent = `The call at ${at.file}:${at.line}:${at.col}, as the run expanded it.`;
    const t0 = performance.now();
    traceBody.innerHTML = `<div class="tr-live"><p class="muted" id="trace-live">Tracing: starting the run…</p>${self.satexProgress.html(null, 'tracing')}</div>`;
    let last = {};
    const draw = () => {
      const live = $('trace-live'), d = last;
      if (!live) return;
      const bits = [d.phase || d.text || 'starting the run', d.file && `${d.file.split('/').pop()}${d.line ? `:${d.line}` : ''}`, d.lines && `${num(d.lines)} lines read`, d.files && plural(d.files, 'file'), d.paths > 1 && `${d.paths} paths`, secs(performance.now() - t0)].filter(Boolean);
      live.textContent = `Tracing: ${bits.join(' · ')}`;
      self.satexProgress.set(traceBody.querySelector('.sx-progress'), d.total ? Math.min((d.reached ?? 0) / d.total, 1) : null);
    };
    const tick = setInterval(draw, 250);
    traceLive = (d) => { if (d.text || d.phase) last = d; draw(); };
    sync();
    const config = traceFor.config = { trace: 'true', trace_document_only: 'false', trace_lines: `[${at.line}, ${at.line}]`, trace_col: String(at.col), trace_file: at.file };
    const request = { file: path(state.root ?? state.file), config, queries: [{ type: 'trace' }] };
    const rs = (await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).catch((e) => ({ error: String(e?.message ?? e) })))?.[0];
    clearInterval(tick);
    traceLive = null;
    if (mine !== traceRun) return;
    traceFor.records = rs?.records ?? [];
    traceFor.error = rs?.error;
    await traceShow(at, name, mine);
  }
  /** The arms of a fork path, "1.0.2". */
  const armsOf = (f) => String(f ?? '').split('.').filter(Boolean);
  const startsWith = (a, b) => b.every((x, k) => a[k] === x);
  /** The paths that end, not continue, another. */
  const forkLeaves = (records) => {
    const all = [...new Set(records.map((r) => r.fork).filter(Boolean))];
    return all.filter((f) => !all.some((g) => g !== f && startsWith(armsOf(g), armsOf(f))));
  };
  /** Each fork's whole path: a later fork replays from an earlier step, unmarked until its own choice. */
  const forkKey = (r) => `${r.name}|${r.file}|${r.line}|${r.col}|${r.depth}`;
  function forkPaths(records) {
    const key = forkKey, paths = [[]];
    records.forEach((r, i) => {
      let cur = paths.at(-1), q;
      if (i && !r.fork && records[i - 1].fork && (q = cur.findLastIndex((x) => !x.fork && key(x) === key(r))) >= 0) paths.push((cur = cur.slice(0, q)));
      cur.push(r);
    });
    return paths;
  }
  /** One fork's path, or the joined view: the steps all paths share, then those after the last choice. */
  /** A brace group's steps under its `{`, a conditional's under its branch up to the `\fi`: one level deeper each. */
  function nestBlocks(steps) {
    const open = [];
    return steps.map((r) => {
      while (open.length && r.depth < open.at(-1)) open.pop();
      if ((r.step === 'close-group' || (r.step === 'execute' && r.name === '\\fi')) && open.at(-1) === r.depth) open.pop();
      const nested = open.length ? { ...r, depth: r.depth + open.length } : r;
      if (r.step === 'open-group' || r.step === 'branch') open.push(r.depth);
      return nested;
    });
  }
  function forkView(records, fork) {
    if (forkLeaves(records).length < 2) return records;
    const paths = forkPaths(records), related = (r) => r.fork && (startsWith(armsOf(r.fork), armsOf(fork)) || startsWith(armsOf(fork), armsOf(r.fork)));
    if (fork) return paths.find((p) => p.some(related)) ?? records;
    let n = 0;
    while (paths.every((p) => p[n] && !p[n].fork && forkKey(p[n]) === forkKey(paths[0][n]))) n++;
    const last = paths.at(-1);
    let k = last.length;
    while (k > n && !last[k - 1].fork) k--;
    return [...paths[0].slice(0, n), ...last.slice(k)];
  }
  /** The trace's tree from the stored records: the library switch asks nothing new. */
  async function traceShow(at, name, mine) {
    const own = (r) => state.files.some((n) => String(r.path ?? r.file ?? '').endsWith(n));
    for (const r of traceFor.records) for (const [k, v] of Object.entries(r.aliases ?? {})) regAlias.set(k, v);
    // one row per statement: `\def` executed and `\tleapy` defined are one step, the define's
    const steps = forkView(traceFor.records, traceFor.fork).filter((r, i, all) => !(r.step === 'execute' && ['define', 'assign'].includes(all[i + 1]?.step)
      && all[i + 1].line === r.line && all[i + 1].path === r.path && all[i + 1].depth === r.depth));
    trace = traceModel(nestBlocks(steps), own, traceInternal.checked);
    if (!trace.order.length && !trace.groups.length) {
      traceBody.innerHTML = traceHead(at, name) + `<p class="muted">${traceFor.error ? `The query failed: ${escape(traceFor.error)}.` : 'The run recorded no step for this call.'}</p>`;
      traceOutput(at, mine);
      return;
    }
    // what the project's own calls do, filled in once known: the tree does not wait for it
    const names = [...new Set(trace.order.filter((i) => !trace.gof.length || trace.gof[i] < 0).slice(0, 2000).map((i) => trace.steps[i].name).filter((n) => /^\\/.test(n)))].slice(0, 50);
    explainMany(names).catch(() => []).then((all) => {
      if (mine !== traceRun) return;
      all.forEach((rs, k) => { if (rs?.[0]?.does) trace.does.set(names[k], rs[0].does); });
      for (const li of traceBody.querySelectorAll('li[data-i]')) {
        const r = trace.steps[+li.dataset.i], row = rowOf(li.dataset.i), d = r && trace.does.get(r.name);
        if (d && row && !row.querySelector(':scope > .tr-does')) row.insertAdjacentHTML('beforeend', `<span class="tr-does">${escape(d)}</span>`);
      }
    });
    traceBody.innerHTML = traceHead(at, name) + traceTree(name);
    const first = traceBody.querySelector('.tr-tree > li[data-i]:not([data-i^="g"])');
    if (first) stepTo(+first.dataset.i);
    else if (trace.order.length) stepTo(trace.order[0]);
    first?.querySelector(':scope > details')?.setAttribute('open', '');
    const lib = trace.groups.reduce((n, g) => n + g.size, 0);
    $('trace-total').textContent = `${plural(trace.order.length, 'step')}${traceInternal.checked || !lib ? '' : `, ${plural(lib, 'library step')} folded`}`;
    traceOutput(at, mine);
  }
  /** The call's source line, the call marked, and a place for the text set up to it. */
  function traceHead(at, name) {
    const text = (state.texts[Object.keys(state.texts).find((n) => at.file.endsWith(n) || n.endsWith(at.file))] ?? '').split('\n');
    const lo = Math.max(1, at.line - 1), hi = Math.min(text.length, at.line + 1);
    const lines = text.slice(lo - 1, hi).map((l, i) => {
      const n = lo + i, mark = n === at.line;
      const body = mark ? `${escape(l.slice(0, at.col - 1))}<mark class="tr-call">${escape(l.slice(at.col - 1, at.col - 1 + name.length))}</mark>${escape(l.slice(at.col - 1 + name.length))}` : escape(l);
      return `<span class="tr-line${mark ? ' on' : ''}"><i>${n}</i>${body || ' '}</span>`;
    });
    const leaves = forkLeaves(traceFor.records ?? []);
    const forks = leaves.length > 1 ? `<label class="tr-fork">paths <select id="trace-fork"><option value="">joined (steps common to all forks, then the join)</option>${leaves.map((f) => `<option value="${f}"${f === traceFor.fork ? ' selected' : ''}>fork ${escape(f)}</option>`).join('')}</select></label>` : '';
    return `<div class="tr-src"><pre><code>${lines.join('')}</code></pre>${forks}<div id="trace-stream" class="tr-before"></div><div id="trace-before" class="tr-before muted"></div></div>`;
  }
  /** The text set before the call, from the typeset rows. */
  async function traceOutput(at, mine) {
    const rows = await lsp('satex/typeset', { textDocument: { uri: uri(state.root ?? state.file) } }).catch(() => null);
    const box = $('trace-before');
    if (!box || mine !== traceRun) return;
    const before = [];
    for (const r of (rows ?? []).filter((r) => r.kind === 'paragraph')) {
      if (String(r.path ?? "").endsWith(at.file) && (r.line > at.line || (r.line === at.line && (r.col ?? 1) >= at.col))) break;
      before.push((r.marked ?? r.detail ?? '').replace(/\uE000[^\uE001]*\uE001/g, ''));
    }
    box.innerHTML = before.length ? `<b>set before</b> <span>${escape(before.slice(-3).join(' ¶ ').slice(-400))}</span>` : '<b>set before</b> <span>nothing yet</span>';
  }
  traceBody.addEventListener('change', (e) => { if (e.target.id === 'trace-fork') { traceFor.fork = e.target.value; traceShow(traceFor.site, traceFor.name, ++traceRun); } });
  traceInternal.onchange = () => { if (traceFor?.records) traceShow(traceFor.site, traceFor.name, ++traceRun); };
  $('trace-close').onclick = () => traceBox.close();
  traceBox.onclick = (e) => {
    if (e.target === traceBox) return traceBox.close();
    const a = e.target.closest('.tr-at');
    if (!a) return;
    e.preventDefault();
    const name = Object.keys(state.texts).find((n) => !(n in state.ro) && a.dataset.path.endsWith(n));
    if (!name) return;
    traceBox.close();
    openPlace(name, { line: +a.dataset.line - 1, character: Math.max(+a.dataset.col - 1, 0) });
  };
  document.addEventListener('click', (e) => { const b = e.target.closest?.('.x-trace'); if (b) { tips.card.hide(); if (b.dataset.try != null) tryIt(b); else traceCall(b.dataset.trace); } });

  function focusExplain() {
    explainBox.open = true;
    explained = '';
    explainAt(editor.position());
    explainBox.scrollIntoView({ block: 'nearest' });
  }

  // ---------------------------------------------------------------- runs

  /** The kernel's flags; a project in its own directory under /work reads the playground's satex.yaml by name. */
  const flags = () => [...manifest.kernels[state.kernel].args, ...(state.dir !== '/work' ? ['--config', '/work/satex.yaml'] : []), ...configFlags()];
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
    const dirs = rows.filter((r) => r.kind === 'directory' && r.count != null);
    if (dirs.length) rows = rows.filter((r) => !dirs.includes(r));
    const shapes = new Map();
    for (const r of rows) {
      const k = Object.keys(r).filter((c) => !HIDDEN_COLS.has(c)).join('\0');
      shapes.set(k, [...(shapes.get(k) ?? []), r]);
    }
    const list = dirs.length ? `<div class="card"><p>${plural(dirs.length, 'directory')}</p><ul class="cache-dirs">${dirs.map((r) => `<li title="${escape(r.root)}/${escape(r.name)}"><span>${escape(r.name || '.')}</span><b>${r.count}</b></li>`).join('')}</ul></div>` : '';
    return list + [...shapes.values()].map((group) => {
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
  const edits = () => Object.fromEntries(Object.entries(state.texts).filter(([name]) => state.files.includes(name) && !(name in state.ro)).map(([name, text]) => [path(name), text]));
  async function start() {
    sync();
    const a = argv();
    button.disabled = true;
    status.textContent = 'running…';
    delete status.dataset.done;
    cmd.textContent = `satex ${words(args.value).filter((x) => x !== 'satex').join(' ')}`;
    try {
      const data = await runCommand({ type: 'run', args: a, edits: edits(), cwd: state.dir });
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
      menu.dataset.moved = '1';
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
    if (opened.has(uri(name)) || name in state.ro || (data(name) && !database(name))) return;
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
  const store = (names = state.files, removed = []) => post({ type: 'project', dir: state.dir, removed,
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
  /** How long the language server may take to answer its first message. */
  const START_MS = 90000;
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
    busy('Starting', 'Starting the language server…');
    try { localStorage.setItem('satex-start', JSON.stringify(configSet)); } catch {}
    // a server that does not answer, or stops, never leaves the page starting: it says so and offers another try
    const started = await Promise.race([lsp('initialize', { processId: null, rootUri: `file://${state.dir}`, capabilities: {} }),
      new Promise((done) => setTimeout(() => done(null), START_MS))]);
    if (!started) {
      following = false;
      idle();
      status.innerHTML = `The language server did not start${lspStatus.textContent ? `: ${escape(lspStatus.textContent)}` : ''}. <button type="button" class="small ghost" id="play-retry">Try again</button>` +
        (Object.keys(configSet).length ? ` <button type="button" class="small ghost" id="play-defaults">Start with the default settings</button>` : '');
      status.dataset.done = 'error';
      $('play-retry').onclick = () => restart();
      $('play-defaults')?.addEventListener('click', () => { configKeep({}); restart(); });
      return;
    }
    try { localStorage.removeItem('satex-start'); } catch {}
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
  const rootable = (name) => /\.(tex|ltx)$/.test(name) && /\\documentclass|\\begin\s*\{document\}|\\bye\b|^%\s*!T[eE]X\s+(?:TS-)?program\s*=/m.test(state.texts[name] ?? '');
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
    heatDraw();
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
    // each example in its own directory: its project sees its own files only
    state.dir = `/work/ex/${ex.replace(/\.[^.]+$/, '')}`;
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
  /** The open project's files that differ from how they were loaded. */
  const changedFiles = () => { sync(); return state.files.filter((n) => n in state.texts && !(n in state.ro) && (state.upload || state.texts[n] !== sampleTexts[n])); };
  /** Asks before `what` (a name, `hash` the page's address for it) takes the place of an edited project:
   * true to go on, a new tab on request. */
  function replaceOk(what, hash) {
    const changed = changedFiles();
    if (!changed.length || (!state.upload && what === state.example)) return Promise.resolve(true);
    const box = $('play-replace');
    $('replace-text').innerHTML = `Open ${escape(what)} in place of ${state.upload ? 'your project' : escape(state.example)}? ` +
      `Your edits to ${changed.map((n) => `<code>${escape(n)}</code>`).join(', ')} are ${state.upload ? 'lost' : 'kept until you leave the page'}. Copy a link first to keep them.`;
    box.returnValue = '';
    box.showModal();
    return new Promise((done) => box.addEventListener('close', () => {
      if (box.returnValue === 'tab') window.open(`${location.pathname}#${hash}`, '_blank', 'noopener');
      done(box.returnValue === 'replace');
    }, { once: true }));
  }
  const exampleAsked = async (ex) => { if (await replaceOk(ex, encodeURIComponent(ex))) await example(ex); };
  examplesBox.onclick = (e) => { const row = e.target.closest('[data-example]'); if (row) exampleAsked(row.dataset.example); };
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
    else if (h.startsWith('s=')) linkAsked(h);
    else if (h in sampleTexts && !shared.has(h) && h !== state.example) exampleAsked(h);
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
    await loadKernel(i);
    state.kernel = i;
    kernelBox.value = i;
  }
  /** Fetches and sets up kernel `i`, once, without selecting it. */
  const loadingKernels = new Map();
  function loadKernel(i) {
    if (!loadingKernels.has(i)) loadingKernels.set(i, fetchKernel(i).catch((e) => { loadingKernels.delete(i); throw e; }));
    return loadingKernels.get(i);
  }
  async function fetchKernel(i) {
    const entry = manifest.kernels[i];
    if (!state.loaded.has(i)) {
      busy('Fetching', `Fetching the ${entry.label} kernel (${size(entry.size)})…`);
      const t = performance.now();
      const given = location.protocol === 'file:' ? await local(entry.bundle) : undefined;
      await ask({ type: 'bundle', base, entry, given }, given ? [given.buffer] : []);
      state.loaded.add(i);
      timing('t-init', `${$('t-init').textContent} · ${entry.label} ${secs(performance.now() - t)}`, 'time to fetch and set up each kernel');
      loads.clear();
      idle(`${entry.label} ready`);
    }
  }

  /** Packages beyond the first download: a bundle each per kernel (the files and cache entries a
   * document loading it adds), with the bundles it needs. Fetched before a project's first run for
   * what its files name, and again whenever a finding or a run says one is not in this installation. */
  const PACKAGES = new Map((manifest.packages ?? []).map((p) => [p.id, p]));
  const fetchedPackages = new Map(); // id -> its load
  // kernel/file name -> the bundle holding it (`pgfmath.sty` comes with tikz)
  const HOLDER = new Map((manifest.packages ?? []).flatMap((p) => (p.files ?? []).map(([f]) => [`${p.kernel}/${f.split('/').pop()}`, p.id])));
  const MISSING = /(?:`([^`\s]+)`|(\S+\.(?:sty|cls))) is not in this installation/g;
  const NAMED = /\\(?:documentclass|usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}/g;
  /** The bundles of the current kernel for `names` (a class too), and what those need, not yet asked for. */
  function wanted_(names, k = state.kernel) {
    const kernel = manifest.kernels[k].id;
    const todo = new Set();
    const add = (id) => {
      const entry = PACKAGES.get(id);
      if (!entry || todo.has(id)) return;
      todo.add(id);
      entry.needs.forEach((n) => add(`${kernel}/${n}`));
    };
    for (const n of names) {
      const held = HOLDER.get(`${kernel}/${n}`) ?? HOLDER.get(`${kernel}/${n}.sty`) ?? HOLDER.get(`${kernel}/${n}.cls`);
      add(`${kernel}/${n.replace(/\.sty$/, '')}`); add(`${kernel}/${n}`); add(`${kernel}/${n}.cls`); if (held) add(held);
    }
    return [...todo].filter((id) => !fetchedPackages.has(id));
  }
  /** The packages and classes the project's own files name. */
  const named = (texts) => [...new Set(texts.flatMap((t) => [...String(t).matchAll(NAMED)].flatMap((m) => m[1].split(',').map((x) => x.trim()).filter(Boolean))))];
  const missing = (texts) => wanted_([...new Set(texts.flatMap((t) => [...String(t).matchAll(MISSING)].map((m) => m[1] ?? m[2])))]);
  const loadedList = [], inFlight = new Set();
  let loadedBytes = 0;
  async function fetchPackages(ids) {
    if (!ids.length) return false;
    const entries = ids.map((id) => PACKAGES.get(id));
    const label = entries.map((e) => e.label).join(', ');
    busy('Fetching', `Fetching ${plural(entries.length, 'package')}, ${size(entries.reduce((n, e) => n + e.size, 0))}…`);
    status.title = entries.map((e) => `${e.label} ${size(e.size)}`).join(', ');
    const t = performance.now();
    const given = location.protocol === 'file:' ? await Promise.all(entries.map((e) => local(e.bundle))) : undefined;
    const load = ask({ type: 'bundles', base, entries, given }, given ? given.map((g) => g.buffer) : []);
    ids.forEach((id) => { fetchedPackages.set(id, load); inFlight.add(id); });
    try { await load; } catch (e) { ids.forEach((id) => fetchedPackages.delete(id)); toast(/: 404$/m.test(e.message) ? 'The playground was updated since this page loaded: reload it to fetch packages.' : `${label} did not load: ${e.message.split('\n')[0]}`); return false; } finally { ids.forEach((id) => inFlight.delete(id)); }
    // the file index cached before this bundle was in the file system does not list its files: drop it, so the next start rebuilds it
    try { const stale = ((await ask({ type: 'caches' })).entries ?? []).filter((e) => /\/index-/.test(e.path)).map((e) => e.path); if (stale.length) await ask({ type: 'uncache', paths: stale }); } catch {}
    loads.clear();
    idle();
    status.textContent = `${plural(entries.length, 'package')} fetched in ${secs(performance.now() - t)}`;
    status.title = label;
    status.dataset.packages = [...fetchedPackages.keys()].join(' ');
    loadedList.push(...entries.map((e) => `${e.label} ${size(e.size)}`));
    loadedBytes += entries.reduce((n, e) => n + e.size, 0);
    timing('t-load', `${plural(loadedList.length, 'bundle')}, ${size(loadedBytes)}`, 'packages fetched after the first download', loadedList);
    return true;
  }
  /** The files the build wrote: alternatives per path, unknown text as chips. */
  const MARKS = { '\u{10FFFF}': 'char', '\u{10FFFE}': 'digit', '\u{10FFFB}': 'digit', '\u{10FFFA}': 'any', '\u{10FFFD}': 'letter', '\u{10FFFC}': 'other' };
  const marked = (text) => escape(text).replace(/[\u{10FFF9}-\u{10FFFF}]+/gu, (run) => {
    const kinds = [...run].map((c) => MARKS[c] ?? 'char');
    return `<span class="unk" title="unknown: ${kinds.length} ${kinds[0]}${kinds.length > 1 ? 's' : ''}">${kinds.length > 1 ? `${kinds[0]}×${kinds.length}` : kinds[0]}</span>`;
  });
  /** The lines of `b` against those of `a` (an LCS): [kind, line], kind '+' added, '-' removed, ' ' kept. */
  function lineDiff(a, b) {
    const x = a.split('\n'), y = b.split('\n'), m = x.length, n = y.length;
    const L = Array.from({ length: m + 1 }, () => new Uint16Array(n + 1));
    for (let i = m - 1; i >= 0; i--) for (let j = n - 1; j >= 0; j--) L[i][j] = x[i] === y[j] ? L[i + 1][j + 1] + 1 : Math.max(L[i + 1][j], L[i][j + 1]);
    const out = [];
    let i = 0, j = 0;
    while (i < m || j < n) {
      if (i < m && j < n && x[i] === y[j]) { out.push([' ', y[j]]); i++; j++; } else if (i < m && (j >= n || L[i + 1][j] >= L[i][j + 1])) out.push(['-', x[i++]]); else out.push(['+', y[j++]]);
    }
    return out;
  }
  /** The written files (`satex/written`): per file its runs, the final text, each against the run before; alternatives as tabs. */
  let writtenFiles = [];
  const writtenPick = new Map(); // name -> [view, alternative]: view is a run's index, or 'final'
  function writtenCard({ name, texts, runs = [] }) {
    // the build's fixpoint is its last run's text: no tab of its own then
    const same = (x, y) => JSON.stringify(x) === JSON.stringify(y), last = (runs ?? []).length - 1;
    const views = (runs ?? []).map((r, i) => [i === last && same(r, texts) ? 'final' : i, `run ${i + 1}`, r]);
    if (views.at(-1)?.[0] !== 'final') views.push(['final', 'final', texts]);
    const [view, alt] = writtenPick.get(name) ?? ['final', 0];
    const k = Math.max(0, views.findIndex(([v]) => v === view)), shown = views[k][2], before = k ? views[k - 1][2] : null;
    const a = Math.min(alt, (shown?.length ?? 1) - 1), text = shown?.[a], prev = before?.[Math.min(a, before.length - 1)];
    const lines = text == null ? 0 : text.split('\n').filter(Boolean).length, short = name.split('/').pop();
    const body = text == null ? '<p class="muted">Not known: the build wrote text satex could not follow here.</p>'
      : `<pre>${(prev == null ? text.split('\n').map((l) => [' ', l]) : lineDiff(prev, text)).filter(([, l], i, all) => l || i < all.length - 1)
        .map(([d, l]) => `<span class="wf-l${d === '+' ? ' wf-add' : d === '-' ? ' wf-del' : ''}">${commandRefs(marked(l)) || ' '}</span>`).join('')}</pre>`;
    const changed = prev != null && text != null && prev !== text;
    const ext = short.split('.').pop(), ty = (manifest.completion?.types ?? []).find((t) => t.name.endsWith(`(.${ext})`));
    const kind = ty ? `<a class="wf-kind ty" data-ty="${escape(ty.name)}" href="${base}wiki/domains.html#files-written-in-the-run" target="_blank" rel="noopener">${escape(ext)}</a>`
      : `<a class="wf-kind" href="${base}wiki/domains.html#files-written-in-the-run" target="_blank" rel="noopener" title="a file the build writes and reads back: how satex follows it">${escape(ext)}</a>`;
    return `<div class="wf" data-name="${escape(name)}"><div class="wf-head"><code>${escape(short)}</code>${kind}` +
      `<span class="muted">${text == null ? 'unknown' : plural(lines, 'line')}${k ? (changed ? `, changed since ${views[k - 1][1]}` : `, as in ${views[k - 1][1]}`) : ''}</span>` +
      `<span class="wf-runs" role="tablist">${views.length > 1 ? views.map(([v, label, t], i) => `<button type="button" role="tab" data-view="${v}" aria-selected="${i === k}"${i !== k && same(t, shown) ? ' class="wf-same" title="the same text as the tab shown"' : ''}>${label}</button>`).join('') : ''}</span></div>` +
      `${shown && shown.length > 1 ? `<div class="wf-alts" role="tablist">${shown.map((_, i) => `<button type="button" role="tab" data-alt="${i}" aria-selected="${i === a}">path ${i + 1}</button>`).join('')}</div>` : ''}${body}</div>`;
  }
  const writtenShow = () => {
    $('written-count').textContent = String(writtenFiles.length || '');
    $('written-body').innerHTML = writtenFiles.length ? '<p class="muted wf-note">Each file is empty when the build starts: the first run reads none of them, the next run reads what this one wrote.</p>' + writtenFiles.map(writtenCard).join('') : '<p class="muted">The build writes no file.</p>';
  };
  $('written-body').addEventListener('click', (e) => {
    const b = e.target.closest('[data-view], [data-alt]'), name = b?.closest('.wf')?.dataset.name;
    if (!name) return;
    const [view, alt] = writtenPick.get(name) ?? ['final', 0];
    writtenPick.set(name, b.dataset.view !== undefined ? [b.dataset.view === 'final' ? 'final' : +b.dataset.view, alt] : [view, +b.dataset.alt]);
    writtenShow();
  });
  async function written(target) {
    const files = await lsp('satex/written', { textDocument: { uri: target } }).catch(() => null);
    if (!files) return;
    writtenFiles = files;
    writtenShow();
  }
  /** What a run's cache report says: the packages and classes served from the caches, the ones analyzed anew. */
  function cacheReport(lines) {
    const reused = [], fresh = [];
    let segments = 0;
    const texts = lines.map(([, t]) => plain(t));
    const run = texts.find((t) => t.startsWith('run '));
    // the language server's report has the run's size; a console run's output does not, and keeps it
    if (run) {
      $('t-run').innerHTML = run.slice(4).split(', ').map((part) => {
        const [, n, label] = part.match(/^(\S+) (.*)$/) ?? [, part, ''];
        return `<span title="${escape(num(n))} ${escape(label)}${RUN_MEANS[label.trim()] ? `: ${RUN_MEANS[label.trim()]}` : ''}"><b>${escape(Number(n) >= 1e4 ? short(n) : num(n))}</b>${escape(label)}</span>`;
      }).join('') + ($('t-time-text').textContent ? `<span title="the whole check"><b>${escape($('t-time-text').textContent)}</b>total</span>` : '');
      const slow = texts.map((t) => t.match(/^slow (\d+) (\d+) (.*)$/)).filter(Boolean);
      $('t-slow').innerHTML = rpBars(slow.map(([, ms, pct, name]) => [name, +ms, pct, ` title="${escape(name)}"`]), 'slowest files',
        ([name]) => escape(name.split('/').pop()), ([, ms, pct]) => `${secs(ms)} · ${pct}%`);
      $('t-limits').innerHTML = texts.filter((t) => t.startsWith('limit ')).map((t) => `<p>reached ${escape(t.slice(6))}</p>`).join('');
      heatShow(texts);
    }
    for (const [, text] of lines) {
      const m = plain(text).match(/cache (?:kernel|class|package) (\S+): (reused|rebuilt \((.*)\))/);
      if (m) (m[2] === 'reused' ? reused : fresh).push(m[3] ? `${m[1]} (${m[3]})` : m[1]);
      const s = plain(text).match(/cache package segments: (\d+) reused/);
      if (s) segments = Number(s[1]);
    }
    if (!reused.length && !fresh.length && !segments) return '';
    const detail = [`${reused.length + segments} reused`, fresh.length && `${fresh.length} analyzed anew`].filter(Boolean).join(', ');
    timing('t-cache', detail, 'what the last run reused from the caches and what it analysed again', [...reused, ...(segments ? [plural(segments, 'package segment')] : []), ...fresh.map((f) => `new: ${f}`)]);
    return `${reused.length + segments} cached${fresh.length ? ` · fresh: ${fresh.map((f) => f.split(' ')[0]).join(', ')}` : ''}`;
  }
  /** The main file's tokens per line, as a strip in the editor's gutter and its costliest lines in the
   * pop-up; each lint rule's time and findings, slowest first. */
  let heat = null;
  /** What each count of the run means: a fork splits one path in two, paths are the runs alive at once. */
  const RUN_MEANS = {
    forks: 'how often a path split in two at a conditional or value satex could not decide. Each split adds one path',
    'peak paths': 'the most alternative runs alive at the same time. Paths that agree again join, so this stays far below the forks',
    steps: 'tokens read and expanded, on all paths together',
  };
  function heatShow(texts) {
    const file = texts.find((t) => t.startsWith('heat '))?.slice(5) ?? '';
    const lines = texts.map((t) => t.match(/^line (\d+) (\d+)$/)).filter(Boolean).map(([, l, n]) => [+l, +n]);
    heat = { file, lines: new Map(lines) };
    heatDraw();
    const top = [...lines].sort((a, b) => b[1] - a[1]).map(([l, n]) => [l, n, 0, ` data-line="${l}" title="go to line ${l}"`]);
    $('t-heat').innerHTML = (top.length ? `<label class="ts-toggle t-heatgutter"><span>heat in the gutter</span><input type="checkbox" role="switch"${heatShown ? ' checked' : ''}></label>` : '') +
      rpBars(top, `busiest lines of ${escape(file.split('/').pop())}`, ([l]) => `line ${l}`, ([, n]) => `${short(n)} tokens`, false);
    // the rules that took time or found something, with their numbers
    const all = texts.map((t) => t.match(/^rule (\S+) (\d+) (\d+)$/)).filter(Boolean).map(([, code, us, n]) => [code, +us, +n]);
    const rules = all.filter(([, us, n]) => n || us >= 500);
    const enabled = PLUGINS.filter((p) => configSet[`plugins.${p.name}`] !== false).length;
    $('t-rules').innerHTML = rpBars(rules, `lint rules: ${plural(all.length, 'rule')}, ${secs(all.reduce((s, r) => s + r[1], 0) / 1000)} in all`,
      ([code]) => `${manifest.rules[code]?.number ? `<code class="rno">${escape(manifest.rules[code].number)}</code> ` : ''}${escape(code)}`, ([, us, n]) => `${secs(us / 1000)} · ${plural(n, 'finding')}`) +
      `<p class="muted rp-note">${plural(PLUGINS.length, 'plugin')}, ${enabled} on</p>`;
  }
  let heatShown = false;
  /** The pop-up's bars, `[label, value, _, attributes]` rows largest first: the first three, the rest folded.
   * A bar's share is of the largest one. */
  const RP_SHOWN = 3;
  function rpBars(rows, title, label, value, fold = true) {
    const most = Math.max(1, ...rows.map((r) => r[1]));
    const bar = (r) => `<div class="rp-bar" style="--p:${(100 * r[1] / most).toFixed(1)}%"${r[3] ?? ''}><span>${label(r)}</span><span>${value(r)}</span></div>`;
    const rest = rows.slice(RP_SHOWN);
    return rows.length ? `<h4>${title}</h4>${rows.slice(0, RP_SHOWN).map(bar).join('')}${rest.length && fold ? `<details class="rp-more"><summary>show all ${rows.length}</summary><div class="rp-scroll">${rest.map(bar).join('')}</div></details>` : ''}` : '';
  }
  const heatDraw = () => editor?.heat?.(heatShown && heat && (heat.file === state.file || heat.file.endsWith(`/${state.file}`)) ? heat.lines : null);
  $('t-heat').addEventListener('change', (e) => { if (e.target.closest('.t-heatgutter')) { heatShown = e.target.checked; heatDraw(); } });
  $('t-heat').addEventListener('click', (e) => { const b = e.target.closest('[data-line]'); if (b) { const at = { line: +b.dataset.line - 1, character: 0 }; editor.focus({ start: at, end: at }); } });
  /** The cache entries the worker keeps (worker.js), each with what it stands for; dropping one
   * restarts the language server, so the next check analyses that part again. */
  const cacheBox = $('play-cache'), cacheList = $('cache-list');
  const CACHE_KINDS = { format: ['kernel', 'kernel'], package: ['package', 'package'], index: ['file index', 'index'], probe: ['installation facts', 'facts'], 'content-hashes': ['file hashes', 'hashes'] };
  const cacheFilter = $('cache-filter');
  let cacheEntries = [];
  /** [kind, css class, name, needs] of an entry; the manifest tells a class from a package. */
  function cacheLabel(path) {
    const file = path.split('/').pop(), [kind, ...rest] = file.replace(/\.postcard$/, '').split('-');
    const name = kind === 'content' ? 'content-hashes' : kind;
    const what = rest.filter((p) => !/^(v\d+|s?[0-9a-f]{8,})$/.test(p) && p !== 'hashes').join(' ');
    let [label, cls] = CACHE_KINDS[name] ?? [name, 'hashes'], needs = [];
    // a package cache names what it holds before `-v<version>`: a class by its file name
    const held = name === 'package' && /^package-(.+?)-v\d+-/.exec(file)?.[1];
    if (held) {
      if (held.endsWith('.cls')) [label, cls] = ['class', 'class'];
      needs = (manifest.packages ?? []).find((p) => p.label === held)?.needs ?? [];
      return [label, cls, held.replace(/\.cls$/, ''), needs];
    }
    return [label, cls, what, needs];
  }
  /** The table: searched, sorted by a header, a class's or package's loads unfolded under it. */
  let cacheOrder = { key: 'size', down: true, used: true };
  const cacheBadge = (cls, text) => `<span class="cache-kind k-${cls}">${escape(text)}</span>`;
  function drawCaches(entries) {
    if (entries) { cacheEntries = entries; $('cache-count').textContent = entries.length ? short(entries.length) : ''; }
    const q = cacheFilter.value.trim().toLowerCase();
    // a row matches by its kind and name, or by a command it defined in the run (`\textcolor`)
    // a package another one loads matches inside it too (pgf in tikz), its row unfolded
    const rows = cacheEntries.map((e) => { const l = cacheLabel(e.path); return { e, l, u: usedOf(l[1], l[2]), hit: [], via: [] }; })
      .filter((r) => !q || (!q.startsWith('\\') && `${r.l[0]} ${r.l[2]}`.toLowerCase().includes(q)) || (r.hit = namesLike(r.l[1], r.l[2], q)).length
        || (!q.startsWith('\\') && (r.via = r.l[3].filter((n) => n.toLowerCase().includes(q))).length));
    const by = { size: (r) => r.e.size, name: (r) => (r.l[2] || r.l[0]).toLowerCase(), kind: (r) => r.l[0] }[cacheOrder.key];
    rows.sort((a, b) => (cacheOrder.used ? Number(b.u[0]) - Number(a.u[0]) : 0) || (by(a) < by(b) ? -1 : by(a) > by(b) ? 1 : 0) * (cacheOrder.down ? -1 : 1));
    for (const th of cacheBox.querySelectorAll('th[data-sort]')) {
      if (th.dataset.sort === cacheOrder.key) th.setAttribute('aria-sort', cacheOrder.down ? 'descending' : 'ascending'); else th.removeAttribute('aria-sort');
    }
    const hits = (verb, hit) => hit.length ? ` <span class="muted cache-hit">${verb} ${hit.slice(0, 3).map((n) => `<code>${escape(n.replace(/\.cls$/, ''))}</code>`).join(', ')}${hit.length > 3 ? ` +${hit.length - 3}` : ''}</span>` : '';
    cacheList.innerHTML = rows.map(({ e, l: [kind, cls, what, needs], u, hit, via }, i) =>
      `<tr title="${escape(e.path)}"><td>${cacheBadge(cls, kind)}</td>` +
      `<td><button type="button" class="cache-name" aria-expanded="${via.length > 0}" aria-controls="cache-sub-${i}">${escape(what || kind)}</button>${hits('defines', hit)}${hits('loads', via)}</td><td class="used">${u[1] || '<span class="use-no" title="not used by the open document">—</span>'}</td><td class="loads">${needs.length ? `<span class="cache-fold" title="it loads ${needs.length}">${needs.length}</span>` : ''}</td>` +
      `<td class="num">${size(e.size)}</td><td><button type="button" class="small ghost" data-path="${escape(e.path)}" aria-label="drop ${escape(what || kind)}">Drop</button></td></tr>` +
      `<tr class="cache-sub" id="cache-sub-${i}"${via.length ? '' : ' hidden'}><td></td><td colspan="5">${details(cls, what, needs)}</td></tr>`).join('') ||
      `<tr><td colspan="6" class="muted">${cacheEntries.length ? 'No match.' : 'Nothing cached yet.'}</td></tr>`;
    $('cache-total').textContent = rows.length ? `${rows.length} ${rows.length === 1 ? 'entry' : 'entries'}, ${size(rows.reduce((n, { e }) => n + e.size, 0))} in total` : '';
  }
  /** How the open document uses each class and package (`satex query dependencies` of the root):
   * loaded by the document itself, or by which packages. */
  let used = new Map();
  async function usage() {
    const request = { file: path(state.root), queries: [{ type: 'dependencies' }] };
    const rs = (await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).catch(() => null))?.[0]?.records ?? [];
    used = new Map();
    for (const r of rs.filter((r) => r.kind === 'package' || r.kind === 'class')) {
      const k = `${r.kind}:${r.name}`, u = used.get(k) ?? { own: false, by: new Set(), always: false };
      u.always ||= r.certain;
      // the document's own: loaded by no package, from a file of the project (a hook of pdftex.def runs at depth 0 too)
      if (state.files.includes(r.file) && !r.by) u.own = true; else u.by.add(r.by ?? r.file);
      used.set(k, u);
    }
  }
  /** One marker for every kind: [used, html] of an entry for the open document. */
  const usedOf = (cls, what) => {
    const mark = (text, title = '') => [true, `<span class="use use-yes" title="${escape(title)}">${text}</span>`];
    if (cls === 'kernel') return what.startsWith(manifest.kernels[state.kernel]?.id ?? '\0') ? mark('the run\'s format') : [false, ''];
    if (cls === 'index') return indexed ? mark('file lookups') : [false, ''];
    if (cls === 'facts' || cls === 'hashes') return mark('every run');
    const u = used.get(`${cls}:${what}`);
    if (!u) return [false, ''];
    const [yes, html] = u.own ? mark('by the document') : mark(`via ${escape([...u.by].slice(0, 2).join(', '))}${u.by.size > 2 ? ` +${u.by.size - 2}` : ''}`, `loaded by ${[...u.by].join(', ')}`);
    return [yes, u.always ? html : `${html} <span class="use-maybe" title="every load of it depends on an undecided conditional">sometimes</span>`];
  };
  /** What each package (and the kernel, `null`) defines in the open document's run, and the installation's
   * index: fetched when the dialog opens, from the root's `definitions` and `distribution` queries. */
  let defined = new Map(), indexed = null, folders = [];
  async function facts() {
    const request = { file: path(state.root), queries: [{ type: 'definitions' }, { type: 'distribution' }] };
    const [defs, dist] = (await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).catch(() => null)) ?? [];
    defined = new Map();
    for (const r of defs?.records ?? []) {
      const d = defined.get(r.package ?? null) ?? {};
      const internal = /[@_:]/.test(r.name);
      const k = r.tag === 'macro' ? (internal ? 'internal macro' : 'public command') : r.tag === 'alias' ? (internal ? 'internal alias' : 'public alias') : r.tag;
      d[k] = (d[k] ?? 0) + 1;
      (d.names ??= new Set()).add(r.name);
      defined.set(r.package ?? null, d);
    }
    indexed = (dist?.records ?? []).find((r) => r.indexed != null) ?? null;
    folders = (dist?.records ?? []).filter((r) => r.kind === 'directory');
  }
  const DEFINED = ['public command', 'internal macro', 'environment', 'option', 'register', 'counter', 'length', 'switch', 'public alias', 'internal alias'];
  /** The names a row's class or package (or the kernel) defined in the run that match `q`: containing
   * what follows a `\`, or else equal to it (`color` finds xcolor by name, not every name with `color` in it). */
  const definedOf = (cls, what) => cls === 'kernel' ? defined.get(null) : (cls === 'package' || cls === 'class') ? defined.get(what) : null;
  const namesLike = (cls, what, q) => {
    const part = q.startsWith('\\'), w = q.replace(/^\\/, '');
    return w ? [...definedOf(cls, what)?.names ?? []].filter((n) => part ? n.toLowerCase().includes(w) : n.replace(/^\\/, '').toLowerCase() === w) : [];
  };
  const plural = (n, w) => `${num(n)} ${w}${n === 1 ? '' : /(s|ch)$/.test(w) ? 'es' : 's'}`;
  /** One row's details: what it defines and loads, and where its documentation is. */
  function details(cls, what, needs) {
    const parts = [];
    if (cls === 'index' && indexed) {
      parts.push(`${indexed.indexed} files indexed${indexed.message ? ` · ${escape(indexed.message)}` : ''}`);
      if (folders.length) parts.push(`<ul class="cache-dirs">${folders.map((r) => `<li title="${escape(r.root)}/${escape(r.name)}"><span>${escape(r.name || '.')}</span><b>${r.count}</b></li>`).join('')}</ul>`);
    }
    const d = definedOf(cls, what);
    if (d) parts.push('defines ' + DEFINED.filter((k) => d[k]).map((k) => plural(d[k], k)).join(', '));
    else if (cls === 'package' || cls === 'class') parts.push('<span class="muted">not used by this document</span>');
    if (needs.length) parts.push(`loads ${needs.map((n) => cacheBadge(n.endsWith('.cls') ? 'class' : 'package', n.replace(/\.cls$/, ''))).join(' ')}`);
    const docs = cls === 'kernel' ? 'https://texdoc.org/serve/source2e/0' : (cls === 'package' || cls === 'class') ? `https://texdoc.org/pkg/${encodeURIComponent(what)}` : null;
    if (docs) parts.push(`<a href="${docs}" target="_blank" rel="noopener">documentation ↗</a>`);
    return parts.map((p) => `<div>${p}</div>`).join('');
  }
  const loadCaches = async () => { await usage(); drawCaches((await ask({ type: 'caches' })).entries); };
  cacheFilter.oninput = () => drawCaches();
  cacheBox.querySelector('thead').onclick = (e) => {
    const key = e.target.closest('th[data-sort]')?.dataset.sort;
    if (!key) return;
    cacheOrder = { key, down: cacheOrder.key === key ? !cacheOrder.down : key === 'size', used: false };
    drawCaches();
  };
  async function uncache(paths) {
    if (!paths.length) return;
    drawCaches((await ask({ type: 'uncache', paths })).entries);
    await restart();
  }
  $('cache-open').onclick = async () => { cacheBox.showModal(); await facts(); loadCaches(); };
  $('cache-close').onclick = () => cacheBox.close();
  cacheBox.onclick = (e) => { if (e.target === cacheBox) cacheBox.close(); }; // a click on the backdrop
  cacheList.onclick = (e) => {
    const fold = e.target.closest('.cache-name');
    if (fold) {
      const open = fold.getAttribute('aria-expanded') !== 'true';
      fold.setAttribute('aria-expanded', String(open));
      $(fold.getAttribute('aria-controls')).hidden = !open;
      return;
    }
    const b = e.target.closest('button[data-path]');
    if (b) uncache([b.dataset.path]);
  };
  $('cache-clear').onclick = async () => uncache((await ask({ type: 'caches' })).entries.map((e) => e.path));

  /** What the project's files ask for, fetched in the background as soon as they name it: the
   * kernel the root's text points to, then its bundles of the packages and classes named, so a
   * check or a switch of kernel finds them loaded. */
  let prefetched = Promise.resolve();
  function prefetch() {
    const k = manualKernel ? state.kernel : wanted();
    const texts = Object.entries(state.texts).filter(([n]) => !(n in state.ro)).map(([, t]) => t);
    prefetched = prefetched.then(async () => {
      if (k !== state.kernel) await loadKernel(k);
      const ids = wanted_(named(texts), k).filter((id) => !fetchedPackages.has(id));
      // the check may have run before the files were there: start over, with the file index rebuilt
      if (ids.length && await fetchPackages(ids)) { await restart(); if (!plainLint()) await start(); }
    }).catch(() => {});
  }

  /** The colors of the open file as swatches; none when the server offers none. */
  async function colorize() {
    const found = await lsp('textDocument/documentColor', { textDocument: { uri: uri() } }).catch(() => null);
    const css = ({ red, green, blue, alpha }) => `rgba(${[red, green, blue].map((x) => Math.round(x * 255)).join(',')},${alpha})`;
    editor?.colors?.((found ?? []).map((c) => [c.range.start, css(c.color)]));
  }
  /** Files the run looked for and did not find (`\IfFileExists`, listings' language files) that a
   * bundle holds: fetched, then the check starts over. A probe of a file no bundle has stays as it is. */
  async function fetchAbsent() {
    const request = { file: path(state.root ?? state.file), queries: [{ type: 'files' }] };
    const records = (await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).catch(() => null))?.[0]?.records ?? [];
    const kernel = manifest.kernels[state.kernel].id;
    const held = records.filter((r) => r.kind === 'absent').map((r) => r.file).filter((f) => HOLDER.has(`${kernel}/${f}`));
    const ids = wanted_(held).filter((id) => !inFlight.has(id));
    if (!ids.length || !await fetchPackages(ids)) return;
    await restart();
    if (!plainLint()) await start();
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
    try { await kernel(+kernelBox.value); await restart(); if (!plainLint()) await start(); } catch (e) { toast(`the kernel did not load: ${e.message}`); kernelBox.value = String(state.kernel); }
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
    post({ type: 'project', dir: state.dir, files, replace: true });
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
    post({ type: 'project', dir: state.dir, files });
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
      f: state.file in state.ro ? state.root : state.file, a: args.value, k: manifest.kernels[state.kernel].id, c: editor.cursor(),
      ...(Object.keys(configSet).length ? { s: configSet } : {}) };
    const packed = await through(new TextEncoder().encode(JSON.stringify(data)), new CompressionStream('deflate-raw'));
    let s = '';
    for (let i = 0; i < packed.length; i += 0x8000) s += String.fromCharCode(...packed.subarray(i, i + 0x8000));
    return `${location.href.split('#')[0]}#s=${btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')}`;
  }
  async function restore(hash) {
    let link;
    try { link = JSON.parse(new TextDecoder().decode(await through(unb64(hash), new DecompressionStream('deflate-raw')))); } catch { return null; }
    // a link from before projects: one file and its text
    if (link && !link.p) link = { ...link, x: projectOf(link.f) ?? link.f, p: { [link.f]: link.t } };
    if (link?.x && !(link.x in sampleTexts)) sampleTexts[link.x] = '';
    return link;
  }
  /** A shared project: its settings, kernel, files, open file, command and cursor. */
  async function openLink(link) {
    const refused = link.s && await configRefused(link.s);
    if (refused) toast(`The link's settings were not applied: ${refused}`);
    else if (link.s) configKeep(link.s);
    const k = manifest.kernels.findIndex((x) => x.id === link.k);
    if (k >= 0 && k !== state.kernel) { manualKernel = true; await kernel(k); }
    if (!link.x) {
      await project(Object.fromEntries(Object.entries(link.p).map(([n, t]) => [n, encode(t)])), 'From the link', link.r);
      if (link.f in state.texts) choose(link.f);
      args.value = link.a;
    } else {
      if (state.upload) Object.assign(state, { dir: '/work', texts: { ...sampleTexts }, upload: false, lists: {} });
      Object.assign(state.texts, link.p);
      delete state.lists[link.x];
      await example(link.x, { file: link.f, list: link.l, root: link.r, command: link.a });
    }
    if (link.c) editor.select(link.c);
  }
  async function linkAsked(hash) {
    const link = await restore(hash.slice(2));
    if (link && await replaceOk(link.x ? `the shared ${link.x}` : 'the shared project', hash)) await openLink(link);
  }
  $('play-link').onclick = async () => {
    const link = await permalink();
    history.replaceState(null, '', link);
    try { await navigator.clipboard.writeText(link); } catch {}
    toast(state.upload ? 'Link copied. It holds the project\'s text files, not its images.' : 'Link copied');
  };

  // A link that opens GitHub's new-issue form, prefilled and short; it posts nothing.
  $('play-issue').onclick = () => {
    const text = (state.texts[state.file] ?? '').slice(0, 1500), version = document.querySelector('a.version')?.textContent.trim() ?? '';
    const body = `satex ${version}, kernel ${manifest.kernels[state.kernel]?.label ?? ''}\n\nWhat happened:\n\n\n${state.file}:\n\`\`\`latex\n${text}\n\`\`\`\n`;
    window.open(`${manifest.repository ?? 'https://github.com/EagleoutIce/satex-analyzer'}/issues/new?title=${encodeURIComponent(`[playground] `)}&body=${encodeURIComponent(body)}`, '_blank', 'noopener');
  };

  // ---------------------------------------------------------------- start

  /** What the page is doing, in one place: the status bar's text and bar, and a card with the details on hover. */
  const bar = $('play-progress'), loads = new Map(), actBox = $('act-detail');
  const act = { phase: '', text: '', since: 0, changed: 0, got: 0, all: 0, started: 0, rest: '' };
  const PHASE = { unpacking: ['Unpacking', 'Unpacking the TeX files…'], importing: ['Importing', 'Importing the kernel caches…'] };
  function busy(name, text, { done = null, got = 0, all = 0 } = {}) {
    const now = performance.now();
    if (!act.phase) { act.started = now; act.rest = status.dataset.done ? status.textContent : 'Ready. Findings update as you type; Enter runs the command.'; }
    if (act.phase !== name) act.since = now;
    if (text !== act.text || got !== act.got) act.changed = now;
    Object.assign(act, { phase: name, text, got, all });
    // a status without both counts shows work going on, not a share of it; with them, the time left
    self.satexProgress.set(bar, Number.isFinite(done) ? done : null);
    const left = Number.isFinite(done) ? self.satexProgress.left(done, now - act.since) : null;
    status.textContent = left ? `${text} · about ${left} s left` : text;
    status.title = '';
    drawAct();
  }
  /** Work is over: the bar goes, the last result stays or comes back. */
  function idle(text = null) {
    bar.hidden = true;
    if (act.phase && text === null && status.textContent === act.text && act.rest) status.textContent = act.rest;
    if (text !== null) status.textContent = text;
    act.phase = ''; act.text = '';
    drawAct();
  }
  const mib = (n) => (n / 2 ** 20).toFixed(1);
  function drawAct() {
    if (actBox.hidden) return;
    const now = performance.now(), s = (t) => `${((now - t) / 1000).toFixed(0)} s`;
    const rows = act.phase ? [['phase', act.phase], ['doing', act.text.replace(/…$/, '')], ...(act.all ? [['bytes', `${mib(act.got)} of ${mib(act.all)} MiB`]] : []),
      ['in this phase', s(act.since)], ['since start', s(act.started)], ['last change', `${s(act.changed)} ago`]] : [['phase', 'idle'], ['status', status.textContent]];
    actBox.innerHTML = rows.map(([k, v]) => `<dt>${k}</dt><dd>${escape(v)}</dd>`).join('');
  }
  let actTick = 0;
  const showAct = (on) => { actBox.hidden = !on; clearInterval(actTick); if (on) { drawAct(); actTick = setInterval(drawAct, 1000); } };
  const actHost = actBox.parentElement;
  actHost.addEventListener('mouseenter', () => showAct(true)); actHost.addEventListener('mouseleave', () => showAct(false));
  actHost.addEventListener('focusin', () => showAct(true)); actHost.addEventListener('focusout', () => showAct(false));
  function progressed({ label, loaded, total }) {
    if (PHASE[label]) { busy(PHASE[label][0], PHASE[label][1]); return; }
    loads.set(label, [loaded, total]);
    const [got, all] = [...loads.values()].reduce(([g, a], [l, t]) => [g + l, a + t], [0, 0]);
    const names = [...loads.keys()];
    busy('Fetching', `Fetching ${names.length > 2 ? plural(names.length, 'download') : names.join(' and ')}: ${mib(got)} of ${mib(all)} MiB`, { done: all ? Math.min(got / all, 1) : null, got, all });
    status.title = names.join(', ');
  }

  worker.onmessage = async ({ data }) => {
    if (data.type === 'lsp') {
      running -= order.shift() ? 1 : 0;
      for (const m of data.out) {
        if (m.id !== undefined && pending.has(m.id)) {
          const p = pending.get(m.id);
          pending.delete(m.id);
          if (m.error) p.reject(new Error(m.error.message.replace(/^[\w/]+: /, ''))); else p.resolve(m.result ?? null);
        } else if (m.method === 'satex/styleCandidates') {
          for (const c of m.params.candidates ?? []) askStyle(c);
        } else if (m.method === 'textDocument/publishDiagnostics' && m.params.uri.startsWith(`file://${state.dir}/`)) {
          // The rule's number is the LSP code: the page keys rules by name.
          for (const d of m.params.diagnostics) [d.number, d.code] = [d.code, d.data?.rule ?? d.code];
          // Findings that come from a missing package wait for it: shown once its bundle is in.
          const unresolved = m.params.diagnostics.filter((d) => d.code === 'unresolved-file').map((d) => d.message);
          const waiting = missing(unresolved);
          if ((waiting.length || inFlight.size) && m.params.uri === uri()) {
            const names = [...new Set([...waiting, ...inFlight])].map((id) => PACKAGES.get(id).label).join(', ');
            busy('Loading', `Loading ${plural(names.split(', ').length, 'package')}…`);
            status.title = names;
            if (waiting.length) fetchMissing(unresolved).then((ok) => { if (!ok) { show(m.params.uri, m.params.diagnostics); typeset(m.params.uri); } });
            continue;
          }
          if (running > 0 && m.params.uri === uri()) continue; // intermediate: a newer text is in flight
          show(m.params.uri, m.params.diagnostics);
          typeset(m.params.uri);
          outline();
          if (m.params.uri === uri()) { fetchAbsent(); colorize(); }
          if (m.params.uri === uri()) prefetchCommands();
          loadCaches(); // the status bar's count, and the table if open
          if (m.params.uri === uri(state.root ?? state.file)) written(m.params.uri);
          if (m.params.uri === uri()) lsp('satex/caches', { textDocument: { uri: m.params.uri } }).then((r) => cacheReport((r ?? []).map((l) => ['err', l]))).catch(() => {});
          fetchMissing(unresolved);
        }
      }
    } else if (data.type === 'status') {
      traceLive?.(data);
      if (data.text && act.phase === 'Analyzing') phase(data.text, data.total ? Math.min(data.reached / data.total, 1) : null);
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
    booted = true;
    try { sessionStorage.removeItem('satex-recovered'); } catch {}
    timing('t-init', secs(performance.now() - started), 'time to fetch satex and the pdfTeX kernel and set them up');
    idle();
    sampleTexts = ready.samples;
    const hash = decodeURIComponent(location.hash.slice(1));
    const link = hash.startsWith('s=') ? await restore(hash.slice(2)) : null;
    if (hash.startsWith('open=')) history.replaceState(null, '', location.pathname);
    const ex = link ? link.x : hash in sampleTexts && !shared.has(hash) ? hash : projectOf(hash) ?? manifest.sample;
    state.texts = { ...sampleTexts, ...(link?.x ? link.p : {}) };
    const k = manifest.kernels.findIndex((x) => x.id === link?.k);
    if (link?.s) configKeep(link.s);
    // stored settings satex refuses, or with which the last start failed, are set aside: the page starts with the defaults
    let failed = null;
    try { failed = localStorage.getItem('satex-start'); } catch {}
    const refused = await configRefused(configSet) ?? (failed === JSON.stringify(configSet) && Object.keys(configSet).length ? 'the last start failed with them' : null);
    if (refused) {
      configRejected = { set: configSet, why: refused };
      configKeep({});
      toast(`Reset ${plural(Object.keys(configRejected.set).length, 'setting')} (${Object.entries(configRejected.set).map(([k, v]) => `${k}: ${yamlValue(v)}`).join(', ')}): ${refused}. Settings shows them.`);
    }
    if (k >= 0) { manualKernel = true; if (k > 0) await kernel(k); }
    kernelBox.value = String(state.kernel);
    // packages.txt's first section, fetched while the editor loads
    const preloading = fetchPackages(wanted_(manifest.preload ?? [])).catch(() => false);
    try {
      editor = await codemirror($('play-editor'), '');
      $('play-source').remove();
    } catch (e) {
      console.warn('CodeMirror did not load, the editor is a textarea', e);
      editor = textarea($('play-source'));
    }
    button.textContent = 'Run';
    button.disabled = false;
    await preloading;
    if (link) await openLink(link);
    else await example(ex, { file: shared.has(hash) ? hash : ex });
    const opening = hash.match(/^open=(.+):(\d+)$/);
    if (opening) await openPlace(opening[1], { line: Number(opening[2]) - 1, character: 0 });
    if (!plainLint()) return link && !link.x ? start() : undefined;
    status.textContent = 'Ready. Findings update as you type; Enter runs the command.';
    status.dataset.done = 'ready';
  })().catch((e) => { status.textContent = `the playground failed: ${e.message}`; status.dataset.done = 'error'; });
})();
