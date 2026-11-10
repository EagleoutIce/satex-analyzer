// Runs a site script as the browser would, for the node tests: `.ts` is transpiled by esbuild first.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { runInThisContext } from 'node:vm';

const local = new URL('../../editors/vscode/node_modules/.bin/esbuild', import.meta.url).pathname;
const esbuild = process.env.ESBUILD || (existsSync(local) ? local : 'esbuild');

/** The `.ts` source of `name` transpiled by esbuild, as the browser runs it, or the `.js` file's text. */
const source = (name) => {
  const ts = new URL(name.replace(/\.js$/, '.ts'), import.meta.url);
  if (!existsSync(ts)) return readFileSync(new URL(name, import.meta.url), 'utf8');
  return execFileSync(esbuild, ['--loader=ts', '--format=iife', '--log-level=error'], { input: readFileSync(ts), maxBuffer: 1 << 28 }).toString();
};

/** The `module.exports` of a script that exports under node (worker.js). */
export const loadModule = (name) => {
  const module = globalThis.module = { exports: {} };
  try { runInThisContext(source(name), { filename: name }); } finally { delete globalThis.module; }
  return module.exports;
};

/** Runs `name` (`views.js`) from this directory, its `.ts` source when there is one. */
export const load = (name) => {
  return runInThisContext(source(name), { filename: name });
};
