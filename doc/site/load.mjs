// Runs a site script as the browser would, for the node tests: `.ts` is transpiled by esbuild first.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { runInThisContext } from 'node:vm';

const local = new URL('../../editors/vscode/node_modules/.bin/esbuild', import.meta.url).pathname;
const esbuild = process.env.ESBUILD || (existsSync(local) ? local : 'esbuild');

/** Runs `name` (`views.js`) from this directory, its `.ts` source when there is one. */
export const load = (name) => {
  const ts = new URL(name.replace(/\.js$/, '.ts'), import.meta.url);
  if (!existsSync(ts)) return runInThisContext(readFileSync(new URL(name, import.meta.url), 'utf8'), { filename: name });
  const code = execFileSync(esbuild, ['--loader=ts', '--format=iife', '--log-level=error'], { input: readFileSync(ts), maxBuffer: 1 << 28 }).toString();
  return runInThisContext(code, { filename: name });
};
