// The crash shapes of tests/it/crash.rs on the playground's wasm build, under node: the worker's own binary and
// its call stack, which is the JS engine's and far smaller than a native thread's.
//   SATEX_CRASH_SHAPES=target/e2e/shapes scripts/t.sh crash::hostile     # writes the shapes
//   node doc/site/e2e/crash.mjs target/site/play target/e2e/shapes       # exit 1 on a crash
// A shape must end by itself: no RangeError (stack), no abort, no panic, no internal error.
import { readFileSync, readdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import vm from 'node:vm';

const [dir, shapes] = process.argv.slice(2);
const names = readdirSync(dir);
const glue = names.find((n) => /^satex(-[0-9a-f]+)?\.js$/.test(n)), wasm = names.find((n) => /^satex(-[0-9a-f]+)?\.wasm$/.test(n));
globalThis.require = createRequire(import.meta.url); globalThis.__filename = join(dir, glue); globalThis.__dirname = dir;
vm.runInThisContext(`${readFileSync(join(dir, glue), 'utf8')}\nglobalThis.createSatex = createSatex;`, { filename: glue });
const binary = readFileSync(join(dir, wasm));
/** The bare engine, as the test runs it: no TeX files are needed. */
const ARGS = ['lint', '-f', 'main.tex', '--no-config', '--no-packages', '--no-classes', '--set', 'load.format=false', '--set', 'texmf.kpsewhich=false', '--set', 'limits.steps=400000'];
const DEATHS = ['panicked at', 'overflowed its stack', 'memory allocation of', 'internal-error', 'satex hit a bug', 'Aborted('];
let failed = 0;
for (const file of readdirSync(shapes).filter((f) => f.endsWith('.tex')).sort()) {
  const said = []; let how = null; const t0 = Date.now();
  try {
    // A module per shape: a crashed one is gone, as the worker's is.
    const m = await globalThis.createSatex({ wasmBinary: binary, locateFile: () => join(dir, wasm), print: (l) => said.push(l), printErr: (l) => said.push(l), noInitialRun: true });
    m.FS.mkdirTree('/w'); m.FS.writeFile('/w/main.tex', readFileSync(join(shapes, file))); m.FS.chdir('/w');
    try { m.callMain(ARGS); } catch (e) { if (e?.name !== 'ExitStatus') how = `${e?.name}: ${String(e?.message).slice(0, 100)}`; }
  } catch (e) { how = `boot: ${String(e?.message).slice(0, 100)}`; }
  how ??= DEATHS.find((d) => said.some((l) => l.includes(d))) ?? null;
  console.log(`${how ? 'CRASH' : 'ok   '} ${file.padEnd(24)} ${((Date.now() - t0) / 1000).toFixed(2)} s${how ? `  ${how}` : ''}`);
  if (how) failed++;
}
process.exit(failed ? 1 : 0);
