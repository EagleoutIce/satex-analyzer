import * as vscode from 'vscode';
import { spawn, spawnSync } from 'child_process';
import { existsSync } from 'fs';
import * as path from 'path';
import { LanguageClient, TransportKind } from 'vscode-languageclient/node';

const INSTALL_HINT = 'Install it with `cargo install satex`, download a release from https://github.com/EagleoutIce/satex-analyzer/releases, or set `satex.path`.';
let client: LanguageClient | undefined;

/** Setting, then the bundled binary, then `satex` on the PATH. */
function resolveBinary(context: vscode.ExtensionContext): string | undefined {
    const configured = vscode.workspace.getConfiguration('satex').get<string>('path');
    if (configured) {
        return existsSync(configured) ? configured : undefined;
    }
    const bundled = context.asAbsolutePath(path.join('bin', `${process.platform}-${process.arch}`, process.platform === 'win32' ? 'satex.exe' : 'satex'));
    if (existsSync(bundled)) {
        return bundled;
    }
    return spawnSync('satex', ['--version']).error ? undefined : 'satex';
}

/** satex reads the installed TeX distribution; warn when it is missing. */
function checkTeX(): void {
    const missing = ['kpsewhich', 'pdflatex'].filter(tool => spawnSync(tool, ['--version']).error);
    if (missing.length > 0) {
        void vscode.window.showWarningMessage(
            `satex uses your TeX distribution, but ${missing.join(' and ')} not found on the PATH. Install TeX Live, MiKTeX or MacTeX for full analysis.`);
    }
}

/** Build the kernel caches in the background unless they are already there. */
function buildCaches(binary: string): void {
    const status = spawnSync(binary, ['--format', 'json', 'cache'], { encoding: 'utf8' });
    try {
        if (JSON.parse(status.stdout).cached === true) {
            return;
        }
    } catch { /* unreadable status: build anyway */ }
    void vscode.window.withProgress(
        { location: vscode.ProgressLocation.Notification, title: 'satex: building LaTeX kernel caches', cancellable: true },
        (progress, token) => new Promise<void>(resolve => {
            const child = spawn(binary, ['cache', 'build']);
            const report = (data: Buffer) => {
                const line = data.toString().trim().split('\n').pop();
                if (line) { progress.report({ message: line }); }
            };
            child.stderr.on('data', report);
            child.stdout.on('data', report);
            token.onCancellationRequested(() => child.kill());
            child.on('error', () => resolve());
            child.on('close', code => {
                if (code) { void vscode.window.showWarningMessage(`satex: cache build failed (exit ${code}); analysis stays slower.`); }
                resolve();
            });
        }));
}

export function activate(context: vscode.ExtensionContext): void {
    const binary = resolveBinary(context);
    if (!binary) {
        void vscode.window.showErrorMessage(`satex binary not found. ${INSTALL_HINT}`);
        return;
    }
    checkTeX();
    client = new LanguageClient(
        'satex', 'satex',
        { command: binary, args: ['lsp'], transport: TransportKind.stdio },
        { documentSelector: ['latex', 'tex', 'bibtex'].map(language => ({ language })) });
    void client.start();
    setImmediate(() => buildCaches(binary));
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
