import * as vscode from 'vscode';
import * as path from 'path';

/** One event of `satex trace --format json`. */
export interface TraceEvent { index: number; step: string; name: string; file: string; path: string; line: number; col: number }

/** A trace held for stepping: the editor follows the current event. */
export class Steps implements vscode.Disposable {
    private events: TraceEvent[] = [];
    private dir = '';
    private at = 0;
    private readonly mark = vscode.window.createTextEditorDecorationType({
        isWholeLine: true,
        backgroundColor: new vscode.ThemeColor('editor.stackFrameHighlightBackground'),
        after: { color: new vscode.ThemeColor('editorCodeLens.foreground'), margin: '0 0 0 2em' },
    });
    private readonly item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);

    /** Hold `events`, whose paths are relative to `dir`, and show the first. */
    async load(events: TraceEvent[], dir: string): Promise<void> {
        this.events = events;
        this.dir = dir;
        this.at = 0;
        await vscode.commands.executeCommand('setContext', 'satex.tracing', events.length > 0);
        await this.show();
    }

    async move(by: number): Promise<void> {
        if (this.events.length === 0) { return; }
        this.at = Math.min(this.events.length - 1, Math.max(0, this.at + by));
        await this.show();
    }

    /** Pick a step from the whole trace. */
    async pick(): Promise<void> {
        const items = this.events.map((e, i) => ({
            label: `${i + 1}. ${e.name}`, description: e.step, detail: `${e.file}:${e.line}:${e.col}`, i,
        }));
        const chosen = await vscode.window.showQuickPick(items, { placeHolder: 'Go to a step of the trace', matchOnDetail: true });
        if (chosen) {
            this.at = chosen.i;
            await this.show();
        }
    }

    async clear(): Promise<void> {
        this.events = [];
        this.item.hide();
        vscode.window.visibleTextEditors.forEach(e => e.setDecorations(this.mark, []));
        await vscode.commands.executeCommand('setContext', 'satex.tracing', false);
    }

    private async show(): Promise<void> {
        const event = this.events[this.at];
        if (!event) { return; }
        this.item.text = `$(debug-step-over) step ${this.at + 1}/${this.events.length}: ${event.name} (${event.step})`;
        this.item.tooltip = 'satex trace: Alt+] next, Alt+[ previous, click to pick';
        this.item.command = 'satex.tracePick';
        this.item.show();
        const file = path.isAbsolute(event.path) ? event.path : path.join(this.dir, event.path);
        let editor: vscode.TextEditor;
        try {
            editor = await vscode.window.showTextDocument(vscode.Uri.file(file), { preserveFocus: true, preview: true });
        } catch {
            return;
        }
        vscode.window.visibleTextEditors.forEach(e => e.setDecorations(this.mark, []));
        const position = new vscode.Position(Math.max(0, event.line - 1), Math.max(0, event.col - 1));
        editor.setDecorations(this.mark, [{
            range: new vscode.Range(position, position),
            renderOptions: { after: { contentText: `${this.at + 1}/${this.events.length} ${event.step} ${event.name}` } },
        }]);
        editor.revealRange(new vscode.Range(position, position), vscode.TextEditorRevealType.InCenterIfOutsideViewport);
    }

    dispose(): void {
        this.mark.dispose();
        this.item.dispose();
    }
}
