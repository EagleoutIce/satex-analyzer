// Other installed copies of SaTeX: two would start two servers, two status items, and the second
// one's commands would clash with the first one's. No `vscode` import: unit tests load it.

/** What `vscode.extensions.all` tells of an extension. */
export interface Installed { id: string; packageJSON: { name?: string; contributes?: { commands?: { command?: string }[] } } }

/** The published extension; any other copy is a stale or local build. */
export const CANONICAL = 'eagleoutice.satex';

/** The installed extensions, other than `self`, that are SaTeX: its name or a `satex.` command. */
export function peers(all: readonly Installed[], self: string): string[] {
    const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();
    return all
        .filter(e => !same(e.id, self))
        .filter(e => e.packageJSON.name === 'satex' || (e.packageJSON.contributes?.commands ?? []).some(c => c.command?.startsWith('satex.')))
        .map(e => e.id);
}

/** Whether `self` steps aside for `others`: only a copy that is not the published one does. */
export function yields(self: string, others: readonly string[]): boolean {
    return self.toLowerCase() !== CANONICAL && others.some(o => o.toLowerCase() === CANONICAL);
}

/** The shell command that removes `id`, for the warning. */
export const uninstallCommand = (id: string) => `code --uninstall-extension ${id}`;
