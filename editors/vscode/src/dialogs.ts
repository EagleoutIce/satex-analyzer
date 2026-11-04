// The table and image dialogs (`satex/dialog`) in a webview: the playground's component
// (doc/site/dialogs.js, copied to media/ by scripts/media.mjs), the server's state and text.
import * as vscode from 'vscode';
import { execFile } from 'child_process';
import * as path from 'path';
import { LanguageClient } from 'vscode-languageclient/node';

type Pos = { line: number; character: number };
type Range = { start: Pos; end: Pos };
type Edit = { range: Range; newText: string };
type View = { kind: string; range: Range; state: unknown; context: { path?: string } };

const RASTER = /\.(png|jpe?g|gif|webp)$/i;

/** A PDF's first page as a PNG in `dir`, by `pdftocairo` if installed. */
async function pdfPage(file: string, dir: vscode.Uri): Promise<vscode.Uri | null> {
    const out = path.join(dir.fsPath, `page-${Date.now()}`);
    await vscode.workspace.fs.createDirectory(dir);
    return new Promise((done) =>
        execFile('pdftocairo', ['-png', '-singlefile', '-f', '1', '-l', '1', '-r', '96', file, out], (err) =>
            done(err ? null : vscode.Uri.file(`${out}.png`))));
}

const toRange = (r: Range) => new vscode.Range(r.start.line, r.start.character, r.end.line, r.end.character);

/** `satex.openDialog {uri, position}`: the dialog of the table or figure there. */
export async function openDialog(context: vscode.ExtensionContext, client: LanguageClient | undefined, arg?: { uri: string; position: Pos }): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    const uri = arg?.uri ?? editor?.document.uri.toString();
    const position = arg?.position ?? (editor ? { line: editor.selection.active.line, character: editor.selection.active.character } : undefined);
    if (!client || !uri || !position) { return; }
    const view = await client.sendRequest<View | null>('satex/dialog', { textDocument: { uri }, position });
    if (!view) {
        void vscode.window.showInformationMessage('No table or figure here that the analysis met.');
        return;
    }
    const file = view.context.path;
    const roots = [vscode.Uri.joinPath(context.extensionUri, 'media'), context.globalStorageUri, ...(file ? [vscode.Uri.file(path.dirname(file))] : [])];
    const panel = vscode.window.createWebviewPanel('satex.dialog', view.kind === 'table' ? 'Edit table' : `Edit image ${path.basename(file ?? '')}`,
        vscode.ViewColumn.Beside, { enableScripts: true, localResourceRoots: roots });
    const web = panel.webview;
    let image: { url: string | null; kind: string | null } = { url: null, kind: null };
    if (file && RASTER.test(file)) { image = { url: web.asWebviewUri(vscode.Uri.file(file)).toString(), kind: 'img' }; }
    else if (file && /\.pdf$/i.test(file)) {
        const png = await pdfPage(file, context.globalStorageUri);
        image = png ? { url: web.asWebviewUri(png).toString(), kind: 'img' } : { url: null, kind: 'pdf' };
    }
    const nonce = Math.random().toString(36).slice(2);
    const script = web.asWebviewUri(vscode.Uri.joinPath(context.extensionUri, 'media', 'dialogs.js'));
    web.html = `<!DOCTYPE html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${web.cspSource} data: blob:; style-src ${web.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}' ${web.cspSource};">
<style>html, body { height: 100%; margin: 0; } body { background: var(--vscode-editor-background); }</style>
</head><body><script nonce="${nonce}" src="${script}"></script><script nonce="${nonce}">
const vscode = acquireVsCodeApi(), pending = new Map(); let next = 0;
addEventListener('message', (e) => { const m = e.data; if (m.type === 'open') self.satexDialogs.open(m.view, host(m.image)); else pending.get(m.id)?.(m); });
const call = (msg) => new Promise((done, fail) => { const id = ++next; pending.set(id, (m) => { pending.delete(id); m.error ? fail(new Error(m.error)) : done(m.result); }); vscode.postMessage({ ...msg, id }); });
const host = (image) => ({ parent: document.body, apply: (state, op) => call({ type: 'apply', state, op }), commit: (edits) => vscode.postMessage({ type: 'commit', edits }), image: async () => image });
vscode.postMessage({ type: 'ready' });
</script></body></html>`;
    web.onDidReceiveMessage(async (m: { type: string; id?: number; state?: unknown; op?: string; edits?: Edit[] }) => {
        if (m.type === 'ready') { void web.postMessage({ type: 'open', view, image }); }
        if (m.type === 'apply') {
            try {
                const result = await client.sendRequest('satex/dialogApply', { textDocument: { uri }, kind: view.kind, range: view.range, state: m.state, op: m.op });
                void web.postMessage({ id: m.id, result });
            } catch (e) { void web.postMessage({ id: m.id, error: String((e as Error).message ?? e) }); }
        }
        if (m.type === 'commit' && m.edits) {
            const edit = new vscode.WorkspaceEdit();
            for (const e of m.edits) { edit.replace(vscode.Uri.parse(uri), toRange(e.range), e.newText); }
            if (await vscode.workspace.applyEdit(edit)) { panel.dispose(); }
        }
    });
}
