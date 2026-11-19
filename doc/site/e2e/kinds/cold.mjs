// A first visit: a fresh browser context (no IndexedDB, no kept bundles) to the first findings of the default
// example. The site ships the kernel's caches (build.py `kernel_caches`): a visit that builds the kernel itself
// (minutes) is a failure, and while any cache builds the status line's share moves (src/util/progress.rs `share`).
// Scenario fields: `ms`, the bound to the first findings (perf); `still`, the longest a kernel build's share may stand.
import { sleep } from '../lib.mjs';

export async function cold(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const seen = new Set(), shares = [];
  let h = null, at = 0, still = 0;
  for (const end = Date.now() + (sc.limit ?? 600000); Date.now() < end; await sleep(200)) {
    h = await p.eval('self.satexHealth?.() ?? null').catch(() => null);
    if (!h) continue;
    if (h.step) seen.add(h.step);
    const pct = h.busy && /(\d+) %/.exec(h.status)?.[1];
    if (pct && h.step === 'kernel cache') {
      if (+pct !== shares.at(-1)) { shares.push(+pct); at = Date.now(); }
      still = Math.max(still, Date.now() - at);
    }
    if (h.shown !== null && !h.busy && !h.running && !h.boot) break;
  }
  const ms = Date.now() - p.t0;
  Object.assign(res.info, { ms, steps: [...seen], shares, still });
  if (h?.shown == null) res.fails.push(`no findings within ${(sc.limit ?? 600000) / 1000} s (${h?.status})`);
  if (seen.has('kernel cache')) res.fails.push(`the kernel cache was built in the browser (${[...seen].join(', ')}): the site's own did not load`);
  if (still > sc.still) res.fails.push(`the kernel build's share stood still for ${(still / 1000).toFixed(1)} s (${shares.join(' → ')} %)`);
  if (ms > sc.ms) res.perf.push(`first findings after ${(ms / 1000).toFixed(1)} s > ${sc.ms / 1000} s`);
  return res;
}
