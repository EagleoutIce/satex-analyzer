// The playground's engine: the `satex` binary, built for Emscripten, run over
// an in-memory file system holding the TeX bundle. A Web Worker in the
// browser, started from the page's copy of this file; doc/site/test.mjs runs
// the same code in node.

/* exported SatexEngine */
function SatexEngine(createSatex) {
  const files = new Map();
  let wasm = null;

  async function bytes(url) {
    if (url instanceof Uint8Array) return url;
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    return new Uint8Array(await response.arrayBuffer());
  }

  async function gunzip(data) {
    if (data[0] !== 0x1f || data[1] !== 0x8b) return data; // the server already decoded it
    const stream = new Blob([data]).stream().pipeThrough(new DecompressionStream('gzip'));
    return new Uint8Array(await new Response(stream).arrayBuffer());
  }

  async function compile(url) {
    if (url instanceof Uint8Array) return WebAssembly.compile(url);
    try {
      return await WebAssembly.compileStreaming(fetch(url));
    } catch {
      return WebAssembly.compile(await bytes(url)); // served without application/wasm
    }
  }

  function walk(FS, dir, into) {
    for (const name of FS.readdir(dir)) {
      if (name === '.' || name === '..') continue;
      const path = `${dir}/${name}`;
      if (FS.isDir(FS.stat(path).mode)) walk(FS, path, into);
      else into.set(path, FS.readFile(path));
    }
  }

  /** A fresh instance over the bundle, `edits` laid over it, in `cwd`. */
  async function boot(lines, edits = {}, cwd = '/work') {
    const module = await createSatex({
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
    return module;
  }

  /** One satex invocation in a fresh instance: the binary exits like a process. */
  async function run(args, edits = {}, cwd = '/work', keep = null) {
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
    if (keep) walk(module.FS, keep, files);
    return { lines, status, ms: Math.round(performance.now() - started) };
  }

  /** `satex lsp` in an instance that stays alive: each JSON-RPC message in, the replies out.
   * `reset` drops it: the next message starts one with these flags, in this directory. */
  let server = null, serverArgs = [], serverCwd = '/work';
  function reset(args = [], cwd = '/work') { server = null; serverArgs = args; serverCwd = cwd; }
  async function lsp(message) {
    if (!server) {
      server = await boot([], {}, serverCwd);
      server.callMain([...serverArgs, 'lsp']); // returns with the runtime alive, see src/lsp/wasm.rs
    }
    const text = server.stringToNewUTF8(JSON.stringify(message));
    try {
      return JSON.parse(server.UTF8ToString(server._satex_lsp(text)));
    } finally {
      server._free(text);
    }
  }

  /** A kernel's bundle: its files join the tree, its caches the cache directory. */
  async function kernel(base, entry, given) {
    const blob = await gunzip(await bytes(given ?? base + entry.bundle));
    let offset = 0;
    for (const [path, size] of entry.files) {
      files.set(path, blob.subarray(offset, offset + size));
      offset += size;
    }
    const imported = await run(['cache', 'import', `/kernels/${entry.id}`], {}, '/work', '/cache');
    files.delete(`/kernels/${entry.id}/satex-cache.bin`);
    return imported;
  }

  /** Files under `dir`: a new project's replace the last one's, added ones join them. */
  function project(dir, given, replace = false) {
    if (replace) for (const path of [...files.keys()]) if (path.startsWith(dir + '/')) files.delete(path);
    for (const [path, data] of Object.entries(given)) files.set(`${dir}/${path}`, data);
  }

  /** A file of the tree by its path, or the first whose name is `path`'s last part. */
  function read(path) {
    const name = path.split('/').pop();
    const found = files.has(path) ? path : [...files.keys()].find((p) => p.endsWith(`/${name}`));
    return { path: found ?? path, data: found ? files.get(found).slice() : null };
  }

  /** Fetches the binary and the default kernel, unless `given` has them. */
  async function init(base, manifest, given = {}) {
    wasm = await compile(given.wasm ?? base + manifest.wasm);
    const imported = await kernel(base, manifest.kernels[0], given.bundle);
    const decoder = new TextDecoder();
    const samples = Object.fromEntries(manifest.samples.map((name) => [name, decoder.decode(files.get(`/work/${name}`))]));
    return { samples, imported };
  }

  return { init, run, lsp, reset, kernel, project, read };
}

if (typeof module === 'object' && module.exports) {
  module.exports = SatexEngine;
} else if (typeof importScripts === 'function') {
  let engine = null;
  self.onmessage = async ({ data }) => {
    try {
      if (data.type === 'init') {
        const given = data.given ?? {};
        importScripts(given.js ? URL.createObjectURL(new Blob([given.js], { type: 'text/javascript' })) : data.base + data.manifest.js);
        engine = SatexEngine(self.createSatex);
        self.postMessage({ type: 'ready', ...(await engine.init(data.base, data.manifest, given)) });
      } else if (data.type === 'kernel') {
        self.postMessage({ type: 'kernel', id: data.entry.id, ...(await engine.kernel(data.base, data.entry, data.given)) });
      } else if (data.type === 'project') {
        engine.project(data.dir, data.files, data.replace);
      } else if (data.type === 'read') {
        self.postMessage({ type: 'file', ...engine.read(data.path) });
      } else if (data.type === 'reset') {
        engine.reset(data.args, data.cwd);
      } else if (data.type === 'run') {
        const encode = (t) => new TextEncoder().encode(t);
        const edits = Object.fromEntries(Object.entries(data.edits).map(([path, text]) => [path, encode(text)]));
        self.postMessage({ type: 'result', ...(await engine.run(data.args, edits, data.cwd)) });
      } else if (data.type === 'lsp') {
        self.postMessage({ type: 'lsp', out: await engine.lsp(data.message) });
      }
    } catch (e) {
      self.postMessage({ type: 'error', of: data.type, message: String(e && e.stack || e) });
    }
  };
}
