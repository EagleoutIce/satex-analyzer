// Typing into the footer and clicks on empty page space (a new paragraph), in the PDF view's Type mode.
// Scenario fields: `source`; `click` (page code: the page's words → [page, x, y] in bp); `keys`; `want`/`not`
// (regexes on the source); `pdf` (regexes on the built page's last row); `note`, `echo` (regexes); `ghost`.
import { sleep } from '../lib.mjs';
import { setText, settled } from '../engine.mjs';

const J = JSON.stringify;
const built = (p) => p.until(`(() => { const i = document.querySelector('#play-pdf .pdf-page img'); return satexPdfWords().length && i?.naturalWidth > 100 && i.__build && i.__build !== self.__e2eL && !document.querySelector('#play-pdf.pdf-running'); })()`, 120000);

export async function foot(p, sc) {
  const res = { name: sc.name, fails: [], perf: [], info: {} };
  const fail = (m) => res.fails.push(m);
  if (!await p.until(`document.querySelector('#play-status')?.dataset.done === 'ready'`, 120000)) { fail('the playground did not start'); return res; }
  await p.eval(`document.querySelector('[data-example="hello.tex"]').click()`);
  await p.until(`satexHealth().example === 'hello.tex' && satexHealth().text?.length > 10`, 20000);
  await sleep(400);
  await setText(p, sc.source);
  await settled(p);
  await p.eval(`document.querySelector('.side-tabs [data-panel=play-pdf]').click()`);
  await p.eval(`(self.__e2eL = document.querySelector('#play-pdf .pdf-page img')?.__build ?? null, document.getElementById('pdf-compile').click())`);
  if (!await built(p)) { fail('no build'); return res; }
  if (await p.eval(`document.getElementById('pdf-type').getAttribute('aria-pressed')`) !== 'true') await p.eval(`document.getElementById('pdf-type').click()`);
  await sleep(600);
  // the click in client px, the page scrolled to show it
  const at = await p.eval(`(async () => { const ws = satexPdfWords(), dpi = ws.dpi, c = (${sc.click})(ws); if (!c) return null;
    const pg = () => document.querySelector('#play-pdf .pdf-page[data-page="' + c[0] + '"] img');
    pg().scrollIntoView({ block: 'center' }); await new Promise((ok) => setTimeout(ok, 400));
    const im = pg(), r = im.getBoundingClientRect(), k = r.width / (im.naturalWidth * 72 / dpi), sc = document.querySelector('#play-pdf .pdf-pages').getBoundingClientRect();
    let y = r.top + c[2] * k; if (y > sc.bottom - 20 || y < sc.top + 20) { document.querySelector('#play-pdf .pdf-pages').scrollTop += y - (sc.top + sc.height / 2); await new Promise((ok) => setTimeout(ok, 400)); }
    const r2 = pg().getBoundingClientRect(); return [r2.left + c[1] * k, r2.top + c[2] * k]; })()`);
  if (!at) { fail('no click target'); return res; }
  const before = await p.eval(`satexHealth().text`);
  await p.mouse(...at);
  await sleep(700);
  res.info.caret = await p.eval(`!!document.querySelector('#play-pdf .pdf-caret')`);
  res.info.ghost = await p.eval(`!!document.querySelector('#play-pdf .pdf-caret.pdf-ghost')`);
  res.info.toastAfterClick = await p.eval(`document.getElementById('play-toast')?.classList.contains('on') ? document.getElementById('play-toast').textContent : ''`);
  if (!res.info.caret) fail(`no caret after the click (${res.toastAfterClick})`);
  if (sc.ghost && !res.info.ghost) fail('no ghost caret');
  await p.shot(`${sc.name}-click.png`);
  for (const k of sc.keys) {
    if (/^(Backspace|Delete|Enter|Escape)$/.test(k)) await p.key(k);
    else for (const ch of k) { await p.key(ch); await sleep(40); }
    await sleep(150);
  }
  await sleep(300);
  res.info.echo = await p.eval(`[...document.querySelectorAll('#play-pdf .pdf-echo')].map((e) => e.textContent).join(' | ')`);
  if (sc.echo && !sc.echo.test(res.info.echo)) fail(`echo ${J(res.info.echo)} lacks ${sc.echo}`);
  res.info.note = await p.eval(`document.querySelector('.type-note')?.textContent ?? ''`);
  if (sc.note && !sc.note.test(res.info.note)) fail(`note ${J(res.info.note)} lacks ${sc.note}`);
  await p.shot(`${sc.name}-typed.png`);
  const text = await p.eval(`satexHealth().text`);
  res.info.diff = text.split('\n').filter((l) => !before.split('\n').includes(l));
  for (const re of sc.want ?? []) if (!re.test(text)) fail(`source lacks ${re}`);
  for (const re of sc.not ?? []) if (re.test(text)) fail(`source has ${re}`);
  // the build of the text as typed
  await p.eval(`(self.__e2eL = document.querySelector('#play-pdf .pdf-page img')?.__build ?? null, document.getElementById('pdf-compile').click())`);
  if (!await built(p)) fail('no build after typing');
  await sleep(500);
  res.info.status = await p.eval(`document.getElementById('pdf-status').textContent.slice(0, 80)`);
  res.info.err = await p.eval(`!!document.querySelector('#play-pdf .pdf-err')`);
  if (res.info.err || /error|fail/i.test(res.info.status)) fail(`compile: ${res.info.status}`);
  const pg = sc.page ?? 1;
  res.info.foot = await p.eval(`(() => { const ws = satexPdfWords().filter((w) => w[0] === ${pg}), last = Math.max(...ws.map((w) => w[4])); return ws.filter((w) => Math.abs(w[4] - last) < 2).sort((a, b) => a[1] - b[1]).map((w) => w[5]).join(' '); })()`);
  res.info.title = sc.name === 'title-page-kept' ? await p.eval(`(() => { const ws = satexPdfWords().filter((w) => w[0] === 1), last = Math.max(...ws.map((w) => w[4])); return ws.filter((w) => Math.abs(w[4] - last) < 2).map((w) => w[5]).join(' '); })()`) : null;
  if (sc.name === 'title-page-kept' && res.info.title !== '1') fail(`title page footer ${J(res.info.title)}: plain kept wants "1"`);
  if (sc.name === 'title-page-kept' && !/^p\. 2$/.test(res.info.foot)) fail(`page 2 footer ${J(res.info.foot)}`);
  else if (sc.name !== 'title-page-kept') for (const re of sc.pdf ?? []) if (!re.test(res.info.foot)) fail(`page footer ${J(res.info.foot)} lacks ${re}`);
  await p.shot(`${sc.name}-built.png`);
  res.info.errors = p.errors.slice();
  if (p.errors.length) fail(`page errors: ${p.errors.join('; ')}`);
  return res;
}

