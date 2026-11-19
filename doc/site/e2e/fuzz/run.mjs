// The fuzzer's runner (fuzz.mjs): seeds over documents and views, each failure printed with its seed and step list,
// shrunk with --min. Replay one: --replay '{"doc":"hello","view":"pdf","steps":[…]}'.
//   node doc/site/e2e/fuzz/run.mjs --port 8481 --seeds 1-10 [--docs hello,markup] [--views pdf,ts] [--n 30] [--min]
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { browser, page } from '../lib.mjs';
import { ctx } from '../engine.mjs';
import { DOCS, gen, run, minimize } from './fuzz.mjs';

const ROOT = fileURLToPath(new URL('../../../../', import.meta.url));
const arg = (n, d) => { const i = process.argv.indexOf(`--${n}`); return i < 0 ? d : process.argv[i + 1]; };
const URL_ = `http://127.0.0.1:${arg('port', '8481')}/play/`, OUT = arg('out', join(ROOT, 'target/scratch/ui-51/fuzz'));
mkdirSync(OUT, { recursive: true });
ctx.out = OUT;
const [lo, hi = lo] = arg('seeds', '1').split('-').map(Number), n = +arg('n', '30');
const docs = arg('docs', Object.keys(DOCS).join(',')).split(','), views = arg('views', 'pdf,ts').split(',');
const b = await browser(OUT);
ctx.cdp = b.cdp; ctx.url = URL_;
const fresh = async () => { const p = await page(b.cdp, URL_, OUT); if (!await p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000)) throw new Error('the playground did not start'); return p; };
const jobs = process.argv.includes('--replay') ? [JSON.parse(arg('replay'))]
  : Array.from({ length: hi - lo + 1 }, (_, i) => lo + i).flatMap((seed) => docs.flatMap((doc) => views.map((view) => ({ seed, doc, view, steps: gen(seed, n) }))));
const found = [];
for (const job of jobs) {
  const p = await fresh(), t0 = Date.now();
  let r;
  try { r = await run(p, job); } catch (e) { r = { fails: [{ step: -1, cls: 'error', msg: String(e?.message ?? e).slice(0, 300) }], log: [] }; }
  const f = r.fails[0];
  console.log(`${f ? 'FAIL' : 'ok  '} seed ${job.seed ?? '-'} ${job.doc}/${job.view} ${((Date.now() - t0) / 1000).toFixed(0)} s${f ? ` step ${f.step} ${job.steps[f.step]} [${f.cls}] ${f.msg}` : ''}`);
  if (process.argv.includes("--log")) console.log("  " + r.log.join("\n  "));
  if (f) {
    await p.shot(`fz-${job.seed}-${job.doc}-${job.view}.png`).catch(() => {});
    const rec = { ...job, steps: f.step >= 0 ? job.steps.slice(0, f.step + 1) : job.steps, fail: f, log: r.log };
    if (process.argv.includes('--min') && f.step >= 0) { rec.min = await minimize(fresh, rec, f.cls, +arg('budget', '8')); console.log(`  minimized: ${J(rec.min)}`); }
    console.log(`  replay: --replay '${J({ doc: job.doc, view: job.view, steps: rec.min ?? rec.steps })}'`);
    found.push(rec);
  }
  await p.close();
}
b.close();
writeFileSync(join(OUT, `found-${lo}-${hi}.json`), J(found, null, 1));
function J(x, ...r) { return JSON.stringify(x, ...r); }
