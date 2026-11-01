// Benchmark for the /draw recogniser (doc/site/draw.js).
// Run:  node doc/site/draw-bench.mjs   (from the repo root; site served on 127.0.0.1:8000)
// Needs playwright-core (PW=dir with node_modules) and a Chromium (CHROME=path).
// Env: ONLY=a,b  run these shapes | TOP=n print top n rows | SWEEP=file.json {label: DRAW overrides}
//      PKGS=all every package on | PORT=8000 | JS=path of draw.js to inject (default doc/site/draw.js) | CSS=style.css to serve | OUT=results json
// Output: one line per shape, a summary (top1/3/5/10, median rank, per category),
//         and target/scratch/draw-bench.json for comparing runs.
import fs from 'fs';
const HOME = process.env.HOME;
const { chromium } = await import(
  (process.env.PW ?? HOME + '/.local/share/pw/node_modules') + '/playwright-core/index.mjs'
);
const only = process.env.ONLY, outFile = process.env.OUT ?? 'target/scratch/draw-bench.json';
const js = fs.readFileSync(process.env.JS ?? 'doc/site/draw.js', 'utf8');
const css = fs.readFileSync(process.env.CSS ?? 'doc/site/style.css', 'utf8') +
  (fs.existsSync('target/scratch/draw-extra.css')
    ? fs.readFileSync('target/scratch/draw-extra.css', 'utf8')
    : '');
const chrome = process.env.CHROME ?? HOME + '/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome';

// ---- stroke helpers (coordinates in [-1,1]^2, y down)
const line = (a, c, n = 14) =>
  Array.from({ length: n + 1 }, (_, i) => [a[0] + (c[0] - a[0]) * i / n, a[1] + (c[1] - a[1]) * i / n]);
const path = (ps) => ps.slice(1).flatMap((p, i) => line(ps[i], p, 8));
const ring = (f, n = 48) => Array.from({ length: n + 1 }, (_, i) => f(i / n * 2 * Math.PI));
const spline = (P, n = 10) => {
  const out = [];
  for (let i = 0; i < P.length - 1; i++) {
    const p0 = P[Math.max(i - 1, 0)], p1 = P[i], p2 = P[i + 1], p3 = P[Math.min(i + 2, P.length - 1)];
    for (let k = 0; k < n; k++) {
      const t = k / n, t2 = t * t, t3 = t2 * t;
      out.push(
        [0, 1].map((d) =>
          0.5 *
          (2 * p1[d] + (-p0[d] + p2[d]) * t + (2 * p0[d] - 5 * p1[d] + 4 * p2[d] - p3[d]) * t2 +
            (-p0[d] + 3 * p1[d] - 3 * p2[d] + p3[d]) * t3)
        ),
      );
    }
  }
  out.push(P.at(-1));
  return out;
};
const blob = (
  cx,
  cy,
) => [
  ring((t) => [cx + .1 * Math.cos(t), cy + .1 * Math.sin(t)]),
  ring((t) => [cx + .05 * Math.cos(t), cy + .05 * Math.sin(t)]),
];
const dot = (
  cx,
  cy,
) => [
  ring((t) => [cx + .08 * Math.cos(t), cy + .08 * Math.sin(t)]),
  ring((t) => [cx + .04 * Math.cos(t), cy + .04 * Math.sin(t)]),
];
const ell = (cx, cy, rx, ry, a = 0) =>
  ring((
    t,
  ) => [
    cx + rx * Math.cos(t) * Math.cos(a) - ry * Math.sin(t) * Math.sin(a),
    cy + rx * Math.cos(t) * Math.sin(a) + ry * Math.sin(t) * Math.cos(a),
  ]);
const arc = (cx, cy, rx, ry, a0, a1, n = 24) =>
  Array.from({ length: n + 1 }, (_, i) => {
    const t = a0 + (a1 - a0) * i / n;
    return [cx + rx * Math.cos(t), cy + ry * Math.sin(t)];
  });
const PI = Math.PI;
// arrow head at tip, pointing along direction d (unit), size s
const head = (tip, d, s = .45) => {
  const n = [-d[1], d[0]];
  return path([[tip[0] - d[0] * s + n[0] * s, tip[1] - d[1] * s + n[1] * s], tip, [
    tip[0] - d[0] * s - n[0] * s,
    tip[1] - d[1] * s - n[1] * s,
  ]]);
};
const arrow = (a, b, s = .45) => {
  const l = Math.hypot(b[0] - a[0], b[1] - a[1]), d = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
  return [line(a, b), head(b, d, s)];
};
const R = (names, extra = '') => new RegExp('\\\\(' + names + ')(?![a-zA-Z])' + (extra ? '|' + extra : ''));

// ---- shapes: [category, name, strokes, regex, target rank]
const G = (name, re, strokes) => ['greek', name, strokes, R(re), 10];
const A = (name, re, strokes, w = 10) => ['arrow', name, strokes, R(re), w];
const L = (name, re, strokes, w = 10) => ['relation', name, strokes, R(re), w];
const O = (name, re, strokes, w = 10) => ['operator', name, strokes, R(re), w];
const S = (name, re, strokes, w = 10) => ['set-logic', name, strokes, R(re), w];
const B = (name, re, strokes, w = 10) => ['bracket', name, strokes, R(re), w];
const T = (name, re, strokes, w = 10) => ['letterlike', name, strokes, R(re), w];
const M = (name, re, strokes, ex = '') => ['misc', name, strokes, R(re, ex), 10];
const I = (name, re, strokes, w = 10) => ['icon', name, strokes, R(re), w];
const eqlines = (n, y0 = -.4, dy = .8, x = .9) =>
  Array.from(
    { length: n },
    (_, i) => line([-x, y0 + dy * i / Math.max(n - 1, 1)], [x, y0 + dy * i / Math.max(n - 1, 1)]),
  );
const shapes = [
  // Greek lower
  G('alpha', 'alpha', [
    spline([[0.9, 0.7], [0.4, 0.2], [-0.2, -0.5], [-0.8, -0.1], [-0.4, 0.7], [0.2, 0.3], [0.6, -0.4], [
      0.85,
      -0.5,
    ]]),
  ]),
  G('beta', 'beta', [
    spline([
      [-0.5, 1],
      [-0.5, -0.4],
      [-0.2, -0.95],
      [0.3, -0.85],
      [0.35, -0.4],
      [-0.3, -0.1],
      [0.5, 0.2],
      [0.6, 0.7],
      [0.1, 1],
      [-0.5, 0.7],
    ]),
  ]),
  G('gamma', '(var)?gamma', [
    spline([[-0.8, -0.5], [-0.2, -0.3], [0.1, 0.1], [0.1, 0.9], [0.1, 0.1], [0.5, -0.3], [0.9, -0.55]]),
  ]),
  G('delta', 'delta', [
    spline([[0.2, -0.95], [-0.2, -0.8], [0.3, -0.5], [0.6, 0], [0.4, 0.8], [-0.2, 0.9], [-0.6, 0.4], [
      -0.3,
      -0.2,
    ], [0.3, -0.5]]),
  ]),
  G('epsilon', '(var)?epsilon', [
    spline([[0.6, -0.6], [0.1, -0.8], [-0.4, -0.5], [-0.1, -0.05], [-0.5, 0.4], [-0.1, 0.85], [0.6, 0.7]]),
  ]),
  G('zeta', 'zeta', [
    spline([[-0.5, -0.95], [0.5, -0.9], [-0.1, -0.4], [-0.5, 0.1], [-0.2, 0.6], [0.4, 0.8], [0.3, 1.0], [
      0,
      0.95,
    ]]),
  ]),
  G('eta', 'eta', [
    spline([[-0.7, -0.4], [-0.55, 0.0], [-0.5, 0.5], [-0.5, 0.1], [-0.2, -0.45], [0.3, -0.4], [0.35, 0.2], [
      0.35,
      1.0,
    ]]),
  ]),
  G('theta', '(var)?theta', [ell(0, 0, .6, .95), line([-.45, 0], [.45, 0])]),
  G('iota', 'iota', [spline([[-0.1, -0.7], [-0.1, 0.2], [0.1, 0.7], [0.5, 0.75]])]),
  G('kappa', '(var)?kappa', [line([-.5, -.8], [-.5, .8]), path([[.6, -.7], [-.5, .1], [.6, .85]])]),
  G('lambda', 'lambda', [line([-.6, -.9], [.7, .9]), line([-.7, .9], [.05, -.1])]),
  G('mu', 'mu', [
    line([-.6, -.6], [-.6, 1]),
    spline([[-.6, .3], [-.5, .8], [0, .8], [.3, .3], [.35, -.6], [.35, .3], [.45, .8], [.8, .6]]),
  ]),
  G('nu', 'nu', [spline([[-.7, -.6], [-.5, .2], [-.2, .8], [.1, .3], [.45, -.3], [.7, -.65]])]),
  G('xi', 'xi', [
    spline([[.5, -.95], [-.4, -.95], [-.4, -.4], [.3, -.1], [-.4, .1], [-.5, .6], [.3, .75], [.45, 1]]),
  ]),
  G('pi', '(var)?pi', [
    line([-.8, -.55], [.8, -.55]),
    line([-.4, -.55], [-.45, .85]),
    line([.35, -.55], [.35, .85]),
  ]),
  G('rho', '(var)?rho', [
    spline([[-0.2, 1], [-0.25, 0.2], [-0.3, -0.4], [0.1, -0.85], [0.6, -0.7], [0.55, -0.3], [0.1, -0.15], [
      -0.3,
      -0.4,
    ]]),
  ]),
  G('sigma', '(var)?sigma', [
    spline([[.8, -.55], [.1, -.55], [-.5, -.2], [-.45, .5], [.1, .8], [.6, .4], [.45, -.2], [.1, -.15]]),
  ]),
  G('tau', 'tau', [line([-.7, -.6], [.7, -.6]), spline([[0, -.6], [0, .3], [.2, .8], [.55, .85]])]),
  G('upsilon', 'upsilon', [spline([[-.6, -.6], [-.5, .3], [-.1, .75], [.4, .4], [.6, -.6]])]),
  G('phi', '(var)?phi', [ell(0, 0, .55, .5), line([0, -1], [0, 1])]),
  G('chi', 'chi', [line([-.7, -.7], [.7, .8]), spline([[.7, -.7], [.1, 0], [-.3, .5], [-.75, .95]])]),
  G('psi', 'psi', [spline([[-.7, -.6], [-.6, .2], [0, .6], [.6, .2], [.7, -.6]]), line([0, -.95], [0, .95])]),
  G('omega', '(var)?omega', [
    spline([[-.9, -.4], [-.8, .4], [-.4, .7], [0, .1], [.4, .7], [.8, .4], [.9, -.4]]),
  ]),
  // Greek upper
  G('Gamma', 'Gamma', [path([[.6, -.9], [-.5, -.9], [-.5, .95]])]),
  G('Delta', 'Delta', [path([[0, -.95], [-.85, .85], [.85, .85], [0, -.95]])]),
  G('Theta', 'Theta', [ell(0, 0, .65, .95), line([-.4, 0], [.4, 0])]),
  G('Lambda', 'Lambda', [path([[-.85, .9], [0, -.95], [.85, .9]])]),
  G('Xi', 'Xi', [line([-.8, -.9], [.8, -.9]), line([-.4, 0], [.4, 0]), line([-.8, .9], [.8, .9])]),
  G('Pi', 'Pi', [
    line([-.8, -.85], [.8, -.85]),
    line([-.55, -.85], [-.55, .9]),
    line([.55, -.85], [.55, .9]),
  ]),
  G('Sigma', 'Sigma', [path([[.7, -.95], [-.7, -.95], [.1, 0], [-.7, .95], [.7, .95]])]),
  G('Phi', 'Phi', [ell(0, 0, .6, .5), line([0, -.95], [0, .95])]),
  G('Psi', 'Psi', [spline([[-.7, -.7], [-.6, .3], [0, .6], [.6, .3], [.7, -.7]]), line([0, -.95], [0, .95])]),
  G('Omega', 'Omega', [
    path([[-.9, .9], [-.4, .9], [-.7, .3], [-.65, -.4], [0, -.9], [.65, -.4], [.7, .3], [.4, .9], [.9, .9]]),
  ]),
  // Arrows
  A('rightarrow', 'rightarrow|to|longrightarrow', arrow([-1, 0], [1, 0])),
  A('leftarrow', 'leftarrow|gets|longleftarrow', arrow([1, 0], [-1, 0])),
  A('uparrow', 'uparrow', arrow([0, 1], [0, -1])),
  A('downarrow', 'downarrow', arrow([0, -1], [0, 1])),
  A('leftrightarrow', 'leftrightarrow', [
    line([-.9, 0], [.9, 0]),
    head([-1, 0], [-1, 0], .4),
    head([1, 0], [1, 0], .4),
  ]),
  A('updownarrow', 'updownarrow', [
    line([0, -.9], [0, .9]),
    head([0, -1], [0, -1], .4),
    head([0, 1], [0, 1], .4),
  ]),
  A('nearrow', 'nearrow', arrow([-.8, .8], [.85, -.85])),
  A('searrow', 'searrow', arrow([-.8, -.8], [.85, .85])),
  A('nwarrow', 'nwarrow', arrow([.8, .8], [-.85, -.85])),
  A('swarrow', 'swarrow', arrow([.8, -.8], [-.85, .85])),
  A('Rightarrow', 'Rightarrow|implies', [
    line([-1, -0.3], [0.6, -0.3]),
    line([-1, 0.3], [0.6, 0.3]),
    path([[0.3, -0.7], [1, 0], [0.3, 0.7]]),
  ]),
  A('Leftarrow', 'Leftarrow|impliedby', [
    line([1, -0.3], [-0.6, -0.3]),
    line([1, 0.3], [-0.6, 0.3]),
    path([[-0.3, -0.7], [-1, 0], [-0.3, 0.7]]),
  ]),
  A('Leftrightarrow', 'Leftrightarrow|iff', [
    line([-.6, -.3], [.6, -.3]),
    line([-.6, .3], [.6, .3]),
    path([[-.3, -.7], [-1, 0], [-.3, .7]]),
    path([[.3, -.7], [1, 0], [.3, .7]]),
  ]),
  A('Uparrow', 'Uparrow', [
    line([-.3, 1], [-.3, -.6]),
    line([.3, 1], [.3, -.6]),
    path([[-.7, -.3], [0, -1], [.7, -.3]]),
  ]),
  A('Downarrow', 'Downarrow', [
    line([-.3, -1], [-.3, .6]),
    line([.3, -1], [.3, .6]),
    path([[-.7, .3], [0, 1], [.7, .3]]),
  ]),
  A('hookrightarrow', 'hookrightarrow', [
    spline([[-1, .3], [-.9, -.2], [-.7, .2], [-.6, 0]]).concat(line([-.6, 0], [.9, 0])),
    head([1, 0], [1, 0], .4),
  ]),
  A('hookleftarrow', 'hookleftarrow', [
    spline([[1, .3], [.9, -.2], [.7, .2], [.6, 0]]).concat(line([.6, 0], [-.9, 0])),
    head([-1, 0], [-1, 0], .4),
  ]),
  A('mapsto', 'mapsto', [line([-.9, -.4], [-.9, .4]), line([-.9, 0], [.9, 0]), head([1, 0], [1, 0], .4)]),
  A('longrightarrow', 'longrightarrow', arrow([-1, 0], [1, 0], .3)),
  A('longmapsto', 'longmapsto', [
    line([-1, -.35], [-1, .35]),
    line([-1, 0], [1, 0]),
    head([1, 0], [1, 0], .3),
  ]),
  A('Longrightarrow', 'Longrightarrow', [
    line([-1, -.25], [.7, -.25]),
    line([-1, .25], [.7, .25]),
    path([[.45, -.6], [1, 0], [.45, .6]]),
  ]),
  A('rightsquigarrow', 'rightsquigarrow|leadsto', [
    spline([[-1, .2], [-.6, -.3], [-.2, .3], [.2, -.3], [.6, .1], [.9, 0]]),
    head([1, 0], [1, 0], .3),
  ], 5),
  A('rightleftharpoons', 'rightleftharpoons', [
    line([-.9, -.3], [.9, -.3]),
    path([[.5, -.65], [.9, -.3]]),
    line([.9, .3], [-.9, .3]),
    path([[-.5, .65], [-.9, .3]]),
  ]),
  A('curvearrow', 'curvearrowright|circlearrowright|looparrowright', [
    arc(0, .2, .8, .7, PI * 1.05, PI * 1.95),
    head([.8, .2], [0, 1], .35),
  ], 5),
  // Relations
  L('equal', 'eq|equal', eqlines(2, -.3, .6)),
  L('neq', 'neq|ne', [...eqlines(2, -.3, .6), line([.5, -.9], [-.5, .9])]),
  L('leq', 'leq|le', [path([[0.8, -0.9], [-0.8, -0.35], [0.8, 0.2]]), line([-0.8, 0.65], [0.8, 0.65])]),
  L('geq', 'geq|ge', [path([[-0.8, -0.9], [0.8, -0.35], [-0.8, 0.2]]), line([-0.8, 0.65], [0.8, 0.65])]),
  L('less', 'less|lt', [path([[0.8, -0.8], [-0.8, 0], [0.8, 0.8]])]),
  L('greater', 'greater|gt', [path([[-0.8, -0.8], [0.8, 0], [-0.8, 0.8]])]),
  L('approx', 'approx', [
    spline([[-.9, -.5], [-.45, -.8], [0, -.45], [.45, -.2], [.9, -.5]]),
    spline([[-.9, .3], [-.45, 0], [0, .35], [.45, .6], [.9, .3]]),
  ]),
  L('equiv', 'equiv', eqlines(3, -.55, 1.1)),
  L('sim', 'sim', [spline([[-.9, .15], [-.45, -.4], [0, 0], [.45, .4], [.9, -.15]])]),
  L('simeq', 'simeq', [
    spline([[-.9, -.25], [-.45, -.7], [0, -.3], [.45, .05], [.9, -.35]]),
    line([-.85, .55], [.9, .55]),
  ]),
  L('subset', 'subset', [spline([[.8, -.8], [-.3, -.8], [-.8, -.4], [-.8, .4], [-.3, .8], [.8, .8]])]),
  L('subseteq', 'subseteq', [
    spline([[.8, -.8], [-.3, -.8], [-.8, -.4], [-.8, .1], [-.3, .5], [.8, .5]]),
    line([-.7, .95], [.8, .95]),
  ]),
  L('supset', 'supset', [spline([[-.8, -.8], [.3, -.8], [.8, -.4], [.8, .4], [.3, .8], [-.8, .8]])]),
  L('in', 'in', [
    spline([[.8, -.8], [-.3, -.8], [-.8, -.4], [-.8, .4], [-.3, .8], [.8, .8]]),
    line([-.8, 0], [.8, 0]),
  ]),
  L('notin', 'notin', [
    spline([[.7, -.8], [-.3, -.8], [-.7, -.4], [-.7, .4], [-.3, .8], [.7, .8]]),
    line([-.7, 0], [.7, 0]),
    line([.5, -1], [-.3, 1]),
  ]),
  L(
    'propto',
    'propto|varpropto',
    [spline([[.9, -.5], [.5, -.6], [.2, -.2], [-.2, .5], [-.6, .6], [-.9, .3], [-.5, -.1], [-.15, .1], [
      .3,
      .6,
    ], [.9, .5]])].map((s) => s),
  ),
  L('perp', 'perp|bot', [line([-.9, .8], [.9, .8]), line([0, .8], [0, -.9])]),
  L('parallel', 'parallel|Vert|\\|', [line([-.3, -.9], [-.3, .9]), line([.3, -.9], [.3, .9])]),
  L('mid', 'mid|vert', [line([0, -1], [.02, 1])], 5),
  // Operators
  O('sum', 'sum', [path([[0.7, -1], [-0.7, -1], [0.15, 0], [-0.7, 1], [0.7, 1]])]),
  O('prod', 'prod', [
    line([-.8, -.85], [.8, -.85]),
    line([-.5, -.85], [-.5, .9]),
    line([.5, -.85], [.5, .9]),
  ]),
  O('int', 'int', [
    path([[0.45, -0.8], [0.3, -1], [0.12, -0.95], [0.04, -0.7], [0, 0], [-0.04, 0.7], [-0.12, 0.95], [
      -0.3,
      1,
    ], [-0.45, 0.8]]),
  ]),
  O('oint', 'oint', [
    path([[0.45, -0.8], [0.3, -1], [0.12, -0.95], [0.04, -0.7], [0, 0], [-0.04, 0.7], [-0.12, 0.95], [
      -0.3,
      1,
    ], [-0.45, 0.8]]),
    ell(0, 0, .5, .3),
  ]),
  O('partial', 'partial', [
    spline([
      [0.5, -0.2],
      [0.1, -0.1],
      [-0.45, 0.2],
      [-0.4, 0.8],
      [0.1, 0.9],
      [0.55, 0.4],
      [0.45, -0.4],
      [0.1, -0.9],
      [-0.3, -0.9],
      [-0.5, -0.6],
    ]),
  ]),
  O('nabla', 'nabla', [path([[-1, -0.8], [1, -0.8], [0, 0.9], [-1, -0.8]])]),
  O('pm', 'pm', [line([-.8, -.1], [.8, -.1]), line([0, -.8], [0, .6]), line([-.8, .9], [.8, .9])]),
  O('mp', 'mp', [line([-.8, .1], [.8, .1]), line([0, -.6], [0, .8]), line([-.8, -.9], [.8, -.9])]),
  O('times', 'times', [line([-1, -1], [1, 1]), line([1, -1], [-1, 1])]),
  O('div', 'div', [line([-.9, 0], [.9, 0]), ...dot(0, -.65), ...dot(0, .65)]),
  O('oplus', 'oplus|bigoplus', [ell(0, 0, 1, 1), line([-1, 0], [1, 0]), line([0, -1], [0, 1])]),
  O('otimes', 'otimes|bigotimes', [ell(0, 0, 1, 1), line([-.7, -.7], [.7, .7]), line([.7, -.7], [-.7, .7])]),
  O('circ', 'circ|bigcirc|mdlgwhtcircle|mdwhtcircle|lgwhtcircle', [ring((t) => [Math.cos(t), Math.sin(t)])]),
  O('bullet', 'bullet|cdot|mdlgblkcircle|bigbullet|filledbigcirc', [
    ring((t) => [.4 * Math.cos(t), .4 * Math.sin(t)]),
    ring((t) => [.2 * Math.cos(t), .2 * Math.sin(t)]),
  ], 5),
  O('sqrt', 'sqrt|surd', [path([[-1, .1], [-.7, -.05], [-.35, .9], [.1, -.9], [1, -.9]])]),
  O('ast', 'ast|star|bigstar', [
    line([0, -.9], [0, .9]),
    line([-.8, -.45], [.8, .45]),
    line([-.8, .45], [.8, -.45]),
  ], 5),
  // Sets & logic
  S('emptyset', 'varnothing|emptyset|O|o', [
    ring((t) => [Math.cos(t), Math.sin(t)]),
    line([-1.15, 1.15], [1.15, -1.15]),
  ]),
  S('infty', 'infty', [
    ring((
      t,
    ) => [Math.cos(t) / (1 + Math.sin(t) ** 2), 1.5 * Math.sin(t) * Math.cos(t) / (1 + Math.sin(t) ** 2)]),
  ]),
  S('forall', 'forall', [path([[-.8, -.9], [0, .9], [.8, -.9]]), line([-.45, -.1], [.45, -.1])]),
  S('exists', 'exists', [path([[-.6, -.9], [.6, -.9], [.6, .9], [-.6, .9]]), line([-.5, 0], [.6, 0])]),
  S('neg', 'neg|lnot', [path([[-.9, -.2], [.9, -.2], [.9, .5]])]),
  S('wedge', 'wedge|land', [path([[-.8, .85], [0, -.85], [.8, .85]])]),
  S('vee', 'vee|lor', [path([[-.8, -.85], [0, .85], [.8, -.85]])]),
  S('cap', 'cap|bigcap', [spline([[-.7, .9], [-.7, -.2], [-.3, -.8], [.3, -.8], [.7, -.2], [.7, .9]])]),
  S('cup', 'cup|bigcup', [spline([[-.7, -.9], [-.7, .2], [-.3, .8], [.3, .8], [.7, .2], [.7, -.9]])]),
  S('mathbbR', 'BbbR|mathbbR|mathbb\\{R\\}', [
    line([-.6, -.9], [-.6, .9]),
    spline([[-.6, -.9], [.3, -.95], [.6, -.5], [.3, -.05], [-.6, 0]]),
    line([-.1, 0], [.6, .9]),
    line([-.4, -.9], [-.4, .9]),
  ], 5),
  // Brackets
  B('langle', 'langle', [path([[.6, -.95], [-.5, 0], [.6, .95]])]),
  B('rangle', 'rangle', [path([[-.6, -.95], [.5, 0], [-.6, .95]])]),
  B('lceil', 'lceil', [path([[.5, -.95], [-.4, -.95], [-.4, .95]])]),
  B('rceil', 'rceil', [path([[-.5, -.95], [.4, -.95], [.4, .95]])]),
  B('lfloor', 'lfloor', [path([[-.4, -.95], [-.4, .95], [.5, .95]])]),
  B('rfloor', 'rfloor', [path([[.4, -.95], [.4, .95], [-.5, .95]])]),
  B('vert', 'vert|mid|lvert|rvert|\\|', [line([0, -1], [.03, 1])], 5),
  B('Vert', 'Vert|\\|', [line([-.25, -1], [-.22, 1]), line([.25, -1], [.28, 1])]),
  B('lbrace', 'lbrace|\\{', [
    spline([[.5, -1], [.0, -.9], [-.05, -.4], [-.1, 0], [-.6, 0], [-.1, 0], [-.05, .4], [.0, .9], [.5, 1]]),
  ]),
  B('lbrack', 'lbrack|\\[', [path([[.5, -.95], [-.4, -.95], [-.4, .95], [.5, .95]])], 5),
  // Letter-like
  T('ell', 'ell', [
    spline([[.4, .8], [0, .9], [-.4, .5], [-.1, -.5], [.15, -.95], [.15, -.5], [-.3, .3], [-.2, .8]]),
  ]),
  T('hbar', 'hbar|hslash', [
    spline([[-.4, -.95], [-.5, .9]]),
    spline([[-.5, .1], [-.2, -.2], [.3, -.1], [.45, .9]]),
    line([-.8, -.45], [.2, -.45]),
  ]),
  T('aleph', 'aleph', [
    line([.6, -.85], [-.6, .85]),
    spline([[-.7, -.8], [-.5, -.1], [-.1, 0]]),
    spline([[.7, .8], [.5, .1], [.1, 0]]),
  ]),
  T('Re', 'Re', [
    line([-.5, -.9], [-.5, .9]),
    spline([[-.5, -.9], [.4, -.95], [.6, -.4], [.3, 0], [-.5, 0]]),
    line([0, 0], [.6, .9]),
    spline([[-.7, .2], [-.8, .9], [-.3, .5]]),
  ]),
  T('Im', 'Im', [
    line([-.2, -.9], [-.2, .9]),
    spline([[.1, -.9], [.7, -.9], [.7, .1], [.2, .1]]),
    spline([[.2, .1], [.8, .1], [.8, .9], [.1, .9]]),
  ]),
  T('wp', 'wp', [
    spline([[-.7, .9], [-.6, .2], [-.5, -.5], [0, -.8], [.6, -.5], [.4, 0], [-.3, .1], [-.5, -.3]]),
  ]),
  // Misc text
  M('section', 'S|textsection', [
    spline([[.4, -.7], [0, -.95], [-.4, -.65], [.4, -.1], [.5, .35], [-.1, .95], [-.5, .65]]),
  ], '^§'),
  M('pilcrow', 'P|textparagraph', [
    line([.25, -.95], [.25, .9]),
    line([-.15, -.95], [-.15, .9]),
    spline([[-.15, -.95], [.6, -.95], [.7, -.55], [.55, -.15], [-.15, -.1]]),
  ], '^¶'),
  M('dagger', 'dagger', [line([0, -1], [0, 1]), line([-.55, -.4], [.55, -.4])]),
  M('ddagger', 'ddagger', [
    line([0, -1], [0, 1]),
    line([-.55, -.5], [.55, -.5]),
    line([-.55, .4], [.55, .4]),
  ]),
  M('copyright', 'copyright|textcopyright', [
    ell(0, 0, 1, 1),
    spline([[.4, -.35], [0, -.55], [-.4, -.2], [-.4, .2], [0, .55], [.4, .35]]),
  ]),
  M('registered', 'textregistered|circledR', [
    ell(0, 0, 1, 1),
    line([-.25, -.5], [-.25, .5]),
    spline([[-.25, -.5], [.35, -.5], [.35, 0], [-.25, 0]]),
    line([0, 0], [.35, .5]),
  ]),
  M('trademark', 'texttrademark|textTM', [
    line([-.9, -.4], [-.1, -.4]),
    line([-.5, -.4], [-.5, .5]),
    path([[.1, .5], [.1, -.4], [.45, .1], [.8, -.4], [.8, .5]]),
  ]),
  M('euro', 'euro|texteuro|EUR', [
    spline([[.6, -.7], [.1, -.95], [-.5, -.5], [-.5, .4], [.1, .9], [.6, .65]]),
    line([-.85, -.2], [.4, -.2]),
    line([-.85, .2], [.4, .2]),
  ]),
  M('pounds', 'pounds|textsterling', [
    spline([[.5, -.7], [.1, -.95], [-.2, -.6], [-.2, 0], [-.3, .5], [-.6, .9]]),
    line([-.6, .9], [.6, .9]),
    line([-.55, -.1], [.2, -.1]),
  ]),
  M(
    'percent',
    '%|percent',
    [ell(-.55, -.55, .25, .25), ell(.55, .55, .25, .25), line([.8, -.9], [-.8, .9])],
    '^%',
  ),
  M('amp', '&', [
    spline([
      [0.9, 0.95],
      [0.1, 0.05],
      [-0.35, -0.5],
      [-0.1, -0.95],
      [0.25, -0.6],
      [-0.1, 0.1],
      [-0.6, 0.5],
      [-0.4, 0.95],
      [0.2, 0.8],
      [0.6, 0.2],
    ]),
  ], '^&'),
  M('question', 'mathquestion|faQuestion|textquestiondown|textinterrobang', [
    spline([[-0.45, -0.6], [-0.2, -0.95], [0.3, -0.9], [0.5, -0.5], [0.1, -0.1], [0, 0.25]]),
    ...dot(0, 0.85),
  ], '^\\?'),
  M('bang', '!|faExclamation', [line([0, -1], [0, 0.35]), ...dot(0, 0.85)], '^!'),
  M('hash', 'hash|#|sharp|faHashtag', [
    line([-.3, -.9], [-.4, .9]),
    line([.4, -.9], [.3, .9]),
    line([-.9, -.3], [.9, -.35]),
    line([-.9, .35], [.9, .3]),
  ], '^#'),
  // Icons
  I('skull', 'faSkullCrossbones|faSkull|skull', [
    path([[-.6, -.9], [.6, -.9], [.8, -.5], [.6, .0], [-.6, .0], [-.8, -.5], [-.6, -.9]]),
    ...blob(-.3, -.5),
    ...blob(.3, -.5),
    line([-.8, .2], [.8, 1]),
    line([.8, .2], [-.8, 1]),
  ]),
  I('heart', 'faHeart|heartsuit|heart|Heart', [
    spline([[0, .9], [-.8, .1], [-.9, -.5], [-.5, -.85], [0, -.4], [.5, -.85], [.9, -.5], [.8, .1], [0, .9]]),
  ]),
  I('star', 'faStar|bigstar|star', [
    path([
      [0, -1],
      [.3, -.3],
      [.95, -.3],
      [.45, .15],
      [.6, .85],
      [0, .45],
      [-.6, .85],
      [-.45, .15],
      [-.95, -.3],
      [-.3, -.3],
      [0, -1],
    ]),
  ]),
  I('check', 'faCheck|checkmark|ding\\{51\\}|Checkmark', [path([[-.8, 0], [-.3, .7], [.9, -.8]])]),
  I('cross', 'faTimes|faXmark|faXmark|ding\\{55\\}|XSolid|xmark', [
    line([-.8, -.8], [.8, .8]),
    line([.8, -.8], [-.8, .8]),
  ]),
  I('smiley', 'faSmile|smiley|Smiley|faFaceSmile|faGrin', [
    ell(0, 0, 1, 1),
    ...blob(-.35, -.3),
    ...blob(.35, -.3),
    arc(0, .05, .6, .55, .25, PI - .25),
  ]),
  I('sun', 'faSun|sun|Sun|Sunny', [
    ell(0, 0, .4, .4),
    ...[...Array(8)].map((_, i) => {
      const a = i * PI / 4;
      return line([.6 * Math.cos(a), .6 * Math.sin(a)], [Math.cos(a), Math.sin(a)], 5);
    }),
  ]),
  I('moon', 'faMoon|Moon|moon|Crescent', [
    spline([[.2, -.95], [-.6, -.6], [-.8, .2], [-.4, .8], [.3, .95], [-.3, .5], [-.4, 0], [-.2, -.5], [
      .2,
      -.95,
    ]]),
  ]),
  I('house', 'faHome|faHouse|house|Home', [
    path([[-1, -.1], [0, -.95], [1, -.1]]),
    path([[-.75, -.1], [-.75, .9], [.75, .9], [.75, -.1]]),
    path([[-.2, .9], [-.2, .3], [.2, .3], [.2, .9]]),
  ]),
  I('envelope', 'faEnvelope|envelope|Letter|Mail|faEnvelopeOpen', [
    path([[-1, -.6], [1, -.6], [1, .6], [-1, .6], [-1, -.6]]),
    path([[-1, -.6], [0, .1], [1, -.6]]),
  ]),
];
// rho by hand: one stroke (loop on a stem), a stem plus a bowl, a bowl that leaves the stem low, a tall thin one
shapes.push(
  G('rho-two', '(var)?rho', [
    line([-.3, -.3], [-.3, 1]),
    spline([[-.3, -.1], [-.1, -.85], [.45, -.85], [.6, -.35], [.25, .05], [-.3, -.1]]),
  ]),
  G('rho-loop', '(var)?rho', [
    spline([[-.3, 1], [-.3, .2], [-.3, -.5], [-.05, -.9], [.4, -.85], [.6, -.4], [.35, 0], [-.05, .05], [
      -.3,
      -.4,
    ]]),
  ]),
  G('rho-low', '(var)?rho', [
    spline([[-.35, .95], [-.35, .1]]),
    spline([[-.35, .5], [-.3, -.4], [.1, -.7], [.6, -.45], [.45, .1], [-.05, .35], [-.35, .2]]),
  ]),
  G('rho-tall', '(var)?rho', [
    spline([[-.1, 1], [-.1, .3], [-.15, -.5], [.15, -.95], [.45, -.7], [.4, -.15], [.05, 0], [-.15, -.5]]),
  ]),
);
// every lowercase Greek letter again, narrower, slanted and wider with a lower stroke
const lower = shapes.filter((x) => x[0] === 'greek' && /^[a-z]+$/.test(x[1]) && !x[1].startsWith('rho'));
const warp = (st, f) => st.map((s) => (typeof s[0][0] === 'number' ? s.map(f) : s.map((t) => t.map(f))));
for (const [c, n, st, re, w] of lower) {
  shapes.push([c, n + '-narrow', warp(st, ([x, y]) => [x * .65, y]), re, w], [
    c,
    n + '-slant',
    warp(st, ([x, y]) => [x * .9 - y * .35, y]),
    re,
    w,
  ], [c, n + '-wide', warp(st, ([x, y]) => [x * 1.15, y * .75 + .1]), re, w]);
}
// stem and bowl letters drawn by hand: stem first then bowl, bowl first then stem, and in one stroke; q, b, d mirror p
const pS = [
  line([-.4, -.6], [-.4, 1]),
  spline([[-.4, -.45], [0, -.75], [.5, -.55], [.55, -.05], [.1, .2], [-.4, .05]]),
];
const pB = [
  spline([[-.4, -.1], [-.3, -.6], [.15, -.75], [.55, -.45], [.45, .05], [0, .2], [-.4, .05]]),
  line([-.4, -.65], [-.4, 1]),
];
const p1 = [[
  ...line([-.4, -.6], [-.4, 1]),
  ...line([-.4, 1], [-.4, -.35]),
  ...spline([[-.4, -.35], [0, -.75], [.5, -.55], [.55, -.05], [.1, .2], [-.4, .05]]),
]];
const fx = (st) => warp(st, ([x, y]) => [-x, y]), fy = (st) => warp(st, ([x, y]) => [x, -y]);
const P = (name, re, ch, strokes) => ['bowl', name, strokes, R(re, `^${ch} `), 10];
shapes.push(
  P('p-stem', '(var)?rho|m[a-z]+p', 'p', pS),
  P('p-bowl', '(var)?rho|m[a-z]+p', 'p', pB),
  P('p-one', '(var)?rho|m[a-z]+p', 'p', p1),
  P('rho-bowl', '(var)?rho', 'ρ', [
    spline([[-.3, -.3], [-.1, -.85], [.45, -.8], [.55, -.3], [.2, .05], [-.25, -.1], [-.3, .4], [-.3, 1]]),
  ]),
  P('rho-up', '(var)?rho', 'ρ', [
    spline([[-.3, 1], [-.3, .3], [-.3, -.35], [.05, -.8], [.5, -.6], [.5, -.1], [.1, .1], [-.3, -.05]]),
  ]),
  P('varrho', '(var)?rho', 'ϱ', [
    spline([[.5, .9], [-.1, .75], [-.4, .3], [-.35, -.4], [0, -.85], [.45, -.7], [.5, -.2], [.1, .1], [
      -.35,
      0,
    ]]),
  ]),
  P('q-stem', 'm[a-z]+q', 'q', fx(pS)),
  P('q-bowl', 'm[a-z]+q', 'q', fx(pB)),
  P('b-stem', 'm[a-z]+b', 'b', fy(pS)),
  P('b-bowl', 'm[a-z]+b', 'b', fy(pB)),
  P('d-stem', 'm[a-z]+d', 'd', fx(fy(pS))),
  P('d-bowl', 'm[a-z]+d', 'd', fx(fy(pB))),
  P('p-tall', '(var)?rho|m[a-z]+p', 'p', [
    line([-.3, -.9], [-.3, 1]),
    spline([[-.3, -.8], [.05, -.95], [.4, -.75], [.4, -.4], [.05, -.25], [-.3, -.35]]),
  ]),
  P('p-tall-bowl', '(var)?rho|m[a-z]+p', 'p', [
    spline([[-.3, -.4], [-.25, -.85], [.1, -.95], [.4, -.7], [.35, -.35], [0, -.25], [-.3, -.4]]),
    line([-.3, -.6], [-.3, 1]),
  ]),
  P('rho-small', '(var)?rho', 'ρ', [
    spline([[-.2, 1], [-.2, .2], [-.2, -.45], [0, -.9], [.35, -.85], [.45, -.55], [.25, -.3], [-.05, -.3], [
      -.2,
      -.45,
    ]]),
  ]),
  [
    'icon',
    'smiley-oval',
    [ell(0, 0, .75, 1), ...dot(-.28, -.3), ...dot(.28, -.3), arc(0, .15, .4, .35, .3, PI - .3)],
    R('faSmile|faSmileO|smiley|Smiley|faFaceSmile|faGrin|frownie|Laughey|SmileyFace'),
    10,
  ],
  [
    'set-logic',
    'slashed-oval',
    [ell(0, 0, .7, .95), line([.75, -1], [-.75, 1])],
    R('varnothing|emptyset'),
    10,
  ],
  // a flag: a rectangle, and one on a pole
  ['icon', 'flag-rect', [path([[-.9, -.6], [.9, -.6], [.9, .6], [-.9, .6], [-.9, -.6]])], R('worldflag|faFlag|flag|Flag|euflag'), 10],
  ['icon', 'flag-pole', [line([-.7, -1], [-.7, 1]), path([[-.7, -.9], [.8, -.9], [.8, .1], [-.7, .1]])], R('worldflag|faFlag|flag|Flag|euflag'), 10],
  // an orientation misremembered: the drawing mirrored, the wanted symbol the other way round
  ['flip', 'flip-slash', [line([-.5, -1], [.5, 1])], R('slash|mathslash|divslash', '^/ '), 10],
  [
    'flip',
    'flip-subset',
    [spline([[-.8, -.8], [.3, -.8], [.8, -.4], [.8, .4], [.3, .8], [-.8, .8]])],
    R('subset'),
    10,
  ],
  [
    'flip',
    'flip-hook',
    [
      spline([[1, .3], [.9, -.2], [.7, .2], [.6, 0]]).concat(line([.6, 0], [-.9, 0])),
      head([-1, 0], [-1, 0], .4),
    ],
    R('hookrightarrow'),
    10,
  ],
  ['flip', 'flip-nearrow', arrow([-.8, -.8], [.85, .85]), R('nearrow'), 10],
  P('nine', 'm[a-z]+nine', '9', [
    spline([[.4, -.3], [.1, -.05], [-.35, -.3], [-.3, -.8], [.15, -.9], [.4, -.6], [.4, 0], [.3, .6], [
      0,
      .95,
    ]]),
  ]),
  P('six', 'm[a-z]+six', '6', [
    spline([[.35, -.9], [-.05, -.75], [-.35, -.2], [-.35, .5], [0, .9], [.35, .6], [.35, .15], [0, -.05], [
      -.35,
      .3,
    ]]),
  ]),
);
const names = new Set(), seen = shapes.map((s) => s[1]).filter((n) => names.has(n) || !names.add(n));
if (seen.length) throw new Error('duplicate shape names ' + seen);

// ---- run
const b = await chromium.launch({ executablePath: chrome });
const pg = await b.newPage({ viewport: { width: 900, height: 700 } });
await pg.route(/style-[0-9a-f]+\.css$/, (r) => r.fulfill({ body: css, contentType: 'text/css' }));
await pg.route(
  /draw-[0-9a-f]+\.js$/,
  (r) =>
    r.fulfill({
      body: js.replace(/return \{\s*load: \(\) =>/, 'self.__dbg = { symbolsLoad, pad, DRAW, drawMatch, tune }; $&'),
      contentType: 'text/javascript',
    }),
);
pg.on('pageerror', (e) => console.log('pageerror', e.message));
pg.on('console', (m) => m.type() === 'error' && console.log('console', m.text()));
await pg.goto(`http://127.0.0.1:${process.env.PORT ?? 8000}/draw/index.html?theme=light`);
await pg.waitForSelector('#draw-pad');
// PKGS=all: every package on, to check that more packages never show fewer matches (compare `count` of two OUT files)
if (process.env.PKGS === 'all') {
  await pg.evaluate(() => document.querySelector('#draw-pkgs').click());
  await pg.click('[data-all="1"]');
  await pg.keyboard.press('Escape');
}
const rowsOf = () =>
  pg.$$eval(
    '#draw-list > li.draw-row',
    (l) =>
      l.map((li) =>
        li.querySelector('.draw-glyph').textContent + ' ' +
        [...li.querySelectorAll('code')].map((c) => c.textContent).join(' ')
      ),
  );
const sweep = process.env.SWEEP ? JSON.parse(fs.readFileSync(process.env.SWEEP, 'utf8')) : null, grid = {};
const results = [];
const t0 = Date.now();
for (const [cat, name, strokes, re, want] of shapes) {
  if (only && !only.split(',').includes(name)) continue;
  await pg.click('#draw-clear');
  const bb = await pg.locator('#draw-pad').boundingBox(),
    c = [bb.x + bb.width / 2, bb.y + bb.height / 2, bb.width * .4];
  const flat = strokes.flatMap((s) => (typeof s[0][0] === 'number' ? [s] : s));
  for (const s of flat) {
    await pg.mouse.move(c[0] + s[0][0] * c[2], c[1] + s[0][1] * c[2]);
    await pg.mouse.down();
    for (const p of s) await pg.mouse.move(c[0] + p[0] * c[2], c[1] + p[1] * c[2]);
    await pg.mouse.up();
  }
  await pg.waitForTimeout(300);
  await pg.waitForFunction(
    () =>
      !document.querySelector('#draw-list.refining') &&
      document.querySelectorAll('#draw-list .draw-glyph').length > 0,
    null,
    { timeout: 240000 },
  );
  await pg.waitForTimeout(400);
  for (let i = 0; i < 6; i++) await pg.evaluate(() => document.querySelector('[data-more]')?.click());
  await pg.waitForTimeout(300);
  if (sweep) {
    for (const [label, cfg] of Object.entries(sweep)) {
      await pg.evaluate(async (cfg) => {
        const d = self.__dbg;
        d.base ??= { ...d.DRAW };
        d.tune({ ...d.base, ...cfg });
        const t = performance.now();
        await d.drawMatch(false);
        d.ms = [...(d.ms ?? []), performance.now() - t];
      }, cfg);
      await pg.waitForFunction(() => !document.querySelector('#draw-list.refining'), null, {
        timeout: 60000,
      });
      await pg.waitForTimeout(500);
      const k = (await rowsOf()).findIndex((t) => re.test(t));
      (grid[label] ??= {})[name] = k < 0 ? 999 : k + 1;
    }
  }
  const rows = await rowsOf(), k = rows.findIndex((t) => re.test(t)), rank = k < 0 ? null : k + 1;
  if (process.env.TOP) {
    console.log(
      rows.slice(0, +process.env.TOP).map((t, i) => i + 1 + ' ' + t.replace(/\s+/g, ' ').slice(0, 40)).join(
        '\n',
      ),
    );
  }
  const ms = await pg.evaluate(() => document.querySelector('#draw-list').dataset.full);
  const top = rows.slice(0, 3).map((t) => t.replace(/\s+/g, ' ').slice(0, 26));
  results.push({
    cat,
    name,
    rank,
    want,
    ok: rank !== null && rank <= want,
    ms: +ms || null,
    shown: rows.length,
    count: +(await pg.evaluate(() => document.querySelector('#draw-list').dataset.count)),
    top,
  });
  console.log(
    name.padEnd(15),
    `${ms}ms`.padEnd(8),
    String(rank ?? '>' + rows.length).padStart(4),
    rank !== null && rank <= want ? 'ok  ' : 'MISS',
    ' top:',
    top.join(' | '),
  );
}
// ---- summary
const R_ = (r) => r.rank ?? Infinity, n = results.length;
if (n) {
  const sorted = results.map(R_).sort((a, b) => a - b), med = sorted[n >> 1];
  console.log(
    `\n== ${n} shapes: top1 ${results.filter((r) => R_(r) <= 1).length}, top3 ${
      results.filter((r) => R_(r) <= 3).length
    }, top5 ${results.filter((r) => R_(r) <= 5).length}, top10 ${
      results.filter((r) => R_(r) <= 10).length
    }, not found ${results.filter((r) => r.rank === null).length}, median rank ${med}, on target ${
      results.filter((r) => r.ok).length
    }/${n}`,
  );
  for (const cat of [...new Set(results.map((r) => r.cat))]) {
    const g = results.filter((r) => r.cat === cat);
    console.log(
      `  ${cat.padEnd(10)} ${g.length} shapes: ok ${g.filter((r) => r.ok).length}, top1 ${
        g.filter((r) => R_(r) <= 1).length
      }, top3 ${g.filter((r) => R_(r) <= 3).length}, top10 ${g.filter((r) => R_(r) <= 10).length}`,
    );
  }
  console.log(
    'worst:',
    results.filter((r) => !r.ok).sort((a, b) => R_(b) - R_(a)).slice(0, 15).map((r) =>
      `${r.name}(${r.rank ?? '>' + r.shown})`
    ).join(' '),
  );
  fs.mkdirSync('target/scratch', { recursive: true });
  fs.writeFileSync(
    outFile,
    JSON.stringify(
      { date: new Date().toISOString(), js: process.env.JS ?? 'doc/site/draw.js', results },
      null,
      1,
    ),
  );
}
for (const [label, g] of Object.entries(grid)) {
  console.log(
    label.padEnd(14),
    Object.values(g).map((r) => String(r).padStart(4)).join(''),
    ' top3',
    Object.values(g).filter((r) => r <= 3).length,
    'sum',
    Object.values(g).reduce((a, r) => a + Math.log(Math.min(r, 210)), 0).toFixed(1),
  );
}
console.log('s', (Date.now() - t0) / 1000);
await b.close();
