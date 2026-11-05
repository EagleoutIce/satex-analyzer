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

## Command tree as JSON

`satex completions json` prints the subcommands, flags, value names, possible values, query names and config keys as JSON. It is hidden from `--help`. Tools such as the web playground use it.

## Benchmarks

`python3 scripts/bench.py --build` builds the release binary, runs the documents of `benches/docs.txt` under every kernel and writes the entry for the current version to `doc/site/bench/data.json`. Entries of other versions stay. `cargo playground` then serves the Benchmarks page from that file, and shows a hint while it is missing. `cargo bench --bench analysis` runs the Criterion micro-benchmarks. CI appends an entry on each release.
