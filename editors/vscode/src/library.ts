// The Library: every name in force at the end of the document (`satex/query` scope) and what the
// loaded packages' manuals document, searchable; a click inserts its usage at the cursor.
import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';
import { page, panel } from './web';

/** A name as the scope and docs queries give it. */
interface Name { name: string; tag?: string; package?: string; effective?: string; does?: string }

async function query(client: LanguageClient, file: string, q: object): Promise<Record<string, unknown>[]> {
    const r = await client.sendRequest<{ records?: Record<string, unknown>[] }[] | null>('workspace/executeCommand',
        { command: 'satex/query', arguments: [{ file, queries: [q] }] }).catch(() => null);
    return r?.[0]?.records ?? [];
}

/** The library's rows: environments once (their `\endX` goes), manual entries nothing in force defines. */
async function names(client: LanguageClient, file: string, all: boolean): Promise<Name[]> {
    const scope = (await query(client, file, { type: 'scope', all })) as unknown as Name[];
    const have = new Set(scope.map(r => r.name));
    const rows = scope.filter(r => !(r.name.startsWith('\\end') && r.name !== '\\end' && have.has(`\\${r.name.slice(4)}`)))
        .map(r => (have.has(`\\end${r.name.slice(1)}`) && r.tag !== 'primitive' ? { ...r, tag: 'environment' } : r));
    for (const d of await query(client, file, { type: 'docs' })) {
        const n = `\\${String(d.name)}`;
        if (/^[A-Za-z@]+$/.test(String(d.name)) && !have.has(n) && (all || d.audience !== 'code')) {
            rows.push({ name: n, tag: 'documented', package: String(d.package_file ?? '').split('/').pop()?.replace(/\.(sty|cls)$/, ''), effective: String(d.usage ?? n), does: String(d.text ?? '') });
        }
    }
    return rows;
}

/** What inserting a row writes: an environment as its block, else its usage without the ⟨⟩ marks. */
export function usage(r: Name): string {
    const use = (r.effective ?? r.name).replace(/[⟨⟩]/g, '');
    if (r.tag !== 'environment') { return use; }
    const env = r.name.slice(1);
    return `\\begin{${env}}${use.startsWith(r.name) ? use.slice(r.name.length) : ''}\n  \n\\end{${env}}`;
}

export async function library(context: vscode.ExtensionContext, client: LanguageClient | undefined): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!client || !editor) { return; }
    const doc = editor.document;
    const p = panel(context, 'library', `Library: ${vscode.workspace.asRelativePath(doc.uri)}`);
    const send = async (all: boolean) => p.webview.postMessage({ rows: (await names(client, doc.uri.fsPath, all)).map(r => ({ ...r, insert: usage(r) })) });
    p.webview.html = page(context, p.webview, [], `
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => '&#' + c.charCodeAt(0) + ';');
document.body.innerHTML = '<div class="bar"><input id="q" placeholder="search names, usages, descriptions" autofocus><label><input id="all" type="checkbox"> internals</label><span id="n"></span></div><ul id="list"><li class="muted">Reading the names in force…</li></ul>';
let rows = [];
const kind = (r) => r.tag === 'environment' ? 'environment' : r.tag === 'primitive' ? 'primitive' : 'command';
const show = () => {
  const q = document.getElementById('q').value.trim().toLowerCase().replace(/^\\\\/, '');
  const rank = (r) => { const n = r.name.toLowerCase().slice(1); return !q || n.startsWith(q) ? 0 : n.includes(q) ? 1 : ((r.effective ?? '') + ' ' + (r.does ?? '')).toLowerCase().includes(q) ? 2 : null; };
  const hits = rows.map((r) => [r, rank(r)]).filter(([, k]) => k !== null).sort((a, b) => a[1] - b[1] || a[0].name.length - b[0].name.length).map(([r]) => r);
  document.getElementById('n').textContent = hits.length + ' of ' + rows.length;
  document.getElementById('list').innerHTML = hits.slice(0, 300).map((r, i) => '<li><button data-i="' + i + '" title="insert at the cursor"><code>' + esc(r.effective ?? r.name) + '</code></button>'
    + '<span class="kind">' + kind(r) + '</span><span class="pkg">' + esc(!r.package || r.package.endsWith('.ltx') ? 'kernel' : r.package) + '</span><div class="does">' + esc(r.does) + '</div></li>').join('')
    || '<li class="muted">No name matches.</li>';
  window.hits = hits;
};
addEventListener('message', (e) => { rows = e.data.rows; show(); });
document.getElementById('q').oninput = show;
document.getElementById('all').onchange = (e) => vscode.postMessage({ all: e.target.checked });
document.getElementById('list').onclick = (e) => { const b = e.target.closest('button[data-i]'); if (b) vscode.postMessage({ insert: window.hits[+b.dataset.i].insert }); };
vscode.postMessage({ all: false });`,
    `.bar { display: flex; gap: .6rem; align-items: center; position: sticky; top: 0; background: var(--vscode-editor-background); padding-bottom: .5rem; }
#q { flex: 1; background: var(--vscode-input-background); color: var(--vscode-input-foreground); border: 1px solid var(--vscode-input-border, transparent); padding: .3rem .5rem; }
ul { list-style: none; padding: 0; margin: 0; } li { padding: .3rem 0; border-bottom: 1px solid var(--vscode-widget-border, transparent); }
li button { background: none; border: 0; color: var(--vscode-textLink-foreground); cursor: pointer; padding: 0; font: inherit; text-align: left; }
.kind, .pkg, #n, .muted { color: var(--vscode-descriptionForeground); margin-left: .6rem; font-size: .85em; } .does { font-size: .9em; margin-top: .15rem; }`);
    p.webview.onDidReceiveMessage(async (m: { all?: boolean; insert?: string }) => {
        if (m.all !== undefined) { await send(m.all); }
        if (m.insert === undefined) { return; }
        // the editor the library was opened from, where its cursor is now
        const at = vscode.window.visibleTextEditors.find(e => e.document === doc)?.selection ?? editor.selection;
        const edit = new vscode.WorkspaceEdit();
        edit.replace(doc.uri, at, m.insert);
        await vscode.workspace.applyEdit(edit);
    });
}
