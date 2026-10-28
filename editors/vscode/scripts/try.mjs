// Build satex, bundle it, package a VSIX and (with --install) install it.
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = join(here, '..', '..');
const run = (cmd, args, cwd = here) => execFileSync(cmd, args, { cwd, stdio: 'inherit', shell: process.platform === 'win32' });

run('cargo', ['build', '--release'], repo);
const exe = process.platform === 'win32' ? 'satex.exe' : 'satex';
const bin = join(here, 'bin', `${process.platform}-${process.arch}`);
mkdirSync(bin, { recursive: true });
copyFileSync(join(repo, 'target', 'release', exe), join(bin, exe));
run('npm', ['run', 'compile']);
const vsix = join(here, 'satex-local.vsix');
run('npx', ['vsce', 'package', '--no-dependencies', '--out', vsix]);
if (process.argv.includes('--install')) {
    run('code', ['--install-extension', vsix, '--force']);
}
