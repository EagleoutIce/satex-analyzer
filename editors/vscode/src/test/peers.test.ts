import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { peers, uninstallCommand, yields } from '../peers';

const ext = (id: string, name: string, commands: string[] = []) => ({ id, packageJSON: { name, contributes: { commands: commands.map(command => ({ command })) } } });

test('finds other SaTeX copies by name or command, case-blind on the own id', () => {
    const all = [ext('eagleoutice.satex', 'satex'), ext('TODO-publisher-id.satex', 'satex'), ext('x.renamed', 'other', ['satex.compile']), ext('james-yu.latex-workshop', 'latex-workshop', ['latex-workshop.build'])];
    assert.deepEqual(peers(all, 'EagleoutIce.satex'), ['TODO-publisher-id.satex', 'x.renamed']);
    assert.deepEqual(peers([ext('eagleoutice.satex', 'satex')], 'eagleoutice.satex'), []);
});

test('only a copy that is not the published one steps aside', () => {
    assert.equal(yields('eagleoutice.satex', ['TODO-publisher-id.satex']), false);
    assert.equal(yields('TODO-publisher-id.satex', ['eagleoutice.satex']), true);
    assert.equal(yields('a.satex', ['b.satex']), false);
    assert.equal(uninstallCommand('TODO-publisher-id.satex'), 'code --uninstall-extension TODO-publisher-id.satex');
});
