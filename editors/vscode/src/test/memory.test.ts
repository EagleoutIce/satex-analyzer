import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { cgroupMax } from '../memory';

/** Bug pinned: the cgroup capped at the budget itself, so the kernel killed the server first. */
test('the cgroup leaves the budget headroom', () => {
    assert.equal(cgroupMax('2GiB'), '3072M');
    assert.equal(cgroupMax('512M'), '768M');
    assert.equal(cgroupMax('1.5G'), '2304M');
    assert.equal(cgroupMax('lots'), undefined);
});
