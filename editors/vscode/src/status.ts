import * as vscode from 'vscode';

/** satex's own findings: what the server and `satex: Compile` report. */
export function isOurs(d: vscode.Diagnostic): boolean {
    return d.source === 'satex' || d.source === 'satex build';
}

/**
 * The status bar entry: what the server does now (`caching pdftex kernel`),
 * else how many errors and warnings it found. A click opens the satex view;
 * `satex.statusBar.enabled` or the status bar's own menu hides it.
 */
export class Status implements vscode.Disposable {
    private readonly item = vscode.window.createStatusBarItem('satex.status', vscode.StatusBarAlignment.Left, 0);
    private readonly listeners: vscode.Disposable[] = [];
    private busy = '';
    private stopped = false;

    constructor() {
        this.item.name = 'satex';
        this.item.command = 'satex.focus';
        this.listeners.push(
            vscode.languages.onDidChangeDiagnostics(() => this.render()),
            vscode.workspace.onDidChangeConfiguration(e => {
                if (e.affectsConfiguration('satex.statusBar')) { this.render(); }
            }));
        this.render();
    }

    /** What the server says it does; empty when it is idle. */
    working(text: string): void {
        this.busy = text;
        this.render();
    }

    /** The server stopped (or runs again). */
    down(stopped: boolean): void {
        this.stopped = stopped;
        this.render();
    }

    private render(): void {
        if (!vscode.workspace.getConfiguration('satex').get<boolean>('statusBar.enabled', true)) {
            this.item.hide();
            return;
        }
        let errors = 0;
        let warnings = 0;
        for (const [, diagnostics] of vscode.languages.getDiagnostics()) {
            for (const d of diagnostics.filter(isOurs)) {
                if (d.severity === vscode.DiagnosticSeverity.Error) { errors++; }
                if (d.severity === vscode.DiagnosticSeverity.Warning) { warnings++; }
            }
        }
        if (this.stopped) {
            this.item.text = '$(error) satex';
            this.item.tooltip = 'The satex server stopped: see the output "satex". Click to open the satex view.';
        } else if (this.busy) {
            this.item.text = `$(sync~spin) satex: ${this.busy}`;
            this.item.tooltip = 'satex is working; kernels and packages are cached once, then reused.';
        } else {
            const counts = errors + warnings > 0 ? ` $(error) ${errors} $(warning) ${warnings}` : ' $(check)';
            this.item.text = `satex${counts}`;
            this.item.tooltip = `satex: ${errors} errors, ${warnings} warnings. Click to open the satex view.`;
        }
        this.item.show();
    }

    dispose(): void {
        this.item.dispose();
        this.listeners.forEach(l => l.dispose());
    }
}
