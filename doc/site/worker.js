// The playground's engine: the `satex` binary, built for Emscripten, run over
// an in-memory file system holding the TeX bundle. A Web Worker in the
// browser, started from the page's copy of this file; doc/site/test.mjs runs
// the same code in node.

/* exported SatexEngine */
function SatexEngine(createSatex, progress = () => {}, store = null, status = () => {}) {
  const files = new Map();
  let wasm = null;
  /** The cache directory outlives the page in `store` (the worker's is IndexedDB), under the
   * binary's name: what one satex build cached, another does not read. `kept` is what the
   * store holds of `files`, `imported` the bundles whose caches it holds. */
  let space = '';
  const kept = new Map(), imported = new Set();
  const cached = (path) => path.startsWith('/cache/') && !path.endsWith('.lock');
  async function keep() {
    if (!store) return;
    const fresh = [...files].filter(([path, data]) => cached(path) && kept.get(path) !== data);
    for (const [path, data] of fresh) kept.set(path, data);
    if (fresh.length) await store.put(space, fresh).catch(() => {});
  }

  /** `url`'s response, its body counted into `progress(label, loaded, total)` as it
   * arrives: `total` from the manifest, since a compressed transfer's length is not
   * the length the body has once the browser decoded it. */
  async function fetched(url, label, total) {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    if (!response.body) return response;
    const size = total || Number(response.headers.get('Content-Length')) || 0;
    let loaded = 0;
    progress(label, 0, size);
    const counted = response.body.pipeThrough(new TransformStream({
      transform(chunk, out) { loaded += chunk.byteLength; progress(label, loaded, size); out.enqueue(chunk); },
    }));
    return new Response(counted, { headers: response.headers });
  }

  async function bytes(url, label = url, total = 0) {
    if (url instanceof Uint8Array) return url;
    return new Uint8Array(await (await fetched(url, label, total)).arrayBuffer());
  }

  async function gunzip(data) {
    if (data[0] !== 0x1f || data[1] !== 0x8b) return data; // the server already decoded it
    const stream = new Blob([data]).stream().pipeThrough(new DecompressionStream('gzip'));
    return new Uint8Array(await new Response(stream).arrayBuffer());
  }

  /** The binary, compiled while it downloads. */
  async function compile(url, total) {
    if (url instanceof Uint8Array) return WebAssembly.compile(url);
    try {
      return await WebAssembly.compileStreaming(fetched(url, 'satex', total));
    } catch {
      return WebAssembly.compile(await bytes(url, 'satex', total)); // served without application/wasm
    }
  }

  /** The files an instance wrote to `dir` since it booted (`booted`: their stamps then). */
  function walk(FS, dir, into, booted = new Map()) {
    for (const name of FS.readdir(dir)) {
      if (name === '.' || name === '..') continue;
      const path = `${dir}/${name}`, st = FS.stat(path);
      if (FS.isDir(st.mode)) walk(FS, path, into, booted);
      else if (cached(path) && booted.get(path) !== stamp(st)) { into.set(path, FS.readFile(path)); booted.set(path, stamp(st)); }
    }
  }
  const stamp = (st) => `${+st.mtime}|${st.size}`;

  /** A fresh instance over the bundle, `edits` laid over it, in `cwd`. */
  async function boot(lines, edits = {}, cwd = '/work', onStatus = undefined) {
    const module = await createSatex({
      onStatus,
      instantiateWasm(imports, done) {
        WebAssembly.instantiate(wasm, imports).then((instance) => done(instance, wasm));
        return {};
      },
      print: (text) => lines.push(['out', text]),
      printErr: (text) => lines.push(['err', text]),
      preRun: [(m) => { m.ENV.SATEX_CACHE = '/cache'; m.ENV.HOME = '/home'; }],
      quit: (status, toThrow) => { throw toThrow; },
    });
    for (const [path, data] of [...files, ...Object.entries(edits)]) {
      module.FS.mkdirTree(path.slice(0, path.lastIndexOf('/')) || '/');
      module.FS.writeFile(path, data);
    }
    module.FS.mkdirTree('/cache');
    module.FS.chdir(cwd);
    module.booted = new Map();
    for (const path of files.keys()) if (cached(path)) module.booted.set(path, stamp(module.FS.stat(path)));
    return module;
  }

  /** One satex invocation in a fresh instance: the binary exits like a process. */
  async function run(args, edits = {}, cwd = '/work') {
    const lines = [];
    const started = performance.now();
    const module = await boot(lines, edits, cwd);
    let status;
    try {
      status = module.callMain(args);
    } catch (e) {
      if (e && e.name === 'ExitStatus') status = e.status;
      else { lines.push(['err', `satex stopped: ${e}`]); status = 'aborted'; }
    }
    walk(module.FS, '/cache', files, module.booted);
    await keep();
    return { lines, status, ms: Math.round(performance.now() - started) };
  }

  /** `satex lsp` in an instance that stays alive: each JSON-RPC message in, the replies out.
   * `reset` drops it: the next message starts one with these flags, in this directory. */
  let server = null, serverArgs = [], serverCwd = '/work', serverLines = [];
  function reset(args = [], cwd = '/work') { server = null; serverArgs = args; serverCwd = cwd; }
  async function lsp(message) {
    if (!server) {
      // What the engine is doing while a message runs (src/lsp/wasm.rs).
      const onStatus = (json) => status(JSON.parse(json));
      serverLines = [];
      server = await boot(serverLines, {}, serverCwd, onStatus);
      server.callMain([...serverArgs, 'lsp']); // returns with the runtime alive, see src/lsp/wasm.rs
    }
    const running = server, text = running.stringToNewUTF8(JSON.stringify(message));
    let out;
    try {
      out = JSON.parse(running.UTF8ToString(running._satex_lsp(text)));
    } catch (e) {
      // an abort: what the server said last, its panic among it
      e.message += serverLines.filter((l) => l[0] === 'err').slice(-8).map((l) => `\n${l[1]}`).join('');
      server = null;
      throw e;
    } finally {
      try { running._free(text); } catch {} // an aborted module takes no more calls
    }
    // an analysis may have cached a class or package
    if (out.some((m) => m.method === 'textDocument/publishDiagnostics')) {
      walk(server.FS, '/cache', files, server.booted);
      await keep();
    }
    return out;
  }

  /** A bundle, a kernel's or a package's: its files join the tree, a kernel's caches
   * the cache directory. `fetching` is its download, when already under way. */
  async function bundle(base, entry, given, fetching) {
    const packed = await (fetching ?? bytes(given ?? base + entry.bundle, entry.label, entry.size));
    progress('unpacking', 0, 0);
    const blob = await gunzip(packed);
    let offset = 0;
    for (const [path, size] of entry.files) {
      files.set(path, blob.subarray(offset, offset + size));
      offset += size;
    }
    // a kernel's caches, imported once: the store keeps them for the next load
    const archive = `/kernels/${entry.id}/satex-cache.bin`;
    if (!files.has(archive)) return { lines: [], status: 0, ms: 0 };
    if (imported.has(entry.id)) { files.delete(archive); return { lines: [], status: 0, ms: 0 }; }
    progress('importing', 0, 0);
    const done = await run(['cache', 'import', `/kernels/${entry.id}`], {}, '/work');
    files.delete(archive);
    imported.add(entry.id);
    await store?.put(space, [[`#imported/${entry.id}`, new Uint8Array()]]).catch(() => {});
    return done;
  }

  /** Several bundles: all downloads start at once, the files and caches join the tree in order. */
  async function bundles(base, entries, given = []) {
    const downloads = entries.map((entry, i) => given[i] ?? bytes(base + entry.bundle, entry.label, entry.size));
    const done = { lines: [], status: 0, ms: 0 };
    for (const [i, entry] of entries.entries()) {
      const one = await bundle(base, entry, null, downloads[i]);
      done.lines.push(...one.lines);
      done.ms += one.ms;
    }
    return done;
  }

  /** Files under `dir`: a new project's replace the last one's, added ones join them,
   * `removed` ones leave. A running language server sees the change at once. */
  function project(dir, given, replace = false, removed = []) {
    const live = (f) => { if (server) try { f(server.FS); } catch {} };
    if (replace) for (const path of [...files.keys()]) if (path.startsWith(dir + '/')) files.delete(path);
    for (const name of removed) { files.delete(`${dir}/${name}`); live((fs) => fs.unlink(`${dir}/${name}`)); }
    for (const [name, data] of Object.entries(given)) {
      const path = `${dir}/${name}`;
      files.set(path, data);
      live((fs) => { fs.mkdirTree(path.slice(0, path.lastIndexOf('/'))); fs.writeFile(path, data); });
    }
  }

  /** A file of the tree by its path, or the first whose name is `path`'s last part. */
  function read(path) {
    const name = path.split('/').pop();
    const found = files.has(path) ? path : [...files.keys()].find((p) => p.endsWith(`/${name}`));
    return { path: found ?? path, data: found ? files.get(found).slice() : null };
  }

  /** The cache entries, by path and size. */
  const caches = () => [...files].filter(([path]) => cached(path)).map(([path, data]) => ({ path, size: data.length }));
  /** Drops cache entries, everywhere they are kept; a bundle's are imported again on the next load. */
  async function uncache(paths) {
    for (const path of paths) {
      files.delete(path);
      kept.delete(path);
      if (server) try { server.FS.unlink(path); } catch {}
    }
    const markers = [...imported].map((id) => `#imported/${id}`);
    imported.clear();
    await store?.remove(space, [...paths, ...markers]).catch(() => {});
  }

  /** Fetches the binary and the default kernel, unless `given` has them. */
  async function init(base, manifest, given = {}) {
    space = manifest.wasm;
    for (const [path, data] of (await store?.load(space).catch(() => null)) ?? []) {
      if (path.startsWith('#imported/')) imported.add(path.slice('#imported/'.length));
      else { files.set(path, data); kept.set(path, data); }
    }
    // Both downloads at once: the binary compiles while the TeX files arrive.
    const entry = manifest.kernels[0];
    const fetching = given.bundle ?? bytes(base + entry.bundle, entry.label, entry.size);
    wasm = await compile(given.wasm ?? base + manifest.wasm, manifest.wasmSize);
    const kernel = await bundle(base, entry, null, fetching);
    const decoder = new TextDecoder();
    const shown = [...new Set([...manifest.samples, ...Object.values(manifest.projects ?? {}).flat()])];
    const samples = Object.fromEntries(shown.map((name) => [name, decoder.decode(files.get(`/work/${name}`))]));
    return { samples, imported: kernel };
  }

  return { init, run, lsp, reset, bundle, bundles, project, read, caches, uncache };
}

if (typeof module === 'object' && module.exports) {
  module.exports = SatexEngine;
} else if (typeof importScripts === 'function') {
  let engine = null;
  /** The cache directory in IndexedDB: one record per file, keyed [binary, path]; a load drops
   * what other binaries left. Without IndexedDB (a private window) nothing is kept. */
  const idb = () => {
    const opened = new Promise((resolve, reject) => {
      const r = indexedDB.open('satex-playground', 1);
      r.onupgradeneeded = () => r.result.createObjectStore('cache');
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
    });
    const tx = async (mode, f) => {
      const t = (await opened).transaction('cache', mode), out = f(t.objectStore('cache'));
      return new Promise((resolve, reject) => { t.oncomplete = () => resolve(out); t.onerror = t.onabort = () => reject(t.error); });
    };
    return {
      async load(space) {
        const found = [];
        await tx('readwrite', (s) => s.openCursor().onsuccess = ({ target: { result: c } }) => {
          if (!c) return;
          if (c.key[0] === space) found.push([c.key[1], c.value]); else c.delete();
          c.continue();
        });
        return found;
      },
      put: (space, entries) => tx('readwrite', (s) => { for (const [path, data] of entries) s.put(data, [space, path]); }),
      remove: (space, paths) => tx('readwrite', (s) => { for (const path of paths) s.delete([space, path]); }),
    };
  };
  self.onmessage = async ({ data }) => {
    try {
      if (data.type === 'init') {
        const given = data.given ?? {};
        importScripts(given.js ? URL.createObjectURL(new Blob([given.js], { type: 'text/javascript' })) : data.base + data.manifest.js);
        // At most every 100 ms, and every phase change.
        let last = 0, phase = '';
        engine = SatexEngine(self.createSatex, (label, loaded, total) => {
          const now = performance.now();
          if (label === phase && now - last < 100 && loaded < total) return;
          [last, phase] = [now, label];
          self.postMessage({ type: 'progress', label, loaded, total });
        }, typeof indexedDB === 'object' ? idb() : null, (s) => self.postMessage({ type: 'status', ...s }));
        self.postMessage({ type: 'ready', ...(await engine.init(data.base, data.manifest, given)) });
      } else if (data.type === 'bundle') {
        self.postMessage({ type: 'bundle', id: data.entry.id, ...(await engine.bundle(data.base, data.entry, data.given)) });
      } else if (data.type === 'bundles') {
        self.postMessage({ type: 'bundle', ids: data.entries.map((e) => e.id), ...(await engine.bundles(data.base, data.entries, data.given)) });
      } else if (data.type === 'project') {
        engine.project(data.dir, data.files ?? {}, data.replace, data.removed);
      } else if (data.type === 'read') {
        self.postMessage({ type: 'file', ...engine.read(data.path) });
      } else if (data.type === 'reset') {
        engine.reset(data.args, data.cwd);
      } else if (data.type === 'run') {
        const encode = (t) => new TextEncoder().encode(t);
        const edits = Object.fromEntries(Object.entries(data.edits).map(([path, text]) => [path, encode(text)]));
        self.postMessage({ type: 'result', ...(await engine.run(data.args, edits, data.cwd)) });
      } else if (data.type === 'caches') {
        self.postMessage({ type: 'caches', entries: engine.caches() });
      } else if (data.type === 'uncache') {
        await engine.uncache(data.paths);
        self.postMessage({ type: 'caches', entries: engine.caches() });
      } else if (data.type === 'lsp') {
        self.postMessage({ type: 'lsp', out: await engine.lsp(data.message) });
      }
    } catch (e) {
      self.postMessage({ type: 'error', of: data.type, message: String(e && e.stack || e) });
    }
  };
}
