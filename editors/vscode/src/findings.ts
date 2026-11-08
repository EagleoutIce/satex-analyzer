/** What the editor shows while a run goes: `hide` only the findings no pass can change, `keep` the last
 * complete ones, `mark` those saying they may change. A complete result swaps in once, when the run ends. */
export type Mode = 'hide' | 'keep' | 'mark';

interface Heard { text?: string; phase?: string }
interface Position { line: number; character: number }
export interface Range { start: Position; end: Position }

/** Whether a newer analysis runs once `work` is heard. Only idle ends one. A hover's trial run
 * (`probe`) and a reused result (no phase) start none and end none. */
export function analyzingAfter(work: Heard, was: boolean): boolean {
    if (!work.text) { return false; }
    return work.phase && work.phase !== 'probe' ? true : was;
}

/** `at` in the text after `change`: inside what it replaced, at its start. */
function movedPosition(at: Position, change: { range: Range; text: string }): Position {
    const { start, end } = change.range;
    const before = at.line < start.line || (at.line === start.line && at.character <= start.character);
    if (before) { return at; }
    const inside = at.line < end.line || (at.line === end.line && at.character < end.character);
    if (inside) { return start; }
    const lines = change.text.split('\n');
    const line = at.line - end.line + start.line + lines.length - 1;
    if (at.line !== end.line) { return { line, character: at.character }; }
    const last = lines[lines.length - 1].length;
    return { line, character: at.character - end.character + last + (lines.length === 1 ? start.character : 0) };
}

/** `range` in the text after `change`. */
export function moved(range: Range, change: { range: Range; text: string }): Range {
    return { start: movedPosition(range.start, change), end: movedPosition(range.end, change) };
}

/** The findings heard from the server per file, and what of them the editor shows. */
export class Findings<D> {
    private readonly published = new Map<string, D[]>();
    /** The settled findings of the build's unfinished run: of a newer text than `published`. */
    private readonly early = new Map<string, D[]>();
    private analyzing = false;

    constructor(
        private readonly mode: () => Mode,
        private readonly mark: (d: D) => D,
        private readonly show: (uri: string, findings: D[]) => void) { }

    private hiding(): boolean { return this.analyzing && this.mode() === 'hide'; }

    private view(uri: string): D[] {
        const all = this.published.get(uri) ?? [];
        if (!this.analyzing || this.mode() === 'keep') { return all; }
        if (this.mode() === 'mark') { return all.map(this.mark); }
        return this.early.get(uri) ?? all.filter(d => (d as { data?: { settled?: boolean } }).data?.settled);
    }

    /** A complete result. While a run is said to go on, it waits for the run's end: one swap for all files. */
    publish(uri: string, findings: D[]): void {
        this.published.set(uri, findings);
        this.early.delete(uri);
        if (!this.hiding()) { this.show(uri, this.view(uri)); }
    }

    settled(uri: string, findings: D[]): void {
        this.early.set(uri, findings);
        if (this.hiding()) { this.show(uri, this.view(uri)); }
    }

    status(work: Heard): void {
        const now = analyzingAfter(work, this.analyzing);
        if (now === this.analyzing) { return; }
        this.analyzing = now;
        if (this.mode() === 'keep') { return; }
        for (const uri of new Set([...this.published.keys(), ...this.early.keys()])) { this.show(uri, this.view(uri)); }
    }

    /** Every finding of `uri` through `move`: an edit changed its text. */
    move(uri: string, move: (d: D) => D): void {
        for (const held of [this.published, this.early]) {
            const findings = held.get(uri);
            if (findings) { held.set(uri, findings.map(move)); }
        }
    }
}
