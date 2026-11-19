// `satex: Open Widget…`: every widget the playground offers, from one list. Those at the cursor
// (the type plugins' views, the table, image or placement dialog) first, then the managers,
// the Library and the Settings.
import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';
import { entry, MANAGERS, views } from './views';

interface Pick extends vscode.QuickPickItem { run: () => Thenable<unknown> }

const DIALOG: Record<string, string> = { table: 'Edit table…', image: 'Edit image…', float: 'Edit placement…' };

export async function widgets(client: LanguageClient | undefined): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    const run = (command: string, ...args: unknown[]) => () => vscode.commands.executeCommand(command, ...args);
    const project: Pick[] = [
        { label: '', kind: vscode.QuickPickItemKind.Separator, run: async () => undefined },
        ...MANAGERS.map(m => ({ label: `Manage ${m}`, run: run('satex.manage', m) })),
        { label: 'Library', description: 'the names in force', run: run('satex.library') },
        { label: 'Settings', description: 'satex.yaml', run: run('satex.projectSettings') },
    ];
    // the list shows at once; the widgets at the cursor join it when the server answers (a run may hold it)
    const qp = vscode.window.createQuickPick<Pick>();
    qp.items = project;
    qp.placeholder = 'Widgets at the cursor (asking the server…), then the project\'s';
    qp.busy = !!(client && editor);
    qp.show();
    if (client && editor) {
        const doc = editor.document, at = editor.selection.active, position = { line: at.line, character: at.character };
        Promise.all([
            views(client, doc, at).catch(() => []),
            client.sendRequest<{ kind: string } | null>('satex/dialog', { textDocument: { uri: doc.uri.toString() }, position }).catch(() => null),
        ]).then(([found, dialog]) => {
            const here: Pick[] = [];
            if (dialog) { here.push({ label: DIALOG[dialog.kind] ?? 'Edit…', description: 'at the cursor', run: run('satex.openDialog', { uri: doc.uri.toString(), position }) }); }
            here.push(...found.map(v => ({ label: entry(v), description: 'at the cursor', run: run('satex.openView', doc.uri, at, v.plugin) })));
            qp.items = [...here, ...project];
            qp.busy = false;
            qp.placeholder = here.length ? 'Widgets at the cursor, then the project\'s' : 'No widget at the cursor: the project\'s';
        });
    }
    const pick = await new Promise<Pick | undefined>(done => {
        qp.onDidAccept(() => { done(qp.selectedItems[0]); qp.hide(); });
        qp.onDidHide(() => done(undefined));
    });
    qp.dispose();
    await pick?.run();
}
