import * as vscode from 'vscode';
import { execFile, spawn, spawnSync } from 'child_process';
import { existsSync } from 'fs';
import * as os from 'os';
import * as path from 'path';
import { LanguageClient, State, StreamInfo } from 'vscode-languageclient/node';
import { Actions } from './sidebar';
import { STALE, Status, Work } from './status';
import { Steps, TraceEvent } from './steps';

const INSTALL_HINT = 'Install it with `cargo install satex-analyzer`, download a release from https://github.com/EagleoutIce/satex-analyzer/releases, or set `satex.path`.';
let client: LanguageClient | undefined;
let binaryPath = '';
const trace = vscode.window.createOutputChannel('satex trace');
const build = vscode.window.createOutputChannel('satex build');
const buildDiagnostics = vscode.languages.createDiagnosticCollection('satex build');
const steps = new Steps();

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

/** The root document (setting `satex.rootFile`, else the active file) and its directory. */
function rootFile(editor: vscode.TextEditor): { root: string; dir: string } {
    const configured = vscode.workspace.getConfiguration('satex').get<string>('rootFile');
    const folder = vscode.workspace.getWorkspaceFolder(editor.document.uri)?.uri.fsPath;
    const root = configured ? path.resolve(folder ?? '', configured) : editor.document.uri.fsPath;
    return { root, dir: path.dirname(root) };
}

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
        await steps.load(events, dir);
    } catch (error) {
        trace.appendLine(String(error));
        trace.show(true);
        void vscode.window.showErrorMessage('satex: trace failed, see the output channel "satex trace".');
    }
}

/** A file's findings as `satex compile --format lsp` writes them. */
interface BuildFile { uri: string; diagnostics: unknown[] }

/** Build the root document and show the build's findings as diagnostics. */
async function compile(): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!editor) { return; }
    // A build runs the project's own code (its latexmkrc): only where the workspace is trusted.
    if (!vscode.workspace.isTrusted) {
        void vscode.window.showWarningMessage('satex: compile runs the project\'s latexmkrc; trust this workspace to build it.');
        return;
    }
    const { root, dir } = rootFile(editor);
    buildDiagnostics.clear();
    build.clear();
    build.show(true);
    const files = await new Promise<BuildFile[]>(resolve => {
        const child = spawn(binaryPath, ['compile', '-f', root, '--format', 'lsp'], { cwd: dir });
        let stdout = '';
        child.stdout.on('data', (data: Buffer) => { stdout += data.toString(); });
        child.stderr.on('data', (data: Buffer) => build.append(data.toString()));
        child.on('error', error => { build.appendLine(String(error)); resolve([]); });
        child.on('close', () => {
            try { resolve(JSON.parse(stdout) as BuildFile[]); } catch { resolve([]); }
        });
    });
    // The server's own conversion: the same ranges, severities and codes as its findings.
    const convert = client?.protocol2CodeConverter;
    for (const file of files) {
        const diagnostics = (await convert?.asDiagnostics(file.diagnostics as never)) ?? [];
        diagnostics.forEach(d => { d.source = 'satex build'; });
        buildDiagnostics.set(vscode.Uri.parse(file.uri), diagnostics);
    }
}

/**
 * The server, started at a lower priority and with a memory limit
 * (`satex.server.maxMemory`), so an analysis never slows or starves the
 * editor; it builds the kernel cache itself on first use.
 */
function startServer(binary: string): Promise<StreamInfo> {
    const config = vscode.workspace.getConfiguration('satex');
    const memory = config.get<string>('server.maxMemory', '2GiB');
    // How long an analysis may run is the project's `limits.seconds` (satex.yaml), not a second setting here.
    const args = ['lsp', ...(memory ? ['--max-memory', memory] : [])];
    // On Linux a cgroup caps the server whatever it does: memory without swap and two cores, so
    // the editor and the desktop stay usable. The in-process budget above still ends a run cleanly first.
    const cores = config.get<number>('server.maxCores', 2);
    const capped = process.platform === 'linux' && config.get<boolean>('server.cgroup', true) && memory
        && !spawnSync('systemd-run', ['--user', '--version']).error;
    const limits = ['-p', `MemoryMax=${memory.replace(/iB$/, '')}`, '-p', 'MemorySwapMax=0', ...(cores > 0 ? ['-p', `CPUQuota=${cores * 100}%`] : [])];
    const [command, argv] = capped ? ['systemd-run', ['--user', '--scope', '--quiet', ...limits, binary, ...args]] : [binary, args];
    const child = spawn(command, argv);
    if (child.pid !== undefined) {
        try { os.setPriority(child.pid, config.get<number>('server.niceness', 10)); } catch { /* not ours to lower */ }
    }
    child.stderr.on('data', (data: Buffer) => client?.outputChannel.append(data.toString()));
    return Promise.resolve({ reader: child.stdout, writer: child.stdin, detached: false });
}

/**
 * LaTeX Workshop lints with chktex too: offer once to leave linting to satex,
 * so a finding is not reported twice. Its build, viewer and snippets stay.
 */
async function harmonize(context: vscode.ExtensionContext): Promise<void> {
    const workshop = vscode.workspace.getConfiguration('latex-workshop');
    const key = 'satex.askedAboutChktex';
    if (!vscode.extensions.getExtension('James-Yu.latex-workshop') || context.globalState.get(key)
        || !workshop.get<boolean>('linting.chktex.enabled')) {
        return;
    }
    await context.globalState.update(key, true);
    const off = 'Turn off chktex';
    const answer = await vscode.window.showInformationMessage(
        'LaTeX Workshop lints with chktex, and satex reports the same and more. Turn LaTeX Workshop\'s chktex linting off?',
        off, 'Keep both');
    if (answer === off) {
        await workshop.update('linting.chktex.enabled', false, vscode.ConfigurationTarget.Global);
    }
}

/** Which kernel, class and package caches the active document's last analysis reused or rebuilt. */
async function showCaches(): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (!client || !editor) { return; }
    const lines = await client.sendRequest<string[]>('satex/caches', { textDocument: { uri: editor.document.uri.toString() } });
    await vscode.window.showQuickPick(lines.length ? lines : ['No analysis of this document yet.'],
        { title: `satex caches: ${path.basename(editor.document.uri.fsPath)}`, placeHolder: 'reused caches load in milliseconds; rebuilt ones are kept for the next run' });
}

export function activate(context: vscode.ExtensionContext): void {
    const binary = resolveBinary(context);
    if (!binary) {
        void vscode.window.showErrorMessage(`satex binary not found. ${INSTALL_HINT}`);
        return;
    }
    binaryPath = binary;
    const status = new Status();
    // The findings live in the Problems view. While a newer analysis runs they
    // are hidden (`satex.findings.whileAnalyzing`: `hide`) or marked as old (`mark`).
    const published = new Map<string, [vscode.Uri, vscode.Diagnostic[]]>();
    let analyzing = false;
    const shown = (diagnostics: vscode.Diagnostic[]): vscode.Diagnostic[] => {
        if (!analyzing) { return diagnostics; }
        if (vscode.workspace.getConfiguration('satex').get<string>('findings.whileAnalyzing', 'hide') === 'hide') { return []; }
        return diagnostics.map(d => Object.assign(new vscode.Diagnostic(d.range, d.message, d.severity),
            { code: d.code, tags: d.tags, relatedInformation: d.relatedInformation, data: (d as { data?: unknown }).data, source: STALE }));
    };
    client = new LanguageClient(
        'satex', 'satex', () => startServer(binary),
        {
            documentSelector: ['latex', 'tex', 'bibtex'].map(language => ({ language })),
            middleware: {
                handleDiagnostics: (uri, diagnostics, next) => {
                    published.set(uri.toString(), [uri, diagnostics]);
                    next(uri, shown(diagnostics));
                },
            },
        });
    const working = (work: Work) => {
        status.working(work);
        // A hover's trial run changes no finding.
        const now = Boolean(work.text) && work.phase !== 'probe';
        if (now === analyzing) { return; }
        analyzing = now;
        for (const [uri, diagnostics] of published.values()) { client?.diagnostics?.set(uri, shown(diagnostics)); }
    };
    context.subscriptions.push(
        trace, build, buildDiagnostics, status, steps,
        vscode.commands.registerCommand('satex.traceNext', () => steps.move(1)),
        vscode.commands.registerCommand('satex.tracePrevious', () => steps.move(-1)),
        vscode.commands.registerCommand('satex.tracePick', () => steps.pick()),
        vscode.commands.registerCommand('satex.traceClose', () => steps.clear()),
        vscode.window.registerTreeDataProvider('satex.actions', new Actions()),
        vscode.commands.registerCommand('satex.traceAtCursor', traceAtCursor),
        vscode.commands.registerCommand('satex.compile', compile),
        vscode.commands.registerCommand('satex.focus', () => vscode.commands.executeCommand('workbench.actions.view.problems')),
        vscode.commands.registerCommand('satex.caches', showCaches),
        vscode.commands.registerCommand('satex.restart', () => client?.restart()),
        vscode.commands.registerCommand('satex.showOutput', () => client?.outputChannel.show(true)),
        vscode.commands.registerCommand('satex.openSettings', () => vscode.commands.executeCommand('workbench.action.openSettings', `@ext:${context.extension.id}`)),
        client.onNotification('satex/status', working),
        client.onDidChangeState(e => status.down(e.newState === State.Stopped)));
    checkTeX();
    void client.start();
    void harmonize(context);
}
export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
