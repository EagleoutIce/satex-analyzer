// Runs the playground of a built site in node: the page's manifest, its
// worker.js engine and the satex wasm, fetched as a browser would. Then, if a
// Chromium is found ($CHROME, or Playwright's download), the page itself in a
// headless browser over the DevTools protocol.
//   python3 -m http.server -d site-out 8000 & node doc/site/test.mjs http://localhost:8000/
//   node doc/site/test.mjs file:///path/to/site-out/
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
const engine = (await load(null, manifest.worker))(await load(manifest.js));
let t = performance.now();
const given = base.startsWith('file:') ? { js: undefined, wasm: await bytes(manifest.wasm), bundle: await bytes(manifest.kernels[0].bundle) } : {};
const { samples, imported } = await engine.init(base, manifest, given);
console.log(`init: ${Math.round(performance.now() - t)} ms, ${imported.lines.map((l) => l[1]).join(' ')}`);
for (const name of manifest.samples) {
  const run = await engine.run(['-v', 'lint', '-f', name], { [`/work/${name}`]: new TextEncoder().encode(samples[name]) });
  const out = run.lines.filter((l) => l[0] === 'out').map((l) => l[1]);
  const summary = out.find((l) => /\d+ errors?, \d+ warnings?/.test(l));
  const rebuilt = run.lines.filter((l) => /building kernel cache|: rebuilt \(/.test(l[1])).map((l) => l[1].trim());
  check(`${name} lints`, run.status === 0 && summary && !rebuilt.length,
    `exit ${run.status}, ${run.ms} ms, ${summary ?? 'no summary'}${rebuilt.length ? `; not served from the bundle's caches: ${rebuilt.join('; ')}` : ''}`);
  check(`${name} carries no command line`, !/^% satex /.test(samples[name]));
}

// The other kernels: each bundle loads, and the default sample lints with it.
for (const entry of manifest.kernels.slice(1)) {
  t = performance.now();
  await engine.kernel(base, entry, base.startsWith('file:') ? await bytes(entry.bundle) : undefined);
  const run = await engine.run([...entry.args, 'lint', '-f', manifest.sample]);
  const summary = run.lines.map((l) => l[1]).find((l) => /\d+ errors?, \d+ warnings?/.test(l));
  check(`${entry.label} kernel`, typeof run.status === 'number' && run.status <= 1 && summary, `${Math.round(performance.now() - t)} ms with loading, ${summary ?? 'no summary'}`);
}

// The language server: a finding for the default file, a completion, a hover.
const uri = `file:///work/${manifest.sample}`;
let source = samples[manifest.sample];
const lsp = (method, params, id) => engine.lsp({ jsonrpc: '2.0', method, params, ...(id ? { id } : {}) });
t = performance.now();
await lsp('initialize', { capabilities: {} }, 1);
const published = (await lsp('textDocument/didOpen', { textDocument: { uri, languageId: 'latex', version: 1, text: source } }))
  .find((m) => m.method === 'textDocument/publishDiagnostics' && m.params.uri === uri);
const codes = published?.params.diagnostics.map((d) => d.code) ?? [];
source = source.replace('\\end{document}', '\\gre\n\\end{document}');
await lsp('textDocument/didChange', { textDocument: { uri, version: 2 }, contentChanges: [{ text: source }] });
const line = source.split('\n').findIndex((l) => l === '\\gre');
const [completion] = await lsp('textDocument/completion', { textDocument: { uri }, position: { line, character: 4 } }, 2);
const items = completion?.result ?? [];
check('lsp', codes.length > 0 && items.some((i) => i.label === 'greet'), `${Math.round(performance.now() - t)} ms, diagnostics [${codes.join(', ')}], ${items.length} completions`);
check('completion marks the document\'s own commands', items.find((i) => i.label === 'greet')?.detail?.endsWith('(document)'));
const nl = source.split('\n').findIndex((l) => l.includes('\\newcommand'));
const [hover] = await lsp('textDocument/hover', { textDocument: { uri }, position: { line: nl, character: 3 } }, 3);
const card = hover?.result?.contents?.value ?? '';
check('hover shows a usage line, no raw signature', /⟨arg1⟩/.test(card) && !/signature:/.test(card), card.split('\n').filter(Boolean).slice(0, 3).join(' | '));

// The same findings as `satex lint`, info ones included, on an unsaved buffer.
const unsaved = samples[manifest.sample].replace('\\begin{document}', '\\def\\foo{}\n\\begin{document}');
const pub = (await lsp('textDocument/didChange', { textDocument: { uri, version: 3 }, contentChanges: [{ text: unsaved }] }))
  .find((m) => m.method === 'textDocument/publishDiagnostics' && m.params.uri === uri);
const lspFound = (pub?.params.diagnostics ?? []).map((d) => `${d.code}/${['', 'error', 'warning', 'info', 'info'][d.severity]}`);
const lintRun = await engine.run(['lint', '--format', 'json', '-f', manifest.sample], { [`/work/${manifest.sample}`]: new TextEncoder().encode(unsaved) });
const lintFound = JSON.parse(lintRun.lines.filter((l) => l[0] === 'out').map((l) => l[1]).join('\n') || '[]').map((r) => `${r.code}/${r.severity}`);
check('unused-definition shown as by lint', lspFound.includes('unused-definition/info') && lspFound.sort().join() === lintFound.sort().join(), `lsp [${lspFound}] lint [${lintFound}]`);

// A primitive's syntax, only while the name means the primitive.
const defLine = unsaved.split('\n').findIndex((l) => l === '\\def\\foo{}');
const hoverAt = async (text, line, id) => {
  await lsp('textDocument/didChange', { textDocument: { uri, version: 10 + id }, contentChanges: [{ text }] });
  return (await lsp('textDocument/hover', { textDocument: { uri }, position: { line, character: 2 } }, id))[0]?.result?.contents?.value ?? '';
};
const prim = await hoverAt(unsaved, defLine, 5);
check('primitive hover shows the syntax', /\\def⟨control sequence⟩⟨parameter text⟩\{⟨replacement text⟩\}/.test(prim) && /TeX primitive/.test(prim), prim.split('\n').filter(Boolean).slice(0, 3).join(' | '));
const relet = unsaved.replace('\\def\\foo{}', '\\let\\def\\relax\n\\def\\foo{}');
const notPrim = await hoverAt(relet, defLine + 1, 6);
check('no syntax once \\def is redefined', !/TeX primitive|⟨control sequence⟩/.test(notPrim), notPrim.split('\n').filter(Boolean).slice(0, 2).join(' | '));

// a finding underlines the whole call, not one character
const redefined = '\\newcommand{\\section}{x}';
const wide = (await lsp('textDocument/didChange', { textDocument: { uri, version: 4 }, contentChanges: [{ text: source.replace('\\begin{document}', `${redefined}\n\\begin{document}`) }] }))
  .find((m) => m.method === 'textDocument/publishDiagnostics' && m.params.uri === uri);
const rline = source.replace('\\begin{document}', `${redefined}\n\\begin{document}`).split('\n').findIndex((l) => l === redefined);
const marks = (wide?.params.diagnostics ?? []).filter((d) => d.range.start.line === rline);
check('finding underlines the whole call', marks.some((d) => d.range.end.line === rline && d.range.end.character - d.range.start.character === redefined.length),
  marks.map((d) => `${d.code} ${d.range.start.character}-${d.range.end.character}`).join(', ') || 'no finding on the line');
// a source cited in running text is a link
const cited = Object.entries(manifest.rules).find(([, r]) => /\[tex\.web § \d+\]\(https:\/\/[^)]*tex\.web#L\d+\)/.test(r.explanation ?? ''));
check('tex.web reference in an explanation is a link', !!cited, cited?.[0] ?? 'no explanation links one');

// every TeXbook link names its chapter, on the rules page, in explanations and in references
const books = [...(await text('wiki/lints.html')).matchAll(/href="([^"]*texbook[^"]*)"/g)].map((m) => m[1])
  .concat(Object.values(manifest.rules).flatMap((r) => [...(r.explanation ?? '').matchAll(/\]\((https:[^)]*texbook[^)]*)\)/g)].map((m) => m[1]).concat((r.references ?? []).map((x) => x.url ?? '').filter((u) => /texbook/.test(u)))));
check('TeXbook links carry their chapter', books.length > 3 && books.every((u) => /#(?:chapter|appendix)-\w+$/.test(u)), books.filter((u) => !/#(?:chapter|appendix)-\w+$/.test(u)).slice(0, 3).join(' '));
const landing = await text('index.html');
check('landing examples are highlighted without JS', /<span class="ln"[^>]*>.*?<span class="tok-cmd[ "]/.test(landing));
check('landing shows no static command output', !/<pre class="output"|\$ satex explain/.test(landing));
// every command of every example comes with what satex explain says of it
const exData = [...landing.matchAll(/<script type="application\/json" class="ex-data">(.*?)<\/script>/gs)].map((m) => JSON.parse(m[1]));
const exCommands = [...landing.matchAll(/data-cs="([^"]*)"/g)].map((m) => m[1].replace(/&amp;/g, '&').replace(/&quot;/g, '"'));
const unexplained = [...new Set(exCommands)].filter((c) => !exData.some((d) => d.cs[c]?.length));
check('every landing command is explained', exData.length >= 3 && exCommands.length > 10 && !unexplained.length, unexplained.join(' '));
check('explain records charge no hook to \\twice', !(exData.flatMap((d) => d.cs['\\twice'] ?? []).some((r) => (r.expands ?? []).some((x) => /everypar/.test(x)))),
  JSON.stringify(exData.flatMap((d) => d.cs['\\twice'] ?? []).map((r) => r.expands)));

await browser();
process.exit(failed ? 1 : 0);

// ---------------------------------------------------------------- the page in a browser

function chromium() {
  if (process.env.CHROME) return process.env.CHROME;
  const root = join(homedir(), '.cache/ms-playwright');
  const dirs = existsSync(root) ? readdirSync(root).filter((d) => /^chromium-\d+$/.test(d)).sort().reverse() : [];
  return dirs.map((d) => ['chrome-linux64/chrome', 'chrome-linux/chrome'].map((c) => join(root, d, c)).find(existsSync)).find(Boolean);
}

async function browser() {
  const chrome = chromium();
  if (!chrome) return console.log('skip: no Chromium for the browser checks (set CHROME)');
  const port = 9300 + Math.floor(Math.random() * 600);
  const profile = mkdtempSync(join(tmpdir(), 'satex-test-'));
  const proc = spawn(chrome, ['--headless=new', '--no-sandbox', '--disable-gpu', `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`,
    '--allow-file-access-from-files', '--window-size=1440,1000', 'about:blank'], { stdio: 'ignore' });
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  try {
    let target;
    for (let i = 0; i < 100 && !target; i++) { await sleep(200); try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((x) => x.type === 'page'); } catch {} }
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((r) => { ws.onopen = r; });
    let id = 0;
    const replies = new Map(), errors = [];
    ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.id) replies.get(m.id)?.(m);
      else if (m.method === 'Runtime.exceptionThrown') errors.push(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text);
    };
    const send = (method, params = {}) => new Promise((r) => { replies.set(++id, r); ws.send(JSON.stringify({ id, method, params })); });
    const js = async (expr) => {
      const r = await send('Runtime.evaluate', { expression: `(async () => { ${expr} })()`, awaitPromise: true, returnByValue: true });
      if (r.result?.exceptionDetails) throw new Error(`${expr.slice(0, 80)}: ${r.result.exceptionDetails.exception?.description}`);
      return r.result?.result?.value;
    };
    const until = async (expr, ms = 120000) => { for (const end = Date.now() + ms; Date.now() < end; await sleep(150)) if (await js(expr).catch(() => false)) return true; return false; };
    const key = async (key, modifiers = 0, code = key) => {
      for (const type of ['keyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key, code, modifiers, windowsVirtualKeyCode: { Enter: 13, Escape: 27, F2: 113, Tab: 9 }[key] ?? 0, ...(key === 'Enter' ? { text: '\r' } : {}) });
    };
    const mouse = async (x, y, opts = {}) => { for (const t of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type: t, x, y, button: 'left', clickCount: 1, ...opts }); };
    /** The screen point of `needle` (its `at`-th character) in the editor's text. */
    const spot = (needle, at = 1) => js(`for (const l of document.querySelectorAll('.cm-line')) { const i = l.textContent.indexOf(${JSON.stringify(needle)}); if (i < 0) continue;
      const w = document.createTreeWalker(l, NodeFilter.SHOW_TEXT); let n, off = i + ${at};
      while ((n = w.nextNode())) { if (off <= n.length) { const r = document.createRange(); r.setStart(n, off); r.setEnd(n, off); const b = r.getBoundingClientRect(); return [b.left + 1, b.top + b.height / 2]; } off -= n.length; } } return null;`);
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
    // a command explains itself on hover: a card with its signature as chips
    let at = await center('#ex-0 [data-cs="\\\\greet"]');
    const before = await js(layout);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
    check('landing command hover card', await until(`const t = document.querySelector('.tip:not([hidden])'); return t && /greet/.test(t.querySelector('.x-name')?.textContent) && t.querySelector('.sig .arg[title]')`, 5000),
      await js(`return document.querySelector('.tip')?.textContent.slice(0, 120) ?? 'no card'`));
    const after = await js(layout);
    check('landing hover moves nothing', before === after, before === after ? '' : `${before} -> ${after}`);
    // findings are wavy underlines over the whole call, with the message on hover
    check('wavy underlines', await js(`const l = [...document.querySelectorAll('#ex-0 .lint')]; return l.length > 0 && l.every((e) => getComputedStyle(e).textDecorationStyle === 'wavy')`));
    check('underline spans the whole call', await js(`const k = [...document.querySelectorAll('#ex-0 [data-lint]')].find((e) => e.closest('.ln').textContent.startsWith('\\\\newcommand{\\\\section}')).dataset.lint.split(' ')[0];
      const line = document.querySelector('#ex-0 [data-lint~="' + k + '"]').closest('.ln'); return [...line.querySelectorAll('[data-lint~="' + k + '"]')].map((e) => e.textContent).join('') === line.textContent`));
    at = await center('#ex-0 [data-lint].f-error');
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
    check('landing finding hover', await until(`return /already defined/.test(document.querySelector('.tip:not([hidden]) .tip-lint')?.textContent ?? '')`, 5000));
    check('landing finding hover moves nothing', before === await js(layout));
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 2, y: 2 });
    // the macros example: an explain panel, signature chips with explanations, no raw JSON
    await js(`[...document.querySelectorAll('.examples [role=tab]')].find((t) => /macros/.test(t.textContent)).click()`);
    const panel = await js(`const f = document.querySelector('.example:not([hidden])'); return [f.querySelector('.ex-explain .xname').textContent, f.querySelector('.ex-explain').textContent,
      [...f.querySelectorAll('.ex-explain .sig .arg')].map((a) => a.title)]`);
    check('macros example explains \\twice', panel[0] === '\\twice' && /#1#1/.test(panel[1]) && panel[2].length === 1 && /mandatory/.test(panel[2][0]), JSON.stringify(panel).slice(0, 200));
    check('no raw JSON on the landing page', await js(`return ![...document.querySelectorAll('.ex-explain, .ex-findings')].some((e) => /[{\\[]"|"max"|"min"/.test(e.textContent))`));
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
    check('no output panel for plain lint', await js(`return document.getElementById('play-out').hidden`));
    for (const width of [1866, 1480]) {
      await send('Emulation.setDeviceMetricsOverride', { width, height: 1000, deviceScaleFactor: 1, mobile: false });
      await sleep(300);
      const fit = await js(`const r = (s) => document.querySelector(s).getBoundingClientRect(); return [r('.play-split').left, r('.play-split').right, r('.play-tools').right, r('#play-link').right, r('.top nav').right, r('#theme').right, innerWidth, document.documentElement.scrollWidth]`);
      check(`playground spans the window at ${width}px`, fit[0] === 0 && Math.abs(fit[1] - fit[6]) < 2 && Math.abs(fit[2] - fit[6]) < 2, JSON.stringify(fit));
      check(`header fits at ${width}px`, fit.slice(3, 6).every((x) => x <= fit[6]) && fit[7] <= fit[6], JSON.stringify(fit));
    }
    await send('Emulation.clearDeviceMetricsOverride');
    check('header: kernel, command line, run, upload, link', await js(`return !document.querySelector('.act, #play-query, #play-reset') && !!document.querySelector('#play-kernel') && !!document.querySelector('#play-args')`));
    check('findings', await until(`return document.querySelectorAll('#lsp-diagnostics li').length > 0`));
    const kernels = await js(`return [...document.querySelectorAll('#play-kernel option')].map((o) => [o.textContent, o.disabled, o.title])`);
    check('kernel labels carry their own size', kernels.some(([t]) => /^pdfTeX · [\d.]+ MiB$/.test(t)) && kernels.some(([t]) => /^plain TeX · \d+ KiB$/.test(t)) && !kernels.some(([t]) => /\+/.test(t)),
      kernels.map((k) => k[0]).join(' | '));
    check('LuaTeX listed as not available', kernels.some(([t, off, why]) => /^LuaTeX/.test(t) && off && /format budget/.test(why)));
    check('editor highlighted', await js(`return document.querySelectorAll('.cm-content .tok-cmd').length > 3`));

    // every finding, unfolded, shows one severity: its own
    const sevs = await js(`const out = []; for (const li of document.querySelectorAll('#lsp-diagnostics li')) { li.querySelector('.why').click(); }
      await new Promise((r) => setTimeout(r, 100));
      for (const li of document.querySelectorAll('#lsp-diagnostics li')) out.push([li.querySelectorAll('.sev').length, !!li.querySelector('.explain'), li.querySelector('code').textContent, li.querySelector('.sev').textContent]);
      return out;`);
    check('one severity per finding', sevs.length && sevs.every(([n, open]) => n === 1 && open), JSON.stringify(sevs));
    const differs = sevs.filter(([, , code, sev]) => manifest.rules[code] && manifest.rules[code].severity !== sev);
    console.log(`     ${differs.length} of them differ from their rule's default severity`);
    // hovering a finding moves and resizes nothing
    const boxes = `return [...document.querySelectorAll('#lsp-diagnostics li')].slice(0, 3).map((li) => { const r = li.getBoundingClientRect(); return [r.x, r.y, r.width, r.height].map(Math.round).join(); }).join(' ') + ' ' + document.getElementById('lsp-diagnostics').scrollHeight`;
    const still = await js(boxes);
    const first = await js(`const r = document.querySelector('#lsp-diagnostics li').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]`);
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
    check('scope --at completes', scopeAt.some(([l]) => /^\d+:\d+$/.test(l)) && scopeAt.some(([l]) => l === 'macros.tex:') && scopeAt.every(([, d]) => d), JSON.stringify(scopeAt.slice(0, 4)));
    await key('Escape');
    await js(`delete document.getElementById('play-status').dataset.done; document.getElementById('play-args').value = 'query definitions'`);
    await js(`document.getElementById('play-args').focus()`);
    await key('Enter');
    await until(`return document.getElementById('play-status').dataset.done !== undefined`, 120000);
    check('query definitions renders a table', await js(`const o = document.getElementById('play-output'); return !!o.querySelector('table.rows tbody tr') && !o.querySelector(':scope > pre')`),
      await js(`return document.getElementById('play-output').innerHTML.slice(0, 200)`));

    // hover card: a usage line, a footer, no raw signature
    let p = await spot('\\newcommand', 3);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: p[0], y: p[1] });
    const hoverText = await until(`return document.querySelector('.cm-tooltip-hover .hover')`, 20000) && await js(`return document.querySelector('.cm-tooltip-hover .hover').textContent`);
    check('hover card', /⟨arg1⟩/.test(hoverText) && !/signature/.test(hoverText) && await js(`return !!document.querySelector('.cm-tooltip-hover .foot')`), hoverText.slice(0, 100));

    // the Explain macro panel follows the mouse, no click
    p = await spot('\\greet{', 2);
    await send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: p[0], y: p[1] });
    check('explain panel follows the mouse', await until(`return document.getElementById('explain-name').textContent === '\\\\greet' && /greet/.test(document.getElementById('explain-body').textContent)`, 20000),
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
    const fresh = await until(`return /salute/.test(document.querySelector('.cm-tooltip-hover .hover')?.textContent ?? '')`, 20000);
    check('hover after rename', renamed && fresh, await js(`return document.querySelector('.cm-tooltip-hover .hover')?.textContent?.slice(0, 80) ?? 'no card'`));
    check('rename applies', await until(`return /\\\\salute\\{/.test(document.querySelector('.cm-content').textContent) && !/\\\\greet/.test(document.querySelector('.cm-content').textContent)`, 10000));

    // a finding satex lowered below its rule's default shows its own severity alone
    p = await spot('\\begin{document}', 0);
    await mouse(...p);
    await send('Input.insertText', { text: '\\newcommand\\again{\\again}\n' });
    const lowered = await until(`return [...document.querySelectorAll('#lsp-diagnostics li')].some((li) => li.querySelector('code').textContent === 'unguarded-recursion')`, 60000) &&
      await js(`const li = [...document.querySelectorAll('#lsp-diagnostics li')].find((li) => li.querySelector('code').textContent === 'unguarded-recursion');
        if (!li.querySelector('.explain')) li.querySelector('.why').click(); await new Promise((r) => setTimeout(r, 100));
        const again = [...document.querySelectorAll('#lsp-diagnostics li')].find((li) => li.querySelector('code').textContent === 'unguarded-recursion');
        return [again.querySelectorAll('.sev').length, again.querySelector('.sev').textContent, /\berror\b/.test(again.querySelector('.explain').textContent.split('.')[0])];`);
    check('lowered severity shown once', lowered && lowered[0] === 1 && lowered[1] !== manifest.rules['unguarded-recursion'].severity && !lowered[2], JSON.stringify(lowered));

    // Ctrl+click: the kernel's \section opens read-only
    p = await spot('\\section', 3);
    await mouse(...p, { modifiers: 2 });
    check('ctrl+click opens a kernel file read-only', await until(`return !!document.querySelector('#play-tabs .ro[aria-selected=true]')`, 20000),
      await js(`return document.querySelector('#play-tabs [aria-selected=true]')?.textContent + ' / ' + document.getElementById('play-toast').textContent`));

    // uploads: one file joins the tabs; a folder becomes the project
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
    check('folder upload', await until(`return [...document.querySelectorAll('#play-tabs [role=tab]')].map((t) => t.textContent).join() === '← samples,chapters/one.tex,main.tex'`, 30000),
      await js(`return [...document.querySelectorAll('#play-tabs [role=tab]')].map((t) => t.textContent).join()`));

    // the drop zone: shown while files are dragged, gone on Escape
    await js(`const dt = new DataTransfer(); dt.items.add(new File(['x'], 'a.tex')); window.dispatchEvent(new DragEvent('dragover', { dataTransfer: dt, bubbles: true, cancelable: true }))`);
    check('drop zone', await js(`const d = document.getElementById('play-drop'); return !d.hidden && /files, a folder or a \\.zip/i.test(d.textContent)`));
    await key('Escape');
    check('drop zone closes on Escape', await js(`return document.getElementById('play-drop').hidden`));

    check('no page errors', !errors.length, errors.join(' | ').slice(0, 300));
    ws.close();
  } catch (e) {
    check('browser checks', false, e.message);
  } finally {
    proc.kill();
  }
}
