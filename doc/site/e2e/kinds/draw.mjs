// The Draw dialog (draw.ts) driven as a person draws: strokes and taps on the pad, its keys and buttons, per view
// size and theme. Scenario fields: `check` (what is drawn and asserted), `size` ([w, h], default the suite's), `theme`.
import { sleep } from '../lib.mjs';

const J = JSON.stringify;
/** The pad's inked pixels. */
/** Ink counts alike: a stroke drawn piece by piece and drawn again whole differ by a few edge pixels. */
const near = (a, b) => Math.abs(a - b) <= 0.02 * Math.max(a, b);
const INK = `(() => { const c = document.getElementById('draw-pad'), d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data; let n = 0; for (let i = 3; i < d.length; i += 4) if (d[i] > 40) n++; return n; })()`;

/** A stroke through `pts` (shares of the pad's box), as a mouse draws it; one point is a tap. */
async function stroke(p, pts) {
  const b = await p.eval(`(() => { const b = document.getElementById('draw-pad').getBoundingClientRect(); return [b.left, b.top, b.width, b.height]; })()`);
  const at = pts.map(([x, y]) => [b[0] + x * b[2], b[1] + y * b[3]]);
  await p.s('Input.dispatchMouseEvent', { type: 'mousePressed', x: at[0][0], y: at[0][1], button: 'left', buttons: 1, clickCount: 1 });
  for (const [x, y] of at.slice(1)) await p.s('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, button: 'left', buttons: 1 });
  await p.s('Input.dispatchMouseEvent', { type: 'mouseReleased', x: at.at(-1)[0], y: at.at(-1)[1], button: 'left', buttons: 0, clickCount: 1 });
  await sleep(120);
}
/** Ctrl (+Shift) with a letter key. */
async function chord(p, k, shift = false) {
  const m = 2 | (shift ? 8 : 0), key = shift ? k.toUpperCase() : k;
  await p.s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key, code: `Key${k.toUpperCase()}`, windowsVirtualKeyCode: k.toUpperCase().charCodeAt(0), modifiers: m });
  await p.s('Input.dispatchKeyEvent', { type: 'keyUp', key, code: `Key${k.toUpperCase()}`, windowsVirtualKeyCode: k.toUpperCase().charCodeAt(0), modifiers: m });
  await sleep(120);
}

export async function draw(p, sc) {
  const r = { name: sc.name, fails: [], perf: [], info: {} }, fail = (m) => r.fails.push(m);
  if (sc.size || sc.theme) {
    const [width, height] = sc.size ?? [1400, 900];
    await p.s('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: width < 500 });
    await p.s('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: sc.theme ?? 'light' }] });
  }
  await p.eval(`document.getElementById('draw-open').click()`);
  if (!await p.until(`document.getElementById('play-draw').open && document.getElementById('draw-pad').getBoundingClientRect().width > 50`, 10000)) throw new Error('the Draw dialog did not open');
  const text0 = await p.eval('satexHealth().text');
  if (sc.check === 'dot') {
    await stroke(p, [[0.5, 0.5]]);
    const n = await p.eval(INK);
    if (!(n > 20)) fail(`a tap inks no dot (${n} px)`);
  } else if (sc.check === 'undo') {
    await stroke(p, [[0.2, 0.2], [0.8, 0.2]]);
    const a = await p.eval(INK);
    await stroke(p, [[0.2, 0.7], [0.8, 0.7]]);
    const ab = await p.eval(INK);
    await chord(p, 'z');
    if (!near(await p.eval(INK), a)) fail(`Ctrl+Z: want the first stroke alone (${a} px), got ${await p.eval(INK)}`);
    await chord(p, 'z', true);
    if (!near(await p.eval(INK), ab)) fail(`Ctrl+Shift+Z: want both strokes (${ab} px), got ${await p.eval(INK)}`);
    await p.click('#draw-clear');
    await sleep(120);
    if (await p.eval(INK) !== 0) fail('clear left ink');
    await p.click('#draw-undo');
    await sleep(120);
    if (!near(await p.eval(INK), ab)) fail(`the undo button after clear: want both strokes back, got ${await p.eval(INK)} px`);
    if (!await p.eval(`document.getElementById('play-draw').open`)) fail('the dialog closed');
  } else if (sc.check === 'fit') {
    await stroke(p, [[0.1, 0.5], [0.9, 0.5]]);
    const fit = await p.eval(`(() => { const d = document.getElementById('play-draw').getBoundingClientRect(), pad = document.getElementById('draw-pad'), cs = getComputedStyle(pad), lum = (c) => { const [r, g, b] = c.match(/[\\d.]+/g).map(Number); return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255; };
      const out = [...document.querySelectorAll('#play-draw .draw-tools button, #draw-pad')].filter((e) => { const b = e.getBoundingClientRect(); return b.left < d.left - 0.5 || b.right > d.right + 0.5; }).map((e) => e.id);
      return { over: document.documentElement.scrollWidth > innerWidth, out, contrast: Math.abs(lum(cs.color) - lum(cs.backgroundColor)) }; })()`);
    r.info.fit = fit;
    if (fit.over) fail('the page scrolls sideways');
    if (fit.out.length) fail(`outside the dialog: ${fit.out.join(', ')}`);
    if (fit.contrast < 0.5) fail(`the ink hardly shows on the pad (contrast ${fit.contrast.toFixed(2)})`);
  } else if (sc.check === 'formula') {
    // the formula bar: a fraction's slots filled by typing and Tab, a superscript, then Insert
    await p.click('#draw-formula');
    await p.click('[data-t=frac]');
    await p.s('Input.insertText', { text: 'a' });
    await p.key('Tab');
    await p.s('Input.insertText', { text: 'b' });
    await p.click('[data-t=sup]');
    await p.s('Input.insertText', { text: '2' });
    const bar = await p.eval(`(() => { const i = document.getElementById('draw-f-src'), d = document.getElementById('play-draw').getBoundingClientRect(), f = document.getElementById('draw-f').getBoundingClientRect();
      return { src: i.value, boxes: (document.getElementById('draw-f-view').innerHTML.match(/□/g) ?? []).length, inside: f.left >= d.left && f.right <= d.right, over: document.documentElement.scrollWidth > innerWidth }; })()`);
    r.info.bar = bar;
    if (bar.src !== '\\frac{a}{b^{2}}') fail(`the formula's LaTeX: want \\frac{a}{b^{2}}, got ${J(bar.src)}`);
    if (bar.boxes) fail(`${bar.boxes} empty slots drawn in a full formula`);
    if (!bar.inside || bar.over) fail('the formula bar does not fit the dialog');
    r.shot = await p.shot(`${sc.name}-bar.png`);
    await p.click('#draw-f-insert');
    if (!await p.until(`satexHealth().text.includes('\\\\frac{a}{b^2}')`, 15000)) fail(`not inserted: ${J((await p.eval('satexHealth().text')).slice(0, 300))}`);
    const t1 = await p.eval('satexHealth().text'), m = /(\\\(|\$)\\frac\{a\}\{b\^2\}(\\\)|\$)/.exec(t1);
    if (!m) fail('the formula is not inline math');
    else if (t1.slice(0, m.index) + t1.slice(m.index + m[0].length) !== text0) fail('more than the formula changed');
    if (await p.eval(`document.getElementById('play-draw').open`)) fail('the dialog stayed open');
    return r;
  }
  // nothing drawn reaches the document: the editor's text stays as it was
  if (await p.eval('satexHealth().text') !== text0) fail('the editor text changed');
  r.shot = await p.shot(`${sc.name}.png`);
  return r;
}
