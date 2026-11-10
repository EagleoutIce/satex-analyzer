// TeX lengths for every widget: one parser, one formatter, one text field.
// `parts(text)` splits `1.5in` or `0.5\linewidth`, `sp(text, opts)` measures it, `show(sp, unit, opts)` writes it,
// `check(text)` says what is wrong, `field(opts)` is the input all length widgets share.
export {};
interface Opts { dflt?: string; per?: Record<string, number>; names?: Record<string, number> }
interface FieldOpts {
  value?: string; label?: string; placeholder?: string; list?: string; ok?: (text: string) => string; bare?: boolean;
  onvalue?: (text: string) => void; oncommit?: (text: string) => void;
}
type Wrap = HTMLSpanElement & { input: HTMLInputElement };
declare global {
  interface Window {
    satexLengths: {
      parts(text: unknown): { n: number; unit: string } | null; check(text: unknown): string; sp(text: string, o?: Opts): number | null;
      to(text: string, unit: string, o?: Opts): number | null; show(value: number, unit: string, o?: { per?: Record<string, number>; decimals?: number }): string;
      field(o: FieldOpts): Wrap; MESSAGE: string; UNITS: string[];
    };
  }
}
(() => {
  // TeX's units in pt (TeXbook p. 57); `opts.per` (sp per unit, the server's table) wins
  const PT: Record<string, number> = { pt: 1, pc: 12, in: 72.27, bp: 72.27 / 72, cm: 72.27 / 2.54, mm: 7.227 / 2.54, dd: 1238 / 1157, cc: 12 * 1238 / 1157, sp: 1 / 65536 };
  const SP = 65536;
  const RE = /^\s*([-+]?)\s*(\d+\.?\d*|\.\d+)?\s*(pt|pc|in|bp|cm|mm|dd|cc|sp|em|ex|mu|px|\\[A-Za-z@]+)?\s*$/;
  const MESSAGE = 'a length: a number and a unit (2cm, 0.5\\linewidth)';
  /** A length as typed: `{n, unit}` (`1.5in`, `0.5\linewidth`, `-\textwidth` is -1), a bare number has unit ''; else null. */
  const parts = (text: unknown) => {
    const m = RE.exec(String(text));
    if (!m || (m[2] == null && !m[3]?.startsWith('\\'))) return null;
    return { n: (m[1] === '-' ? -1 : 1) * (m[2] == null ? 1 : Number(m[2])), unit: m[3] ?? '' };
  };
  /** What is wrong with `text` as a length, else ''. Empty is none yet. */
  const check = (text: unknown) => !String(text).trim() || parts(text)?.unit ? '' : MESSAGE;
  const factor = (unit: string, per?: Record<string, number>): number | null => per?.[unit] ?? (PT[unit] ? PT[unit] * SP : null);
  /**
   * The length in sp, else null: `opts.dflt` is the unit of a bare number, `opts.per` the sp of each unit,
   * `opts.names` the sp of the length commands it can resolve (`{textwidth: …}` for `\textwidth`).
   */
  const measure = (text: string, { dflt = 'pt', per, names }: Opts = {}) => {
    const p = parts(text);
    if (!p) return null;
    const base = (p.unit || dflt).startsWith('\\') ? names?.[(p.unit || dflt).slice(1)] : factor(p.unit || dflt, per);
    return base == null ? null : p.n * base;
  };
  const sp = (text: string, o?: Opts) => { const v = measure(text, o); return v == null ? null : Math.round(v); };
  /** The length in `unit` as a number, unrounded, else null. */
  const to = (text: string, unit: string, o?: Opts) => { const v = measure(text, o), f = factor(unit, o?.per); return v == null || f == null ? null : v / f; };
  /** `value` sp as text in `unit`, `opts.decimals` digits at most (2). */
  const show = (value: number, unit: string, { per, decimals = 2 }: { per?: Record<string, number>; decimals?: number } = {}) => `${+(value / (factor(unit, per) ?? 1)).toFixed(decimals)}${unit}`;
  let styled = false;
  /**
   * A text input for a length, with the message under it. `onvalue(text)` runs on each valid edit, `oncommit(text)` when
   * the edit ends. `ok(text)` is a widget's own check after the shared one, `bare` takes a bare number (the widget's default unit). Returns the wrapper, `.input` its input.
   */
  const field = ({ value = '', label, placeholder = '', list, ok = () => '', bare = false, onvalue = () => {}, oncommit = () => {} }: FieldOpts): Wrap => {
    if (!styled && self.document) { document.head.append(Object.assign(document.createElement('style'), { textContent: '.sd-len { display: block; min-width: 0; } .sd-len > input { width: 100%; min-width: 0; box-sizing: border-box; } .sd-len-err { display: block; color: var(--error, #c33); font-size: .8em; margin-top: .1rem; } .sd-len-err[hidden] { display: none; }' })); styled = true; }
    const input = Object.assign(document.createElement('input'), { type: 'text', className: 'tv-input', value, placeholder, spellcheck: false });
    if (label) input.setAttribute('aria-label', label);
    if (list) input.setAttribute('list', list);
    const say = Object.assign(document.createElement('span'), { className: 'sd-len-err', hidden: true, role: 'alert' });
    const wrap = Object.assign(document.createElement('span'), { className: 'sd-len' }) as Wrap;
    wrap.append(input, say);
    const test = () => {
      const m = (bare && parts(input.value) && !parts(input.value)?.unit ? '' : check(input.value)) || ok(input.value);
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
