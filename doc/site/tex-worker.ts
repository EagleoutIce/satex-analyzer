// Compile without a server: pdfTeX of BusyTeX, built for WebAssembly, in a worker of its own.
// The site ships the engine and the core of a TeX Live tree. A file outside that core
// comes from a TeX Live package server, once, then from the browser's cache.

export {};
/** The BusyTeX module and its Emscripten file system, as far as used here. */
interface Mod {
  FS: Rec; HEAPU8: Uint8Array; callMain(args: string[]): number; kpse_remote_register(name: string, format: number, bytes: Uint8Array): void;
  kpse_remote_clear(): void; _flush_streams?(): void; PATH: Rec; ENV: Rec;
}
/** A preamble dumped as a format (mylatexformat): what it was dumped from, its file (none: unusable), the dump's log. */
interface Dumped { key: string; fmt: string | null; log: string }
interface Instance { m: Mod; out: { lines: string[] }; keep: Cache | null | undefined; header: Uint8Array; core: Set<string>; asked?: string; last?: { main: string; inputs: string[] }; dumped?: Dumped }
interface Command { command: string; status: number; ms: number }
const TEXMF = '/texlive/texmf-dist', PROJECT = '/home/web_user/project_dir';
const FMT = `${TEXMF}/texmf-var/web2c/pdftex/pdflatex.fmt`;
/** Where the preamble formats go: beside the project, which a build for another main file mounts anew. */
const DUMPS = '/home/web_user/formats';
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
/** Files every source answered 404 for, kept between visits (`ABSENT` keys) while the source list stays: each would
 * cost a round trip per source and build again. `missed`: the ones this build found. */
let absent = new Set<string>(), missed = new Set<string>();
const ABSENT = `${ANSWERED}absent/`, ABSENT_MS = 7 * 24 * 3600e3;
/** Lookups for the document's own files stay here: nothing to find, and their names are the document's. */
const askable = (name: string) => !OWN.test(name) && !stems.has(name.replace(/\.[^.]*$/, ''));
const hostOf = (pattern: string) => new URL(pattern.split('{')[0]).host;
/** One file from the sources, in order: a 404 goes on to the next, a host that fails is left out for the rest of the build. */
function ask(format: string, name: string): [number, ArrayBuffer | null] {
  if (!askable(name)) return [404, null];
  const key = `${format}/${encodeURIComponent(name)}`;
  if (absent.has(key)) return [404, null];
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
        held.add(`${format}/${name}`);
        return [200, x.response];
      }
      if (x.status === 404) continue;
    } catch {}
    dead.add(host);
    clean = false;
  }
  if (!clean) failed.add(name); else missed.add(key);
  // not a 404 after a failure: the engine remembers a 404 as absent
  return [clean ? 404 : 503, null];
}
/** kpathsea's format numbers (kpse_file_format_type) by extension: the engine asks `{format}/{name}`. */
const FORMATS: Record<string, number> = { tfm: 3, bib: 6, bst: 7, map: 11, pfb: 32, vf: 33, ist: 35, enc: 44 };
const formatOf = (name: string) => FORMATS[name.split('.').pop()!] ?? 26;
/** The engine's keys of the files it holds: fetched, or kept by the browser. */
const held = new Set<string>();
function hold(m: Mod, format: number, name: string, bytes: Uint8Array, host?: string) {
  m.kpse_remote_register(name, format, bytes);
  held.add(`${format}/${name}`);
  if (host) got.set(`${format}/${encodeURIComponent(name)}`, { bytes, host });
}
const PARALLEL = 16;
/** The font map's files per font: Type 1 and encoding. */
const MAP = 'texmf-var/fonts/map/pdftex/updmap/pdftex.map';
let map: Map<string, string[]> | undefined;
/** The files `names` (what the analysis saw the document read) fetched side by side before the first run asks for
 * them one at a time: each from the sources in order, as ask() would. All 404: absent. A host that fails is left
 * to ask(). `\input xkvutils` asks the bare name: a `.tex` is held under its stem too. */
async function prefetch({ m, core }: Instance, names: string[]) {
  // a font's metrics come with its probe for a virtual font and, by the font map, its Type 1 file and encoding
  const fonts = names.filter((n) => n.endsWith('.tfm')).map((n) => n.slice(0, -4));
  if (fonts.length) {
    map ??= new Map((m.FS.readFile(`${TEXMF}/${MAP}`, { encoding: 'utf8' }) as string).split('\n')
      .map((l) => [l.split(/\s/, 1)[0]!, [...l.matchAll(/<+\[?([\w.-]+\.(?:pfb|enc))/g)].map((x) => x[1]!)] as [string, string[]]));
    names = [...names, ...fonts.flatMap((f) => [`${f}.vf`, ...map!.get(f) ?? []])];
  }
  const todo = [...new Set(names)].filter((n) => askable(n) && !core.has(n) && !held.has(`${formatOf(n)}/${n}`) && !absent.has(`${formatOf(n)}/${encodeURIComponent(n)}`));
  if (!todo.length) return;
  let done = 0, bytes = 0, found = 0;
  const say = () => post({ phase: `fetching ${todo.length} TeX files side by side: ${done} done, ${found} found, ${(bytes / 1048576).toFixed(1)} MiB` });
  say();
  const one = async (name: string) => {
    const format = formatOf(name), key = `${format}/${encodeURIComponent(name)}`;
    for (const pattern of sources) {
      const host = hostOf(pattern);
      if (dead.has(host)) return;
      try {
        const r = await fetch(pattern.replace('{format}', String(format)).replace('{name}', encodeURIComponent(name)), { signal: AbortSignal.timeout(timeout) });
        if (r.ok) {
          const data = await bytesOf(r);
          if (!data.length) return;
          hold(m, format, name, data, host);
          if (format === 26 && name.endsWith('.tex')) hold(m, format, name.slice(0, -4), data, host);
          bytes += data.length; found++;
          return;
        }
        if (r.status !== 404) return;
      } catch { return; }
    }
    missed.add(key); absent.add(key);
  };
  const queue = [...todo];
  await Promise.all(Array.from({ length: PARALLEL }, async () => { for (let n; (n = queue.shift());) { await one(n); done++; say(); } }));
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
  post({ phase: 'compiling the TeX engine' });
  const wasm = await WebAssembly.compile(wasmBytes);
  post({ phase: `unpacking ${tex.files.length} files of the TeX tree` });
  // a host that already decoded the tree sends it plain
  const core = coreBytes[0] === 0x1f && coreBytes[1] === 0x8b ? await gunzip(coreBytes) : coreBytes;
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
    if (request.url.startsWith(ABSENT)) {
      const r = await keep!.match(request);
      if (r?.headers.get('x-sources') === encodeURIComponent(sources.join('\n')) && Date.now() - +(r.headers.get('x-at') ?? 0) < ABSENT_MS) absent.add(`${format}/${name}`);
      else keep!.delete(request).catch(() => {});
      continue;
    }
    hold(m, +format, decodeURIComponent(name), await bytesOf((await keep!.match(request))!));
  }
  return { m, out, keep, header: Uint8Array.from(m.HEAPU8.subarray(0, HEADER)), core: new Set(tex.files.map(([p]: [string]) => p.slice(p.lastIndexOf('/') + 1))) };
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
  return status;
}

/** Builds `main` of `files`: pdfTeX until its references rest, BibTeX and makeindex where the run asks for them. */
async function build(instance: Instance, files: Record<string, Uint8Array | string>, main: string, want: string[]) {
  const { m } = instance, { FS, PATH } = m;
  // a new list or a host that failed: what the engine remembers as absent is asked for again
  const asked = sources.join('\n');
  if (asked !== instance.asked || dead.size) m.kpse_remote_clear();
  if (instance.asked !== undefined && asked !== instance.asked) absent.clear();
  instance.asked = asked;
  missed = new Set();
  got = new Map(); failed = new Set(); dead = new Set();
  stems = new Set(Object.keys(files).map((n) => n.split('/').pop()!.replace(/\.[^.]*$/, '')));
  // the preamble's dump reads mylatexformat.ltx, which the core lacks
  if (sources.length) await prefetch(instance, ['mylatexformat.ltx', ...want]);
  // The last clean build of this main file left its .aux and the like: they seed this one, as latexmk's directory would.
  // Any other build starts from an empty directory.
  const seeded = instance.last?.main === main;
  if (seeded) {
    for (const name of instance.last!.inputs) if (!(name in files)) try { FS.unlink(PATH.join(PROJECT, name)); } catch {}
  } else {
    if (FS.analyzePath(PROJECT).object?.mount?.mountpoint === PROJECT) FS.unmount(PROJECT);
    FS.mount(FS.filesystems.MEMFS, {}, PROJECT);
  }
  instance.last = undefined;
  for (const [name, data] of Object.entries(files)) {
    const path = PATH.join(PROJECT, name);
    FS.mkdirTree(PATH.dirname(path));
    FS.writeFile(path, data);
  }
  const dir = PATH.join(PROJECT, PATH.dirname(main)), file = PATH.basename(main), job = file.replace(/\.tex$/, '');
  FS.chdir(dir);
  const read = (name: string, binary = false) => FS.analyzePath(PATH.join(dir, name)).exists ? FS.readFile(PATH.join(dir, name), { encoding: binary ? 'binary' : 'utf8' }) : binary ? null : '';
  // what this build shows is what its own runs write: no PDF of the last one survives a run that writes none
  for (const n of [`${job}.pdf`, `${job}.synctex.gz`, `${job}.log`]) if (FS.analyzePath(PATH.join(dir, n)).exists) FS.unlink(PATH.join(dir, n));
  /** The files the runs read back (OWN, without the logs): equal before and after a pass, the references rest. */
  const own = (at = dir): string[] => (FS.readdir(at) as string[]).filter((n) => n !== '.' && n !== '..').sort().flatMap((n) => {
    const full = PATH.join(at, n);
    if (FS.isDir(FS.stat(full).mode)) return own(full);
    return OWN.test(n) && !/\.(blg|ilg|synctex(\.gz)?)$/.test(n) ? [`${full}\n${FS.readFile(full, { encoding: 'utf8' })}`] : [];
  });
  const commands: Command[] = [];
  let status = 0;
  const fmt = preamble(instance, files, main, dir, file, job, commands);
  // a fresh build's first pass is never its last: it writes no PDF (-draftmode)
  const tex = (draft: boolean) => status = run(instance, ['pdflatex', ...draft ? ['-draftmode'] : ['-synctex=1'], '--no-shell-escape', '-interaction=nonstopmode', '-file-line-error', '--fmt', fmt?.fmt ?? FMT, file], commands);
  let was = seeded ? own().join('\0') : null, biblatex = false;
  for (let pass = 1; pass <= PASSES; pass++) {
    post({ phase: `pdfTeX, pass ${pass}${pass === 1 && !seeded ? ' (writes the .aux, no PDF)' : ''}` });
    tex(pass === 1 && !seeded);
    if (pass === 1) {
      if (/\\bibdata\{/.test(read(`${job}.aux`))) { post({ phase: 'BibTeX' }); run(instance, ['bibtex8', '--8bit', `${job}.aux`], commands); }
      biblatex = !!read(`${job}.bcf`);
      if (read(`${job}.idx`).trim()) { post({ phase: 'makeindex' }); run(instance, ['makeindex', `${job}.idx`], commands); }
    }
    const now = own().join('\0'), log = read(`${job}.log`);
    if (now === was && !/Rerun to get|Label\(s\) may have changed/.test(log)) break;
    was = now;
  }
  // the preamble's messages were said while its format was dumped: they head the log
  if (fmt) FS.writeFile(PATH.join(dir, `${job}.log`), fmt.log + read(`${job}.log`));
  // only a clean last run seeds the next build: an error may have left a broken .aux
  if (!status && read(`${job}.pdf`, true)) instance.last = { main, inputs: Object.keys(files) };
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
  const said = { 'x-sources': encodeURIComponent(sources.join('\n')), 'x-at': String(Date.now()) };
  if (instance.keep) for (const key of missed) { absent.add(key); instance.keep.put(ABSENT + key, new Response('', { headers: said })).catch(() => {}); }
  const from: Record<string, number> = {};
  for (const { host } of got.values()) from[host] = (from[host] ?? 0) + 1;
  return { job, outputs, commands, biblatex, from, asked: [...new Set(sources.map(hostOf))], dead: [...dead], failed: [...failed] };
}

/** The format of `main`'s preamble, dumped again when the preamble or another file of the project changed.
 * None where the dump fails or reports an error: the runs then read the preamble themselves and report it. */
function preamble(instance: Instance, files: Record<string, Uint8Array | string>, main: string, dir: string, file: string, job: string, commands: Command[]) {
  // the page sends its texts as strings, images as bytes
  const str = (d: Uint8Array | string, enc = 'utf-8') => typeof d === 'string' ? d : new TextDecoder(enc).decode(d);
  const source = str(files[main]!), at = source.indexOf('\\begin{document}');
  if (at < 0 || source.startsWith('%&')) return null;
  const others = Object.keys(files).filter((n) => n !== main).sort().map((n) => `${n}\0${str(files[n]!, 'latin1')}`);
  const key = [main, source.slice(0, at), ...others].join('\0');
  if (instance.dumped?.key !== key) {
    const { FS, PATH } = instance.m;
    FS.mkdirTree(DUMPS);
    const log = PATH.join(DUMPS, `${job}.log`);
    post({ phase: 'dumping the preamble as a format' });
    const status = run(instance, ['pdftex', '-ini', '--no-shell-escape', '-interaction=nonstopmode', '-file-line-error', `-output-directory=${DUMPS}`, `-jobname=${job}`, `&${FMT.replace(/\.fmt$/, '')}`, 'mylatexformat.ltx', file], commands);
    const said = FS.analyzePath(log).exists ? FS.readFile(log, { encoding: 'utf8' }) as string : '';
    const fmt = PATH.join(DUMPS, `${job}.fmt`), usable = !status && FS.analyzePath(fmt).exists && !/^(! |\S+:\d+: )/m.test(said);
    instance.dumped = { key, fmt: usable ? fmt : null, log: said };
    FS.chdir(dir);
  }
  return instance.dumped.fmt ? instance.dumped as { fmt: string; log: string } : null;
}

let booting: Promise<Instance> | null = null;
self.onmessage = async ({ data: { tex, files, main, from, want = [], boot } }) => {
  // the page gathers what to build meanwhile: a failed start is said by the build that waits for it
  if (boot) { sources = from; timeout = boot.timeoutMs; booting ??= start(boot); booting.catch(() => {}); return; }
  try {
    sources = from;
    timeout = tex.timeoutMs;
    const instance = await (booting ??= start(tex));
    const answer = await build(instance, files, main, want);
    post(answer, answer.outputs.map((o) => o.bytes.buffer));
  } catch (e) {
    post({ error: String((e as Error)?.message ?? e) });
  }
};
