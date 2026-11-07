# Command line

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
- The language server is the native `satex`, not the WebAssembly build. It is faster and reads the project's `satex.yaml`.
- Compile builds the PDF with your TeX, in a copy of the project under the cache directory.

`satex gui` prints the address and opens it in the default browser. `--no-open` only prints it. `--port N` picks the port, 8800 by default, or the next free one above it.
The terminal names the document each run analyzes. The language server's messages go to `satex-gui.log` in the cache directory. `-v` prints them in the terminal instead.
The server listens on 127.0.0.1 only. The address carries a random token, and the server answers only a browser that has it. Other users and other web sites cannot reach the project.
A file that is not UTF-8 is never overwritten, and a file with `\r\n` line ends keeps them.

The pages come from a built site: `share/satex/site` beside the binary, or `--site DIR`. In a checkout of satex, `cargo playground` builds one in `target/site`, which `satex gui` finds by itself.

## Command tree as JSON

`satex completions json` prints the subcommands, flags, value names, possible values, query names and config keys as JSON. It is hidden from `--help`. Tools such as the web playground use it.

## Benchmarks

`python3 scripts/bench.py --build` builds the release binary, runs the documents of `benches/docs.txt` under every kernel and writes the entry for the current version to `doc/site/bench/data.json`. Entries of other versions stay. `cargo playground` then serves the Benchmarks page from that file, and shows a hint while it is missing. `cargo bench --bench analysis` runs the Criterion micro-benchmarks. CI appends an entry on each release.
