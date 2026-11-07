// TeX lengths for every widget: one parser, one formatter, one text field.
// `parts(text)` splits `1.5in` or `0.5\linewidth`, `sp(text, opts)` measures it, `show(sp, unit, opts)` writes it,
// `check(text)` says what is wrong, `field(opts)` is the input all length widgets share.
(() => {
  // TeX's units in pt (TeXbook p. 57); `opts.per` (sp per unit, the server's table) wins
  const PT = { pt: 1, pc: 12, in: 72.27, bp: 72.27 / 72, cm: 72.27 / 2.54, mm: 7.227 / 2.54, dd: 1238 / 1157, cc: 12 * 1238 / 1157, sp: 1 / 65536 };
  const SP = 65536;
  const RE = /^\s*([-+]?)\s*(\d+\.?\d*|\.\d+)?\s*(pt|pc|in|bp|cm|mm|dd|cc|sp|em|ex|mu|px|\\[A-Za-z@]+)?\s*$/;
  const MESSAGE = 'a length: a number and a unit (2cm, 0.5\\linewidth)';
  /** A length as typed: `{n, unit}` (`1.5in`, `0.5\linewidth`, `-\textwidth` is -1), a bare number has unit ''; else null. */
  const parts = (text) => {
    const m = RE.exec(String(text));
    if (!m || (m[2] == null && !m[3]?.startsWith('\\'))) return null;
    return { n: (m[1] === '-' ? -1 : 1) * (m[2] == null ? 1 : Number(m[2])), unit: m[3] ?? '' };
  };
  /** What is wrong with `text` as a length, else ''. Empty is none yet. */
  const check = (text) => !String(text).trim() || parts(text)?.unit ? '' : MESSAGE;
  const factor = (unit, per) => per?.[unit] ?? (PT[unit] ? PT[unit] * SP : null);
  /**
   * The length in sp, else null: `opts.dflt` is the unit of a bare number, `opts.per` the sp of each unit,
   * `opts.names` the sp of the length commands it can resolve (`{textwidth: …}` for `\textwidth`).
   */
  const measure = (text, { dflt = 'pt', per, names } = {}) => {
    const p = parts(text);
    if (!p) return null;
    const base = (p.unit || dflt).startsWith('\\') ? names?.[(p.unit || dflt).slice(1)] : factor(p.unit || dflt, per);
    return base == null ? null : p.n * base;
  };
  const sp = (text, o) => { const v = measure(text, o); return v == null ? null : Math.round(v); };
  /** The length in `unit` as a number, unrounded, else null. */
  const to = (text, unit, o) => { const v = measure(text, o); return v == null ? null : v / factor(unit, o?.per); };
  /** `value` sp as text in `unit`, `opts.decimals` digits at most (2). */
  const show = (value, unit, { per, decimals = 2 } = {}) => `${+(value / factor(unit, per)).toFixed(decimals)}${unit}`;
  let styled = false;
  /**
   * A text input for a length, with the message under it. `onvalue(text)` runs on each valid edit, `oncommit(text)` when
   * the edit ends. `ok(text)` is a widget's own check after the shared one, `bare` takes a bare number (the widget's default unit). Returns the wrapper, `.input` its input.
   */
  const field = ({ value = '', label, placeholder = '', list, ok = () => '', bare = false, onvalue = () => {}, oncommit = () => {} }) => {
    if (!styled && self.document) { document.head.append(Object.assign(document.createElement('style'), { textContent: '.sd-len { display: block; min-width: 0; } .sd-len > input { width: 100%; min-width: 0; box-sizing: border-box; } .sd-len-err { display: block; color: var(--error, #c33); font-size: .8em; margin-top: .1rem; } .sd-len-err[hidden] { display: none; }' })); styled = true; }
    const input = Object.assign(document.createElement('input'), { type: 'text', className: 'tv-input', value, placeholder, spellcheck: false });
    if (label) input.setAttribute('aria-label', label);
    if (list) input.setAttribute('list', list);
    const say = Object.assign(document.createElement('span'), { className: 'sd-len-err', hidden: true, role: 'alert' });
    const wrap = Object.assign(document.createElement('span'), { className: 'sd-len' });
    wrap.append(input, say);
    const test = () => {
      const m = (bare && parts(input.value) && !parts(input.value).unit ? '' : check(input.value)) || ok(input.value);
      say.textContent = m;
      say.hidden = !m;
      for (const c of ['tv-bad', 'sd-bad']) input.classList.toggle(c, !!m);
      input.setAttribute('aria-invalid', String(!!m));
      return !m;
    };
    input.addEventListener('input', () => { if (test()) onvalue(input.value.trim()); });
    input.addEventListener('change', () => { if (test()) oncommit(input.value.trim()); });
    test();
    wrap.input = input;
    return wrap;
  };
  self.satexLengths = { parts, check, sp, to, show, field, MESSAGE, UNITS: Object.keys(PT) };
})();
