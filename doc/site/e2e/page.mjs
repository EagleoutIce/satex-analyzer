// The drivers' one step vocabulary (showcase/capture.mjs, the e2e kinds): a person's mouse and keys over the DevTools
// protocol, the page read through the test probe (probe.js, injected before the page's scripts). The page is only
// ever asked `satexTest.run(<JSON>)`: targets, conditions and actions are data (probe.js lists them).
// A step: { until: cond, ms } · { do: action, until?: cond } (repeated until the condition holds) · { click: target,
// right, double, glide, after, retry: cond } · { hover: target, glide, after } · { drag: [from, to], over }
// · { menu: [label, …] } through an open page-actions menu · { type: text } (\t Tab, \n Enter) · { key: name, shift, ctrl } · { upload: { sel, file } } · { open: true | {…} }
// · { wait: ms } after any of them.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

export const PROBE = readFileSync(new URL('probe.js', import.meta.url), 'utf8');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const ease = (t) => t < .5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;

/** A driver over `send(method, params)` of one page's session (resolving to the reply's `result`). `scale()` stretches waits (machine load), `deadline()`
 * caps them (a take's shared budget). */
export function driver(send, { scale = () => 1, deadline = () => Infinity } = {}) {
  let at = [1, 1];
  const d = {
    async run(cmd) {
      const r = await send('Runtime.evaluate', { expression: `satexTest.run(${JSON.stringify(cmd)})`, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(`${JSON.stringify(cmd).slice(0, 120)}: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}`);
      return r.result?.value;
    },
    point: (t) => d.run({ point: t }),
    act: (a) => d.run({ act: a }),
    cond: (c) => d.run({ cond: c }),
    /** `c` held (for `hold` ms without a break) within `ms`, stretched by load and capped by the deadline; polled in the page. */
    async until(c, ms = 60000, hold = 0) {
      const left = Math.max(0, Math.min(ms * scale(), deadline() - Date.now()));
      return d.run({ until: c, ms: left, hold }).catch(() => false);
    },
    health: () => d.run({ health: true }),
    text: () => d.run({ text: true }),
    /** A page word (probe.js word target) as [left, top, right, bottom, page, text, scale] in client px, or null. */
    word: async (t) => { const w = await d.run({ words: t }); return w && [...w.box, w.page, w.text, w.k]; },
    state: () => d.run({ state: true }).catch((e) => e.message.slice(0, 80)),
    mouse: async (type, [x, y], extra = {}) => { at = [x, y]; return send('Input.dispatchMouseEvent', { type, x, y, button: 'none', ...extra }); },
    /** An eased move from where the mouse is to `to` over `ms`, as a hand moves. */
    async glide(to, ms, extra = {}) {
      const a = at, n = Math.max(1, Math.round(ms / 30));
      for (let i = 1; i <= n; i++) { const e = ease(i / n); await d.mouse('mouseMoved', [a[0] + (to[0] - a[0]) * e, a[1] + (to[1] - a[1]) * e], extra); await sleep(ms / n); }
    },
    async target(t) { const p = await d.point(t); if (!p) throw new Error(`nothing at ${JSON.stringify(t)}; the page: ${await d.state()}`); return p; },
    /** The mouse to `t` (glided over `glide` ms, else a short nudge onto it), clicked unless `hoverOnly`. */
    async click(t, { right = false, double = false, glide, after, hoverOnly = false } = {}) {
      const p = await d.target(t);
      if (glide) await d.glide(p, glide); else { await d.mouse('mouseMoved', [p[0] - 4, p[1]]); await d.mouse('mouseMoved', p); }
      if (!hoverOnly) {
        const button = right ? 'right' : 'left';
        for (const n of double ? [1, 2] : [1]) for (const type of ['mousePressed', 'mouseReleased']) await d.mouse(type, p, { button, buttons: type === 'mousePressed' ? (right ? 2 : 1) : 0, clickCount: n });
      }
      if (after) await sleep(after);
    },
    /** A mouse drag from one target to another over `ms`, eased; the mouse glides to the start first. */
    async drag(from, to, ms = 900) {
      const a = await d.target(from), b = await d.target(to), n = Math.max(8, Math.round(ms / 30));
      await d.glide(a, Math.min(500, Math.hypot(a[0] - at[0], a[1] - at[1]) * 2));
      await d.mouse('mousePressed', a, { button: 'left', buttons: 1, clickCount: 1 });
      for (let i = 1; i <= n; i++) { const e = ease(i / n); await d.mouse('mouseMoved', [a[0] + (b[0] - a[0]) * e, a[1] + (b[1] - a[1]) * e], { button: 'left', buttons: 1 }); await sleep(ms / n); }
      await d.mouse('mouseReleased', b, { button: 'left', buttons: 0, clickCount: 1 });
    },
    /** Keys typed as a hand types them, 70-130 ms apart (`pace` overrides): `\t` is Tab, `\n` Enter. */
    async type(text, pace = () => 70 + Math.random() * 60) {
      const KEYS = { '\t': ['Tab', 9], '\n': ['Enter', 13] };
      for (const c of text) {
        // a sign (`$`, `\`, `_`, `}`) has no key code of its own here: its code as a letter would be Home, F12…
        const [key, code] = KEYS[c] ?? (c === '.' ? ['.', 190] : [c, /[a-z\d ]/i.test(c) ? c.toUpperCase().charCodeAt(0) : 0]);
        const name = KEYS[c] ? key : /\d/.test(c) ? `Digit${c}` : c === '.' ? 'Period' : /[a-z]/i.test(c) ? `Key${c.toUpperCase()}` : c === ' ' ? 'Space' : '';
        const base = { key, code: name, windowsVirtualKeyCode: code, nativeVirtualKeyCode: code };
        await send('Input.dispatchKeyEvent', { type: 'keyDown', ...base, ...(KEYS[c] ? (c === '\n' ? { text: '\r' } : {}) : { text: c }) });
        await send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
        await sleep(pace());
      }
    },
    /** A named key (F10, Escape, ArrowRight…) or a letter, with Shift and Ctrl as asked. */
    async key(k, { shift = false, ctrl = false } = {}) {
      const CODES = { Backspace: 8, Tab: 9, Enter: 13, Escape: 27, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, Delete: 46, F10: 121 };
      const vk = CODES[k] ?? k.toUpperCase().charCodeAt(0), code = CODES[k] ? k : `Key${k.toUpperCase()}`, modifiers = (shift ? 8 : 0) | (ctrl ? 2 : 0);
      await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers });
      await send('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers });
    },
    /** A file given to a file input as a person picks it. */
    async upload(sel, file) {
      const { root } = await send('DOM.getDocument', { depth: 0 });
      const { nodeId } = await send('DOM.querySelector', { nodeId: root.nodeId, selector: sel });
      await send('DOM.setFileInputFiles', { nodeId, files: [resolve(file)] });
    },
    /** Through an open page-actions menu by its row labels: each row hovered (its submenu opens), the last one clicked. */
    async menu(path, pause = 140) {
      for (let i = 0; i < path.length; i++) {
        const row = { sel: ':scope > button', within: '.pa-menu', exact: path[i], left: 40 }, at = await d.point(row);
        if (!at) throw new Error(`no ${JSON.stringify(path[i])} in ${JSON.stringify(await d.run({ labels: { sel: ':scope > button', within: '.pa-menu' } }))}`);
        await d.mouse('mouseMoved', at);
        await sleep(pause);
        if (i === path.length - 1) for (const type of ['mousePressed', 'mouseReleased']) await d.mouse(type, at, { button: 'left', buttons: type === 'mousePressed' ? 1 : 0, clickCount: 1 });
      }
    },
    /** One step (the vocabulary above); `ctx.open` is the entry's widget for `{ open: true }`, `ctx.dir` where files are. */
    async step(s, ctx = {}) {
      if (s.open) {
        const w = s.open === true ? Object.fromEntries(['widget', 'at', 'k'].map((k, i) => [k, ctx.open[i]])) : s.open;
        s = { do: { open: w }, until: 'dialog', ...s, open: undefined };
      }
      if (s.drag) await d.drag(...s.drag, s.over);
      else if (s.type != null) await d.type(s.type);
      else if (s.key) await d.key(s.key, s);
      else if (s.upload) await d.upload(s.upload.sel, resolve(ctx.dir ?? '.', s.upload.file));
      else if (s.menu) await d.menu(s.menu);
      else if (s.click || s.hover) {
        await d.click(s.click ?? s.hover, { ...s, hoverOnly: !s.click });
        // a menu a click did not open (an event lost under load): clicked again until it shows
        for (let k = 0; s.retry && k < 4 && !await d.until(s.retry, 3000); k++) await d.click(s.click, { ...s, glide: 0 });
      } else if (s.do) {
        // an action with a condition repeats until it holds: a widget asked for before the server answers opens nothing
        let ok = !s.until;
        for (const end = Math.min(Date.now() + (s.ms ?? 120000) * scale(), deadline()); !ok && Date.now() < end || !ok && !s.until;) {
          await d.act(s.do).catch((e) => process.env.DEBUG && console.log('action failed', e.message.slice(0, 300)));
          ok = !s.until || await d.until(s.until, 8000);
        }
        // an action without a condition must find its target: a press on nothing is a failed step, never a silent one
        if (!s.until && await d.act(s.do) === false) throw new Error(`nothing to act on: ${JSON.stringify(s.do).slice(0, 160)}; the page: ${await d.state()}`);
        if (!ok) throw Object.assign(new Error(`never held: ${JSON.stringify(s.until).slice(0, 160)}; the page: ${await d.state()}`), { wait: true });
      } else if (s.until && !await d.until(s.until, s.ms ?? 120000)) throw Object.assign(new Error(`never held: ${JSON.stringify(s.until).slice(0, 160)}; the page: ${await d.state()}`), { wait: true });
      if (s.wait) await sleep(s.wait);
    },
  };
  return d;
}
