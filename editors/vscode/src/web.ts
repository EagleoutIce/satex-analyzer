// One webview page for every widget: the shared scripts of media/ (copied from doc/site by
// `npm run compile`), a nonce'd inline script, VS Code's theme through its CSS variables.
import * as vscode from 'vscode';

/** A panel beside the editor; `roots` adds folders its images come from. */
export function panel(context: vscode.ExtensionContext, kind: string, title: string, roots: vscode.Uri[] = []): vscode.WebviewPanel {
    return vscode.window.createWebviewPanel(`satex.${kind}`, title, { viewColumn: vscode.ViewColumn.Beside, preserveFocus: false },
        { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, 'media'), ...roots] });
}

/** The page: `scripts` of media/ first, then `inline`, which has `vscode` (the webview API) in scope. */
export function page(context: vscode.ExtensionContext, web: vscode.Webview, scripts: string[], inline: string, style = ''): string {
    const nonce = Math.random().toString(36).slice(2);
    const src = scripts.map(s => `<script nonce="${nonce}" src="${web.asWebviewUri(vscode.Uri.joinPath(context.extensionUri, 'media', s))}"></script>`).join('');
    return `<!DOCTYPE html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${web.cspSource} data: blob:; style-src ${web.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}' ${web.cspSource};">
<style>body { padding: 1rem; font-family: var(--vscode-font-family); color: var(--vscode-foreground); background: var(--vscode-editor-background); } ${style}</style>
</head><body>${src}<script nonce="${nonce}">const vscode = acquireVsCodeApi();\n${inline}</script></body></html>`;
}

/** Edits as a view or dialog sends them: TextEdits on `uri`, or a WorkspaceEdit's `{changes}`. */
export type Edits = { range: vscode.Range; newText: string }[] | { changes: Record<string, { range: vscode.Range; newText: string }[]> };

/** Applies `edits`; true once written. */
export async function apply(uri: vscode.Uri, edits: Edits): Promise<boolean> {
    const edit = new vscode.WorkspaceEdit();
    const changes = Array.isArray(edits) ? { [uri.toString()]: edits } : edits.changes;
    for (const [file, list] of Object.entries(changes)) {
        for (const e of list) {
            edit.replace(vscode.Uri.parse(file), new vscode.Range(e.range.start.line, e.range.start.character, e.range.end.line, e.range.end.character), e.newText);
        }
    }
    return vscode.workspace.applyEdit(edit);
}

/** JSON for an inline script: no `</script>` can end it. */
export const inlineJson = (value: unknown) => JSON.stringify(value).replace(/</g, '\\u003c');
