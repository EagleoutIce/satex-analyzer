import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { capMiB } from '../memory';

/** Bug pinned: the cgroup capped at the budget itself, so the kernel killed the server first. */
test('the cgroup leaves the budget headroom', () => {
    assert.equal(capMiB('2GiB'), 3072);
    assert.equal(capMiB('512M'), 768);
    assert.equal(capMiB('1.5G'), 2304);
    assert.equal(capMiB('lots'), undefined);
    // `0` is no limit, not a cap of nothing
    assert.equal(capMiB('0'), undefined);
});
