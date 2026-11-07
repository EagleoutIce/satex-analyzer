import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';
import { apply, Edits, inlineJson, page, panel } from './web';

/** A type plugin's view as `satex/view` returns it (src/lsp/plugins); `media/views.js` renders it. */
export interface View { plugin: string; title: string; sections: unknown[] }

const VERB: Record<string, string> = { color: 'Pick', units: 'Convert', environment: '', tikz: '', forest: '' };

/** The views at a position, from the server; without a position, `plugin`'s manager. */
export async function views(client: LanguageClient, doc: vscode.TextDocument, at?: vscode.Position, plugin?: string): Promise<View[]> {
    const params = at ? { textDocument: { uri: doc.uri.toString() }, position: { line: at.line, character: at.character } } : { textDocument: { uri: doc.uri.toString() }, plugin };
    return (await client.sendRequest<View[]>('satex/view', params).catch(() => [])) ?? [];
}

/** A view's entry title: "Pick color…". */
export const entry = (view: View) => `${VERB[view.plugin] ?? 'Open'} ${view.title}…`.trim();

/** A light-bulb entry per view at the cursor: VS Code's way into the dialogs. */
export class ViewActions implements vscode.CodeActionProvider {
    constructor(private readonly client: () => LanguageClient | undefined) {}
    async provideCodeActions(doc: vscode.TextDocument, range: vscode.Range): Promise<vscode.CodeAction[]> {
        const client = this.client();
        if (!client) { return []; }
        return (await views(client, doc, range.start)).map(view => {
            const a = new vscode.CodeAction(entry(view), vscode.CodeActionKind.RefactorRewrite);
            a.command = { title: a.title, command: 'satex.openView', arguments: [doc.uri, range.start, view.plugin] };
            return a;
        });
    }
}

/** Every `markdown` string of a view, rendered by VS Code's own Markdown (`ctx.markdown` is synchronous). */
async function rendered(view: unknown): Promise<Record<string, string>> {
    const all = new Set<string>();
    const walk = (v: unknown): void => {
        if (Array.isArray(v)) { v.forEach(walk); } else if (v && typeof v === 'object') {
            for (const [k, x] of Object.entries(v)) { if (k === 'markdown' && typeof x === 'string') { all.add(x); } else { walk(x); } }
        }
    };
    walk(view);
    const out: Record<string, string> = {};
    for (const md of all) { out[md] = await vscode.commands.executeCommand<string>('markdown.api.render', md).then(h => h, () => ''); }
    return out;
}

/** Opens the view of `plugin` (else the first) at `at` in a webview beside the editor. */
export async function openView(context: vscode.ExtensionContext, client: LanguageClient | undefined, uri?: vscode.Uri | string, at?: vscode.Position | { line: number; character: number }, plugin?: string, manage = false): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    // a code lens from the server passes its arguments as JSON
    const doc = uri ? await vscode.workspace.openTextDocument(typeof uri === 'string' ? vscode.Uri.parse(uri) : uri) : editor?.document;
    const pos = manage ? undefined : at ? new vscode.Position(at.line, at.character) : editor?.selection.active;
    if (!client || !doc || (!pos && !manage)) { return; }
    const all = await views(client, doc, pos, plugin);
    // a command's lens has no view of its own at the cursor: the plugin's manager
    const view = all.find(v => v.plugin === plugin) ?? (plugin ? (await views(client, doc, undefined, plugin))[0] : all[0]);
    if (!view) { void vscode.window.showInformationMessage('satex: no view here'); return; }
    const p = panel(context, 'view', view.title);
    p.webview.html = page(context, p.webview, ['lengths.js', 'views.js'], `
const view = ${inlineJson(view)}, md = ${inlineJson(await rendered(view))};
const title = Object.assign(document.createElement('h3'), { textContent: view.title }), box = document.createElement('div');
document.body.append(title, box);
satexViews.render(view, box, { apply: (edits) => vscode.postMessage({ edits }), markdown: (s) => md[s] || s.replace(/[&<>]/g, (c) => '&#' + c.charCodeAt(0) + ';') });`,
    'h3 { margin: 0 0 .8rem; } .tv { max-width: 60rem; }');
    p.webview.onDidReceiveMessage(async (m: { edits: Edits }) => { if (await apply(doc.uri, m.edits)) { p.dispose(); } });
}

/** The managers (`satex/view` without a position): glossary, bibliography, index, theorems. */
export const MANAGERS = ['glossary', 'bibliography', 'index', 'theorem'];

export async function manage(context: vscode.ExtensionContext, client: LanguageClient | undefined, pick?: string): Promise<void> {
    pick ??= await vscode.window.showQuickPick(MANAGERS, { placeHolder: 'Manage' });
    if (pick) { await openView(context, client, undefined, undefined, pick, true); }
}
