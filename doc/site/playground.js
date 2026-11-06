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
    try { for (const k of Object.keys(localStorage)) if (k.startsWith('satex') && k !== 'satex-drafts') localStorage.removeItem(k); } catch {}
    try { indexedDB.deleteDatabase('satex-playground'); } catch {}
    try { sessionStorage.setItem('satex-note', 'The stored layout, settings and caches did not fit this version of the playground: they were reset.'); } catch {}
    location.reload();
  };
  // the browser's layout notice (`ResizeObserver loop …`) is no failure: it carries no error object
  addEventListener('error', (e) => { if (!booted && e.error) recover(e.error); });
  addEventListener('unhandledrejection', (e) => {
    if (!booted) return recover(e.reason);
    const m = String(e.reason?.message ?? e.reason ?? '');
    if (e.reason?.name === 'AbortError' || /cancel|abort/i.test(m)) return;
    console.warn('unhandled', e.reason);
    self.satexToast?.(`Something failed: ${m.slice(0, 120)}`);
  });
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
    // the tabs scroll, the actions wrapped under them, or on a row of their own they run past the bar
    const past = () => actions.getBoundingClientRect().right > filebar.getBoundingClientRect().right + 1;
    const check = collapsing(filebar, () => tabs.scrollWidth > tabs.clientWidth + 1 || actions.offsetTop > tablist.offsetTop + tablist.offsetHeight / 2 || past());
    // the menu the buttons fold into: a click opens them as a column under it
    const more = Object.assign(document.createElement('button'), { type: 'button', className: 'pill more-actions', title: 'more actions', innerHTML: '<svg viewBox="0 0 16 16" aria-hidden="true"><circle cx="3.5" cy="8" r="1.2"/><circle cx="8" cy="8" r="1.2"/><circle cx="12.5" cy="8" r="1.2"/></svg>' });
    more.setAttribute('aria-expanded', 'false');
    actions.append(more);
    more.onclick = (e) => { e.stopPropagation(); const open = !actions.classList.contains('menu-open'); actions.classList.toggle('menu-open', open); more.setAttribute('aria-expanded', String(open)); };
    document.addEventListener('click', (e) => { if (!more.contains(e.target)) { actions.classList.remove('menu-open'); more.setAttribute('aria-expanded', 'false'); } });
    new MutationObserver(check).observe(tabs, { childList: true });
    const ask = args.closest('.ask');
    collapsing(ask, () => args.clientWidth < 12 * parseFloat(getComputedStyle(args).fontSize));
  }
  // Panes snap, as in flowR's playground: a splitter dragged into a pane's last half
  // minimum folds that pane away, a click on the folded splitter brings it back; near a
  // third, half or two thirds it rests there. A section's grip sets its height, dragged
  // under half the minimum it closes the section. Sizes are kept per browser.
  let foldConsole; // the satex console's fold, set below
  // the side pane: one tab per panel, the PDF optionally beside the open one; both kept per browser
  const showPanel = (() => {
    const side = document.querySelector('.side-pane'), tabs = [...side.querySelectorAll('.side-tabs [role=tab]')], split = $('side-split');
    const get = (k) => { try { return localStorage.getItem(k); } catch { return null; } };
    const set = (k, v) => { try { localStorage.setItem(k, v); } catch {} };
    const phone = matchMedia('(max-width: 36rem)'), splitEl = document.querySelector('.play-split'), editorTab = $('phone-editor');
    // a phone shows the editor or one panel, the bottom bar switches
    const view = (v) => { splitEl.dataset.view = v; editorTab.setAttribute('aria-pressed', String(v === 'editor')); };
    let ready = false;
    const show = (id) => {
      if (ready && phone.matches) view('panel');
      const beside = side.classList.contains('beside') && !phone.matches;
      if (!tabs.some((t) => t.dataset.panel === id) || beside && id === 'play-pdf') id = 'play-typeset';
      for (const t of tabs) {
        const on = t.dataset.panel === id;
        t.setAttribute('aria-selected', String(on));
        t.tabIndex = on ? 0 : -1;
        $(t.dataset.panel).hidden = !on && !(beside && t.dataset.panel === 'play-pdf');
      }
      set('satex-side-tab', id);
    };
    const first = get('satex-side-tab') ?? 'play-typeset';
    const current = () => tabs.find((t) => t.getAttribute('aria-selected') === 'true')?.dataset.panel ?? first;
    const pair = (on) => { side.classList.toggle('beside', on); split.setAttribute('aria-pressed', String(on)); set('satex-side-split', on ? '1' : '0'); show(current()); };
    for (const t of tabs) t.onclick = () => show(t.dataset.panel);
    side.querySelector('.side-tabs').addEventListener('keydown', (e) => {
      const step = { ArrowLeft: -1, ArrowRight: 1 }[e.key], shown = tabs.filter((t) => t.offsetParent);
      if (!step || !e.target.matches('[role=tab]')) return;
      const next = shown[(shown.indexOf(e.target) + step + shown.length) % shown.length];
      show(next.dataset.panel);
      next.focus();
    });
    split.onclick = () => pair(!side.classList.contains('beside'));
    editorTab.onclick = () => view('editor');
    phone.addEventListener('change', () => show(current()));
    view('editor');
    pair(get('satex-side-split') === '1');
    ready = true;
    return show;
  })();
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

    // each section of the file tree: a grip under its body, and open or folded as the reader left it
    kept.heights ??= {};
    kept.open ??= {};
    for (const sect of document.querySelectorAll('.tree > details.sect')) {
      const body = sect.querySelector('.grow');
      if (!body) continue;
      if (sect.id in kept.open) sect.open = kept.open[sect.id];
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

    // the file tree: a toggle in the tab row, a grip for its width; folded below 64rem
    const tree = $('play-tree'), toggle = $('play-treetoggle'), work = tree.parentElement;
    const showTree = (on) => { tree.hidden = !on; toggle.setAttribute('aria-expanded', on); };
    showTree(kept.tree ?? !matchMedia('(max-width: 64rem)').matches);
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
    foldConsole(kept.console ?? matchMedia('(max-width: 36rem)').matches, false);
    // the run output over the command line: its grip sets its height
    const out = $('play-out');
    if (kept.term) out.style.setProperty('--h', `${kept.term}px`);
    drag($('play-termgrip'), () => out.getBoundingClientRect().height, (m, e, h) => {
      kept.term = Math.round(clamp(h - (m.clientY - e.clientY), 4 * rem(), innerHeight * 0.75));
      out.style.setProperty('--h', `${kept.term}px`);
    }, keep);
  }
  // The worker's code comes with the page: file:// pages cannot start a worker from a file.
  const newWorker = () => new Worker(URL.createObjectURL(new Blob([manifest.worker], { type: 'text/javascript' })));
  let worker = newWorker();
  /** Texts sent to the server, shared with the worker where the page is cross-origin isolated: a run
   * stops once a newer text was sent (src/lsp/wasm.rs), instead of being waited for. */
  const textsSent = self.crossOriginIsolated ? new Int32Array(new SharedArrayBuffer(4)) : null;
  worker.onmessage = (e) => onWorkerMessage(e);
  worker.onerror = (e) => onWorkerError(e);
  const escape = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  const SEVERITY = ['', 'error', 'warning', 'info', 'hint'];
  const sev = (d) => SEVERITY[d.severity] || 'info';
  const secs = self.satexCards.duration;
  const compact = new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 });
  /** A count with its thousands set apart by thin spaces: 21 939. */
  const num = (n) => Number(n).toLocaleString('en').replace(/,/g, '\u2009');
  /** A count in a chip or badge: 22k, 1.4M; the exact one is `num`'s. */
  const short = (n) => compact.format(Number(n)).toLowerCase().replace('m', 'M');
  /** A TeX length (in pt) in the other units too, as TeX counts them: 1in = 72.27pt, 1bp = 72.27/72pt, 1cm = 72.27/2.54pt. */
  const lengths = (pt) => {
    const sig = (x) => String(+x.toPrecision(3));
    return `${sig(pt)}pt = ${sig(pt * 2.54 / 72.27 * 10)}mm = ${sig(pt * 2.54 / 72.27)}cm = ${sig(pt / 72.27)}in = ${sig(pt * 72 / 72.27)}bp`;
  };
  /** A length in every unit TeX knows, as the editor's length hover shows it (TeXbook p. 57). */
  const UNITS_PER_PT = { pt: 1, mm: 25.4 / 72.27, cm: 2.54 / 72.27, in: 1 / 72.27, bp: 72 / 72.27, pc: 1 / 12, dd: 1157 / 1238, cc: 1157 / 14856, sp: 65536 };
  const unitsCard = (pt) => {
    const sig = (x) => String(+x.toPrecision(4));
    const units = Object.keys(UNITS_PER_PT);
    return `**${sig(pt)}pt**\n\n| ${units.join(' | ')} |\n|${units.map(() => '---').join('|')}|\n| ${units.map((u) => sig(pt * UNITS_PER_PT[u])).join(' | ')} |`;
  };
  // a length shown on the page (the PDF's text block) explains itself as an editor length does
  document.addEventListener('mouseover', (e) => {
    const el = e.target instanceof Element && e.target.closest('[data-pt]');
    if (el) { self.satexTip.card.show(el, self.satexCards.hover(unitsCard(+el.dataset.pt))); el.addEventListener('mouseleave', () => self.satexTip.card.hide(150), { once: true }); }
  });
  const size = (n) => n < 2 ** 20 ? `${Math.ceil(n / 1024)} KiB` : `${(n / 2 ** 20).toFixed(1)} MiB`;
  const started = performance.now();

  // The project, as in flowR's playground: its files in the tree, the open ones as tabs, one
  // root that runs and the language server analyze, its \input files with it. An example
  // (`manifest.projects` names the files it reads besides itself) or an upload is a project.
  // `texts` holds every file under `dir`: an example's edits stay while another one is open.
  // Read-only files (`ro`) are kernel or package files opened by a definition: never sent or run.
  const state = { dir: '/work', texts: {}, file: null, root: manifest.sample, kernel: 0, loaded: new Set([0]), upload: false, ro: {}, bins: {},
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
    ask.innerHTML = `<p>${inline(candidate.question)}</p><button data-a="off">Yes, turn it off</button><button data-a="show">Show first</button><button>No</button><button data-a="keep">Never ask</button>`;
    ask.onclick = (e) => {
      const button = e.target.closest('button');
      if (!button) return;
      if (button.dataset.a === 'show') {
        // the prompt stays: the first finding opens beside it
        const f = candidate.first ?? {}, at = { line: Math.max(0, (f.line ?? 1) - 1), character: Math.max(0, (f.col ?? 1) - 1) };
        return void openPlace(String(f.path ?? ''), at);
      }
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
  /** The fixes the open hover card offers, by the index its buttons carry. */
  const hoverFixes = [];
  /** A finding as a card; `fixes`: with a button per fix, applied in place. */
  const findingCard = (d, fixes = false) => {
    const buttons = fixes && d.fixes?.length ? `<div class="hover-tools">${d.fixes.map((f) => `<button type="button" class="small${f.safety === 'safe' ? '' : ' ghost'}" data-fix="${hoverFixes.push(f) - 1}" title="${escape(f.safety)} fix">${escape(f.title)}</button>`).join('')}</div>` : '';
    return `<div class="tip-lint f-${sev(d)}"><p><span class="sev sev-${sev(d)}">${sev(d)}</span> <code>${escape(d.code)}</code> ${inline(d.message)}</p>${buttons}</div>`;
  };
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
  // a dead worker fails its waiters instead of hanging them, and is replaced
  function onWorkerError(e) {
    e.preventDefault?.();
    failWaiters(new Error(e.message || 'the worker stopped'));
    recoverWorker(e.message || 'the worker stopped');
  }
  /** The waiter for a reply, no longer owed once answered (the watchdog counts what is owed). */
  const answer = (type) => { const w = waiting[type]; delete waiting[type]; return w; };
  function failWaiters(error) {
    for (const [k, w] of Object.entries(waiting)) { delete waiting[k]; w.reject(error); }
  }
  /** How long the worker may stay silent while it owes an answer before it counts as stuck: a run reports its
   * progress every few seconds (chaos.mjs measures the longest silence). Each restart for silence doubles it until a run finishes. */
  const HANG_MS = 30000;
  let hang = HANG_MS;
  // a server starting (an instance over every file) is given the start's own limit, restart() says when that passes
  const hangLimit = () => self.satexHangMs ?? (act.phase === 'Starting' ? START_MS + 5000 : hang);
  let lastHeard = performance.now(), recovering = false, maxGap = 0, recoveries = 0, recent = [];
  /** A stuck or dead worker is replaced: the new one gets the kernel, bundles and files the old one had,
   * then the language server starts again on them. What the page shows from the last finished run stays. */
  async function recoverWorker(why, note = `The language server stopped (${why}). Restarting it, the findings shown are from its last run.`) {
    if (recovering || !booted) return;
    // a worker that keeps dying on this document (out of memory) is not restarted in a loop: the line says so, a click tries again
    const now = performance.now();
    recent = [...recent.filter((t) => now - t < 120000), now];
    if (recent.length > 3 && why !== 'asked again') {
      worker.terminate();
      failWaiters(new Error(why));
      for (const p of pending.values()) p.resolve(null);
      pending.clear(); order.length = 0; running = 0;
      return stoppedLine(`The language server keeps stopping on this project (${escape(why)}).`, () => { recent = []; recoverWorker('asked again', 'Restarting the language server…'); });
    }
    recovering = true;
    recoveries++;
    if (why.startsWith('no answer')) hang *= 2;
    const old = worker;
    starts++; // a start under way was the old worker's: it stops quietly, this one starts again
    try {
      old.terminate();
      failWaiters(new Error(why));
      for (const p of pending.values()) p.resolve(null);
      pending.clear();
      order.length = 0; running = 0;
      if (!mirrorable) { busy('Stopped', 'The worker stopped. Reload the page to start it again.'); status.dataset.done = 'error'; return; }
      toast(note);
      busy('Restarting', 'Restarting the language server…');
      const w = newWorker();
      const replies = {};
      w.onmessage = ({ data }) => (data.type === 'error' ? replies[REPLY[data.of]]?.reject(new Error(data.message.split('\n')[0])) : replies[data.type]?.resolve(data));
      const send = (msg) => within(new Promise((resolve, reject) => {
        const reply = REPLY[msg.type];
        if (reply) replies[reply] = { resolve, reject }; else resolve(null);
        w.postMessage(msg);
      }), START_MS, `the new worker's ${msg.type}`);
      for (const msg of mirrored) await send(msg);
      await send({ type: 'project', dir: runnerFiles.dir, files: runnerFiles.files, replace: true });
      w.onmessage = (e) => onWorkerMessage(e);
      w.onerror = (e) => onWorkerError(e);
      worker = w;
      lastHeard = performance.now();
      recovering = false;
      for (const [msg, transfer] of held.splice(0)) post(msg, transfer);
      synced = 0;
      await restart();
    } catch (e) {
      stoppedLine(`The language server could not be restarted: ${escape(e.message)}.`, () => recoverWorker('asked again', 'Restarting the language server…'));
    } finally {
      recovering = false;
    }
  }
  /** Work that cannot go on by itself: the status line says why, that the text is kept, and offers to try again. */
  function stoppedLine(why, retry) {
    idle();
    status.innerHTML = `<span class="st-msg">${why} Your text is kept.</span> <button type="button" class="small ghost" id="play-retry">Try again</button>`;
    status.dataset.done = 'error';
    $('play-retry').onclick = retry;
  }
  /** `p`, or an error naming `what` once `ms` pass without its answer. */
  const within = (p, ms, what) => Promise.race([p, new Promise((_, reject) => setTimeout(() => reject(new Error(`${what} got no answer in ${Math.round(ms / 1000)} s`)), ms))]);
  const act = { phase: '', text: '', shown: '', lastSecond: /** @type {number | null} */ (null), since: 0, changed: 0, got: 0, all: 0, started: 0, rest: '', steps: [], run: 0, after: '', version: -1, ended: 0 };
  setInterval(() => {
    // a status that says Analyzing with nothing in flight is stale: findings held back are shown, the bar goes
    if (act.phase === 'Analyzing' && !running && !analysisQueued && !pending.size && performance.now() - act.changed > 10000) { if (heldFindings) publish(heldFindings); phase(); }
    // a step that says nothing new for a while (a kernel cache built) shows its time, so the line is seen to live
    else if (act.phase && performance.now() - act.changed > 5000 && status.textContent === act.shown) status.textContent = act.shown = `${act.text} · ${secs(performance.now() - act.since)}`;
    if (!recovering && performance.now() - lastHeard > hangLimit() && (running > 0 || pending.size || Object.keys(waiting).length))
      recoverWorker(`no answer for ${Math.round(hangLimit() / 1000)} s`);
  }, 2000);
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
  /** What is asked while a new worker replaces a stuck one waits for it: the old one would never answer. */
  const held = [];
  function post(msg, transfer = []) {
    if (recovering) { held.push([msg, transfer]); return; }
    // a file:// page hands its buffers over, so a second worker cannot get them: commands share the first
    if (msg.given) mirrorable = false;
    // another project, or a new server: the run under way is of texts no longer wanted
    if (textsSent && (msg.type === 'reset' || (msg.type === 'project' && msg.replace))) Atomics.add(textsSent, 0, 1);
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
    // a crashed console worker fails its command, and the next command starts a new one
    w.onerror = (e) => { e.preventDefault?.(); runner = null; for (const r of Object.values(replies)) r.reject(new Error(e.message || 'the console worker stopped')); };
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
  /** Per file, the version of the last text sent to the server, and whether the editor's text is newer still. */
  const sentVersions = new Map();
  let typedSincePush = false;
  /** Findings of a text older than the server's: the editor no longer has it. */
  const outdated = (p) => p.version != null && (sentVersions.get(p.uri) ?? -Infinity) > p.version;
  function lsp(method, params, notify = false) {
    // while a new worker starts, nothing is asked of the old one: restart() then tells the new one everything
    if (recovering) return notify ? null : Promise.resolve(null);
    // a widget never waits for a run: the run stops (the shared counter), the widget is answered from the last
    // finished one, and the run starts again after it
    if (running > 0 && textsSent && /^satex\/(view|dialog|dialogApply)$/.test(method)) { Atomics.add(textsSent, 0, 1); analysisQueued = true; }
    const message = { jsonrpc: '2.0', method, params, ...(notify ? {} : { id: nextId++ }) };
    const runs = method === 'satex/analyze';
    // A read-only question asked while a run goes on is answered from the last finished one, never queued behind it.
    const read = !notify && !runs && (method === 'workspace/executeCommand' ? params.command === 'satex/query' : /^textDocument\/(hover|documentSymbol|definition|references|documentHighlight|signatureHelp|foldingRange|inlayHint|documentLink)$/.test(method));
    const key = read && method + JSON.stringify(params);
    if (read && running > 0) {
      if (answered.has(key)) return Promise.resolve(answered.get(key));
      if (method === 'textDocument/hover') return Promise.resolve(null);
    }
    // the version of each text the server has: findings of an older one are dropped
    if (method === 'textDocument/didOpen' || method === 'textDocument/didChange') {
      sentVersions.set(params.textDocument.uri, params.textDocument.version);
      if (textsSent) Atomics.add(textsSent, 0, 1);
    }
    order.push(runs);
    running += +runs;
    worker.postMessage({ type: 'lsp', message, sent: textsSent ? Atomics.load(textsSent, 0) : null });
    if (notify) return null;
    const reply = new Promise((resolve, reject) => pending.set(message.id, { resolve, reject }));
    return read ? reply.then((r) => { if (answered.size > 300) answered.delete(answered.keys().next().value); answered.set(key, r); return r; }) : reply;
  }
  const answered = new Map(); // read-only request -> its last finished answer
  // a 0-based LSP line, clamped: a stale or negative position never throws
  const lineOf = (doc, n) => doc.line(Math.max(1, Math.min((n | 0) + 1, doc.lines)));
  const at = (doc, p) => Math.max(0, Math.min(lineOf(doc, p.line).from + (p.character | 0), doc.length));
  const position = (doc, offset) => { const l = doc.lineAt(offset); return { line: l.number - 1, character: offset - l.from }; };
  /** The text `range` (LSP) spans in `text`. */
  const offsetIn = (text, p) => { let i = 0; for (let l = 0; l < p.line; l++) { const n = text.indexOf('\n', i); if (n < 0) return text.length; i = n + 1; } return Math.min(i + p.character, text.length); };
  const textIn = (text, r) => text.slice(offsetIn(text, r.start), offsetIn(text, r.end));
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
  let version = 0, synced = 0, saved = 0, analyzedVersion = 0, timer = null, checkStart = 0, retryCompletion = false;
  let typedAt = 0;
  function changed() {
    version++;
    typedSincePush = true;
    typedAt = performance.now();
    if (refs && refs.file === state.file) clearRefs();
    clearTimeout(timer);
    // A command name or an argument still being typed (`\dra`, `\cite{kn`) waits: the analysis
    // would block completion for seconds, and its findings would be about unfinished text.
    const line = editor.text().split('\n')[editor.position().line] ?? '', before = line.slice(0, editor.position().character);
    dirty = true;
    timer = setTimeout(sync, /\\[A-Za-z@]*$|\{[^{}\s]*$/.test(before) ? 1500 : 400);
    markStale();
    pdfEdited();
    keepDrafts();
  }
  /** Drafts: the examples' edited and added files, kept in this browser, so a reload, a crash or a closed tab loses no text. */
  const DRAFTS = 'satex-drafts';
  let draftTimer = 0;
  function keepDrafts(now = false) {
    clearTimeout(draftTimer);
    if (!now) { draftTimer = setTimeout(() => keepDrafts(true), 300); return; }
    if (state.upload || !sampleTexts) return;
    if (editor && state.file && !(state.file in state.ro)) state.texts[state.file] = editor.text();
    const texts = Object.fromEntries(Object.entries(state.texts).filter(([n, t]) => !(n in state.ro) && t !== sampleTexts[n]));
    try { if (Object.keys(texts).length) localStorage.setItem(DRAFTS, JSON.stringify({ texts, lists: state.lists })); else localStorage.removeItem(DRAFTS); } catch {}
  }
  addEventListener('pagehide', () => keepDrafts(true));
  const drafts = () => { try { return JSON.parse(localStorage.getItem(DRAFTS) ?? 'null') ?? {}; } catch { return {}; } };
  /** After an analysis, the command list the server would offer after a `\\`, asked once at a command
   * of the file: the first `\\` typed then completes at once. */
  function prefetchCommands() {
    // every name in force: what a `\\` completes from at once, beyond the server's best few hundred
    namesInForce(false).then((records) => scopeItems.set(uri(), records.map((r) => ({ label: r.name.replace(/^\\/, ''), detail: r.effective ?? r.name, kind: 3 }))));
    // each kind of place once, from whichever open file of the project has one (`\\ref{` may stand in a chapter only)
    for (const name of Object.keys(state.texts).filter((n) => opened.has(uri(n)))) {
      const text = name === state.file ? editor?.text() ?? '' : state.texts[name];
      for (const [place, offset] of askablePlaces(text)) {
        if (commandItems.has(place)) continue;
        const before = text.slice(0, offset).split('\n');
        completions({ line: before.length - 1, character: before.at(-1).length }, before.at(-1).length, place, uri(name));
      }
    }
  }
  /** The kinds of place the text offers a completion in, each with one offset to ask at: after a `\\`,
   * and after the first `{` or `[` of each command (`ref{`, `cite{`, `color{`, `usepackage[`…). */
  const PREFETCH_PLACES = 40;
  function askablePlaces(text) {
    const places = new Map();
    const first = text.search(/\\[A-Za-z@]/);
    if (first >= 0) places.set('\\', first + 1);
    for (const m of text.matchAll(/\\([A-Za-z@]+)\*?([{[])/g)) {
      const place = argumentPlace(m[1], m[2]);
      if (!places.has(place) && places.size < PREFETCH_PLACES) places.set(place, m.index + m[0].length);
    }
    return places;
  }
  /** The name of the kind of place inside the argument of `command` opened by `bracket`. */
  const argumentPlace = (command, bracket) => bracket === '[' ? `${command}[` : command;
  /** Sends the buffer to the server unless it already has it: the text
   * only, so what is asked meanwhile is answered from the last analysis. */
  function push() {
    if (!state.file || state.file in state.ro) return false;
    state.texts[state.file] = editor.text();
    // `satex gui` saves each text to its file as it is sent
    if (manifest.local && saved !== version) {
      saved = version;
      post({ type: 'project', dir: state.dir, files: { [state.file]: new TextEncoder().encode(editor.text()) } });
    }
    if (data(state.file) && synced !== version) {
      synced = version;
      post({ type: 'project', dir: state.dir, files: { [state.file]: new TextEncoder().encode(editor.text()) } });
      if (database(state.file) && opened.has(uri())) lsp('textDocument/didChange', { textDocument: { uri: uri(), version }, contentChanges: [{ text: editor.text() }] }, true);
      return true;
    }
    if (synced === version || !opened.has(uri())) return false;
    synced = version;
    typedSincePush = false;
    lsp('textDocument/didChange', { textDocument: { uri: uri(), version }, contentChanges: [{ text: editor.text() }] }, true);
    return true;
  }
  /** Once typing pauses: the buffer, and an analysis of it. */
  function sync() {
    clearTimeout(timer); timer = null;
    // a completion may have sent the text already: what counts is whether this version was analyzed
    const pushed = push();
    if (pushed || (analyzedVersion !== version && !!state.file && !(state.file in state.ro))) {
      analyzedVersion = version;
      prefetch();
      // the root's text may now ask for another engine
      if (state.file === state.root && wanted() !== state.kernel && !manualKernel && !following) { following = true; restart().finally(() => { following = false; }); return; }
      analyze(); explainSoon();
    }
    dirty = false; markStale();
  }
  /** An analysis wanted while one runs is not queued behind it in the worker: one follows it, on the newest text,
   * however many edits came meanwhile (the worker cannot stop a run, so the stale ones are never started). */
  let analysisQueued = false;
  /** Findings of an older text dim while a newer analysis is wanted, pending or running. */
  let dirty = false;
  const pendingRun = () => !!state.file && (dirty || running > 0 || analysisQueued || timer !== null);
  /** Every view (findings, their count, the typeset text, explain, the PDF's layers) is tagged with the file and text
   * version its results are of ([`shownFor`]): it dims while the editor holds another file or a newer text, or a run is pending. */
  const shownOf = new Map(); // element -> [uri, version, class]
  function shownFor(el, target, of, cls = 'stale') { if (el) { shownOf.set(el, [target, of ?? null, cls]); markStale(); } }
  function markStale() {
    const sent = sentVersions.get(uri()), pending = pendingRun();
    if (!shownOf.has(list)) list.classList.toggle('stale', pending);
    for (const [el, [target, of, cls]] of shownOf) el.classList.toggle(cls, target !== uri() || (of !== null && sent !== undefined && of !== sent) || typedSincePush || pending);
  }
  /** Whether files beside the texts changed (a package landed) since the last analysis asked for: then every document runs again, not only the edited ones. */
  let filesChanged = false;
  function analyze(files = false) {
    dirty = false;
    filesChanged ||= files;
    if (!boot?.started) return; // before the first start: it analyzes once up, an instance booted now would be thrown away
    if (running > 0) { analysisQueued = true; markStale(); return; }
    checking();
    lsp('satex/analyze', filesChanged ? { files: true } : {}, true);
    filesChanged = false;
    markStale();
  }
  function drainAnalysis() {
    if (running === 0) hang = HANG_MS;
    if (running > 0) return;
    if (!analysisQueued) { if (heldFindings) publish(heldFindings); markStale(); return; }
    analysisQueued = false;
    analyze();
  }
  /** A running check shows in the status bar like any other work (`busy`); the worker reports how much of the file is read once the engine says. */
  const phase = (text, done = null, stage = null) => text ? busy('Analyzing', text[0].toUpperCase() + text.slice(1), { done, stage }) : idle();
  function checking() { phase(`Analyzing ${state.file}…`); checkStart = performance.now(); }

  /** Completion order: how well the item matches first (the exact name, a prefix of the label, a part
   * of it, a part of what it is found by such as a citation's title, the typed characters in order:
   * `m/f` finds `mode/fast`); then the project's own commands, documented ones, internals (`@`, `_`, `:`)
   * last; then the server's order and shorter names. */
  const tier = (i) => /\(document\)$/.test(i.detail ?? '') ? 0 : /[@_:]/.test(i.label) ? 2 : 1;
  const subsequence = (text, typed) => { let k = 0; for (const c of text) if (c === typed[k]) k++; return k === typed.length; };
  const closeness = (i, typed) => {
    const label = i.label.toLowerCase(), found = (i.filterText ?? '').toLowerCase();
    return label === typed ? -1 : label.startsWith(typed) ? 0 : label.includes(typed) ? 1 : found.includes(typed) ? 2
      : subsequence(label, typed) || subsequence(found, typed) ? 3 : null;
  };
  const rank = (items, typed) => items
    .map((i) => [i, tier(i), closeness(i, typed.toLowerCase())]).filter((x) => x[2] !== null)
    .sort((a, b) => a[2] - b[2] || a[1] - b[1] || (a[0].sortText ?? '').localeCompare(b[0].sortText ?? '') || a[0].label.length - b[0].label.length || a[0].label.localeCompare(b[0].label)).map((x) => x[0]);
  /** The `#rrggbb` of a Color item (kind 16) whose documentation is one, else null. */
  const colorOf = (item) => {
    const doc = typeof item.documentation === 'string' ? item.documentation : item.documentation?.value;
    return item.kind === 16 && /^#[0-9a-f]{6}$/i.test(doc ?? '') ? doc : null;
  };
  /** The kind of place at the end of `before`: the command whose open argument (or option list) it is in. */
  function placeAt(before) {
    const brace = before.match(/\\([A-Za-z@]+)\*?(?:\[[^\]]*\])*\{[^{}]*$/), option = before.match(/\\([A-Za-z@]+)\*?(?:\{[^{}]*\})*\[[^\]]*$/);
    return brace ? argumentPlace(brace[1], '{') : option ? argumentPlace(option[1], '[') : null;
  }
  /** The language server's completion items at `position`, kept per buffer version. While a run
   * keeps the server busy, the last items stand in only where they were asked for the same word. */
  const asked = new Map();
  let lastItems = null, lastWord = '';
  /** What the server last offered in each kind of place of the project (after a `\\`, in `\begin{`, `\cite{`, …):
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
  /** The commands a `\\` completed last visit, per project: the first `\\` of a visit answers before the first run ends. */
  const seedKey = () => `satex-complete:${state.root ?? state.file}`;
  function keepSeed(items) {
    const slim = items.slice(0, 400).map(({ label, detail, kind, insertText, insertTextFormat }) => ({ label, detail, kind, insertText, insertTextFormat }));
    try { localStorage.setItem(seedKey(), JSON.stringify(slim)); } catch {}
  }
  function loadSeed() {
    try { const seed = JSON.parse(localStorage.getItem(seedKey()) ?? 'null'); if (Array.isArray(seed)) commandItems.set('\\', seed); } catch {}
  }
  /** How long a completion waits for the server before the kept commands answer (the server's answer still replaces them). */
  const INSTANT_MS = 50;
  function completions(position, start = position.character, place = null, file = uri()) {
    const k = `${file}|${version}|${position.line}:${position.character}`, word = `${file}|${position.line}:${start}`;
    if (running && lastItems && lastWord === word) return lastItems;
    const served = place && commandItems.get(place), names = place === '\\' && scopeItems.get(file);
    const both = names ? [...(served ?? []), ...names.filter((n) => !served?.some((i) => i.label === n.label))] : served;
    // items asked elsewhere replace no range here: their text goes in at the cursor's word
    const kept = both?.map(({ textEdit, ...i }) => (textEdit ? { ...i, insertText: i.insertText ?? textEdit.newText } : i));
    // A run keeps the worker busy, so a question would wait behind it. The last items answer,
    // else the bundle's classes and packages, else nothing, and the server's answer is kept for next time.
    const seeds = { documentclass: NAMES.classes, LoadClass: NAMES.classes, usepackage: NAMES.packages, RequirePackage: NAMES.packages }[place];
    const now = kept ?? seeds?.map((label) => ({ label, kind: 17 })) ?? [];
    if (running && kept) return kept;
    if (!asked.has(k)) {
      asked.clear();
      asked.set(k, lsp('textDocument/completion', { textDocument: { uri: file }, position }).then((r) => {
        [lastItems, lastWord] = [(Array.isArray(r) ? r : r?.items) ?? [], word];
        if (place && lastItems.length) commandItems.set(place, lastItems);
        if (place === '\\' && lastItems.length) keepSeed(lastItems);
        return lastItems;
      }).catch(() => []));
    }
    if (!kept && !running) return asked.get(k);
    // what answers now; the server's items replace it when they come
    let early = running > 0;
    asked.get(k).then(() => { if (early && document.querySelector('.cm-tooltip-autocomplete') && k.endsWith(`|${version}|${position.line}:${position.character}`)) editor.complete?.(); });
    if (early) return now;
    return Promise.race([asked.get(k), new Promise((done) => setTimeout(() => { early = true; done(now); }, INSTANT_MS))]);
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
      // inside a {…} or […] the cursor's list item starts after `{`, `[`, `,`, `=` or a placement's `!`
      const open = [...line.text.slice(0, key.from - line.from)].reduce((d, c) => d + +'{['.includes(c) - +'}]'.includes(c), 0) > 0;
      const item = open && /[{[,=!]\s*$/.test(line.text.slice(0, key.from - line.from));
      // `(` opens a node name or coordinate (`\\draw (a) -- (`)
      if (!ctx.explicit && before !== '\\' && before !== '{' && before !== '(' && !item) return null;
      push();
      const start = before === '\\' ? word.from : key.from;
      // the kind of place: a command name, or the argument of the command whose brace is open
      const place = before === '\\' ? '\\' : placeAt(line.text.slice(0, key.from - line.from));
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
          label: i.label, detail: i.documentation ? i.detail : undefined, usage: colorOf(i) || i.documentation ? null : i.labelDetails?.detail ? `${i.labelDetails.detail}${i.labelDetails.description ? ` (${i.labelDetails.description})` : ''}` : i.detail,
          hex: colorOf(i), preview: i.data?.preview, color: colorOf(i), type: KIND[i.kind] ?? 'text',
          // the side panel: a command's card, the one hover and Explain show; else the server's documentation
          info: before === '\\' ? () => itemInfo(i) : i.documentation ? () => Object.assign(document.createElement('div'), { className: 'hover cm-item-card', innerHTML: card(i.documentation.value ?? i.documentation) }) : i.data?.key ? () => itemInfo(i, true) : undefined,
          apply: again(i, i.insertTextFormat === 2 ? ac.snippet((i.insertText ?? i.textEdit?.newText ?? i.label).replace(/\$\{?(\d+)\}?/g, '#{$1}')) : i.textEdit?.newText ?? i.insertText ?? i.label),
        })),
      };
    }
    /** A completion item's side panel: the shared macro card (not for a `key`), else its resolved documentation. */
    async function itemInfo(i, key) {
      const name = `\\${i.label}`, rs = key ? [] : await explainRecords(name).catch(() => []);
      let html = key ? '' : macroCard(name, null, rs);
      if (!html) {
        const r = await lsp('completionItem/resolve', i).catch(() => null), d = r?.documentation;
        const md = typeof d === 'string' ? d : d?.value;
        html = md ? card(md) : '';
      }
      return html ? Object.assign(document.createElement('div'), { className: 'hover cm-item-card', innerHTML: html }) : null;
    }
    /** An item whose command reopens completion (a key family's `/demo/`): inserted, then completion opens on its keys. */
    function again(item, apply) {
      if (item.command?.command === 'editor.action.triggerParameterHints') return (v, c, from, to) => {
        if (typeof apply === 'function') apply(v, c, from, to);
        else v.dispatch({ changes: { from, to, insert: apply }, selection: { anchor: from + apply.length }, userEvent: 'input.complete' });
        setTimeout(() => v.dom.dispatchEvent(new CustomEvent('satex-signature', { bubbles: true })), 0);
      };
      if (item.command?.command !== 'editor.action.triggerSuggest') return apply;
      return (v, c, from, to) => {
        if (typeof apply === 'function') apply(v, c, from, to);
        else v.dispatch({ changes: { from, to, insert: apply }, selection: { anchor: from + apply.length }, userEvent: 'input.complete' });
        setTimeout(() => ac.startCompletion(v), 0);
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
        .then((r) => { if (key && r) keptHovers.set(key, r); return r; });
      const result = running && keptHovers.has(key) ? keptHovers.get(key) : await asking;
      const c = result?.contents;
      let value = typeof c === 'string' ? c : c?.value ?? (Array.isArray(c) ? c.map((x) => x.value ?? x).join('\n') : '');
      // `[Edit table…](command:satex.openDialog?…)`: the dialog's link, a button here
      let opens = '';
      value = value.replace(/\[([^\]]+)\]\(command:satex\.openDialog\?([^ )]+)[^)]*\)(\n\n---\n\n)?/g, (m, title, q) => {
        const pos = JSON.parse(decodeURIComponent(q))?.[0]?.position;
        opens += `<p class="sd-hover-open"><a href="#" class="sd-open" data-position='${escape(JSON.stringify(pos))}'>${escape(title)}</a></p>`;
        return '';
      }).replace(/(?: · )?\[[^\]]+\]\(command:satex\.openView\?[^)\s]+\)/g, '').trim(); // a widget's link: its button below
      const p = position(v.state.doc, pos), found = findingsAt(p.line, p.character);
      const tools = value && !running ? await typedTools(p) : [];
      // a command: the same card as the Explain panel, the server's Markdown until its records are there
      const macro = word?.[3] && !running ? macroCard(`\\${word[2]}`, p, await Promise.race([explainRecords(`\\${word[2]}`), new Promise((r) => setTimeout(() => r([]), 1500))])) : '';
      if (!value && !opens && !found.length) return null;
      // the card stays over the word it is of: in a finding's range, each word has its own hover
      const range = result?.range, [from, to] = range ? [at(v.state.doc, range.start), at(v.state.doc, range.end)]
        : word ? [line.from + word[0], line.from + word[1]] : found.length ? span(found[0][0].range) : [pos, pos];
      const widgets = tools.filter((t) => t[3]);
      const buttons = widgets.length ? `<div class="hover-tools">${widgets.map(([label, , i]) => `<button type="button" class="small ghost" data-tool="${i}">${escape(label)}</button>`).join('')}</div>` : '';
      const body = macro || (value ? card(value) : '');
      return { from, to, html: (hoverFixes.length = 0, found).map(([d]) => findingCard(d, true) + explanation(d)).join('') + (body || opens ? `<div class="hover">${opens}${body}${buttons}</div>` : '') };
    }
    // Another file or project replaces the whole text: every range-based decoration field starts empty, in this one place
    // (a mapped range would otherwise stretch over the new text).
    const reset = st.StateEffect.define();
    const resetting = (tr) => tr.effects.some((e) => e.is(reset));
    // Ctrl (Cmd) held over a command underlines it as a link to its definition.
    const link = st.StateEffect.define();
    const linkField = st.StateField.define({
      create: () => view.Decoration.none,
      update: (deco, tr) => tr.effects.reduce((d, e) => e.is(link) ? (e.value ? view.Decoration.set([view.Decoration.mark({ class: 'cm-deflink' }).range(...e.value)]) : view.Decoration.none) : d, resetting(tr) ? view.Decoration.none : deco.map(tr.changes)),
      provide: (f) => view.EditorView.decorations.from(f),
    });
    // The places Find references lists, marked in the open file.
    const marks = st.StateEffect.define(), flashes = st.StateEffect.define();
    const markField = st.StateField.define({
      create: () => view.Decoration.none,
      update: (deco, tr) => tr.effects.reduce((d, e) => e.is(marks) ? view.Decoration.set(e.value.map(([f, t]) => view.Decoration.mark({ class: 'cm-refmark' }).range(f, t)), true) : d, resetting(tr) ? view.Decoration.none : deco.map(tr.changes)),
      provide: (f) => view.EditorView.decorations.from(f),
    });
    // A finding's place, flashed for a moment when its row is clicked; never a selection.
    const flashField = st.StateField.define({
      create: () => view.Decoration.none,
      update: (deco, tr) => tr.effects.reduce((d, e) => e.is(flashes) ? view.Decoration.set(e.value.map(([f, t, cls = 'cm-flash']) => view.Decoration.mark({ class: cls }).range(f, t)), true) : d, resetting(tr) ? view.Decoration.none : deco.map(tr.changes)),
      provide: (f) => view.EditorView.decorations.from(f),
    });
    // A swatch before each color the server resolved (`textDocument/documentColor`), as editors show them.
    class Swatch extends view.WidgetType {
      constructor(css) { super(); this.css = css; }
      eq(o) { return o.css === this.css; }
      toDOM(v) {
        const dot = Object.assign(document.createElement('span'), { className: 'cm-swatch', title: `${this.css}: click to pick a color`, style: `background:${this.css}` });
        dot.onmousedown = (e) => { e.preventDefault(); openViews(position(v.state.doc, v.posAtDOM(dot)), 'color'); };
        return dot;
      }
    }
    const swatches = st.StateEffect.define();
    const swatchField = st.StateField.define({
      create: () => view.Decoration.none,
      update: (deco, tr) => tr.effects.reduce((d, e) => e.is(swatches) ? view.Decoration.set(e.value.map(([at, css]) => view.Decoration.widget({ widget: new Swatch(css), side: -1 }).range(at)), true) : d, resetting(tr) ? view.Decoration.none : deco.map(tr.changes)),
      provide: (f) => view.EditorView.decorations.from(f),
    });
    // A code lens above each table, figure and drawing (`textDocument/codeLens`): the button that opens its dialog.
    class Lens extends view.WidgetType {
      constructor(lens) { super(); this.lens = lens; }
      eq(o) { return o.lens.command.title === this.lens.command.title && o.lens.range.start.line === this.lens.range.start.line; }
      toDOM() {
        const c = this.lens.command, line = this.lens.range.start.line;
        const b = Object.assign(document.createElement('button'), { type: 'button', className: 'cm-lens', textContent: c.title, title: `${c.title.replace('…', '')}: open the dialog` });
        b.onclick = (e) => { e.preventDefault(); if (c.command === 'satex.openView') openViews(c.arguments[1], c.arguments[2]); else openDialog(c.arguments?.[0]?.position ?? { line, character: 0 }); };
        const div = Object.assign(document.createElement('div'), { className: 'cm-lens-line' });
        const indent = /^\s*/.exec(lineOf(ev.state.doc, line).text)[0];
        div.append(indent.replace(/\t/g, '    '), b);
        return div;
      }
      ignoreEvent() { return false; }
    }
    const lensSet = st.StateEffect.define();
    const lensField = st.StateField.define({
      create: () => view.Decoration.none,
      update: (deco, tr) => tr.effects.reduce((d, e) => e.is(lensSet) ? view.Decoration.set(e.value.map(([at, l]) => view.Decoration.widget({ widget: new Lens(l), side: -1, block: true }).range(at)), true) : d, deco.map(tr.changes)),
      provide: (f) => view.EditorView.decorations.from(f),
    });
    // A `\begin` name and its `\end` name, from the server's linked editing:
    // while the cursor is in one, what is typed there is typed in both.
    const linkedSet = st.StateEffect.define();
    const linkedField = st.StateField.define({
      create: () => null,
      update: (r, tr) => tr.effects.reduce((v, e) => e.is(linkedSet) ? e.value : v,
        resetting(tr) ? null : r && tr.docChanged ? r.map(([f, t]) => [tr.changes.mapPos(f, -1), tr.changes.mapPos(t, 1)]) : r),
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
    const heatField = st.StateField.define({ create: () => null, update: (h, tr) => tr.effects.reduce((v, e) => e.is(heatSet) ? e.value : v, tr.docChanged || resetting(tr) ? null : h) });
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
    // A marker in the gutter on each line a widget applies to: the lenses' targets and the colors.
    const WIDGET_ICON = {
      table: '<rect x="2.5" y="3" width="11" height="10" rx="1.5"/><path d="M2.5 6.5h11M2.5 9.8h11M6.2 3v10"/>',
      image: '<rect x="2.5" y="3" width="11" height="10" rx="1.5"/><circle cx="6" cy="6.5" r="1.1"/><path d="M2.8 12l3.7-3.6 2.4 2.3 1.7-1.6 2.6 2.6"/>',
      float: '<rect x="2.5" y="2.5" width="11" height="11" rx="1.5"/><rect x="5" y="5" width="6" height="3.2" rx=".6"/><path d="M5 10.8h6"/>',
      tikz: '<circle cx="4" cy="11.5" r="1.6"/><circle cx="12" cy="4.5" r="1.6"/><path d="M5.3 10.4l5.4-4.8"/>',
      forest: '<circle cx="8" cy="3.8" r="1.5"/><circle cx="4" cy="12" r="1.5"/><circle cx="12" cy="12" r="1.5"/><path d="M7.2 5.1L4.8 10.6M8.8 5.1l2.4 5.5"/>',
      widget: '<path d="M3 5h10M3 11h10"/><circle cx="6" cy="5" r="1.5"/><circle cx="10.5" cy="11" r="1.5"/>',
    };
    class WidgetMark extends view.GutterMarker {
      constructor(items) { super(); this.items = items; }
      eq(o) { return o.items.length === this.items.length && o.items.every((x, i) => x.key === this.items[i].key); }
      toDOM() {
        const [first] = this.items, more = this.items.length > 1;
        const icon = first.css ? `<span class="cm-wdot" style="background:${first.css}"></span>` : `<svg viewBox="0 0 16 16">${WIDGET_ICON[first.kind] ?? WIDGET_ICON.widget}</svg>`;
        return Object.assign(document.createElement('span'), { className: 'cm-wmark' + (more ? ' more' : ''), innerHTML: icon, title: this.items.map((x) => x.title).join('\n') + (more ? '\nclick: choose' : '') });
      }
    }
    /** The widgets per line: `{kind, title, key, open}` from the lenses and the color swatches. */
    let widgetMemo = [null, null, null];
    const widgetsOf = (s) => {
      if (widgetMemo[0] === s.field(lensField) && widgetMemo[1] === s.field(swatchField)) return widgetMemo[2];
      const lines = new Map(), add = (pos, item) => { const n = s.doc.lineAt(pos).number; if (!lines.get(n)?.some((x) => x.key === item.key)) lines.set(n, [...lines.get(n) ?? [], item]); };
      s.field(lensField).between(0, s.doc.length, (from, _, d) => {
        const c = d.spec.widget.lens.command, line = d.spec.widget.lens.range.start.line;
        if (!/^satex\.open(Dialog|View)$/.test(c.command)) return;
        const kind = c.command === 'satex.openView' ? c.arguments[2] : /table/i.test(c.title) ? 'table' : /placement/i.test(c.title) ? 'float' : 'image';
        add(from, { kind, title: c.title.replace('…', ''), key: `${kind}:${c.title}`, open: () => c.command === 'satex.openView' ? openViews(c.arguments[1], c.arguments[2]) : openDialog(c.arguments?.[0]?.position ?? { line, character: 0 }) });
      });
      s.field(swatchField).between(0, s.doc.length, (from, _, d) => add(from, { kind: 'color', css: d.spec.widget.css, title: `Pick color ${d.spec.widget.css}`, key: `color:${from}`, open: (v) => openViews(position(v.state.doc, from), 'color') }));
      const set = st.RangeSet.of([...lines].sort((a, b) => a[0] - b[0]).map(([n, items]) => new WidgetMark(items).range(s.doc.line(n).from)));
      widgetMemo = [s.field(lensField), s.field(swatchField), set];
      return set;
    };
    const widgetGutter = view.gutter({
      class: 'cm-widgets',
      markers: (v) => widgetsOf(v.state),
      lineMarkerChange: (u) => u.startState.field(lensField) !== u.state.field(lensField) || u.startState.field(swatchField) !== u.state.field(swatchField),
      domEventHandlers: {
        mousedown: (v, line, e) => {
          const items = []; widgetsOf(v.state).between(line.from, line.from, (from, _, m) => { if (from === line.from) items.push(...m.items); });
          if (!items.length || !e.target.closest?.('.cm-wmark')) return false;
          // the window's mousedown closes open menus: the chooser must not see it
          e.preventDefault(); e.stopPropagation();
          if (items.length === 1) items[0].open(v);
          else showMenu(items.map((x) => [x.title + '…', '', () => x.open(v)]), e.clientX, e.clientY);
          return true;
        },
      },
    });
    const theme = new st.Compartment(), lock = new st.Compartment(), lang = new st.Compartment(), setup = new st.Compartment();
    const ev = new EditorView({
      parent, doc: text,
      extensions: [setup.of(basicSetup), closing, lang.of(/\.bib$/.test(state.file) ? tex.bib : tex.language), language.syntaxHighlighting(tex.highlighter), heatGutter, lint.lintGutter({ tooltipFilter: () => [] }), widgetGutter, lint.linter(null, { tooltipFilter: () => [] }), ac.autocompletion({ override: [complete], activateOnTypingDelay: 40, addToOptions: [{ position: 80, render: (c) => {
        // the detail: a usage line colored like the hover's, then where it comes from
        const span = Object.assign(document.createElement('span'), { className: 'cm-completionDetail' });
        const [, use, from] = (c.usage ?? '').match(/^(.*?)(?:\s+(\([^)]*\)))?$/) ?? [];
        span.innerHTML = (use ? cards.usage(use).html : '') + (from ? ` <span class="muted">${escape(from)}</span>` : '');
        // what the item draws (a line width, dash pattern, arrow tip or pattern), from the server
        // a color: its swatch, then its hex as text (no usage line: `#0` is no parameter)
        if (c.color) {
          const swatch = Object.assign(document.createElement('span'), { className: 'cm-swatch', title: c.color });
          swatch.style.background = c.color;
          span.replaceChildren(swatch, Object.assign(document.createElement('code'), { className: 'cm-hex', textContent: c.hex }));
        }
        if (c.preview) span.prepend(Object.assign(document.createElement('img'), { className: 'cm-preview', src: `data:image/svg+xml;utf8,${encodeURIComponent(c.preview)}`, alt: '' }));
        return span;
      } }] }),
        linkField, markField, flashField, swatchField, lensField, linkedField, heatField, mirror, st.Prec.high(closeEnvironment), view.keymap.of([{ key: 'F2', run: () => { renameBox(); return true; } }, { key: 'Shift-F10', run: () => { const c = editor.coords() ?? { left: 40, bottom: 80 }; menuAt(c.left, c.bottom); return true; } }]),
        EditorView.updateListener.of((u) => { if (u.docChanged) { resolves.clear(); changed(); } else if (u.selectionSet) explainSoon(); if (u.docChanged || u.selectionSet) linkAround(u.view); }), theme.of([]), lock.of([])],
    });
    ev.dom.satexView = ev; // doc/site/showcase.mjs reads the text and the places through it
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
      return w && (w[3] || resolves.get(l.from + w[0]) || fileArg(l.text, pos - l.from)) ? [l.from + w[0], l.from + w[1]] : null;
    };
    function resolve(e) {
      const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos == null) return;
      const l = doc().lineAt(pos), w = wordAt(l.text, pos - l.from);
      if (!w || w[3] || resolves.has(l.from + w[0])) return;
      resolves.set(l.from + w[0], false);
      if (fileArg(l.text, pos - l.from)) { resolves.set(l.from + w[0], true); return underline(under(e)); }
      definitionAt(doc().toString(), position(doc(), pos)).then((r) => {
        if (!(Array.isArray(r) ? r.length : r)) return;
        resolves.set(l.from + w[0], true);
        underline(under(e));
      });
    }
    let linked = null;
    const underline = (r) => { if (String(r) !== String(linked)) { linked = r; ev.dispatch({ effects: link.of(r) }); } };
    parent.addEventListener('mousemove', (e) => { const ctrl = e.ctrlKey || e.metaKey; if (ctrl) resolve(e); underline(ctrl ? under(e) : null); const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY }); if (pos != null) explainSoon(position(doc(), pos)); });
    parent.addEventListener('mouseleave', () => underline(null));
    const inRect = (r, x, y) => x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
    let hovered = null, hoverTimer = 0;
    const unhover = () => { clearTimeout(hoverTimer); if (hovered) { hovered = null; tips.card.hide(150); } };
    parent.addEventListener('mousemove', (e) => {
      const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY });
      if (hovered && pos != null && pos >= hovered.from && pos <= hovered.to) return;
      // the way from the word to its card is part of the hover: the card stays while the pointer is on it
      if (hovered?.hull && inRect(hovered.hull, e.clientX, e.clientY)) return;
      unhover();
      if (pos == null || e.buttons) return;
      const version = doc();
      hoverTimer = setTimeout(async () => {
        const c = await hoverCard(ev, pos);
        if (!c || doc() !== version || !parent.matches(':hover')) return;
        const a = ev.coordsAtPos(c.from), b = ev.coordsAtPos(c.to) ?? a;
        if (!a) return;
        hovered = c;
        const word = { left: a.left, top: a.top, right: b.right, bottom: Math.max(a.bottom, b.bottom) };
        tips.card.show(word, c.html);
        const k = tips.card.el.getBoundingClientRect();
        c.hull = { left: Math.min(word.left, k.left), top: Math.min(word.top, k.top), right: Math.max(word.right, k.right), bottom: Math.max(word.bottom, k.bottom) };
      }, 250);
    });
    parent.addEventListener('mouseleave', unhover);
    // After a command is accepted: its signature, the argument being typed marked (`textDocument/signatureHelp`).
    parent.addEventListener('satex-signature', async () => {
      const head = ev.state.selection.main.head, help = await lsp('textDocument/signatureHelp', { textDocument: { uri: uri() }, position: position(ev.state.doc, head) }).catch(() => null);
      const sig = help?.signatures?.[help.activeSignature ?? 0], c = ev.coordsAtPos(head);
      if (!sig || !c) return;
      const p = sig.parameters?.[help.activeParameter ?? sig.activeParameter ?? 0]?.label;
      const label = escape(sig.label), mark = typeof p === 'string' ? escape(p) : Array.isArray(p) ? escape(sig.label.slice(p[0], p[1])) : null;
      const doc = typeof sig.documentation === 'string' ? sig.documentation : sig.documentation?.value;
      tips.card.show({ left: c.left, top: c.top, right: c.right, bottom: c.bottom },
        `<div class="hover"><p><code>${mark ? label.replace(mark, `<b class="sig-active">${mark}</b>`) : label}</code></p>${doc ? card(doc) : ''}</div>`);
      hovered = { from: head, to: head };
    });
    // a modifier alone (a screenshot shortcut, ctrl for the link) keeps the card
    parent.addEventListener('keydown', (e) => ['Control', 'Meta', 'Shift', 'Alt', 'OS', 'Super'].includes(e.key) || unhover());
    addEventListener('keyup', (e) => (e.key === 'Control' || e.key === 'Meta') && underline(null));
    parent.addEventListener('mousedown', (e) => {
      if (!(e.ctrlKey || e.metaKey) || e.button !== 0 || !under(e)) return;
      e.preventDefault();
      const pos = ev.posAtCoords({ x: e.clientX, y: e.clientY });
      ev.dispatch({ selection: { anchor: pos } });
      definition(e.clientX, e.clientY);
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
    let holdTimer = 0;
    return {
      text: () => doc().toString(),
      // another file's text starts at its top: the last file's scroll position may lie past its end
      // another file: its text without the last one's undo history (an undo never brings another file's text back)
      set: (t, readOnly = false) => { ev.dispatch({ effects: [setup.reconfigure([]), lang.reconfigure(/\.bib$/.test(state.file) ? tex.bib : tex.language)] }); ev.dispatch({ changes: { from: 0, to: doc().length, insert: t }, selection: { anchor: 0 }, effects: [reset.of(null), lock.reconfigure(readOnly ? [st.EditorState.readOnly.of(true)] : [])] }); ev.dispatch({ effects: setup.reconfigure(basicSetup) }); ev.scrollDOM.scrollTop = 0; ev.dispatch({ effects: lensSet.of(textLenses(t).map((l) => [doc().line(l.range.start.line + 1).from, l])) }); },
      apply: (edits) => ev.dispatch({ changes: edits.map((e) => { const [from, to] = span(e.range); return { from, to, insert: e.newText }; }) }),
      diagnostics: (ds) => ev.dispatch(lint.setDiagnostics(ev.state, ds.map((d, i) => {
        const [from, to] = span(d.range);
        return {
          from, to: Math.min(Math.max(to, from + 1), doc().length), severity: sev(d),
          message: d.message, source: d.code,
          renderMessage: () => Object.assign(document.createElement('span'), { innerHTML: inline(d.message) }),
          actions: [...d.fixes.map((f) => ({ name: `Fix (${f.safety})`, apply: () => fix(f) })), { name: 'Explain rule', apply: () => unfold(i, false) }],
        };
      }))),
      complete: () => ac.startCompletion(ev),
      /** The open hover card asked again where it stands: after one of its links rewrote the text. */
      rehover: async () => {
        const was = hovered;
        if (!was) return;
        const c = await hoverCard(ev, was.from);
        const a = c && ev.coordsAtPos(c.from), b = c && (ev.coordsAtPos(c.to) ?? a);
        if (!a || hovered !== was) return;
        hovered = { ...c, hull: was.hull };
        tips.card.show({ left: a.left, top: a.top, right: b.right, bottom: Math.max(a.bottom, b.bottom) }, c.html);
      },
      lenses: (list) => ev.dispatch({ effects: lensSet.of(list.map((l) => [lineOf(doc(), l.range.start.line).from, l]).sort((a, b) => a[0] - b[0])) }),
      colors: (list) => ev.dispatch({ effects: swatches.of(list.map(([p, css]) => [at(doc(), p), css]).filter(([a]) => a != null && a <= doc().length)) }),
      heat: (lines) => ev.dispatch({ effects: heatSet.of(lines?.size ? { lines, most: Math.max(...lines.values()) } : null) }),
      cursor: () => [ev.state.selection.main.anchor, ev.state.selection.main.head],
      scroll: (top) => top == null ? ev.scrollDOM.scrollTop : (ev.scrollDOM.scrollTop = top),
      select: ([anchor, head] = [0, 0]) => { const n = doc().length; ev.dispatch({ selection: { anchor: Math.min(anchor, n), head: Math.min(head, n) }, scrollIntoView: true }); },
      position: () => position(doc(), ev.state.selection.main.head),
      range: () => [position(doc(), ev.state.selection.main.from), position(doc(), ev.state.selection.main.to)],
      selected: () => { const m = ev.state.selection.main; return doc().sliceString(m.from, m.to); },
      word: () => { const l = doc().lineAt(ev.state.selection.main.head); return wordAt(l.text, ev.state.selection.main.head - l.from); },
      coords: () => ev.coordsAtPos(ev.state.selection.main.head),
      mark: (ranges) => ev.dispatch({ effects: marks.of(ranges.map((r) => span(r)).filter(([f, t]) => t > f)) }),
      line: () => { const l = doc().lineAt(ev.state.selection.main.head); return [l.from, l.to]; },
      // the cursor goes to the start; the range only flashes, or is selected with `select`
      // back in the editor where its cursor was (a widget closed)
      refocus: () => ev.focus(),
      focus: (r, flash = false, select = false) => {
        const [from, to] = span(r);
        ev.dispatch({ selection: { anchor: from, ...(select ? { head: to } : {}) }, scrollIntoView: true, effects: flash && to > from ? flashes.of([[from, to]]) : [] });
        ev.focus();
        if (flash) setTimeout(() => ev.dispatch({ effects: flashes.of([]) }), 1300);
      },
      // a place opened from elsewhere (the PDF, a card): marked a moment, then faded; `null` drops it at once
      hold: (r) => {
        clearTimeout(holdTimer);
        const [from, to] = r ? span(r) : [0, 0];
        ev.dispatch({ effects: flashes.of(to > from ? [[from, to, 'cm-hold']] : []) });
        if (to > from) holdTimer = setTimeout(() => ev.dispatch({ effects: flashes.of([]) }), 2800);
      },
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
      position: pos, word: () => { const p = pos(); return wordAt(area.value.split('\n')[p.line], p.character); }, focus: () => area.focus(), refocus: () => area.focus(),
      coords: () => { const r = area.getBoundingClientRect(); return { left: r.left + 8, bottom: r.top + 24 }; },
    };
  }

  // ---------------------------------------------------------------- findings and fixes

  const diagnostics = {}; // uri -> the last list, each with its fixes
  const diagnosticVersions = {}; // uri -> the version of the text they are of
  const key = (d) => `${d.data?.rule ?? d.code}|${d.range.start.line}:${d.range.start.character}|${d.message}`;
  /** What the open file's paragraphs set, from the language server's last
   * analysis (`satex query typeset`); a click goes to the paragraph. */
  const typesetBody = $('typeset-body');
  const notation = self.satexCards.notation;
  // the Typeset panel's layers, kept across visits; what may be set shows unless switched off
  const TS_LAYERS = {
    envs: 'Name the environment each paragraph is set in',
    keys: 'Show the keys of labels, references and citations where they stand',
    boxes: 'Show the text set in boxes that never reach the page',
    glue: 'Mark the glue that may stretch or shrink (\\hfill, \\vfill, \\parskip, \\hskip … plus …): hover one for its spec',
    ligs: 'Mark the ligatures the fonts set: the letters, then the glyph they join to',
    findings: 'Mark the paragraphs with findings; hover a mark for them',
    maybe: 'Show text that may be typeset or not: where the run could not decide a conditional (\\ifdraft), both cases, with a switch per conditional to see one. Off: only text that is surely typeset',
  };
  const tsLegacy = {};
  try { for (const k of Object.keys(TS_LAYERS)) { const v = localStorage.getItem(`satex-typeset-${k}`); if (v != null) tsLegacy[k] = v === '1'; } } catch {}
  const tsOn = self.satexLayers.state('typeset', Object.fromEntries(Object.keys(TS_LAYERS).map((k) => [k, k === 'maybe'])), self.satexStore, tsLegacy);
  $('typeset-layers').innerHTML = self.satexLayers.html(TS_LAYERS, tsOn, { prefix: 'typeset' });
  // the preamble's findings sit on the document's begin mark only while the marks show
  const tsApply = (k) => { typesetBody.classList.toggle(`show-${k}`, tsOn[k]); if (k === 'envs') typesetFindings(); };
  self.satexLayers.wire($('typeset-layers'), 'typeset', tsOn, self.satexStore, tsApply);
  for (const k of Object.keys(TS_LAYERS)) tsApply(k);
  /** The document's outline from the root's run (`satex query occurrences`), in reading order: its
   * sectioning units indented by level, each with its labels, then the bibliography and its entries. */
  // the examples fold away; kept across visits
  const exSect = $('play-exsect');
  try { exSect.open = localStorage.getItem('satex-examples') === '1'; } catch {}
  exSect.addEventListener('toggle', () => { try { localStorage.setItem('satex-examples', exSect.open ? '1' : '0'); } catch {} });
  // A fold's whole head toggles it: a link in a <summary> (an outline row's title) both acts and folds.
  // The browser leaves a click on a link alone, so the fold is toggled here, for every <summary> on the page.
  document.addEventListener('click', (e) => {
    const a = e.target.closest?.('summary a[href="#"]'), d = a?.closest('summary')?.parentElement;
    if (!(d instanceof HTMLDetailsElement)) return;
    e.preventDefault();
    d.open = !d.open;
  });
  const outlineBody = $('outline-body'), outlineOpen = new Set(), outlineFolded = new Set();
  outlineBody.addEventListener('toggle', (e) => { const id = e.target.dataset?.id; if (id) { if (e.target.open) outlineOpen.add(id); else outlineOpen.delete(id); } }, true);
  // a section's fold hides the deeper sections after it; kept across checks
  outlineBody.addEventListener('click', (e) => {
    const b = e.target.closest('.o-fold');
    if (!b) return;
    const id = b.dataset.id;
    if (!outlineFolded.delete(id)) outlineFolded.add(id);
    applyFolds();
  });
  /** Hides the rows under a folded section and flips the fold buttons, on the rows already there. */
  function applyFolds() {
    const stack = [];
    for (const li of outlineBody.querySelectorAll('li.o-sec[data-id]')) {
      const depth = +li.style.getPropertyValue('--d');
      while (stack.length && stack.at(-1)[0] >= depth) stack.pop();
      li.hidden = stack.some(([, id]) => outlineFolded.has(id));
      const folded = outlineFolded.has(li.dataset.id), button = li.firstElementChild;
      if (button?.classList.contains('o-fold')) {
        button.setAttribute('aria-expanded', String(!folded));
        button.title = `${folded ? 'show' : 'hide'} the subsections`;
      }
      stack.push([depth, li.dataset.id]);
    }
  }
  const LEVELS = ['part', 'chapter', 'section', 'subsection', 'subsubsection', 'paragraph', 'subparagraph'];
  /** The document's sections, labels, references and citations of the last check, for the outline and the PDF. */
  var occurrences = []; // var: the PDF code reads it before this line may run
  async function outline() {
    // an edit in an included file leaves the held analysis of the root stale: a config forces a fresh one
    const request = { file: path(state.root), queries: [{ type: 'occurrences' }], ...(state.file !== state.root ? { config: { trace: 'false' } } : {}) };
    const rs = (await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).catch(() => null))?.[0]?.records;
    if (!rs) return;
    // sections in the run's order; labels, citations and the rest come after them, each placed by its source position
    const own = rs.filter((r) => !r.origin || r.origin === 'document'), file = (r) => r.path ?? r.file;
    occurrences = own;
    pdfOverlay();
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
    const stack = [];
    const row = (id, depth, head, body, kids = false) => {
      while (stack.length && stack.at(-1)[0] >= depth) stack.pop();
      const hidden = stack.some(([, f]) => outlineFolded.has(f)), folded = outlineFolded.has(id);
      stack.push([depth, id]);
      return `<li class="o-sec${kids ? ' o-parent' : ''}" data-id="${escape(id)}" style="--d:${depth}"${hidden ? ' hidden' : ''}>${kids ? `<button type="button" class="o-fold" data-id="${escape(id)}" aria-expanded="${!folded}" title="${folded ? 'show' : 'hide'} the subsections"></button>` : ''}` + (body
      ? `<details class="o-row" data-id="${escape(id)}"${outlineOpen.has(id) ? ' open' : ''}><summary>${head}</summary>${body}</details>` : `<div class="o-row o-flat">${head}</div>`) + '</li>';
    };
    const defOf = new Map(own.filter((r) => r.kind === 'label' || r.kind === 'bibitem').map((r) => [`${r.kind === 'label' ? 'label' : 'cite'}:${r.key}`, r]));
    const chips = (list) => list.length ? `<span class="o-labels">${list.join('')}</span>` : '';
    const close = () => {
      if (!open) return;
      const files = [...open.files].filter((f) => /\.(tex|ltx)$/.test(f) && state.files.some((n) => String(f).endsWith(n)));
      const cites = [...open.cites.entries()].map(([k, c]) => `<a href="#" class="o-go o-label o-use" ${at(defOf.get(`cite:${k}`) ?? c[0])} title="cited ${c.length}× in this part${defOf.has(`cite:${k}`) ? ', click for its entry' : ''}">${escape(k)}</a>`);
      const fileChips = files.length > 1 || (files.length && !String(files[0]).endsWith(state.root)) ? files.map((f) => `<span class="o-file" title="a file this part is set from">${escape(base(f))}</span>`) : [];
      html.push(row(open.id, open.depth, open.head + (open.first ? `<span class="o-first" title="label ${escape(open.first)}">${escape(open.first)}</span>` : ''), chips(open.labels) + chips(fileChips) + chips(cites), open.kids));
      open = null;
    };
    const placed = own.filter((r) => r.kind !== 'section').map((r) => [r, owner(r)]);
    const inOrder = [...placed.filter(([, o]) => o < 0).map(([r]) => r), ...heads.flatMap(([s, i]) => [s,
      ...placed.filter(([, o]) => o === i).map(([r]) => r).sort((a, b) => file(a) === file(b) ? (before(a, b) ? -1 : 1) : 0)])];
    for (const r of inOrder) {
      if (r.kind === 'section') {
        const depth = Math.max(0, present.indexOf(LEVELS.indexOf(r.detail))), num = numberOf(r);
        if (open && depth > open.depth) open.kids = true;
        close();
        open = { first: null, labels: [], cites: new Map(), files: new Set([r.path ?? r.file]), id: `${r.path ?? r.file}:${r.line}`, depth,
          head: `<a href="#" class="o-go" ${at(r)} title="${escape(r.detail)} at ${escape((r.file ?? '') + ':' + r.line)}">${num ? `<span class="o-num">${num}</span>` : ''}${escape(r.key)}</a>` };
      } else if (open && r.kind !== 'bibitem') {
        open.files.add(r.path ?? r.file);
        if (r.kind === 'cite') open.cites.set(r.key, [...(open.cites.get(r.key) ?? []), r]);
      }
      if (r.kind === 'label') {
        const chip = `<a href="#" class="o-go o-label o-def" ${at(r)} title="label at ${escape((r.file ?? '') + ':' + r.line)}">${escape(r.key)}</a>`;
        if (open) { open.labels.push(chip); open.first ??= r.key; } else html.push(`<li class="o-sec" style="--d:0"><span class="o-labels">${chip}</span></li>`);
      }
    }
    close();
    // the references: one fold per kind of key, each key at its definition with the places that use it
    const cats = new Map(), keyCat = new Map();
    const cat = (id, title) => { if (!cats.has(id)) cats.set(id, { title, keys: new Map() }); return cats.get(id); };
    const entry = (c, key) => { if (!c.keys.has(key)) c.keys.set(key, { defs: [], uses: [] }); return c.keys.get(key); };
    const cmd = (r) => String(r.detail ?? '').replace(/^\\/, '');
    for (const r of own) {
      if (r.kind === 'bibitem') entry(cat('cite', 'citations'), r.key).defs.push(r);
      else if (r.kind === 'cite') entry(cat('cite', 'citations'), r.key).uses.push(r);
      else if (r.kind === 'label') entry(cat('label', 'labels'), r.key).defs.push(r);
      else if (r.kind === 'ref') entry(cat('label', 'labels'), r.key).uses.push(r);
      else if (r.kind === 'key') { const c = cat(`key:${cmd(r)}`, `keys of \\${cmd(r)}`); keyCat.set(r.key, c); entry(c, r.key).defs.push(r); }
    }
    for (const r of own) {
      if (r.kind === 'key-use') entry(keyCat.get(r.key) ?? cat('key:', 'other keys'), r.key).uses.push(r);
      else if (r.kind === 'entry') entry(cat(`entry:${cmd(r)}`, `\\${cmd(r)} lines`), r.key).uses.push(r);
    }
    const place = (r) => `${escape(base(r.file ?? r.path))}:${r.line}`;
    const catHtml = [...cats].map(([id, c]) => {
      const keys = [...c.keys].sort((x, y) => x[0].localeCompare(y[0])).map(([key, k]) => {
        const first = k.defs[0];
        const head = first ? `<a href="#" class="o-go o-label o-def" ${at(first)} title="defined at ${place(first)}">${escape(key)}</a>` : `<span class="o-label o-undef" title="no definition found">${escape(key)}</span>`;
        const uses = k.uses.map((u) => `<a href="#" class="o-go o-label o-use" ${at(u)} title="used at ${place(u)}">${u.line}</a>`).join('');
        return `<li class="o-key">${head}<span class="o-uses" title="the lines that use it">${uses || '<span class="muted">unused</span>'}</span></li>`;
      }).join('');
      const fid = `ref:${id}`;
      return `<details class="o-cat" data-id="${escape(fid)}"${outlineOpen.has(fid) ? ' open' : ''}><summary>${escape(c.title)} <b class="count">${c.keys.size}</b></summary><ul class="o-keys">${keys}</ul></details>`;
    }).join('');
    const where = own.find((r) => r.kind === 'bibliography');
    if (cats.size) html.push(row('references', 0, `<a href="#" class="o-go"${where ? ` ${at(where)}` : ''}><span class="o-kind">references</span>${cats.size === 1 ? '1 kind' : `${cats.size} kinds`} of key</a>`, catHtml));
    $('outline-count').textContent = String(sections.length || '');
    const outlineHtml = html.length ? `<ul class="outline-list">${html.join('')}</ul>` : '<p class="muted">No sections, labels or bibliography yet.</p>';
    // an unchanged outline keeps its DOM: no rebuild, no lost hover or scroll
    if (outlineHtml === outlineShown) return;
    outlineShown = outlineHtml;
    outlineBody.innerHTML = outlineHtml;
  }
  let outlineShown = '';
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
  /** The open file's paragraphs in the view, a folded file's and the cursor's excerpts not. */
  const tsOwn = () => [...typesetBody.querySelectorAll('p.ts[data-line]:not(.ts-excerpt)')].filter((p) => !p.closest('.ts-other'));
  /** The paragraphs the selection (else the cursor) touches, marked; with "only at cursor" the others hidden. */
  function typesetFollow() {
    const [from, to] = editor?.range?.() ?? [editor?.position(), editor?.position()];
    if (!from) return;
    // a selection that ends at a line's start does not take that line
    const first = from.line + 1, last = to.line + 1 - Number(to.line > from.line && to.character === 0);
    const own = tsOwn();
    const here = own.filter((p, i) => +p.dataset.line <= last && (i + 1 >= own.length || +own[i + 1].dataset.line > first));
    for (const p of typesetBody.querySelectorAll('p.ts')) p.classList.toggle('ts-current', here.includes(p));
    const only = tsHere;
    typesetBody.classList.toggle('ts-only-here', only);
    // nothing set where the cursor stands: said so, not the whole document
    typesetBody.querySelector('.ts-nothing')?.remove();
    typesetBody.querySelectorAll('.ts-excerpt').forEach((x) => x.remove());
    if (only && !here.length) typesetBody.insertAdjacentHTML('afterbegin', '<p class="muted ts-nothing">Nothing is set at the cursor.</p>');
    else if (only) for (const p of here) p.insertAdjacentHTML('afterend', excerpt(p, from, to));
    typesetBody.classList.toggle('ts-excerpted', only && !!typesetBody.querySelector('.ts-excerpt'));
  }
  /** Where `needle` stands in `plain` nearest to `near`, soft hyphens between its letters allowed: [from, to], else null. */
  const nearest = (plain, needle, near) => {
    const hit = needle ? [...plain.matchAll(new RegExp([...needle].map((ch) => ch.replace(/[\\^$.*+?()[\]{}|]/g, '\\$&')).join('·?'), 'g'))]
      .sort((x, y) => Math.abs(x.index - near) - Math.abs(y.index - near))[0] : null;
    return hit ? [hit.index, hit.index + hit[0].length] : null;
  };
  /** The selected part of the paragraph's text (else the word at the cursor), with a word of it before and after in grey.
   * The run's line marks (`lines`: where each source line starts in the text) place a selection; within a line, its share. */
  function excerpt(p, from, to) {
    const plain = p.dataset.plain ?? '';
    if (!plain) return '';
    const src = editor.text().split('\n'), start = +p.dataset.line - 1;
    const after = tsOwn(), k0 = after.indexOf(p);
    const next = after[k0 + 1] ? +after[k0 + 1].dataset.line - 1 : src.length;
    const marks = [{ line: start + 1, at: 0 }, ...JSON.parse(p.dataset.lines || '[]').filter((m) => m.line > start + 1 && m.line <= next)].sort((x, y) => x.line - y.line || x.at - y.at);
    /** A source position as an offset in the text: its line's mark, then its share of the source up to the next mark. */
    const offset = ({ line, character }) => {
      if (line < start) return 0;
      if (line >= next) return plain.length;
      const k = marks.findLastIndex((m) => m.line - 1 <= line), m = marks[k], end = marks[k + 1] ?? { line: next + 1, at: plain.length };
      const span = src.slice(m.line - 1, end.line - 1).join('\n'), before = src.slice(m.line - 1, line).join('\n').length + (line > m.line - 1 ? 1 : 0) + character;
      return Math.round(m.at + (end.at - m.at) * Math.min(before / Math.max(span.length, 1), 1));
    };
    let range = null;
    if (from.line !== to.line || from.character !== to.character) {
      let [a, b] = [offset(from), offset(to)];
      // whole words: from the start of the first to the end of the last
      a = plain.slice(0, a).search(/\S*$/);
      b += plain.slice(b).search(/\s|$/);
      range = [a, b];
      // a selection on one line keeps its own words where they stand in the text
      const sel = (editor.selected?.() ?? '').trim();
      if (from.line === to.line) range = nearest(plain, sel.replace(/^\W+|\W+$/g, '') || sel, a) ?? range;
    } else {
      const w = wordAt(src[from.line] ?? '', from.character), at = offset(from);
      range = nearest(plain, w && !w[3] ? w[2].replace(/^\W+|\W+$/g, '') : '', at);
      if (!range) for (const x of plain.matchAll(/\S+/g)) if (x.index + x[0].length > Math.min(at, plain.length - 1)) { range = [x.index, x.index + x[0].length]; break; }
    }
    // the run's own provenance: every character set while the lines at the cursor were read (a call's whole text)
    const rec = typesetRows?.[+p.dataset.k];
    const lines = [from.line + 1, to.line + 1 - Number(to.line > from.line && to.character === 0)];
    const own = rec && siteHits(rec, state.file, ([l]) => lines[0] <= l && l <= lines[1]);
    if (own?.first >= 0) range = [own.first, own.last + 1];
    const [a, b] = range && range[1] > range[0] ? range : [0, (plain.match(/^\s*\S+/)?.[0] ?? plain).length];
    const word = (t) => t.match(/\S+\s*$/)?.[0] ?? '', then = (t) => t.match(/^\s*\S+/)?.[0] ?? '';
    const [pre, post] = [word(plain.slice(0, a)), then(plain.slice(b))];
    return `<p class="ts ts-excerpt" style="${escape(p.getAttribute('style') ?? '')}"><span class="ts-at">${p.dataset.line}</span><span class="ts-rails" aria-hidden="true"></span><span class="ts-text"><span class="ts-grey">${a > pre.length ? '… ' : ''}${escape(pre)}</span><mark>${escape(plain.slice(a, b))}</mark>` +
      `<span class="ts-grey">${escape(post)}${b + post.length < plain.length ? ' …' : ''}</span></span></p>`;
  }
  /** 'all' or 'at cursor': one state, the buttons' aria-pressed drawn from it (a hidden checkbox the browser restores was a second one). */
  var tsHere = false; // var: a cursor move may run typesetFollow before this line
  const tsScope = (here) => {
    tsHere = here;
    self.satexStore.set('typeset-scope', here ? 'here' : null);
    for (const x of document.querySelectorAll('.ts-scope button')) x.setAttribute('aria-pressed', String((x.dataset.scope === 'here') === here));
    typesetFollow();
  };
  for (const b of document.querySelectorAll('.ts-scope button')) b.onclick = () => tsScope(b.dataset.scope === 'here');
  tsScope(self.satexStore.get('typeset-scope') === 'here');
  /** The open file's findings on the paragraphs they stand in: a mark in the margin, the worst first, shown with the findings switch. */
  /** Each finding on the paragraph set from its line: the one whose lines hold it, else (an error) the last one before it, else the document's begin, else the first;
   * a mark's click goes to its first finding. */
  var tsMarked = []; // var: the envs switch runs typesetFindings before this line
  function typesetFindings() {
    typesetBody.querySelectorAll('.ts-finding').forEach((m) => m.remove());
    tsMarked = [];
    const rows = [...typesetBody.querySelectorAll('p.ts[data-line]:not(.ts-excerpt)')];
    const uriOf = (p) => { const o = p.closest('.ts-other'); return o ? Object.keys(diagnostics).find((u) => p.dataset.path && (p.dataset.path.endsWith(u.split('/').pop()))) : uri(); };
    const by = new Map();
    for (const p of rows) { const u = uriOf(p); if (u) by.set(u, [...by.get(u) ?? [], p]); }
    const on = new Map();
    for (const [u, ps] of by) for (const d of diagnostics[u] ?? []) {
      const n = d.range.start.line + 1, line = (p) => +p.dataset.line;
      const holds = ps.filter((p) => line(p) <= n && n <= +(p.dataset.last ?? p.dataset.line));
      const before = ps.filter((p) => line(p) <= n);
      // a finding outside every paragraph's lines: an error only, on the paragraph set before it
      // one before every paragraph (the preamble's) on the document's begin mark when the view has one
      const p = holds.sort((a, b) => line(b) - line(a))[0] ?? (sev(d) === 'error' ? before.sort((a, b) => line(b) - line(a))[0] ?? (u === uri() && typesetBody.classList.contains('show-envs') && typesetBody.querySelector('.ts-envmark.ts-env-begin[data-env="document"]')) ?? ps[0] : null);
      if (p) on.set(p, [...on.get(p) ?? [], { ...d, target: u }]);
    }
    for (const [p, fs] of on) {
      const worst = ['error', 'warning', 'info', 'hint'].find((v) => fs.some((d) => sev(d) === v));
      tsMarked.push(fs);
      p.querySelector('.ts-at')?.insertAdjacentHTML('afterbegin', `<span class="ts-finding f-${worst}" data-f="${tsMarked.length - 1}" title="${escape(fs.map((d) => `${d.range.start.line + 1}: ${d.code}: ${d.message}`).join('\n'))}\nclick: go to it">${fs.length}</span>`);
    }
  }
  const withKeys = cards.withKeys;
  /** The paragraphs of the last typeset, with the source site of their characters: the PDF's findings mark only theirs. */
  var typesetRows = null;
  async function typeset(target, of = diagnosticVersions[target]) {
    if (target !== uri()) return;
    // a step of its own in the status card, when nothing else is going on
    const alone = !act.phase;
    if (alone) busy('Typesetting', 'Typesetting the text…');
    const rows = await lsp('satex/typeset', { textDocument: { uri: target } }).catch(() => null);
    if (alone && act.phase === 'Typesetting') idle();
    if (!rows || target !== uri()) return;
    typesetRows = rows;
    shownFor(typesetBody, target, of);
    if (pdfOverlayOn?.findings || pdfOverlayOn?.glue) pdfOverlay();
    const lines = state.texts[state.file]?.split('\n') ?? [];
    const html = cards.typesetView(rows, { file: state.file, closed: tsClosed, plural, lineOf: (n) => lines[n - 1] });
    typesetBody.innerHTML = html || '<p class="muted">The document sets no paragraph.</p>';
    typesetFindings();
    typesetFollow();
  }
  cards.typesetWire(typesetBody);
  typesetBody.addEventListener('click', (e) => {
    const mark = e.target.closest('.ts-finding');
    if (mark) {
      const [d] = tsMarked[+mark.dataset.f] ?? [];
      if (!d) return;
      e.stopPropagation();
      const name = d.target === uri() ? null : Object.keys(state.texts).find((n) => d.target.endsWith(`/${n}`));
      if (name) openPlace(name, d.range.start, d.range.end); else editor.focus({ start: d.range.start, end: d.range.end });
      return;
    }
    const p = e.target.closest('.ts, .ts-envmark');
    if (!p) return;
    const at = { line: +p.dataset.line - 1, character: Math.max(+(p.dataset.col ?? 1) - 1, 0) };
    if (p.dataset.path && !p.dataset.path.endsWith(state.file)) {
      const name = Object.keys(state.texts).find((n) => !(n in state.ro) && p.dataset.path.endsWith(n));
      openPlace(name ?? p.dataset.path, at);
    } else editor.focus({ start: at, end: at });
  });

  async function show(target, ds, of = null) {
    const actions = await lsp('textDocument/codeAction', { textDocument: { uri: target },
      range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } }, context: { diagnostics: [] } }).catch(() => []);
    // a newer text was sent while the fixes were asked for
    if (outdated({ uri: target, version: of })) return;
    diagnosticVersions[target] = of;
    const by = new Map();
    fixAlls[target] = (actions ?? []).filter((a) => a.data?.scope);
    for (const a of (actions ?? []).filter((x) => !x.data?.scope)) for (const d of a.diagnostics ?? []) {
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
    // Findings go on the editor's text only when they are of it: else the editor keeps the last ones, moved along with its edits.
    const of = diagnosticVersions[uri()];
    if (!typedSincePush && (of == null || of === sentVersions.get(uri()))) editor.diagnostics(ds);
    const others = Object.entries(diagnostics).filter(([u]) => u !== uri()).flatMap(([u, rs]) => rs.map((d) => ({ ...d, file: u.split('/').pop(), target: u })));
    listed = [...ds, ...others];
    if (retryCompletion && !running) { retryCompletion = false; asked.clear(); editor.complete?.(); }
    count.textContent = listed.length;
    shownFor(list, uri(), diagnosticVersions[uri()]);
    shownFor(count, uri(), diagnosticVersions[uri()]);
    pills(ds);
    if (checkStart) { timing('t-check', secs(performance.now() - checkStart), 'the language server analyzed the open file'); shownTime(secs(performance.now() - checkStart)); }
    checkStart = 0;
    phase();
    $('fix-all').innerHTML = editableFile() ? fixAllButton('all') : '';
    // the unsafe switch only where there is something to fix
    document.querySelector('.fix-bar').hidden = !$('fix-all').innerHTML;
    lspStatus.textContent = state.file in state.ro ? 'read-only file' : provisional ? `provisional: ${provisional} still downloading; the check runs again once ${provisional.includes(', ') ? 'they are' : 'it is'} in` : listed.length ? '' : 'none';
    list.classList.toggle('provisional', !!provisional);
    markStale();
    if (pdfOverlayOn?.findings) pdfOverlay();
    typesetFindings();
    const fixButton = (f, j) => `<button class="small apply fix-${f.safety}" data-j="${j}" title="${escape(f.title)}">Fix (${f.safety})</button>`;
    const item = (d, i) => `<li class="f-${sev(d)}${unfolded.has(key(d)) ? ' open' : ''}" data-i="${i}" tabindex="0">` +
      `<span class="msg" title="${escape(d.message ?? '')}">${d.number ? `<code class="rno" title="rule number">${escape(d.number)}</code> ` : ''}<b class="rn">${escape(d.code ?? '')}</b>: ${commandRefs(inline(d.message))}</span>` +
      `<span class="where"><b class="loc">${d.file ? `<span class="loc-file">${escape(d.file)}:</span>` : ''}${d.range.start.line + 1}:${d.range.start.character + 1}</b><span class="shape" title="${sev(d)}" aria-label="${sev(d)}"></span></span>` +
      `<span class="acts">${d.fixes.map(fixButton).join('')}${!d.file && editableFile() ? fixAllButton('rule', d.code, `Fix all ${d.code}`) : ''}<button class="small ghost why">${unfolded.has(key(d)) ? 'Hide rule' : 'Explain rule'}</button></span>` +
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
        `<span class="cat-sevs" aria-hidden="true">${sevs.map(([s, n]) => `<span class="cat-sev sev-${s}" title="${n} ${s}">${n}</span>`).join('')}</span>${editableFile() ? fixAllButton('category', cat) : ''}`);
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
  const editableFile = () => !(state.file in state.ro);
  // a Fix all in a category's head acts, it does not fold the category
  for (const box of [list, $('fix-all')]) box.addEventListener('click', (e) => {
    const b = e.target.closest('.fix-all');
    if (!b) return;
    e.preventDefault(); e.stopPropagation();
    fixAll(b.dataset.key || undefined, fixUnsafe, b.dataset.scope);
  }, true);
  $('fix-unsafe').onchange = (e) => { fixUnsafe = e.target.checked; render(); };
  list.onclick = (e) => {
    const li = e.target.closest('li[data-i]');
    if (!li) return;
    const d = listed[li.dataset.i];
    const apply = e.target.closest('.apply');
    if (apply) return fix(d.fixes[apply.dataset.j]);
    if (e.target.closest('.explain a')) return;
    // another file's finding opens that file at its place
    if (d.target && !e.target.closest('.why')) return openPlace(Object.keys(state.texts).find((n) => d.target.endsWith(`/${n}`)) ?? d.file, d.range.start);
    if (!e.target.closest('.why')) editor.focus(d.range, true);
    unfold(+li.dataset.i);
  };
  function fix(action) {
    const changes = { ...action.edit?.changes };
    for (const d of action.edit?.documentChanges ?? []) if (d.edits) changes[d.textDocument.uri] = d.edits;
    const files = Object.keys(changes).map((u) => u.split('/').pop());
    // an edit of one other file (a manager writing refs.bib): that file opens first and takes it through the
    // editor, so one Ctrl+Z there undoes it
    const other = Object.keys(changes).length === 1 && !(uri() in changes) && Object.keys(state.texts).find((n) => uri(n) in changes && !(n in state.ro));
    if (other) choose(other);
    if (!applyEdit(action.edit)) return toast(files.length ? `Not applied: it edits ${files.join(', ')}` : 'Not applied: the fix has no edit');
    // a fix that edits other files only: said where, and that file opened at the edit
    const elsewhere = Object.keys(changes).filter((u) => u !== uri());
    toast(`Applied: ${action.title}${elsewhere.length ? ` (in ${elsewhere.map((u) => u.split('/').pop()).join(', ')})` : ''}`);
    const away = !(uri() in changes) && elsewhere[0], name = away && Object.keys(state.texts).find((n) => uri(n) === away);
    if (name) openPlace(name, changes[away][0].range.start);
  }
  /** What the type plugins offer at `p` (src/lsp/plugins): the widgets (their views, `satex/view`, and
   * the actions that open a dialog) and the rewrites, as [label, hint, index, widget] rows; `runTool(index)` opens or applies one. */
  let tools = [], viewPlugin = null;
  async function typedTools(p) {
    const doc = { uri: uri() };
    const [views, actions] = await Promise.all([
      lsp('satex/view', { textDocument: doc, position: p }).catch(() => []),
      lsp('textDocument/codeAction', { textDocument: doc, range: { start: p, end: p }, context: { diagnostics: [] } }).catch(() => []),
    ]);
    const own = (actions ?? []).filter((a) => !a.diagnostics?.length && !a.data?.scope);
    // an action that only opens a dialog (`satex.openDialog`, VS Code's `satex.openView`) is a widget here
    const opens = own.filter((a) => !a.edit && /^satex\.open(Dialog|View)$/.test(a.command?.command ?? ''));
    tools = [...(views ?? []).map((view) => ({ view, target: doc.uri })), ...opens.map((open) => ({ open: open.command })), ...own.filter((a) => a.edit).map((action) => ({ action }))];
    // a title that names its action already (`Edit colour brand`) stands as it is
    const verb = (v) => /^Edit\b/.test(v.title) ? '' : VIEW_VERB[v.plugin] ?? 'Edit', title = (v) => verb(v) ? v.title.replace(/^\p{Lu}(?!\p{Lu})/u, (c) => c.toLowerCase()) : v.title;
    const label = (t) => t.view ? `${verb(t.view)} ${title(t.view)}…`.trim() : (t.open ?? t.action).title;
    const seen = new Set();
    return tools.map((t, i) => [label(t), t.view ? t.view.plugin : t.open ? 'dialog' : 'rewrite', i, !t.action]).filter(([l, , , w]) => !w || !seen.has(l) && seen.add(l));
  }
  const VIEW_VERB = { color: 'Pick', units: 'Convert', environment: '', tikz: '', keys: 'Edit' };
  function runTool(i) {
    const t = tools[i];
    if (t?.action) return fix(t.action);
    if (t?.open) return t.open.command === 'satex.openView' ? openViews(t.open.arguments[1], t.open.arguments[2]) : openDialog(t.open.arguments?.[0]?.position);
    if (!t?.view) return;
    const dialog = $('play-view');
    viewPlugin = t.view.plugin;
    viewGuards = [];
    dialog.querySelector(':scope > .sd-discard')?.remove();
    dialog.satexAsk = null;
    $('view-title').textContent = t.view.title;
    self.satexViews.render(t.view, $('view-body'), {
      markdown: (md) => card(md),
      apply: (edits) => { closeView(true); fix({ title: t.view.title, edit: Array.isArray(edits) ? { changes: { [t.target]: edits } } : edits }); },
      close: () => closeView(),
      guard: (dirty) => viewGuards.push(dirty),
    });
    if (!dialog.open) dialog.showModal();
  }
  /** The views' dialog closes, after asking when a view has unwritten changes (`force`: after Apply); the editor gets the focus back. */
  let viewGuards = [];
  async function closeView(force = false) {
    const dialog = $('play-view');
    if (!dialog.open) return;
    if (force !== true && viewGuards.some((dirty) => dirty()) && !(await self.satexDialogs.discard(dialog))) return;
    viewGuards = [];
    dialog.close();
    editor?.refocus?.();
  }
  $('play-view').addEventListener('cancel', (e) => { e.preventDefault(); const d = $('play-view'); if (d.satexAsk) d.satexKeep(); else closeView(); });
  /** The views at `p`, the one of `plugin` opened at once. */
  async function openViews(p, plugin) {
    const rows = await typedTools(p);
    const i = tools.findIndex((t) => t.view?.plugin === plugin);
    if (i >= 0) runTool(i); else if (rows.length) runTool(rows[0][2]);
  }
  // a value of the units hover writes itself in place of the length
  document.addEventListener('click', (e) => {
    const a = e.target.closest('a.cmd-edit');
    if (!a) return;
    e.preventDefault();
    const [target, range, newText] = JSON.parse(decodeURIComponent(a.dataset.args));
    fix({ title: `Convert to ${newText}`, edit: { changes: { [target]: [{ range, newText }] } } });
    // the card shows the new length: every value again, the written unit left out
    editor?.rehover?.();
  });
  // a float's `[ht]` chip in the Typeset view: the server's hover of the placement, letter by letter
  document.addEventListener('mouseover', async (e) => {
    const chip = e.target.closest?.('.ts-block-place');
    if (!chip || chip.dataset.asked) return;
    chip.dataset.asked = '1';
    const line = +chip.closest('.ts-block')?.dataset.line - 1, text = editor?.text().split('\n')[line] ?? '';
    const at = text.indexOf('[', text.indexOf('\\begin{'));
    if (at < 0) return;
    const r = await lsp('textDocument/hover', { textDocument: { uri: uri() }, position: { line, character: at + 1 } }).catch(() => null);
    const v = r?.contents?.value ?? '';
    const plain = v.replace(/\[[^\]]*\]\(command:[^)]*\)\s*/g, '').replace(/\*\*|`/g, '').trim();
    if (plain) chip.title = plain;
  });
  document.addEventListener('click', (e) => { const b = e.target.closest('[data-tool]'); if (b) { e.preventDefault(); runTool(+b.dataset.tool); } });
  // a fix offered on a finding's hover card: applied, the card goes
  document.addEventListener('click', (e) => { const b = e.target.closest('.tip [data-fix]'); const f = b && hoverFixes[+b.dataset.fix]; if (f) { e.preventDefault(); tips.card.hide(); fix(f); } });
  $('view-close').onclick = () => closeView();
  $('cfg-reset-view').onclick = () => resetView();
  // dev mode: traces carry the abstract state of each step (`trace_state`); off, the runs pay nothing for it
  let devMode = false;
  const devShown = (on) => { devMode = on; document.body.classList.toggle('dev-mode', on); $('cfg-dev').checked = on; self.satexStore.set('dev', on || null); };
  devShown(self.satexStore.get('dev') === true);
  $('cfg-dev').onchange = (e) => devShown(e.target.checked);
  // every dialog is the widgets' popup: peek, detach, full screen, kept per dialog (dialogs.js)
  for (const d of document.querySelectorAll('dialog.cache-dialog:not(.ask-dialog)')) self.satexDialogs?.enhance(d);
  /** The open file's fixes at once, as the language server merges them (render.rs `fix_all`): per rule, per category or all. */
  var fixAlls = {}; // var: a render may run before this line
  /** What a fix-all of `scope` (`rule`, `category`, `all`) and `key` applies: the server's action, else the one fix there is. */
  function fixAllOf(scope, key = '', unsafe = fixUnsafe) {
    const mine = (fixAlls[uri()] ?? []).filter((a) => a.data.scope === scope && a.data.key === key);
    const merged = mine.find((a) => a.data.unsafe === unsafe) ?? mine.find((a) => !a.data.unsafe);
    if (merged) return merged;
    const inScope = (d) => scope === 'all' || (scope === 'rule' ? d.code : manifest.rules[d.code]?.category) === key;
    const lone = (diagnostics[uri()] ?? []).filter(inScope).flatMap((d) => d.fixes.filter((f) => unsafe || f.safety === 'safe').slice(0, 1));
    return lone.length === 1 ? { ...lone[0], data: { count: 1 } } : null;
  }
  /** Applies a fix-all as one edit (one undo step); the check runs again on the change. */
  function fixAll(code, unsafe = fixUnsafe, scope = code ? 'rule' : 'all') {
    sync();
    const action = fixAllOf(scope, code ?? '', unsafe), edits = action?.edit?.changes?.[uri()] ?? [];
    const what = unsafe ? 'fix' : 'safe fix';
    if (!edits.length) return toast(`No ${what}es${code ? ` for ${code}` : ''} in this file`);
    editor.apply(edits);
    toast(`Applied ${plural(edits.length, what)}${edits.length < action.data.count ? `; ${action.data.count - edits.length} overlapping wait for the next check` : ''}`);
  }
  /** Whether Fix all also applies the unsafe fixes: off unless asked, as the command line's --unsafe-fixes. */
  var fixUnsafe = false; // var: a render may run before this line
  /** A Fix all button of a scope, with how many findings it fixes; none where nothing is fixable. */
  const fixAllButton = (scope, key = '', label = 'Fix all') => {
    const a = fixAllOf(scope, key);
    return a ? `<button type="button" class="small ghost fix-all" data-scope="${scope}" data-key="${escape(key)}" title="${escape(a.title)}">${escape(label)} <b class="count">${a.data.count}</b></button>` : '';
  };

  // ---------------------------------------------------------------- language server actions: definition, rename, the context menu

  /** The definition of the command at the cursor: a project file's tab, a bundle file read-only, or a
   * primitive's documentation; with several (redefinitions, the kernel's), a menu of them all. */
  /** Navigation never waits behind a run. A file argument (`\\input{chapter}`) names its file: opened from the text alone.
   * A name's definition comes from the last answer for it while the worker is busy; only a name not asked before waits. */
  const FILE_ARG = /\\(?:input|include|subfile|InputIfFileExists|bibliography|addbibresource|includegraphics|addplot\s*(?:\[[^\]]*\])?\s*table)\s*(?:\[[^\]]*\])?\s*\{([^{}]*)\}/g;
  function fileArg(line, col) {
    for (const m of line.matchAll(FILE_ARG)) {
      const from = m.index + m[0].length - m[1].length - 1;
      if (col < from || col > from + m[1].length) continue;
      // the comma-separated name under the column
      let at = from;
      for (const part of m[1].split(',')) {
        if (col <= at + part.length) {
          const n = part.trim();
          return [n, `${n}.tex`, `${n}.bib`].find((f) => state.files.includes(f) || f in state.bins) ?? null;
        }
        at += part.length + 1;
      }
    }
    return null;
  }
  const defIndex = new Map(); // `uri name` -> the last definition answer for it
  async function definitionAt(text, p) {
    const line = text.split('\n', p.line + 1)[p.line] ?? '', file = fileArg(line, p.character);
    if (file) return [{ uri: uri(file), range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } } }];
    const w = wordAt(line, p.character), k = w && `${uri()} ${w[2]}`;
    if (k && running > 0 && defIndex.has(k)) return defIndex.get(k);
    const r = await lsp('textDocument/definition', { textDocument: { uri: uri() }, position: p }).catch(() => null);
    if (k && (Array.isArray(r) ? r.length : r)) { if (defIndex.size > 500) defIndex.delete(defIndex.keys().next().value); defIndex.set(k, r); }
    return r;
  }
  async function definition(x = innerWidth / 2, y = innerHeight / 3) {
    sync();
    const r = await definitionAt(editor.text(), editor.position());
    const locs = (Array.isArray(r) ? r : r ? [r] : []).map((l) => [new URL(l.uri ?? l.targetUri), (l.range ?? l.targetSelectionRange).start]);
    if (!locs.length) return toast('No definition found');
    const open = ([url, start]) => url.protocol === 'file:' ? openPlace(decodeURIComponent(url.pathname), start) : window.open(url.href, '_blank', 'noopener');
    if (locs.length === 1) return open(locs[0]);
    showMenu(locs.map((l) => [l[0].protocol === 'file:' ? `${decodeURIComponent(l[0].pathname).split('/').pop()}:${l[1].line + 1}` : l[0].href, '', () => open(l)]), x, y);
  }
  /** Binary inputs never open as text: a PDF or an image in a tab of the browser, another binary as a download. */
  const BINARY = /\.(pdf|png|jpe?g|gif|webp|bmp|tiff?|eps|ps|tfm|pfb|otf|ttf|ofm|jp2)$/i, SHOWN = { pdf: 'application/pdf', png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp' };
  const binaries = new Map();
  /** The file as a blob URL, read from the bundle (or the project) once, with how it shows: `kind` is `pdf`, `img` or null. */
  function binaryOf(file) {
    if (!binaries.has(file)) {
      const ext = /\.(\w+)$/.exec(file)?.[1].toLowerCase() ?? '', type = SHOWN[ext];
      binaries.set(file, ask({ type: 'read', path: file }).then((got) => got?.data ? { url: URL.createObjectURL(new Blob([got.data], type ? { type } : {})), kind: !type ? null : ext === 'pdf' ? 'pdf' : 'img', name: file.split('/').pop(), size: got.data.length } : null).catch(() => null));
    }
    return binaries.get(file);
  }
  async function openBinary(file) {
    const b = await binaryOf(file);
    if (!b) return toast(`${file.split('/').pop()} is not available`);
    if (b.kind) return void window.open(b.url, '_blank');
    Object.assign(document.createElement('a'), { href: b.url, download: b.name }).click();
  }
  /** `file` (a path or a bare name) at `start`; a file outside the project opens read-only. */
  async function openPlace(file, start = { line: 0, character: 0 }, end = start) {
    if (BINARY.test(file)) return openBinary(file);
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
    editor.focus({ start, end }, false, true);
    editor.hold?.({ start: { line: start.line, character: 0 }, end: { line: start.line + 1, character: 0 } });
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
  /** `satex slice` on the cursor's line, or on the selected lines, backward and forward. */
  function sliceItems() {
    const text = editor.text(), [a, b] = editor.cursor().map((o) => text.slice(0, o).split('\n').length).sort((x, y) => x - y);
    const place = a === b ? `${a}:1` : `${a}:1-${b}`, what = a === b ? 'line' : 'selection';
    return [['Backward', ''], ['Forward', ' --forward']].map(([way, flag]) =>
      [`${way} from this ${what}`, '', () => execute(`slice --at ${state.file === state.root ? '' : `${state.file}:`}${place}${flag}`)]);
  }
  /** A group of menu rows: a submenu row `[label ▸]`, its one row as is, nothing when empty. */
  const group = (label, rows) => rows.length > 1 ? [[label, '', rows]] : rows;
  /** The editor's context menu at the cursor: the widgets there first, then the symbol's navigation, then
   * one submenu per group (Explain, Fix, Refactor, Slice), a link to the line last. */
  async function menuAt(x, y) {
    const w = editor.word(), p = editor.position();
    sync(); // renaming is offered where the server says the name is the project's own
    const renamable = w && !(state.file in state.ro) && !!(await lsp('textDocument/prepareRename', { textDocument: { uri: uri() }, position: p }).catch(() => null));
    const found = findingsAt(p.line, p.character).map(([d]) => d);
    const fixes = found.flatMap((d) => d.fixes);
    const rules = [...new Set(found.filter((d) => d.fixes.length).map((d) => d.code))];
    const editable = !(state.file in state.ro);
    const rows = editable ? await typedTools(p) : [];
    const run = ([label, hint, i]) => [label, hint, () => runTool(i)];
    const items = [
      ...rows.filter((r) => r[3]).map(run),
      ...(w?.[3] ? [['Go to definition', 'Ctrl+click', () => definition(x, y)]] : []),
      ...(w && editable ? [['Find references', '', references]] : []), ...(renamable ? [['Rename symbol', 'F2', renameBox]] : []),
      ...group('Explain', [
        ...(w?.[3] ? [['Explain', '', focusExplain], ['Show expansions', '', () => expansions(w)], ['Trace expansion', '', () => traceCall(`\\${w[2]}`, { file: state.file, line: p.line + 1, col: w[0] + 1 })]] : []),
        ...found.map((d) => [`Explain rule ${d.code}`, '', () => unfold(diagnostics[uri()].indexOf(d), false)]),
        ...(devMode ? [['Inspect state here', 'dev', () => traceCall(w?.[3] ? `\\${w[2]}` : `line ${p.line + 1}`, { file: state.file, line: p.line + 1, col: (w?.[3] ? w[0] : p.character) + 1 })]] : []),
      ]),
      ...group('Fix', editable ? [...fixes.map((f) => [`Fix: ${f.title}`, f.safety, () => fix(f)]),
        ...rules.map((c) => [`Fix all ${c}`, `${fixAllOf('rule', c, true)?.data.count ?? 0}×`, () => fixAll(c, true)])] : []),
      ...group('Refactor', rows.filter((r) => !r[3]).map(run)),
      ['Slice', '', sliceItems()],
      ['Copy link to this line', '', lineLink],
    ];
    if (!items.length) return false;
    showMenu(items, x, y);
    return true;
  }
  /** The page's one context menu at (x, y): rows `[label, hint, action]`, a row `[label, hint, rows]` a submenu.
   * The keys: arrows move, Right or Enter opens a submenu, Left or Escape closes it, Enter runs a row. */
  function showMenu(items, x, y) {
    const close = () => { ctxMenu.hidden = true; ctxMenu.innerHTML = ''; };
    const build = (box, rows) => {
      box.innerHTML = rows.map(([label, hint, act], i) => Array.isArray(act)
        ? `<li role="menuitem" tabindex="-1" data-i="${i}" aria-haspopup="menu" aria-expanded="false" class="ctx-sub">${escape(label)}<span class="muted">▸</span></li>`
        : `<li role="menuitem" tabindex="-1" data-i="${i}">${escape(label)}<span class="muted">${escape(hint)}</span></li>`).join('');
      box.rows = rows;
    };
    /** Opens the submenu of `li` beside it, its first row focused when `focus`. */
    const openSub = (li, focus) => {
      const box = li.closest('.ctx');
      for (const o of box.querySelectorAll(':scope > li[aria-expanded=true]')) if (o !== li) shut(o);
      if (li.getAttribute('aria-expanded') !== 'true') {
        const sub = Object.assign(document.createElement('ul'), { className: 'ctx ctx-nested' });
        sub.setAttribute('role', 'menu');
        build(sub, box.rows[li.dataset.i][2]);
        li.append(sub);
        li.setAttribute('aria-expanded', 'true');
        const r = li.getBoundingClientRect();
        const left = r.right + sub.offsetWidth + 4 > innerWidth ? r.left - sub.offsetWidth - 2 : r.right + 2;
        Object.assign(sub.style, { left: `${Math.max(4, left)}px`, top: `${Math.max(4, Math.min(r.top - 4, innerHeight - sub.offsetHeight - 8))}px` });
      }
      if (focus) li.querySelector('.ctx > li')?.focus();
    };
    const shut = (li) => { li.querySelector(':scope > .ctx')?.remove(); li.setAttribute('aria-expanded', 'false'); };
    const runRow = (li) => {
      const act = li.closest('.ctx').rows[li.dataset.i][2];
      if (Array.isArray(act)) return openSub(li, true);
      close();
      act();
    };
    build(ctxMenu, items);
    ctxMenu.hidden = false;
    tips.card.hide();
    tips.note.hide();
    Object.assign(ctxMenu.style, { left: `${Math.max(4, Math.min(x, innerWidth - ctxMenu.offsetWidth - 8))}px`, top: `${Math.max(4, Math.min(y, innerHeight - ctxMenu.offsetHeight - 8))}px` });
    ctxMenu.onclick = (e) => { const li = e.target.closest('li'); if (li) { e.stopPropagation(); runRow(li); } };
    ctxMenu.onmouseover = (e) => {
      const li = e.target.closest('li');
      if (!li) return;
      li.focus({ preventScroll: true });
      if (li.classList.contains('ctx-sub')) openSub(li, false);
      else for (const o of li.closest('.ctx').querySelectorAll(':scope > li[aria-expanded=true]')) shut(o);
    };
    ctxMenu.onkeydown = (e) => {
      const li = document.activeElement?.closest?.('#play-ctx li'), box = li?.closest('.ctx') ?? ctxMenu;
      const rows = [...box.querySelectorAll(':scope > li')], i = rows.indexOf(li), parent = box.parentElement?.closest('li');
      const go = { ArrowDown: () => rows[(i + 1) % rows.length]?.focus(), ArrowUp: () => rows[(i - 1 + rows.length) % rows.length]?.focus(),
        Home: () => rows[0]?.focus(), End: () => rows.at(-1)?.focus(),
        ArrowRight: () => li?.classList.contains('ctx-sub') && openSub(li, true),
        ArrowLeft: () => parent && (shut(parent), parent.focus()),
        Escape: () => parent ? (shut(parent), parent.focus()) : close(),
        Enter: () => li && runRow(li), ' ': () => li && runRow(li), Tab: () => close() }[e.key];
      if (!go) return;
      e.preventDefault();
      e.stopPropagation();
      go();
    };
    ctxMenu.querySelector('li')?.focus();
  }
  addEventListener('mousedown', (e) => { if (!e.target.closest('.ctx')) for (const m of document.querySelectorAll('.ctx')) m.hidden = true; });
  // Escape closes the open menu, else the open dialog, wherever the focus is: the browser's own close
  // of a dialog opened by a script can wait for a user gesture, and the keys after it would go nowhere
  addEventListener('keydown', (e) => {
    if (e.key !== 'Escape' || e.defaultPrevented) return;
    const menus = [...document.querySelectorAll('.ctx:not([hidden])')], dialog = [...document.querySelectorAll('dialog[open]')].pop();
    drop.hidden = true;
    if (menus.length) menus.forEach((m) => { m.hidden = true; });
    // the dialog's own cancel runs first: a widget with unwritten changes asks instead of closing
    else if (dialog) { e.preventDefault(); if (dialog.dispatchEvent(new Event('cancel', { cancelable: true }))) dialog.close(); }
  });

  // ---------------------------------------------------------------- the Explain panel

  /** The command at the caret or under the mouse, explained live: what `satex explain`
   * says of it in the current text, run in the worker and kept per name and text version.
   * The language server's hover fills in until the run is back. */
  const explainBody = $('explain-body'), explainOff = $('explain-off');
  var explainTimer = 0, explained = '';
  const explanations = new Map(); // `name|file|version|kernel` -> records
  function explainSoon(at) {
    if (!at) { typesetFollow(); pdfFollow(); }
    clearTimeout(explainTimer);
    explainTimer = setTimeout(() => explainAt(at ?? editor?.position()), 200);
  }
  async function explainAt(at) {
    if (!editor || !at) return;
    const line = editor.text().split('\n')[at.line] ?? '', w = wordAt(line, at.character);
    explainOff.hidden = !!w?.[3] || !explained;
    explainBody.classList.toggle('off', !w?.[3] && !!explained);
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
  /** A command's card from its explain records, the one the Explain panel, the editor's hover and a
   * finding's hover all show: the definition in force at `at` first, the others folded; '' for none. */
  function macroCard(name, at, rs) {
    const shown = rs.filter((r) => r.name === name);
    if (!shown.length) return '';
    const text = editor.text(), begin = text.split('\n').findIndex((l) => /\\begin\s*\{document\}/.test(l));
    const now = at && shown.length > 1 ? (begin >= 0 ? at.line < begin : /\\documentclass/.test(text)) ? 'preamble' : 'document' : null;
    let first = shown.filter((r) => !now || r.when === now), rest = shown.filter((r) => !first.includes(r));
    if (!first.length) [first, rest] = [shown.slice(0, 1), shown.slice(1)];
    return first.map((r) => cards.explain(r)).join('') +
      (rest.length ? `<details class="more"><summary>More definitions</summary>${rest.map((r) => cards.explain(r)).join('')}</details>` : '');
  }
  /** The Explain macro panel shows `name`: its full record, or the hover until that is back. */
  async function explainName(name, at = null, records = null) {
    const k = `${name}|${version}|${state.file}`;
    if (k === explained && !records) return;
    explained = k;
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
    shownFor(explainBody, uri(), sentVersions.get(uri()));
    const html = macroCard(name, at, rs);
    if (html) explainBody.innerHTML = html;
    else if (!explainBody.querySelector('.hover')) explainBody.innerHTML = `<p class="muted">SaTeX knows no ${escape(name)} here.</p>`;
  }
  /** Any control-sequence chip (`.cs-ref`, a name chip `a.cs`, a hover-only `.cs-tip`) shows the name's hover card after a short rest. The records are kept per name. */
  const CS_CHIP = '.cs-ref, a.cs[data-cs], .cs-tip[data-cs]';
  let csTimer = 0;
  document.addEventListener('mouseover', (e) => {
    const ref = e.target.closest?.(CS_CHIP);
    if (!ref || ref.contains(e.relatedTarget)) return;
    clearTimeout(csTimer);
    csTimer = setTimeout(async () => {
      const name = ref.dataset.cs, rs = (await explainRecords(name)).filter((r) => r.name === name);
      if (!rs.length || !ref.matches(':hover')) return;
      tips.card.show(ref, cards.explain(rs[0], true));
    }, 150);
  });
  document.addEventListener('mouseout', (e) => { if (e.target.closest?.(CS_CHIP) && !e.target.closest(CS_CHIP).contains(e.relatedTarget)) { clearTimeout(csTimer); if (!tips.card.el.dataset.pin) tips.card.hide(150); } });
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
    return [r.tag === 'documented' ? 1 : 0, used ? 0 : 1, r.package === 'document' ? 0 : 1, /[@_:]/.test(r.name.slice(1)) ? 1 : 0, r.does ? 0 : 1,
      r.tag === 'primitive' ? 1 : 0, /^\\[A-Za-z]+$/.test(r.name) ? 0 : 1, -used];
  };
  const byUse = (a, b) => { const x = libUseful(a), y = libUseful(b); for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return x[i] - y[i]; return 0; };
  /** The names in force at the end of the root's last run (`scope`), refreshed once per run. */
  async function libraryLoad() {
    libBusy(1);
    try { await libraryLoadRun(); } finally { libBusy(-1); }
  }
  /** A thin bar over the list while its parts load (names, manuals, signatures). */
  let libLoading = 0;
  const libBusy = (n) => { libLoading += n; libList.classList.toggle('lib-loading', libLoading > 0); libList.setAttribute('aria-busy', String(libLoading > 0)); };
  async function libraryLoadRun() {
    const key = `${state.root ?? state.file}|${libInternal.checked}|${analyzedVersion}`;
    if (key === libFor) return libraryShow();
    // a new run of the same project shows the signatures it had until fresh ones arrive
    if (libFor.replace(/\|[^|]*$/, '') !== key.replace(/\|[^|]*$/, '')) libSig.clear();
    libFresh.clear();
    libFor = key;
    const records = await namesInForce(libInternal.checked);
    // an environment once, by its name: \X stays as the environment, its \endX goes
    const names = new Set(records.map((r) => r.name));
    const paired = (n) => names.has(`\\end${n.slice(1)}`) && n !== '\\end';
    libNames = records.filter((r) => !(r.name.startsWith('\\end') && r.name !== '\\end' && names.has(`\\${r.name.slice(4)}`)))
      .map((r) => (paired(r.name) && r.tag !== 'primitive' ? { ...r, tag: 'environment' } : r));
    // what the loaded packages' manuals document and nothing in force defines (their mined entries)
    const manuals = await minedDocs();
    libNames.push(...manuals.filter((d) => !names.has(d.name)));
    libUses = new Map();
    for (const [file, text] of Object.entries(state.texts)) if (state.files.includes(file) && !data(file))
      for (const m of String(text).matchAll(/\\begin\s*\{([^{}]+)\}|\\([A-Za-z@]+|.)/g)) { const n = `\\${m[1] ?? m[2]}`; libUses.set(n, (libUses.get(n) ?? 0) + 1); }
    libraryShow();
  }
  /** The mined manual entries of the loaded packages, as library records: `package` the stem of the file they document. */
  async function minedDocs() {
    const request = { file: path(state.root ?? state.file), queries: [{ type: 'docs' }] };
    const rows = await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).then((r) => r?.[0]?.records ?? []).catch(() => []);
    return rows.filter((d) => /^[A-Za-z@]+$/.test(d.name) && (d.audience !== 'code' || libInternal.checked)).map((d) => ({
      name: `\\${d.name}`, tag: 'documented', package: String(d.package_file ?? '').split('/').pop().replace(/\.(sty|cls)$/, ''),
      effective: d.usage ?? `\\${d.name}`, does: d.text ?? '' }));
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
      `<span class="lib-pkg"${r.tag === 'documented' ? ' title="from the package\'s manual: nothing in this run defines it"' : ''}>${escape(libSource(r))}</span></li>`; }).join('') +
      (hits.length > 200 ? `<li class="muted">${num(hits.length - 200)} more: narrow the search</li>` : '') ||
      `<li class="muted">${!libNames.length ? 'Names in force, after each check.' : from && !libNames.some((r) => libSource(r) === from)
        ? `${escape(from)} is not loaded in this document: <code>\\usepackage{${escape(from)}}</code> loads it.` : 'No name matches.'}</li>`;
  }
  // the package filter: a search over the sources of the names in force with their counts, then the packages the playground knows
  /** The packages whose manuals the build mined, for hover descriptions. */
  const manualPackages = () => new Set((manifest.packages ?? []).filter((p) => p.manual).map((p) => p.label.replace(/\.cls$/, '')));
  const libFromList = $('lib-from-list');
  function libFromDraw() {
    const counts = new Map();
    for (const r of libNames) counts.set(libSource(r), (counts.get(libSource(r)) ?? 0) + 1);
    const prefix = `${manifest.kernels[state.kernel]?.id}/`;
    const known = [...new Set((manifest.packages ?? []).filter((p) => p.id.startsWith(prefix)).map((p) => p.label.replace(/\.cls$/, '')))].filter((p) => !counts.has(p)).sort();
    const manual = manualPackages();
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
  const LIB_SIGNED = 60, LIB_BATCH = 12, libSig = new Map(), libFresh = new Set();
  async function libSigned(rows) {
    const todo = rows.map((r) => r.name).filter((n) => !libFresh.has(n));
    if (!todo.length) return;
    todo.forEach((n) => { libFresh.add(n); if (!libSig.has(n)) libSig.set(n, null); });
    // a few at a time, so the first rows show theirs soon
    libBusy(1);
    try {
      for (let k = 0; k < todo.length; k += LIB_BATCH) {
        const part = todo.slice(k, k + LIB_BATCH), found = await explainMany(part);
        part.forEach((n, i) => libSig.set(n, found[i]?.find((r) => !r.error) ?? found[i]?.[0] ?? null));
        libSignedShow(part);
      }
    } finally { libBusy(-1); }
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
    out.innerHTML = `<div class="typeset">${set.map((r) => `<p class="ts"><span class="ts-text">${notation(withKeys(r.marked ?? r.detail ?? '', [], r.fonts, r.align, r.ligatures, r.lines), r.sketches, r.sketch)}</span></p>`).join('') || '<p class="muted">It sets no text.</p>'}</div>` +
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
  // widgets open from their gutter mark; the lens line above them with `playground.lenses`
  const lensesShown = () => document.body.classList.toggle('show-lenses', configSet['playground.lenses'] === true);
  const configKeep = (set) => { configSet = set; lensesShown(); try { localStorage.setItem('satex-config', JSON.stringify(set)); } catch {} };
  lensesShown();
  const yamlValue = (v) => (typeof v === 'string' ? v : JSON.stringify(v));
  function configFlags(set = configSet) { return Object.entries(set).flatMap(([k, v]) => ['--set', `${k}=${yamlValue(v)}`]); }
  /** Whether satex takes the settings `set`: null, else its reason, from a run that only reads them. */
  async function configRefused(set) {
    if (!Object.keys(set).length) return null;
    const r = await ask({ type: 'run', args: [...configFlags(set), 'query', '--list'], edits: {}, cwd: '/work' }).catch((e) => ({ status: 1, lines: [['err', e.message]] }));
    if (r.status === 0) return null;
    return r.lines.filter(([k, t]) => k === 'err' && !/ignored: /.test(t)).map(([, t]) => plain(t).replace(/^SaTeX: /, '')).pop() ?? 'SaTeX does not take it';
  }
  let configKeys = null;
  /** Settings set aside at the start ({set, why}): shown in the dialog, never applied. */
  let configRejected = null;
  const leaves = (props, at = '') => Object.entries(props ?? {}).flatMap(([k, p]) => ((p.type ?? 'object') === 'object' && p.properties
    ? leaves(p.properties, `${at}${k}.`) : [{ key: at + k, type: [].concat(p.type ?? 'string')[0], options: p.enum, optionTexts: p.enumDescriptions, def: p.default, about: p.description ?? '',
      affects: [].concat(p['x-affects'] ?? []), example: p['x-example'], items: p.items, min: p.minimum, max: p.maximum,
      plugin: p['x-plugin'], suggest: (p.oneOf ?? []).flatMap((o) => o.enum ?? []), pattern: (p.oneOf ?? []).find((o) => o.pattern)?.pattern }]));
  /** A key's group: its first part (`limits`), or `general` for a key of one part. */
  const groupOf_ = (k) => (k.key.includes('.') ? k.key.split('.')[0] : 'general');
  /** The plugins (build.py `plugins`) and the keys each one reads: its own section, `plugins.<name>` its switch. */
  const PLUGINS = manifest.plugins ?? [];
  /** The type plugins' tiles (build.py `plugin_tiles`): a type plugin's keys sit under `type_plugins.<name>`. */
  const TILES = manifest.typePlugins ?? [];
  const pluginOf = (k) => k.plugin ?? k.key.match(/^type_plugins\.[^.]+/)?.[0] ?? PLUGINS.find((p) => k.key === `plugins.${p.name}` || k.key.split('.')[0] === p.name)?.name ?? null;
  /** The settings a tile's plugin owns: those under its switch's parent. */
  const tileBase = (p) => p.key.replace(/\.enabled$/, '');
  const cfgMoreOpen = new Set();
  let cfgTab = 'core', cfgYaml = false;
  const configLoad = () => (configKeys ??= fetch(`${base}${manifest.schema}`).then((r) => r.json()).then((s) => leaves(s.properties)).catch(() => []));
  const sameValue = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  /** The names a list of packages or classes completes from: by the key, its description or what its default holds. */
  const NAMES = (() => {
    const pk = new Set(), cls = new Set();
    for (const p of manifest.packages ?? []) (/\.cls$/.test(p.label) ? cls : pk).add(p.label.replace(/\.cls$/, ''));
    return { packages: [...pk].sort(), classes: [...cls].sort() };
  })();
  NAMES.both = [...new Set([...NAMES.packages, ...NAMES.classes])].sort();
  const configNames = (k) => {
    const text = `${k.key} ${k.about}`, known = new Set(NAMES.both), held = Array.isArray(k.def) && k.def.length && k.def.filter((x) => known.has(x)).length * 2 > k.def.length;
    const pk = /package/i.test(text) || held, cl = /class/i.test(text);
    return pk && cl ? 'both' : pk ? 'packages' : cl ? 'classes' : null;
  };
  /** The profile a key sits in and the top-level key it overrides: `profiles.package.load.packages` -> [package, load.packages]. */
  const profileKey = (key) => key.match(/^profiles\.([^.]+)\.(.+)$/)?.slice(1) ?? null;
  function configInput(k, value) {
    if (k.type === 'boolean') return `<input type="checkbox" role="switch" data-cfg="${escape(k.key)}"${value ? ' checked' : ''}>`;
    // an enum without a default is detected by satex: unset is a choice of its own
    if (k.options) {
      // each value says what it means (the schema's enumDescriptions): in the list, and under the one chosen
      const why = (o) => k.optionTexts?.[k.options.indexOf(o)] ?? '';
      const shown = value == null ? 'SaTeX picks it from the file, as it does when the setting is not given' : why(value);
      return `<select class="dropdown" data-cfg="${escape(k.key)}">${k.def === undefined ? `<option value=""${value == null ? ' selected' : ''} title="SaTeX picks it from the file">automatic (from the file)</option>` : ''}${k.options.map((o) => `<option${sameValue(o, value) ? ' selected' : ''} title="${escape(why(o))}">${escape(o)}</option>`).join('')}</select>${shown ? `<small class="cfg-opt-why muted">${escape(shown)}</small>` : ''}`;
    }
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
  /** The plugins tab: the type plugins' tiles with a switch and the other settings of each, then the other
   *  plugins with settings as groups, then the others in one compact list, one line each. */
  function configPlugins(hits, q, section, closed, row, keys) {
    const tiled = new Set(TILES.flatMap((c) => c.plugins.map(tileBase)));
    const switches = {
      control: (p) => {
        const k = keys.find((x) => x.key === p.key);
        return k ? `<label class="ts-toggle" title="${escape(p.key)}">${configInput(k, p.key in configSet ? configSet[p.key] : k.def)}</label>` : '';
      },
      more: (p) => {
        const ks = hits.filter((k) => k.key !== p.key && pluginOf(k) === tileBase(p));
        return ks.length ? `<details class="pt-more" data-more="${escape(tileBase(p))}"${q || cfgMoreOpen.has(tileBase(p)) ? ' open' : ''}><summary>${plural(ks.length, 'more setting')}</summary><ul>${ks.map(row).join('')}</ul></details>` : '';
      },
    };
    const shown = TILES.map((c) => ({ ...c, plugins: c.plugins.filter((p) => !q || hits.some((k) => pluginOf(k) === tileBase(p)) || configMatch({ key: p.name, about: p.about, affects: [] }, q)) }))
      .filter((c) => c.plugins.length);
    const tiles = shown.length ? `<li class="cfg-group pt-settings">${self.satexTiles(shown, switches)}</li>` : '';
    const own = PLUGINS.filter((p) => !tiled.has(p.name)).map((p) => [p, hits.filter((k) => pluginOf(k) === p.name)]);
    const set = own.filter(([, ks]) => ks.length).map(([p, ks]) => section(p.name, p.name, p.about, ks, !!q || !closed.has(p.name)));
    const plain = own.filter(([p, ks]) => !ks.length && !q && p.about)
      .map(([p]) => `<li class="cfg-plain" title="${escape(p.about.replaceAll('`', ''))}"><b>${escape(p.name)}</b><span>${inline(p.about)}</span></li>`);
    const request = '<li class="cfg-group cfg-request"><button type="button" class="ghost small" data-request-plugin>Request a plugin…</button></li>';
    return [tiles, ...set, ...(plain.length ? [`<li class="cfg-group"><p class="cfg-plain-head">Without settings, always on</p><ul>${plain.join('')}</ul></li>`] : []), request];
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
      // a line that is one code span spells a value or code; backticks elsewhere are prose with code in it
      const bare = line.match(/^`([^`]+)`$/)?.[1] ?? line, value = exampleValue(k, bare);
      if (value !== undefined) return `<p class="cfg-try"><code>${escape(configYamlOf({ [k.key]: value }).replace(/\n\s*/g, ' '))}</code><button type="button" class="ghost small" data-use="${escape(k.key)}" data-value="${escape(JSON.stringify(value))}">use</button></p>`;
      return /^[\\%]/.test(bare) && bare !== line || /^[\\%]/.test(line) ? `<pre><code>${escape(bare)}</code></pre>` : `<p>${inline(line)}</p>`;
    }).join('');
  }
  /** A list's editor: a row per item, an object's fields side by side, remove and add. */
  function configList(k, value) {
    if (!k.items?.properties && ['string', 'integer', 'number'].includes(k.items?.type ?? 'string')) {
      const names = configNames(k), list = Array.isArray(value) ? value : [], allowed = k.items?.enum;
      return `<div class="cfg-chips" data-cfg="${escape(k.key)}" data-chips="1"${allowed ? ` data-enum="${escape(JSON.stringify(allowed))}"` : ''}${['integer', 'number'].includes(k.items?.type) ? ' data-num' : ''}>${list.map((v) => `<span class="cfg-chip" data-v="${escape(JSON.stringify(v))}">${escape(String(v))}<button type="button" data-chip-drop aria-label="remove ${escape(String(v))}">×</button></span>`).join('')}` +
        `<input class="cfg-chip-in" type="text" spellcheck="false" placeholder="${list.length ? '' : allowed ? allowed.join(', ') : 'add…'}" aria-label="add to ${escape(k.key)}"${allowed ? ` list="cfg-dle-${escape(k.key)}"` : names ? ` list="cfg-dl-${names}"` : ''}>${allowed ? `<datalist id="cfg-dle-${escape(k.key)}">${allowed.map((o, i) => `<option value="${escape(o)}">${escape(k.items.enumDescriptions?.[i] ?? '')}</option>`).join('')}</datalist>` : ''}<small class="cfg-chip-hint muted" hidden></small></div>`;
    }
    const fields = k.items?.properties ? Object.keys(k.items.properties) : null, list = Array.isArray(value) ? value : [];
    const item = (v, i) => `<li data-item="${i}">${fields ? fields.map((f) => `<input type="text" data-field="${escape(f)}" placeholder="${escape(f)}" value="${escape(v?.[f] ?? '')}" spellcheck="false">`).join('')
      : `<input type="text" value="${escape(typeof v === 'string' ? v : JSON.stringify(v))}" spellcheck="false">`}<button type="button" class="ghost small" data-drop="${i}" aria-label="remove">×</button></li>`;
    return `<ul class="cfg-items" data-cfg="${escape(k.key)}" data-list="1">${list.map(item).join('')}<li><button type="button" class="ghost small" data-add="1">Add</button></li></ul>`;
  }
  /** A map key as YAML writes it: plain when YAML takes it so, else quoted (a start such as `%` or `*`, a space, `: `). */
  const yamlKey = (k) => (/^[^\s#%*&!@`|>'"{}[\],?:-][^\s]*$/u.test(k) && !/:(\s|$)/.test(k) ? k : JSON.stringify(k));
  const YAML_KEY = String.raw`"(?:[^"\\]|\\.)*"|[^\s"#:][^\s:]*`;
  /** The name a key's text spells: a quoted key unquoted. */
  const keyName = (raw) => { try { return raw.startsWith('"') ? JSON.parse(raw) : raw; } catch { return raw; } };
  /** The settings changed, as the satex.yaml that sets them. */
  const configYamlOf = (set) => {
    const tree = {};
    for (const [k, v] of Object.entries(set)) { const parts = k.split('.'); let at = tree; parts.slice(0, -1).forEach((p) => { at = at[p] ??= {}; }); at[parts.at(-1)] = v; }
    const out = (o, pad) => Object.entries(o).map(([k, v]) => v && typeof v === 'object' && !Array.isArray(v) ? `${pad}${yamlKey(k)}:\n${out(v, `${pad}  `)}` : `${pad}${yamlKey(k)}: ${yamlValue(v)}`).join('\n');
    return out(tree, '');
  };
  /** Every key at its default, as the YAML view shows them: the changed ones replace theirs. */
  const yamlDefaults = (keys) => Object.fromEntries(keys.filter((k) => !k.extra && k.def != null && !k.key.startsWith('profiles.')).map((k) => [k.key, k.def]));
  /** The settings a YAML text changes: its keys at their default are no change. */
  const yamlChanges = (set, keys) => {
    const defaults = yamlDefaults(keys);
    return Object.fromEntries(Object.entries(set).filter(([k, v]) => !(k in defaults) || !sameValue(v, defaults[k])));
  };
  /** The YAML view's text as settings: nested keys by indentation, each value as JSON or else as text; null when it does not read. */
  const configFromYaml = (text, objects = new Set()) => {
    const set = {}, stack = [], isObj = (p) => objects.has(p) || objects.has(p.replace(/^profiles\.[^.]+\./, ''));
    for (const line of text.split('\n')) {
      if (!line.trim() || /^\s*#/.test(line)) continue;
      const m = line.match(new RegExp(`^(\\s*)(${YAML_KEY})\\s*:\\s*(.*)$`));
      if (!m) return null;
      const depth = m[1].length, name = keyName(m[2]);
      while (stack.length && stack.at(-1)[0] >= depth) stack.pop();
      const path = [...stack.map((s) => s[1]), name];
      // under a map-valued setting, the keys are the map's entries, whatever they spell
      const at = path.findIndex((_, i) => isObj(path.slice(0, i + 1).join('.')));
      const value = m[3] === '' ? undefined : (() => { try { return JSON.parse(m[3]); } catch { return m[3]; } })();
      if (at >= 0 && at < path.length - 1) {
        let o = (set[path.slice(0, at + 1).join('.')] ??= {});
        for (const part of path.slice(at + 1, -1)) o = (o[part] ??= {});
        if (value === undefined) stack.push([depth, name]); else o[name] = value;
        continue;
      }
      if (value === undefined) { stack.push([depth, name]); if (at === path.length - 1) set[path.join('.')] ??= {}; continue; }
      set[path.join('.')] = value;
    }
    return set;
  };
  /** Overrides added to a profile (`profiles.package.engine`), shown as rows although nothing is set yet. */
  const cfgAdded = new Set();
  let cfgFocus = null;
  /** The schema's keys and a row for each override of a top-level key that a profile holds (set, or just added). */
  const withOverrides = (keys) => {
    const have = new Set(keys.map((k) => k.key));
    const extra = [...new Set([...Object.keys(configSet).map((x) => x.replace(/\+$/, '')), ...cfgAdded])].filter((x) => !have.has(x)).map((x) => {
      const base = keys.find((k) => k.key === profileKey(x)?.[1]);
      return base && { ...base, key: x, extra: true };
    }).filter(Boolean);
    return [...keys, ...extra];
  };
  /** A key's sub-group: a profile's own name, or the second part of a key of three or more. */
  const subOf = (k) => { const p = k.key.split('.'); return p.length >= 3 || (p[0] === 'profiles' && p.length === 2) ? p[1] : null; };
  async function configShow() {
    const all = withOverrides(await configLoad()), q = $('cfg-search').value.trim().toLowerCase();
    for (const t of $('cfg-tabs').querySelectorAll('[data-tab]')) t.setAttribute('aria-pressed', String(t.dataset.tab === (cfgYaml ? 'yaml' : cfgTab)));
    $('cfg-yaml').hidden = $('cfg-yaml-ed').hidden = true;
    $('cfg-list').hidden = cfgYaml;
    if (cfgYaml) {
      const v = await yamlEditor(), box = v ? $('cfg-yaml-ed') : $('cfg-yaml'), text = configYamlOf({ ...yamlDefaults(all), ...configSet });
      box.hidden = false;
      if (v) { if (!v.hasFocus && v.state.doc.toString() !== text) { yamlSetting = true; v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: text } }); yamlSetting = false; } v.requestMeasure(); }
      else if (document.activeElement !== box) box.value = text;
    }
    const keys = all;
    // the filters as chips under the search: what the keys of this tab can be narrowed by
    const tabKeys = keys.filter((k) => (cfgTab === 'plugins') === !!pluginOf(k));
    const groups = [...new Set(tabKeys.map(groupOf_))], affects = [...new Set(tabKeys.flatMap((k) => k.affects))];
    // the filters complete a word that starts with `@`
    const typing = $('cfg-search').value.match(/(?:^|\s)(@\S*)$/)?.[1]?.toLowerCase();
    const filters = ['@changed', ...affects.map((a) => `@affects:${a}`), ...groups.map((g) => `@group:${g}`)].filter((f) => typing != null && f.startsWith(typing) && f !== typing);
    $('cfg-filters').hidden = !filters.length;
    $('cfg-filters').innerHTML = filters.map((f) => `<button type="button" class="lib-kind" data-filter="${escape(f)}">${escape(f)}</button>`).join('');
    // a profile's key mirrors a top-level one: each row links to the other
    const overrides = new Map();
    for (const k of keys) { const [prof, base] = profileKey(k.key) ?? []; if (base && keys.some((x) => x.key === base)) overrides.set(base, [...(overrides.get(base) ?? []), [prof, k.key]]); }
    const link = (key, text) => `<a href="#" class="cfg-link" data-goto="${escape(key)}">${text}</a>`;
    const row = (k) => {
      const added = configSet[`${k.key}+`], changed = k.key in configSet || !!added;
      const rejected = configRejected && k.key in configRejected.set && !(k.key in configSet);
      const [prof, base] = profileKey(k.key) ?? [], top = k.extra || (base && keys.some((x) => x.key === base)) ? base : null;
      const value = rejected ? configRejected.set[k.key] : k.key in configSet ? configSet[k.key] : k.extra ? configSet[top] ?? keys.find((x) => x.key === top)?.def : k.def;
      const badges = k.affects.map((a) => `<span class="cfg-aff" title="changing it changes the ${escape(a)}">${escape(a)}</span>`).join('');
      const example = k.example == null ? '' : `<details class="cfg-example"><summary>example</summary>${configExample(k)}</details>`;
      const links = [...(top ? [`overrides ${link(top, `<code>${escape(top)}</code>`)}`] : []), ...(overrides.get(k.key) ?? []).map(([p, key]) => `overridden in ${link(key, `<code>profiles.${escape(p)}</code>`)}`)];
      const wide = k.type === 'array';
      return `<li class="cfg-row${cfgOpened.has(k.key) ? ' cfg-open' : ''}${wide ? ' cfg-wide' : ''}${changed ? ' cfg-changed' : ''}${rejected ? ' cfg-bad' : ''}" data-key="${escape(k.key)}"><div class="cfg-what"${rejected ? ` data-why="not applied: ${escape(configRejected.why)}"` : ''}><span class="cfg-name"><code>${k.key.includes('.') ? `<span class="cfg-prefix">${escape(k.key.slice(0, k.key.lastIndexOf('.') + 1))}</span>` : ''}${escape(k.key.slice(k.key.lastIndexOf('.') + 1))}</code>${badges}<button type="button" class="cfg-perm" data-perm="${escape(k.key)}" title="copy a link that opens this setting" aria-label="copy a link to ${escape(k.key)}">#</button></span><span class="muted cfg-about">${cfgAbout(k.about)}</span>` +
        `${added ? `<span class="muted">adds ${added.map((x) => `<code>${escape(x)}</code>`).join(', ')}</span>` : ''}${links.length ? `<span class="cfg-links muted">${links.join(' · ')}</span>` : ''}${example}</div>` +
        (wide ? `<div class="cfg-value">${configList(k, value)}</div>` : `<label class="${k.type === 'boolean' ? 'ts-toggle ' : ''}cfg-value">${configInput(k, value)}</label>`) +
        `<button type="button" class="ghost small" data-reset="${escape(k.key)}"${changed || k.extra ? '' : ' hidden'} title="${k.extra ? 'remove this override' : `back to the default: ${escape(yamlValue(k.def ?? ''))}`}">${k.extra ? 'Remove' : 'Reset'}</button></li>`;
    };
    const changedIn = (ks) => ks.filter((k) => k.key in configSet || `${k.key}+` in configSet).length;
    const hits = tabKeys.filter((k) => configMatch(k, q));
    // rows of a group, those of a sub-group (profiles.package, cache.index) under a small sticky header of its own
    const rows = (g, ks) => {
      const subs = new Map([[null, []]]);
      for (const k of ks) { const s = subOf(k); if (!subs.has(s)) subs.set(s, []); subs.get(s).push(k); }
      return [...subs].filter(([, rs]) => rs.length || g === 'profiles').map(([s, rs]) => s == null ? rs.map(row).join('')
        : `<li class="cfg-sub" data-sub="${escape(`${g}.${s}`)}"><div class="cfg-subhead"><b><span class="cfg-prefix">${escape(g)}.</span>${escape(s)}</b>` +
          `${g === 'profiles' ? `<input type="text" class="cfg-addkey" list="cfg-dl-top" data-addto="${escape(s)}" placeholder="add a key to this profile…" aria-label="add a key to profile ${escape(s)}" spellcheck="false">` : ''}</div><ul>${rs.map(row).join('')}</ul></li>`).join('');
    };
    const section = (id, head, about, ks, open) => `<li class="cfg-group"><details${open ? ' open' : ''} data-group="${escape(id)}"><summary><b>${escape(head)}</b>` +
      `${about ? ` <span class="muted">${escape(about)}</span>` : ''} <span class="muted">${plural(ks.length, 'key')}${changedIn(ks) ? `, ${changedIn(ks)} changed` : ''}</span></summary><ul>${rows(id, ks)}</ul></details></li>`;
    const lists = ['packages', 'classes', 'both'].map((n) => `<datalist id="cfg-dl-${n}">${NAMES[n].map((x) => `<option value="${escape(x)}">`).join('')}</datalist>`).join('') +
      `<datalist id="cfg-dl-top">${tabKeys.filter((k) => !k.key.startsWith('profiles.') && !k.extra).map((k) => `<option value="${escape(k.key)}">`).join('')}</datalist>`;
    $('cfg-list').innerHTML = (cfgTab === 'plugins'
      ? configPlugins(hits, q, section, cfgClosed, row, keys)
      : groups.map((g) => [g, hits.filter((k) => groupOf_(k) === g)]).filter(([, ks]) => ks.length).map(([g, ks]) => section(g, g, '', ks, !!q || changedIn(ks) > 0 || !cfgClosed.has(g)))).join('')
      || '<li class="muted">No setting matches.</li>';
    $('cfg-list').insertAdjacentHTML('beforeend', lists);
    if (cfgFocus) { const el = $('cfg-list').querySelector(`[data-cfg="${CSS.escape(cfgFocus)}"] .cfg-chip-in`); cfgFocus = null; el?.focus(); }
    const n = Object.keys(configSet).length;
    $('cfg-total').textContent = n ? `${plural(n, 'setting')} changed` : 'every setting at its default';
  }
  /** Compact: the rows show their key and value only, a click on a row opens its description and example. */
  let cfgCompact = false;
  try { cfgCompact = localStorage.getItem('satex-config-compact') === '1'; } catch {}
  const cfgOpened = new Set();
  $('cfg-compact').checked = cfgCompact;
  $('cfg-list').classList.toggle('cfg-compact', cfgCompact);
  $('cfg-compact').onchange = (e) => {
    cfgCompact = e.target.checked;
    try { localStorage.setItem('satex-config-compact', cfgCompact ? '1' : '0'); } catch {}
    $('cfg-list').classList.toggle('cfg-compact', cfgCompact);
  };
  $('cfg-list').addEventListener('click', (e) => {
    const row = e.target.closest('.cfg-row');
    if (!cfgCompact || !row || e.target.closest('input, select, button, a, summary, label, .cfg-example')) return;
    row.classList.toggle('cfg-open', !cfgOpened.delete(row.dataset.key) && !!cfgOpened.add(row.dataset.key));
  });
  let configTimer = 0;
  const configApply = (set) => { configKeep(set); configShow(); clearTimeout(configTimer); configTimer = setTimeout(() => restart(), 600); };
  $('cfg-open').onclick = () => { $('play-config').showModal(); configShow(); $('cfg-search').focus(); };
  $('cfg-search').oninput = configShow;
  const cfgClosed = new Set();
  $('cfg-list').addEventListener('toggle', (e) => { const g = e.target.dataset?.group; if (g) { if (e.target.open) cfgClosed.delete(g); else cfgClosed.add(g); } }, true);
  $('cfg-list').addEventListener('toggle', (e) => { const m = e.target.dataset?.more; if (m) { if (e.target.open) cfgMoreOpen.add(m); else cfgMoreOpen.delete(m); } }, true);
  // a tile's demo opens its example (the hash) behind the closed dialog
  $('cfg-list').addEventListener('click', (e) => { const d = e.target.closest('.pt-demo'); if (d) { e.preventDefault(); $('play-config').close(); location.hash = new URL(d.href).hash; } });
  $('cfg-tabs').addEventListener('click', (e) => { const t = e.target.closest('[data-tab]'); if (!t) return; cfgYaml = t.dataset.tab === 'yaml'; if (!cfgYaml) cfgTab = t.dataset.tab; configShow(); });
  $('cfg-filters').addEventListener('click', (e) => {
    const f = e.target.closest('[data-filter]')?.dataset.filter, s = $('cfg-search');
    if (!f) return;
    s.value = `${s.value.replace(/@\S*$/, '')}${f} `;
    s.focus();
    configShow();
  });
  // the YAML view: the whole of the changed settings as satex.yaml, read back and checked as it is typed
  let yamlTimer = 0, yamlView = null, yamlLint = null, yamlSetting = false, yamlWhy = { text: null, why: null };
  const yamlGet = () => (yamlView ? yamlView.state.doc.toString() : $('cfg-yaml').value);
  const yamlChanged = () => {
    clearTimeout(yamlTimer);
    yamlTimer = setTimeout(async () => {
      const text = yamlGet(), keys = await configLoad(), read = configFromYaml(text, new Set(keys.filter((k) => k.type === 'object').map((k) => k.key))), set = read && yamlChanges(read, keys), why = set ? await configRefused(set) : 'it does not read as `key: value` lines';
      (yamlView ? $('cfg-yaml-ed') : $('cfg-yaml')).classList.toggle('cfg-yaml-bad', !!why);
      $('cfg-total').textContent = why ? `not applied: ${why}` : '';
      yamlWhy = { text, why };
      if (yamlView) yamlLint(yamlView);
      if (!why && !sameValue(set, configSet)) configApply(set);
    }, 500);
  };
  $('cfg-yaml').addEventListener('input', yamlChanged);
  /** The YAML tab's CodeMirror: keys, completion, hover and marks from the schema. Null (the textarea stays) when CodeMirror does not load. */
  async function yamlEditor() {
    if (yamlView) return yamlView;
    try {
      const [{ EditorView, basicSetup }, language, lint, ac, view] = await EDITOR, hl = (await self.siteCM()).highlight, keys = await configLoad(), t = hl.tags;
      const leafMap = new Map(keys.map((k) => [k.key, k])), strip = (full) => full.replace(/^profiles\.[^.]+\./, '');
      const leafOf = (full) => { full = full.replace(/\+$/, ''); return leafMap.get(full) ?? (full !== strip(full) ? leafMap.get(strip(full)) : undefined); };
      /** What follows `prefix` in the keys: name -> 'group' or the leaf. A profile holds every top-level key too. */
      const childrenOf = (prefix) => {
        const out = new Map(), from = [prefix, ...(prefix.startsWith('profiles.') && strip(prefix) !== prefix ? [strip(prefix)] : [])];
        from.forEach((pre, i) => { for (const k of keys) { if (!k.key.startsWith(pre) || (i && k.key.startsWith('profiles.'))) continue; const [h, ...r] = k.key.slice(pre.length).split('.'); if (!out.has(h)) out.set(h, r.length ? 'group' : k); } });
        return out;
      };
      const known = (full) => { const f = full.replace(/\+$/, ''); return [f, strip(f)].some((c) => keys.some((k) => k.key.startsWith(`${c}.`))) || /^profiles\.[^.]+$/.test(f)
        || f.split('.').slice(0, -1).some((_, i, p) => leafMap.get(p.slice(0, i + 1).join('.'))?.type === 'object'); };
      const lineKey = (text) => text.match(new RegExp(`^(\\s*)(${YAML_KEY})(\\s*:)(\\s*)(.*)$`));
      /** The keys that enclose line `n` by its indentation. */
      const pathOf = (doc, n) => {
        let need = doc.line(n).text.match(/^\s*/)[0].length;
        const out = [];
        for (let i = n - 1; i >= 1 && need > 0; i--) { const m = doc.line(i).text.match(new RegExp(`^(\\s*)(${YAML_KEY}):\\s*$`)); if (m && m[1].length < need) { out.unshift(keyName(m[2])); need = m[1].length; } }
        return out;
      };
      const mode = language.StreamLanguage.define({ token(st) {
        if (st.sol()) { st.eatSpace(); if (st.peek() === '#') { st.skipToEnd(); return 'comment'; } if (st.match(new RegExp(`(?:${YAML_KEY})(?=\\s*:(\\s|$))`))) return 'propertyName'; }
        if (st.eatSpace()) return null;
        if (st.match(/"(?:[^"\\]|\\.)*"?/)) return 'string';
        if (st.match(/-?\d+(\.\d+)?\b/)) return 'number';
        if (st.match(/(true|false|null)\b/)) return 'bool';
        if (st.match(/[[\]{},:]/)) return 'punctuation';
        if (!st.match(/[^\s,[\]{}":]+/)) st.next();
        return 'string';
      } });
      const highlighter = hl.tagHighlighter([[t.propertyName, 'yk'], [t.string, 'ys'], [t.number, 'yn'], [t.bool, 'yb'], [t.comment, 'yc'], [t.punctuation, 'yp']].map(([tag, c]) => ({ tag, class: c })));
      const complete = (ctx) => {
        const line = ctx.state.doc.lineAt(ctx.pos), before = line.text.slice(0, ctx.pos - line.from), path = pathOf(ctx.state.doc, line.number);
        let m = before.match(/^(\s*)([\w.+-]*)$/);
        if (m) {
          if (!ctx.explicit && !m[2]) return null;
          // a dotted start (`cache.in`) completes its last part
          const dot = m[2].lastIndexOf('.') + 1;
          return { from: ctx.pos - (m[2].length - dot), validFor: /^[\w+-]*$/, options: [...childrenOf(`${path.length ? `${path.join('.')}.` : ''}${m[2].slice(0, dot)}`)].map(([name, v]) => (v === 'group'
            ? { label: name, type: 'namespace', apply: `${name}:\n${m[1]}  ` } : { label: name, type: 'property', detail: v.type, info: v.about, apply: `${name}: ` })) };
        }
        // inside a list of names: `[\"foo\", \"ba`
        m = before.match(/^\s*([\w.+-]+):\s*\[(?:.*,)?\s*("?)([^",\]]*)$/);
        if (m) {
          const names = NAMES[configNames(leafOf([...path, m[1]].join('.')) ?? { key: '', about: '' }) ?? ''];
          return names && { from: ctx.pos - m[3].length - m[2].length, options: names.map((n) => ({ label: `"${n}"`, type: 'text' })), validFor: /^"?[^",\]]*$/ };
        }
        m = before.match(/^\s*([\w.+-]+):\s+([^\s"[{]*)$/);
        const k = m && leafOf([...path, m[1]].join('.'));
        if (!k) return null;
        const opts = k.options ?? (k.type === 'boolean' ? ['true', 'false'] : [...k.suggest, ...(k.def != null && typeof k.def !== 'object' ? [yamlValue(k.def)] : [])]);
        return { from: ctx.pos - m[2].length, options: [...new Set(opts)].map((o) => ({ label: String(o), type: 'enum', detail: sameValue(o, k.def) ? 'default' : '' })) };
      };
      const hover = view.hoverTooltip((v, pos) => {
        const line = v.state.doc.lineAt(pos), m = lineKey(line.text);
        if (!m) return null;
        const from = line.from + m[1].length, to = from + m[2].length;
        if (pos < from || pos > to) return null;
        const full = [...pathOf(v.state.doc, line.number), keyName(m[2])].join('.'), k = leafOf(full), dom = document.createElement('div');
        dom.className = 'cfg-tip';
        if (k) dom.innerHTML = `<code>${escape(full)}</code> <span class="muted">${escape([k.type, k.def !== undefined && `default ${yamlValue(k.def).slice(0, 60)}`].filter(Boolean).join(', '))}</span><p>${escape(k.about)}</p>`;
        else if (known(full)) {
          // an entry of a map: its code points, and the other entries that look the same but are other characters
          const name = keyName(m[2]), cp = (x) => [...x].map((c) => `U+${c.codePointAt(0).toString(16).toUpperCase().padStart(4, '0')}`).join(' ');
          const twins = [...Array(v.state.doc.lines)].map((_, i) => lineKey(v.state.doc.line(i + 1).text)).filter((o) => o && o[1] === m[1]).map((o) => keyName(o[2])).filter((o) => o !== name && o.normalize('NFKC') === name.normalize('NFKC'));
          dom.innerHTML = /[^\x00-\x7f]/.test(name) || twins.length ? `<code>${escape(name)}</code> <span class="muted">${cp(name)}</span>${twins.length ? `<p>Looks like ${twins.map((o) => `<code>${escape(o)}</code> ${cp(o)}`).join(', ')}, but is another character. Both are kept.</p>` : ''}`
            : `<code>${escape(full)}</code> <span class="muted">${plural(childrenOf(`${full}.`).size, 'setting')}</span>`;
        }
        else return null;
        return { pos: from, end: to, above: true, create: () => ({ dom }) };
      });
      const check = (v) => {
        const doc = v.state.doc, out = [];
        for (let n = 1; n <= doc.lines; n++) {
          const line = doc.line(n);
          if (!line.text.trim() || /^\s*#/.test(line.text)) continue;
          const m = lineKey(line.text);
          if (!m) { out.push({ from: line.from, to: line.to, severity: 'error', message: 'not a `key: value` line' }); continue; }
          const from = line.from + m[1].length, key = { from, to: from + m[2].length }, full = [...pathOf(doc, n), keyName(m[2])].join('.'), k = leafOf(full);
          if (!k) { if (!known(full)) out.push({ ...key, severity: 'warning', message: `unknown setting ${full}` }); continue; }
          if (!m[5]) continue;
          let value; try { value = JSON.parse(m[5]); } catch { value = m[5]; }
          const bad = (why) => out.push({ from: line.from + m[0].length - m[5].length, to: line.to, severity: 'error', message: why });
          if (full.endsWith('+') || k.type === 'array') { if (!Array.isArray(value)) bad('a list: ["a", "b"]'); }
          else if (k.options && !k.options.some((o) => sameValue(o, value))) bad(`one of ${k.options.join(', ')}`);
          else if (k.type === 'boolean' && typeof value !== 'boolean') bad('true or false');
          else if (['integer', 'number'].includes(k.type) && (typeof value !== 'number' || (k.type === 'integer' && !Number.isInteger(value)) || (k.min != null && value < k.min) || (k.max != null && value > k.max))) bad(`a${k.type === 'integer' ? 'n integer' : ' number'}${k.min != null ? `, at least ${k.min}` : ''}${k.max != null ? `, at most ${k.max}` : ''}`);
        }
        // what satex itself refused, set against the line that names the key
        if (!out.length && yamlWhy.why && yamlWhy.text === doc.toString()) {
          const at = [...Array(doc.lines)].map((_, i) => doc.line(i + 1)).find((l) => { const m = lineKey(l.text); return m && yamlWhy.why.includes(keyName(m[2])); }) ?? doc.line(1);
          out.push({ from: at.from, to: at.to, severity: 'error', message: `not applied: ${yamlWhy.why}` });
        }
        return out;
      };
      yamlLint = (v) => lint.forceLinting(v);
      yamlView = new EditorView({ parent: $('cfg-yaml-ed'), doc: configYamlOf({ ...yamlDefaults(keys), ...configSet }), extensions: [basicSetup, mode, language.syntaxHighlighting(highlighter), ac.autocompletion({ override: [complete] }), hover,
        lint.linter(check, { delay: 250 }), EditorView.updateListener.of((u) => { if (u.docChanged && !yamlSetting) yamlChanged(); })] });
      return yamlView;
    } catch (e) { console.warn('settings YAML editor:', e); return null; }
  }
  // a list's rows: an item added or removed, any field changed, is the key's new value
  $('cfg-list').addEventListener('click', (e) => {
    const list = e.target.closest('[data-list]');
    if (!list) return;
    if (e.target.closest('[data-add]')) { const li = document.createElement('li'); li.dataset.item = 'new'; li.innerHTML = list.querySelector('li[data-item]')?.innerHTML.replace(/value="[^"]*"/g, 'value=""') ?? '<input type="text" spellcheck="false"><button type="button" class="ghost small" data-drop aria-label="remove">×</button>'; list.lastElementChild.before(li); li.querySelector('input')?.focus(); }
    else if (e.target.closest('[data-drop]')) { e.target.closest('li').remove(); list.dispatchEvent(new Event('change', { bubbles: true })); }
  });
  $('cfg-list').addEventListener('change', async (e) => {
    const el = e.target.closest('[data-cfg]'), k = el && withOverrides(await configLoad()).find((x) => x.key === el.dataset.cfg);
    if (!k) return;
    // text left in a chips field when it loses focus is an item
    if (e.target.classList.contains('cfg-chip-in')) { chipAdd(el, e.target.value); return; }
    const listOf = (ul) => [...ul.querySelectorAll('li[data-item]')].map((li) => {
      const fields = [...li.querySelectorAll('[data-field]')];
      if (fields.length) return Object.fromEntries(fields.filter((f) => f.value.trim()).map((f) => [f.dataset.field, f.value.trim()]));
      const v = li.querySelector('input').value.trim();
      try { return JSON.parse(v); } catch { return v; }
    }).filter((v) => v !== '' && !(typeof v === 'object' && !Object.keys(v).length));
    let value = el.dataset.chips ? [...el.querySelectorAll('.cfg-chip')].map((c) => JSON.parse(c.dataset.v)) : el.dataset.list ? listOf(el) : el.type === 'checkbox' ? el.checked : el.value;
    el.setCustomValidity?.('');
    const holder = el.closest('.cfg-row, .pt-plugin');
    holder.classList.remove('cfg-bad');
    const refuse = (why) => { el.setCustomValidity?.(why); el.reportValidity?.(); holder.classList.add('cfg-bad'); (holder.querySelector('.cfg-what') ?? holder).dataset.why = why; };
    if (['integer', 'number'].includes(k.type)) {
      value = Number(value);
      if (!Number.isFinite(value) || (k.type === 'integer' && !Number.isInteger(value))) return refuse(`a${k.type === 'integer' ? 'n integer' : ' number'}`);
      if (k.min != null && value < k.min) return refuse(`at least ${k.min}`);
      if (k.max != null && value > k.max) return refuse(`at most ${k.max}`);
    } else if (k.type === 'object' && !el.dataset.list) { try { value = JSON.parse(value); } catch { return refuse('JSON: {"key": "value"}'); } }
    const { [k.key]: _, ...rest } = configSet;
    // unset (detected) and the default drop the key, an override of a profile stays whatever it holds
    const next = el.tagName === 'SELECT' && el.value === '' || (!k.extra && sameValue(value, k.def)) ? rest : { ...rest, [k.key]: value };
    // a value satex refuses is shown, never applied: the language server would not start with it
    const why = await configRefused(next);
    if (why) return refuse(why);
    configApply(next);
  });
  $('cfg-list').addEventListener('click', (e) => { const u = e.target.closest('[data-use]'); if (u) { configApply({ ...configSet, [u.dataset.use]: JSON.parse(u.dataset.value) }); return; } });
  $('cfg-list').addEventListener('click', (e) => { const b = e.target.closest('[data-reset]'); if (b) { const { [b.dataset.reset]: _, [`${b.dataset.reset}+`]: __, ...rest } = configSet; cfgAdded.delete(b.dataset.reset); configApply(rest); } });
  /** The chktex manual, for every description that names one of its options (`chktex's HyphDash`). */
  const CHKTEX_MANUAL = 'https://www.nongnu.org/chktex/ChkTeX.pdf';
  const cfgAbout = (text) => inline(text).replace(/\bchktex(?:&#39;|')s ([A-Z][A-Za-z]+)/g, (m, name) => `<a href="${CHKTEX_MANUAL}" target="_blank" rel="noopener" class="cfg-link" title="the chktex manual, option ${name}">${m}</a>`);
  /** A link that opens the settings at `key`, with its value when it is changed. */
  const cfgLink = (key) => {
    const v = configSet[key], q = new URLSearchParams({ setting: key });
    if (key in configSet) q.set('value', Array.isArray(v) ? v.join(',') : typeof v === 'object' ? JSON.stringify(v) : String(v));
    return `${location.origin}${location.pathname}#${q}`;
  };
  $('cfg-list').addEventListener('click', async (e) => {
    const b = e.target.closest('[data-perm]');
    if (!b) return;
    e.stopPropagation();
    try { await navigator.clipboard.writeText(cfgLink(b.dataset.perm)); toast('Copied a link to this setting'); } catch { toast(cfgLink(b.dataset.perm)); }
  });
  /** Opens the settings at a key from a link's `setting` and `value`; the value is applied when satex accepts it. */
  async function cfgFromLink(q) {
    const key = q.get('setting'), k = withOverrides(await configLoad()).find((x) => x.key === key);
    if (!k) return;
    if (q.has('value')) {
      const raw = q.get('value');
      const value = k.type === 'array' ? raw.split(',').map((x) => x.trim()).filter(Boolean).map((x) => (Number.isFinite(Number(x)) ? Number(x) : x))
        : k.type === 'boolean' ? raw === 'true' || raw === '1' : ['integer', 'number'].includes(k.type) ? Number(raw) : k.type === 'object' ? (() => { try { return JSON.parse(raw); } catch { return raw; } })() : raw;
      const next = { ...configSet, [key]: value };
      if (!(await configRefused(next))) configApply(next);
    }
    $('play-config').showModal();
    await cfgGoto(key);
  }
  /** Scrolls to the row of a key (opening its group, clearing a search that hides it) and flashes it. */
  async function cfgGoto(key) {
    const keys = withOverrides(await configLoad()), k = keys.find((x) => x.key === key);
    if (!k) return;
    cfgYaml = false;
    cfgTab = pluginOf(k) ? 'plugins' : 'core';
    if (!$('cfg-list').querySelector(`[data-key="${CSS.escape(key)}"]`) || !configMatch(k, $('cfg-search').value.trim().toLowerCase())) $('cfg-search').value = '';
    cfgClosed.delete(pluginOf(k) ?? groupOf_(k));
    await configShow();
    const row = $('cfg-list').querySelector(`[data-key="${CSS.escape(key)}"]`);
    row?.closest('details')?.setAttribute('open', '');
    row?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    row?.classList.add('cfg-flash');
    setTimeout(() => row?.classList.remove('cfg-flash'), 1600);
  }
  $('cfg-list').addEventListener('click', (e) => { const g = e.target.closest('[data-goto]'); if (g) { e.preventDefault(); cfgGoto(g.dataset.goto); } });
  // a key typed or picked in a profile's header becomes an override row of that profile
  $('cfg-list').addEventListener('change', async (e) => {
    const box = e.target.closest('[data-addto]');
    if (!box) return;
    const key = box.value.trim(), keys = await configLoad();
    if (!keys.some((k) => k.key === key && !key.startsWith('profiles.'))) { box.setCustomValidity('a top-level setting'); box.reportValidity(); return; }
    box.setCustomValidity('');
    cfgAdded.add(`profiles.${box.dataset.addto}.${key}`);
    cfgGoto(`profiles.${box.dataset.addto}.${key}`);
  });
  // chips: a list of names in one line, Enter or a comma adds, Backspace takes the last off
  function chipAdd(box, text) {
    const input = box.querySelector('.cfg-chip-in');
    const words = text.split(',').map((w) => w.trim().replace(/^"|"$/g, '')).filter(Boolean);
    input.value = '';
    if (!words.length) return;
    const numbers = 'num' in box.dataset, allowed = box.dataset.enum && JSON.parse(box.dataset.enum), hint = box.querySelector('.cfg-chip-hint');
    if (hint) hint.hidden = true;
    for (const w of words) {
      // a value outside the list is not added: the allowed ones show beside the field
      if (allowed && !allowed.includes(w)) { input.value = w; if (hint) { hint.textContent = `not one of: ${allowed.join(', ')}`; hint.hidden = false; } return; }
      if (numbers && !Number.isFinite(Number(w))) { input.value = w; input.setCustomValidity('a number'); input.reportValidity(); return; }
      const chip = Object.assign(document.createElement('span'), { className: 'cfg-chip' });
      chip.dataset.v = JSON.stringify(numbers ? Number(w) : w);
      chip.append(w, Object.assign(document.createElement('button'), { type: 'button', ariaLabel: `remove ${w}`, textContent: '×' }));
      chip.lastChild.dataset.chipDrop = '';
      input.before(chip);
    }
    input.setCustomValidity('');
    cfgFocus = box.dataset.cfg;
    box.dispatchEvent(new Event('change', { bubbles: true }));
  }
  $('cfg-list').addEventListener('keydown', (e) => {
    const input = e.target.closest('.cfg-chip-in'), box = input?.closest('[data-chips]');
    if (!box) return;
    if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); chipAdd(box, input.value); }
    else if (e.key === 'Backspace' && !input.value && box.querySelector('.cfg-chip')) { box.querySelector('.cfg-chip:last-of-type').remove(); cfgFocus = box.dataset.cfg; box.dispatchEvent(new Event('change', { bubbles: true })); }
  });
  $('cfg-list').addEventListener('input', (e) => {
    const input = e.target.closest('.cfg-chip-in');
    if (input && (e.inputType === 'insertReplacementText' || input.value.includes(','))) chipAdd(input.closest('[data-chips]'), input.value);
  });
  $('cfg-list').addEventListener('click', (e) => {
    const box = e.target.closest('[data-chips]');
    if (!box) return;
    const drop = e.target.closest('[data-chip-drop]');
    if (drop) { drop.closest('.cfg-chip').remove(); cfgFocus = box.dataset.cfg; box.dispatchEvent(new Event('change', { bubbles: true })); }
    else box.querySelector('.cfg-chip-in').focus();
  });
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
      `<button type="button" class="ghost small tr-allnames">all names at this step</button><ul class="tr-means tr-every"></ul></details>` +
      (r.state ? `<details class="tr-dev"${trace.devOpen !== false ? ' open' : ''}><summary>Domains at this step <span class="muted">dev mode</span></summary>${self.satexViews.dev.stateHtml(r.state, (n) => csCode(n))}</details>` : '');
  }
  /** The trace's records as a file: JSON as the server gave them, or CSV with one column per key. */
  function traceExport(kind) {
    const rs = traceFor?.records ?? [];
    const text = kind === 'csv' ? self.satexViews.dev.traceCsv(rs) : JSON.stringify(rs, null, 1);
    const url = URL.createObjectURL(new Blob([text], { type: kind === 'csv' ? 'text/csv' : 'application/json' }));
    Object.assign(document.createElement('a'), { href: url, download: `trace-${String(traceFor?.name ?? 'run').replace(/[^\w@:-]+/g, '')}.${kind}` }).click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }
  for (const k of ['json', 'csv']) $(`trace-${k}`).onclick = () => traceExport(k);
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
    // the cards of its names asked at once, in one query: a hover then finds them ready
    explainMany([...new Set([...box.querySelectorAll('.tr-cs')].map((c) => c.dataset.cs))]);
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
  /** The card of a name in the trace: what Explain says at once, its meaning at the step once the run stopped there answers. */
  async function showTip(ref, pin = false) {
    const name = ref.dataset.cs, si = +(ref.closest('[data-step]')?.dataset.step ?? trace?.cur ?? -1), r = trace?.steps[si];
    const label = r ? `at step ${trace.order.includes(si) ? trace.order.indexOf(si) + 1 : `event ${r.index}`}` : '';
    const mean = (m) => `<p class="tr-tip-mean"><b>meaning ${label}</b> <code>${m == null ? 'undefined' : csText(m)}</code></p>`;
    const open = () => pin || (!trTip.dataset.pin && ref.matches(':hover'));
    const show = (head, rec) => {
      trTip.ref = ref;
      tips.card.show(ref, (pin ? '<p class="tr-tip-pin muted">pinned · click outside to close</p>' : '') + (head + (rec ? cards.explain(rec, true) : '') || `<p class="muted">SaTeX knows no ${escape(name)} here.</p>`));
    };
    const known = r?.meanings?.[name];
    const rec = (await explainRecords(name)).find((x) => x.name === name);
    if (!open()) return;
    show(known != null ? mean(known) : '', rec);
    if (known != null || !r) return;
    // the run stopped at this step: slow, so the card grows when it answers and still points here
    stopped(r, [{ type: 'explain', names: [name] }]).then((rs) => {
      const at = rs.find((x) => x.name === name);
      if (trTip.ref !== ref || trTip.hidden) return;
      show(mean(at ? (at.tag === 'macro' && at.body != null ? `macro:${at.parameters ?? ''}->${at.body}` : at.body ?? at.effective ?? at.tag ?? '') : null), at ?? rec);
    });
  }
  traceBox.addEventListener('mouseover', (e) => { const ref = e.target.closest?.('.tr-cs'); if (ref && !trTip.dataset.pin) showTip(ref); });
  traceBox.addEventListener('mouseout', (e) => { if (e.target.closest?.('.tr-cs') && !trTip.dataset.pin) tips.card.hide(150); });
  traceBody.addEventListener('toggle', (e) => {
    const li = e.target.parentElement;
    if (li?.dataset.i && e.target.open) fill(li);
    if (e.target.classList?.contains('tr-state') && trace) trace.stateOpen = e.target.open;
    if (e.target.classList?.contains('tr-dev') && trace) trace.devOpen = e.target.open;
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
    const config = traceFor.config = { trace: 'true', trace_document_only: 'false', trace_lines: `[${at.line}, ${at.line}]`, trace_col: String(at.col), trace_file: at.file, ...(devMode ? { trace_state: 'true' } : {}) };
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
    // a place in the dev state (`latex.ltx:15355`) opens behind the dialog: the document's handler opens it
    if (e.target.closest('a.place')) return traceBox.close();
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
    showPanel('play-explain');
    explained = '';
    explainAt(editor.position());
  }

  // ---------------------------------------------------------------- runs

  /** The kernel's flags; a project in its own directory under /work reads the playground's satex.yaml by name. */
  const flags = () => [...manifest.kernels[state.kernel].args, ...(state.dir !== '/work' && !manifest.local ? ['--config', '/work/satex.yaml'] : []), ...configFlags()];
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
  /** A slice beside what it typesets: the reconstruction analyzed as a file next to the root. */
  async function slicePreview(text) {
    const root = (state.root ?? state.file).split('/').pop(), name = `slice-of-${root}`;
    output.innerHTML = `<p><button type="button" class="small" id="slice-compile" title="Build the slice alone, in an empty directory: it is a standalone MWE">Compile</button></p><div class="slice-split"><pre class="raw">${escape(text)}</pre><div class="typeset"><p class="muted">typesetting the slice…</p></div></div>`;
    $('slice-compile').onclick = (e) => { e.preventDefault(); compile(false, false, text); };
    post({ type: 'project', dir: state.dir, files: { [name]: encode(text) } });
    const [ts] = (await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [{ file: `${state.dir}/${name}`, queries: [{ type: 'typeset' }] }] }).catch(() => null)) ?? [];
    const box = output.querySelector('.slice-split .typeset');
    if (!box) return;
    const set = (ts?.records ?? []).filter((r) => r.kind === 'paragraph');
    box.innerHTML = ts?.error ? `<p class="muted">${escape(ts.error)}</p>`
      : set.map((r) => `<p class="ts${r.heading ? ` ts-h ts-${escape(r.heading)}` : ''}"><span class="ts-text">${notation(withKeys(r.marked ?? r.detail ?? '', [], r.fonts, r.align, r.ligatures, r.lines), r.sketches, r.sketch)}</span></p>`).join('')
        || '<p class="muted">The slice sets no text.</p>';
  }
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
      if (command() === 'slice' && !a.some((x) => x === '--list' || x === '--out' || x.startsWith('--format')) && printed.trim()) slicePreview(printed);
      // `explain \x` fills the Explain panel: one card, not a second one under Output
      let records = null;
      try { records = command() === 'explain' && !a.includes('--at') ? JSON.parse(printed) : null; } catch {}
      const explainedHere = Array.isArray(records) && records.length > 0;
      if (explainedHere) { explained = ''; explainName(records[0].name, null, records); showPanel('play-explain'); }
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

  const past = self.satexStore.get('console-history', []).filter((x) => typeof x === 'string'); // the command line's history, kept
  let back = past.length;
  /** The playground's own commands, next to satex's: language server edits in the editor. */
  const LOCAL = {
    rename: { about: 'rename the command or label at the cursor, in every file', run: (rest) => rest[0] ? renameTo(rest[0].replace(/^\\/, '')) : renameBox() },
    fix: { about: 'apply the safe fixes, of one rule if given', run: (rest) => fixAll(rest[0], false) },
    'reset-view': { about: 'forget the kept view (panels, tabs, cursor places, zoom, dialog places); edits and settings stay', run: resetView },
    manage: { about: 'open a manager: glossary, bibliography, index or theorem', run: (rest) => manage(rest[0] ?? 'glossary') },
  };
  /** A type plugin's manager: `satex/view` without a position. */
  async function manage(plugin) {
    sync();
    const [view] = (await lsp('satex/view', { textDocument: { uri: uri() }, plugin }).catch(() => [])) ?? [];
    if (!view) return toast(`No ${plugin} here`);
    tools = [{ view, target: uri() }];
    runTool(0);
  }
  /** Forgets every kept view state and reloads; the drafts, the settings and the theme stay. */
  function resetView() {
    const stay = new Set([DRAFTS, 'satex-config', 'satex-theme', 'satex-download-ok', 'satex-offline']);
    self.satexStore.clear();
    try { for (const k of Object.keys(localStorage)) if (k.startsWith('satex-') && !stay.has(k)) localStorage.removeItem(k); } catch {}
    removeEventListener('pagehide', keepView);
    location.reload();
  }
  /** Runs `line` (or the command line): a playground command, or satex. */
  function execute(line = args.value) {
    if (!editor) return;
    args.value = line;
    menu.hidden = true;
    if (past.at(-1) !== line) { past.push(line); self.satexStore.set('console-history', past.slice(-50)); }
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
    else if (flag?.takes_value) items = (flag.possible_values ?? []).map((v) => [v, flag.help]);
    else if (cur.startsWith('-')) items = node.args.filter((f) => f.long).map((f) => [`--${f.long}`, f.help]);
    else if (node.name === 'explain') {
      // the commands in scope at the end of the open file, from the language server
      const doc = editor.text().split('\n');
      const found = await completions({ line: doc.length - 1, character: doc.at(-1).length });
      if (ticket !== asking) return;
      items = rank(found, cur.replace(/^\\/, '')).map((i) => [`\\${i.label}`, i.detail]);
    } else if (node.name === 'fix') items = [...new Set((diagnostics[uri()] ?? []).filter((d) => d.fixes.some((f) => f.safety === 'safe')).map((d) => d.code))].map((c) => [c, manifest.rules[c]?.message]);
    else if (node.name === 'rename') items = [];
    else if (node.name === 'query' && !toks.some((t) => manifest.queries.includes(t))) items = manifest.queries.map((q) => [q, manifest.queryAbout?.[q] ?? 'query']);
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
  // hovering a word of the command line shows its help (cli.js): the character under the pointer, measured in the input's font
  const measure = document.createElement('canvas').getContext('2d');
  args.addEventListener('mousemove', (e) => {
    const css = getComputedStyle(args), x = e.offsetX - parseFloat(css.paddingLeft) + args.scrollLeft;
    measure.font = css.font;
    let i = 0;
    while (i < args.value.length && measure.measureText(args.value.slice(0, i + 1)).width < x) i++;
    const hit = self.satexCli?.at({ ...tree, command: top }, args.value, i), html = hit && self.satexCli.card(hit, manifest.queryAbout);
    if (html) self.satexTip?.note.show(args, html); else self.satexTip?.note.hide(150);
  });
  args.addEventListener('mouseleave', () => self.satexTip?.note.hide(250));
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
  /** Opens `name` on the server; `run` asks for the analysis it needs, which a batch of opens asks once. */
  function open(name, run = true) {
    if (opened.has(uri(name)) || name in state.ro || (data(name) && !database(name))) return;
    opened.add(uri(name));
    lsp('textDocument/didOpen', { textDocument: { uri: uri(name), languageId: 'latex', version, text: state.texts[name] } }, true);
    if (run) analyze();
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
  const ENGINES = { xelatex: 'xetex', lualatex: 'luatex', luatex: 'luatex', context: 'context', xetex: 'xetex', pdflatex: 'pdftex', pdftex: 'pdftex', latex: 'pdftex', tex: 'plain', plain: 'plain' };
  function engineOf(text = '') {
    const magic = text.match(/^%\s*!T[eE]X\s+(?:TS-)?program\s*=\s*(\w+)/m);
    if (magic && ENGINES[magic[1].toLowerCase()]) return ENGINES[magic[1].toLowerCase()];
    if (/\\(?:usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{[^}]*\b(?:fontspec|unicode-math|polyglossia|xltxtra|xunicode)\b/.test(text)) return 'xetex';
    if (/\\starttext\b/.test(text) && !/\\documentclass/.test(text)) return 'context';
    return /\\bye\b/.test(text) && !/\\documentclass/.test(text) ? 'plain' : 'pdftex';
  }
  // an engine offered on another kernel's bundle (ConTeXt on LuaTeX's) names itself in `engine`
  const wanted = () => Math.max(0, manifest.kernels.findIndex((k) => (k.engine ?? k.id) === engineOf(state.texts[state.root])));
  /** Loads and selects the engine the root asks for, unless the selector was used; true if it changed. */
  async function followRoot() {
    const want = wanted();
    if (manualKernel || want === state.kernel) return false;
    try { await kernel(want); } catch (e) { toast(`the ${manifest.kernels[want].label} kernel did not load: ${e.message}`); return false; }
    toast(`${manifest.kernels[want].label} selected for ${state.root}`);
    return true;
  }
  /** Stops the run under way: a cross-origin isolated page tells it through the shared counter (it stops at its next
   * check); else the worker is replaced, which starts the server again. False when the replacement took over. */
  function stopRun(why) {
    if (!running) return true;
    if (textsSent) { Atomics.add(textsSent, 0, 1); return true; }
    recoverWorker(why, 'Stopping the run…');
    return false;
  }
  /** Starts the language server again: another kernel, project or root. */
  /** How long the language server may take to answer its first message. */
  const START_MS = 90000;
  /** Each start counts: one superseded by a newer (a switch while the last one starts) stops quietly at its next step. */
  let starts = 0;
  async function restart() {
    const gen = ++starts, stale = () => gen !== starts;
    following = true;
    pdfOwner();
    sync();
    await followRoot();
    if (stale()) return false;
    const names = named(projectTexts());
    await fetchPackages(wanted_(names));
    // what the project needs and a background load still fetches
    const loading = wanted_(names, state.kernel, true).filter((id) => inFlight.has(id) || background.has(id));
    if (loading.length) await awaitPackages(loading);
    if (stale()) return false;
    store();
    opened.clear();
    for (const u of Object.keys(diagnostics)) if (!u.startsWith(`file://${state.dir}/`)) delete diagnostics[u];
    asked.clear();
    commandItems.clear();
    loadSeed();
    for (const p of pending.values()) p.resolve(null);
    pending.clear();
    // the run under way is of the server this replaces: it stops now, never runs to its end first
    if (running > 0 && !stopRun('restarted')) return false;
    worker.postMessage({ type: 'reset', args: [...flags(), '-f', state.root], cwd: state.dir });
    busy('Starting', 'Starting the language server…');
    try { localStorage.setItem('satex-start', JSON.stringify(configSet)); } catch {}
    // a server that does not answer, or stops, never leaves the page starting: it says so and offers another try
    const started = await Promise.race([lsp('initialize', { processId: null, rootUri: `file://${state.dir}`, capabilities: {} }),
      new Promise((done) => setTimeout(() => done(null), START_MS))]);
    if (stale()) return false;
    if (!started) {
      following = false;
      idle();
      status.innerHTML = `<span class="st-msg">The language server did not start${lspStatus.textContent ? `: ${escape(lspStatus.textContent)}` : ''}.</span> <button type="button" class="small ghost" id="play-retry">Try again</button>` +
        (Object.keys(configSet).length ? ` <button type="button" class="small ghost" id="play-defaults">Start with the default settings</button>` : '');
      status.dataset.done = 'error';
      $('play-retry').onclick = () => restart();
      $('play-defaults')?.addEventListener('click', () => { configKeep({}); restart(); });
      return;
    }
    try { localStorage.removeItem('satex-start'); } catch {}
    // an error said before (a failed start, a stopped server) is over: the line comes back to ready, not to it
    if (status.dataset.done === 'error') { status.dataset.done = 'ready'; act.rest = READY; }
    // the server answered: the card may go once no download is left, whichever phase idle() last saw
    boot.started = true;
    bootSettled();
    lsp('initialized', {}, true);
    open(state.root, false);
    open(state.file, false);
    // the server publishes findings of open files only: the project's own, up to a few, for the tree's counts
    for (const n of state.files.slice(0, OPEN_AT_START)) open(n, false);
    analyze();
    synced = version;
    typedSincePush = false;
    following = false;
    return true;
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
  /** The example's view kept per browser (satexStore): the open file, the tabs, each file's cursor and scroll. */
  const viewKey = () => `view:${state.example}`;
  function keepView() {
    if (!editor || state.upload || !state.file) return;
    const v = self.satexStore.get(viewKey(), {});
    v.file = state.file;
    v.open = state.open;
    (v.at ??= {})[state.file] = { c: editor.cursor(), y: Math.round(editor.scroll()) };
    self.satexStore.set(viewKey(), v);
  }
  function restoreAt() {
    const at = self.satexStore.get(viewKey(), {}).at?.[state.file];
    if (!at) return;
    editor.select(at.c);
    requestAnimationFrame(() => editor.scroll(at.y));
  }
  addEventListener('pagehide', keepView);
  /** Shows `name` in the editor, as a tab; the language server gets it if it does not have it. */
  function choose(name) {
    if (!editor || !(name in state.texts)) return;
    if (!state.open.includes(name)) state.open.push(name);
    if (name === state.file) return redraw();
    keepView();
    sync();
    state.file = name;
    redraw();
    editor.set(state.texts[name], name in state.ro);
    restoreAt();
    keepView();
    drawRefs();
    open(name);
    synced = version; // the server has this text: opened with it, or kept in sync with it
    typedSincePush = false;
    render();
    typeset(uri());
    heatDraw();
    // a new text drops every decoration: the swatches and lenses of this file come back from the server's last run
    lensify();
    colorize();
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
    if (state.upload) Object.assign(state, { dir: '/work', texts: { ...sampleTexts, ...drafts().texts }, upload: false, lists: drafts().lists ?? {}, bins: {} });
    for (const n of Object.keys(state.ro)) delete state.texts[n];
    state.ro = {};
    state.files = state.lists[ex] ??= list ?? [ex, ...(projects[ex] ?? [])];
    // each example in its own directory: its project sees its own files only
    state.dir = `/work/ex/${ex.replace(/\.[^.]+$/, '')}`;
    manualKernel = false;
    if (state.example && state.example !== ex) keepView();
    Object.assign(state, { example: ex, root: state.files.includes(root) ? root : ex, open: [...state.files], file: state.files.includes(file) ? file : ex });
    const kept = self.satexStore.get(viewKey(), {});
    if (file === ex && state.files.includes(kept.file)) state.file = kept.file;
    if (Array.isArray(kept.open) && kept.open.length && kept.open.every((n) => state.files.includes(n))) state.open = [...new Set([...kept.open, state.file])];
    editor.set(state.texts[state.file]);
    restoreAt();
    history.replaceState(null, '', `#${encodeURIComponent(ex)}`);
    redraw();
    if (!await restart()) return;
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
      `Your edits to ${changed.map((n) => `<code>${escape(n)}</code>`).join(', ')} are ${state.upload ? 'lost' : 'replaced'}. Copy a link first to keep them.`;
    box.returnValue = '';
    box.showModal();
    return new Promise((done) => box.addEventListener('close', () => {
      if (box.returnValue === 'tab') window.open(`${location.pathname}#${hash}`, '_blank', 'noopener');
      done(box.returnValue === 'replace');
    }, { once: true }));
  }
  // an example's edits stay (drafts): switching between examples asks nothing; an upload's would be lost
  const exampleAsked = async (ex) => { if (!state.upload || await replaceOk(ex, encodeURIComponent(ex))) await example(ex); };
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
      // an example's file as it came: the kept draft goes
      ...(!state.upload && name in sampleTexts && (name === state.file ? editor.text() : state.texts[name]) !== sampleTexts[name] ? [['Undo all my edits', '', () => {
        if (name === state.file) editor.set(sampleTexts[name]); else state.texts[name] = sampleTexts[name];
        if (opened.has(uri(name)) && name !== state.file) lsp('textDocument/didChange', { textDocument: { uri: uri(name), version: ++version }, contentChanges: [{ text: sampleTexts[name] }] }, true);
        keepDrafts(true); analyze(); toast(`${name} is as it came`);
      }]] : []),
    ], e.clientX, e.clientY);
  };
  treeBox.addEventListener('keydown', (e) => { const row = e.target.closest?.('.file[data-name]'); if (e.key === 'F2' && row && !e.target.closest('.renaming')) { e.preventDefault(); renameFile(row.dataset.name); } });
  for (const box of [treeBox, examplesBox]) box.onkeydown = (e) => { if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('[data-name], [data-example], [data-folder]')) { e.preventDefault(); e.target.click(); } };
  /** A hash `name&widget=…&at=…&k=…` as [name, the widget it asks for or null]. */
  const linkWidget = (raw) => {
    const [name, rest] = raw.split(/&(?=widget=)/), q = new URLSearchParams(rest ?? '');
    return [name, q.get('widget') ? { widget: q.get('widget'), at: q.get('at') ?? '', k: q.get('k') ?? 0 } : null];
  };
  addEventListener('hashchange', () => {
    const [named, asked] = linkWidget(location.hash.slice(1)), h = decodeURIComponent(named), opening = h.match(/^open=(.+):(\d+)$/);
    if (asked && h === state.example) self.satexOpen(asked);
    else if (asked && h in sampleTexts && !shared.has(h)) { widgetAsked(asked); exampleAsked(h); }
    else if (opening) openPlace(opening[1], { line: Number(opening[2]) - 1, character: 0 });
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
      if (state.root === name) { state.root = to; redraw(); await restart(); } else { redraw(); open(state.file, false); analyze(); }
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
      idle();
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
  function wanted_(names, k = state.kernel, all = false) {
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
    return [...todo].filter((id) => all || !fetchedPackages.has(id));
  }
  /** The project's own texts: another example's are no package this one waits for (preload() fetches those later). */
  const projectTexts = () => state.files.filter((n) => !(n in state.ro) && n in state.texts).map((n) => state.texts[n]);
  /** The packages and classes the project's own files name. */
  const named = (texts) => [...new Set(texts.flatMap((t) => [...String(t).matchAll(NAMED)].flatMap((m) => m[1].split(',').map((x) => x.trim()).filter(Boolean))))];
  const missing = (texts, all = false) => wanted_([...new Set(texts.flatMap((t) => [...String(t).matchAll(MISSING)].map((m) => m[1] ?? m[2])))], state.kernel, all);
  const loadedList = [], inFlight = new Set();
  let loadedBytes = 0;
  /** `quiet`: in the background, after the open document's first run; the status line stays the analysis'. */
  let quietLoads = 0;
  const background = new Set();
  /** packages.txt's first section, after the open document's first run: a batch at a time while the page is idle, so a switch waits for one batch at most. */
  /** The status line at rest: never ready while a package still downloads, even one fetched in the background. */
  const READY = 'Ready. Findings update as you type.', FETCH_REST = 'Fetching ';
  const restLine = () => {
    const n = new Set([...inFlight, ...background]).size;
    return n ? `${FETCH_REST}${plural(n, 'package')} in the background…` : READY;
  };
  async function preload() {
    const todo = wanted_(manifest.preload ?? []);
    while (todo.length) {
      while (act.phase || running > 0 || pending.size || !boot.started) await new Promise((r) => setTimeout(r, 500));
      const batch = [];
      for (let bytes = 0; todo.length && bytes < 2 ** 21;) { const id = todo.shift(); if (!fetchedPackages.has(id)) { batch.push(id); bytes += PACKAGES.get(id).size; } }
      const load = batch.length && fetchPackages(batch, true);
      if (load && !act.phase && (status.textContent === READY || status.textContent.startsWith(FETCH_REST))) status.textContent = restLine();
      if (load && !await load) return;
    }
  }
  async function fetchPackages(ids, quiet = false) {
    if (!ids.length) return false;
    const entries = ids.map((id) => PACKAGES.get(id));
    const label = entries.map((e) => e.label).join(', ');
    if (!quiet) busy('Fetching', `Fetching ${plural(entries.length, 'package')}, ${size(entries.reduce((n, e) => n + e.size, 0))}…`);
    if (!quiet) status.title = entries.map((e) => `${e.label} ${size(e.size)}`).join(', ');
    const t = performance.now();
    const given = location.protocol === 'file:' ? await Promise.all(entries.map((e) => local(e.bundle))) : undefined;
    const load = ask({ type: 'bundles', base, entries, given }, given ? given.map((g) => g.buffer) : []);
    // a background load is no package the shown findings wait for
    const flying = quiet ? background : inFlight;
    ids.forEach((id) => { fetchedPackages.set(id, load); flying.add(id); });
    quietLoads += +quiet;
    try { await load; } catch (e) {
      ids.forEach((id) => fetchedPackages.delete(id));
      // said where the work was shown, the status line, and once more as a note; the next edit tries again
      const why = /: 404$/m.test(e.message) ? 'The playground was updated since this page loaded: reload it to fetch packages.' : `${label} did not load (${navigator.onLine === false ? 'offline' : e.message.split('\n')[0]}). The next edit tries again.`;
      if (!quiet) { loads.clear(); idle(why); status.dataset.done = 'error'; toast(why); }
      return false;
    } finally {
      ids.forEach((id) => flying.delete(id)); quietLoads -= +quiet;
      if (!act.phase && status.textContent.startsWith(FETCH_REST)) status.textContent = restLine();
    }
    if (!quiet) {
      loads.clear();
      idle();
      status.textContent = `${plural(entries.length, 'package')} fetched in ${secs(performance.now() - t)}`;
      status.title = label;
    }
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
    const body = text == null ? '<p class="muted">Not known: the build wrote text SaTeX could not follow here.</p>'
      : `<pre>${(prev == null ? text.split('\n').map((l) => [' ', l]) : lineDiff(prev, text)).filter(([, l], i, all) => l || i < all.length - 1)
        .map(([d, l]) => `<span class="wf-l${d === '+' ? ' wf-add' : d === '-' ? ' wf-del' : ''}">${commandRefs(marked(l)) || ' '}</span>`).join('')}</pre>`;
    const changed = prev != null && text != null && prev !== text;
    const ext = short.split('.').pop(), ty = (manifest.completion?.types ?? []).find((t) => t.name.endsWith(`(.${ext})`));
    const kind = ty ? `<a class="wf-kind ty" data-ty="${escape(ty.name)}" href="${base}wiki/domains.html#files-written-in-the-run" target="_blank" rel="noopener">${escape(ext)}</a>`
      : `<a class="wf-kind" href="${base}wiki/domains.html#files-written-in-the-run" target="_blank" rel="noopener" title="a file the build writes and reads back: how SaTeX follows it">${escape(ext)}</a>`;
    return `<div class="wf" data-name="${escape(name)}"><div class="wf-head"><code>${escape(short)}</code>${kind}` +
      `<span class="muted">${text == null ? 'unknown' : plural(lines, 'line')}${k ? (changed ? `, changed since ${views[k - 1][1]}` : `, as in ${views[k - 1][1]}`) : ''}</span>` +
      `<span class="wf-runs" role="tablist">${views.length > 1 ? views.map(([v, label, t], i) => `<button type="button" role="tab" data-view="${v}" aria-selected="${i === k}"${i !== k && same(t, shown) ? ' class="wf-same" title="the same text as the tab shown"' : ''}>${label}</button>`).join('') : ''}</span></div>` +
      `${shown && shown.length > 1 ? `<div class="wf-alts" role="tablist">${shown.map((_, i) => `<button type="button" role="tab" data-alt="${i}" aria-selected="${i === a}">path ${i + 1}</button>`).join('')}</div>` : ''}${body}</div>`;
  }
  /** The files the run read (`query files`): opened on a click. Absent ones were looked for, not found. */
  let readFiles = [];
  /** The run's loads (`query dependencies`): each class, package and library file once, with what its `\Provides…` says. */
  let libraries = [];
  const LIB_KINDS = [['class', 'Classes'], ['package', 'Packages'], ['', 'Libraries and other files']];
  const writtenShow = () => {
    const written = new Set(writtenFiles.map((f) => f.name.split('/').pop()));
    // a library is a load from outside the project: the project's own files stay with the files read
    const own = (p) => state.files.some((n) => String(p).endsWith(`/${n}`));
    const libs = [...new Map(libraries.filter((l) => l.path && l.status === 'read' && !own(l.path) && !['font', 'lua'].includes(l.kind)).map((l) => [l.path, l])).values()];
    const libPaths = new Set(libs.map((l) => l.path));
    const read = readFiles.filter((r) => !written.has(String(r.file).split('/').pop()) && !libPaths.has(r.path ?? r.file));
    $('written-count').textContent = String(writtenFiles.length + libs.length + read.length || '');
    const libItem = (l) => `<li><a href="#" class="fd-open" data-path="${escape(l.path)}" title="${escape(l.path)}">${escape(String(l.path).split('/').pop())}</a>`
      + ` <span class="fd-ver">${l.provided ? escape(l.provided) : '<span class="muted">no version</span>'}</span>${l.by ? ` <span class="muted">by ${escape(String(l.by).replace(/^\\/, ''))}</span>` : ''}</li>`;
    const libGroups = Map.groupBy(libs, (l) => LIB_KINDS.find(([k]) => k === l.kind)?.[0] ?? '');
    const libHtml = LIB_KINDS.filter(([k]) => libGroups.has(k)).map(([k, label]) => `<details class="fd-group"${k ? ' open' : ''}><summary>${label} <span class="muted">${plural(libGroups.get(k).length, 'file')}</span></summary><ul class="fd-list">${libGroups.get(k).map(libItem).join('')}</ul></details>`).join('');
    // the files read, one group per kind of file: the project's, the packages', the kernel's …
    // an absent file a bundle holds is not fetched yet; the build bundles every TeX Live file a run looks for (build.py checks it), so the rest TeX Live lacks too
    const kernelId = manifest.kernels[state.kernel].id, ABSENT = ['looked for, not in the playground', 'looked for, absent in TeX Live too'];
    const groups = Map.groupBy(read, (r) => r.kind === 'absent' ? ABSENT[+!HOLDER.has(`${kernelId}/${r.file}`)] : (String(r.file).match(/\.([A-Za-z0-9]+)$/)?.[1] ?? 'other'));
    const order = [...groups.keys()].sort((a, b) => Number(ABSENT.includes(a)) - Number(ABSENT.includes(b)) || groups.get(b).length - groups.get(a).length);
    const item = (r) => r.kind === 'absent' ? `<li><code>${escape(r.file)}</code></li>`
      : `<li><a href="#" class="fd-open" data-path="${escape(r.path ?? r.file)}"${BINARY.test(r.file) ? '' : ` title="${escape(r.path ?? r.file)}"`}>${escape(String(r.file).split('/').pop())}</a> <span class="muted">${escape(r.kind ?? '')}</span></li>`;
    const readHtml = order.map((g) => `<details class="fd-group"${g === 'tex' ? ' open' : ''}><summary><code>${escape(g)}</code> <span class="muted">${plural(groups.get(g).length, 'file')}</span></summary><ul class="fd-list">${groups.get(g).map(item).join('')}</ul></details>`).join('');
    $('written-body').innerHTML = `<h4 class="fd-head">Written in SaTeX's analysis <span class="muted">${plural(writtenFiles.length, 'file')}, computed by SaTeX, not by a TeX build</span></h4>`
      + (writtenFiles.length ? '<p class="muted wf-note">SaTeX runs the document as LaTeX would and keeps what it writes in memory, on every path: each file is empty when its analysis starts, the next run reads what this one wrote. What a real compile wrote is in the PDF tab, under Files.</p>' + writtenFiles.map(writtenCard).join('') : '<p class="muted">The analysis writes no file.</p>')
      + (libs.length ? `<h4 class="fd-head">Loaded in SaTeX's run <span class="muted">${plural(libs.length, 'file')}, with the version each says it is</span></h4>${libHtml}` : '')
      + (read.length ? `<h4 class="fd-head">Read in SaTeX's run <span class="muted">${plural(read.length, 'file')}</span></h4>${readHtml}` : '');
  };
  // a hover on a PDF or an image of the lists shows a preview in the site's note: the first page, the picture
  {
    let hovered = null;
    $('written-body').addEventListener('mouseover', async (e) => {
      const a = e.target.closest?.('.fd-open');
      if (a === hovered) return;
      hovered = a;
      if (!a || !BINARY.test(a.dataset.path)) return;
      await new Promise((r) => setTimeout(r, 300));
      if (hovered !== a) return;
      const b = await binaryOf(a.dataset.path);
      if (hovered !== a || !b?.kind) return;
      const cap = `<div class="fd-cap">${escape(b.name)} <span class="muted">${Math.max(1, Math.round(b.size / 1024))} KiB, click to open</span></div>`;
      self.satexTip.note.show(a, `<div class="fd-prev">${cap}${b.kind === 'pdf' ? `<embed src="${b.url}#page=1&toolbar=0&navpanes=0&view=Fit" type="application/pdf">` : `<img src="${b.url}" alt="${escape(b.name)}">`}</div>`);
    });
    $('written-body').addEventListener('mouseout', (e) => { if (e.target.closest?.('.fd-open')) { hovered = null; self.satexTip.note.hide(150); } });
  }
  $('written-body').addEventListener('click', (e) => {
    const open = e.target.closest('.fd-open');
    if (open) { e.preventDefault(); openPlace(open.dataset.path); return; }
    const b = e.target.closest('[data-view], [data-alt]'), name = b?.closest('.wf')?.dataset.name;
    if (!name) return;
    const [view, alt] = writtenPick.get(name) ?? ['final', 0];
    writtenPick.set(name, b.dataset.view !== undefined ? [b.dataset.view === 'final' ? 'final' : +b.dataset.view, alt] : [view, +b.dataset.alt]);
    writtenShow();
  });
  /* PDF: Compile builds the project with the TeX of the machine serving the playground (`cargo playground`,
   * src/cli/gui/compile.rs). The request and the SyncTeX parse run in a worker of their own,
   * so the analysis stays responsive. Pages come as images; SyncTeX links them to the source both ways. */
  const COMPILE = `${location.origin}/__satex/compile`;
  /** The compile worker's code: posts the build, fetches and parses its SyncTeX data and the words of its pages. */
  function compileWorkerMain() {
    const BP = 65781.76; // scaled points per big point
    /** SyncTeX records of the project's own files: page, file, line, kind, x and y (baseline) in bp from the top left; a box's size, a kern's width. */
    const parse = (text) => {
      const files = {}, boxes = [];
      let unit = 1, page = 0;
      for (const l of text.split('\n')) {
        const m = /^([[(hvxkg$])(\d+),(\d+):(-?\d+),(-?\d+)(?::(-?\d+)(?:,(-?\d+),(-?\d+))?)?/.exec(l);
        if (m && page && files[m[2]]) {
          const s = unit / BP;
          boxes.push({ page, file: files[m[2]], line: +m[3], kind: m[1], x: m[4] * s, y: m[5] * s, w: (m[6] ?? 0) * s, h: (m[7] ?? 0) * s, d: (m[8] ?? 0) * s });
          continue;
        }
        const i = /^Input:(\d+):.*\/\.\/(.*)$/.exec(l), u = /^Unit:(\d+)/.exec(l), p = /^\{(\d+)/.exec(l);
        if (i) files[i[1]] = i[2]; else if (u) unit = +u[1]; else if (p) page = +p[1];
      }
      return boxes;
    };
    const unescapeHtml = (s) => s.replace(/&(amp|lt|gt|quot|#39);/g, (_, c) => ({ amp: '&', lt: '<', gt: '>', quot: '"', '#39': "'" })[c]);
    /** The words of `pdftotext -bbox`: [page, x0, y0, x1, y1, text], in bp from the top left. */
    const words = (html) => {
      const out = [];
      let page = 0;
      for (const l of html.split('\n')) {
        const w = /<word xMin="([\d.]+)" yMin="([\d.]+)" xMax="([\d.]+)" yMax="([\d.]+)">(.*)<\/word>/.exec(l);
        if (w) out.push([page, +w[1], +w[2], +w[3], +w[4], unescapeHtml(w[5])]); else if (l.includes('<page ')) page++;
      }
      return out;
    };
    /** The link annotations of `qpdf --json`: per page `{page, rect, uri}` or `{page, rect, to}`, a rectangle
     * `[left, top, width, height]` and a target `{page, x, y}` as fractions of their page, from the top left. */
    const links = ({ pages, qpdf }) => {
      const objs = qpdf[1], at = (v) => typeof v === 'string' && /^\d+ \d+ R$/.test(v) ? (objs[`obj:${v}`]?.value ?? objs[`obj:${v}`]?.stream?.dict) : v;
      const str = (s) => s?.startsWith('b:') ? s.slice(2).replace(/../g, (h) => String.fromCharCode(parseInt(h, 16))) : s?.slice(2);
      const index = new Map(pages.map((p, i) => [p.object, i + 1]));
      const box = (page) => { for (let p = at(page), n = 0; p && n < 32; p = at(p['/Parent']), n++) { const b = at(p['/CropBox'] ?? p['/MediaBox']); if (b) return b.map(at); } return [0, 0, 612, 792]; };
      const frac = (page, x, y) => { const [x0, y0, x1, y1] = box(page); return [(x - x0) / (x1 - x0), (y1 - y) / (y1 - y0)]; };
      // named destinations: the catalog's name tree, or its older /Dests dictionary
      const named = new Map(), root = at(at(qpdf[1].trailer?.value)?.['/Root']) ?? {};
      const walk = (node, depth = 0) => {
        node = at(node);
        if (!node || depth > 32) return;
        const n = at(node['/Names']) ?? [];
        for (let i = 0; i + 1 < n.length; i += 2) named.set(n[i], n[i + 1]);
        for (const k of at(node['/Kids']) ?? []) walk(k, depth + 1);
      };
      walk(at(root['/Names'])?.['/Dests']);
      for (const [k, v] of Object.entries(at(root['/Dests']) ?? {})) named.set(`u:${k.slice(1)}`, v);
      const dest = (d) => {
        d = at(typeof d === 'string' && !/ R$/.test(d) ? named.get(d) ?? named.get(`u:${d.slice(1)}`) : d);
        d = at(d?.['/D'] ?? d);
        if (!Array.isArray(d) || !index.has(d[0])) return null;
        const [ref, kind, ...a] = [d[0], ...d.slice(1).map(at)], [, y0, , y1] = box(ref);
        const [x, y] = { '/XYZ': [a[0], a[1]], '/FitH': [null, a[0]], '/FitBH': [null, a[0]], '/FitR': [a[0], a[3]] }[kind] ?? [];
        const [fx, fy] = frac(ref, x ?? box(ref)[0], y ?? y1 ?? y0);
        return { page: index.get(ref), x: fx, y: Math.max(0, fy) };
      };
      const out = [];
      pages.forEach((p, i) => {
        for (const ref of at(at(p.object)?.['/Annots']) ?? []) {
          const a = at(ref), act = at(a?.['/A']);
          if (a?.['/Subtype'] !== '/Link' || !a['/Rect']) continue;
          const [l, b, r, t] = a['/Rect'].map(at), [fl, ft] = frac(p.object, Math.min(l, r), Math.max(b, t)), [fr, fb] = frac(p.object, Math.max(l, r), Math.min(b, t));
          const rect = [fl, ft, fr - fl, fb - ft];
          if (act?.['/S'] === '/URI') out.push({ page: i + 1, rect, uri: str(act['/URI']) });
          else { const to = dest(act?.['/S'] === '/GoTo' ? act['/D'] : a['/Dest']); if (to) out.push({ page: i + 1, rect, to }); }
        }
      });
      return out;
    };
    const text = async (url, gzip) => { const r = await fetch(url); return gzip ? new Response(r.body.pipeThrough(new DecompressionStream('gzip'))).text() : r.text(); };
    self.onmessage = async ({ data: { url, request, timeout } }) => {
      try {
        const answer = await (await fetch(url, { method: 'POST', body: JSON.stringify(request), signal: AbortSignal.timeout(timeout) })).json();
        if (answer.synctex) answer.boxes = parse(await text(`${url}/${answer.base}${answer.synctex}`, true));
        if (answer.words) answer.wordList = words(await text(`${url}/${answer.base}${answer.words}`));
        // a PDF whose objects do not parse only lacks its links
        if (answer.links) answer.linkList = await text(`${url}/${answer.base}${answer.links}`).then((t) => links(JSON.parse(t))).catch(() => []);
        self.postMessage(answer);
      } catch (e) { self.postMessage({ error: String(e?.message ?? e) }); }
    };
  }
  // the draw popup offers the same embed snippet as the /draw page: that page's address beside this one
  self.satexEmbedButton?.(document.querySelector('#play-draw .draw-tools, .draw-tools'), `<iframe src="${new URL('../draw/index.html', location.href).href}" title="Draw a symbol" width="720" height="560" style="border:0"></iframe>`);
  const pdfBody = $('pdf-body'), pdfStatus = $('pdf-status'), pdfButtons = [$('pdf-compile')];
  var pdf = null, pdfWorker = null, pdfLine = '', pdfBusy = false, pdfWarming = false; // var: the cursor may move before this line runs
  /** The server builds its native caches once after a rebuild; a Compile meanwhile waits for that. */
  function pdfWatchSetup() {
    if (!pdfBusy) pdfStatus.textContent = 'setting up: the server builds its TeX caches once, in the background';
    setTimeout(() => fetch(COMPILE).then((r) => r.json()).then((j) => {
      pdfWarming = j.warming;
      if (pdfWarming) pdfWatchSetup(); else if (!pdfBusy) pdfStatus.textContent = 'ready';
    }).catch(() => {}), 3000);
  }
  const NO_SERVER = 'This needs the playground server (<code>cargo playground</code>): it runs the machine\'s TeX. Everything else here keeps working without it.';
  /** Whether the server answers, asked cheaply (its compile endpoint's status page) and not for the sake of a build. */
  let serverUp = false;
  async function probeServer() {
    let j = null;
    try { const r = await fetch(COMPILE, { cache: 'no-store', signal: AbortSignal.timeout(2500) }); j = r.ok && await r.json(); } catch {}
    const up = !!j?.compile;
    if (up) {
      pdfWarming = j.warming;
      if (pdfWarming) pdfWatchSetup();
      if (!serverUp && !pdfBusy) pdfStatus.textContent = pdfWarming ? pdfStatus.textContent : 'ready';
      if (!serverUp && pdfBody.querySelector('.no-server')) pdfBody.innerHTML = '<p class="muted">Compile shows the PDF here.</p>';
    } else if (!pdfBusy) {
      pdfBody.innerHTML = pdf ? pdfBody.innerHTML : `<p class="muted no-server">${NO_SERVER}</p>`;
      pdfStatus.textContent = 'needs the playground server';
    }
    serverUp = up;
    pdfButtons.forEach((b) => { b.disabled = !up; b.title = up ? "Build the project with this machine's TeX and show the PDF (Ctrl+S)" : 'Needs the playground server'; });
    if (up) pdfAutoKeep();
    return up;
  }
  const pdfServer = probeServer();
  // the server may come or go while the page is open
  addEventListener('focus', () => probeServer());
  addEventListener('online', () => probeServer());
  addEventListener('offline', () => probeServer());
  /** Zoom of the pages as a factor of the pane's width, in the address's `zoom` like the symbol list's. */
  const pdfZoom = { value: 1, fit: /** @type {'' | 'width' | 'page'} */ ('') };
  {
    const z = new URLSearchParams(location.search).get('zoom');
    const kept = self.satexStore.get('pdf-zoom');
    if (z === 'fit' || z === 'page') pdfZoom.fit = z === 'fit' ? 'width' : 'page';
    else if (!z && kept) Object.assign(pdfZoom, { value: Math.min(4, Math.max(0.3, Number(kept.value) || 1)), fit: ['', 'width', 'page'].includes(kept.fit) ? kept.fit : '' });
    else pdfZoom.value = Math.min(4, Math.max(0.3, Number(z) || 1));
  }
  /** `fit` keeps the zoom at the pane (width: the page as wide as the pane; page: the whole first page in view); a number zoom ends it. */
  function pdfZoomTo(z, fit = '') {
    pdfZoom.fit = fit;
    if (fit) {
      const img = pdfBody.querySelector('.pdf-page img'), w = pdfBody.clientWidth - 2, h = pdfBody.clientHeight - 8;
      z = fit === 'page' && img?.naturalWidth && w > 0 ? Math.min(1, h / (w * img.naturalHeight / img.naturalWidth)) : 1;
    }
    pdfZoom.value = Math.min(4, Math.max(0.3, Math.round(z * 100) / 100));
    self.satexStore.set('pdf-zoom', { value: pdfZoom.value, fit });
    pdfBody.style.setProperty('--pdf-zoom', String(pdfZoom.value));
    $('pdf-zoom-level').textContent = `${Math.round(pdfZoom.value * 100)}%`;
    const f = $('pdf-zoom-fit');
    f.setAttribute('aria-pressed', String(!!fit));
    f.title = fit === 'width' ? 'Fitted to the width (click: fit the whole page)' : fit === 'page' ? 'Fitted to the whole page (click: fit the width)' : 'Fit the page width (click again: fit the whole page)';
    f.setAttribute('aria-label', fit === 'width' ? 'fit whole page' : 'fit width');
    const q = new URLSearchParams(location.search);
    if (fit) q.set('zoom', fit === 'page' ? 'page' : 'fit'); else if (pdfZoom.value !== 1) q.set('zoom', String(pdfZoom.value)); else q.delete('zoom');
    history.replaceState(null, '', `${location.pathname}${q.size ? `?${q}` : ''}${location.hash}`);
  }
  pdfZoomTo(pdfZoom.value, pdfZoom.fit);
  $('pdf-zoom-out').onclick = (e) => { e.preventDefault(); pdfZoomTo(pdfZoom.value / 1.25); };
  $('pdf-zoom-in').onclick = (e) => { e.preventDefault(); pdfZoomTo(pdfZoom.value * 1.25); };
  $('pdf-zoom-fit').onclick = (e) => { e.preventDefault(); pdfZoomTo(0, pdfZoom.fit === 'width' ? 'page' : 'width'); };
  new ResizeObserver(() => { if (pdfZoom.fit) pdfZoomTo(0, pdfZoom.fit); }).observe(pdfBody);
  pdfBody.addEventListener('wheel', (e) => { if (e.ctrlKey && e.target.closest('.pdf-pages')) { e.preventDefault(); pdfZoomTo(pdfZoom.value * (e.deltaY < 0 ? 1.1 : 1 / 1.1)); } }, { passive: false });
  /** A binary file (an image) as base64 text for the request, once per file. */
  const b64s = new WeakMap();
  const base64 = (bytes) => {
    if (!b64s.has(bytes)) {
      let s = '';
      for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      b64s.set(bytes, btoa(s));
    }
    return b64s.get(bytes);
  };
  /** The project as the server builds it: its texts and the images it holds. */
  const pdfFiles = () => ({
    ...Object.fromEntries(state.files.filter((n) => n in state.texts && !(n in state.ro)).map((n) => [n, state.texts[n]])),
    ...Object.fromEntries(Object.entries(state.bins).map(([n, d]) => [n, { b64: base64(d) }])),
  });
  /** The settings of the playground's runs (the kernel's and Settings'), as the `KEY=VALUE` of each `--set`. */
  const pdfSets = () => { const f = [...manifest.kernels[state.kernel].args, ...configFlags()]; return f.filter((_, i) => f[i - 1] === '--set'); };
  /** Builds the project, or with `slice` only what the cursor's line needs. `cached` only shows a kept build. `alone`: a standalone slice's text, built on its own. */
  async function compile(slice = false, cached = false, alone = null, auto = false) {
    // a click is answered at once, in the PDF pane: one build at a time, a newer request waits (the newest only) and says so.
    // A build nobody clicked for (auto, on save, after a switch) leaves the open tab alone.
    if (!cached && !auto && $('play-pdf').hidden) showPanel('play-pdf');
    // a build for another root (an example switched to) is not waited for: it stops, its answer is dropped
    const wanted = { root: alone === null ? state.root ?? state.file : null, dir: state.dir };
    const supersedes = pdfBusy && !cached && (pdfRunning?.root !== wanted.root || pdfRunning?.dir !== wanted.dir);
    if (supersedes) pdfStop();
    if (pdfBusy) { if (!cached) { pdfQueued = [slice, false, alone, true]; $('play-pdf').classList.add('pdf-running'); pdfStatus.textContent = pdfWarming ? 'compiling once the compile server has finished its setup…' : 'compiling next, once the build under way ends…'; flare([pdfStatus], true); } return; }
    if (!cached) { $('play-pdf').classList.add('pdf-running'); pdfStatus.textContent = 'compiling: asking the compile server…'; }
    if (!(await pdfServer) && !(await probeServer())) {
      $('play-pdf').classList.remove('pdf-running');
      if (!cached) { pdfStatus.innerHTML = `<span class="x-err">${NO_SERVER_SHORT}</span>`; toast(NO_SERVER_SHORT); }
      return;
    }
    if (pdfBusy) { if (!cached) pdfQueued = [slice, false, alone, true]; return; }
    push();
    const root = alone === null ? state.root ?? state.file : `slice-of-${(state.root ?? state.file).split('/').pop()}`, p = editor.position();
    const at = `${state.file === root ? '' : `${state.file}:`}${p.line + 1}:${p.character + 1}`;
    pdfWorker ??= new Worker(URL.createObjectURL(new Blob([`(${compileWorkerMain})()`], { type: 'text/javascript' })));
    const start = performance.now(), what = slice ? `slice at ${at}` : root;
    const tick = cached ? 0 : setInterval(() => {
      const why = `${pdfWarming ? "waiting for the compile server's setup, then compiling" : 'compiling'} ${what}: ${secs(performance.now() - start)}${pdfQueued ? ' · one more queued' : ''}`;
      pdfStatus.textContent = why;
      pdfWait(why);
    }, 100);
    pdfTick = tick;
    pdfBusy = true;
    pdfRunning = wanted;
    $('play-pdf').classList.toggle('pdf-running', !cached);
    // the zip is of the build shown: none while the next one runs
    $('pdf-zip').setAttribute('aria-disabled', String(!cached));
    const dir = state.dir;
    pdfWorker.onmessage = ({ data }) => {
      clearInterval(tick);
      pdfWait(null);
      pdfBusy = false;
      pdfRunning = null;
      $('play-pdf').classList.remove('pdf-running');
      $('pdf-zip').removeAttribute('aria-disabled');
      const next = pdfQueued;
      pdfQueued = null;
      if (data.none || cached && pdf?.slice) pdfStatus.textContent = pdfSummary;
      else {
        pdfSummary = data.error ? '' : `${what} in ${data.cached ? 'cache' : secs(performance.now() - start)}`;
        pdf = { ...data, slice, root, dir };
        pdfShow();
        pdfOwner();
        pdfLoaded(!data.error && !(data.records ?? []).some((r) => r.severity === 'error'));
        // a failed request may be the server gone: say so plainly
        if (data.error) probeServer().then((up) => { if (!up) { pdfStatus.textContent = 'needs the playground server'; toast('Compiling needs the playground server'); } });
      }
      if (next) compile(...next);
    };
    pdfWorker.postMessage({ url: COMPILE, timeout: COMPILE_MS, request: { main: root, files: alone === null ? pdfFiles() : { [root]: alone }, sets: pdfSets(), ...(slice ? { at } : {}), ...(cached ? { cached } : {}), ...(supersedes ? { supersedes } : {}) } });
  }
  /** The build under way (its root and project) and its status timer. */
  var pdfRunning = null, pdfTick = 0;
  /** Drops the build under way: its worker goes, the server stops it once the next request says it supersedes it. */
  function pdfStop() {
    pdfWorker?.terminate();
    pdfWorker = null;
    clearInterval(pdfTick);
    pdfWait(null);
    pdfBusy = false;
    pdfRunning = null;
    pdfQueued = null;
  }
  /** Why the PDF pane waits, in full, with a spinner: in the bar naming whose PDF it is, else a bar of its own. */
  function pdfWait(text) {
    let el = pdfBody.querySelector('.pdf-wait');
    if (text === null) { (el?.closest('.pdf-wait-bar') ?? el)?.remove(); return; }
    if (!el) {
      el = document.createElement('span');
      el.className = 'pdf-wait';
      el.setAttribute('role', 'status');
      const owner = pdfBody.querySelector('.pdf-owner');
      if (owner) owner.append(el); else { const bar = document.createElement('p'); bar.className = 'pdf-owner pdf-wait-bar'; bar.append(el); pdfBody.prepend(bar); }
    }
    el.textContent = text;
  }
  /** How long a build may take before the page gives up on it: the server's own limit is shorter. */
  const COMPILE_MS = 600000;
  const NO_SERVER_SHORT = 'Compiling needs the playground server (cargo playground), which this page cannot reach.';
  /** The PDF is of one project and root: after a switch it dims, says whose it is, and takes no clicks, until a build of this one replaces it. */
  function pdfOwner() {
    const el = $('play-pdf'), other = !!pdf && (pdf.dir !== state.dir || (!pdf.slice && pdf.root !== (state.root ?? state.file)));
    const became = other && !el.classList.contains('pdf-other');
    el.classList.toggle('pdf-other', other);
    pdfBody.querySelector('.pdf-owner')?.remove();
    if (other) pdfBody.insertAdjacentHTML('afterbegin', `<p class="pdf-owner">Built for ${escape(pdf.root)}${pdf.dir !== state.dir ? ' of another project' : ''}. <button type="button" class="small">Compile ${escape(state.root ?? state.file)}</button></p>`);
    if (became && pdfAuto.on && serverUp) compile(false, false, null, true);
  }
  /** A build asked for while one runs (the newest only), and what the last build said. */
  var pdfQueued = null, pdfSummary = '';
  pdfButtons.forEach((b, i) => { b.onclick = (e) => { e.preventDefault(); compile(); }; });
  /** Auto-compile: a build once the edits rest for the chosen delay, remembered per viewer. var: an edit may come first. */
  var pdfAuto = { on: false, delay: 1000, timer: 0 };
  try { const kept = JSON.parse(localStorage.getItem('satex-pdf-auto') ?? '{}'); pdfAuto.on = !!kept.on; pdfAuto.delay = +kept.delay || 1000; } catch {}
  const DELAYS = [[0, 'off'], [500, '0.5 s'], [1000, '1 s'], [2000, '2 s'], [5000, '5 s']];
  $('pdf-auto-list').insertAdjacentHTML('beforeend', DELAYS.map(([ms, name]) => `<label><input type="radio" name="pdf-delay" value="${ms}"> ${name}</label>`).join(''));
  const pdfAutoKeep = () => {
    $('pdf-auto-list').querySelectorAll('input').forEach((i) => { i.checked = +i.value === (pdfAuto.on ? pdfAuto.delay : 0); });
    $('pdf-compile').classList.toggle('auto', pdfAuto.on);
    $('pdf-compile').title = pdfAuto.on ? `Automatic compile is on: a build ${pdfAuto.delay / 1000} s after the edits rest. Click to build now (Ctrl+S)` : "Build the project with this machine's TeX and show the PDF (Ctrl+S)";
    try { localStorage.setItem('satex-pdf-auto', JSON.stringify({ on: pdfAuto.on, delay: pdfAuto.delay })); } catch {}
  };
  pdfAutoKeep();
  $('pdf-auto-list').onchange = (e) => { const ms = +e.target.value; pdfAuto.on = ms > 0; pdfAuto.delay = ms || pdfAuto.delay; pdfAutoKeep(); pdfEdited(); };
  // the Compile menu closes after a choice and when clicking elsewhere
  addEventListener('pointerdown', (e) => { for (const d of document.querySelectorAll('.pdf-menu[open], .pdf-layers[open]')) if (!d.contains(e.target)) d.open = false; });
  /** An edit: with auto-compile on, a build once the edits rest; one running then queues it. A slice is asked for, never rebuilt on its own. */
  function pdfEdited() {
    if (!pdfAuto?.on || pdf?.slice) return;
    clearTimeout(pdfAuto.timer);
    pdfAuto.timer = setTimeout(() => { if (pdfAuto.on && !pdfButtons[0].disabled) compile(false, false, null, true); }, pdfAuto.delay);
  }
  /** The last build of this very text, kept by the server, shown when the page opens. */
  const pdfRestore = () => compile(false, true);
  // Ctrl+S builds, as in an editor with a build on save. Ctrl+Enter in the PDF pane too: in the editor it runs the command.
  addEventListener('keydown', (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 's' || mod && e.key === 'Enter' && e.target.closest?.('#play-pdf')) {
      e.preventDefault();
      compile(false, false, null, true);
    }
  });
  /** A short jitter on failure, a soft pulse on success: on each of `els`, once. */
  function flare(els, ok) {
    for (const el of els) {
      if (!el) continue;
      el.classList.remove('flare-ok', 'flare-bad');
      void el.offsetWidth; // restarts the animation
      el.classList.add(ok ? 'flare-ok' : 'flare-bad');
      el.addEventListener('animationend', () => el.classList.remove('flare-ok', 'flare-bad'), { once: true });
    }
  }
  const pdfOrder = (a, b) => a.path.localeCompare(b.path) || (a.line ?? 0) - (b.line ?? 0) || (b.severity === 'error') - (a.severity === 'error');
  const pdfRecord = (r) => {
    const s = r.severity === 'error' ? 'error' : 'warning', where = r.line ? `${r.path}:${r.line}` : r.path;
    return `<li class="f-${s}" tabindex="0" data-path="${escape(r.path)}" data-line="${r.line ?? 1}"><p><span class="sev sev-${s}">${s}</span> <code>${escape(r.code)}</code> ${escape(r.message)} <span class="muted">${escape(where)}</span></p>${r.fix ? `<p class="pdf-why"><b>satex:</b> ${escape(r.fix)}</p>` : ''}</li>`;
  };
  /** A page's links as areas over its image, placed in percent, so they hold at every zoom. */
  const pdfLinks = (page) => (pdf.linkList ?? []).map((l, i) => l.page !== page ? '' : `<a class="pdf-link" style="${pdfPlace(...l.rect)}" ${l.uri
    ? `href="${escape(l.uri)}" target="_blank" rel="noopener" title="${escape(l.uri)}"` : `href="#" data-link="${i}" title="to page ${l.to.page}"`}></a>`).join('');
  /** The style of a box at fractions of its page. */
  const pdfPlace = (l, t, w, h) => `left:${l * 100}%;top:${t * 100}%;width:${w * 100}%;height:${h * 100}%`;
  /** The places left by following links, the latest last: Back returns to them. */
  const pdfBack = [], pdfBackButton = $('pdf-back');
  function pdfShow() {
    pdfLine = '';
    pdfBack.length = 0;
    pdfBackButton.hidden = true;
    const records = (pdf.records ?? []).sort(pdfOrder), file = (n) => `${COMPILE}/${pdf.base}${n}`, pages = pdf.pages ?? [];
    const errors = records.filter((r) => r.severity === 'error').length, all = pdfRuns();
    const runs = all.filter(pdfBuilds), shows = all.filter((r) => !pdfBuilds(r));
    // the title line: what the build gave, its views, its files as one zip
    $('play-pdf').classList.toggle('pdf-is-slice', !!pdf.slice);
    pdfStatus.innerHTML = (pdf.slice ? '<b class="pdf-badge" title="Only what the cursor\'s line needs, built alone. Its lines are not the file\'s.">slice</b> ' : '') + (pdf.error ? `<span class="x-err" title="${escape(pdf.error)}">${escape(pdf.error)}</span>`
      : `${pdf.pdf ? `${plural(pages.length, 'page')} <a class="pdf-open" href="${file(pdf.pdf)}" target="_blank" rel="noopener" title="open ${escape(pdf.pdf)} in a new tab" aria-label="open ${escape(pdf.pdf)}">&#x2197;</a>` : '<span class="x-err">no PDF</span>'}`
      + [errors && `<span class="x-err">${plural(errors, 'error')}</span>`, records.length - errors && plural(records.length - errors, 'warning')].filter(Boolean).map((t, i) => (i ? ', ' : ' · ') + t).join('') + `${pdfSummary ? ` <span class="muted">· ${escape(pdfSummary)}</span>` : ''}`);
    const zip = /** @type {HTMLAnchorElement} */ ($('pdf-zip'));
    zip.hidden = !pdf.base;
    if (pdf.base) { zip.href = `${COMPILE}/${pdf.base.replace(/\/$/, '')}.zip`; zip.download = `${pdf.root.replace(/\.tex$/, '')}${pdf.slice ? '-slice' : ''}-build.zip`; }
    // one view at a time: the pages, the messages, the log, the commands run
    const views = [['pages', 'Pages', pages.length || ''], ['messages', 'Messages', records.length || ''], ['log', 'Logs', ''], ['files', 'Files', (pdf.outputs ?? []).length || ''], ['commands', 'Commands', runs.length || '']];
    if (pdf.error && pdfView === 'pages') pdfView = 'commands';
    const tab = ([id, label, n]) => `<button type="button" role="tab" data-view="${id}" aria-selected="${id === pdfView}"${id === 'messages' && errors ? ' class="pdf-err"' : ''}>${label}${n !== '' ? ` <b class="count">${n}</b>` : ''}</button>`;
    const log = (title, text) => text ? `<h4>${title}</h4><pre>${escape(text)}</pre>` : '';
    $('pdf-views').innerHTML = views.map(tab).join('');
    pdfViewsSel.innerHTML = views.map(([id, label, n]) => `<option value="${id}"${id === pdfView ? ' selected' : ''}>${label}${n !== '' ? ` (${n})` : ''}</option>`).join('');
    pdfViewsSel.classList.toggle('pdf-err', !!errors);
    pdfBody.innerHTML = (pdf.error ? `<p class="pdf-head x-err">${escape(pdf.error)}</p>` : '')
      + `<div class="pdf-pages" data-view="pages">${pdf.slice ? '<div class="pdf-sep" role="separator"><b class="pdf-badge">slice</b><span>not the full build</span></div>' : ''}${pdfFrames(pages).map((g, frame) => { frame += 1; const html = g.map((i) => `<div class="pdf-page" data-page="${i + 1}"><img src="${file(pages[i])}" alt="page ${i + 1}" draggable="false">${pdfLinks(i + 1)}</div>`).join(''); return g.length < 2 ? html : `<div class="pdf-frame" data-step="${g.length - 1}"><div class="pdf-frame-bar" role="group" aria-label="overlay steps"><span><b>frame ${frame}</b> · step <b class="pdf-step">${g.length}/${g.length}</b></span><button type="button" class="small ghost icon" data-step="-1" title="Previous step" aria-label="previous step">&#x2039;</button><button type="button" class="small ghost icon" data-step="1" title="Next step" aria-label="next step">&#x203A;</button></div>${html}</div>`; }).join('') || '<p class="muted">No pages.</p>'}</div>`
      + `<div class="pdf-text" data-view="messages">${records.length ? `<ul class="diagnostics pdf-records">${records.map(pdfRecord).join('')}</ul>` : '<p class="muted">The log has no error or warning.</p>'}</div>`
      + `<div class="pdf-text pdf-log" data-view="log">${(pdf.logs ? Object.entries(pdf.logs).map(([n, t]) => log(escape(n), t)).join('') : log('TeX log', pdf.log)) + log('Console: what satex compile and latexmk printed', pdf.stderr) || '<p class="muted">The build wrote no log.</p>'}</div>`
      + `<div class="pdf-text" data-view="files"><p class="muted">What the build wrote beside the sources. Each opens in a new tab, ↓ saves it. <a href="${escape(zip.href)}" download="${escape(zip.download)}">All of them with the sources as compiled, as one zip</a>.</p><ul class="pdf-cmds">${(pdf.outputs ?? []).map((o) => `<li><a href="${file(o.name)}" target="_blank" rel="noopener">${escape(o.name)}</a> <a href="${file(o.name)}" download="${escape(o.name.split('/').pop())}" title="save ${escape(o.name)}" aria-label="save ${escape(o.name)}">↓</a> <span class="muted">${size(o.size)}</span></li>`).join('')}</ul></div>`
      + `<div class="pdf-text" data-view="commands"><p class="muted">Run in a fresh directory on this machine${pdfSetsShown()}. <code>satex compile</code> runs latexmk where it is installed, else the engine, and reads the project's latexmkrc and satex.yaml.</p>`
      + `<ol class="pdf-cmds">${runs.map(pdfCommand).join('')}</ol>`
      + (shows.length ? `<details class="pdf-shows"><summary class="muted">How the playground shows the build (${shows.length})</summary><ol class="pdf-cmds">${shows.map(pdfCommand).join('')}</ol></details>` : '') + '</div>';
    pdfViewShow();
    pdfBody.querySelectorAll('.pdf-frame').forEach((f) => pdfStep(f, +f.dataset.step));
    pdfBody.querySelectorAll('.pdf-page img').forEach((img) => img.addEventListener('load', () => { if (pdfZoom.fit === 'page') pdfZoomTo(0, 'page'); if (pdfFollowOn) pdfFollow(true); pdfOverlay(); }, { once: true }));
  }
  /** The commands of the build: the server's, the engine runs `satex compile` names on its console (`$ …`) after it. */
  const pdfRuns = () => (pdf.commands ?? []).flatMap((r) => [r, ...(/ compile( |$)/.test(r.command) ? String(pdf.stderr ?? '').split('\n').filter((l) => l.startsWith('$ ')).map((l) => ({ command: l.slice(2), inner: true })) : [])]);
  /** One command run, with its exit and time; one `satex compile` ran shows as its. */
  const pdfCommand = (r) => `<li${r.status ? ' class="x-err"' : ''}><code>$ ${escape(r.command)}</code>${r.inner ? ' <span class="muted">(by satex compile)</span>' : ` <span class="muted">${r.error ? escape(r.error) : `exit ${r.status ?? '?'}${r.ms != null ? `, ${secs(r.ms)}` : ''}`}</span>`}</li>`;
  /** The build's own commands (`satex compile` and what it ran) apart from the tools the playground shows the PDF with. */
  const pdfBuilds = (r) => r.inner || / compile( |$)/.test(r.command);
  const pdfSetsShown = () => { const sets = pdfSets(); return sets.length ? `, with the playground's settings: ${sets.map((x) => `<code>--set ${escape(x)}</code>`).join(' ')}` : ''; };
  /** The view of the build shown: `pages`, `messages`, `log` or `commands`. */
  let pdfView = 'pages';
  function pdfViewShow() {
    for (const t of $('pdf-views').querySelectorAll('[data-view]')) t.setAttribute('aria-selected', String(t.dataset.view === pdfView));
    pdfViewsSel.value = pdfView;
    for (const v of pdfBody.querySelectorAll(':scope > [data-view]')) v.hidden = v.dataset.view !== pdfView;
  }
  /** A page's size in bp, from its image. */
  const pageSize = (img) => [img.naturalWidth * 72 / pdf.dpi, img.naturalHeight * 72 / pdf.dpi];
  /** The word of the page whose box holds the baseline `y` and is the first at or after `x` (`before`: the last ending by it). */
  function pdfWordAt(page, y, x, before = false) {
    const row = (pdf.wordList ?? []).filter((w) => w[0] === page && w[2] - 1 <= y && y <= w[4] + 1);
    return before ? row.filter((w) => w[3] <= x + 3).at(-1) : row.find((w) => w[3] > x);
  }
  /** Where `file:line` stands on its first page: one rectangle per baseline, in bp. Only the points TeX set
   * (glue, kerns, math, the current point), not the boxes, which carry the line where a whole frame or float closed.
   * Glue lies between words, so a row grows to the words beside its first and last point. */
  function pdfRects(file, line, all = false) {
    const rows = new Map(), hits = (pdf.boxes ?? []).filter((b) => b.file === file && b.line === line && 'xgk$'.includes(b.kind));
    for (const b of hits.filter((b) => all || b.page === hits[0].page)) {
      const key = `${b.page}|${Math.round(b.y / 6)}`, o = rows.get(key); // one row per baseline, a lowered letter (the E of \TeX) in it
      rows.set(key, { page: b.page, y: b.y, l: Math.min(o?.l ?? b.x, b.x), r: Math.max(o?.r ?? 0, b.x + 4), t: b.y - 8, d: b.y + 2 });
    }
    for (const r of rows.values()) {
      r.l = pdfWordAt(r.page, r.y, r.l, true)?.[1] ?? r.l;
      r.r = Math.max(r.r, pdfWordAt(r.page, r.y, r.r - 4)?.[3] ?? r.r);
    }
    return [...rows.values()];
  }
  /** Whether the pages follow the editor's cursor, remembered per viewer. */
  var pdfFollowOn = true;
  try { pdfFollowOn = localStorage.getItem('satex-pdf-follow') !== 'off'; } catch {}
  const pdfFollowButton = $('pdf-follow');
  const followState = () => { pdfFollowButton.setAttribute('aria-pressed', String(pdfFollowOn)); pdfFollowButton.title = `The page follows the editor's cursor: ${pdfFollowOn ? 'on' : 'off'}`; };
  followState();
  pdfFollowButton.onclick = (e) => {
    e.preventDefault();
    pdfFollowOn = !pdfFollowOn;
    followState();
    try { localStorage.setItem('satex-pdf-follow', pdfFollowOn ? 'on' : 'off'); } catch {}
    if (pdfFollowOn) pdfFollow(); else pdfBody.querySelectorAll('.pdf-mark').forEach((m) => m.remove());
  };
  /** Dark PDF: unset follows the site theme, the button overrides it, remembered per viewer. */
  const pdfDarkButton = document.createElement('button');
  pdfDarkButton.type = 'button'; pdfDarkButton.id = 'pdf-dark'; pdfDarkButton.className = 'small ghost icon';
  pdfDarkButton.setAttribute('aria-label', 'dark PDF');
  pdfFollowButton.before(pdfDarkButton);
  const siteDark = () => { const t = document.documentElement.dataset.theme; return t ? t === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches; };
  const darkState = () => {
    let v = null;
    try { v = localStorage.getItem('satex-pdf-dark'); } catch {}
    const dark = v ? v === 'on' : siteDark(), pane = $('play-pdf');
    pane.classList.toggle('pdf-night', v === 'on'); pane.classList.toggle('pdf-day', v === 'off');
    pdfDarkButton.setAttribute('aria-pressed', String(dark)); pdfDarkButton.textContent = dark ? '☾' : '☀'; pdfDarkButton.title = `Dark PDF: ${dark ? 'on' : 'off'}`;
  };
  darkState();
  pdfDarkButton.onclick = (e) => {
    e.preventDefault();
    try { localStorage.setItem('satex-pdf-dark', pdfDarkButton.getAttribute('aria-pressed') === 'true' ? 'off' : 'on'); } catch {}
    darkState();
  };
  /** A build's download asks first, once: the answer stays with this viewer. */
  addEventListener('click', (e) => {
    const a = e.target instanceof Element ? e.target.closest('#play-pdf a[download]') : null;
    if (!a) return;
    let ok = false;
    try { ok = localStorage.getItem('satex-download-ok') === 'yes'; } catch {}
    if (ok) return;
    const bytes = a.id === 'pdf-zip' ? (pdf?.outputs ?? []).reduce((n, o) => n + o.size, 0) : (pdf?.outputs ?? []).find((o) => a.href.endsWith(encodeURI(o.name)))?.size;
    const what = a.id === 'pdf-zip' ? 'the whole build as a zip' : a.download;
    if (!confirm(`Download ${what}${bytes ? ` (${a.id === 'pdf-zip' ? 'about ' : ''}${size(bytes)})` : ''}? You will not be asked again.`)) { e.preventDefault(); e.stopImmediatePropagation(); return; }
    try { localStorage.setItem('satex-download-ok', 'yes'); } catch {}
  }, true);
  /** The bar folded to one line, remembered per viewer. */
  $('pdf-fold').onclick = (e) => {
    e.preventDefault();
    const on = $('play-pdf').classList.toggle('pdf-folded');
    $('pdf-fold').setAttribute('aria-pressed', String(on));
    $('pdf-fold').querySelector('.fold-label').textContent = on ? 'Unfold' : 'Fold';
    $('pdf-fold').title = on ? 'Show the whole bar again' : 'Fold this bar to one line';
    try { localStorage.setItem('satex-pdf-fold', on ? 'on' : 'off'); } catch {}
  };
  try { if (localStorage.getItem('satex-pdf-fold') === 'on') $('pdf-fold').click(); } catch {}
  /** The cursor's line marked on the pages, scrolled into view when it moved. */
  function pdfFollow(again = false) {
    const pos = editor?.position();
    if (!pdf?.boxes || pdf.slice || !pos || !again && !pdfFollowOn || $('play-pdf').classList.contains('pdf-other')) return;
    const line = pos.line + 1, key = `${state.file}:${line}`;
    if (key === pdfLine && !again) return;
    pdfLine = key;
    pdfBody.querySelectorAll('.pdf-mark').forEach((m) => m.remove());
    // a line that sets nothing (a blank line, a comment) is marked by the nearest line above that does
    let rects = [];
    for (let l = line; l > 0 && l > line - 8 && !rects.length; l--) rects = pdfRects(state.file, l);
    // a line that closes a frame or a float sets all of it: outlined, not filled
    const coarse = rects.length > 8;
    // the word at the cursor, its n-th occurrence in the line the n-th among the marked rows' words
    const src = state.texts[state.file]?.split('\n')[line - 1] ?? '', at = wordAt(src, pos.character);
    const nth = at && !at[3] ? [...src.slice(0, at[0]).matchAll(new RegExp(`(?<![\\p{L}\\p{N}\\\\])${at[2].replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`, 'gu'))].length : -1;
    const same = nth < 0 || coarse ? [] : rects.flatMap((r) => (pdf.wordList ?? []).filter((w) => w[0] === r.page && w[4] >= r.t && w[2] <= r.d && w[3] > r.l && w[1] < r.r && wordInLine(w[5], at[2])));
    const mark = (page, cls, l, t, r, d) => {
      const el = pdfBody.querySelector(`.pdf-page[data-page="${page}"]`), img = el?.querySelector('img');
      if (!img?.naturalWidth) return;
      const [w, h] = pageSize(img), pad = 1.5;
      el.insertAdjacentHTML('beforeend', `<div class="pdf-mark${cls}" style="${pdfPlace((l - pad) / w, (t - pad) / h, (r - l + 2 * pad) / w, (d - t + 2 * pad) / h)}"></div>`);
    };
    // a whole frame or float: one outline per page around all of it, not one per line
    const union = (rs) => Object.values(rs.reduce((u, r) => { const b = u[r.page]; u[r.page] = b ? { page: r.page, l: Math.min(b.l, r.l), t: Math.min(b.t, r.t), r: Math.max(b.r, r.r), d: Math.max(b.d, r.d) } : { ...r }; return u; }, {}));
    for (const r of coarse ? union(rects) : rects) mark(r.page, coarse ? ' coarse' : '', r.l, r.t, r.r, r.d);
    const word = same[Math.min(nth, same.length - 1)];
    if (word) mark(word[0], ' word', word[1], word[2], word[3], word[4]);
    const first = pdfBody.querySelector('.pdf-mark'), pages = pdfBody.querySelector('.pdf-pages');
    if (first && !again) {
      const a = first.getBoundingClientRect(), b = pages.getBoundingClientRect();
      if (a.top < b.top || a.bottom > b.bottom) pages.scrollTop += a.top - b.top - b.height / 3;
    }
  }
  /** The source of a point on a page: the nearest record of the project's files, one line above another counting more.
   * Boxes carry the line where they closed, so only the points TeX set count. Over a word, only those on its row. */
  function pdfSource(page, x, y, word, fine = true) {
    let best = null, score = Infinity;
    for (const b of pdf.boxes ?? []) {
      if (b.page !== page || !(b.file in state.texts) || fine && !'xgk$'.includes(b.kind) || word && Math.abs(b.y - word[4]) > 4) continue;
      const s = Math.abs(x - b.x) / 4 + Math.abs(y - (b.y - 3));
      if (s < score) { score = s; best = b; }
    }
    return best ?? (word ? pdfSource(page, x, y, null, fine) : fine ? pdfSource(page, x, y, null, false) : null);
  }
  /** The word on the page at a point, if one is under it. */
  const pdfWordUnder = (page, x, y) => (pdf.wordList ?? []).find((w) => w[0] === page && w[1] - 1 <= x && x <= w[3] + 1 && w[2] - 1 <= y && y <= w[4] + 1);
  /** Where `word` stands in a source line: its first whole occurrence, else null. Edge punctuation is not part of it. */
  function wordInLine(text, word) {
    const bare = word.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, '');
    if (!bare) return null;
    const re = new RegExp(`(?<![\\p{L}\\p{N}\\\\])${bare.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\p{L}\\p{N}])`, 'u').exec(text);
    return re ? [re.index, re.index + bare.length] : null;
  }
  $('pdf-views').addEventListener('click', (e) => { const view = e.target.closest('[data-view]'); if (view) { pdfView = view.dataset.view; pdfViewShow(); } });
  // a double click selects nothing on the pages (the browser paints the image and its overlays)
  pdfBody.addEventListener('mousedown', (e) => { if (e.detail > 1 && e.target.closest('.pdf-pages')) e.preventDefault(); });
  pdfBody.addEventListener('click', (e) => {
    if (e.target.closest('.pdf-owner button')) { compile(); return; }
    if ($('play-pdf').classList.contains('pdf-other')) { e.preventDefault(); return; }
    const chip = e.target.closest('.pdf-key');
    if (chip) { e.preventDefault(); pdfToTarget(pdfKeys[+chip.dataset.k]); return; }
    const rec = e.target.closest('.pdf-records li');
    if (rec) {
      const name = Object.keys(state.texts).find((n) => !(n in state.ro) && rec.dataset.path.endsWith(n));
      if (name && !pdf.slice) openPlace(name, { line: +rec.dataset.line - 1, character: 0 });
      return;
    }
    const link = e.target.closest('.pdf-link');
    if (link) {
      if (link.dataset.link) { e.preventDefault(); pdfJump(pdf.linkList[+link.dataset.link].to); }
      return;
    }
    // an overlay (a finding, an environment) over the page lets the click through to the page under it
    const img = e.target.closest('.pdf-page img') ?? (e.target.closest('.pdf-ov:not(.pdf-keys)') && e.target.closest('.pdf-page')?.querySelector('img'));
    if (!img || pdf.slice) return;
    const [w, h] = pageSize(img), r = img.getBoundingClientRect(), page = +img.parentElement.dataset.page;
    const x = (e.clientX - r.left) / r.width * w, y = (e.clientY - r.top) / r.height * h;
    pdfGo(page, x, y, pdfWordUnder(page, x, y));
  });
  /** The source of a point on a page opened in the editor, `word` (a word box) selected in its line when found there.
   * A word the line does not spell under a reference (its number) follows the reference, unless `follow` is false. */
  async function pdfGo(page, x, y, word, follow = true) {
    const hit = pdfSource(page, x, y, word);
    if (!hit) return;
    const text = state.texts[hit.file]?.split('\n')[hit.line - 1] ?? '';
    let span = word && wordInLine(text, word[5]);
    // a word the line does not spell (a reference's number, a citation's label) is a reference set there: followed
    const refs = follow && !span && word ? occurrences.filter((o) => ['ref', 'cite'].includes(o.kind) && occFile(o) === hit.file && o.line === hit.line).sort((a, b) => (a.col ?? 0) - (b.col ?? 0)) : [];
    if (refs.length) {
      const set = (pdf.wordList ?? []).filter((w) => w[0] === page && Math.abs(w[4] - word[4]) < 3 && !wordInLine(text, w[5])).sort((a, b) => a[1] - b[1]);
      return pdfToTarget(refs[Math.min(Math.max(set.indexOf(word), 0), refs.length - 1)]);
    }
    // a word set at the line but read elsewhere (`\title`'s argument, set by `\maketitle`): where it was read
    const from = !span && word ? readAt(hit.file, hit.line, word[5]) : null;
    if (from) {
      await openPlace(from.file, { line: from.line - 1, character: from.col - 1 }, { line: from.end[0] - 1, character: from.end[1] });
      return pdfFollow(true);
    }
    let start = { line: hit.line - 1, character: span?.[0] ?? 0 };
    // a word none of the user's files spells: the code run there set it (`\maketitle`'s date, `\today`, from the class)
    if (word && !span && /[\p{L}\p{N}]/u.test(word[5])) {
      const bare = word[5].replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ''), lines = state.texts[hit.file]?.split('\n') ?? [];
      // SyncTeX names the line where \maketitle's paragraph ended: it stands a few lines up
      const title = lines.slice(Math.max(0, hit.line - 4), hit.line).findLastIndex((l) => /^[^%]*\\maketitle\b/.test(l));
      const dated = title >= 0 && !Object.values(state.texts).some((t) => /^[^%\n]*\\date\s*\{/m.test(t) || t.includes(bare)) && /^(\p{L}+|\d{1,2}|\d{4})$/u.test(bare);
      if (dated) { start = { line: Math.max(0, hit.line - 4) + title, character: lines[Math.max(0, hit.line - 4) + title].indexOf('\\maketitle') }; span = [start.character, start.character + '\\maketitle'.length]; }
      toast(dated ? 'The date comes from the class: \\maketitle sets \\today, as no \\date{…} is given. Showing the \\maketitle line'
        : `“${bare}” is not written in your files: the code run at line ${hit.line} set it`);
    }
    await openPlace(hit.file, start, span ? { line: start.line, character: span[1] } : undefined);
    pdfFollow(true);
  }
  /** Where the pages stand: the page at the top of the view and how far down it, and the editor's place. */
  function pdfSpot() {
    const pages = pdfBody.querySelector('.pdf-pages'), top = pages.getBoundingClientRect().top;
    const page = [...pages.querySelectorAll('.pdf-page')].find((p) => p.getBoundingClientRect().bottom > top);
    const r = page?.getBoundingClientRect();
    return { page: +(page?.dataset.page ?? 1), y: r ? (top - r.top) / r.height : 0, file: state.file, at: editor.position() };
  }
  /** The pages as runs of one PDF page label: beamer gives the overlay steps of a frame the same one. Others stand alone. */
  function pdfFrames(pages) {
    // the ranges of /PageLabels: a page's label is its range's prefix and, with a numbering style, its number
    const ranges = pdf.pagelabels ?? [], runs = [], label = (i) => { const r = ranges.findLast((x) => x.index <= i); return r ? `${r.label['/P'] ?? ''}${r.label['/S'] ? (r.label['/St'] ?? 1) + i - r.index : ''}` : ''; };
    pages.forEach((_, i) => { const last = runs.at(-1); if (last && label(i) && label(i) === label(last.at(-1))) last.push(i); else runs.push([i]); });
    return runs;
  }
  /** Shows step `k` (or the one holding `page`) of a frame, the others hidden. */
  function pdfStep(frame, k) {
    const pagesOf = [...frame.querySelectorAll('.pdf-page')];
    k = Math.min(pagesOf.length - 1, Math.max(0, k));
    frame.dataset.step = String(k);
    pagesOf.forEach((p, i) => { p.hidden = i !== k; });
    frame.querySelector('.pdf-step').textContent = `${k + 1}/${pagesOf.length}`;
    frame.querySelector('[data-step="-1"]').disabled = k === 0;
    frame.querySelector('[data-step="1"]').disabled = k === pagesOf.length - 1;
  }
  const pdfReveal = (el) => { const f = el?.closest('.pdf-frame'); if (f && el.hidden) pdfStep(f, [...f.querySelectorAll('.pdf-page')].indexOf(el)); };
  pdfBody.addEventListener('click', (e) => { const b = e.target.closest('.pdf-frame-bar button'); if (b) { const f = b.closest('.pdf-frame'); pdfStep(f, +f.dataset.step + +b.dataset.step); } });
  /** Scrolls the pages to `y` (a fraction) of `page`, `x` into view when given. */
  function pdfScroll(page, y, x) {
    const pages = pdfBody.querySelector('.pdf-pages'), el = pages?.querySelector(`.pdf-page[data-page="${page}"]`);
    if (!el) return;
    pdfReveal(el);
    const a = el.getBoundingClientRect(), b = pages.getBoundingClientRect();
    pages.scrollTop += a.top + y * a.height - b.top;
    const left = x === undefined ? null : a.left + x * a.width;
    if (left !== null && (left < b.left || left > b.right - 40)) pages.scrollLeft += left - b.left - 16;
  }
  /** New pages: the editor's mark of an old place goes, a search runs again, the button says how it went. */
  function pdfLoaded(ok) {
    editor?.hold?.(null);
    if (!pdfFindBar.hidden) pdfFind(0);
    flare([pdfButtons[0], pdfStatus], ok);
  }
  /* Ctrl+F in the PDF pane searches the pages' words (`pdftotext -bbox`), not the editor's text. */
  const pdfFindBar = Object.assign(document.createElement('div'), { className: 'pdf-find', hidden: true, role: 'search',
    innerHTML: '<input type="search" placeholder="Find in the PDF" aria-label="find in the PDF" spellcheck="false"><span class="muted" aria-live="polite"></span>'
      + '<button type="button" class="small ghost" data-step="-1" title="previous (Shift+Enter)" aria-label="previous">↑</button><button type="button" class="small ghost" data-step="1" title="next (Enter)" aria-label="next">↓</button>'
      + '<button type="button" class="small ghost" data-step="close" title="close (Escape)" aria-label="close">×</button>' });
  $('play-pdf').append(pdfFindBar);
  const pdfFindInput = pdfFindBar.querySelector('input'), pdfFindCount = pdfFindBar.querySelector('span');
  let pdfHits = [], pdfHit = 0, pdfInside = false;
  /** Marks every hit of the query on the pages, then steps `step` from the current one and scrolls to it. */
  function pdfFind(step) {
    pdfBody.querySelectorAll('.pdf-hit').forEach((m) => m.remove());
    const q = pdfFindInput.value.trim().toLowerCase().replace(/\s+/g, ' '), ws = pdf?.wordList ?? [];
    if (step === 0) {
      // the words as one text, a space between two: a hit is the words its characters fall in
      let text = '';
      const starts = ws.map((w) => { const at = text.length; text += `${String(w[5]).toLowerCase()} `; return at; });
      pdfHits = [];
      for (let at = q ? text.indexOf(q) : -1; at >= 0 && pdfHits.length < 2000; at = text.indexOf(q, at + 1)) {
        const first = starts.findLastIndex((x) => x <= at), last = starts.findLastIndex((x) => x < at + q.length);
        pdfHits.push(ws.slice(first, last + 1));
      }
      pdfHit = Math.min(pdfHit, Math.max(pdfHits.length - 1, 0));
    } else if (pdfHits.length) pdfHit = (pdfHit + step + pdfHits.length) % pdfHits.length;
    pdfFindCount.textContent = !q ? '' : pdfHits.length ? `${pdfHit + 1} of ${pdfHits.length}` : 'no match';
    pdfFindBar.classList.toggle('none', !!q && !pdfHits.length);
    pdfHits.forEach((hit, i) => hit.forEach((w) => {
      const img = /** @type {HTMLImageElement | null} */ (pdfBody.querySelector(`.pdf-page[data-page="${w[0]}"] img`));
      if (!img?.naturalWidth) return;
      const [pw, ph] = pageSize(img);
      img.parentElement.insertAdjacentHTML('beforeend', `<div class="pdf-hit${i === pdfHit ? ' on' : ''}" style="${pdfPlace((w[1] - 1) / pw, (w[2] - 1) / ph, (w[3] - w[1] + 2) / pw, (w[4] - w[2] + 2) / ph)}"></div>`);
    }));
    const w = pdfHits[pdfHit]?.[0], img = w && /** @type {HTMLImageElement | null} */ (pdfBody.querySelector(`.pdf-page[data-page="${w[0]}"] img`));
    if (img?.naturalWidth) { const [pw, ph] = pageSize(img); pdfScroll(w[0], Math.max(0, w[2] / ph - 0.15), w[1] / pw); }
  }
  const pdfFindClose = () => { pdfFindBar.hidden = true; pdfBody.querySelectorAll('.pdf-hit').forEach((m) => m.remove()); };
  pdfFindInput.addEventListener('input', () => { pdfHit = 0; pdfFind(0); });
  pdfFindInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); pdfFind(e.shiftKey ? -1 : 1); } else if (e.key === 'Escape') { e.preventDefault(); pdfFindClose(); }
  });
  pdfFindBar.addEventListener('click', (e) => { const b = e.target.closest('[data-step]'); if (b) b.dataset.step === 'close' ? pdfFindClose() : pdfFind(+b.dataset.step); });
  // the PDF pane has Ctrl+F while the last click was in it (a click on a page moves the cursor to the editor),
  // until something is typed elsewhere: then Ctrl+F is the editor's again
  addEventListener('pointerdown', (e) => { pdfInside = !!e.target.closest?.('#play-pdf'); }, true);
  addEventListener('keydown', (e) => {
    const find = (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'f';
    if (!find && e.key.length === 1 && !e.target.closest?.('#play-pdf')) pdfInside = false;
    if (!find || $('play-pdf').hidden || !pdf?.wordList) return;
    if (!(e.target.closest?.('#play-pdf') || pdfInside && !e.target.closest?.('input, textarea'))) return;
    e.preventDefault();
    e.stopPropagation();
    pdfFindBar.hidden = false;
    pdfFindInput.focus();
    pdfFindInput.select();
    pdfFind(0);
  }, true); // before the editor's own Ctrl+F
  /** Follows an internal link: the pages scroll to its target, marked a moment, the editor to its source. Back returns. */
  async function pdfJump(to) {
    pdfBack.push(pdfSpot());
    pdfBackButton.hidden = false;
    const el = pdfBody.querySelector(`.pdf-page[data-page="${to.page}"]`), img = el?.querySelector('img');
    if (!el) return;
    if (img?.naturalWidth && !pdf.slice) {
      // the source of the first word at or below the target
      const [w, h] = pageSize(img), y = to.y * h;
      const below = (pdf.wordList ?? []).filter((v) => v[0] === to.page && v[4] >= y).sort((a, b) => a[4] - b[4] || a[1] - b[1]);
      const word = below.find((v) => v[2] <= below[0][4]);
      if (word) await pdfGo(to.page, word[1] + 1, word[4] - 1, word, false);
    }
    pdfBody.querySelectorAll('.pdf-target').forEach((m) => m.remove());
    el.insertAdjacentHTML('beforeend', `<div class="pdf-target" style="left:${to.x * 100}%;top:${to.y * 100}%"></div>`);
    pdfScroll(to.page, Math.max(0, to.y - 0.02), to.x);
  }
  /** Back to the place before the last link followed. */
  async function pdfReturn() {
    const spot = pdfBack.pop();
    pdfBackButton.hidden = !pdfBack.length;
    if (!spot) return;
    if (spot.file in state.texts && !pdf.slice) { await openPlace(spot.file, spot.at); pdfFollow(true); }
    pdfScroll(spot.page, spot.y);
  }
  pdfBackButton.onclick = (e) => { e.preventDefault(); pdfReturn(); };
  /** An occurrence's file as the project names it, else its base name (a .bbl the build wrote). */
  const occFile = (o) => { const p = String(o.path ?? o.file ?? ''); return Object.keys(state.texts).find((n) => !(n in state.ro) && p.endsWith(n)) ?? p.split('/').pop(); };
  /** Where an occurrence stands on the pages: the first row SyncTeX gives its line. */
  const occRect = (o) => pdfRects(occFile(o), o.line)[0];
  /** The keys placed on the pages, as the overlay's chips index them. */
  let pdfKeys = [];
  /** The view tabs fade at the bar's edge while some stand past it. */
  /** Where the tabs do not fit the bar, the views are one menu (a select: keyboard and touch for free). */
  const pdfViewsSel = Object.assign(document.createElement('select'), { id: 'pdf-views-sel', title: 'The view of the build' });
  pdfViewsSel.setAttribute('aria-label', 'view');
  pdfViewsSel.onchange = () => { pdfView = pdfViewsSel.value; pdfViewShow(); };
  $('pdf-views').after(pdfViewsSel);
  const pdfViewsFit = () => {
    const v = $('pdf-views'), bar = v.parentElement;
    bar.classList.remove('views-menu');
    if (v.scrollWidth > v.clientWidth + 1 || bar.scrollWidth > bar.clientWidth + 1) bar.classList.add('views-menu');
  };
  new ResizeObserver(pdfViewsFit).observe($('pdf-views').parentElement);
  new MutationObserver(pdfViewsFit).observe($('pdf-views'), { childList: true, subtree: true });
  /** The layers drawn over the pages, each switched on its own and remembered per viewer. */
  const PDF_LAYERS = {
    envs: 'Where each environment begins and ends',
    keys: 'Labels, bibliography entries, references and citations where they are set: click a reference to follow it',
    glue: 'Glue that stretches in a line (\\hfill, \\hspace{\\fill}, \\hskip … plus …) where SyncTeX places it, its spec on hover',
    geometry: 'The text block the lines fill, the margins shaded',
    figures: 'The bounding box of each figure, table, picture, minipage … with its environment name',
    grid: 'A grid over each page from its top left corner: its step in any TeX unit, labelled along the edges',
    findings: 'The findings of the check and of the build, on the characters they are about',
    bad: 'Overfull and underfull boxes of the log, an overfull one by how much it sticks out',
  };
  const FIGURE_ENV = /^(figure|table|sidewaysfigure|sidewaystable|wrapfigure|wraptable|subfigure|tikzpicture|picture|minipage|lstlisting|tabular|center)\*?$/;
  /** Sub-filters per layer, from the data: [group, title, rows of [key, label, count]]. What the viewer switched off is kept as `layer|group|key`. */
  const PDF_FACETS = new Set(['keys', 'findings', 'envs', 'figures', 'glue', 'grid']);
  /** The grid's step (a TeX length) and how many steps lie between labels, kept per viewer. */
  var pdfGrid = { step: '1cm', every: 5 }; // var: a check may render before this line ran
  try { Object.assign(pdfGrid, self.satexStore?.get('pdf-grid')); } catch {}
  var pdfHide = new Set(); // var: a check may render before this line ran
  try { pdfHide = new Set(JSON.parse(localStorage.getItem('satex-pdf-facets') ?? '[]').map((k) => k.replace(/^(defs|refs)\|/, 'keys|'))); } catch {}
  /** Environments begun in class and package code (beamer's own nesting): off unless asked, kept per viewer. */
  var pdfInternal = false; // var: a check may render before this line ran
  try { pdfInternal = localStorage.getItem('satex-pdf-internal') === 'on'; } catch {}
  const pdfShown = (layer, group, key) => !pdfHide.has(`${layer}|${group}|${key}`);
  const DEF_KIND = ['label', 'bibitem'], REF_KIND = /^(ref|cite|gls|glossary|index|acr)/, isKey = (k) => DEF_KIND.includes(k) || REF_KIND.test(k);
  const facetTally = (items) => [...Map.groupBy(items, (x) => x[0])].map(([key, xs]) => [key, xs[0][1] ?? key, xs.length]).sort((a, b) => b[2] - a[2] || String(a[0]).localeCompare(String(b[0])));
  /** The findings drawn over the pages: the check's, then the build's, as [file, line, severity, text, range, code]. */
  const pdfFound = () => Object.keys(state.texts).filter((n) => !(n in state.ro)).flatMap((n) => (diagnostics[uri(n)] ?? []).map((d) => [n, d.range.start.line + 1, sev(d), `${d.code}: ${d.message}`, d.range, d.code]))
    .concat((pdf?.records ?? []).filter((r) => r.line).map((r) => [occFile(r), r.line, r.severity === 'error' ? 'error' : 'warning', `${r.code}: ${r.message} (the build)`, null, r.code]));
  /** The environments' extents, from the begin and end occurrences: [begin, end, file]. */
  function pdfEnvPairs() {
    const open = [], out = [];
    // a begin the user wrote has its \begin{name} in their file at the place the run read it (a macro's, like beamer's frame handling, only borders it)
    const at = (o) => (o.expanded && occFile(o.expanded) === occFile(o) ? { ...o, line: o.expanded.line, col: o.expanded.col } : o);
    const own = (b) => (state.texts[occFile(b)] ?? '').split('\n')[b.line - 1]?.includes(`\\begin{${b.key}}`);
    for (const o of occurrences.filter((o) => o.kind === 'begin-environment' || o.kind === 'end-environment').map(at).sort((a, b) => occFile(a).localeCompare(occFile(b)) || a.line - b.line || (a.col ?? 0) - (b.col ?? 0))) {
      if (o.kind === 'begin-environment') { open.push(o); continue; }
      const k = open.findLastIndex((b) => b.key === o.key && occFile(b) === occFile(o));
      if (k >= 0) out.push([...open.splice(k, 1), o]);
    }
    // beamer reads a frame's body again and again (once per overlay pass) and ends it where it began: the end is then the \end in the source,
    // a begin seen again is one environment, and the depth is the nesting of what is left
    const text = (f) => (state.texts[f] ?? '').split('\n');
    const endOf = (b, o) => {
      if (o.line > b.line) return o;
      let depth = 0;
      const lines = text(occFile(b));
      for (let l = b.line - 1; l < lines.length; l++) {
        for (const m of lines[l].matchAll(new RegExp(`\\\\(begin|end)\\{${b.key.replace(/[*]/g, '\\*')}\\}`, 'g'))) {
          depth += m[1] === 'begin' ? 1 : -1;
          if (!depth) return { ...o, line: l + 1 };
        }
      }
      return o;
    };
    const seen = new Set(), pairs = [];
    for (const [b, o] of out) {
      const e = endOf(b, o), key = `${occFile(b)}|${b.key}|${b.line}|${e.line}`;
      if (!seen.has(key)) { seen.add(key); pairs.push([b, e]); }
    }
    const kept = pdfInternal ? pairs : pairs.filter(([b]) => own(b));
    return kept.map(([b, o]) => [b, o, kept.filter(([b2, o2]) => b2 !== b && occFile(b2) === occFile(b) && b2.line <= b.line && o2.line >= o.line && (b2.line < b.line || o2.line > o.line)).length]);
  }
  /** The groups of a layer's sub-filters. */
  function pdfFacetGroups(layer) {
    const kinds = (pick) => facetTally(occurrences.filter((o) => pick(o.kind)).map((o) => [o.kind]));
    if (layer === 'keys') return [['kind', 'kind', kinds(isKey)]];
    if (layer === 'glue') return [['kind', 'kind', facetTally(pdfGlue().map((g) => [g.kind]))]];
    if (layer === 'findings') {
      const found = pdfFound();
      return [['severity', 'severity', facetTally(found.map((f) => [f[2]]))], ['rule', 'category and rule', facetTally(found.map((f) => [f[5], `${manifest.rules[f[5]]?.category ?? 'other'} · ${f[5]}`]))]];
    }
    const pairs = pdfEnvPairs();
    if (layer === 'envs') return [['name', 'name', facetTally(pairs.filter(([b]) => b.key !== 'document').map(([b]) => [b.key]))]];
    if (layer === 'figures') return [['environment', 'environment', facetTally(pairs.filter(([, o]) => FIGURE_ENV.test(o.key)).map(([, o]) => [o.key]))]];
    return [];
  }
  /** Fills each layer's fold, keeping the ones open that are; a click on a row never closes the menu. */
  function pdfFacetsDraw() {
    for (const d of $('pdf-layer-list').querySelectorAll('.layer-sub')) {
      const layer = d.dataset.sub, list = d.querySelector('.layer-sub-list');
      // the grid's fields hold what was typed: drawn once, never under the cursor
      if (layer === 'grid' && list.querySelector('[data-grid]')) continue;
      const internal = layer === 'envs' ? `<div class="pdf-sub-group"><label><input type="checkbox" data-internal${pdfInternal ? ' checked' : ''}>internal (class, packages)</label></div>` : '';
      const html = layer === 'grid' ? `<div class="pdf-sub-group pdf-grid-set"><label>step <input type="text" data-grid="step" value="${escape(pdfGrid.step)}" list="pdf-grid-units" size="6" spellcheck="false" title="a TeX length: 1cm, 5mm, 10pt, .5in, 36bp, 2pc …"></label>` +
        `<label>label every <input type="number" data-grid="every" value="${escape(pdfGrid.every)}" min="1" max="50"> steps</label>` +
        `<datalist id="pdf-grid-units">${['1cm', '5mm', '1mm', '10pt', '12pt', '1in', '.5in', '36bp', '1pc'].map((v) => `<option value="${v}">`).join('')}</datalist></div>` : pdfFacetGroups(layer).filter(([, , rows]) => rows.length).map(([g, title, rows]) => `<div class="pdf-sub-group"><span class="muted">${escape(title)}</span>${rows.map(([key, label, n]) => `<label><input type="checkbox" data-facet="${escape(`${layer}|${g}|${key}`)}"${pdfShown(layer, g, key) ? ' checked' : ''}>${escape(label)} <b class="count">${n}</b></label>`).join('')}</div>`).join('') + internal;
      $('pdf-layer-list').querySelector(`[data-fold="${layer}"]`).disabled = !html;
      if (list.dataset.html !== html) { list.innerHTML = html; list.dataset.html = html; }
    }
  }
  // kept per viewer; an older version kept them apart, definitions and references as two
  const pdfLegacy = {};
  try {
    const kept = JSON.parse(localStorage.getItem('satex-pdf-overlay') ?? '{}');
    Object.assign(pdfLegacy, kept, { keys: kept.keys ?? (kept.defs || kept.refs), bad: kept.boxes });
  } catch {}
  var pdfOverlayOn = self.satexLayers.state('pdf', Object.fromEntries(Object.keys(PDF_LAYERS).map((k) => [k, false])), self.satexStore, pdfLegacy); // var: a check may render before this line ran
  $('pdf-layer-list').innerHTML = self.satexLayers.html(PDF_LAYERS, pdfOverlayOn, { prefix: 'pdf-layer', facets: PDF_FACETS });
  self.satexLayers.wire($('pdf-layer-list'), 'pdf', pdfOverlayOn, self.satexStore, () => { pdfLayersCount(); pdfOverlay(); });
  // a layer's filters fold open under its group, the chevron stays beside its switch
  $('pdf-layer-list').addEventListener('click', (e) => {
    const t = e.target.closest('.layer-sub-toggle');
    if (!t) return;
    e.preventDefault();
    const sub = $('pdf-layer-list').querySelector(`.layer-sub[data-sub="${t.dataset.fold}"]`), open = sub.hidden;
    sub.hidden = !open;
    t.setAttribute('aria-expanded', String(open));
    if (open) pdfFacetsDraw();
  });
  /** "Near the pointer": layers show only within a radius of the mouse, fading with distance. Off by default, kept per viewer. */
  const NEAR_PX = 110;
  var pdfNear = false;
  try { pdfNear = localStorage.getItem('satex-pdf-near') === 'on'; } catch {}
  $('pdf-layer-list').insertAdjacentHTML('beforeend', `<label class="pdf-near" title="Draw the layers only around the mouse pointer, fading with distance"><input type="checkbox" data-near${pdfNear ? ' checked' : ''}>near the pointer</label>`);
  let nearFrame = 0, nearAt = null;
  function pdfNearDraw() {
    nearFrame = 0;
    pdfBody.classList.toggle('pdf-near', pdfNear);
    for (const el of pdfBody.querySelectorAll('.pdf-ov')) {
      if (!pdfNear) { el.style.opacity = ''; continue; }
      const r = el.getBoundingClientRect();
      const d = nearAt ? Math.hypot(Math.max(r.left - nearAt[0], 0, nearAt[0] - r.right), Math.max(r.top - nearAt[1], 0, nearAt[1] - r.bottom)) : Infinity;
      el.style.opacity = d <= NEAR_PX * 0.6 ? '1' : d >= NEAR_PX ? '0.02' : String((1 - ((d - NEAR_PX * 0.6) / (NEAR_PX * 0.4)) ** 0.5 * 0.98).toFixed(2));
    }
  }
  const pdfNearSoon = () => { nearFrame ||= requestAnimationFrame(pdfNearDraw); };
  pdfBody.addEventListener('pointermove', (e) => { nearAt = [e.clientX, e.clientY]; if (pdfNear) pdfNearSoon(); });
  pdfBody.addEventListener('pointerleave', () => { nearAt = null; if (pdfNear) pdfNearSoon(); });
  $('pdf-layer-list').addEventListener('change', (e) => {
    if (e.target.dataset?.near !== undefined) {
      pdfNear = e.target.checked;
      try { localStorage.setItem('satex-pdf-near', pdfNear ? 'on' : 'off'); } catch {}
      pdfNearDraw();
      return;
    }
    if (e.target.dataset?.internal !== undefined) {
      pdfInternal = e.target.checked;
      try { localStorage.setItem('satex-pdf-internal', pdfInternal ? 'on' : 'off'); } catch {}
      pdfOverlay();
      return;
    }
    if (e.target.dataset?.grid) {
      const k = e.target.dataset.grid, v = k === 'every' ? Math.round(+e.target.value) : e.target.value.trim();
      const ok = k === 'every' ? v >= 1 && v <= 50 : !!self.satexLayers.length(v);
      e.target.setCustomValidity(ok ? '' : k === 'every' ? 'a whole number from 1 to 50' : 'a TeX length above 0: 1cm, 5mm, 10pt, .5in …');
      e.target.reportValidity();
      if (!ok) return;
      pdfGrid[k] = v;
      self.satexStore?.set('pdf-grid', { ...pdfGrid });
      pdfOverlay();
      return;
    }
    if (e.target.dataset?.facet) {
      pdfHide[e.target.checked ? 'delete' : 'add'](e.target.dataset.facet);
      try { localStorage.setItem('satex-pdf-facets', JSON.stringify([...pdfHide])); } catch {}
      pdfOverlay();
      return;
    }
  });
  const pdfLayersCount = () => { const n = Object.values(pdfOverlayOn).filter(Boolean).length; $('pdf-layers').querySelector('summary').innerHTML = `&#x25A4; <span>${n ? `layers ${n}` : 'layers'}</span>`; };
  pdfLayersCount();
  /** The typeset characters whose source lies in `range` of `file` (LSP positions), by the sites the typeset query gives. */
  function typesetIn(file, range) {
    const at = (p) => [p.line + 1, p.character + 1], before = (a, b) => a[0] < b[0] || a[0] === b[0] && a[1] < b[1];
    let [from, to] = [at(range.start), at(range.end)];
    if (!before(from, to)) {
      // a point: the token or word there
      const w = wordAt(state.texts[file]?.split('\n')[range.start.line] ?? '', range.start.character);
      if (!w) return '';
      [from, to] = [[from[0], w[0] + 1], [from[0], w[1] + 1]];
    }
    return (typesetRows ?? []).map((r) => siteHits(r, file, (p) => !before(p, from) && before(p, to)).text).filter(Boolean).join(' ');
  }
  /** Where `word`, set while about line `line` of `file` was read (SyncTeX's line, within a few), was itself read (the
   * run's `origins`): its file, first line and column, and its last character's place; null when it was read where set. */
  function readAt(file, line, word) {
    const bare = word.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, '');
    let best = null, near = 4;
    for (const r of bare ? typesetRows ?? [] : []) {
      if (!r.sites || !r.origins) continue;
      const chars = [], marked = [...(r.marked ?? r.detail ?? '')];
      let k = 0, o = -1, i = 0, note = false, math = false;
      for (let j = 0; j < marked.length; j++) {
        const c = marked[j];
        if (c === '\uE000') { note = true; math = marked[j + 1] === '\u0006'; continue; }
        if (c === '\uE001') { note = math = false; continue; }
        if (math) continue;
        while (k + 1 < r.sites.length && r.sites[k + 1].at <= i) k++;
        while (o + 1 < r.origins.length && r.origins[o + 1].at <= i) o++;
        const s = r.sites[k], g = r.origins[o];
        if (!note && s && s.at <= i && String(s.path ?? r.path ?? '').endsWith(file)) {
          chars.push([c, s.line, g?.line ? { path: g.path ?? r.path, line: g.line, col: g.col + i - g.at } : null]);
        }
        i++;
      }
      const text = chars.map((c) => c[0]).join('');
      for (let at = text.indexOf(bare); at >= 0; at = text.indexOf(bare, at + 1)) {
        const first = chars[at], last = chars[at + bare.length - 1], d = Math.abs(first[1] - line);
        const name = first[2] && last[2] && Object.keys(state.texts).find((n) => String(first[2].path).endsWith(n));
        if (name && d < near) { near = d; best = { file: name, line: first[2].line, col: first[2].col, end: [last[2].line, last[2].col] }; }
      }
    }
    return best;
  }
  /** The characters of row `r` whose source site (the run's `sites`: where the user's file was read when it was set)
   * `wanted` takes: their text, and the first and last one's offset in `detail`. */
  function siteHits(r, file, wanted) {
    if (!r.sites) return { text: '' };
    // `marked` is `detail` with its notes bracketed: a note (a hyphen's ·) is no text, a math note not even in `detail`
    const marked = [...(r.marked ?? r.detail ?? '')];
    // `i` counts as `sites` do (a note's own characters too), `d` as `detail` does
    let k = 0, i = 0, d = 0, hit = '', note = false, math = false, first = -1, last = -1;
    for (let j = 0; j < marked.length; j++) {
      const c = marked[j];
      if (c === '\uE000') { note = true; math = marked[j + 1] === '\u0006'; continue; }
      if (c === '\uE001') { note = math = false; continue; }
      if (math) continue;
      while (k + 1 < r.sites.length && r.sites[k + 1].at <= i) k++;
      const s = r.sites[k], p = s && [s.line, s.col + (s.same ? 0 : i - s.at)];
      const on = !note && s && s.at <= i && String(s.path ?? r.path ?? '').endsWith(file) && wanted(p);
      hit += on ? c : ' ';
      if (on && c.trim()) { if (first < 0) first = d; last = d; }
      if (!note) d++;
      i++;
    }
    return { text: hit.trim().replace(/\s+/g, ' '), first, last };
  }
  /** The words of the rows `rects`, in reading order. */
  const pdfRowWords = (rects) => rects.flatMap((r) => (pdf.wordList ?? []).filter((w) => w[0] === r.page && w[4] >= r.t && w[2] <= r.d && w[3] > r.l - 2 && w[1] < r.r + 2))
    .sort((a, b) => a[0] - b[0] || a[2] - b[2] || a[1] - b[1]);
  /** Where `text` (typeset words) stands among the rows' words: one box per row it covers, else null. */
  function pdfWordsOf(rects, text) {
    const bare = (t) => t.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ''), want = text.split(/\s+/).map(bare).filter(Boolean), words = pdfRowWords(rects);
    if (!want.length) return null;
    const fits = (w, t) => { const b = bare(w[5]); return b && (b.includes(t) || t.includes(b)); };
    for (let i = 0; i + want.length <= words.length; i++) {
      if (!want.every((t, j) => fits(words[i + j], t))) continue;
      const rows = new Map();
      for (const w of words.slice(i, i + want.length)) {
        const key = `${w[0]}:${Math.round(w[4])}`, o = rows.get(key);
        rows.set(key, { page: w[0], l: Math.min(o?.l ?? w[1], w[1]), t: Math.min(o?.t ?? w[2], w[2]), r: Math.max(o?.r ?? w[3], w[3]), d: Math.max(o?.d ?? w[4], w[4]) });
      }
      return [...rows.values()];
    }
    return null;
  }
  /** The glue the run set in the user's lines (its `fill` and `hskip` marks), where SyncTeX placed glue on their source
   * lines: as many as the run set there, the widest first and infinite glue widest. Stretched glue may be a kern record
   * (its width, ending at its x), other glue a point, up to the row's next record. Glue between lines has no record.
   * In bp: {page, l, t, r, d, file, line, kind, spec}. */
  function pdfGlue() {
    const want = new Map();
    for (const r of typesetRows ?? []) {
      for (const m of r.align ?? []) {
        const g = /^(fill|hskip) (.+)$/.exec(m.mark ?? ''), s = g && r.sites?.length && (r.sites.find((x) => x.at >= m.at) ?? r.sites.at(-1));
        if (!s) continue;
        const path = String(s.path ?? r.path ?? ''), key = `${path}|${s.line}`;
        if (!want.has(key)) want.set(key, { path, line: s.line, marks: [] });
        want.get(key).marks.push([g[1], g[2]]);
      }
    }
    const boxes = pdf?.boxes ?? [], out = [];
    const near = (a, b) => a.page === b.page && Math.abs(a.y - b.y) < 0.5;
    const span = (b) => b.kind === 'k' ? [b.x - b.w, b.x] : [b.x, Math.min(...boxes.filter((o) => near(o, b) && o.x > b.x + 0.01).map((o) => o.x), ...boxes.filter((o) => o.kind === '(' && near(o, b)).map((o) => o.x + o.w))];
    for (const w of want.values()) {
      const marks = w.marks.sort((a, b) => (a[0] === 'fill' ? 0 : 1) - (b[0] === 'fill' ? 0 : 1));
      const own = boxes.filter((b) => b.line === w.line && w.path.endsWith(b.file) && (b.kind === 'k' && b.w > 0 || b.kind === 'g'));
      const wide = own.map((b) => [b, span(b)]).filter(([, [l, r]]) => Number.isFinite(r) && r > l).sort((a, b) => (b[1][1] - b[1][0]) - (a[1][1] - a[1][0]));
      wide.slice(0, marks.length).forEach(([b, [l, r]], i) => {
        // a low bracket on the baseline, as the Typeset view draws it
        out.push({ page: b.page, l, r, t: b.y - 3.5, d: b.y + 0.5, file: b.file, line: w.line, kind: marks[i][0], spec: marks[i][1] });
      });
    }
    return out;
  }
  /** The text block of each page, from the lines SyncTeX places: the most common left edge and width of its line boxes, from the first to the last. */
  function pdfBlocks() {
    const out = new Map(), mode = (xs) => { const n = new Map(); for (const x of xs) n.set(x, (n.get(x) ?? 0) + 1); return [...n].sort((a, b) => b[1] - a[1])[0]?.[0]; };
    for (const [page, lines] of Map.groupBy((pdf.boxes ?? []).filter((b) => b.kind === '(' && b.w > 0), (b) => b.page)) {
      const left = mode(lines.map((b) => Math.round(b.x)));
      const width = mode(lines.filter((b) => Math.round(b.x) === left).map((b) => Math.round(b.w)));
      if (left === undefined || !width) continue;
      out.set(page, { l: left, r: left + width, t: Math.min(...lines.map((b) => b.y - b.h)), d: Math.max(...lines.map((b) => b.y + b.d)) });
    }
    return out;
  }
  /** Over the pages, each when switched on: keys (definitions apart from references), findings on their characters,
   * environments, the text block and the bad boxes of the log. Placed by SyncTeX, so a slice has none. */
  function pdfOverlay() {
    shownFor(pdfBody, uri(), diagnosticVersions[uri()], 'ov-stale');
    pdfFacetsDraw();
    pdfOverlayDraw();
    for (const [k, cls] of [['envs', 'pdf-env'], ['figures', 'pdf-fig'], ['findings', 'pdf-finding'], ['glue', 'pdf-glue']]) {
      self.satexLayers.count($('pdf-layer-list'), k, pdfOverlayOn[k] && pdfBody.querySelectorAll(`.pdf-ov.${cls}`).length);
    }
    pdfLabelsFit();
    if (pdfNear) pdfNearSoon();
  }
  /** The overlay as a diff: an element whose page and markup are unchanged stays, so a toggled layer or an update that changes nothing touches nothing else. */
  function pdfOverlayDraw() {
    if (!pdf?.boxes || !pdfBody.querySelector('.pdf-pages')) return;
    const wanted = [];
    if (!pdf.slice) pdfOverlayBuild((page, html) => wanted.push([page, html]));
    const left = new Map();
    for (const [page, html] of wanted) left.set(`${page}|${html}`, (left.get(`${page}|${html}`) ?? 0) + 1);
    for (const el of pdfBody.querySelectorAll('.pdf-ov')) {
      const key = el.dataset.ovKey ?? '', n = left.get(key) ?? 0;
      if (n > 0) left.set(key, n - 1); else el.remove();
    }
    for (const [page, html] of wanted) {
      const key = `${page}|${html}`;
      if (!left.get(key)) continue;
      left.set(key, left.get(key) - 1);
      const host = pdfBody.querySelector(`.pdf-page[data-page="${page}"]`);
      host?.insertAdjacentHTML('beforeend', html);
      const el = host?.lastElementChild;
      if (el) el.dataset.ovKey = key;
    }
  }
  function pdfOverlayBuild(put) {
    const size = (page) => { const img = /** @type {HTMLImageElement | null} */ (pdfBody.querySelector(`.pdf-page[data-page="${page}"] img`)); return img?.naturalWidth ? pageSize(img) : null; };
    /** A box in bp on `page`, as an overlay element of class `cls`. */
    const box = (page, cls, l, t, r, d, title = '', inner = '') => {
      const wh = size(page);
      if (wh) put(page, `<span class="pdf-ov ${cls}" style="${pdfPlace(l / wh[0], t / wh[1], (r - l) / wh[0], (d - t) / wh[1])}"${title ? ` title="${escape(title)}"` : ''}>${inner}</span>`);
    };
    const blocks = pdfBlocks(), rowsOf = (file, a, b) => { const out = []; for (let l = a; l <= b; l++) out.push(...pdfRects(file, l, true)); return out; };
    pdfKeys = [];
    const wantKind = (k) => isKey(k) && pdfOverlayOn.keys && pdfShown('keys', 'kind', k);
    for (const os of Map.groupBy(occurrences.filter((o) => wantKind(o.kind)), (o) => `${occFile(o)}:${o.line}`).values()) {
      const r = occRect(os[0]), wh = r && size(r.page);
      if (!wh) continue;
      const chips = os.sort((a, b) => (a.col ?? 0) - (b.col ?? 0)).map((o) => `<a href="#" class="pdf-key k-${o.kind}" data-k="${pdfKeys.push(o) - 1}" title="${['label', 'bibitem'].includes(o.kind) ? 'defines' : 'refers to'} ${escape(o.key)} (${o.kind} at ${escape(occFile(o))}:${o.line})${['label', 'bibitem'].includes(o.kind) ? '' : ': click to go to its target'}">${escape(o.key)}</a>`).join('');
      put(r.page, `<span class="pdf-ov pdf-keys" style="left:${r.l / wh[0] * 100}%;top:${r.t / wh[1] * 100}%">${chips}</span>`);
    }
    if (pdfOverlayOn.findings) {
      for (const [n, line, s, what, range, code] of pdfFound().filter((f) => pdfShown('findings', 'severity', f[2]) && pdfShown('findings', 'rule', f[5]))) {
        const rows = pdfRects(n, line), text = range && rows.length ? typesetIn(n, range) : '';
        for (const r of (text && pdfWordsOf(rows, text)) || rows.slice(0, 1)) box(r.page, `pdf-finding f-${s}`, r.l - 1, r.t - 1, r.r + 1, r.d + 2, `${n}:${line}${text ? ` · ${text}` : ''}\n${what}`);
      }
    }
    if (pdfOverlayOn.envs) {
      for (const [b, o, depth] of pdfEnvPairs()) {
        const file = occFile(o);
        if (b.key === 'document' || !pdfShown('envs', 'name', b.key)) continue;
        // the lines inside: the begin and end lines carry what closed there (a whole float)
        const inner = o.line - b.line > 1 ? rowsOf(file, b.line + 1, o.line - 1) : [], rows = inner.length ? inner : rowsOf(file, b.line, o.line);
        if (!rows.length) continue;
        for (const [page, rs] of Map.groupBy(rows, (r) => r.page)) {
          const g = blocks.get(page), t = Math.min(...rs.map((r) => r.t)) - 2, d = Math.max(...rs.map((r) => r.d)) + 2;
          const l = (g?.l ?? Math.min(...rs.map((r) => r.l))) - 6 - 4 * depth, r = (g?.r ?? Math.max(...rs.map((x) => x.r))) + 2;
          box(page, 'pdf-env', l, t, r, d, `${b.key}: lines ${b.line} to ${o.line} of ${file}`, `<b>${escape(b.key)}</b>`);
        }
      }
    }
    if (pdfOverlayOn.figures) {
      // the extent of an environment is the box SyncTeX records on its \end line: the largest list there
      for (const [, o] of pdfEnvPairs()) {
        if (!FIGURE_ENV.test(o.key) || !pdfShown('figures', 'environment', o.key)) continue;
        const best = (pdf.boxes ?? []).filter((b) => b.file === occFile(o) && b.line === o.line && '[('.includes(b.kind) && b.w > 0 && b.h + b.d > 0).sort((a, b) => b.w * (b.h + b.d) - a.w * (a.h + a.d))[0];
        if (best) box(best.page, 'pdf-fig', best.x, best.y - best.h, best.x + best.w, best.y + best.d, `bounding box of ${o.key}, ends at line ${o.line} of ${occFile(o)}`, `<b>${escape(o.key)}</b>`);
      }
    }
    if (pdfOverlayOn.geometry) {
      for (const [page, g] of blocks) {
        const wh = size(page);
        if (!wh) continue;
        const pt = (bp) => (bp * 72.27 / 72).toFixed(0);
        for (const [l, t, r, d] of [[0, 0, wh[0], g.t], [0, g.d, wh[0], wh[1]], [0, g.t, g.l, g.d], [g.r, g.t, wh[0], g.d]]) box(page, 'pdf-margin', l, t, r, d, 'margin');
        box(page, 'pdf-block', g.l, g.t, g.r, g.d, `text block: ${lengths((g.r - g.l) * 72.27 / 72)} wide, its lines from ${pt(g.t)}pt to ${pt(g.d)}pt from the top`, `<b data-pt="${(g.r - g.l) * 72.27 / 72}">${pt(g.r - g.l)}pt</b>`);
      }
    }
    if (pdfOverlayOn.glue) {
      for (const g of pdfGlue().filter((g) => pdfShown('glue', 'kind', g.kind))) box(g.page, `pdf-glue g-${g.kind}`, g.l, g.t, g.r, g.d, `${g.kind === 'fill' ? 'infinite glue' : 'glue'} ${g.spec} · ${g.file}:${g.line}`);
    }
    if (pdfOverlayOn.grid) {
      // lines every step from the top left corner, stronger and labelled every `every` steps; fewer when they would crowd
      const step = self.satexLayers.length(pdfGrid.step) ?? self.satexLayers.length('1cm'), every = Math.max(1, Math.round(pdfGrid.every) || 5);
      const s = step.pt * 72 / 72.27, num = (k) => `${+(k * step.num).toFixed(3)}${step.unit}`;
      for (const page of [...pdfBody.querySelectorAll('.pdf-page[data-page]')].map((p) => +p.dataset.page)) {
        const wh = size(page);
        if (!wh || wh[0] / s > 2000) continue;
        const minor = wh[0] / s <= 200, labels = [];
        for (let k = every; k * s < wh[0]; k += every) labels.push(`<b class="gx" style="left:${k * s / wh[0] * 100}%">${num(k)}</b>`);
        for (let k = every; k * s < wh[1]; k += every) labels.push(`<b class="gy" style="top:${k * s / wh[1] * 100}%">${num(k)}</b>`);
        const at = `--gx:${s / wh[0] * 100}%;--gy:${s / wh[1] * 100}%;--Gx:${every * s / wh[0] * 100}%;--Gy:${every * s / wh[1] * 100}%`;
        put(page, `<span class="pdf-ov pdf-grid${minor ? '' : ' g-coarse'}" style="${at}" title="grid: ${escape(num(1))} steps, labelled every ${escape(num(every))}">${labels.join('')}</span>`);
      }
    }
    if (pdfOverlayOn.bad) {
      for (const rec of pdf.records ?? []) {
        const m = /^(Over|Under)full \\([hv])box \((?:([\d.]+)pt too \w+|badness (\d+))\).*?lines? (\d+)(?:--(\d+))?/.exec(rec.message ?? '');
        if (!m) continue;
        const [, kind, hv, by, , a, b] = m, file = occFile(rec), rows = rowsOf(file, +a, +(b ?? a));
        if (!rows.length) continue;
        const over = kind === 'Over', what = `${rec.message.replace(/\s*\[\].*$/, '')} (${file})`;
        if (hv === 'h') {
          // the line that sticks out most is the overfull one
          const r = over ? rows.reduce((x, y) => (y.r > x.r ? y : x)) : rows[0], g = blocks.get(r.page), edge = g?.r ?? r.r;
          box(r.page, `pdf-bad ${over ? 'b-over' : 'b-under'}`, r.l - 1, r.t - 1, Math.max(r.r, edge) + 1, r.d + 2, what);
          if (over) box(r.page, 'pdf-bar-out', edge, r.t - 1, edge + Math.max(+by * 72 / 72.27, 1.5), r.d + 2, `${lengths(+by)} too wide`, `<b>${by}pt</b>`);
        } else {
          const r = rows.at(-1), g = blocks.get(r.page);
          box(r.page, `pdf-bad ${over ? 'b-over' : 'b-under'}`, g?.l ?? r.l, r.t - 1, g?.r ?? r.r, r.d + 2, what);
          if (over && g) box(r.page, 'pdf-bar-out pdf-bar-v', g.l, g.d, g.r, g.d + Math.max(+by * 72 / 72.27, 1.5), `${lengths(+by)} too high`, `<b>${by}pt</b>`);
        }
      }
    }
  }
  /** Keeps the labels over the pages from overdrawing: a chip equal to one just placed goes, one that hits another moves up by its height. */
  function pdfLabelsFit() {
    for (const page of pdfBody.querySelectorAll('.pdf-page')) {
      const placed = [], seen = new Set();
      for (const el of page.querySelectorAll('.pdf-env > b, .pdf-keys')) {
        el.style.transform = '';
        el.hidden = false;
        const r = el.getBoundingClientRect();
        if (!r.width) continue;
        const id = `${el.tagName}${el.textContent}${Math.round(r.left / 4)}${Math.round(r.top / 4)}`;
        if (seen.has(id)) { el.hidden = true; continue; }
        seen.add(id);
        let top = r.top, hit;
        for (let n = 0; n < 6 && (hit = placed.find((p) => r.left < p.right && r.right > p.left && top < p.bottom && top + r.height > p.top)); n++) top = hit.top - r.height;
        if (top !== r.top) el.style.transform = `translateY(calc(-100% + ${top - r.top}px))`;
        placed.push({ left: r.left, right: r.right, top, bottom: top + r.height });
      }
    }
  }
  /** Follows a reference or citation (`o`, an occurrence) to what it names: the editor to its definition,
   * the pages to where that is set (a label's line, the bibliography's entry). Back returns. A label opens itself. */
  async function pdfToTarget(o) {
    if (!o) return;
    const from = pdfSpot(), name = occFile(o), at = { line: o.line - 1, character: Math.max((o.col ?? 1) - 1, 0) };
    const kind = { ref: 'label', cite: 'bibitem' }[o.kind];
    const target = kind && occurrences.find((x) => x.kind === kind && x.key === o.key);
    // the definition of the key, asked where the key stands (on `\\ref` it would be the command's)
    const keyAt = (state.texts[name]?.split('\n')[at.line] ?? '').indexOf(o.key, at.character);
    const def = kind && keyAt >= 0 ? (await lsp('textDocument/definition', { textDocument: { uri: uri(name) }, position: { line: at.line, character: keyAt + 1 } }).catch(() => null)) : null;
    const loc = [def].flat().find(Boolean);
    if (kind && !target && !loc) return toast(`${o.key}: no ${kind === 'label' ? '\\label' : 'bibliography entry'} found`);
    pdfBack.push(from);
    pdfBackButton.hidden = false;
    if (loc) await openPlace(decodeURIComponent(new URL(loc.uri ?? loc.targetUri).pathname), (loc.range ?? loc.targetSelectionRange).start);
    else if (target && occFile(target) in state.texts) await openPlace(occFile(target), { line: target.line - 1, character: Math.max((target.col ?? 1) - 1, 0) });
    else if (!kind) await openPlace(name, at);
    const r = occRect(target ?? o), img = r && /** @type {HTMLImageElement | null} */ (pdfBody.querySelector(`.pdf-page[data-page="${r.page}"] img`));
    if (!img?.naturalWidth) return;
    const [w, h] = pageSize(img);
    pdfBody.querySelectorAll('.pdf-target').forEach((m) => m.remove());
    img.parentElement.insertAdjacentHTML('beforeend', `<div class="pdf-target" style="left:${r.l / w * 100}%;top:${r.t / h * 100}%"></div>`);
    pdfScroll(r.page, Math.max(0, r.t / h - 0.05), r.l / w);
  }
  pdfBody.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target.matches('.pdf-records li')) e.target.click(); });
  $('play-pdf').addEventListener('keydown', (e) => { if (e.altKey && e.key === 'ArrowLeft' && pdfBack.length) { e.preventDefault(); pdfReturn(); } });
  async function written(target) {
    const request = { file: path(state.root ?? state.file), queries: [{ type: 'files' }, { type: 'dependencies' }] };
    const [files, [read, loads] = []] = await Promise.all([lsp('satex/written', { textDocument: { uri: target } }).catch(() => null),
      lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).then((r) => r?.map((q) => q.records)).catch(() => null)]);
    if (!files) return;
    writtenFiles = files;
    readFiles = read ?? readFiles;
    libraries = loads ?? libraries;
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
        return `<span title="${escape(num(n))} ${escape(label)}${RUN_MEANS[label.trim()] ? `: ${RUN_MEANS[label.trim()]}` : ''}"><b>${escape(Number(n) >= 1e4 ? short(n) : num(n))}</b>${escape(label.replace(/\d{5,}/g, short))}</span>`;
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
    forks: 'how often a path split in two at a conditional or value SaTeX could not decide. Each split adds one path',
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
    // counts as the findings panel shows them: package code and switched-off rules leave none
    const shown = new Map();
    for (const d of Object.values(diagnostics).flat()) shown.set(d.code, (shown.get(d.code) ?? 0) + 1);
    const all = texts.map((t) => t.match(/^rule (\S+) (\d+) (\d+)$/)).filter(Boolean).map(([, code, us]) => [code, +us, shown.get(code) ?? 0]);
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
  const CACHE_KINDS = { format: ['kernel', 'kernel'], package: ['package', 'package'], index: ['file index', 'index'], probe: ['installation facts', 'facts'], 'content-hashes': ['file hashes', 'hashes'], 'previews.json': ['previews.json', 'previews'] };
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
      `<tr><td title="${escape(e.path)}">${cacheBadge(cls, kind)}</td>` +
      `<td><button type="button" class="cache-name" aria-expanded="${via.length > 0}" aria-controls="cache-sub-${i}">${escape(what || kind)}</button>${(cls === 'package' || cls === 'class') && manualPackages().has(what) ? ' <span class="manual" title="the package\'s manual is mined for descriptions">docs</span>' : ''}${exported(cls, what)}${hits('defines', hit)}${hits('loads', via)}${CACHE_NOTES[cls] ? ` <span class="muted cache-hit">${CACHE_NOTES[cls]}</span>` : ''}</td><td class="used">${u[1] || '<span class="use-no" title="not used by the open document">—</span>'}</td><td class="loads">${needs.length ? `<span class="cache-fold" title="it loads ${needs.length}">${needs.length}</span>` : ''}</td>` +
      `<td class="num">${size(e.size)}</td><td><button type="button" class="small ghost" data-path="${escape(e.path)}" aria-label="drop ${escape(what || kind)}">Drop</button></td></tr>` +
      `<tr class="cache-sub" id="cache-sub-${i}"${via.length ? '' : ' hidden'}><td></td><td colspan="5">${details(cls, what, needs)}</td></tr>`).join('') ||
      `<tr><td colspan="6" class="muted">${cacheEntries.length ? 'No match.' : 'Nothing cached yet.'}</td></tr>`;
    // the packages and classes listed, and how many of them have their manual mined
    const pkgs = new Set(rows.filter(({ l }) => l[1] === 'package' || l[1] === 'class').map(({ l }) => l[2])), manual = manualPackages();
    $('cache-total').textContent = rows.length ? `${rows.length} ${rows.length === 1 ? 'entry' : 'entries'}, ${size(rows.reduce((n, { e }) => n + e.size, 0))} in total`
      + (pkgs.size ? ` · manual information for ${num([...pkgs].filter((p) => manual.has(p)).length)} of ${plural(pkgs.size, 'package')}` : '') : '';
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
  const USE_HOW = { always: 'always: every run, or the format', direct: 'directly: the document loads it', indirect: 'indirectly: another package loads it' };
  /** One marker for every kind: [used, html] of an entry for the open document. */
  const usedOf = (cls, what) => {
    // how it comes in: always (every run, the format), directly (the document loads it), indirectly (another package does)
    const mark = (text, how, title = '') => [true, `<span class="use use-yes use-${how}" title="${escape(`${USE_HOW[how]}${title ? `: ${title}` : ''}`)}">${text}</span>`];
    if (cls === 'kernel') return what.startsWith(manifest.kernels[state.kernel]?.id ?? '\0') ? mark('the run\'s format', 'always') : [false, ''];
    if (cls === 'index') return indexed ? mark('file lookups', 'always') : [false, ''];
    if (cls === 'facts' || cls === 'hashes' || cls === 'previews') return mark('every run', 'always');
    const u = used.get(`${cls}:${what}`);
    if (!u) return [false, ''];
    const [yes, html] = u.own ? mark('by the document', 'direct') : mark(`via ${escape([...u.by].slice(0, 2).join(', '))}${u.by.size > 2 ? ` +${u.by.size - 2}` : ''}`, 'indirect', `loaded by ${[...u.by].join(', ')}`);
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
      if (r.tag === 'macro' && /^\\color@./.test(r.name)) {
        (d.colorList ??= []).push([r.name.slice(7), swatchOf(r.body)]);
        d.color = (d.color ?? 0) + 1;
        defined.set(r.package ?? null, d);
        continue;
      }
      const internal = /[@_:]/.test(r.name);
      const k = r.tag === 'macro' ? (internal ? 'internal macro' : 'public command') : r.tag === 'alias' ? (internal ? 'internal alias' : 'public alias') : r.tag;
      d[k] = (d[k] ?? 0) + 1;
      (d.names ??= new Set()).add(r.name);
      if (k === 'public command') (d.commands ??= []).push(r.name);
      defined.set(r.package ?? null, d);
    }
    indexed = (dist?.records ?? []).find((r) => r.indexed != null) ?? null;
    folders = (dist?.records ?? []).filter((r) => r.kind === 'directory');
  }
  /** The CSS color of an xcolor store such as `\color {rgb}{1,0,0}` (core models only), else null. */
  function swatchOf(body) {
    const [, model, spec] = /\{(rgb|cmyk|cmy|gray)\}\s*\{([^}]*)\}/.exec(body ?? '') ?? [];
    const v = (spec ?? '').split(',').map(Number);
    if (!model || v.some(Number.isNaN)) return null;
    const c = (x) => Math.round(255 * Math.min(1, Math.max(0, x)));
    const [r, g, b] = model === 'rgb' ? v.map(c) : model === 'gray' ? [c(v[0]), c(v[0]), c(v[0])]
      : model === 'cmy' ? v.map((x) => c(1 - x)) : [0, 1, 2].map((i) => c((1 - v[i]) * (1 - (v[3] ?? 0))));
    return `rgb(${r} ${g} ${b})`;
  }
  const colorSwatches = (list) => `<details class="cache-colors"><summary>defines ${plural(list.length, 'color')}</summary><ul>${list.map(([n, css]) => `<li title="${escape(n)}"><i${css ? ` style="background:${css}"` : ' class="nocolor"'}></i>${escape(n)}</li>`).join('')}</ul></details>`;
  /** One line for the entries whose name says little. */
  const CACHE_NOTES = {
    previews: 'Sprites of TikZ arrow tips, patterns and colors that TeX drew, shown in completion previews.',
    hashes: 'Content hashes SaTeX uses to tell whether a cached package or file changed.',
    facts: 'What the installation is like (TeX Live version, engines), probed once.',
  };
  const DEFINED = ['public command', 'internal macro', 'environment', 'option', 'register', 'counter', 'length', 'switch', 'public alias', 'internal alias'];
  /** The names a row's class or package (or the kernel) defined in the run that match `q`: containing
   * what follows a `\`, or else equal to it (`color` finds xcolor by name, not every name with `color` in it). */
  const definedOf = (cls, what) => cls === 'kernel' ? defined.get(null) : (cls === 'package' || cls === 'class') ? defined.get(what) : null;
  const namesLike = (cls, what, q) => {
    const part = q.startsWith('\\'), w = q.replace(/^\\/, '');
    return w ? [...definedOf(cls, what)?.names ?? []].filter((n) => part ? n.toLowerCase().includes(w) : n.replace(/^\\/, '').toLowerCase() === w) : [];
  };
  /** A few public commands a class or package defined in the run, as chips; the rest behind "+N". */
  // its own: letters only, and not the kernel's too (a load sets \CurrentFile, which is no command of the package)
  const exported = (cls, what) => {
    const kernel = defined.get(null)?.names ?? new Set();
    const names = (cls === 'package' || cls === 'class') ? [...new Set(definedOf(cls, what)?.commands ?? [])].filter((n) => /^\\[A-Za-z]{2,}$/.test(n) && !kernel.has(n)).sort() : [];
    return names.length ? ` ${cards.chipList(names, cards.csChip, 4)}` : '';
  };
  const plural = (n, w) => `${num(n)} ${w}${n === 1 ? '' : /(s|ch)$/.test(w) ? 'es' : 's'}`;
  /** One row's details: what it defines and loads, and where its documentation is. */
  function details(cls, what, needs) {
    const parts = [];
    if (cls === 'index' && indexed) {
      parts.push(`${indexed.indexed} files indexed${indexed.message ? ` · ${escape(indexed.message)}` : ''}`);
      parts.push('<span class="muted">The index lets <code>\\input</code> and <code>\\usepackage</code> find files the way kpsewhich does. The number beside a directory is how many files of it the index knows.</span>');
      if (folders.length) parts.push(`<span class="muted">Files the index knows, by directory:</span><ul class="cache-dirs">${folders.map((r) => `<li title="${escape(r.root)}/${escape(r.name)}"><span>${escape(r.name || '.')}</span><b>${r.count}</b></li>`).join('')}</ul>`);
    }
    const d = definedOf(cls, what);
    if (d) {
      parts.push('defines ' + DEFINED.filter((k) => d[k]).map((k) => plural(d[k], k)).join(', '));
      if (d.colorList) parts.push(colorSwatches(d.colorList));
    }
    else if (cls === 'package' || cls === 'class') parts.push('<span class="muted">not used by this document</span>');
    if (needs.length) parts.push(`loads ${needs.map((n) => cacheBadge(n.endsWith('.cls') ? 'class' : 'package', n.replace(/\.cls$/, ''))).join(' ')}`);
    const docs = cls === 'kernel' ? 'https://texdoc.org/serve/source2e/0' : (cls === 'package' || cls === 'class') ? `https://texdoc.org/pkg/${encodeURIComponent(what)}` : null;
    if (cls === 'package' || cls === 'class') parts.push(manualPackages().has(what) ? '<span class="manual" title="the package\'s manual is mined: its commands are described on hover and in the library">manual information available</span>' : '<span class="muted">no manual information mined</span>');
    if (docs) parts.push(`<a href="${docs}" target="_blank" rel="noopener">documentation ↗</a>`);
    return parts.map((p) => `<div>${p}</div>`).join('');
  }
  // a worker restarted meanwhile fails the question: the next run asks again
  const loadCaches = async () => { try { await usage(); drawCaches((await ask({ type: 'caches' })).entries); } catch {} };
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
    const texts = projectTexts();
    prefetched = prefetched.then(async () => {
      if (k !== state.kernel) await loadKernel(k);
      const ids = wanted_(named(texts), k).filter((id) => !fetchedPackages.has(id));
      // the check may have run before the files were there: it runs again, the server sees them at once
      if (ids.length && await fetchPackages(ids)) await recheck();
    }).catch(() => {});
  }

  /** The lenses the text shows before a run answers: a seed of the kernel's and common packages' names, each
   * mark at once on opening a file; the run's lenses (`lensify`) replace them, so a name the run reads
   * otherwise (a redefined `table`, a float of a package) is right once it lands. */
  const SEED_LENSES = [[/\\begin\{(?:figure|table)\*?\}/, 'Edit placement…'], [/\\begin\{(?:tabular[*x]?|array|longtable)\}/, 'Edit table…'],
    [/\\includegraphics\b/, 'Edit image…'], [/\\begin\{tikzpicture\}/, 'Edit drawing…', 'tikz']];
  function textLenses(text) {
    if (!/\.tex$/.test(state.file ?? '')) return [];
    const at = uri(), out = [];
    String(text).split('\n').forEach((raw, line) => {
      const code = raw.replace(/(^|[^\\])%.*/, '$1');
      for (const [re, title, view] of SEED_LENSES) {
        const m = re.exec(code);
        if (!m) continue;
        const position = { line, character: m.index };
        out.push({ range: { start: position, end: position }, command: { title, command: view ? 'satex.openView' : 'satex.openDialog', arguments: view ? [at, position, view] : [{ uri: at, position }] } });
      }
    });
    return out;
  }
  /** The code lenses of the open file, one per plugin target: "Edit table…", "Edit image…", "Edit drawing…". */
  async function lensify() {
    const asked = uri(), found = await lsp('textDocument/codeLens', { textDocument: { uri: asked } }).catch(() => null);
    // no answer (a newer run cancelled this one) keeps the marks: the next run's lenses replace them
    if (Array.isArray(found) && asked === uri()) editor?.lenses?.(found);
  }
  /** The compile server's PNG of a PDF's first page, else null: a static host has no renderer. */
  async function pdfPage(data) {
    const r = await fetch(`${COMPILE}/render`, { method: 'POST', body: data }).catch(() => null);
    return r?.ok && /image/.test(r.headers.get('content-type') ?? '') ? URL.createObjectURL(await r.blob()) : null;
  }
  /** The table or image dialog at `position` of the open file (`satex/dialog`, doc/site/dialogs.js). */
  async function openDialog(position, quiet = false) {
    const target = uri();
    const v = await lsp('satex/dialog', { textDocument: { uri: target }, position }).catch(() => null);
    if (!v) {
      if (quiet) return;
      // the server has no finished run of this text yet (a start, a restart, an edit): the widget opens once it has
      if (running > 0 || act.phase || !(target in diagnostics)) { toast('Opening once the analysis is done…'); afterRun = () => openDialog(position, true); if (!running && !act.phase) analyze(); return; }
      return toast('No table or figure here: the analysis found none at this place');
    }
    const was = textIn(editor.text(), v.range);
    self.satexDialogs.open(v, {
      apply: (state, op) => lsp('satex/dialogApply', { textDocument: { uri: target }, kind: v.kind, range: v.range, state, op }),
      // the text the dialog edits is rechecked: a detached dialog leaves the editor free to change it
      commit: (edits) => target !== uri() ? toast('The file changed meanwhile: not written')
        : textIn(editor.text(), v.range) !== was ? toast('The text changed meanwhile: not written. Open the widget again')
        : editor.apply(edits),
      closed: () => editor.refocus?.(),
      image: async (path) => {
        const b = await binaryOf(path);
        if (b?.kind !== 'pdf') return b;
        const data = await ask({ type: 'read', path }).then((g) => g?.data).catch(() => null);
        const url = data && await pdfPage(data);
        return url ? { url, kind: 'img' } : { url: null, kind: 'pdf' };
      },
      download: binaryOf,
      // an upload lands next to the figure, in place of a file of its name; next to the document when the figure is the installation's
      upload: async (old, name, bytes) => {
        const beside = !!old?.startsWith(`${state.dir}/`), file = beside ? old : path(state.file);
        const at = `${file.slice(0, file.lastIndexOf('/'))}/${name}`;
        if (!at.startsWith(`${state.dir}/`)) throw new Error('the document is outside the project');
        const rel = at.slice(state.dir.length + 1);
        state.bins[rel] = bytes;
        binaries.delete(at);
        post({ type: 'project', dir: state.dir, files: { [rel]: bytes } });
        return { beside };
      },
      reopen: () => { afterRun = () => openDialog(v.range.start); analyze(); },
      feedback: () => feedback(v.kind, v.range.start.line),
    });
  }
  /** Called once with the next run's results of the open file. */
  let afterRun = null;
  /** Opens a widget at a place (0-based) of the open file, a manager by its kind ('manager', 'glossary'): media.mjs and the tests drive the page through it. */
  self.satexWidget = (plugin, line = 0, character = 0) => ['table', 'image', 'float'].includes(plugin) ? openDialog({ line, character })
    : plugin === 'manager' ? manage(line || 'glossary') : openViews({ line, character }, plugin);
  /** The widget a link asks for (`#figure.tex&widget=image&at=\includegraphics&k=2`, the landing's
   * showcase): `plugin` at `k` characters into the first `at` of the open file, a manager by its
   * kind (`at`), a panel by its id, the trace of the call at `at`, the Library searched for `at`, the Settings. True once it is open. */
  self.satexOpen = async ({ widget, at = '', k = 0 }) => {
    if (widget === 'panel') { document.querySelector(`[data-panel=${CSS.escape(at)}]`)?.click(); return true; }
    // the trace of the call `at` begins (its first command), stepped from its first step
    if (widget === 'trace') {
      const text = editor.text(), i = text.indexOf(at), before = text.slice(0, i).split('\n');
      if (i < 0) return false;
      await traceCall(at.match(/^\\[A-Za-z@]+/)?.[0] ?? at, { file: state.file, line: before.length, col: before.at(-1).length + 1 });
      return $('play-trace').open && !!traceBody.querySelector('[data-move]');
    }
    // the Library searched for `at`, the Settings
    if (widget === 'library' || widget === 'settings') {
      $(widget === 'library' ? 'lib-open' : 'cfg-open').click();
      const box = $(widget === 'library' ? 'lib-search' : 'cfg-search');
      if (at && box) { box.value = at; box.dispatchEvent(new Event('input', { bubbles: true })); }
      return $(widget === 'library' ? 'play-library' : 'play-config').open;
    }
    if (widget === 'manager') { await manage(at || 'glossary'); return $('play-view').open; }
    const text = editor.text(), i = text.indexOf(at);
    if (!at || i < 0) return false;
    const before = text.slice(0, i + Number(k)).split('\n'), p = { line: before.length - 1, character: before.at(-1).length };
    editor.focus({ start: p, end: p });
    await (['table', 'image', 'float'].includes(widget) ? openDialog(p, true) : openViews(p, widget));
    return !!document.querySelector('dialog.sd-dialog[open]') || $('play-view').open;
  };
  /** Opens a link's widget after the open file's next run, again after the next while it finds none. */
  const widgetAsked = (want, tries = 4) => {
    afterRun = async () => { if (!await self.satexOpen(want).catch(() => false) && --tries) widgetAsked(want, tries); };
  };
  // a hover's "Edit table…" link opens the dialog
  document.addEventListener('click', (e) => {
    const a = e.target.closest?.('a.sd-open');
    if (!a) return;
    e.preventDefault();
    openDialog(JSON.parse(a.dataset.position));
  });
  /** The colors of the open file as swatches; none when the server offers none. */
  async function colorize() {
    const asked = uri(), found = await lsp('textDocument/documentColor', { textDocument: { uri: asked } }).catch(() => null);
    const css = ({ red, green, blue, alpha }) => `rgba(${[red, green, blue].map((x) => Math.round(x * 255)).join(',')},${alpha})`;
    // an answer for a file no longer shown, or none (a run cancelled it), leaves the swatches as they are
    if (Array.isArray(found) && asked === uri()) editor?.colors?.(found.map((c) => [c.range.start, css(c.color)]));
  }
  /** The bundles holding files the run looked for and did not find (`\IfFileExists`, listings' language files). */
  async function absentBundles() {
    const request = { file: path(state.root ?? state.file), queries: [{ type: 'files' }] };
    const records = (await lsp('workspace/executeCommand', { command: 'satex/query', arguments: [request] }).catch(() => null))?.[0]?.records ?? [];
    const kernel = manifest.kernels[state.kernel].id;
    const held = records.filter((r) => r.kind === 'absent').map((r) => r.file).filter((f) => HOLDER.has(`${kernel}/${f}`));
    return wanted_(held).filter((id) => !inFlight.has(id));
  }
  /** Fetches `ids` (absent files' bundles), then the check starts over. A probe of a file no bundle has stays as it is. */
  async function fetchAbsent(ids) {
    if (!ids.length || !await fetchPackages(ids)) return false;
    await recheck();
    return true;
  }
  /** A load landed: the worker wrote its files into the running server too, so the check runs again on them,
   * no restart (that would drop the server's checkpoints and the run's .aux). The probes it made of them
   * for absent are part of its checkpoints, so none that saw them missing is reused. */
  async function recheck() {
    if (following) return; // a start under way analyzes once it is up
    analyze(true);
    if (!plainLint()) await start();
  }
  /** Waits for package loads already under way, counting them down as each lands. */
  async function awaitPackages(ids) {
    let landed = 0;
    const show = () => busy('Fetching', `Fetching ${plural(ids.length, 'package')}: ${landed} of ${ids.length} in`, { done: landed / ids.length });
    show();
    await Promise.allSettled(ids.map((id) => Promise.resolve(fetchedPackages.get(id)).finally(() => { landed++; show(); })));
    idle();
  }
  /** After findings or a run name missing packages: fetch them, start over. */
  async function fetchMissing(texts) {
    const ids = missing(texts), flying = missing(texts, true).filter((id) => background.has(id));
    if (!ids.length && !flying.length) return false;
    if (ids.length && !await fetchPackages(ids)) return false;
    // one the background load fetches now: waited for, not fetched twice
    if (flying.length) await awaitPackages(flying);
    await recheck();
    return true;
  }
  // Closed, the selector shows the engine's name; its sizes only while the list is open.
  kernelBox.innerHTML = manifest.kernels.map((k, i) => `<option value="${i}" title="${escape(k.note ? `${k.note} Download ${size(k.size)}.` : `download ${size(k.size)}`)}" data-name="${k.label}" data-more="${k.badge ? `${k.badge} · ` : ''}${size(k.size)}"></option>`).join('') +
    (manifest.unavailable ?? []).map(([label, why, badge]) => `<option disabled title="${escape(why)}" data-name="${label}" data-more="${badge ?? 'not available'}"></option>`).join('');
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
  async function project(files, label, root = null, dir = '/work/upload') {
    sync();
    const decoder = new TextDecoder();
    const texts = Object.fromEntries(Object.entries(files).filter(([n]) => TEXT.test(n)).sort(([a], [b]) => a.localeCompare(b)).map(([n, d]) => [n, decoder.decode(d)]));
    const found = roots(texts);
    if (!found.length) throw new Error('no root document: no file has \\documentclass and \\begin{document}');
    const first = root in texts ? root : found[0];
    manualKernel = false;
    Object.assign(state, { dir, texts, root: first, file: first, open: [first], files: Object.keys(texts), example: null, upload: true, ro: {}, bins: Object.fromEntries(Object.entries(files).filter(([n]) => !TEXT.test(n))) });
    post({ type: 'project', dir: state.dir, files, replace: true });
    redraw();
    editor.set(texts[state.file]);
    history.replaceState(null, '', location.pathname);
    toast(`${label}: ${Object.keys(files).length} files, root ${state.root}${found.length > 1 ? ` (${found.length} candidates: ◎ in the tree makes another the root)` : ''}`);
    await restart();
    args.value = 'lint';
    outBox.hidden = true;
  }
  /** `satex gui`: the project is the folder it serves, its texts read from there (src/cli/gui/mod.rs). */
  async function openLocal({ dir, name, root, files }) {
    $('play-exsect').hidden = true;
    const texts = {};
    for (const [n, bytes] of files) if (TEXT.test(n) && bytes <= LIMIT) texts[n] = (await ask({ type: 'read', path: `${dir}/${n}` })).data ?? new Uint8Array();
    await project(texts, name, root, dir);
  }
  /** Single files join the open project; images and the like only its file system. */
  async function add(files) {
    const decoder = new TextDecoder();
    const texts = Object.entries(files).filter(([n]) => TEXT.test(n)).map(([n, d]) => [n, decoder.decode(d)]);
    Object.assign(state.texts, Object.fromEntries(texts));
    Object.assign(state.bins, Object.fromEntries(Object.entries(files).filter(([n]) => !TEXT.test(n))));
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
    const whole = entries.length > 1 || /\.zip$/i.test(entries[0][0]);
    // `satex gui` works on its own folder: another one is another `satex gui DIR`
    if (whole && manifest.local) return toast(`This page works on ${manifest.local.dir}. Run satex gui in another folder to open that one.`);
    if (whole) bootOpen(`Opening ${entries.length === 1 ? entries[0][0] : entries[0][0].split('/')[0]}`);
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
      bootClose();
    } catch (e) { bootClose(); toast(`Upload: ${e.message}`); }
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
      if (state.upload) Object.assign(state, { dir: '/work', texts: { ...sampleTexts, ...drafts().texts }, upload: false, lists: drafts().lists ?? {}, bins: {} });
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
  const issue = (title, body) => window.open(`${manifest.repository ?? 'https://github.com/EagleoutIce/satex-analyzer'}/issues/new?title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`, '_blank', 'noopener');
  const versionLine = () => `SaTeX ${document.querySelector('a.version')?.textContent.trim() ?? ''}, kernel ${manifest.kernels[state.kernel]?.label ?? ''}`;
  $('play-issue').onclick = () => {
    const text = (state.texts[state.file] ?? '').slice(0, 1500);
    issue('[playground] ', `${versionLine()}\n\nWhat happened:\n\n\n${state.file}:\n\`\`\`latex\n${text}\n\`\`\`\n`);
  };
  /** Feedback on a plugin's widget (`plugin`, at `line`, 0-based), or a plugin request (`plugin` null): a short form, then
   * GitHub's issue form in a new tab. The document's text goes along only when the user ticks it, and nothing is sent until they submit there. */
  function feedback(plugin, line = null) {
    const dlg = $('play-feedback') ?? document.body.appendChild(Object.assign(document.createElement('dialog'), { id: 'play-feedback', className: 'cache-dialog feedback-dialog' }));
    const where = line == null ? state.file : `${state.file}:${line + 1}`;
    dlg.innerHTML = `<header><h3>${plugin ? `Feedback on ${escape(plugin)}` : 'Request a plugin'}</h3><button type="button" class="ghost small" data-close aria-label="close">✕</button></header>
      <form method="dialog" class="fb-form"><p class="muted">${plugin ? `Tell us what to change in this widget. It names the plugin and ${escape(where)}.` : 'Tell us which widget or hover you miss, and for which package or command.'}</p>
      <textarea name="what" rows="4" placeholder="${plugin ? 'What did you expect?' : 'What should the plugin do?'}" aria-label="your feedback"></textarea>
      ${plugin && line != null ? '<label class="fb-snip"><input type="checkbox" name="snip"> include my snippet (the lines around the cursor)</label>' : ''}
      <p class="muted">Opens GitHub's issue form in a new tab. Nothing is sent until you submit it there.</p>
      <div class="fb-actions"><button type="button" class="ghost" data-close>Cancel</button><button type="submit">Open issue form</button></div></form>`;
    for (const b of dlg.querySelectorAll('[data-close]')) b.onclick = () => dlg.close();
    dlg.querySelector('form').onsubmit = (e) => {
      e.preventDefault();
      const f = e.target, lines = (state.texts[state.file] ?? '').split('\n');
      const snip = f.snip?.checked ? `\n\n${where}:\n\`\`\`latex\n${lines.slice(Math.max(0, line - 3), line + 4).join('\n')}\n\`\`\`` : '';
      issue(plugin ? `[${plugin}] ` : '[plugin request] ', `${versionLine()}${plugin ? `\nPlugin: ${plugin}\nAt: ${where}` : ''}\n\n${f.what.value.trim()}${snip}\n`);
      dlg.close();
    };
    dlg.showModal();
    dlg.querySelector('textarea').focus();
  }
  const cursorLine = () => { const text = editor?.text() ?? '', [c] = editor?.cursor() ?? [0]; return text.slice(0, c).split('\n').length - 1; };
  $('view-feedback').onclick = () => feedback(viewPlugin, cursorLine());
  $('lib-feedback').onclick = () => feedback('library');
  $('cfg-feedback').onclick = () => feedback('settings');
  document.addEventListener('click', (e) => { if (e.target.closest?.('[data-request-plugin]')) feedback(null); });

  // ---------------------------------------------------------------- start

  /** What the page is doing, in one place: the status bar's text and bar, and a card with the details on hover. */
  const bar = $('play-progress'), loads = new Map(), actBox = $('act-detail');
  const PHASE = { unpacking: ['Unpacking', 'Unpacking the TeX files…'], importing: ['Importing', 'Importing the kernel caches…'] };
  /* The start, and an uploaded project, as one card over the page: the logo, the version, the stages in one bar.
   * Each stage holds its share of the bar; a stage's own progress (bytes, lines read) fills its share. */
  const BOOT = /** @type {[string, string, number][]} */ ([['Fetching', 'downloads', 0.55], ['Unpacking', 'TeX files', 0.15], ['Importing', 'caches', 0.15], ['Starting', 'SaTeX starts', 0.15]]);
  var boot = { el: /** @type {HTMLElement | null} */ (null), at: 0, stage: -1, open: false, started: false }; // var: busy may run before this line
  function bootOpen(title) {
    if (!boot.el) {
      boot.el = Object.assign(document.createElement('div'), { className: 'boot', role: 'dialog' });
      boot.el.setAttribute('aria-label', 'loading the playground');
      const logo = document.querySelector('header .brand img')?.cloneNode(), version = document.querySelector('header a.version')?.textContent.trim() ?? '';
      boot.el.innerHTML = `<div class="boot-card"><div class="boot-logo"></div><p class="boot-title"></p><p class="muted boot-version">SaTeX ${escape(version)}</p>`
        + `<ol class="boot-stages">${BOOT.map(([n, label]) => `<li data-stage="${n}">${label}</li>`).join('')}</ol>`
        + `<span class="boot-bar" role="progressbar" aria-label="loading" aria-valuemin="0" aria-valuemax="100">${BOOT.map(([, , w]) => `<i style="flex-grow:${w * 100}"><b></b></i>`).join('')}</span><p class="muted boot-text" aria-live="polite"></p>`
        + '<button type="button" class="small ghost boot-hide" title="keep loading in the background">Hide</button></div>';
      if (logo) boot.el.querySelector('.boot-logo').append(logo);
      boot.el.querySelector('.boot-hide').onclick = bootClose;
      document.body.append(boot.el);
    }
    boot.el.querySelector('.boot-title').textContent = title;
    boot.el.querySelectorAll('[data-stage]').forEach((li) => { li.className = ''; });
    boot.at = 0;
    boot.stage = -1;
    boot.open = true;
    boot.el.classList.remove('gone');
    boot.el.hidden = false;
    bootStep('', null, '');
  }
  /** The bar's segments: each stage's share filled to its own progress, a stage with none known striped. */
  function bootBar(at, unknown) {
    let from = 0;
    const bar = boot.el.querySelector('.boot-bar');
    bar.setAttribute('aria-valuenow', String(Math.round(at * 100)));
    bar.querySelectorAll('i').forEach((seg, i) => {
      const w = BOOT[i][2], f = Math.min(1, Math.max(0, (at - from) / w));
      from += w;
      seg.style.setProperty('--done', String(f));
      seg.className = f >= 1 ? 'done' : i === boot.stage ? 'on' + (unknown || f === 0 ? ' idle' : '') : '';
    });
  }
  function bootStep(name, done, text) {
    if (!boot?.open) return;
    // a download once the engine started is the example's own files, asked for on demand: the editor is
    // usable, so the card goes and the status line shows that download
    if (name === 'Fetching' && boot.stage >= 3) { if (boot.started) bootClose(); return; }
    const k = BOOT.findIndex(([n]) => n === name);
    // The card covers what blocks the editor: every download, and satex starting. A download after
    // satex started keeps the card open on its text, stages never run backwards, and the first
    // analysis goes to the status line.
    if (name && k < 0 && boot.stage >= 0) return bootSettled();
    if (k >= 0 && k >= boot.stage) {
      const before = BOOT.slice(0, k).reduce((n, b) => n + b[2], 0);
      boot.stage = k;
      boot.at = Math.max(boot.at, before + BOOT[k][2] * (Number.isFinite(done) ? done : 0));
      boot.el.querySelectorAll('[data-stage]').forEach((li, i) => { li.className = i < k ? 'done' : i === k ? 'on' : ''; });
    }
    bootBar(boot.at, k >= 0 && !Number.isFinite(done));
    boot.el.querySelector('.boot-text').textContent = text;
  }
  /** The card goes once satex has started and no download is left. It never comes back: later downloads show in the status line. */
  function bootSettled() {
    if (boot?.started && !loads.size && !inFlight.size) bootClose();
  }
  function bootClose() {
    if (!boot?.open) return;
    boot.open = false;
    bootBar(1, false);
    boot.el.classList.add('gone');
    setTimeout(() => { if (!boot.open) boot.el.hidden = true; }, 350);
  }
  bootOpen('Loading the playground');
  /** The engine's phases (`Status::phase`, src/util/progress.rs) as the card names them. */
  const STAGE = { format: 'kernel cache', package: 'package caches', preamble: 'preamble', document: 'document', lint: 'lint', probe: 'trying commands out' };
  /** Work that starts this soon after other work ended, with no edit between, goes on in its timeline (a package fetched, the server restarted for it). */
  const FOLLOW_MS = 1500;
  function busy(name, text, { done = null, got = 0, all = 0, stage = null } = {}) {
    const now = performance.now();
    bootStep(name, done, text);
    if (!act.phase) {
      // one analysis is one timeline: only an edit, or work long after, starts the next run
      const edited = version !== act.version;
      if (edited || now - act.ended > FOLLOW_MS) Object.assign(act, { started: now, steps: [], run: act.run + 1, after: act.run && edited ? 'after your edit' : '', version });
      act.rest = status.dataset.done && status.textContent !== restLine() ? status.textContent : READY;
    }
    // one step per phase the page or the engine names, in the order they came
    const key = stage?.phase ? `${stage.run ?? 1} ${stage.phase}` : name;
    let step = act.steps.at(-1);
    if (step?.key !== key) {
      step = { key, label: stage?.phase ? STAGE[stage.phase] ?? stage.phase : name.toLowerCase(), at: now, items: new Set(), run: stage?.phase ? `build run ${stage.run ?? 1}${stage.runs > 1 ? ` of at most ${stage.runs}` : ''}` : '' };
      act.steps.push(step);
    }
    if (stage?.phase === 'package') step.items.add(text);
    act.since = step.at;
    // where the engine reads now: a moving line is progress too
    const where = stage?.file ? `${stage.file}:${stage.line ?? ''}` : '';
    if (text !== act.text || got !== act.got || where !== act.where) act.changed = now;
    Object.assign(act, { phase: name, text, got, all, where });
    // a status without both counts shows work going on, not a share of it; with them, the time left
    self.satexProgress.set(bar, Number.isFinite(done) ? done : null);
    const left = Number.isFinite(done) ? self.satexProgress.left(done, now - act.since) : null;
    // an estimate that keeps saying a second is left is overrun (the work goes on past the text's end): the time spent instead
    act.lastSecond = left === 1 ? act.lastSecond ?? now : null;
    const overrun = act.lastSecond !== null && now - act.lastSecond > 3000;
    status.textContent = act.shown = overrun ? `${text} · ${secs(now - act.started)}` : left ? `${text} · about ${left} s left` : text;
    status.title = '';
    status.dataset.busy = '';
    $('play-stop')?.toggleAttribute('hidden', !(running > 0 || name === 'Starting'));
    drawAct();
  }
  // Stop: the run under way ends now; a start under way is dropped and the last findings stay
  const stopButton = $('play-stop') ?? document.createElement('button');
  stopButton.onclick = () => {
    stopButton.hidden = true;
    if (act.phase === 'Starting' && !running) { starts++; following = false; idle('Stopped. Edit or press Run to start again.'); return; }
    if (stopRun('stopped')) idle('Stopped. The findings are from the last finished run.');
  };
  /** Work is over: the bar goes, the last result stays or comes back. */
  function idle(text = null) {
    // satex has started, or did not: the card gives way to the page
    if (act.phase === 'Stopped') bootClose();
    if (act.phase === 'Starting') boot.started = true;
    bootSettled();
    bar.hidden = true;
    if (act.phase && text === null && status.textContent === act.shown && act.rest) status.textContent = act.rest === READY ? restLine() : act.rest;
    if (text !== null) status.textContent = text;
    if (act.phase) act.ended = performance.now();
    act.phase = ''; act.text = '';
    delete status.dataset.busy;
    $('play-stop')?.toggleAttribute('hidden', true);
    drawAct();
  }
  const mib = (n) => (n / 2 ** 20).toFixed(1);
  /** The card shows while work goes on and the status line is hovered or focused; idle, it covers nothing. */
  let actWanted = false;
  function drawAct() {
    actBox.hidden = !(actWanted && act.phase);
    if (actBox.hidden) return;
    const now = performance.now(), s = (t) => secs(now - t);
    const item = (st, i) => {
      const last = i === act.steps.length - 1, end = last ? now : act.steps[i + 1].at;
      const n = st.items.size > 1 ? ` · ${st.items.size}` : '';
      return `<li class="act-step${last ? ' act-now' : ''}"${last ? ' aria-current="step"' : ''}><span>${escape(st.label + n)}</span><span class="act-t">${secs(end - st.at)}</span></li>`;
    };
    // each build run's steps fold under the run, with its time; the run going on is open
    const steps = [];
    act.steps.forEach((st, i) => {
      const g = st.run && steps.at(-1)?.run === st.run ? steps.at(-1) : null;
      if (g) { g.rows.push(item(st, i)); g.end = i; } else steps.push({ run: st.run, at: st.at, end: i, rows: [item(st, i)] });
    });
    const steps_ = steps.map((g, k) => {
      if (!g.run) return g.rows.join('');
      const last = g.end === act.steps.length - 1, until = last ? now : act.steps[g.end + 1].at;
      return `<li class="act-run"><details data-run="${escape(g.run)}"${actOpen.get(g.run) ?? last ? ' open' : ''}><summary><span>${escape(g.run)}</span><span class="act-t">${secs(until - g.at)}</span></summary><ol class="act-steps">${g.rows.join('')}</ol></details></li>`;
    }).join('');
    // each kind of step once, with how often and how long; steps under 50 ms only in the full list
    const kinds = new Map();
    act.steps.forEach((st, i) => {
      const last = i === act.steps.length - 1, g = kinds.get(st.label) ?? kinds.set(st.label, { n: 0, ms: 0, now: false }).get(st.label);
      g.n++; g.ms += (last ? now : act.steps[i + 1].at) - st.at; g.now ||= last;
    });
    const summary = [...kinds].filter(([, g]) => g.now || g.ms >= 50).map(([label, g]) =>
      `<li class="act-step${g.now ? ' act-now' : ''}"><span>${escape(label)}${g.n > 1 ? ` · ${g.n}×` : ''}</span><span class="act-t">${secs(g.ms)}</span></li>`).join('');
    const build = act.steps.at(-1)?.run;
    const rows = [['run', `${act.run}${act.after ? ` ${act.after}` : ''}`], ['doing', escape(act.text.replace(/…$/, ''))], ...(build ? [['build', escape(build)]] : []), ...(act.all ? [['bytes', `${mib(act.got)} of ${mib(act.all)} MiB`]] : []),
      ...(act.where ? [['at', escape(act.where)]] : []), ['steps', `<ol class="act-steps">${summary}</ol>${act.steps.length > kinds.size || summary.length < kinds.size ? `<details class="act-all" data-run="all"${actOpen.get('all') ? ' open' : ''}><summary class="muted">all ${act.steps.length} steps</summary><ol class="act-steps">${steps_}</ol></details>` : ''}`],
      ['in this step', s(act.since)], ['since start', s(act.started)], ['last change', `${s(act.changed)} ago`]];
    actBox.innerHTML = rows.map(([k, v]) => `<dt>${k}</dt><dd>${v}</dd>`).join('');
  }
  var actOpen = new Map(); // var: drawAct may run before this line; build run -> opened or closed by the viewer
  actBox.addEventListener('click', (e) => {
    const sum = e.target instanceof Element && e.target.closest('summary');
    if (!sum) return;
    e.preventDefault();
    actOpen.set(sum.parentElement.dataset.run, !sum.parentElement.open);
    drawAct();
  });
  let actTick = 0;
  const showAct = (on) => { actWanted = on; clearInterval(actTick); drawAct(); if (on) actTick = setInterval(drawAct, 1000); };
  const actHost = actBox.parentElement;
  // a click leaves focus on the line: leaving it with the mouse still closes the card
  actHost.addEventListener('mouseenter', () => showAct(true)); actHost.addEventListener('mouseleave', () => showAct(false));
  actHost.addEventListener('focusin', (e) => { if (e.target.matches(':focus-visible')) showAct(true); }); actHost.addEventListener('focusout', () => showAct(false));
  function progressed({ label, loaded, total }) {
    if (quietLoads && !inFlight.size) return; // only a background load: the status line is the analysis'
    if (PHASE[label]) { busy(PHASE[label][0], PHASE[label][1]); return; }
    loads.set(label, [loaded, total]);
    const [got, all] = [...loads.values()].reduce(([g, a], [l, t]) => [g + l, a + t], [0, 0]);
    const names = [...loads.keys()], landed = [...loads.values()].filter(([l, t]) => t && l >= t).length;
    busy('Fetching', `Fetching ${names.length > 2 ? `${landed} of ${names.length} downloads` : names.join(' and ')}: ${mib(got)} of ${mib(all)} MiB`, { done: all ? Math.min(got / all, 1) : null, got, all });
    status.title = names.join(', ');
  }

  /** A published set of findings. One that arrives while a newer run is in flight is held, and shown if that run ends without publishing its own. */
  let heldFindings = null;
  /** Per file, the latest set of findings published: an older one still asking about absent files is dropped. */
  const published = new Map();
  /** The packages still downloading when the findings shown were made: they may change once those are in. */
  var provisional = ''; // var: a render may run before this line
  async function publish(p) {
    // of a text the server no longer has: its analysis of the newer one follows, else is asked for
    if (outdated(p)) { if (p.uri === uri() && timer === null) analyze(); return; }
    if (running > 0 && p.uri === uri()) { heldFindings = p; return; }
    heldFindings = null;
    // The rule's number is the LSP code: the page keys rules by name.
    for (const d of p.diagnostics) [d.number, d.code] = [d.code, d.data?.rule ?? d.code];
    // Findings that come from a missing package wait for it: shown once its bundle is in.
    const unresolved = p.diagnostics.filter((d) => d.code === 'unresolved-file').map((d) => d.message);
    const waiting = missing(unresolved);
    if ((waiting.length || inFlight.size) && p.uri === uri()) {
      const names = [...new Set([...waiting, ...inFlight])].map((id) => PACKAGES.get(id).label).join(', ');
      busy('Loading', `Loading ${plural(names.split(', ').length, 'package')}…`);
      status.title = names;
      if (waiting.length) fetchMissing(unresolved).then((ok) => { if (!ok && !outdated(p)) { show(p.uri, p.diagnostics, p.version); typeset(p.uri, p.version); } });
      // loads another step started (a start since superseded): once in, the check runs again on them
      else Promise.allSettled([...inFlight].map((id) => fetchedPackages.get(id))).then(() => { if (!outdated(p) && p.uri === uri()) recheck(); });
      return;
    }
    // A run that missed files a bundle holds is redone once they are in: its findings
    // (a listings language "not found") are shown as provisional meanwhile, never as final.
    const seq = (published.get(p.uri) ?? 0) + 1;
    published.set(p.uri, seq);
    const absent = p.uri === uri() ? await absentBundles() : [];
    if (seq !== published.get(p.uri) || outdated(p)) return;
    if (p.uri === uri()) provisional = [...new Set([...absent, ...inFlight])].map((id) => PACKAGES.get(id)?.label ?? id).join(', ');
    // what was answered from the last run meanwhile is asked again of this one
    explanations.clear(); scopeAsked.clear(); explained = ''; explainSoon();
    show(p.uri, p.diagnostics, p.version);
    typeset(p.uri, p.version); // tagged with this run's version: `show` sets the shown one only after its code-action round trip
    outline();
    if (p.uri === uri()) {
      lensify();
      colorize();
      const then = afterRun;
      afterRun = null;
      then?.();
      // a failed download leaves the findings as they are: final
      if (absent.length) fetchAbsent(absent).then((ok) => { if (!ok && seq === published.get(p.uri)) { provisional = ''; render(); } });
    }
    if (p.uri === uri()) prefetchCommands();
    loadCaches(); // the status bar's count, and the table if open
    if (p.uri === uri(state.root ?? state.file)) written(p.uri);
    if (p.uri === uri()) lsp('satex/caches', { textDocument: { uri: p.uri } }).then((r) => cacheReport((r ?? []).map((l) => ['err', l]))).catch(() => {});
    fetchMissing(unresolved);
  }
  /** The server aborted (a panic, out of memory): it starts again on the same texts, with a note. One that
   * keeps aborting stops, saying why, with a way to try again. */
  let aborts = [];
  function serverStopped(why) {
    const now = performance.now();
    aborts = [...aborts.filter((t) => now - t < 60000), now];
    if (aborts.length > 2) {
      return stoppedLine(`The language server stopped again: ${escape(why)}.`, () => { aborts = []; restart(); });
    }
    toast(`The language server stopped (${why}). It starts again on your text.`);
    restart();
  }
  async function onWorkerMessage({ data }) {
    if (running > 0 || pending.size) maxGap = Math.max(maxGap, Math.round(performance.now() - lastHeard));
    lastHeard = performance.now();
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
          publish(m.params);
        }
      }
      drainAnalysis();
    } else if (data.type === 'status') {
      traceLive?.(data);
      if (data.text && act.phase === 'Analyzing') phase(data.text, data.total ? Math.min(data.reached / data.total, 1) : null, data);
      // a run cannot be stopped: the last project's goes on before the new server starts, and the line says so
      else if (data.text && act.phase === 'Starting' && running > 0) busy('Starting', `Starting once the last run ends (${data.text.replace(/^Analyzing /, '')})`, { done: data.total ? Math.min(data.reached / data.total, 1) : null });
    } else if (data.type === 'progress') {
      progressed(data);
    } else if (data.type === 'error' && data.of === 'lsp') {
      lspStatus.textContent = `language server failed: ${data.message.split('\n')[0]}`;
      running -= order.shift() ? 1 : 0;
      for (const p of pending.values()) p.resolve(null);
      pending.clear();
      drainAnalysis();
      serverStopped(data.message.split('\n')[0]);
    } else if (data.type === 'error') {
      answer(REPLY[data.of])?.reject(new Error(data.message.split('\n')[0]));
    } else {
      answer(data.type)?.resolve(data);
    }
  }

  /** A build file's bytes on file://, where fetch fails: build.py's `.js` copy sets them as base64. */
  async function local(file) {
    try { return new Uint8Array(await (await fetch(base + file)).arrayBuffer()); } catch {}
    await new Promise((resolve, reject) => document.head.append(Object.assign(document.createElement('script'),
      { src: base + file + '.js', onload: resolve, onerror: () => reject(new Error(`${file}.js did not load`)) })));
    const text = self.SATEX_FILES[file];
    delete self.SATEX_FILES[file];
    return Uint8Array.fromBase64?.(text) ?? Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
  }

  /* Offline: sw.js keeps the page, the wasm and every bundle once fetched. When the playground is idle the
   * remaining bundles are fetched in the background (play/offline.json lists them), so it also works without its server. */
  const offlineBox = $('offline'), offlineText = $('offline-text'), offlineBtn = $('offline-btn');
  const offlineState = { todo: [], total: 0, got: 0, stop: false, running: false };
  /** The status line's offline note: only while packages are still being fetched; once everything is kept, nothing is shown. */
  const offlineShow = (text, button = '', title = '') => {
    offlineBox.hidden = !text;
    offlineText.textContent = text;
    offlineBtn.textContent = button;
    offlineBtn.hidden = !button;
    offlineBox.title = title;
  };
  async function offlineSetup() {
    if (manifest.local || !navigator.serviceWorker || !/^https?:$/.test(location.protocol) || !self.caches) return;
    try {
      await navigator.serviceWorker.register(`${base}play/sw.js`);
      await navigator.serviceWorker.ready;
      const list = await (await fetch(`${base}play/offline.json`, { cache: 'no-cache' })).json();
      const have = new Set((await (await caches.open('satex-assets')).keys()).map((r) => r.url));
      const todo = list.more.filter(([p]) => !have.has(new URL(p, base).href));
      Object.assign(offlineState, { todo, total: todo.reduce((n, [, s]) => n + s, 0), got: 0 });
    } catch (e) { console.warn('offline mode unavailable', e); return; }
    if (!offlineState.todo.length) { offlineShow(''); return; }
    let off = false;
    try { off = localStorage.getItem('satex-offline') === 'off'; } catch {}
    if (navigator.connection?.saveData || off) { offlineShow('', `Keep offline (${mib(offlineState.total)} MiB)`, 'fetch every package now, so the playground works without its server'); return; }
    await new Promise((r) => (self.requestIdleCallback ? requestIdleCallback(r, { timeout: 2000 }) : setTimeout(r, 2000)));
    offlineRun();
  }
  async function offlineRun() {
    const s = offlineState, next = [...s.todo];
    if (s.running) return;
    Object.assign(s, { running: true, stop: false, got: s.todo.length === next.length ? s.got : 0 });
    try { localStorage.removeItem('satex-offline'); } catch {}
    const line = () => offlineShow(`making it available offline: ${mib(Math.min(s.got, s.total))} of ${mib(s.total)} MiB`, 'Stop', 'the packages are fetched in the background and kept in this browser');
    line();
    let failed = false;
    const one = async () => {
      while (!s.stop && next.length) {
        // the analysis comes first
        while (!s.stop && (act.phase || running > 0)) await new Promise((r) => setTimeout(r, 1000));
        if (s.stop) break;
        const [path, size] = next.shift();
        try {
          const response = await fetch(new URL(path, base).href);
          if (!response.ok) throw new Error(String(response.status));
          const reader = response.body.getReader();
          for (let n = 0; ;) {
            const { done, value } = await reader.read();
            if (done) break;
            s.got += value.length; n += value.length; line();
            if (s.stop) { reader.cancel(); next.unshift([path, size]); s.got -= n; break; }
          }
        } catch { next.unshift([path, size]); failed = true; s.stop = true; }
      }
    };
    await Promise.all([one(), one()]);
    s.running = false;
    s.todo = next;
    if (!next.length) offlineShow('');
    else s.got = s.total - next.reduce((n, [, z]) => n + z, 0), offlineShow(failed ? 'offline copy paused: no connection' : `offline copy stopped at ${mib(s.got)} of ${mib(s.total)} MiB`, 'Resume', 'fetch the rest of the packages');
  }
  offlineBtn.onclick = () => {
    if (offlineState.running) { offlineState.stop = true; try { localStorage.setItem('satex-offline', 'off'); } catch {} } else offlineRun();
  };
  addEventListener('online', () => { if (!offlineState.running && offlineState.todo.length && offlineBtn.textContent === 'Resume' && offlineText.textContent.includes('no connection')) offlineRun(); });

  /** What doc/site/chaos.mjs reads of the page: what it shows and for which text. Read-only. */
  self.satexHealth = () => ({
    busy: 'busy' in status.dataset, phase: act.phase, status: status.textContent, quiet: Math.round(performance.now() - act.changed), heard: Math.round(performance.now() - lastHeard),
    boot: !!boot.open, example: state.example, file: state.file, root: state.root, dir: state.dir, version, text: editor?.text() ?? null,
    list: shownOf.get(list)?.slice(0, 2) ?? null, listStale: list.classList.contains('stale'), sent: sentVersions.get(uri()) ?? null,
    shown: diagnosticVersions[uri()] ?? null, due: timer !== null || dirty, running, pending: pending.size, waiting: Object.keys(waiting), queued: analysisQueued, recovering,
    pdf: pdf ? { dir: pdf.dir, root: pdf.root } : null, pdfOther: $('play-pdf').classList.contains('pdf-other'), pdfBusy, gap: maxGap, undimmed: [...shownOf].filter(([el, [t, , cls]]) => t !== uri() && !el.classList.contains(cls)).map(([el]) => el.id || el.className), loading: [...inFlight], held: held.length, recoveries, runs: act.run, step: act.steps.at(-1)?.label ?? '', since: Math.round(performance.now() - act.started),
  });
  /** chaos.mjs: the worker crashes (an error event) or hangs (goes silent). */
  self.satexKill = (how) => { worker.terminate(); if (how === 'crash') onWorkerError({ message: 'the worker crashed' }); };
  (async () => {
    let given;
    if (location.protocol === 'file:') {
      const [js, wasm, bundle] = await Promise.all([manifest.js, manifest.wasm, manifest.kernels[0].bundle].map(local));
      given = { js, wasm, bundle };
    }
    const ready = await ask({ type: 'init', base, manifest, given, cancel: textsSent }, given ? Object.values(given).map((b) => b.buffer) : []);
    booted = true;
    try { sessionStorage.removeItem('satex-recovered'); } catch {}
    timing('t-init', secs(performance.now() - started), 'time to fetch SaTeX and the pdfTeX kernel and set them up');
    idle();
    sampleTexts = ready.samples;
    const setting = location.hash.startsWith('#setting=') ? new URLSearchParams(location.hash.slice(1)) : null;
    const [named, asked] = linkWidget(setting ? '' : location.hash.slice(1)), hash = decodeURIComponent(named);
    const link = hash.startsWith('s=') ? await restore(hash.slice(2)) : null;
    if (hash.startsWith('open=')) history.replaceState(null, '', location.pathname);
    const ex = link ? link.x : hash in sampleTexts && !shared.has(hash) ? hash : projectOf(hash) ?? manifest.sample;
    const kept = manifest.local ? {} : drafts(), restored = Object.keys(kept.texts ?? {}).filter((n) => typeof kept.texts[n] === 'string');
    state.texts = { ...sampleTexts, ...Object.fromEntries(restored.map((n) => [n, kept.texts[n]])), ...(link?.x ? link.p : {}) };
    if (kept.lists && typeof kept.lists === 'object') state.lists = Object.fromEntries(Object.entries(kept.lists).filter(([ex, l]) => ex in sampleTexts && Array.isArray(l) && l.every((n) => n in state.texts)));
    if (restored.length) toast(`Your edits to ${restored.slice(0, 3).join(', ')}${restored.length > 3 ? ` and ${restored.length - 3} more` : ''} are restored from this browser.`);
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
    try {
      editor = await codemirror($('play-editor'), '');
      $('play-source').remove();
    } catch (e) {
      console.warn('CodeMirror did not load, the editor is a textarea', e);
      editor = textarea($('play-source'));
    }
    button.textContent = 'Run';
    button.disabled = false;
    if (setting) cfgFromLink(setting);
    if (asked && !link) widgetAsked(asked);
    if (ready.local) await openLocal(ready.local);
    else if (link) await openLink(link);
    else await example(ex, { file: shared.has(hash) ? hash : ex });
    const opening = hash.match(/^open=(.+):(\d+)$/);
    if (opening) await openPlace(opening[1], { line: Number(opening[2]) - 1, character: 0 });
    offlineSetup();
    preload().catch(() => {});
    if (!plainLint()) return link && !link.x ? start() : undefined;
    status.textContent = restLine();
    status.dataset.done = 'ready';
    pdfRestore();
  })().catch((e) => { status.textContent = `the playground failed: ${e.message}`; status.dataset.done = 'error'; bootClose(); });
})();
