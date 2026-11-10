// The playground's engine: the `satex` binary, built for Emscripten, run over
// an in-memory file system holding the TeX bundle. A Web Worker in the
// browser, started from the page's copy of this file; doc/site/test.mjs runs
// the same code in node.

/** A bundle of the manifest: its file, label, size, the files it holds (path, size) and its kernel id. */
interface Entry { bundle: string; label: string; size: number; id: string; files: [string, number][] }
/** The cache directory outside the page's memory (the worker's is IndexedDB). */
interface Store {
  load(space: string): Promise<[string, Uint8Array][]>; put(space: string, entries: [string, Uint8Array][]): Promise<unknown>; remove(space: string, paths: string[]): Promise<unknown>;
  bundle?(name: string): Promise<Uint8Array | null>; keepBundle?(name: string, data: Uint8Array): Promise<unknown>; unbundle?(name: string): Promise<unknown>;
  pruneBundles?(keep: Set<string>): Promise<unknown>;
}
/** An Emscripten instance of the satex binary, as far as used here. */
interface Mod {
  FS: Rec; ENV: Rec; callMain(args: string[]): number; booted: Map<string, string>; stringToNewUTF8(s: string): number; UTF8ToString(p: number): string;
  _satex_lsp(p: number): number; _free(p: number): void;
}
/** What a run answers. */
interface RunResult { lines: [string, string][]; status: number | string; ms: number }
/** The engine's calls (SatexEngine's and LocalEngine's alike). */
interface Engine {
  init(base?: string, manifest?: Rec, given?: Rec): Promise<Rec>; run(args: string[], edits?: Record<string, any>, cwd?: string): Promise<RunResult>;
  lsp(message: Rec, cancelled?: () => boolean): Promise<Rec[]>; reset(args?: string[], cwd?: string): unknown;
  bundle(base: string, entry: Entry, given?: Uint8Array | null, fetching?: Promise<Uint8Array>): Promise<RunResult>;
  bundles(base: string, entries: Entry[], given?: (Uint8Array | undefined)[]): Promise<RunResult>;
  project(dir: string, given: Record<string, any>, replace?: boolean, removed?: string[]): unknown;
  read(path: string): Promise<{ path: string; data: Uint8Array | null }> | { path: string; data: Uint8Array | null };
  caches(): { path: string; size: number }[]; uncache(paths: string[]): Promise<unknown>;
}
function SatexEngine(createSatex: (o: Rec) => Promise<Mod>, progress: (label: string, loaded: number, total: number) => void = () => {}, store: Store | null = null, status: (s: any) => void = () => {}): Engine {
  const STALL_MS = 30000, RETRIES = 4, BACKOFF_MS = 1000, PARALLEL = 4;
  const files = new Map<string, Uint8Array>();
  /** Each file's mtime, kept from boot to boot: a fresh instance's stamps (an index's, a
   * document cache's, the content hashes') must not see an unchanged file as new. */
  const times = new Map<string, number>();
  /** A time no file had: two versions of one path never share a stamp. */
  let clock = 0;
  const now = () => (clock = Math.max(clock + 1, Date.now()));
  let wasm: WebAssembly.Module | null = null;
  /** The cache directory outlives the page in `store` (the worker's is IndexedDB), under the
   * binary's name: what one satex build cached, another does not read. `kept` is what the
   * store holds of `files`, `imported` the bundles whose caches it holds. */
  let space = '';
  const kept = new Map<string, Uint8Array>(), imported = new Set<string>();
  let saving: Promise<unknown> = Promise.resolve();
  const cached = (path: string) => path.startsWith('/cache/') && !path.endsWith('.lock');
  async function keep() {
    if (!store) return;
    const fresh = [...files].filter(([path, data]) => cached(path) && kept.get(path) !== data);
    for (const [path, data] of fresh) kept.set(path, data);
    // in the background: a reply never waits on IndexedDB
    if (fresh.length) saving = saving.then(() => store!.put(space, fresh)).catch(() => {});
  }

  /** `url`'s response, its body counted into `progress(label, loaded, total)` as it
   * arrives: `total` from the manifest, since a compressed transfer's length is not
   * the length the body has once the browser decoded it. */
  async function fetched(url: string, label: string, total: number) {
    // a download that stalls (no byte for STALL_MS) fails, saying so, instead of waiting for good
    const stop = new AbortController();
    let stall: ReturnType<typeof setTimeout> | 0 = 0;
    const watch = () => { clearTimeout(stall || undefined); stall = setTimeout(() => stop.abort(new Error(`${label}: no data for ${STALL_MS / 1000} s`)), STALL_MS); };
    watch();
    // the server gone: what the service worker (or an earlier visit) kept
    const response = await fetch(url, { signal: stop.signal }).catch((e: unknown) => globalThis.caches?.match(url).then((kept) => kept ?? Promise.reject(e)))
      .catch((e) => { clearTimeout(stall || undefined); throw e; });
    if (!response.ok) { clearTimeout(stall || undefined); throw new Error(`${url}: ${response.status}`); }
    if (!response.body) { clearTimeout(stall || undefined); return response; }
    const size = total || Number(response.headers.get('Content-Length')) || 0;
    let loaded = 0;
    progress(label, 0, size);
    const counted = response.body.pipeThrough(new TransformStream({
      transform(chunk, out) { watch(); loaded += chunk.byteLength; progress(label, loaded, size); out.enqueue(chunk); },
      flush() { clearTimeout(stall || undefined); },
    }));
    return new Response(counted, { headers: response.headers });
  }

  async function bytes(url: string | Uint8Array, label = String(url), total = 0): Promise<Uint8Array> {
    if (url instanceof Uint8Array) return url;
    return new Uint8Array(await (await fetched(url, label, total)).arrayBuffer());
  }

  /** A bundle's files as one blob: the store's copy if sound, else downloaded, retried with backoff
   * and checked, then kept whole (one put, after the check: never a half-written bundle). */
  async function obtain(base: string, entry: Entry, given?: Uint8Array | null): Promise<Uint8Array> {
    if (given) return checked(entry, given);
    const kept = await store?.bundle?.(entry.bundle).catch(() => null);
    if (kept) {
      const blob = await checked(entry, kept).catch(() => null);
      if (blob) { progress(entry.label, blob.length, blob.length); return blob; }
      store!.unbundle?.(entry.bundle).catch(() => {});
    }
    for (let attempt = 1; ; attempt++) {
      try {
        const data = await slot(() => bytes(base + entry.bundle, entry.label, entry.size)), blob = await checked(entry, data);
        store?.keepBundle?.(entry.bundle, data).catch(() => {});
        return blob;
      } catch (e) {
        // a 404 is a newer site, not a bad connection: no retry helps
        if (attempt >= RETRIES || /: 404$/.test((e as Error).message)) throw e;
        await new Promise((r) => setTimeout(r, BACKOFF_MS * 2 ** (attempt - 1)));
      }
    }
  }
  /** At most PARALLEL downloads at once: bundles land one after another, so a reload mid-download finds the landed ones kept. */
  let open_ = 0;
  const queue: (() => void)[] = [];
  async function slot<T>(f: () => Promise<T>): Promise<T> {
    if (open_ >= PARALLEL) await new Promise<void>((r) => queue.push(r));
    open_++;
    try { return await f(); } finally { open_--; queue.shift()?.(); }
  }
  /** The blob of `data`, a bundle as fetched; an error unless it is whole and the one its name's digest says. */
  async function checked(entry: Entry, data: Uint8Array): Promise<Uint8Array> {
    const digest = entry.bundle.match(/-([0-9a-f]{12})\.[\w.]+$/)?.[1];
    if (digest && data[0] === 0x1f && data[1] === 0x8b && globalThis.crypto?.subtle) {
      const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', data as BufferSource))].slice(0, 6).map((b) => b.toString(16).padStart(2, '0')).join('');
      if (hash !== digest) throw new Error(`${entry.label}: damaged download (digest ${hash}, not ${digest})`);
    }
    const blob = await gunzip(data), size = entry.files.reduce((n, [, z]) => n + z, 0);
    if (blob.length !== size) throw new Error(`${entry.label}: damaged download (${blob.length} of ${size} bytes)`);
    return blob;
  }

  async function gunzip(data: Uint8Array): Promise<Uint8Array> {
    if (data[0] !== 0x1f || data[1] !== 0x8b) return data; // the server already decoded it
    const stream = new Blob([data as BlobPart]).stream().pipeThrough(new DecompressionStream('gzip'));
    return new Uint8Array(await new Response(stream).arrayBuffer());
  }

  /** The binary, compiled while it downloads. */
  async function compile(url: string | Uint8Array, total: number): Promise<WebAssembly.Module> {
    if (url instanceof Uint8Array) return WebAssembly.compile(url as BufferSource);
    try {
      return await WebAssembly.compileStreaming(fetched(url as string, 'satex', total));
    } catch {
      return WebAssembly.compile((await bytes(url, 'satex', total)) as BufferSource); // served without application/wasm
    }
  }

  /** The files an instance wrote to `dir` since it booted (`booted`: their stamps then). */
  function walk(FS: Rec, dir: string, into: Map<string, Uint8Array>, booted = new Map<string, string>()) {
    for (const name of FS.readdir(dir)) {
      if (name === '.' || name === '..') continue;
      const path = `${dir}/${name}`, st = FS.stat(path);
      if (FS.isDir(st.mode)) walk(FS, path, into, booted);
      else if (cached(path) && booted.get(path) !== stamp(st)) { into.set(path, FS.readFile(path)); times.set(path, (clock = Math.max(clock, +st.mtime))); booted.set(path, stamp(st)); }
    }
  }
  const stamp = (st: Rec) => `${+st.mtime}|${st.size}`;

  /** A fresh instance over the bundle, `edits` laid over it, in `cwd`. */
  async function boot(lines: [string, string][], edits: Record<string, any> = {}, cwd = '/work', onStatus?: (json: string) => void): Promise<Mod> {
    const module = await createSatex({
      onStatus,
      cancelled: () => stale(),
      instantiateWasm(imports: WebAssembly.Imports, done: (i: WebAssembly.Instance, m: WebAssembly.Module) => void) {
        WebAssembly.instantiate(wasm!, imports).then((instance) => done(instance, wasm!));
        return {};
      },
      print: (text: string) => lines.push(['out', text]),
      printErr: (text: string) => lines.push(['err', text]),
      preRun: [(m: Mod) => { m.ENV.SATEX_CACHE = '/cache'; m.ENV.HOME = '/home'; }],
      quit: (_status: number, toThrow: unknown) => { throw toThrow; },
    });
    for (const [path, data] of [...files, ...Object.entries(edits)] as [string, Uint8Array][]) {
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
  async function run(args: string[], edits: Record<string, any> = {}, cwd = '/work'): Promise<RunResult> {
    const lines: [string, string][] = [];
    const started = performance.now();
    const module = await boot(lines, edits, cwd);
    let status: number | string;
    try {
      status = module.callMain(args);
    } catch (e) {
      if (e && (e as Rec).name === 'ExitStatus') status = (e as Rec).status;
      else { lines.push(['err', `satex stopped: ${e}`]); status = 'aborted'; }
    }
    walk(module.FS, '/cache', files, module.booted);
    await keep();
    return { lines, status, ms: Math.round(performance.now() - started) };
  }

  /** `satex lsp` in an instance that stays alive: each JSON-RPC message in, the replies out.
   * `reset` drops it: the next message starts one with these flags, in this directory. */
  let server: Mod | null = null, serverArgs: string[] = [], serverCwd = '/work', serverLines: [string, string][] = [];
  /** Whether the page no longer wants the run under way: src/lsp/wasm.rs asks while it runs. */
  let stale = () => false;
  function reset(args: string[] = [], cwd = '/work') { server = null; serverArgs = args; serverCwd = cwd; }
  async function lsp(message: Rec, cancelled: () => boolean = () => false): Promise<Rec[]> {
    if (!server) {
      // What the engine is doing while a message runs (src/lsp/wasm.rs).
      const onStatus = (json: string) => status(JSON.parse(json));
      serverLines = [];
      server = await boot(serverLines, {}, serverCwd, onStatus);
      server.callMain([...serverArgs, 'lsp']); // returns with the runtime alive, see src/lsp/wasm.rs
    }
    const running = server, text = running.stringToNewUTF8(JSON.stringify(message));
    let out: Rec[];
    stale = cancelled;
    try {
      out = JSON.parse(running.UTF8ToString(running._satex_lsp(text)));
    } catch (e) {
      // an abort: what the server said last, its panic among it
      (e as Error).message += serverLines.filter((l) => l[0] === 'err').slice(-8).map((l) => `\n${l[1]}`).join('');
      server = null;
      throw e;
    } finally {
      stale = () => false;
      try { running._free(text); } catch {} // an aborted module takes no more calls
    }
    // an analysis may have cached a class or package
    if (out.some((m) => m.method === 'textDocument/publishDiagnostics')) {
      walk(server!.FS, '/cache', files, server!.booted);
      await keep();
    }
    return out;
  }

  /** A bundle, a kernel's or a package's: its files join the tree, a kernel's caches
   * the cache directory. `fetching` is its download, when already under way. */
  async function bundle(base: string, entry: Entry, given?: Uint8Array | null, fetching?: Promise<Uint8Array>): Promise<RunResult> {
    const archive = await unpack(entry, fetching ?? obtain(base, entry, given));
    return archive ? cacheImport([archive]) : { lines: [], status: 0, ms: 0 };
  }
  async function unpack(entry: Entry, fetching: Promise<Uint8Array>): Promise<string | null> {
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
  async function cacheImport(ids: string[]): Promise<RunResult> {
    progress('importing', 0, 0);
    const before = new Map(files);
    const done = await run(['cache', 'import', ...ids.map((id) => `/kernels/${id}`)], {}, '/work');
    live([...files.keys()].filter((path) => before.get(path) !== files.get(path)));
    for (const id of ids) { files.delete(`/kernels/${id}/satex-cache.bin`); imported.add(id); }
    await store?.put(space, ids.map((id) => [`#imported/${id}`, new Uint8Array()])).catch(() => {});
    return done;
  }

  /** Several bundles: all downloads start at once, the files and caches join the tree in order. */
  async function bundles(base: string, entries: Entry[], given: (Uint8Array | undefined)[] = []): Promise<RunResult> {
    const downloads = entries.map((entry, i) => obtain(base, entry, given[i]));
    downloads.forEach((d) => d.catch(() => {})); // one failing fails the call, the others are not left unhandled
    const ids: string[] = [];
    for (const [i, entry] of entries.entries()) {
      const id = await unpack(entry, downloads[i]);
      if (id) ids.push(id);
    }
    return ids.length ? cacheImport(ids) : { lines: [], status: 0, ms: 0 };
  }

  /** Files a load added, written into a running language server too: it finds them without a restart
   * (the index lists every bundle's files, a lookup asks whether one is there), and a checkpoint
   * that probed one for absent is no longer reused. */
  function live(paths: string[]) {
    if (!server) return;
    for (const path of paths) {
      if (!files.has(path)) continue;
      try {
        server.FS.mkdirTree(path.slice(0, path.lastIndexOf('/')) || '/');
        server.FS.writeFile(path, files.get(path)!);
        const t = times.get(path);
        if (t) server.FS.doSetAttr(null, server.FS.lookupPath(path).node, { atime: t, mtime: t, ctime: t });
        if (cached(path)) server.booted.set(path, stamp(server.FS.stat(path)));
      } catch {}
    }
  }

  /** Files under `dir`: a new project's replace the last one's, added ones join them,
   * `removed` ones leave. A running language server sees the change at once. */
  function project(dir: string, given: Record<string, any>, replace = false, removed: string[] = []) {
    const live = (f: (fs: Rec) => void) => { if (server) try { f(server.FS); } catch {} };
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
  function read(path: string) {
    const name = path.split('/').pop();
    const found = files.has(path) ? path : [...files.keys()].find((p) => p.endsWith(`/${name}`));
    return { path: found ?? path, data: found ? files.get(found)!.slice() : null };
  }

  /** The cache entries, by path and size. */
  const caches = () => [...files].filter(([path]) => cached(path)).map(([path, data]) => ({ path, size: data.length }));
  /** Drops cache entries, everywhere they are kept; a bundle's are imported again on the next load. */
  async function uncache(paths: string[]) {
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
  async function init(base: string, manifest: Rec, given: Rec = {}) {
    space = manifest.wasm;
    // the kept caches are a head start, never a wait
    const loading = store?.load(space).catch(() => null);
    const kept_ = await Promise.race([loading, new Promise<'late'>((r) => setTimeout(() => r('late'), 3000))]);
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
    store?.pruneBundles?.(new Set([...manifest.kernels, ...manifest.packages ?? []].map((e: Entry) => e.bundle))).catch(() => {});
    wasm = await compile(given.wasm ?? base + manifest.wasm, manifest.wasmSize);
    const kernel = await bundle(base, entry, null, fetching);
    const decoder = new TextDecoder();
    const shown = [...new Set([...manifest.samples, ...Object.values(manifest.projects ?? {}).flat()])];
    const samples = Object.fromEntries(shown.map((name: string) => [name, decoder.decode(files.get(`/work/${name}`))]));
    return { samples, imported: kernel };
  }

  return { init, run, lsp, reset, bundle, bundles, project, read, caches, uncache };
}

/* exported LocalEngine */
/**
 * The engine under `satex gui` (src/cli/gui/mod.rs): SatexEngine's calls, answered by the local server
 * on this machine's files, TeX installation and native language server. Nothing is downloaded.
 * @param {string} base the site's URL
 * @param {(status: any) => void} [status]
 * @param {(url: string) => any} [hold] keeps a connection open while this engine lives (an EventSource)
 * @param {(message: any) => void} [note] a notification of a run that ended while the message's others go on
 */
function LocalEngine(base: string, status: (s: any) => void = () => {}, hold: (url: string) => unknown = (url) => new EventSource(url), note: (m: any) => void = () => {}) {
  const session = Math.random().toString(36).slice(2);
  const url = (path: string, query: Record<string, string> = {}) => `${base}__satex/${path}?${new URLSearchParams(query)}`;
  async function call(path: string, init: RequestInit = {}, query: Record<string, string> = {}) {
    const r = await fetch(url(path, query), init);
    if (!r.ok) throw new Error(`${path}: ${r.status}`);
    return r;
  }
  const post = (path: string, body: unknown, query: Record<string, string> = {}) => call(path, { method: 'POST', body: typeof body === 'string' ? body : JSON.stringify(body) }, query);
  const nothing = { lines: [], status: 0, ms: 0 };
  let dir = '', held: unknown = null;
  return {
    async init() {
      const local = await (await call('local')).json();
      dir = local.dir;
      return { samples: {}, imported: nothing, local };
    },
    async run(args: string[], edits: Record<string, any> = {}, cwd = dir) {
      for (const [path, text] of Object.entries(edits)) await call('file', { method: 'PUT', body: text }, { path }).catch(() => {});
      return (await post('run', { args, cwd })).json();
    },
    /** One message: the server streams its status lines, then the replies. A newer text stops the run, as in the wasm. */
    async lsp(message: Rec, cancelled: () => boolean = () => false) {
      // the server ends the session, and its language server, once this connection closes
      held ??= hold(url('session', { s: session }));
      const watch = setInterval(() => { if (cancelled()) { clearInterval(watch); post('lsp/cancel', '', { s: session }).catch(() => {}); } }, 50);
      try {
        const r = await post('lsp', message, { s: session });
        const reader = r.body!.pipeThrough(new TextDecoderStream()).getReader();
        for (let rest = ''; ;) {
          const { done, value } = await reader.read();
          if (done) break;
          const lines = (rest + value).split('\n');
          rest = lines.pop()!;
          for (const line of lines.filter(Boolean)) {
            const m = JSON.parse(line);
            if (Array.isArray(m)) return m;
            if (m.error) throw new Error(m.error);
            if (m.method === 'satex/status') status(m.params); else note(m);
          }
        }
        throw new Error('the language server stopped');
      } finally { clearInterval(watch); }
    },
    reset: (args: string[] = [], cwd = dir) => post('lsp/reset', { args, cwd }, { s: session }),
    bundle: async () => nothing,
    bundles: async () => nothing,
    /** Edits are saved to the project's files; nothing leaves it in bulk, only what `removed` names. */
    async project(at: string, given: Record<string, any>, replace = false, removed: string[] = []) {
      for (const name of removed) await call('file', { method: 'DELETE' }, { path: `${at}/${name}` }).catch(() => {});
      for (const [name, data] of Object.entries(given)) await call('file', { method: 'PUT', body: data }, { path: `${at}/${name}` }).catch(() => {});
    },
    async read(path: string) {
      const r = await call('file', {}, { path }).catch(() => null);
      return { path, data: r ? new Uint8Array(await r.arrayBuffer()) : null };
    },
    caches: () => [],
    uncache: async () => {},
  };
}

/** A question that only reads the last analysis (a completion asked ahead, the typeset text, the lenses): one still
 * waiting when a newer text comes, or another file is shown, is of a text or file the page no longer shows, and is answered null unasked, so
 * the newer text's analysis never waits behind it. */
const readOnly = (m: Rec) => m.id !== undefined && (/^textDocument\/(completion|hover|codeLens|documentColor|inlayHint|codeAction|documentSymbol|documentHighlight|documentLink|foldingRange|signatureHelp)$|^satex\/(typeset|written|caches)$/.test(m.method)
  || (m.method === 'workspace/executeCommand' && m.params?.command === 'satex/query'));

// under node (the tests) the caller leaves a `module` on the global object; esbuild would wrap a bare `module` as CommonJS
const cjs: { exports: any } | undefined = (globalThis as Rec).module;
if (typeof cjs === 'object' && cjs.exports) {
  cjs.exports = SatexEngine;
  cjs.exports.LocalEngine = LocalEngine;
  cjs.exports.readOnly = readOnly;
} else if (typeof importScripts === 'function') {
  let engine: Engine | null = null;
  /** The cache directory in IndexedDB: one record per file, keyed [binary, path]; a load drops
   * what other binaries left. Without IndexedDB (a private window) nothing is kept. */
  const CAP = 150 * 2 ** 20; // bytes kept between visits
  const idb = (): Store => {
    const opened = new Promise<IDBDatabase>((resolve, reject) => {
      const r = indexedDB.open('satex-playground', 3);
      r.onupgradeneeded = () => { for (const n of ['cache', 'used', 'bundles']) if (!r.result.objectStoreNames.contains(n)) r.result.createObjectStore(n); };
      // an older tab yields to a newer version; a blocked open means no cache
      r.onsuccess = () => { r.result.onversionchange = () => r.result.close(); resolve(r.result); };
      r.onblocked = () => reject(new Error('the cache store is held by another tab'));
      r.onerror = () => reject(r.error);
    });
    const tx = async <T,>(mode: IDBTransactionMode, f: (...s: IDBObjectStore[]) => T, names = ['cache', 'used']): Promise<T> => {
      const t = (await opened).transaction(names, mode), out = f(...names.map((n) => t.objectStore(n)));
      return new Promise<T>((resolve, reject) => { t.oncomplete = () => resolve(out); t.onerror = t.onabort = () => reject(t.error); });
    };
    /** Bundles by file name (it holds their digest), apart from the cache directory and its cap. */
    const bundles = <T,>(mode: IDBTransactionMode, f: (b: IDBObjectStore) => T) => tx(mode, f as (...s: IDBObjectStore[]) => T, ['bundles']);
    return {
      async load(space: string) {
        const range = IDBKeyRange.bound([space], [space, []]);
        const [keys, values, used] = (await tx('readonly', (st, u) => [st!.getAllKeys(range), st!.getAll(range), u!.getAll(range)])) as [IDBRequest<IDBValidKey[]>, IDBRequest<any[]>, IDBRequest<any[]>];
        const found = keys.result.map((k, i): [string, Uint8Array, number] => [(k as IDBValidKey[])[1] as string, values.result[i], used.result[i] ?? 0]);
        // background: drop other binaries' records, then evict past CAP
        let total = found.reduce((n, [, d]) => n + (d?.byteLength ?? 0), 0);
        const evict = [...found].sort((a, b) => a[2] - b[2]).filter(([, d]) => total > CAP && (total -= d?.byteLength ?? 0) >= 0).map(([p]) => p);
        tx('readwrite', (st, u) => {
          for (const store of [st!, u!]) {
            store.delete(IDBKeyRange.upperBound([space], true));
            store.delete(IDBKeyRange.lowerBound([space, []], true));
            for (const p of evict) store.delete([space, p]);
          }
        }).catch(() => {});
        return found.filter(([p]) => !evict.includes(p)).map(([p, d]) => [p, d]);
      },
      put: (space: string, entries: [string, Uint8Array][]) => tx('readwrite', (s, u) => { for (const [path, data] of entries) { s!.put(data, [space, path]); u!.put(Date.now(), [space, path]); } }),
      remove: (space: string, paths: string[]) => tx('readwrite', (s, u) => { for (const path of paths) { s!.delete([space, path]); u!.delete([space, path]); } }),
      bundle: async (name: string) => (await bundles('readonly', (b) => b.get(name))).result ?? null,
      keepBundle: (name: string, data: Uint8Array) => bundles('readwrite', (b) => b.put(data, name)),
      unbundle: (name: string) => bundles('readwrite', (b) => b.delete(name)),
      pruneBundles: async (keep: Set<string>) => {
        const names = (await bundles('readonly', (b) => b.getAllKeys())).result as string[];
        if (names.some((n) => !keep.has(n))) await bundles('readwrite', (b) => { for (const n of names) if (!keep.has(n)) b.delete(n); });
      },
    };
  };
  /** Messages are answered in the order they came (the page pairs each reply with its request by that order):
   * one at a time, except downloads, which the page sends one at a time itself. Two `lsp` messages after a
   * `reset` would otherwise each start a server of their own. */
  let serial = Promise.resolve();
  /** The page's count of texts sent (an Int32Array on a SharedArrayBuffer), when the page is cross-origin isolated. */
  let cancelFlag: Int32Array | null = null;
  /** Texts the page sent, and files it showed: each message knows how many came before it. */
  let texts = 0, shown: unknown = null;
  self.onmessage = ({ data }: { data: Rec }) => {
    const m = data.type === 'lsp' ? data.message : {};
    if (/^textDocument\/did(Open|Change)$/.test(m.method) || (m.method === 'satex/analyze' && m.params?.first !== shown)) texts++;
    if (m.method === 'satex/analyze') shown = m.params?.first ?? shown;
    data.texts = texts;
    if (['init', 'bundle', 'bundles'].includes(data.type)) handle(data);
    else serial = serial.then(() => handle(data));
  };
  const handle = async (data: Rec): Promise<void> => {
    try {
      if (data.type === 'init') {
        const given = data.given ?? {};
        cancelFlag = data.cancel ?? null;
        if (data.manifest.local) {
          engine = LocalEngine(data.base, (s) => self.postMessage({ type: 'status', ...s }), undefined, (m) => self.postMessage({ type: 'note', out: [m] }));
          self.postMessage({ type: 'ready', ...(await engine!.init()) });
          return;
        }
        importScripts(given.js ? URL.createObjectURL(new Blob([given.js], { type: 'text/javascript' })) : data.base + data.manifest.js);
        // At most every 100 ms, and every phase change.
        let last = 0, phase = '';
        engine = SatexEngine(self.createSatex, (label, loaded, total) => {
          const now = performance.now();
          if (label === phase && now - last < 100 && loaded < total) return;
          [last, phase] = [now, label];
          self.postMessage({ type: 'progress', label, loaded, total });
        }, typeof indexedDB === 'object' ? idb() : null, (s) => self.postMessage({ type: 'status', ...s }));
        self.postMessage({ type: 'ready', ...(await engine!.init(data.base, data.manifest, given)) });
      } else if (data.type === 'bundle') {
        self.postMessage({ type: 'bundle', id: data.entry.id, ...(await engine!.bundle(data.base, data.entry, data.given)) });
      } else if (data.type === 'bundles') {
        self.postMessage({ type: 'bundle', ids: data.entries.map((e: Entry) => e.id), ...(await engine!.bundles(data.base, data.entries, data.given)) });
      } else if (data.type === 'project') {
        await engine!.project(data.dir, data.files ?? {}, data.replace, data.removed);
      } else if (data.type === 'read') {
        self.postMessage({ type: 'file', ...(await engine!.read(data.path)) });
      } else if (data.type === 'reset') {
        await engine!.reset(data.args, data.cwd);
      } else if (data.type === 'run') {
        const encode = (t: string) => new TextEncoder().encode(t);
        const edits = Object.fromEntries(Object.entries(data.edits).map(([path, text]) => [path, encode(text as string)]));
        self.postMessage({ type: 'result', ...(await engine!.run(data.args, edits, data.cwd)) });
      } else if (data.type === 'caches') {
        self.postMessage({ type: 'caches', entries: engine!.caches() });
      } else if (data.type === 'uncache') {
        await engine!.uncache(data.paths);
        self.postMessage({ type: 'caches', entries: engine!.caches() });
      } else if (data.type === 'lsp' && data.texts !== texts && readOnly(data.message)) {
        self.postMessage({ type: 'lsp', out: [{ jsonrpc: '2.0', id: data.message.id, result: null }] });
      } else if (data.type === 'lsp') {
        // the page's shared counter of texts sent: a run of fewer stops once a newer one came
        const flag = cancelFlag, sent = data.sent;
        self.postMessage({ type: 'lsp', out: await engine!.lsp(data.message, flag && sent != null ? () => Atomics.load(flag, 0) > sent : undefined) });
      }
    } catch (e) {
      self.postMessage({ type: 'error', of: data.type, message: e instanceof Error ? `${e.message}\n${e.stack}` : String(e) });
    }
  };
}
