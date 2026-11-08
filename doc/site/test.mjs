// Runs the playground of a built site in node: the page's manifest, its
// worker.js engine and the satex wasm, fetched as a browser would. Then, if a
// Chromium is found ($CHROME, or Playwright's download), the page itself in a
// headless browser over the DevTools protocol.
//   python3 -m http.server -d site-out 8000 & node doc/site/test.mjs http://localhost:8000/
//   node doc/site/test.mjs file:///path/to/site-out/
// QUICK=1 checks a sample of the packages offered on demand, for the edit loop.
import { createRequire } from 'node:module';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const base = process.argv[2] ?? 'http://localhost:8000/';
const bytes = async (url) => base.startsWith('file:') ? readFileSync(fileURLToPath(base + url)) : new Uint8Array(await (await fetch(base + url)).arrayBuffer());
const text = async (url) => new TextDecoder().decode(await bytes(url));
const load = async (url, code) => {
  const module = { exports: {} };
  new Function('module', 'exports', 'require', '__dirname', '__filename', code ?? await text(url))(
    module, module.exports, createRequire(import.meta.url), '/', '/x.js');
  return module.exports;
};
let failed = false;
const check = (name, ok, detail = '') => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? `: ${detail}` : ''}`); failed ||= !ok; };

const page = await text('play/index.html');
const manifest = JSON.parse(page.match(/<script id="satex-play" type="application\/json">(.*?)<\/script>/s)[1]);
const SatexEngine = await load(null, manifest.worker), createSatex = await load(manifest.js);
// BROWSER_ONLY=1: the page in a browser alone, to iterate on its checks
if (process.env.BROWSER_ONLY) { await browser(); process.exit(failed ? 1 : 0); }
// The page keeps the cache directory in IndexedDB (worker.js); here a map stands in for it.
const memory = new Map();
const store = {
  load: async (space) => [...memory].filter(([k]) => k.startsWith(`${space}|`)).map(([k, v]) => [k.slice(space.length + 1), v]),
  put: async (space, entries) => { for (const [path, data] of entries) memory.set(`${space}|${path}`, data); },
  remove: async (space, paths) => { for (const path of paths) memory.delete(`${space}|${path}`); },
};
const engine = SatexEngine(createSatex, () => {}, store);
let t = performance.now();
const given = base.startsWith('file:') ? { js: undefined, wasm: await bytes(manifest.wasm), bundle: await bytes(manifest.kernels[0].bundle) } : {};
const { samples, imported } = await engine.init(base, manifest, given);
console.log(`init: ${Math.round(performance.now() - t)} ms, ${imported.lines.map((l) => l[1]).join(' ')}`);
// The bundles a document's files name (a package or class), and those they need, not loaded into `have` yet:
// what the page fetches before a project's first run.
const offered = manifest.packages ?? [];
const NAMED = /\\(?:documentclass|usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}/g;
const needed = (kernel, files, have) => {
  const picked = new Set();
  const pick = (id) => { const e = offered.find((p) => p.id === id); if (e && !picked.has(e)) { picked.add(e); e.needs.forEach((n) => pick(`${kernel.id}/${n}`)); } };
  for (const f of files) for (const m of (samples[f] ?? '').matchAll(NAMED)) for (const n of m[1].split(',').map((x) => x.trim())) { pick(`${kernel.id}/${n}`); pick(`${kernel.id}/${n}.cls`); }
  return [...picked].filter((e) => !have.has(e.id));
};
const fetchedHere = new Set();
const fetchFor = async (eng, kernel, files) => {
  const entries = needed(kernel, files, fetchedHere);
  entries.forEach((e) => fetchedHere.add(e.id));
  await eng.bundles(base, entries, base.startsWith('file:') ? await Promise.all(entries.map((e) => bytes(e.bundle))) : undefined);
  return entries;
};
// a sample's magic comment names its engine, as the page's `engineOf` reads it: plain TeX runs on its own kernel
const kernelOf = (text) => /^%\s*!T[eE]X\s+(?:TS-)?program\s*=\s*tex\b/m.test(text) ? manifest.kernels.find((k) => k.id === 'plain') : manifest.kernels[0];
const kernelsLoaded = new Set([manifest.kernels[0].id]);
for (const name of manifest.samples) {
  const kernel = kernelOf(samples[name]);
  if (!kernelsLoaded.has(kernel.id)) { kernelsLoaded.add(kernel.id); await engine.bundle(base, kernel, base.startsWith('file:') ? await bytes(kernel.bundle) : undefined); }
  await fetchFor(engine, kernel, [name, ...(manifest.projects?.[name] ?? [])]);
  const run = await engine.run([...kernel.args, '-v', 'lint', '-f', name], { [`/work/${name}`]: new TextEncoder().encode(samples[name]) });
  const out = run.lines.filter((l) => l[0] === 'out').map((l) => l[1]);
  const summary = out.find((l) => /\d+ errors?, \d+ warnings?/.test(l));
  // what the bundles hold must come from them; the project's own packages are analysed here
  const bundled = (l) => !/cache package /.test(l) || offered.some((p) => p.name === /cache package (\S+):/.exec(l)?.[1]);
  const rebuilt = run.lines.filter((l) => /building kernel cache|: rebuilt \(/.test(l[1]) && bundled(l[1])).map((l) => l[1].trim());
  check(`${name} lints`, run.status === 0 && summary && !rebuilt.length,
    `exit ${run.status}, ${run.ms} ms, ${summary ?? 'no summary'}${rebuilt.length ? `; not served from the bundle's caches: ${rebuilt.join('; ')}` : ''}`);
  check(`${name} carries no command line`, !/^% satex /.test(samples[name]));
}

// A second visit reads the caches the first kept, without importing the kernel's again.
{
  const again = SatexEngine(createSatex, () => {}, store);
  const t = performance.now();
  const { imported: second } = await again.init(base, manifest, given);
  const kept = again.caches();
  check('a second visit reuses the kept caches', !second.lines.length && kept.length === engine.caches().length && kept.some((e) => /format-/.test(e.path)),
    `${Math.round(performance.now() - t)} ms, ${kept.length} entries`);
  await again.uncache(kept.map((e) => e.path));
  check('dropped caches are gone', !again.caches().length && !(await store.load(manifest.wasm)).length);
}

// The other kernels: each bundle loads, and the default sample lints with it.
for (const entry of manifest.kernels.slice(1)) {
  t = performance.now();
  await engine.bundle(base, entry, base.startsWith('file:') ? await bytes(entry.bundle) : undefined);
  const run = await engine.run([...entry.args, 'lint', '-f', manifest.sample]);
  const summary = run.lines.map((l) => l[1]).find((l) => /\d+ errors?, \d+ warnings?/.test(l));
  check(`${entry.label} kernel`, typeof run.status === 'number' && run.status <= 1 && summary, `${Math.round(performance.now() - t)} ms with loading, ${summary ?? 'no summary'}`);
}

// Packages on demand: missing until its bundle is loaded, then every offered one resolves
// (under the first kernel that has it: fontspec wants XeTeX).
const unresolved = (run, id) => run.lines.some((l) => l[1].includes(`\`${id}\` is not in this installation`) || l[1].includes(`\`${id}.sty\` is not in this installation`));
const usePackage = (id) => new TextEncoder().encode(`\\documentclass{article}\n\\usepackage{${id}}\n\\begin{document}\nText.\n\\end{document}\n`);
const packageRun = (id, kernel) => engine.run([...kernel.args, 'lint', '-f', 'probe.tex'], { '/work/probe.tex': usePackage(id) });
const micro = offered.find((p) => p.name === 'microtype' && p.kernel === manifest.kernels[0].id);
check('microtype is offered on demand', !!micro && micro.size < 2 ** 20, micro ? `${micro.files.length} files, ${micro.size} bytes` : offered.map((p) => p.id).join(' '));
check('the first download stays small', manifest.kernels[0].size < 8 * 2 ** 20, `${manifest.kernels[0].size} bytes`);
if (micro) check('microtype is missing before its bundle', unresolved(await packageRun('microtype', manifest.kernels[0]), 'microtype'));
t = performance.now();
for (const entry of offered) await engine.bundle(base, entry, base.startsWith('file:') ? await bytes(entry.bundle) : undefined);
const stillMissing = [];
// QUICK=1 (the edit loop): every 8th package; the full run, before a push, all of them
const sampled = offered.filter((p) => !p.label.endsWith('.cls')).filter((_, i) => !process.env.QUICK || i % 8 === 0);
for (const entry of sampled) {
  const kernel = manifest.kernels.find((k) => k.id === entry.kernel);
  let run;
  try { run = await packageRun(entry.name, kernel); } catch (e) { console.log(`FAIL ${entry.id} aborted the engine: ${e.message}`); throw e; }
  if (!(typeof run.status === 'number' && run.status <= 1 && !unresolved(run, entry.name))) stillMissing.push(entry.id);
}
check(`${sampled.length} of ${offered.length} packages on demand resolve`, offered.length >= 20 && !stillMissing.length, `${Math.round(performance.now() - t)} ms; missing ${stillMissing.join(' ')}`);

// An example opened as the page opens it: only the bundles its files name, and the ones those need.
// Its findings are the ones the build found against the whole installation.
{
  const second = (await load(null, manifest.worker))(await load(manifest.js));
  const { samples: texts } = await second.init(base, manifest, base.startsWith('file:') ? { wasm: await bytes(manifest.wasm), bundle: await bytes(manifest.kernels[0].bundle) } : {});
  const home = await text('index.html');
  const tabs = [...home.matchAll(/<button role="tab"[^>]*id="ex-tab-\d+">([^<]+)<\/button>/g)].map((m) => m[1]).filter((n) => n.endsWith('.tex'));
  // each example panel's own data, not the live showcase's before them
  const built = [...home.matchAll(/id="ex-\d+"[^]*?<script type="application\/json" class="ex-data">(.*?)<\/script>/gs)].map((m) => JSON.parse(m[1]).lints.map((d) => `${d.code}@${d.start[0] + 1}`).sort().join());
  const kernel = manifest.kernels[0];
  fetchedHere.clear();
  for (const [i, name] of tabs.entries()) {
    t = performance.now();
    const entries = await fetchFor(second, kernel, [name, ...(manifest.projects?.[name] ?? [])]);
    const run = await second.run([...kernel.args, 'lint', '--format', 'lsp', '-f', name]);
    const found = JSON.parse(run.lines.filter((l) => l[0] === 'out').map((l) => l[1]).join('\n') || '[]')
      .flatMap((f) => (f.diagnostics ?? []).map((d) => `${d.data?.rule ?? d.code}@${d.range.start.line + 1}`)).sort().join();
    check(`${name} opens with its ${entries.length} bundles only`, found === built[i], `${entries.reduce((n, e) => n + e.size, 0)} bytes, ${Math.round(performance.now() - t)} ms; ${found === built[i] ? found.split(',').filter(Boolean).length + ' findings' : `got [${found}] want [${built[i]}]`}`);
  }
}

// The language server: a finding for the default file, a completion, a hover.
const uri = `file:///work/${manifest.sample}`;
let source = samples[manifest.sample];
const lsp = (method, params, id) => engine.lsp({ jsonrpc: '2.0', method, params, ...(id ? { id } : {}) });
// An edit, then the analysis the page asks for once typing pauses: its messages.
const edit = async (version, text) => [
  ...await lsp('textDocument/didChange', { textDocument: { uri, version }, contentChanges: [{ text }] }),
  ...await lsp('satex/analyze', {}),
];
t = performance.now();
await lsp('initialize', { capabilities: {} }, 1);
// the open file's findings come with the analysis the page asks for, as after an edit
const published = [...await lsp('textDocument/didOpen', { textDocument: { uri, languageId: 'latex', version: 1, text: source } }), ...await lsp('satex/analyze', {})]
  .findLast((m) => m.method === 'textDocument/publishDiagnostics' && m.params.uri === uri);
const codes = published?.params.diagnostics.map((d) => d.data?.rule ?? d.code) ?? [];
source = source.replace('\\end{document}', '\\gre\n\\end{document}');
await edit(2, source);
const line = source.split('\n').findIndex((l) => l === '\\gre');
const [completion] = await lsp('textDocument/completion', { textDocument: { uri }, position: { line, character: 4 } }, 2);
// An array, or a list the server cut to the best (`isIncomplete`).
const items = Array.isArray(completion?.result) ? completion.result : completion?.result?.items ?? [];
check('lsp', codes.length > 0 && items.some((i) => i.label === 'greet'), `${Math.round(performance.now() - t)} ms, diagnostics [${codes.join(', ')}], ${items.length} completions`);
check('completion marks the document\'s own commands', items.find((i) => i.label === 'greet')?.detail?.endsWith('(document)'));
const nl = source.split('\n').findIndex((l) => l.includes('\\newcommand'));
const [hover] = await lsp('textDocument/hover', { textDocument: { uri }, position: { line: nl, character: 3 } }, 3);
const card = hover?.result?.contents?.value ?? '';
check('hover shows a usage line, no raw signature', /⟨arg1⟩/.test(card) && !/signature:/.test(card), card.split('\n').filter(Boolean).slice(0, 3).join(' | '));

// The same findings as `satex lint`, info ones included, on an unsaved buffer.
const unsaved = samples[manifest.sample].replace('\\begin{document}', '\\def\\foo{}\n\\begin{document}');
const pub = (await edit(3, unsaved))
  .find((m) => m.method === 'textDocument/publishDiagnostics' && m.params.uri === uri);
const lspFound = (pub?.params.diagnostics ?? []).map((d) => `${d.data?.rule ?? d.code}/${['', 'error', 'warning', 'info', 'info'][d.severity]}`);
const lintRun = await engine.run(['lint', '--format', 'json', '-f', manifest.sample], { [`/work/${manifest.sample}`]: new TextEncoder().encode(unsaved) });
const lintFound = JSON.parse(lintRun.lines.filter((l) => l[0] === 'out').map((l) => l[1]).join('\n') || '[]').map((r) => `${r.code}/${r.severity}`);
check('unused-definition shown as by lint', lspFound.includes('unused-definition/info') && lspFound.sort().join() === lintFound.sort().join(), `lsp [${lspFound}] lint [${lintFound}]`);

// A primitive's syntax, only while the name means the primitive.
const defLine = unsaved.split('\n').findIndex((l) => l === '\\def\\foo{}');
const hoverAt = async (text, line, id) => {
  await edit(10 + id, text);
  return (await lsp('textDocument/hover', { textDocument: { uri }, position: { line, character: 2 } }, id))[0]?.result?.contents?.value ?? '';
};
const prim = await hoverAt(unsaved, defLine, 5);
check('primitive hover shows the syntax', /\\def⟨control sequence⟩⟨parameter text⟩\{⟨replacement text⟩\}/.test(prim) && /TeX primitive/.test(prim), prim.split('\n').filter(Boolean).slice(0, 3).join(' | '));
const relet = unsaved.replace('\\def\\foo{}', '\\let\\def\\relax\n\\def\\foo{}');
const notPrim = await hoverAt(relet, defLine + 1, 6);
check('no syntax once \\def is redefined', !/TeX primitive|⟨control sequence⟩/.test(notPrim), notPrim.split('\n').filter(Boolean).slice(0, 2).join(' | '));

// a finding underlines the whole call, not one character
const redefined = '\\newcommand{\\section}{x}';
const wide = (await edit(4, source.replace('\\begin{document}', `${redefined}\n\\begin{document}`)))
  .find((m) => m.method === 'textDocument/publishDiagnostics' && m.params.uri === uri);
const rline = source.replace('\\begin{document}', `${redefined}\n\\begin{document}`).split('\n').findIndex((l) => l === redefined);
const marks = (wide?.params.diagnostics ?? []).filter((d) => d.range.start.line === rline);
check('finding underlines the whole call', marks.some((d) => d.range.end.line === rline && d.range.end.character - d.range.start.character === redefined.length),
  marks.map((d) => `${d.data?.rule ?? d.code} ${d.range.start.character}-${d.range.end.character}`).join(', ') || 'no finding on the line');
// a source cited in running text is a link
const cited = Object.entries(manifest.rules).find(([, r]) => /\[tex\.web § \d+\]\(https:\/\/[^)]*tex\.web#L\d+\)/.test(r.explanation ?? ''));
check('tex.web reference in an explanation is a link', !!cited, cited?.[0] ?? 'no explanation links one');

// every TeXbook link names its chapter in its text, on the rules page, in explanations and in references
const books = [...(await text('wiki/lints.html')).matchAll(/<a href="[^"]*texbook[^"]*">([^<]*)<\/a>/g)].map((m) => m[1])
  .concat(Object.values(manifest.rules).flatMap((r) => [...(r.explanation ?? '').matchAll(/\[([^\]]*)\]\(https:[^)]*texbook[^)]*\)/g)].map((m) => m[1])
    .concat((r.references ?? []).filter((x) => /texbook/.test(x.url ?? '')).map((x) => x.cite ?? ''))));
check('TeXbook links carry their chapter', books.length > 3 && books.every((t) => /chapter|appendix/.test(t)), books.filter((t) => !/chapter|appendix/.test(t)).slice(0, 3).join(' | '));
const landing = await text('index.html');
check('landing examples are highlighted without JS', /<span class="ln"[^>]*>.*?<span class="tok-cmd[ "]/.test(landing));
check('landing shows no static command output', !/<pre class="output"|\$ satex explain/.test(landing));
// the showcase: every image sized (no layout shift) and served, in both themes
const shotImgs = [...landing.matchAll(/<img src="(showcase\/[^"]+)" width="\d+" height="\d+"/g)].map((m) => m[1]);
const shotMissing = [];
for (const src of shotImgs.flatMap((s) => [s, s.replace(/png$/, 'webp'), s.replace(/png$/, 'avif')])) if (!(base.startsWith('file:') ? existsSync(fileURLToPath(base + src)) : (await fetch(base + src).catch(() => ({}))).ok)) shotMissing.push(src);
check('landing showcase images are sized and served', shotImgs.length >= 4 && shotImgs.length % 2 === 0 && !shotMissing.length, shotMissing.join(' '));
// its videos are sized, lazy (no source until site.js loads one) and served in both formats; each try-it link names its widget
const shotVideos = [...landing.matchAll(/<video class="shot-video[^"]*" width="\d+" height="\d+" muted playsinline preload="none"[^>]* data-src="([^"]+)"><\/video>/g)].map((m) => m[1]);
const videoMissing = [];
for (const src of shotVideos.flatMap((s) => [`${s}.webm`, `${s}.mp4`])) if (!(base.startsWith('file:') ? existsSync(fileURLToPath(base + src)) : (await fetch(base + src).catch(() => ({}))).ok)) videoMissing.push(src);
check('landing showcase videos are sized, lazy and served', shotVideos.length >= 2 && !videoMissing.length, videoMissing.join(' '));
check('landing try-it links open their widget', [...landing.matchAll(/class="shot-try" href="play\/index\.html#([^"]+)"/g)].filter((m) => /&amp;widget=/.test(m[1])).length >= 6);
// every command of every example comes with what satex explain says of it
const exData = [...landing.matchAll(/<script type="application\/json" class="ex-data">(.*?)<\/script>/gs)].map((m) => JSON.parse(m[1]));
const exCommands = [...landing.matchAll(/data-cs="([^"]*)"/g)].map((m) => m[1].replace(/&amp;/g, '&').replace(/&quot;/g, '"'));
const unexplained = [...new Set(exCommands)].filter((c) => !exData.some((d) => d.cs[c]?.length));
check('every landing command is explained', exData.length >= 3 && exCommands.length > 10 && !unexplained.length, unexplained.join(' '));
check('explain records charge no hook to \\twice', !(exData.flatMap((d) => d.cs['\\twice'] ?? []).some((r) => (r.expands ?? []).some((x) => /everypar/.test(x)))),
  JSON.stringify(exData.flatMap((d) => d.cs['\\twice'] ?? []).map((r) => r.expands)));

await browser();
process.exit(failed ? 1 : 0);


function chromium() {
  if (process.env.CHROME) return process.env.CHROME;
  const root = join(homedir(), '.cache/ms-playwright');
  const dirs = existsSync(root) ? readdirSync(root).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse() : [];
  return dirs.map((d) => ['chrome-linux64/chrome', 'chrome-linux/chrome'].map((c) => join(root, d, c)).find(existsSync)).find(Boolean);
}

async function browser() {
  const chrome = chromium();
  if (!chrome) return console.log('skip: no Chromium for the browser checks (set CHROME)');
  const profile = mkdtempSync(join(tmpdir(), 'satex-test-'));
  // the DevTools protocol over a pipe (fds 3 and 4, messages ending in NUL): no port to open or find
  const proc = spawn(chrome, ['--headless=new', '--no-sandbox', '--disable-gpu', '--remote-debugging-pipe', `--user-data-dir=${profile}`,
    '--allow-file-access-from-files', '--window-size=1440,1000', 'about:blank'], { stdio: ['ignore', 'ignore', process.env.CHROME_LOG ? 'inherit' : 'ignore', 'pipe', 'pipe'] });
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  try {
    let id = 0, session, rest = '';
    const replies = new Map(), errors = [], logs = [];
    const ws = { close: () => proc.stdio[3].end() };
    const raw = (method, params = {}, sessionId = session) => new Promise((r) => { replies.set(++id, r); proc.stdio[3].write(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }) + '\0'); });
    proc.stdio[4].on('data', (chunk) => {
      const parts = (rest + chunk.toString()).split('\0');
      rest = parts.pop();
      for (const part of parts) if (part) ws.onmessage({ data: part });
    });
    ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.id) replies.get(m.id)?.(m);
      else if (m.method === 'Runtime.consoleAPICalled' && /warning|error/.test(m.params.type)) logs.push(m.params.args.map((x) => x.description ?? x.value).join(' ').slice(0, 200));
      else if (m.method === 'Runtime.exceptionThrown') errors.push(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text);
    };
    let page;
    for (let i = 0; i < 100 && !page; i++) { await sleep(100); page = (await raw('Target.getTargets', {}, null)).result?.targetInfos?.find((t) => t.type === 'page'); }
    session = (await raw('Target.attachToTarget', { targetId: page.targetId, flatten: true }, null)).result.sessionId;
    const send = (method, params = {}) => raw(method, params);
    const js = async (expr) => {
      const r = await send('Runtime.evaluate', { expression: `(async () => { ${expr} })()`, awaitPromise: true, returnByValue: true });
      if (r.result?.exceptionDetails) throw new Error(`${expr.slice(0, 80)}: ${r.result.exceptionDetails.exception?.description}`);
      return r.result?.result?.value;
    };
    const until = async (expr, ms = 120000) => { for (const end = Date.now() + ms; Date.now() < end; await sleep(150)) if (await js(expr).catch(() => false)) return true; return false; };
    const key = async (key, modifiers = 0, code = key) => {
      for (const type of ['keyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key, code, modifiers, windowsVirtualKeyCode: { Enter: 13, Escape: 27, F2: 113, Tab: 9, Backspace: 8, ArrowLeft: 37, z: 90 }[key] ?? 0, ...(key === 'Enter' ? { text: '\r' } : {}) });
    };
    const mouse = async (x, y, opts = {}) => { for (const t of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type: t, x, y, button: 'left', clickCount: 1, ...opts }); };
    /** The screen point of `needle` (its `at`-th character) in the editor's text. */
    const spot = async (needle, at = 1) => (await settled(needle, at)) ?? (await shot('spot-miss'), Promise.reject(new Error(`no ${needle} in the editor of ${await js(`return document.querySelector('#play-tabs [aria-selected=true]')?.textContent`)}: ${await js(`return document.querySelectorAll('.cm-line').length + ' lines: ' + [...document.querySelectorAll('.cm-line')].map((l) => l.textContent).join(' | ').slice(0, 200) + ' / content: ' + (document.querySelector('.cm-content')?.textContent ?? '').slice(0, 100)`)}`)));
    // the editor renders a file's lines a moment after its tab is chosen
    const settled = async (needle, at) => { for (let i = 0; i < 25; i++, await sleep(200)) { const p = await place(needle, at); if (p) return p; } return null; };
    const place = (needle, at = 1) => js(`for (const l of document.querySelectorAll('.cm-line')) { const i = l.textContent.indexOf(${JSON.stringify(needle)}); if (i < 0) continue;
      const w = document.createTreeWalker(l, NodeFilter.SHOW_TEXT); let n, off = i + ${at};
      while ((n = w.nextNode())) { if (off <= n.length) { const r = document.createRange(); r.setStart(n, off); r.setEnd(n, off); const b = r.getBoundingClientRect(); return [b.left + 1, b.top + b.height / 2]; } off -= n.length; } } return null;`);
    /** A screenshot into $SHOTS, when set: what the checks saw. */
    const shot = async (name) => {
      if (!process.env.SHOTS) return;
      mkdirSync(process.env.SHOTS, { recursive: true });
      const r = await send('Page.captureScreenshot', { format: 'png' });
      writeFileSync(join(process.env.SHOTS, `${name}.png`), Buffer.from(r.result.data, 'base64'));
    };
    const upload = async (input, files) => {
      const { result } = (await send('Runtime.evaluate', { expression: `document.getElementById('${input}')` })).result;
      const r = await send('DOM.setFileInputFiles', { files, objectId: result.objectId });
      if (r.error) throw new Error(`upload: ${r.error.message}`);
    };
    await send('Runtime.enable');
    await send('DOM.enable');

    // the landing page: the examples are colored by the editor's tokenizer and keep their indentation
    await send('Page.navigate', { url: base + 'index.html' });
    check('examples are highlighted', await until(`return document.querySelectorAll('.example pre.src .tok-cmd').length > 3`, 30000));
    check('examples show no command comment', await js(`return ![...document.querySelectorAll('.example pre.src')].some((p) => /^% satex/.test(p.textContent))`));
    const center = (sel) => js(`const e = document.querySelector(${JSON.stringify(sel)}); e.scrollIntoView({ block: 'center' }); const b = e.getBoundingClientRect(); return [b.left + Math.min(4, b.width / 2), b.top + b.height / 2]`);
    // sizes and places within the example, so a scroll by a subpixel does not count
    const layout = `const f = document.querySelector('.example:not([hidden])').getBoundingClientRect(); return [...document.querySelectorAll('.example:not([hidden]) :is(pre.src, .ex-side, .ln, .lint, .src-cs)')].map((e) => { const r = e.getBoundingClientRect(); return [r.x - f.x, r.y - f.y, r.width, r.height].map(Math.round).join(); }).join(' ')`;
    // the macros example's tab: its \\twice explains itself on hover, a card with its signature as chips
    await js(`[...document.querySelectorAll('.examples [role=tab]')].find((t) => /macros/.test(t.textContent)).click()`);
    let at = await center('.example:not([hidden]) [data-cs="\\\\twice"]');
    const before = await js(layout);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
    check('landing command hover card', await until(`const t = document.querySelector('.tip-card:not([hidden])'); return t && /twice/.test(t.querySelector('.x-name')?.textContent) && t.querySelector('.sig .arg[title]')`, 5000),
      await js(`return document.querySelector('.tip-card')?.textContent.slice(0, 120) ?? 'no card'`));
    await shot('landing-command-hover');
    const after = await js(layout);
    check('landing hover moves nothing', before === after, before === after ? '' : `${before} -> ${after}`);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 2, y: 2 });
    // the first landing example with a finding: findings are wavy underlines, with the message on hover
    let faulty = '#none';
    for (const id of await js(`return [...document.querySelectorAll('.examples [role=tab]')].map((t) => t.id)`)) {
      await js(`document.getElementById('${id}').click()`);
      const panel = await js(`return document.getElementById('${id}').getAttribute('aria-controls')`);
      if (await until(`return !!document.querySelector('#${panel} .lint[data-lint]')`, 3000)) { faulty = '#' + panel; break; }
    }
    check('wavy underlines', await js(`const l = [...document.querySelectorAll('${faulty} .lint')]; return l.length > 0 && l.every((e) => getComputedStyle(e).textDecorationStyle === 'wavy')`));
    const steady = await js(layout);
    at = await center(`${faulty} .lint[data-lint]`);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
    check('landing finding hover', await until(`return (document.querySelector('.tip-card:not([hidden]) .tip-lint')?.textContent ?? '').trim().length > 0`, 5000));
    await shot('landing-finding-hover');
    check('landing finding hover moves nothing', steady === await js(layout));
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 2, y: 2 });
    // the macros example: a click on a command pins its full card, signature chips explained; beside it, what it typesets
    await js(`[...document.querySelectorAll('.examples [role=tab]')].find((t) => /macros/.test(t.textContent)).click()`);
    at = await center('.example:not([hidden]) [data-cs="\\\\twice"]');
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
    await mouse(...at);
    await until(`return document.querySelector('.tip-card:not([hidden])')?.dataset.pin === '1'`, 5000);
    const panel = await js(`const t = document.querySelector('.tip-card'); return [t.querySelector('.x-name')?.textContent, t.textContent, [...t.querySelectorAll('.sig .arg')].map((a) => a.title)]`);
    await shot('landing-macros');
    check('macros example: a click pins the card of \\twice', panel[0] === '\\twice' && /#1#1/.test(panel[1]) && panel[2].length === 1 && /mandatory/.test(panel[2][0]), JSON.stringify(panel).slice(0, 200));
    check('examples show what they typeset', await js(`return [...document.querySelectorAll('.example:not(.live) .ex-side')].every((s) => s.querySelector('.ts'))`));
    check('no raw JSON on the landing page', await js(`return ![...document.querySelectorAll('.ex-side, .tip-card')].some((e) => /[{\\[]"|"max"|"min"/.test(e.textContent))`));
    await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: 2, y: 2, button: 'left', clickCount: 1 });
    await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: 2, y: 2, button: 'left', clickCount: 1 });
    await js(`[...document.querySelectorAll('.examples [role=tab]')].find((t) => /spacing/.test(t.textContent)).click()`);
    check('typographic findings marked', await js(`const t = [...document.querySelectorAll('.example:not([hidden]) .lint.typo')]; return t.length >= 2 && t.some((e) => e.textContent === '-')`));

    // the lint rules page: one card per rule, the table links to them, the contents grouped
    await send('Page.navigate', { url: base + 'wiki/lints.html' });
    check('rule cards', await until(`return document.querySelectorAll('section.rule h3[id] + .badges .sev').length > 50`, 30000));
    check('rule table links to cards', await js(`const a = document.querySelector('main table a[href^="#"]'); return !!document.getElementById(a.getAttribute('href').slice(1))?.closest('section.rule')`));
    check('rule entry shows the default severity once', await js(`return [...document.querySelectorAll('section.rule')].every((c) => c.querySelectorAll('.sev').length === 1 && !/ — /.test(c.querySelector('.badges').textContent))`));
    check('contents grouped by category', await js(`return document.querySelectorAll('#toc .grp').length > 3`));

    // the playground: no lint run on load, the findings come from the language server
    await send('Page.navigate', { url: base + 'play/index.html' });
    check('ready without a run', await until(`return document.getElementById('play-status').dataset.done === 'ready'`, 240000),
      await js(`return document.getElementById('play-status').textContent`));
    await shot('play-ready');
    // ---- the wishes of the playground round: each checked in the built page
    const wishes = async () => {
      const root = mkdtempSync(join(tmpdir(), 'satex-fresh-'));
      const editorText = `document.querySelector('.cm-content').textContent`;
      const quiet = `document.getElementById('lsp-status').textContent !== 'checking…' && !document.getElementById('play-status').classList.contains('busy')`;
      /** A folder with one main.tex becomes the project; resolves once its first analysis is shown. */
      const open = async (name, text) => {
        const d = join(root, name);
        mkdirSync(d, { recursive: true });
        writeFileSync(join(d, 'main.tex'), text);
        await upload('play-folder', [d]);
        await until(`return ${editorText} === ${JSON.stringify(text.replace(/\n/g, ''))}`, 30000);
        await sleep(500);
        return until(`return ${quiet}`, 120000);
      };
      check('the editor is CodeMirror', await until(`return !!document.querySelector('.cm-content')`, 60000), logs.slice(-3).join(' | '));
      // SECTION=name: only the wish sections whose name holds it
      const section = async (name, f) => { if (process.env.SECTION && !name.includes(process.env.SECTION)) return; try { await f(); } catch (e) { check(`${name}`, false, e.message.slice(0, 300)); } };
      const findings = `[...document.querySelectorAll('#lsp-diagnostics li')].map((li) => li.textContent)`;
      const unresolved = (names) => `${findings}.filter((t) => /unresolved-file|not in this installation/.test(t))`;
      // 2: a missing package is fetched, however deep the next one lies
      await open('pgf', '\\documentclass{article}\n\\usepackage{pgfmath}\n\\begin{document}\n$\\pgfmathparse{2+3}$\n\\end{document}\n');
      const pgf = await until(`return /pgfmath/.test(document.getElementById('play-status').dataset.packages ?? '') && ${quiet} && ${unresolved()}.length === 0`, 120000);
      check('pgfmath is fetched on demand and resolves', pgf, await js(`return (document.getElementById('play-status').dataset.packages ?? '') + ' | ' + ${findings}.join(' / ').slice(0, 300)`));
      await shot('w2-pgfmath');
      // 3: the written files: a card per file, its runs, the second run's .aux against the first's
      await open('written', '\\documentclass{article}\n\\makeatletter\n\\AtBeginDocument{\\immediate\\write\\@mainaux{\\@percentchar\\space seen \\@ifundefined{r@sec:a}{no}{yes}}}\n\\makeatother\n\\begin{document}\n\\tableofcontents\n\\section{Intro}\\label{sec:a}\nSee \\ref{sec:a}.\n\\end{document}\n');
      await js(`document.querySelector('.side-tabs [data-panel=play-written]').click()`);
      const wrote = await until(`return [...document.querySelectorAll('#written-body .wf-head code')].map((c) => c.textContent).join() === 'main.aux,main.toc'`, 30000);
      const runs = await js(`return [...document.querySelectorAll('#written-body .wf')[0]?.querySelectorAll('[data-view]') ?? []].map((b) => b.textContent)`);
      await js(`document.querySelector('#written-body .wf [data-view="1"]')?.click()`);
      const diff = await js(`const w = document.querySelector('#written-body .wf'); if (!w) return [[], []]; return [[...w.querySelectorAll('.wf-del')].map((e) => e.textContent), [...w.querySelectorAll('.wf-add')].map((e) => e.textContent)]`);
      check('written files: a card each, 2 runs, the diff shows the changed line', wrote && runs.join() === 'run 1,run 2' && diff[0].join() === '% seen no' && diff[1].join() === '% seen yes',
        JSON.stringify({ runs, diff, text: await js(`return document.getElementById('written-body').textContent.slice(0, 200)`) }));
            await shot('w3-written');
      // 1: the run pop-up
      await js(`document.getElementById('t-time').click()`);
      await sleep(400);
      const pop = await js(`const p = document.getElementById('t-detail'), r = p.getBoundingClientRect();
        return { shown: !p.hidden, tiles: p.querySelectorAll('.rp-nums > span').length, bars: p.querySelectorAll('.rp-bar').length, over: p.scrollWidth > p.clientWidth + 1 || r.right > innerWidth || r.left < 0, w: Math.round(r.width) }`);
      check('run pop-up: tiles, slow files, no overflow', pop.shown && pop.tiles >= 5 && pop.bars >= 1 && !pop.over, JSON.stringify(pop));
      await shot('w1-runpop-light');
      await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
      await sleep(300);
      await shot('w1-runpop-dark');
      await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
      await js(`document.getElementById('t-time').click()`);
      // 9: a click on the switch itself, with the mouse
      const sw = await js(`document.querySelector('.side-tabs [data-panel=play-typeset]').click(); const b = document.getElementById('typeset-keys'); b.scrollIntoView({ block: 'center' }); const r = b.getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2, b.checked, !document.getElementById('play-typeset').hidden]`);
      await mouse(sw[0], sw[1]);
      await sleep(150);
      const after = await js(`return [document.getElementById('typeset-keys').checked, !document.getElementById('play-typeset').hidden]`);
      check('the switch flips on a real click, the panel stays', after[0] !== sw[2] && after[1] === sw[3], JSON.stringify([sw, after]));
      await mouse(sw[0], sw[1]);
      // 5: the cache store is bounded and stamped
      const idb = await js(`const db = await new Promise((ok, no) => { const r = indexedDB.open('satex-playground'); r.onsuccess = () => ok(r.result); r.onerror = no; });
        const n = (s) => new Promise((ok) => { const q = db.transaction(s).objectStore(s).count(); q.onsuccess = () => ok(q.result); });
        return { stores: [...db.objectStoreNames], cache: await n('cache'), used: await n('used') }`);
      check('cache entries carry a last-used stamp', idb.stores.includes('used') && idb.used > 0 && idb.used <= idb.cache, JSON.stringify(idb));
      await section('8', async () => {
      // 8: two tooltips never overlap: the latest hovered wins
      await open('hover', '\\documentclass{article}\n\\begin{document}\nText \\emph{x}\n\\end{document}\n');
      // one card for both: what it shows says which hover won
      const visible = `[...document.querySelectorAll('.tip-card')].filter((t) => !t.hidden && t.getBoundingClientRect().width > 0).map((t) => t.querySelector(':scope > .hover') ? 'editor' : 'chip')`;
      const ed = await spot('\\emph', 3);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: ed[0], y: ed[1] });
      const cm = await until(`return ${visible}.includes('editor')`, 10000);
      await js(`document.getElementById('lib-open').click()`);
      await until(`return document.querySelectorAll('#lib-list .cs-ref').length > 3`, 60000);
      const chip = await js(`const e = document.querySelector('#lib-list .cs-ref'); e.scrollIntoView({ block: 'center' }); const b = e.getBoundingClientRect(); return [b.x + 4, b.y + b.height / 2]`);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: chip[0], y: chip[1] });
      await until(`return ${visible}.includes('chip')`, 10000);
      const one = await js(`return ${visible}`);
      check('chip card hides the editor hover', cm && one.length === 1 && one[0] === 'chip', JSON.stringify(one));
      await shot('w8-latest-hover');
      await js(`document.getElementById('play-library').close()`);
      const ed2 = await spot('\\emph', 3);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: ed2[0], y: ed2[1] });
      await until(`return ${visible}.includes('editor')`, 10000);
      const two = await js(`return ${visible}`);
      check('editor hover hides the chip card', two.length === 1 && two[0] === 'editor', JSON.stringify(two));
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
      });
      await section('typeset', async () => {
      // the Typeset panel: a table as a grid, its \hline a rule; a reference's key where the \ref stands, never inside a word
      await open('table', '\\documentclass{article}\n\\begin{document}\n\\section{Intro}\\label{sec:a}\nSee Section~\\ref{sec:a} for the hyphenation of everything \\makebox[2cm]{boxed}.\n\n\\begin{tabular}{lc}\nMethod & Score \\\\ \\hline\nOurs & 0.92 \\\\\nBase & 1.5 \\\\\n\\end{tabular}\n\\end{document}\n');
      await js(`document.querySelector('.side-tabs [data-panel=play-typeset]').click(); const k = document.getElementById('typeset-keys'); if (!k.checked) k.click()`);
      await until(`return !!document.querySelector('#typeset-body .ts-table')`, 30000);
      const ts = await js(`const t = document.querySelector('#typeset-body .ts-table');
        const rows = [...t.querySelectorAll('.ts-tr')].filter((r) => r.textContent.trim()).map((r) => [...r.querySelectorAll('.ts-td')].map((c) => c.textContent.trim()));
        const chips = [...document.querySelectorAll('#typeset-body .ts-key-ref')].map((k) => [k.previousSibling?.textContent ?? '', k.nextSibling?.textContent ?? '']);
        return { rows, ruled: t.querySelectorAll('.ts-ruled').length, chips }`);
      await shot('typeset-table');
      check('a table is a grid: rows, cells, the hline a rule', JSON.stringify(ts.rows) === JSON.stringify([['Method', 'Score'], ['Ours', '0.92'], ['Base', '1.5']]) && ts.ruled === 1, JSON.stringify(ts));
      check('a reference key stands where the \\ref does', ts.chips.length === 1 && !ts.chips.some(([a, b]) => /\w$/.test(a) && /^\w/.test(b)), JSON.stringify(ts.chips));
      // a paragraph sets its words on full lines: a box in it (a label, a \hbox to the width) never squeezes the text into a column
      const narrow = await js(`return [...document.querySelectorAll('#typeset-body .ts-text')].filter((t) => t.textContent.trim().length > 12)
        .filter((t) => t.getBoundingClientRect().width < 8 * parseFloat(getComputedStyle(t).fontSize)).map((t) => t.textContent.trim().slice(0, 30))`);
      check('typeset paragraphs keep the width of the pane', narrow.length === 0, JSON.stringify(narrow));
      // once the run has ended no view of the open file is dimmed: a view is stale only while a newer analysis is pending
      const dim = await until(`return !document.querySelector('#status.busy, .busy') && !document.querySelector('#typeset-body.stale, #explain-body.stale, #findings-list.stale, #play-list.stale')`, 15000);
      check('no view is dimmed after the run ends', dim, await js(`return [...document.querySelectorAll('.stale')].map((e) => e.id || e.className).join(' ')`));
      });
      await section('outline', async () => {
      // a label belongs to the section it stands in, though the run reports labels after every section
      await open('outline', '\\documentclass{article}\n\\begin{document}\n\\section{Alpha}\\label{sec:alpha}\nText~\\cite{k}.\n\n\\subsection{Beta}\nMore.\n\\end{document}\n');
      await until(`return document.querySelectorAll('#outline-body .o-sec').length >= 2`, 30000);
      const owners = await js(`return [...document.querySelectorAll('#outline-body .o-sec')].map((li) => [li.querySelector('.o-go')?.textContent ?? '', [...li.querySelectorAll('.o-label')].map((l) => l.textContent)])`);
      const alpha = owners.find(([h]) => h.includes('Alpha')), beta = owners.find(([h]) => h.includes('Beta'));
      check('a label sits under the section it follows', alpha?.[1].includes('sec:alpha') && !beta?.[1].includes('sec:alpha'), JSON.stringify(owners));
      });
      await section('draw', async () => {
      // drawn shapes, strokes of points in [-1, 1]² (y down), among every package's math symbols: each finds its command
      // among the first three, and a mirrored twin never ranks above it
      await open('draw', '\\documentclass{article}\n\\begin{document}\nText.\n\\end{document}\n');
      const line = (a, b, n = 16) => Array.from({ length: n + 1 }, (_, i) => [a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n]);
      const path = (ps) => ps.slice(1).flatMap((p, i) => line(ps[i], p, 8).slice(i ? 1 : 0));
      const ring = (f, n = 64) => Array.from({ length: n + 1 }, (_, i) => f(i / n * 2 * Math.PI));
      const shapes = {
        Rightarrow: [line([-1, -0.3], [0.6, -0.3]), line([-1, 0.3], [0.6, 0.3]), path([[0.3, -0.7], [1, 0], [0.3, 0.7]])],
        rightarrow: [line([-1, 0], [1, 0]), path([[0.5, -0.45], [1, 0], [0.5, 0.45]])],
        leftarrow: [line([1, 0], [-1, 0]), path([[-0.5, -0.45], [-1, 0], [-0.5, 0.45]])],
        int: [path([[0.45, -0.8], [0.3, -1], [0.12, -0.95], [0.04, -0.7], [0, 0], [-0.04, 0.7], [-0.12, 0.95], [-0.3, 1], [-0.45, 0.8]])],
        sum: [path([[0.7, -1], [-0.7, -1], [0.15, 0], [-0.7, 1], [0.7, 1]])],
        infty: [ring((t) => [Math.cos(t) / (1 + Math.sin(t) ** 2), 0.9 * Math.sin(t) * Math.cos(t) / (1 + Math.sin(t) ** 2)])],
        leq: [path([[0.8, -0.9], [-0.8, -0.35], [0.8, 0.2]]), line([-0.8, 0.65], [0.8, 0.65])],
        circ: [ring((t) => [Math.cos(t), Math.sin(t)])],
        plus: [line([-1, 0], [1, 0]), line([0, -1], [0, 1])],
        times: [line([-1, -1], [1, 1]), line([1, -1], [-1, 1])],
      };
      const expected = { Rightarrow: /\\Rightarrow\b/, rightarrow: /\\(rightarrow|to)\b/, leftarrow: /\\(leftarrow|gets)\b/, int: /\\int\b/, sum: /\\sum\b/,
        infty: /\\infty\b/, leq: /\\(leq|le)\b/, circ: /\\(circ|bigcirc|mdlgwhtcircle|mdwhtcircle|lgwhtcircle)\b/, plus: /\\(plus|mathplus)\b|^\+/, times: /\\times\b/ };
      const twins = { Rightarrow: /\\Leftarrow\b/, rightarrow: /\\leftarrow\b/, leftarrow: /\\rightarrow\b/ };
      await js(`document.getElementById('draw-open').click(); document.querySelector('[data-f=from][data-v=all]').click(); document.querySelector('[data-f=mode][data-v=math]').click()`);
      const got = {};
      for (const [name, strokes] of Object.entries(shapes)) {
        await js(`document.getElementById('draw-clear').click()`);
        const b = await js(`const r = document.getElementById('draw-pad').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2, r.width * 0.32]`);
        for (const stroke of strokes) {
          const at = ([x, y]) => ({ x: b[0] + x * b[2], y: b[1] + y * b[2] });
          await send('Input.dispatchMouseEvent', { type: 'mousePressed', ...at(stroke[0]), button: 'left', buttons: 1, clickCount: 1 });
          for (const p of stroke) await send('Input.dispatchMouseEvent', { type: 'mouseMoved', ...at(p), button: 'left', buttons: 1 });
          await send('Input.dispatchMouseEvent', { type: 'mouseReleased', ...at(stroke.at(-1)), button: 'left', buttons: 0, clickCount: 1 });
        }
        // the match of the whole drawing: one per stroke ended, the last one shown
        await until(`return document.querySelectorAll('#draw-list li .draw-glyph').length > 2`, 60000);
        // the matcher refines in a worker: the answer stands once it is done
        await until(`return !document.querySelector('#draw-list.refining')`, 60000);
        await sleep(300);
        got[name] = await js(`return [...document.querySelectorAll('#draw-list > li')].map((li) => li.querySelector('.draw-glyph').textContent + ' ' + li.querySelector('.draw-what').textContent)`);
        await shot(`draw-${name}`);
      }
      const rank = (name, re) => got[name].findIndex((t) => re.test(t));
      const top1 = Object.keys(shapes).filter((n) => rank(n, expected[n]) === 0), top3 = Object.keys(shapes).filter((n) => rank(n, expected[n]) >= 0 && rank(n, expected[n]) < 3);
      console.log(`draw precision: top-1 ${top1.length}/${Object.keys(shapes).length} (${top1.join(' ')}), top-3 ${top3.length}/${Object.keys(shapes).length}`);
      // the measured bar (top-3 8 of 10 on 2026-10-31): a drop below it is a regression of the matcher
      check('drawn shapes find their command among the first three', top3.length >= 8, JSON.stringify(Object.fromEntries(Object.entries(got).map(([n, t]) => [n, t.slice(0, 3)]))));
      check('a mirrored twin never ranks above the drawn arrow', Object.entries(twins).every(([n, re]) => rank(n, re) < 0 || rank(n, re) > rank(n, expected[n])),
        JSON.stringify(Object.fromEntries(Object.keys(twins).map((n) => [n, [rank(n, expected[n]), rank(n, twins[n])]]))));
      await js(`document.getElementById('play-draw').close()`);
      });
      await section('lib', async () => {
      // the library: each environment once by its name, Try wraps it in \\begin…\\end and typesets it
      await js(`document.getElementById('lib-open').click()`);
      await until(`return document.querySelectorAll('#lib-list .cs-ref').length > 3`, 60000);
      await js(`document.querySelector('.lib-kind[data-kind=environment]').click()`);
      const env = await js(`return { names: [...document.querySelectorAll('#lib-list .cs-ref')].map((e) => e.textContent), count: +document.querySelector('.lib-kind[data-kind=environment] b').textContent }`);
      check('library lists each environment once, by name', env.names.includes('itemize') && env.count === env.names.length && new Set(env.names).size === env.names.length
        && !env.names.some((n) => n.startsWith('\\') || env.names.includes(n.replace(/^end/, '')) && n !== n.replace(/^end/, '')), JSON.stringify(env).slice(0, 300));
      await js(`const s = document.getElementById('lib-search'); s.value = 'itemize'; s.dispatchEvent(new Event('input')); document.querySelector('#lib-list .lib-try-it').click()`);
      const tried = await js(`const t = document.getElementById('lib-text'); t.setRangeText('\\\\item one', t.selectionStart, t.selectionStart, 'end'); document.getElementById('lib-run').click(); return t.value`);
      const out = await until(`return /one/.test(document.querySelector('#lib-out .typeset')?.textContent ?? '')`, 60000);
      check('Try wraps an environment and typesets it', out && /^\\begin\{itemize\}\n\s*\\item one\n\\end\{itemize\}$/.test(tried), JSON.stringify(tried));
      await shot('w-lib-env-try');
      await js(`document.getElementById('play-library').close()`);
      });
      await section('hover', async () => {
      // chip cards: every ⟨argN⟩ explains itself, a type's path stops below `any`, the card never covers the editor's hover
      await open('chips', '\\documentclass{article}\n\\newcommand{\\twice}[1]{#1#1}\n\\begin{document}\n\\twice{a}\n\\setlength{\\parindent}{1em}\n\\end{document}\n');
      const tipOf = async (needle) => {
        await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
        await sleep(300);
        await send('Input.dispatchMouseEvent', { type: 'mouseMoved', ...Object.fromEntries((await spot(needle, 3)).map((v, i) => [i ? 'y' : 'x', v])) });
        await until(`return !!document.querySelector('.tip-card .arg[data-tip]')`, 15000);
        const chips = await js(`return [...document.querySelectorAll('.tip-card .arg[data-tip]')].map((c) => { const b = c.getBoundingClientRect(); return [b.x + 4, b.y + b.height / 2, !!c.querySelector('a.ty')]; })`);
        const c = chips.find((x) => x[2]) ?? chips[0];
        await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: c[0], y: c[1] });
        await until(`return !document.querySelector('.tip-note')?.hidden`, 5000);
        return js(`const t = document.querySelector('.tip-note'), a = t.getBoundingClientRect(), b = document.querySelector('.tip-card').getBoundingClientRect();
          return { what: t.querySelector('.ty-what')?.textContent ?? '', chain: t.querySelector('.ty-chain')?.textContent ?? '', over: a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom }`);
      };
      const plain = await tipOf('\\twice{a}');
      await shot('w-hover-untyped');
      check('an untyped ⟨arg1⟩ shows its card, beside the hover', /1\. argument/.test(plain.what) && !plain.over, JSON.stringify(plain));
      const typed = await tipOf('\\setlength');
      await shot('w-hover-typed');
      check('a type card: its path without any, beside the hover', typed.chain && !/\bany\b/.test(typed.chain) && !typed.over, JSON.stringify(typed));
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
      });
      await section('6, 7', async () => {
      // 6, 7: completion at once, even right after an edit; tikz keys; argument counts
      await open('comp', '\\documentclass{article}\n\\usepackage{tikz}\n\\newcommand{\\x}[]{x}\n\\begin{document}\n\\begin{tikzpicture}[]\n\\draw (0,0) -- (1,1);\n\\end{tikzpicture}\n\n\\end{document}\n');
      const popup = `[...document.querySelectorAll('.cm-tooltip-autocomplete li')].map((l) => l.textContent)`;
      const at1 = await spot('\\begin{tikzpicture}[', 20);
      await mouse(...at1);
      await send('Input.insertText', { text: 'th' });
      const tk = await until(`return ${popup}.some((t) => /^thick/.test(t))`, 15000);
      check('tikzpicture options complete to tikz keys', tk, await js(`return ${popup}.slice(0, 8).join(' ')`));
      await shot('w7-tikz-keys');
      await key('Escape');
      const at2 = await spot('\\newcommand{\\x}[', 16);
      await mouse(...at2);
      await key(' ', 2, 'Space');
      const cnt = await until(`return ${popup}.filter((t) => /^[0-9]/.test(t)).length >= 5`, 15000);
      check('\\newcommand[ offers argument counts', cnt, await js(`return ${popup}.slice(0, 10).join(' ')`));
      await shot('w7-newcommand-count');
      await key('Escape');
      const at3 = await spot('\\end{tikzpicture}', 17);
      await mouse(...at3);
      await send('Input.insertText', { text: '\n' });
      // timed in the page, frame by frame: the polling here is coarser than the bound
      await js(`window.__seen = 0; const t0 = performance.now(), f = () => { if (${popup}.some((t) => /^pgfmath/.test(t))) window.__seen = Math.round(performance.now() - t0); else requestAnimationFrame(f); }; requestAnimationFrame(f)`);
      await send('Input.insertText', { text: '\\pgfm' });
      const fast = await until(`return window.__seen > 0`, 15000);
      const took = await js(`return window.__seen`);
      check('typing \\pgfm shows candidates within 300 ms of the keystroke', fast && took < 300, `${took} ms, ${await js(`return ${popup}.slice(0, 5).join(' ')`)}`);
      await shot('w6-fast-complete');
      await key('Escape');
      });
      await section('11, 12', async () => {
      // 11, 12: the trace of \begin has steps; the library switch re-renders without a query
      await js(`window.__q = 0; const o = Worker.prototype.postMessage; Worker.prototype.postMessage = function (m, ...r) { if (m?.type === 'lsp' && JSON.stringify(m).includes('satex/query')) window.__q++; return o.call(this, m, ...r); }`);
      await open('trace', '\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\pgfmathparse{sign(42)}\n\\pgfmathresult\n\\end{document}\n');
      const traceAt = async (needle, at) => {
        const p = await spot(needle, at);
        await mouse(...p);
        await js(`document.querySelectorAll('#play-ctx li').forEach((l) => l.remove())`);
        await mouse(...p, { button: 'right', buttons: 2 });
        await until(`return [...document.querySelectorAll('#play-ctx li')].some((l) => /Trace/.test(l.textContent))`, 30000);
        await js(`[...document.querySelectorAll('#play-ctx li')].find((l) => /Trace/.test(l.textContent)).click()`);
        return until(`return !!document.querySelector('#trace-body .tr-now') || /no step|failed|not used/.test(document.getElementById('trace-body').textContent)`, 120000);
      };
      await traceAt('\\begin{document}', 3);
      const steps = await js(`return [document.getElementById('trace-total').textContent, document.getElementById('trace-body').textContent.slice(0, 160)]`);
      check('the trace of \\begin has steps', /\d+ steps?/.test(steps[0]) && !/no step/.test(steps[1]), JSON.stringify(steps));
      await shot('w11-trace-begin');
      const q0 = await js(`return window.__q`), n0 = steps[0];
      await js(`document.getElementById('trace-internal').click()`);
      await sleep(1500);
      const n1 = await js(`return document.getElementById('trace-total').textContent`);
      check('library switch re-renders without a new query', await js(`return window.__q`) === q0 && n1 !== n0, `${n0} -> ${n1}, ${q0} queries`);
      await js(`document.getElementById('trace-internal').click()`);
      await js(`document.getElementById('trace-close').click()`);
      });
      await section('leapyear', async () => {
      await js(`document.getElementById('play-exsect').open = true; [...document.querySelectorAll('#play-examples *')].find((e) => e.children.length === 0 && e.textContent.trim().startsWith('leapyear'))?.click()`);
      await until(`return /leapyear/.test(document.querySelector('.tabs [aria-selected=true]')?.textContent ?? '')`, 30000);
      await until(`return ${quiet}`, 120000);
      const p = await spot('\\ckleapyear{2000}', 3);
      await mouse(...p, { button: 'right', buttons: 2 });
      await until(`return [...document.querySelectorAll('#play-ctx li')].some((l) => /Trace/.test(l.textContent))`, 30000);
      await js(`[...document.querySelectorAll('#play-ctx li')].find((l) => /Trace/.test(l.textContent)).click()`);
      await until(`return !!document.querySelector('#trace-body .tr-now')`, 120000);
      await js(`document.querySelectorAll('#trace-body details').forEach((d) => (d.open = true))`);
      const text = await js(`return document.getElementById('trace-body').textContent`);
      check('the leap-year trace shows the values assigned', /= 2000/.test(text) && /2000 → 500/.test(text), text.slice(0, 300));
      await shot('w-leapyear-trace');
      await js(`document.getElementById('play-trace').close()`);
      });
      await section('trace viewer', async () => {
      // the trace dialog: library groups, the how-row, the stepper's token stream, breadcrumb, cards, forks, progress
      const body = `document.getElementById('trace-body')`;
      const traceOf = async (needle, at, live = null) => {
        const p = await spot(needle, at);
        await mouse(...p);
        await js(`document.querySelectorAll('#play-ctx li').forEach((l) => l.remove())`); // no stale menu
        await mouse(...p, { button: 'right', buttons: 2 });
        if (!await until(`return [...document.querySelectorAll('#play-ctx li')].some((l) => /Trace/.test(l.textContent))`, 30000)) {
          await shot(`trv-noctx`);
          throw new Error(`no Trace in the menu at ${needle}: ${await js(`return document.getElementById('play-ctx')?.textContent.slice(0, 200)`)}`);
        }
        await js(`[...document.querySelectorAll('#play-ctx li')].find((l) => /Trace/.test(l.textContent)).click()`);
        if (live) await live();
        return until(`return !!document.querySelector('#trace-body .tr-now') || /no step|failed|not used/.test(${body}.textContent)`, 120000);
      };
      const setLib = (on) => js(`const b = document.getElementById('trace-internal'); if (b.checked !== ${on}) b.click()`);
      await open('trv', '\\documentclass{article}\n\\usepackage{tikz}\n\\newcommand{\\twice}[1]{#1#1}\n\\newcommand{\\pick}{\\ifnum\\pdfuniformdeviate 10>5 late\\else early\\fi}\n\\begin{document}\n\\pgfmathparse{sign(42)}\n\\pgfmathresult\n\\twice{ab}\n\\pick\\relax\n\\end{document}\n');
      // 7: live progress while the run goes
      let seen = [];
      const watch = async () => { for (let k = 0; k < 12; k++) { seen.push(await js(`return document.getElementById('trace-live')?.textContent ?? ''`)); await sleep(250); } };
      await setLib(false);
      await traceOf('\\pgfmathresult', 3, watch);
      seen = seen.filter(Boolean);
      check('trace shows live progress: phase and elapsed', seen.some((t) => /^Tracing: .+ · \d+\.\d\d s$/.test(t)) && new Set(seen).size > 1, JSON.stringify(seen.slice(0, 4)));
      // 1: library work folds into one named line; the switch opens it without a query
      const groups = await js(`return [...document.querySelectorAll('#trace-body .tr-group')].map((g) => [g.tagName, g.textContent])`);
      const libRows = await js(`return [...document.querySelectorAll('#trace-body .tr-tree .tr-at')].filter((a) => !/main\\.tex/.test(a.textContent)).length`);
      check('switch off: library work is one line per trigger, none of its steps', groups.some(([t, x]) => t === 'DIV' && /^paragraph starts \(everypar hook, \d+ steps\)$/.test(x)) && libRows === 0, JSON.stringify([groups, libRows]));
      await shot('trv1-folded');
      const q0 = await js(`return window.__q ?? 0`);
      await setLib(true);
      await sleep(800);
      const opened = await js(`const g = document.querySelector('#trace-body details.tr-group'); if (!g) return null; g.open = true; await new Promise((r) => setTimeout(r, 200)); return g.querySelectorAll('li[data-i]').length`);
      check('switch on: the group opens to its steps, no new query', opened > 0 && (await js(`return window.__q ?? 0`)) === q0, `${opened} kids`);
      await shot('trv1-open');
      await setLib(false);
      await js(`document.getElementById('trace-close').click()`);
      // 2, 3, 4: \\twice{ab} on one row; the stepper's ends; chips by catcode; the breadcrumb
      await traceOf('\\twice{ab}', 3);
      const how = await js(`const h = document.querySelector('#trace-body .tr-tree .tr-how'), res = h?.querySelector('.tr-res'), c = res?.querySelector('code');
        return h && [h.textContent, Math.abs(res.getBoundingClientRect().top - c.getBoundingClientRect().top) < 4]`);
      check('\\twice{ab}: body, binding, result on one row, the arrow with its result', how && /body #1#1/.test(how[0]) && /#1 = ab/.test(how[0]) && /→ abab/.test(how[0]) && how[1], JSON.stringify(how));
      const ends = await js(`const d = (m) => document.querySelector('[data-move=' + m + ']').disabled; return [d('prev'), d('next')]`);
      check('stepper: back disabled at the first step', ends[0] === true, JSON.stringify(ends));
      const ins = await js(`return [...document.querySelectorAll('#trace-stream .tr-ins')].map((t) => t.textContent).join('')`);
      check('stream: what the step put back highlighted', ins === 'abab', ins);
      const few = await js(`return document.querySelectorAll('.tr-legend .tr-key').length`);
      await js(`document.querySelector('.tr-legend-btn').click()`);
      const all = await js(`return [document.querySelectorAll('.tr-legend .tr-key').length, document.querySelector('.tr-legend').textContent]`);
      check('catcode legend: the categories in the stream, all behind the toggle, by name', few < 17 && all[0] === 17 && /11 letter/.test(all[1]), JSON.stringify([few, all]));
      await shot('trv2-twice');
      await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
      await sleep(300);
      await shot('trv2-twice-dark');
      await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
      for (let k = 0; k < 400 && !(await js(`return document.querySelector('[data-move=next]').disabled`)); k++) await js(`document.querySelector('[data-move=next]').click()`);
      check('stepper: next and over disabled at the last step', await js(`return document.querySelector('[data-move=next]').disabled && document.querySelector('[data-move=over]').disabled`));
      await setLib(true);
      await until(`return !!document.querySelector('#trace-body .tr-now')`, 5000);
      await js(`document.getElementById('trace-close').click()`);
      await traceOf('\\pgfmathresult', 3);
      for (let k = 0; k < 60 && (await js(`return document.querySelectorAll('.tr-crumbs .tr-crumb').length`)) < 3; k++) await js(`document.querySelector('[data-move=next]').click()`);
      const crumbs = await js(`return [...document.querySelectorAll('.tr-crumbs .tr-crumb')].map((c) => [c.textContent, c.classList.contains('on')])`);
      check('breadcrumb: the enclosing calls, the current one marked', crumbs.length >= 2 && crumbs.at(-1)[1] && crumbs.slice(0, -1).every((c) => !c[1]), JSON.stringify(crumbs));
      await shot('trv4-crumbs');
      const toks = await js(`return [...document.querySelectorAll('#trace-stream .tr-chips .tr-tok:not(.tr-ins)')].map((a) => [a.textContent, a.title, a.getAttribute('href')])`);
      check('chips carry their catcode number', await js(`return [...document.querySelectorAll('#trace-stream a.tr-tok')].every((a) => a.querySelector('.tr-cc')?.textContent === a.getAttribute('href').split('-').pop())`));
      await shot('trv3-chips');
      check('stream: the input ahead as chips, each with its catcode and wiki link', toks.length > 3 && toks.every(([x, t, h]) => /^\\/.test(x) ? !h : /\(\d+\)$|catcode \d+/.test(t) && /wiki\/domains\.html#catcode-\d+$/.test(h)), JSON.stringify(toks.slice(0, 4)));
      // 5: every name hoverable, its meaning at the step; click pins; the state panel
      const cs = await js(`const e = document.querySelector('#trace-stream .tr-chips .tr-cs'); if (!e) return null; e.scrollIntoView({ block: 'center' }); const b = e.getBoundingClientRect(); return [b.x + 3, b.y + b.height / 2, e.dataset.cs]`)
        ?? await js(`const e = document.querySelector('#trace-now .tr-cs'); e.scrollIntoView({ block: 'center' }); const b = e.getBoundingClientRect(); return [b.x + 3, b.y + b.height / 2, e.dataset.cs]`);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: cs[0], y: cs[1] });
      const tip = await until(`const t = document.querySelector('.tip-card'); return t && !t.hidden && /meaning at /.test(t.textContent)`, 60000);
      check('a name in the trace shows its meaning at the step', tip, `${cs[2]}: ${await js(`return document.querySelector('.tip-card')?.textContent.slice(0, 160)`)}`);
      await mouse(cs[0], cs[1]);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
      await sleep(400);
      check('a click pins the card', await js(`const t = document.querySelector('.tip-card'); return !t.hidden && t.dataset.pin === '1'`));
      await shot('trv5-pinned');
      await js(`document.querySelector('#trace-now').click()`);
      // a name the step's input does not hold: one query of the run stopped at the step
      const head = await js(`const e = document.querySelector('#trace-now .tr-now-head .tr-cs'); const b = e.getBoundingClientRect(); return [b.x + 3, b.y + b.height / 2, e.dataset.cs, !!document.querySelector('.tr-means li[data-n="' + CSS.escape(e.dataset.cs) + '"]')]`);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: head[0], y: head[1] });
      const lazy = await until(`const t = document.querySelector('.tip-card'); return t && !t.hidden && t.textContent.includes(${JSON.stringify(head[2])}) && /meaning at step/.test(t.textContent)`, 120000);
      check('a name not queued at the step: its meaning from the run stopped there', lazy && !head[3], `${head[2]}: ${await js(`return document.querySelector('.tip-card')?.textContent.slice(0, 120)`)}`);
      await shot('trv5-lazy');
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
      await js(`const s = document.querySelector('.tr-state'); s.open = true; s.scrollIntoView()`);
      await js(`document.querySelector('.tr-allnames').click()`);
      const every = await until(`return document.querySelectorAll('.tr-every li[data-n]').length > 100`, 120000);
      await js(`const f = document.querySelector('.tr-filter'); f.value = 'twice'; f.dispatchEvent(new Event('input', { bubbles: true }))`);
      const filtered = await js(`return [...document.querySelectorAll('.tr-every li[data-n]:not([hidden])')].map((l) => l.dataset.n)`);
      check('state panel: all names at the step, searchable', every && filtered.includes('\\twice') && filtered.every((n) => /twice/.test(n)), JSON.stringify(filtered.slice(0, 5)));
      await shot('trv5-state');
      await setLib(false);
      await js(`document.getElementById('trace-close').click()`);
      // 6: an undecided \\ifnum: the joined view and each fork, the stepper following
      await traceOf('\\pick\\relax', 2);
      const forks = await js(`return [...document.querySelectorAll('#trace-fork option')].map((o) => o.value)`);
      const n0 = await js(`return document.getElementById('trace-total').textContent`);
      await js(`const s = document.getElementById('trace-fork'); s.value = s.options[1].value; s.dispatchEvent(new Event('change', { bubbles: true }))`);
      await until(`return !!document.querySelector('#trace-body .tr-now')`, 5000);
      const n1 = await js(`return [document.getElementById('trace-total').textContent, document.getElementById('trace-fork').value]`);
      check('forks: joined plus each fork, the stepper follows', forks.length >= 3 && forks[0] === '' && n1[1] === forks[1] && n1[0] !== n0, JSON.stringify([forks, n0, n1]));
      await js(`document.querySelector('#trace-body .tr-all').open = true; document.querySelectorAll('#trace-body .tr-tree details').forEach((d) => d.open = true)`);
      await sleep(300);
      await shot('trv6-fork');
      await js(`const s = document.getElementById('trace-fork'); s.value = ''; s.dispatchEvent(new Event('change', { bubbles: true }))`);
      await until(`return !!document.querySelector('#trace-body .tr-now')`, 5000);
      await js(`document.querySelector('#trace-body .tr-all').open = true; document.querySelectorAll('#trace-body .tr-tree details').forEach((d) => d.open = true)`);
      await sleep(300);
      await shot('trv6-joined');
      await js(`document.getElementById('trace-close').click()`);
      // 8: a missing package does not end the run before the traced call
      await open('trmiss', '\\documentclass{article}\n\\usepackage{nosuchpackage}\n\\begin{document}\nHi\n\\end{document}\n');
      await traceOf('\\begin{document}', 3);
      const miss = await js(`return [document.getElementById('trace-total').textContent, ${body}.textContent.slice(0, 120)]`);
      check('trace with a missing package has steps', /\d+ steps?/.test(miss[0]) && !/no step/.test(miss[1]), JSON.stringify(miss));
      await js(`document.getElementById('trace-close').click()`);
      });
      await section('10', async () => {
      // 10: user macros with delimited parameters read naturally: literal text as ‹…›, no stray letters
      await open('weird', '\\documentclass{article}\n\\def\\Weird key: #1, value: #2, magic: #3\;{#1#2#3}\n\\def\\WeirdMore key: #1, value: #2, magic: #3\;super #4\; {}\n\\begin{document}\n\\Weird key: a, value: b, magic: c\;\n\\end{document}\n');
      const wp = await spot('\\Weird key', 3);
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: wp[0], y: wp[1] });
      await until(`return !!document.querySelector('.tip-card .hover')`, 10000);
      const chips = await js(`return [[...document.querySelectorAll('.tip-card .sig .arg')].map((a) => a.textContent), document.querySelector('.tip-card .hover')?.textContent.slice(0, 300)]`);
      check('hover signature of \\Weird: literal text and natural chips', chips[0].length >= 3 && chips[0].some((t) => /key/.test(t)) && !chips[0].some((t) => /^(pp|U)$/.test(t)), JSON.stringify(chips));
      await shot('w10-weird-hover');
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
      });
      await section('examples', async () => {
      // the examples beside pdflatex's verdict: each with the finding satex computes for it
      await send('Page.navigate', { url: base + 'index.html' });
      await until(`return document.querySelectorAll('.examples [role=tab]').length > 3`, 30000);
      for (const [name, code] of [['csname.tex', 'undefined-control-sequence'], ['option-clash.tex', 'option-clash'], ['rerun.tex', 'undefined-reference']]) {
        const got = await js(`const t = [...document.querySelectorAll('.examples [role=tab]')].find((t) => t.textContent === '${name}'); if (!t) return null; t.click();
          const f = document.getElementById(t.getAttribute('aria-controls')); return { codes: [...f.querySelectorAll('.eol')].map((c) => c.title.split(' ')[0]), pdflatex: /% pdflatex/.test(f.querySelector('pre.src').textContent) }`);
        check(`example ${name}: pdflatex's verdict and satex's ${code}`, got?.pdflatex && got.codes.includes(code), JSON.stringify(got));
      }
      });
      await section('live', async () => {
      // the landing page's last tab: no engine until it opens, then the editor, typeset text and underlined findings, again after an edit
      for (const [w, h, dark] of [[1440, 900, false], [1440, 900, true], [375, 800, false], [375, 800, true]]) {
        await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: w < 500 });
        await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: dark ? 'dark' : 'light' }] });
        await send('Page.navigate', { url: base + 'index.html' });
        await until(`return !!document.querySelector('.example.live')`, 30000);
        await sleep(800);
        const early = await js(`return performance.getEntriesByType('resource').filter((r) => /\\.wasm|\\.bin\\.gz/.test(r.name)).length`);
        await js(`const t = document.getElementById(document.querySelector('.example.live').getAttribute('aria-labelledby')); t.scrollIntoView({ block: 'start' }); t.click()`);
        const done = await until(`return /Analyzed in/.test(document.querySelector('.example.live .live-status').textContent)`, 120000);
        await until(`return document.querySelectorAll('.example.live .cm-lintRange').length > 0`, 10000);
        const got = await js(`const f = document.querySelector('.example.live'); return { text: f.querySelector('.live-ts').textContent.slice(0, 120), found: f.querySelectorAll('.cm-lintRange').length,
          colored: f.querySelectorAll('.cm-content .tok-cmd').length, over: document.documentElement.scrollWidth > innerWidth }`);
        check(`own tab at ${w}px${dark ? ' dark' : ''}: loads when opened, an editor, typesets with findings`, !early && done && /Hello, world!/.test(got.text) && got.found > 0 && got.colored > 3 && !got.over, JSON.stringify({ early, ...got }));
        await shot(`w-live-${w}${dark ? '-dark' : ''}`);
      }
      const pe = await spot('\\end{document}', 0);
      await mouse(...pe);
      await send('Input.insertText', { text: 'A new line -- here.\n' });
      check('own tab follows an edit', await until(`return /new line/.test(document.querySelector('.example.live .live-ts').textContent)`, 30000));
      await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
      await send('Emulation.clearDeviceMetricsOverride');
      });
      await section('docs', async () => {
      // documentation pages: the contents list heading names only, nothing overflows at 1440 and 1024 px
      for (const [page, w] of [['guide.html', 1440], ['guide.html', 1024], ['wiki/domains.html', 1440], ['wiki/domains.html', 1024]]) {
        await send('Emulation.setDeviceMetricsOverride', { width: w, height: 900, deviceScaleFactor: 1, mobile: false });
        await send('Page.navigate', { url: base + page });
        await until(`return document.querySelectorAll('#toc a').length > 3`, 30000);
        const got = await js(`return { toc: [...document.querySelectorAll('#toc a')].map((a) => a.textContent), over: document.documentElement.scrollWidth > innerWidth }`);
        check(`${page} at ${w}px: contents by heading name, no overflow`, !got.toc.some((t) => /#|: /.test(t)) && !got.over, JSON.stringify(got).slice(0, 200));
        await shot(`w-docs-${page.replace(/\W+/g, '-')}-${w}`);
      }
      await send('Emulation.clearDeviceMetricsOverride');
      });
      await section('4', async () => {
      // 4: the landing hero
      for (const [w, h, name] of [[1440, 900, 'w4-hero-1440'], [375, 800, 'w4-hero-375']]) {
        await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: w < 500 });
        await send('Page.navigate', { url: base + 'index.html' });
        // one line types each claim in turn: a dot per claim, labelled with it
        await until(`return document.querySelectorAll('.claim-dots button').length > 1`, 30000);
        const hero = await js(`const rot = document.querySelector('.claim-rot').getBoundingClientRect(), claim = document.querySelector('.claim').getBoundingClientRect();
          return { dots: document.querySelectorAll('.claim-dots button').length, claims: new Set([...document.querySelectorAll('.claim-dots button')].map((d) => d.getAttribute('aria-label'))).size, lead: !!document.querySelector('.hero .lead, .lead'), overflow: document.documentElement.scrollWidth > innerWidth, rotBelow: rot.top >= claim.top + 10, text: document.querySelector('.claim-rot .on')?.textContent }`);
        check(`hero at ${w}px: rotating claim on its own line, dots, no overflow, no tagline`, hero.dots > 1 && hero.dots === hero.claims && !hero.lead && !hero.overflow && hero.rotBelow, JSON.stringify(hero));
        await js(`document.querySelectorAll('.claim-dots button')[2].click()`);
        await sleep(3000);
        const jumped = await js(`const d = document.querySelectorAll('.claim-dots button'), t = [...document.querySelector('[data-knows]').childNodes].filter((n) => !n.classList?.contains('claim-complete')).map((n) => n.textContent).join('');
          return d[2].getAttribute('aria-selected') === 'true' && t.length > 3 && d[2].getAttribute('aria-label').startsWith(t) ? 2 : t`);
        check(`hero at ${w}px: a dot jumps to its claim`, jumped === 2, String(jumped));
        await shot(name);
        await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
        await sleep(300);
        await shot(`${name}-dark`);
        await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'light' }] });
      }
      await send('Emulation.clearDeviceMetricsOverride');
      });
      await section('drawpage', async () => {
      // Draw alone (draw/index.html): no site chrome, fits 320 px, a drawn circle finds candidates, ?theme=dark holds
      await send('Emulation.setDeviceMetricsOverride', { width: 320, height: 700, deviceScaleFactor: 1, mobile: true });
      await send('Page.navigate', { url: base + 'draw/index.html?theme=dark' });
      await until(`return !!self.satexDraw && !!document.getElementById('draw-pad')`, 30000);
      const b = await js(`const r = document.getElementById('draw-pad').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2, r.width * 0.3]`);
      const ring = [...Array(49)].map((_, i) => [b[0] + Math.cos(i / 48 * 2 * Math.PI) * b[2], b[1] + Math.sin(i / 48 * 2 * Math.PI) * b[2]]);
      await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: ring[0][0], y: ring[0][1], button: 'left', buttons: 1, clickCount: 1 });
      for (const [x, y] of ring) await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, button: 'left', buttons: 1 });
      await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: ring.at(-1)[0], y: ring.at(-1)[1], button: 'left', buttons: 0, clickCount: 1 });
      const found = await until(`return document.querySelectorAll('#draw-list li .draw-glyph').length > 2`, 60000);
      const page = await js(`return { header: !!document.querySelector('header.top'), dark: document.documentElement.dataset.theme, over: document.documentElement.scrollWidth > innerWidth }`);
      await shot('draw-page-320');
      check('Draw alone: candidates at 320 px, no chrome, the theme from the link', found && !page.header && page.dark === 'dark' && !page.over, JSON.stringify(page));
      await send('Emulation.clearDeviceMetricsOverride');
      await send('Page.navigate', { url: base + 'play/index.html' });
      await until(`return document.getElementById('play-status').dataset.done === 'ready'`, 180000);
      });
      await section('lenses', async () => {
      // the plugins' lenses show by default: the paper's tabular (its root reads an input) and the drawing
      const lenses = `[...document.querySelectorAll('.cm-lens')].map((b) => b.textContent)`;
      for (const [file, lens] of [['paper.tex', 'Edit table…'], ['spacing.tex', 'Edit drawing…']]) {
        await send('Page.navigate', { url: base + 'play/index.html#' + file });
        const seen = await until(`return ${lenses}.includes(${JSON.stringify(lens)})`, 180000);
        await shot(`lens-${file}`);
        check(`${file} shows "${lens}"`, seen, JSON.stringify(await js(`return ${lenses}`)));
      }
      await send('Page.navigate', { url: base + 'play/index.html' });
      await until(`return document.getElementById('play-status').dataset.done === 'ready'`, 180000);
      });
      await section('managers', async () => {
      // Intuition: the bibliography, glossary and index managers are one layout (search, chips, list, divider, detail), so each is checked
      // on the thesis example for the same facts. The bug pinned: the bibliography's list sat in an inner frame that did not fill its pane
      // at 35%, the glossary and index were accordions that looked unlike it, and delete stood where the uses should be inspected.
      await send('Page.navigate', { url: base + 'play/index.html?managers#thesis.tex' });
      await until(`return document.getElementById('play-status').dataset.done === 'ready'`, 180000);
      for (const kind of ['bibliography', 'glossary', 'index']) {
        await until(`return document.getElementById('lsp-status').textContent !== 'checking…' && await self.satexOpen({ widget: 'manager', at: '${kind}' })`, 180000);
        await until(`return !!document.querySelector('#play-view[open] .tv-md')`, 30000);
        await sleep(400);
        const f = await js(`const md = document.querySelector('#play-view .tv-md'); if (!md) return null;
          const l = md.querySelector('.tv-md-left').getBoundingClientRect().width, w = md.getBoundingClientRect().width, t = md.querySelector('.tv-md-table'), rows = md.querySelector('.tv-md-rows');
          return { pct: l / w * 100, fills: !!t && Math.abs(t.getBoundingClientRect().width - rows.clientWidth) < 2, selected: md.querySelectorAll('tbody tr.on').length, search: !!md.querySelector('input[aria-label=search]'),
            bar: !!md.querySelector('.tv-md-bar[role=separator][tabindex="0"]'), uses: document.querySelectorAll('#play-view .tv-use').length, danger: document.querySelectorAll('#play-view .tv-actions > .tv-danger:not(.tv-fold *)').length };`);
        check(`${kind} manager: shared layout, list 35%, table fills, a selection, uses listed, delete folded`, f && Math.abs(f.pct - 35) < 3 && f.fills && f.selected === 1 && f.search && f.bar && f.uses >= 1 && !f.danger, JSON.stringify(f));
        await shot(`manager-${kind}`);
        await js(`document.getElementById('play-view').close()`);
      }
      });
      await section('settings', async () => {
      // a satex.yaml key set in the settings dialog reaches the run: `clock` fixes what \\year reads
      await open('clock', '\\documentclass{article}\n\\begin{document}\nYear \\the\\year.\n\\end{document}\n');
      await js(`document.getElementById('cfg-open').click()`);
      await until(`return !!document.querySelector('#cfg-list [data-cfg="clock"]')`, 30000);
      await js(`const i = document.querySelector('#cfg-list [data-cfg="clock"]'); i.value = '1999-06-01T00:00:00'; i.dispatchEvent(new Event('change', { bubbles: true })); document.getElementById('play-config').close()`);
      await js(`document.querySelector('.side-tabs [data-panel=play-typeset]').click()`);
      const year = await until(`return /Year 1999/.test(document.getElementById('typeset-body').textContent)`, 120000);
      check('the clock setting changes \\the\\year', year, await js(`return document.getElementById('typeset-body').textContent.slice(0, 120)`));
      await js(`document.getElementById('cfg-open').click(); document.getElementById('cfg-reset').click(); document.getElementById('play-config').close()`);
      });
      await section('plugin tiles', async () => {
      // the Plugins tab: a tile per category of type plugins, each plugin with its switch, which reaches the settings
      await js(`document.getElementById('cfg-open').click(); document.querySelector('#cfg-tabs [data-tab=plugins]').click()`);
      const tiles = await until(`return document.querySelectorAll('#cfg-list .pt-tile').length > 1 && !!document.querySelector('#cfg-list .pt-tile [data-cfg="type_plugins.image.enabled"]')`, 30000);
      check('the Plugins tab shows a tile per category with switches', tiles, JSON.stringify(await js(`return [...document.querySelectorAll('#cfg-list .pt-tile h3')].map((h) => h.textContent)`)));
      await js(`const i = document.querySelector('#cfg-list [data-cfg="type_plugins.image.enabled"]'); i.checked = false; i.dispatchEvent(new Event('change', { bubbles: true }))`);
      check('a tile switch turns its plugin off', await until(`return JSON.parse(localStorage.getItem('satex-config') ?? '{}')['type_plugins.image.enabled'] === false`, 30000));
      await js(`document.getElementById('cfg-reset').click(); document.getElementById('play-config').close()`);
      });
      await section('pdflinks', async () => {
      // the compiled PDF's links (with the local server's TeX, or the browser's on a static host): areas over the pages,
      // an internal one scrolls to its target and moves the editor there, Back returns, a URL opens a new tab
      if (await js(`return document.getElementById('pdf-compile').disabled`)) return console.log('skip: no TeX to compile with');
      await open('links', '\\documentclass{article}\n\\usepackage{hyperref}\n\\begin{document}\n\\section{Intro}\\label{a}\nSee \\ref{b} and \\url{https://example.org}.\n\\newpage\n\\section{Two}\\label{b}\nBack to \\ref{a}.\n\\end{document}\n');
      await js(`document.querySelector('.side-tabs [data-panel=play-pdf]').click(); document.getElementById('pdf-compile').click()`);
      const built = await until(`return document.querySelectorAll('.pdf-page').length === 2 && document.querySelectorAll('.pdf-page img').length === [...document.querySelectorAll('.pdf-page img')].filter((i) => i.naturalWidth).length`, 180000);
      const links = await js(`return [...document.querySelectorAll('.pdf-link')].map((a) => [a.closest('.pdf-page').dataset.page, a.getAttribute('href'), a.target, a.title])`);
      check('the PDF has its links: two internal, one URL in a new tab', built && links.length === 3 && links.some(([p, h, t]) => p === '1' && h === 'https://example.org' && t === '_blank')
        && links.filter(([, h, , t]) => h === '#' && /^to page \d$/.test(t)).length === 2, JSON.stringify(links));
      const line = `document.querySelector('.cm-activeLine')?.textContent`;
      await js(`document.querySelector('.pdf-page[data-page="1"] .pdf-link[title="to page 2"]').click()`);
      const went = await until(`return /^\\\\section\\{Two\\}/.test(${line}) && !document.getElementById('pdf-back').hidden && document.querySelector('.pdf-page[data-page="2"] .pdf-target')`, 10000);
      await shot('pdf-link');
      await js(`document.getElementById('pdf-back').click()`);
      const back = await until(`return document.getElementById('pdf-back').hidden && document.querySelector('.pdf-pages').scrollTop < 20`, 10000);
      check('an internal link scrolls to its target and moves the editor, Back returns', went && back, await js(`return ${line} + ' | ' + document.querySelector('.pdf-pages').scrollTop`));
      // the link stays on its words at another zoom
      const onWord = `const a = document.querySelector('.pdf-link[href^=http]').getBoundingClientRect(), img = document.querySelector('.pdf-page[data-page="1"] img').getBoundingClientRect(); return [(a.left - img.left) / img.width, (a.top - img.top) / img.height].map((v) => v.toFixed(3)).join()`;
      const at1 = await js(onWord);
      await js(`document.getElementById('pdf-zoom-in').click()`);
      await sleep(300);
      check('links hold their place at every zoom', at1 === await js(onWord), at1);
      await js(`document.getElementById('pdf-zoom-out').click()`);
      });
      await section('settings', async () => {
      // a stored setting satex refuses never holds the start up: the page starts with the defaults
      await send('Page.navigate', { url: base + 'play/index.html' });
      await until(`return !!document.getElementById('play-status')`, 30000);
      await js(`localStorage.setItem('satex-config', JSON.stringify({ clock: 'unk' }))`);
      await send('Page.reload');
      const started = await until(`return document.getElementById('play-status').dataset.done === 'ready'`, 180000);
      check('an invalid stored setting does not block the start', started && await js(`return localStorage.getItem('satex-config') === '{}' && /Reset 1 setting \\(clock: unk\\)/.test(document.getElementById('play-toast').textContent)`),
        await js(`return document.getElementById('play-status').textContent + ' | ' + document.getElementById('play-toast').textContent`));
      });
    };
    if (process.env.NEW_ONLY) { await wishes(); check('no page errors', !errors.length, errors.join(' | ').slice(0, 300)); ws.close(); return; }
    check('Run sits in the console bar', await js(`return !!document.getElementById('play-run').closest('.ask')`));
    check('the engine selector shows its name only', await js(`const k = document.getElementById('play-kernel'); return k.selectedOptions[0].textContent === 'pdfTeX'`),
      await js(`return document.getElementById('play-kernel').selectedOptions[0].textContent`));
    check('links are neither underlined nor bold', await js(`return [...document.querySelectorAll('a')].every((a) => getComputedStyle(a).textDecorationLine === 'none' && +getComputedStyle(a).fontWeight < 600)`));
    check('GitHub is an icon in the header', await js(`return !!document.querySelector('.top nav a.icon svg')`));
    check('no output panel for plain lint', await js(`return document.getElementById('play-out').hidden`));
    // flowR's layout: the project's files in a tree left of the editor, the examples under them
    check('file tree beside the editor', await js(`const t = document.getElementById('play-tree'), e = document.getElementById('play-editor').getBoundingClientRect();
      return !t.hidden && t.getBoundingClientRect().right <= e.left + 1 && !!t.querySelector('.file.root[data-name="${manifest.sample}"]') && t.querySelectorAll('#play-examples [data-example]').length > 3`));
    for (const width of [1866, 1480]) {
      await send('Emulation.setDeviceMetricsOverride', { width, height: 1000, deviceScaleFactor: 1, mobile: false });
      await sleep(300);
      const fit = await js(`const r = (s) => document.querySelector(s).getBoundingClientRect(); return [r('.play-split').left, r('.play-split').right, r('#play-run').right, r('#play-link').right, r('.top nav').right, r('#theme').right, innerWidth, document.documentElement.scrollWidth]`);
      check(`playground spans the window at ${width}px`, fit[0] === 0 && Math.abs(fit[1] - fit[6]) < 2, JSON.stringify(fit));
      check(`header fits at ${width}px`, fit.slice(2, 6).every((x) => x <= fit[6]) && fit[7] <= fit[6], JSON.stringify(fit));
    }
    await send('Emulation.clearDeviceMetricsOverride');
    // panes snap: dragged to an edge a pane folds, a click on the splitter brings it back, near half it rests at half
    const dragTo = async (x) => {
      const d = await js(`const r = document.getElementById('play-divider').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]`);
      await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: d[0], y: d[1], button: 'left', clickCount: 1 });
      for (const f of [0.3, 0.7, 1]) await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: d[0] + (x - d[0]) * f, y: d[1], button: 'left', buttons: 1 });
      await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y: d[1], button: 'left', clickCount: 1 });
      await sleep(100);
      return js(`const s = document.querySelector('.play-split'); return [s.dataset.fold ?? '', s.style.getPropertyValue('--split')]`);
    };
    const folded = await dragTo(8);
    await shot('play-folded');
    const d = await js(`const r = document.getElementById('play-divider').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]`);
    await mouse(...d);
    await sleep(100);
    const back = await js(`return document.querySelector('.play-split').dataset.fold ?? ''`);
    const half = await dragTo(await js(`const r = document.querySelector('.play-split').getBoundingClientRect(); return r.x + r.width / 2 + 7`));
    check('panes snap and fold', folded[0] === 'editor' && back === '' && half[0] === '' && half[1] === '50.0%', JSON.stringify([folded, back, half]));
    check('header: kernel, command line, run, upload, link', await js(`return !document.querySelector('.act, #play-query, #play-reset') && !!document.querySelector('#play-kernel') && !!document.querySelector('#play-args')`));
    check('findings', await until(`return document.querySelectorAll('#lsp-diagnostics li[data-i]').length > 0`));
    // closed, the selector names the engine; open, each option adds its size
    const kernels = await js(`return [...document.querySelectorAll('#play-kernel option')].map((o) => [o.dataset.name + ' · ' + o.dataset.more, o.disabled, o.title])`);
    check('kernel labels carry their own size', kernels.some(([t]) => /^pdfTeX · [\d.]+ MiB$/.test(t)) && kernels.some(([t]) => /^plain TeX · \d+ KiB$/.test(t)) && !kernels.some(([t]) => /\+/.test(t)),
      kernels.map((k) => k[0]).join(' | '));
    check('LuaTeX listed as not available', kernels.some(([t, off, why]) => /^LuaTeX/.test(t) && off && /format budget/.test(why)));
    check('editor highlighted', await js(`return document.querySelectorAll('.cm-content .tok-cmd').length > 3`));

    // every finding, unfolded, shows one severity: its own
    const sevs = await js(`const out = []; for (const li of document.querySelectorAll('#lsp-diagnostics li[data-i]')) { li.querySelector('.why').click(); }
      await new Promise((r) => setTimeout(r, 100));
      for (const li of document.querySelectorAll('#lsp-diagnostics li[data-i]')) out.push([li.querySelectorAll('.shape, .sev').length, !!li.querySelector('.explain'), li.querySelector('.rn').textContent, li.querySelector('.shape').title]);
      return out;`);
    check('one severity per finding', sevs.length && sevs.every(([n, open]) => n === 1 && open), JSON.stringify(sevs));
    const differs = sevs.filter(([, , code, sev]) => manifest.rules[code] && manifest.rules[code].severity !== sev);
    console.log(`     ${differs.length} of them differ from their rule's default severity`);
    // hovering a finding moves and resizes nothing
    const boxes = `return [...document.querySelectorAll('#lsp-diagnostics li[data-i]')].slice(0, 3).map((li) => { const r = li.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(Math.round).join(); }).join(' ') + ' ' + document.getElementById('lsp-diagnostics').scrollHeight`;
    const still = await js(boxes);
    const first = await js(`const r = document.querySelector('#lsp-diagnostics li[data-i]').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]`);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: first[0], y: first[1] });
    await sleep(400);
    const hovered = await js(boxes);
    const shown = await js(`return getComputedStyle(document.querySelector('#lsp-diagnostics .acts')).opacity`);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
    await sleep(400);
    check('no layout shift on hover', still === hovered && shown === '1', `${still} -> ${hovered}`);
    check('actions only on hover', await js(`const a = document.querySelector('#lsp-diagnostics .acts'); return a && getComputedStyle(a).opacity === '0'`));

    // the command line: own commands first in `explain \…`, internals last
    await js(`const a = document.getElementById('play-args'); a.focus(); a.value = 'explain \\\\'; a.dispatchEvent(new Event('input'));`);
    await until(`return !document.getElementById('play-complete').hidden`, 20000);
    const ranked = await js(`return [...document.querySelectorAll('#play-complete li code')].map((c) => c.textContent)`);
    const internal = (l) => /[@_:]/.test(l);
    check('completion ranks own commands first', ranked[0] === '\\greet' && ranked.findIndex(internal) < 0 || ranked.findIndex(internal) > ranked.findIndex((l) => !internal(l)),
      ranked.slice(0, 6).join(' '));
    check('explain listed once', (await js(`const a = document.getElementById('play-args'); a.value = 'ex'; a.dispatchEvent(new Event('input')); await new Promise((r) => setTimeout(r, 50));
      return [...document.querySelectorAll('#play-complete li code')].filter((c) => c.textContent === 'explain').length`)) === 1);
    await js(`const a = document.getElementById('play-args'); a.value = 'explain \\\\greet'; a.dispatchEvent(new Event('input'));`);
    await js(`delete document.getElementById('play-status').dataset.done`);
    await key('Escape');
    await js(`document.getElementById('play-args').focus()`);
    await key('Enter');
    await until(`return document.getElementById('play-status').dataset.done !== undefined`, 120000);
    check('Enter runs explain into the Explain panel', await until(`return document.getElementById('play-out').hidden && /greet/.test(document.querySelector('#explain-body .xcard')?.textContent ?? '')`, 20000),
      await js(`return document.getElementById('play-status').textContent + ' | ' + document.getElementById('play-args').value`));
    await key('ArrowUp');
    check('history', (await js(`return document.getElementById('play-args').value`)) === 'explain \\greet');
    const xcard = await js(`const c = document.querySelector('#explain-body .xcard'); return [[...c.querySelectorAll('dt')].map((d) => d.textContent), c.textContent, [...c.querySelectorAll('.sig .arg')].map((a) => a.title)]`);
    check('explain card: signature first, no arity, no raw JSON', xcard[0][0] === 'signature' && !xcard[0].includes('arity') && !xcard[0].includes('takes') && !/[{\\[]"|"max"/.test(xcard[1]), JSON.stringify(xcard[0]));
    check('signature chips explain themselves', xcard[2].length === 1 && /^m: mandatory/.test(xcard[2][0]), JSON.stringify(xcard[2]));
    // --at completes
    const atItems = async (line) => { await js(`const a = document.getElementById('play-args'); a.focus(); a.value = ${JSON.stringify(line)}; a.selectionStart = a.selectionEnd = a.value.length; a.dispatchEvent(new Event('input'));`); await sleep(150);
      return js(`return [...document.querySelectorAll('#play-complete li')].map((li) => [li.querySelector('code').textContent, li.querySelector('.muted')?.textContent ?? ''])`); };
    const explainAt = await atItems('explain --at ');
    check('explain --at completes', ['preamble', 'document'].every((x) => explainAt.some(([l]) => l === x)) && explainAt.some(([l]) => /^hello\.tex:\d+$/.test(l)) && explainAt.every(([, d]) => d), JSON.stringify(explainAt));
    const scopeAt = await atItems('scope --at ');
    check('scope --at completes', scopeAt.some(([l]) => /^\d+:\d+$/.test(l)) && scopeAt.some(([l]) => l === `${manifest.sample}:`) && scopeAt.every(([, d]) => d), JSON.stringify(scopeAt.slice(0, 4)));
    await key('Escape');
    await js(`delete document.getElementById('play-status').dataset.done; document.getElementById('play-args').value = 'query definitions'`);
    await js(`document.getElementById('play-args').focus()`);
    await key('Enter');
    await until(`return document.getElementById('play-status').dataset.done !== undefined`, 120000);
    check('query definitions renders a table', await js(`const o = document.getElementById('play-output'); return !!o.querySelector('table.rows tbody tr') && !o.querySelector(':scope > pre')`),
      await js(`return document.getElementById('play-output').innerHTML.slice(0, 200)`));
    await js(`delete document.getElementById('play-status').dataset.done; document.getElementById('play-args').value = 'query distribution'`);
    await js(`document.getElementById('play-args').focus()`);
    await key('Enter');
    await until(`return document.getElementById('play-status').dataset.done !== undefined`, 120000);
    // its summary and its trees are records of two shapes: never one table with empty columns
    const tables = await js(`const o = document.getElementById('play-output'), ts = [...o.querySelectorAll('table.rows')];
      return [o.querySelectorAll('.card').length + ts.length, ts.every((t) => [...t.tHead.rows[0].cells].every((_, i) => [...t.tBodies[0].rows].some((r) => r.cells[i].textContent.trim()))),
        [...o.querySelectorAll('table.rows th, .card dt')].every((h) => h.getBoundingClientRect().height < 24), o.innerHTML.slice(0, 160)]`);
    check('one table or card per record shape, labels on one line', tables[0] >= 2 && tables[1] && tables[2], JSON.stringify(tables));
    await js(`document.getElementById('play-out').scrollIntoView()`);
    await shot('play-query-distribution');

    // hover card: a usage line, a footer, no raw signature
    let p = await spot('\\newcommand', 3);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: p[0], y: p[1] });
    const hoverText = await until(`return document.querySelector('.tip-card .hover')`, 20000) && await js(`return document.querySelector('.tip-card .hover').textContent`);
    check('hover card', /⟨arg1⟩/.test(hoverText) && !/signature/.test(hoverText) && await js(`return !!document.querySelector('.tip-card .foot')`), hoverText.slice(0, 100));
    check('hover usage colored by argument kind', await js(`const u = [...document.querySelectorAll('.tip-card code.usage .arg')].map((a) => a.className.split('-').pop());
      return ['flag', 'mand', 'opt'].every((k) => u.includes(k)) && !document.querySelector('.tip-card .arg .arg')`), await js(`return document.querySelector('.tip-card code.usage')?.innerHTML.slice(0, 300) ?? 'no usage'`));
    await shot('play-hover-newcommand');

    // the Explain panel follows the mouse, no click
    p = await spot('\\greet{', 2);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: p[0], y: p[1] });
    check('explain panel follows the mouse', await until(`return /greet/.test(document.getElementById('explain-body').textContent)`, 20000),
      await js(`return document.getElementById('play-explain').textContent.slice(0, 120)`));

    check('explain panel shows the full record', await until(`return /greet/.test(document.querySelector('#explain-body .xcard .x-name')?.textContent ?? '') && /Hello, #1!/.test(document.getElementById('explain-body').textContent)`, 30000));
    // a kernel place opens the file read-only at its line
    p = await spot('\\newcommand', 3);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: p[0], y: p[1] });
    const ltx = await until(`return !!document.querySelector('#explain-body a.place[data-path$="latex.ltx"]')`, 30000) &&
      await js(`const a = document.querySelector('#explain-body a.place[data-path$="latex.ltx"]'); return [Number(a.dataset.line), document.querySelector('#explain-body a.upstream')?.href ?? ''];`);
    check('latex.ltx place with its upstream sources', ltx && ltx[0] > 0 && /github\.com\/latex3\/latex2e\/tree\/HEAD\/base$/.test(ltx[1]), JSON.stringify(ltx));
    if (ltx) {
      await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 3, y: 3 });
      await js(`document.querySelector('#explain-body a.place[data-path$="latex.ltx"]').click()`);
      check('clicking latex.ltx:N opens it at line N', await until(`return document.querySelector('#play-tabs [aria-selected=true]')?.textContent === 'latex.ltx' && document.querySelector('.cm-activeLineGutter')?.textContent === '${ltx[0]}'`, 20000),
        await js(`return document.querySelector('#play-tabs [aria-selected=true]')?.textContent + ' ' + document.querySelector('.cm-activeLineGutter')?.textContent`));
      await js(`document.querySelector('#play-tabs [data-name="${manifest.sample}"]').click()`);
      await until(`return document.querySelector('#play-tabs [aria-selected=true]')?.textContent === '${manifest.sample}'`, 5000);
    }

    // context menu and F2 rename at `\greet`
    p = await spot('\\greet{', 2);
    await mouse(...p);
    await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: p[0], y: p[1], button: 'right', clickCount: 1 });
    await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: p[0], y: p[1], button: 'right', clickCount: 1 });
    const menu = await until(`return !document.getElementById('play-ctx').hidden`, 5000) && await js(`return document.getElementById('play-ctx').textContent`);
    check('context menu', ['Go to definition', 'Find references', 'Rename symbol', 'Explain', 'Show expansions', 'Copy link to this line'].every((x) => menu.includes(x)), menu);
    await js(`[...document.querySelectorAll('#play-ctx li')].find((li) => li.textContent.startsWith('Find references')).click()`);
    check('find references lists and marks', await until(`return document.querySelectorAll('#play-refs li').length >= 2 && document.querySelectorAll('.cm-refmark').length >= 2`, 20000),
      await js(`return document.getElementById('play-refs').textContent + ' / marks ' + document.querySelectorAll('.cm-refmark').length`));
    await js(`document.querySelector('#play-refs .close').click()`);
    await key('Escape');
    await mouse(...p);
    await key('F2');
    check('F2 opens a rename box', await until(`return document.querySelector('.renamebox')?.value === 'greet'`, 10000));
    await js(`document.querySelector('.renamebox').value = 'salute'`);
    await key('Enter');
    const renamed = await until(`return /\\\\salute\\{/.test(document.querySelector('.cm-content').textContent)`, 10000);
    p = await spot('\\salute{', 3);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: p[0] + 40, y: p[1] + 200 });
    await sleep(200);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: p[0], y: p[1] });
    const fresh = await until(`return /salute/.test(document.querySelector('.tip-card .hover')?.textContent ?? '')`, 20000);
    check('hover after rename', renamed && fresh, await js(`return document.querySelector('.tip-card .hover')?.textContent?.slice(0, 80) ?? 'no card'`));
    check('rename applies', await until(`return /\\\\salute\\{/.test(document.querySelector('.cm-content').textContent) && !/\\\\greet/.test(document.querySelector('.cm-content').textContent)`, 10000));

    // a finding satex lowered below its rule's default shows its own severity alone
    p = await spot('\\begin{document}', 0);
    await mouse(...p);
    await send('Input.insertText', { text: '\\newcommand\\again{\\again}\n' });
    const lowered = await until(`return [...document.querySelectorAll('#lsp-diagnostics li[data-i]')].some((li) => li.querySelector('.rn').textContent === 'unguarded-recursion')`, 60000) &&
      await js(`const li = [...document.querySelectorAll('#lsp-diagnostics li[data-i]')].find((li) => li.querySelector('.rn').textContent === 'unguarded-recursion');
        if (!li.querySelector('.explain')) li.querySelector('.why').click(); await new Promise((r) => setTimeout(r, 100));
        const again = [...document.querySelectorAll('#lsp-diagnostics li[data-i]')].find((li) => li.querySelector('.rn').textContent === 'unguarded-recursion');
        return [again.querySelectorAll('.shape, .sev').length, again.querySelector('.shape').title, /\berror\b/.test(again.querySelector('.explain').textContent.split('.')[0])];`);
    check('lowered severity shown once', lowered && lowered[0] === 1 && lowered[1] !== manifest.rules['unguarded-recursion'].severity && !lowered[2], JSON.stringify(lowered));

    // the editor's completion: the usage line colored like the hover's (\greet is \salute since the rename)
    await mouse(...await spot('\\end{document}', 14));
    await send('Input.insertText', { text: '\n\\salu' });
    const detail = await until(`return [...document.querySelectorAll('.cm-tooltip-autocomplete li')].some((li) => /salute/.test(li.textContent) && li.querySelector('.cm-completionDetail .arg-mand'))`, 20000);
    await shot('play-completion');
    check('completion usage colored by argument kind', detail, await js(`return document.querySelector('.cm-tooltip-autocomplete')?.textContent.slice(0, 120) ?? 'no completion'`));
    // the library lists the names in force and finds one; a scratch text is read in the document's state
    await js(`document.getElementById('lib-open').click()`);
    const listed = await until(`const s = document.getElementById('lib-search'); s.value = 'textbf'; s.dispatchEvent(new Event('input'));
      return [...document.querySelectorAll('#lib-list .cs-ref')].some((c) => c.textContent === '\\\\textbf')`, 60000);
    check('the library finds a name in force', listed, await js(`return document.getElementById('lib-list').textContent.slice(0, 160)`));
    await js(`document.getElementById('lib-text').value = '\\\\textbf{tried} 10--12'; document.getElementById('lib-run').click()`);
    const tried = await until(`const o = document.getElementById('lib-out').textContent; return /tried/.test(o) && /10–12/.test(o)`, 120000);
    check('a tried text is typeset, ligatures and all', tried, await js(`const o = document.getElementById('lib-out'); return o.textContent.slice(0, 200)`));
    await key('Escape');
    for (let i = 0; i < 6; i++) await key('Backspace');

    // typing a `\begin{…}`'s `}` closes it, and its name is edited together with the `\end`'s
    const content = `document.querySelector('.cm-content').textContent`;
    await send('Input.insertText', { text: '\n\\begin{quote' });
    await key('Escape');
    await send('Input.insertText', { text: '}' });
    check('a typed \\begin gets its \\end', await until(`return /\\\\begin\\{quote\\}\\\\end\\{quote\\}/.test(${content})`, 5000), await js(`return ${content}.slice(-80)`));
    await key('ArrowLeft');
    const linked = await until(`return document.querySelectorAll('.cm-linked').length === 2`, 20000);
    await send('Input.insertText', { text: 'x' });
    check('\\begin and \\end names are edited together', linked && await until(`return /\\\\begin\\{quotex\\}\\\\end\\{quotex\\}/.test(${content})`, 5000), await js(`return ${content}.slice(-80)`));
    for (let i = 0; i < 6 && await js(`return /quote/.test(${content})`); i++) await key('z', 2, 'KeyZ');

    // Ctrl+click: the kernel's \section opens read-only
    p = await spot('\\section', 3);
    await mouse(...p, { modifiers: 2 });
    check('ctrl+click opens a kernel file read-only', await until(`return !!document.querySelector('#play-tabs .ro[aria-selected=true]')`, 20000),
      await js(`return document.querySelector('#play-tabs [aria-selected=true]')?.textContent + ' / ' + document.getElementById('play-toast').textContent`));

    // projects: an example's files in the tree, its root marked; an \input file is analyzed with the root
    // the open example is edited: its edits are kept (drafts), the switch asks nothing
    await js(`document.querySelector('#play-examples [data-example="thesis.tex"]').click()`);
    check('an edited example switches unasked, its edits kept', await js(`await new Promise((r) => setTimeout(r, 500)); return !document.getElementById('play-replace').open && /"texts"/.test(localStorage.getItem('satex-drafts') ?? '')`));
    check('an example is a project', await until(`return [...document.querySelectorAll('#play-files-tree .file')].map((f) => f.dataset.name).join() === 'chapter.tex,refs.bib,thesis.tex'
      && !!document.querySelector('#play-files-tree .root[data-name="thesis.tex"]') && !document.querySelector('#play-tabs .ro')`, 20000),
      await js(`return [...document.querySelectorAll('#play-files-tree .file')].map((f) => f.dataset.name).join()`));
    check('an input file is analyzed with its root', await until(`return !!document.querySelector('#play-files-tree [data-name="chapter.tex"] .tcount')`, 60000));
    // switching files replaces the whole text: no decoration range (linked names, marks) may stretch over the new one
    await mouse(...await spot('\\begin{', 8));
    const stale = await until(`return document.querySelectorAll('.cm-linked').length === 2`, 20000);
    await js(`document.querySelector('#play-tabs [data-name="chapter.tex"]').click()`);
    check('a file switch leaves no stale decorations', stale && await until(`return document.querySelector('#play-tabs [aria-selected=true]')?.textContent === 'chapter.tex'`, 5000)
      && await js(`return document.querySelectorAll('.cm-linked, .cm-refmark, .cm-flash, .cm-hold, .cm-deflink').length`) === 0);
    await js(`document.querySelector('#play-tabs [data-name="thesis.tex"]').click()`);
    await shot('play-project');
    // renaming a file renames the root's \input of it
    await until(`return !!document.querySelector('#play-files-tree [data-name="chapter.tex"] [data-act=rename]')`, 10000);
    await js(`document.querySelector('#play-files-tree [data-name="chapter.tex"] [data-act=rename]').click()`);
    await until(`return !!document.querySelector('.tnamebox')`, 5000);
    await js(`document.querySelector('.tnamebox').value = 'background.tex'`);
    await key('Enter');
    check('rename follows into \\input', await until(`return /1 reference updated/.test(document.getElementById('play-toast').textContent)
      && !!document.querySelector('#play-tabs [data-name="background.tex"]')`, 5000), await js(`return document.getElementById('play-toast').textContent`));
    // a new file opens as a tab; delete asks once more
    await js(`document.getElementById('play-upload').click(); document.querySelector('#play-upmenu [data-pick=new]').click()`);
    await until(`return !!document.querySelector('.tnamebox')`, 5000);
    await js(`document.querySelector('.tnamebox').value = 'notes.tex'`);
    await key('Enter');
    check('new file', await until(`return document.querySelector('#play-tabs [aria-selected=true]')?.textContent === 'notes.tex' && !!document.querySelector('#play-files-tree [data-name="notes.tex"]')`, 5000));
    await js(`const del = () => document.querySelector('#play-files-tree [data-name="notes.tex"] [data-act=delete]'); del().click(); del().click()`);
    check('delete', await until(`return !document.querySelector('#play-files-tree [data-name="notes.tex"]') && !document.querySelector('#play-tabs [data-name="notes.tex"]')`, 5000));
    await js(`document.querySelector('#play-examples [data-example="${manifest.sample}"]').click(); await new Promise((r) => setTimeout(r, 300)); document.querySelector('#play-replace[open] button[value=replace]')?.click()`);
    await until(`return document.querySelector('#play-tabs [aria-selected=true]')?.textContent === '${manifest.sample}'`, 10000);

    // uploads: one file joins the project; a folder becomes the project
    const dir = mkdtempSync(join(tmpdir(), 'satex-up-'));
    writeFileSync(join(dir, 'extra.tex'), '\\newcommand\\extra{x}\n');
    await upload('play-files', [join(dir, 'extra.tex')]);
    check('single file upload', await until(`return [...document.querySelectorAll('#play-tabs [role=tab]')].some((t) => t.textContent === 'extra.tex')`, 30000),
      await js(`return document.getElementById('play-toast').textContent`));
    const folder = join(dir, 'thesis');
    mkdirSync(join(folder, 'chapters'), { recursive: true });
    writeFileSync(join(folder, 'main.tex'), '\\documentclass{article}\n\\begin{document}\n\\input{chapters/one}\n\\end{document}\n');
    writeFileSync(join(folder, 'chapters/one.tex'), 'One -- two.\n');
    await upload('play-folder', [folder]);
    const tree = `[...document.querySelectorAll('#play-files-tree .file')].map((f) => f.dataset.name).join() + ' | ' + [...document.querySelectorAll('#play-tabs [role=tab]')].map((t) => t.textContent).join()`;
    check('folder upload', await until(`return ${tree} === 'chapters/one.tex,main.tex | main.tex' && !!document.querySelector('#play-files-tree .root[data-name="main.tex"]')`, 30000), await js(`return ${tree}`));

    // a package beyond the first download: fetched when the findings say it is missing
    const micro = join(dir, 'micro');
    mkdirSync(micro);
    writeFileSync(join(micro, 'main.tex'), '\\documentclass{article}\n\\usepackage{microtype}\n\\begin{document}\nText.\n\\end{document}\n');
    await upload('play-folder', [micro]);
    check('a package loads on demand', await until(`return /\\bmicrotype\\b/.test(document.getElementById('play-status').dataset.packages ?? '') && document.getElementById('lsp-status').textContent !== 'checking…'
      && ![...document.querySelectorAll('#lsp-diagnostics li code')].some((c) => c.textContent === 'unresolved-file')`, 90000),
      await js(`return document.getElementById('play-status').textContent + ' | ' + document.getElementById('lsp-diagnostics').textContent.slice(0, 200)`));
    await shot('play-package');

    // the drop zone: shown while files are dragged, gone on Escape
    await js(`const dt = new DataTransfer(); dt.items.add(new File(['x'], 'a.tex')); window.dispatchEvent(new DragEvent('dragover', { dataTransfer: dt, bubbles: true, cancelable: true }))`);
    check('drop zone', await js(`const d = document.getElementById('play-drop'); return !d.hidden && /files, a folder or a \\.zip/i.test(d.textContent)`));
    await key('Escape');
    check('drop zone closes on Escape', await js(`return document.getElementById('play-drop').hidden`));

    // buttons lose their labels when their bar runs out of room, and get them back
    const label = `getComputedStyle(document.querySelector('#play-link .label')).display`;
    const barWidth = (w) => js(`document.querySelector('.filebar').style.width = '${w}'`);
    await barWidth('200rem');
    const wide = await until(`return ${label} !== 'none'`, 5000);
    await barWidth('18rem');
    const narrow = await until(`return ${label} === 'none'`, 5000);
    await shot('play-tight');
    await barWidth('200rem');
    const restored = await until(`return ${label} !== 'none'`, 5000);
    await barWidth('');
    check('buttons collapse when their bar is short', wide && narrow && restored, JSON.stringify([wide, narrow, restored]));
    // the cache dialog finds a cache by a command it defined in the run
    await js(`document.getElementById('cache-open').click()`);
    await until(`return document.querySelectorAll('#cache-list tr').length > 1`, 20000);
    const found = await until(`const f = document.getElementById('cache-filter'); f.value = '\\\\microtypesetup'; f.dispatchEvent(new Event('input'));
      return [...document.querySelectorAll('#cache-list .cache-hit')].some((h) => /\\\\microtypesetup\\b/.test(h.textContent))`, 20000);
    check('caches are found by a command they define', found, await js(`return document.getElementById('cache-list').textContent.slice(0, 160)`));
    await js(`document.getElementById('cache-close').click()`);

    // several at once, with what they need, in one batch; a package nobody offers stays a finding
    const many = join(dir, 'many');
    mkdirSync(many);
    writeFileSync(join(many, 'main.tex'), '\\documentclass{article}\n\\usepackage{siunitx,booktabs}\n\\usepackage{tcolorbox}\n\\usepackage{nosuchpackage}\n\\begin{document}\n\\SI{1}{m}\n\\end{document}\n');
    await upload('play-folder', [many]);
    const unresolvedNames = `[...document.querySelectorAll('#lsp-diagnostics li')].filter((li) => /unresolved-file/.test(li.textContent)).map((li) => li.textContent)`;
    check('packages load on demand in one batch', await until(`const p = document.getElementById('play-status').dataset.packages ?? '';
      return ['siunitx', 'booktabs', 'tcolorbox', 'tikz'].every((n) => new RegExp('/' + n + '\\\\b').test(p)) && document.getElementById('lsp-status').textContent !== 'checking…'
      && ${unresolvedNames}.length > 0 && ${unresolvedNames}.every((t) => t.includes('nosuchpackage'))`, 180000),
      await js(`return document.getElementById('play-status').dataset.packages + ' | ' + ${unresolvedNames}.join(' / ').slice(0, 300)`));
    // a switch in a section header flips when the switch itself is clicked, not only its text
    const flipped = await js(`const b = document.getElementById('typeset-keys'), was = b.checked; b.click(); const now = b.checked; b.click(); return now !== was && b.checked === was`);
    check('a header switch flips on its own click', flipped);
    check('no page errors', !errors.length, errors.join(' | ').slice(0, 300));
    ws.close();
  } catch (e) {
    check('browser checks', false, e.message);
  } finally {
    proc.kill();
  }
}
