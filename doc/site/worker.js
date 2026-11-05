// The playground's engine: the `satex` binary, built for Emscripten, run over
// an in-memory file system holding the TeX bundle. A Web Worker in the
// browser, started from the page's copy of this file; doc/site/test.mjs runs
// the same code in node.

/* exported SatexEngine */
/**
 * @param {any} createSatex
 * @param {(label: string, loaded: number, total: number) => void} [progress]
 * @param {any} [store]
 * @param {(status: any) => void} [status]
 */
function SatexEngine(createSatex, progress = () => {}, store = null, status = () => {}) {
  const STALL_MS = 30000, RETRIES = 4, BACKOFF_MS = 1000, PARALLEL = 4;
  const files = new Map();
  /** Each file's mtime, kept from boot to boot: a fresh instance's stamps (an index's, a
   * document cache's, the content hashes') must not see an unchanged file as new. */
  const times = new Map();
  /** A time no file had: two versions of one path never share a stamp. */
  let clock = 0;
  const now = () => (clock = Math.max(clock + 1, Date.now()));
  let wasm = null;
  /** The cache directory outlives the page in `store` (the worker's is IndexedDB), under the
   * binary's name: what one satex build cached, another does not read. `kept` is what the
   * store holds of `files`, `imported` the bundles whose caches it holds. */
  let space = '';
  const kept = new Map(), imported = new Set();
  let saving = Promise.resolve();
  const cached = (path) => path.startsWith('/cache/') && !path.endsWith('.lock');
  async function keep() {
    if (!store) return;
    const fresh = [...files].filter(([path, data]) => cached(path) && kept.get(path) !== data);
    for (const [path, data] of fresh) kept.set(path, data);
    // in the background: a reply never waits on IndexedDB
    if (fresh.length) saving = saving.then(() => store.put(space, fresh)).catch(() => {});
  }

  /** `url`'s response, its body counted into `progress(label, loaded, total)` as it
   * arrives: `total` from the manifest, since a compressed transfer's length is not
   * the length the body has once the browser decoded it. */
  async function fetched(url, label, total) {
    // a download that stalls (no byte for STALL_MS) fails, saying so, instead of waiting for good
    const stop = new AbortController();
    let stall = 0;
    const watch = () => { clearTimeout(stall); stall = setTimeout(() => stop.abort(new Error(`${label}: no data for ${STALL_MS / 1000} s`)), STALL_MS); };
    watch();
    // the server gone: what the service worker (or an earlier visit) kept
    const response = await fetch(url, { signal: stop.signal }).catch((e) => globalThis.caches?.match(url).then((kept) => kept ?? Promise.reject(e)))
      .catch((e) => { clearTimeout(stall); throw e; });
    if (!response.ok) { clearTimeout(stall); throw new Error(`${url}: ${response.status}`); }
    if (!response.body) { clearTimeout(stall); return response; }
    const size = total || Number(response.headers.get('Content-Length')) || 0;
    let loaded = 0;
    progress(label, 0, size);
    const counted = response.body.pipeThrough(new TransformStream({
      transform(chunk, out) { watch(); loaded += chunk.byteLength; progress(label, loaded, size); out.enqueue(chunk); },
      flush() { clearTimeout(stall); },
    }));
    return new Response(counted, { headers: response.headers });
  }

  async function bytes(url, label = url, total = 0) {
    if (url instanceof Uint8Array) return url;
    return new Uint8Array(await (await fetched(url, label, total)).arrayBuffer());
  }

  /** A bundle's files as one blob: the store's copy if sound, else downloaded, retried with backoff
   * and checked, then kept whole (one put, after the check: never a half-written bundle). */
  async function obtain(base, entry, given) {
    if (given) return checked(entry, given);
    const kept = await store?.bundle?.(entry.bundle).catch(() => null);
    if (kept) {
      const blob = await checked(entry, kept).catch(() => null);
      if (blob) { progress(entry.label, blob.length, blob.length); return blob; }
      store.unbundle?.(entry.bundle).catch(() => {});
    }
    for (let attempt = 1; ; attempt++) {
      try {
        const data = await slot(() => bytes(base + entry.bundle, entry.label, entry.size)), blob = await checked(entry, data);
        store?.keepBundle?.(entry.bundle, data).catch(() => {});
        return blob;
      } catch (e) {
        // a 404 is a newer site, not a bad connection: no retry helps
        if (attempt >= RETRIES || /: 404$/.test(e.message)) throw e;
        await new Promise((r) => setTimeout(r, BACKOFF_MS * 2 ** (attempt - 1)));
      }
    }
  }
  /** At most PARALLEL downloads at once: bundles land one after another, so a reload mid-download finds the landed ones kept. */
  let open_ = 0;
  const queue = [];
  async function slot(f) {
    if (open_ >= PARALLEL) await new Promise((r) => queue.push(r));
    open_++;
    try { return await f(); } finally { open_--; queue.shift()?.(); }
  }
  /** The blob of `data`, a bundle as fetched; an error unless it is whole and the one its name's digest says. */
  async function checked(entry, data) {
    const digest = entry.bundle.match(/-([0-9a-f]{12})\.[\w.]+$/)?.[1];
    if (digest && data[0] === 0x1f && data[1] === 0x8b && globalThis.crypto?.subtle) {
      const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', data))].slice(0, 6).map((b) => b.toString(16).padStart(2, '0')).join('');
      if (hash !== digest) throw new Error(`${entry.label}: damaged download (digest ${hash}, not ${digest})`);
    }
    const blob = await gunzip(data), size = entry.files.reduce((n, [, z]) => n + z, 0);
    if (blob.length !== size) throw new Error(`${entry.label}: damaged download (${blob.length} of ${size} bytes)`);
    return blob;
  }

  async function gunzip(data) {
    if (data[0] !== 0x1f || data[1] !== 0x8b) return data; // the server already decoded it
    const stream = new Blob([data]).stream().pipeThrough(new DecompressionStream('gzip'));
    return new Uint8Array(await new Response(stream).arrayBuffer());
  }

  /** The binary, compiled while it downloads. */
  async function compile(url, total) {
    if (url instanceof Uint8Array) return WebAssembly.compile(/** @type {BufferSource} */ (url));
    try {
      return await WebAssembly.compileStreaming(fetched(url, 'satex', total));
    } catch {
      return WebAssembly.compile(/** @type {BufferSource} */ (await bytes(url, 'satex', total))); // served without application/wasm
    }
  }

  /** The files an instance wrote to `dir` since it booted (`booted`: their stamps then). */
  function walk(FS, dir, into, booted = new Map()) {
    for (const name of FS.readdir(dir)) {
      if (name === '.' || name === '..') continue;
      const path = `${dir}/${name}`, st = FS.stat(path);
      if (FS.isDir(st.mode)) walk(FS, path, into, booted);
      else if (cached(path) && booted.get(path) !== stamp(st)) { into.set(path, FS.readFile(path)); times.set(path, (clock = Math.max(clock, +st.mtime))); booted.set(path, stamp(st)); }
    }
  }
  const stamp = (st) => `${+st.mtime}|${st.size}`;

  /** A fresh instance over the bundle, `edits` laid over it, in `cwd`. */
  async function boot(lines, edits = {}, cwd = '/work', onStatus = undefined) {
    const module = await createSatex({
      onStatus,
      cancelled: () => stale(),
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
      // ctime too: satex's stamps read it (src/caches/format.rs `stamp`)
      const t = path in edits ? now() : times.get(path);
      if (t) module.FS.doSetAttr(null, module.FS.lookupPath(path).node, { atime: t, mtime: t, ctime: t });
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
  /** Whether the page no longer wants the run under way: src/lsp/wasm.rs asks while it runs. */
  let stale = () => false;
  function reset(args = [], cwd = '/work') { server = null; serverArgs = args; serverCwd = cwd; }
  async function lsp(message, cancelled = () => false) {
    if (!server) {
      // What the engine is doing while a message runs (src/lsp/wasm.rs).
      const onStatus = (json) => status(JSON.parse(json));
      serverLines = [];
      server = await boot(serverLines, {}, serverCwd, onStatus);
      server.callMain([...serverArgs, 'lsp']); // returns with the runtime alive, see src/lsp/wasm.rs
    }
    const running = server, text = running.stringToNewUTF8(JSON.stringify(message));
    let out;
    stale = cancelled;
    try {
      out = JSON.parse(running.UTF8ToString(running._satex_lsp(text)));
    } catch (e) {
      // an abort: what the server said last, its panic among it
      e.message += serverLines.filter((l) => l[0] === 'err').slice(-8).map((l) => `\n${l[1]}`).join('');
      server = null;
      throw e;
    } finally {
      stale = () => false;
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
    const archive = await unpack(entry, fetching ?? obtain(base, entry, given));
    return archive ? cacheImport([archive]) : { lines: [], status: 0, ms: 0 };
  }
  async function unpack(entry, fetching) {
    const blob = await fetching;
    progress('unpacking', 0, 0);
    let offset = 0;
    // a bundle's name holds its digest: its files' time, the same on every visit
    let t = 0;
    for (const c of entry.bundle) t = (t * 31 + c.charCodeAt(0)) % 1e9;
    for (const [path, size] of entry.files) {
      files.set(path, blob.subarray(offset, offset + size));
      times.set(path, 1e11 + t); // before any `now()`
      offset += size;
    }
    live(entry.files.map(([path]) => path));
    // a kernel's caches, imported once: the store keeps them for the next load
    const archive = `/kernels/${entry.id}/satex-cache.bin`;
    if (!files.has(archive)) return null;
    if (imported.has(entry.id)) { files.delete(archive); return null; }
    return entry.id;
  }
  /** The caches of these bundles, imported in one run: each run boots an instance with the whole tree. */
  async function cacheImport(ids) {
    progress('importing', 0, 0);
    const before = new Map(files);
    const done = await run(['cache', 'import', ...ids.map((id) => `/kernels/${id}`)], {}, '/work');
    live([...files.keys()].filter((path) => before.get(path) !== files.get(path)));
    for (const id of ids) { files.delete(`/kernels/${id}/satex-cache.bin`); imported.add(id); }
    await store?.put(space, ids.map((id) => [`#imported/${id}`, new Uint8Array()])).catch(() => {});
    return done;
  }

  /** Several bundles: all downloads start at once, the files and caches join the tree in order. */
  async function bundles(base, entries, given = []) {
    const downloads = entries.map((entry, i) => obtain(base, entry, given[i]));
    downloads.forEach((d) => d.catch(() => {})); // one failing fails the call, the others are not left unhandled
    const ids = [];
    for (const [i, entry] of entries.entries()) {
      const id = await unpack(entry, downloads[i]);
      if (id) ids.push(id);
    }
    return ids.length ? cacheImport(ids) : { lines: [], status: 0, ms: 0 };
  }

  /** Files a load added, written into a running language server too: it finds them without a restart
   * (the index lists every bundle's files, a lookup asks whether one is there), and a checkpoint
   * that probed one for absent is no longer reused. */
  function live(paths) {
    if (!server) return;
    for (const path of paths) {
      if (!files.has(path)) continue;
      try {
        server.FS.mkdirTree(path.slice(0, path.lastIndexOf('/')) || '/');
        server.FS.writeFile(path, files.get(path));
        const t = times.get(path);
        if (t) server.FS.doSetAttr(null, server.FS.lookupPath(path).node, { atime: t, mtime: t, ctime: t });
        if (cached(path)) server.booted.set(path, stamp(server.FS.stat(path)));
      } catch {}
    }
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
      times.set(path, now());
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
    saving = saving.then(() => store?.remove(space, [...paths, ...markers])).catch(() => {});
  }

  /** Fetches the binary and the default kernel, unless `given` has them. */
  async function init(base, manifest, given = {}) {
    space = manifest.wasm;
    // the kept caches are a head start, never a wait
    const loading = store?.load(space).catch(() => null);
    const kept_ = await Promise.race([loading, new Promise((r) => setTimeout(() => r('late'), 3000))]);
    if (kept_ === 'late') status({ text: 'the browser cache is busy (another tab?): starting without it' });
    for (const [path, data] of (kept_ === 'late' ? null : kept_) ?? []) {
      if (path.startsWith('#imported/')) imported.add(path.slice('#imported/'.length));
      else { files.set(path, data); kept.set(path, data); times.set(path, now()); }
    }
    // Both downloads at once: the binary compiles while the TeX files arrive.
    const entry = manifest.kernels[0];
    const fetching = obtain(base, entry, given.bundle);
    fetching.catch(() => {});
    // the bundles this site no longer ships leave the store
    store?.pruneBundles?.(new Set([...manifest.kernels, ...manifest.packages ?? []].map((e) => e.bundle))).catch(() => {});
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
  const CAP = 150 * 2 ** 20; // bytes kept between visits
  const idb = () => {
    const opened = new Promise((resolve, reject) => {
      const r = indexedDB.open('satex-playground', 3);
      r.onupgradeneeded = () => { for (const n of ['cache', 'used', 'bundles']) if (!r.result.objectStoreNames.contains(n)) r.result.createObjectStore(n); };
      // an older tab yields to a newer version; a blocked open means no cache
      r.onsuccess = () => { r.result.onversionchange = () => r.result.close(); resolve(r.result); };
      r.onblocked = () => reject(new Error('the cache store is held by another tab'));
      r.onerror = () => reject(r.error);
    });
    const tx = async (mode, f, names = ['cache', 'used']) => {
      const t = (await opened).transaction(names, mode), out = f(...names.map((n) => t.objectStore(n)));
      return new Promise((resolve, reject) => { t.oncomplete = () => resolve(out); t.onerror = t.onabort = () => reject(t.error); });
    };
    /** Bundles by file name (it holds their digest), apart from the cache directory and its cap. */
    const bundles = (mode, f) => tx(mode, f, ['bundles']);
    return {
      async load(space) {
        const range = IDBKeyRange.bound([space], [space, []]);
        const [keys, values, used] = await tx('readonly', (st, u) => [st.getAllKeys(range), st.getAll(range), u.getAll(range)]);
        const found = keys.result.map((k, i) => [k[1], values.result[i], used.result[i] ?? 0]);
        // background: drop other binaries' records, then evict past CAP
        let total = found.reduce((n, [, d]) => n + (d?.byteLength ?? 0), 0);
        const evict = [...found].sort((a, b) => a[2] - b[2]).filter(([, d]) => total > CAP && (total -= d?.byteLength ?? 0) >= 0).map(([p]) => p);
        tx('readwrite', (st, u) => {
          for (const store of [st, u]) {
            store.delete(IDBKeyRange.upperBound([space], true));
            store.delete(IDBKeyRange.lowerBound([space, []], true));
            for (const p of evict) store.delete([space, p]);
          }
        }).catch(() => {});
        return found.filter(([p]) => !evict.includes(p)).map(([p, d]) => [p, d]);
      },
      put: (space, entries) => tx('readwrite', (s, u) => { for (const [path, data] of entries) { s.put(data, [space, path]); u.put(Date.now(), [space, path]); } }),
      remove: (space, paths) => tx('readwrite', (s, u) => { for (const path of paths) { s.delete([space, path]); u.delete([space, path]); } }),
      bundle: async (name) => (await bundles('readonly', (b) => b.get(name))).result ?? null,
      keepBundle: (name, data) => bundles('readwrite', (b) => b.put(data, name)),
      unbundle: (name) => bundles('readwrite', (b) => b.delete(name)),
      pruneBundles: async (keep) => {
        const names = (await bundles('readonly', (b) => b.getAllKeys())).result;
        if (names.some((n) => !keep.has(n))) await bundles('readwrite', (b) => { for (const n of names) if (!keep.has(n)) b.delete(n); });
      },
    };
  };
  /** Messages are answered in the order they came (the page pairs each reply with its request by that order):
   * one at a time, except downloads, which the page sends one at a time itself. Two `lsp` messages after a
   * `reset` would otherwise each start a server of their own. */
  let serial = Promise.resolve();
  /** The page's count of texts sent (an Int32Array on a SharedArrayBuffer), when the page is cross-origin isolated. */
  let cancelFlag = null;
  self.onmessage = ({ data }) => {
    if (['init', 'bundle', 'bundles'].includes(data.type)) handle(data);
    else serial = serial.then(() => handle(data));
  };
  const handle = async (data) => {
    try {
      if (data.type === 'init') {
        const given = data.given ?? {};
        cancelFlag = data.cancel ?? null;
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
        // the page's shared counter of texts sent: a run of fewer stops once a newer one came
        const flag = cancelFlag, sent = data.sent;
        self.postMessage({ type: 'lsp', out: await engine.lsp(data.message, flag && sent != null ? () => Atomics.load(flag, 0) > sent : undefined) });
      }
    } catch (e) {
      self.postMessage({ type: 'error', of: data.type, message: e instanceof Error ? `${e.message}\n${e.stack}` : String(e) });
    }
  };
}
