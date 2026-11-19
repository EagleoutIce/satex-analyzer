// The shared ground of the site's unit tests (doc/site/tests/*.test.mjs, run by doc/site/views.test.mjs): the site's
// scripts loaded as the browser loads them, a test registry, and a seeded random source so every run is the same.
import { load } from '../load.mjs';

globalThis.self = globalThis;
for (const f of ['./lengths.js', './snapshot.js', './views.js', './layers.js', './pdfedit.js', './mathlayout.js', './dialogs.js', './actions.js', './refactor.js']) load(f);
export { load };

/** The tests registered so far: `[file, name, fn]`; the runner runs them in order. */
export const tests = [];
let file = '';
/** Names the file whose tests follow (the runner sets it before each import). */
export const from = (f) => { file = f; };
/** Registers test `name`; `fn` may be async. */
export const test = (name, fn) => { tests.push([file, name, fn]); };

/** A seeded random source (mulberry32): `int(n)` in 0..n-1, `pick(list)`, `chance(p)`, `float()`. */
export function rng(seed) {
  let s = seed >>> 0;
  const float = () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const int = (n) => Math.floor(float() * n);
  return { float, int, pick: (l) => l[int(l.length)], chance: (p) => float() < p };
}
/** `text` with `[a, b, body]` applied. */
export const apply = (text, [a, b, body]) => text.slice(0, a) + body + text.slice(b);
