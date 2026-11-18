// Property tests of Type mode's one gate (pdfedit.ts `checked`, `key`): seeded random documents and random edits, each
// accepted edit checked by an independent reader (tests/reader.mjs). Deterministic: every seed is fixed.
import assert from 'node:assert/strict';
import { test, rng, apply } from './harness.mjs';
import { source } from './gen.mjs';
import { read } from './reader.mjs';

const E = self.satexPdfEdit;
const ESCW = /^(textbackslash|textasciitilde|textasciicircum)$/;
// a key command's key, its option and a line break before them allowed (TeX skips one), never a blank line
const keysOf = (t) => [...t.matchAll(/\\(label|ref|cite)(?:[ \t]*\n?[ \t]*\[[^\]]*\])?[ \t]*\n?[ \t]*\{([^}]*)\}/g)].map((m) => `${m[1]}:${m[2]}`).join(' ');

/** Why `now` (the edit `[a, b)` → `n` characters of `was`) breaks `was`'s structure, '' when it keeps it. */
function broken(was, now, a, b, n) {
  const r0 = read(was), r1 = read(now), why = [];
  if (r1.depth !== r0.depth || r1.low < r0.low) why.push('braces');
  if (r1.env.join() !== r0.env.join()) why.push('environments');
  if (r1.com.join('\n') !== r0.com.join('\n')) why.push('comments');
  if (r1.verb.length !== r0.verb.length) why.push('verbatim');
  if (r1.bad > r0.bad) why.push('math');
  if (r1.parIn > r0.parIn) why.push('paragraph in a group or math');
  if (r1.script > r0.script) why.push('script outside math');
  if (r1.amp > r0.amp) why.push('&');
  if (r1.hash > r0.hash) why.push('#');
  if (keysOf(now) !== keysOf(was)) why.push('keys');
  // every command outside the edit as it was; inside it only escapes go
  const kept = (cs, p, q) => cs.filter(([i]) => i < p || i >= q).map(([, nm]) => nm).join(' ');
  if (kept(r0.cs, a, b) !== kept(r1.cs, a, a + n)) why.push('commands');
  if (r0.cs.some(([i, nm]) => i >= a && i < b && !ESCW.test(nm))) why.push('a command deleted');
  return why.join(', ');
}

const POOL = ['a', 'b', ' ', '{', '}', '{}', '$', '$$', '\\', '\\(', '\\)', '\\[', '\\]', '_', '^', '%', '&', '#', '\n', '\n\n', '[', ']', '~', '\\x', '\\%', 'x}', '{y'];
const DOCS = Array.from({ length: 40 }, (_, k) => source(rng(1000 + k)));

// Intuition: whatever splice the gate lets through keeps every brace, environment, comment, math pair and key; pins the
// gate as the single guard of the playground's writes (the drawn and the written text pass it alike).
test('gate: an accepted random splice keeps the structure', () => {
  const R = rng(0xc0ffee), fails = [];
  let accepted = 0, tried = 0;
  for (const doc of DOCS) {
    for (let k = 0; k < 250; k++) {
      const a = R.int(doc.length + 1), b = Math.min(doc.length, a + (R.chance(0.5) ? 0 : R.int(4)));
      const body = Array.from({ length: R.int(3) }, () => R.pick(POOL)).join('');
      tried++;
      if (E.checked(doc, [a, b, body]) != null) continue;
      accepted++;
      const why = broken(doc, apply(doc, [a, b, body]), a, b, body.length);
      if (why) fails.push({ why, cut: doc.slice(a, b), body, around: doc.slice(Math.max(0, a - 20), b + 20) });
    }
  }
  assert.ok(accepted > 1500 && accepted < tried, `accepted ${accepted} of ${tried}`);
  assert.deepEqual(fails.slice(0, 3), [], `${fails.length} accepted splices broke the structure`);
});

// Intuition: an edit the gate accepts can be taken back, and the gate accepts that too (what it typed counts as typed
// since the caret was put, as the playground tracks it): no accepted state is a trap. The inverse restores the text.
test('gate: an accepted splice round-trips', () => {
  const R = rng(77), fails = [];
  let n = 0;
  for (const doc of DOCS) {
    for (let k = 0; k < 150; k++) {
      const a = R.int(doc.length + 1), b = Math.min(doc.length, a + R.int(3));
      const body = Array.from({ length: R.int(3) }, () => R.pick(POOL)).join('');
      if (E.checked(doc, [a, b, body]) != null) continue;
      const now = apply(doc, [a, b, body]), back = [a, a + body.length, doc.slice(a, b)];
      assert.equal(apply(now, back), doc);
      n++;
      const why = E.checked(now, back, { fresh: body ? [a, a + body.length] : null });
      if (why) fails.push({ why, cut: doc.slice(a, b), body, around: doc.slice(Math.max(0, a - 15), b + 15) });
    }
  }
  assert.ok(n > 800, `${n} round trips`);
  assert.deepEqual(fails.slice(0, 3), [], `${fails.length} accepted splices could not be undone`);
});

// Intuition: random key runs from any offset (inside names, options, keys, math, verbatim too, not only page words)
// through E.key, the playground's one entry: every edit stays in the text, the caret too, and the structure holds
// (commands typed since the caret was put may change). Pins edits that indexed past the text's end or moved the caret out.
test('key: runs from any offset keep offsets in range and the structure', () => {
  const R = rng(4242), fails = [];
  const KEYS = [...'abcxyz', ' ', 'Backspace', 'Backspace', 'Delete', 'Ctrl+Backspace', 'Ctrl+Delete', 'Enter', 'Shift+Enter', '{', '}', '$', '\\', '_', '^', '%', '&', '#', '~'];
  let edits = 0;
  for (const doc of DOCS) {
    for (let run = 0; run < 60; run++) {
      let t = doc, at = R.int(doc.length + 1), f = null, fresh = new Array(t.length).fill(false);
      for (let n = 1 + R.int(10); n--;) {
        const k = R.pick(KEYS), e = E.key(t, at, k, { fresh: f, inline: R.chance(0.5) ? '\\(' : '$', smart: R.chance(0.5) });
        if (typeof e === 'string') continue;
        const [a, b, body, step] = e, was = t;
        if (!(0 <= a && a <= b && b <= t.length)) { fails.push({ k, why: `edit ${a}..${b} outside 0..${t.length}` }); break; }
        t = apply(t, e);
        if (!(0 <= a + step && a + step <= t.length)) { fails.push({ k, why: `caret ${a + step} outside 0..${t.length}` }); break; }
        fresh = [...fresh.slice(0, a), ...Array(body.length).fill(true), ...fresh.slice(b)];
        f = E.grow(f, a, b, body.length);
        at = a + step;
        edits++;
        const r0 = read(doc), r1 = read(t), why = [];
        if (r1.depth !== r0.depth || r1.low < r0.low) why.push('braces');
        if (r1.env.join() !== r0.env.join()) why.push('environments');
        if (r1.com.join('\n') !== r0.com.join('\n')) why.push('comments');
        if (r1.bad > r0.bad) why.push('math');
        if (r1.parIn > r0.parIn) why.push('paragraph in a group');
        if (r1.script > r0.script) why.push('script outside math');
        if (r1.amp > read(was).amp && k !== 'Enter') why.push('&');
        if (keysOf(t) !== keysOf(doc)) why.push('keys');
        const own = (cs, mask) => cs.filter(([i, nm]) => !ESCW.test(nm) && !(mask && mask.slice(i, i + nm.length + 1).every(Boolean))).map(([, nm]) => nm).join(' ');
        if (own(r1.cs, fresh) !== own(r0.cs)) why.push('commands');
        if (why.length) { fails.push({ why: why.join(), k, before: was.slice(Math.max(0, a - 20), b + 20), after: t.slice(Math.max(0, a - 20), a + body.length + 20) }); break; }
      }
    }
  }
  assert.ok(edits > 3000, `${edits} edits`);
  assert.deepEqual(fails.slice(0, 3), [], `${fails.length} key runs went wrong`);
});

// Intuition: plain text typed into plain text is never refused and Backspace takes it back character by character;
// pins a gate so strict that ordinary typing stalls (the caret moved but nothing was written).
test('key: plain words typed and erased again', () => {
  const R = rng(9);
  let n = 0;
  for (const doc of DOCS.slice(0, 20)) {
    const carets = [...doc.matchAll(/(?<![\\A-Za-z@])[A-Za-z]{3,}/g)].map((m) => m.index + 1).filter((p) => E.editable(doc, p, p, []) == null);
    for (let k = 0; k < 10 && carets.length; k++) {
      const at0 = R.pick(carets), word = R.pick(['hello', 'ab cd', 'q']);
      let t = doc, at = at0;
      for (const c of word) { const e = E.key(t, at, c); assert.notEqual(typeof e, 'string', `${c} at ${JSON.stringify(t.slice(at - 10, at + 10))}: ${e}`); t = apply(t, e); at = e[0] + e[3]; }
      assert.equal(t, doc.slice(0, at0) + word + doc.slice(at0));
      for (const _ of word) { const e = E.key(t, at, 'Backspace'); t = apply(t, e); at = e[0]; }
      assert.equal(t, doc);
      n++;
    }
  }
  assert.ok(n > 100, `${n} words`);
});
