// The engine of worker.js over a stand-in for the Emscripten module: node doc/site/worker.test.mjs
import { createRequire } from 'node:module';
import assert from 'node:assert/strict';

const SatexEngine = createRequire(import.meta.url)('./worker.js');

// A file system that keeps what a boot wrote and the times set on it.
const boots = [];
const createSatex = async () => {
  const nodes = new Map();
  const FS = {
    mkdirTree() {}, chdir() {},
    writeFile(path, data) { const t = Date.now(); nodes.set(path, { data, mtime: t, ctime: t }); },
    lookupPath(path) { return { node: nodes.get(path) }; },
    doSetAttr(_, node, attr) { Object.assign(node, attr); },
    stat(path) { const n = nodes.get(path); return { mode: 0, mtime: new Date(n.mtime), size: n.data.length }; },
    isDir: () => false, readdir: () => [],
  };
  boots.push(nodes);
  return { FS, callMain: () => 0 };
};
const engine = SatexEngine(createSatex);
const stamp = (boot, path) => { const n = boots[boot].get(path); return `${n.mtime}|${n.ctime}`; };
const text = (t) => new TextEncoder().encode(t);

engine.project('/work', { 'a.tex': text('one'), 'b.tex': text('two') });
await engine.run(['lint']);
await new Promise((r) => setTimeout(r, 5));
await engine.run(['lint']);
assert.equal(stamp(1, '/work/a.tex'), stamp(0, '/work/a.tex'), 'an unchanged file keeps its stamp from boot to boot');

engine.project('/work', { 'a.tex': text('uno') });
await engine.run(['lint']);
assert.notEqual(stamp(2, '/work/a.tex'), stamp(1, '/work/a.tex'), 'a changed file has a new stamp');
assert.equal(stamp(2, '/work/b.tex'), stamp(1, '/work/b.tex'), 'its neighbour keeps its own');

// a run's edits are new each time, and never take the stamp the kept text had
await engine.run(['lint'], { '/work/b.tex': text('dos') });
assert.notEqual(stamp(3, '/work/b.tex'), stamp(2, '/work/b.tex'));
await engine.run(['lint']);
assert.equal(stamp(4, '/work/b.tex'), stamp(2, '/work/b.tex'), 'without the edit, the kept text and its stamp');
console.log('worker.test.mjs: ok');

// Bundles: kept whole in the store by name, checked against the digest the name holds, retried, and a
// language server already up sees a load's files without a restart.
{
  const { gzipSync } = await import('node:zlib');
  const { createHash } = await import('node:crypto');
  const body = text('\\ProvidesPackage{x}');
  const gz = gzipSync(body);
  const entry = { id: 'pdftex/x', label: 'x', size: gz.length, bundle: `pkg/pdftex-x-${createHash('sha256').update(gz).digest('hex').slice(0, 12)}.bin.gz`, files: [['/texmf.bundle/x.sty', body.length]] };
  const kept = new Map();
  const store = { bundle: async (n) => kept.get(n) ?? null, keepBundle: async (n, d) => { kept.set(n, d); }, unbundle: async (n) => { kept.delete(n); } };
  let asked = 0, answers = [];
  globalThis.fetch = async () => { asked++; const a = answers.shift() ?? gz; if (a instanceof Error) throw a; return new Response(a); };
  const settle = () => new Promise((r) => setTimeout(r, 10));

  // a reload mid-download: what failed was not kept, the retry lands and is kept whole
  answers = [new Error('connection reset'), gz.subarray(0, gz.length >> 1)];
  const first = SatexEngine(createSatex, () => {}, store);
  await first.bundles('/', [entry]);
  await settle();
  assert.equal(asked, 3, 'two failures (one a cut-off body), then the whole bundle');
  assert.deepEqual(kept.get(entry.bundle), new Uint8Array(gz), 'only the whole, checked bundle is kept');
  assert.deepEqual(first.read('/texmf.bundle/x.sty').data, body);

  // a reload after everything arrived: no download
  asked = 0;
  const second = SatexEngine(createSatex, () => {}, store);
  await second.bundles('/', [entry]);
  assert.equal(asked, 0, 'a kept bundle is read from the store');
  assert.deepEqual(second.read('/texmf.bundle/x.sty').data, body);

  // a damaged copy in the store is dropped and fetched again
  const bad = new Uint8Array(gz); bad[bad.length - 9] ^= 1;
  kept.set(entry.bundle, bad);
  await SatexEngine(createSatex, () => {}, store).bundles('/', [entry]);
  await settle();
  assert.equal(asked, 1, 'a damaged kept bundle is fetched again');
  assert.deepEqual(kept.get(entry.bundle), new Uint8Array(gz));

  // the replay of a page's messages: a server is up, a package lands, the next analysis runs in the same instance and finds its files
  const lspBoots = [];
  const lspSatex = async (options) => {
    const m = await createSatex(options);
    lspBoots.push(m);
    const nodes = boots.at(-1);
    Object.assign(m, { stringToNewUTF8: () => 0, UTF8ToString: () => '[]', _free() {}, _satex_lsp: () => 0, has: (p) => nodes.has(p) });
    m.FS.readdir = () => []; m.FS.unlink = (p) => nodes.delete(p);
    return m;
  };
  const page = SatexEngine(lspSatex, () => {}, store);
  page.project('/work', { 'a.tex': text('\\usepackage{x}') });
  page.reset(['-f', 'a.tex']);
  await page.lsp({ jsonrpc: '2.0', id: 1, method: 'initialize', params: {} });
  await page.bundles('/', [entry]);
  await page.lsp({ jsonrpc: '2.0', method: 'satex/analyze', params: {} });
  assert.equal(lspBoots.length, 1, 'a package load needs no new server');
  assert.ok(lspBoots[0].has('/texmf.bundle/x.sty'), 'the running server has the package files');
}
console.log('worker.test.mjs: bundles ok');
