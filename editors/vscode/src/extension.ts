import * as vscode from 'vscode';
import { execFile, spawn, spawnSync } from 'child_process';
import { chmodSync, existsSync } from 'fs';
import * as os from 'os';
import * as path from 'path';
import { LanguageClient, State, StreamInfo } from 'vscode-languageclient/node';
import { openDialog } from './dialogs';
import { Revive } from './revive';
import { capMiB } from './memory';
import { binaryProblem, Cap, failure, Ran, startProblem, usableCaps, wrap } from './launch';
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

/** `command args`, waited for at most `ms`. */
function run(command: string, args: string[], ms = 10_000): Ran {
    const r = spawnSync(command, args, { timeout: ms, encoding: 'utf8' });
    return { error: r.error as NodeJS.ErrnoException | undefined, status: r.status, signal: r.signal, stderr: r.stderr };
}

/** The output "satex": how the server was found and started, then what it says. */
const serverLog = vscode.window.createOutputChannel('satex');
function log(line: string): void { serverLog.appendLine(line); }

/**
 * The first binary that runs here: the setting, the bundled one, `satex` on the PATH. A broken
 * one is skipped with a warning; none at all is an error that says how to get one.
 */
function resolveBinary(context: vscode.ExtensionContext): string | undefined {
    const configured = vscode.workspace.getConfiguration('satex').get<string>('path');
    const bundled = context.asAbsolutePath(path.join('bin', `${process.platform}-${process.arch}`, process.platform === 'win32' ? 'satex.exe' : 'satex'));
    const problems: string[] = [];
    const candidates: [string, string][] = [];
    if (configured) {
        if (existsSync(configured)) { candidates.push([configured, '`satex.path`']); } else { problems.push(`\`satex.path\` names ${configured}, which does not exist.`); }
    }
    if (existsSync(bundled)) { candidates.push([bundled, 'the bundled binary']); }
    candidates.push(['satex', '`satex` on the PATH']);
    for (const [binary, from] of candidates) {
        let r = run(binary, ['--version']);
        // a package that lost the executable bit: ours to restore
        if (binary === bundled && r.error?.code === 'EACCES') {
            try { chmodSync(bundled, 0o755); r = run(binary, ['--version']); } catch { /* reported below */ }
        }
        if (binary === 'satex' && r.error?.code === 'ENOENT') { continue; }
        const problem = binaryProblem(binary, r);
        if (!problem) {
            if (problems.length > 0) {
                log(`satex: using ${from} (${binary}); ${problems.join(' ')}`);
                void offer('warning', `SaTeX uses ${from}. ${problems.join(' ')}`, 'path', 'output');
            }
            return binary;
        }
        problems.push(`${from}: ${problem}`);
    }
    problems.forEach(p => log(`satex: ${p}`));
    void offer('error', `SaTeX cannot start: no satex binary runs here. ${problems.join(' ')} `
        + `Download one for ${process.platform}-${process.arch}, install it with \`cargo install satex-analyzer\`, or set \`satex.path\`.`, 'releases', 'path', 'output');
    return undefined;
}

type Fix = 'releases' | 'path' | 'output' | 'restart' | 'tex' | 'memory';
const FIXES: Record<Fix, [string, () => unknown]> = {
    releases: ['Download satex', () => vscode.env.openExternal(vscode.Uri.parse(RELEASES))],
    path: ['Set satex.path', () => vscode.commands.executeCommand('workbench.action.openSettings', 'satex.path')],
    output: ['Show Output', () => serverLog.show(true)],
    restart: ['Restart Server', () => restartServer()],
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
function checkTeX(): void {
    const missing = ['kpsewhich', 'pdflatex'].filter(tool => run(tool, ['--version']).error);
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

/** The caps that work here (probed once), and those that failed while starting the server. */
let caps: Cap[] | undefined;
const broken = new Set<Cap>();
/** What made the last start fail, shown once until the server runs. */
let lastProblem: string | undefined;
let told = false;
let crashTold = false;
/** A cap failed while starting: start again without it at once. */
let retry = false;

/**
 * The server, at a lower priority and with a memory limit (`satex.server.maxMemory`), so an
 * analysis never slows or starves the editor; it builds the kernel cache itself on first use.
 * Above the in-process budget, which ends a run cleanly first, a hard cap holds the whole
 * process: a systemd scope where a user bus runs, else `ulimit -d`, else only the budget.
 */
function startServer(binary: string): Promise<StreamInfo> {
    const config = vscode.workspace.getConfiguration('satex');
    const memory = config.get<string>('server.maxMemory', '2GiB');
    const cores = config.get<number>('server.maxCores', 2);
    // How long an analysis may run is the project's `limits.seconds` (satex.yaml), not a second setting here.
    const args = ['lsp', ...(memory ? ['--max-memory', memory] : []), ...(cores > 0 ? ['--threads', String(cores)] : [])];
    const mib = memory ? capMiB(memory) : undefined;
    if (!caps) {
        const found = process.platform === 'linux'
            ? usableCaps((c, a) => run(c, a, 5_000), mib, cores, config.get<boolean>('server.cgroup', true))
            : { caps: ['plain'] as Cap[], skipped: [] };
        caps = found.caps;
        found.skipped.forEach(s => log(`satex: no ${s}`));
        if (process.platform === 'linux' && mib !== undefined && caps[0] === 'plain') {
            void offer('warning', 'SaTeX runs without a hard memory cap here (no systemd user scope, no `ulimit`). '
                + 'Its own budget `satex.server.maxMemory` still ends an analysis that grows past it.', 'output', 'memory');
        }
    }
    retry = false;
    const cap = caps.find(c => !broken.has(c)) ?? 'plain';
    const [command, argv] = wrap(cap, [binary, ...args], mib ?? 0, cores);
    log(`satex: starting ${binary} (${cap === 'plain' ? 'no hard cap' : `capped through ${cap}`})`);
    const child = spawn(command, argv);
    if (child.pid !== undefined) {
        try { os.setPriority(child.pid, config.get<number>('server.niceness', 10)); } catch { /* not ours to lower */ }
    }
    let answered = false;
    let stderr = '';
    child.stdout.once('data', () => { answered = true; });
    child.stderr.on('data', (data: Buffer) => {
        stderr = (stderr + data.toString()).slice(-4096);
        log(data.toString().trimEnd());
    });
    let ended = false;
    const failed = (code: number | null, signal: string | null, error?: Error) => {
        if (ended) { return; }
        ended = true;
        // a crash after it ran: say so once, the client starts it again
        if (answered) {
            if ((code ?? 0) !== 0 || (signal && signal !== 'SIGTERM')) {
                const why = startProblem(code, signal, stderr);
                log(`satex: the server stopped: ${why}`);
                if (!crashTold) { crashTold = true; void offer('warning', `SaTeX: the server stopped (${why}). It starts again.`, 'output', 'memory'); }
            }
            return;
        }
        const why = error ? (failure({ error, status: null }) ?? String(error)) : startProblem(code, signal, stderr);
        if (cap !== 'plain') {
            broken.add(cap);
            retry = true;
            log(`satex: starting through ${cap} failed (${why}); trying without it`);
            return;
        }
        lastProblem = `The satex server stopped before it answered: ${why}`;
        log(`satex: ${lastProblem}`);
    };
    child.on('error', error => failed(null, null, error));
    child.on('exit', (code, signal) => failed(code, signal));
    return Promise.resolve({ reader: child.stdout, writer: child.stdin, detached: false });
}

/** The server changed state: report a failed start once, and give up loudly. */
function stateChanged(state: State, revive: Revive): void {
    if (state === State.Running) { lastProblem = undefined; told = false; retry = false; return; }
    if (state !== State.Stopped) { return; }
    if (retry) { retry = false; void client?.start().catch(() => undefined); return; }
    if (revive.exhausted) {
        void offer('error', `SaTeX stopped and stays off: it failed several times. ${lastProblem ?? 'See the output for why.'}`, 'output', 'restart', 'memory');
    } else if (lastProblem && !told) {
        told = true;
        void offer('error', `SaTeX: ${lastProblem} It starts again in a minute.`, 'output', 'restart', 'path');
    }
}

/** A start by hand: probe the caps again and forget earlier failures. */
function restartServer(): void {
    caps = undefined;
    broken.clear();
    told = false;
    void (client?.state === State.Stopped ? client.start() : client?.restart())?.catch(() => undefined);
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
    const reload = 'Reload Window';
    if (await vscode.window.showInformationMessage('SaTeX: reload the window to stop the removed copy.', reload) === reload) {
        await vscode.commands.executeCommand('workbench.action.reloadWindow');
    }
}

export function activate(context: vscode.ExtensionContext): void {
    const others = peers(vscode.extensions.all, context.extension.id);
    if (others.length > 0) {
        void warnPeers(context, others);
        // a local build gives way to the published extension
        if (yields(context.extension.id, others)) { return; }
    }
    const binary = resolveBinary(context);
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
    const revive = new Revive(() => { trace.appendLine('satex: the server stopped; starting it again'); void client?.restart(); }, 60_000);
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
        trace, build, buildDiagnostics, serverLog, bar, edited, { dispose: () => { status = undefined; } }, steps,
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
            bar.down(e.newState === State.Stopped);
            if (!(retry && e.newState === State.Stopped)) { revive.stopped(e.newState === State.Stopped); }
            stateChanged(e.newState, revive);
        }),
        { dispose: () => revive.dispose() });
    checkTeX();
    // a failed start is reported by `stateChanged`, not as an unhandled rejection
    void client.start().catch(error => log(`satex: ${String(error)}`));
    void harmonize(context);
}
export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
