import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as vscode from 'vscode';
import { library } from '../library';
import { settings } from '../settings';
import { openView } from '../views';
import { seen } from './vscode-stub';

/* eslint-disable @typescript-eslint/no-explicit-any */
const context = { extensionUri: (vscode.Uri as any).file('/ext') } as any;
/** The page's inline script parses. */
const parses = (html: string) => new Function(html.split('<script nonce="')!.pop()!.replace(/^[^>]*>/, '').replace(/<\/script>[\s\S]*$/, ''));
const client = (answer: (method: string, params: any) => unknown) => ({ sendRequest: async (m: string, p: any) => answer(m, p) }) as any;

test('the library lists the names in force, environments once, and inserts a usage', async () => {
    const records = [{ name: '\\section', effective: '\\section{⟨title⟩}' }, { name: '\\itemize' }, { name: '\\enditemize' }];
    await library(context, client((m, p) => [{ records: p.arguments[0].queries[0].type === 'scope' ? records : [{ name: 'foo', usage: '\\foo', package_file: 'x/foo.sty' }] }]));
    parses(seen.html.at(-1)!);
    await seen.handler!({ all: false });
    const rows = seen.posts.at(-1).rows;
    assert.deepEqual(rows.map((r: any) => r.insert), ['\\section{title}', '\\begin{itemize}\n  \n\\end{itemize}', '\\foo']);
    await seen.handler!({ insert: '\\section{title}' });
    assert.deepEqual(seen.edits.at(-1), ['/p/main.tex', '\\section{title}']);
});

test('the settings write a key of satex.yaml in place and read it back', async () => {
    seen.files.set('/p/satex.yaml', '# mine\nlimits:\n  seconds: 30\n');
    await settings(context);
    parses(seen.html.at(-1)!);
    await seen.handler!({ ready: true });
    const keys = seen.posts.at(-1).keys;
    assert.equal(keys.find((k: any) => k.key === 'limits.seconds').value, '30');
    await seen.handler!({ key: 'limits.seconds', value: '60' });
    assert.equal(seen.files.get('/p/satex.yaml'), '# mine\nlimits:\n  seconds: 60\n');
    assert.equal(seen.posts.at(-1).keys.find((k: any) => k.key === 'limits.seconds').value, '60');
});

test('a view renders its markdown through the editor and applies its edits', async () => {
    const view = { plugin: 'color', title: 'color', sections: [{ kind: 'text', markdown: '**red**' }] };
    await openView(context, client(() => [view]), 'file:///p/main.tex', { line: 0, character: 0 }, 'color');
    const html = seen.html.at(-1)!;
    parses(html);
    assert.ok(html.includes('"**red**":"\\u003cp>**red**\\u003c/p>"'), html);
    await seen.handler!({ edits: [{ range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } }, newText: 'x' }] });
    assert.deepEqual(seen.edits.at(-1), ['/p/main.tex', 'x']);
});
