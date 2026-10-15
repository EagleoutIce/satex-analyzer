import * as vscode from 'vscode';
import * as path from 'path';
import { isOurs } from './status';

const ORDER = [vscode.DiagnosticSeverity.Error, vscode.DiagnosticSeverity.Warning,
    vscode.DiagnosticSeverity.Information, vscode.DiagnosticSeverity.Hint];
const ICON: Record<number, vscode.ThemeIcon> = {
    [vscode.DiagnosticSeverity.Error]: new vscode.ThemeIcon('error', new vscode.ThemeColor('problemsErrorIcon.foreground')),
    [vscode.DiagnosticSeverity.Warning]: new vscode.ThemeIcon('warning', new vscode.ThemeColor('problemsWarningIcon.foreground')),
    [vscode.DiagnosticSeverity.Information]: new vscode.ThemeIcon('info', new vscode.ThemeColor('problemsInfoIcon.foreground')),
    [vscode.DiagnosticSeverity.Hint]: new vscode.ThemeIcon('lightbulb'),
};

interface Finding { uri: vscode.Uri; diagnostic: vscode.Diagnostic }
interface Rule { code: string; severity: vscode.DiagnosticSeverity; findings: Finding[] }
type Node = Rule | Finding;

function codeOf(d: vscode.Diagnostic): string {
    const code = typeof d.code === 'object' ? d.code.value : d.code;
    return code === undefined ? 'satex' : String(code);
}

/** The findings of every open document, one node per rule, most severe first. */
export class Findings implements vscode.TreeDataProvider<Node>, vscode.Disposable {
    private readonly changed = new vscode.EventEmitter<void>();
    readonly onDidChangeTreeData = this.changed.event;
    private rules: Rule[] = [];
    private timer: NodeJS.Timeout | undefined;
    private readonly listener: vscode.Disposable;
    view: vscode.TreeView<Node> | undefined;

    constructor() {
        // Diagnostics come in bursts, one per file: gather them first.
        this.listener = vscode.languages.onDidChangeDiagnostics(() => {
            clearTimeout(this.timer);
            this.timer = setTimeout(() => this.refresh(), 300);
        });
        this.refresh();
    }

    private refresh(): void {
        const rules = new Map<string, Rule>();
        for (const [uri, diagnostics] of vscode.languages.getDiagnostics()) {
            for (const diagnostic of diagnostics.filter(isOurs)) {
                const code = codeOf(diagnostic);
                const rule = rules.get(code) ?? { code, severity: diagnostic.severity, findings: [] };
                rule.severity = Math.min(rule.severity, diagnostic.severity);
                rule.findings.push({ uri, diagnostic });
                rules.set(code, rule);
            }
        }
        this.rules = [...rules.values()].sort((a, b) =>
            ORDER.indexOf(a.severity) - ORDER.indexOf(b.severity) || a.code.localeCompare(b.code));
        const shown = this.rules.filter(r => r.severity <= vscode.DiagnosticSeverity.Warning)
            .reduce((n, r) => n + r.findings.length, 0);
        if (this.view) {
            this.view.badge = shown > 0 ? { value: shown, tooltip: `${shown} errors and warnings` } : undefined;
            this.view.message = this.rules.length === 0 ? 'No findings in the open documents.' : undefined;
        }
        this.changed.fire();
    }

    getTreeItem(node: Node): vscode.TreeItem {
        if ('findings' in node) {
            const item = new vscode.TreeItem(node.code, vscode.TreeItemCollapsibleState.Expanded);
            item.description = String(node.findings.length);
            item.iconPath = ICON[node.severity];
            return item;
        }
        const { uri, diagnostic } = node;
        const item = new vscode.TreeItem(diagnostic.message.split('\n')[0], vscode.TreeItemCollapsibleState.None);
        item.description = `${path.basename(uri.fsPath)}:${diagnostic.range.start.line + 1}`;
        item.tooltip = new vscode.MarkdownString().appendText(diagnostic.message);
        item.iconPath = ICON[diagnostic.severity];
        item.command = { command: 'vscode.open', title: 'Open', arguments: [uri, { selection: diagnostic.range }] };
        return item;
    }

    getChildren(node?: Node): Node[] {
        if (!node) { return this.rules; }
        return 'findings' in node
            ? [...node.findings].sort((a, b) => a.uri.fsPath.localeCompare(b.uri.fsPath)
                || a.diagnostic.range.start.line - b.diagnostic.range.start.line)
            : [];
    }

    dispose(): void {
        clearTimeout(this.timer);
        this.listener.dispose();
        this.changed.dispose();
    }
}

interface Action { label: string; command: string; icon: string; tooltip: string }

const ACTIONS: Action[] = [
    { label: 'Compile document', command: 'satex.compile', icon: 'play', tooltip: 'Build with latexmk and explain the log' },
    { label: 'Trace expansion here', command: 'satex.traceAtCursor', icon: 'debug-step-into', tooltip: 'What the text at the cursor expands to' },
    { label: 'Restart server', command: 'satex.restart', icon: 'debug-restart', tooltip: 'Restart the satex language server' },
    { label: 'Show server output', command: 'satex.showOutput', icon: 'output', tooltip: 'The satex server log' },
    { label: 'Settings', command: 'satex.openSettings', icon: 'gear', tooltip: 'satex settings' },
];

/** What satex can do from the view. */
export class Actions implements vscode.TreeDataProvider<Action> {
    getTreeItem(action: Action): vscode.TreeItem {
        const item = new vscode.TreeItem(action.label);
        item.iconPath = new vscode.ThemeIcon(action.icon);
        item.tooltip = action.tooltip;
        item.command = { command: action.command, title: action.label };
        return item;
    }

    getChildren(): Action[] {
        return ACTIONS;
    }
}
