import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';

/** A type plugin's view as `satex/view` returns it (src/lsp/plugins); `media/views.js` renders it. */
interface View { plugin: string; title: string; sections: unknown[] }
interface Edit { range: { start: { line: number; character: number }; end: { line: number; character: number } }; newText: string }

const VERB: Record<string, string> = { color: 'Pick', units: 'Convert', environment: '' };

/** The views at a position, from the server. */
async function views(client: LanguageClient, doc: vscode.TextDocument, at: vscode.Position): Promise<View[]> {
    const params = { textDocument: { uri: doc.uri.toString() }, position: { line: at.line, character: at.character } };
    return (await client.sendRequest<View[]>('satex/view', params).catch(() => [])) ?? [];
}

/** A light-bulb entry per view at the cursor: VS Code's way into the dialogs. */
export class ViewActions implements vscode.CodeActionProvider {
    constructor(private readonly client: () => LanguageClient | undefined) {}
    async provideCodeActions(doc: vscode.TextDocument, range: vscode.Range): Promise<vscode.CodeAction[]> {
        const client = this.client();
        if (!client) { return []; }
        return (await views(client, doc, range.start)).map(view => {
            const a = new vscode.CodeAction(`${VERB[view.plugin] ?? 'Open'} ${view.title}…`.trim(), vscode.CodeActionKind.RefactorRewrite);
            a.command = { title: a.title, command: 'satex.openView', arguments: [doc.uri, range.start, view.plugin] };
            return a;
        });
    }
}

/** Opens the view of `plugin` (else the first) at `at` in a webview beside the editor. */
export async function openView(context: vscode.ExtensionContext, client: LanguageClient | undefined, uri?: vscode.Uri, at?: vscode.Position, plugin?: string): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    const doc = uri ? await vscode.workspace.openTextDocument(uri) : editor?.document;
    const pos = at ?? editor?.selection.active;
    if (!client || !doc || !pos) { return; }
    const all = await views(client, doc, pos);
    const view = all.find(v => v.plugin === plugin) ?? all[0];
    if (!view) { void vscode.window.showInformationMessage('satex: no view here'); return; }
    const media = vscode.Uri.joinPath(context.extensionUri, 'media');
    const panel = vscode.window.createWebviewPanel('satex.view', view.title, { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
        { enableScripts: true, localResourceRoots: [media] });
    const script = panel.webview.asWebviewUri(vscode.Uri.joinPath(media, 'views.js'));
    const nonce = Math.random().toString(36).slice(2);
    panel.webview.html = `<!DOCTYPE html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; script-src 'nonce-${nonce}' ${panel.webview.cspSource};">
<style>body { padding: 1rem; font-family: var(--vscode-font-family); } h3 { margin: 0 0 .8rem; }</style></head>
<body><h3></h3><div id="view"></div>
<script nonce="${nonce}" src="${script}"></script>
<script nonce="${nonce}">
const vscode = acquireVsCodeApi();
const view = ${JSON.stringify(view).replace(/</g, '\\u003c')};
document.querySelector('h3').textContent = view.title;
satexViews.render(view, document.getElementById('view'), { apply: (edits) => vscode.postMessage({ edits }) });
</script></body></html>`;
    panel.webview.onDidReceiveMessage(async (m: { edits: Edit[] }) => {
        const edit = new vscode.WorkspaceEdit();
        for (const e of m.edits) {
            edit.replace(doc.uri, new vscode.Range(e.range.start.line, e.range.start.character, e.range.end.line, e.range.end.character), e.newText);
        }
        await vscode.workspace.applyEdit(edit);
        panel.dispose();
    });
}
