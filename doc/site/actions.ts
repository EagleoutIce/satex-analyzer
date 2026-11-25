// Page actions: what a right click on the PDF or the Typeset view offers on a thing drawn there (a TikZ node, path or
// label, a forest node, a table column). Each plugin names the targets it serves and gives grouped menu items; an item
// writes through the context the playground hands it (`ctx.write`: one undo step through the snapshot printer). The
// playground only finds the target under the pointer and dispatches here. A plugin is switched per viewer (Playground
// settings) and with its type plugin (`type_plugins.<name>.enabled`).
export {};
declare global {
  /** A menu row: a choice (`run`), a submenu (`items`), radio state `on`, a color `swatch`, a picture `img` (SVG), a rule and caption `head` above it. */
  type PaItem = { label: string; run?: () => void; items?: PaItem[]; on?: boolean; swatch?: string; hint?: string; img?: string | null; sample?: PaSample; head?: string; title?: string; disabled?: boolean; bar?: PaBar };
  /** Text set in a face the page loaded (`font`, a CSS family list): a font or math alphabet's sample. */
  type PaSample = { text: string; font: string };
  /** A toolbar row (the selection menu's first): toggles on or off, dropdowns at their current value. */
  type PaBar = { color?: { css: string | null; title: string; run: () => void }; toggles: { label: string; title: string; on: boolean; run: () => void }[]; selects: { title: string; value: string; options: [string, string, PaSample?][]; pick: (v: string) => void }[] };
  /** A group of rows: a submenu row by its title, or its rows inline. */
  type PaGroup = { title: string; items: PaItem[]; inline?: boolean };
  type PaEdit = { from: number; to: number; now: string };
  /** What the playground hands a plugin: the text the menu opened on, writes, a prompt, the run's facts. */
  interface PaCtx {
    text: string; all: string; x: number; y: number; facts: Rec | null;
    write(edits: (PaEdit | null)[]): Promise<boolean> | void; toast(msg: string): void; caret?(at: number): void;
    pick(prompt: string, done: (n: TikzNode) => void): void;
    /** A line from forest node target `f` to the node clicked next, drawn after its tree. */
    forestLink?(f: Rec): void; colAlign(t: number, k: number, a: string, w?: string, h?: string): void; tableOp?(t: number, op: { what: string; act: string; at: number; to?: number }): void;
    scratch?(b: ScBlock): void;
    /** Writes Insert's `kind` (refactor.ts `inserts`) at target `t`'s place and opens its widget. */
    insert?(kind: string, t: PaTarget): void;
    /** Math at the Type caret (`inline`, `display`, `align`), and how inline math is written there (`\\(…\\)`, `$…$`). */
    math?(kind: string): void;
    mathInline?: string;
    /** The LaTeX kernel's release (`2024-11-01`), when known: newer commands (`\\mathcolor`, 2022-06) only where it has them. */
    kernel?: string;
    /** The open file's name, for where a style is defined. */
    file?: string;
    /** Whether the grid of the picture at offset `at` shows; with `on` it is switched (this session). */
    grid?(at: number, on?: boolean): boolean;
    /** The fonts the engine can set (the font widget's offers, src/lsp/plugins/font.rs): name, what it writes
     * (`\\usepackage{…}` on pdfLaTeX, `\\setmainfont{…}` with fontspec), TeX's sample, and `main()` making it the main
     * font; with `math` the math fonts (`\\usepackage{newtxmath}`, `\\setmathfont{…}` with unicode-math) instead. */
    fonts?(math?: boolean): Promise<{ title: string; detail: string; svg?: string | null; main(): void }[]>;
    /** TeX's pictures by `kind:name` (`arrow:-Stealth`), previews.json. */
    previews?: Record<string, string> | null;
  }
  /** Lines `from`–`to` (1-based) of `file`: what a scratch document is sliced from; `what` names it in the menu. */
  type ScBlock = { file: string; from: number; to: number; what: string };
  /** A scratch document's marked region: the file and lines it came from, its text now. */
  type ScRegion = { file: string; from: number; to: number; body: string };
  /** Where a merge writes: `clean` at the lines it came from, `moved` where the original's lines now stand,
   * `conflict` the original changed there (`theirs`), `same` nothing changed. */
  type ScMerge = { status: 'clean' | 'moved' | 'conflict' | 'same'; from: number; to: number; theirs: string };
  type PaTarget = { kind: string; [k: string]: any };
  /** `category`: the Settings → Plugins group it is listed in (`page actions` when none). */
  interface PaPlugin { name: string; title: string; about: string; type: string; category?: string; targets: string[]; groups(t: PaTarget, ctx: PaCtx): PaGroup[] }
  interface Window {
    /** Loads the samples' face, Latin Modern Math (playground.js pdfLmLoad). */
    satexLmLoad?: () => void;
    satexPageActions: {
      register(p: PaPlugin): void; plugins: PaPlugin[]; on(name: string, cfg?: Record<string, unknown>): boolean; setOn(name: string, on: boolean): void;
      groups(t: PaTarget, ctx: PaCtx, cfg?: Record<string, unknown>): PaGroup[]; open(x: number, y: number, title: string, groups: PaGroup[]): HTMLElement | null; close(): void;
      ask(x: number, y: number, value: string, what: string, done: (v: string) => void, always?: boolean, suggest?: Promise<PaItem[]>): void;
      bar(b: PaBar, done: () => void): HTMLElement; face(r: PaItem): string;
      libEdit(text: string, all: string, loaded: string[], lib: string): { have: true } | { edit: PaEdit } | { line: string };
      colors(all: string, facts: Rec | null): { doc: [string, string][]; base: [string, string][]; more: [string, string][] };
      /** One color menu for every place that picks a color: none, the document's, xcolor's, More, Mix…, Custom…. */
      colorRows(ctx: PaCtx, now: string | null, pick: (c: string | null) => void, none?: string): PaItem[];
      mix(expr: string, known: Map<string, number[]>): number[] | null; fontSet(font: string | undefined, size: string | null): string | null;
      cut(text: string, from: number, to: number): PaEdit;
      styles(text: string): { name: string; body: string; line: number; note: string }[];
    };
    satexScratch: {
      build(slice: string, file: string, text: string, from: number, to: number, isRoot: boolean): string | null;
      local(texts: Record<string, string>, root: string, file: string, from: number, to: number, defs: { file: string; line: number; name: string }[]): string | null;
      extract(text: string): ScRegion | null; merge(base: string, mine: string, current: string, from: number, to: number, size: number): ScMerge;
      apply(current: string, from: number, to: number, body: string): string; block(text: string, line: number): [number, number, string];
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
  /** The groups of the plugins on for target `t`; groups of one title (Refactor from several plugins) are one. */
  /** The editor's and the pages' menus share one order: a widget's own groups first, then these (Insert ▸, the
   * selection's Format ▸ … Size ▸, Refactor ▸, Scratch ▸), each where it applies. */
  const ORDER = ['Insert', 'Format', 'Color', 'Font', 'Size', 'Refactor', 'Scratch'];
  const rank = (g: PaGroup) => g.inline ? 0 : ORDER.indexOf(g.title) + 1;
  const groups = (t: PaTarget, ctx: PaCtx, cfg: Record<string, unknown> = {}) => plugins.filter((p) => p.targets.includes(t.kind) && on(p.name, cfg)).flatMap((p) => p.groups(t, ctx))
    .reduce<PaGroup[]>((out, g) => { const o = out.find((x) => x.title === g.title && !x.inline && !g.inline); if (o) o.items = [...o.items, ...g.items]; else out.push(g); return out; }, [])
    // a widget's group named like a shared one (a node's Color ▸) keeps its place: only source and block targets sort
    .map((g, i) => ({ g, i })).sort((a, b) => (/^(source|block|place|caret)$/.test(t.kind) ? rank(a.g) - rank(b.g) : 0) || a.i - b.i).map(({ g }) => g);

  // ---------------------------------------------------------------- the menu
  const POPS = '.pa-menu, .tikz-input, .pa-mix';
  /** What had the focus before the first menu, prompt or mixer opened: it gets it back when they close (the editor after
   * Extract to \newcommand, the Type caret after a node's text); none when the page had it. */
  let back: HTMLElement | null = null;
  const hold = () => { const a = document.activeElement as HTMLElement | null; if (!a?.closest?.(POPS)) back = a && a !== document.body ? a : null; };
  const close = () => {
    for (const el of document.querySelectorAll(POPS)) el.remove();
    if (back?.isConnected && (!document.activeElement || document.activeElement === document.body)) back.focus({ preventScroll: true });
  };
  /** One menu box of `rows` at screen point (x, y), nested under `parent` (a row) when given. */
  function box(rows: (PaItem | 'hr' | string)[], x: number, y: number, parent: HTMLElement | null) {
    const m = Object.assign(document.createElement('div'), { className: `pa-menu${parent ? ' pa-sub' : ''}` });
    m.setAttribute('role', 'menu');
    const acts: PaItem[] = [];
    m.innerHTML = rows.map((r, k) => {
      if (r === 'hr') return '<hr>';
      if (typeof r === 'string') return `<b>${esc(r)}</b>`;
      if (r.bar) return `<div class="pa-bar-at" data-k="${k}"></div>`;
      acts.push(r);
      const head = r.head ? `${k ? '<hr>' : ''}<b>${esc(r.head)}</b>` : '';
      const role = r.items ? 'menuitem' : r.on == null ? 'menuitem' : 'menuitemradio';
      return `${head}<button type="button" role="${role}"${r.disabled ? ' aria-disabled="true" class="pa-wait"' : ''}${r.title ? ` title="${esc(r.title)}"` : ''}${r.on == null ? '' : ` aria-checked="${r.on}"`}${r.items ? ' aria-haspopup="menu" aria-expanded="false"' : ''} data-i="${acts.length - 1}">` +
        `${face(r)}</button>`;
    }).join('');
    for (const at of [...m.querySelectorAll('.pa-bar-at')] as HTMLElement[]) at.replaceWith(toolbar((rows[+at.dataset.k!] as PaItem).bar!, close));
    document.body.append(m);
    const w = m.offsetWidth, h = m.offsetHeight;
    m.style.left = `${Math.max(4, x + w > innerWidth - 4 ? (parent ? parent.getBoundingClientRect().left - w - 2 : innerWidth - w - 8) : x)}px`;
    m.style.top = `${Math.max(4, Math.min(y, innerHeight - h - 8))}px`;
    (m as any).acts = acts;
    (m as any).up = parent;
    return m;
  }
  /** Toolbar `b` as one row of a menu, the editor's (playground.js showMenu) and the pages' alike: a toggle or a pick
   * closes the menu (`done`), then writes. */
  function toolbar(b: PaBar, done: () => void): HTMLElement {
    const el = Object.assign(document.createElement('div'), { className: 'pa-bar' });
    el.setAttribute('role', 'toolbar');
    el.innerHTML = b.toggles.map((t, k) => `<button type="button" class="pa-tog pa-${esc(t.label.toLowerCase())}" data-k="${k}" aria-pressed="${t.on}" title="${esc(t.title)}">${esc(t.label)}</button>`).join('') +
      (b.color ? `<button type="button" class="pa-tog pa-col" title="${esc(b.color.title)}" aria-label="${esc(b.color.title)}"><i class="pa-sw"${b.color.css ? ` style="background:${esc(b.color.css)}"` : ''}></i></button>` : '') +
      // a dropdown is a button opening a menu of its choices, each with its sample (a native select shows text only)
      b.selects.map((s, k) => `<button type="button" class="pa-dd" data-s="${k}" data-value="${esc(s.value)}" data-labels="${esc(s.options.map(([l]) => l).join('\n'))}" title="${esc(s.title)}" aria-label="${esc(s.title)}" aria-haspopup="menu">${esc(s.options.find(([, v]) => v === s.value)?.[0] ?? s.title)}</button>`).join('');
    el.addEventListener('click', (e) => {
      const x = (e.target as HTMLElement).closest?.('.pa-tog, .pa-dd') as HTMLElement | null;
      if (!x) return;
      e.stopPropagation();
      const r = x.getBoundingClientRect();
      done();
      if (x.classList.contains('pa-dd')) { const s = b.selects[+x.dataset.s!]!; open(r.left, r.bottom + 2, s.title, [{ title: s.title, inline: true, items: s.options.map(([label, v, sample]) => ({ label, sample, on: v === s.value, run: () => s.pick(v) })) }]); }
      else if (x.classList.contains('pa-col')) b.color!.run(); else b.toggles[+x.dataset.k!]!.run();
    });
    return el;
  }
  /** A row's face, in the pages' menu and the editor's alike (playground.js showMenu): swatch, picture, label, hint or ▸. */
  const face = (r: PaItem) => `${r.swatch ? `<i class="pa-sw" style="background:${esc(r.swatch)}"></i>` : ''}${r.img ? `<img class="pa-img" alt="" src="data:image/svg+xml;charset=utf-8,${esc(encodeURIComponent(r.img))}">` : ''}<span>${esc(r.label)}</span>${r.sample ? (self.satexLmLoad?.(), `<span class="pa-sample" style="font-family:${esc(r.sample.font)}">${esc(r.sample.text)}</span>`) : ''}${r.items ? '<span class="pa-more">▸</span>' : r.hint ? `<small>${esc(r.hint)}</small>` : ''}`;
  /** Opens the submenu of row `b`, its first row focused when `focus`. */
  function sub(b: HTMLElement, focus: boolean) {
    const m = b.closest('.pa-menu') as any, it: PaItem = m.acts[+b.dataset.i!];
    if (!it?.items) return;
    // the row's own open submenu stays: a pointer moving over its row does not build it anew
    for (const o of [...document.querySelectorAll('.pa-sub')] as any[]) if (!isAbove(o, m) && !((b as any).sub && isAbove((b as any).sub, o))) shut(o);
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
    hold();
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
      if (it.disabled) return;
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
  /** A one-line input at (x, y): Enter writes `done(value)` (a value unchanged only when `always`), Escape leaves it.
   * `suggest`: rows listed under it as they come, those matching the typed text; a click or Down and Enter takes one. */
  function ask(x: number, y: number, value: string, what: string, done: (v: string) => void, always = false, suggest?: Promise<PaItem[]>) {
    hold();
    close();
    const b = Object.assign(document.createElement('div'), { className: 'tikz-input' });
    b.innerHTML = `<input type="text" aria-label="${esc(what)}" placeholder="${esc(what)}"><small>Enter writes it, Escape leaves it</small>`;
    b.style.left = `${Math.max(8, Math.min(x - 120, innerWidth - 260))}px`;
    b.style.top = `${Math.max(8, Math.min(y - 16, innerHeight - 60))}px`;
    document.body.append(b);
    const input = b.querySelector('input')!;
    input.value = value;
    input.focus(); input.select();
    let rows: PaItem[] = [], shown: PaItem[] = [], at = -1;
    const list = Object.assign(document.createElement('div'), { className: 'pa-suggest' });
    const draw = () => {
      const q = input.value.trim().toLowerCase();
      shown = rows.filter((r) => !q || `${r.label} ${r.hint ?? ''}`.toLowerCase().includes(q)).slice(0, 40);
      at = Math.min(at, shown.length - 1);
      list.innerHTML = shown.map((r, k) => `<button type="button" data-k="${k}"${k === at ? ' class="on"' : ''}>${face(r)}</button>`).join('');
      list.hidden = !shown.length;
    };
    const take = (r: PaItem) => { close(); r.run?.(); };
    if (suggest) { b.querySelector('small')!.before(list); list.hidden = true; suggest.then((r) => { rows = r; if (b.isConnected) draw(); }).catch(() => {}); }
    input.oninput = () => { at = -1; draw(); };
    list.onclick = (e) => { const r = shown[+((e.target as HTMLElement).closest('button') as HTMLElement | null)?.dataset.k!]; if (r) take(r); };
    input.onkeydown = (k) => {
      k.stopPropagation();
      if (k.key === 'Escape') { k.preventDefault(); close(); }
      if ((k.key === 'ArrowDown' || k.key === 'ArrowUp') && shown.length) { k.preventDefault(); at = (at + (k.key === 'ArrowDown' ? 1 : shown.length - 1)) % shown.length; draw(); list.querySelector('.on')?.scrollIntoView({ block: 'nearest' }); }
      if (k.key === 'Enter') { k.preventDefault(); if (shown[at]) return take(shown[at]!); const t = input.value; close(); if (always || t !== value) done(t); }
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
  /** How to have TikZ library `lib`: loaded (by the run or a `\usetikzlibrary` of the files), else joined to this file's
   * last `\usetikzlibrary` list (`{calc}` -> `{calc,lib}`, the list's own separator), else its load after the tikz
   * `\usepackage`, else the line to add by hand. The printer's rule (src/linter/shared.rs `preamble_load`). */
  function libEdit(text: string, all: string, loaded: string[], lib: string): { have: true } | { edit: PaEdit } | { line: string } {
    const name = lib.replace(/\./g, '\\.');
    if (loaded.includes(lib) || new RegExp(`\\\\usetikzlibrary\\s*\\{[^}]*(?<![\\w.])${name}(?![\\w.])`).test(all.replace(/(?<!\\)%.*$/gm, ''))) return { have: true };
    const line = `\\usetikzlibrary{${lib}}`, end = /\\begin\s*\{document\}/.exec(text)?.index ?? text.length;
    const code = text.slice(0, end).replace(/(?<!\\)%.*$/gm, (c) => ' '.repeat(c.length));
    const list = [...code.matchAll(/\\usetikzlibrary\s*\{([^}]*)\}/g)].at(-1);
    if (list) {
      const close = list.index! + list[0].length - 1, items = list[1]!, sep = items.includes(', ') ? ', ' : ',';
      return { edit: { from: close, to: close, now: `${items.trim() ? sep : ''}${lib}` } };
    }
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
  self.satexPageActions = { register: (p) => { plugins.push(p); }, plugins, on, setOn, groups, open, close, ask, pops: POPS, bar: toolbar, face, libEdit, colors, colorRows, mix, fontSet, cut, styles };

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
    hold();
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
  /** The named styles `text` defines (`\tikzset{box/.style={…}}`, a picture's options, `\forestset`, `\tikzstyle{box}=[…]`):
   * name, body, 1-based line, and the comment lines right above the definition. `every …` styles are no names to apply. */
  function styles(text: string): { name: string; body: string; line: number; note: string }[] {
    const out: { name: string; body: string; line: number; note: string }[] = [], lines = text.split('\n');
    const lineOf = (i: number) => text.slice(0, i).split('\n').length;
    // the prose comment above: commented-out code is no note
    const note = (line: number) => { const c: string[] = []; for (let l = line - 2; l >= 0 && /^\s*%/.test(lines[l]!); l--) c.unshift(lines[l]!.replace(/^\s*%+\s?/, '')); return c.filter((x) => !/^\s*\\/.test(x)).join(' ').trim(); };
    const group = (i: number, open: string, shut: string) => { for (let k = i, d = 0; k < text.length; k++) { if (text[k] === '\\') { k++; continue; } if (text[k] === open) d++; else if (text[k] === shut && !--d) return k; } return -1; };
    const scan = (from: number, to: number) => {
      let d = 0, start = from;
      for (let k = from; k <= to; k++) {
        const c = text[k];
        if (c === '{') d++; else if (c === '}') d--;
        if (k === to || (c === ',' && !d)) {
          const kv = text.slice(start, k), m = /^\s*([^=/{}]+?)\s*\/\.(?:append\s+)?style(?:\s+n\s+args\s*=\s*\{?\d\}?)?\s*=\s*(.*)$/s.exec(kv);
          if (m && !/^every\b/.test(m[1]!)) { const at = start + kv.length - kv.trimStart().length, line = lineOf(at); out.push({ name: m[1]!, body: m[2]!.trim().replace(/^\{([^]*)\}$/, '$1').trim(), line, note: note(line) }); }
          start = k + 1;
        }
      }
    };
    for (const m of text.matchAll(/\\(tikzset|forestset|pgfkeys)\s*\{|\\begin\{tikzpicture\}\s*\[|\\tikzstyle\s*\{([^}]*)\}\s*=\s*\[/g)) {
      if (/(?<!\\)%[^\n]*$/.test(text.slice(text.lastIndexOf('\n', m.index!) + 1, m.index))) continue;
      const o = m.index! + m[0].length - 1, e = group(o, text[o]!, text[o] === '[' ? ']' : '}');
      if (e < 0) continue;
      if (m[2] != null) { const line = lineOf(m.index!); out.push({ name: m[2].trim(), body: text.slice(o + 1, e).trim(), line, note: note(line) }); } else scan(o + 1, e);
    }
    return out;
  }
  /** A key as compared: spaces around `=` and runs of spaces dropped. */
  const norm = (k: string) => k.trim().replace(/\s*=\s*/g, '=').replace(/\s+/g, ' ');
  /** `s` (a statement written by a menu row) with the keys of part `i` that spell a document style's whole body (two keys
   * or more, in any order) written as that style's name: `draw, rounded corners, fill=blue!10` becomes `box`. */
  function preferStyle(ctx: PaCtx, s: string | null, i: number) {
    const T = TM(), ks = s == null ? null : T.keysAt(s, i);
    if (!ks?.length) return s;
    const have = ks.map(norm);
    for (const d of styles(ctx.all)) {
      const body = d.body.split(/,(?![^{]*\})/).map(norm).filter(Boolean);
      if (body.length < 2 || have.includes(d.name) || !body.every((b) => have.includes(b))) continue;
      const at = have.findIndex((k) => body.includes(k)), rest = ks.filter((_, j) => !body.includes(have[j]!));
      return T.withKeys(s!, i, [...rest.slice(0, at), d.name, ...rest.slice(at)]);
    }
    return s;
  }
  /** Rows applying the document's named styles to part `i` (-1: the path), each ticked where used, with where it is defined. */
  function namedRows(ctx: PaCtx, stmt: string, i: number, put: (s: string | null) => void): PaItem[] {
    const T = TM(), ks = T.keysAt(stmt, i) ?? [], own = styles(ctx.text), seen = new Set<string>();
    const all = [...own.map((x) => ({ ...x, where: `${ctx.file ?? 'this file'}:${x.line}` })), ...styles(ctx.all).filter((x) => !own.some((o) => o.name === x.name)).map((x) => ({ ...x, where: 'another file' }))]
      .filter((x) => !seen.has(x.name) && seen.add(x.name));
    return all.map((x) => ({ label: x.name, on: ks.includes(x.name), hint: `defined in ${x.where}`, title: `${x.note ? `${x.note}\n` : ''}${x.name}: ${x.body}`,
      run: () => put(T.set(stmt, i, `style:${x.name}`, ks.includes(x.name) ? [] : [x.name])) }));
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
    const named = namedRows(ctx, stmt, i, put);
    return [
      ...(named.length ? [{ label: 'Document styles', items: named }] : []),
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
      const put = (s0: string | null, pre: PaEdit[] = []) => { const s = preferStyle(ctx, s0, n.i); if (s != null && s !== stmt) ctx.write([...pre, { from: n.from, to: n.to, now: s }]); };
      const shapeTo = (to: string, label: string, extra: string[] = []) => { const r = T.shape(stmt, n.i, n.pic, to, extra); if (r) { const pre = needs(ctx, ctx.facts?.shapes?.find((s: Rec) => s.name === to)?.load?.match(/^\\usetikzlibrary\{(.+)\}$/)?.[1] ?? r.library, label); if (pre) put(r.text, pre); } };
      const ks = T.keysAt(stmt, n.i) ?? [], sides = val(ks, 'regular polygon sides'), points = val(ks, 'star points');
      const shapes: PaItem[] = T.SHAPES.map(([k, label]) => k === 'regular polygon'
        ? { label, on: now === k, items: [...['3', '4', '5', '6', '8'].map((v) => ({ label: `${v} sides`, on: now === k && (sides ?? '5') === v, run: () => shapeTo(k, label, v === '5' ? [] : [`regular polygon sides=${v}`]) })),
          custom(ctx, 'Sides…', 'the number of sides', sides ?? '5', (v) => { if (+v >= 3) shapeTo(k, label, +v === 5 ? [] : [`regular polygon sides=${+v}`]); })] }
        : k === 'star' ? { label, on: now === k, items: ['4', '5', '6', '8'].map((v) => ({ label: `${v} points`, on: now === k && (points ?? '5') === v, run: () => shapeTo(k, label, v === '5' ? [] : [`star points=${v}`]) })) }
        : { label, on: now === k, run: () => shapeTo(k, label) }).map((r, i) => ({ ...r, img: shapeImg(ctx, T.SHAPES[i]![0]) }));
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
  const TIPS: [string, string][] = [['→', '->'], ['←', '<-'], ['↔', '<->'], ['none', '']];
  // the tips offered first, in this order (TikZ manual § 16.5: the classic ones, then arrows.meta's); the run's other declared tips follow
  const TIP_SEED = ['>', 'to', 'latex', 'stealth', '|', 'Stealth', 'Latex', 'To', 'Kite', 'Triangle', 'Circle', 'Square', 'Diamond', 'Ellipse', 'Rectangle', 'Turned Square',
    'Bar', 'Bracket', 'Parenthesis', 'Hooks', 'Rays', 'Straight Barb', 'Arc Barb', 'Tee Barb', 'Classical TikZ Rightarrow', 'Computer Modern Rightarrow', 'Implies'];
  /** arrows.meta tips are capitalized; their variants are options (`Stealth[open]`), `open` only where the tip is filled. */
  const META = /^[A-Z]/, OPENS = /^(Stealth|Latex|Kite|Triangle|Circle|Square|Diamond|Ellipse|Rectangle|Turned Square)$/;
  /** The start tip of end tip `t` as written: the classic `>` starts as `<`. */
  const startOf = (t: string) => (t === '>' ? '<' : t), endOf = (t: string) => (t === '<' ? '>' : t);
  /** TeX's picture of node shape `k` from previews.json (`rounded`: shapes.misc's rounded rectangle), else null. */
  function shapeImg(ctx: PaCtx, k: string) {
    const P = ctx.previews ?? {};
    return (k === 'rounded' ? P['shape:rounded rectangle'] : P[`shape:${k}`]) ?? null;
  }
  /** TeX's picture of one end (`-Stealth`, `Stealth-`) from previews.json or the run's tips, `open` drawn unfilled. */
  function tipImg(ctx: PaCtx, name: string, start: boolean, opts: string[]) {
    const T = TM(), spec = start ? T.tipSpec(startOf(name), '') : T.tipSpec('', name);
    const svg = ctx.previews?.[`arrow:${spec}`] ?? (start ? null : (ctx.facts?.tips ?? []).find((x: Rec) => x.name === name)?.svg) ?? null;
    if (!svg) return null;
    // the tip's end of the sample, 12pt of it, so the tip shows large
    const box = svg.match(/viewBox='([^']*)'/)?.[1]!.trim().split(/\s+/).map(Number), cut = box?.length === 4 ? Math.min(box[2]!, 12) : 0;
    const crop = cut ? svg.replace(/viewBox='[^']*'/, `viewBox='${start ? box![0] : box![0]! + box![2]! - cut} ${box![1]} ${cut} ${box![3]}'`).replace(/(<svg[^>]*?) width='[^']*' height='[^']*'/, `$1 width='${cut * 2}' height='${box![3]! * 2}'`) : svg;
    return opts.includes('open') ? crop.replace(/<path d='(?!M0 0H)/g, "<path fill='none' d='") : crop;
  }
  /** The rows of one end's submenu: none, the tips (current one ticked, each with its picture), then the current tip's variants. */
  function tipRows(ctx: PaCtx, now: string, start: boolean, pick: (t: string) => void): PaItem[] {
    const T = TM(), cur = T.tipParts(start ? endOf(now) : now), declared = (ctx.facts?.tips ?? []).map((x: Rec) => String(x.name));
    const more = declared.filter((n: string) => !TIP_SEED.includes(n) && !/[@{}=,]/.test(n));
    const row = (n: string, i: number, head?: string): PaItem => ({ label: n === '>' ? 'default (>)' : n, head: i ? undefined : head, on: cur?.name === n,
      img: tipImg(ctx, n, start, []), run: () => pick(start ? startOf(n) : n) });
    const vary = cur && META.test(cur.name) ? ['open', 'round', 'reversed'].filter((f) => f !== 'open' || OPENS.test(cur.name)) : [];
    return [{ label: 'none', on: !now, run: () => pick('') },
      ...TIP_SEED.filter((n) => !META.test(n)).map((n, i) => row(n, i, 'TikZ')), ...TIP_SEED.filter((n) => META.test(n)).map((n, i) => row(n, i, 'arrows.meta')),
      ...(more.length ? [{ label: 'More of this document', items: more.map((n: string, i: number) => row(n, i)) }] : []),
      ...vary.map((f, i) => ({ label: f, head: i ? undefined : `${cur!.name} variant`, on: cur!.opts.includes(f), run: () => { const v = T.tipVary(now, f, !cur!.opts.includes(f)); if (v != null) pick(v); } }))];
  }
  /** Tip rows of a path: the common pairs, then each end on its own. `now` the path's arrow key, `to` writes a new one. */
  /** The arrow key a path's named style brings (`flow/.style={-Stealth, thick}`), the last style's that has one: its key and the style. */
  function styleTip(ctx: PaCtx, opts: string[]): [string, string] | null {
    const T = TM(), defs = styles(ctx.all);
    for (const k of [...opts].reverse()) {
      const d = defs.find((x) => x.name === k), keys = d ? d.body.split(/,(?![^{]*\})/).map((x) => x.trim()) : [];
      const tip = keys.filter((x) => x.includes('-') && T.tipEnds(x)).at(-1);
      if (tip) return [tip, k];
    }
    return null;
  }
  function tipGroup(ctx: PaCtx, now: string, to: (spec: string, metaNeeded: boolean) => void, from: string | null = null): PaItem[] {
    const T = TM(), [s, e] = T.tipEnds(now) ?? ['', ''], declared = new Set((ctx.facts?.tips ?? []).map((x: Rec) => String(x.name)));
    // a run that declared arrows.meta's tips has the library
    const loaded = [...declared].some((d) => META.test(d)), meta = (t: string) => !loaded && META.test(T.tipParts(t)?.name ?? t);
    const put = (a: string, b: string) => to(T.tipSpec(a, b), meta(a) || meta(b));
    return [...TIPS.map(([label, v], i) => ({ label, head: i || !from ? undefined : `From style ${from}`, on: T.tipSpec(...(T.tipEnds(v) ?? ['', ''])) === T.tipSpec(s, e), run: () => put(...(T.tipEnds(v) ?? ['', ''])) })),
      { label: 'Start', head: 'Each end', items: tipRows(ctx, s, true, (t) => put(t, e)) }, { label: 'End', items: tipRows(ctx, e, false, (t) => put(s, t)) }];
  }
  self.satexPageActions.register({
    name: 'tikz-edge', title: 'TikZ paths', about: 'A path\'s arrow tips, color, width, dash, routing (straight, right angles, bends, angles) and a new label', type: 'tikz', targets: ['tikz-edge'],
    groups(t, ctx) {
      const T = TM(), e: TikzEdge = t.e, k: number = t.k, stmt = ctx.text.slice(e.from, e.to), tip = T.tipOf(e), g = T.segOf(stmt, k);
      const put = (s0: string | null, pre: PaEdit[] = []) => { const s = preferStyle(ctx, s0, -1); if (s != null && s !== stmt) ctx.write([...pre, { from: e.from, to: e.to, now: s }]); };
      const tipTo = (v: string, meta: boolean) => { const pre = needs(ctx, meta ? 'arrows.meta' : null, `The ${v} tip`); if (pre) put(T.tip(stmt, v), pre); };
      const bent = g?.keys.map((x) => /^bend (left|right)(?:\s*=\s*(.*))?$/.exec(x)).find(Boolean), angles = !!g?.keys.some((x) => /^(in|out)\s*=/.test(x));
      const bendRows = (side: string): PaItem[] => [...['15', '30', '45', '60'].map((a) => ({ label: `${a}°`, on: bent?.[1] === side && (bent[2] ?? '30') === a, run: () => put(T.bend(stmt, k, side, +a)) })),
        custom(ctx, 'Custom…', 'the bend angle in degrees', bent?.[2] ?? '30', (v) => { if (Number.isFinite(+v)) put(T.bend(stmt, k, side, +v)); })];
      const loose = /^looseness\s*=\s*(.*)$/.exec(g?.keys.find((x) => /^looseness/.test(x)) ?? '')?.[1] ?? '1';
      return [
        { title: 'Arrow tips', items: tip ? tipGroup(ctx, tip, tipTo) : ((st) => tipGroup(ctx, st?.[0] ?? '', tipTo, st?.[1] ?? null))(styleTip(ctx, e.opts)) },
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
    name: 'tikz-bend', title: 'TikZ bend points', about: 'A bend point of a path (a coordinate between two segments): removed, or the path a curve through it', type: 'tikz', targets: ['tikz-bend'],
    groups(t, ctx) {
      const T = TM(), e: TikzEdge = t.e, k: number = t.k, stmt = ctx.text.slice(e.from, e.to), c: [number, number] | null = t.curve;
      const put = (s: string | null) => { if (s != null && s !== stmt) ctx.write([{ from: e.from, to: e.to, now: s }]); };
      return [{ title: 'Bend point', inline: true, items: [{ label: 'Remove bend point', run: () => put(T.bendRemove(stmt, k)) },
        ...(c ? [{ label: 'Convert to curve', hint: `out=${Math.round(c[0])}, in=${Math.round(c[1])}`, run: () => put(T.bendCurve(stmt, k, c[0], c[1])) }] : [])] }];
    },
  });
  self.satexPageActions.register({
    name: 'tikz-grid', title: 'TikZ picture grid', about: 'A grid over one picture in its own units, its axes labelled at the origin: switched per picture', type: 'tikz', targets: ['tikz-node', 'tikz-label', 'tikz-edge', 'tikz-bend'],
    groups(t, ctx) {
      const at: number | undefined = (t.n ?? t.e)?.pic?.at;
      if (at == null || !ctx.grid) return [];
      const on = ctx.grid(at);
      return [{ title: 'Picture', inline: true, items: [{ label: 'Show grid', on, hint: 'Ctrl snaps to it', run: () => { ctx.grid!(at, !on); } }] }];
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
        { title: 'Shape', inline: true, items: SH.map(([k, l]) => ({ label: l, on: shape === k, img: shapeImg(ctx, k === 'rounded corners' ? 'rounded' : k || 'rectangle'), run: () => write(() => { n.opts = n.opts.filter((o: string) => !SH.some(([s]) => s && key(o) === s)); if (k) n.opts.push(k); }) })) },
        { title: 'Style', inline: true, items: [{ label: 'Frame', on: has('draw'), run: () => write(() => { n.opts = has('draw') ? n.opts.filter((o: string) => key(o) !== 'draw') : [...n.opts, 'draw']; }) }] },
        { title: 'Tree', inline: true, items: [{ label: 'Add child', run: () => write(() => { FOREST.addChild(n); }) },
          ...(parent ? [{ label: 'Add sibling', run: () => write(() => { FOREST.addSibling(tree, n); }) }, { label: n.children.length ? 'Delete with its subtree' : 'Delete', run: () => write(() => { FOREST.remove(tree, n); }) }] : [])] },
        { title: 'Text', inline: true, items: [{ label: 'Edit text…', run: () => ask(ctx.x, ctx.y, n.content, 'the text of the node', (v) => write(() => { n.content = v; })) },
          ...(ctx.forestLink ? [{ label: 'Draw a line to…', hint: 'click the other node', run: () => ctx.forestLink!(t.f) }] : [])] },
        ...(parent ? [{ title: 'Edge from parent', items: edgeRows(ctx, parent, n, write) }] : []),
        { title: 'Every node', items: [
          // the run's strut key (forest plugin), toggled in `for tree`
          ...(ctx.facts?.strut ? [{ label: 'Strut every node', on: FOREST.forTreeKeys(tree).includes(ctx.facts.strut), run: () => write(() => { FOREST.forTreeToggle(tree, ctx.facts!.strut, !FOREST.forTreeKeys(tree).includes(ctx.facts!.strut)); }) }] : []),
          ((lvl: number) => { const w = FOREST.whereLevel(tree.pre, lvl), now = w ? tree.pre.slice(...w.keys) : '';
            return { label: `Style level ${lvl}…`, hint: now || 'where level', run: () => ask(ctx.x, ctx.y, now, `the keys of every node on level ${lvl} (draw, fill=…)`, (v) => write(() => { FOREST.setWhereLevel(tree, lvl, v); }), true) }; })(depth(tree.root, n)),
        ] },
        ...(FOREST.presetOf(tree) ? [] : [{ title: 'Preset', inline: true, items: [{ label: 'Transform to feature model', run: () => toPreset(ctx, t.f, 'feature model') }] }]),
        ...(FOREST.presetOf(tree) === 'feature model' && styleBody(ctx.all, 'fm feature') ? [{ title: 'Feature model look', items: fmLookRows(ctx) }] : []),
      ];
    },
  });
  /** A forest node's depth below `root` (its `level`), -1 when not under it. */
  const depth = (root: Rec, n: Rec): number => root === n ? 0 : root.children.reduce((d: number, c: Rec) => d >= 0 ? d : ((k) => k < 0 ? -1 : k + 1)(depth(c, n)), -1);
  /** The edge into forest node `n` from `parent`: where it leaves the parent (all its edges) and arrives, the anchors of the
   * run's shapes (forest plugin `anchors`), and its label (`edge label`) with its side. */
  function edgeRows(ctx: PaCtx, parent: Rec, n: Rec, write: (change: () => unknown) => void): PaItem[] {
    const FOREST = self.satexViews.forest, list: string[] = ctx.facts?.anchors ?? [], lab = FOREST.edgeLabel(n);
    const anchorRows = (x: Rec, key: string): PaItem[] => [{ label: 'default', on: FOREST.valOf(x, key) == null, run: () => write(() => { FOREST.setKey(x, key, null); }) },
      ...list.map((a) => ({ label: a, on: FOREST.valOf(x, key) === a, run: () => write(() => { FOREST.setKey(x, key, a); }) })),
      custom(ctx, 'Angle or other…', 'an anchor name or an angle in degrees', FOREST.valOf(x, key) ?? '', (v) => write(() => { FOREST.setKey(x, key, v.trim() || null); }))];
    return [{ label: `Leaves ${parent.content || 'the parent'} at`, hint: 'parent anchor', items: anchorRows(parent, 'parent anchor') },
      { label: `Arrives at ${n.content || 'the node'} at`, hint: 'child anchor', items: anchorRows(n, 'child anchor') },
      { label: lab ? 'Edit label…' : 'Add label…', run: () => ask(ctx.x, ctx.y, lab?.text ?? '', 'the label on the edge', (v) => write(() => { FOREST.setEdgeLabel(n, v, lab?.side || 'left'); })) },
      ...(lab ? [{ label: 'Label side', items: ['left', 'right', 'above', 'below', 'auto'].map((sd) => ({ label: sd, on: lab.side === sd, run: () => write(() => { FOREST.setEdgeLabel(n, lab.text, sd); }) })) },
        { label: 'Remove label', run: () => write(() => { FOREST.setEdgeLabel(n, '', ''); }) }] : [])];
  }
  /** The body of style `name`'s definition in `text` (`name/.style={…}`): its range inside the braces, null when not there. */
  function styleBody(text: string, name: string): [number, number] | null {
    const m = new RegExp(`(^|[,{\\s])${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*/\\.style\\s*=\\s*\\{`).exec(text);
    if (!m) return null;
    const a = m.index + m[0].length;
    for (let k = a, d = 1; k < text.length; k++) { if (text[k] === '\\') { k++; continue; } if (text[k] === '{') d++; else if (text[k] === '}' && !--d) return [a, k]; }
    return null;
  }
  /** The keys of a style body split at its top-level commas. */
  const bodyKeys = (b: string) => { const out = ['']; let d = 0; for (const c of b) { if (c === '{') d++; if (c === '}') d--; if (c === ',' && !d) out.push(''); else out[out.length - 1] += c; } return out.map((k) => k.trim()).filter(Boolean); };
  /** The edit setting key `key` of style `name` in `text` to `value` (null: the key goes; '' a bare key), keeping its other keys. */
  function styleKeySet(text: string, name: string, key: string, value: string | null): PaEdit | null {
    const r = styleBody(text, name);
    if (!r) return null;
    const ks = bodyKeys(text.slice(r[0], r[1])), at = ks.findIndex((k) => k.split('=')[0]!.trim() === key), rest = ks.filter((k) => k.split('=')[0]!.trim() !== key);
    if (value != null) rest.splice(at < 0 ? rest.length : at, 0, value === '' ? key : `${key}=${value}`);
    return { from: r[0], to: r[1], now: rest.join(', ') };
  }
  /** The value of key `key` in style `name` of `text` ('' bare, null absent). */
  const styleKeyOf = (text: string, name: string, key: string) => { const r = styleBody(text, name), k = r && bodyKeys(text.slice(r[0], r[1])).find((x) => x.split('=')[0]!.trim() === key); return k == null ? null : k.includes('=') ? k.slice(k.indexOf('=') + 1).trim() : ''; };
  /** A feature model's look, set in its `\forestset` styles: level and sibling distance, which way it grows, arrow ends,
   * and how far a group's arc bends. Each row one edit of the document's style definitions. */
  function fmLookRows(ctx: PaCtx): PaItem[] {
    const F = 'fm feature', t = ctx.text, set = (e: (PaEdit | null)[]) => { if (e.every(Boolean)) ctx.write(e); else ctx.toast('The feature model\'s styles are not in this file: change them where its \\forestset is'); };
    const len = (key: string, list: string[]): PaItem[] => list.map((v) => ({ label: v, on: styleKeyOf(t, F, key) === v, run: () => set([styleKeySet(t, F, key, v)]) }));
    const east = styleKeyOf(t, F, "grow'") === '0' || styleKeyOf(t, F, 'grow') === 'east', edge = styleKeyOf(t, F, 'edge');
    /** The edit growing the tree `grow'=g` (null: down, forest's default) with its edges from anchor `pa` to `ca`. */
    const grow = (g: string | null, pa: string, ca: string) => { const r = styleBody(t, F); if (!r) return null; const ks = bodyKeys(t.slice(...r)).filter((k) => !/^(grow'?|parent anchor|child anchor)\s*=/.test(k));
      return { from: r[0], to: r[1], now: [...ks, ...(g == null ? [] : [`grow'=${g}`]), `parent anchor=${pa}`, `child anchor=${ca}`].join(', ') }; };
    // the arc of `fm alternative` and `fm or`: one bend for both
    const bendOf = (n: string) => { const r = styleBody(t, n); return r && /bend right=(\d+)/.exec(t.slice(...r))?.[1]; };
    const bends = (deg: string) => ['fm alternative', 'fm or'].map((n) => { const r = styleBody(t, n); return r && { from: r[0], to: r[1], now: t.slice(...r).replace(/bend right=\d+/g, `bend right=${deg}`) }; });
    return [
      { label: 'Level distance (l sep)', items: len('l sep', ['4mm', '8mm', '12mm']) },
      { label: 'Sibling distance (s sep)', items: len('s sep', ['2mm', '4mm', '8mm']) },
      { label: 'Grows', items: [{ label: 'down (edges south to north)', on: !east, run: () => set([grow(null, 'south', 'north')]) }, { label: 'right (edges east to west)', on: east, run: () => set([grow('0', 'east', 'west')]) }] },
      { label: 'Arrow ends', items: [['none', null], ['→ to the child', '{->}'], ['← to the parent', '{<-}']].map(([l, v]) => ({ label: l!, on: (edge ?? null) === v, run: () => set([styleKeySet(t, F, 'edge', v)]) })) },
      { label: 'Group arcs', items: ['25', '35', '50'].map((d) => ({ label: `bend ${d}°`, on: bendOf('fm alternative') === d, run: () => set(bends(d)) })) },
    ];
  }
  /** Tree `f` with preset `p` on (its key in `for tree`) and the preset's styles in a `\forestset` above `\begin{document}`
   * once, as the forest widget writes them; one undo step. A file without `\begin{document}` is told the styles it lacks. */
  function toPreset(ctx: PaCtx, f: Rec, p: string) {
    const FOREST = self.satexViews.forest, P = FOREST.PRESETS[p], t = f.t, ft = FOREST.forTree(t.pre), now = ft ? FOREST.splitTop(t.pre.slice(...ft)) : [];
    FOREST.setForTree(t, [...now.filter((k: string) => k !== P.every), P.every].join(', '));
    FOREST.settle(t);
    const edits: PaEdit[] = [{ from: f.from, to: f.to, now: FOREST.emit(t) }], begin = ctx.text.search(/^[ \t]*\\begin\s*\{document\}/m);
    if (!ctx.all.includes(`${P.every}/.style`)) {
      if (begin < 0) ctx.toast(`The ${p} needs its styles: the forest widget writes them where \\begin{document} is`);
      else edits.push({ from: begin, to: begin, now: `\\forestset{\n${(P.styles as [string, string][]).map(([k, v]) => `  ${k}=${v},`).join('\n')}\n}\n` });
    }
    ctx.write(edits);
  }
  const ALIGNS: Record<string, string> = { l: 'left', c: 'centered', r: 'right', p: 'top (paragraph)', m: 'middle (paragraph)', b: 'bottom (paragraph)' };
  /** A paragraph's horizontal alignments (pdfedit.ts HALIGN), each with the letter it is as a natural-width column. */
  const HALIGNS: [string, string, string][] = [['j', 'justified', 'l'], ['l', 'left (\\raggedright)', 'l'], ['c', 'centered (\\centering)', 'c'], ['r', 'right (\\raggedleft)', 'r']];
  self.satexPageActions.register({
    name: 'table-column', title: 'Table columns', about: 'A column\'s alignment and width, rewritten in the column spec, and the table\'s column spacing', type: 'table', targets: ['table-column'],
    groups(t, ctx) {
      const col = t.col, kin = col?.spec && ['lcr', 'pmb', 'X'].find((s) => s.includes(col.align));
      // the table's structure: the column's and, on a cell, its row's insert, delete and move (pdfedit.ts tableOp)
      const op = (what: string, act: string, at: number, to?: number) => () => ctx.tableOp?.(t.t, { what, act, at, to });
      const struct = !ctx.tableOp ? [] : [{ title: 'Column', items: [{ label: 'Insert left', run: op('col', 'insert', t.k) }, { label: 'Insert right', run: op('col', 'insert', t.k + 1) },
        { label: 'Move left', run: op('col', 'move', t.k, t.k - 1) }, { label: 'Move right', run: op('col', 'move', t.k, t.k + 1) }, { label: 'Delete column', run: op('col', 'delete', t.k) }] },
      ...(t.r == null ? [] : [{ title: 'Row', items: [{ label: 'Insert above', run: op('row', 'insert', t.r) }, { label: 'Insert below', run: op('row', 'insert', t.r + 1) },
        { label: 'Move up', run: op('row', 'move', t.r, t.r - 1) }, { label: 'Move down', run: op('row', 'move', t.r, t.r + 1) }, { label: 'Delete row', run: op('row', 'delete', t.r) }] }])];
      if (!kin) return struct;
      // a paragraph's alignment as its `>{…}` sets it; an l, c or r column given a width keeps its own (pdfedit.ts respec)
      const h = self.satexPdfEdit.halign(col.spec.text ?? ''), para = kin !== 'lcr';
      const horiz = { title: 'Paragraph', inline: true, items: HALIGNS.map(([a, label]) => ({ label, on: para && a === h, run: () => ctx.colAlign(t.t, t.k, col.align, col.width, a) })) };
      // the padding each side of every cell (\tabcolsep, an array's \arraycolsep): the table's own, else the run's
      const at = col.spec.range && t.env ? ctx.text.lastIndexOf(`\\begin{${t.env}}`, self.satexPdfEdit.offsetAt(ctx.text, col.spec.range.start)) : -1;
      const pad = at >= 0 ? self.satexPdfEdit.padding(ctx.text, at, t.env) : null, doc = pad && t.seps?.[pad.name];
      const space = pad ? [{ title: 'Spacing', items: [{ label: `default (${doc == null ? 'the document\'s' : `${+doc.toFixed(2)}pt`})`, on: pad.now == null, run: () => ctx.write(pad.edits(null)) },
        ...['0pt', '2pt', '4pt', '6pt', '8pt', '12pt'].map((v) => ({ label: `\\${pad.name} ${v}`, on: pad.now === v, run: () => ctx.write(pad.edits(v)) }))] }] : [];
      if (kin === 'X') return [horiz, ...space, ...struct];
      const widths = ['2cm', '3cm', '4cm', '0.25\\linewidth', '0.33\\linewidth', '0.5\\linewidth'];
      const natural = para ? HALIGNS.find(([a]) => a === h)![2] : col.align;
      return [{ title: 'Align', inline: true, items: [...kin].map((a) => ({ label: `${a}: ${ALIGNS[a]}`, on: a === col.align, run: () => ctx.colAlign(t.t, t.k, a) })) },
        ...(para ? [horiz] : []),
        { title: 'Width', items: [{ label: 'natural (as wide as its text)', on: !para, run: () => ctx.colAlign(t.t, t.k, natural, '') },
          ...widths.map((w) => ({ label: w, on: col.width === w, run: () => ctx.colAlign(t.t, t.k, col.align, w) }))] }, ...space, ...struct];
    },
  });

  // ---------------------------------------------------------------- scratch slices
  // A scratch document is the slice (`satex slice`) of some lines, their own text between two marker lines in it, so the
  // edits made there map back to the lines they came from. Pure: the playground runs the slice and writes the merge.
  // The markers are a satex directive (`%\satex@scratch{…}`): TeX skips the comment, the editor shows it as one.
  const MARK = '%\\satex@scratch', END = `${MARK}{end}`;
  const head = (file: string, from: number, to: number) => `${MARK}{begin ${file}:${from}-${to}} Merge back writes the lines up to the end marker there`;
  const ownLines = (text: string) => text.replace(/\n$/, '').split('\n');
  const region = (text: string, from: number, to: number) => text.split('\n').slice(from - 1, to).join('\n');
  /** The scratch text: `slice` with its copy of lines `from`–`to` of `file` (`text`) replaced by those lines, whole, between
   * the markers. Matched in order, the window with the most of the lines (then the shortest) wins, so a line that occurs
   * twice (`\centering`) does not pull it apart. A file the slice reads whole by `\input` is written out in its place.
   * Null when the slice holds none of the lines. */
  function build(slice: string, file: string, text: string, from: number, to: number, isRoot: boolean): string | null {
    const S = ownLines(slice), R = text.split('\n').slice(from - 1, to);
    const marked = [head(file, from, to), ...R, END];
    const own = R.map((l) => l.trim()).filter((l) => l && !l.startsWith('%'));
    // the lines of `file` in the slice: after its `%% from` header, unless it is the root
    const start = isRoot ? 0 : S.indexOf(`%% from ${file}`) + 1;
    let best: [number, number, number] | null = null;
    for (let c = isRoot || start ? start : S.length; c < S.length; c++) {
      let k = own.indexOf(S[c]!.trim()) + 1, n = 1, last = c;
      if (!k) continue;
      for (let i = c + 1; i < S.length && k < own.length && i - last <= R.length + 1; i++) if (S[i]!.trim() === own[k]) { n++; k++; last = i; }
      if (!best || n > best[2] || n === best[2] && last - c < best[1] - best[0]) best = [c, last, n];
    }
    if (best) return [...S.slice(0, best[0]), ...marked, ...S.slice(best[1] + 1)].join('\n') + '\n';
    const stem = file.replace(/\.tex$/, '').replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), name = file.split('/').pop()!;
    // a standalone slice's own copy of the file (`filecontents*`) goes: the file is written out where it is read
    const fc = S.findIndex((l) => /^\\begin\{filecontents\*?\}/.test(l) && l.trimEnd().endsWith(`{${name}}`)), fe = S.findIndex((l, i) => fc >= 0 && i > fc && /^\\end\{filecontents\*?\}/.test(l));
    if (fc >= 0 && fe > fc) S.splice(fc, fe - fc + 1);
    const load = S.findIndex((l) => new RegExp(`\\\\(?:input|include)\\s*\\{\\s*${stem}(?:\\.tex)?\\s*\\}`).test(l.replace(/%.*/, '')));
    if (load < 0) return null;
    const all = text.split('\n');
    return [...S.slice(0, load), `%% from ${file}`, ...all.slice(0, from - 1), ...marked, ...all.slice(to), ...S.slice(load + 1)].join('\n') + '\n';
  }
  /** A brace-balanced run of lines from 1-based `line` of `L`: a definition over several lines comes whole. */
  function balanced(L: string[], line: number): [number, number] {
    let depth = 0, b = line;
    for (; b <= L.length; b++) {
      for (const m of (L[b - 1] ?? '').replace(/\\[\\{}%]|%.*$/g, '').matchAll(/[{}]/g)) depth += m[0] === '{' ? 1 : -1;
      if (depth <= 0) break;
    }
    return [line, Math.min(b, L.length)];
  }
  /** The local scratch text: the root's preamble as it stands (the same prefix: its run resumes warm), the body's
   * definitions the region needs, the region between the markers, the end. `defs` are the slice's definitions in the
   * document's own files (`file`, 1-based `line`, `name`); one counts when the region or a kept line names it and it
   * stands in the body outside the region. Null when the root has no `\begin{document}`. */
  function local(texts: Record<string, string>, root: string, file: string, from: number, to: number, defs: { file: string; line: number; name: string }[]): string | null {
    const R = texts[root]!.split('\n'), begin = R.findIndex((l) => /^[^%]*\\begin\s*\{document\}/.test(l));
    if (begin < 0) return null;
    const own = texts[file]!.split('\n'), region = own.slice(from - 1, to);
    if (file === root && from <= begin + 1) return [...R.slice(0, from - 1), head(file, from, to), ...region, END, ...R.slice(to, begin + 1), '\\end{document}', ''].join('\n');
    const inBody = (d: { file: string; line: number }) => d.file in texts && !(d.file === root && d.line <= begin + 1) && !(d.file === file && d.line >= from && d.line <= to);
    const named = (text: string, name: string) => new RegExp(`${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![A-Za-z])`).test(text);
    const kept = new Map<string, [string, number, number]>();
    let text = region.join('\n'), grew = true;
    while (grew) {
      grew = false;
      for (const d of defs) {
        const key = `${d.file}:${d.line}`;
        if (kept.has(key) || !inBody(d) || !/^\\[A-Za-z]+$/.test(d.name) || !named(text, d.name)) continue;
        const L = texts[d.file]!.split('\n'), [a, b] = balanced(L, d.line);
        kept.set(key, [d.file, a, b]);
        text += `\n${L.slice(a - 1, b).join('\n')}`;
        grew = true;
      }
    }
    const body = [...kept.values()].sort((x, y) => x[0].localeCompare(y[0]) || x[1] - y[1]).flatMap(([f, a, b]) => [`%% from ${f}:${a}`, ...texts[f]!.split('\n').slice(a - 1, b)]);
    return [...R.slice(0, begin + 1), ...body, ...(file === root ? [] : [`%% from ${file}`]), head(file, from, to), ...region, END, '\\end{document}', ''].join('\n');
  }
  /** The marked region of a scratch text, null when a marker is gone. */
  function extract(text: string): ScRegion | null {
    const L = text.split('\n'), a = L.findIndex((l) => l.startsWith(`${MARK}{begin `)), b = L.findIndex((l, i) => i > a && l.startsWith(END));
    const m = a >= 0 && b > a && /^(.+):(\d+)-(\d+)\}/.exec(L[a]!.slice(MARK.length + 7));
    return m ? { file: m[1]!, from: +m[2]!, to: +m[3]!, body: L.slice(a + 1, b).join('\n') } : null;
  }
  /** Where `mine` (the region's text, sliced from `base` at lines `from`–`to` of a file of `size` lines) goes in the file
   * as it is now (`current`): the same lines when they still read `base`, else the one place they moved to, else a
   * conflict on the lines that stand there now (shifted by what the file grew or shrank). */
  function merge(base: string, mine: string, current: string, from: number, to: number, size: number): ScMerge {
    const C = current.split('\n'), n = to - from + 1;
    if (mine === base) return { status: 'same', from, to, theirs: region(current, from, to) };
    if (region(current, from, to) === base) return { status: 'clean', from, to, theirs: base };
    const at: number[] = [];
    for (let i = 0; i + n <= C.length; i++) if (C.slice(i, i + n).join('\n') === base) at.push(i + 1);
    if (at.length === 1) return { status: 'moved', from: at[0]!, to: at[0]! + n - 1, theirs: base };
    const end = Math.max(from - 1, Math.min(C.length, to + C.length - size));
    return { status: 'conflict', from, to: end, theirs: region(current, from, end) };
  }
  /** `current` with lines `from`–`to` (to = from - 1: none) replaced by `body`. */
  const apply = (current: string, from: number, to: number, body: string) => { const C = current.split('\n'); return [...C.slice(0, from - 1), ...body.split('\n'), ...C.slice(to)].join('\n'); };
  const FLOATS = /^(?:figure|table|wrapfigure|wraptable|sidewaysfigure|sidewaystable)\*?$/, MATH = /^(?:equation|align|gather|multline|flalign|eqnarray|displaymath)\*?$/;
  /** The block around 1-based `line` of `text`: the outermost figure or table, else the outermost display, else the
   * paragraph (to the blank lines around it), named by its environment when it is one. `[from, to, what]`. */
  function block(text: string, line: number): [number, number, string] {
    const L = text.split('\n'), open: [string, number][] = [], around: [string, number, number][] = [];
    L.forEach((l, i) => {
      for (const m of l.replace(/(?<!\\)%.*/, '').matchAll(/\\(begin|end)\s*\{([^}]*)\}/g)) {
        if (m[1] === 'begin') { open.push([m[2]!, i + 1]); continue; }
        const k = open.map(([e]) => e).lastIndexOf(m[2]!);
        if (k < 0) continue;
        const [e, b] = open.splice(k)[0]!;
        if (b <= line && line <= i + 1) around.push([e, b, i + 1]);
      }
    });
    for (const kind of [FLOATS, MATH]) {
      const hit = around.filter(([e]) => kind.test(e)).sort((a, b) => a[1] - b[1])[0];
      if (hit) return [hit[1], hit[2], kind === FLOATS ? hit[0].replace(/\*$/, '').replace(/^(?:wrap|sideways)/, '') : 'equation'];
    }
    // a paragraph ends at a blank line, the document's frame, a heading or a page break
    const head = (i: number) => /^\s*\\(?:part|chapter|(?:sub)*section|(?:sub)?paragraph|clearpage|newpage|maketitle|tableofcontents)\b/.test(L[i - 1] ?? '');
    const blank = (i: number) => !L[i - 1]?.trim() || /\\(?:begin|end)\s*\{document\}/.test(L[i - 1]!) || head(i);
    let a = Math.max(1, Math.min(line, L.length)), b = a;
    if (blank(a)) return [a, a, head(a) ? 'heading' : 'line'];
    while (a > 1 && !blank(a - 1)) a--;
    while (b < L.length && !blank(b + 1)) b++;
    // a paragraph that is one environment (a picture, a list, a tabular) is named by it
    return [a, b, around.find(([, x, y]) => x === a && y === b)?.[0].replace(/\*$/, '') ?? 'paragraph'];
  }
  self.satexScratch = { build, local, extract, merge, apply, block };
  self.satexPageActions.register({
    name: 'slice', title: 'Scratch slices', about: 'Work on a paragraph, figure, table or equation in a scratch document: its slice, fast to check and build, merged back after', type: 'slice',
    targets: ['block', 'tikz-node', 'tikz-label', 'tikz-edge', 'forest-node', 'table-column'],
    groups(t, ctx) {
      const b: ScBlock | undefined = t.block;
      return b && ctx.scratch ? [{ title: 'Scratch', inline: true, items: [{ label: `Work on this ${b.what} in a scratch document`, hint: `${b.file.split('/').pop()}:${b.from}–${b.to}`, run: () => ctx.scratch!(b) }] }] : [];
    },
  });
})();
