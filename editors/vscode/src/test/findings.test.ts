import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { analyzingAfter, Findings, Mode, moved } from '../findings';

type D = { id: string; data?: { settled?: boolean } };
const stable: D = { id: 'marker', data: { settled: true } };
const ref: D = { id: 'ref' };
const run = { text: 'analyzing main.tex', phase: 'document' };
const probe = { text: 'learning what \\cref does', phase: 'probe' };

/** What each `show` set, for a client in `mode`. */
function client(mode: Mode) {
    const shows: string[][] = [];
    const findings = new Findings<D>(() => mode, d => ({ ...d, id: `${d.id}?` }), (_, ds) => shows.push(ds.map(d => d.id)));
    return { shows, findings };
}

/** Intuition: only a run's own status starts it, and only idle ends it.
 * Bug pinned: a hover's trial run during a run read as its end, so the findings flipped with each status. */
test('a probe or a reused result neither starts nor ends an analysis', () => {
    assert.equal(analyzingAfter(probe, true), true);
    assert.equal(analyzingAfter(probe, false), false);
    assert.equal(analyzingAfter({ text: 'reused: comment' }, false), false);
    assert.equal(analyzingAfter(run, false), true);
    assert.equal(analyzingAfter({ text: 'analysis follows', phase: 'pending' }, true), true);
    assert.equal(analyzingAfter({ text: '' }, true), false);
});

/** Intuition: while a run goes only what no pass can change is shown, and its end swaps once.
 * Bug pinned: findings came back with each status, and a stable one left during a run. */
test('hide: one hide, settled findings stay, one swap at the end', () => {
    const { shows, findings } = client('hide');
    findings.publish('a', [stable, ref]);
    findings.status(run);
    findings.status(probe);
    findings.status({ text: 'run 2', phase: 'document' });
    findings.publish('a', [stable, ref]); // the run's result, heard before its idle
    assert.deepEqual(shows, [['marker', 'ref'], ['marker']]);
    findings.status({ text: '' });
    assert.deepEqual(shows, [['marker', 'ref'], ['marker'], ['marker', 'ref']]);
});

/** Intuition: a run stopped for newer text is followed by one, so nothing comes back in between.
 * Bug pinned: each keystroke during a long run brought the findings back and took them away again. */
test('hide: a stopped run followed by a newer one shows nothing in between', () => {
    const { shows, findings } = client('hide');
    findings.publish('a', [stable, ref]);
    findings.status(run);
    findings.status({ text: 'analysis follows', phase: 'pending' });
    findings.status(run);
    assert.deepEqual(shows, [['marker', 'ref'], ['marker']]);
});

/** Intuition: a build's first pass already knows what no later pass changes: a first run shows it, and it only grows.
 * Bug pinned: nothing to show until the whole build ended. */
test('hide: settled findings of an unfinished run show at once and only grow', () => {
    const { shows, findings } = client('hide');
    findings.status(run);
    findings.settled('a', [stable]);
    findings.publish('a', [stable, ref]);
    findings.status({ text: '' });
    assert.deepEqual(shows, [['marker'], ['marker', 'ref']]);
});

/** Intuition: `keep` never touches the view during a run, `mark` only says the findings may change. */
test('keep and mark keep every finding', () => {
    const keep = client('keep');
    keep.findings.publish('a', [ref]);
    keep.findings.status(run);
    keep.findings.settled('a', []);
    keep.findings.status({ text: '' });
    assert.deepEqual(keep.shows, [['ref']]);
    const mark = client('mark');
    mark.findings.publish('a', [ref]);
    mark.findings.status(run);
    mark.findings.status({ text: '' });
    assert.deepEqual(mark.shows, [['ref'], ['ref?'], ['ref']]);
});

/** Intuition: what is held back moves with the text, so it comes back at its place.
 * Bug pinned: a re-set of the findings put them where they stood before the edit. */
test('an edit moves a held range', () => {
    const at = (line: number, character: number) => ({ line, character });
    const range = { start: at(5, 4), end: at(5, 8) };
    assert.deepEqual(moved(range, { range: { start: at(1, 0), end: at(1, 0) }, text: 'new\nline\n' }), { start: at(7, 4), end: at(7, 8) });
    assert.deepEqual(moved(range, { range: { start: at(5, 0), end: at(5, 2) }, text: 'x' }), { start: at(5, 3), end: at(5, 7) });
    assert.deepEqual(moved(range, { range: { start: at(9, 0), end: at(9, 0) }, text: 'after' }), range);
});
