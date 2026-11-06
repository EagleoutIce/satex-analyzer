// The project's settings (satex.yaml) as the playground's Settings show them: every key of
// satex.schema.json (copied to media/ by `npm run compile`) with its description, default and
// the value the file sets; an edit writes the key in place.
import * as vscode from 'vscode';
import * as path from 'path';
import { inlineJson, page, panel } from './web';
import { read, write, yamlValue } from './yaml';

interface Schema { type?: string | string[]; description?: string; default?: unknown; enum?: unknown[]; properties?: Record<string, Schema>; 'x-example'?: string }
interface Key { key: string; type: string; about: string; example?: string; default?: unknown; choices?: unknown[]; value?: string }

/** The schema's leaves as dotted keys. */
function leaves(props: Record<string, Schema>, prefix = ''): Key[] {
    return Object.entries(props).flatMap(([k, s]) => s.properties ? leaves(s.properties, `${prefix}${k}.`)
        : [{ key: prefix + k, type: [s.type].flat().filter(t => t !== 'null')[0] ?? 'string', about: s.description ?? '', example: s['x-example'], default: s.default, choices: s.enum }]);
}

/** The satex.yaml governing `doc`: the nearest one up to its workspace folder, else a new one there. */
async function yamlOf(doc: vscode.Uri): Promise<vscode.Uri> {
    const top = vscode.workspace.getWorkspaceFolder(doc)?.uri.fsPath;
    for (let dir = path.dirname(doc.fsPath); ; dir = path.dirname(dir)) {
        const at = vscode.Uri.file(path.join(dir, 'satex.yaml'));
        if (await vscode.workspace.fs.stat(at).then(() => true, () => false)) { return at; }
        if (!top || dir === top || dir === path.dirname(dir)) { return vscode.Uri.file(path.join(top ?? path.dirname(doc.fsPath), 'satex.yaml')); }
    }
}

export async function settings(context: vscode.ExtensionContext): Promise<void> {
    const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
    const doc = vscode.window.activeTextEditor?.document.uri ?? (folder && vscode.Uri.joinPath(folder, 'satex.yaml'));
    if (!doc) { return; }
    const file = await yamlOf(doc);
    const schema = JSON.parse(Buffer.from(await vscode.workspace.fs.readFile(vscode.Uri.joinPath(context.extensionUri, 'media', 'satex.schema.json'))).toString()) as Schema;
    const keys = leaves(schema.properties ?? {});
    const text = async () => vscode.workspace.openTextDocument(file).then(d => d.getText(), () => '');
    const p = panel(context, 'settings', `Settings: ${vscode.workspace.asRelativePath(file)}`);
    const send = async () => { const t = await text(); return p.webview.postMessage({ keys: keys.map(k => ({ ...k, value: read(t, k.key) })) }); };
    p.webview.html = page(context, p.webview, [], `
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => '&#' + c.charCodeAt(0) + ';');
const file = ${inlineJson(vscode.workspace.asRelativePath(file))};
document.body.innerHTML = '<div class="bar"><input id="q" placeholder="search settings" autofocus><label><input id="set" type="checkbox"> set in ' + esc(file) + '</label><span id="n"></span></div><ul id="list"></ul>';
let keys = [];
const field = (k, i) => k.type === 'boolean' ? '<input type="checkbox" data-i="' + i + '"' + ((k.value ?? String(k.default)) === 'true' ? ' checked' : '') + '>'
  : k.choices ? '<select data-i="' + i + '">' + k.choices.map((c) => '<option' + (String(c) === (k.value ?? String(k.default)).replace(/^"|"$/g, '') ? ' selected' : '') + '>' + esc(c) + '</option>').join('') + '</select>'
  : '<input data-i="' + i + '" value="' + esc(k.value ?? '') + '" placeholder="' + esc(JSON.stringify(k.default) ?? '') + '">';
const show = () => {
  const q = document.getElementById('q').value.trim().toLowerCase(), only = document.getElementById('set').checked;
  const hits = keys.filter((k) => (!only || k.value !== undefined) && (k.key + ' ' + k.about).toLowerCase().includes(q));
  document.getElementById('n').textContent = hits.length + ' of ' + keys.length;
  document.getElementById('list').innerHTML = hits.slice(0, 300).map((k) => '<li' + (k.value !== undefined ? ' class="changed"' : '') + '><div class="head"><code>' + esc(k.key) + '</code>' + field(k, keys.indexOf(k)) + '</div>'
    + '<div class="about">' + esc(k.about) + (k.example ? '<br><i>' + esc(k.example) + '</i>' : '') + '</div></li>').join('') || '<li class="muted">No setting matches.</li>';
};
addEventListener('message', (e) => { keys = e.data.keys; show(); });
document.getElementById('q').oninput = show;
document.getElementById('set').onchange = show;
document.getElementById('list').onchange = (e) => { const f = e.target.closest('[data-i]'); if (f) vscode.postMessage({ key: keys[+f.dataset.i].key, value: f.type === 'checkbox' ? String(f.checked) : f.value }); };
vscode.postMessage({ ready: true });`,
    `.bar { display: flex; gap: .6rem; align-items: center; position: sticky; top: 0; background: var(--vscode-editor-background); padding-bottom: .5rem; }
#q { flex: 1; } input, select { background: var(--vscode-input-background); color: var(--vscode-input-foreground); border: 1px solid var(--vscode-input-border, transparent); padding: .2rem .4rem; }
ul { list-style: none; padding: 0; margin: 0; } li { padding: .4rem 0; border-bottom: 1px solid var(--vscode-widget-border, transparent); }
li.changed code { color: var(--vscode-textLink-foreground); } .head { display: flex; gap: .8rem; align-items: center; justify-content: space-between; }
.about, #n, .muted { color: var(--vscode-descriptionForeground); font-size: .88em; margin-top: .15rem; }`);
    p.webview.onDidReceiveMessage(async (m: { ready?: boolean; key?: string; value?: string }) => {
        const key = keys.find(k => k.key === m.key);
        if (key && m.value !== undefined) {
            const before = await text();
            const edit = new vscode.WorkspaceEdit();
            const after = write(before, key.key, yamlValue(key.type, m.value));
            if (before === '' && !(await vscode.workspace.fs.stat(file).then(() => true, () => false))) { edit.createFile(file, { ignoreIfExists: true }); edit.insert(file, new vscode.Position(0, 0), after); }
            else { const d = await vscode.workspace.openTextDocument(file); edit.replace(file, d.validateRange(new vscode.Range(0, 0, d.lineCount, 0)), after); }
            // saved: the server reads the file from disk
            if (await vscode.workspace.applyEdit(edit)) { await (await vscode.workspace.openTextDocument(file)).save(); }
        }
        await send();
    });
}
