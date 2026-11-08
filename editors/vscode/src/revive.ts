// A server the client gave up on (it closed five times in three minutes) stays down for the
// whole session: no lenses, hovers or findings. This starts it again after a pause.
// No `vscode` import: unit tests load it.

/** Restarts a stopped server after `delayMs`, again and again; `stopped(false)` or `dispose` cancels. */
export class Revive {
    private timer: ReturnType<typeof setTimeout> | undefined;

    constructor(private readonly restart: () => void, private readonly delayMs: number,
        private readonly timers = { set: setTimeout, clear: clearTimeout }) { }

    /** The server stopped (`true`) or runs (`false`). */
    stopped(down: boolean): void {
        this.dispose();
        if (down) { this.timer = this.timers.set(() => { this.timer = undefined; this.restart(); }, this.delayMs); }
    }

    dispose(): void {
        if (this.timer !== undefined) { this.timers.clear(this.timer); this.timer = undefined; }
    }
}
