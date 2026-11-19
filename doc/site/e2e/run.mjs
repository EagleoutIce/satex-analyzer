// The playground's browser e2e suite (README.md). Needs a running `satex-playground --dev` (scripts/e2e.sh starts one).
//   node doc/site/e2e/run.mjs --port 8472 [--tier quick|full|perf] [--only a,b] [--area math,clicks] [--repeat 3] [--out DIR]
// Writes results.md, results.json and a screenshot per failure into the run's own directory (default
// target/e2e/<tier>-<time>/). Exit 1 on a failure that is not a known red (known-red.json).
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { browser, page } from './lib.mjs';
import { ctx, KINDS, reseed } from './engine.mjs';
import { foot } from './kinds/foot.mjs';
import { echo } from './kinds/echo.mjs';
import { paper } from './kinds/paper.mjs';
import { follow } from './kinds/follow.mjs';
import { tikz } from './kinds/tikz.mjs';
import { crash } from './kinds/crash.mjs';
import { gutter, close } from './kinds/editor.mjs';
import { SCENARIOS, TIERS } from './scenarios/index.mjs';

const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
const arg = (n, d) => { const i = process.argv.indexOf(`--${n}`); return i < 0 ? d : process.argv[i + 1]; };
const list = (n) => (arg(n, '') ?? '').split(',').filter(Boolean);
const tier = arg('tier', 'quick'), only = list('only'), areas = list('area'), repeat = +arg('repeat', '1');
if (!['quick', 'full', 'perf'].includes(tier)) throw new Error(`--tier ${tier}: quick, full or perf`);
const URL_ = arg('url', `http://127.0.0.1:${arg('port', '8472')}/play/`);
const stamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\..*/, '').replace('T', '-');
const OUT = arg('out', join(ROOT, 'target/e2e', `${tier}-${stamp}`));
mkdirSync(OUT, { recursive: true });
ctx.out = OUT;
const KNOWN = JSON.parse(readFileSync(new URL('known-red.json', import.meta.url), 'utf8'));
const RUN = { ...KINDS, foot, echo, paper, follow, tikz, crash, gutter, close };

const picked = SCENARIOS.filter((sc) => only.length ? only.includes(sc.name)
  : (!areas.length || areas.includes(sc.area)) && (tier === 'full' || TIERS[tier].includes(sc.name)));
if (!picked.length) throw new Error('no scenario picked');
for (const n of only) if (!SCENARIOS.some((s) => s.name === n)) throw new Error(`no scenario ${n}`);

// the server answers before a browser starts: a clear message instead of a timeout per scenario
try { const r = await fetch(URL_); if (!r.ok) throw new Error(`HTTP ${r.status}`); }
catch (e) { console.error(`no playground at ${URL_} (${e.message}): start one with scripts/e2e.sh or satex-playground --dev --port N`); process.exit(2); }

const started = Date.now(), b = await browser(OUT), runs = new Map();
ctx.cdp = b.cdp; ctx.url = URL_;
let p;
b.cdp.on('Page.javascriptDialogOpening', () => p?.s('Page.handleJavaScriptDialog', { accept: true }).catch(() => {}));
for (let round = 1; round <= repeat; round++) {
  for (const sc of picked) {
    const t0 = Date.now();
    let r;
    reseed();
    try {
      // a fresh browser context per scenario: no state (caret, cache, edited files, a stopped server) leaks between them
      p = await page(b.cdp, URL_, OUT);
      if (!await p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000)) throw new Error('the playground did not start');
      r = await RUN[sc.kind ?? 'type'](p, sc);
    } catch (e) {
      r = { name: sc.name, fails: [`error: ${String(e?.message ?? e).slice(0, 300)}`], perf: [], info: {} };
      r.shot = await p?.shot(`${sc.name}-error.png`).catch(() => undefined);
    }
    r.perf ??= [];
    // no overlay over a page without its image, in any frame of any scenario
    const bare = await p?.eval(`({ n: self.__e2e.bareFrames, at: self.__e2e.bare.slice(0, 3) })`).catch(() => null);
    if (bare?.n) r.fails.push(`overlay over a page with no drawn image in ${bare.n} frames: ${JSON.stringify(bare.at)}`);
    r.pageErrors = (p?.errors ?? []).slice(0, 5);
    await p?.close();
    p = null;
    // perf bounds are failures in the perf tier only: elsewhere a loaded machine would fail correct code
    const fails = tier === 'perf' ? [...r.fails, ...r.perf] : r.fails;
    const known = KNOWN[sc.name];
    r.status = !fails.length ? (known ? 'green-known' : 'pass') : known ? 'known-red' : 'FAIL';
    r.ms = Date.now() - t0; r.area = sc.area; r.round = round;
    (runs.get(sc.name) ?? runs.set(sc.name, []).get(sc.name)).push(r);
    const note = r.status === 'known-red' ? ` (known red, ${known.owner}: ${known.why})` : r.status === 'green-known' ? ' (listed in known-red.json: remove it)' : '';
    console.log(`${r.status.padEnd(11)} ${sc.name} ${(r.ms / 1000).toFixed(1)} s${note}${fails.map((f) => '\n    ' + f.slice(0, 400)).join('')}${tier !== 'perf' && r.perf.length ? `\n    (perf: ${r.perf.join('; ').slice(0, 200)})` : ''}`);
  }
}
b.close();

// per scenario over the rounds: a name that passed and failed is flaky
const rows = [...runs].map(([name, rs]) => {
  const st = new Set(rs.map((r) => r.status === 'green-known' ? 'pass' : r.status === 'known-red' ? 'FAIL' : r.status));
  const status = st.size > 1 ? 'FLAKY' : rs[0].status;
  return { name, area: rs[0].area, status, ms: Math.round(rs.reduce((s, r) => s + r.ms, 0) / rs.length), fails: [...new Set(rs.flatMap((r) => r.fails))], perf: [...new Set(rs.flatMap((r) => r.perf))], shot: rs.find((r) => r.shot)?.shot, runs: rs };
});
const count = (s) => rows.filter((r) => r.status === s).length;
const secs = Math.round((Date.now() - started) / 1000);
const esc = (s) => String(s).replace(/\|/g, '\\|').replace(/\n/g, ' ');
const md = [`# e2e ${tier} ${new Date().toISOString()} (${secs} s, ${repeat} round${repeat > 1 ? 's' : ''})`, '',
  `${count('pass')} pass · ${count('FAIL')} FAIL · ${count('FLAKY')} flaky · ${count('known-red')} known red · ${count('green-known')} known red now green`, '',
  '| scenario | area | result | s | failures | perf | shot |', '|---|---|---|---|---|---|---|',
  ...rows.map((r) => `| ${r.name} | ${r.area} | ${r.status} | ${(r.ms / 1000).toFixed(1)} | ${esc(r.fails.join('<br>').slice(0, 600))} | ${esc(r.perf.join('<br>').slice(0, 200))} | ${r.shot ? relative(OUT, r.shot) : ''} |`)].join('\n');
writeFileSync(join(OUT, 'results.md'), md + '\n');
writeFileSync(join(OUT, 'results.json'), JSON.stringify(rows, null, 1));
console.log(`\n${count('pass')}/${rows.length} pass, ${count('FAIL')} fail, ${count('FLAKY')} flaky, ${count('known-red')} known red${count('green-known') ? `, ${count('green-known')} known red now green (remove from known-red.json)` : ''} · ${secs} s · ${join(OUT, 'results.md')}`);
process.exit(count('FAIL') || count('FLAKY') ? 1 : 0);
