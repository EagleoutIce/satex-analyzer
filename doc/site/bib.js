// The bibliography manager (src/lsp/plugins/bibliography.rs `references`): a reference manager in two
// panes. Left the entries as a compact list with search, type and status filters and sorting; right the
// selected entry's fields in its type's order, each checked by its datatype. A second tab holds the
// `@string` constants, a third imports (DOI, arXiv, URL, pasted or dropped RIS/EndNote/CSL-JSON/BibTeX,
// Zotero) and checks entries against dblp. Every edit is a snapshot the server prints (`ctx.print`).
(() => {
  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  const el = (tag, cls, html) => Object.assign(document.createElement(tag), cls ? { className: cls } : {}, html != null ? { innerHTML: html } : {});
  const button = (label, onclick, cls = '') => Object.assign(el('button', `tv-button ${cls}`, esc(label)), { type: 'button', onclick });

  // ---- the model, no page needed (views.test.mjs)
  /** A type's badge colour: a hue from its name, the same in every view. */
  const hue = (type) => [...String(type).toLowerCase()].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7);
  /** The fields of an entry's snapshot, and the names written bare. */
  const keysOf = (node) => node.args[1].keys ??= [];
  const bareOf = (node) => node.args[1].bare ??= [];
  const get = (node, name) => keysOf(node).find((k) => k.key === name)?.value;
  /** Whether a typed value reads as constants (`jan`, `cacm # " 1"`) or a number: then it is written bare. */
  function isBare(value, constants) {
    const v = String(value).trim();
    if (/^\d+$/.test(v)) return true;
    const known = new Set(constants.map((c) => c.toLowerCase()));
    const parts = v.split('#').map((p) => p.trim());
    return parts.every((p) => /^"[^"]*"$/.test(p) || /^\{.*\}$/.test(p) || /^\d+$/.test(p) || known.has(p.toLowerCase())) && parts.some((p) => known.has(p.toLowerCase()));
  }
  /** `name` set to `value` in `node` (bare when it reads so), removed when empty. */
  function setField(node, name, value, constants = []) {
    const keys = keysOf(node), bare = bareOf(node), i = keys.findIndex((k) => k.key === name);
    if (!String(value).trim()) { if (i >= 0) keys.splice(i, 1); }
    else if (i >= 0) keys[i].value = value; else keys.push({ key: name, value });
    const b = String(value).trim() && isBare(value, constants);
    node.args[1].bare = bare.filter((n) => n !== name).concat(b ? [name] : []);
    return node;
  }
  /** The filters a list offers: an entry passes when it passes one chosen per group. */
  const STATUS = {
    cited: (r) => r.cited > 0, uncited: (r) => !r.cited, missing: (r) => r.missing.length > 0, duplicates: (r) => r.duplicates.length > 0,
    doi: (r) => !!get(r.node, 'doi'), url: (r) => !!get(r.node, 'url'), constants: (r) => r.constants.length > 0, undefined: (r) => r.undefined.length > 0,
  };
  const STATUS_LABEL = { cited: 'cited', uncited: 'uncited', missing: 'missing fields', duplicates: 'duplicates', doi: 'DOI', url: 'URL', constants: '@string users', undefined: 'undefined @string' };
  function matches(r, q, on) {
    const types = on.filter((f) => f.startsWith('type:')), status = on.filter((f) => !f.startsWith('type:'));
    if (types.length && !types.includes(`type:${r.type}`)) return false;
    if (status.some((f) => !STATUS[f]?.(r))) return false;
    const hay = [r.key, r.type, r.author, r.year, r.title, ...keysOf(r.node).map((k) => k.value)].join(' ').toLowerCase();
    return String(q).toLowerCase().split(/\s+/).filter(Boolean).every((w) => hay.includes(w));
  }
  const SORTS = { author: (r) => r.author, year: (r) => r.year, title: (r) => r.title, key: (r) => r.key, cited: (r) => String(r.cited).padStart(6, '0') };
  /** What a text names to fetch: a DOI, an arXiv id (fetched by its DataCite DOI), else a URL. */
  function identify(text) {
    const t = String(text).trim();
    const arxiv = t.match(/(?:arxiv\.org\/(?:abs|pdf)\/|arxiv:\s*)?(\d{4}\.\d{4,5})(?:v\d+)?/i);
    if (arxiv && (/arxiv/i.test(t) || /^\d{4}\.\d{4,5}(v\d+)?$/.test(t))) return { doi: `10.48550/arXiv.${arxiv[1]}`, what: `arXiv ${arxiv[1]}` };
    const doi = t.match(/10\.\d{4,9}\/[^\s"'<>]+/);
    if (doi) return { doi: doi[0].replace(/[.,;]$/, ''), what: `DOI ${doi[0]}` };
    if (/^https?:\/\//.test(t)) return { url: t, what: t };
    return null;
  }
  /** The import's chosen fields as snapshots per database: an update sets them in its entry, a new entry keeps them. */
  function chosen(rows, picks, keys = {}, constants = []) {
    const out = {};
    rows.forEach((row, i) => {
      const on = picks[i] ?? {};
      if (!Object.values(on).some(Boolean) || !row.node) return;
      const node = structuredClone(row.node);
      for (const f of row.fields) {
        if (row.match && on[f.name]) setField(node, f.name, f.value, f.bare ? [f.value] : constants);
        if (!row.match && !on[f.name]) setField(node, f.name, '');
      }
      if (!row.match && keys[i]) node.args[0].text = keys[i];
      (out[row.file] ??= []).push(node);
    });
    return out;
  }
  /** WorkspaceEdits as one. */
  const joined = (edits) => ({ changes: edits.reduce((all, e) => { for (const [u, es] of Object.entries(e?.changes ?? {})) (all[u] ??= []).push(...es); return all; }, {}) });

  // ---- the view
  const STYLE = `
.cache-dialog.view-dialog:has(.tv-refs) { width: min(84rem, calc(100vw - 1rem)); } .tv:has(.tv-refs) { max-width: none !important; }
.tv-refs { display: grid; gap: .6rem; min-width: min(60rem, 100%); }
.tv-refs-tabs { display: flex; gap: .3rem; align-items: center; border-bottom: 1px solid var(--tv-border); }
.tv-refs-tabs > button { font: inherit; padding: .35rem .8rem; border: 0; border-bottom: 2px solid transparent; background: none; color: var(--tv-muted); cursor: pointer; }
.tv-refs-tabs > button[aria-selected=true] { color: var(--tv-fg); border-bottom-color: var(--tv-accent); font-weight: 600; } .tv-refs-tabs .tv-muted { margin-left: auto; font-size: .8rem; }
.tv-refs-main { display: grid; grid-template-columns: minmax(22rem, 1.15fr) minmax(22rem, 1fr); gap: .8rem; min-height: 0; }
.tv-refs-left, .tv-refs-right { display: grid; gap: .45rem; align-content: start; min-width: 0; }
.tv-refs-chips { display: flex; flex-wrap: wrap; gap: .25rem; }
.tv-chip { font: inherit; font-size: .74rem; padding: .08rem .45rem; border-radius: 999px; border: 1px solid var(--tv-border); background: none; color: var(--tv-muted); cursor: pointer; }
.tv-chip[aria-pressed=true] { background: color-mix(in srgb, var(--tv-accent) 16%, transparent); border-color: var(--tv-accent); color: var(--tv-fg); }
.tv-type { display: inline-block; font-size: .7rem; font-weight: 600; padding: .02rem .4rem; border-radius: 4px; white-space: nowrap;
  color: hsl(var(--h) 55% 32%); background: hsl(var(--h) 70% 90%); }
:root[data-theme=dark] .tv-type, .vscode-dark .tv-type { color: hsl(var(--h) 70% 82%); background: hsl(var(--h) 35% 24%); }
@media (prefers-color-scheme: dark) { :root:not([data-theme=light]) .tv-type { color: hsl(var(--h) 70% 82%); background: hsl(var(--h) 35% 24%); } }
.tv-refs-list { max-height: 58vh; overflow: auto; border: 1px solid var(--tv-border); border-radius: 8px; outline: none; }
.tv-refs-list:focus-visible { border-color: var(--tv-accent); }
.tv-refs-list table { width: 100%; border-collapse: collapse; font-size: .82rem; table-layout: fixed; }
.tv-refs-list th { position: sticky; top: 0; background: var(--tv-sunk); text-align: left; font-weight: 600; padding: .3rem .4rem; cursor: pointer; user-select: none; color: var(--tv-muted); }
.tv-refs-list th[aria-sort] { color: var(--tv-fg); } .tv-refs-list td { padding: .25rem .4rem; border-top: 1px solid color-mix(in srgb, var(--tv-border) 50%, transparent); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tv-refs-list tr { cursor: pointer; } .tv-refs-list tbody tr:hover { background: color-mix(in srgb, var(--tv-accent) 6%, transparent); }
.tv-refs-list tr.on { background: color-mix(in srgb, var(--tv-accent) 16%, transparent); } .tv-refs-list td small { color: var(--tv-muted); font-family: ui-monospace, monospace; }
.tv-flag { font-size: .66rem; padding: 0 .3rem; border-radius: 4px; margin-left: .25rem; background: var(--tv-sunk); color: var(--tv-muted); }
.tv-flag.warn { background: color-mix(in srgb, #e0a000 22%, transparent); color: inherit; }
.tv-refs-head { display: flex; flex-wrap: wrap; gap: .4rem; align-items: center; } .tv-refs-head input { flex: 1; min-width: 8rem; }
.tv-refs-head select { width: auto; }
.tv-refs-fields { display: grid; grid-template-columns: 7.5rem 1fr auto; gap: .25rem .4rem; align-items: start; }
.tv-refs-fields > label { padding-top: .3rem; font-size: .8rem; color: var(--tv-muted); font-family: ui-monospace, monospace; overflow: hidden; text-overflow: ellipsis; }
.tv-refs-fields > label.req::after { content: ' *'; color: var(--tv-accent); } .tv-refs-fields > label.extra { font-style: italic; }
.tv-refs-fields .tv-input { width: 100%; box-sizing: border-box; } .tv-refs-fields textarea { min-height: 2.6rem; resize: vertical; }
.tv-refs-fields .tv-under { grid-column: 2 / 4; display: grid; margin-top: -.2rem; } .tv-refs .sd-err { font-size: .76rem; color: #c03030; } .tv-refs .sd-err:empty { display: none; }
.tv-x { font: inherit; border: 0; background: none; color: var(--tv-muted); cursor: pointer; padding: .3rem; } .tv-x:hover { color: #c03030; }
.tv-const { font: inherit; font-size: .74rem; border: 1px dashed var(--tv-accent); border-radius: 4px; background: none; color: var(--tv-accent); cursor: pointer; padding: 0 .3rem; margin-top: .15rem; justify-self: start; }
.tv-refs-note { font-size: .8rem; padding: .35rem .55rem; border-radius: 6px; background: var(--tv-sunk); display: flex; gap: .5rem; align-items: center; flex-wrap: wrap; }
.tv-refs-note.warn { background: color-mix(in srgb, #e0a000 16%, var(--tv-sunk)); }
.tv-refs-empty { color: var(--tv-muted); padding: 1rem; text-align: center; }
.tv-refs-diff { width: 100%; border-collapse: collapse; font-size: .8rem; } .tv-refs-diff td { padding: .15rem .35rem; vertical-align: top; border-top: 1px solid color-mix(in srgb, var(--tv-border) 50%, transparent); }
.tv-refs-diff td:first-child { width: 1.4rem; } .tv-refs-diff td:nth-child(2) { width: 7.5rem; font-family: ui-monospace, monospace; }
.tv-refs-diff del { color: #b03030; } .tv-refs-diff ins { text-decoration: none; color: #208040; }
.tv-refs-row { border: 1px solid var(--tv-border); border-radius: 8px; padding: .4rem .55rem; display: grid; gap: .3rem; }
.tv-refs-drop { border: 2px dashed var(--tv-border); border-radius: 8px; padding: .5rem; display: grid; gap: .4rem; } .tv-refs-drop.over { border-color: var(--tv-accent); }
.tv-refs-drop textarea { min-height: 6rem; font-family: ui-monospace, monospace; font-size: .8rem; }
.tv-refs-consts { width: 100%; border-collapse: collapse; font-size: .84rem; } .tv-refs-consts td, .tv-refs-consts th { text-align: left; padding: .25rem .4rem; border-top: 1px solid color-mix(in srgb, var(--tv-border) 50%, transparent); }
.tv-refs-consts tr.on { background: color-mix(in srgb, var(--tv-accent) 14%, transparent); } .tv-refs-consts .tv-input { width: 9rem; }
.tv-link { font: inherit; border: 0; background: none; color: var(--tv-accent); cursor: pointer; padding: 0; text-decoration: underline dotted; }
@media (max-width: 760px) { .tv-refs-main { grid-template-columns: 1fr; } }`;

  function references(s, view, ctx) {
    if (!document.getElementById('tv-refs-style')) document.head.append(Object.assign(document.createElement('style'), { id: 'tv-refs-style', textContent: STYLE }));
    const D = self.satexDialogs;
    const box = el('div', 'tv-refs');
    const constantNames = [...s.constants.map((c) => c.name), ...s.macros];
    const typeOf = (name) => s.types.find((t) => t.name === name) ?? { name, fields: [], required: [] };
    const allKeys = s.items.map((r) => r.key);
    const net = ctx.fetch ?? (async (url, accept) => { const r = await fetch(url, { headers: accept ? { Accept: accept } : {} }); return { status: r.status, type: r.headers.get('content-type') ?? '', text: await r.text() }; });
    const status = el('span', 'tv-muted');
    const say = (m) => { status.textContent = m; };
    const ask = (payload) => ctx.print(payload).then((r) => { if (r?.error) throw new Error(r.error); return r; });
    // written: the WorkspaceEdit applied, the manager shown again on this entry
    const write = (r) => {
      const n = Object.values(r?.edits?.changes ?? {}).flat().length;
      if (!n) return say((r?.notes ?? []).join('; ') || 'no changes');
      dirty = false;
      ctx.apply(r.edits, { q: search.value, open: selected?.key });
    };
    const fail = (e) => say(String(e?.message ?? e));

    // tabs
    const tabs = el('div', 'tv-refs-tabs');
    tabs.setAttribute('role', 'tablist');
    const panes = {};
    const tab = (name, label) => {
      const b = Object.assign(el('button', '', esc(label)), { type: 'button', onclick: () => show(name) });
      b.setAttribute('role', 'tab');
      b.dataset.tab = name;
      tabs.append(b);
      panes[name] = el('div');
    };
    tab('entries', `Entries (${s.items.length})`);
    tab('constants', `@string (${s.constants.length})`);
    tab('import', 'Import & sync');
    tabs.append(status);
    const show = (name) => {
      for (const b of tabs.querySelectorAll('[role=tab]')) b.setAttribute('aria-selected', String(b.dataset.tab === name));
      for (const [n, p] of Object.entries(panes)) p.hidden = n !== name;
    };
    box.append(tabs, ...Object.values(panes));
    for (const t of s.twice ?? []) {
      box.insertBefore(el('div', 'tv-refs-note warn', `<span><b>${esc(t.name)}</b> is loaded twice (${esc(t.file.split('/').pop())}:${t.line}): every entry is read twice. The lint <code>duplicate-bibliography-load</code> drops the second load.</span>`), tabs.nextSibling);
    }

    // ---- entries: list and editor
    const main = el('div', 'tv-refs-main'), left = el('div', 'tv-refs-left'), right = el('div', 'tv-refs-right');
    main.append(left, right);
    panes.entries.append(main);
    const head = el('div', 'tv-refs-head');
    const search = Object.assign(el('input', 'tv-input'), { placeholder: 'search author, title, year, key, any field', spellcheck: false, value: s.query ?? '' });
    search.ariaLabel = 'search';
    head.append(search);
    const chips = el('div', 'tv-refs-chips');
    const on = new Set();
    const count = (f) => s.items.filter((r) => f.startsWith('type:') ? `type:${r.type}` === f : STATUS[f](r)).length;
    const chip = (f, html) => {
      const c = Object.assign(el('button', 'tv-chip', html), { type: 'button' });
      c.setAttribute('aria-pressed', 'false');
      c.onclick = () => { on.has(f) ? on.delete(f) : on.add(f); c.setAttribute('aria-pressed', String(on.has(f))); draw(); };
      chips.append(c);
    };
    for (const t of [...new Set(s.items.map((r) => r.type))].sort()) chip(`type:${t}`, `<span class="tv-type" style="--h:${hue(t)}">${esc(t)}</span> ${count(`type:${t}`)}`);
    for (const f of Object.keys(STATUS)) if (count(f)) chip(f, `${esc(STATUS_LABEL[f])} ${count(f)}`);
    const list = el('div', 'tv-refs-list');
    list.tabIndex = 0;
    list.ariaLabel = 'entries';
    left.append(head, chips, list);
    let by = 'author', down = false, rows = [];
    let selected = s.items.find((r) => r.key === s.focus) ?? null;
    let dirty = false;
    const draw = () => {
      rows = s.items.filter((r) => matches(r, search.value, [...on])).sort((a, b) => String(SORTS[by](a)).localeCompare(String(SORTS[by](b)), undefined, { numeric: true, sensitivity: 'base' }) * (down ? -1 : 1));
      const th = (k, label, w) => `<th data-sort="${k}" style="width:${w}"${by === k ? ` aria-sort="${down ? 'descending' : 'ascending'}"` : ''}>${label}${by === k ? (down ? ' ▾' : ' ▴') : ''}</th>`;
      const table = el('table', '', `<thead><tr><th style="width:6.4rem">Type</th>${th('author', 'Author', '24%')}${th('year', 'Year', '3.4rem')}${th('title', 'Title', 'auto')}</tr></thead>`);
      const body = el('tbody');
      for (const r of rows.slice(0, 500)) {
        const tr = el('tr', r === selected ? 'on' : '');
        const flags = [!r.cited && '<span class="tv-flag">uncited</span>', r.missing.length && '<span class="tv-flag warn" title="missing required fields">missing</span>',
          r.duplicates.length && '<span class="tv-flag warn">duplicate</span>', r.undefined.length && '<span class="tv-flag warn">@string?</span>'].filter(Boolean).join('');
        tr.innerHTML = `<td><span class="tv-type" style="--h:${hue(r.type)}">${esc(r.type)}</span></td><td title="${esc(r.author)}">${esc(r.author)}</td><td>${esc(r.year)}</td><td title="${esc(r.title)}">${esc(r.title)}<br><small>${esc(r.key)}</small>${flags}</td>`;
        tr.onclick = () => pick(r);
        body.append(tr);
      }
      table.append(body);
      for (const h of table.querySelectorAll('th[data-sort]')) h.onclick = () => { down = by === h.dataset.sort ? !down : false; by = h.dataset.sort; draw(); };
      list.replaceChildren(rows.length ? table : el('div', 'tv-refs-empty', 'No entry matches.'));
    };
    const pick = async (r) => {
      if (r === selected) return;
      if (dirty && D?.discard && !(await D.discard(box.closest('dialog') ?? box))) return;
      selected = r;
      dirty = false;
      draw();
      editor();
      list.querySelector('tr.on')?.scrollIntoView({ block: 'nearest' });
    };
    list.onkeydown = (e) => {
      const i = rows.indexOf(selected);
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); const r = rows[Math.max(0, Math.min(rows.length - 1, i + (e.key === 'ArrowDown' ? 1 : -1)))]; if (r) pick(r); }
      if (e.key === 'Enter') { e.preventDefault(); right.querySelector('input, textarea')?.focus(); }
    };
    search.oninput = draw;
    search.onkeydown = (e) => { if (e.key === 'ArrowDown') { e.preventDefault(); list.focus(); if (!selected && rows[0]) pick(rows[0]); } };

    // the detail editor of the selected entry
    const editor = (kept = null) => {
      const r = selected;
      if (!r) return right.replaceChildren(el('div', 'tv-refs-empty', s.items.length ? 'Select an entry.' : 'No entries yet: import some.'));
      const draft = kept ?? structuredClone(r.node);
      const was = JSON.stringify(r.node);
      const changed = () => { dirty = JSON.stringify(draft) !== was; save.disabled = !dirty || bad(); revert.disabled = !dirty; };
      const top = el('div', 'tv-refs-head');
      const key = Object.assign(el('input', 'tv-input'), { value: draft.args[0].text, spellcheck: false, ariaLabel: 'citation key' });
      const keyMsg = el('div', 'sd-err');
      key.oninput = () => { draft.args[0].text = key.value.trim(); keyMsg.textContent = D?.check?.(key.value, ['required', 'bibkey', 'unique'], allKeys.filter((k) => k !== r.key)) ?? ''; changed(); };
      const type = el('select', 'tv-input', [...new Set([r.type, ...s.types.map((t) => t.name)])].map((t) => `<option${t === r.type ? ' selected' : ''}>${esc(t)}</option>`).join(''));
      type.ariaLabel = 'entry type';
      const convert = button('Convert to…', () => {
        if (type.value === r.type) return say('choose another type first');
        ask({ bib: 'convert', key: r.key, to: type.value }).then((res) => {
          const note = el('div', 'tv-refs-note', `<span>@${esc(r.type)} → @${esc(type.value)}${res.notes?.length ? ': ' + esc(res.notes.join('; ')) : ''}</span>`);
          note.append(button('Apply', () => write(res), 'tv-primary'), button('Cancel', () => note.remove()));
          top.after(note);
        }, fail);
      });
      convert.disabled = true;
      type.onchange = () => { convert.disabled = type.value === r.type || dirty; };
      top.append(Object.assign(el('span', 'tv-type', esc(r.type)), { style: `--h:${hue(r.type)}` }), key, type, convert);
      const where = el('div', 'tv-muted tv-uses', ctx.markdown(`In \`${r.file.split('/').pop()}:${r.line}\`. ${r.places}`));
      where.onclick = (e) => { if (e.target.closest('a.place')) ctx.close?.(); };
      const notes = el('div');
      if (r.missing.length) notes.append(el('div', 'tv-refs-note warn', `<span>@${esc(r.type)} requires ${r.missing.map((m) => `<b>${esc(m)}</b>`).join(', ')} (${esc(s.style)}).</span>`));
      if (r.undefined.length) notes.append(el('div', 'tv-refs-note warn', `<span>No <code>@string</code> defines ${r.undefined.map((m) => `<b>${esc(m)}</b>`).join(', ')}.</span>`));
      for (const d of r.duplicates) {
        const n = el('div', 'tv-refs-note warn', `<span>${d.drop ? `<b>${esc(d.keep)}</b> and <b>${esc(d.drop)}</b> have ${esc(d.why)}.` : `Another entry has ${esc(d.why)}.`}</span>`);
        if (d.drop) n.append(button(`Merge ${d.drop} into ${d.keep}`, () => ask({ bib: 'merge', keep: d.keep, drop: d.drop }).then(write, fail)));
        notes.append(n);
      }
      // the fields: required first, then the type's order, then what the type does not read
      const t = typeOf(draft.name.toLowerCase());
      const required = new Set(t.required.flat());
      const present = keysOf(draft).map((k) => k.key);
      const missingNames = t.required.filter((alt) => !alt.some((f) => present.includes(f))).map((alt) => alt[0]);
      const order = [...new Set([...t.required.flat().filter((f) => present.includes(f)), ...missingNames, ...t.fields.filter((f) => present.includes(f)), ...present])];
      const grid = el('div', 'tv-refs-fields');
      const checks = [];
      const bad = () => checks.some((c) => c());
      const row = (name) => {
        const value = get(draft, name) ?? '';
        const lab = el('label', required.has(name) ? 'req' : t.fields.length && !t.fields.includes(name) ? 'extra' : '', esc(name));
        if (lab.className === 'extra') lab.title = `@${draft.name} does not read ${name}`;
        const long = value.length > 60 || ['abstract', 'title', 'note'].includes(name);
        const input = Object.assign(el(long ? 'textarea' : 'input', 'tv-input'), { value, spellcheck: !['doi', 'url', 'pages', 'isbn', 'issn'].includes(name) });
        input.ariaLabel = name;
        if (!long) input.setAttribute('list', 'tv-refs-consts');
        const msg = el('div', 'sd-err');
        const link = el('div');
        const under = el('div', 'tv-under');
        under.append(msg, link);
        const rules = s.checks[name] ?? ['balanced'];
        const check = () => {
          const v = input.value;
          // a constant (or a concatenation of them) is checked where it is defined
          const m = !v.trim() || isBare(v, constantNames) ? '' : D?.check?.(v, rules, allKeys) ?? '';
          msg.textContent = m;
          input.classList.toggle('tv-bad', !!m);
          const words = isBare(v, constantNames) ? v.split('#').map((p) => p.trim()).filter((p) => constantNames.some((c) => c.toLowerCase() === p.toLowerCase())) : [];
          link.replaceChildren(...words.map((w) => { const c = s.constants.find((x) => x.name.toLowerCase() === w.toLowerCase()); return Object.assign(el('button', 'tv-const', `@string ${esc(w)}${c ? ` = ${esc(c.value)}` : ' (style)'}`), { type: 'button', onclick: () => c && focusConstant(c.name) }); }));
          return !!m;
        };
        checks.push(check);
        input.oninput = () => { setField(draft, name, input.value, constantNames); check(); changed(); };
        const x = Object.assign(el('button', 'tv-x', '×'), { type: 'button', title: `remove ${name}`, onclick: () => { setField(draft, name, ''); input.value = ''; check(); changed(); } });
        grid.append(lab, input, x, under);
        check();
      };
      for (const f of order) row(f);
      // a new field: any the data model knows, its value checked by its type
      const add = el('div', 'tv-refs-head');
      const name = Object.assign(el('input', 'tv-input'), { placeholder: 'add field…', spellcheck: false });
      name.setAttribute('list', 'tv-refs-fieldnames');
      name.ariaLabel = 'new field name';
      const names = el('datalist', null, [...t.fields, ...s.fields].filter((f, i, a) => a.indexOf(f) === i && !present.includes(f)).map((f) => `<option value="${esc(f)}">`).join(''));
      names.id = 'tv-refs-fieldnames';
      const value = Object.assign(el('input', 'tv-input'), { placeholder: 'value', spellcheck: false });
      value.ariaLabel = 'new field value';
      value.setAttribute('list', 'tv-refs-consts');
      const addMsg = el('div', 'sd-err');
      const addCheck = () => { const m = !name.value.trim() ? '' : D?.check?.(name.value, ['key', 'unique'], keysOf(draft).map((k) => k.key)) || (isBare(value.value, constantNames) ? '' : D?.check?.(value.value, ['required', ...(s.checks[name.value] ?? ['balanced'])], allKeys)) || ''; addMsg.textContent = m; return m; };
      name.oninput = value.oninput = addCheck;
      const addIt = () => { if (addCheck() || !name.value.trim()) return; setField(draft, name.value.trim().toLowerCase(), value.value, constantNames); changed(); rebuild(); };
      value.onkeydown = (e) => { if (e.key === 'Enter') { e.preventDefault(); addIt(); } };
      add.append(name, value, button('Add', addIt), names);
      const consts = el('datalist', null, constantNames.map((c) => `<option value="${esc(c)}">`).join(''));
      consts.id = 'tv-refs-consts';
      // writing: the snapshot printed into its database (a renamed key rewrites its \cite calls too)
      const acts = el('div', 'tv-actions');
      const save = button('Save', () => ask({ bib: 'print', file: r.file, nodes: [draft] }).then(write, fail), 'tv-primary');
      const revert = button('Revert', () => { dirty = false; editor(); });
      const dblp = button('Check with dblp', () => sync([r]).then((rows) => results(rows, right), fail));
      const del = button(`Delete ${r.key}`, () => ask({ bib: 'print', file: r.file, nodes: [{ ...r.node, drop: true }] }).then(write, fail), 'tv-danger');
      del.disabled = r.cited > 0;
      del.title = r.cited ? `cited ${r.cited} times` : '';
      acts.append(save, revert, dblp, del);
      const rebuild = () => editor(draft);
      right.replaceChildren(top, keyMsg, where, notes, grid, add, addMsg, acts, consts);
      changed();
    };

    // ---- @string constants
    const focusConstant = (name) => { show('constants'); drawConstants(name); };
    const drawConstants = (focus) => {
      const table = el('table', 'tv-refs-consts', '<thead><tr><th>Name</th><th>Value</th><th>Defined</th><th>Used by</th><th></th></tr></thead>');
      const body = el('tbody');
      for (const c of s.constants) {
        const tr = el('tr', c.name === focus ? 'on' : '');
        const name = Object.assign(el('input', 'tv-input'), { value: c.name, spellcheck: false, ariaLabel: `rename ${c.name}` });
        const uses = el('td');
        for (const k of c.uses) uses.append(Object.assign(el('button', 'tv-link', esc(k)), { type: 'button', onclick: () => { show('entries'); pick(s.items.find((r) => r.key === k)); } }), ' ');
        const acts = el('td');
        acts.append(button('Rename', () => ask({ bib: 'constant', name: c.name, rename: name.value.trim(), file: c.file }).then(write, fail)),
          Object.assign(button('Delete', () => ask({ bib: 'constant', name: c.name, delete: true, file: c.file }).then(write, fail), 'tv-danger'), { disabled: c.uses.length > 0, title: c.uses.length ? 'used' : '' }));
        const n = el('td');
        n.append(name);
        tr.append(n, el('td', '', esc(c.value)), el('td', 'tv-muted', `${esc(c.file.split('/').pop())}:${c.line}`), uses, acts);
        body.append(tr);
        if (c.name === focus) setTimeout(() => tr.scrollIntoView({ block: 'nearest' }), 0);
      }
      table.append(body);
      const parts = [s.constants.length ? table : el('div', 'tv-refs-empty', 'No @string constants.')];
      if (s.macros.length) parts.push(el('p', 'tv-muted', `The style predefines ${s.macros.map(esc).join(', ')}.`));
      if (s.repeated.length) {
        const make = el('div', 'tv-refs-row', '<b>Make a constant</b><span class="tv-muted">Values written in several entries: one @string, used by name.</span>');
        for (const r of s.repeated) {
          const line = el('div', 'tv-refs-head');
          const suggested = r.value.split(/[^A-Za-z]+/).map((w) => w[0] ?? '').join('').toLowerCase() || 'name';
          const n = Object.assign(el('input', 'tv-input'), { value: suggested, spellcheck: false, ariaLabel: `constant for ${r.value}` });
          n.style.maxWidth = '8rem';
          line.append(el('span', '', `${esc(r.field)} = {${esc(r.value)}} <span class="tv-muted">in ${r.keys.length} entries</span>`), n,
            button('Make constant', () => ask({ bib: 'constant', make: r.value, name: n.value.trim(), file: s.files[0]?.path }).then(write, fail)));
          make.append(line);
        }
        parts.push(make);
      }
      panes.constants.replaceChildren(...parts);
    };

    // ---- import and dblp
    const from = el('select', 'tv-input', s.files.map((f) => `<option value="${esc(f.path)}">${esc(f.name)}</option>`).join(''));
    from.ariaLabel = 'database to write to';
    const policy = el('select', 'tv-input', '<option value="keep">keep their keys</option><option value="new">make keys (author year word)</option>');
    policy.ariaLabel = 'keys';
    const out = el('div');
    const parse = (text, keys = policy.value) => ask({ bib: 'import', text, file: from.value, keys });
    /** The rows of each entry checked against dblp's search, one request a second; a refusal stops. */
    const sync = async (entries) => {
      const got = [];
      for (const [i, r] of entries.entries()) {
        if (i) await new Promise((ok) => setTimeout(ok, 1100));
        say(`dblp: ${i + 1}/${entries.length} ${r.key}`);
        const q = r.title.replace(/[{}\\$]/g, '');
        const res = await net(`https://dblp.org/search/publ/api?q=${encodeURIComponent(q)}&format=json&h=5`, 'application/json').catch((e) => ({ status: 0, text: String(e) }));
        if (res.status === 429) throw new Error('dblp.org asks to slow down (HTTP 429): try again in a minute');
        if (res.status !== 200 || !res.text.trim().startsWith('{')) throw new Error(`dblp.org did not answer with records (HTTP ${res.status}${/bot/i.test(res.text) ? ', a bot check: open dblp.org in this browser once' : res.status ? '' : ': the browser blocked the request; VS Code fetches without that limit'})`);
        const one = await ask({ bib: 'import', text: res.text, file: r.file, into: r.key }).catch(() => null);
        if (one?.rows?.length) got.push(...one.rows);
      }
      say(`dblp: ${got.length} of ${entries.length} found`);
      return got;
    };
    /** The rows of an import: per reference its fields, a new value unchecked where it replaces one (no local edit is lost unasked). */
    const results = (rows, into = out) => {
      if (!rows.length) return into.replaceChildren(el('div', 'tv-refs-empty', 'Nothing to change: the records agree.'));
      const picks = rows.map((row) => Object.fromEntries(row.fields.map((f) => [f.name, f.old == null])));
      const keys = rows.map((row) => row.key);
      const parts = rows.map((row, i) => {
        const card = el('div', 'tv-refs-row');
        const title = el('div', 'tv-refs-head', `<span class="tv-type" style="--h:${hue(row.type)}">${esc(row.type)}</span> <b>${esc(row.title)}</b>`);
        if (row.match) title.append(el('span', 'tv-muted', `updates <b>${esc(row.match)}</b>`));
        else {
          const k = Object.assign(el('input', 'tv-input'), { value: row.key, ariaLabel: 'key of the new entry' });
          k.style.maxWidth = '14rem';
          k.oninput = () => { keys[i] = k.value.trim(); };
          title.append(el('span', 'tv-muted', 'new, as'), k);
        }
        const table = el('table', 'tv-refs-diff');
        for (const f of row.fields) {
          const tr = el('tr');
          const c = Object.assign(el('input'), { type: 'checkbox', checked: picks[i][f.name], ariaLabel: `take ${f.name}` });
          c.onchange = () => { picks[i][f.name] = c.checked; };
          const td = el('td');
          td.append(c);
          tr.append(td, el('td', 'tv-muted', esc(f.name)), el('td', '', f.old == null ? `<ins>${esc(f.value)}</ins>` : `<del>${esc(f.old)}</del><br><ins>${esc(f.value)}</ins>`));
          table.append(tr);
        }
        card.append(title, table);
        return card;
      });
      const apply = button(`Apply selected`, async () => {
        const nodes = chosen(rows, picks, keys, constantNames);
        const files = Object.entries(nodes);
        if (!files.length) return say('nothing selected');
        const all = [];
        for (const [file, ns] of files) all.push(await ask({ bib: 'print', file, nodes: ns }).catch((e) => { fail(e); return null; }));
        write({ edits: joined(all.filter(Boolean).map((r) => r.edits)), notes: all.flatMap((r) => r?.notes ?? []) });
      }, 'tv-primary');
      into.replaceChildren(...parts, apply);
    };
    const fetchOne = async (text) => {
      const id = identify(text);
      if (!id) throw new Error('give a DOI, an arXiv id or a URL');
      say(`fetching ${id.what}…`);
      let res = await net(id.doi ? `https://doi.org/${id.doi}` : id.url, id.doi ? 'application/x-bibtex' : 'application/x-bibtex, application/x-research-info-systems, application/vnd.citationstyles.csl+json;q=0.9, text/html;q=0.5')
        .catch((e) => { throw new Error(`the request failed (${e.message ?? e}): the site may allow no browser requests`); });
      // a page that names its DOI (`citation_doi`, a doi.org link): its record
      const inPage = !id.doi && res.text.match(/citation_doi"\s+content="([^"]+)"|doi\.org\/(10\.\d{4,9}\/[^\s"'<>]+)/);
      if (inPage) res = await net(`https://doi.org/${inPage[1] ?? inPage[2]}`, 'application/x-bibtex');
      if (res.status !== 200) throw new Error(`${id.what}: HTTP ${res.status}`);
      return res.text;
    };
    const target = el('input', 'tv-input');
    Object.assign(target, { placeholder: 'DOI, arXiv id or URL', spellcheck: false, ariaLabel: 'DOI, arXiv id or URL' });
    const getIt = button('Fetch', () => fetchOne(target.value).then((t) => parse(t, 'new')).then((r) => { say(''); results(r.rows); }, fail), 'tv-primary');
    target.onkeydown = (e) => { if (e.key === 'Enter') getIt.click(); };
    const drop = el('div', 'tv-refs-drop', '<b>Paste or drop</b> <span class="tv-muted">BibTeX (Zotero Better BibTeX, dblp), RIS, EndNote (.enw), CSL-JSON or arXiv Atom</span>');
    const paste = Object.assign(el('textarea', 'tv-input'), { spellcheck: false, placeholder: 'TY  - JOUR …   or   @article{…}   or   [{"type": …}]' });
    paste.ariaLabel = 'references to import';
    const file = Object.assign(el('input'), { type: 'file', accept: '.bib,.ris,.enw,.json,.txt,.xml' });
    file.onchange = async () => { paste.value = await file.files[0]?.text() ?? ''; };
    drop.ondragover = (e) => { e.preventDefault(); drop.classList.add('over'); };
    drop.ondragleave = () => drop.classList.remove('over');
    drop.ondrop = async (e) => { e.preventDefault(); drop.classList.remove('over'); paste.value = (await e.dataTransfer.files[0]?.text()) ?? e.dataTransfer.getData('text'); };
    const readIt = button('Import', () => parse(paste.value).then((r) => results(r.rows), fail), 'tv-primary');
    const dr = el('div', 'tv-refs-head');
    dr.append(file, readIt);
    drop.append(paste, dr);
    // Zotero running here: its local API (Zotero 7), else Better BibTeX's pull export
    const zotero = button('Import update from Zotero', async () => {
      const urls = ['http://127.0.0.1:23119/better-bibtex/export/library?/1/library.biblatex', 'http://127.0.0.1:23119/api/users/0/items?format=bibtex&limit=100'];
      for (const u of urls) {
        const res = await net(u, '').catch(() => null);
        if (res?.status === 200 && res.text.trim()) return parse(res.text, 'keep').then((r) => results(r.rows), fail);
      }
      fail('Zotero is not reachable: start it and allow other applications (Settings › Advanced) or install Better BibTeX; else export the library as .bib or CSL-JSON and drop it above');
    });
    const all = button('Check all with dblp', () => sync(s.items).then((rows) => { show('import'); results(rows); }, fail));
    const line = el('div', 'tv-refs-head');
    line.append(target, getIt);
    const opts = el('div', 'tv-refs-head');
    opts.append(el('span', 'tv-muted', 'into'), from, el('span', 'tv-muted', 'keys:'), policy, zotero, all);
    panes.import.append(el('div', 'tv-refs-left'));
    panes.import.firstChild.append(line, drop, opts, el('p', 'tv-muted', 'Nothing is sent anywhere until you click. A record that matches an entry (by key, DOI or title) updates it: new fields are checked, changed values are not, so no local edit is lost unasked.'), out);

    draw();
    editor();
    drawConstants();
    show('entries');
    ctx.guard?.(() => dirty);
    setTimeout(() => (selected ? list : search).focus(), 0);
    return box;
  }

  self.satexViews.RENDER.references = references;
  self.satexBib = { hue, isBare, setField, matches, identify, chosen, joined, STATUS };
})();
