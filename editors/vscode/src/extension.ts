import * as vscode from 'vscode';
import { execFile, spawn } from 'child_process';
import { existsSync } from 'fs';
import * as os from 'os';
import * as path from 'path';
import { CloseAction, ErrorHandler, LanguageClient, ResponseError, State, StreamInfo } from 'vscode-languageclient/node';
import { openDialog } from './dialogs';
import { Revive } from './revive';
import { capMiB } from './memory';
import { Candidate, findBinary, Launcher, run } from './launch';
import { peers, uninstallCommand, yields } from './peers';
import { Actions } from './sidebar';
import { Findings, Mode, moved, Range } from './findings';
import { STALE, Status, Work } from './status';
import { Steps, TraceEvent } from './steps';
import { library } from './library';
import { settings } from './settings';
import { manage, openView, ViewActions } from './views';
import { widgets } from './widgets';

const RELEASES = 'https://github.com/EagleoutIce/satex-analyzer/releases';
let client: LanguageClient | undefined;
/** The window's one status item. */
let status: Status | undefined;
let binaryPath = '';
const trace = vscode.window.createOutputChannel('satex trace');
const build = vscode.window.createOutputChannel('satex build');
const buildDiagnostics = vscode.languages.createDiagnosticCollection('satex build');
const steps = new Steps();

/** The output "satex": how the server was found and started, then what it says. */
const serverLog = vscode.window.createOutputChannel('satex');
function log(line: string): void { serverLog.appendLine(line); }

/**
 * The first binary that runs here: the setting, the bundled one, `satex` on the PATH. A broken
 * one is skipped with a warning; none at all is an error that says how to get one.
 */
async function resolveBinary(context: vscode.ExtensionContext): Promise<string | undefined> {
    const configured = vscode.workspace.getConfiguration('satex').get<string>('path');
    const bundled = context.asAbsolutePath(path.join('bin', `${process.platform}-${process.arch}`, process.platform === 'win32' ? 'satex.exe' : 'satex'));
    const candidates: Candidate[] = [
        ...(configured ? [{ binary: configured, from: '`satex.path`' }] : []),
        ...(existsSync(bundled) ? [{ binary: bundled, from: 'the bundled binary', ours: true }] : []),
        { binary: 'satex', from: '`satex` on the PATH' },
    ];
    const { binary, from, problems } = await findBinary(candidates, run);
    problems.forEach(p => log(`satex: ${p}`));
    if (binary) {
        log(`satex: using ${from} (${binary})`);
        if (problems.length > 0) { void offer('warning', `SaTeX uses ${from}. ${problems.join(' ')}`, 'path', 'output'); }
        return binary;
    }
    void offer('error', `SaTeX cannot start: no satex binary runs here. ${problems.join(' ')} `
        + `Download one for ${process.platform}-${process.arch}, install it with \`cargo install satex-analyzer\`, or set \`satex.path\`.`, 'releases', 'path', 'output');
    return undefined;
}

type Fix = 'releases' | 'path' | 'output' | 'restart' | 'reload' | 'tex' | 'memory';
const FIXES: Record<Fix, [string, () => unknown]> = {
    releases: ['Download satex', () => vscode.env.openExternal(vscode.Uri.parse(RELEASES))],
    path: ['Set satex.path', () => vscode.commands.executeCommand('workbench.action.openSettings', 'satex.path')],
    output: ['Show Output', () => serverLog.show(true)],
    restart: ['Restart Server', () => restartServer()],
    reload: ['Reload Window', () => vscode.commands.executeCommand('workbench.action.reloadWindow')],
    tex: ['Install TeX Live', () => vscode.env.openExternal(vscode.Uri.parse('https://tug.org/texlive/quickinstall.html'))],
    memory: ['Memory Settings', () => vscode.commands.executeCommand('workbench.action.openSettings', 'satex.server')],
};

/** A message with buttons that fix what it reports. */
async function offer(level: 'error' | 'warning' | 'info', message: string, ...fixes: Fix[]): Promise<void> {
    const show = { error: vscode.window.showErrorMessage, warning: vscode.window.showWarningMessage, info: vscode.window.showInformationMessage }[level];
    const picked = await show(message, ...fixes.map(f => FIXES[f][0]));
    const fix = fixes.find(f => FIXES[f][0] === picked);
    if (fix) { await FIXES[fix][1](); }
}

/** satex reads the installed TeX distribution; warn when it is missing. */
async function checkTeX(): Promise<void> {
    const ran = await Promise.all(['kpsewhich', 'pdflatex'].map(async tool => [tool, await run(tool, ['--version'], 10_000)] as const));
    const missing = ran.filter(([, r]) => r.error?.code === 'ENOENT').map(([tool]) => tool);
    if (missing.length > 0) {
        void offer('warning', `SaTeX uses your TeX distribution, but ${missing.join(' and ')} ${missing.length > 1 ? 'are' : 'is'} not on the PATH`
            + `${vscode.env.remoteName ? ` of the ${vscode.env.remoteName} host` : ''}. Install TeX Live, MiKTeX or MacTeX for full analysis; without it packages are not read.`, 'tex');
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

/** Messages about the server, each kind shown once until a restart by hand. */
const shown = new Set<string>();
function once(kind: string, level: 'error' | 'warning', message: string, ...fixes: Fix[]): void {
    if (!shown.has(kind)) { shown.add(kind); void offer(level, message, ...fixes); }
}

const launcher = new Launcher({
    log,
    uncapped: () => void offer('warning', 'SaTeX runs without a hard memory cap here (no systemd user scope, no `ulimit`). '
        + 'Its own budget `satex.server.maxMemory` still ends an analysis that grows past it.', 'output', 'memory'),
    failedStart: (problem, cap, fallback) => {
        log(`satex: the server stopped before it answered: ${problem}${fallback ? `; starting it without ${cap}` : ''}`);
        if (!fallback) { once('failed', 'error', `SaTeX: the server stopped before it answered: ${problem}`, 'output', 'restart', 'path'); }
    },
    crashed: problem => {
        log(`satex: the server stopped: ${problem}`);
        once('crashed', 'warning', `SaTeX: the server stopped (${problem}). It starts again.`, 'output', 'memory');
    },
});

/**
 * The server, at a lower priority and with a memory limit (`satex.server.maxMemory`), so an
 * analysis never slows or starves the editor; it builds the kernel cache itself on first use.
 * Above the in-process budget, which ends a run cleanly first, a hard cap holds the whole
 * process: a systemd scope where a user bus runs, else `ulimit -d`, else only the budget.
 */
async function startServer(binary: string): Promise<StreamInfo> {
    const config = vscode.workspace.getConfiguration('satex');
    const memory = config.get<string>('server.maxMemory', '2GiB');
    const cores = config.get<number>('server.maxCores', 2);
    // How long an analysis may run is the project's `limits.seconds` (satex.yaml), not a second setting here.
    const args = ['lsp', ...(memory ? ['--max-memory', memory] : []), ...(cores > 0 ? ['--threads', String(cores)] : [])];
    const { child } = await launcher.start(binary, args, {
        mib: memory ? capMiB(memory) : undefined, cores, scope: config.get<boolean>('server.cgroup', true), linux: process.platform === 'linux',
    });
    if (child.pid !== undefined) {
        try { os.setPriority(child.pid, config.get<number>('server.niceness', 10)); } catch { /* not ours to lower */ }
    }
    return { reader: child.stdout!, writer: child.stdin!, detached: false };
}

/**
 * The client's own restarts (five in three minutes), and at once after a dropped cap. When it
 * gives up, `Revive` takes over, so its "will not be restarted" stays unsaid.
 */
let standardHandler: ErrorHandler | undefined;
const standard = () => standardHandler ??= client!.createDefaultErrorHandler(4);
const errorHandler: ErrorHandler = {
    error: (error, message, count) => standard().error(error, message, count),
    closed: async () => {
        if (launcher.takeDropped()) { return { action: CloseAction.Restart, handled: true }; }
        const result = await standard().closed();
        return result.action === CloseAction.DoNotRestart ? { action: CloseAction.DoNotRestart, handled: true } : result;
    },
};

/** A start by hand: probe the caps again and forget earlier failures. */
function restartServer(): void {
    launcher.reset();
    shown.clear();
    void (client?.state === State.Stopped ? client.start() : client?.restart())?.catch(() => undefined);
}

/** A changed path needs a new window, changed server limits a new server. */
function configChanged(e: vscode.ConfigurationChangeEvent): void {
    if (e.affectsConfiguration('satex.path')) {
        void offer('info', 'SaTeX: reload the window to use the new `satex.path`.', 'reload');
    } else if (e.affectsConfiguration('satex.server')) {
        launcher.reset();
        void offer('info', 'SaTeX: restart the server to apply its new limits.', 'restart');
    }
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

/** A command; one another copy of SaTeX holds already stays its, and this activation goes on. */
function command(id: string, run: (...args: never[]) => unknown): vscode.Disposable {
    try {
        return vscode.commands.registerCommand(id, run);
    } catch (error) {
        trace.appendLine(`satex: ${String(error)}`);
        return new vscode.Disposable(() => undefined);
    }
}

/** Once per set of other SaTeX copies: say they double the server and offer to uninstall them. */
async function warnPeers(context: vscode.ExtensionContext, others: string[]): Promise<void> {
    const key = `satex.warnedPeers:${others.join(',')}`;
    if (context.globalState.get(key)) { return; }
    await context.globalState.update(key, true);
    const remove = others.length === 1 ? `Uninstall ${others[0]}` : 'Uninstall the others';
    const answer = await vscode.window.showWarningMessage(
        `SaTeX is installed more than once (${others.join(', ')} besides ${context.extension.id}): each copy starts a server and a status item. `
        + `Uninstall the others? From a terminal: ${others.map(uninstallCommand).join('; ')}`, remove, 'Keep');
    if (answer !== remove) { return; }
    for (const id of others) { await vscode.commands.executeCommand('workbench.extensions.uninstallExtension', id); }
    await offer('info', 'SaTeX: reload the window to stop the removed copy.', 'reload');
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    const others = peers(vscode.extensions.all, context.extension.id);
    if (others.length > 0) {
        void warnPeers(context, others);
        // a local build gives way to the published extension
        if (yields(context.extension.id, others)) { return; }
    }
    context.subscriptions.push(serverLog, vscode.workspace.onDidChangeConfiguration(configChanged));
    const binary = await resolveBinary(context);
    if (!binary) { return; }
    binaryPath = binary;
    const bar = (status ??= new Status());
    // What the Problems view shows while a run goes (`satex.findings.whileAnalyzing`).
    const findings = new Findings<vscode.Diagnostic>(
        () => vscode.workspace.getConfiguration('satex').get<Mode>('findings.whileAnalyzing', 'hide'),
        d => Object.assign(new vscode.Diagnostic(d.range, d.message, d.severity),
            { code: d.code, tags: d.tags, relatedInformation: d.relatedInformation, data: (d as { data?: unknown }).data, source: STALE }),
        (uri, diagnostics) => client?.diagnostics?.set(vscode.Uri.parse(uri), diagnostics));
    client = new LanguageClient(
        'satex', 'satex', () => startServer(binary),
        {
            documentSelector: ['latex', 'tex', 'bibtex'].map(language => ({ language })),
            outputChannel: serverLog,
            errorHandler,
            // A dead server is explained by the launcher when it exits; the client would only say
            // "write EPIPE". An error the server answered with is its own to show.
            initializationFailedHandler: error => {
                log(`satex: initialize failed: ${String(error)}`);
                if (error instanceof ResponseError) { once('init', 'error', `SaTeX: the server did not start: ${error.message}`, 'output', 'restart'); }
                return false;
            },
            middleware: {
                handleDiagnostics: (uri, diagnostics) => findings.publish(uri.toString(), diagnostics),
                // a hover's "Edit table…" link runs `satex.openDialog`; the units hover's values write themselves (`satex.applyEdit`)
                provideHover: async (document, position, token, next) => {
                    const hover = await next(document, position, token);
                    for (const c of hover?.contents ?? []) {
                        if (c instanceof vscode.MarkdownString) { c.isTrusted = { enabledCommands: ['satex.openDialog', 'satex.applyEdit'] }; }
                    }
                    return hover;
                },
            },
        });
    const revive = new Revive(() => {
        log('satex: the server stopped; starting it again');
        void client?.restart().catch(error => log(`satex: ${String(error)}`));
    }, 60_000);
    const working = (work: Work) => {
        bar.working(work);
        findings.status(work);
    };
    // An edit moves what is held back as the editor moves what it shows.
    const edited = vscode.workspace.onDidChangeTextDocument(e => findings.move(e.document.uri.toString(), d => {
        const range = e.contentChanges.reduce<Range>(moved, d.range);
        d.range = new vscode.Range(range.start.line, range.start.character, range.end.line, range.end.character);
        return d;
    }));
    context.subscriptions.push(
        trace, build, buildDiagnostics, bar, edited, { dispose: () => { status = undefined; } }, steps,
        command('satex.traceNext', () => steps.move(1)),
        command('satex.tracePrevious', () => steps.move(-1)),
        command('satex.tracePick', () => steps.pick()),
        command('satex.traceClose', () => steps.clear()),
        vscode.window.registerTreeDataProvider('satex.actions', new Actions()),
        command('satex.traceAtCursor', traceAtCursor),
        command('satex.compile', compile),
        command('satex.focus', () => vscode.commands.executeCommand('workbench.actions.view.problems')),
        command('satex.caches', showCaches),
        // the table and image dialogs (src/lsp/dialogs.rs): `satex/dialog` in a webview
        command('satex.openDialog', (arg) => openDialog(context, client, arg)),
        command('satex.restart', restartServer),
        command('satex.showOutput', () => serverLog.show(true)),
        // type plugins' dialogs (color picker, length converter, drawing): the server's `satex/view` in a webview
        command('satex.applyEdit', async (uri: string, range: vscode.Range, newText: string) => {
            const edit = new vscode.WorkspaceEdit();
            edit.replace(vscode.Uri.parse(uri), new vscode.Range(range.start.line, range.start.character, range.end.line, range.end.character), newText);
            await vscode.workspace.applyEdit(edit);
        }),
        command('satex.openView', (uri?: vscode.Uri | string, at?: vscode.Position, plugin?: string) => openView(context, client, uri, at, plugin)),
        vscode.languages.registerCodeActionsProvider(['latex', 'tex'].map(language => ({ language })), new ViewActions(() => client)),
        command('satex.manage', (kind?: string) => manage(context, client, kind)),
        command('satex.library', () => library(context, client)),
        command('satex.projectSettings', () => settings(context)),
        command('satex.openWidget', () => widgets(client)),
        command('satex.openSettings', () => vscode.commands.executeCommand('workbench.action.openSettings', `@ext:${context.extension.id}`)),
        client.onNotification('satex/status', working),
        client.onNotification('satex/settled', async (p: { uri: string; diagnostics: never[] }) =>
            findings.settled(p.uri, await client?.protocol2CodeConverter.asDiagnostics(p.diagnostics) ?? [])),
        // the client gives up on a server that closed five times in three minutes: try again after a minute
        client.onDidChangeState(e => {
            const stopped = e.newState === State.Stopped;
            bar.down(stopped, launcher.problem);
            // a failed initialize stops the client without `closed`: a dropped cap starts again at once
            if (stopped && launcher.takeDropped()) { void client?.start().catch(error => log(`satex: ${String(error)}`)); return; }
            revive.stopped(stopped);
            if (stopped && revive.exhausted) {
                once('off', 'error', `SaTeX stopped and stays off after several failed starts: ${launcher.problem ?? 'see the output'}.`, 'output', 'restart', 'memory');
            }
        }),
        { dispose: () => revive.dispose() });
    void checkTeX();
    // a failed start is reported by the launcher, not as an unhandled rejection
    void client.start().catch(error => log(`satex: ${String(error)}`));
    void harmonize(context);
}
export async function deactivate(): Promise<void> {
    await client?.stop().catch(() => undefined);
    launcher.stop();
}
