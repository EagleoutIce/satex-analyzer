// Compile without a server: pdfTeX of BusyTeX, built for WebAssembly, in a worker of its own.
// The site ships the engine and the core of a TeX Live tree. A file outside that core
// comes from a TeX Live package server, once, then from the browser's cache.

export {};
/** The BusyTeX module and its Emscripten file system, as far as used here. */
interface Mod {
  FS: Rec; HEAPU8: Uint8Array; callMain(args: string[]): number; kpse_remote_register(name: string, format: number, bytes: Uint8Array): void;
  kpse_remote_clear(): void; _flush_streams?(): void; PATH: Rec; ENV: Rec;
}
interface Instance { m: Mod; out: { lines: string[] }; keep: Cache | null | undefined; header: Uint8Array; asked?: string }
interface Command { command: string; status: number; ms: number }
const TEXMF = '/texlive/texmf-dist', PROJECT = '/home/web_user/project_dir';
const FMT = `${TEXMF}/texmf-var/web2c/pdftex/pdflatex.fmt`;
/** The engine's memory up to here holds its static state: restored between runs. BusyTeX's own value. */
const HEADER = 2 ** 26;
/** What a run writes and reads back of its own: asked of no server. */
const OWN = /\.(aux|toc|lof|lot|out|bbl|blg|idx|ind|ilg|nav|snm|vrb|bcf|xdv|gls|glo|acn|acr|ist|nlo|nls|fls|run\.xml|tdo|thm|loa|lol|synctex(\.gz)?)$/;
/** Passes at most: a document that keeps moving its references stops here. */
const PASSES = 4;
const KEEP = 'satex-tex-files';
/** The address the engine asks and the keeping's keys: never fetched, ask() answers. */
const ANSWERED = 'https://tex.invalid/';
const post = (m: Rec, transfer: Transferable[] = []) => self.postMessage(m, transfer);
const bytesOf = async (r: Response) => new Uint8Array(await r.arrayBuffer());

const gunzip = (data: Uint8Array) => bytesOf(new Response(new Blob([data as BlobPart]).stream().pipeThrough(new DecompressionStream('gzip'))));

/** What the sources gave during this build, the files lost to a host that failed, the hosts that failed, the project's own names. */
let got = new Map<string, { bytes: Uint8Array; host: string }>(), failed = new Set<string>(), dead = new Set<string>(), stems = new Set<string>();
/** The address patterns to ask, in order, and how long one file may take. */
let sources: string[] = [], timeout = 0;
/** Lookups for the document's own files stay here: nothing to find, and their names are the document's. */
const askable = (name: string) => !OWN.test(name) && !stems.has(name.replace(/\.[^.]*$/, ''));
const hostOf = (pattern: string) => new URL(pattern.split('{')[0]).host;
/** One file from the sources, in order: a 404 goes on to the next, a host that fails is left out for the rest of the build. */
function ask(format: string, name: string): [number, ArrayBuffer | null] {
  if (!askable(name)) return [404, null];
  let clean = true;
  for (const pattern of sources) {
    const host = hostOf(pattern);
    if (dead.has(host)) { clean = false; continue; }
    post({ fetching: `${name} from ${host}` });
    try {
      const x = new Native();
      x.open('GET', pattern.replace('{format}', format).replace('{name}', encodeURIComponent(name)), false);
      x.timeout = timeout;
      x.responseType = 'arraybuffer';
      x.send();
      if (x.status === 200 && x.response?.byteLength) {
        got.set(`${format}/${encodeURIComponent(name)}`, { bytes: new Uint8Array(x.response!.slice(0)), host });
        return [200, x.response];
      }
      if (x.status === 404) continue;
    } catch {}
    dead.add(host);
    clean = false;
  }
  if (!clean) failed.add(name);
  // not a 404 after a failure: the engine remembers a 404 as absent
  return [clean ? 404 : 503, null];
}
const Native = self.XMLHttpRequest as unknown as new () => { open(m: string, url: string, async: boolean): void; timeout: number; responseType: string; send(): void; status: number; response: ArrayBuffer | null };
// the engine's one kind of request (kpse_remote.js): its address says which file, ask() says where from
// @ts-expect-error: only what the engine uses
self.XMLHttpRequest = class {
  status = 0; response: ArrayBuffer | null = null; timeout = 0; responseType = ''; url = '';
  open(_: string, url: string) { this.url = url; }
  send() {
    const [format, name] = this.url.split('/').slice(-2) as [string, string];
    [this.status, this.response] = ask(format, decodeURIComponent(name));
  }
};

/** The engine, booted once. */
async function start(tex: Rec): Promise<Instance> {
  importScripts(tex.engine);
  // Both at once: the binary compiles from its bytes, the tree lands after the boot.
  const total = tex.wasmSize + tex.coreSize, seen: Record<string, number> = {};
  const part = async (url: string) => {
    const r = await fetch(url), chunks: Uint8Array[] = [];
    if (!r.ok) throw new Error(`${url}: ${r.status}`);
    for (const reader = r.body!.getReader(); ;) {
      const { done, value } = await reader.read();
      if (done) return new Uint8Array(await new Blob(chunks as BlobPart[]).arrayBuffer());
      chunks.push(value);
      seen[url] = (seen[url] ?? 0) + value.length;
      post({ progress: { label: 'the TeX engine', loaded: seen[tex.wasm]! + (seen[tex.core] ?? 0) || 0, size: total } });
    }
  };
  const [wasmBytes, coreBytes] = await Promise.all([part(tex.wasm), part(tex.core)]);
  post({ phase: 'starting the TeX engine' });
  // a host that already decoded the tree sends it plain
  const wasm = await WebAssembly.compile(wasmBytes), core = coreBytes[0] === 0x1f && coreBytes[1] === 0x8b ? await gunzip(coreBytes) : coreBytes;
  const out = { lines: [] as string[] };
  const Module: Rec = {
    thisProgram: '/bin/busytex', noInitialRun: true,
    preRun: [() => {
      Object.assign(Module.ENV, { TEXMFDIST: TEXMF, TEXMFVAR: `${TEXMF}/texmf-var`, TEXMFCACHE: `${TEXMF}/texmf-var`, TEXMFCNF: `${TEXMF}/web2c`, TEXLIVE_REMOTE_ENDPOINT: ANSWERED, max_print_line: '10000' });
      Module.FS.mkdir(PROJECT);
    }],
    instantiateWasm(imports: WebAssembly.Imports, done: (i: WebAssembly.Instance, m: WebAssembly.Module) => void) {
      WebAssembly.instantiate(wasm, imports).then((instance) => done(instance, wasm));
      return {};
    },
    print: (t: string) => out.lines.push(t),
    printErr: (t: string) => out.lines.push(t),
  };
  const m: Mod = await self.busytex(Module);
  // kpathsea looks for the program it runs as
  m.FS.mkdirTree('/bin');
  m.FS.writeFile('/bin/busytex', '');
  let at = 0;
  for (const [path, size] of tex.files) {
    m.FS.mkdirTree(path.slice(0, path.lastIndexOf('/')));
    m.FS.writeFile(path, core.subarray(at, at += size));
  }
  // what earlier builds fetched
  const keep = await globalThis.caches?.open(KEEP).catch(() => null);
  for (const request of await keep?.keys() ?? []) {
    const [format, name] = request.url.split('/').slice(-2) as [string, string];
    m.kpse_remote_register(decodeURIComponent(name), +format, await bytesOf((await keep!.match(request))!));
  }
  return { m, out, keep, header: Uint8Array.from(m.HEAPU8.subarray(0, HEADER)) };
}

/** One program run: the module's memory as after its boot again, the files it left open closed. */
function run({ m, out, header }: Instance, args: string[], commands: Command[]) {
  const open = new Set(m.FS.streams.filter(Boolean).map((s: Rec) => s.fd)), began = performance.now();
  out.lines.length = 0;
  let status: number;
  try { status = m.callMain([...args]); } catch (e) { status = (e as Rec)?.status ?? 1; if ((e as Rec)?.name !== 'ExitStatus') out.lines.push(String(e)); }
  try { m._flush_streams?.(); } catch {}
  for (const s of m.FS.streams) if (s && !open.has(s.fd)) m.FS.close(s);
  m.HEAPU8.fill(0);
  m.HEAPU8.set(header);
  commands.push({ command: args.join(' '), status, ms: Math.round(performance.now() - began) });
}

/** Builds `main` of `files`: pdfTeX until its references rest, BibTeX and makeindex where the run asks for them. */
async function build(instance: Instance, files: Record<string, Uint8Array>, main: string) {
  const { m } = instance, { FS, PATH } = m;
  // a new list or a host that failed: what the engine remembers as absent is asked for again
  const asked = sources.join('\n');
  if (asked !== instance.asked || dead.size) m.kpse_remote_clear();
  instance.asked = asked;
  got = new Map(); failed = new Set(); dead = new Set();
  stems = new Set(Object.keys(files).map((n) => n.split('/').pop()!.replace(/\.[^.]*$/, '')));
  if (FS.analyzePath(PROJECT).object?.mount?.mountpoint === PROJECT) FS.unmount(PROJECT);
  FS.mount(FS.filesystems.MEMFS, {}, PROJECT);
  for (const [name, data] of Object.entries(files)) {
    const path = PATH.join(PROJECT, name);
    FS.mkdirTree(PATH.dirname(path));
    FS.writeFile(path, data);
  }
  const dir = PATH.join(PROJECT, PATH.dirname(main)), file = PATH.basename(main), job = file.replace(/\.tex$/, '');
  FS.chdir(dir);
  const read = (name: string, binary = false) => FS.analyzePath(PATH.join(dir, name)).exists ? FS.readFile(PATH.join(dir, name), { encoding: binary ? 'binary' : 'utf8' }) : binary ? null : '';
  const commands: Command[] = [];
  const tex = () => run(instance, ['pdflatex', '-synctex=1', '--no-shell-escape', '-interaction=nonstopmode', '-file-line-error', '--fmt', FMT, file], commands);
  let aux = null, biblatex = false;
  for (let pass = 1; pass <= PASSES; pass++) {
    post({ phase: `pdfTeX, pass ${pass}` });
    tex();
    if (pass === 1) {
      if (/\\bibdata\{/.test(read(`${job}.aux`))) { post({ phase: 'BibTeX' }); run(instance, ['bibtex8', '--8bit', `${job}.aux`], commands); }
      biblatex = !!read(`${job}.bcf`);
      if (read(`${job}.idx`).trim()) { post({ phase: 'makeindex' }); run(instance, ['makeindex', `${job}.idx`], commands); }
    }
    const now = read(`${job}.aux`), log = read(`${job}.log`);
    if (now === aux && !/Rerun to get|Label\(s\) may have changed/.test(log)) break;
    aux = now;
  }
  const outputs: { name: string; bytes: Uint8Array }[] = [], inputs = new Set(Object.keys(files).map((n) => PATH.join(PROJECT, n)));
  const walk = (at: string) => {
    for (const name of FS.readdir(at)) {
      if (name === '.' || name === '..') continue;
      const full = PATH.join(at, name);
      if (FS.isDir(FS.stat(full).mode)) walk(full);
      else if (!inputs.has(full)) outputs.push({ name: full.slice(dir.length + 1), bytes: FS.readFile(full) });
    }
  };
  walk(dir);
  // kept for the next visit, after the answer is on its way
  if (instance.keep) for (const [key, { bytes }] of got) instance.keep.put(ANSWERED + key, new Response(bytes as BodyInit)).catch(() => {});
  const from: Record<string, number> = {};
  for (const { host } of got.values()) from[host] = (from[host] ?? 0) + 1;
  return { job, outputs, commands, biblatex, from, asked: [...new Set(sources.map(hostOf))], dead: [...dead], failed: [...failed] };
}

let booting: Promise<Instance> | null = null;
self.onmessage = async ({ data: { tex, files, main, from } }) => {
  try {
    sources = from;
    timeout = tex.timeoutMs;
    const instance = await (booting ??= start(tex));
    const answer = await build(instance, files, main);
    post(answer, answer.outputs.map((o) => o.bytes.buffer));
  } catch (e) {
    post({ error: String((e as Error)?.message ?? e) });
  }
};
