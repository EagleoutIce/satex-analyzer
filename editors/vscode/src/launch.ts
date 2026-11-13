// How the server starts, and what to tell the user when it cannot.
// No `vscode` import: unit tests load it.

/** What a finished process left: `spawnSync`'s fields that matter here. */
export interface Ran { error?: NodeJS.ErrnoException; status: number | null; signal?: NodeJS.Signals | null; stderr?: string }
export type Run = (command: string, args: string[]) => Ran;

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
export function usableCaps(run: Run, mib: number | undefined, cores: number, scope: boolean): { caps: Cap[]; skipped: string[] } {
    const caps: Cap[] = [];
    const skipped: string[] = [];
    if (mib !== undefined) {
        for (const cap of (scope ? ['scope', 'ulimit'] : ['ulimit']) as Cap[]) {
            const [command, args] = wrap(cap, ['true'], mib, cores);
            const why = failure(run(command, args));
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
    return text?.split('\n').map(l => l.trim()).filter(l => l && !/^satex: .*ignored/.test(l)).pop();
}

/** Why `binary --version` (ran as `r`) shows the binary cannot serve here, with what to do. */
export function binaryProblem(binary: string, r: Ran): string | undefined {
    const glibc = /GLIBC_[\d.]+' not found/.exec(r.stderr ?? '');
    if (glibc) {
        return `${binary} needs a newer C library than this system has (${glibc[0]}). Build it here with \`cargo install satex-analyzer\` and set \`satex.path\`.`;
    }
    if (r.error) {
        switch (r.error.code) {
            case 'ENOENT':
                return `${binary} exists but cannot run here: its loader is missing (a musl system such as Alpine, or another architecture). Build it here with \`cargo install satex-analyzer\` and set \`satex.path\`.`;
            case 'EACCES':
                return `${binary} is not executable: run \`chmod +x ${binary}\`.`;
            case 'ENOEXEC':
                return `${binary} is built for another architecture than this ${process.arch} system. Download the matching release or build it with \`cargo install satex-analyzer\`.`;
            case 'ETIMEDOUT':
                return `${binary} did not answer \`--version\` within 10 seconds.`;
            default:
                return `${binary} could not start: ${r.error.message}.`;
        }
    }
    const why = failure(r);
    return why && `${binary} --version failed: ${why}.`;
}

/** Why a server that exited (`code`, `signal`) before it answered stopped, from its `stderr`. */
export function startProblem(code: number | null, signal: string | null, stderr: string): string {
    const glibc = /GLIBC_[\d.]+' not found/.exec(stderr);
    if (glibc) { return `the binary needs a newer C library than this system has (${glibc[0]}). Build it here with \`cargo install satex-analyzer\` and set \`satex.path\`.`; }
    if (/memory allocation of \d+ bytes failed/.test(stderr)) { return 'it ran out of its memory cap. Raise `satex.server.maxMemory`, or set `limits.memory` in satex.yaml lower so an analysis stops before.'; }
    if (signal === 'SIGKILL') { return 'it was killed, most likely for memory. Lower `satex.server.maxMemory` or close other programs.'; }
    const line = lastLine(stderr);
    const how = signal ? `killed by ${signal}` : `exit code ${code}`;
    return line ? `${line} (${how})` : how;
}
