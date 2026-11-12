// Fills media/ with the webviews' scripts from doc/site: a `.ts` source is transpiled (one IIFE), a `.js` copied.
import { transformSync } from 'esbuild';
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename } from 'node:path';

const site = new URL('../../../doc/site/', import.meta.url), media = new URL('../media/', import.meta.url);
mkdirSync(media, { recursive: true });
for (const name of ['lengths', 'snapshot', 'views', 'bib', 'dialogs']) {
  const ts = new URL(`${name}.ts`, site), out = new URL(`${name}.js`, media);
  // esbuild's API, not its binary: on Windows the binary is a `.cmd` that execFileSync cannot start.
  if (existsSync(ts)) writeFileSync(out, transformSync(readFileSync(ts, 'utf8'), { loader: 'ts', format: 'iife', logLevel: 'error' }).code);
  else copyFileSync(new URL(`${name}.js`, site), out);
}
copyFileSync(new URL('../../../satex.schema.json', import.meta.url), new URL(basename('satex.schema.json'), media));
