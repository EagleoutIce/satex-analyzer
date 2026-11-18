// The runner of the site's unit tests: imports each tests/*.test.mjs (or the files named on the command line), runs
// every registered test in order, prints a line per file and a total, and fails when one does.
//   node doc/site/views.test.mjs              all
//   node doc/site/tests/run.mjs forest        the files whose name holds `forest`
import { readdirSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { tests, from } from './harness.mjs';

const dir = new URL('./', import.meta.url);

/** Runs the test files whose names hold one of `only` (all when empty); resolves to the number of failures. */
export async function main(only = []) {
  const files = readdirSync(dir).filter((f) => f.endsWith('.test.mjs') && (!only.length || only.some((o) => f.includes(o)))).sort();
  for (const f of files) { from(f); await import(new URL(f, dir).href); }
  const t0 = performance.now(), per = new Map();
  let failed = 0;
  for (const [file, name, fn] of tests) {
    const s = per.get(file) ?? { n: 0, bad: 0, ms: 0 }, t = performance.now();
    per.set(file, s);
    try { await fn(); } catch (e) { failed++; s.bad++; console.error(`FAIL ${file}: ${name}\n${e?.stack ?? e}\n`); }
    s.n++;
    s.ms += performance.now() - t;
  }
  for (const [file, s] of per) console.log(`${s.bad ? 'FAIL' : 'ok  '} ${file.padEnd(28)} ${String(s.n).padStart(3)} tests ${s.ms.toFixed(0).padStart(5)} ms${s.bad ? `, ${s.bad} failed` : ''}`);
  console.log(`${tests.length - failed}/${tests.length} passed in ${((performance.now() - t0) / 1000).toFixed(2)} s`);
  return failed;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) process.exitCode = (await main(process.argv.slice(2))) ? 1 : 0;
