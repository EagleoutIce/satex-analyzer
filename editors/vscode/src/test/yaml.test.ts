import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { read, scalar, write, yamlValue } from '../yaml';

const doc = '# project\nlimits:\n  seconds: 30 # a backstop\n  memory: 2GiB\n\nlints:\n  off: [a]\n';

test('reads a dotted key, its comment off', () => {
    assert.equal(read(doc, 'limits.seconds'), '30');
    assert.equal(read(doc, 'lints.off'), '[a]');
    assert.equal(read(doc, 'limits.paths'), undefined);
    assert.equal(read(doc, 'seconds'), undefined, 'a nested key is not a top one');
});

test('writes in place, keeping comments and the other keys', () => {
    assert.equal(write(doc, 'limits.seconds', '60'), doc.replace('seconds: 30 #', 'seconds: 60 #'));
    assert.equal(read(write(doc, 'limits.paths', '8'), 'limits.paths'), '8');
    assert.equal(write(doc, 'limits.paths', '8').split('\n')[5], '', 'added inside its mapping, before the blank line');
    const added = write(doc, 'analysis.assume_compiles', 'false');
    assert.ok(added.startsWith(doc) && added.endsWith('analysis:\n  assume_compiles: false\n'), added);
    assert.equal(write('', 'a.b.c', '1'), 'a:\n  b:\n    c: 1\n');
    assert.equal(write('a: 3\n', 'a.b', '1'), 'a:\n  b: 1\n', 'a scalar becomes the mapping');
});

test('scalars read back the same', () => {
    assert.equal(scalar(true), 'true');
    assert.equal(scalar('2GiB'), '2GiB');
    assert.equal(scalar('yes'), '"yes"');
    assert.equal(scalar('a b'), '"a b"');
    assert.equal(scalar(['x', 'y']), '["x","y"]');
});

test('a setting typed by its schema', () => {
    assert.equal(yamlValue('boolean', 'true'), 'true');
    assert.equal(yamlValue('integer', '12'), '12');
    assert.equal(yamlValue('integer', 'x'), 'x');
    assert.equal(yamlValue('array', 'a, b'), '["a","b"]');
    assert.equal(yamlValue('array', '["c"]'), '["c"]');
    assert.equal(yamlValue('string', '2024'), '"2024"');
});
