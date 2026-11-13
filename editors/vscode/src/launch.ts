// How the server starts, and what to tell the user when it cannot.
// No `vscode` import: unit tests load it.
import { ChildProcess, spawn } from 'child_process';
import { chmodSync, existsSync } from 'fs';

/** What a finished process left. */
export interface Ran { error?: NodeJS.ErrnoException; status: number | null; signal?: NodeJS.Signals | null; stderr?: string }
export type Run = (command: string, args: string[], ms: number) => Promise<Ran>;

/** `command args`, killed after `ms`; never blocks the extension host. */
export function run(command: string, args: string[], ms: number): Promise<Ran> {
    return new Promise(resolve => {
        let stderr = '';
        let timedOut = false;
        const child = spawn(command, args, { stdio: ['ignore', 'ignore', 'pipe'] });
        const timer = setTimeout(() => { timedOut = true; child.kill('SIGKILL'); }, ms);
        child.stderr?.on('data', (d: Buffer) => { stderr = (stderr + d.toString()).slice(-4096); });
        child.on('error', error => { clearTimeout(timer); resolve({ error, status: null, stderr }); });
        child.on('close', (status, signal) => {
            clearTimeout(timer);
            resolve(timedOut ? { error: Object.assign(new Error(`no answer in ${ms / 1000} s`), { code: 'ETIMEDOUT' }), status, signal, stderr }
                : { status, signal, stderr });
        });
    });
}

/**
 * Ways to cap the server, strictest first. `scope`: a systemd user scope (memory without swap,
 * cores). `ulimit`: the data segment, for hosts without a user bus (containers, WSL). `plain`:
 * only the server's own budget and niceness.
 */
export type Cap = 'scope' | 'ulimit' | 'plain';

/** `target` (a command and its arguments) under `cap`, at most `mib` MiB and `cores` cores. */
export function wrap(cap: Cap, target: string[], mib: number, cores: number): [string, string[]] {
    switch (cap) {
        case 'scope':
            return ['systemd-run', ['--user', '--scope', '--quiet', '-p', `MemoryMax=${mib}M`, '-p', 'MemorySwapMax=0',
                ...(cores > 0 ? ['-p', `CPUQuota=${cores * 100}%`] : []), '--', ...target]];
        case 'ulimit':
            return ['/bin/sh', ['-c', 'ulimit -d "$1" && shift && exec "$@"', 'satex-cap', String(mib * 1024), ...target]];
        case 'plain':
            return [target[0], target.slice(1)];
    }
}

/** The caps that work here, strictest first, and why the others do not. */
export async function usableCaps(run: Run, mib: number | undefined, cores: number, scope: boolean): Promise<{ caps: Cap[]; skipped: string[] }> {
    const caps: Cap[] = [];
    const skipped: string[] = [];
    if (mib !== undefined) {
        for (const cap of (scope ? ['scope', 'ulimit'] : ['ulimit']) as Cap[]) {
            const [command, args] = wrap(cap, ['true'], mib, cores);
            const why = failure(await run(command, args, 5_000));
            if (why) { skipped.push(`${cap}: ${why}`); } else { caps.push(cap); }
        }
    }
    caps.push('plain');
    return { caps, skipped };
}

/** Why a process did not succeed, or `undefined` when it did. */
export function failure(r: Ran): string | undefined {
    if (r.error) { return r.error.code === 'ENOENT' ? 'not installed' : r.error.message; }
    if (r.status === 0) { return undefined; }
    return lastLine(r.stderr) ?? (r.signal ? `killed by ${r.signal}` : `exit code ${r.status}`);
}

function lastLine(text: string | undefined): string | undefined {
    return text?.split('\n').map(l => l.trim()).filter(l => l && !/^satex: .*ignored|^note: run with `RUST_BACKTRACE/.test(l)).pop();
}

const BUILD = 'Build it here with `cargo install satex-analyzer` and set `satex.path`.';

/** Why `binary --version` (ran as `r`) shows the binary cannot serve here, with what to do. */
export function binaryProblem(binary: string, r: Ran): string | undefined {
    const glibc = /GLIBC_[\d.]+' not found/.exec(r.stderr ?? '');
    if (glibc) { return `${binary} needs a newer C library than this system has (${glibc[0]}). ${BUILD}`; }
    if (r.error) {
        switch (r.error.code) {
            case 'ENOENT':
                return `${binary} exists but cannot run here: its loader is missing (a musl system such as Alpine, or another architecture). ${BUILD}`;
            case 'EACCES':
                return `${binary} is not executable: run \`chmod +x ${binary}\`.`;
            case 'ENOEXEC':
                return `${binary} is built for another architecture than this ${process.arch} system. ${BUILD}`;
            case 'ETIMEDOUT':
                return `${binary} --version gave no answer in 10 s.`;
            default:
                return `${binary} could not start: ${r.error.message}.`;
        }
    }
    const why = failure(r);
    return why && `${binary} --version failed: ${why}.`;
}

/** A binary to try, and how to name where it came from. */
export interface Candidate { binary: string; from: string; ours?: boolean }

/**
 * The first candidate whose `--version` runs, and the problems of those before it. A command
 * name (`satex`) is looked up on the PATH; one not found there is no problem, only skipped.
 */
export async function findBinary(candidates: Candidate[], run: Run): Promise<{ binary?: string; from?: string; problems: string[] }> {
    const problems: string[] = [];
    for (const { binary, from, ours } of candidates) {
        const command = !/[/\\]/.test(binary);
        if (!command && !existsSync(binary)) { problems.push(`${from} names ${binary}, which does not exist.`); continue; }
        let r = await run(binary, ['--version'], 10_000);
        // a package that lost the executable bit: ours to restore
        if (ours && r.error?.code === 'EACCES') {
            try { chmodSync(binary, 0o755); r = await run(binary, ['--version'], 10_000); } catch { /* reported below */ }
        }
        if (command && r.error?.code === 'ENOENT') {
            if (!from.includes('PATH')) { problems.push(`${from} names ${binary}, which is not on the PATH.`); }
            continue;
        }
        const problem = binaryProblem(binary, r);
        if (!problem) { return { binary, from, problems }; }
        problems.push(`${from}: ${problem}`);
    }
    return { problems };
}

/** Why a server that exited (`code`, `signal`) stopped, from its `stderr`. */
export function startProblem(code: number | null, signal: string | null, stderr: string): string {
    const glibc = /GLIBC_[\d.]+' not found/.exec(stderr);
    if (glibc) { return `the binary needs a newer C library than this system has (${glibc[0]}). ${BUILD}`; }
    if (/memory allocation of \d+ bytes failed/.test(stderr)) {
        return 'it ran out of its memory cap. Raise `satex.server.maxMemory`, or lower `limits.memory` in satex.yaml so an analysis stops before.';
    }
    if (signal === 'SIGKILL') { return 'it was killed, most likely for memory. Lower `satex.server.maxMemory` or close other programs.'; }
    const line = lastLine(stderr);
    const how = signal ? `killed by ${signal}` : `exit code ${code}`;
    return line ? `${line} (${how})` : how;
}

/** What the launcher reports. */
export interface Events {
    log(line: string): void;
    /** Linux, a budget, and still no hard cap works here. */
    uncapped(): void;
    /** The server stopped before it answered; with `fallback` the next start drops `cap`. */
    failedStart(problem: string, cap: Cap, fallback: boolean): void;
    /** The server stopped after it ran. */
    crashed(problem: string): void;
}

export interface Limits { mib?: number; cores: number; scope: boolean; linux: boolean }

/**
 * Starts the server under the strictest cap that works. The caps are probed once; one that
 * fails anyway (the server never answered through it) is dropped for the next start.
 */
export class Launcher {
    private caps: Promise<Cap[]> | undefined;
    private readonly broken = new Set<Cap>();
    private dropped = false;
    private current: ChildProcess | undefined;
    private readonly stopped = new WeakSet<ChildProcess>();
    /** Why the last server stopped unasked, until one answers. */
    problem: string | undefined;

    constructor(private readonly events: Events, private readonly runner: Run = run) { }

    /** Probe again and forget failed caps: a start by hand. */
    reset(): void { this.caps = undefined; this.broken.clear(); this.dropped = false; }

    /** Ends the running server, if any: the client cannot kill a process it only has streams of. */
    stop(): void {
        if (this.current && this.current.exitCode === null && this.current.signalCode === null) {
            this.stopped.add(this.current);
            this.current.kill();
        }
        this.current = undefined;
    }

    /** Whether a cap was dropped since the last call: the next start can follow at once. */
    takeDropped(): boolean {
        const dropped = this.dropped;
        this.dropped = false;
        return dropped;
    }

    private probe(limits: Limits): Promise<Cap[]> {
        return this.caps ??= (async () => {
            if (!limits.linux) { return ['plain']; }
            const found = await usableCaps(this.runner, limits.mib, limits.cores, limits.scope);
            found.skipped.forEach(s => this.events.log(`satex: no ${s}`));
            if (limits.mib !== undefined && found.caps[0] === 'plain') { this.events.uncapped(); }
            return found.caps;
        })();
    }

    async start(binary: string, args: string[], limits: Limits): Promise<{ child: ChildProcess; cap: Cap }> {
        const cap = (await this.probe(limits)).find(c => !this.broken.has(c)) ?? 'plain';
        const [command, argv] = wrap(cap, [binary, ...args], limits.mib ?? 0, limits.cores);
        this.events.log(`satex: starting ${binary} ${args.join(' ')} (${cap === 'plain' ? 'no hard cap' : `capped through ${cap}`})`);
        this.stop();
        const child = spawn(command, argv);
        this.current = child;
        let answered = false;
        let stderr = '';
        // The client notices a dead server by its streams, maybe before `exit`: drop the cap at once.
        const silent = () => {
            if (!answered && cap !== 'plain' && !this.broken.has(cap)) { this.broken.add(cap); this.dropped = true; }
        };
        child.stdout?.once('data', () => { answered = true; this.problem = undefined; });
        child.stdout?.once('end', silent);
        child.stdin?.on('error', silent);
        child.stderr?.on('data', (data: Buffer) => {
            stderr = (stderr + data.toString()).slice(-4096);
            this.events.log(data.toString().trimEnd());
        });
        let ended = false;
        const end = (code: number | null, signal: NodeJS.Signals | null, error?: Error) => {
            if (ended) { return; }
            ended = true;
            if (this.current === child) { this.current = undefined; }
            // ours to stop: no failure; after it answered, a clean exit or SIGTERM is a stop asked for
            if (this.stopped.has(child) || (answered && !error && (signal === 'SIGTERM' || (code === 0 && !signal)))) { return; }
            const problem = error ? (failure({ error, status: null }) ?? String(error)) : startProblem(code, signal, stderr);
            this.problem = problem;
            if (answered) { this.events.crashed(problem); return; }
            silent();
            this.events.failedStart(problem, cap, cap !== 'plain');
        };
        child.on('error', error => end(null, null, error));
        child.on('exit', (code, signal) => end(code, signal));
        return { child, cap };
    }
}
