import { strict as assert } from 'node:assert';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { binaryProblem, Ran, startProblem, usableCaps, wrap } from '../launch';

const ok: Ran = { status: 0, stderr: '' };
const errno = (code: string): Ran => ({ error: Object.assign(new Error(code), { code }), status: null });

/**
 * Bug pinned: in a devcontainer `systemd-run --user --version` succeeds but no user bus runs, so
 * the scope failed ("Failed to connect to user scope bus") and the server never started (EPIPE).
 * The probe starts a real scope, and a failed one falls back to `ulimit`.
 */
test('a host without a user bus caps through ulimit', () => {
    const seen: string[] = [];
    const found = usableCaps((command) => {
        seen.push(command);
        return command === 'systemd-run'
            ? { status: 1, stderr: 'Failed to connect to user scope bus via local transport: No such file or directory\n' }
            : ok;
    }, 3072, 2, true);
    assert.deepEqual(found.caps, ['ulimit', 'plain']);
    assert.deepEqual(seen, ['systemd-run', '/bin/sh']);
    assert.match(found.skipped[0], /^scope: Failed to connect to user scope bus/);
});

test('without systemd-run, sh or a budget the server still starts', () => {
    assert.deepEqual(usableCaps(() => errno('ENOENT'), 3072, 2, true).caps, ['plain']);
    assert.deepEqual(usableCaps(() => ok, undefined, 2, true).caps, ['plain']);
    assert.deepEqual(usableCaps(() => ok, 3072, 2, false).caps, ['ulimit', 'plain']);
    assert.deepEqual(usableCaps(() => ok, 3072, 2, true).caps, ['scope', 'ulimit', 'plain']);
});

test('each cap wraps the server command', () => {
    assert.deepEqual(wrap('plain', ['satex', 'lsp'], 3072, 2), ['satex', ['lsp']]);
    assert.deepEqual(wrap('scope', ['satex', 'lsp'], 3072, 0)[1],
        ['--user', '--scope', '--quiet', '-p', 'MemoryMax=3072M', '-p', 'MemorySwapMax=0', '--', 'satex', 'lsp']);
    assert.ok(wrap('scope', ['satex'], 3072, 2)[1].includes('CPUQuota=200%'));
});

/** The ulimit cap really holds: the wrapped shell sees the limit, and the command runs. */
test('the ulimit cap sets the data limit and runs the command', { skip: process.platform !== 'linux' }, () => {
    const [command, args] = wrap('ulimit', ['/bin/sh', '-c', 'ulimit -d'], 512, 2);
    const r = spawnSync(command, args, { encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
    assert.equal(r.stdout.trim(), String(512 * 1024));
});

test('a binary that cannot run here says why and what to do', () => {
    assert.equal(binaryProblem('satex', ok), undefined);
    assert.match(binaryProblem('/x/satex', errno('ENOENT')) ?? '', /loader is missing.*cargo install/);
    assert.match(binaryProblem('/x/satex', errno('EACCES')) ?? '', /chmod \+x \/x\/satex/);
    assert.match(binaryProblem('/x/satex', { status: 1, stderr: "/x/satex: /lib/libc.so.6: version `GLIBC_2.39' not found\n" }) ?? '',
        /newer C library.*GLIBC_2\.39/);
    assert.match(binaryProblem('/x/satex', { status: 2, stderr: 'boom\n' }) ?? '', /--version failed: boom/);
});

test('a server that died before it answered is explained', () => {
    assert.match(startProblem(null, 'SIGKILL', ''), /killed.*memory.*maxMemory/);
    assert.equal(startProblem(1, null, 'satex: x.yaml: plugins.packages ignored: only the user\'s own config sets it\nerror: bad flag\n'), 'error: bad flag (exit code 1)');
    assert.equal(startProblem(3, null, ''), 'exit code 3');
    assert.match(startProblem(null, 'SIGABRT', 'memory allocation of 4096 bytes failed\n'), /memory cap.*maxMemory/);
});
