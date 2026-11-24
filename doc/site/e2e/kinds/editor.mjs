// The editor's gutter and the widgets' close, as a person uses them.
// `gutter`: hover a finding's marker (its card shows the finding and its fixes), click it (the Findings panel opens on it).
// `close`: open widget `widget` at `at` (+`k` characters) of example `example`, close it untouched (no discard question);
// with `edit` (page fields: a key and a value) Close asks and Close again (`how`: the X or Esc) discards.
import { sleep } from '../lib.mjs';

const J = JSON.stringify;
const started = (p) => p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000);
const example = async (p, name) => {
  await p.eval(`document.querySelector('[data-example=${J(name)}]').click()`);
  return p.until(`satexHealth().example === ${J(name)} && satexHealth().text?.length > 10`, 20000);
};

export async function gutter(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  if (!await started(p) || !await example(p, sc.example)) { fail('the playground did not start'); return res; }
  if (sc.dark) await p.s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
  const mark = `document.querySelector('.cm-gutter-lint .cm-lint-marker')`;
  if (!await p.until(`!!${mark}`, 120000)) { fail('no finding marker'); return res; }
  await sleep(600);
  await p.until(`!!${mark}`, 30000);
  const m = await p.eval(`(() => { const r = ${mark}.getBoundingClientRect(), x = r.x + r.width / 2, y = r.y + r.height / 2; return { x, y, top: !!document.elementFromPoint(x, y)?.closest('.cm-lint-marker') }; })()`);
  if (!m.top) fail('the marker is covered');
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: m.x - 2, y: m.y });
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: m.x, y: m.y });
  res.info.card = await p.until(`[...document.querySelectorAll('.tip')].find((t) => !t.hidden && t.getClientRects().length && t.querySelector('.tip-lint, [class*=f-]'))?.textContent?.slice(0, 80)`, 4000);
  if (!res.info.card) fail('hovering the marker shows no finding');
  else if (sc.card && !sc.card.test(res.info.card)) fail(`the card says ${J(res.info.card)}`);
  await p.shot(`${sc.name}-hover.png`);
  await p.eval(`document.querySelector('[data-panel=play-typeset]').click()`);
  await sleep(300);
  await p.mouse(m.x, m.y);
  await sleep(700);
  res.info.after = await p.eval(`({ panel: document.querySelector('[data-panel=play-findings]').getAttribute('aria-selected'), open: document.querySelectorAll('#play-findings li.open').length })`);
  if (res.info.after.panel !== 'true' || !res.info.after.open) fail(`clicking the marker opens no finding: ${J(res.info.after)}`);
  await p.shot(`${sc.name}-click.png`);
  return res;
}

export async function close(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  if (!await started(p) || !await example(p, sc.example)) { fail('the playground did not start'); return res; }
  const open = `self.satexOpen(${J({ widget: sc.widget, at: sc.at, k: sc.k ?? 0 })})`;
  if (!await p.until(`(self.__e2eOpen ??= ${open}.then((v) => (self.__e2eOpen = null, v))) && document.querySelector('#play-view[open], dialog.sd-dialog[open]')`, 120000)) { fail('the widget did not open'); return res; }
  await sleep(1500);
  const state = `(() => ({ open: !!document.querySelector('#play-view[open], dialog.sd-dialog[open]'), ask: !!document.querySelector('dialog .sd-discard') }))()`;
  const x = `(() => { const d = [...document.querySelectorAll('dialog[open]')].pop(); (d.querySelector('#view-close') ?? d.querySelector('header [aria-label=close]')).click(); })()`;
  const act = async () => { if (sc.how === 'escape') await p.key('Escape'); else await p.eval(x); await sleep(400); };
  if (sc.edit) {
    const [key, value] = sc.edit;
    await p.eval(`(() => { const i = document.querySelector('#play-view input[data-key=${key}]'); i.focus(); i.value = ${J(value)}; i.dispatchEvent(new Event('input', { bubbles: true })); i.dispatchEvent(new Event('change', { bubbles: true })); })()`);
    await sleep(300);
    await act();
    res.info.asked = await p.eval(state);
    await p.shot(`${sc.name}-ask.png`);
    if (!res.info.asked.open || !res.info.asked.ask) fail(`an edit, then ${sc.how ?? 'close'}: no question ${J(res.info.asked)}`);
  }
  await act();
  res.info.closed = await p.eval(state);
  if (res.info.closed.open || res.info.closed.ask) fail(`${sc.edit ? 'closing again' : 'closing untouched'} left ${J(res.info.closed)}`);
  return res;
}

/** `auto`: a fresh viewer presses Compile once with the PDF shown, then types a word in the editor: a new build of that
 * text lands without another press (the first Compile turns the automatic one on). With the Typeset tab shown instead,
 * no build runs until the PDF tab is back (`hidden`). */
export async function auto(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  await example(p, 'hello.tex');
  await started(p);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  const built = () => p.eval(`document.querySelector('#play-pdf .pdf-page img')?.__build ?? null`).then((b) => b && JSON.stringify([b.text?.length, b.ms ?? 0]));
  await p.eval(`document.getElementById('pdf-compile').click()`);
  if (!await p.until(`document.querySelector('#play-pdf .pdf-page img')?.__build && !document.querySelector('#play-pdf.pdf-running')`, 90000)) { res.fails.push('setup: the first build never landed'); return res; }
  const word = sc.word ?? 'Quokka';
  if (sc.hidden) await p.eval(`document.querySelector('.side-tabs [data-panel=play-typeset]').click()`);
  // the builds started from here on: each turns the pane's pdf-running on
  await p.eval(`(() => { const el = document.getElementById('play-pdf'); let was = el.classList.contains('pdf-running'); self.__e2eRuns = 0; new MutationObserver(() => { const now = el.classList.contains('pdf-running'); if (now && !was) self.__e2eRuns++; was = now; }).observe(el, { attributes: true, attributeFilter: ['class'] }); })()`);
  await p.eval(`document.querySelector('.cm-content').focus()`);
  for (const [type, key, code, mod] of [['rawKeyDown', 'End', 35, 2], ['keyUp', 'End', 35, 2]]) await p.s('Input.dispatchKeyEvent', { type, key, code: key, windowsVirtualKeyCode: code, modifiers: mod });
  await p.s('Input.insertText', { text: `\n${word}\n` });
  const text = await p.eval(`satexHealth().text`);
  const landed = (ms) => p.until(`[...document.querySelectorAll('#play-pdf .pdf-page img')].some((i) => i.__build?.text === ${J(text)})`, ms);
  if (sc.hidden) {
    await sleep(4000);
    if (await landed(100) || await p.eval(`self.__e2eRuns`)) res.fails.push('a build ran while the PDF tab was hidden');
    await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  }
  if (!await landed(45000)) res.fails.push(`no build of the typed text landed without a second press (auto: ${await p.eval(`document.getElementById('pdf-compile').title`)})`);
  res.info.built = await built();
  return res;
}

/** `dated`: automatic compile off; a build of `text` fails; the editor's text then becomes `fixed`: the bar's counts (and
 * a failure banner, shown after Type edits) say they are of an older text; Compile replaces them with the build of the text shown. */
export async function dated(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  await example(p, 'hello.tex');
  await started(p);
  const { setText } = await import('../engine.mjs');
  await setText(p, sc.text);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  await p.eval(`(() => { const i = document.querySelector('#pdf-auto-list input[value="0"]'); i.checked = true; i.dispatchEvent(new Event('change', { bubbles: true })); })()`);
  const banner = () => p.eval(`document.querySelector('#play-pdf .pdf-fail')?.textContent ?? ''`);
  const build = async () => { await p.eval(`(self.__e2eG = document.querySelector('#play-pdf .pdf-page img')?.__build ?? null, document.getElementById('pdf-compile').click())`); return p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return i?.__build && i.__build !== self.__e2eG && !document.querySelector('#play-pdf.pdf-running'); })()`, 90000); };
  if (!await build()) { res.fails.push('setup: the first build never landed'); return res; }
  const bar0 = await p.eval(`document.getElementById('pdf-status').textContent`);
  if (!/error/.test(bar0)) { res.fails.push(`setup: the build did not fail: ${J(bar0)}`); return res; }
  await setText(p, sc.fixed);
  await sleep(1500);
  const old = await banner(), bar = await p.eval(`document.getElementById('pdf-status').textContent`);
  res.info.old = old; res.info.bar = bar;
  if (old && !/older text/.test(old)) res.fails.push(`the old build's banner does not say it is of an older text: ${J(old)}`);
  if (/error/.test(bar) && !/older text/.test(bar)) res.fails.push(`the bar counts the old build's errors as the text's: ${J(bar)}`);
  if (!await build()) { res.fails.push('the second build never landed'); return res; }
  const now = await banner(), bar1 = await p.eval(`document.getElementById('pdf-status').textContent`);
  if (now || /error|older/.test(bar1)) res.fails.push(`the build of the fixed text still shows a failure: ${J(now || bar1)}`);
  return res;
}

/** `hoverlate`: `\toprul` Tab in a tabular writes `\toprule` and its `\usepackage{booktabs}`; the pointer rests on
 * `\toprule` while the run of that text goes on. Once the runs end, the open card is the new text's (its macro card),
 * never the last run's "not reached" or a "shows once the analysis ends" left standing. */
export async function hoverlate(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const { setText } = await import('../engine.mjs');
  const quiet = () => p.until(`(() => { const h = satexHealth(); if (h.busy || h.due || h.running || h.queued || h.pending) { self.__q = null; return false; } self.__q ??= performance.now(); return performance.now() - self.__q > 1500; })()`, 240000);
  await example(p, 'hello.tex');
  await started(p);
  await setText(p, sc.text);
  await quiet();
  const raw = (key, code, mod = 0) => ['rawKeyDown', 'keyUp'].reduce((q, type) => q.then(() => p.s('Input.dispatchKeyEvent', { type, key, code: key, windowsVirtualKeyCode: code, modifiers: mod })), Promise.resolve());
  await raw('Home', 36, 2); for (let i = 1; i < sc.line; i++) await raw('ArrowDown', 40); await raw('End', 35);
  for (const k of sc.keys) { await p.key(k); await sleep(150); }
  await sleep(1500);
  await p.key('Tab');
  await sleep(500);
  const at = await p.eval(`(() => { const l = [...document.querySelectorAll('.cm-line')].find((x) => x.textContent.includes(${J(sc.word)})); const w = l && document.createTreeWalker(l, NodeFilter.SHOW_TEXT); let n; while (w && (n = w.nextNode())) { const i = n.textContent.indexOf(${J(sc.word.slice(1))}); if (i >= 0) { const r = document.createRange(); r.setStart(n, i + 2); r.setEnd(n, i + 3); const b = r.getBoundingClientRect(); return [b.x + 1, b.y + b.height / 2]; } } return null; })()`);
  if (!at) { res.fails.push(`setup: no ${sc.word} written: ${J(await p.eval(`satexHealth().text`))}`); return res; }
  await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
  await sleep(600);
  await quiet();
  await sleep(800);
  const card = await p.eval(`[...document.querySelectorAll('.hover')].filter((t) => t.offsetParent).map((t) => t.textContent).join(' || ')`);
  res.info.card = card.slice(0, 120);
  if (!sc.want.test(card)) res.fails.push(`the card once the runs ended: ${J(card.slice(0, 160))}, want ${sc.want}`);
  return res;
}

/** `heldback`: a key at a finding's place holds it back while typing goes on (the word is unfinished); once typing
 * pauses it shows, with no new run. `line` (from 1) starts with `key`'s character typed and then erased. */
export async function heldback(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const count = () => p.eval(`document.getElementById('lsp-count').textContent`);
  const quiet = () => p.until(`(() => { const h = satexHealth(); return !h.busy && !h.due && !h.running && !h.queued && !h.pending; })()`, 120000);
  await example(p, 'hello.tex');
  await started(p);
  await quiet();
  const before = await count();
  const at = await p.eval(`(() => { const r = document.querySelectorAll('.cm-line')[${sc.line - 1}].getBoundingClientRect(); return [r.left + 1, r.top + r.height / 2]; })()`);
  await p.mouse(...at);
  await p.key(sc.key);
  await sleep(500); await quiet();
  await p.key('Backspace');
  await sleep(500); await quiet();
  // past the hold after the last key
  await sleep(4500);
  const after = await count();
  res.info = { before, after };
  if (after !== before) res.fails.push(`findings after the key was erased and typing paused: ${after}, before ${before}`);
  return res;
}

/** `fixfile`: in example `example`, the Findings panel's Fix of the finding that reads `finding` edits only another
 * file: that file is shown with the caret on the edited line (`line` in it), the note names the file. */
export async function fixfile(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  await example(p, sc.example);
  await started(p);
  await p.until(`!satexHealth().busy && [...document.querySelectorAll('#lsp-diagnostics li[data-i]')].some((l) => l.textContent.includes(${J(sc.finding)}))`, 120000);
  await p.eval(`document.querySelector('[data-panel=play-findings]').click()`);
  await p.eval(`[...document.querySelectorAll('#lsp-diagnostics li[data-i]')].find((l) => l.textContent.includes(${J(sc.finding)})).querySelector('button.apply').click()`);
  await sleep(1000);
  const [file, row, toast] = await p.eval(`[satexHealth().file, document.querySelector('.cm-activeLine')?.textContent ?? '', document.getElementById('play-toast').textContent]`);
  res.info = { file, row, toast };
  if (file !== sc.file) res.fails.push(`shown after the fix: ${file}, want ${sc.file}`);
  if (!row.includes(sc.line)) res.fails.push(`the caret's line after the fix: ${J(row)}, want one with ${J(sc.line)}`);
  if (!toast.includes(sc.file)) res.fails.push(`the note does not name ${sc.file}: ${J(toast)}`);
  return res;
}

/** `gutterkey`: a click on line number `line` of hello.tex, then a key: the key lands at that line's start. */
export async function gutterkey(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  await example(p, 'hello.tex');
  await started(p);
  const at = await p.eval(`(() => { const g = [...document.querySelectorAll('.cm-lineNumbers .cm-gutterElement')].find((e) => e.textContent === ${J(String(sc.line))}); const r = g.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
  await p.mouse(...at);
  await sleep(200);
  await p.key('W');
  await sleep(300);
  const row = await p.eval(`satexHealth().text.split('\\n')[${sc.line - 1}]`);
  res.info.row = row;
  if (!row.startsWith('W')) res.fails.push(`line ${sc.line} after a click on its number and W: ${J(row)}`);
  return res;
}

/** `typeundo`: in the Typeset view a click on `word` of hello.tex and a key types it; Ctrl+Z there undoes it in the
 * source, Ctrl+Shift+Z redoes it, and the next key still types (the caret follows the undo). */
export async function typeundo(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  await example(p, 'hello.tex');
  await started(p);
  await p.until(`!satexHealth().busy`, 60000);
  await p.eval(`document.querySelector('[data-panel=play-typeset]').click()`);
  const at = await p.until(`(() => { const w = document.createTreeWalker(document.getElementById('typeset-body'), NodeFilter.SHOW_TEXT); let n; while ((n = w.nextNode())) { const i = n.textContent.indexOf(${J(sc.word)}); if (i >= 0) { const r = document.createRange(); r.setStart(n, i + 2); r.setEnd(n, i + 3); const b = r.getBoundingClientRect(); return b.width && [b.left + 1, b.top + b.height / 2]; } } return null; })()`, 30000);
  if (!at) { res.fails.push(`setup: no ${sc.word} in the Typeset view`); return res; }
  const text = () => p.eval(`satexHealth().text`);
  const mod = async (shift) => { for (const type of ['rawKeyDown', 'keyUp']) await p.s('Input.dispatchKeyEvent', { type, key: shift ? 'Z' : 'z', code: 'KeyZ', windowsVirtualKeyCode: 90, modifiers: 2 | (shift ? 8 : 0) }); await sleep(300); };
  await p.mouse(...at);
  await sleep(800);
  const t0 = await text();
  await p.key('K'); await sleep(400);
  const t1 = await text();
  await mod(false);
  const t2 = await text();
  await mod(true);
  const t3 = await text();
  await mod(false);
  await p.key('J'); await sleep(400);
  const t4 = await text();
  if (t1 === t0) { res.fails.push('setup: the key did not type'); return res; }
  if (t2 !== t0) res.fails.push('Ctrl+Z in the Typeset view did not undo the key');
  if (t3 !== t1) res.fails.push('Ctrl+Shift+Z did not redo it');
  if (t4 === t0) res.fails.push('the key after the undo did not type');
  return res;
}

/** `catcomplete`: the editor's completion and command colors follow the catcodes at the caret (user 2026-11-21 13:16).
 * In a document's body `\(\sum_` names nothing (`_` a subscript): no list opens, and `\sum` lists no name a body cannot
 * type (`\c__socket_…/…`); under `\ExplSyntaxOn` `\c__` lists expl3 names. The colors: `\l_tmpa_tl` in the body is
 * `\l` alone, under `\ExplSyntaxOn` one name. */
export async function catcomplete(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  if (!await started(p) || !await example(p, 'hello.tex')) { fail('the playground did not start'); return res; }
  const T = '\\documentclass{article}\n\\begin{document}\nBody \\l_tmpa_tl here.\n\n\\ExplSyntaxOn\n\\tl_set:Nn \\l_tmpa_tl {x}\n\\ExplSyntaxOff\nEnd.\n\\end{document}\n';
  await p.eval(`(() => { const v = document.querySelector('.cm-content'); v.focus(); })()`);
  // the whole text replaced as a person would: select all, type it
  await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65, modifiers: 2 });
  await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65, modifiers: 2 });
  await p.s('Input.insertText', { text: T });
  // the run of this text, its letter table with it
  await p.until(`satexHealth().shown === satexHealth().version && !satexHealth().busy`, 120000);
  await sleep(1500);
  const list = () => p.eval(`[...document.querySelectorAll('.cm-tooltip-autocomplete li')].map((l) => l.querySelector('.cm-completionLabel')?.textContent ?? l.textContent)`);
  const typeAt = async (needle, text) => {
    // the caret right after `needle`, then the keys
    const at = await p.eval(`(() => { const t = satexHealth().text; return t.indexOf(${J(needle)}) + ${J(needle)}.length; })()`);
    await p.eval(`satexEditorSelect(${at})`);
    for (const ch of text) { await p.s('Input.insertText', { text: ch }); await sleep(120); }
    await sleep(900);
  };
  // body: a new line before `End.`, `\(\sum` then `_`
  await typeAt('ExplSyntaxOff\n', '\\(\\sum');
  const sum = await list();
  res.info.sum = sum.slice(0, 8);
  if (!sum.length) fail('`\\sum` opens no list in the body');
  const odd = sum.filter((l) => l.length > 1 && /[_:@/]/.test(l));
  if (odd.length) fail(`names a body cannot type: ${J(odd.slice(0, 5))}`);
  await p.shot(`${sc.name}-sum.png`);
  await p.s('Input.insertText', { text: '_' });
  await sleep(900);
  const sub = await list();
  res.info.sub = sub.slice(0, 5);
  if (sub.length) fail(`after \`\\sum_\` a list opened: ${J(sub.slice(0, 5))}`);
  await p.shot(`${sc.name}-sub.png`);
  await p.key('Escape');
  // expl3: a new line after `\ExplSyntaxOn`, `\c__`
  await typeAt('\\ExplSyntaxOn\n', '\\c__');
  const expl = await list();
  res.info.expl = expl.slice(0, 5);
  if (!expl.some((l) => /^c__/.test(l))) fail(`\`\\c__\` under \\ExplSyntaxOn lists no expl3 name: ${J(expl.slice(0, 5))}`);
  if (expl.some((l) => l.includes('/'))) fail('a name with `/` listed under \\ExplSyntaxOn');
  await p.shot(`${sc.name}-expl.png`);
  await p.key('Escape');
  // the colors: the command tokens of each line
  res.info.letters = await p.eval(`satexLetters()`);
  // the colors: each line's text with the command-colored characters upper-cased in a mask (a finding's underline
  // splits a token's DOM spans, so characters, not spans, are compared)
  res.info.masks = await p.eval(`[...document.querySelectorAll('.cm-line')].map((l) => { let m = ''; const w = document.createTreeWalker(l, NodeFilter.SHOW_TEXT); for (let t = w.nextNode(); t; t = w.nextNode()) m += t.parentElement.closest('.tok-cmd') ? t.data.replace(/./g, 'C') : t.data.replace(/./g, '.'); return [l.textContent, m]; })`);
  const colored = (needle, nth) => { const rows = res.info.masks.filter(([t]) => t.includes(needle)), r = rows[nth]; if (!r) return null; const i = r[0].indexOf(needle); return r[1].slice(i, i + needle.length); };
  if (colored('\\l_tmpa_tl', 0) !== 'CC........') fail(`body \\l_tmpa_tl colored ${colored('\\l_tmpa_tl', 0)} (want only \\l)`);
  if (colored('\\l_tmpa_tl', 1) !== 'CCCCCCCCCC') fail(`expl3 \\l_tmpa_tl colored ${colored('\\l_tmpa_tl', 1)} (want whole)`);
  return res;
}

/** `hoversteady`: the pointer rests on a finding's word of `line` (from 1, `word`) while a run goes on (a comment line
 * added at the end starts it): the card shows from the last run and is asked again when the run ends. It never hides
 * and is written again only when its text changes. It offers Ignore here and Ignore rule. */
export async function hoversteady(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  const quiet = () => p.until(`(() => { const h = satexHealth(); if (h.busy || h.due || h.running || h.queued || h.pending) { self.__q = null; return false; } self.__q ??= performance.now(); return performance.now() - self.__q > 1500; })()`, 240000);
  if (!await started(p) || !await example(p, sc.example ?? 'hello.tex')) { fail('the playground did not start'); return res; }
  await quiet();
  const word = `(() => { const l = document.querySelectorAll('.cm-line')[${sc.line - 1}]; const w = l && document.createTreeWalker(l, NodeFilter.SHOW_TEXT); let n; while (w && (n = w.nextNode())) { const i = n.textContent.indexOf(${J(sc.word)}); if (i >= 0) { const r = document.createRange(); r.setStart(n, i + 1); r.setEnd(n, i + 2); const b = r.getBoundingClientRect(); return [b.x + b.width / 2, b.y + b.height / 2]; } } return null; })()`;
  const card = `[...document.querySelectorAll('.tip-card')].find((t) => !t.hidden && t.querySelector('.tip-lint'))`;
  // every write of the card once it shows: the same text written again (a redraw) or the card hidden is a flicker
  await p.eval(`(() => { self.__hs = { writes: 0, same: 0, hidden: 0, shown: 0, text: '' };
    const watch = (el) => new MutationObserver((ms) => { const s = self.__hs; if (!s.on) return; if (ms.some((m) => m.type === 'attributes') && el.hidden) s.hidden++;
      if (ms.some((m) => m.type === 'childList')) { s.writes++; if (el.textContent === s.text) s.same++; s.text = el.textContent; } })
      .observe(el, { childList: true, subtree: true, attributes: true, attributeFilter: ['hidden'] });
    const el = document.querySelector('.tip-card'); if (el) watch(el); else self.__hsWatch = watch; })()`);
  // each round: a comment line added at the end of the file (a run starts), the pointer onto the word at once, resting
  // there while the run ends (the card answered from the last run, then asked again)
  for (let i = 0; i < (sc.runs ?? 3); i++) {
    await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 5, y: 5 });
    await sleep(300);
    await p.drv.act({ insert: { at: '\\end{document}', k: 14, text: '\n%' } });
    const at = await p.eval(word);
    if (!at) { fail(`setup: no ${sc.word} on line ${sc.line}`); return res; }
    await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0] - 1, y: at[1] });
    await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x: at[0], y: at[1] });
    if (!await p.until(`!!${card}`, 8000)) { fail(`round ${i}: hovering the finding shows no card`); return res; }
    await p.eval(`(() => { const el = document.querySelector('.tip-card'); if (self.__hsWatch) { self.__hsWatch(el); self.__hsWatch = null; } Object.assign(self.__hs, { on: true, text: el.textContent }); self.__hs.shown++; })()`);
    await sleep(200);
    await quiet();
    await sleep(1500);
    await p.eval(`self.__hs.on = false`);
  }
  res.info.steady = await p.eval(`(({ writes, same, hidden, shown }) => ({ writes, same, hidden, shown }))(self.__hs)`);
  const { writes, same, hidden } = res.info.steady;
  if (same || hidden) fail(`the resting card flickered: ${same} rewrites of the same text, ${hidden} hides (of ${writes} writes)`);
  const buttons = await p.eval(`[...(${card})?.querySelectorAll('button[data-fix]') ?? []].map((b) => b.textContent)`);
  res.info.buttons = buttons;
  if (!buttons.some((b) => /^Ignore \S+ here$/.test(b)) || !buttons.some((b) => /^Ignore rule /.test(b))) fail(`the card offers ${J(buttons)}, want Ignore here and Ignore rule`);
  await p.shot(`${sc.name}-card.png`);
  return res;
}
