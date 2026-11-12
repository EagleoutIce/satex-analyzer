import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { Revive } from '../revive';

/**
 * Intuition: VS Code's client stops restarting a server after five closes in three minutes, and
 * then no code lens (the table widget's "Edit table...") ever shows again in that window, with
 * nothing to tell why. Bug pinned: the extension must start the server again after a pause, but
 * not while it runs, and not after it was disposed.
 */
test('a stopped server restarts after the pause, a running or disposed one does not', () => {
    let now = 0;
    const jobs = new Map<number, { at: number; run: () => void }>();
    let next = 0;
    const timers = {
        set: ((run: () => void, ms: number) => { jobs.set(++next, { at: now + ms, run }); return next; }) as unknown as typeof setTimeout,
        clear: ((id: number) => { jobs.delete(id); }) as unknown as typeof clearTimeout,
    };
    const advance = (ms: number) => { now += ms; for (const [id, job] of [...jobs]) { if (job.at <= now) { jobs.delete(id); job.run(); } } };
    let restarts = 0;
    const revive = new Revive(() => { restarts++; }, 100, timers);
    revive.stopped(true);
    advance(99);
    assert.equal(restarts, 0);
    advance(1);
    assert.equal(restarts, 1);
    revive.stopped(true);
    revive.stopped(false);
    advance(1000);
    assert.equal(restarts, 1, 'running again cancels the pending restart');
    revive.stopped(true);
    revive.dispose();
    advance(1000);
    assert.equal(restarts, 1, 'disposed: no restart');
});

/**
 * Bug pinned: a server the memory cap kills a minute after each start was revived forever, one
 * "memory shortage" notice per kill. Revivals now back off and stop after three short runs; a
 * long healthy run earns the full count back.
 */
test('revivals back off, stop after three short runs, and reset after a healthy one', () => {
    let now = 0;
    const jobs = new Map<number, { at: number; run: () => void }>();
    let next = 0;
    const timers = {
        set: ((run: () => void, ms: number) => { jobs.set(++next, { at: now + ms, run }); return next; }) as unknown as typeof setTimeout,
        clear: ((id: number) => { jobs.delete(id); }) as unknown as typeof clearTimeout,
    };
    const advance = (ms: number) => { now += ms; for (const [id, job] of [...jobs]) { if (job.at <= now) { jobs.delete(id); job.run(); } } };
    let restarts = 0;
    const revive = new Revive(() => { restarts++; revive.stopped(false); }, 100, timers, () => now, 3, 10_000);
    const killed = () => { advance(50); revive.stopped(true); };
    revive.stopped(false);
    killed(); advance(100); assert.equal(restarts, 1);
    killed(); advance(199); assert.equal(restarts, 1, 'second pause doubles'); advance(1); assert.equal(restarts, 2);
    killed(); advance(400); assert.equal(restarts, 3);
    killed(); advance(100_000); assert.equal(restarts, 3, 'gives up after three');
    assert.ok(revive.exhausted);
    revive.stopped(false); advance(10_000); revive.stopped(true); advance(100);
    assert.equal(restarts, 4, 'a healthy run resets the count');
});
