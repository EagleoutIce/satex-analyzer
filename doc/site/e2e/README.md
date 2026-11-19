# Playground e2e suite

Browser tests of the playground: typing in the PDF and Typeset views, clicks, math, widgets, completion, double clicks, the footer, the follow mark, page colours, TikZ menus and crash recovery. Each scenario runs in a fresh headless Chromium context against `satex-playground --dev`, which serves `doc/site` from the sources.

The unit tests without a browser live in `doc/site/tests` (`node doc/site/views.test.mjs`). A behavior that a unit test can pin goes there. A scenario here checks what needs the page: the frames drawn, the caret on screen, a build landing, the worker.

## Running

```sh
cargo playground                  # once: builds target/playground/satex-playground and target/site
scripts/e2e.sh quick              # the push check, about 5 min
scripts/e2e.sh full               # every scenario
scripts/e2e.sh perf               # timing bounds as failures
scripts/e2e.sh full --area math,clicks
scripts/e2e.sh full --only caption,bold --repeat 3
```

`scripts/e2e.sh` starts a server on the first free port of 8472–8479, waits until it answers, runs `run.mjs` and stops the server by its PID. With a server already running, call the runner directly:

```sh
node doc/site/e2e/run.mjs --port 8472 --tier quick
```

| option | meaning |
|---|---|
| `--tier quick\|full\|perf` | which scenarios (`scenarios/index.mjs` `TIERS`), default quick |
| `--only a,b` | these scenarios, whatever the tier |
| `--area a,b` | the scenarios of these areas (file names in `scenarios/`) |
| `--repeat N` | N rounds; a scenario that passes in one and fails in another is `FLAKY` |
| `--out DIR` | the run's directory, default `target/e2e/<tier>-<time>/` |
| `--port N`, `--url U` | the server, default `http://127.0.0.1:8472/play/` |

| environment | meaning |
|---|---|
| `CHROME` | the browser binary, else `chromium`, `google-chrome` or rod's download |
| `E2E_SERVER` | the server binary for `scripts/e2e.sh`, default `target/playground/satex-playground` |
| `E2E_SETTLE=1` | typing scenarios wait for the first run to end before the click |
| `E2E_DUMPFRAMES=1` | typing scenarios write their frames, boxes and words as JSON |

Each run writes `results.md`, `results.json`, a screenshot per failure and, when the language server stopped, a crash repro (`crash/<scenario>/`: every text sent, the timeline, the worker log) into its own directory. Runs side by side never share a directory or a browser profile.

## Tiers

- `quick`: the behaviors a push must not break, one scenario each, within about five minutes.
- `full`: every scenario. Timing bounds are reported beside a pass but fail nothing.
- `perf`: the typing scenarios whose timing bounds fail there: echo draw p95 ≤ 8 ms, the last key in the source ≤ 200 ms. Machine load moves these, so they are their own tier and never a correctness failure.

## Results

| status | meaning |
|---|---|
| `pass` | every check held |
| `FAIL` | a check failed; the run exits 1 |
| `FLAKY` | passed and failed across `--repeat` rounds; the run exits 1 |
| `known-red` | failed and is listed in `known-red.json`; the run still exits 0 |
| `green-known` | passed but is listed in `known-red.json`: remove the entry |

`known-red.json` is the ledger of reds the suite knows: `{ "name": { "owner": "…", "why": "…", "since": "YYYY-MM-DD" } }`. A red scenario gets an entry with an owner, never a skip. The fix removes the entry in the same patch.

## Layout

| file | what |
|---|---|
| `lib.mjs` | the browser over the DevTools protocol, a page per scenario, the in-page frame sampler and draw recorders |
| `engine.mjs` | the scenario kinds of the PDF and Typeset views: `type` (default), `click`, `tint`, `ts`, `select`, `col`, `forest`, `shift`, `complete`, `dbl`, `bare` |
| `kinds/*.mjs` | `foot`, `echo`, `paper`, `follow`, `tikz`, `crash` |
| `scenarios/<area>.mjs` | the scenarios of one area; `sources.mjs` the documents they share |
| `scenarios/index.mjs` | all areas, the tiers, unique names |
| `fixtures/titles/` | title pages of ten classes |
| `run.mjs` | the runner |

A typing scenario names the word to click, where the caret must land in the source, the keys and what must hold after: the source, the final caret, the echo's font and rows, no duplicated or stray echo text, no caret jump without a key. The field list is at the top of `scenarios/sources.mjs` and in each kind's comment.

## Robustness rules

- Wait on readiness, never on time alone: `#play-status[data-done=ready]`, `satexHealth()` (busy, running, due, queued, recovering, as `../quiet.mjs`), a new build's image, a page word. A fixed sleep only lets a frame or a transition pass after such a wait.
- Typing delays come from a seeded generator reset per scenario, so a scenario types the same keys at the same pace whatever ran before it.
- `/__dev/events` is blocked: an edit to the sources by someone else never reloads a running scenario. The sources are still read live, so a run tests the working tree as it is at that moment.
- A fresh browser context per scenario: no caret, cache, edited file or stopped server leaks into the next one.
- Bounds tolerate load: waits are long (a cold kernel cache takes minutes), timing bounds live in the perf tier.
- Every frame of every scenario checks that no overlay shows over a page whose image is not drawn.
- A failure says what was expected, what was seen and where (source offsets with context, client px), with a screenshot.

## Adding a scenario

Put it into the area's file in `scenarios/`, with a `steps` line that says what is clicked and typed and the bug it pins. Run it alone with `--only name --repeat 3`. Add it to `TIERS.quick` only when it guards a behavior a push must not break and it is not flaky. Two scenarios that differ only in the word clicked or the keys typed are one scenario: keep the one closer to the reported bug. Rewrite rules (TikZ options, tables, forest) belong in the unit tests; here one action per menu kind and code path.

## CI

Not in CI. The Pages job has Node and a Chromium but serves the built site statically; these scenarios need `satex-playground --dev` (its compile endpoint and the dev sources). `quick` is a manual push check (doc/developing.md).
