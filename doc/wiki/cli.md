# Command line

This page is for what every `satex` command shares: input, configuration, output and exit codes. It also covers shell completion and the local playground. `satex --help` and `satex COMMAND --help` list the flags. [Lint rules](lints.md) and [queries](queries.md) describe what the commands print. The [configuration page](configuration.md) describes `satex.yaml`.

## Rules every command shares

Every `satex` command follows the same rules for its input, its configuration and its output. A command differs only where its job does: `lint` shows early findings, `satex cache` reads no document.

### Input

- `-f FILE` names the document. `-f -` reads it from stdin.
- Without `-f`, a document piped in is read. An empty stdin (`</dev/null`, as cron, CI and editors start a program) counts as nothing piped.
- With nothing piped, each root document in the current directory is analyzed in turn. Files no root reads are named on stderr.
- `explain` and `scope` with no root, or several, analyze the smallest LaTeX document instead and say so on stderr.
- A stdin that stays open and silent for two seconds gets one note on stderr. satex never waits unexplained.
- `satex cache` and its subcommands never read stdin or a project's document. `-f` still names one for `satex cache`.
- `query --request -` reads the request from stdin.

### Configuration

Every command reads the same layers: built-in defaults, the global config, each `satex.yaml` from the filesystem root down to the document's directory (the current one without `-f`), `--config FILE`, then `--set PATH=VALUE`. `--no-config` drops every file. `--profile` picks the defaults ([profiles](configuration.md#profiles)). `satex --version` lists the files in effect. A file that does not read is named once, by its full path. `satex gui` hands its config flags to every run it starts. `satex completions` reads no config, since nothing it prints depends on one.

### Output

- Results go to stdout. Progress, notes, early findings and the one-line package cache summary go to stderr.
- `--format` names what a command prints. A command given a format it cannot print stops with an error that names the ones it can. `satex cache` and its subcommands print text or json. `completions`, `lsp` and `gui` print text only.
- Progress shows only in text format, on a terminal.
- Every command that analyzes a document says on stderr how many package caches it reused or built.
- The first-start question about background caches comes only when stdin, stdout and stderr are terminals. `satex cache`, `lsp`, `gui` and `completions` never ask. `install`, `query` and `trace -i` read the terminal themselves, so they do not ask either.

### Exit codes

- `0`: the command ran. Findings, and a failed `compile`, still exit 0.
- `1`: the command could not run. The message on stderr starts with `satex:`.
- `2`: the command line did not parse.

## Shell completion

`satex completions install` sets completion up for the current shell (bash, zsh or fish). It adds one line to the shell's startup file, and running it again changes nothing.

`satex completions <shell>` prints a completion script to stdout. The shell can be `bash`, `zsh`, `fish`, `elvish` or `powershell`.

The script asks `satex` for completions each time you press Tab. So the lists are always those of the installed version:

- `satex query <Tab>` lists the query names, the same as `satex query --list`.
- `satex --set <Tab>` lists the keys of `satex.yaml`, as dotted paths such as `limits.steps=`.
- Flags, subcommands and fixed values complete as usual. Files complete as files.

### bash

Add this line to `~/.bashrc`:

```sh
source <(satex completions bash)
```

### zsh

Add this line to `~/.zshrc`, after `compinit`:

```sh
source <(satex completions zsh)
```

### fish

```sh
satex completions fish > ~/.config/fish/completions/satex.fish
```

## The playground on your machine

`satex gui [DIR]` opens the browser playground on the project in `DIR`, the current directory by default. A file names the root document, its folder the project.
Without a file the root is the document `satex` itself would start from in `DIR`.
Only the file shown and the files it reads are analyzed, the file shown first. Another document of the folder runs once you open it.
It differs from the web playground in four ways:

- The project's files are the files in `DIR`. Hidden and git-ignored files are left out. Edits are saved to the files as you type.
- The TeX packages are those installed on your computer, found by kpathsea. Nothing is downloaded.
- The language server is the native `satex`, not the WebAssembly build. It is faster and reads the project's `satex.yaml`. The config flags given to `satex gui` (`--set`, `--config`, …) reach it too.
- Compile builds the PDF with your TeX, in a copy of the project under the cache directory. The web playground's Compile runs pdfTeX in the browser, with TeX Live files fetched on demand.

`satex gui` prints the address and opens it in the default browser. `--no-open` only prints it. `--port N` picks the port, 8800 by default, or the next free one above it.
The terminal names the document each run analyzes. The language server's messages go to `satex-gui.log` in the cache directory. `-v` prints them in the terminal instead.
The server listens on 127.0.0.1 only. The address carries a random token, and the server answers only a browser that has it. Other users and other web sites cannot reach the project.
A file that is not UTF-8 is never overwritten, and a file with `\r\n` line ends keeps them.

The pages come from a built site: `share/satex/site` beside the binary, or `--site DIR`. In a checkout of satex, `cargo playground` builds one in `target/site`, which `satex gui` finds by itself.

## Command tree as JSON

`satex completions json` prints the subcommands, flags, value names, possible values, query names and config keys as JSON. It is hidden from `--help`. Tools such as the web playground use it.

## Benchmarks

`python3 scripts/bench.py --build` builds the release binary, runs the documents of `benches/docs.txt` under every kernel and writes the entry for the current version to `doc/site/bench/data.json`. Entries of other versions stay. `cargo playground` then serves the Benchmarks page from that file, and shows a hint while it is missing. `cargo bench --bench analysis` runs the Criterion micro-benchmarks. CI appends an entry on each release.

`python3 scripts/bench_compile.py` compares `satex compile --system satex` with latexmk and the plain engine on the documents of `benches/compile.txt` and, with `--big article-200,book-1000`, on generated ones. It measures a cold build, a rebuild with nothing changed and a rebuild after one changed word. For each it records the median wall time of N runs and its spread, CPU time, peak memory and engine passes. It also checks that each PDF has latexmk's page count and text. The clock is pinned for every tool. The entry goes to `doc/site/bench/compile.json`; the module docstring says which caches each scenario has.
