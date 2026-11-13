// No `vscode` import: unit tests load it.

/**
 * The hard cap for a server whose own budget is `memory` (`2GiB`, `512M`): half again as much,
 * in MiB. The budget counts what the analysis holds, the cap the whole process, so with equal
 * caps the kernel killed the server before its budget could end the run cleanly.
 */
export function capMiB(memory: string): number | undefined {
    const m = /^\s*(\d+(?:\.\d+)?)\s*([KMGT]?)(?:i?B)?\s*$/i.exec(memory);
    if (!m) { return undefined; }
    const mib = Number(m[1]) * ({ '': 1 / 2 ** 20, K: 1 / 1024, M: 1, G: 1024, T: 2 ** 20 } as Record<string, number>)[m[2].toUpperCase()];
    return mib > 0 ? Math.ceil(mib * 1.5) : undefined;
}

