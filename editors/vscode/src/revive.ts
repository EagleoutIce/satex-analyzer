// A server the client gave up on (it closed five times in three minutes) stays down for the
// whole session: no lenses, hovers or findings. This starts it again after a pause, a longer one
// each time, and gives up after `limit` revivals that did not run `healthyMs`: a server killed
// over and over (memory) must not come back for good.
// No `vscode` import: unit tests load it.

/** Restarts a stopped server after `delayMs`, doubling; `stopped(false)` or `dispose` cancels. */
export class Revive {
    private timer: ReturnType<typeof setTimeout> | undefined;
    private attempts = 0;
    private upSince: number | undefined;

    constructor(private readonly restart: () => void, private readonly delayMs: number,
        private readonly timers = { set: setTimeout, clear: clearTimeout },
        private readonly now: () => number = Date.now, private readonly limit = 3,
        private readonly healthyMs = 10 * 60_000) { }

    /** The server stopped (`true`) or runs (`false`). */
    stopped(down: boolean): void {
        this.dispose();
        if (!down) { this.upSince = this.now(); return; }
        if (this.upSince !== undefined && this.now() - this.upSince >= this.healthyMs) { this.attempts = 0; }
        this.upSince = undefined;
        if (this.attempts >= this.limit) { return; }
        const delay = this.delayMs * 2 ** this.attempts++;
        this.timer = this.timers.set(() => { this.timer = undefined; this.restart(); }, delay);
    }

    /** Whether it gave up: the server stays down until a manual restart. */
    get exhausted(): boolean { return this.attempts >= this.limit && this.timer === undefined; }

    dispose(): void {
        if (this.timer !== undefined) { this.timers.clear(this.timer); this.timer = undefined; }
    }
}
