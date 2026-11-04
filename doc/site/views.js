// The type plugins' views (`satex/view`, src/lsp/plugins): one renderer for the playground and
// the VS Code webview. A view is `{plugin, title, sections: [{kind, …}]}`; each kind has a
// renderer in `RENDER`, and edits go back through `ctx.apply(edits)` as LSP TextEdits.
(() => {
  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  const el = (tag, cls, html) => Object.assign(document.createElement(tag), cls ? { className: cls } : {}, html != null ? { innerHTML: html } : {});
  const fill = (text, vars) => text.replace(/\$\{(\w+)\}/g, (m, k) => vars[k] ?? m);
  // a WorkspaceEdit `{changes: {uri: [TextEdit]}}` with its templates filled, and several as one
  const filled = (edit, vars) => ({ changes: Object.fromEntries(Object.entries(edit?.changes ?? {}).map(([u, es]) => [u, es.map((e) => ({ range: e.range, newText: fill(e.newText, vars) }))])) });
  const merge = (edits) => ({ changes: edits.reduce((all, e) => { for (const [u, es] of Object.entries(e.changes)) (all[u] ??= []).push(...es); return all; }, {}) });
  // braces that close in order, as TeX reads a value (a `\{` is no group)
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
      const input = Object.assign(el('input', 'tv-input'), { placeholder: s.placeholder ?? 'search', spellcheck: false, ariaLabel: s.placeholder ?? 'search' });
      const sort = el('select', 'tv-input', (s.sorts ?? []).map((k) => `<option>${esc(k)}</option>`).join(''));
      sort.ariaLabel = 'sort by';
      head.append(input);
      if (s.sorts?.length > 1) head.append(sort);
      const list = el('div', 'tv-list');
      let open = s.items.find((r) => r.title === s.focus) ?? null;
      const draw = () => {
        const q = input.value.trim().toLowerCase(), by = sort.value;
        const hit = (r) => !q || q.split(/\s+/).every((w) => (r.search ?? r.title).toLowerCase().includes(w));
        const key = (r) => String(r.sort?.[by] ?? r.title).toLowerCase();
        const rows = s.items.filter(hit).sort((a, b) => key(a).localeCompare(key(b), undefined, { numeric: true }));
        list.replaceChildren(...rows.slice(0, 300).map((r) => {
          const item = el('div', 'tv-record' + (open === r ? ' on' : ''));
          const b = el('button', 'tv-choice', `<b>${esc(r.title)}${r.badge ? ` <em class="tv-badge">${esc(r.badge)}</em>` : ''}</b><span>${esc(r.detail ?? '')}</span>`);
          b.type = 'button';
          b.ariaExpanded = String(open === r);
          b.onclick = () => { open = open === r ? null : r; draw(); };
          item.append(b);
          if (open === r) item.append(record(r));
          return item;
        }));
        if (!rows.length) list.append(el('div', 'tv-text', `<p>${esc(s.empty ?? 'nothing found')}</p>`));
      };
      const record = (r) => {
        const body = el('div', 'tv-body');
        if (r.fields?.length) body.append(form(r.fields, r.save ?? 'Save', (vals, changed) => merge(r.fields.filter((f) => changed.has(f.name)).map((f) => filled(f.edit, { value: vals[f.name] })))));
        for (const f of r.forms ?? []) body.append(form(f.fields, f.title, (vals) => filled(f.edit, vals)));
        if (r.table?.rows.length) body.append(RENDER.table(r.table));
        if (r.actions?.length) {
          const acts = el('div', 'tv-actions');
          for (const a of r.actions) {
            const b = el('button', 'tv-button', esc(a.title));
            b.type = 'button';
            b.disabled = !!a.disabled;
            b.title = a.disabled ?? '';
            b.onclick = () => ctx.apply(a.edit);
            acts.append(b);
          }
          body.append(acts);
          const why = r.actions.filter((a) => a.disabled).map((a) => `${a.title}: ${a.disabled}`);
          if (why.length) body.append(el('div', 'tv-text', why.map((w) => `<p>${esc(w)}</p>`).join('')));
        }
        return body;
      };
      // labelled inputs; `go(values, changed names)` makes the edit, applied when every value is balanced
      const form = (fields, title, go) => {
        const f = el('form', 'tv-form');
        const ins = fields.map((d) => {
          const lab = el('label', 'tv-field', `<span>${esc(d.name)}</span>`);
          const i = Object.assign(el(d.choices ? 'select' : d.long ? 'textarea' : 'input', 'tv-input'), { spellcheck: false, name: d.name });
          if (d.choices) i.innerHTML = d.choices.map((c) => `<option${c === d.value ? ' selected' : ''}>${esc(c)}</option>`).join('');
          else i.value = d.value ?? '';
          if (d.placeholder) i.placeholder = d.placeholder;
          i.oninput = () => i.classList.toggle('tv-bad', !balanced(i.value));
          lab.append(i);
          f.append(lab);
          return [d, i];
        });
        const b = el('button', 'tv-button tv-primary', esc(title));
        b.type = 'submit';
        f.append(el('div', 'tv-actions'));
        f.lastChild.append(b);
        f.onsubmit = (e) => {
          e.preventDefault();
          if (ins.some(([, i]) => !balanced(i.value))) return;
          const vals = Object.fromEntries(ins.map(([d, i]) => [d.name, i.value]));
          const changed = new Set(ins.filter(([d, i]) => i.value !== (d.value ?? '')).map(([d]) => d.name));
          if (changed.size || !fields.some((d) => 'value' in d)) ctx.apply(go(vals, changed));
        };
        return f;
      };
      input.oninput = draw;
      sort.onchange = draw;
      draw();
      box.append(head, list);
      if (s.add) {
        const more = el('details', 'tv-add', `<summary>${esc(s.add.title)}</summary>`);
        const kinds = s.add.kinds ?? [s.add];
        const pick = el('select', 'tv-input', kinds.map((k) => `<option>${esc(k.name ?? s.add.title)}</option>`).join(''));
        pick.ariaLabel = 'kind';
        const slot = el('div');
        const show = () => { const k = kinds[pick.selectedIndex]; slot.replaceChildren(form(k.fields, s.add.title, (vals) => filled(k.edit, vals))); };
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
          const cell = el('button', 'tv-unit', `<b>${range ? `${show(s.lo, u.name)} … ${show(s.hi, u.name)}` : show(sp(), u.name)}</b><span>${esc(u.name)}</span>`);
          cell.type = 'button';
          cell.title = s.range ? `write ${show(sp(), u.name)}${u.name} here` : u.name;
          cell.disabled = !s.range || range || u.name === 'em' || u.name === 'ex';
          cell.onclick = () => {
            const d = s.decimals ?? 2, v = +(sp() / per[u.name]).toFixed(d);
            ctx.apply([{ range: s.range, newText: `${v}${u.name}` }]);
          };
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
      drag(square, (x, y) => { hsb = [hsb[0], x, 1 - y]; });
      drag(hue, (x) => { hsb = [Math.min(x, 0.9999), hsb[1], hsb[2]]; });
      pick.append(square, hue);
      // was and is, then the spec in each model
      const side = el('div', 'tv-side');
      const swatch = el('div', 'tv-swatches', `<span class="tv-was" title="${esc(s.name)}"></span><span class="tv-is"></span>`);
      swatch.querySelector('.tv-was').style.background = s.rgb ? hex(s.rgb) : 'repeating-linear-gradient(45deg, #8884 0 4px, transparent 4px 8px)';
      const tabs = el('div', 'tv-tabs');
      for (const m of s.models) {
        const t = el('button', 'tv-tab', esc(m));
        t.type = 'button';
        t.onclick = () => { model = m; draw(); };
        tabs.append(t);
      }
      const spec = Object.assign(el('input', 'tv-input tv-spec'), { spellcheck: false, ariaLabel: 'spec' });
      spec.onchange = () => { const c = PARSE[model]?.(spec.value); if (c) { hsb = toHsb(c); draw(); } };
      const all = el('div', 'tv-specs');
      side.append(swatch, tabs, spec, all);
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
      if (s.defined.length) {
        const list = el('div', 'tv-defined');
        for (const d of s.defined) {
          const b = el('button', 'tv-chip', `<i style="background:${d.css}"></i>${esc(d.name)}`);
          b.type = 'button';
          b.title = s.pick ? `write ${d.name} here` : d.name;
          b.onclick = () => s.pick ? ctx.apply([{ range: s.pick, newText: d.name }]) : (hsb = toHsb(d.rgb), draw());
          list.append(b);
        }
        box.append(el('h4', 'tv-h', 'Defined here'), list);
      }
      const foot = el('div', 'tv-actions');
      if (s.apply) {
        const write = el('button', 'tv-button tv-primary');
        write.type = 'button';
        write.onclick = () => {
          if (expr && s.pick) return ctx.apply([{ range: s.pick, newText: expr }]);
          const vars = { model, spec: SPEC[model](rgb()), name: s.apply.name ?? s.name };
          ctx.apply(s.apply.edits.map((e) => ({ range: e.range, newText: fill(e.newText, vars) })));
        };
        foot.append(write);
        box.append(foot);
      }
      function draw(clearExpr = true) {
        if (clearExpr) expr = null;
        const c = rgb();
        square.style.background = `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, ${hex(fromHsb([hsb[0], 1, 1]))})`;
        dot.style.left = `${hsb[1] * 100}%`; dot.style.top = `${(1 - hsb[2]) * 100}%`;
        knob.style.left = `${hsb[0] * 100}%`;
        swatch.querySelector('.tv-is').style.background = hex(c);
        for (const t of tabs.children) t.classList.toggle('on', t.textContent === model);
        if (document.activeElement !== spec) spec.value = SPEC[model](c);
        all.innerHTML = s.models.map((m) => `<span><b>${esc(m)}</b> ${esc(SPEC[m](c))}</span>`).join('');
        const w = foot.querySelector('.tv-primary');
        if (w) w.textContent = expr ? `Write ${expr}` : s.apply.edits.length > 1 ? `Define ${s.apply.name} as ${model} ${SPEC[model](c)}` : `Write [${model}]{${SPEC[model](c)}}`;
      }
      draw();
      return box;
    },
  };

  // drawings (tikz plugin): a picture's text as statements; nodes `\node[…] (n) at (x,y) {…};`
  // and arrows `\draw[->] (a) -- (b);` are edited, any other statement is kept as code
  const NUM = String.raw`(-?\d*\.?\d+)\s*(?:cm)?`;
  const NODE = new RegExp(String.raw`^\\(node|coordinate)\s*(?:\[([^\]]*)\])?\s*\(([\w -]+)\)\s*at\s*\(\s*${NUM}\s*,\s*${NUM}\s*\)\s*(?:\{([^{}]*)\})?\s*;$`);
  const EDGE = /^\\draw\s*(?:\[([^\]]*)\])?\s*\(([\w -]+)\)\s*--\s*(?:node\s*(?:\[([^\]]*)\])?\s*\{([^{}]*)\}\s*)?\(([\w -]+)\)\s*;$/;
  const PATH = /^\\draw\s*(?:\[[^\]]*\])?\s*(\([^()]*\)(?:\s*--\s*\([^()]*\))+)\s*;$/;
  const ARROWS = ['->', '<->', '<-', '-'];
  const opts = (s) => (s ?? '').split(',').map((o) => o.trim()).filter(Boolean);
  const fmt = (v) => String(+(+v).toFixed(3));
  /** A picture's body as statements `{text, lead, code, type…}` and the text after the last. */
  function parse(body) {
    const items = [];
    let depth = 0, from = 0, groups = 0, glued = false;
    const cut = (i, semi) => {
      const text = body.slice(from, i), code = text.replace(/^(\s|%[^\n]*\n?)*/, '');
      const it = { text, lead: text.slice(0, text.length - code.length), code, type: 'code' };
      const n = !glued && code.match(NODE), e = !glued && code.match(EDGE);
      if (n) Object.assign(it, { type: 'node', coordinate: n[1] === 'coordinate', opts: opts(n[2]), name: n[3].trim(), x: +n[4], y: +n[5], label: n[6] ?? '' });
      else if (e) {
        const o = opts(e[1]);
        Object.assign(it, { type: 'edge', arrow: o.find((x) => ARROWS.includes(x)) ?? '-', opts: o.filter((x) => !ARROWS.includes(x)), from: e[2].trim(), at: e[3] ?? '', label: e[4] ?? '', to: e[5].trim() });
      }
      items.push(it);
      from = i;
      // `\foreach \x in {…}` with its body on the next line: that statement is the loop's
      glued = !semi && groups < 2 && /^\\foreach\b/.test(code);
      groups = 0;
    };
    for (let i = 0; i < body.length; i++) {
      const c = body[i];
      if (c === '\\') { i++; continue; }
      if (c === '%') { while (i < body.length && body[i] !== '\n') i++; continue; }
      if ('{['.includes(c)) groups += depth++ === 0 && c === '{';
      else if ('}]'.includes(c)) {
        // a group closed at the end of its line (`\foreach … { … }`) ends a statement too
        if (--depth === 0 && c === '}' && /^[ \t]*(%[^\n]*)?\n/.test(body.slice(i + 1))) cut(i + 1, false);
      } else if (c === ';' && depth === 0) cut(i + 1, true);
    }
    return { items, tail: body.slice(from) };
  }
  const nodeText = (n) => n.coordinate ? `\\coordinate (${n.name}) at (${fmt(n.x)},${fmt(n.y)});`
    : `\\node${n.opts.length ? `[${n.opts.join(', ')}]` : ''} (${n.name}) at (${fmt(n.x)},${fmt(n.y)}) {${n.label}};`;
  const edgeText = (e) => {
    const o = [...(e.arrow === '-' ? [] : [e.arrow]), ...e.opts];
    return `\\draw${o.length ? `[${o.join(', ')}]` : ''} (${e.from}) -- ${e.label ? `node[${e.at || 'above'}] {${e.label}} ` : ''}(${e.to});`;
  };
  /** The body again: untouched statements as they were, edited and new ones written out. */
  function emit({ items, tail }) {
    const indent = items.find((i) => i.lead.includes('\n'))?.lead.split('\n').pop() ?? '  ';
    const text = (i) => (i.lead || `\n${indent}`) + (i.type === 'node' ? nodeText(i) : edgeText(i));
    const kept = items.filter((i) => !i.deleted && i.lead), added = items.filter((i) => !i.deleted && !i.lead);
    // new statements go after the last one's line, its comment included
    const line = tail.match(/^[^\n]*/)[0];
    return kept.map((i) => i.dirty ? text(i) : i.text).join('') + line + added.map(text).join('') + tail.slice(line.length);
  }
  // Mermaid flowcharts (`graph TD`, `A[x] -->|y| B((z))`)
  const MNODE = /^\s*([A-Za-z_][\w]*)\s*(?:\(\((.*?)\)\)|\[(.*?)\]|\((.*?)\)|\{(.*?)\})?/;
  const MLINK = /^\s*(?:(<-->|-->|---|==>|-\.->)|--\s*(.+?)\s*(-->|---))\s*(?:\|([^|]*)\|)?/;
  const unquote = (s) => s?.trim().replace(/^"(.*)"$/, '$1');
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
  const SHAPE_OPTS = { rectangle: ['draw'], circle: ['draw', 'circle'], rounded: ['draw', 'rounded corners'] };
  const shapeOf = (n) => n.opts?.includes('circle') ? 'circle' : n.opts?.includes('rounded corners') ? 'rounded' : n.shape ?? 'rectangle';
  /** The flowchart of `nodes` and `edges`, top-down unless wider than tall. */
  function mermaidOut(nodes, edges) {
    const xs = nodes.map((n) => n.x), ys = nodes.map((n) => n.y);
    const wide = nodes.length && Math.max(...xs) - Math.min(...xs) > Math.max(...ys) - Math.min(...ys);
    const id = (s) => s.replace(/\W/g, '_');
    const q = (s) => /[[\](){}|"]/.test(s) ? `"${s.replace(/"/g, '#quot;')}"` : s;
    const box = (n) => { const l = q(n.label || n.name); return { circle: `((${l}))`, rounded: `(${l})` }[shapeOf(n)] ?? `[${l}]`; };
    const link = { '->': '-->', '<->': '<-->', '<-': '-->', '-': '---' };
    return [`graph ${wide ? 'LR' : 'TD'}`, ...nodes.map((n) => `  ${id(n.name)}${box(n)}`),
      ...edges.map((e) => { const [a, b] = e.arrow === '<-' ? [e.to, e.from] : [e.from, e.to]; return `  ${id(a)} ${link[e.arrow] ?? '-->'}${e.label ? `|${q(e.label)}|` : ''} ${id(b)}`; })].join('\n');
  }
  /** Adds a flowchart to `state`: names already there are reused, the rest laid out below. */
  function importMermaid(state, src, spacing) {
    const g = mermaidIn(src), have = new Set(state.items.filter((i) => i.type === 'node' && !i.deleted).map((i) => i.name));
    const ys = state.items.filter((i) => i.type === 'node' && !i.deleted).map((i) => i.y);
    const below = ys.length ? Math.min(...ys) - spacing : 0;
    for (const n of layout(g, spacing)) if (!have.has(n.name)) state.items.push({ type: 'node', dirty: true, lead: '', name: n.name, label: n.label, opts: SHAPE_OPTS[n.shape], x: n.x, y: n.y + below });
    for (const e of g.edges) state.items.push({ type: 'edge', dirty: true, lead: '', from: e.from, to: e.to, arrow: e.arrow, opts: [], label: e.label, at: '' });
    return g;
  }

  // the dialog: an SVG canvas in cm (y up), a tool bar, the selection's label, code kept, Mermaid
  const NS = 'http://www.w3.org/2000/svg';
  const svgEl = (tag, attrs = {}) => { const e = document.createElementNS(NS, tag); for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v); return e; };
  function drawing(s, view, ctx) {
    const state = parse(s.body), grid = s.grid || 0.5, run = new Map((s.nodes ?? []).map((n) => [n.name, n]));
    // a node the run put elsewhere than its text says (a scope, a loop) is not plain: code
    for (const it of state.items) { const r = run.get(it.name); if (it.type === 'node' && r?.x != null && Math.hypot(r.x - it.x, r.y - it.y) > 0.05) it.type = 'code'; }
    let tool = 'select', arrow = '->', sel = null, pending = null, box = null;
    const live = () => state.items.filter((i) => !i.deleted);
    const own = () => live().filter((i) => i.type === 'node');
    // the nodes the run made that no statement here edits: read-only where the run placed them
    const fixed = () => [...run.values()].filter((n) => n.x != null && !own().some((o) => o.name === n.name));
    const nodes = () => [...own(), ...fixed().map((n) => ({ ...n, fixed: true, label: n.name }))];
    const size = (n) => { const r = run.get(n.name); return r?.w != null && !n.dirty ? [r.w, r.h] : n.coordinate ? [0.15, 0.15] : [Math.max(0.6, 0.19 * (n.label || ' ').length + 0.3), 0.6]; };
    const snap = (v) => Math.round(v / grid) * grid;
    const root = el('div', 'tv-draw');
    const bar = el('div', 'tv-tools');
    const TOOLS = [['select', 'Move'], ['rectangle', 'Rectangle'], ['circle', 'Circle'], ['connect', 'Connect'], ['delete', 'Delete']];
    for (const [k, label] of TOOLS) {
      const b = el('button', 'tv-tab', esc(label));
      b.type = 'button';
      b.dataset.tool = k;
      b.onclick = () => { tool = k; pending = null; draw(); };
      bar.append(b);
    }
    const arrows = el('select', 'tv-input', ['->', '<->', '--'].map((a) => `<option>${a}</option>`).join(''));
    arrows.ariaLabel = 'arrow';
    arrows.onchange = () => { arrow = arrows.value === '--' ? '-' : arrows.value; };
    bar.append(arrows, el('span', 'tv-muted', `grid ${grid} cm`));
    const svg = svgEl('svg', { class: 'tv-canvas', role: 'img', 'aria-label': 'drawing' });
    const side = el('div', 'tv-row tv-sel');
    const kept = el('div', 'tv-kept');
    const status = el('span', 'tv-muted');
    // where a pointer is, in cm
    const at = (e) => { const p = svg.createSVGPoint(); p.x = e.clientX; p.y = e.clientY; const q = p.matrixTransform(svg.getScreenCTM().inverse()); return [q.x, -q.y]; };
    const fresh = (base) => { let k = 1; while (nodes().some((n) => n.name === `${base}${k}`)) k++; return `${base}${k}`; };
    const add = (it) => { state.items.push({ dirty: true, lead: '', opts: [], label: '', ...it }); };
    const pick = (n) => {
      if (tool === 'delete') return remove(n);
      if (tool === 'connect') {
        if (!pending) { pending = n.name; return draw(); }
        if (pending !== n.name) add({ type: 'edge', from: pending, to: n.name, arrow, at: '' });
        pending = null;
        return draw(true);
      }
      sel = n;
      draw();
    };
    const remove = (it) => {
      if (it.fixed) return;
      it.deleted = true;
      if (it.type === 'node') for (const e of live()) if (e.type === 'edge' && (e.from === it.name || e.to === it.name)) e.deleted = true;
      sel = null;
      draw(true);
    };
    svg.onpointerdown = (e) => {
      if (e.target !== svg && !e.target.classList.contains('tv-grid')) return;
      if (tool !== 'rectangle' && tool !== 'circle') { sel = null; pending = null; return draw(); }
      const [x, y] = at(e).map(snap);
      add({ type: 'node', name: fresh('n'), label: 'Node', opts: SHAPE_OPTS[tool], x, y });
      sel = state.items[state.items.length - 1];
      tool = 'select';
      draw(true);
      side.querySelector('input')?.select();
    };
    const drag = (g, n) => g.onpointerdown = (e) => {
      e.stopPropagation();
      if (n.fixed || tool !== 'select') return pick(n);
      sel = n;
      const [x0, y0] = at(e), [nx, ny] = [n.x, n.y];
      try { svg.setPointerCapture(e.pointerId); } catch { /* a synthetic pointer */ }
      svg.onpointermove = (ev) => { const [x, y] = at(ev); n.x = snap(nx + x - x0); n.y = snap(ny + y - y0); n.dirty ||= n.x !== nx || n.y !== ny; draw(); };
      svg.onpointerup = () => { svg.onpointermove = svg.onpointerup = null; draw(true); };
      draw();
    };
    function draw(refit = false) {
      for (const b of bar.querySelectorAll('[data-tool]')) b.classList.toggle('on', b.dataset.tool === tool);
      const all = nodes(), by = new Map(all.map((n) => [n.name, n]));
      if (refit || !box) {
        const xs = all.flatMap((n) => [n.x - size(n)[0] / 2, n.x + size(n)[0] / 2]), ys = all.flatMap((n) => [n.y - size(n)[1] / 2, n.y + size(n)[1] / 2]);
        for (const it of live()) for (const [x, y] of it.path ?? []) { xs.push(x); ys.push(y); }
        const lo = [Math.min(0, ...xs) - 1, Math.min(0, ...ys) - 1], hi = [Math.max(4, ...xs) + 1, Math.max(2, ...ys) + 1];
        box = [Math.floor(lo[0]), Math.floor(lo[1]), Math.ceil(hi[0]) - Math.floor(lo[0]), Math.ceil(hi[1]) - Math.floor(lo[1])];
        // at least 4:3, widened on both sides, so the grid fills the canvas
        const wide = Math.ceil(Math.max(0, box[3] * 4 / 3 - box[2]) / 2);
        box = [box[0] - wide, box[1], box[2] + 2 * wide, box[3]];
        svg.setAttribute('viewBox', `${box[0]} ${-box[1] - box[3]} ${box[2]} ${box[3]}`);
      }
      const lines = [];
      for (let x = box[0]; x <= box[0] + box[2]; x += grid) lines.push(`M${x} ${-box[1]}V${-box[1] - box[3]}`);
      for (let y = box[1]; y <= box[1] + box[3]; y += grid) lines.push(`M${box[0]} ${-y}H${box[0] + box[2]}`);
      const out = [svgEl('path', { class: 'tv-grid', d: lines.join('') }), svgEl('defs')];
      out[1].innerHTML = `<marker id="tv-tip" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="currentColor"/></marker>`;
      // the plain paths kept as code
      for (const it of live()) if (it.path) out.push(svgEl('polyline', { class: 'tv-path', points: it.path.map(([x, y]) => `${x},${-y}`).join(' ') }));
      for (const e of live().filter((i) => i.type === 'edge')) {
        const a = by.get(e.from), b = by.get(e.to);
        if (!a || !b) continue;
        // from border to border: cut each end where the line leaves its node's box
        const end = (p, q) => { const [w, h] = size(p), dx = q.x - p.x, dy = q.y - p.y, t = Math.min(Math.abs(w / 2 / (dx || 1e-9)), Math.abs(h / 2 / (dy || 1e-9)), 0.5); return [p.x + dx * t, p.y + dy * t]; };
        const [p, q] = [end(a, b), end(b, a)];
        const g = svgEl('g', { class: `tv-edge${sel === e ? ' on' : ''}` });
        const line = svgEl('line', { x1: p[0], y1: -p[1], x2: q[0], y2: -q[1] });
        if (e.arrow.includes('>')) line.setAttribute('marker-end', 'url(#tv-tip)');
        if (e.arrow.includes('<')) line.setAttribute('marker-start', 'url(#tv-tip)');
        const hit = svgEl('line', { x1: p[0], y1: -p[1], x2: q[0], y2: -q[1], class: 'tv-hit' });
        g.append(hit, line);
        if (e.label) g.append(Object.assign(svgEl('text', { x: (p[0] + q[0]) / 2, y: -(p[1] + q[1]) / 2 - 0.15 }), { textContent: e.label }));
        g.onpointerdown = (ev) => { ev.stopPropagation(); if (tool === 'delete') remove(e); else { sel = e; draw(); } };
        out.push(g);
      }
      for (const n of all) {
        const [w, h] = size(n), shape = shapeOf(n);
        const g = svgEl('g', { class: `tv-node${n.fixed ? ' fixed' : ''}${n.opts?.includes('draw') || n.fixed ? '' : ' bare'}${sel === n ? ' on' : ''}${pending === n.name ? ' pending' : ''}` });
        g.append(shape === 'circle' ? svgEl('circle', { cx: n.x, cy: -n.y, r: Math.max(w, h) / 2 })
          : svgEl('rect', { x: n.x - w / 2, y: -n.y - h / 2, width: w, height: h, rx: shape === 'rounded' ? 0.15 : 0 }));
        g.append(Object.assign(svgEl('text', { x: n.x, y: -n.y }), { textContent: n.coordinate ? '' : n.label }));
        if (n.fixed) g.append(Object.assign(svgEl('title'), { textContent: `${n.name}: made by code (line ${n.line + 1}), kept as it is` }));
        drag(g, n);
        out.push(g);
      }
      svg.replaceChildren(...out);
      // the selection: its label, or why it is read-only
      side.replaceChildren();
      if (sel && !sel.deleted && !sel.fixed && !sel.coordinate) {
        const name = el('span', 'tv-muted', esc(sel.type === 'edge' ? `${sel.from} ${sel.arrow === '-' ? '--' : sel.arrow} ${sel.to}` : `(${sel.name})`));
        const label = Object.assign(el('input', 'tv-input'), { value: sel.label, placeholder: 'label', spellcheck: false, ariaLabel: 'label' });
        label.oninput = () => { sel.label = label.value; sel.dirty = true; draw(); side.querySelector('input').focus(); };
        const del = el('button', 'tv-button', 'Delete');
        del.type = 'button';
        del.onclick = () => remove(sel);
        side.append(name, label, del);
      } else side.append(el('span', 'tv-muted', esc(tool === 'connect' ? (pending ? `from (${pending}): click the target node` : 'click the node an arrow starts at') : tool === 'select' ? 'drag a node; click one to edit its label' : tool === 'delete' ? 'click what to delete' : 'click the canvas to place a node')));
      const changed = state.items.filter((i) => i.dirty || i.deleted).length;
      status.textContent = changed ? `${changed} statement${changed > 1 ? 's' : ''} to write` : 'no changes';
      write.disabled = !changed && !s.wrap;
    }
    // what stays code: listed read-only, never dropped
    const code = state.items.filter((i) => i.type === 'code' && i.code.trim());
    for (const it of code) { const m = it.code.match(PATH); if (m) { const pts = [...m[1].matchAll(/\(\s*(-?[\d.]+)\s*(?:cm)?\s*,\s*(-?[\d.]+)\s*(?:cm)?\s*\)/g)].map((p) => [+p[1], +p[2]]); if (pts.length === m[1].split('--').length) it.path = pts; } }
    if (code.length) kept.innerHTML = `<h4 class="tv-h">Kept as code (${code.length})</h4>` + code.map((i) => `<pre>${esc(i.code.length > 160 ? i.code.slice(0, 160) + '…' : i.code)}</pre>`).join('');
    // Mermaid in and out
    const mer = el('details', 'tv-mermaid', '<summary>Mermaid flowchart</summary>');
    const text = Object.assign(el('textarea', 'tv-input'), { rows: 5, spellcheck: false, placeholder: 'graph TD\n  A[Start] --> B((End))', ariaLabel: 'Mermaid' });
    const mrow = el('div', 'tv-actions');
    const button = (label, f) => { const b = el('button', 'tv-button', label); b.type = 'button'; b.onclick = f; mrow.append(b); };
    button('Import', () => { if (importMermaid(state, text.value, s.spacing || 2).nodes.length) draw(true); });
    button('Export', () => { text.value = mermaidOut(nodes(), live().filter((i) => i.type === 'edge')); text.select(); });
    mer.append(text, mrow);
    const foot = el('div', 'tv-actions tv-foot');
    const write = el('button', 'tv-button tv-primary', s.wrap ? 'Insert drawing' : 'Write');
    write.type = 'button';
    write.onclick = () => {
      const body = emit(state);
      ctx.apply([{ range: s.range, newText: s.wrap ? `\\begin{tikzpicture}${body}\n\\end{tikzpicture}` : body }, ...(s.extra ? [s.extra] : [])]);
    };
    foot.append(status, write);
    root.append(bar, svg, side, mer, kept);
    if (s.note) root.append(el('div', 'tv-text', ctx.markdown(s.note)));
    root.append(foot);
    draw(true);
    return root;
  }
  RENDER.drawing = drawing;

  const STYLE = `
.tv { --tv-bg: var(--panel, var(--vscode-editor-background, #fff)); --tv-fg: var(--fg, var(--vscode-foreground, #222)); --tv-muted: var(--muted, var(--vscode-descriptionForeground, #666));
  --tv-border: var(--border, var(--vscode-widget-border, #ccc)); --tv-accent: var(--accent, var(--vscode-button-background, #0b3d91)); --tv-accent-fg: var(--accent-fg, var(--vscode-button-foreground, #fff));
  --tv-sunk: var(--sunk, var(--vscode-input-background, #f1f3f6)); color: var(--tv-fg); display: grid; gap: .8rem; font-size: .9rem; }
.tv h3.tv-title { margin: 0; font-size: 1rem; }
.tv-h { margin: .2rem 0 0; font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .04em; color: var(--tv-muted); }
.tv-text p { margin: .2rem 0; } .tv-text { color: var(--tv-muted); font-size: .82rem; }
.tv-table { border-collapse: collapse; } .tv-table td, .tv-table th { padding: .2rem .6rem; border-bottom: 1px solid var(--tv-border); text-align: left; }
.tv-actions { display: flex; flex-wrap: wrap; gap: .4rem; }
.tv-button { font: inherit; font-size: .82rem; padding: .35rem .7rem; border-radius: 6px; border: 1px solid var(--tv-border); background: var(--tv-sunk); color: var(--tv-fg); cursor: pointer; }
.tv-button:hover { border-color: var(--tv-accent); } .tv-primary { background: var(--tv-accent); color: var(--tv-accent-fg); border-color: var(--tv-accent); }
.tv-input { font: inherit; font-size: .88rem; padding: .3rem .5rem; border-radius: 6px; border: 1px solid var(--tv-border); background: var(--tv-sunk); color: var(--tv-fg); min-width: 0; }
.tv-input.tv-bad { border-color: var(--error, #c33); }
.tv-converter { display: grid; gap: .5rem; } .tv-row { display: flex; gap: .4rem; } .tv-row input { flex: 1; font-size: 1.1rem; }
.tv-units { display: grid; grid-template-columns: repeat(auto-fill, minmax(6.5rem, 1fr)); gap: .4rem; }
.tv-unit { font: inherit; display: flex; flex-direction: column; align-items: flex-start; gap: .1rem; padding: .45rem .6rem; border-radius: 8px; border: 1px solid var(--tv-border); background: var(--tv-sunk); color: var(--tv-fg); cursor: pointer; text-align: left; }
.tv-unit b { font-variant-numeric: tabular-nums; font-weight: 600; } .tv-unit span { font-size: .72rem; color: var(--tv-muted); }
.tv-unit:not(:disabled):hover { border-color: var(--tv-accent); } .tv-unit:disabled { cursor: default; opacity: .85; }
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
.tv-tabs { display: flex; flex-wrap: wrap; gap: .25rem; }
.tv-tab { font: inherit; font-size: .75rem; padding: .2rem .5rem; border-radius: 999px; border: 1px solid var(--tv-border); background: none; color: var(--tv-muted); cursor: pointer; }
.tv-tab.on { background: var(--tv-accent); color: var(--tv-accent-fg); border-color: var(--tv-accent); }
.tv-spec { font-family: ui-monospace, monospace; }
.tv-specs { display: grid; gap: .1rem; font-family: ui-monospace, monospace; font-size: .75rem; color: var(--tv-muted); }
.tv-specs b { display: inline-block; min-width: 3.2rem; color: var(--tv-fg); font-weight: 600; }
.tv-expr { display: flex; align-items: center; gap: .4rem; font-size: .8rem; color: var(--tv-muted); } .tv-expr input { flex: 1; font-family: ui-monospace, monospace; }
.tv-defined { display: flex; flex-wrap: wrap; gap: .3rem; max-height: 7.5rem; overflow: auto; }
.tv-chip { font: inherit; font-size: .78rem; display: inline-flex; align-items: center; gap: .3rem; padding: .15rem .5rem .15rem .25rem; border-radius: 999px; border: 1px solid var(--tv-border); background: var(--tv-sunk); color: var(--tv-fg); cursor: pointer; }
.tv-chip i { width: .9rem; height: .9rem; border-radius: 50%; border: 1px solid #8886; }
.tv-chip:hover { border-color: var(--tv-accent); }
.tv-records { display: grid; gap: .5rem; } .tv-records > .tv-list { max-height: 60vh; }
.tv-record.on { border: 1px solid var(--tv-border); border-radius: 8px; background: var(--tv-sunk); }
.tv-record > .tv-choice { width: 100%; } .tv-record > .tv-choice b { font-weight: 600; } .tv-badge { font-style: normal; font-weight: 400; font-size: .7rem; padding: 0 .4rem; border-radius: 999px; border: 1px solid var(--tv-border); color: var(--tv-muted); }
.tv-body { display: grid; gap: .6rem; padding: .3rem .6rem .7rem; min-width: 0; } .tv-body .tv-table { font-size: .8rem; display: block; overflow-x: auto; }
.tv-form { display: grid; gap: .4rem; } .tv-field { display: grid; grid-template-columns: 7rem 1fr; gap: .5rem; align-items: center; font-size: .8rem; color: var(--tv-muted); }
.tv-field textarea { min-height: 3.2rem; resize: vertical; } .tv-field .tv-input { font-family: ui-monospace, monospace; font-size: .82rem; }
.tv-button:disabled { opacity: .5; cursor: not-allowed; }
.tv-add summary { cursor: pointer; font-size: .85rem; color: var(--tv-accent); } .tv-add[open] { display: grid; gap: .5rem; } .tv-add > select { justify-self: start; }
@media (max-width: 560px) { .tv-field { grid-template-columns: 1fr; gap: .15rem; } }
@media (max-width: 560px) { .tv-color { grid-template-columns: 1fr; } .tv-pick { max-width: 16rem; } }
.tv-draw { display: grid; gap: .5rem; min-width: min(40rem, 100%); } .tv-tools { display: flex; flex-wrap: wrap; gap: .3rem; align-items: center; }
.tv-muted { color: var(--tv-muted); font-size: .78rem; } .tv-sel { align-items: center; min-height: 2rem; } .tv-sel input { flex: 1; font-size: .88rem; }
.tv-canvas { width: 100%; max-height: 55vh; background: var(--tv-sunk); border: 1px solid var(--tv-border); border-radius: 8px; touch-action: none; color: var(--tv-fg); font-family: ui-sans-serif, system-ui, sans-serif; }
.tv-grid { stroke: var(--tv-border); stroke-width: .01; fill: none; }
.tv-canvas text { font-size: .32px; text-anchor: middle; dominant-baseline: central; fill: currentColor; pointer-events: none; }
.tv-node { cursor: grab; } .tv-node rect, .tv-node circle { fill: var(--tv-bg); stroke: currentColor; stroke-width: .03; }
.tv-node.bare rect, .tv-node.bare circle { stroke-dasharray: .04 .06; stroke-width: .015; }
.tv-node.fixed { cursor: not-allowed; color: var(--tv-muted); } .tv-node.fixed rect, .tv-node.fixed circle { stroke-dasharray: .1 .07; }
.tv-node.on rect, .tv-node.on circle, .tv-node.pending rect, .tv-node.pending circle { stroke: var(--tv-accent); stroke-width: .06; }
.tv-edge line { stroke: currentColor; stroke-width: .03; } .tv-edge line.tv-hit { stroke: transparent; stroke-width: .3; cursor: pointer; }
.tv-edge.on line:not(.tv-hit) { stroke: var(--tv-accent); stroke-width: .05; } .tv-edge text { font-size: .26px; }
.tv-path { fill: none; stroke: var(--tv-muted); stroke-width: .03; stroke-dasharray: .1 .07; }
.tv-kept pre { margin: .15rem 0; padding: .3rem .5rem; border-radius: 6px; background: var(--tv-sunk); font-size: .75rem; white-space: pre-wrap; color: var(--tv-muted); }
.tv-mermaid summary { cursor: pointer; font-size: .82rem; color: var(--tv-muted); } .tv-mermaid textarea { width: 100%; box-sizing: border-box; font-family: ui-monospace, monospace; margin: .4rem 0; }
.tv-foot { justify-content: flex-end; align-items: center; } .tv-foot .tv-muted { margin-right: auto; }`;

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
  self.satexViews = { render, RENDER, SPEC, mix, drawing: { parse, emit, mermaidIn, mermaidOut, layout, importMermaid } };
})();
