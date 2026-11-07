// The table and image dialogs (`satex/dialog`) in a webview: the playground's component
// (doc/site/dialogs.js, copied to media/ by `npm run compile`), the server's state and text.
import * as vscode from 'vscode';
import { execFile } from 'child_process';
import * as path from 'path';
import { LanguageClient } from 'vscode-languageclient/node';
import { apply, Edits, page, panel } from './web';

type Pos = { line: number; character: number };
type Range = { start: Pos; end: Pos };
type Edit = { range: Range; newText: string };
type View = { kind: string; range: Range; state: unknown; context: { path?: string } };
type Message = { type: string; id?: number; state?: unknown; op?: string; edits?: Edit[]; file?: string; old?: string; name?: string; bytes?: number[] };

const RASTER = /\.(png|jpe?g|gif|webp)$/i;

/** A PDF's first page as a PNG in `dir`, by `pdftocairo` if installed. */
async function pdfPage(file: string, dir: vscode.Uri): Promise<vscode.Uri | null> {
    const out = path.join(dir.fsPath, `page-${Date.now()}`);
    await vscode.workspace.fs.createDirectory(dir);
    return new Promise((done) =>
        execFile('pdftocairo', ['-png', '-singlefile', '-f', '1', '-l', '1', '-r', '96', file, out], (err) =>
            done(err ? null : vscode.Uri.file(`${out}.png`))));
}

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
    const title = view.kind === 'table' ? 'Edit table' : view.kind === 'float' ? 'Edit placement' : `Edit image ${path.basename(file ?? '')}`;
    const p = panel(context, 'dialog', title, [context.globalStorageUri, ...(file ? [vscode.Uri.file(path.dirname(file))] : [])]);
    const web = p.webview;
    let image: { url: string | null; kind: string | null } = { url: null, kind: null };
    if (file && RASTER.test(file)) { image = { url: web.asWebviewUri(vscode.Uri.file(file)).toString(), kind: 'img' }; }
    else if (file && /\.pdf$/i.test(file)) {
        const png = await pdfPage(file, context.globalStorageUri);
        image = png ? { url: web.asWebviewUri(png).toString(), kind: 'img' } : { url: null, kind: 'pdf' };
    }
    web.html = page(context, web, ['lengths.js', 'dialogs.js'], `
const pending = new Map(); let next = 0;
addEventListener('message', (e) => { const m = e.data; if (m.type === 'open') self.satexDialogs.open(m.view, host(m.image)); else pending.get(m.id)?.(m); });
const call = (msg) => new Promise((done, fail) => { const id = ++next; pending.set(id, (m) => { pending.delete(id); m.error ? fail(new Error(m.error)) : done(m.result); }); vscode.postMessage({ ...msg, id }); });
const host = (image) => ({ parent: document.body, apply: (state, op) => call({ type: 'apply', state, op }), commit: (edits) => vscode.postMessage({ type: 'commit', edits }), image: async () => image,
  closed: () => vscode.postMessage({ type: 'closed' }), download: (file) => call({ type: 'download', file }), upload: (old, name, bytes) => call({ type: 'upload', old, name, bytes: [...bytes] }) });
vscode.postMessage({ type: 'ready' });`, 'html, body { height: 100%; margin: 0; padding: 0; }');
    const doc = vscode.Uri.parse(uri);
    // the host's calls: the dialog's text from the server, the figure saved elsewhere or replaced by an upload beside it
    const calls: Record<string, (m: Message) => Thenable<unknown>> = {
        apply: m => client.sendRequest('satex/dialogApply', { textDocument: { uri }, kind: view.kind, range: view.range, state: m.state, op: m.op }),
        download: async m => {
            const to = m.file && await vscode.window.showSaveDialog({ defaultUri: vscode.Uri.file(m.file) });
            if (to) { await vscode.workspace.fs.copy(vscode.Uri.file(m.file!), to, { overwrite: true }); }
            return { saved: true };
        },
        upload: async m => {
            // beside the figure when it is the project's, else (the installation's) beside the document
            const beside = Boolean(m.old && vscode.workspace.getWorkspaceFolder(vscode.Uri.file(m.old)));
            const at = vscode.Uri.file(path.join(path.dirname(beside ? m.old! : doc.fsPath), path.basename(m.name ?? '')));
            await vscode.workspace.fs.writeFile(at, Uint8Array.from(m.bytes ?? []));
            return { beside };
        },
    };
    web.onDidReceiveMessage(async (m: Message) => {
        if (m.type === 'ready') { void web.postMessage({ type: 'open', view, image }); }
        if (m.type === 'closed') { p.dispose(); }
        if (calls[m.type]) {
            try { void web.postMessage({ id: m.id, result: await calls[m.type](m) }); }
            catch (e) { void web.postMessage({ id: m.id, error: String((e as Error).message ?? e) }); }
        }
        if (m.type === 'commit' && m.edits && await apply(vscode.Uri.parse(uri), m.edits as Edits)) { p.dispose(); }
    });
}
