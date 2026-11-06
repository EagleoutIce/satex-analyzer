// A sliver of the `vscode` API: enough to build the widget pages and drive their messages in node.
/* eslint-disable @typescript-eslint/no-explicit-any */
export class Uri {
    constructor(readonly fsPath: string) {}
    get path() { return this.fsPath; }
    static file(p: string) { return new Uri(p); }
    static parse(s: string) { return new Uri(s.replace(/^file:\/\//, '')); }
    static joinPath(u: Uri, ...parts: string[]) { return new Uri([u.fsPath, ...parts].join('/')); }
    toString() { return `file://${this.fsPath}`; }
}
export class Position { constructor(readonly line: number, readonly character: number) {} }
export class Range { constructor(readonly startLine: number, readonly startCharacter: number, readonly endLine: number, readonly endCharacter: number) {} }
export enum ViewColumn { Beside = -2 }
/** What the test sees: pages, posts, edits, files. */
export const seen = { html: [] as string[], posts: [] as any[], edits: [] as [string, string][], files: new Map<string, string>(), handler: undefined as undefined | ((m: any) => Promise<void>) };
export class WorkspaceEdit {
    readonly list: [string, string][] = [];
    replace(uri: Uri, _r: unknown, text: string) { this.list.push([uri.fsPath, text]); }
    insert(uri: Uri, _p: unknown, text: string) { this.list.push([uri.fsPath, text]); }
    createFile() { /* the insert writes it */ }
}
const webview = { cspSource: 'csp', asWebviewUri: (u: Uri) => u, postMessage: async (m: unknown) => { seen.posts.push(m); return true; },
    onDidReceiveMessage: (h: (m: any) => Promise<void>) => { seen.handler = h; },
    set html(h: string) { seen.html.push(h); } };
const doc = { uri: Uri.file('/p/main.tex'), getText: () => '' };
export const window = {
    activeTextEditor: { document: doc, selection: new Range(0, 0, 0, 0) },
    visibleTextEditors: [] as unknown[],
    createWebviewPanel: () => ({ webview, dispose: () => undefined }),
    showInformationMessage: async () => undefined,
};
export const commands = { executeCommand: async (_c: string, md: string) => `<p>${md}</p>` };
export const workspace = {
    asRelativePath: (u: Uri) => u.fsPath.replace(/^\/p\//, ''),
    getWorkspaceFolder: (u: Uri) => (u.fsPath.startsWith('/p/') ? { uri: Uri.file('/p') } : undefined),
    workspaceFolders: [{ uri: Uri.file('/p') }],
    fs: {
        stat: async (u: Uri) => { if (!seen.files.has(u.fsPath)) { throw new Error('none'); } },
        // the extension's own files from media/, as `npm run compile` made them
        readFile: async (u: Uri) => Buffer.from(seen.files.get(u.fsPath) ?? (await import('fs')).readFileSync(u.fsPath.replace(/^.*\/media\//, 'media/'), 'utf8')),
    },
    openTextDocument: async (u: Uri) => {
        if (u.fsPath === doc.uri.fsPath) { return doc; }
        if (!seen.files.has(u.fsPath)) { throw new Error('none'); }
        return { getText: () => seen.files.get(u.fsPath), lineCount: 1, validateRange: (r: Range) => r, save: async () => true };
    },
    applyEdit: async (e: WorkspaceEdit) => { for (const [f, t] of e.list) { seen.edits.push([f, t]); seen.files.set(f, t); } return true; },
};
