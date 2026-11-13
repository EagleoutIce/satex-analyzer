import { strict as assert } from 'node:assert';
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { binaryProblem, Events, findBinary, Launcher, Ran, run, startProblem, usableCaps, wrap } from '../launch';

const ok: Ran = { status: 0, stderr: '' };
const errno = (code: string): Ran => ({ error: Object.assign(new Error(code), { code }), status: null });
const BUS = 'Failed to connect to user scope bus via local transport: No such file or directory';
const linux = process.platform === 'linux';

/** A directory of shell scripts that stand in for `systemd-run` and the server. */
const dir = mkdtempSync(join(tmpdir(), 'satex-launch-'));
after(() => rmSync(dir, { recursive: true, force: true }));
function script(name: string, body: string): string {
    const file = join(dir, name);
    writeFileSync(file, `#!/bin/sh\n${body}\n`);
    chmodSync(file, 0o755);
    return file;
}

/** What a launcher reported, and a promise for its next start failure or crash. */
function recorder() {
    const seen: string[] = [];
    let wake: () => void = () => undefined;
    const next = () => new Promise<void>(resolve => { wake = resolve; });
    const events: Events = {
        log: line => seen.push(`log ${line}`),
        uncapped: () => seen.push('uncapped'),
        failedStart: (problem, cap, fallback) => { seen.push(`failed ${cap} ${fallback} ${problem}`); wake(); },
        crashed: problem => { seen.push(`crashed ${problem}`); wake(); },
    };
    return { seen, events, next };
}

/** Everything the server says on stdout, until it closes it. */
function stdout(child: import('node:child_process').ChildProcess): Promise<string> {
    return new Promise(resolve => {
        let out = '';
        child.stdout!.on('data', (d: Buffer) => { out += d.toString(); });
        child.stdout!.on('end', () => resolve(out));
    });
}

/**
 * Bug pinned: in a devcontainer `systemd-run --user --version` succeeds but no user bus runs, so
 * the scope failed ("Failed to connect to user scope bus") and the server never started (EPIPE).
 * The probe starts a real scope, and a failed one falls back to `ulimit`.
 */
test('a host without a user bus caps through ulimit', async () => {
    const seen: string[] = [];
    const found = await usableCaps(async command => {
        seen.push(command);
        return command === 'systemd-run' ? { status: 1, stderr: `${BUS}\n` } : ok;
    }, 3072, 2, true);
    assert.deepEqual(found.caps, ['ulimit', 'plain']);
    assert.deepEqual(seen, ['systemd-run', '/bin/sh']);
    assert.equal(found.skipped[0], `scope: ${BUS}`);
});

test('without systemd-run, sh or a budget the server still starts', async () => {
    assert.deepEqual((await usableCaps(async () => errno('ENOENT'), 3072, 2, true)).caps, ['plain']);
    assert.deepEqual((await usableCaps(async () => ok, undefined, 2, true)).caps, ['plain']);
    assert.deepEqual((await usableCaps(async () => ok, 3072, 2, false)).caps, ['ulimit', 'plain']);
    assert.deepEqual((await usableCaps(async () => ok, 3072, 2, true)).caps, ['scope', 'ulimit', 'plain']);
});

test('each cap wraps the server command', () => {
    assert.deepEqual(wrap('plain', ['satex', 'lsp'], 3072, 2), ['satex', ['lsp']]);
    assert.deepEqual(wrap('scope', ['satex', 'lsp'], 3072, 0)[1],
        ['--user', '--scope', '--quiet', '-p', 'MemoryMax=3072M', '-p', 'MemorySwapMax=0', '--', 'satex', 'lsp']);
    assert.ok(wrap('scope', ['satex'], 3072, 2)[1].includes('CPUQuota=200%'));
});

/** The ulimit cap really holds: the wrapped command sees the limit and runs with its arguments. */
test('the ulimit cap sets the data limit and runs the command', { skip: !linux }, async () => {
    const [command, args] = wrap('ulimit', [script('limit', 'ulimit -d; echo "$@"'), 'a b', 'c'], 512, 2);
    const r = await run(command, args, 5_000);
    assert.equal(r.status, 0, r.stderr);
    const [command2, args2] = wrap('ulimit', ['/bin/sh', '-c', 'ulimit -d >&2'], 512, 2);
    assert.equal((await run(command2, args2, 5_000)).stderr?.trim(), String(512 * 1024));
});

test('run reports a missing command, a failure and a hang', { skip: !linux }, async () => {
    assert.equal((await run(join(dir, 'nothing'), [], 5_000)).error?.code, 'ENOENT');
    const failed = await run(script('fails', 'echo "it broke" >&2; exit 4'), [], 5_000);
    assert.deepEqual([failed.status, failed.stderr?.trim()], [4, 'it broke']);
    assert.equal((await run(script('hangs', 'exec sleep 30'), [], 200)).error?.code, 'ETIMEDOUT');
});

/**
 * The real start in a devcontainer, with stand-ins: the scope probe passes but the scope fails
 * for the server. The first start reports the scope as broken, the next one runs through ulimit.
 */
test('a scope that fails for the server is dropped for the next start', { skip: !linux }, async () => {
    script('systemd-run', `[ "$#" -gt 0 ] && eval "last=\\\${$#}" && [ "$last" = true ] && exit 0\necho "${BUS}" >&2; exit 1`);
    const server = script('server', 'echo "Content-Length: 2"; echo; echo "{}"');
    const path = process.env.PATH;
    process.env.PATH = `${dir}:${path}`;
    try {
        const r = recorder();
        const launcher = new Launcher(r.events);
        const limits = { mib: 3072, cores: 2, scope: true, linux: true };
        const first = await launcher.start(server, ['lsp'], limits);
        assert.equal(first.cap, 'scope');
        await r.next();
        assert.ok(r.seen.includes(`failed scope true ${BUS} (exit code 1)`), r.seen.join('\n'));
        // the client may restart at once, without its count of crashes
        assert.equal(launcher.takeDropped(), true);
        assert.equal(launcher.takeDropped(), false);
        assert.match(launcher.problem ?? '', /user scope bus/);
        const second = await launcher.start(server, ['lsp'], limits);
        assert.equal(second.cap, 'ulimit');
        assert.match(await stdout(second.child), /Content-Length: 2/);
        assert.equal(launcher.problem, undefined);
        assert.equal(launcher.takeDropped(), false);
        assert.ok(!r.seen.includes('uncapped'));
    } finally {
        process.env.PATH = path;
    }
});

test('a server that never answers or dies later is reported', { skip: !linux }, async () => {
    const r = recorder();
    const launcher = new Launcher(r.events, async () => errno('ENOENT'));
    const limits = { mib: 3072, cores: 2, scope: true, linux: true };
    // nothing caps here: said once, and the server runs plain
    const broken = await launcher.start(script('broken', 'echo "error: unexpected argument --threads" >&2; exit 2'), ['lsp'], limits);
    assert.equal(broken.cap, 'plain');
    await r.next();
    assert.ok(r.seen.includes('uncapped'));
    assert.ok(r.seen.includes('failed plain false error: unexpected argument --threads (exit code 2)'), r.seen.join('\n'));
    await launcher.start(join(dir, 'missing'), ['lsp'], limits);
    await r.next();
    assert.ok(r.seen.includes('failed plain false not installed'), r.seen.join('\n'));
    await launcher.start(script('crashes', 'echo hello; sleep 0.1; echo "memory allocation of 4096 bytes failed" >&2; kill -ABRT $$'), ['lsp'], limits);
    await r.next();
    assert.ok(r.seen.some(s => /^crashed it ran out of its memory cap/.test(s)), r.seen.join('\n'));
    // a stop asked for is no failure
    const stopped = await launcher.start(script('quiet', 'echo hello; exit 0'), ['lsp'], limits);
    await new Promise(resolve => stopped.child.on('exit', resolve));
    assert.equal(r.seen.filter(s => /^(failed|crashed)/.test(s)).length, 3);
    assert.equal(r.seen.filter(s => s === 'uncapped').length, 1);
});

/** Review finding pinned: an exit before the first answer is a failure even with code 0, and
 * a server the launcher replaces or stops is ended without a report. */
test('a silent clean exit fails, a replaced server is ended quietly', { skip: !linux }, async () => {
    const r = recorder();
    const launcher = new Launcher(r.events, async () => errno('ENOENT'));
    const limits = { mib: 3072, cores: 2, scope: true, linux: true };
    await launcher.start(script('mute', 'exit 0'), ['lsp'], limits);
    await r.next();
    assert.ok(r.seen.includes('failed plain false exit code 0'), r.seen.join('\n'));
    const first = await launcher.start(script('long', 'echo hello; exec sleep 30'), ['lsp'], limits);
    const gone = new Promise(resolve => first.child.on('exit', (_code, signal) => resolve(signal)));
    const second = await launcher.start(script('long2', 'echo hello; exec sleep 30'), ['lsp'], limits);
    assert.equal(await gone, 'SIGTERM');
    const end = new Promise(resolve => second.child.on('exit', resolve));
    launcher.stop();
    await end;
    assert.equal(r.seen.filter(s => /^(failed|crashed)/.test(s)).length, 1, r.seen.join('\n'));
});

test('the first binary that runs is used, the others say why not', { skip: !linux }, async () => {
    const good = script('good', 'echo satex 0.2.3');
    const old = script('old', "echo \"$0: /lib/libc.so.6: version \\`GLIBC_2.39' not found\" >&2; exit 1");
    const found = await findBinary([
        { binary: join(dir, 'gone'), from: '`satex.path`' },
        { binary: old, from: 'the bundled binary', ours: true },
        { binary: 'no-such-satex', from: '`satex` on the PATH' },
        { binary: good, from: 'a good one' },
    ], run);
    assert.equal(found.binary, good);
    assert.equal(found.problems.length, 2, found.problems.join('\n'));
    assert.match(found.problems[0], /`satex.path` names .*gone, which does not exist/);
    assert.match(found.problems[1], /^the bundled binary: .* needs a newer C library .*GLIBC_2\.39/);
    // a bundled binary that lost its executable bit gets it back
    const bare = script('bare', 'echo satex');
    chmodSync(bare, 0o644);
    assert.equal((await findBinary([{ binary: bare, from: 'bundled', ours: true }], run)).binary, bare);
    // a command name in the setting is looked up on the PATH
    assert.match((await findBinary([{ binary: 'no-such-satex', from: '`satex.path`' }], run)).problems[0], /not on the PATH/);
});

test('a binary that cannot run here says why and what to do', () => {
    assert.equal(binaryProblem('satex', ok), undefined);
    assert.match(binaryProblem('/x/satex', errno('ENOENT')) ?? '', /loader is missing.*cargo install/);
    assert.match(binaryProblem('/x/satex', errno('EACCES')) ?? '', /chmod \+x \/x\/satex/);
    assert.match(binaryProblem('/x/satex', { status: 2, stderr: 'boom\n' }) ?? '', /--version failed: boom/);
});

test('a server that died is explained', () => {
    assert.match(startProblem(null, 'SIGKILL', ''), /killed.*memory.*maxMemory/);
    assert.equal(startProblem(1, null, 'satex: x.yaml: plugins.packages ignored: only the user\'s own config sets it\nerror: bad flag\n'), 'error: bad flag (exit code 1)');
    assert.equal(startProblem(3, null, ''), 'exit code 3');
    assert.equal(startProblem(101, null, "thread 'main' panicked at src/x.rs:1:1:\nboom\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n"), 'boom (exit code 101)');
    assert.match(startProblem(null, 'SIGABRT', 'memory allocation of 4096 bytes failed\n'), /memory cap.*maxMemory/);
});

