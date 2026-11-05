import * as vscode from 'vscode';

/** satex's own findings: what the server and `satex: Compile` report. */
export function isOurs(d: vscode.Diagnostic): boolean {
    return d.source?.startsWith('satex') ?? false;
}

/** The source a finding carries while a newer analysis runs: the Problems view says it may change. */
export const STALE = 'satex · analyzing…';

/** What the server does now (`satex/status`): `text` empty when idle. */
export interface Work {
    text: string;
    phase?: string;
    file?: string;
    line?: number;
    files?: number;
    lines?: number;
    cached?: number;
    /** Characters of the job's file read, of `total`. */
    reached?: number;
    total?: number;
    ms?: number;
    paths?: number;
    probe?: string;
    why?: string;
}

/** `work` as a markdown table for the status item's hover. */
function explain(work: Work): vscode.MarkdownString {
    const n = (v?: number) => (v ?? 0).toLocaleString();
    const rows: [string, string | undefined][] = [
        ['Phase', work.phase],
        ['Trying out', work.probe && `\`${work.probe}\` on typed arguments, for its hover`],
        ['Why', work.why],
        ['Reading', work.file && `\`${work.file}${work.line ? `:${work.line}` : ''}\``],
        ['Read', work.lines ? `${n(work.lines)} lines in ${n(work.files)} files` : undefined],
        ['From caches', work.cached ? `${n(work.cached)} lines` : undefined],
        ['Paths alive', work.paths ? n(work.paths) : undefined],
        ['Elapsed', work.ms ? `${(work.ms / 1000).toFixed(1)} s` : undefined],
    ];
    const md = new vscode.MarkdownString(`**satex: ${work.text}**\n\n| | |\n|---|---|\n`);
    md.appendMarkdown(rows.filter(([, v]) => v).map(([k, v]) => `| ${k} | ${v} |`).join('\n'));
    md.appendMarkdown('\n\nKernels and packages are cached once, then reused. Click for the cache report.');
    return md;
}

/**
 * The status bar entry: what the server does now (`caching pdftex kernel`),
 * else how many errors and warnings it found. A click shows the caches while it
 * works, else satex's findings in the Problems view;
 * `satex.statusBar.enabled` or the status bar's own menu hides it.
 */
export class Status implements vscode.Disposable {
    private readonly item = vscode.window.createStatusBarItem('satex.status', vscode.StatusBarAlignment.Left, 0);
    private readonly listeners: vscode.Disposable[] = [];
    private busy: Work = { text: '' };
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
    working(work: Work): void {
        this.busy = work;
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
            this.item.tooltip = 'The satex server stopped: see the output "satex". Click to show the findings.';
            this.item.command = 'satex.focus';
        } else if (this.busy.text) {
            const { reached, total } = this.busy;
            const done = total ? ` ${Math.min(100, Math.floor((reached ?? 0) * 100 / total))}%` : '';
            this.item.text = `$(sync~spin) satex: ${this.busy.text}${done}`;
            this.item.tooltip = explain(this.busy);
            this.item.command = 'satex.caches';
        } else {
            const counts = errors + warnings > 0 ? ` $(error) ${errors} $(warning) ${warnings}` : ' $(check)';
            this.item.text = `satex${counts}`;
            this.item.tooltip = `satex: ${errors} errors, ${warnings} warnings. Click to show them in the Problems view.`;
            this.item.command = 'satex.focus';
        }
        this.item.show();
    }

    dispose(): void {
        this.item.dispose();
        this.listeners.forEach(l => l.dispose());
    }
}
