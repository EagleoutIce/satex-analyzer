// The table and image dialogs of `satex/dialog`: one component for the playground and the VS Code webview.
// `open(view, host)`: `view` is the server's `{kind, range, state, context}`; `host` writes and shows:
//   host.apply(state, op?) -> Promise<{state?, edits, notes}>   (the server's `satex/dialogApply`, nothing written)
//   host.commit(edits) -> void                                  (writes the edits)
//   host.image(path) -> Promise<{url, kind: 'img'|'pdf'|null}>  (the figure's file, a PDF's first page as an image)
//   host.parent: an element to render into (VS Code), else a modal <dialog>.
(() => {
  const CSS = `
.sd { --c-bg: var(--panel, var(--vscode-editor-background, #fff)); --c-fg: var(--fg, var(--vscode-foreground, #1b1d21));
  --c-border: var(--border, var(--vscode-panel-border, #d6dae1)); --c-accent: var(--accent, var(--vscode-focusBorder, #0b3d91));
  --c-sunk: var(--sunk, var(--vscode-input-background, #f1f3f6)); --c-muted: var(--muted, var(--vscode-descriptionForeground, #5c6270));
  --c-accent-fg: var(--accent-fg, var(--vscode-button-foreground, #fff));
  color: var(--c-fg); background: var(--c-bg); font: 13px/1.4 system-ui, sans-serif; display: flex; flex-direction: column; min-height: 0; }
dialog.sd-dialog { width: min(1100px, 96vw); max-width: none; height: min(88vh, 46rem); padding: 0; border: 1px solid var(--c-border); border-radius: 12px; }
dialog.sd-dialog[open] { display: flex; }
dialog.sd-dialog > .sd { flex: 1; min-width: 0; }
dialog.sd-dialog::backdrop { background: rgb(0 0 0 / .35); }
.sd > header, .sd > footer { max-width: none; width: auto; margin: 0; box-sizing: border-box; display: flex; align-items: center; gap: .5rem; padding: .55rem .9rem; flex-wrap: wrap; }
.sd > header { border-bottom: 1px solid var(--c-border); }
.sd > header h3 { margin: 0; font-size: 1rem; flex: 1; }
.sd > footer { border-top: 1px solid var(--c-border); justify-content: flex-end; }
.sd-body { flex: 1; min-height: 0; overflow: auto; padding: .7rem .9rem; display: grid; gap: .8rem; grid-template-columns: minmax(0, 1fr) minmax(16rem, 22rem); }
.sd-body > * { min-width: 0; }
@media (max-width: 760px) { .sd-body { grid-template-columns: 1fr; } }
.sd button, .sd select, .sd input { font: inherit; color: inherit; max-width: 100%; }
.sd button { background: var(--c-sunk); border: 1px solid var(--c-border); border-radius: 6px; padding: .2rem .55rem; cursor: pointer; }
.sd button:hover { border-color: var(--c-accent); }
.sd button.sd-primary { background: var(--c-accent); color: var(--c-accent-fg); border-color: var(--c-accent); }
.sd button[aria-pressed=true] { outline: 2px solid var(--c-accent); outline-offset: -1px; }
.sd input, .sd select { background: var(--c-bg); border: 1px solid var(--c-border); border-radius: 5px; padding: .15rem .3rem; min-width: 0; }
.sd-bar { display: flex; flex-wrap: wrap; gap: .3rem; align-items: center; margin-bottom: .5rem; }
.sd-bar .sd-sep { width: 1px; align-self: stretch; background: var(--c-border); margin: 0 .2rem; }
.sd-muted { color: var(--c-muted); font-size: .85em; }
.sd-note { color: var(--warning, var(--vscode-editorWarning-foreground, #8a5a00)); font-size: .85em; }
.sd-tex { margin: 0; padding: .5rem; background: var(--c-sunk); border-radius: 6px; font: 12px/1.35 ui-monospace, monospace; white-space: pre-wrap; overflow-wrap: anywhere; overflow: auto; max-height: 18rem; }
.sd-side h4 { margin: .6rem 0 .3rem; font-size: .9rem; }
.sd-side h4:first-child { margin-top: 0; }
.sd-grid { border-collapse: collapse; }
.sd-grid td, .sd-grid th { padding: 0; border: 1px dashed var(--c-border); }
.sd-grid th { font-weight: normal; padding: .15rem; background: var(--c-sunk); }
.sd-grid th select { width: 100%; }
.sd-grid th input { width: 4.5rem; margin-top: .15rem; }
.sd-grid td input { border: 0; border-radius: 0; width: 100%; min-width: 4rem; padding: .3rem .4rem; background: transparent; }
.sd-grid td.sd-cur { outline: 2px solid var(--c-accent); outline-offset: -2px; }
.sd-grid td[data-align=c] input, .sd-grid td[data-align=S] input { text-align: center; }
.sd-grid td[data-align=r] input { text-align: right; }
.sd-grid tr.sd-rule td { border: 0; padding: 0 0 0 .2rem; height: 1.3rem; }
.sd-grid tr.sd-rule td.sd-ruled { background: linear-gradient(var(--c-fg), var(--c-fg)) 0 50% / 100% var(--w, 1px) no-repeat; }
.sd-grid .sd-rh { border: 0; padding: 0 .3rem; text-align: right; color: var(--c-muted); font-size: .8em; white-space: nowrap; }
.sd-grid tr.sd-rule select { font-size: .78em; padding: 0 .1rem; }
.sd-grid th.sd-v { width: .6rem; min-width: .6rem; border: 0; background: none; cursor: pointer; padding: 0; }
.sd-grid th.sd-v.on { background: linear-gradient(var(--c-fg), var(--c-fg)) 50% 0 / 1px 100% no-repeat; }
.sd-chip { display: inline-block; font: 11px ui-monospace, monospace; background: var(--c-sunk); border-radius: 4px; padding: 0 .3rem; margin-right: .2rem; }
.sd-stage { position: relative; display: inline-block; touch-action: none; user-select: none; background: repeating-conic-gradient(#8882 0 25%, transparent 0 50%) 0 0 / 16px 16px; max-width: 100%; }
.sd-stage img, .sd-stage .sd-ph { display: block; max-width: 100%; max-height: 52vh; }
.sd-ph { display: grid; place-items: center; text-align: center; color: var(--c-muted); border: 1px dashed var(--c-border); padding: 1rem; box-sizing: border-box; }
.sd-crop { position: absolute; box-shadow: 0 0 0 9999px rgb(0 0 0 / .45); outline: 1px solid #fff; cursor: move; }
.sd-stage { overflow: hidden; }
.sd-h { position: absolute; width: 14px; height: 14px; background: #fff; border: 1px solid #000; border-radius: 3px; transform: translate(-50%, -50%); }
.sd-prev-box { display: grid; place-items: center; min-height: 9rem; background: var(--c-sunk); border-radius: 6px; overflow: hidden; padding: .5rem; }
.sd-prev-crop { overflow: hidden; position: relative; outline: 1px solid var(--c-border); }
.sd-prev-crop > * { position: absolute; display: block; max-width: none; max-height: none; }
.sd-row { display: flex; gap: .35rem; align-items: center; flex-wrap: wrap; margin: .25rem 0; }
.sd-row label { display: inline-flex; gap: .25rem; align-items: center; }
.sd-num { width: 4.2rem; }
.sd-len { width: 9rem; }
.sd-inline { height: 100%; }
`;
  const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  const h = (tag, attrs = {}, ...kids) => {
    const e = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
      else if (k === 'class') e.className = v;
      else if (v === true) e.setAttribute(k, '');
      else if (v !== false && v != null) e.setAttribute(k, v);
    }
    e.append(...kids.flat().filter((k) => k != null));
    return e;
  };
  const clone = (x) => JSON.parse(JSON.stringify(x));
  let styled = false;

  /** The frame both dialogs share: title, body, the TeX they write with its notes, Apply. */
  function frame(view, host, title, render) {
    if (!styled) { document.head.append(h('style', {}, CSS)); styled = true; }
    const root = h('div', { class: 'sd' + (host.parent ? ' sd-inline' : '') });
    const tex = h('pre', { class: 'sd-tex', 'aria-label': 'the TeX written' });
    const notes = h('div', { class: 'sd-note' });
    let result = null, timer = 0, seq = 0;
    const close = () => { dlg?.close(); dlg?.remove(); host.closed?.(); };
    const apply = h('button', { type: 'button', class: 'sd-primary', onclick: () => { if (result) { host.commit(result.edits); close(); } } }, 'Apply');
    root.append(
      h('header', {}, h('h3', {}, title), host.parent ? null : h('button', { type: 'button', 'aria-label': 'close', onclick: close }, '✕')),
      h('div', { class: 'sd-body' }, render.main, h('div', { class: 'sd-side' }, render.side, h('h4', {}, 'Written as'), tex, notes)),
      h('footer', {}, h('span', { class: 'sd-muted', style: 'flex:1' }, render.foot ?? ''), host.parent ? null : h('button', { type: 'button', onclick: close }, 'Cancel'), apply),
    );
    /** Asks the server for the text `state` writes; `op` transforms the state first (booktabs, siunitx). */
    const refresh = (op) => {
      clearTimeout(timer);
      return new Promise((done) => {
        timer = setTimeout(async () => {
          const mine = ++seq;
          const r = await host.apply(view.state, op).catch((e) => ({ error: e.message }));
          if (mine !== seq) return done();
          if (r?.error) { notes.textContent = r.error; apply.disabled = true; return done(); }
          result = r;
          if (r.state) { view.state = r.state; render.redraw(); }
          tex.textContent = r.edits?.[0]?.newText ?? '';
          notes.textContent = (r.notes ?? []).join('; ');
          apply.disabled = false;
          done();
        }, op ? 0 : 150);
      });
    };
    let dlg = null;
    if (host.parent) host.parent.replaceChildren(root);
    else {
      dlg = h('dialog', { class: 'sd-dialog', 'aria-label': title }, root);
      dlg.addEventListener('close', () => dlg.remove());
      document.body.append(dlg);
      dlg.showModal();
    }
    return { refresh, root };
  }

  // ------------------------------------------------------------ table

  const ALIGNS = [['l', 'left'], ['c', 'center'], ['r', 'right'], ['p', 'paragraph p{…}'], ['S', 'siunitx S']];
  const RULES = ['\\hline', '\\toprule', '\\midrule', '\\bottomrule'];
  const width = (row) => row.cells.reduce((n, c) => n + Math.max(1, c.span), 0);
  /** The index of the cell covering column `col`, and the column it starts at. */
  function cellAt(row, col) {
    let at = 0;
    for (let i = 0; i < row.cells.length; i++) {
      const s = Math.max(1, row.cells[i].span);
      if (col < at + s) return [i, at];
      at += s;
    }
    return [-1, at];
  }
  const blank = () => ({ text: '', span: 1, spec: '' });

  function table(view, host) {
    const t = () => view.state;
    const ctx = view.context ?? {};
    let cur = [0, 0]; // row, column
    const main = h('div');
    const side = h('div');
    const ui = { main, side, redraw: () => draw(), foot: 'Rows the dialog leaves as they are keep their text.' };
    const f = frame(view, host, 'Edit table', ui);
    const change = (fn) => { fn(t()); draw(); f.refresh(); };
    const ops = {
      addRow: (s) => {
        const r = Math.min(cur[0], s.rows.length - 1);
        if (r >= 0 && !s.rows[r].end) s.rows[r].end = '\\\\';
        s.rows.splice(r + 1, 0, { before: [], inline: false, cells: s.cols.map(blank), end: r + 1 < s.rows.length || s.rows.length === 0 ? '\\\\' : '\\\\', comment: '', raw: '' });
        cur = [r + 1, cur[1]];
      },
      delRow: (s) => { if (s.rows.length > 1) { const [gone] = s.rows.splice(cur[0], 1); if (s.rows[cur[0]]) s.rows[cur[0]].before = [...gone.before, ...s.rows[cur[0]].before]; cur[0] = Math.min(cur[0], s.rows.length - 1); } },
      moveRow: (s, d) => {
        const a = cur[0], b = a + d;
        if (b < 0 || b >= s.rows.length) return;
        for (const k of ['cells', 'raw', 'comment']) [s.rows[a][k], s.rows[b][k]] = [s.rows[b][k], s.rows[a][k]];
        cur[0] = b;
      },
      addCol: (s) => {
        const at = Math.min(cur[1] + 1, s.cols.length);
        s.cols.splice(at, 0, { align: 'l', arg: '', pre: '', post: '' });
        s.seps.splice(at === s.cols.length - 1 ? at : at + 1, 0, '');
        for (const r of s.rows) {
          const [i, start] = cellAt(r, at);
          if (i < 0) r.cells.push(blank());
          else if (start === at) r.cells.splice(i, 0, blank());
          else r.cells[i].span++;
        }
        cur[1] = at;
      },
      delCol: (s) => {
        if (s.cols.length < 2) return;
        const c = cur[1];
        s.cols.splice(c, 1);
        s.seps.splice(c === s.cols.length ? c : c + 1, 1);
        for (const r of s.rows) {
          const [i] = cellAt(r, c);
          if (i < 0) continue;
          if (r.cells[i].span > 1) r.cells[i].span--; else r.cells.splice(i, 1);
        }
        cur[1] = Math.min(c, s.cols.length - 1);
      },
      moveCol: (s, d) => {
        const a = cur[1], b = a + d;
        if (b < 0 || b >= s.cols.length) return;
        const pairs = s.rows.map((r) => [cellAt(r, a), cellAt(r, b)]);
        if (pairs.some(([[i], [j]], k) => i >= 0 && j >= 0 && (s.rows[k].cells[i].span > 1 || s.rows[k].cells[j].span > 1))) return note('A merged cell spans the columns: split it first.');
        [s.cols[a], s.cols[b]] = [s.cols[b], s.cols[a]];
        s.rows.forEach((r, k) => { const [[i], [j]] = pairs[k]; if (i >= 0 && j >= 0) [r.cells[i], r.cells[j]] = [r.cells[j], r.cells[i]]; });
        cur[1] = b;
      },
      merge: (s) => {
        const r = s.rows[cur[0]], [i] = cellAt(r, cur[1]);
        if (i < 0 || i + 1 >= r.cells.length) return;
        const [a, b] = [r.cells[i], r.cells[i + 1]];
        a.text = [a.text, b.text].filter((x) => x.trim()).join(' ');
        a.span = Math.max(1, a.span) + Math.max(1, b.span);
        a.spec ||= 'c';
        r.cells.splice(i + 1, 1);
      },
      split: (s) => {
        const r = s.rows[cur[0]], [i] = cellAt(r, cur[1]);
        if (i < 0 || r.cells[i].span < 2) return;
        r.cells[i].span--;
        if (r.cells[i].span === 1) r.cells[i].spec = '';
        r.cells.splice(i + 1, 0, blank());
      },
    };
    const msg = h('span', { class: 'sd-note' });
    const note = (m) => { msg.textContent = m; setTimeout(() => { if (msg.textContent === m) msg.textContent = ''; }, 4000); };
    const btn = (label, title, fn) => h('button', { type: 'button', title, 'aria-label': title, onclick: () => change(fn) }, label);
    const pkg = (name) => ctx[name] ? '' : ` (adds \\usepackage{${name}})`;
    const bar = h('div', { class: 'sd-bar' },
      btn('+ row', 'Add a row below the current one', ops.addRow), btn('− row', 'Remove the current row', ops.delRow),
      btn('↑', 'Move the row up', (s) => ops.moveRow(s, -1)), btn('↓', 'Move the row down', (s) => ops.moveRow(s, 1)),
      h('span', { class: 'sd-sep' }),
      btn('+ col', 'Add a column right of the current one', ops.addCol), btn('− col', 'Remove the current column', ops.delCol),
      btn('←', 'Move the column left', (s) => ops.moveCol(s, -1)), btn('→', 'Move the column right', (s) => ops.moveCol(s, 1)),
      h('span', { class: 'sd-sep' }),
      btn('Merge →', 'Merge the cell with the next (\\multicolumn)', ops.merge), btn('Split', 'Split a merged cell', ops.split),
      msg);
    const grid = h('table', { class: 'sd-grid' });
    main.append(bar, h('div', { style: 'overflow:auto' }, grid));
    side.append(
      h('h4', {}, 'Rewrite'),
      h('div', { class: 'sd-row' }, h('button', { type: 'button', title: `booktabs' rules: \\toprule, \\midrule, \\bottomrule, no vertical rules${pkg('booktabs')}`, onclick: () => f.refresh('booktabs') }, 'Convert to booktabs')),
      h('div', { class: 'sd-row' }, h('button', { type: 'button', title: `Numeric columns as siunitx S columns aligned at the decimal mark${pkg('siunitx')}`, onclick: () => f.refresh('siunitx') }, 'Numbers as siunitx S')),
      ctx.booktabs && ctx.siunitx ? null : h('p', { class: 'sd-muted' }, `${['booktabs', 'siunitx'].filter((p) => !ctx[p]).join(' and ')} not loaded here: using it adds its \\usepackage to the preamble.`),
    );
    /** A rule gap: the rules before row `k` (or after the last, `k === rows.length`). */
    function ruleRow(k) {
      const s = t(), items = k < s.rows.length ? s.rows[k].before : s.after;
      const main = items.find((x) => RULES.includes(x)) ?? '';
      const choices = ['', '\\hline', ...(ctx.booktabs || items.some((x) => /rule/.test(x)) ? ['\\toprule', '\\midrule', '\\bottomrule'] : [])];
      const sel = h('select', { 'aria-label': `rule ${k === s.rows.length ? 'after the last row' : `before row ${k + 1}`}`, onchange: () => change(() => {
        const i = items.findIndex((x) => RULES.includes(x));
        if (i >= 0) { if (sel.value) items[i] = sel.value; else items.splice(i, 1); } else if (sel.value) items.unshift(sel.value);
      }) }, choices.map((c) => h('option', { value: c, selected: c === main }, c || '—')));
      const others = items.filter((x) => !RULES.includes(x)).map((x) => h('span', { class: 'sd-chip', title: x }, x.length > 24 ? x.slice(0, 22) + '…' : x));
      const n = s.cols.length * 2 + 1;
      return h('tr', { class: 'sd-rule' }, h('td', { class: 'sd-rh' }, sel), h('td', { colspan: n, class: main ? 'sd-ruled' : '', style: main === '\\toprule' || main === '\\bottomrule' ? '--w:2px' : '' }, others));
    }
    function draw() {
      const s = t();
      const vbar = (k) => h('th', { class: 'sd-v' + (s.seps[k]?.includes('|') ? ' on' : ''), title: 'vertical rule (click)', onclick: () => change((s) => { s.seps[k] = s.seps[k].includes('|') ? s.seps[k].replace(/\|/g, '') : s.seps[k] + '|'; }) });
      const head = h('tr', {}, h('th', { class: 'sd-rh' }), s.cols.flatMap((c, k) => {
        const known = ALIGNS.some(([a]) => a === c.align);
        const sel = h('select', { 'aria-label': `column ${k + 1} alignment`, onchange: () => change(() => { c.align = sel.value; if (c.align === 'p' && !c.arg) c.arg = '3cm'; if (!['p', 'S', 'm', 'b'].includes(c.align)) c.arg = ''; }) },
          [...ALIGNS, ...(known ? [] : [[c.align, c.align]])].map(([a, n]) => h('option', { value: a, selected: a === c.align }, n)));
        const arg = ['p', 'm', 'b', 'S'].includes(c.align) ? h('input', { value: c.arg, 'aria-label': `column ${k + 1} argument`, placeholder: c.align === 'S' ? 'options' : 'width', oninput: (e) => { c.arg = e.target.value; f.refresh(); } }) : null;
        return [vbar(k), h('th', { onclick: () => { cur[1] = k; } }, sel, arg)];
      }), vbar(s.cols.length));
      const rows = [];
      s.rows.forEach((r, k) => {
        rows.push(ruleRow(k));
        let col = 0;
        const short = s.cols.length - width(r);
        const tds = r.cells.flatMap((c, i) => {
          const span = Math.max(1, c.span), at = col, align = (c.spec || s.cols[at]?.align || 'l').replace(/[^lcrS]/g, '')[0] ?? 'l';
          col += span;
          const input = h('input', { value: c.text, 'aria-label': `row ${k + 1} column ${at + 1}`, oninput: (e) => { c.text = e.target.value; f.refresh(); }, onfocus: () => { cur = [k, at]; mark(); } });
          const spec = span > 1 ? h('input', { value: c.spec, title: '\\multicolumn spec', 'aria-label': 'merged cell spec', style: 'width:3rem;font-size:.8em', oninput: (e) => { c.spec = e.target.value; f.refresh(); } }) : null;
          return [h('td', { colspan: span * 2 + (i === r.cells.length - 1 && short <= 0 ? 1 : 0), 'data-r': k, 'data-c': at, 'data-align': align }, spec ? h('div', { style: 'display:flex' }, input, spec) : input)];
        });
        rows.push(h('tr', {}, h('td', { class: 'sd-rh' }, String(k + 1)), tds, short > 0 ? h('td', { colspan: short * 2 + 1, class: 'sd-muted' }) : null));
      });
      rows.push(ruleRow(s.rows.length));
      grid.replaceChildren(h('thead', {}, head), h('tbody', {}, rows));
      mark();
    }
    function mark() {
      for (const td of grid.querySelectorAll('td[data-r]')) td.classList.toggle('sd-cur', +td.dataset.r === cur[0] && +td.dataset.c === cur[1]);
    }
    draw();
    f.refresh();
    return f;
  }

  // ------------------------------------------------------------ image

  const fmt = (x) => String(Math.round(x * 100) / 100);
  function image(view, host) {
    const s = () => view.state;
    const ctx = view.context ?? {};
    const main = h('div');
    const side = h('div');
    const f = frame(view, host, `Edit image ${s().file}`, { main, side, redraw: () => sync(), foot: 'Crop, rotation and size are written as \\includegraphics options (graphicx).' });
    let nat = s().natural; // [w, h] bp
    const stage = h('div', { class: 'sd-stage' });
    const crop = h('div', { class: 'sd-crop' });
    const handles = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'].map((d) => h('div', { class: 'sd-h', 'data-d': d, style: `cursor:${d}-resize` }));
    crop.append(...handles);
    const prevBox = h('div', { class: 'sd-prev-box' });
    main.append(stage, h('h4', {}, 'Result'), prevBox);
    let picture = null;
    const num = (key, i, label) => h('label', {}, label, h('input', { type: 'number', step: '1', class: 'sd-num', 'data-k': key, 'data-i': i, oninput: (e) => { const st = s(); st.trim ??= [0, 0, 0, 0]; st.trim[i] = +e.target.value || 0; st.clip = true; sync(); } }));
    const angle = h('input', { type: 'range', min: -180, max: 180, step: 1, 'aria-label': 'angle', oninput: (e) => setAngle(+e.target.value) });
    const angleNum = h('input', { type: 'number', step: 'any', class: 'sd-num', 'aria-label': 'angle in degrees', oninput: (e) => setAngle(+e.target.value) });
    const setAngle = (a) => { s().angle = a ? ((a % 360) + 540) % 360 - 180 : null; sync(); };
    const lenInput = (key, presets) => {
      const input = h('input', { class: 'sd-len', list: `sd-${key}`, 'aria-label': key, placeholder: 'natural', oninput: (e) => { s()[key] = e.target.value || null; sync(); } });
      return [input, h('datalist', { id: `sd-${key}` }, presets.map((p) => h('option', { value: p })))];
    };
    const [wIn, wList] = lenInput('width', ['\\linewidth', '0.8\\linewidth', '0.5\\linewidth', '\\textwidth', '5cm']);
    const [hIn, hList] = lenInput('height', ['\\textheight', '0.3\\textheight', '4cm']);
    const scale = h('input', { type: 'number', step: '0.05', min: '0', class: 'sd-num', 'aria-label': 'scale', placeholder: '1', oninput: (e) => { s().scale = e.target.value ? +e.target.value : null; sync(); } });
    const keep = h('input', { type: 'checkbox', onchange: (e) => { s().keepaspectratio = e.target.checked; sync(); } });
    const clip = h('input', { type: 'checkbox', onchange: (e) => { s().clip = e.target.checked; sync(); } });
    const order = h('select', { 'aria-label': 'what the size applies to', onchange: (e) => { s().size_first = e.target.value === 'upright'; sync(); } },
      h('option', { value: 'rotated' }, 'the rotated figure (angle, then width)'), h('option', { value: 'upright' }, 'the upright figure (width, then angle)'));
    const unread = h('p', { class: 'sd-note' });
    side.append(
      h('h4', {}, 'Crop (trim, bp)'),
      h('div', { class: 'sd-row' }, num('trim', 0, 'left'), num('trim', 2, 'right')),
      h('div', { class: 'sd-row' }, num('trim', 3, 'top'), num('trim', 1, 'bottom')),
      h('div', { class: 'sd-row' }, h('label', {}, clip, 'clip'), h('button', { type: 'button', onclick: () => { s().trim = null; s().clip = false; sync(); } }, 'Reset crop')),
      h('h4', {}, 'Rotate (angle, counterclockwise)'),
      h('div', { class: 'sd-row' }, h('button', { type: 'button', title: 'Rotate 90° counterclockwise', onclick: () => setAngle((s().angle ?? 0) + 90) }, '⟲ 90°'), h('button', { type: 'button', title: 'Rotate 90° clockwise', onclick: () => setAngle((s().angle ?? 0) - 90) }, '⟳ 90°'), angleNum),
      h('div', { class: 'sd-row' }, angle),
      h('h4', {}, 'Size'),
      h('div', { class: 'sd-row' }, h('label', {}, 'width', wIn), wList),
      h('div', { class: 'sd-row' }, h('label', {}, 'height', hIn), hList),
      h('div', { class: 'sd-row' }, h('label', {}, 'scale', scale), h('label', { title: 'with both width and height: fit inside, keep the proportions' }, keep, 'keep aspect ratio')),
      h('div', { class: 'sd-row' }, h('label', {}, 'size applies to', order)),
      h('p', { class: 'sd-muted' }, 'graphicx reads the keys in order: angle=90,width=3cm makes the rotated figure 3cm wide; width=3cm,angle=90 makes it 3cm high.'),
      unread,
    );
    // the figure: a raster as it is, a PDF's first page as the host renders it
    (async () => {
      const got = ctx.path ? await host.image(ctx.path).catch(() => null) : null;
      if (got?.url && got.kind !== null) {
        picture = h('img', { src: got.url, alt: s().file, draggable: 'false' });
        picture.addEventListener('load', () => { nat ??= [picture.naturalWidth, picture.naturalHeight]; sync(); }, { once: true });
      } else {
        const [w, hh] = nat ?? [400, 300];
        const k = Math.min(480 / w, 320 / hh);
        picture = h('div', { class: 'sd-ph', style: `width:${w * k}px;height:${hh * k}px` }, h('div', {}, h('b', {}, s().file), h('br'), got?.kind === 'pdf' ? 'PDF: no renderer here, the crop works on its box' : 'no preview', h('br'), nat ? `${fmt(nat[0])} × ${fmt(nat[1])} bp` : ''));
      }
      stage.replaceChildren(picture, crop);
      sync();
    })();
    /** Display pixels per bp. */
    const k = () => (picture?.getBoundingClientRect().width || 1) / (nat?.[0] || 1);
    function sync() {
      const st = s(), [l, b, r, tp] = st.trim ?? [0, 0, 0, 0];
      for (const i of side.querySelectorAll('input[data-k=trim]')) if (document.activeElement !== i) i.value = fmt((st.trim ?? [0, 0, 0, 0])[+i.dataset.i]);
      if (document.activeElement !== angleNum) angleNum.value = fmt(st.angle ?? 0);
      angle.value = String(st.angle ?? 0);
      if (document.activeElement !== wIn) wIn.value = st.width ?? '';
      if (document.activeElement !== hIn) hIn.value = st.height ?? '';
      if (document.activeElement !== scale) scale.value = st.scale ?? '';
      keep.checked = !!st.keepaspectratio;
      clip.checked = !!st.clip;
      order.value = st.size_first ? 'upright' : 'rotated';
      unread.textContent = st.unread?.length ? `Kept as written, not shown here: ${st.unread.join(', ')}.` : '';
      if (picture && nat) {
        const q = k(), W = nat[0] * q, H = nat[1] * q;
        Object.assign(crop.style, { left: `${l * q}px`, top: `${tp * q}px`, width: `${Math.max(0, W - (l + r) * q)}px`, height: `${Math.max(0, H - (b + tp) * q)}px` });
        const pos = { nw: [0, 0], n: [.5, 0], ne: [1, 0], e: [1, .5], se: [1, 1], s: [.5, 1], sw: [0, 1], w: [0, .5] };
        for (const hd of handles) { const [x, y] = pos[hd.dataset.d]; Object.assign(hd.style, { left: `${x * 100}%`, top: `${y * 100}%` }); }
        preview(l, b, r, tp);
      }
      f.refresh();
    }
    function preview(l, b, r, tp) {
      const st = s(), cw = nat[0] - l - r, ch = nat[1] - b - tp;
      if (cw <= 0 || ch <= 0) return prevBox.replaceChildren(h('span', { class: 'sd-note' }, 'The crop leaves nothing.'));
      const a = (st.angle ?? 0) * Math.PI / 180, rw = Math.abs(cw * Math.cos(a)) + Math.abs(ch * Math.sin(a)), rh = Math.abs(cw * Math.sin(a)) + Math.abs(ch * Math.cos(a));
      const q = Math.min(300 / rw, 180 / rh);
      const inner = picture.tagName === 'IMG' ? h('img', { src: picture.src, alt: '' }) : h('div', { class: 'sd-ph' }, st.file);
      Object.assign(inner.style, { width: `${nat[0] * q}px`, height: `${nat[1] * q}px`, left: `${-l * q}px`, top: `${-tp * q}px` });
      const box = h('div', { class: 'sd-prev-crop', style: `width:${cw * q}px;height:${ch * q}px;transform:rotate(${-(st.angle ?? 0)}deg)` }, inner);
      if (!st.clip && st.trim) box.style.overflow = 'visible';
      const holder = h('div', { style: `width:${rw * q}px;height:${rh * q}px;display:grid;place-items:center` }, box);
      prevBox.replaceChildren(holder, h('div', { class: 'sd-muted', style: 'margin-top:.3rem' }, `${fmt(rw)} × ${fmt(rh)} bp before the size keys`));
    }
    // dragging: the frame moves, a handle moves its edges; the state's trim is in bp
    let drag = null;
    stage.addEventListener('pointerdown', (e) => {
      if (!nat || !picture) return;
      const d = e.target.closest('.sd-h')?.dataset.d ?? (e.target === crop ? 'move' : null);
      if (!d) return;
      e.preventDefault();
      stage.setPointerCapture(e.pointerId);
      drag = { d, x: e.clientX, y: e.clientY, t: [...(s().trim ?? [0, 0, 0, 0])] };
    });
    stage.addEventListener('pointermove', (e) => {
      if (!drag) return;
      const q = k(), dx = (e.clientX - drag.x) / q, dy = (e.clientY - drag.y) / q, [W, H] = nat;
      let [l, b, r, tp] = drag.t;
      const d = drag.d;
      if (d === 'move') { const mx = Math.max(-l, Math.min(r, dx)), my = Math.max(-tp, Math.min(b, dy)); l += mx; r -= mx; tp += my; b -= my; }
      if (d.includes('w')) l = Math.min(Math.max(0, l + dx), W - r - 1);
      if (d.includes('e')) r = Math.min(Math.max(0, r - dx), W - l - 1);
      if (d.includes('n')) tp = Math.min(Math.max(0, tp + dy), H - b - 1);
      if (d.includes('s')) b = Math.min(Math.max(0, b - dy), H - tp - 1);
      s().trim = [l, b, r, tp].map((x) => Math.round(x));
      s().clip = true;
      sync();
    });
    const end = () => { drag = null; };
    stage.addEventListener('pointerup', end);
    stage.addEventListener('pointercancel', end);
    sync();
    return f;
  }

  self.satexDialogs = {
    open(view, host) {
      if (view?.kind === 'table') return table(view, host);
      if (view?.kind === 'image') return image(view, host);
      return null;
    },
  };
})();
