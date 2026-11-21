// Math laid out from its TeX source for the live preview of typed math (playground.js draws it on the PDF), and the
// script and spacing parameters the Typeset view's math takes (site.ts). Cosmetic only: the build sets the real formula.
// TeX's rules (The TeXbook, Appendix G) over its noads, simplified where only a glyph's ink is known.
export {};
/** A glyph `[text, x, y, scale, face, sy]` in em of the formula's size (`y` up from the baseline; face 'it' math italic,
 * 'rm' upright, 'bf' bold, 'sy' symbols, 'ex' large operators, 'ms' a math font's alphabets), `sy` its extra vertical stretch (delimiters);
 * a rule `[x, y, w, h]`; the box's width, height and depth about the baseline. */
type Glyph = [string, number, number, number, string, number?];
/** `br`: a formula in text's glue between the atoms of its outer list, from `at` to `next`, `mu` its TeX space (3 thin,
 * 4 medium: plus 2mu minus 4mu, 5 thick: plus 5mu; 0 in a script); with `pen` the line may break there at that penalty,
 * after a binary operator or relation (tex.web §767), the space then dropped. */
type MBox = { g: Glyph[]; r: [number, number, number, number][]; w: number; h: number; d: number; br?: { at: number; next: number; mu: number; pen?: number }[] };
/** Measures `t` in `face` at 1 em: its width, or `[width, height, depth]` of its ink. */
type Metric = (t: string, face: string) => number | [number, number, number];
/** The font parameters TeX's math reads, in em: cmsy10's σ5..σ22, cmex10's ξ8..ξ13 (`\fontdimen`), and what the
 * preview adds: a math italic letter's italic correction, a script's space, how much wider the script fonts set
 * (cmr7 and cmmi7 over cmr10 and cmmi10 scaled down: `1` is 0.57 em against 0.5), and the width of cmex10's ∑ in text
 * and display and of its `=`, `+`, `≤` (cmr10, cmsy10; a fallback font's are narrower). */
const PARAMS = {
  xHeight: 0.430555, quad: 1, num1: 0.676508, num2: 0.393732, num3: 0.443731, denom1: 0.685951, denom2: 0.344841,
  sup1: 0.412892, sup2: 0.362892, sup3: 0.288889, sub1: 0.15, sub2: 0.247217, supDrop: 0.386108, subDrop: 0.05,
  delim1: 2.39, delim2: 1.01, axis: 0.25, rule: 0.04, bigOp1: 0.111112, bigOp2: 0.166667, bigOp3: 0.2, bigOp4: 0.6, bigOp5: 0.1,
  italic: 0.05, scriptSpace: 0.05, script: 1.12, scriptScript: 1.3, opText: 1.05556, opDisplay: 1.44445, sign: 0.77778,
  // plain TeX's and LaTeX's \\relpenalty and \\binoppenalty: what a break after a relation or binary operator costs
  relPenalty: 500, binPenalty: 700,
};
type Params = typeof PARAMS;
/** Styles: 0 display, 1 text, 2 script, 3 scriptscript; their sizes. */
const SIZE = [1, 1, 0.7, 0.5];
type Style = { s: number; c: boolean };
const up = (st: Style): Style => ({ s: st.s < 2 ? 2 : 3, c: st.c });
const down = (st: Style): Style => ({ s: up(st).s, c: true });
const numer = (st: Style): Style => ({ s: st.s === 0 ? 1 : st.s === 1 ? 2 : 3, c: st.c });
const denom = (st: Style): Style => ({ ...numer(st), c: true });
/** TeX's spaces between atoms (tex.web §764): rows the left atom, columns the right one, in order ord op bin rel
 * open close punct inner; 0 none, 1 thin unless in a script, 2 thin, 3 medium unless in a script, 4 thick unless in one. */
const CLASSES = ['ord', 'op', 'bin', 'rel', 'open', 'close', 'punct', 'inner'];
const SPACING = '02340001' + '22*40001' + '33**3**3' + '44*04004' + '00*00000' + '02340001' + '11*11111' + '12341011';
const spaceMu = (a: string, b: string, script: boolean): number => {
  const c = SPACING[CLASSES.indexOf(a) * 8 + CLASSES.indexOf(b)];
  return c === '2' || c === '1' && !script ? 3 : c === '3' && !script ? 4 : c === '4' && !script ? 5 : 0;
};

/** A noad: its class, nucleus and scripts; `lim` true for `\limits`, false for `\nolimits`, unset for TeX's default. */
type Noad = { k: string; n: Field; sup?: Field; sub?: Field; lim?: boolean };
type Field =
  | { t: 'char'; c: string; f: string } | { t: 'text'; c: string; f: string } | { t: 'list'; l: Noad[] }
  | { t: 'frac'; a: Noad[]; b: Noad[]; bar: boolean; st?: number; l?: string; r?: string }
  | { t: 'sqrt'; a: Noad[]; i?: Noad[] } | { t: 'acc'; c: string; a: Noad[]; wide?: boolean }
  | { t: 'line'; over: boolean; a: Noad[] } | { t: 'fence'; l: string; r: string; a: Noad[] }
  | { t: 'big'; c: string; size: number } | { t: 'grid'; rows: Noad[][][]; align: string; gap: number[]; st: number; l: string; r: string }
  | { t: 'space'; w: number; mu: boolean } | { t: 'style'; st: number } | { t: 'phantom'; a: Noad[]; h: boolean; v: boolean };

/** The display-only fallback of the preview: a command's Unicode character, its face and its class. For drawing
 * only; what a command means is the build's. */
let SYMS: Map<string, [string, string, string]> | null = null;
function sym(name: string): [string, string, string] | undefined {
  SYMS ??= new Map(Object.entries({
    'it ord': 'alpha α beta β gamma γ delta δ epsilon ϵ varepsilon ε zeta ζ eta η theta θ vartheta ϑ iota ι kappa κ lambda λ mu μ nu ν xi ξ pi π varpi ϖ rho ρ varrho ϱ sigma σ varsigma ς tau τ upsilon υ phi ϕ varphi φ chi χ psi ψ omega ω ell ℓ imath ı jmath ȷ wp ℘',
    'rm ord': 'Gamma Γ Delta Δ Theta Θ Lambda Λ Xi Ξ Pi Π Sigma Σ Upsilon Υ Phi Φ Psi Ψ Omega Ω # # % % & & $ $ _ _ S § P ¶',
    'sy ord': 'infty ∞ partial ∂ nabla ∇ forall ∀ exists ∃ nexists ∄ emptyset ∅ varnothing ∅ neg ¬ lnot ¬ prime ′ ldots … dots … cdots ⋯ vdots ⋮ ddots ⋱ angle ∠ top ⊤ bot ⊥ hbar ℏ Re ℜ Im ℑ aleph ℵ | ‖ Vert ‖ vert | backslash ∖ triangle △ Box □ Diamond ◇ clubsuit ♣ diamondsuit ♢ heartsuit ♡ spadesuit ♠ flat ♭ natural ♮ sharp ♯ surd √ mho ℧ complement ∁ dagger † ddagger ‡ checkmark ✓ colon :',
    'sy bin': 'pm ± mp ∓ times × cdot ⋅ div ÷ ast ∗ star ⋆ circ ∘ bullet ∙ cup ∪ cap ∩ setminus ∖ wedge ∧ land ∧ vee ∨ lor ∨ oplus ⊕ ominus ⊖ otimes ⊗ oslash ⊘ odot ⊙ uplus ⊎ sqcup ⊔ sqcap ⊓ amalg ⨿ wr ≀ diamond ⋄ bigtriangleup △ bigtriangledown ▽ triangleleft ◁ triangleright ▷ bigcirc ◯',
    'sy rel': 'le ≤ leq ≤ ge ≥ geq ≥ ne ≠ neq ≠ ll ≪ gg ≫ approx ≈ equiv ≡ sim ∼ simeq ≃ cong ≅ propto ∝ in ∈ notin ∉ ni ∋ subset ⊂ subseteq ⊆ supset ⊃ supseteq ⊇ sqsubseteq ⊑ sqsupseteq ⊒ prec ≺ succ ≻ preceq ⪯ succeq ⪰ asymp ≍ doteq ≐ triangleq ≜ coloneqq ≔ bowtie ⋈ smile ⌣ frown ⌢ mid ∣ parallel ∥ perp ⊥ to → rightarrow → leftarrow ← gets ← leftrightarrow ↔ Rightarrow ⇒ Leftarrow ⇐ Leftrightarrow ⇔ longrightarrow ⟶ longleftarrow ⟵ Longrightarrow ⟹ Longleftarrow ⟸ implies ⟹ iff ⟺ mapsto ↦ longmapsto ⟼ hookrightarrow ↪ hookleftarrow ↩ uparrow ↑ downarrow ↓ updownarrow ↕ Uparrow ⇑ Downarrow ⇓ nearrow ↗ searrow ↘ rightharpoonup ⇀ leftharpoonup ↼ rightleftharpoons ⇌ leadsto ⇝ vdash ⊢ dashv ⊣ models ⊨',
    'ex op': 'sum ∑ prod ∏ coprod ∐ int ∫ iint ∬ iiint ∭ oint ∮ bigcup ⋃ bigcap ⋂ bigvee ⋁ bigwedge ⋀ bigoplus ⨁ bigotimes ⨂ bigodot ⨀ biguplus ⨄ bigsqcup ⨆',
    'sy open': 'langle ⟨ lbrace { { { lceil ⌈ lfloor ⌊ lbrack [ lvert | lVert ‖',
    'sy close': 'rangle ⟩ rbrace } } } rceil ⌉ rfloor ⌋ rbrack ] rvert | rVert ‖',
  }).flatMap(([fk, list]) => { const [f, k] = fk.split(' ') as [string, string], xs = list.split(' '); return xs.flatMap((x, j) => j % 2 ? [] : [[x, [xs[j + 1]!, f, k]] as [string, [string, string, string]]]); }));
  return SYMS.get(name);
}
/** Operator names set upright; those whose scripts are limits in display (`\lim`, `\max`), the others beside. */
const OPNAME = /^(arc)?(sin|cos|tan|cot)h?$|^(sec|csc|log|ln|lg|exp|lim(inf|sup)?|max|min|sup|inf|det|dim|ker|deg|gcd|arg|Pr|hom|argmax|argmin)$/;
const OPLIMITS = /^(lim(inf|sup)?|max|min|sup|inf|det|gcd|Pr|argmax|argmin)$/;
/** Accents over a nucleus, as spacing characters; `wide` stretch over it. */
const ACCENTS: Record<string, string> = { hat: 'ˆ', check: 'ˇ', tilde: '˜', acute: '´', grave: '`', dot: '˙', ddot: '¨', breve: '˘', bar: '¯', vec: '→', mathring: '˚', widehat: 'ˆ', widetilde: '˜', overrightarrow: '→', overleftarrow: '←' };
/** Math alphabets: the first capital of each, the letters Unicode keeps elsewhere (ℬ, ℂ). */
const ALPHA: Record<string, [number, number, string]> = {
  mathcal: [0x1d49c, 0x1d4b6, 'B ℬ E ℰ F ℱ H ℋ I ℐ L ℒ M ℳ R ℛ e ℯ g ℊ o ℴ'], mathscr: [0x1d49c, 0x1d4b6, 'B ℬ E ℰ F ℱ H ℋ I ℐ L ℒ M ℳ R ℛ e ℯ g ℊ o ℴ'],
  mathbb: [0x1d538, 0x1d552, 'C ℂ H ℍ N ℕ P ℙ Q ℚ R ℝ Z ℤ'], mathfrak: [0x1d504, 0x1d51e, 'C ℭ H ℌ I ℑ R ℜ Z ℨ'],
};
const alpha = (font: string, c: string): string => {
  const [cap, low, holes] = ALPHA[font]!, h = holes.split(' '), j = h.indexOf(c);
  if (j >= 0 && j % 2 === 0) return h[j + 1]!;
  if (/[A-Z]/.test(c)) return String.fromCodePoint(cap + c.charCodeAt(0) - 65);
  if (/[a-z]/.test(c)) return String.fromCodePoint(low + c.charCodeAt(0) - 97);
  if (font === 'mathbb' && /[0-9]/.test(c)) return String.fromCodePoint(0x1d7d8 + c.charCodeAt(0) - 48);
  return c;
};
/** Grids: per environment its delimiters, column alignment (repeated), column gap in em and cell style. */
const GRIDS: Record<string, [string, string, string, number, number]> = {
  matrix: ['', '', 'c', 1, 1], pmatrix: ['(', ')', 'c', 1, 1], bmatrix: ['[', ']', 'c', 1, 1], Bmatrix: ['{', '}', 'c', 1, 1],
  vmatrix: ['|', '|', 'c', 1, 1], Vmatrix: ['‖', '‖', 'c', 1, 1], smallmatrix: ['', '', 'c', 0.5, 2], array: ['', '', 'c', 1, 1],
  cases: ['{', '', 'l', 1, 1], dcases: ['{', '', 'l', 1, 0], rcases: ['', '}', 'l', 1, 1], aligned: ['', '', 'rl', 0, 0], alignedat: ['', '', 'rl', 0, 0],
  gathered: ['', '', 'c', 0, 0], split: ['', '', 'rl', 0, 0], subarray: ['', '', 'c', 0, 2],
};

/** `src`, math in TeX's notation, as noads. Never fails: a command half typed is its name, an open group runs to the end.
 * `rows`: a display's body (`align`, `gather`) whose `&` and `\\` make an aligned grid. */
function parse(src: string, rows = false): Noad[] {
  let i = 0;
  const skip = () => { while (/\s/.test(src[i] ?? '')) i++; };
  const cmd = (): string => { const m = /^\\([A-Za-z@]+\*?|.?)/.exec(src.slice(i))!; i += m[0].length; return m[1]!; };
  /** A group's text as written, or the next token's. */
  const raw = (): string => {
    skip();
    if (src[i] !== '{') { const m = /^\\(?:[A-Za-z@]+|.?)|^[^]?/.exec(src.slice(i))![0]; i += m.length; return m; }
    let n = 1, j = ++i;
    for (; j < src.length && n; j++) n += src[j] === '\\' ? (j++, 0) : src[j] === '{' ? 1 : src[j] === '}' ? -1 : 0;
    const t = src.slice(i, n ? j : j - 1);
    i = j;
    return t;
  };
  const opt = (): string | null => { skip(); if (src[i] !== '[') return null; const e = src.indexOf(']', i); const t = src.slice(i + 1, e < 0 ? src.length : e); i = e < 0 ? src.length : e + 1; return t; };
  /** A delimiter after `\left`, `\big`: its character, '' for `.`. */
  const delim = (): string => {
    skip();
    if (src[i] === '\\') { const n = cmd(); return n === '{' || n === '}' ? n : n === '|' ? '‖' : sym(n)?.[0] ?? ''; }
    const c = src[i++] ?? '';
    return c === '.' ? '' : c;
  };
  /** One argument: a group's noads, else the next atom's. */
  const arg = (face: string | null = null): Noad[] => {
    skip();
    if (i >= src.length || src[i] === '}') return [];
    if (src[i] === '{') { i++; return list('}', face).l; }
    const a = atom(face, true);
    return a ? [a] : [];
  };
  const ord = (n: Field, k = 'ord'): Noad => ({ k, n });
  /** Noads up to `close` (or the end); in a grid also up to `&`, `\\` or `\end`, `stop` saying which ended it. */
  const list = (close: string | null, face: string | null, grid = false, fence = false): { l: Noad[]; stop: string } => {
    let l: Noad[] = [];
    let over: [Noad[], string] | null = null;
    for (skip(); i < src.length; skip()) {
      const c = src[i]!;
      if (c === close) { i++; break; }
      if (c === '}') { i++; continue; }
      if (grid && c === '&') { i++; return { l: fin(l, over), stop: '&' }; }
      if (grid && src.startsWith('\\\\', i)) { i += 2; opt(); return { l: fin(l, over), stop: 'row' }; }
      if (grid && /^\\end\b/.test(src.slice(i))) { i += 4; raw(); return { l: fin(l, over), stop: 'end' }; }
      if (fence && /^\\(right|middle)(?![A-Za-z])/.test(src.slice(i))) return { l: fin(l, over), stop: 'fence' };
      if (c === '^' || c === '_' || c === "'") {
        i++;
        const last = l.at(-1), at = last && last.n.t !== 'space' && last.n.t !== 'style' && !(c === '_' ? last.sub : c === '^' && last.sup && !/^′+$/.test(chars(last.sup))) ? last : (l.push(ord({ t: 'list', l: [] })), l.at(-1)!);
        if (c === "'") { at.sup = { t: 'list', l: [...(at.sup?.t === 'list' ? at.sup.l : []), ord({ t: 'char', c: '′', f: 'sy' })] }; continue; }
        const x: Field = { t: 'list', l: arg(face) };
        if (c === '^') at.sup = at.sup?.t === 'list' ? { t: 'list', l: [...at.sup.l, ...x.l] } : x; else at.sub = x;
        continue;
      }
      if (/^\\(over|atop|choose|brack|brace)(?![A-Za-z])/.test(src.slice(i))) { const n = cmd(); over = [l, n]; l = []; continue; }
      if (/^\\(limits|nolimits|displaylimits)(?![A-Za-z])/.test(src.slice(i))) { const n = cmd(), last = l.at(-1); if (last) last.lim = n === 'limits' ? true : n === 'nolimits' ? false : undefined; continue; }
      const a = atom(face, false);
      if (a) l.push(a);
    }
    return { l: fin(l, over), stop: '' };
  };
  const fin = (l: Noad[], over: [Noad[], string] | null): Noad[] => {
    if (!over) return l;
    const [a, n] = over, lr = ({ choose: '()', brack: '[]', brace: '{}' } as Record<string, string>)[n] ?? '';
    return [ord({ t: 'frac', a, b: l, bar: n === 'over', l: lr[0], r: lr[1] }, 'inner')];
  };
  const chars = (f: Field): string => f.t === 'char' ? f.c : f.t === 'list' ? f.l.map((n) => chars(n.n)).join('') : '';
  /** A grid environment's body, its cells split at `&` and its rows at `\\`. */
  const grid = (env: string): Field => {
    const [l, r, al, gap, st] = GRIDS[env] ?? GRIDS.matrix!;
    let align = al;
    if (env === 'array' || env === 'subarray') align = (raw().match(/[lcr]/g) ?? ['c']).join('');
    if (env === 'alignedat') raw();
    const rows: Noad[][][] = [[]];
    for (;;) {
      const { l: cell, stop } = list(null, null, true);
      rows.at(-1)!.push(cell);
      if (stop === 'row') rows.push([]);
      else if (stop !== '&') break;
    }
    if (rows.length > 1 && rows.at(-1)!.every((c) => !c.length)) rows.pop();
    return { t: 'grid', rows, align, gap: /rl/.test(al) ? [0, 2] : [gap], st, l, r };
  };
  /** The next atom, or null for markup with no glyph; `one` reads a single character (`\frac12`). */
  const atom = (face: string | null, one: boolean): Noad | null => {
    skip();
    const c = src[i];
    if (c == null) return null;
    if (c === '{') { i++; return ord({ t: 'list', l: list('}', face).l }); }
    if (c === '\\') {
      const n = cmd();
      const mu = ({ ',': 3, thinspace: 3, ':': 4, '>': 4, medspace: 4, ';': 5, thickspace: 5, '!': -3, negthinspace: -3 } as Record<string, number>)[n];
      if (mu != null) return ord({ t: 'space', w: mu, mu: true });
      const em = ({ ' ': 1 / 3, quad: 1, qquad: 2, enspace: 0.5, '\\': 0 } as Record<string, number>)[n];
      if (em != null) return ord({ t: 'space', w: em, mu: false });
      if (n === 'hspace' || n === 'hspace*' || n === 'mspace' || n === 'kern' || n === 'mkern') { const t = raw(); const v = parseFloat(t) || 0; return ord({ t: 'space', w: /mu/.test(t) ? v : v * (/em/.test(t) ? 18 : /pt/.test(t) ? 1.8 : 1), mu: true }); }
      if (/^([dt]?frac|cfrac|[dt]?binom|genfrac)$/.test(n)) {
        if (n === 'genfrac') { raw(); raw(); raw(); raw(); }
        const a = arg(face), b = arg(face), bin = /binom/.test(n);
        return ord({ t: 'frac', a, b, bar: !bin, st: n[0] === 'd' || n === 'cfrac' ? 0 : n[0] === 't' ? 1 : undefined, l: bin ? '(' : undefined, r: bin ? ')' : undefined }, 'inner');
      }
      if (n === 'sqrt') { const ix = opt(); const save = [src, i] as const; const index = ix == null ? undefined : (src = ix, i = 0, list(null, face).l); [src, i] = save; return ord({ t: 'sqrt', a: arg(face), i: index }); }
      if (ACCENTS[n]) return ord({ t: 'acc', c: ACCENTS[n]!, a: arg(face), wide: /^(wide|over)/.test(n) || n === 'vec' });
      if (n === 'overline' || n === 'underline') return ord({ t: 'line', over: n === 'overline', a: arg(face) });
      if (n === 'overbrace' || n === 'underbrace') return { k: 'op', n: { t: 'line', over: n === 'overbrace', a: arg(face) }, lim: true };
      if (n === 'left') {
        // the body up to `\right` (or the end, still being typed), a `\middle` delimiter sized with the others
        const l = delim(), body: Noad[] = [];
        let r = '';
        for (;;) {
          const sub = list(null, face, false, true);
          body.push(...sub.l);
          if (sub.stop !== 'fence') break;
          if (cmd() === 'right') { r = delim(); break; }
          body.push(ord({ t: 'big', c: delim(), size: 0 }, 'rel'));
        }
        return ord({ t: 'fence', l, r, a: body }, 'inner');
      }
      if (n === 'right' || n === 'middle') { delim(); return null; }
      const big = /^([bB]igg?)([lrm]?)$/.exec(n);
      if (big) return ord({ t: 'big', c: delim(), size: { big: 1.2, Big: 1.8, bigg: 2.4, Bigg: 3 }[big[1]!]! }, { l: 'open', r: 'close', m: 'rel' }[big[2]!] ?? 'ord');
      if (n === 'begin') { const env = raw().replace(/\*$/, ''); return ord(GRIDS[env] ? grid(env) : { t: 'list', l: list(null, face).l }); }
      if (/^(text(rm|bf|it|sf|tt|normal|up|md|sl)?|mbox|hbox|emph|textsc)$/.test(n)) return ord({ t: 'text', c: raw().replace(/\\[A-Za-z@]+\s*|[{}$]/g, ''), f: n === 'textbf' ? 'bf' : /^(textit|emph|textsl)$/.test(n) ? 'it' : 'rm' });
      if (n === 'operatorname' || n === 'operatorname*') return { k: 'op', n: { t: 'text', c: raw().replace(/\\[A-Za-z@]+\s*|[{}]/g, ''), f: 'rm' }, lim: n.endsWith('*') ? undefined : false };
      if (ALPHA[n]) return ord({ t: 'list', l: arg(n) });
      const f = /^(mathbf|boldsymbol|bm|pmb)$/.test(n) ? 'bf' : n === 'mathit' ? 'it' : /^math(rm|sf|tt|normal|up)$/.test(n) ? 'rm' : null;
      if (f) return ord({ t: 'list', l: arg(f) });
      const cls = /^math(ord|op|bin|rel|open|close|punct|inner)$/.exec(n);
      if (cls) return { k: cls[1]!, n: { t: 'list', l: arg(face) } };
      if (/^(h|v)?phantom$/.test(n)) return ord({ t: 'phantom', a: arg(face), h: n !== 'vphantom', v: n !== 'hphantom' });
      if (n === 'mathstrut') return ord({ t: 'phantom', a: [ord({ t: 'char', c: '(', f: 'rm' })], h: false, v: true });
      if (/^(color|label|tag\*?|eqref|ref|hfill)$/.test(n)) { if (n !== 'hfill') raw(); return null; }
      if (/^(textcolor|colorbox)$/.test(n)) { raw(); return ord({ t: 'list', l: arg(face) }); }
      const style = ['displaystyle', 'textstyle', 'scriptstyle', 'scriptscriptstyle'].indexOf(n);
      if (style >= 0) return ord({ t: 'style', st: style });
      if (/^(nonumber|notag|nolimits|limits|displaylimits|end|relax|hline|cr|nobreak|allowbreak|quad\*|smash)$/.test(n)) { if (n === 'end') raw(); return null; }
      if (n === 'not') { const a = atom(face, true); if (a?.n.t === 'char') a.n.c = a.n.c === '=' ? '≠' : a.n.c === '∈' ? '∉' : `${a.n.c}̸`; return a; }
      const s = sym(n);
      if (s) return { k: s[2], n: { t: 'char', c: s[0], f: s[1] === 'it' && face === 'bf' ? 'bf' : s[1] }, ...(s[2] === 'op' && /^i*int$|^oint$/.test(n) ? { lim: false } : {}) };
      if (OPNAME.test(n)) return { k: 'op', n: { t: 'text', c: n, f: 'rm' }, ...(OPLIMITS.test(n) ? {} : { lim: false }) };
      if (/^[{}|#%&$_]$/.test(n)) return ord({ t: 'char', c: n === '|' ? '‖' : n, f: 'rm' }, n === '{' ? 'open' : n === '}' ? 'close' : 'ord');
      // any other command (a user's macro, a name half typed): its name, upright
      return n ? ord({ t: 'text', c: n, f: 'rm' }) : null;
    }
    i++;
    if (c === '~') return ord({ t: 'space', w: 1 / 3, mu: false });
    if (c === '&') return ord({ t: 'space', w: 0, mu: false });
    if (/[A-Za-z]/.test(c)) return ord({ t: 'char', c: face && ALPHA[face] ? alpha(face, c) : c, f: face && ALPHA[face] ? 'ms' : face ?? 'it' });
    if (/[0-9]/.test(c)) {
      const m = one ? '' : /^[0-9.]*/.exec(src.slice(i))![0];
      i += m.length;
      return ord({ t: 'char', c: face === 'mathbb' ? [...c + m].map((d) => alpha('mathbb', d)).join('') : c + m, f: face === 'bf' ? 'bf' : face === 'mathbb' ? 'ms' : 'rm' });
    }
    const k = '+-*'.includes(c) ? 'bin' : '=<>:'.includes(c) ? 'rel' : '(['.includes(c) ? 'open' : ')]!?'.includes(c) ? 'close' : ',;'.includes(c) ? 'punct' : 'ord';
    const t = ({ '-': '−', '*': '∗' } as Record<string, string>)[c] ?? c;
    return ord({ t: 'char', c: t, f: t === c ? 'rm' : 'sy' }, k);
  };
  if (!rows) return list(null, null).l;
  const g = grid('aligned') as Field & { t: 'grid' };
  if (g.rows.length === 1 && g.rows[0]!.length === 1) return g.rows[0]![0]!;
  if (g.rows.every((r) => r.length === 1)) Object.assign(g, { align: 'c', gap: [0] });
  return [ord(g)];
}

/** Math source `src` laid out: inline in text style, or `display`. `metric` measures a glyph; `params` override TeX's
 * font parameters (the run's own fonts, when the client has them). */
function layout(src: string, metric: Metric, display = false, params: Partial<Params> = {}): MBox {
  const P = { ...PARAMS, ...params };
  const empty = (): MBox => ({ g: [], r: [], w: 0, h: 0, d: 0 });
  const put = (into: MBox, b: MBox, dx: number, dy: number) => {
    for (const [t, x, y, s, f, sy] of b.g) into.g.push(sy == null ? [t, x + dx, y + dy, s, f] : [t, x + dx, y + dy, s, f, sy]);
    for (const [x, y, w, h] of b.r) into.r.push([x + dx, y + dy, w, h]);
    into.h = Math.max(into.h, b.h + dy); into.d = Math.max(into.d, b.d - dy);
  };
  /** A glyph's width, height and depth at 1 em: measured, its height and depth guessed where only a width is. */
  const box = (t: string, f: string): [number, number, number] => {
    const m = metric(t, f);
    if (Array.isArray(m)) return m;
    const d = /[()[\]{}|‖⟨⟩⌊⌋⌈⌉∑∏∐⋃⋂⨁⨂]/.test(t) ? 0.25 : /[gjpqy,;βγζημξρφχψς]/.test(t) || f === 'it' && /f/.test(t) ? 0.194 : 0;
    const h = /[()[\]{}|‖⟨⟩⌊⌋⌈⌉∑∏∐⋃⋂⨁⨂]/.test(t) ? 0.75 : /[∫∮∬]/.test(t) ? 0.8 : /[A-Z0-9bdfhklΓ-Ωβδζθλξ∂ℓ′]/.test(t) ? 0.694 : /^[+−=<>≤≥×±→←∈⋅∗]$/.test(t) ? 0.5 : 0.431;
    return [m, h, /[∫∮∬]/.test(t) ? 0.3 : d];
  };
  /** A glyph at size `s`, stretched `sy`; a script's set wider, as its own font sets it, its height kept. */
  const glyph = (t: string, f: string, s: number, sy = 1): MBox => {
    const [w, h, d] = box(t, f), o = s === SIZE[3] ? P.scriptScript : s === SIZE[2] ? P.script : 1, y = sy / o;
    return { g: [y === 1 ? [t, 0, 0, s * o, f] : [t, 0, 0, s * o, f, y]], r: [], w: w * s * o, h: h * s * sy, d: d * s * sy };
  };
  /** A delimiter at least `size` em tall (of size `s`), centered on the axis; nothing for ''. */
  const delimiter = (c: string, size: number, s: number): MBox => {
    if (!c) return { ...empty(), w: 0.12 * s };
    const [, h, d] = box(c, 'rm'), nat = (h + d) * s, k = Math.max(1, size / nat), wide = 1 + (k - 1) * 0.25;
    const g = glyph(c, 'rm', s * wide, k / wide), out = empty();
    put(out, g, 0, P.axis * s - (g.h - g.d) / 2);
    out.w = g.w;
    return out;
  };
  const hpack = (bs: MBox[]): MBox => { const out = empty(); for (const b of bs) { put(out, b, out.w, 0); out.w += b.w; } return out; };
  /** Noads as a horizontal list in style `st0` (TeX's mlist to hlist). */
  const mlist = (noads: Noad[], st0: Style, top = false): MBox => {
    let st = st0;
    const atoms: { k: string; b: MBox; s: Style }[] = [];
    for (const nd of noads) {
      if (nd.n.t === 'style') { st = { s: nd.n.st, c: st.c }; continue; }
      if (nd.n.t === 'space') { const s = SIZE[st.s]!; atoms.push({ k: 'sp', b: { ...empty(), w: nd.n.mu ? nd.n.w / 18 * s : nd.n.w * s }, s: st }); continue; }
      atoms.push({ k: nd.k, b: noad(nd, st), s: st });
    }
    // a binary operator with no operand on one side is ordinary (a unary minus; tex.web §728)
    const real = atoms.filter((a) => a.k !== 'sp');
    real.forEach((a, j) => { if (a.k === 'bin' && (!j || /^(bin|op|rel|open|punct)$/.test(real[j - 1]!.k) || !real[j + 1] || /^(rel|close|punct)$/.test(real[j + 1]!.k))) a.k = 'ord'; });
    const out = empty(), br: NonNullable<MBox['br']> = [];
    let prev: string | null = null, cut: number | null = null;
    for (const a of atoms) {
      if (a.k !== 'sp') {
        // a break after a binary operator or relation of the outer list, none before a relation (tex.web §767)
        const pen = cut != null && a.k !== 'rel' ? cut : null;
        cut = null;
        if (prev) {
          const mu = spaceMu(prev, a.k, a.s.s > 1), w = mu / 18 * SIZE[a.s.s]!;
          if (top && (w > 0 || pen != null)) br.push({ at: out.w, next: out.w + w, mu: a.s.s > 1 ? 0 : mu, ...(pen != null && { pen }) });
          out.w += w;
        }
      }
      put(out, a.b, out.w, 0);
      out.w += a.b.w;
      if (a.k !== 'sp') prev = a.k;
      if (top && (a.k === 'bin' || a.k === 'rel')) cut = a.k === 'bin' ? P.binPenalty : P.relPenalty;
    }
    if (br.length) out.br = br;
    return out;
  };
  /** A nucleus in style `st`: its box and, for a single character, its italic correction. */
  const nucleus = (n: Field, st: Style, op: boolean): { b: MBox; char: boolean; ic: number } => {
    const s = SIZE[st.s]!;
    switch (n.t) {
      case 'char': {
        if (op) {
          // a large operator: bigger in display, centered on the axis (rule 13)
          const integral = /[∫∮∬∭]/.test(n.c), [, h, d] = box(n.c, n.f), want = (st.s === 0 ? integral ? 2.22 : 1.4 : integral ? 1.11 : 1) * s, k = Math.max(0.9, want / (h + d) / s);
          // a sum or product as wide as TeX's summation sign at least, the glyph centered in it
          const g = glyph(n.c, n.f, s * k), out = empty(), w = /[∑∏∐]/.test(n.c) ? Math.max(g.w, (st.s === 0 ? P.opDisplay : P.opText) * s) : g.w;
          put(out, g, (w - g.w) / 2, P.axis * s - (g.h - g.d) / 2);
          out.w = w;
          // after it is centered it counts as a box, not a character (its scripts drop from its top and bottom)
          return { b: out, char: false, ic: integral ? 0.1 * (out.h + out.d) : 0 };
        }
        // a relation or operator sign as wide as Computer Modern sets it at least (a fallback's `=` is narrower)
        const g = glyph(n.c, n.f, s), sign = /^[=+−<>×±∓≤≥≡∼≈]$/.test(n.c) ? P.sign * s : 0;
        if (sign > g.w) { const out = empty(); put(out, g, (sign - g.w) / 2, 0); out.w = sign; return { b: out, char: true, ic: 0 }; }
        return { b: g, char: true, ic: n.f === 'it' ? P.italic * s : 0 };
      }
      case 'text': return { b: glyph(n.c, n.f, s), char: false, ic: 0 };
      case 'list': return { b: mlist(n.l, st), char: false, ic: 0 };
      case 'big': return { b: delimiter(n.c, n.size ? n.size * s : 1, s), char: false, ic: 0 };
      case 'phantom': { const b = mlist(n.a, st); return { b: { g: [], r: [], w: n.h ? b.w : 0, h: n.v ? b.h : 0, d: n.v ? b.d : 0 }, char: false, ic: 0 }; }
      case 'frac': {
        // rule 15: the parts one style down (display: text style), stacked about the axis with TeX's clearances
        const fs: Style = n.st != null ? { s: n.st, c: st.c } : st, x = mlist(n.a, numer(fs)), z = mlist(n.b, denom(fs)), S = SIZE[fs.s]!;
        const th = n.bar ? P.rule * S : 0, D = fs.s === 0, w = Math.max(x.w, z.w);
        let u = (D ? P.num1 : n.bar ? P.num2 : P.num3) * S, v = (D ? P.denom1 : P.denom2) * S;
        if (!n.bar) {
          const clr = (D ? 7 : 3) * P.rule * S, psi = clr - ((u - x.d) - (z.h - v));
          if (psi > 0) { u += psi / 2; v += psi / 2; }
        } else {
          const clr = (D ? 3 : 1) * th, a = P.axis * S;
          u += Math.max(0, clr - ((u - x.d) - (a + th / 2)));
          v += Math.max(0, clr - ((a - th / 2) - (z.h - v)));
        }
        const out = empty(), pad = n.l || n.r ? 0 : 0.12 * S;
        put(out, x, pad + (w - x.w) / 2, u);
        put(out, z, pad + (w - z.w) / 2, -v);
        if (th) out.r.push([pad, P.axis * S - th / 2, w, th]);
        out.w = w + 2 * pad;
        if (!n.l && !n.r) return { b: out, char: false, ic: 0 };
        const size = (D ? P.delim1 : P.delim2) * S;
        return { b: hpack([delimiter(n.l ?? '', size, S), out, delimiter(n.r ?? '', size, S)]), char: false, ic: 0 };
      }
      case 'sqrt': {
        // rule 11: the body cramped, a rule over it a clearance above, the radical as tall as both
        const x = mlist(n.a, { s: st.s, c: true }), th = P.rule * s, clr = st.s === 0 ? th + P.xHeight * s / 4 : th + th / 4;
        const [, gh, gd] = box('√', 'sy'), need = x.h + x.d + clr + th, k = Math.max(1, need / ((gh + gd) * s));
        const r = glyph('√', 'sy', s, k), out = empty(), top = x.h + clr;
        put(out, r, 0, top + th - r.h);
        put(out, x, r.w, 0);
        out.r.push([r.w, top, x.w, th]);
        out.w = r.w + x.w;
        out.h = Math.max(out.h, top + th + th);
        if (!n.i) return { b: out, char: false, ic: 0 };
        // `\sqrt[n]`: the index in scriptscript, raised 60% of the radical's height, tucked over it (plain's \root)
        const ix = mlist(n.i, { s: 3, c: false }), res = empty();
        put(res, ix, 5 / 18 * s, 0.6 * (out.h - out.d));
        put(res, out, Math.max(0, 5 / 18 * s + ix.w - 10 / 18 * s), 0);
        res.w = Math.max(0, 5 / 18 * s + ix.w - 10 / 18 * s) + out.w;
        return { b: res, char: false, ic: 0 };
      }
      case 'acc': {
        // rule 12: the accent over the nucleus, as high as the nucleus passes the x-height
        const x = mlist(n.a, { s: st.s, c: true }), single = n.a.length === 1 && n.a[0]!.n.t === 'char' && !n.a[0]!.sup && !n.a[0]!.sub;
        // an arrow (`\vec`) scaled down to the nucleus, a wide accent stretched over it
        const arrow = /[→←]/.test(n.c), [aw] = box(n.c, 'rm'), fit = x.w / (aw * s), k = arrow ? Math.max(0.5, Math.min(1, fit)) : 1, sx = n.wide ? Math.max(1, fit * (arrow ? 1 : 0.9)) : 1;
        const a = glyph(n.c, 'rm', s * k * sx, 1 / sx), out = empty(), skew = single && (n.a[0]!.n as { f: string }).f === 'it' ? 0.08 * s : 0;
        put(out, x, 0, 0);
        const y = arrow ? x.h + 2 * P.rule * s - a.d + (a.h + a.d) * 0.05 : Math.max(0, x.h - P.xHeight * s);
        put(out, a, (x.w - a.w) / 2 + skew, y);
        out.w = x.w;
        return { b: out, char: false, ic: 0 };
      }
      case 'line': {
        // rules 9, 10: a rule three rule widths above (below) the box, another width of space past it
        const x = mlist(n.a, { s: st.s, c: n.over || st.c }), th = P.rule * s, out = empty();
        put(out, x, 0, 0);
        if (n.over) { out.r.push([0, x.h + 3 * th, x.w, th]); out.h = x.h + 5 * th; } else { out.r.push([0, -x.d - 4 * th, x.w, th]); out.d = x.d + 5 * th; }
        out.w = x.w;
        return { b: out, char: false, ic: 0 };
      }
      case 'fence': {
        // rule 19: the delimiters as tall as the body reaches from the axis (\delimiterfactor 901, \delimitershortfall 5pt)
        const x = mlist(n.a, st), a = P.axis * s, delta = Math.max(x.h - a, x.d + a), size = Math.max(delta * 2 * 0.901, 2 * delta - 0.5 * s);
        const mid = n.a.some((m) => m.n.t === 'big' && !m.n.size), body = mid ? mlist(n.a.map((m) => m.n.t === 'big' && !m.n.size ? { ...m, n: { ...m.n, size: size / s } } : m), st) : x;
        return { b: hpack([delimiter(n.l, size, s), body, delimiter(n.r, size, s)]), char: false, ic: 0 };
      }
      case 'grid': {
        // an array: cells in its style, columns as wide as their widest cell, rows a baseline skip apart at least,
        // centered on the axis
        const cs: Style = { s: Math.max(st.s, n.st === 0 ? st.s : n.st), c: false }, S = SIZE[cs.s]!;
        const cells = n.rows.map((r) => r.map((c) => mlist(c, n.st === 0 ? { s: 0, c: false } : cs)));
        const cols = Math.max(1, ...cells.map((r) => r.length)), widths = Array.from({ length: cols }, (_, j) => Math.max(0, ...cells.map((r) => r[j]?.w ?? 0)));
        const al = (j: number) => n.align[j % n.align.length] ?? 'c', gap = (j: number) => (n.gap.length > 1 ? n.gap[j % 2]! : n.gap[0]!) * S;
        const strut = n.st === 2 ? [0.6, 0.25] : [0.84, 0.36], jot = n.gap.length > 1 ? 0.3 : 0, out = empty();
        let y = 0;
        cells.forEach((r, ri) => {
          const h = Math.max(strut[0]! * S, ...r.map((c) => c.h)), d = Math.max(strut[1]! * S, ...r.map((c) => c.d));
          if (ri) y -= h + jot * S; else y = 0;
          let x = 0;
          r.forEach((c, j) => { const w = widths[j]!, dx = al(j) === 'l' ? 0 : al(j) === 'r' ? w - c.w : (w - c.w) / 2; put(out, c, x + dx, y); x += w + gap(j); });
          y -= d;
        });
        out.w = widths.reduce((a, w, j) => a + w + (j < cols - 1 ? gap(j) : 0), 0);
        // the whole grid shifted so its middle sits on the axis
        const mid = (out.h - out.d) / 2, shifted = empty();
        put(shifted, out, 0, P.axis * S - mid);
        shifted.w = out.w;
        if (!n.l && !n.r) return { b: shifted, char: false, ic: 0 };
        const size = Math.max(shifted.h - P.axis * S, shifted.d + P.axis * S) * 2;
        return { b: hpack([delimiter(n.l, size, s), { ...empty(), w: n.l ? 0.1 * s : 0 }, shifted, { ...empty(), w: n.r ? 0.1 * s : 0 }, delimiter(n.r, size, s)]), char: false, ic: 0 };
      }
      default: return { b: empty(), char: false, ic: 0 };
    }
  };
  /** A noad in style `st`: its nucleus with its scripts placed (rules 13, 17, 18). */
  const noad = (nd: Noad, st: Style): MBox => {
    const op = nd.k === 'op', { b: x, char, ic } = nucleus(nd.n, st, op);
    if (!nd.sup && !nd.sub) return x;
    const s = SIZE[st.s]!, sup = nd.sup && mlist(fieldList(nd.sup), up(st)), sub = nd.sub && mlist(fieldList(nd.sub), down(st)), out = empty();
    // limits: above and below the operator, centered, in display unless \nolimits (rule 13a)
    if (op && (nd.lim ?? st.s === 0)) {
      const w = Math.max(x.w, sup?.w ?? 0, sub?.w ?? 0);
      put(out, x, (w - x.w) / 2, 0);
      if (sup) put(out, sup, (w - sup.w) / 2 + ic / 2, x.h + Math.max(P.bigOp1 * s, P.bigOp3 * s - sup.d) + sup.d);
      if (sub) put(out, sub, (w - sub.w) / 2 - ic / 2, -(x.d + Math.max(P.bigOp2 * s, P.bigOp4 * s - sub.h) + sub.h));
      if (sup) out.h += P.bigOp5 * s;
      if (sub) out.d += P.bigOp5 * s;
      out.w = w;
      return out;
    }
    // scripts beside: raised and lowered by the font's sup and sub shifts, apart by four rule widths (rule 18)
    const ss = SIZE[up(st).s]!, th = P.rule * s;
    let u = char ? 0 : x.h - P.supDrop * ss, v = char ? 0 : x.d + P.subDrop * ss;
    put(out, x, 0, 0);
    if (!sup) {
      v = Math.max(v, P.sub1 * s, sub!.h - 4 / 5 * P.xHeight * s);
      put(out, sub!, x.w - (op ? ic : 0), -v);
      out.w = x.w - (op ? ic : 0) + sub!.w + P.scriptSpace * s;
      return out;
    }
    u = Math.max(u, (st.s === 0 ? P.sup1 : st.c ? P.sup3 : P.sup2) * s, sup.d + P.xHeight * s / 4);
    if (sub) {
      v = Math.max(v, P.sub2 * s);
      if ((u - sup.d) - (sub.h - v) < 4 * th) {
        v = 4 * th - u + sup.d + sub.h;
        const psi = 4 / 5 * P.xHeight * s - (u - sup.d);
        if (psi > 0) { u += psi; v -= psi; }
      }
    }
    const xs = x.w + (op ? 0 : ic);
    put(out, sup, xs, u);
    if (sub) put(out, sub, x.w - (op ? ic : 0), -v);
    out.w = Math.max(xs + sup.w, sub ? x.w - (op ? ic : 0) + sub.w : 0) + P.scriptSpace * s;
    return out;
  };
  const fieldList = (f: Field): Noad[] => f.t === 'list' ? f.l : [{ k: 'ord', n: f }];
  return mlist(parse(src, display), { s: display ? 0 : 1, c: false }, !display);
}
/** Formula `b` cut at its glue (`br`): each part its glyphs and rules moved to start at 0, its width, and after every
 * part but the last the glue to the next (`space` em, `mu`) and, where the line may break there, its penalty. */
function parts(b: MBox): { box: MBox; pen?: number; space?: number; mu?: number }[] {
  const cuts = b.br ?? [], out: { box: MBox; pen?: number; space?: number; mu?: number }[] = [];
  let from = 0;
  for (let i = 0; i <= cuts.length; i++) {
    const c = cuts[i], to = c ? c.at : b.w, inside = (x: number) => x >= from - 1e-6 && (c ? x < to - 1e-6 : true);
    const g = b.g.filter((q) => inside(q[1])).map((q) => [q[0], q[1] - from, ...q.slice(2)] as Glyph);
    const r = b.r.filter((q) => inside(q[0])).map((q) => [q[0] - from, q[1], q[2], q[3]] as [number, number, number, number]);
    out.push({ box: { g, r, w: to - from, h: b.h, d: b.d }, ...(c && { space: c.next - c.at, mu: c.mu, ...(c.pen != null && { pen: c.pen }) }) });
    if (c) from = c.next;
  }
  return out;
}

/** The Typeset view's math as CSS custom properties from the same parameters: scripts at script size, raised by
 * TeX's text-style sup shift and lowered by its sub shift (in em of the script), the spaces about binary operators,
 * relations and punctuation, a display operator's size. */
function css(params: Partial<Params> = {}): Record<string, string> {
  const P = { ...PARAMS, ...params }, sc = SIZE[2]!, em = (x: number) => `${+x.toFixed(3)}em`;
  return {
    '--m-script': String(sc), '--m-sup': em(P.sup2 / sc), '--m-sub': em(-P.sub1 / sc),
    '--m-bin': em(4 / 18), '--m-rel': em(5 / 18), '--m-punct': em(3 / 18), '--m-op': String(1.4),
    // stacked scripts: the column raised by sup2, its rows sup2 + sub2 apart (in lines of the script's size)
    '--m-sup2': em(P.sup2), '--m-stack': String(+((P.sup2 + P.sub2) / sc).toFixed(3)),
  };
}
/** Commands set from an argument, with the sample a completion item draws them on (`\\frac` as a/b). */
const SAMPLE: [RegExp, string][] = [
  [/^([dt]?frac|cfrac)$/, '{a}{b}'], [/^[dt]?binom$/, '{n}{k}'], [/^sqrt$/, '{x}'], [/^(overline|underline|overbrace|underbrace)$/, '{x}'],
  [/^(mathbf|boldsymbol|bm|mathit|mathrm|mathsf|mathtt)$/, '{x}'], [/^(mathcal|mathscr|mathbb|mathfrak)$/, '{A}'],
];
/** The sample `\\name` is drawn on, null for a command the preview does not know as math. */
const sampleOf = (name: string): string | null => {
  if (sym(name) || OPNAME.test(name)) return `\\${name}`;
  if (ACCENTS[name]) return `\\${name}{a}`;
  const s = SAMPLE.find(([re]) => re.test(name));
  return s ? `\\${name}${s[1]}` : null;
};
/** Whether the preview knows `name` as a math command: a symbol, an operator name, an accent, a fraction, a math alphabet. */
const isMath = (name: string): boolean => sampleOf(name) != null;
/** A rough metric: half an em a character (an operator's glyph wider), ascenders 0.7, descenders 0.2. */
const roughMetric: Metric = (t, face) => [[...t].length * (face === 'ex' ? 0.9 : 0.55), face === 'ex' ? 0.75 : 0.7, face === 'ex' ? 0.25 : 0.2];
const xml = (t: string) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
/** Math command `name` drawn small as an SVG string, in `currentColor` so it follows the theme: `\\sum` as ∑, `\\frac`
 * as a/b. Null for a command the preview does not know as math. */
const preview = (name: string, metric: Metric = roughMetric): string | null => {
  const src = sampleOf(name);
  return src ? svgOf(src, name, metric) : null;
};
/** Math `src` drawn as an SVG string in `currentColor`, labelled `label`. */
function svgOf(src: string, label: string, metric: Metric = roughMetric, display = false): string {
  const b = layout(src, metric, display), pad = 0.08, w = b.w + 2 * pad, h = b.h + b.d + 2 * pad, f = (x: number) => +x.toFixed(3);
  const face = (k: string) => k === 'it' ? ' font-style="italic"' : k === 'bf' ? ' font-weight="bold"' : '';
  const glyphs = b.g.map(([t, x, y, sc, k, sy]) => `<text x="${f(x + pad)}" y="${f((b.h + pad - y) / (sy ?? 1))}" font-size="${f(sc)}"${face(k)}${sy && sy !== 1 ? ` transform="scale(1 ${f(sy)})"` : ''}>${xml(t)}</text>`);
  const rules = b.r.map(([x, y, rw, rh]) => `<rect x="${f(x + pad)}" y="${f(b.h + pad - y - rh)}" width="${f(rw)}" height="${f(rh)}"/>`);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${f(w)} ${f(h)}" width="${f(w)}em" height="${f(h)}em" fill="currentColor" font-family="'Latin Modern Math','STIX Two Math','Cambria Math',serif" role="img" aria-label="${xml(label)}">${glyphs.join('')}${rules.join('')}</svg>`;
}

// A formula built from templates (the Draw dialog's formula bar): its LaTeX `src` and a selection `at`..`to`. A slot
// is an empty group `{}` or an empty matrix cell; a template puts the selection into its first slot (`#`).
type Formula = { src: string; at: number; to: number };
const TEMPLATES: Record<string, string> = {
  frac: '\\frac{#}{}', sup: '^{#}', sub: '_{#}', sqrt: '\\sqrt{#}', sum: '\\sum_{#}^{}', int: '\\int_{#}^{}', paren: '\\left(#\\right)',
  matrix: '\\begin{pmatrix}#&\\\\&\\end{pmatrix}',
};
/** The slots of `src` as caret offsets: inside `{}`, in an empty cell of a matrix. */
const slots = (src: string) => [...src.matchAll(/\{\}|(?<=&|\\\\|\\begin\{pmatrix\})[ ]*(?=&|\\\\|\\end\{pmatrix\})/g)].map((m) => m.index! + (m[0] === '{}' ? 1 : 0));
/** Tab: the next slot after the caret; without one, out of the group the caret is in (past its `}`), else the end. */
const next = (f: Formula): Formula => {
  let at = slots(f.src).find((i) => i > f.at);
  for (let i = f.at, d = 0; at == null && i < f.src.length; i++) {
    if (f.src[i] === '\\') i++;
    else if (f.src[i] === '{') d++;
    else if (f.src[i] === '}' && d-- === 0) at = i + 1;
  }
  at ??= f.src.length;
  return { src: f.src, at, to: at };
};
/** `text` with a blank between a command's name and a letter that would run into it (`\\alpha x`). */
const join = (a: string, b: string) => a + (/\\[A-Za-z]+$/.test(a) && /^[A-Za-z]/.test(b) ? ' ' : '') + b;
/** `text` put in place of the selection, the caret after it. */
const put = (f: Formula, text: string): Formula => {
  const head = join(f.src.slice(0, f.at), text), src = join(head, f.src.slice(f.to));
  return { src, at: head.length, to: head.length };
};
/** Template `kind` around the selection; the caret in its first slot when nothing was selected, else in the next one.
 * A script goes after the selection, which stays its base. */
const wrap = (f: Formula, kind: string): Formula => {
  const t = TEMPLATES[kind]!, sel = f.src.slice(f.at, f.to), script = kind === 'sup' || kind === 'sub';
  const [pre, post] = t.split('#') as [string, string], body = script ? `${sel}${pre}` : `${pre}${sel}`;
  const g = put(f, body + post), at = g.at - post.length, more = slots(g.src).find((i) => i > at);
  // a selection put in: on to the template's next slot, or past the template without one
  const to = !sel || script ? at : more ?? g.at;
  return { src: g.src, at: to, to };
};
/** Clean LaTeX: Unicode symbols as commands, empty scripts dropped, a one-token script without braces (`x^2`,
 * `_\\alpha`), single blanks, trimmed. */
/** The command the preview's table names first for character `c` (`α` → `alpha`), for non-ASCII characters. */
let NAMES: Map<string, string> | null = null;
const nameOf = (c: string) => {
  if (!NAMES) { sym('alpha'); NAMES = new Map(); for (const [n, [ch]] of SYMS!) if (/[^\x00-\x7f]/.test(ch) && !NAMES.has(ch)) NAMES.set(ch, n); }
  return NAMES.get(c);
};
const clean = (src: string): string => {
  // a symbol typed as Unicode (`α`, `≤`) as its command: pdfTeX's math has no Unicode input
  src = src.replace(/([^\x00-\x7f])(?=([A-Za-z])?)/gu, (c, _, after) => { const n = nameOf(c); return n ? `\\${n}${after ? ' ' : ''}` : c; });
  // an empty script goes, nested ones too (`^{_{}}`); a command's name before it keeps its blank to a letter after
  let s = src, was;
  do { was = s; s = s.replace(/[\^_]\{\s*\0*\s*\}/g, '\0'); } while (s !== was);
  s = s.replace(/(\\[A-Za-z]+)\0+(?=[A-Za-z])/g, '$1 ').replace(/\0/g, '');
  s = s.replace(/([\^_])\{\s*(\\[A-Za-z]+|[^{}\\\s])\s*\}([A-Za-z]?)/g, (_, op, tok, after) => op + tok + (after && /[A-Za-z]$/.test(tok) ? ' ' : '') + after);
  // a matrix: one blank around its cells' `&` and `\\`, and inside its begin and end
  s = s.replace(/[ \t]+/g, ' ').replace(/ ?((?<!\\)&|\\\\) ?/g, ' $1 ').replace(/(\\begin\{\w*matrix\}) ?/g, '$1 ').replace(/ ?(\\end\{\w*matrix\})/g, ' $1');
  return s.replace(/ {2,}/g, ' ').trim();
};
/** The formula drawn: empty slots as boxes. */
const formulaSvg = (src: string, metric: Metric = roughMetric) =>
  svgOf(src.replace(/\{\}/g, '{\\Box}').replace(/(&|\\\\|\\begin\{pmatrix\})[ ]*(?=&|\\\\|\\end\{pmatrix\})/g, '$1\\Box') || '\\Box', clean(src), metric);
self.satexMath = { layout, parts, parse, css, PARAMS, spaceMu, isMath, preview, formula: { TEMPLATES, slots, next, put, wrap, clean, svg: formulaSvg } };
// the Typeset view's math reads them (style.css falls back to the same values where this script is not loaded)
if (self.document) for (const [k, v] of Object.entries(css())) document.documentElement.style.setProperty(k, v);
