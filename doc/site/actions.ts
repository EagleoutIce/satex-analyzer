// Page actions: what a right click on the PDF or the Typeset view offers on a thing drawn there (a TikZ node, path or
// label, a forest node, a table column). Each plugin names the targets it serves and gives grouped menu items; an item
// writes through the context the playground hands it (`ctx.write`: one undo step through the snapshot printer). The
// playground only finds the target under the pointer and dispatches here. A plugin is switched per viewer (Playground
// settings) and with its type plugin (`type_plugins.<name>.enabled`).
export {};
declare global {
  /** A menu row: a choice (`run`), a submenu (`items`), radio state `on`, a color `swatch`. */
  type PaItem = { label: string; run?: () => void; items?: PaItem[]; on?: boolean; swatch?: string; hint?: string };
  /** A group of rows: a submenu row by its title, or its rows inline. */
  type PaGroup = { title: string; items: PaItem[]; inline?: boolean };
  type PaEdit = { from: number; to: number; now: string };
  /** What the playground hands a plugin: the text the menu opened on, writes, a prompt, the run's facts. */
  interface PaCtx {
    text: string; all: string; x: number; y: number; facts: Rec | null;
    write(edits: (PaEdit | null)[]): void; toast(msg: string): void;
    pick(prompt: string, done: (n: TikzNode) => void): void; colAlign(t: number, k: number, a: string): void;
  }
  type PaTarget = { kind: string; [k: string]: any };
  interface PaPlugin { name: string; title: string; about: string; type: string; targets: string[]; groups(t: PaTarget, ctx: PaCtx): PaGroup[] }
  interface Window {
    satexPageActions: {
      register(p: PaPlugin): void; plugins: PaPlugin[]; on(name: string, cfg?: Record<string, unknown>): boolean; setOn(name: string, on: boolean): void;
      groups(t: PaTarget, ctx: PaCtx, cfg?: Record<string, unknown>): PaGroup[]; open(x: number, y: number, title: string, groups: PaGroup[]): HTMLElement | null; close(): void;
      ask(x: number, y: number, value: string, what: string, done: (v: string) => void): void;
      libEdit(text: string, all: string, loaded: string[], lib: string): { have: true } | { edit: PaEdit } | { line: string };
      colors(all: string, facts: Rec | null): { doc: [string, string][]; base: [string, string][]; more: [string, string][] };
      mix(expr: string, known: Map<string, number[]>): number[] | null; fontSet(font: string | undefined, size: string | null): string | null;
      cut(text: string, from: number, to: number): PaEdit;
    };
  }
}
(() => {
  const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!);
  const plugins: PaPlugin[] = [];
  const OFF = 'page-actions-off';
  const offSet = () => { try { return new Set<string>(self.satexStore?.get(OFF) ?? []); } catch { return new Set<string>(); } };
  /** Whether plugin `name` is on: this viewer's switch and its type plugin's (`type_plugins.tikz.enabled` in `cfg`). */
  function on(name: string, cfg: Record<string, unknown> = {}) {
    const p = plugins.find((x) => x.name === name);
    return !!p && !offSet().has(name) && cfg[`type_plugins.${p.type}.enabled`] !== false;
  }
  function setOn(name: string, yes: boolean) {
    const s = offSet();
    if (yes) s.delete(name); else s.add(name);
    self.satexStore?.set(OFF, s.size ? [...s] : null);
  }
  const groups = (t: PaTarget, ctx: PaCtx, cfg: Record<string, unknown> = {}) => plugins.filter((p) => p.targets.includes(t.kind) && on(p.name, cfg)).flatMap((p) => p.groups(t, ctx));

  // ---------------------------------------------------------------- the menu
  const close = () => { for (const el of document.querySelectorAll('.pa-menu, .tikz-input, .pa-mix')) el.remove(); };
  /** One menu box of `rows` at screen point (x, y), nested under `parent` (a row) when given. */
  function box(rows: (PaItem | 'hr' | string)[], x: number, y: number, parent: HTMLElement | null) {
    const m = Object.assign(document.createElement('div'), { className: `pa-menu${parent ? ' pa-sub' : ''}` });
    m.setAttribute('role', 'menu');
    const acts: PaItem[] = [];
    m.innerHTML = rows.map((r) => {
      if (r === 'hr') return '<hr>';
      if (typeof r === 'string') return `<b>${esc(r)}</b>`;
      acts.push(r);
      const role = r.items ? 'menuitem' : r.on == null ? 'menuitem' : 'menuitemradio';
      return `<button type="button" role="${role}"${r.on == null ? '' : ` aria-checked="${r.on}"`}${r.items ? ' aria-haspopup="menu" aria-expanded="false"' : ''} data-i="${acts.length - 1}">` +
        `${r.swatch ? `<i class="pa-sw" style="background:${esc(r.swatch)}"></i>` : ''}<span>${esc(r.label)}</span>${r.items ? '<span class="pa-more">▸</span>' : r.hint ? `<small>${esc(r.hint)}</small>` : ''}</button>`;
    }).join('');
    document.body.append(m);
    const w = m.offsetWidth, h = m.offsetHeight;
    m.style.left = `${Math.max(4, x + w > innerWidth - 4 ? (parent ? parent.getBoundingClientRect().left - w - 2 : innerWidth - w - 8) : x)}px`;
    m.style.top = `${Math.max(4, Math.min(y, innerHeight - h - 8))}px`;
    (m as any).acts = acts;
    (m as any).up = parent;
    return m;
  }
  /** Opens the submenu of row `b`, its first row focused when `focus`. */
  function sub(b: HTMLElement, focus: boolean) {
    const m = b.closest('.pa-menu') as any, it: PaItem = m.acts[+b.dataset.i!];
    if (!it?.items) return;
    for (const o of [...document.querySelectorAll('.pa-sub')] as any[]) if (!isAbove(o, m)) shut(o);
    if (b.getAttribute('aria-expanded') !== 'true') {
      const r = b.getBoundingClientRect(), s = box(it.items, r.right + 2, r.top - 4, b);
      b.setAttribute('aria-expanded', 'true');
      (b as any).sub = s;
    }
    if (focus) ((b as any).sub?.querySelector('button') as HTMLElement | null)?.focus();
  }
  /** Whether menu `o` is `m` or one `m` opened from. */
  const isAbove = (o: any, m: any): boolean => { for (let x = m; x; x = x.up?.closest('.pa-menu')) if (x === o) return true; return false; };
  function shut(s: any) { for (const o of [...document.querySelectorAll('.pa-sub')] as any[]) if (o !== s && isAbove(s, o)) o.remove(); s.up?.setAttribute('aria-expanded', 'false'); if (s.up) s.up.sub = null; s.remove(); }
  /** The menu titled `title` of `groups` at (x, y): a group a submenu row, an inline one its rows between rules. */
  function open(x: number, y: number, title: string, gs: PaGroup[]) {
    close();
    if (!gs.length) return null;
    const rows: (PaItem | 'hr' | string)[] = [title];
    let wasInline = false;
    for (const g of gs) {
      if (g.inline) { if (rows.length > 1) rows.push('hr'); rows.push(...g.items); wasInline = true; continue; }
      if (wasInline) { rows.push('hr'); wasInline = false; }
      if (g.items.length) rows.push({ label: g.title, items: g.items });
    }
    const m = box(rows, x, y, null);
    m.classList.add('pa-root');
    const run = (b: HTMLElement) => {
      const it: PaItem = (b.closest('.pa-menu') as any).acts[+b.dataset.i!];
      if (it.items) return sub(b, true);
      close();
      it.run?.();
    };
    const root = (e: Event) => (e.target as HTMLElement).closest?.('.pa-menu');
    const click = (e: MouseEvent) => { const b = (e.target as HTMLElement).closest?.('.pa-menu button[data-i]') as HTMLElement | null; if (b) { e.stopPropagation(); run(b); } };
    const over = (e: MouseEvent) => {
      const b = (e.target as HTMLElement).closest?.('.pa-menu button[data-i]') as HTMLElement | null;
      if (!b) return;
      b.focus({ preventScroll: true });
      const mm = b.closest('.pa-menu') as any;
      for (const o of [...document.querySelectorAll('.pa-sub')] as any[]) if (o.up?.closest('.pa-menu') === mm && o.up !== b) shut(o);
      if (b.getAttribute('aria-haspopup')) sub(b, false);
    };
    const key = (e: KeyboardEvent) => {
      const b = document.activeElement?.closest?.('.pa-menu button') as HTMLElement | null, mm = (b?.closest('.pa-menu') ?? m) as any;
      const bs = [...mm.querySelectorAll(':scope > button')] as HTMLElement[], i = bs.indexOf(b!);
      const go: Record<string, () => void> = {
        ArrowDown: () => bs[(i + 1) % bs.length]?.focus(), ArrowUp: () => bs[(i - 1 + bs.length) % bs.length]?.focus(),
        ArrowRight: () => b && sub(b, true), ArrowLeft: () => { if (mm.up) { const u = mm.up; shut(mm); u.focus(); } },
        Escape: () => { if (mm.up) { const u = mm.up; shut(mm); u.focus(); } else close(); }, Enter: () => b && run(b), ' ': () => b && run(b),
      };
      if (!go[e.key]) return;
      e.preventDefault(); e.stopPropagation();
      go[e.key]();
    };
    // the menus are many boxes: one listener on the document while one is open
    const off = () => { removeEventListener('click', click, true); removeEventListener('mouseover', over, true); removeEventListener('keydown', key, true); removeEventListener('pointerdown', down, true); };
    const down = (e: PointerEvent) => { if (!root(e)) { close(); off(); } };
    addEventListener('click', click, true); addEventListener('mouseover', over, true); addEventListener('keydown', key, true); addEventListener('pointerdown', down, true);
    new MutationObserver((_, ob) => { if (!m.isConnected) { off(); ob.disconnect(); } }).observe(document.body, { childList: true });
    (m.querySelector('button') as HTMLElement | null)?.focus();
    return m;
  }
  /** A one-line input at (x, y): Enter writes `done(value)`, Escape leaves it. */
  function ask(x: number, y: number, value: string, what: string, done: (v: string) => void) {
    close();
    const b = Object.assign(document.createElement('div'), { className: 'tikz-input' });
    b.innerHTML = `<input type="text" aria-label="${esc(what)}" placeholder="${esc(what)}"><small>Enter writes it, Escape leaves it</small>`;
    b.style.left = `${Math.max(8, Math.min(x - 120, innerWidth - 260))}px`;
    b.style.top = `${Math.max(8, Math.min(y - 16, innerHeight - 60))}px`;
    document.body.append(b);
    const input = b.querySelector('input')!;
    input.value = value;
    input.focus(); input.select();
    input.onkeydown = (k) => {
      k.stopPropagation();
      if (k.key === 'Escape') { k.preventDefault(); close(); }
      if (k.key === 'Enter') { k.preventDefault(); const t = input.value; close(); if (t !== value) done(t); }
    };
    const away = (e: PointerEvent) => { if (!(e.target as HTMLElement).closest?.('.tikz-input')) { b.remove(); removeEventListener('pointerdown', away, true); } };
    addEventListener('pointerdown', away, true);
  }

  // ---------------------------------------------------------------- colors and libraries
  /** xcolor's base colors (xcolor manual, § 4.1): offered when the run has not listed the colors. */
  const BASE: [string, number[]][] = [['black', [0, 0, 0]], ['darkgray', [.25, .25, .25]], ['gray', [.5, .5, .5]], ['lightgray', [.75, .75, .75]], ['white', [1, 1, 1]],
    ['red', [1, 0, 0]], ['orange', [1, .5, 0]], ['yellow', [1, 1, 0]], ['lime', [.75, 1, 0]], ['green', [0, 1, 0]], ['olive', [.5, .5, 0]], ['teal', [0, .5, .5]], ['cyan', [0, 1, 1]],
    ['blue', [0, 0, 1]], ['violet', [.5, 0, .5]], ['purple', [.75, 0, .25]], ['magenta', [1, 0, 1]], ['pink', [1, .75, .75]], ['brown', [.75, .5, .25]]];
  const hex = (c: number[]) => `#${c.map((v) => Math.round(Math.max(0, Math.min(1, v)) * 255).toString(16).padStart(2, '0')).join('')}`;
  const rgb = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
  /** An xcolor expression `a!p!b!q…` (xcolor § 2.3.2): p% of a, the rest the next color (white when none). */
  function mix(expr: string, known: Map<string, number[]>): number[] | null {
    const ps = expr.split('!').map((x) => x.trim());
    let c = known.get(ps[0]!);
    if (!c) return null;
    for (let k = 1; k < ps.length; k += 2) {
      const p = +ps[k]! / 100, o = ps[k + 1] == null ? [1, 1, 1] : known.get(ps[k + 1]!);
      if (!(p >= 0 && p <= 1) || !o) return null;
      c = c.map((v, j) => p * v + (1 - p) * o[j]!);
    }
    return c;
  }
  /** A `\definecolor` spec as rgb: models HTML, rgb, RGB, gray, cmy(k). */
  function spec(model: string, v: string): number[] | null {
    const n = v.split(',').map((x) => +x.trim());
    if (model === 'HTML' && /^[0-9A-Fa-f]{6}$/.test(v.trim())) return rgb(`#${v.trim()}`);
    if (model === 'rgb' && n.length === 3) return n;
    if (model === 'RGB' && n.length === 3) return n.map((x) => x / 255);
    if (model === 'gray' && n.length === 1) return [n[0]!, n[0]!, n[0]!];
    if (model === 'cmyk' && n.length === 4) return n.slice(0, 3).map((x) => (1 - x) * (1 - n[3]!));
    if (model === 'cmy' && n.length === 3) return n.map((x) => 1 - x);
    return null;
  }
  /** The colors to offer: the document's own (`\definecolor`, `\colorlet`) first, xcolor's base ones, then the rest the run knows; each [name, css]. */
  function colors(all: string, facts: Rec | null) {
    const known = new Map<string, number[]>([...((facts?.colors ?? []) as Rec[]).filter((c) => /^#[0-9a-f]{6}$/i.test(c.hex ?? '')).map((c): [string, number[]] => [c.name, rgb(c.hex)])]);
    if (!known.size) for (const [n, c] of BASE) known.set(n, c);
    const doc: [string, string][] = [];
    for (const m of all.matchAll(/\\(definecolor|colorlet)\s*(?:\[[^\]]*\])?\s*\{([^}]+)\}\s*(?:\{([^}]*)\}\s*\{([^}]*)\}|\{([^}]*)\})?/g)) {
      const name = m[2]!.trim(), c = known.get(name) ?? (m[1] === 'definecolor' ? spec(m[3] ?? '', m[4] ?? '') : mix(m[5] ?? '', known));
      if (c) known.set(name, c);
      if (!doc.some(([n]) => n === name)) doc.push([name, c ? hex(c) : 'transparent']);
    }
    const base = BASE.filter(([n]) => known.has(n) && !doc.some(([d]) => d === n)).map(([n]): [string, string] => [n, hex(known.get(n)!)]);
    const more = [...known].filter(([n]) => !doc.some(([d]) => d === n) && !base.some(([b]) => b === n)).map(([n, c]): [string, string] => [n, hex(c)]).sort((a, b) => a[0].localeCompare(b[0]));
    return { doc, base, more };
  }
  /** How to have TikZ library `lib`: loaded (by the run or a `\usetikzlibrary` of the files), else its load after the last
   * `\usetikzlibrary` or the tikz `\usepackage` of this file, else the line to add by hand. */
  function libEdit(text: string, all: string, loaded: string[], lib: string): { have: true } | { edit: PaEdit } | { line: string } {
    const name = lib.replace(/\./g, '\\.');
    if (loaded.includes(lib) || new RegExp(`\\\\usetikzlibrary\\s*\\{[^}]*(?<![\\w.])${name}(?![\\w.])`).test(all.replace(/(?<!\\)%.*$/gm, ''))) return { have: true };
    const line = `\\usetikzlibrary{${lib}}`, end = /\\begin\s*\{document\}/.exec(text)?.index ?? text.length;
    const anchor = [...text.slice(0, end).matchAll(/^[^%\n]*\\(?:usetikzlibrary\s*\{[^}]*\}|usepackage\s*(?:\[[^\]]*\])?\s*\{[^}]*\btikz\b[^}]*\})[^\n]*\n/gm)].at(-1);
    return anchor ? { edit: { from: anchor.index! + anchor[0].length, to: anchor.index! + anchor[0].length, now: `${line}\n` } } : { line };
  }
  const SIZES = ['\\tiny', '\\scriptsize', '\\footnotesize', '\\small', '\\normalsize', '\\large', '\\Large', '\\LARGE', '\\huge', '\\Huge'];
  /** A `font=` value with its size command replaced by `size` (null: none), the rest (`\bfseries`) kept; null when nothing is left. */
  function fontSet(font: string | undefined, size: string | null) {
    const rest = (font ?? '').replace(/^\{|\}$/g, '').replace(/\\(tiny|scriptsize|footnotesize|small|normalsize|large|Large|LARGE|huge|Huge)(?![A-Za-z])/g, '').trim();
    const v = `${size ?? ''}${rest}`;
    return v ? v : null;
  }
  /** Removing `from`..`to`: its whole line when nothing else stands on it. */
  function cut(text: string, from: number, to: number): PaEdit {
    const a = text.lastIndexOf('\n', from - 1) + 1, e = text.indexOf('\n', to), b = e < 0 ? text.length : e + 1;
    return text.slice(a, from).trim() || text.slice(to, b).trim() ? { from, to, now: '' } : { from: a, to: b, now: '' };
  }
  self.satexPageActions = { register: (p) => { plugins.push(p); }, plugins, on, setOn, groups, open, close, ask, libEdit, colors, mix, fontSet, cut };

  // ---------------------------------------------------------------- the plugins
  const TM = () => self.satexTikzMove;
  /** Radio rows: `[label, value]` each, `now` the value set, `pick` what a row writes. */
  const radio = (list: [string, string][], now: string, pick: (v: string) => void): PaItem[] => list.map(([label, v]) => ({ label, on: v === now, run: () => pick(v) }));
  /** A row that asks for a value (`what`, e.g. `2pt`) and writes `done(v)`. */
  const custom = (ctx: PaCtx, label: string, what: string, value: string, done: (v: string) => void): PaItem => ({ label, run: () => ask(ctx.x, ctx.y, value, what, done) });
  /** The color rows: the document's, xcolor's, the rest; a mix and a custom expression. `now` the color set, `pick` writes one (null: none). */
  function colorRows(ctx: PaCtx, now: string | null, pick: (c: string | null) => void, none = 'Default'): PaItem[] {
    const c = colors(ctx.all, ctx.facts), row = ([n, css]: [string, string]): PaItem => ({ label: n, swatch: css, on: n === now, run: () => pick(n) });
    const names = [...c.doc, ...c.base, ...c.more].map(([n]) => n);
    return [{ label: none, on: now == null, run: () => pick(null) }, ...c.doc.map(row), ...c.base.map(row),
      ...(c.more.length ? [{ label: `More colors (${c.more.length})`, items: c.more.map(row) }] : []),
      { label: 'Mix…', run: () => mixer(ctx, names, now, pick) },
      custom(ctx, 'Custom…', 'an xcolor expression, e.g. blue!30!white', now ?? '', (v) => pick(v.trim() || null))];
  }
  /** The mixer: two colors and a share of the first (`a!p!b`), previewed, written by Apply. */
  function mixer(ctx: PaCtx, names: string[], now: string | null, pick: (c: string) => void) {
    close();
    const all = colors(ctx.all, ctx.facts), css = new Map([...all.doc, ...all.base, ...all.more]), known = new Map([...css].map(([n, h]): [string, number[]] => [n, rgb(h)]));
    const [a0, p0, b0] = (now ?? '').split('!'), opt = (sel: string) => names.map((n) => `<option${n === sel ? ' selected' : ''}>${esc(n)}</option>`).join('');
    const b = Object.assign(document.createElement('div'), { className: 'pa-mix' });
    b.innerHTML = `<label>first <select name="a">${opt(names.includes(a0!) ? a0! : names[0] ?? 'black')}</select></label><label>share <input type="range" name="p" min="0" max="100" step="5" value="${+p0! >= 0 && p0 ? +p0! : 50}"><output></output></label>` +
      `<label>then <select name="b"><option value="">white</option>${opt(names.includes(b0!) ? b0! : '')}</select></label><i class="pa-sw pa-big"></i><code></code><button type="button">Apply</button>`;
    b.style.left = `${Math.max(8, Math.min(ctx.x, innerWidth - 300))}px`;
    b.style.top = `${Math.max(8, Math.min(ctx.y, innerHeight - 200))}px`;
    document.body.append(b);
    const q = (n: string) => b.querySelector(`[name=${n}]`) as HTMLInputElement, expr = () => `${q('a').value}!${q('p').value}${q('b').value ? `!${q('b').value}` : ''}`;
    const show = () => { const c = mix(expr(), known); (b.querySelector('.pa-big') as HTMLElement).style.background = c ? hex(c) : 'transparent'; b.querySelector('code')!.textContent = expr(); b.querySelector('output')!.textContent = `${q('p').value}%`; };
    b.oninput = show;
    b.querySelector('button')!.onclick = () => { const e = expr(); b.remove(); pick(e); };
    b.onkeydown = (k) => { k.stopPropagation(); if (k.key === 'Escape') b.remove(); if (k.key === 'Enter') { const e = expr(); b.remove(); pick(e); } };
    const away = (e: PointerEvent) => { if (!(e.target as HTMLElement).closest?.('.pa-mix')) { b.remove(); removeEventListener('pointerdown', away, true); } };
    addEventListener('pointerdown', away, true);
    show();
    q('a').focus();
  }
  const WIDTHS: [string, string][] = [['ultra thin (0.1pt)', 'ultra thin'], ['very thin (0.2pt)', 'very thin'], ['thin (0.4pt)', 'thin'], ['semithick (0.6pt)', 'semithick'],
    ['thick (0.8pt)', 'thick'], ['very thick (1.2pt)', 'very thick'], ['ultra thick (1.6pt)', 'ultra thick']];
  const DASHES: [string, string][] = [['solid', ''], ['dashed', 'dashed'], ['dotted', 'dotted'], ['densely dashed', 'densely dashed'], ['loosely dashed', 'loosely dashed'], ['densely dotted', 'densely dotted'], ['dash dot', 'dash dot']];
  /** The value of key `name=` among `ks`. */
  const val = (ks: string[], name: string) => ks.map((k) => /^([^=]+?)\s*=\s*(.*)$/.exec(k)).find((m) => m?.[1] === name)?.[2];
  /** A loaded library or its load: the edits to add with the change, null when it cannot be added (the toast says the line). */
  function needs(ctx: PaCtx, lib: string | null, what: string): PaEdit[] | null {
    if (!lib) return [];
    const r = libEdit(ctx.text, ctx.all, ctx.facts?.libraries ?? [], lib);
    if ('have' in r) return [];
    if ('edit' in r) return [r.edit];
    ctx.toast(`${what} needs ${r.line} in the preamble: add that line where tikz is loaded`);
    return [];
  }
  /** The style rows of part `i` of statement `stmt` (a node, a label; -1 the path): each group's rows writing through `put`. */
  function styleRows(ctx: PaCtx, stmt: string, i: number, put: (s: string | null) => void, path: boolean): PaItem[] {
    const T = TM(), c = colors(ctx.all, ctx.facts), names = [...c.doc, ...c.base, ...c.more].map(([n]) => n);
    const cur = (g: string) => T.current(stmt, i, g, path ? names : []), setG = (g: string, add: string[]) => put(T.set(stmt, i, g, add, path ? names : []));
    const colorNow = (g: string) => { const k = cur(g).at(-1); return k == null ? null : k.includes('=') ? k.split('=')[1]!.trim() : k; };
    const ks = T.keysAt(stmt, i) ?? [];
    const width = cur('width').at(-1) ?? '', corners = cur('corners').at(-1) ?? '', dash = cur('dash').at(-1) ?? '', op = val(ks, 'opacity') ?? '';
    const len = (g: string, label: string, list: string[]): PaItem => ({ label, items: [...radio([['default', ''], ...list.map((v): [string, string] => [v, v])], val(ks, g) ?? '', (v) => setG(g, v ? [`${g}=${v}`] : [])),
      custom(ctx, 'Custom…', `${g}, e.g. 3mm`, val(ks, g) ?? '', (v) => setG(g, v.trim() ? [`${g}=${v.trim()}`] : []))] });
    return [
      { label: path ? 'Color' : 'Draw color', items: colorRows(ctx, colorNow('draw'), (v) => setG('draw', v == null ? [] : [path ? v : `draw=${v}`])) },
      ...(path ? [] : [{ label: 'Fill color', items: colorRows(ctx, colorNow('fill'), (v) => setG('fill', v == null ? [] : [`fill=${v}`]), 'None') }]),
      { label: 'Line width', items: [...radio([['default', ''], ...WIDTHS], width, (v) => setG('width', v ? [v] : [])),
        custom(ctx, 'Custom…', 'a line width, e.g. 1.5pt', val(ks, 'line width') ?? '', (v) => setG('width', v.trim() ? [`line width=${v.trim()}`] : []))] },
      ...(path ? [] : [{ label: 'Rounded corners', items: [...radio([['none', ''], ['2pt', 'rounded corners=2pt'], ['4pt', 'rounded corners=4pt']], corners === 'rounded corners' ? 'rounded corners=4pt' : corners, (v) => setG('corners', v ? [v] : [])),
        custom(ctx, 'Custom…', 'a corner radius, e.g. 3pt', val(ks, 'rounded corners') ?? '', (v) => setG('corners', v.trim() ? [`rounded corners=${v.trim()}`] : []))] }]),
      { label: 'Dash', items: radio(DASHES, dash, (v) => setG('dash', v ? [v] : [])) },
      { label: 'Opacity', items: [...radio([['100%', ''], ['75%', '0.75'], ['50%', '0.5'], ['25%', '0.25']], op, (v) => setG('opacity', v ? [`opacity=${v}`] : [])),
        custom(ctx, 'Custom…', 'an opacity from 0 to 1', op, (v) => setG('opacity', v.trim() ? [`opacity=${v.trim()}`] : []))] },
      ...(path ? [] : [len('inner sep', 'Inner sep', ['0pt', '2pt', '4pt', '8pt']), len('minimum width', 'Minimum width', ['1cm', '2cm', '3cm']), len('minimum height', 'Minimum height', ['0.5cm', '1cm', '2cm'])]),
    ];
  }
  /** The text rows of part `i`: its text, color and font size. */
  function textRows(ctx: PaCtx, stmt: string, n: TikzNode, put: (s: string | null) => void): PaItem[] {
    const T = TM(), ks = T.keysAt(stmt, n.i) ?? [], font = val(ks, 'font'), size = SIZES.find((s) => (font ?? '').includes(s)) ?? '';
    return [
      { label: 'Text color', items: colorRows(ctx, val(ks, 'text')?.trim() ?? null, (v) => put(T.set(stmt, n.i, 'text', v == null ? [] : [`text=${v}`]))) },
      { label: 'Font size', items: radio([['default', ''], ...SIZES.map((s): [string, string] => [s.slice(1), s])], size, (v) => { const f = fontSet(font, v || null); put(T.set(stmt, n.i, 'font', f ? [`font=${f}`] : [])); }) },
    ];
  }
  /** The menu's first row: the text, edited in place. */
  const editRow = (ctx: PaCtx, stmt: string, n: TikzNode, put: (s: string | null) => void): PaGroup =>
    ({ title: 'Edit', inline: true, items: [{ label: 'Edit text…', hint: 'double click', run: () => ask(ctx.x, ctx.y, n.text, `the text of ${n.name ? `node ${n.name}` : n.kind === 'label' ? 'the label' : 'the node'}`, (t) => put(TM().retext(stmt, n.i, t))) }] });
  const ROT: [string, string][] = [['0°', '0'], ['15°', '15'], ['45°', '45'], ['90°', '90'], ['180°', '180'], ['-45°', '-45'], ['-90°', '-90']];
  /** The node's whole picture rotated (`rotate=` on it, or `rotate around` this node) and its nodes with it (`transform shape`). */
  function pictureRows(ctx: PaCtx, n: TikzNode): PaItem[] {
    const T = TM(), o = T.picOpts(ctx.text, n.pic.at!), ks = o?.keys ?? [], ROTS = /^(rotate|rotate around)\s*=/;
    const now = ks.find((k) => ROTS.test(k)) ?? '', deg = /^rotate\s*=\s*(-?[\d.]+)$/.exec(now)?.[1] ?? (now ? '' : '0'), shape = ks.includes('transform shape');
    const write = (add: string[], group = ROTS) => { const e = T.picSet(ctx.text, n.pic.at!, group, add); if (e) ctx.write([{ from: e.from, to: e.to, now: e.now }]); };
    return [
      ...radio([['0°', '0'], ['90°', '90'], ['180°', '180'], ['-90°', '-90']], deg, (v) => write(+v ? [`rotate=${v}`] : [])),
      custom(ctx, 'Custom…', 'degrees, counterclockwise', deg, (v) => { const d = +v; if (Number.isFinite(d)) write(d ? [`rotate=${d}`] : []); }),
      ...(n.name ? [custom(ctx, `Around node ${n.name}…`, 'degrees, counterclockwise', '90', (v) => { const d = +v; if (Number.isFinite(d)) write(d ? [`rotate around={${d}:(${n.name})}`] : []); })] : []),
      { label: 'Nodes turn with it', on: shape, run: () => write(shape ? [] : ['transform shape'], /^transform shape$/) },
    ];
  }
  /** The name of node `n`, or a fresh one written onto it (`edits` gets that rewrite): a path end needs one. */
  function nameOf(ctx: PaCtx, n: TikzNode, edits: PaEdit[]) {
    if (n.name) return n.name;
    const T = TM(), name = T.fresh(ctx.text, n.pic.at!, n.text), stmt = ctx.text.slice(n.from, n.to), now = T.named(stmt, n.i, name);
    if (now) edits.push({ from: n.from, to: n.to, now });
    return name;
  }
  /** A use of node name `name` in a statement: a point `(name)`, an anchor `(name.east)`, a placement `of name`. */
  const refsTo = (name: string) => { const q = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); return new RegExp(`\\(\\s*${q}\\s*(\\.[^)]*)?\\)|\\bof\\s*=?\\s*${q}(?![\\w])`); };
  /** Where the picture's statements start: the line after `\begin{tikzpicture}[…]`. */
  const bodyStart = (text: string, n: TikzNode) => { const o = TM().picOpts(text, n.pic.at!); const at = o ? o.to : n.pic.at!; const nl = text.indexOf('\n', at); return nl < 0 ? at : nl + 1; };
  /** The order rows: a node's statement brought to the front (drawn last) or sent to the back, duplicated, deleted with its paths. */
  function arrangeRows(ctx: PaCtx, n: TikzNode, own: boolean): PaItem[] {
    const T = TM(), text = ctx.text, stmt = text.slice(n.from, n.to), alone = (() => { const e = cut(text, n.from, n.to); return e.from !== n.from || e.to !== n.to; })();
    const end = text.indexOf('\\end{tikzpicture}', n.from), endLine = text.lastIndexOf('\n', end - 1) + 1, line = (s: string) => `${/^\s*/.exec(text.slice(text.lastIndexOf('\n', n.from - 1) + 1, n.from))![0]}${s}\n`;
    const rows: PaItem[] = [{ label: 'Add edge from this node…', run: () => ctx.pick(`Click the node the edge from ${n.name ?? 'this node'} goes to (Escape: none)`, (m) => {
      if (m.pic.at !== n.pic.at || (m.from === n.from && m.i === n.i)) return ctx.toast('An edge goes to another node of the same picture');
      const edits: PaEdit[] = [], a = nameOf(ctx, n, edits), b = nameOf(ctx, m, edits), e = T.addEdge(text, n.pic.at!, a, b);
      if (e) edits.push({ from: e.at, to: e.at, now: e.text });
      ctx.write(edits);
    }) }];
    // TikZ knows a name only after its node: the order changes only where no statement then refers to a name before it stands
    const stmts = [...T.nodes(text).filter((m) => m.pic.at === n.pic.at && m.from !== n.from), ...T.edges(text).filter((e) => e.pic.at === n.pic.at)]
      .filter((m, k, all) => all.findIndex((x) => x.from === m.from) === k);
    const names = T.nodes(text).filter((m) => m.pic.at === n.pic.at && m.name && m.name !== n.name).map((m) => m.name!);
    const later = stmts.filter((m) => m.from > n.from), refers = (s: string, name: string) => refsTo(name).test(s);
    const front = !n.name || !later.some((m) => refers(text.slice(m.from, m.to), n.name!)), back = !names.some((x) => refers(stmt, x));
    const placedBy = n.name ? stmts.filter((m) => !('segs' in m) && refers(text.slice(m.from, m.to), n.name!)) : [];
    if (own && alone) {
      if (front && later.length) rows.push({ label: 'Bring to front', hint: 'drawn last', run: () => ctx.write([cut(text, n.from, n.to), { from: endLine, to: endLine, now: line(stmt) }]) });
      if (back && stmts.some((m) => m.from < n.from)) rows.push({ label: 'Send to back', hint: 'drawn first', run: () => { const at = bodyStart(text, n); ctx.write([{ from: at, to: at, now: line(stmt) }, cut(text, n.from, n.to)]); } });
      rows.push({ label: 'Duplicate', run: () => {
        const name = n.name ? T.fresh(text, n.pic.at!, n.name) : null, copy = name ? stmt.replace(new RegExp(`\\(\\s*${n.name!.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\)`), `(${name})`) : stmt;
        // a new node stands relative to the one it copies (`below=of a`), else shifted from it
        const lib = n.name ? libEdit(text, ctx.all, ctx.facts?.libraries ?? [], 'positioning') : { line: '' }, e = text.indexOf('\n', n.to);
        const moved = (!('line' in lib) && T.place(copy, n.i, `below=of ${n.name}`)) || (T.move(copy, n.i, n.pic, { by: [0.5, -0.5] })?.text ?? copy);
        ctx.write([...('edit' in lib ? [lib.edit] : []), { from: e < 0 ? text.length : e + 1, to: e < 0 ? text.length : e + 1, now: line(moved) }]);
      } });
      if (!placedBy.length) rows.push({ label: n.name ? 'Delete with its paths' : 'Delete', run: () => {
        const gone = n.name ? T.edges(text).filter((p) => p.pic.at === n.pic.at && p.segs.some((g) => [g.a, g.b].some((x) => x.split('.')[0]!.trim() === n.name))) : [];
        ctx.write([cut(text, n.from, n.to), ...gone.map((p) => cut(text, p.from, p.to))]);
      } });
    }
    return rows;
  }
  self.satexPageActions.register({
    name: 'tikz-node', title: 'TikZ nodes', about: 'A node\'s shape, style, text, rotation and place in its picture, and new edges from it', type: 'tikz', targets: ['tikz-node'],
    groups(t, ctx) {
      const T = TM(), n: TikzNode = t.n, stmt = ctx.text.slice(n.from, n.to), now = T.shapeOf(n), looks = T.looks(n);
      const put = (s: string | null, pre: PaEdit[] = []) => { if (s != null && s !== stmt) ctx.write([...pre, { from: n.from, to: n.to, now: s }]); };
      const shapeTo = (to: string, label: string, extra: string[] = []) => { const r = T.shape(stmt, n.i, n.pic, to, extra); if (r) { const pre = needs(ctx, ctx.facts?.shapes?.find((s: Rec) => s.name === to)?.load?.match(/^\\usetikzlibrary\{(.+)\}$/)?.[1] ?? r.library, label); if (pre) put(r.text, pre); } };
      const ks = T.keysAt(stmt, n.i) ?? [], sides = val(ks, 'regular polygon sides'), points = val(ks, 'star points');
      const shapes: PaItem[] = T.SHAPES.map(([k, label]) => k === 'regular polygon'
        ? { label, on: now === k, items: [...['3', '4', '5', '6', '8'].map((v) => ({ label: `${v} sides`, on: now === k && (sides ?? '5') === v, run: () => shapeTo(k, label, v === '5' ? [] : [`regular polygon sides=${v}`]) })),
          custom(ctx, 'Sides…', 'the number of sides', sides ?? '5', (v) => { if (+v >= 3) shapeTo(k, label, +v === 5 ? [] : [`regular polygon sides=${+v}`]); })] }
        : k === 'star' ? { label, on: now === k, items: ['4', '5', '6', '8'].map((v) => ({ label: `${v} points`, on: now === k && (points ?? '5') === v, run: () => shapeTo(k, label, v === '5' ? [] : [`star points=${v}`]) })) }
        : { label, on: now === k, run: () => shapeTo(k, label) });
      const deg = String(T.angle(stmt, n.i));
      const place: PaItem[] = [...(t.rel ? [{ label: `Relative to ${t.rel.label.replace(/^.* of /, '')}`, hint: t.rel.label, run: () => put(t.rel.text, t.rel.pre) }] : []),
        ...(t.fixed ? [{ label: 'Fixed position', hint: 'at (x,y)', run: () => put(t.fixed) }] : [])];
      return [
        editRow(ctx, stmt, n, put),
        { title: 'Shape', items: shapes },
        { title: 'Style', items: [{ label: 'Frame', on: looks.draw, run: () => put(T.flip(stmt, n.i, n.pic, 'draw')) }, { label: 'Fill', on: looks.fill, run: () => put(T.flip(stmt, n.i, n.pic, 'fill')) }, ...styleRows(ctx, stmt, n.i, put, false)] },
        { title: 'Text', items: textRows(ctx, stmt, n, put) },
        { title: 'Position', items: [...place, { label: 'Rotate', items: [...radio(ROT, deg, (v) => put(T.set(stmt, n.i, 'rotate', +v ? [`rotate=${v}`] : []))),
          custom(ctx, 'Custom…', 'degrees, counterclockwise', deg, (v) => { if (Number.isFinite(+v)) put(T.set(stmt, n.i, 'rotate', +v ? [`rotate=${+v}`] : [])); }),
          { label: 'Rotate the picture', items: pictureRows(ctx, n) }] }] },
        { title: 'Arrange', items: arrangeRows(ctx, n, /^\s*\\node\b/.test(stmt)) },
      ];
    },
  });
  self.satexPageActions.register({
    name: 'tikz-label', title: 'TikZ edge labels', about: 'A label on a path: its text, place along the path, side, slope and style', type: 'tikz', targets: ['tikz-label'],
    groups(t, ctx) {
      const T = TM(), n: TikzNode = t.n, stmt = ctx.text.slice(n.from, n.to), ks = T.keysAt(stmt, n.i) ?? [];
      const put = (s: string | null) => { if (s != null && s !== stmt) ctx.write([{ from: n.from, to: n.to, now: s }]); };
      const pos = T.current(stmt, n.i, 'pos').at(-1) ?? '', side = T.current(stmt, n.i, 'side').at(-1) ?? '', auto = T.current(stmt, n.i, 'auto').at(-1) ?? '';
      return [
        editRow(ctx, stmt, n, put),
        { title: 'Text', items: textRows(ctx, stmt, n, put) },
        { title: 'Position', items: [...radio([['at start', 'at start'], ['near start', 'near start'], ['midway', 'midway'], ['near end', 'near end'], ['at end', 'at end']], pos, (v) => put(T.set(stmt, n.i, 'pos', [v]))),
          custom(ctx, 'pos=…', 'a place along the path, 0 to 1', /^pos\s*=\s*(.*)$/.exec(pos)?.[1] ?? '0.5', (v) => { if (+v >= 0 && +v <= 1) put(T.set(stmt, n.i, 'pos', [`pos=${+v}`])); })] },
        { title: 'Side', items: [...radio([['above', 'above'], ['below', 'below'], ['left', 'left'], ['right', 'right'], ['on the path', '']], auto ? '-' : side, (v) => put(T.set(stmt, n.i, 'side', v ? [v] : []))),
          { label: 'auto', on: auto === 'auto', run: () => put(T.set(stmt, n.i, 'auto', auto === 'auto' ? [] : ['auto'])) },
          { label: 'auto, other side', on: auto === "auto=right" || auto === 'swap', run: () => put(T.set(stmt, n.i, 'auto', auto === 'auto=right' ? [] : ['auto=right'])) },
          { label: 'sloped', on: ks.includes('sloped'), run: () => put(T.set(stmt, n.i, 'sloped', ks.includes('sloped') ? [] : ['sloped'])) }] },
        { title: 'Style', items: [{ label: 'Fill color', items: colorRows(ctx, val(ks, 'fill')?.trim() ?? null, (v) => put(T.set(stmt, n.i, 'fill', v == null ? [] : [`fill=${v}`])), 'None') }, ...styleRows(ctx, stmt, n.i, put, false).filter((r) => /^(Draw color|Line width|Inner sep)$/.test(r.label))] },
        { title: 'Arrange', inline: true, items: [{ label: 'Delete label', run: () => put(T.unlabel(stmt, n.i)) }] },
      ];
    },
  });
  const TIPS: [string, string][] = [['→', '->'], ['←', '<-'], ['↔', '<->'], ['none', ''], ['Stealth', '-Stealth'], ['Latex', '-Latex'], ['To', '-To'], ['Stealth both', 'Stealth-Stealth']];
  self.satexPageActions.register({
    name: 'tikz-edge', title: 'TikZ paths', about: 'A path\'s arrow tips, color, width, dash, routing (straight, right angles, bends, angles) and a new label', type: 'tikz', targets: ['tikz-edge'],
    groups(t, ctx) {
      const T = TM(), e: TikzEdge = t.e, k: number = t.k, stmt = ctx.text.slice(e.from, e.to), tip = T.tipOf(e), g = T.segOf(stmt, k);
      const put = (s: string | null, pre: PaEdit[] = []) => { if (s != null && s !== stmt) ctx.write([...pre, { from: e.from, to: e.to, now: s }]); };
      const tipTo = (v: string) => { const meta = /[A-Z]/.test(v) && !(ctx.facts?.tips ?? []).some((x: Rec) => v.includes(x.name)); const pre = needs(ctx, meta ? 'arrows.meta' : null, `The ${v} tip`); if (pre) put(T.tip(stmt, v), pre); };
      const bent = g?.keys.map((x) => /^bend (left|right)(?:\s*=\s*(.*))?$/.exec(x)).find(Boolean), angles = !!g?.keys.some((x) => /^(in|out)\s*=/.test(x));
      const bendRows = (side: string): PaItem[] => [...['15', '30', '45', '60'].map((a) => ({ label: `${a}°`, on: bent?.[1] === side && (bent[2] ?? '30') === a, run: () => put(T.bend(stmt, k, side, +a)) })),
        custom(ctx, 'Custom…', 'the bend angle in degrees', bent?.[2] ?? '30', (v) => { if (Number.isFinite(+v)) put(T.bend(stmt, k, side, +v)); })];
      const loose = /^looseness\s*=\s*(.*)$/.exec(g?.keys.find((x) => /^looseness/.test(x)) ?? '')?.[1] ?? '1';
      return [
        { title: 'Arrow tips', items: TIPS.map(([label, v]) => ({ label, on: v === tip || (!v && !tip), run: () => tipTo(v) })) },
        { title: 'Style', items: styleRows(ctx, stmt, -1, put, true) },
        ...(g ? [{ title: 'Route', items: [
          { label: 'Straight', on: g.kind === '--' || (g.kind === 'to' && !bent && !angles), run: () => put(g.kind === 'to' ? T.bend(stmt, k, null) : T.route(stmt, k, '--')) },
          { label: 'Right angle, vertical first (|-)', on: g.kind === '|-', run: () => put(T.route(stmt, k, '|-')) },
          { label: 'Right angle, horizontal first (-|)', on: g.kind === '-|', run: () => put(T.route(stmt, k, '-|')) },
          { label: 'Bend left', on: bent?.[1] === 'left', items: bendRows('left') }, { label: 'Bend right', on: bent?.[1] === 'right', items: bendRows('right') },
          custom(ctx, 'Out and in angles…', 'out, in in degrees, e.g. 0, 180', angles ? `${g.keys.find((x) => /^out/.test(x))?.split('=')[1]?.trim() ?? 0}, ${g.keys.find((x) => /^in\b/.test(x))?.split('=')[1]?.trim() ?? 180}` : '0, 180',
            (v) => { const [o, i] = v.split(',').map((x) => +x.trim()); if (Number.isFinite(o) && Number.isFinite(i)) put(T.inout(stmt, k, o!, i!)); }),
          ...(g.kind === 'to' && (bent || angles) ? [{ label: 'Looseness', items: [...radio([['0.5', '0.5'], ['1', '1'], ['1.5', '1.5'], ['2', '2']], loose, (v) => put(T.looseness(stmt, k, +v))),
            custom(ctx, 'Custom…', 'the looseness, 1 is the default', loose, (v) => { if (+v > 0) put(T.looseness(stmt, k, +v)); })] }] : []),
        ] }] : []),
        { title: 'Label', items: [
          ...[['above', ['midway', 'above']], ['below', ['midway', 'below']], ['sloped, above', ['midway', 'above', 'sloped']], ['auto', ['midway', 'auto']]].map(([l, o]) => ({ label: `Add label, ${l}…`,
            run: () => ask(ctx.x, ctx.y, '', 'the new label', (s) => { const r = T.label(stmt, k, s, o as string[]); if (r) put(r.text); }) })),
        ] },
        { title: 'Arrange', inline: true, items: [{ label: 'Delete path', run: () => ctx.write([cut(ctx.text, e.from, e.to)]) }] },
      ];
    },
  });
  self.satexPageActions.register({
    name: 'forest-node', title: 'Forest nodes', about: 'A tree node\'s shape, frame and text, and the tree around it', type: 'forest', targets: ['forest-node'],
    groups(t, ctx) {
      const FOREST = self.satexViews.forest, { t: tree, n } = t.f, key = (o: string) => o.split('=')[0]!.trim(), has = (k: string) => n.opts.some((o: string) => key(o) === k);
      const SH: [string, string][] = [['', 'Rectangle'], ['rounded corners', 'Rounded rectangle'], ['circle', 'Circle']];
      const write = (change: () => unknown) => { if (change() === false) return; n.dirty = true; FOREST.settle(tree); ctx.write([{ from: t.f.from, to: t.f.to, now: FOREST.emit(tree) }]); };
      const shape = SH.find(([k]) => k && has(k))?.[0] ?? '', parent = FOREST.parentOf(tree, n);
      return [
        { title: 'Shape', inline: true, items: SH.map(([k, l]) => ({ label: l, on: shape === k, run: () => write(() => { n.opts = n.opts.filter((o: string) => !SH.some(([s]) => s && key(o) === s)); if (k) n.opts.push(k); }) })) },
        { title: 'Style', inline: true, items: [{ label: 'Frame', on: has('draw'), run: () => write(() => { n.opts = has('draw') ? n.opts.filter((o: string) => key(o) !== 'draw') : [...n.opts, 'draw']; }) }] },
        { title: 'Tree', inline: true, items: [{ label: 'Add child', run: () => write(() => { FOREST.addChild(n); }) },
          ...(parent ? [{ label: 'Add sibling', run: () => write(() => { FOREST.addSibling(tree, n); }) }, { label: n.children.length ? 'Delete with its subtree' : 'Delete', run: () => write(() => { FOREST.remove(tree, n); }) }] : [])] },
        { title: 'Text', inline: true, items: [{ label: 'Edit text…', run: () => ask(ctx.x, ctx.y, n.content, 'the text of the node', (v) => write(() => { n.content = v; })) }] },
      ];
    },
  });
  const ALIGNS: Record<string, string> = { l: 'left', c: 'centered', r: 'right', p: 'top (paragraph)', m: 'middle (paragraph)', b: 'bottom (paragraph)' };
  self.satexPageActions.register({
    name: 'table-column', title: 'Table columns', about: 'A column\'s alignment, its letter rewritten in the column spec', type: 'table', targets: ['table-column'],
    groups(t, ctx) {
      const col = t.col, kin = col?.spec && ['lcr', 'pmb'].find((s) => s.includes(col.align));
      return kin ? [{ title: 'Align', inline: true, items: [...kin].map((a) => ({ label: `${a}: ${ALIGNS[a]}`, on: a === col.align, run: () => ctx.colAlign(t.t, t.k, a) })) }] : [];
    },
  });
})();
