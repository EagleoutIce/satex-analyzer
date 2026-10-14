import * as vscode from 'vscode';
import { execFile, spawn, spawnSync } from 'child_process';
import { existsSync } from 'fs';
import * as path from 'path';
import { LanguageClient, TransportKind } from 'vscode-languageclient/node';

const INSTALL_HINT = 'Install it with `cargo install satex`, download a release from https://github.com/EagleoutIce/satex-analyzer/releases, or set `satex.path`.';
let client: LanguageClient | undefined;
let binaryPath = '';
const trace = vscode.window.createOutputChannel('satex trace');
const build = vscode.window.createOutputChannel('satex build');
const buildDiagnostics = vscode.languages.createDiagnosticCollection('satex build');

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

/** The root document (setting `satex.rootFile`, else the active file) and its directory. */
function rootFile(editor: vscode.TextEditor): { root: string; dir: string } {
    const configured = vscode.workspace.getConfiguration('satex').get<string>('rootFile');
    const folder = vscode.workspace.getWorkspaceFolder(editor.document.uri)?.uri.fsPath;
    const root = configured ? path.resolve(folder ?? '', configured) : editor.document.uri.fsPath;
    return { root, dir: path.dirname(root) };
}

interface TraceEvent { index: number; step: string; name: string; file: string; line: number; col: number }

function runTrace(args: string[]): Promise<TraceEvent[]> {
    return new Promise((resolve, reject) => execFile(binaryPath, args, { maxBuffer: 1 << 28 }, (error, stdout, stderr) => {
        if (stderr) { trace.appendLine(stderr); }
        if (error) { reject(error); } else { resolve(JSON.parse(stdout) as TraceEvent[]); }
    }));
}

/** Trace what the selection (or the control sequence at the cursor) expands to here. */
async function traceAtCursor(): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!editor) { return; }
    const { selection, document } = editor;
    const range = selection.isEmpty ? document.getWordRangeAtPosition(selection.active, /\\([a-zA-Z@]+|.)/) : selection;
    const text = await vscode.window.showInputBox({ prompt: 'Text to inject at the cursor', value: range ? document.getText(range) : '' });
    if (!text) { return; }
    const { root, dir } = rootFile(editor);
    const at = `${path.relative(dir, document.uri.fsPath)}:${selection.start.line + 1}:${selection.start.character + 1}`;
    try {
        const events = await runTrace(['trace', '-f', root, '--inject', text, '--at', at, '--format', 'json']);
        const rows = events.map(e => `| ${e.index} | ${e.step} | \`${e.name}\` | ${e.file}:${e.line} |`);
        const content = `# Expansion of \`${text}\` at ${at}\n\n| # | step | name | location |\n| --- | --- | --- | --- |\n${rows.join('\n')}\n`;
        const doc = await vscode.workspace.openTextDocument({ language: 'markdown', content });
        await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside, true);
    } catch (error) {
        trace.appendLine(String(error));
        trace.show(true);
        void vscode.window.showErrorMessage('satex: trace failed, see the output channel "satex trace".');
    }
}

interface BuildRecord { path: string; line: number | null; col: number; severity: string; code: string; message: string; fix: string | null }

const SEVERITY: Record<string, vscode.DiagnosticSeverity> = {
    error: vscode.DiagnosticSeverity.Error,
    warning: vscode.DiagnosticSeverity.Warning,
};

/** Build the root document and show the build's findings as diagnostics. */
async function compile(): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!editor) { return; }
    const { root, dir } = rootFile(editor);
    buildDiagnostics.clear();
    build.clear();
    build.show(true);
    const records = await new Promise<BuildRecord[]>(resolve => {
        const child = spawn(binaryPath, ['compile', '-f', root, '--format', 'json'], { cwd: dir });
        let stdout = '';
        child.stdout.on('data', (data: Buffer) => { stdout += data.toString(); });
        child.stderr.on('data', (data: Buffer) => build.append(data.toString()));
        child.on('error', error => { build.appendLine(String(error)); resolve([]); });
        child.on('close', () => {
            try { resolve(JSON.parse(stdout) as BuildRecord[]); } catch { resolve([]); }
        });
    });
    const byFile = new Map<string, vscode.Diagnostic[]>();
    for (const r of records) {
        const line = Math.max((r.line ?? 1) - 1, 0);
        const col = Math.max(r.col - 1, 0);
        const diagnostic = new vscode.Diagnostic(new vscode.Range(line, col, line, Number.MAX_SAFE_INTEGER),
            r.fix ? `${r.message}\n${r.fix}` : r.message, SEVERITY[r.severity] ?? vscode.DiagnosticSeverity.Information);
        diagnostic.source = 'satex build';
        diagnostic.code = r.code;
        const file = path.resolve(dir, r.path);
        byFile.set(file, [...byFile.get(file) ?? [], diagnostic]);
    }
    for (const [file, diagnostics] of byFile) { buildDiagnostics.set(vscode.Uri.file(file), diagnostics); }
}

export function activate(context: vscode.ExtensionContext): void {
    const binary = resolveBinary(context);
    if (!binary) {
        void vscode.window.showErrorMessage(`satex binary not found. ${INSTALL_HINT}`);
        return;
    }
    binaryPath = binary;
    context.subscriptions.push(
        trace, build, buildDiagnostics,
        vscode.commands.registerCommand('satex.traceAtCursor', traceAtCursor),
        vscode.commands.registerCommand('satex.compile', compile));
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
