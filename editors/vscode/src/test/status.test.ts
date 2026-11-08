import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { analyzingAfter } from '../status';

/** Intuition: only a run's own status starts it, and only idle ends it.
 * Bug pinned: a hover's trial run during a run read as its end, so the findings flipped with each status. */
test('a probe neither starts nor ends an analysis', () => {
    const probe = { text: 'learning what \\cref does', phase: 'probe' };
    assert.equal(analyzingAfter(probe, true), true);
    assert.equal(analyzingAfter(probe, false), false);
    assert.equal(analyzingAfter({ text: 'analyzing main.tex', phase: 'document' }, false), true);
    assert.equal(analyzingAfter({ text: '' }, true), false);
});
