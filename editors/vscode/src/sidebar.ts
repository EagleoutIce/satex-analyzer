import * as vscode from 'vscode';

interface Action { label: string; command: string; icon: string; tooltip: string }

const ACTIONS: Action[] = [
    { label: 'Show findings', command: 'satex.focus', icon: 'warning', tooltip: 'satex\'s findings in the Problems view' },
    { label: 'Show caches', command: 'satex.caches', icon: 'database', tooltip: 'Which kernel, class and package caches the last analysis reused or rebuilt' },
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
