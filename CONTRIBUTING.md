# Thank you for every contribution

Just select and or create an issue, work on that in your own fork and open a pull-request!

Run `scripts/setup-hooks.sh` once per clone. It points git at `.githooks`, which checks your commit message, keeps conflict markers and stray `dbg!` calls out of a commit, and runs `cargo fmt --all --check` and clippy before a push, which CI checks too.

Commit messages follow the conventional form `type(scope): subject`, with a lowercase subject and no full stop. The types are:

```
build  builds  ci  doc  docs  feat  perf  refactor  test  tests  dep  deps  lint  wip  meta
```

There is no bare `fix`. A fix carries the type of what it fixes, so `feat-fix`, `doc-fix`, `ci-fix` and so on. A breaking change adds a `!`, as in `feat!: drop the old flag`.

If you have any questions, just send me a mail at: <florian.sihler@uni-ulm.de>.
