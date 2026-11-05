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
