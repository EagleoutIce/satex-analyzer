// The type plugins' views (`satex/view`, src/lsp/plugins): one renderer for the playground and
// the VS Code webview. A view is `{plugin, title, sections: [{kind, …}]}`; each kind has a
// renderer in `RENDER`, and edits go back through `ctx.apply(edits)` as LSP TextEdits.
(() => {
  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  const el = (tag, cls, html) => Object.assign(document.createElement(tag), cls ? { className: cls } : {}, html != null ? { innerHTML: html } : {});
  const fill = (text, vars) => text.replace(/\$\{(\w+)\}/g, (m, k) => vars[k] ?? m);

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
@media (max-width: 560px) { .tv-color { grid-template-columns: 1fr; } .tv-pick { max-width: 16rem; } }`;

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
  self.satexViews = { render, RENDER, SPEC, mix };
})();
