// The type plugins' views (`satex/view`, src/lsp/plugins): one renderer for the playground and
// the VS Code webview. A view is `{plugin, title, sections: [{kind, …}]}`; each kind has a
// renderer in `RENDER`, and edits go back through `ctx.apply(edits)` as LSP TextEdits.
(() => {
  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  const el = (tag, cls, html) => Object.assign(document.createElement(tag), cls ? { className: cls } : {}, html != null ? { innerHTML: html } : {});
  // `${name}` the value; `${?name}…${/name}` kept only when the value is not empty (an optional field)
  let optsId = 0;
  const fill = (text, vars) => text.replace(/\$\{\?(\w+)\}([\s\S]*?)\$\{\/\1\}/g, (m, k, body) => String(vars[k] ?? '').trim() ? body : '').replace(/\$\{(\w+)\}/g, (m, k) => vars[k] ?? m);
  // a WorkspaceEdit `{changes: {uri: [TextEdit]}}` with its templates filled, and several as one
  const filled = (edit, vars) => ({ changes: Object.fromEntries(Object.entries(edit?.changes ?? {}).map(([u, es]) => [u, es.map((e) => ({ range: e.range, newText: fill(e.newText, vars) }))])) });
  // a form whose edit depends on a choice (`by`: the field, `edits`: one per choice)
  const chosen = (f, vals) => f.by ? f.edits?.[vals[f.by]] : f.edit;
  const merge = (edits) => ({ changes: edits.reduce((all, e) => { for (const [u, es] of Object.entries(e.changes)) (all[u] ??= []).push(...es); return all; }, {}) });
  // braces that close in order, as TeX reads a value (a `\{` is no group)
  /** The footer every view shares (dialogs.js `footer`): `status`, the undo tools, Reset, Cancel and Apply. Ctrl+Enter
   * and the undo keys go to the root before a focused child; closing with unwritten changes asks first (`ctx.guard`). */
  const footer = (root, ctx, { status, apply, label = 'Apply', H, tools = false }) => {
    const D = self.satexDialogs;
    ctx.guard?.(() => !!H?.dirty);
    const f = D.footer({ note: status, tag: 'div', cls: 'tv-actions tv-foot', extra: tools ? H?.controls?.() ?? [] : [], reset: H?.reset && (() => H.reset()), cancel: ctx.close, apply, label });
    D.keys(root, H, () => f.apply.click());
    return f;
  };
  const balanced = (text) => { let d = 0; for (const m of text.replace(/\\./g, '').matchAll(/[{}]/g)) if ((d += m[0] === '{' ? 1 : -1) < 0) return false; return d === 0; };

  // colors: rgb in 0..1 to each model's spec, as xcolor writes them (xcolor § 6.3)
  const clamp = (v) => Math.min(1, Math.max(0, v));
  const num = (v) => { const s = clamp(v).toFixed(4).replace(/0+$/, '').replace(/\.$/, ''); return s || '0'; };
  const byte = (v) => Math.round(clamp(v) * 255);
  const hex = ([r, g, b]) => '#' + [r, g, b].map((v) => byte(v).toString(16).padStart(2, '0')).join('');
  const toHsb = ([r, g, b]) => {
    const max = Math.max(r, g, b), d = max - Math.min(r, g, b);
    const h = d === 0 ? 0 : max === r ? (((g - b) / d) % 6 + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
    return [h / 6, max === 0 ? 0 : d / max, max];
  };
  const fromHsb = ([h, s, v]) => {
    const i = Math.floor(h * 6) % 6, f = h * 6 - Math.floor(h * 6), p = v * (1 - s), q = v * (1 - s * f), t = v * (1 - s * (1 - f));
    return [[v, t, p], [q, v, p], [p, v, t], [p, q, v], [t, p, v], [v, p, q]][i];
  };
  const SPEC = {
    HTML: (c) => c.map((v) => byte(v).toString(16).padStart(2, '0')).join('').toUpperCase(),
    rgb: (c) => c.map(num).join(','),
    RGB: (c) => c.map(byte).join(','),
    cmyk: ([r, g, b]) => { const k = 1 - Math.max(r, g, b); return (k >= 1 ? [0, 0, 0, 1] : [(1 - r - k) / (1 - k), (1 - g - k) / (1 - k), (1 - b - k) / (1 - k), k]).map(num).join(','); },
    gray: ([r, g, b]) => num(0.3 * r + 0.59 * g + 0.11 * b),
    hsb: (c) => toHsb(c).map(num).join(','),
  };
  // OKLCH of an sRGB color (Ottosson's OKLab): [lightness, chroma, hue in degrees]
  const oklch = (c) => {
    const [r, g, b] = c.map((v) => v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
    const [l, m, s] = [0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b, 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b, 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b].map(Math.cbrt);
    const [L, A, B] = [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
    return [L, Math.hypot(A, B), (Math.atan2(B, A) * 180 / Math.PI + 360) % 360];
  };
  /** The defined color nearest to `c` as the eye sees it (distance in OKLab), or null. */
  const closest = (c, defined) => {
    const lab = (x) => { const [l, ch, h] = oklch(x); return [l, ch * Math.cos(h * Math.PI / 180), ch * Math.sin(h * Math.PI / 180)]; };
    const [l, a, b] = lab(c);
    let best = null, gap = Infinity;
    for (const d of defined ?? []) {
      if (!d.rgb) continue;
      const [l2, a2, b2] = lab(d.rgb), g = Math.hypot(l - l2, a - a2, b - b2);
      if (g < gap) { gap = g; best = d; }
    }
    return best;
  };
  /** The defined colors as groups by who defined them (the document first, then xcolor, then the rest by name),
   * each sorted by perception: hue families of 30° by lightness, greys (low chroma) last by lightness. */
  const colorBand = (defined) => {
    const key = (d) => { const [l, c, h] = oklch(d.rgb); return c < 0.03 ? [1, 0, l] : [0, Math.floor(((h + 15) % 360) / 30), l]; };
    const cmp = (a, b) => { const x = key(a), y = key(b); return x[0] - y[0] || x[1] - y[1] || x[2] - y[2]; };
    const rank = (g) => g === 'document' ? 0 : g === 'xcolor' ? 1 : 2;
    const groups = new Map();
    for (const d of defined) (groups.get(d.group ?? '') ?? groups.set(d.group ?? '', []).get(d.group ?? '')).push(d);
    return [...groups].map(([name, colors]) => ({ name, colors: colors.sort(cmp) })).sort((a, b) => rank(a.name) - rank(b.name) || a.name.localeCompare(b.name));
  };
  // a spec typed in a model, back to rgb
  const PARSE = {
    HTML: (s) => /^#?[0-9a-f]{6}$/i.test(s.trim()) ? [0, 2, 4].map((i) => parseInt(s.trim().replace('#', '').slice(i, i + 2), 16) / 255) : null,
    rgb: (s) => { const v = s.split(',').map(Number); return v.length === 3 && v.every((x) => x >= 0 && x <= 1) ? v : null; },
    RGB: (s) => { const v = s.split(',').map(Number); return v.length === 3 && v.every((x) => x >= 0 && x <= 255) ? v.map((x) => x / 255) : null; },
    cmyk: (s) => { const v = s.split(',').map(Number); return v.length === 4 && v.every((x) => x >= 0 && x <= 1) ? v.slice(0, 3).map((x) => (1 - x) * (1 - v[3])) : null; },
    gray: (s) => { const v = Number(s); return s.trim() && v >= 0 && v <= 1 ? [v, v, v] : null; },
    hsb: (s) => { const v = s.split(',').map(Number); return v.length === 3 && v.every((x) => x >= 0 && x <= 1) ? fromHsb(v) : null; },
  };
  // an xcolor expression `a!p!b!q…` over the colors defined (xcolor § 2.3.2), else null
  const mix = (expr, defined) => {
    const named = (n) => defined.find((d) => d.name === n.trim())?.rgb ?? null;
    const parts = expr.split('!');
    let acc = named(parts.shift());
    while (acc && parts.length) {
      const p = Number(parts.shift());
      if (!(p >= 0 && p <= 100)) return null;
      const other = parts.length ? named(parts.shift()) : [1, 1, 1];
      if (!other) return null;
      acc = acc.map((v, i) => p / 100 * v + (1 - p / 100) * other[i]);
    }
    return acc;
  };

  /** Chips a group of defined colors shows before its `+N`. */
  const BAND = 10;
  const RENDER = {
    text: (s, view, ctx) => el('div', 'tv-text', ctx.markdown(s.markdown)),
    table: (s) => el('table', 'tv-table', `<thead><tr>${s.columns.map((c) => `<th>${esc(c)}</th>`).join('')}</tr></thead><tbody>${s.rows.map((r) => `<tr>${r.map((c) => `<td>${esc(c)}</td>`).join('')}</tr>`).join('')}</tbody>`),
    actions: (s, view, ctx) => {
      const box = el('div', 'tv-actions');
      for (const a of s.items) {
        const b = el('button', 'tv-button', esc(a.title));
        b.type = 'button';
        b.onclick = () => ctx.apply(a.edits);
        box.append(b);
      }
      return box;
    },
    // a filterable list: each choice applies its edits
    choices: (s, view, ctx) => {
      const box = el('div', 'tv-choices');
      const input = Object.assign(el('input', 'tv-input'), { placeholder: s.placeholder ?? 'filter', spellcheck: false, ariaLabel: s.placeholder ?? 'filter' });
      const list = el('div', 'tv-list');
      const draw = () => {
        const q = input.value.trim().toLowerCase();
        list.replaceChildren(...s.items.filter((c) => c.title.toLowerCase().includes(q)).slice(0, 200).map((c) => {
          const b = el('button', 'tv-choice', `<b>${esc(c.title)}</b><span>${esc(c.detail ?? '')}</span>`);
          b.type = 'button';
          b.onclick = () => ctx.apply(c.edits);
          return b;
        }));
      };
      input.oninput = draw;
      input.onkeydown = (e) => { if (e.key === 'Enter') list.querySelector('button')?.click(); };
      draw();
      box.append(input, list);
      setTimeout(() => input.focus(), 0);
      return box;
    },
    // a manager: searchable, sortable records; each opens to its fields (edited through edit
    // templates, `${value}` the new text), a table and actions; `add` is a form of templates
    records: (s, view, ctx) => {
      const box = el('div', 'tv-records');
      const head = el('div', 'tv-row');
      const input = Object.assign(el('input', 'tv-input'), { placeholder: s.placeholder ?? 'search', spellcheck: false, ariaLabel: s.placeholder ?? 'search', value: s.query ?? '' });
      const sort = el('select', 'tv-input', (s.sorts ?? []).map((k) => `<option>${esc(k)}</option>`).join(''));
      sort.ariaLabel = 'sort by';
      head.append(input);
      if (s.sorts?.length > 1) head.append(sort);
      const list = el('div', 'tv-list');
      // exactly one record is open for editing: the focused one, else the first
      let open = s.items.find((r) => r.title === s.focus) ?? s.items[0] ?? null;
      // what the manager shows again after an Apply: the search and the open record
      const kept = () => ({ q: input.value, open: open?.title });
      const draw = () => {
        const q = input.value.trim().toLowerCase(), by = sort.value;
        const hit = (r) => !q || q.split(/\s+/).every((w) => (r.search ?? r.title).toLowerCase().includes(w));
        const key = (r) => String(r.sort?.[by] ?? r.title).toLowerCase();
        const rows = s.items.filter(hit).sort((a, b) => key(a).localeCompare(key(b), undefined, { numeric: true }));
        list.replaceChildren(...rows.slice(0, 300).map((r) => {
          const item = el('div', 'tv-record' + (open === r ? ' on' : ''));
          const b = el('button', 'tv-choice tv-line', `<b>${esc(r.title)}</b><span>${esc(r.detail ?? '')}</span>${r.badge ? `<em class="tv-badge">${esc(r.badge)}</em>` : ''}`);
          b.type = 'button';
          b.title = [r.title, r.detail].filter(Boolean).join(': ');
          b.ariaExpanded = String(open === r);
          b.onclick = () => { if (open !== r) { open = r; draw(); } };
          item.append(b);
          if (open === r) item.append(record(r));
          return item;
        }));
        if (!rows.length) list.append(el('div', 'tv-text', `<p>${esc(s.empty ?? 'nothing found')}</p>`));
      };
      const record = (r) => {
        const body = el('div', 'tv-body');
        // an emptied field with a `remove` edit goes; the others write their value
        if (r.fields?.length) body.append(form(r.fields, r.save ?? 'Save', (vals, changed) => merge(r.fields.filter((f) => changed.has(f.name)).map((f) => !vals[f.name].trim() && f.remove ? f.remove : filled(f.edit, { value: vals[f.name] })))));
        // a new field of the record: a name the run knows (or any) and its value
        if (r.more) body.append(form([{ name: 'name', label: 'new field', value: '', placeholder: 'field name', options: r.more.names, check: ['required', 'key', 'unique'], taken: (r.fields ?? []).map((f) => f.name) }, { name: 'value', value: '', placeholder: 'value', check: ['required', r.more.check ?? 'tex'] }], r.more.title, (vals) => filled(r.more.edit, vals)));
        for (const f of r.forms ?? []) body.append(form(f.fields, f.title, (vals) => filled(chosen(f, vals), vals), f.note));
        if (r.why) body.append(el('div', 'tv-text', `<p>${esc(r.why)}</p>`));
        // a place opens in the editor: the manager steps aside
        if (r.uses) body.append(Object.assign(el('div', 'tv-text tv-uses', ctx.markdown(r.uses)), { onclick: (e) => { if (e.target.closest('a.place')) ctx.close?.(); } }));
        if (r.table?.rows.length) body.append(RENDER.table(r.table));
        if (r.actions?.length) {
          const acts = el('div', 'tv-actions');
          for (const a of r.actions) {
            const b = el('button', 'tv-button' + (a.danger ? ' tv-danger' : ''), esc(a.title));
            b.type = 'button';
            b.disabled = !!a.disabled;
            b.title = a.disabled ?? '';
            b.onclick = () => ctx.apply(a.edit, kept());
            acts.append(b);
          }
          body.append(acts);
          const why = r.actions.filter((a) => a.disabled || a.note).map((a) => `${a.title}: ${a.disabled ?? a.note}`);
          if (why.length) body.append(el('div', 'tv-text', why.map((w) => `<p>${esc(w)}</p>`).join('')));
        }
        return body;
      };
      // labelled inputs; `go(values, changed names)` makes the edit, applied when every value is balanced
      // `more` fields fold under "More fields"; `options` offer names to an input; `note` says what it writes
      const form = (fields, title, go, note) => {
        const f = el('form', 'tv-form');
        const fold = fields.some((d) => d.more) ? el('details', 'tv-more', `<summary>More fields (${fields.filter((d) => d.more).length})</summary>`) : null;
        const ins = fields.map((d) => {
          const lab = el('label', 'tv-field', `<span>${esc(d.label ?? d.name)}</span>`);
          const i = Object.assign(el(d.choices ? 'select' : d.long ? 'textarea' : 'input', 'tv-input'), { spellcheck: false, name: d.name });
          if (d.choices) i.innerHTML = d.choices.map((c) => `<option${c === d.value ? ' selected' : ''}>${esc(c)}</option>`).join('');
          else { i.value = d.value ?? ''; i.dataset.was = i.value; }
          if (d.placeholder) i.placeholder = d.placeholder;
          if (d.options?.length) {
            const list = el('datalist', null, d.options.map((o) => `<option value="${esc(o)}">`).join(''));
            list.id = `tv-opts-${++optsId}`;
            i.setAttribute('list', list.id);
            lab.append(list);
          }
          lab.append(i);
          (d.more ? fold : f).append(lab);
          // the server's checks of the field (dialogs.js `check`), the message under it
          return [d, i, d.choices ? null : self.satexDialogs.guard(i, d.check ?? ['balanced'], d.taken ?? [])];
        });
        if (fold) f.append(fold);
        if (note) f.append(el('div', 'tv-text tv-note', `<p>${esc(note)}</p>`));
        const b = el('button', 'tv-button tv-primary', esc(title));
        b.type = 'submit';
        f.append(el('div', 'tv-actions'));
        f.lastChild.append(b);
        // a field showing a problem holds the button; a submit shows every problem, `needed` too
        f.oninput = () => { b.disabled = ins.some(([, i]) => i.getAttribute('aria-invalid') === 'true'); };
        f.onsubmit = (e) => {
          e.preventDefault();
          const bad = ins.filter(([, , g]) => g && !g.ok(true));
          if (bad.length) { b.disabled = true; return bad[0][1].focus(); }
          const vals = Object.fromEntries(ins.map(([d, i]) => [d.name, i.value]));
          const changed = new Set(ins.filter(([d, i]) => i.value !== (d.value ?? '')).map(([d]) => d.name));
          const out = changed.size || !fields.some((d) => 'value' in d) ? go(vals, changed) : null;
          if (out?.insert != null) ctx.insert(out.insert); else if (out) ctx.apply(out, kept());
        };
        return f;
      };
      input.oninput = draw;
      sort.onchange = draw;
      draw();
      // closing with a field typed in and not written asks first
      ctx.guard?.(() => [...box.querySelectorAll('[data-was]')].some((i) => i.value !== i.dataset.was));
      box.append(head, list);
      // a kind that `insert`s writes at the editor's cursor: only where the page has one
      const kinds = s.add ? (s.add.kinds ?? [s.add]).filter((k) => !k.insert || ctx.insert) : [];
      if (kinds.length) {
        const more = el('details', 'tv-add', `<summary>${esc(s.add.title)}</summary>`);
        const pick = el('select', 'tv-input', kinds.map((k) => `<option>${esc(k.name ?? s.add.title)}</option>`).join(''));
        pick.ariaLabel = 'kind';
        const slot = el('div');
        const show = () => { const k = kinds[pick.selectedIndex]; slot.replaceChildren(form(k.fields, s.add.title, (vals) => k.insert ? { insert: fill(k.insert, vals) } : filled(chosen(k, vals), vals), k.note)); };
        pick.onchange = show;
        if (kinds.length > 1) more.append(pick);
        more.append(slot);
        show();
        box.append(more);
      }
      setTimeout(() => input.focus(), 0);
      return box;
    },
    converter: (s, view, ctx) => {
      const box = el('div', 'tv-converter');
      const per = Object.fromEntries(s.units.map((u) => [u.name, u.sp]));
      const show = (sp, u) => { const v = sp / per[u]; return Number.isFinite(v) ? String(+v.toFixed(5)) : '–'; };
      const head = el('div', 'tv-row');
      const input = Object.assign(el('input', 'tv-input'), { type: 'number', step: 'any', value: show(s.lo, s.unit), ariaLabel: 'length' });
      const unit = el('select', 'tv-input', s.units.map((u) => `<option${u.name === s.unit ? ' selected' : ''}>${esc(u.name)}</option>`).join(''));
      unit.ariaLabel = 'unit';
      head.append(input, unit);
      const grid = el('div', 'tv-units');
      const sp = () => Math.round(Number(input.value) * per[unit.value]);
      const draw = () => {
        grid.innerHTML = '';
        const range = s.lo !== s.hi && Number(input.value) === Number(show(s.lo, s.unit));
        for (const u of s.units) {
          // the value written: rounded to `decimals`; exact when TeX reads it back as the same sp
          const v = +(sp() / per[u.name]).toFixed(s.decimals ?? 2), exact = !range && Math.round(v * per[u.name]) === sp();
          const cell = el('button', 'tv-unit' + (exact ? ' exact' : ''), `<b>${range ? `${show(s.lo, u.name)} … ${show(s.hi, u.name)}` : show(sp(), u.name)}</b><span>${esc(u.name)}${exact ? ' · exact' : ''}</span>`);
          cell.type = 'button';
          cell.disabled = !s.range || range || u.name === 'em' || u.name === 'ex';
          cell.title = cell.disabled ? u.name : `write as ${v}${u.name}${exact ? ' (exact)' : ' (rounded)'}`;
          cell.onclick = () => ctx.apply([{ range: s.range, newText: `${v}${u.name}` }]);
          grid.append(cell);
        }
      };
      input.oninput = draw;
      unit.onchange = () => { input.value = show(sp(), unit.value); draw(); };
      draw();
      box.append(head, grid);
      return box;
    },
    color: (s, view, ctx) => {
      const box = el('div', 'tv-color');
      const start = s.rgb ?? [0.5, 0.5, 0.5];
      // `name`: the closest color a name stands for here, written as that name
      const models = s.defined?.length && s.pick ? [...s.models, 'name'] : s.models;
      const specOf = (m, c) => m === 'name' ? closest(c, s.defined)?.name ?? '' : SPEC[m](c);
      const parse = (m, text) => m === 'name' ? s.defined.find((d) => d.name === text.trim())?.rgb ?? null : PARSE[m]?.(text) ?? null;
      let hsb = toHsb(start), model = s.models[0];
      const rgb = () => fromHsb(hsb);
      // the saturation/brightness square and the hue strip
      const pick = el('div', 'tv-pick');
      const square = el('div', 'tv-square'), dot = el('i', 'tv-dot'), hue = el('div', 'tv-hue'), knob = el('i', 'tv-knob');
      square.append(dot);
      hue.append(knob);
      const drag = (node, f) => node.onpointerdown = (e) => {
        node.setPointerCapture(e.pointerId);
        const move = (ev) => { const r = node.getBoundingClientRect(); f(clamp((ev.clientX - r.left) / r.width), clamp((ev.clientY - r.top) / r.height)); draw(); };
        move(e);
        node.onpointermove = move;
        node.onpointerup = () => { node.onpointermove = null; };
      };
      // the shared undo model (dialogs.js) when the page has it: one step per gesture
      const hist = self.satexDialogs?.history?.(() => hsb, (v) => { hsb = v; draw(); });
      const settle = () => hist?.settle();
      for (const n of [square, hue]) n.addEventListener('pointerup', settle);
      drag(square, (x, y) => { hsb = [hsb[0], x, 1 - y]; });
      drag(hue, (x) => { hsb = [Math.min(x, 0.9999), hsb[1], hsb[2]]; });
      pick.append(square, hue);
      // was and is, then the spec in each model
      const side = el('div', 'tv-side');
      const swatch = el('div', 'tv-swatches', `<span class="tv-was" title="${esc(s.name)}"></span><span class="tv-is"></span>`);
      swatch.querySelector('.tv-was').style.background = s.rgb ? hex(s.rgb) : 'repeating-linear-gradient(45deg, #8884 0 4px, transparent 4px 8px)';
      const tabs = el('div', 'tv-tabs');
      for (const m of models) {
        const t = el('button', 'tv-tab', esc(m));
        t.type = 'button';
        t.onclick = () => { model = m; draw(); };
        tabs.append(t);
      }
      const spec = Object.assign(el('input', 'tv-input tv-spec'), { spellcheck: false, ariaLabel: 'spec' });
      const HOW = { HTML: 'six hex digits, as 1A2B3C', rgb: 'three numbers 0 to 1, as 0.2,0.4,0.6', RGB: 'three numbers 0 to 255', cmyk: 'four numbers 0 to 1', gray: 'one number 0 to 1', hsb: 'three numbers 0 to 1', name: 'a color defined here' };
      spec.oninput = () => {
        const ok = !!parse(model, spec.value);
        spec.classList.toggle('tv-bad', !ok);
        spec.title = ok ? '' : `not a ${model} spec: ${HOW[model] ?? 'unknown model'}`;
      };
      spec.onchange = () => { const c = parse(model, spec.value); if (c) { hsb = toHsb(c); draw(); settle(); } };
      // what does not read goes back to the color shown
      spec.onblur = () => { if (!parse(model, spec.value)) { spec.value = specOf(model, rgb()); spec.classList.remove('tv-bad'); spec.title = ''; } };
      // a color from anywhere on the screen (EyeDropper), else the system's picker
      const pickers = el('div', 'tv-pickers');
      const native = Object.assign(el('input', 'tv-native'), { type: 'color', title: "the system's color picker" });
      native.oninput = () => { const c = PARSE.HTML(native.value); if (c) { hsb = toHsb(c); draw(); } };
      native.onchange = settle;
      pickers.append(native);
      if ('EyeDropper' in self) {
        const eye = el('button', 'tv-button', 'Pick from screen');
        eye.type = 'button';
        eye.onclick = async () => { try { const { sRGBHex } = await new self.EyeDropper().open(); const c = PARSE.HTML(sRGBHex); if (c) { hsb = toHsb(c); draw(); settle(); } } catch {} };
        pickers.append(eye);
      }
      const all = el('div', 'tv-specs');
      side.append(swatch, tabs, spec, pickers, all);
      // an xcolor expression, mixed from the colors defined here
      let expr = null;
      if (s.expressions) {
        const row = el('label', 'tv-expr', '<span>mix</span>');
        const input = Object.assign(el('input', 'tv-input'), { placeholder: 'red!40!blue', spellcheck: false, value: s.name.includes('!') ? s.name : '' });
        input.oninput = () => { const c = mix(input.value, s.defined); expr = c ? input.value.trim() : null; input.classList.toggle('tv-bad', !c && !!input.value); if (c) { hsb = toHsb(c); draw(false); } };
        row.append(input);
        side.append(row);
      }
      box.append(pick, side);
      // the colors defined at the position: picking one writes its name
      // grouped by who defined them, alike colors side by side; a long group shows a few and `+N`
      if (s.defined.length) {
        const list = el('div', 'tv-defined');
        const chip = (d) => {
          const b = el('button', 'tv-chip', `<i style="background:${d.css}"></i>${esc(d.name)}`);
          b.type = 'button';
          b.title = s.pick ? `write ${d.name} here` : d.name;
          b.onclick = () => s.pick ? ctx.apply([{ range: s.pick, newText: d.name }]) : (hsb = toHsb(d.rgb), draw(), settle());
          return b;
        };
        for (const g of colorBand(s.defined)) {
          const row = el('div', 'tv-band', `<span class="tv-band-name">${esc(g.name === 'document' ? 'this document' : g.name)} <small>${g.colors.length}</small></span>`);
          const few = g.colors.length > BAND ? BAND - 1 : g.colors.length;
          row.append(...g.colors.slice(0, few).map(chip));
          if (few < g.colors.length) {
            const more = el('button', 'tv-chip tv-more', `+${g.colors.length - few}`);
            more.type = 'button';
            more.title = `show all ${g.colors.length} colors of ${g.name}`;
            more.ariaExpanded = 'false';
            more.onclick = () => more.replaceWith(...g.colors.slice(few).map(chip));
            row.append(more);
          }
          list.append(row);
        }
        box.append(el('h4', 'tv-h', 'Defined here'), list);
      }
      const foot = el('div', 'tv-actions');
      if (hist) { foot.append(...hist.controls(), el('span', 'tv-muted tv-hint', esc(self.satexDialogs.HINT.replace(', Ctrl+Enter applies', '')))); hist.keys(box); ctx.guard?.(() => !!s.apply && hist.dirty); }
      if (s.apply) {
        const write = el('button', 'tv-button tv-primary');
        write.type = 'button';
        write.onclick = () => {
          if (expr && s.pick) return ctx.apply([{ range: s.pick, newText: expr }]);
          // a color some name already has: that name, never its value spelled out
          const same = model === 'name' ? closest(rgb(), s.defined) : named();
          if (same && s.pick) return ctx.apply(same.name === s.name ? [] : [{ range: s.pick, newText: same.name }]);
          const vars = { model, spec: SPEC[model](rgb()), name: s.apply.name ?? s.name };
          ctx.apply(s.apply.edits.map((e) => ({ range: e.range, newText: fill(e.newText, vars) })));
        };
        foot.append(write);
      }
      if (foot.children.length) box.append(foot);
      /** The defined color the picked one equals, the color's own name first. */
      function named() {
        const h = hex(rgb());
        const all = (s.defined ?? []).filter((d) => d.rgb && hex(d.rgb) === h);
        return all.find((d) => d.name === s.name) ?? all[0] ?? null;
      }
      function draw(clearExpr = true) {
        if (clearExpr) expr = null;
        const c = rgb();
        square.style.background = `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, ${hex(fromHsb([hsb[0], 1, 1]))})`;
        dot.style.left = `${hsb[1] * 100}%`; dot.style.top = `${(1 - hsb[2]) * 100}%`;
        knob.style.left = `${hsb[0] * 100}%`;
        swatch.querySelector('.tv-is').style.background = hex(c);
        for (const t of tabs.children) t.classList.toggle('on', t.textContent === model);
        if (document.activeElement !== spec) spec.value = specOf(model, c);
        native.value = hex(c);
        // each model's value is its own action: a click writes the color in that model
        all.replaceChildren(...s.models.map((m) => {
          const b = el(s.apply ? 'button' : 'span', 'tv-spec-value' + (m === model ? ' on' : ''), `<b>${esc(m)}</b> ${esc(SPEC[m](c))}`);
          if (s.apply) { b.type = 'button'; b.title = `write as [${m}]{${SPEC[m](c)}}`; b.onclick = () => { model = m; draw(); foot.querySelector('.tv-primary')?.click(); }; }
          return b;
        }));
        const w = foot.querySelector('.tv-primary');
        const same = !expr && s.pick && (model === 'name' ? closest(c, s.defined) : named());
        if (w) w.textContent = expr ? `Write ${expr}` : same ? (same.name === s.name ? `Keep ${s.name}` : `Write ${same.name}`) : s.apply.edits.length > 1 ? `Define ${s.apply.name} as ${model} ${SPEC[model](c)}` : `Write [${model}]{${SPEC[model](c)}}`;
      }
      draw();
      return box;
    },
  };

  // drawings (tikz plugin): a picture's text as statements; nodes `\node[…] (n) at (x,y) {…};`,
  // or placed relative to another (`right of=a`, `right=of a`), arrows `\draw[->] (a.north) -- (b);`,
  // matrices of nodes and layer blocks are edited, any other statement is kept as code
  const NUM = String.raw`(-?\d*\.?\d+)\s*(?:cm)?`;
  const POINT = new RegExp(String.raw`^\s*${NUM}\s*,\s*${NUM}\s*$`);
  const END = String.raw`\(\s*([\w -]+?)\s*(?:\.\s*([A-Za-z][\w ]*?|-?\d*\.?\d+))?\s*\)`;
  const EDGE = new RegExp(String.raw`^\\draw\s*(?:\[((?:[^\[\]]|\[[^\]]*\])*)\])?\s*${END}\s*(--|to\b\s*(?:\[([^\]]*)\])?)\s*(?:node\s*(?:\[([^\]]*)\])?\s*\{([^{}]*)\}\s*)?${END}\s*;$`);
  const PATH = /^\\draw\s*(?:\[[^\]]*\])?\s*(\([^()]*\)(?:\s*--\s*\([^()]*\))+)\s*;$/;
  /** An option list split at its top-level commas. */
  const opts = (s = '') => {
    const out = [];
    let depth = 0, from = 0;
    for (let i = 0; i <= s.length; i++) {
      if (s[i] === '{') depth++;
      else if (s[i] === '}') depth--;
      else if (i === s.length || (s[i] === ',' && !depth)) { out.push(s.slice(from, i).trim()); from = i + 1; }
    }
    return out.filter(Boolean);
  };
  /** `key=value` as `[key, value]`, a value's outer braces dropped; a bare key has value ''. */
  const kv = (o) => { const i = o.indexOf('='); return i < 0 ? [o.trim(), ''] : [o.slice(0, i).trim(), o.slice(i + 1).trim().replace(/^\{([^]*)\}$/, '$1')]; };
  /** `list` with `key`'s entries replaced by `key=value`, or dropped for null. */
  const setOpt = (list, key, value) => {
    const i = list.findIndex((o) => kv(o)[0] === key), out = list.filter((o) => kv(o)[0] !== key);
    if (value != null) out.splice(i < 0 ? out.length : i, 0, value === '' ? key : `${key}=${/[,=]/.test(value) ? `{${value}}` : value}`);
    return out;
  };
  /** `->`, `Stealth-{Latex[open]}`: an arrow option's start and end tips, else null. */
  const tipsOf = (o) => {
    const cuts = [];
    for (let i = 0, d = 0; i < o.length; i++) {
      const c = o[i];
      if ('{['.includes(c)) d++;
      else if ('}]'.includes(c)) d--;
      else if (!d && c === '=') return null;
      else if (!d && c === '-') cuts.push(i);
    }
    return cuts.length === 1 ? [o.slice(0, cuts[0]).trim(), o.slice(cuts[0] + 1).trim()] : null;
  };
  const fmt = (v) => String(+(+v).toFixed(3));
  // TeX's units in cm (TeXbook p. 57); a bare number is pt, as pgf reads a length
  const UNITS = { cm: 1, mm: 0.1, in: 2.54, pt: 2.54 / 72.27, bp: 2.54 / 72 };
  const cm = (s) => { const m = s?.trim().match(/^(-?\d*\.?\d+)\s*(cm|mm|in|pt|bp)?$/); return m ? +m[1] * UNITS[m[2] ?? 'pt'] : null; };
  const SIDES = { right: [1, 0], left: [-1, 0], above: [0, 1], below: [0, -1] };
  /** The words of a direction: `[1, 1]` is `above right`. */
  const words = ([dx, dy]) => [dy > 0 ? 'above' : dy < 0 ? 'below' : '', dx > 0 ? 'right' : dx < 0 ? 'left' : ''].filter(Boolean).join(' ');
  /** `right of=a` (centers apart, tikz) or `above right=1cm of a` (borders apart, positioning). */
  function placement(o) {
    const m = o.match(/^((?:above|below)?\s*(?:left|right)?)\s*(?:of\s*=\s*(.+)|=\s*(?:(.+?)\s+)?of\s+(.+))$/);
    if (!m || !m[1].trim()) return null;
    const dir = m[1].trim().split(/\s+/).reduce((v, w) => [v[0] + SIDES[w][0], v[1] + SIDES[w][1]], [0, 0]);
    return m[2] != null ? { dir, of: m[2].trim(), centers: true } : { dir, of: m[4].trim(), dist: m[3] };
  }
  /** A placement's option text again. */
  const placeText = (p) => p.centers ? `${words(p.dir)} of=${p.of}` : `${words(p.dir)}=${p.dist ? `${p.dist} ` : ''}of ${p.of}`;
  /** A matrix of nodes' cells: `|[opts] (name)| text` split at `&` and `\\`, unnamed ones `m-r-c`. */
  function cellsOf(body, m) {
    const rows = [];
    let row = [], depth = 0, from = 0;
    const end = (i) => { row.push(body.slice(from, i).trim()); from = i; };
    for (let i = 0; i < body.length; i++) {
      const c = body[i];
      if (c === '{') depth++;
      else if (c === '}') depth--;
      else if (depth) { if (c === '\\') i++; }
      else if (c === '&') { end(i); from = i + 1; }
      else if (c === '\\' && body[i + 1] === '\\') { end(i); rows.push(row); row = []; from = ++i + 1; }
      else if (c === '\\') i++;
    }
    end(body.length);
    if (row.some(Boolean)) rows.push(row);
    const cells = [];
    for (const [r, texts] of rows.entries()) for (const [c, t] of texts.entries()) {
      if (!t) continue;
      const k = t.match(/^\|\s*(?:\[([^\]]*)\])?\s*(?:\(([\w -]+)\))?\s*\|\s*([^]*)$/);
      if (/\\node\b|\|/.test(k ? k[3] : t)) return null;
      cells.push({ type: 'node', cell: true, row: r, col: c, name: k?.[2]?.trim() ?? `${m}-${r + 1}-${c + 1}`, named: !!k?.[2], opts: opts(k?.[1]), label: k ? k[3] : t, at: true, x: 0, y: 0 });
    }
    return cells;
  }
  /** `\node`/`\coordinate`/`\matrix`: options, name, `at` and options in any order, then `{label};`. */
  function nodeOf(code) {
    const head = code.match(/^\\(node|coordinate|matrix)\b/);
    if (!head) return null;
    let i = head[0].length;
    const n = { coordinate: head[1] === 'coordinate', opts: [], name: null, at: null, label: '' };
    const group = (open, close) => {
      for (let j = i, depth = 0; j < code.length; j++) {
        if (code[j] === '\\') j++;
        else if (code[j] === open) depth++;
        else if (code[j] === close && --depth === 0) { const s = code.slice(i + 1, j); i = j + 1; return s; }
      }
      return null;
    };
    for (;;) {
      i += code.slice(i).match(/^\s*/)[0].length;
      if (code[i] === '[') { const o = group('[', ']'); if (o == null) return null; n.opts.push(...opts(o)); }
      else if (code[i] === '(' && n.name == null) n.name = group('(', ')');
      else if (/^at\s*\(/.test(code.slice(i)) && !n.at && !n.atNode) {
        i = code.indexOf('(', i);
        const g = group('(', ')') ?? '';
        // `at (b)`: another node's center, shifts on top (`xshift=2cm`)
        if (/^\s*[A-Za-z][\w -]*\s*$/.test(g)) n.atNode = g.trim();
        else if (!(n.at = g.match(POINT))) return null;
      }
      else break;
    }
    if (!n.coordinate) { if (code[i] !== '{') return null; n.label = group('{', '}'); }
    if (n.label == null || !/^\s*;$/.test(code.slice(i)) || !/^[\w -]+$/.test(n.name ?? '')) return null;
    const p = n.opts.map(placement), k = p.findIndex(Boolean);
    // two placements, or one with `at`: the last wins in TikZ, kept as code here
    if (p.filter(Boolean).length > (n.at || n.atNode ? 0 : 1)) return null;
    const place = n.atNode ? { dir: [0, 0], of: n.atNode, shift: true, opt: null } : k < 0 ? null : { ...p[k], opt: n.opts[k] };
    const { atNode, ...rest } = n;
    const out = { ...rest, name: n.name.trim(), x: n.at ? +n.at[1] : 0, y: n.at ? +n.at[2] : 0, at: !!n.at, place };
    if (head[1] !== 'matrix') return out;
    // a matrix: of nodes, placed by `at`
    const cells = n.opts.includes('matrix of nodes') && !out.place && cellsOf(n.label, out.name);
    if (!cells) return null;
    for (const c of cells) c.parent = out;
    return { ...out, matrix: true, cells };
  }
  const LAYER_OPEN = /^\\(?:begin\s*\{pgfonlayer\}|pgfonlayer)\s*\{([^{}]*)\}/;
  const LAYER_CLOSE = /^\\(?:end\s*\{pgfonlayer\}|endpgfonlayer\b)/;
  /** The leading blank lines and comments of a statement's text, and its code. */
  const piece = (text) => { const code = text.replace(/^(\s|%[^\n]*\n?)*/, ''); return { text, lead: text.slice(0, text.length - code.length), code }; };
  /**
   * A picture's body as statements `{text, lead, code, type, layer…}` and the text after the last.
   * Layer blocks (`pgfonlayer`, a scope with a key of `layerKeys` that opens a layer) are items of
   * their own; the statements in one carry its layer.
   */
  function parse(body, layerKeys = {}) {
    const items = [], blocks = [];
    let depth = 0, from = 0, groups = 0, glued = false;
    // the innermost layer block; a statement in a plain scope inside one cannot leave it
    const open = () => blocks.findLast((b) => b.open.layer)?.open ?? null;
    const cut = (i, semi) => {
      const inner = blocks.at(-1)?.open, it = { ...piece(body.slice(from, i)), type: 'code', layer: open()?.layer ?? 'main', block: inner?.layer ? inner : null, nested: !!(inner && !inner.layer && open()) };
      const n = !glued && nodeOf(it.code), e = !glued && it.code.match(EDGE);
      if (n) Object.assign(it, { type: n.matrix ? 'matrix' : 'node' }, n);
      else if (e) {
        const o = opts(e[1]);
        Object.assign(it, { type: 'edge', arrow: o.find(tipsOf) ?? '-', opts: o.filter((x) => !tipsOf(x)), from: e[2].trim(), fromAt: e[3]?.trim() ?? '', at: e[6] ?? '', label: e[7] ?? '', to: e[8].trim(), toAt: e[9]?.trim() ?? '' });
        // `to[bend left]`: a curve; its own options kept with the operation
        if (e[4] !== '--') it.via = e[5] ?? '';
      }
      if (it.cells) for (const c of it.cells) c.parent = it;
      items.push(it);
      from = i;
      // `\foreach \x in {…}` with its body on the next line: that statement is the loop's
      glued = !semi && groups < 2 && /^\\foreach\b/.test(it.code);
      groups = 0;
    };
    // a block's begin or end as an item; text before it without `;` stays code
    const mark = (i, len, what) => {
      if (body.slice(from, i).trim()) cut(i, false);
      const it = { ...piece(body.slice(from, i + len)), type: 'layer', ...what };
      items.push(it);
      from = i + len;
      glued = false;
      return it;
    };
    for (let i = 0; i < body.length; i++) {
      const c = body[i];
      if (c === '\\') {
        const rest = depth ? '' : body.slice(i);
        let m;
        if ((m = rest.match(LAYER_OPEN))) {
          const it = mark(i, m[0].length, { layer: m[1].trim() });
          blocks.push({ open: it });
          i += m[0].length - 1;
        } else if ((m = rest.match(LAYER_CLOSE)) && blocks.at(-1)?.open && !blocks.at(-1).scope) {
          blocks.pop().open.closeCode = mark(i, m[0].length, { close: true }).code;
          i += m[0].length - 1;
        } else if ((m = rest.match(/^\\begin\s*\{scope\}\s*(?=\[)/))) {
          // the scope's options: one that opens a layer makes it a block
          let j = i + m[0].length, d = 0;
          for (; j < body.length; j++) { if (body[j] === '[' || body[j] === '{') d++; else if ((body[j] === ']' || body[j] === '}') && --d === 0) break; }
          const layer = opts(body.slice(i + m[0].length + 1, j)).map((o) => layerKeys[kv(o)[0]]).find(Boolean);
          blocks.push({ open: mark(i, j + 1 - i, layer ? { layer } : { scope: true }), scope: true });
          i = j;
        } else if ((m = rest.match(/^\\begin\s*\{scope\}/))) {
          blocks.push({ open: mark(i, m[0].length, { scope: true }), scope: true });
          i += m[0].length - 1;
        } else if ((m = rest.match(/^\\end\s*\{scope\}/)) && blocks.at(-1)?.scope) {
          blocks.pop().open.closeCode = mark(i, m[0].length, { close: true }).code;
          i += m[0].length - 1;
        } else i++;
        continue;
      }
      if (c === '%') { while (i < body.length && body[i] !== '\n') i++; continue; }
      if ('{['.includes(c)) groups += depth++ === 0 && c === '{';
      else if ('}]'.includes(c)) {
        // a group closed at the end of its line (`\foreach … { … }`) ends a statement too
        if (--depth === 0 && c === '}' && /^[ \t]*(%[^\n]*)?\n/.test(body.slice(i + 1))) cut(i + 1, false);
      } else if (c === ';' && depth === 0) cut(i + 1, true);
    }
    return { items, tail: body.slice(from) };
  }
  // `at` only where the text had one or a drag put one: a relative node stays relative; `k` the picture's scale
  const nodeText = (n, k = 1) => {
    const at = n.place?.shift ? ` at (${n.place.of})` : n.at ? ` at (${fmt(n.x / k)},${fmt(n.y / k)})` : '';
    return n.coordinate ? `\\coordinate (${n.name})${at};` : `\\node${n.opts.length ? `[${n.opts.join(', ')}]` : ''} (${n.name})${at} {${n.label}};`;
  };
  const cellText = (c) => (c.opts.length || c.named ? `|${c.opts.length ? `[${c.opts.join(', ')}]` : ''}${c.named ? `${c.opts.length ? ' ' : ''}(${c.name})` : ''}| ` : '') + c.label;
  const matrixText = (m, k = 1, indent = '  ') => {
    const rows = Math.max(...m.cells.map((c) => c.row)) + 1, cols = Math.max(...m.cells.map((c) => c.col)) + 1;
    const grid = Array.from({ length: rows }, (_, r) => Array.from({ length: cols }, (_, c) => m.cells.find((x) => x.row === r && x.col === c && !x.deleted)));
    // a row ends at its last node: no empty cells after it
    const lines = grid.map((r) => `${indent}  ${r.slice(0, r.findLastIndex(Boolean) + 1).map((c) => (c ? cellText(c) : '')).join(' & ')} \\\\`);
    return `\\matrix (${m.name}) [${m.opts.join(', ')}]${m.at ? ` at (${fmt(m.x / k)},${fmt(m.y / k)})` : ''} {\n${lines.join('\n')}\n${indent}};`;
  };
  const end = (name, at) => `(${name}${at ? `.${at}` : ''})`;
  /** A bend makes a curve only on a `to` path: an arrow with one is written with `to`. */
  const bends = (list) => list.some((o) => /^bend (left|right)\b/.test(o));
  const edgeText = (e) => {
    const o = [...(e.arrow === '-' ? [] : [e.arrow]), ...e.opts];
    const op = e.via != null || bends(e.opts) ? `to${e.via ? `[${e.via}]` : ''}` : '--';
    return `\\draw${o.length ? `[${o.join(', ')}]` : ''} ${end(e.from, e.fromAt)} ${op} ${e.label ? `node[${e.at || 'above'}] {${e.label}} ` : ''}${end(e.to, e.toAt)};`;
  };
  /**
   * The body again: untouched statements as they were, edited and new ones written out. One
   * moved to another layer is written where it stood, out of its block and into one of its own.
   */
  function emit({ items, tail }, k = 1) {
    const indent = items.find((i) => i.lead.includes('\n'))?.lead.split('\n').pop() ?? '  ';
    const stmt = (i) => i.type === 'matrix' ? matrixText(i, k, indent) : i.type === 'node' ? nodeText(i, k) : edgeText(i);
    const text = (i) => {
      const nl = `\n${i.lead.includes('\n') ? i.lead.split('\n').pop() : indent}`, first = i.lead || nl;
      if (!i.moved) return first + stmt(i);
      const own = i.layer === 'main' ? `${first}${stmt(i)}` : `${first}\\begin{pgfonlayer}{${i.layer}}${nl}  ${stmt(i)}${nl}\\end{pgfonlayer}`;
      if (!i.block) return own;
      // the block's end and begin again at the block's indent
      const bl = `\n${i.block.lead.split('\n').pop()}`;
      return `${first.replace(/\n[ \t]*$/, '') || ''}${bl}${i.block.closeCode}${nl}${own.slice(first.length)}${bl}${i.block.code}`;
    };
    const kept = items.filter((i) => !i.deleted && i.lead), added = items.filter((i) => !i.deleted && !i.lead);
    // new statements go after the last one's line, its comment included
    const line = tail.match(/^[^\n]*/)[0];
    return kept.map((i) => i.dirty || i.moved ? text(i) : i.text).join('') + line + added.map(text).join('') + tail.slice(line.length);
  }
  /** Moves `it` to `layer`; written where it stands. */
  const toLayer = (it, layer) => { if (it.nested) return; it.layer = layer; it.moved = layer !== (it.block?.layer ?? 'main'); };
  /**
   * The nodes in a grid as one `matrix of nodes`, centers kept: equal steps between origins, the
   * matrix anchored at its first node. Null, with why, when they are not in a grid.
   */
  function toMatrix(state, nodes, name, k = 1) {
    if (nodes.length < 2) return { why: 'needs two nodes or more' };
    if (nodes.some((n) => n.coordinate || n.cell || n.layer !== 'main' || /&|\\\\/.test(n.label))) return { why: 'only plain nodes on the main layer' };
    const axis = (vs) => {
      const u = [...new Set(vs.map((v) => +v.toFixed(3)))].sort((a, b) => a - b);
      const step = Math.min(...u.slice(1).map((v, i) => v - u[i]));
      if (u.length > 1 && !u.every((v) => Math.abs((v - u[0]) / step - Math.round((v - u[0]) / step)) < 1e-3)) return null;
      return { lo: u[0], hi: u.at(-1), step: u.length > 1 ? step : 0 };
    };
    const X = axis(nodes.map((n) => n.x)), Y = axis(nodes.map((n) => n.y));
    if (!X || !Y) return { why: 'the nodes are not in a grid' };
    const at = (n) => [Y.step ? Math.round((Y.hi - n.y) / Y.step) : 0, X.step ? Math.round((n.x - X.lo) / X.step) : 0];
    if (new Set(nodes.map((n) => at(n).join())).size < nodes.length) return { why: 'two nodes share a cell' };
    const first = nodes.reduce((a, b) => (state.items.indexOf(a) < state.items.indexOf(b) ? a : b));
    const m = { type: 'matrix', matrix: true, dirty: true, lead: first.lead, layer: 'main', name, label: '', at: true, x: first.x, y: first.y, coordinate: false, place: null,
      opts: ['matrix of nodes', ...(Y.step ? [`row sep={${fmt(Y.step)}cm,between origins}`] : []), ...(X.step ? [`column sep={${fmt(X.step)}cm,between origins}`] : []), `matrix anchor=${first.name}.center`] };
    m.cells = nodes.map((n) => {
      const [row, col] = at(n);
      const own = n.opts.filter((o) => o !== n.place?.opt && !/^[xy]shift\b|^node distance\b/.test(o));
      return { type: 'node', cell: true, parent: m, row, col, name: n.name, named: true, opts: own, label: n.label, at: true, x: n.x, y: n.y };
    });
    state.items.splice(state.items.indexOf(first), 1, m);
    for (const n of nodes) n.deleted = true;
    return { matrix: m };
  }
  /** Where a matrix's cells stand: steps between origins from its anchor cell, else null. */
  function cellPlaces(m) {
    const o = new Map(m.opts.map(kv));
    const sep = (k) => { const [len, how] = (o.get(k) ?? '').split(',').map((s) => s.trim()); return how === 'between origins' ? cm(len) : o.has(k) ? null : 0; };
    const [rs, cs] = [sep('row sep'), sep('column sep')];
    const anchor = (o.get('matrix anchor') ?? '').match(/^(.+)\.center$/)?.[1], a = m.cells.find((c) => c.name === anchor);
    if (rs == null || cs == null || !a || !m.at) return null;
    return m.cells.map((c) => [m.x + (c.col - a.col) * cs, m.y - (c.row - a.row) * rs]);
  }
  // Mermaid flowcharts (`graph TD`, `A[x] -->|y| B((z))`)
  const MNODE = /^\s*([A-Za-z_][\w]*)\s*(?:\(\((.*?)\)\)|\[(.*?)\]|\((.*?)\)|\{(.*?)\})?/;
  const MLINK = /^\s*(?:(<-->|-->|---|==>|-\.->)|--\s*(.+?)\s*(-->|---))\s*(?:\|([^|]*)\|)?/;
  const unquote = (s) => s?.trim().replace(/^"(.*)"$/, '$1').replace(/#quot;/g, '"');
  /** A flowchart's nodes in order of appearance and its edges. */
  function mermaidIn(src) {
    const nodes = new Map(), edges = [];
    let dir = 'TD';
    const node = (m) => {
      const [, id, circle, rect, round, rhomb] = m;
      const label = unquote(circle ?? rect ?? round ?? rhomb);
      const n = nodes.get(id) ?? { name: id, label: id, shape: 'rectangle' };
      if (label != null) Object.assign(n, { label, shape: circle != null ? 'circle' : round != null ? 'rounded' : 'rectangle' });
      nodes.set(id, n);
      return id;
    };
    for (const line of src.split(/\n|;/).map((l) => l.replace(/%%.*/, '').trim()).filter(Boolean)) {
      const head = line.match(/^(?:graph|flowchart)\s+(TD|TB|LR|RL|BT)\b/);
      if (head) { dir = head[1] === 'TB' ? 'TD' : head[1]; continue; }
      let m = line.match(MNODE), rest = line;
      if (!m) continue;
      let from = node(m);
      rest = rest.slice(m[0].length);
      while ((m = rest.match(MLINK))) {
        rest = rest.slice(m[0].length);
        const t = rest.match(MNODE);
        if (!t) break;
        rest = rest.slice(t[0].length);
        const to = node(t), kind = m[1] ?? m[3];
        edges.push({ from, to, arrow: kind === '<-->' ? '<->' : kind === '---' ? '-' : '->', label: unquote(m[4] ?? m[2]) ?? '' });
        from = to;
      }
    }
    return { dir, nodes: [...nodes.values()], edges };
  }
  /** Positions by rank (the longest path from a source), `spacing` cm apart, centered per rank. */
  function layout({ dir, nodes, edges }, spacing = 2) {
    // a depth-first walk in order of appearance drops the edges that close a cycle
    const state = new Map(), dag = [];
    const walk = (u) => {
      state.set(u, 1);
      for (const e of edges.filter((e) => e.from === u)) if (!state.has(e.to)) { dag.push(e); walk(e.to); } else if (state.get(e.to) === 2) dag.push(e);
      state.set(u, 2);
    };
    for (const n of nodes) if (!state.has(n.name)) walk(n.name);
    const rank = new Map(nodes.map((n) => [n.name, 0]));
    for (let k = 0; k < nodes.length; k++) for (const e of dag) rank.set(e.to, Math.max(rank.get(e.to), rank.get(e.from) + 1));
    const per = new Map();
    for (const n of nodes) per.set(rank.get(n.name), [...(per.get(rank.get(n.name)) ?? []), n]);
    for (const [r, row] of per) row.forEach((n, i) => {
      const across = (i - (row.length - 1) / 2) * spacing, along = r * spacing;
      [n.x, n.y] = { TD: [across, -along], BT: [across, along], LR: [along, -across], RL: [-along, -across] }[dir] ?? [across, -along];
    });
    return nodes;
  }
  const SHAPE_OPTS = { rectangle: ['draw'], circle: ['draw', 'circle'], rounded: ['draw', 'rounded corners'], text: [] };
  const shapeOf = (n) => n.opts?.includes('circle') ? 'circle' : n.opts?.some((o) => /^rounded corners\b/.test(o)) ? 'rounded' : n.shape ?? 'rectangle';
  const id = (s) => s.replace(/\W/g, '_');
  /** The flowchart of `nodes` and `edges`, top-down unless wider than tall. */
  function mermaidOut(nodes, edges) {
    const xs = nodes.map((n) => n.x), ys = nodes.map((n) => n.y);
    const wide = nodes.length && Math.max(...xs) - Math.min(...xs) > Math.max(...ys) - Math.min(...ys);
    const q = (s) => /[[\](){}|"]/.test(s) ? `"${s.replace(/"/g, '#quot;')}"` : s;
    // a label as it reads: TeX's markup dropped
    const box = (n) => { const l = q(plain(n.label) || n.name); return { circle: `((${l}))`, rounded: `(${l})` }[shapeOf(n)] ?? `[${l}]`; };
    const link = (e) => { const [s, t] = tipsOf(e.arrow) ?? ['', '']; return s && t ? '<-->' : s || t ? '-->' : '---'; };
    // a start tip alone points back: the flowchart's arrow runs the other way
    const back = (e) => { const [s, t] = tipsOf(e.arrow) ?? ['', '']; return s && !t; };
    return [`graph ${wide ? 'LR' : 'TD'}`, ...nodes.map((n) => `  ${id(n.name)}${box(n)}`),
      ...edges.map((e) => { const [a, b] = back(e) ? [e.to, e.from] : [e.from, e.to]; return `  ${id(a)} ${link(e)}${e.label ? `|${q(e.label)}|` : ''} ${id(b)}`; })].join('\n');
  }
  /** What a flowchart of the picture leaves out, as short phrases. */
  function mermaidLoss(nodes, edges, code = 0) {
    const kinds = new Set(['', 'draw', 'circle', 'rounded corners']);
    const out = [];
    if (nodes.length > 1) out.push('positions');
    if (nodes.some((n) => (n.opts ?? []).some((o) => !kinds.has(o) && o !== n.place?.opt))) out.push('node styles');
    if (nodes.some((n) => n.label !== '' && n.name !== id(n.name))) out.push('names with spaces or dashes');
    if (edges.some((e) => e.fromAt || e.toAt)) out.push('anchors');
    if (edges.some((e) => (tipsOf(e.arrow) ?? []).some((t) => t && t !== '>' && t !== '<'))) out.push('arrow tips');
    if (edges.some((e) => e.opts.length)) out.push('arrow styles');
    if ([...nodes, ...edges].some((i) => (i.layer ?? 'main') !== 'main')) out.push('layers');
    if (code) out.push(`${code} statement${code > 1 ? 's' : ''} kept as code`);
    return out;
  }
  /** Adds a flowchart to `state`: names already there are reused, the rest laid out below; `replace` drops the nodes and arrows first. */
  function importMermaid(state, src, spacing, replace = false) {
    const g = mermaidIn(src);
    if (!g.nodes.length) return g;
    if (replace) for (const i of state.items) if (['node', 'edge', 'matrix'].includes(i.type)) i.deleted = true;
    const live = state.items.filter((i) => i.type === 'node' && !i.deleted), have = new Set(live.map((i) => i.name));
    const below = live.length ? Math.min(...live.map((i) => i.y)) - spacing : 0;
    for (const n of layout(g, spacing)) if (!have.has(n.name)) state.items.push({ type: 'node', dirty: true, lead: '', layer: 'main', name: n.name, label: n.label, opts: SHAPE_OPTS[n.shape], at: true, x: n.x, y: n.y + below });
    for (const e of g.edges) state.items.push({ type: 'edge', dirty: true, lead: '', layer: 'main', from: e.from, to: e.to, fromAt: '', toAt: '', arrow: e.arrow, opts: [], label: e.label, at: '' });
    return g;
  }
  /** The picture's options as `[key, value, text]`; `set` gives the text with `key` changed (null drops it). */
  const picOptions = (text) => {
    const list = opts(text).map((o) => [...kv(o), o]);
    return {
      list,
      get: (k) => list.find(([key]) => key === k)?.[1],
      set(k, v) { const i = list.findIndex(([key]) => key === k), o = v == null || v === '' ? null : `${k}=${/[,=]/.test(v) || /style$/.test(k) ? `{${v}}` : v}`; if (i < 0) { if (o) list.push([k, v, o]); } else if (o) list[i] = [k, v, o]; else list.splice(i, 1); },
      text: () => list.map((e) => e[2]).join(', '),
    };
  };
  /** A key that defines a style: `box/.style`, `every node/.append style`. */
  const STYLE_DEF = /^(.+?)\s*\/\.(?:append )?style$/;
  /** Options with each style named in them followed by its keys (`bodies`: name → keys), as pgfkeys follows them, a few levels deep; a style with parameters stays as it is. */
  const expandStyles = (list, bodies, depth = 4) => list.flatMap((o) => {
    const [key, v] = kv(o), b = !v && depth ? bodies.get(key) : null;
    return typeof b === 'string' && !b.includes('#') ? [o, ...expandStyles(opts(b), bodies, depth - 1)] : [o];
  });
  /** TikZ's own `every …` styles apply by themselves, never by name. */
  const STYLE_EVERY = /^every /;
  /** A distance `v and h` (or one for both) as `[h, v]` in cm. */
  const apart = (d) => { const [v, h = v] = (d ?? '1cm').split(/\s+and\s+/).map(cm); return [h, v]; };
  /** Where TikZ puts relative node `n` from `r`, shifts included; null for a distance it cannot read. */
  function relPos(n, r, { size, distance, onGrid }) {
    const p = n.place, o = new Map(n.opts.map(kv));
    // `at (r)` and shifts: r's center moved, whatever the sizes
    if (p.shift) return [r.x + (cm(o.get('xshift')) ?? 0), r.y + (cm(o.get('yshift')) ?? 0)];
    const nd = o.has('node distance') ? apart(o.get('node distance')) : distance;
    const [dx, dy] = p.dir, [h, v] = p.dist ? apart(p.dist) : nd, [rw, rh] = size(r), [w, ht] = size(n);
    if (h == null || v == null || nd[1] == null) return null;
    const flat = p.centers || onGrid, len = p.centers ? Math.hypot(dx, dy) : 1;
    return [r.x + dx * (p.centers ? nd[1] / len : h + (flat ? 0 : (rw + w) / 2)) + (cm(o.get('xshift')) ?? 0),
      r.y + dy * (p.centers ? nd[1] / len : v + (flat ? 0 : (rh + ht) / 2)) + (cm(o.get('yshift')) ?? 0)];
  }
  /** A shift up to this many cm is a nudge of a relative node, not a new place. */
  const NUDGE = 1;
  const SHIFTS = ['xshift', 'yshift'];
  /**
   * The smallest edit that puts relative node `n` near (x, y): its relation at a new distance, a
   * small shift, a relation to the nearest node before it, else `at`. Distances snap to `grid`. A
   * node without a relation gets one to the nearest node before it (`right=1cm of a`), else `at`.
   * `{opts, place, at, x?, y?, hint}`.
   */
  function relocate(n, x, y, env) {
    const { by, earlier, size, distance, onGrid, grid } = env;
    const sn = (v) => (grid ? Math.round(v / grid) * grid : v), tol = Math.max(grid, 0.2) / 2;
    const unshifted = n.opts.filter((o) => !SHIFTS.includes(kv(o)[0]));
    const bare = unshifted.filter((o) => kv(o)[0] !== 'node distance');
    // the placement option replaced where it stood, `extra` after it
    const put = (list, p, extra) => {
      const opt = placeText(p), i = list.indexOf(n.place?.opt), out = [...list];
      out.splice(i < 0 ? out.length : i, i < 0 ? 0 : 1, opt, ...extra);
      return { opts: out, place: { ...placement(opt), opt }, at: false };
    };
    const rel = (r, centers) => {
      const ox = x - r.x, oy = y - r.y;
      const dir = [Math.abs(ox) < tol ? 0 : Math.sign(ox), Math.abs(oy) < tol ? 0 : Math.sign(oy)];
      if (!dir[0] && !dir[1]) return null;
      if (centers) {
        if (dir[0] && dir[1] && Math.abs(Math.abs(ox) - Math.abs(oy)) >= tol) return null;
        const d = sn(Math.hypot(dir[0] ? ox : 0, dir[1] ? oy : 0));
        return d > 0 ? { p: { dir, of: r.name, centers: true }, extra: Math.abs(d - distance[1]) < 1e-6 ? [] : [`node distance=${fmt(d)}cm`], d: fmt(d) } : null;
      }
      const [rw, rh] = size(r), [w, h] = size(n), gap = onGrid ? [0, 0] : [(rw + w) / 2, (rh + h) / 2];
      const hd = sn(Math.abs(ox) - gap[0]), vd = sn(Math.abs(oy) - gap[1]);
      if ((dir[0] && hd < 0) || (dir[1] && vd < 0)) return null;
      const d = dir[0] && dir[1] ? `${fmt(vd)}cm and ${fmt(hd)}cm` : `${fmt(dir[0] ? hd : vd)}cm`;
      const same = (!dir[0] || Math.abs(hd - distance[0]) < 1e-6) && (!dir[1] || Math.abs(vd - distance[1]) < 1e-6);
      return { p: { dir, of: r.name, dist: same ? undefined : d }, extra: [], d: d.replace(/cm/g, '') };
    };
    const hint = (q) => `${words(q.p.dir)} of ${q.p.of}, ${q.d} cm`;
    // a node at `at` (or none) gets a relation of positioning's kind
    const r = n.place && by.get(n.place.of), centers = n.place?.centers ?? false;
    // `at (r)` shifted: the shifts follow the pointer
    const shifted = (to) => {
      const sx = sn(x - to.x), sy = sn(y - to.y);
      return { opts: [...bare.filter((o) => o !== n.place?.opt), ...(sx ? [`xshift=${fmt(sx)}cm`] : []), ...(sy ? [`yshift=${fmt(sy)}cm`] : [])], place: { dir: [0, 0], of: to.name, shift: true, opt: null }, at: false, hint: `at (${to.name}) shifted ${fmt(sx)}, ${fmt(sy)} cm` };
    };
    if (n.place?.shift && r) return shifted(r);
    // the same relation, a new distance
    const q = r && rel(r, centers);
    if (q && q.p.dir.join() === n.place.dir.join()) return { ...put(bare, q.p, q.extra), hint: hint(q) };
    // the relation as it is, nudged
    const at0 = r && relPos({ ...n, opts: unshifted }, r, env);
    if (at0) {
      const sx = sn(x - at0[0]), sy = sn(y - at0[1]);
      if (Math.max(Math.abs(sx), Math.abs(sy)) <= NUDGE) {
        const shift = [...(sx ? [`xshift=${fmt(sx)}cm`] : []), ...(sy ? [`yshift=${fmt(sy)}cm`] : [])];
        return { opts: [...unshifted, ...shift], place: n.place, at: false, hint: `${words(n.place.dir)} of ${n.place.of}${shift.length ? `, shifted ${fmt(sx)}, ${fmt(sy)} cm` : ''}` };
      }
    }
    // a relation to the nearest node before it
    const near = earlier.filter((m) => m.x != null && m !== n).sort((a, b) => Math.hypot(a.x - x, a.y - y) - Math.hypot(b.x - x, b.y - y))[0];
    // where a size is a guess (a new or restyled node), only a shift from its center is sure
    if (near && env.exact && !(env.exact(n) && env.exact(near))) return shifted(near);
    const q2 = near && rel(near, centers);
    if (q2) return { ...put(bare, q2.p, q2.extra), hint: hint(q2) };
    return { opts: bare.filter((o) => o !== n.place?.opt), place: null, at: true, x: sn(x), y: sn(y), hint: `at (${fmt(sn(x))}, ${fmt(sn(y))})` };
  }
  // anchors: the compass points a shape has, as directions
  const COMPASS = { center: [0, 0], north: [0, 1], south: [0, -1], east: [1, 0], west: [-1, 0], 'north east': [1, 1], 'north west': [-1, 1], 'south east': [1, -1], 'south west': [-1, -1] };
  /** Anchor `a` of a node at `[x, y]` with size `[w, h]`: a compass point or an angle on its border. */
  function anchorAt(n, [w, h], round, a) {
    const deg = COMPASS[a] ? Math.atan2(COMPASS[a][1], COMPASS[a][0]) : (+a * Math.PI) / 180;
    if (a === 'center' || Number.isNaN(deg)) return [n.x, n.y];
    const c = Math.cos(deg), s = Math.sin(deg);
    if (round) return [n.x + (c * w) / 2, n.y + (s * h) / 2];
    // a rectangle's compass points are its corners and sides; an angle meets the border
    if (COMPASS[a]) return [n.x + (COMPASS[a][0] * w) / 2, n.y + (COMPASS[a][1] * h) / 2];
    const t = Math.min(Math.abs(w / 2 / (c || 1e-9)), Math.abs(h / 2 / (s || 1e-9)));
    return [n.x + c * t, n.y + s * t];
  }
  /** The anchor of `n` nearest to `[x, y]`: a compass point within `reach`, else the angle in steps of 15°. */
  function nearestAnchor(n, wh, round, [x, y], reach = 0.25) {
    const [name, d] = Object.keys(COMPASS).map((a) => [a, Math.hypot(...anchorAt(n, wh, round, a).map((v, i) => v - [x, y][i]))]).sort((a, b) => a[1] - b[1])[0];
    return d <= reach ? name : String(((Math.round((Math.atan2(y - n.y, x - n.x) * 180) / Math.PI / 15) * 15) + 360) % 360);
  }

  // the dialog: an SVG canvas in cm (y up), a tool bar, the selection's inspector, layers, the
  // picture's options, code kept, Mermaid
  const NS = 'http://www.w3.org/2000/svg';
  const svgEl = (tag, attrs = {}) => { const e = document.createElementNS(NS, tag); for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v); return e; };
  // a label as TeX sets it, near enough to read: the logos, `$…$` in italics, other commands and
  // braces dropped for their text; `[text, dx, dy, size]` runs (em)
  const LOGO = { TeX: [['T'], ['E', -0.17, 0.22], ['X', -0.12, -0.22]] };
  LOGO.LaTeX = [['L'], ['A', -0.36, -0.2, 0.7], ['T', -0.15, 0.2], ...LOGO.TeX.slice(1)];
  const runs = (label) => {
    const out = [];
    let italic = false;
    for (const [m, cs, sym] of (label ?? '').matchAll(/\\([A-Za-z]+)\s*(?:\{\})?|\\(.)|[^\\${}~]+|./g)) {
      if (cs) out.push(...(LOGO[cs] ?? []).map(([t, dx = 0, dy = 0, size = 1]) => [t, dx, dy, size, italic]));
      else if (m === '$') italic = !italic;
      else if (m === '~' || sym === '\\' || sym === ',') out.push([' ']);
      else if (sym) out.push([sym, 0, 0, 1, italic]);
      else if (!'{}'.includes(m)) out.push([m, 0, 0, 1, italic]);
    }
    return out;
  };
  const plain = (label) => runs(label).map((r) => r[0]).join('');
  function setLabel(t, label) {
    for (const [text, dx, dy, size, italic] of runs(label)) {
      const span = Object.assign(svgEl('tspan'), { textContent: text });
      if (dx) span.setAttribute('dx', `${dx}em`);
      if (dy) span.setAttribute('dy', `${dy}em`);
      if (size && size !== 1) span.setAttribute('font-size', `${size}em`);
      if (italic) span.setAttribute('font-style', 'italic');
      t.append(span);
    }
    return t;
  }
  // LaTeX's font switches a `font=` key may hold: the size relative to \normalsize, and the face
  const FONT = { tiny: 0.5, scriptsize: 0.7, footnotesize: 0.8, small: 0.9, normalsize: 1, large: 1.2, Large: 1.44, LARGE: 1.73, huge: 2.07, Huge: 2.49 };
  const PT = 2.54 / 72.27;
  /** An arrow tip TeX drew (`previews.json`) as a marker: its last 8pt, the line's height its reference. */
  function tipMarker(svg) {
    const box = svg.match(/viewBox='([^']*)'/)?.[1].trim().split(/\s+/).map(Number), line = svg.match(/translate\(([-\d.]+),([-\d.]+)\)/);
    if (box?.length !== 4 || !line) return null;
    const [x, y, w, h] = box, cut = Math.min(w, 8);
    const inner = svg.replace(/^[^]*?<svg[^>]*>/, '').replace(/<\/svg>\s*$/, '').replace(/\sid='[^']*'/g, '').replace(/#888(888)?\b/g, 'currentColor');
    return { box: [x + w - cut, y, cut, h], ref: [x + w, +line[2]], inner };
  }
  // the canvas grid each viewer chose: its step in cm (0: none) and whether drags snap to it
  const GRID_KEY = 'satex.canvas.grid';
  const gridPref = (step) => { try { return { step, snap: true, ...JSON.parse(localStorage.getItem(GRID_KEY) ?? '{}') }; } catch { return { step, snap: true }; } };
  const TRASH = '<svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true"><path d="M6 2h4M2.5 4h11M4 4l.7 9.2a1 1 0 0 0 1 .8h4.6a1 1 0 0 0 1-.8L12 4M6.5 6.5v5M9.5 6.5v5" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/></svg>';

  function drawing(s, view, ctx) {
    const R = s.run ?? {}, keys = R.layer_keys ?? {};
    const state = parse(s.body, keys), run = new Map((s.nodes ?? []).map((n) => [n.name, n]));
    const pic = picOptions(s.options);
    let picDirty = false;
    // the styles the picture's options define
    const picStyles = () => pic.list.map(([key, v]) => [key.match(STYLE_DEF), v]).filter(([m]) => m).map(([m, v]) => ({ name: m[1], body: v, key: m[0] }));
    // the picture's options as the canvas applies them
    let style, pathStyle, distance, k, font, tipDefault, onGrid;
    // the document's styles (`\tikzset{box/.style={…}}`) as edited here: written on Apply
    let docStyles = (R.keys ?? []).filter((x) => x.style).map((x) => ({ name: x.name, body: x.body ?? '', orig: x.body ?? '', range: x.style.range, entry: x.style.entry }));
    // a style's keys by name: the run's, the document's as edited, the picture's own
    const styleBody = () => new Map([...(R.keys ?? []).filter((x) => x.body != null).map((x) => [x.name, x.body]),
      ...docStyles.filter((d) => !d.deleted).map((d) => [d.name, d.body]), ...picStyles().map((p) => [p.name, p.body])]);
    let bodies = new Map();
    const expand = (list) => expandStyles(list, bodies);
    const applyPic = () => {
      bodies = styleBody();
      style = expand(['every node', ...['every node/.style', 'every node/.append style', 'nodes'].flatMap((key) => opts(pic.get(key)))]);
      // the picture's own keys reach every path, as its `every path` style does
      pathStyle = expand(['every path', ...pic.list.filter(([key]) => !STYLE_DEF.test(key)).map((e) => e[2]), ...['every path/.style', 'every path/.append style'].flatMap((key) => opts(pic.get(key)))]);
      distance = apart(pic.get('node distance'));
      const k1 = +(pic.get('scale') ?? 1) || 1;
      // the text's coordinates stay; where they land changes with the scale
      if (k && k1 !== k) for (const it of state.items) if (it.at && (it.type === 'node' || it.type === 'matrix')) { it.x *= k1 / k; it.y *= k1 / k; }
      k = k1;
      font = pic.get('font') ?? '';
      tipDefault = pic.get('>') || '';
      onGrid = pic.get('on grid') != null;
    };
    applyPic();
    for (const it of state.items) if (it.at && (it.type === 'node' || it.type === 'matrix')) { it.x *= k; it.y *= k; }
    // the keys that change a node's size: not where it stands
    const lookKey = (list) => list?.filter((o) => !placement(o) && !['xshift', 'yshift', 'node distance'].includes(kv(o)[0])).join();
    for (const it of state.items) for (const n of it.cells ?? [it]) n.orig = lookKey(n.opts);
    const pref = { relative: !!s.relative, ...gridPref(s.grid || 0.5) };
    // relative placement: moved and new nodes are written relative to a neighbour, not `at`
    const relMode = () => !!pref.relative;
    const step = () => pref.step || 0, snapping = () => pref.snap && step() > 0;
    const colors = new Map((R.colors ?? []).map((c) => [c.name, c.hex]));
    const defined = (R.colors ?? []).filter((c) => c.hex).map((c) => ({ name: c.name, rgb: [1, 3, 5].map((i) => parseInt(c.hex.slice(i, i + 2), 16) / 255) }));
    const css = (name) => { if (!name) return null; if (colors.get(name)) return colors.get(name); const m = mix(name, defined); return m ? hex(m) : name; };
    const shapes = R.shapes?.length ? R.shapes : ['rectangle', 'circle'];
    const tips = (R.tips ?? []).map((t) => t.name);
    const order = R.layers?.order?.length ? R.layers.order : ['main'];
    const layers = [...new Set([...order, ...(R.layers?.declared ?? []), ...state.items.map((i) => i.layer).filter(Boolean)])];
    const hidden = new Set();
    let tool = 'select', arrow = '->', sel = null, pending = null, box = null, hint = '', anchorsOf = null;
    const live = () => state.items.filter((i) => !i.deleted);
    const own = () => live().flatMap((i) => (i.type === 'node' ? [i] : i.type === 'matrix' ? i.cells.filter((c) => !c.deleted) : []));
    const said = new Map(state.items.flatMap((i) => i.cells ?? [i]).filter((i) => i.name).map((i) => [i.name, i.label]));
    const mats = () => new Set(live().filter((i) => i.type === 'matrix').map((m) => m.name));
    // the nodes the run made that no statement here edits: read-only where the run placed them
    const fixed = () => [...run.values()].filter((n) => n.x != null && !mats().has(n.name) && !own().some((o) => o.name === n.name));
    const nodes = () => [...own(), ...fixed().map((n) => ({ ...n, fixed: true, label: said.get(n.name) ?? n.name }))];
    const touch = (n) => { n.dirty = true; if (n.parent) n.parent.dirty = true; };
    const lookOf = (n) => [...style, ...expand(n.opts ?? [])];
    const shapeName = (n) => { const e = lookOf(n).map(kv).findLast(([key, v]) => key === 'shape' || (!v && shapes.includes(key))); return e ? e[1] || e[0] : run.get(n.name)?.shape ?? 'rectangle'; };
    const round = (n) => ['circle', 'ellipse'].includes(shapeName(n));
    // the run's size while label and options are the text's, else one from the label's length and the minimum size
    /** Whether the run's size of `n` holds: its text and look as the run had them. */
    const exact = (n) => { const r = run.get(n.name); return r?.w != null && (n.label === undefined || n.label === said.get(n.name)) && (lookKey(n.opts) ?? n.orig) === n.orig; };
    const size = (n) => {
      const r = run.get(n.name);
      if (exact(n)) return [r.w, r.h];
      if (n.coordinate) return [0.15, 0.15];
      const o = new Map(lookOf(n).map(kv)), min = cm(o.get('minimum size')) ?? 0;
      const wh = [Math.max(0.6, 0.19 * plain(n.label || ' ').length + 0.3, cm(o.get('minimum width')) ?? 0, min), Math.max(0.6, cm(o.get('minimum height')) ?? 0, min)];
      return shapeName(n) === 'circle' ? [Math.max(...wh), Math.max(...wh)] : wh;
    };
    const env = () => ({ size, distance, onGrid, grid: snapping() ? step() : 0, exact });
    /** Relative nodes where TikZ puts them, from where their reference is now: they follow a drag. */
    const settle = () => {
      const by = new Map([...run].filter(([, r]) => r.x != null));
      for (const it of live()) {
        if (it.type === 'matrix') {
          const at = it.geometry !== 'run' && cellPlaces(it);
          if (at) it.cells.forEach((c, i) => { [c.x, c.y] = at[i]; });
          for (const c of it.cells) by.set(c.name, c);
        } else if (it.type === 'node') {
          const r = it.place && by.get(it.place.of), at = r && relPos(it, r, env());
          if (it.place) it.lost = !at;
          if (at) [it.x, it.y] = at;
          by.set(it.name, it);
        }
      }
    };
    // a matrix the canvas cannot lay out stands where the run put its cells
    for (const m of state.items.filter((i) => i.type === 'matrix')) if (!cellPlaces(m)) m.geometry = m.cells.every((c) => run.get(c.name)?.x != null) ? 'run' : 'none';
    for (const m of state.items.filter((i) => i.geometry === 'run')) for (const c of m.cells) Object.assign(c, { x: run.get(c.name).x, y: run.get(c.name).y });
    settle();
    // a node TikZ puts elsewhere than the canvas would (a scope, a loop, a key it does not read) is code
    for (const it of state.items.filter((i) => i.type === 'node' || i.type === 'matrix')) {
      const off = (n) => { const r = run.get(n.name); return r?.x != null ? Math.hypot(r.x - n.x, r.y - n.y) > 0.05 : n.lost; };
      if (it.geometry === 'none' || (it.cells ?? [it]).some(off)) it.type = 'code';
    }
    const snap = (v) => (snapping() ? Math.round(v / step()) * step() : +v.toFixed(3));
    const layerOf = (i) => (i.parent ?? i).layer ?? 'main';
    const rank = (i) => { const r = order.indexOf(layerOf(i)); return r < 0 ? order.indexOf('main') : r; };

    // undo: the shared history of the dialogs, one step per gesture
    // a snapshot without cycles: a block by its index, cells without their matrix
    const snapshot = () => ({ items: state.items.map((i) => ({ ...i, block: i.block ? state.items.indexOf(i.block) : null, cells: i.cells?.map(({ parent, ...c }) => c) })), pic: pic.list, picDirty, docStyles });
    const H = self.satexDialogs?.history?.(snapshot, (v) => {
      state.items = v.items;
      for (const i of v.items) { i.block = i.block == null ? null : v.items[i.block]; for (const c of i.cells ?? []) c.parent = i; }
      pic.list.splice(0, pic.list.length, ...v.pic);
      picDirty = v.picDirty;
      docStyles = v.docStyles;
      sel = null;
      applyPic();
      draw();
      fields();
      picture();
      styles();
    });
    const done = () => H?.settle?.();

    const root = el('div', 'tv-draw');
    const bar = el('div', 'tv-tools');
    const modes = el('div', 'tv-tabs tv-modes');
    for (const [key, label] of [['select', 'Move'], ['rectangle', 'Rectangle'], ['circle', 'Circle'], ['text', 'Text'], ['connect', 'Connect']]) {
      const b = el('button', 'tv-tab', esc(label));
      b.type = 'button';
      b.dataset.tool = key;
      b.onclick = () => { tool = key; pending = null; draw(); };
      modes.append(b);
    }
    const tipOptions = (chosen) => ['', ...new Set([...(tips.includes('>') ? [] : ['>']), ...tips])].map((t) => `<option value="${esc(t)}"${t === chosen ? ' selected' : ''}>${esc(t || 'none')}</option>`).join('');
    const arrows = el('select', 'tv-input', tipOptions('>'));
    arrows.ariaLabel = 'arrow tip of new arrows';
    arrows.title = 'The tip new arrows end in';
    arrows.onchange = () => { arrow = `-${arrows.value}`; };
    // the grid: presets, a custom step, snapping; each viewer's own
    const grid = el('span', 'tv-row tv-grid-pick');
    const gsel = el('select', 'tv-input', ['0.25', '0.5', '1', 'off', 'custom'].map((v) => `<option value="${v}">${v === 'off' || v === 'custom' ? v : `${v} cm`}</option>`).join(''));
    gsel.ariaLabel = 'grid';
    const gnum = Object.assign(el('input', 'tv-input'), { type: 'number', step: 'any', min: '0.05', ariaLabel: 'grid step in cm', title: 'Grid step in cm' });
    const gsnap = Object.assign(el('label', 'tv-muted', '<input type="checkbox"> snap'), { title: 'Snap drags to the grid' });
    const showGrid = () => {
      const v = pref.step ? String(pref.step) : 'off';
      gsel.value = [...gsel.options].some((o) => o.value === v) ? v : 'custom';
      gnum.hidden = gsel.value !== 'custom';
      gnum.value = pref.step || '';
      gsnap.firstChild.checked = pref.snap;
    };
    const saveGrid = () => { try { localStorage.setItem(GRID_KEY, JSON.stringify(pref)); } catch { /* storage off */ } showGrid(); box = null; draw(true); };
    gsel.onchange = () => { if (gsel.value === 'custom') { gnum.hidden = false; return gnum.focus(); } pref.step = gsel.value === 'off' ? 0 : +gsel.value; saveGrid(); };
    gnum.onchange = () => { if (+gnum.value > 0) { pref.step = +gnum.value; saveGrid(); } };
    gsnap.firstChild.onchange = () => { pref.snap = gsnap.firstChild.checked; saveGrid(); };
    grid.append(el('span', 'tv-muted', 'grid'), gsel, gnum, gsnap);
    showGrid();
    const matrix = el('button', 'tv-button', 'Convert to matrix');
    matrix.type = 'button';
    matrix.title = 'Write the nodes, laid out in a grid, as one matrix of nodes';
    matrix.onclick = () => {
      const r = toMatrix(state, own().filter((n) => !n.cell), fresh('m'), k);
      hint = r.why ? `Convert to matrix: ${r.why}` : `matrix (${r.matrix.name}) made of ${r.matrix.cells.length} nodes${(R.libraries ?? []).includes('matrix') ? '' : ', loads the matrix library'}`;
      sel = null;
      draw(true);
      if (r.matrix) done();
    };
    const trash = el('button', 'tv-button tv-danger sd-icon', TRASH);
    trash.type = 'button';
    trash.title = 'Delete';
    trash.ariaLabel = 'Delete';
    trash.onclick = () => sel && remove(sel);
    bar.append(modes, arrows, grid, matrix, ...(H?.controls?.() ?? []), trash);
    const svg = svgEl('svg', { class: 'tv-canvas', role: 'img', 'aria-label': 'drawing', tabindex: '0' });
    const side = el('div', 'tv-inspect');
    const layerRow = el('div', 'tv-row tv-layers');
    const kept = el('div', 'tv-kept');
    const status = el('span', 'tv-muted');
    // where a pointer is, in cm
    const at = (e) => { const p = svg.createSVGPoint(); p.x = e.clientX; p.y = e.clientY; const q = p.matrixTransform(svg.getScreenCTM().inverse()); return [q.x, -q.y]; };
    const fresh = (base) => { let i = 1; while (nodes().some((n) => n.name === `${base}${i}`) || mats().has(`${base}${i}`)) i++; return `${base}${i}`; };
    const add = (it) => { state.items.push({ dirty: true, lead: '', opts: [], label: '', layer: 'main', ...it }); };
    const pick = (n) => {
      if (tool === 'connect') {
        if (!pending) { pending = n.name; return draw(); }
        if (pending !== n.name) { add({ type: 'edge', from: pending, to: n.name, fromAt: '', toAt: '', arrow, at: '' }); done(); }
        pending = null;
        return draw(true);
      }
      sel = n;
      draw();
      fields();
    };
    const remove = (it) => {
      if (it.fixed) return;
      it.deleted = true;
      if (it.parent) it.parent.dirty = true;
      if (it.type === 'node') for (const e of live()) if (e.type === 'edge' && (e.from === it.name || e.to === it.name)) e.deleted = true;
      sel = null;
      draw(true);
      fields();
      done();
    };
    root.onkeydown = (e) => { if ((e.key === 'Delete' || e.key === 'Backspace') && sel && !/^(INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName)) { e.preventDefault(); remove(sel); } };
    svg.onpointerdown = (e) => {
      if (e.target !== svg && !e.target.classList.contains('tv-grid')) return;
      if (!SHAPE_OPTS[tool]) { sel = null; pending = null; draw(); return fields(); }
      const [x, y] = at(e).map(snap);
      add({ type: 'node', name: fresh('n'), label: tool === 'text' ? 'Text' : 'Node', opts: [...SHAPE_OPTS[tool]], at: true, x, y });
      sel = state.items[state.items.length - 1];
      if (relMode()) hint = relate(sel).hint;
      tool = 'select';
      draw(true);
      fields();
      side.querySelector('input')?.select();
      done();
    };
    // a pointer gesture on the canvas: `move([x, y])` while it lasts, `up()` at its end
    const gesture = (e, move, up) => {
      try { svg.setPointerCapture(e.pointerId); } catch { /* a synthetic pointer */ }
      svg.onpointermove = (ev) => move(at(ev));
      svg.onpointerup = () => { svg.onpointermove = svg.onpointerup = null; hint = ''; anchorsOf = null; up(); draw(true); fields(); };
    };
    /** `n` placed relative to the nearest node before it, where it stands now (its `at` the fallback). */
    const relate = (n) => {
      const by = new Map(nodes().map((o) => [o.name, o]));
      const earlier = own().filter((o) => !o.cell && o !== n && state.items.indexOf(o) < state.items.indexOf(n));
      const r = relocate({ ...n, place: null }, n.x, n.y, { ...env(), by, earlier });
      Object.assign(n, r);
      touch(n);
      return r;
    };
    const drag = (g, n) => g.onpointerdown = (e) => {
      e.stopPropagation();
      if (n.fixed || tool !== 'select') return pick(n);
      sel = n;
      const m = n.parent;
      if (m && m.geometry === 'run') { draw(); return fields(); }
      const [x0, y0] = at(e), it = m ?? n, [nx, ny] = [it.x, it.y], before = { opts: [...n.opts], place: n.place, at: n.at };
      let moved = false;
      gesture(e, ([x, y]) => {
        const tx = nx + x - x0, ty = ny + y - y0;
        if (!moved && Math.hypot(x - x0, y - y0) < 0.05) return;
        moved = true;
        if ((n.place || relMode()) && !m) {
          // relative: the smallest edit of its placement
          const by = new Map(nodes().map((o) => [o.name, o]));
          const earlier = own().filter((o) => state.items.indexOf(o.parent ?? o) < state.items.indexOf(n));
          Object.assign(n, before);
          const r = relocate(n, tx, ty, { ...env(), by, earlier });
          Object.assign(n, r);
          hint = r.hint;
        } else {
          // a node TikZ puts at the origin, unplaced: a drag gives it `at`
          it.x = snap(tx); it.y = snap(ty); it.at = true;
          hint = `at (${fmt(it.x / k)}, ${fmt(it.y / k)})`;
        }
        touch(n);
        draw();
      }, () => { if (moved) done(); });
      draw();
      fields();
    };
    // an arrow's end dragged onto a node's anchor
    const endDrag = (h, e, which) => h.onpointerdown = (ev) => {
      ev.stopPropagation();
      let to = null;
      gesture(ev, ([x, y]) => {
        const near = nodes().map((n) => [n, Math.max(Math.abs(n.x - x) - size(n)[0] / 2, Math.abs(n.y - y) - size(n)[1] / 2)]).sort((a, b) => a[1] - b[1])[0];
        to = near && near[1] < 0.4 ? [near[0].name, nearestAnchor(near[0], size(near[0]), round(near[0]), [x, y])] : null;
        anchorsOf = to && near[0];
        hint = to ? `${which === 'from' ? 'from' : 'to'} (${to[0]}.${to[1]})` : 'drop on a node';
        draw();
      }, () => {
        if (!to) return;
        [e[which], e[`${which}At`]] = [to[0], to[1]];
        e.dirty = true;
        done();
      });
    };
    // the markers of the tips in use, by tip and color
    const markers = new Map();
    // `bent`: the tip's straight shaft is clipped to the tip, else it sticks out of a curve
    const marker = (defs, tip, color, start, bent = false) => {
      // a start tip is the end tip drawn backwards: `<` at the start is `>` turned round
      let name = start ? tip.replace(/[<>]/g, (c) => (c === '<' ? '>' : '<')) : tip;
      if (name === '>' && tipDefault) name = tipDefault;
      const key = `${name}|${color}|${bent}`;
      if (!markers.has(key)) {
        const t = (R.tips ?? []).find((x) => x.name === name)?.svg, m = t && tipMarker(t), mid = `tv-tip-${markers.size}`;
        const e = m ? svgEl('marker', { id: mid, viewBox: m.box.join(' '), refX: m.ref[0], refY: m.ref[1], markerUnits: 'userSpaceOnUse', markerWidth: m.box[2] * PT, markerHeight: m.box[3] * PT, orient: 'auto-start-reverse', overflow: bent ? 'hidden' : 'visible' })
          : svgEl('marker', { id: mid, viewBox: '0 0 10 10', refX: 9, refY: 5, markerWidth: 5, markerHeight: 5, orient: 'auto-start-reverse' });
        e.innerHTML = m ? m.inner : '<path d="M0 0L10 5L0 10z" fill="currentColor"/>';
        e.style.color = color;
        markers.set(key, e);
      }
      defs.append(markers.get(key));
      return `url(#${markers.get(key).id})`;
    };
    // a path's look from its options, styles followed: color, width (cm), dashes, bend (degrees, left positive)
    const isColor = (key) => colors.has(key) || !!mix(key, defined);
    const stroke = (list) => {
      let color = null, width = null, dash = null, bend = null, opacity = null;
      for (const o of expand(list)) {
        const [key, v] = kv(o);
        if ((key === 'draw' || key === 'color') && v && v !== 'none') color = css(v);
        else if (!v && isColor(key)) color = css(key);
        else if (key === 'line width') width = cm(v) ?? width;
        else if (key === 'dash pattern') dash = [...v.matchAll(/(?:on|off)\s+([^\s]+)/g)].map((m) => cm(m[1]) ?? 0).join(' ') || null;
        else if (key === 'solid') dash = null;
        else if (key === 'bend left' || key === 'bend right') bend = (key === 'bend left' ? 1 : -1) * (v === '' ? 30 : +v || 30);
        else if (key === 'opacity' || key === 'draw opacity') opacity = +v;
      }
      return { color, width, dash, bend, opacity };
    };
    function draw(refit = false) {
      settle();
      for (const b of modes.querySelectorAll('[data-tool]')) { b.classList.toggle('on', b.dataset.tool === tool); b.ariaPressed = String(b.dataset.tool === tool); }
      trash.disabled = !sel || sel.fixed;
      const all = nodes(), by = new Map(all.map((n) => [n.name, n]));
      if (refit || !box) {
        const xs = all.flatMap((n) => [n.x - size(n)[0] / 2, n.x + size(n)[0] / 2]), ys = all.flatMap((n) => [n.y - size(n)[1] / 2, n.y + size(n)[1] / 2]);
        for (const it of live()) for (const [x, y] of it.path ?? []) { xs.push(x * k); ys.push(y * k); }
        const lo = [Math.min(0, ...xs) - 1, Math.min(0, ...ys) - 1], hi = [Math.max(4, ...xs) + 1, Math.max(2, ...ys) + 1];
        let b = [Math.floor(lo[0]), Math.floor(lo[1]), Math.ceil(hi[0]) - Math.floor(lo[0]), Math.ceil(hi[1]) - Math.floor(lo[1])];
        // a refit only grows the view: what stays inside does not move under the pointer
        if (box && refit !== 'all') { const x0 = Math.min(b[0], box[0]), y0 = Math.min(b[1], box[1]); b = [x0, y0, Math.max(b[0] + b[2], box[0] + box[2]) - x0, Math.max(b[1] + b[3], box[1] + box[3]) - y0]; }
        // at least 4:3, widened on both sides, so the grid fills the canvas
        const wide = Math.ceil(Math.max(0, b[3] * 4 / 3 - b[2]) / 2);
        box = [b[0] - wide, b[1], b[2] + 2 * wide, b[3]];
        svg.setAttribute('viewBox', `${box[0]} ${-box[1] - box[3]} ${box[2]} ${box[3]}`);
      }
      const lines = [];
      if (step()) {
        for (let x = box[0]; x <= box[0] + box[2] + 1e-9; x += step()) lines.push(`M${x} ${-box[1]}V${-box[1] - box[3]}`);
        for (let y = box[1]; y <= box[1] + box[3] + 1e-9; y += step()) lines.push(`M${box[0]} ${-y}H${box[0] + box[2]}`);
      }
      const defs = svgEl('defs'), out = [svgEl('path', { class: 'tv-grid', d: lines.join('') || 'M0 0' }), defs];
      const base = stroke(pathStyle), lw = cm(pic.get('line width'));
      // the plain paths kept as code
      for (const it of live()) if (it.path) out.push(svgEl('polyline', { class: 'tv-path', points: it.path.map(([x, y]) => `${x * k},${-y * k}`).join(' ') }));
      const shown = (i) => !hidden.has(layerOf(i));
      // in layer order, each layer in the text's order
      const items = [...live().filter((i) => i.type === 'edge'), ...all].map((i, n) => [i, n]).sort((a, b) => rank(a[0]) - rank(b[0]) || a[1] - b[1]).map(([i]) => i);
      handles.length = 0;
      for (const it of items.filter(shown)) out.push(it.type === 'edge' ? edgeG(it, by, defs, base, lw) : nodeG(it));
      // the selected arrow's end handles over the nodes they sit on
      out.push(...handles);
      if (anchorsOf) for (const a of Object.keys(COMPASS)) { const [x, y] = anchorAt(anchorsOf, size(anchorsOf), round(anchorsOf), a); out.push(svgEl('circle', { class: 'tv-anchor', cx: x, cy: -y, r: 0.06 })); }
      if (hint && sel && !sel.type?.startsWith('edge')) out.push(Object.assign(svgEl('text', { class: 'tv-hint', x: sel.x, y: -sel.y - size(sel)[1] / 2 - 0.25 }), { textContent: hint }));
      svg.replaceChildren(...out);
      const changed = state.items.filter((i) => i.dirty || i.deleted || i.moved).length + (picDirty ? 1 : 0) + docStyles.filter((d) => d.isNew || d.deleted || d.body !== d.orig).length;
      status.textContent = hint || (changed ? `${changed} change${changed > 1 ? 's' : ''} to apply` : tool === 'connect' ? (pending ? `from (${pending}): click the target node` : 'Click the node an arrow starts at') : tool === 'select' ? 'Drag a node to move it. Click one to edit it.' : 'Click the canvas to place a node');
      write.disabled = !changed && !s.wrap;
      layersRow();
      mermaid();
    }
    function edgeG(e, by, defs, base, lw) {
      const a = by.get(e.from), b = by.get(e.to), g = svgEl('g', { class: `tv-edge${sel === e ? ' on' : ''}` });
      if (!a || !b) return g;
      const look = { ...base, ...Object.fromEntries(Object.entries(stroke([...e.opts, ...opts(e.via ?? '')])).filter(([, v]) => v != null)) };
      if (e.via == null && !bends(e.opts)) look.bend = null;
      // `bend left=a`: out at a to the chord, in at 180-a, control points 0.3915 of the chord away (TikZ's looseness 1)
      const turn = (look.bend ?? 0) * Math.PI / 180, chord = Math.atan2(b.y - a.y, b.x - a.x);
      const away = (n, ang) => [n.x + Math.cos(ang), n.y + Math.sin(ang)];
      // from border to border, or from the anchors the text names
      const cut = (p, q) => { const [w, h] = size(p), dx = q[0] - p.x, dy = q[1] - p.y, t = Math.min(Math.abs(w / 2 / (dx || 1e-9)), Math.abs(h / 2 / (dy || 1e-9)), turn ? Infinity : 0.5); return [p.x + dx * t, p.y + dy * t]; };
      const fa = e.fromAt && anchorAt(a, size(a), round(a), e.fromAt), ta = e.toAt && anchorAt(b, size(b), round(b), e.toAt);
      const p = fa || cut(a, turn ? away(a, chord + turn) : ta || [b.x, b.y]), q = ta || cut(b, turn ? away(b, chord + Math.PI - turn) : fa || [a.x, a.y]);
      const d = Math.hypot(q[0] - p[0], q[1] - p[1]) * 0.3915;
      const c1 = [p[0] + d * Math.cos(chord + turn), p[1] + d * Math.sin(chord + turn)], c2 = [q[0] + d * Math.cos(chord + Math.PI - turn), q[1] + d * Math.sin(chord + Math.PI - turn)];
      const path = turn ? `M${p[0]} ${-p[1]}C${c1[0]} ${-c1[1]} ${c2[0]} ${-c2[1]} ${q[0]} ${-q[1]}` : `M${p[0]} ${-p[1]}L${q[0]} ${-q[1]}`;
      const mid = turn ? [0, 1].map((i) => (p[i] + 3 * c1[i] + 3 * c2[i] + q[i]) / 8) : [(p[0] + q[0]) / 2, (p[1] + q[1]) / 2];
      const color = sel === e ? 'var(--tv-accent)' : look.color ?? 'currentColor';
      const line = svgEl('path', { d: path });
      if (look.color) line.style.stroke = look.color;
      if (look.width ?? lw) line.style.strokeWidth = Math.max(look.width ?? lw, 0.012);
      if (look.dash) line.style.strokeDasharray = look.dash;
      if (look.opacity != null) line.style.opacity = look.opacity;
      // tips written in the arrow's options, else those a style of it brings (`flow/.style={-Stealth}`)
      const [st, en] = tipsOf(e.arrow !== '-' ? e.arrow : expand(e.opts).findLast(tipsOf) ?? '-') ?? ['', ''];
      if (en) line.setAttribute('marker-end', marker(defs, en, color, false, !!turn));
      if (st) line.setAttribute('marker-start', marker(defs, st, color, true, !!turn));
      const hit = svgEl('path', { d: path, class: 'tv-hit' });
      g.append(hit, line);
      if (e.label) g.append(Object.assign(svgEl('text', { x: mid[0], y: -mid[1] - 0.15 }), { textContent: plain(e.label) }));
      g.onpointerdown = (ev) => { ev.stopPropagation(); sel = e; draw(); fields(); };
      if (sel === e) for (const [which, [x, y]] of [['from', p], ['to', q]]) {
        const h = svgEl('circle', { class: 'tv-handle', cx: x, cy: -y, r: 0.09 });
        h.append(Object.assign(svgEl('title'), { textContent: `Drag onto an anchor of a node` }));
        endDrag(h, e, which);
        handles.push(h);
      }
      return g;
    }
    const handles = [];
    function nodeG(n) {
      const look = lookOf(n), [w, h] = size(n), shape = shapeName(n), o = new Map(look.map(kv));
      const drawn = (look.map(kv).findLast(([key]) => key === 'draw')?.[1] ?? 'none') !== 'none' || n.fixed;
      const g = svgEl('g', { class: `tv-node${n.fixed ? ' fixed' : ''}${drawn ? '' : ' bare'}${sel === n ? ' on' : ''}${pending === n.name ? ' pending' : ''}` });
      const rc = o.has('rounded corners') ? cm(o.get('rounded corners')) ?? 4 * PT : 0;
      const body = shape === 'circle' ? svgEl('circle', { cx: n.x, cy: -n.y, r: Math.max(w, h) / 2 })
        : shape === 'ellipse' ? svgEl('ellipse', { cx: n.x, cy: -n.y, rx: w / 2 * Math.SQRT2, ry: h / 2 * Math.SQRT2 })
        : shape === 'diamond' ? svgEl('polygon', { points: `${n.x - w},${-n.y} ${n.x},${-n.y - h} ${n.x + w},${-n.y} ${n.x},${-n.y + h}` })
        : svgEl('rect', { x: n.x - w / 2, y: -n.y - h / 2, width: w, height: h, rx: rc });
      if (o.get('fill')) body.style.fill = css(o.get('fill'));
      if (o.get('draw')) body.style.stroke = css(o.get('draw'));
      g.append(body);
      const t = setLabel(svgEl('text', { x: n.x, y: -n.y }), n.coordinate ? '' : n.label);
      const f = (o.get('font') ?? font).match(/\\(\w+)/g) ?? [];
      for (const cs of f.map((x) => x.slice(1))) {
        if (FONT[cs]) t.style.fontSize = `${0.32 * FONT[cs]}px`;
        if (cs === 'bfseries') t.style.fontWeight = 'bold';
        if (cs === 'itshape') t.style.fontStyle = 'italic';
        if (cs === 'ttfamily') t.style.fontFamily = 'ui-monospace, monospace';
        if (cs === 'sffamily') t.style.fontFamily = 'ui-sans-serif, sans-serif';
      }
      const text = o.get('text') ?? look.map(kv).findLast(([key, v]) => !v && colors.has(key))?.[0];
      // on a fill, black text as TikZ sets it, white on a dark one, in either theme
      const bg = /^#[0-9a-f]{6}$/i.test(body.style.fill ? css(o.get('fill')) : '') && PARSE.HTML(css(o.get('fill')));
      if (text) t.style.fill = css(text);
      else if (bg) t.style.fill = 0.3 * bg[0] + 0.59 * bg[1] + 0.11 * bg[2] > 0.45 ? '#000' : '#fff';
      g.append(t);
      if (n.fixed) g.append(Object.assign(svgEl('title'), { textContent: `${n.name}: made by code, kept as it is` }));
      else if (!drawn) g.append(Object.assign(svgEl('title'), { textContent: `${n.name}: no outline drawn (no draw key), the faint frame shows its size` }));
      drag(g, n);
      return g;
    }
    // the layers the run declared, in the order pgf stacks them: shown or hidden
    function layersRow() {
      layerRow.hidden = layers.length < 2;
      if (layerRow.hidden || layerRow.childElementCount) return layerRow.querySelectorAll('input').forEach((i) => { i.checked = !hidden.has(i.name); });
      layerRow.append(el('span', 'tv-muted', 'Layers'));
      for (const l of layers) {
        const off = !order.includes(l);
        const lab = el('label', `tv-chip${off ? ' tv-off' : ''}`, `<input type="checkbox" name="${esc(l)}" checked> ${esc(l)}`);
        lab.title = off ? `${l} is declared but not in \\pgfsetlayers: pgf does not draw it` : `Show or hide what is on ${l}`;
        lab.firstChild.onchange = (e) => { if (e.target.checked) hidden.delete(l); else hidden.add(l); draw(); };
        layerRow.append(lab);
      }
    }
    // the property editor (dialogs.js `properties`) over a list of options, completed from the run's keys
    const D = self.satexDialogs;
    const keyList = () => {
      const have = new Set((R.keys ?? []).map((x) => x.name));
      const more = [...picStyles(), ...docStyles.filter((d) => !d.deleted)].filter((x) => !have.has(x.name)).map((x) => ({ name: x.name, type: 'flag' }));
      return [...(R.keys ?? []), ...more];
    };
    const props = (o) => D?.properties?.({ keys: keyList(), colors: [...colors.keys()], nodes: nodes().map((n) => n.name), css, commit: done, ...o }) ?? el('span');
    // the selection's inspector: a node's text and shape, an arrow's label, tips and anchors, the
    // layer of either; every other key in the property editor
    function fields() {
      side.replaceChildren();
      const it = sel;
      if (!it || it.deleted || it.fixed || it.coordinate) {
        side.append(el('span', 'tv-muted', it?.fixed ? `(${esc(it.name)}) is made by code: edit it in the source` : 'Select a node or an arrow to edit it.'));
        return styles();
      }
      const grid = el('div', 'tv-form tv-inspector');
      const row = (label, input) => { const l = el('label', 'tv-field', `<span>${esc(label)}</span>`); l.append(input); grid.append(l); return input; };
      const input = (value, placeholder, set) => {
        const i = Object.assign(el('input', 'tv-input'), { value: value ?? '', placeholder, spellcheck: false });
        i.oninput = () => { if (!balanced(i.value)) return i.classList.add('tv-bad'); i.classList.remove('tv-bad'); set(i.value); touch(it); draw(); };
        i.onchange = done;
        return i;
      };
      const select = (choices, value, set) => {
        const i = el('select', 'tv-input', choices.map(([v, l]) => `<option value="${esc(v)}"${v === value ? ' selected' : ''}>${esc(l)}</option>`).join(''));
        i.onchange = () => { set(i.value); touch(it); draw(); done(); };
        return i;
      };
      const isShape = (o) => { const [key, v] = kv(o); return key === 'shape' || (!v && shapes.includes(key)); };
      const handled = (o) => isShape(o) || o === it.place?.opt || tipsOf(o) || SHIFTS.includes(kv(o)[0]);
      const head = el('div', 'tv-row');
      head.append(el('b', 'tv-muted', esc(it.type === 'edge' ? `${it.from} ${it.arrow === '-' ? '--' : it.arrow} ${it.to}` : `(${it.name})${it.parent ? ` in matrix (${it.parent.name})` : ''}`)));
      if (it.place) head.append(el('span', 'tv-muted', esc(`placed ${it.place.opt}: drag to change`)));
      side.append(head, grid);
      if (it.type === 'edge') {
        row('label', input(it.label, 'label on the arrow', (v) => { it.label = v; }));
        const lat = row('label at', input(it.label ? it.at || 'above' : it.at, 'above, below left, sloped, near end', (v) => { it.at = v.trim(); }));
        lat.setAttribute('list', 'tv-label-at');
        grid.append(el('datalist', '', ['above', 'below', 'left', 'right', 'above left', 'above right', 'below left', 'below right', 'midway', 'near start', 'near end', 'sloped, above', 'swap'].map((w) => `<option value="${w}">`).join('')));
        grid.lastChild.id = 'tv-label-at';
        const [st, en] = tipsOf(it.arrow) ?? ['', ''];
        const tip = (which, value) => {
          const wrap = el('span', 'tv-row');
          const sel2 = select([], value, (v) => { const t = tipsOf(it.arrow) ?? ['', '']; t[which] = v; it.arrow = `${t[0]}-${t[1]}`; fields(); });
          sel2.innerHTML = tipOptions(value);
          wrap.append(sel2);
          const svg2 = (R.tips ?? []).find((t) => t.name === (value === '>' && tipDefault ? tipDefault : value))?.svg;
          if (svg2) wrap.append(Object.assign(el('img', 'tv-tip-preview'), { src: `data:image/svg+xml;utf8,${encodeURIComponent(svg2)}`, alt: value, style: which ? '' : 'transform: scaleX(-1)' }));
          return wrap;
        };
        row('start tip', tip(0, st));
        row('end tip', tip(1, en));
        row('anchors', el('span', 'tv-muted', esc(`${it.fromAt || 'border'} → ${it.toAt || 'border'}: drag an end onto a node's anchor`)));
      } else {
        row('text', input(it.label, 'text', (v) => { it.label = v; }));
        const sh = it.opts.map(kv).find(([key, v]) => key === 'shape' || (!v && shapes.includes(key)));
        row('shape', select(shapes.map((x) => [x, x]), sh ? sh[1] || sh[0] : 'rectangle', (v) => {
          const i = it.opts.findIndex(isShape), list = it.opts.filter((_, j) => j !== i);
          // rectangle is the default: no key for it
          if (v !== 'rectangle') list.splice(i < 0 ? list.length : i, 0, v);
          it.opts = list;
          fields();
        }));
        if (!it.parent) {
          // a new node beside this one, placed relative to it (positioning's `right=of`, else TikZ's `right of=`)
          const beside = el('span', 'tv-row tv-beside');
          for (const [dir, arrowGlyph] of [['left', '←'], ['above', '↑'], ['below', '↓'], ['right', '→']]) {
            const b = el('button', 'tv-button', arrowGlyph);
            b.type = 'button';
            b.title = `Add a node ${dir} of (${it.name})`;
            b.ariaLabel = b.title;
            b.onclick = () => addBeside(it, dir);
            beside.append(b);
          }
          row('add beside', beside);
        }
      }
      if (layers.length > 1) row('layer', select(layers.map((l) => [l, l]), layerOf(it), (v) => { toLayer(it.parent ?? it, v); }));
      side.append(el('h4', 'tv-h', 'Properties'), props({ get: () => it.opts, set: (l) => { it.opts = l; touch(it); draw(); }, skip: handled }));
      styles();
    }
    /** A node `dir` of `n`, its look copied: positioning's `dir=of n` when loaded, else `dir of=n`. */
    const addBeside = (n, dir) => {
      const opt = (R.libraries ?? []).includes('positioning') ? `${dir}=of ${n.name}` : `${dir} of=${n.name}`;
      const look = n.opts.filter((o) => o !== n.place?.opt && !SHIFTS.includes(kv(o)[0]) && kv(o)[0] !== 'node distance');
      add({ type: 'node', name: fresh('n'), label: 'Node', opts: [opt, ...look], at: false, x: n.x, y: n.y, place: { ...placement(opt), opt } });
      sel = state.items.at(-1);
      draw(true);
      fields();
      side.querySelector('input')?.select();
      done();
    };
    // what stays code: listed read-only, never dropped
    const code = state.items.filter((i) => i.type === 'code' && i.code.trim());
    for (const it of code) { const m = it.code.match(PATH); if (m) { const pts = [...m[1].matchAll(/\(\s*(-?[\d.]+)\s*(?:cm)?\s*,\s*(-?[\d.]+)\s*(?:cm)?\s*\)/g)].map((p) => [+p[1], +p[2]]); if (pts.length === m[1].split('--').length) it.path = pts; } }
    if (code.length) kept.innerHTML = `<h4 class="tv-h">Kept as code (${code.length})</h4>` + code.map((i) => `<pre>${esc(i.code.length > 160 ? i.code.slice(0, 160) + '…' : i.code)}</pre>`).join('');
    // the picture's options: written into \begin{tikzpicture}[…], applied as typed
    const canPic = !!(s.head || s.wrap);
    const settings = el('details', 'tv-mermaid', '<summary>Picture options</summary>');
    const picChanged = () => { picDirty = true; applyPic(); draw(); };
    const picSet = (l) => { pic.list.splice(0, pic.list.length, ...l.map((o) => [...kv(o), o])); picChanged(); };
    // relative placement: a toggle each viewer keeps, and converting the nodes at `at` once
    const relRow = el('div', 'tv-row tv-relative');
    const relBox = Object.assign(el('label', 'tv-muted', '<input type="checkbox"> place nodes relative to a neighbour'), { title: 'Moved and new nodes are written as right=1cm of a (positioning), not at (x,y)' });
    relBox.firstChild.checked = relMode();
    relBox.firstChild.onchange = () => { pref.relative = relBox.firstChild.checked; saveGrid(); };
    const relAll = el('button', 'tv-button', 'Make all relative');
    relAll.type = 'button';
    relAll.title = 'Write every node at (x,y) relative to the nearest node before it, distances on the grid';
    relAll.onclick = () => {
      const absolute = own().filter((n) => !n.cell && n.at);
      let made = 0;
      for (const n of absolute) if (!relate(n).at) made++;
      draw(true);
      fields();
      done();
      status.textContent = `${made} of ${absolute.length} node${absolute.length === 1 ? '' : 's'} at (x,y) now relative${made < absolute.length ? '; the rest have no node before them to refer to' : ''}`;
    };
    relRow.append(relBox, relAll);
    const picture = () => {
      settings.replaceChildren(settings.firstChild, relRow);
      if (!canPic) return settings.append(el('div', 'tv-muted', 'The options span lines: edit them in the source.'));
      settings.append(props({ get: () => pic.list.map((e) => e[2]), set: picSet, skip: (o) => STYLE_DEF.test(kv(o)[0]), label: 'Add a picture option' }));
    };
    picture();
    // named styles: the picture's (`box/.style={…}` in its options) and the document's (`\tikzset`), each
    // edited with the property editor, applied to the selection by name, new ones in either place
    const stylesBox = el('details', 'tv-mermaid tv-styles', '<summary>Styles</summary>');
    let openStyle = null;
    function styles() {
      const list = [...picStyles().map((p) => ({ ...p, where: 'picture' })), ...docStyles.filter((d) => !d.deleted).map((d) => ({ ...d, where: 'document' }))];
      const target = sel && !sel.fixed && !sel.deleted && !sel.coordinate ? sel : null;
      const rows = list.map((st) => {
        const r = el('div', 'tv-style' + (openStyle === `${st.where}:${st.name}` ? ' on' : ''));
        const head = el('div', 'tv-row');
        const name = el('button', 'tv-choice', `<b>${esc(st.name)}</b><span>${esc(st.body || 'no keys')}</span>`);
        name.type = 'button';
        name.title = `Edit the keys of ${st.name}`;
        name.onclick = () => { openStyle = openStyle === `${st.where}:${st.name}` ? null : `${st.where}:${st.name}`; styles(); };
        const has = target?.opts.includes(st.name);
        const use = el('button', 'tv-button', has ? 'Remove' : 'Apply');
        use.type = 'button';
        use.disabled = !target || STYLE_EVERY.test(st.name);
        use.title = !target ? 'Select a node or an arrow first' : has ? `Take ${st.name} off the selection` : `Add ${st.name} to the selection's options`;
        use.onclick = () => { target.opts = has ? target.opts.filter((o) => o !== st.name) : [...target.opts, st.name]; touch(target); draw(); fields(); done(); };
        const del = el('button', 'tv-button tv-danger sd-icon', TRASH);
        del.type = 'button';
        del.title = `Delete the style ${st.name}`;
        del.ariaLabel = del.title;
        del.onclick = () => {
          if (st.where === 'picture') picSet(pic.list.map((e) => e[2]).filter((o) => kv(o)[0] !== st.key));
          else { const d = docStyles.find((x) => x.name === st.name); if (d.isNew) docStyles = docStyles.filter((x) => x !== d); else d.deleted = true; applyPic(); draw(); }
          done();
          styles();
        };
        head.append(name, el('span', 'tv-badge', st.where), use, del);
        r.append(head);
        if (openStyle === `${st.where}:${st.name}`) {
          const setBody = (l) => {
            const body = l.join(', ');
            if (st.where === 'picture') { pic.set(st.key, body); picChanged(); }
            else { docStyles.find((x) => x.name === st.name).body = body; applyPic(); draw(); }
            name.querySelector('span').textContent = body || 'no keys';
          };
          r.append(props({ get: () => opts((st.where === 'picture' ? picStyles() : docStyles).find((x) => x.name === st.name)?.body ?? ''), set: setBody, label: `Add a key to ${st.name}` }));
        }
        return r;
      });
      // a new style: a name, where it goes, its keys from the selection's own keys if one is selected
      const add = el('div', 'tv-row tv-style-new');
      const nm = Object.assign(el('input', 'tv-input'), { placeholder: 'new style name', spellcheck: false, ariaLabel: 'new style name' });
      const where = el('select', 'tv-input', `${canPic ? '<option value="picture">in this picture</option>' : ''}${R.load_at ? '<option value="document">in the preamble (\\tikzset)</option>' : ''}`);
      where.ariaLabel = 'where the style goes';
      const make = el('button', 'tv-button', target ? 'Create from selection' : 'Create');
      make.type = 'button';
      make.title = target ? 'A style of the selection\'s keys (not its shape or place); the selection then uses it' : 'A new style with no keys yet';
      make.disabled = !where.options.length;
      make.onclick = () => {
        const n = nm.value.trim();
        const taken = list.some((x) => x.name === n) || (R.keys ?? []).some((x) => x.name === n && !x.style);
        if (!/^[A-Za-z][\w -]*$/.test(n) || taken) { nm.classList.add('tv-bad'); nm.title = taken ? `${n} is a key already` : 'letters, digits, spaces and dashes'; return; }
        const own = target ? target.opts.filter((o) => !(kv(o)[0] === 'shape' || shapes.includes(o) || o === target.place?.opt || tipsOf(o) || SHIFTS.includes(kv(o)[0]))) : [];
        const body = own.join(', ');
        if (where.value === 'picture') { pic.set(`${n}/.style`, body); picDirty = true; }
        else docStyles = [...docStyles, { name: n, body, orig: null, isNew: true }];
        if (target) { target.opts = [...target.opts.filter((o) => !own.includes(o)), n]; touch(target); }
        openStyle = `${where.value}:${n}`;
        applyPic();
        draw();
        fields();
        done();
      };
      nm.onkeydown = (e) => { nm.classList.remove('tv-bad'); if (e.key === 'Enter') { e.preventDefault(); make.click(); } };
      add.append(nm, where, make);
      stylesBox.replaceChildren(stylesBox.firstChild, ...(list.length ? rows : [el('div', 'tv-muted', 'No named styles yet. A style is a set of keys under a name: apply it to many nodes, change it once.')]), add);
      stylesBox.firstChild.textContent = `Styles${list.length ? ` (${list.length})` : ''}`;
    }
    // Mermaid: the drawing as a flowchart, live; a flowchart typed or pasted imports
    const mer = el('details', 'tv-mermaid', '<summary>Mermaid flowchart</summary>');
    const text = Object.assign(el('textarea', 'tv-input'), { rows: 6, spellcheck: false, ariaLabel: 'Mermaid flowchart' });
    let typed = false;
    text.oninput = () => { typed = true; };
    const loss = el('div', 'tv-muted');
    const mrow = el('div', 'tv-actions');
    const button = (label, title, f) => { const b = el('button', 'tv-button', label); b.type = 'button'; b.title = title; b.onclick = f; mrow.append(b); };
    const take = (replace) => () => { if (importMermaid(state, text.value, s.spacing || 2, replace).nodes.length) { typed = false; sel = null; draw(true); fields(); done(); } };
    button('Import, replace', 'Replace the nodes and arrows by the flowchart', take(true));
    button('Import, merge', 'Add the flowchart: nodes with a name already here are kept', take(false));
    button('Copy', 'Copy the flowchart', () => { if (typed) { typed = false; mermaid(); } navigator.clipboard?.writeText(text.value); text.select(); });
    function mermaid() {
      const edges = live().filter((i) => i.type === 'edge');
      if (!typed) text.value = mermaidOut(nodes(), edges);
      const lost = mermaidLoss(own(), edges, code.length);
      loss.textContent = lost.length ? `Not in the flowchart: ${lost.join(', ')}.` : '';
    }
    mer.append(text, loss, mrow);
    /** The preamble's new lines (the matrix library, new document styles) and the document's styles edited in place. */
    const preamble = () => {
      const made = docStyles.filter((d) => d.isNew && !d.deleted).map((d) => `${d.name}/.style={${d.body}}`);
      // the libraries the drawing now needs: matrix, positioning for `right=of`
      const need = [['matrix', live().some((i) => i.type === 'matrix')], ['positioning', own().some((n) => n.place && !n.place.centers)]].filter(([l, used]) => used && !(R.libraries ?? []).includes(l)).map(([l]) => l);
      const text = (need.length ? `\\usetikzlibrary{${need.join(',')}}\n` : '') + (made.length ? `\\tikzset{${made.join(', ')}}\n` : '');
      // a style deleted goes with its comma
      const restyled = docStyles.filter((d) => !d.isNew && d.range && (d.deleted || d.body !== d.orig)).map((d) => (d.deleted ? { range: d.entry, newText: '' } : { range: d.range, newText: d.body }));
      return { text: R.load_at ? text : '', restyled };
    };
    const writeIt = () => {
      const body = emit(state, k), o = pic.text() ? `[${pic.text()}]` : '', { text, restyled } = preamble();
      const lib = text ? [{ range: { start: R.load_at, end: R.load_at }, newText: text }] : [];
      // with tikz loaded by this edit too: the preamble's lines after it, in one insert
      const pre = s.extra ? [{ ...s.extra, newText: s.extra.newText + text }] : lib;
      if (s.wrap) return ctx.apply([{ range: s.range, newText: `\\begin{tikzpicture}${o}${body}\n\\end{tikzpicture}` }, ...pre, ...restyled]);
      ctx.apply([picDirty && s.head ? { range: { start: s.head.start, end: s.range.end }, newText: o + body } : { range: s.range, newText: body }, ...lib, ...restyled]);
    };
    const { el: foot, apply: write } = footer(root, ctx, { status, apply: writeIt, label: s.wrap ? 'Insert drawing' : 'Apply', H });
    // the canvas and, beside it, a panel that scrolls on its own: tools and footer stay in view
    const panel = el('div', 'tv-draw-side');
    panel.append(side, layerRow, stylesBox, settings, mer, kept);
    if (s.note) panel.append(el('div', 'tv-text', ctx.markdown(s.note)));
    const main = el('div', 'tv-draw-main');
    main.append(svg, panel);
    root.append(bar, main);
    root.append(foot);
    draw(true);
    fields();
    return root;
  }
  RENDER.drawing = drawing;

  // trees (forest plugin): the bracket notation `[content, options [child] …]` after the
  // preamble (`for tree={…}, …`); untouched text comes back as it was
  /** Splits `s` at top-level commas, braces kept. */
  function splitTop(s) {
    const out = [];
    let depth = 0, from = 0;
    for (let i = 0; i < s.length; i++) {
      const c = s[i];
      if (c === '\\') i++;
      else if (c === '{') depth++;
      else if (c === '}') depth--;
      else if (c === ',' && depth === 0) { out.push(s.slice(from, i)); from = i + 1; }
    }
    out.push(s.slice(from));
    return out.map((o) => o.trim()).filter(Boolean);
  }
  /** A node's head `content, options` as its parts, the text around them kept. */
  function head(text) {
    let depth = 0, comma = -1;
    for (let i = 0; i < text.length && comma < 0; i++) {
      const c = text[i];
      if (c === '\\') i++;
      else if (c === '{') depth++;
      else if (c === '}') depth--;
      else if (c === ',' && depth === 0) comma = i;
    }
    const part = comma < 0 ? text : text.slice(0, comma), rest = comma < 0 ? '' : text.slice(comma + 1);
    const lead = part.match(/^\s*/)[0], content = part.trim(), tail = (comma < 0 ? part : rest).match(/\s*$/)[0];
    return { lead, content, mid: comma < 0 ? '' : part.slice(lead.length + content.length), opts: comma < 0 ? [] : splitTop(rest), optsText: rest.slice(0, rest.length - tail.length), tail, hasOpts: comma >= 0 };
  }
  /** A forest body: `{pre, root, post}`, each node `{place, …head, children, inner}`; null without a tree. */
  function treeParse(body) {
    let i = 0;
    const skip = () => { // to the next top-level `[` or `]`, over groups, escapes and comments
      let depth = 0;
      for (; i < body.length; i++) {
        const c = body[i];
        if (c === '\\') i++;
        else if (c === '%') { while (i < body.length && body[i] !== '\n') i++; }
        else if (c === '{') depth++;
        else if (c === '}') depth--;
        else if ((c === '[' || c === ']') && depth === 0) return;
      }
    };
    const ws = (from) => { const m = body.slice(from, i).match(/\s*$/)[0]; return [body.slice(from, i - m.length), m]; };
    skip();
    if (body[i] !== '[') return null;
    const [pre, rootLead] = ws(0);
    const node = (place) => {
      i++; // `[`
      let from = i;
      skip();
      const [h, gap] = ws(from);
      const n = { place, ...head(h), children: [], inner: '' };
      let pending = gap;
      while (body[i] === '[') {
        n.children.push(node(pending));
        from = i;
        skip();
        if (body.slice(from, i).trim()) throw new Error(`text between nodes: ${body.slice(from, i).trim()}`);
        pending = body.slice(from, i);
      }
      if (body[i] !== ']') throw new Error('a node is not closed');
      // head + gap with no children: the gap is the head's own tail
      if (!n.children.length) { n.tail += pending; pending = ''; }
      n.inner = pending;
      i++;
      return n;
    };
    const root = node(rootLead);
    return { pre, root, post: body.slice(i) };
  }
  const braced = (s) => /[,[\]]/.test(s.replace(/\{[^{}]*\}/g, '')) ? `{${s}}` : s;
  /** A node's text: an untouched head as it was, an edited one written from its parts. */
  function nodeOut(n) {
    const h = n.dirty ? n.lead + braced(n.content) + (n.opts.length ? `${n.hasOpts ? n.mid : ''}, ${n.opts.join(', ')}` : '') + n.tail
      : n.lead + n.content + n.mid + (n.hasOpts ? ',' + n.optsText : '') + n.tail;
    return `${n.place ?? ''}[${h}${n.children.map(nodeOut).join('')}${n.inner}]`;
  }
  const treeEmit = (t) => t.pre + nodeOut(t.root) + t.post;
  /** A new or moved node's `place` (the white space before its `[`): as the sibling before it has it. */
  function settle(t) {
    const walk = (n, depth) => {
      n.children.forEach((c, k) => {
        if (c.place == null || c.moved) {
          const ok = (o) => o !== c && o.place != null && !o.moved;
          const other = n.children.slice(0, k).reverse().find(ok) ?? n.children.find(ok);
          c.place = other ? other.place : (n.inner.includes('\n') || t.root.place?.includes('\n') ? `\n${'  '.repeat(depth + 2)}` : ' ');
          c.moved = false;
        }
        walk(c, depth + 1);
      });
    };
    walk(t.root, 0);
  }
  const fresh = (content) => ({ place: null, lead: '', content, mid: '', opts: [], optsText: '', tail: '', hasOpts: false, children: [], inner: '', dirty: true });
  /** An indented outline (`-`, `*`, `+`, `1.` markers dropped) as nodes. */
  function outlineIn(src) {
    const top = { children: [] }, stack = [[-1, top]];
    for (const raw of src.split('\n')) {
      if (!raw.trim()) continue;
      const depth = raw.replace(/\t/g, '    ').match(/^ */)[0].length, text = raw.trim().replace(/^(?:[-*+]|\d+[.)])\s+/, '');
      while (stack.length > 1 && stack[stack.length - 1][0] >= depth) stack.pop();
      const n = fresh(text);
      stack[stack.length - 1][1].children.push(n);
      stack.push([depth, n]);
    }
    return top.children;
  }
  const outlineOut = (n, d = 0) => [`${'  '.repeat(d)}- ${n.content}`, ...n.children.map((c) => outlineOut(c, d + 1))].join('\n');
  // the store's moves: by path into the tree
  const parentOf = (t, x) => { const go = (n) => n.children.includes(x) ? n : n.children.map(go).find(Boolean); return go(t.root); };
  const within = (a, b) => a === b || a.children.some((c) => within(c, b));
  const texIssue = (text) => self.satexDialogs.texIssue(text.replace(/^\{([^]*)\}$/, '$1'));
  const size = (n) => 1 + n.children.reduce((k, c) => k + size(c), 0);
  /** Moves `n` before or after `target` (`where` = 'before' | 'after'), or under it ('child'). */
  function move(t, n, target, where) {
    if (within(n, target)) return false;
    const from = parentOf(t, n);
    if (!from) return false;
    from.children.splice(from.children.indexOf(n), 1);
    n.moved = true;
    if (where === 'child') target.children.push(n);
    else { const p = parentOf(t, target); if (!p) return false; p.children.splice(p.children.indexOf(target) + (where === 'after'), 0, n); }
    return true;
  }
  /** The `for tree={…}` option of the preamble: `[start, end]` of its braces' inside, or null. */
  function forTree(pre) {
    const m = /for\s+tree\s*=\s*\{/.exec(pre);
    if (!m) return null;
    let depth = 1, i = m.index + m[0].length;
    for (; i < pre.length && depth; i++) if (pre[i] === '\\') i++; else if (pre[i] === '{') depth++; else if (pre[i] === '}') depth--;
    return depth ? null : [m.index + m[0].length, i - 1];
  }
  function setForTree(t, text) {
    const at = forTree(t.pre);
    if (at) t.pre = t.pre.slice(0, at[0]) + text + t.pre.slice(at[1]);
    else if (text.trim()) {
      const lead = t.pre.match(/^\s*/)[0], rest = t.pre.slice(lead.length);
      t.pre = rest ? `${lead}for tree={${text}}, ${rest}` : `${t.root.place ?? '\n'}for tree={${text}}`;
    }
  }
  /** Leaves side by side, each `width(n)` wide, parents centered over their children, a level per row. */
  function treeLayout(root, width = () => 1) {
    const out = [];
    let leaf = 0;
    const walk = (n, depth) => {
      const kids = n.children.map((c) => walk(c, depth + 1));
      const x = kids.length ? (kids[0].x + kids[kids.length - 1].x) / 2 : (leaf += width(n)) - width(n) / 2;
      const it = { n, x, y: -depth };
      out.push(it);
      return it;
    };
    walk(root, 0);
    return out;
  }
  const forestFns = { splitTop, head, parse: treeParse, emit: treeEmit, settle, fresh, outlineIn, outlineOut, move, forTree, setForTree, layout: treeLayout };

  function forest(s, view, ctx) {
    const root = el('div', 'tv-draw');
    let t;
    try { t = s.wrap ? { pre: '\n  ', root: fresh('root'), post: '\n' } : treeParse(s.body); } catch (e) { t = null; root.append(el('div', 'tv-text', esc(`The tree reads as code only: ${e.message}`))); }
    if (!t) { if (!root.childElementCount) root.append(el('div', 'tv-text', 'No bracket tree in this environment.')); return root; }
    if (s.wrap) t.root.place = '';
    // what forest computed, in creation order, which is the bracket order
    const ran = () => { const order = []; const pre = (n) => { order.push(n); n.children.forEach(pre); }; pre(t.root);
      return (s.nodes ?? []).length === order.length && s.nodes.every((r, k) => r.content === order[k].content) ? new Map(order.map((n, k) => [n, s.nodes[k]])) : new Map(); };
    let run = ran();
    let sel = t.root, dragging = null;
    // undo: the shared history of the tree's data; a step back selects the root
    let ftRow = null;
    const H = self.satexDialogs.history(() => t, (v) => {
      t = v; sel = t.root; run = ran(); draw();
      const ft = forTree(t.pre);
      if (ftRow) ftRow.querySelector('input').value = ft ? t.pre.slice(...ft) : '';
    });
    const changed = () => H.changed;
    const svg = svgEl('svg', { class: 'tv-canvas', role: 'img', 'aria-label': 'tree' });
    const side = el('div', 'tv-forest-side');
    const status = el('span', 'tv-muted');
    const touch = () => { H.settle(); draw(); };
    const button = (row, label, f, title) => { const b = el('button', 'tv-button', esc(label)); b.type = 'button'; b.onclick = f; if (title) b.title = title; row.append(b); return b; };
    const field = (label, value, f, rules = ['balanced']) => {
      const row = el('label', 'tv-row');
      const input = Object.assign(el('input', 'tv-input'), { value, spellcheck: false, ariaLabel: label });
      // typing is one undo step: held while the field has the focus
      input.onfocus = () => H.hold(true);
      input.onblur = () => H.hold(false);
      input.oninput = () => { f(input.value); drawTree(); status.textContent = 'changes to apply'; write.disabled = false; };
      row.append(el('span', 'tv-muted', esc(label)), input);
      self.satexDialogs.guard(input, rules);
      return row;
    };
    const at = (e) => { const p = svg.createSVGPoint(); p.x = e.clientX; p.y = e.clientY; return p.matrixTransform(svg.getScreenCTM().inverse()); };
    function drawTree() {
      const text = (n) => n.content.replace(/^\{(.*)\}$/, '$1') || (n.children.length ? '∘' : '·');
      const wide = (n) => Math.max(0.5, 0.17 * text(n).length + 0.3);
      const pos = treeLayout(t.root, (n) => wide(n) + 0.3);
      // positions from the run when it computed every one (forest's x, y in pt; y grows up)
      const known = run.size && [...run.values()].every((r) => r.x != null && r.y != null);
      const scale = 1 / 28.45;
      for (const p of pos) if (known && !changed()) { const r = run.get(p.n); p.x = r.x * scale; p.y = r.y * scale; } else p.y *= 1.1;
      const by = new Map(pos.map((p) => [p.n, p]));
      const xs = pos.map((p) => p.x), ys = pos.map((p) => p.y);
      // at least 8 by 4 units, so a small tree keeps its letters small
      const w = Math.max(8, Math.max(...xs) - Math.min(...xs) + 2), h = Math.max(4, Math.max(...ys) - Math.min(...ys) + 1.2);
      const box = [(Math.max(...xs) + Math.min(...xs) - w) / 2, -Math.max(...ys) - 0.6, w, h];
      svg.setAttribute('viewBox', box.join(' '));
      const out = [];
      for (const p of pos) for (const c of p.n.children) out.push(svgEl('line', { class: 'tv-tree-edge', x1: p.x, y1: -p.y + 0.2, x2: by.get(c).x, y2: -by.get(c).y - 0.2 }));
      for (const p of pos) {
        const label = text(p.n), w = wide(p.n);
        const g = svgEl('g', { class: `tv-node${sel === p.n ? ' on' : ''}${p.n.dirty ? ' dirty' : ''}`, 'data-content': p.n.content });
        g.append(svgEl('rect', { x: p.x - w / 2, y: -p.y - 0.2, width: w, height: 0.4, rx: 0.08 }), Object.assign(svgEl('text', { x: p.x, y: -p.y }), { textContent: label }));
        const name = run.get(p.n)?.name;
        if (name) g.append(Object.assign(svgEl('title'), { textContent: `name: ${name}` }));
        g.onpointerdown = (e) => { e.stopPropagation(); sel = p.n; dragging = p.n; try { svg.setPointerCapture(e.pointerId); } catch { /* synthetic */ } draw(); };
        g.dataset.x = p.x; g.dataset.w = w;
        out.push(g);
        p.g = g;
      }
      // a drop: on a node's left or right third a sibling before or after it, in the middle its last child
      svg.onpointerup = (e) => {
        const n = dragging; dragging = null;
        if (!n) return;
        const q = at(e), hit = pos.find((p) => Math.abs(q.x - p.x) < +p.g.dataset.w / 2 && Math.abs(-q.y - p.y) < 0.3);
        if (!hit || hit.n === n) return;
        const third = (q.x - hit.x) / +hit.g.dataset.w;
        const where = hit.n === t.root ? 'child' : third < -1 / 6 ? 'before' : third > 1 / 6 ? 'after' : 'child';
        if (move(t, n, hit.n, where)) touch();
      };
      svg.replaceChildren(...out);
    }
    function draw() {
      drawTree();
      side.replaceChildren();
      const n = sel;
      side.append(field('content', n.content, (v) => { n.content = v; n.dirty = true; }, ['tex']),
        field('options', n.opts.join(', '), (v) => { n.opts = splitTop(v); n.dirty = true; }));
      const row = el('div', 'tv-actions');
      button(row, 'Add child', () => { const c = fresh('new'); n.children.push(c); sel = c; touch(); });
      const parent = parentOf(t, n);
      if (parent) {
        button(row, 'Add sibling', () => { const c = fresh('new'); parent.children.splice(parent.children.indexOf(n) + 1, 0, c); sel = c; touch(); });
        button(row, '←', () => { const k = parent.children.indexOf(n); if (k > 0) { move(t, n, parent.children[k - 1], 'before'); touch(); } }, 'move left');
        button(row, '→', () => { const k = parent.children.indexOf(n); if (k < parent.children.length - 1) { move(t, n, parent.children[k + 1], 'after'); touch(); } }, 'move right');
        // a subtree goes as one undoable step; the status says what went
        button(row, 'Delete', () => { parent.children.splice(parent.children.indexOf(n), 1); sel = parent; touch(); const k = size(n) - 1; if (k) status.textContent = `deleted ${n.content || 'a node'} and ${k} below it: Ctrl+Z brings them back`; },
          n.children.length ? `Delete this node and the ${size(n) - 1} below it (Ctrl+Z undoes)` : 'Delete this node');
      }
      side.append(row);
      status.textContent = changed() ? 'changes to apply' : 'no changes';
      write.disabled = !changed() && !s.wrap;
    }
    const opts = el('div', 'tv-forest-tree');
    const ft = forTree(t.pre);
    opts.append(el('h4', 'tv-h', 'Every node (for tree)'), ftRow = field('for tree', ft ? t.pre.slice(...ft) : '', (v) => setForTree(t, v)));
    const rest = t.pre.trim();
    if (rest && !(ft && rest.replace(/^for\s+tree\s*=\s*\{[\s\S]*\}\s*,?$/, '') === '')) opts.append(el('pre', 'tv-kept', esc(rest)));
    // indented outlines in and out
    const out = el('details', 'tv-mermaid', '<summary>Indented outline</summary>');
    const text = Object.assign(el('textarea', 'tv-input'), { rows: 5, spellcheck: false, placeholder: '- root\n  - child\n    - grandchild\n  - child', ariaLabel: 'outline' });
    const orow = el('div', 'tv-actions');
    button(orow, 'Add under selection', () => { const kids = outlineIn(text.value); if (!kids.length) return; sel.children.push(...kids); touch(); });
    button(orow, 'Replace tree', () => { const kids = outlineIn(text.value); if (!kids.length) return; const r = kids.length === 1 ? kids[0] : Object.assign(fresh(''), { children: kids }); r.place = t.root.place; r.lead = t.root.lead; t.root = r; sel = r; touch(); });
    button(orow, 'Export', () => { text.value = outlineOut(t.root); text.select(); });
    out.append(text, orow);
    const writeIt = () => {
      // a node whose text breaks the TeX (`50% off`) is shown with its message and the Escape fix, nothing written
      const slip = (n) => texIssue(n.content) ? n : n.children.map(slip).find(Boolean);
      const bad = slip(t.root);
      if (bad) { sel = bad; draw(); status.textContent = `not written: ${bad.content}: ${texIssue(bad.content)}`; return side.querySelector('input')?.focus(); }
      settle(t);
      const body = treeEmit(t);
      ctx.apply([{ range: s.range, newText: s.wrap ? `\\begin{forest}${body}\\end{forest}` : body }, ...(s.extra ? [s.extra] : [])]);
    };
    const { el: foot, apply: write } = footer(root, ctx, { status, apply: writeIt, label: s.wrap ? 'Insert tree' : 'Apply', H, tools: true });
    root.append(svg, side, opts, out);
    if (s.note) root.append(el('div', 'tv-text', ctx.markdown(s.note)));
    root.append(foot);
    draw();
    return root;
  }
  RENDER.forest = forest;

  // page layout (page plugin): LaTeX's page model in sp as the run computed it, edited as body
  // margins; written as geometry's keys, typearea's DIV and BCOR, or `\newgeometry` around a part
  const ONE_IN = 4736286; // 1in in sp (TeXbook p. 57)
  /** geometry's keys by what they set: a changed group drops its keys and writes its own. */
  const GEOMETRY = {
    paper: ['paper', 'papername', 'paperwidth', 'paperheight', 'papersize', 'landscape', 'portrait'],
    margins: ['margin', 'hmargin', 'vmargin', 'left', 'right', 'inner', 'outer', 'lmargin', 'rmargin', 'top', 'bottom', 'tmargin', 'bmargin',
      'textwidth', 'textheight', 'width', 'height', 'hscale', 'vscale', 'scale', 'body', 'total', 'text', 'hmarginratio', 'vmarginratio', 'marginratio', 'ratio',
      'hcentering', 'vcentering', 'centering', 'lines', 'includehead', 'includefoot', 'includeheadfoot', 'includemp', 'includeall', 'ignorehead', 'ignorefoot', 'ignoreheadfoot', 'ignoremp', 'ignoreall'],
    head: ['headheight', 'head', 'headsep', 'footskip', 'foot', 'nohead', 'nofoot', 'noheadfoot'],
    marginpar: ['marginparwidth', 'marginpar', 'marginparsep', 'nomarginpar'],
  };
  /** The page as edited, from the view: body margins in sp (an interval's middle), the lengths the run left open. */
  function pageOf(s) {
    const st = pageRun(s);
    st.ran = { div: st.div, bcor: st.bcor };
    return st;
  }
  function pageRun(s) {
    const mid = (k) => { const v = s.run?.[k]; return v ? Math.round((v[0] + v[1]) / 2) : 0; };
    const pw = mid('paperwidth'), ph = mid('paperheight'), tw = mid('textwidth'), th = mid('textheight');
    const left = ONE_IN + mid('hoffset') + mid('oddsidemargin'), top = ONE_IN + mid('voffset') + mid('topmargin') + mid('headheight') + mid('headsep');
    // a paper within a point of the run's (geometry's sizes in mm, the class's in pt), the one the source names first
    const fits = (s.papers ?? []).filter((p) => Math.abs(p.w - pw) < 65536 && Math.abs(p.h - ph) < 65536);
    const written = [s.geometry?.global?.options, s.koma?.options].join(',');
    const paper = (fits.find((p) => opts(written).some((o) => kv(o)[0] === p.name || kv(o)[1] === p.name)) ?? fits[0])?.name ?? '';
    const bcor = s.koma?.bcor ?? 0;
    return {
      mode: s.koma ? 'koma' : 'geometry', paper, pw, ph, left, right: pw - left - tw, top, bottom: ph - top - th,
      headheight: mid('headheight'), headsep: mid('headsep'), footskip: mid('footskip'), mpwidth: mid('marginparwidth'), mpsep: mid('marginparsep'),
      div: s.koma ? s.koma.div ?? Math.round(3 / (1 - tw / (pw - bcor))) : null, bcor, ran: null,
      scope: 'all', from: s.part?.here ? 'here' : '0', to: 'end', header: headerOf(s.header, s.twoside),
      open: Object.entries(s.run ?? {}).filter(([, v]) => v[0] !== v[1]).map(([k]) => k),
    };
  }
  /** The body's margins `st` sets: typearea's from DIV and BCOR (inner 1, outer 2 stripes; one-sided both alike), else its own. */
  function pageBox(st, twoside) {
    // as the run computed it, until DIV or BCOR change
    if (st.mode !== 'koma' || (st.div === st.ran?.div && st.bcor === st.ran?.bcor)) return st;
    const aw = st.pw - st.bcor, tw = aw * (st.div - 3) / st.div, inner = twoside ? aw / st.div : (aw - tw) / 2;
    return { ...st, left: st.bcor + inner, right: aw - inner - tw, top: st.ph / st.div, bottom: 2 * st.ph / st.div };
  }
  /** geometry's keys of each group for `st`, lengths through `len`. */
  function geometryKeys(st, twoside, len) {
    const b = pageBox(st, twoside);
    return {
      paper: st.paper ? [[st.paper, '']] : [['paperwidth', len(st.pw)], ['paperheight', len(st.ph)]],
      margins: [[twoside ? 'inner' : 'left', len(b.left)], [twoside ? 'outer' : 'right', len(b.right)], ['top', len(b.top)], ['bottom', len(b.bottom)]],
      head: [['headheight', len(st.headheight)], ['headsep', len(st.headsep)], ['footskip', len(st.footskip)]],
      marginpar: [['marginparwidth', len(st.mpwidth)], ['marginparsep', len(st.mpsep)]],
    };
  }
  /** The groups whose lengths differ between two states. */
  const changedGroups = (a, b) => Object.entries({ paper: ['paper', 'pw', 'ph'], margins: ['left', 'right', 'top', 'bottom', 'div', 'bcor', 'mode'], head: ['headheight', 'headsep', 'footskip'], marginpar: ['mpwidth', 'mpsep'] })
    .filter(([, ks]) => ks.some((k) => typeof a[k] === 'number' ? Math.abs(a[k] - b[k]) > 1 : a[k] !== b[k])).map(([g]) => g);
  /** A key list as written, the keys `drop` dropped, `keys` added, in the list's own spacing. */
  function setKeys(written, drop, keys) {
    const kept = opts(written).filter((o) => !drop.includes(kv(o)[0]));
    return [...kept, ...keys.map(([k, v]) => v === '' ? k : `${k}=${v}`)].join(/,[^\S\n]/.test(written) || !written.includes(',') ? ', ' : ',');
  }
  /** geometry's keys of `groups` dropped (papers by name too) and written anew. */
  const regroup = (s, written, groups, keys) => setKeys(written, groups.flatMap((g) => GEOMETRY[g]).concat(groups.includes('paper') ? (s.papers ?? []).map((p) => p.name) : []), groups.flatMap((g) => keys[g]));
  /** The edits that write `st` over the view's `s` as it opened (`st0`): geometry or typearea for the whole document, `\newgeometry` around a part. */
  function pageEdits(s, st0, st, len) {
    const two = !!s.twoside, out = [];
    const put = (t, options) => out.push({ range: t.range, newText: fill(t.template, { options }) });
    if (st.scope === 'part') {
      const at = (k) => k === 'here' ? s.part.here : k === 'end' ? null : s.part.marks[+k]?.at;
      const keys = geometryKeys(st, two, len), from = at(st.from), to = at(st.to);
      const list = [...keys.margins, ...keys.head, ...keys.marginpar].map(([k, v]) => `${k}=${v}`).join(', ');
      if (s.geometry.load) out.push(s.geometry.load);
      out.push({ range: { start: from, end: from }, newText: `\\newgeometry{${list}}\n` });
      if (to) out.push({ range: { start: to, end: to }, newText: '\\restoregeometry\n' });
    } else {
      let groups = changedGroups(st0, st);
      const keys = geometryKeys(st, two, len);
      // typearea's margins are class options; the rest geometry's keys
      if (st.mode === 'koma' && s.koma && groups.includes('margins')) {
        put(s.koma, setKeys(s.koma.options, ['DIV', 'BCOR'], [['DIV', String(st.div)], ['BCOR', len(st.bcor)]]));
        groups = groups.filter((g) => g !== 'margins');
      }
      if (groups.length) put(s.geometry.global, regroup(s, s.geometry.global.options, groups, keys));
    }
    return out.concat(headerEdits(s.header, st.header, s.twoside));
  }

  // header and footer: fancyhdr's `\fancyhead[LE,RO]{…}` fields or scrlayer-scrpage's `\ihead{…}`
  const SPOTS = { fancyhdr: ['L', 'C', 'R'], 'scrlayer-scrpage': ['i', 'c', 'o'] };
  /** The fields the header's calls set, in order: `{package, head: {OL: text}, foot, rules, …}`. */
  function headerOf(h, twoside) {
    const m = { package: h?.package ?? null, head: {}, foot: {}, opt: {}, rules: { head: h?.rules?.head ?? null, foot: h?.rules?.foot ?? null }, ruled: {} };
    for (const c of h?.calls ?? []) {
      if (c.name === 'renewcommand') { const k = c.args[0]?.trim().startsWith('\\head') ? 'head' : 'foot'; m.rules[k] = c.args[1]?.trim() ?? m.rules[k]; m.ruled[k] = true; continue; }
      if (m.package === 'scrlayer-scrpage') { const [spot, part] = [c.name[0], c.name.slice(1)]; m[part][`O${spot}`] = c.args[0] ?? ''; if (c.opt != null) m.opt[`${part}${spot}`] = c.opt; continue; }
      const parts = c.name === 'fancyhead' ? ['head'] : c.name === 'fancyfoot' ? ['foot'] : null;
      for (const sel of (c.opt ?? '').split(',').map((x) => x.trim().toUpperCase())) {
        const pick = (set) => { const got = [...set].filter((x) => sel.includes(x)); return got.length ? got : [...set]; };
        for (const part of parts ?? pick('HF').map((x) => (x === 'H' ? 'head' : 'foot'))) for (const side of pick('EO')) for (const spot of pick('LCR')) m[part][side + spot] = c.args[0] ?? '';
      }
    }
    if (!twoside && m.package === 'fancyhdr') for (const part of ['head', 'foot']) for (const k of Object.keys(m[part])) if (k[0] === 'E') delete m[part][k];
    return m;
  }
  /** The text of fields `m`: fancyhdr's selectors grouped by text, scrlayer-scrpage's one call per field. */
  function headerText(m, twoside, clear) {
    const lines = [];
    if (m.package === 'scrlayer-scrpage') {
      for (const part of ['head', 'foot']) for (const spot of SPOTS[m.package]) {
        const t = m[part][`O${spot}`] ?? '', o = m.opt[`${part}${spot}`];
        if (t || o != null) lines.push(`\\${spot}${part}${o != null ? `[${o}]` : ''}{${t}}`);
      }
      return lines.join('\n');
    }
    for (const part of ['head', 'foot']) {
      const by = new Map();
      for (const side of twoside ? ['E', 'O'] : ['O']) for (const spot of SPOTS.fancyhdr) {
        const t = m[part][side + spot] ?? '';
        if (t) by.set(t, [...(by.get(t) ?? []), twoside ? spot + side : spot]);
      }
      for (const [t, sel] of by) lines.push(`\\fancy${part}[${sel.join(',')}]{${t}}`);
    }
    if (lines.length || clear) lines.unshift('\\fancyhf{}');
    return lines.join('\n');
  }
  /** The edits from header `h` as written to fields `m`: the first call becomes the new block, the rest go. */
  function headerEdits(h, m, twoside) {
    if (!h || !m.package) return [];
    const first = headerOf(h, twoside), rules = [];
    for (const k of ['head', 'foot']) if (m.rules[k] && (m.ruled[k] || m.rules[k] !== first.rules[k])) rules.push(`\\renewcommand{\\${k}rulewidth}{${m.rules[k]}}`);
    if (JSON.stringify({ ...m, ruled: 0 }) === JSON.stringify({ ...first, ruled: 0 })) return [];
    const calls = h.calls ?? [];
    const text = [headerText(m, twoside, calls.length), ...(m.package === 'fancyhdr' ? rules : [])].filter(Boolean).join('\n');
    const block = text ? text + '\n' : '';
    if (!h.package) return [{ range: { start: h.insert, end: h.insert }, newText: h.load + block }];
    if (!calls.length) return [{ range: { start: h.insert, end: h.insert }, newText: (h.style ?? '') + block }];
    return calls.map((c, i) => ({ range: c.range, newText: i ? '' : (h.style ?? '') + block }));
  }

  function page(s, view, ctx) {
    const root = el('div', 'tv-page');
    const st0 = pageOf(s), two = !!s.twoside;
    let st = structuredClone(st0), tab = 'margins', unit = s.unit ?? 'cm';
    const per = Object.fromEntries(s.units.map((u) => [u.name, u.sp]));
    const num = (sp, u = unit) => +(sp / per[u]).toFixed(s.decimals ?? 2);
    const len = (sp) => `${num(sp)}${unit}`;
    const H = self.satexDialogs?.history?.(() => st, (v) => { st = v; draw(); });
    const svg = svgEl('svg', { class: 'tv-canvas tv-sheet', role: 'img', 'aria-label': 'the page to scale' });
    const side = el('div', 'tv-page-side'), status = el('span', 'tv-muted');
    const tabs = el('div', 'tv-tabs');
    for (const [k, label] of [['margins', 'Margins'], ['header', 'Header and footer']]) {
      const b = el('button', 'tv-tab', esc(label)); b.type = 'button'; b.dataset.tab = k;
      b.onclick = () => { tab = k; draw(); };
      tabs.append(b);
    }
    const units = el('select', 'tv-input', s.units.filter((u) => u.name !== 'sp').map((u) => `<option${u.name === unit ? ' selected' : ''}>${esc(u.name)}</option>`).join(''));
    units.ariaLabel = 'unit';
    units.onchange = () => { unit = units.value; draw(); };
    const bar = el('div', 'tv-tools');
    bar.append(tabs, units);
    const touch = () => { H?.settle?.(); paint(); };
    /** A length field: shown in the unit, an interval of the run as its range. */
    const field = (label, key, title, f) => {
      const row = el('label', 'tv-field');
      const open = st.open.includes(title) && Math.abs(st[key] - st0[key]) <= 1 && s.run[title];
      const input = Object.assign(el('input', 'tv-input'), { type: 'number', step: 'any', value: num(st[key]), ariaLabel: label });
      input.dataset.key = key;
      if (open) { input.placeholder = `${num(s.run[title][0])} … ${num(s.run[title][1])}`; input.title = `the run left \\${title} open: ${input.placeholder} ${unit}`; }
      input.oninput = () => { const v = Number(input.value); if (input.value === '' || !Number.isFinite(v)) return; (f ?? ((sp) => { st[key] = sp; }))(Math.round(v * per[unit])); paint(); };
      input.onchange = touch;
      row.append(el('span', '', esc(label) + (open ? ' <i title="an interval: the run did not decide it">≈</i>' : '')), input);
      return row;
    };
    const group = (title, ...rows) => { const g = el('div', 'tv-form'); g.append(el('h4', 'tv-h', esc(title)), ...rows); return g; };
    function margins() {
      const out = [];
      if (s.koma) {
        const modes = el('div', 'tv-tabs');
        for (const [k, label] of [['koma', 'typearea: DIV and BCOR'], ['geometry', 'geometry: exact margins']]) {
          const b = el('button', `tv-tab${st.mode === k ? ' on' : ''}`, esc(label)); b.type = 'button';
          b.onclick = () => { if (k === 'geometry' && st.mode === 'koma') Object.assign(st, pageBox(st, two)); st.mode = k; touch(); draw(); };
          modes.append(b);
        }
        out.push(modes, el('div', 'tv-text', ctx.markdown(st.mode === 'koma'
          ? `The class loads typearea: DIV cuts the page into stripes, the text keeps ${two ? 'one inside and two outside' : 'the rest, split evenly'}, one above and two below; BCOR is the binding's loss. Written as class options.`
          : 'geometry sets each margin itself and overrides what typearea computed.')));
      }
      if (st.mode === 'koma') {
        const div = Object.assign(el('input', 'tv-input'), { type: 'number', min: 4, step: 1, value: st.div, ariaLabel: 'DIV' });
        div.oninput = () => { const v = Math.round(Number(div.value)); if (v >= 4) { st.div = v; paint(); } };
        div.onchange = touch;
        const row = el('label', 'tv-field'); row.append(el('span', '', 'DIV'), div);
        out.push(group('Type area', row, field('BCOR', 'bcor', 'ta@bcor')));
      } else {
        const paper = el('select', 'tv-input', `<option value="">custom</option>${(s.papers ?? []).map((p) => `<option value="${esc(p.name)}"${p.name === st.paper ? ' selected' : ''}>${esc(p.name)}${(s.papers.filter((q) => q !== p && q.w === p.w && q.h === p.h).map((q) => ` = ${q.name}`).join(''))}</option>`).join('')}`);
        paper.ariaLabel = 'paper';
        paper.onchange = () => { const p = s.papers.find((x) => x.name === paper.value); st.paper = paper.value; if (p) { const [dw, dh] = [p.w - st.pw, p.h - st.ph]; Object.assign(st, { pw: p.w, ph: p.h, right: st.right + dw, bottom: st.bottom + dh }); } touch(); draw(); };
        const prow = el('label', 'tv-field'); prow.append(el('span', '', 'Paper'), paper);
        const custom = (k) => (sp) => { st.paper = ''; st[k === 'pw' ? 'right' : 'bottom'] += sp - st[k]; st[k] = sp; };
        // \newgeometry keeps the paper
        if (st.scope !== 'part') out.push(group('Paper', prow, field('Width', 'pw', 'paperwidth', custom('pw')), field('Height', 'ph', 'paperheight', custom('ph'))));
        out.push(group('Margins', field(two ? 'Inner' : 'Left', 'left', 'oddsidemargin'), field(two ? 'Outer' : 'Right', 'right', 'textwidth'),
          field('Top', 'top', 'topmargin'), field('Bottom', 'bottom', 'textheight'),
          field('Text width', 'tw', 'textwidth', (sp) => { st.right = st.pw - st.left - sp; }), field('Text height', 'th', 'textheight', (sp) => { st.bottom = st.ph - st.top - sp; })));
      }
      out.push(group('Header and footer', field('Head height', 'headheight', 'headheight'), field('Head sep', 'headsep', 'headsep'), field('Foot skip', 'footskip', 'footskip')),
        group('Margin notes', field('Width', 'mpwidth', 'marginparwidth'), field('Sep', 'mpsep', 'marginparsep')));
      // the whole document, or from a place to another with \newgeometry … \restoregeometry
      const scope = el('div', 'tv-tabs');
      for (const [k, label] of [['all', 'Whole document'], ['part', 'These pages only']]) {
        const b = el('button', `tv-tab${st.scope === k ? ' on' : ''}`, esc(label)); b.type = 'button';
        b.disabled = k === 'part' && !(s.part?.here || s.part?.marks?.length);
        b.onclick = () => { st.scope = k; touch(); draw(); };
        scope.append(b);
      }
      const where = group('Applies to', scope);
      if (st.scope === 'part') {
        const places = [...(s.part.here ? [['here', `the cursor (line ${s.part.here.line + 1})`]] : []), ...s.part.marks.map((m, i) => [String(i), `${m.level ?? 'section'} ${m.title} (line ${m.at.line + 1})`])];
        const line = (k) => (k === 'here' ? s.part.here : k === 'end' ? { line: Infinity } : s.part.marks[+k].at).line;
        const pick = (label, key, list) => {
          const sel = el('select', 'tv-input', list.map(([k, t]) => `<option value="${k}"${k === st[key] ? ' selected' : ''}>${esc(t)}</option>`).join(''));
          sel.ariaLabel = label;
          sel.onchange = () => { st[key] = sel.value; if (line(st.to) <= line(st.from)) st.to = 'end'; touch(); draw(); };
          const row = el('label', 'tv-field'); row.append(el('span', '', esc(label)), sel); return row;
        };
        where.append(pick('From', 'from', places), pick('Up to', 'to', [...places.filter(([k]) => line(k) > line(st.from)), ['end', 'the end of the document']]),
          el('div', 'tv-text', ctx.markdown('Writes `\\newgeometry` there and `\\restoregeometry` before the end. Both start a new page. Places are chosen in the source: which page numbers they get, only typesetting tells.' + (s.geometry.load ? ' Adds `\\usepackage[pass]{geometry}`: the layout elsewhere stays.' : ''))));
      }
      out.unshift(where);
      return out;
    }
    function header() {
      const m = st.header, out = [];
      if (!m.package) {
        const b = el('button', 'tv-button tv-start', 'Load fancyhdr'); b.type = 'button';
        b.onclick = () => { m.package = 'fancyhdr'; touch(); draw(); };
        out.push(el('div', 'tv-text', ctx.markdown('No package for headers is loaded. fancyhdr sets the header and footer in three fields each.')), b);
        return out;
      }
      const spots = SPOTS[m.package], names = { L: 'Left', C: 'Center', R: 'Right', i: 'Inner', c: 'Center', o: 'Outer' };
      const sides = m.package === 'fancyhdr' && two ? [['O', 'odd pages'], ['E', 'even pages']] : [['O', '']];
      for (const part of ['head', 'foot']) for (const [side, on] of sides) {
        const rows = spots.map((spot) => {
          const row = el('label', 'tv-field');
          const input = Object.assign(el('input', 'tv-input'), { value: m[part][side + spot] ?? '', spellcheck: false, ariaLabel: `${part} ${names[spot]} ${on}`.trim() });
          input.oninput = () => { m[part][side + spot] = input.value; paint(); };
          input.onchange = touch;
          row.append(el('span', '', names[spot]), input);
          return row;
        });
        out.push(group(`${part === 'head' ? 'Header' : 'Footer'}${on ? `, ${on}` : ''}`, ...rows));
      }
      if (m.package === 'fancyhdr') {
        const rule = (k) => {
          const row = el('label', 'tv-field');
          const input = Object.assign(el('input', 'tv-input'), { value: m.rules[k] ?? '', placeholder: 'as the package sets it', spellcheck: false, ariaLabel: `${k} rule width` });
          input.oninput = () => { m.rules[k] = input.value.trim(); paint(); };
          input.onchange = touch;
          row.append(el('span', '', k === 'head' ? 'Header rule' : 'Footer rule'), input);
          return row;
        };
        out.push(group('Rules', rule('head'), rule('foot')));
      } else out.push(el('div', 'tv-text', ctx.markdown('Rules under the header and over the footer: KOMA\'s `headsepline` and `footsepline` options.')));
      return out;
    }
    /** The page (both, two-sided) to scale, in pt, each margin labelled. */
    function sheet() {
      const b = pageBox(st, two), pt = (sp) => sp / 65536, W = pt(st.pw), Hh = pt(st.ph), gap = W / 12, fs = W / (two ? 20 : 28);
      const tw = st.pw - b.left - b.right, th = st.ph - b.top - b.bottom;
      const out = [];
      const label = (x, y, text, cls = '') => Object.assign(svgEl('text', { x, y, class: `tv-page-label ${cls}`, style: `font-size:${fs}px` }), { textContent: text });
      const pages = two ? [[0, true], [W + gap, false]] : [[0, false]];
      for (const [x0, even] of pages) {
        const left = even ? b.right : b.left, x = x0 + pt(left), y = pt(b.top);
        const rect = (cls, rx, ry, w, h, title) => { const r = svgEl('rect', { class: cls, x: rx, y: ry, width: Math.max(0, w), height: Math.max(0, h) }); r.append(Object.assign(svgEl('title'), { textContent: title })); out.push(r); };
        rect('tv-page-paper', x0, 0, W, Hh, `paper ${len(st.pw)} × ${len(st.ph)}`);
        rect('tv-page-head', x, y - pt(st.headsep + st.headheight), pt(tw), pt(st.headheight), `header: \\headheight ${len(st.headheight)}, \\headsep ${len(st.headsep)}`);
        rect('tv-page-body', x, y, pt(tw), pt(th), `text: \\textwidth ${len(tw)}, \\textheight ${len(th)}`);
        const base = y + pt(th + st.footskip), fh = Math.min(pt(st.headheight), pt(st.footskip));
        rect('tv-page-foot', x, base - fh, pt(tw), fh, `footer: \\footskip ${len(st.footskip)} to its baseline`);
        rect('tv-page-mp', even ? x - pt(st.mpsep + st.mpwidth) : x + pt(tw + st.mpsep), y, pt(st.mpwidth), pt(th), `margin notes: \\marginparwidth ${len(st.mpwidth)}, \\marginparsep ${len(st.mpsep)}`);
        // the header's and footer's fields as written, at their places
        const m = st.header, side = two && even && m.package === 'fancyhdr' ? 'E' : 'O';
        (SPOTS[m.package] ?? []).forEach((spot, i) => {
          for (const [part, fy] of [['head', y - pt(st.headsep) - fs * 0.4], ['foot', base - fs * 0.4]]) {
            const t = (m[part][side + spot] ?? '').replace(/\\(\w+)/g, '$1').slice(0, 24);
            if (t) { const n = label(x + [0, pt(tw) / 2, pt(tw)][i], fy, t, 'tv-page-field'); n.style.textAnchor = ['start', 'middle', 'end'][i]; out.push(n); }
          }
        });
        out.push(label(x + pt(tw) / 2, y + pt(th) / 2 - fs * 0.7, `${num(tw)} × ${num(th)} ${unit}`, 'tv-page-big'), label(x + pt(tw) / 2, y + pt(th) / 2 + fs * 0.7, 'text'));
        const mid = y + pt(th) * 0.75;
        out.push(label(x0 + pt(left) / 2, mid, num(left)), label(x + pt(tw) + pt(even ? b.left : b.right) / 2, mid, num(even ? b.left : b.right)),
          label(x + pt(tw) / 2, pt(b.top) / 2 - pt(st.headheight + st.headsep) / 2, num(b.top)), label(x + pt(tw) / 2, Hh - pt(b.bottom) / 2 + fs * 0.6, num(b.bottom)));
      }
      const width = two ? 2 * W + gap : W;
      svg.setAttribute('viewBox', [-fs, -fs, width + 2 * fs, Hh + 2 * fs].join(' '));
      svg.replaceChildren(...out);
    }
    /** What Apply writes now. */
    const edits = () => pageEdits(s, st0, st, len);
    function paint() {
      st.tw = st.pw - st.left - st.right; st.th = st.ph - st.top - st.bottom;
      sheet();
      const e = edits();
      status.textContent = e.length ? `${e.length} edit${e.length > 1 ? 's' : ''} to write` : 'no changes';
      write.disabled = !e.length;
      for (const f of side.querySelectorAll('input[type=number]')) if (f !== document.activeElement && f.dataset.key) f.value = num(st[f.dataset.key]);
    }
    function draw() {
      for (const b of tabs.children) b.classList.toggle('on', b.dataset.tab === tab);
      side.replaceChildren(...(tab === 'margins' ? margins() : header()));
      paint();
    }
    const { el: foot, apply: write } = footer(root, ctx, { status, apply: () => ctx.apply(edits()), H, tools: true });
    const notes = el('div', 'tv-text', ctx.markdown(`The run's page${s.class ? ` with ${s.class}` : ''}${two ? ', two-sided' : ''}: the lengths as it ended.${st0.open.length ? ` It left ${st0.open.map((k) => `\\${k}`).join(', ')} open: the middle of each interval is drawn.` : ''}${s.geometry.global.note ? ` Apply ${s.geometry.global.note}.` : ''}`));
    const main = el('div', 'tv-page-main');
    main.append(svg, side);
    root.append(bar, notes, main, foot);
    draw();
    return root;
  }
  RENDER.page = page;

  const STYLE = `
.tv { --tv-bg: var(--panel, var(--vscode-editor-background, #fff)); --tv-fg: var(--fg, var(--vscode-foreground, #222)); --tv-muted: var(--muted, var(--vscode-descriptionForeground, #666));
  --tv-border: var(--border, var(--vscode-widget-border, #ccc)); --tv-accent: var(--accent, var(--vscode-button-background, #0b3d91)); --tv-accent-fg: var(--accent-fg, var(--vscode-button-foreground, #fff));
  --tv-sunk: var(--sunk, var(--vscode-input-background, #f1f3f6)); color: var(--tv-fg); display: grid; gap: .8rem; font-size: .9rem; }
.tv h3.tv-title { margin: 0; font-size: 1rem; }
.tv-h { margin: .2rem 0 0; font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .04em; color: var(--tv-muted); }
.tv-text p { margin: .2rem 0; } .tv-text { color: var(--tv-muted); font-size: .82rem; }
.tv-table { border-collapse: collapse; } .tv-table td, .tv-table th { padding: .2rem .6rem; border-bottom: 1px solid var(--tv-border); text-align: left; }
.tv-actions { display: flex; flex-wrap: wrap; gap: .4rem; }
.tv-input { font: inherit; font-size: .88rem; padding: .3rem .5rem; border-radius: 6px; border: 1px solid var(--tv-border); background: var(--tv-sunk); color: var(--tv-fg); min-width: 0; }
.tv-input.tv-bad { border-color: var(--error, #c33); } .tv-row:has(> .sd-err.on) { flex-wrap: wrap; } .tv-row > .sd-err { flex-basis: 100%; } .tv-field > .sd-err { grid-column: 2; }
.tv-converter { display: grid; gap: .5rem; } .tv-row { display: flex; gap: .4rem; } .tv-row input { flex: 1; font-size: 1.1rem; }
.tv-units { display: grid; grid-template-columns: repeat(auto-fill, minmax(6.5rem, 1fr)); gap: .4rem; }
.tv-unit { font: inherit; display: flex; flex-direction: column; align-items: flex-start; gap: .1rem; padding: .45rem .6rem; border-radius: 8px; border: 1px solid var(--tv-border); background: var(--tv-sunk); color: var(--tv-fg); cursor: pointer; text-align: left; }
.tv-unit b { font-variant-numeric: tabular-nums; font-weight: 600; } .tv-unit span { font-size: .72rem; color: var(--tv-muted); }
.tv-unit:not(:disabled):hover, .tv-unit:focus-visible { border-color: var(--tv-accent); background: color-mix(in srgb, var(--tv-accent) 10%, var(--tv-sunk)); transform: translateY(-1px); } .tv-unit:disabled { cursor: default; opacity: .85; }
.tv-specs .tv-spec-value { font: inherit; display: block; width: 100%; text-align: left; background: none; border: 0; border-radius: 6px; padding: .1rem .35rem; color: inherit; font-variant-numeric: tabular-nums; }
button.tv-spec-value { cursor: pointer; } button.tv-spec-value:hover, button.tv-spec-value:focus-visible { background: color-mix(in srgb, var(--tv-accent) 12%, transparent); color: var(--tv-accent); outline: none; }
.tv-unit { transition: border-color .12s, background .12s, transform .12s; } .tv-unit.exact b { color: var(--tv-accent); }
@media (prefers-reduced-motion: reduce) { .tv-unit { transition: none; } .tv-unit:hover { transform: none; } }
.tv-choices { display: grid; gap: .5rem; } .tv-list { display: grid; gap: .2rem; max-height: 50vh; overflow: auto; }
.tv-choice { font: inherit; display: flex; justify-content: space-between; gap: 1rem; padding: .35rem .6rem; border-radius: 6px; border: 1px solid transparent; background: none; color: var(--tv-fg); cursor: pointer; text-align: left; }
.tv-choice:hover, .tv-choice:focus { border-color: var(--tv-accent); background: var(--tv-sunk); } .tv-choice span { color: var(--tv-muted); font-family: ui-monospace, monospace; font-size: .8rem; }
.tv-color { display: grid; grid-template-columns: minmax(10rem, 15rem) 1fr; gap: .9rem; align-items: start; }
.tv-color > .tv-h, .tv-color > .tv-defined, .tv-color > .tv-actions { grid-column: 1 / -1; }
.tv-pick { display: grid; gap: .5rem; }
.tv-square { position: relative; aspect-ratio: 1; border-radius: 8px; cursor: crosshair; touch-action: none; border: 1px solid var(--tv-border); }
.tv-hue { position: relative; height: 14px; border-radius: 7px; cursor: ew-resize; touch-action: none; border: 1px solid var(--tv-border);
  background: linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00); }
.tv-dot, .tv-knob { position: absolute; width: 14px; height: 14px; border-radius: 50%; border: 2px solid #fff; box-shadow: 0 0 0 1px #0008, 0 1px 3px #0006; transform: translate(-50%, -50%); pointer-events: none; }
.tv-knob { top: 50%; }
.tv-side { display: grid; gap: .5rem; align-content: start; min-width: 0; }
.tv-swatches { display: flex; height: 2.4rem; border-radius: 8px; overflow: hidden; border: 1px solid var(--tv-border); }
.tv-swatches span { flex: 1; }
.tv-spec { font-family: ui-monospace, monospace; }
.tv-specs { display: grid; gap: .1rem; font-family: ui-monospace, monospace; font-size: .75rem; color: var(--tv-muted); }
.tv-specs b { display: inline-block; min-width: 3.2rem; color: var(--tv-fg); font-weight: 600; }
.tv-expr { display: flex; align-items: center; gap: .4rem; font-size: .8rem; color: var(--tv-muted); } .tv-expr input { flex: 1; font-family: ui-monospace, monospace; }
.tv-defined { display: grid; gap: .45rem; max-height: 16rem; overflow: auto; }
.tv-band { display: flex; flex-wrap: wrap; gap: .3rem; align-items: center; } .tv-band-name { flex: 0 0 100%; font-size: .72rem; color: var(--tv-muted); } .tv-band-name small { opacity: .7; }
.tv-more { padding-left: .5rem; color: var(--tv-muted); }
.tv-chip i { width: .9rem; height: .9rem; border-radius: 50%; border: 1px solid #8886; }
.tv-records { display: grid; gap: .5rem; } .tv-records > .tv-list { max-height: 60vh; }
.tv-record.on { border: 1px solid var(--tv-border); border-radius: 8px; background: var(--tv-sunk); }
.tv-record > .tv-choice { width: 100%; } .tv-line { display: grid; grid-template-columns: minmax(4rem, max-content) minmax(0, 1fr) auto; align-items: baseline; gap: .8rem; }
.tv-line > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: inherit; font-size: .8rem; } .tv-record.on > .tv-line { cursor: default; } .tv-record > .tv-choice b { font-weight: 600; } .tv-badge { font-style: normal; font-weight: 400; font-size: .7rem; padding: 0 .4rem; border-radius: 999px; border: 1px solid var(--tv-border); color: var(--tv-muted); }
.tv-body { display: grid; gap: .6rem; padding: .3rem .6rem .7rem; min-width: 0; } .tv-body .tv-table { font-size: .8rem; display: block; overflow-x: auto; }
.tv-form { display: grid; gap: .4rem; } .tv-field { display: grid; grid-template-columns: 7rem 1fr; gap: .5rem; align-items: center; font-size: .8rem; color: var(--tv-muted); }
.tv-field textarea { min-height: 3.2rem; resize: vertical; } .tv-field .tv-input { font-family: ui-monospace, monospace; font-size: .82rem; }
.tv-add summary { cursor: pointer; font-size: .85rem; color: var(--tv-accent); } .tv-add[open] { display: grid; gap: .5rem; } .tv-add > select { justify-self: start; }
@media (max-width: 560px) { .tv-field { grid-template-columns: 1fr; gap: .15rem; } .tv-field > .sd-err { grid-column: auto; } }
@media (max-width: 560px) { .tv-color { grid-template-columns: 1fr; } .tv-pick { max-width: 16rem; } }
.tv-draw { display: grid; grid-template-rows: auto minmax(0, 1fr) auto; gap: .5rem; min-width: min(40rem, 100%); height: min(calc(90vh - 6rem), 48rem); } .tv-tools { display: flex; flex-wrap: wrap; gap: .3rem; align-items: center; }
.tv-draw-main { display: grid; grid-template-columns: minmax(0, 1fr) minmax(17rem, 24rem); gap: .7rem; min-height: 0; }
.tv-draw-main > .tv-canvas { height: 100%; max-height: none; min-height: 16rem; }
.tv-draw-side { display: flex; flex-direction: column; gap: .5rem; overflow: auto; min-height: 0; padding-right: .2rem; } .tv-draw-side .tv-inspector { grid-template-columns: 1fr; }
.tv-draw-side .tv-h { margin: .3rem 0 0; font-size: .72rem; text-transform: uppercase; letter-spacing: .06em; color: var(--tv-muted); font-weight: 650; }
.tv-style { display: grid; gap: .3rem; padding: .15rem 0; } .tv-style.on { border: 1px solid var(--tv-border); border-radius: 8px; padding: .3rem; background: var(--tv-sunk); }
.tv-style > .tv-row { display: grid; grid-template-columns: minmax(0, 1fr) auto auto auto; gap: .3rem; align-items: center; } .tv-style .tv-choice { min-width: 0; padding: .2rem .4rem; overflow: hidden; } .tv-style .tv-choice span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tv-style-new { display: flex; flex-wrap: wrap; gap: .3rem; margin-top: .3rem; } .tv-style-new input { flex: 1 1 7rem; min-width: 0; } .tv-style-new select { flex: 0 1 11rem; min-width: 0; } .tv-draw-side .tv-input { font-size: .82rem; } .tv-styles[open], .tv-draw-side > .tv-mermaid[open] { display: grid; gap: .35rem; }
.tv-beside { display: flex; gap: .2rem; } .tv-beside .tv-button { min-width: 2rem; padding: 0 .4rem; }
@media (max-width: 760px) { .tv-draw { height: auto; } .tv-draw-main { grid-template-columns: 1fr; } .tv-draw-side { overflow: visible; } .tv-draw-main > .tv-canvas { height: auto; max-height: 55vh; } }
.tv-muted { color: var(--tv-muted); font-size: .78rem; } .tv-sel { align-items: center; min-height: 2rem; } .tv-sel input { flex: 1; font-size: .88rem; }
.tv-canvas { width: 100%; max-height: 55vh; background: var(--tv-sunk); border: 1px solid var(--tv-border); border-radius: 8px; overflow: hidden; clip-path: inset(0 round 8px); touch-action: none; color: var(--tv-fg); font-family: ui-sans-serif, system-ui, sans-serif; }
.tv-grid { stroke: var(--tv-border); stroke-width: .01; fill: none; }
.tv-canvas text { font-size: .32px; text-anchor: middle; dominant-baseline: central; fill: currentColor; pointer-events: none; }
.tv-node { cursor: grab; } .tv-node text { font: .32px var(--serif, Georgia, serif); } .tv-node rect, .tv-node circle { fill: var(--tv-bg); stroke: currentColor; stroke-width: .03; }
.tv-node.bare rect, .tv-node.bare circle { stroke-dasharray: .04 .06; stroke-width: .015; }
.tv-node.fixed { cursor: not-allowed; color: var(--tv-muted); } .tv-node.fixed rect, .tv-node.fixed circle { stroke-dasharray: .1 .07; }
.tv-node.on rect, .tv-node.on circle, .tv-node.pending rect, .tv-node.pending circle { stroke: var(--tv-accent); stroke-width: .06; }
.tv-edge line { stroke: currentColor; stroke-width: .03; } .tv-edge line.tv-hit { stroke: transparent; stroke-width: .3; cursor: pointer; }
.tv-edge.on line:not(.tv-hit) { stroke: var(--tv-accent); stroke-width: .05; } .tv-edge text { font-size: .26px; }
.tv-node ellipse, .tv-node polygon { fill: var(--tv-bg); stroke: currentColor; stroke-width: .03; } .tv-node.on ellipse, .tv-node.on polygon { stroke: var(--tv-accent); stroke-width: .06; }
.tv-handle { fill: var(--tv-bg); stroke: var(--tv-accent); stroke-width: .03; cursor: crosshair; } .tv-anchor { fill: var(--tv-accent); opacity: .75; pointer-events: none; }
.tv-hint { font-size: .26px; fill: var(--tv-accent) !important; }
.tv-tools .tv-danger { margin-left: auto !important; }
.tv-grid-pick { gap: .3rem; align-items: center; } .tv-grid-pick input[type=number] { width: 4.5rem; }
.tv-inspect { display: grid; gap: .3rem; } .tv-inspector { grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr)); gap: .3rem .8rem; }
.tv-tip-preview { height: 1rem; width: 2.2rem; object-fit: cover; object-position: right; flex: none; border-radius: 4px; }
.tv-inspector .tv-row { display: flex; gap: .3rem; align-items: center; min-width: 0; } .tv-inspector .tv-row select { flex: 1; min-width: 0; }
.tv-layers { flex-wrap: wrap; gap: .3rem; align-items: center; } .tv-off { opacity: .55; }
.tv-path { fill: none; stroke: var(--tv-muted); stroke-width: .03; stroke-dasharray: .1 .07; }
.tv-kept pre { margin: .15rem 0; padding: .3rem .5rem; border-radius: 6px; background: var(--tv-sunk); font-size: .75rem; white-space: pre-wrap; color: var(--tv-muted); }
.tv-mermaid summary { cursor: pointer; font-size: .82rem; color: var(--tv-muted); } .tv-mermaid textarea { width: 100%; box-sizing: border-box; font-family: ui-monospace, monospace; margin: .4rem 0; }
.tv-tree-edge { stroke: currentColor; stroke-width: .02; } .tv-node.dirty rect { stroke: var(--tv-accent); }
.tv-forest-side, .tv-forest-tree { display: grid; gap: .4rem; } .tv-forest-side .tv-row > span, .tv-forest-tree .tv-row > span { min-width: 4.5rem; align-self: center; }
.tv-page { min-width: min(46rem, 100%); } .tv-page-main { display: grid; grid-template-columns: minmax(14rem, 1.2fr) minmax(15rem, 1fr); gap: .9rem; align-items: start; }
.tv-sheet { max-height: 62vh; } .tv-page-side { display: grid; gap: .6rem; max-height: 62vh; overflow: auto; padding-right: .2rem; } .tv-page-side .tv-field { grid-template-columns: 6.5rem 1fr; }
.tv-page-paper { fill: var(--tv-bg); stroke: var(--tv-muted); stroke-width: .6; } .tv-page-body { fill: color-mix(in srgb, var(--tv-accent) 16%, transparent); stroke: var(--tv-accent); stroke-width: .8; }
.tv-page-head, .tv-page-foot { fill: color-mix(in srgb, var(--tv-accent) 10%, transparent); stroke: var(--tv-accent); stroke-width: .8; stroke-dasharray: 3 2; } .tv-start { justify-self: start; }
.tv-sheet text.tv-page-field { fill: var(--tv-fg); font-style: italic; } .tv-page-mp { fill: none; stroke: var(--tv-muted); stroke-width: .5; stroke-dasharray: 2 2; }
.tv-sheet text.tv-page-label { fill: var(--tv-muted); font-family: ui-sans-serif, system-ui, sans-serif; } .tv-sheet text.tv-page-big { fill: var(--tv-fg); font-weight: 600; }
@media (max-width: 640px) { .tv-page-main { grid-template-columns: 1fr; } }
.tv-hint { margin-right: auto; align-self: center; } .tv-foot { justify-content: flex-end; align-items: center; } .tv-foot .tv-muted { margin-right: auto; }`;

  /** Renders `view` into `container`; `ctx.apply(edits)` writes, `ctx.markdown(md)` makes HTML. */
  function render(view, container, ctx) {
    if (!document.getElementById('tv-style')) document.head.append(Object.assign(document.createElement('style'), { id: 'tv-style', textContent: STYLE }));
    const c = { markdown: (md) => `<p>${esc(md)}</p>`, ...ctx };
    const root = el('div', 'tv');
    root.dataset.plugin = view.plugin ?? '';
    for (const s of view.sections ?? []) {
      const node = RENDER[s.kind]?.(s, view, c);
      if (node) root.append(node);
    }
    container.replaceChildren(root);
    return root;
  }
  // ---- dev mode: a trace step's abstract state (`trace_state`, src/analysis/machine/inspect.rs) and the trace as CSV
  const cell = (v) => { const t = v == null ? '' : typeof v === 'object' ? JSON.stringify(v) : String(v); return /[",\n]/.test(t) ? `"${t.replace(/"/g, '""')}"` : t; };
  /** Records as CSV: every key any record has is a column, a nested value one JSON cell. */
  const traceCsv = (records) => {
    const cols = [...new Set(records.flatMap((r) => Object.keys(r)))];
    return [cols.join(','), ...records.map((r) => cols.map((c) => cell(r[c])).join(','))].join('\n') + '\n';
  };
  const known = (v) => (v.interval ? `[${v.interval.join(', ')}]` : v.set ? `{${v.set.join(', ')}}` : v.unknown ? '?' : v.exact ?? v.abstract ?? '');
  const list = (title, items, row) => (items?.length ? `<div class="ds-sec"><b>${title}</b><ul>${items.map((x) => `<li>${row(x)}</li>`).join('')}</ul></div>` : '');
  /** A `file:line[:col]` as a place that opens (playground.js), with its upstream source as the cards link it (site.js), else as text. */
  const place = (at) => {
    const m = /^(.+?):(\d+)(?::\d+)?$/.exec(String(at ?? ''));
    if (!m) return esc(at);
    return self.satexCards?.placeLink?.(m[1], +m[2]) ?? `<a class="place" href="#" data-path="${esc(m[1])}" data-line="${m[2]}" title="open ${esc(m[1])} at line ${m[2]}">${esc(at)}</a>`;
  };
  /** One step's state, each domain a section; `cs` marks up a command name. */
  const stateHtml = (st, cs = esc) => {
    if (!st) return '<p class="muted">No state: the trace ran without dev mode.</p>';
    const p = st.paths ?? {};
    const head = `<div class="ds-sec"><b>paths</b><span>${p.live} live · ${p.forks} forks so far · peak ${p.peak}</span>${p.apart ? `<pre class="ds-apart">${esc(p.apart)}</pre>` : ''}</div>`;
    const rows = (st.bindings ?? []).map((b) => `<tr><td>${cs(b.name)}</td><td><code>${esc(b.meaning)}</code>${b.may ? `<div class="ds-may">may: ${b.may.map((m) => `<code>${esc(m)}</code>`).join(' | ')}${b.maybe_undefined ? ' <i>maybe undefined</i>' : ''}</div>` : ''}</td>` +
      `<td>${b.value ? `<span class="ds-ty">${esc(b.value.type)}</span> ${esc(known(b.value))}` : ''}</td><td>${[b.certain === false && 'uncertain', b.global && 'global'].filter(Boolean).join(', ')}</td></tr>`).join('');
    const cats = Object.entries(st.catcodes ?? {}).map(([c, k]) => `<code class="ds-cat" title="catcode ${k}">${esc(c === ' ' ? '␣' : c)}<sub>${k}</sub></code>`).join('');
    return head +
      list('choices', st.choices, (c) => `arm ${c.arm + 1} of ${c.arms}: ${cs(c.by)} at ${place(c.at)}${c.joined ? ' (joined)' : ''}`) +
      `<div class="ds-sec"><b>mode</b><span>${esc((st.mode ?? []).join(' | ') || 'unknown')}</span></div>` +
      list('conditionals', st.conditionals, (c) => `${place(c.at)} · ${esc(c.limit)}${c.many ? ' · widened' : ''}`) +
      list('groups', st.groups, (g) => `${esc(g.kind)} at ${place(g.at)}${g.many ? ' · widened' : ''}`) +
      list('input', st.input, (f) => (f.file ? `file ${place(f.file)}` : f.scantokens ? `\\scantokens at ${place(f.scantokens)}` : f.returned != null ? `${f.returned} tokens given back` : `${f.tokens} tokens${f.expanding ? ` of ${cs(f.expanding)}` : ''}`)) +
      (rows ? `<div class="ds-sec"><b>bindings</b><table class="ds-bind"><thead><tr><th>name</th><th>meaning</th><th>value</th><th></th></tr></thead><tbody>${rows}</tbody></table></div>` : '') +
      (st.paired?.names?.length ? `<div class="ds-sec"><b>paired by a join</b><span>${st.paired.names.map(cs).join(' ')} · ${st.paired.arms} arms</span></div>` : '') +
      (cats ? `<div class="ds-sec"><b>catcodes ahead</b><span>${cats}</span></div>` : '');
  };
  self.satexViews = { render, RENDER, SPEC, mix, colorBand, oklch, drawing: { expandStyles, STYLE_DEF, parse, emit, mermaidIn, mermaidOut, mermaidLoss, layout, importMermaid, tipsOf, toMatrix, cellPlaces, toLayer, picOptions, relocate, relPos, nearestAnchor, anchorAt, tipMarker }, forest: forestFns, page: { pageOf, pageBox, pageEdits, headerOf, headerText, headerEdits, setKeys }, dev: { traceCsv, stateHtml }, templates: { fill, filled, chosen } };
})();
