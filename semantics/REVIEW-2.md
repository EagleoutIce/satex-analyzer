# Soundness review 2 (HEAD 26ef7ce, 2026-11-04)

Scope: `90d9ed8..26ef7ce`, the pending cor patch (`target/scratch/cor-on-26ef7ce.diff`) and the open items.
Nothing was built or run. Every MWE is **unverified**: TeX's result is from tex.web, SaTeX's is reasoned from the code. Lines are of 26ef7ce.

## Findings

### R2-1. A refuted give-back ignores a scan that is already fuzzy (unsound)

- Code: machine.rs:10407-10419 (`gives_back`), called at 10445-10447.
  Fuzzy sources it ignores: 10349 (`scan_fuzzy`), 10528 (a refused
  `may_begin` fork), 10689 (a refused straddle fork), 10582 (unknown text
  in a group).
- Rule: `scanDelim` (Abstract.lean:769) keeps every split. `branch`
  refused must over-approximate (mapping, D8). `DropSound` drops only
  certain library faults.
- Under `assume_compiles` (the default), a library scan that reads
  document text to the end of input is refuted (`no_arm`). The refutation
  holds only if TeX also reads to the end. When `fuzzy` is set, a fork
  where the argument may end earlier was refused. That ending was never
  explored. So a run that compiles is dropped. Because the run is
  infeasible, `retract` also removes its `unforked-argument-end`
  imprecision. Nothing is reported. If it was the only path, the drive
  "reads on" from the end of the file, so the rest of the document is
  never analysed.
- MWE (`lib.tex` must be library code, not one of the project's files;
  run with `--set limits.choices=0` to refuse the fork):
  ```tex
  % lib.tex:  \long\def\grab#1xi{\def\got{#1}}
  \input lib
  \count1=\pdfuniformdeviate 3 \advance\count1 by 1
  \edef\r{\romannumeral\count1}      % typed text: i, ii or iii
  \expandafter\grab\expandafter x\r  % "xi" is always there
  \message{after}\bye
  ```
  In TeX, `\r` begins with `i`, so `\grab` ends at once and `after` is
  printed. In SaTeX, the fork at the typed text is refused (fuzzy, read
  on) and no `xi` follows, so input ends. `gives_back` then calls `no_arm`
  and no path reaches `\message`, with no diagnostic.
- Fix: refute only where `fuzzy.is_none()` and no fork was refused in this scan. Otherwise take the old `give_back` + `Unk::Any` branch.

### R2-2. Retraction hides the warning of a path ended as a cycle (unsound report, new in 26ef7ce)

- Code: explore.rs:812-815 retracts every raise of an `infeasible` run.
  machine.rs:9378-9389 raises `endless-loop` and then calls `no_arm()`
  when another path waits. directive.rs:146-149 does the same for a
  failed `\satex@assume`.
- Rule: `run_covered`. A run that does not end is a concrete outcome
  (TeX hangs). `no_arm` means "no run", which is false here.
- MWE:
  ```tex
  \def\r{\r}
  \ifodd\pdfrandomseed \r \fi
  \message{done}\bye
  ```
  In TeX, an odd seed loops forever. SaTeX before 26ef7ce reported
  `endless-loop`. Now the arm with `\r` ends by `no_arm`, its run is
  infeasible, the row's live count drops to 0, and the row is filtered
  in `analyze_once` (machine.rs:3070-3072). The report shows only the
  even path. The `cycle_after` test also fires on `rewritten` states
  (machine.rs:9367), which need not be a real cycle. A path that ends
  could then be dropped without a trace.
- Fix: end a cycle without refuting it (`halted` and not `infeasible`,
  or a separate `end_path`). Keep `no_arm` for real refutations. Do the
  same for `\satex@assume`, or raise its Info after the retraction.

### R2-3. A written unknown digit is read as catcode 12 whatever the catcode (unsound, new in e35054c)

- Code: tex.rs:1699-1703 (`written && is_unknown_digit(c)` gives
  `Chr(c, Other)`). The digits come from `join_line` with `digits = true`,
  machine.rs:14195 and 14240-14248, which path joins always use
  (`join_written(.., true)`, machine.rs:14072 and 14081).
- Rule: `AItem.digits` / `printInt` mean characters of category 12, but
  a file is re-tokenized under the catcodes in force (tex.web § 343).
- MWE:
  ```tex
  \newwrite\w \immediate\openout\w=t.tex
  \ifodd\pdfrandomseed \immediate\write\w{\string\x{a}{1}}%
  \else \immediate\write\w{\string\x{a}{2}}\fi
  \immediate\closeout\w
  \def\x#1#2{#2}
  \catcode`\1=13 \def1{\message{one}}
  \input t \bye
  ```
  In TeX, an odd seed reads the active `1` and prints `one`. In SaTeX,
  the paths join at `\fi` to `\x{a}{N}`, `N` is read as an other
  character, and `\message{one}` never runs.
- Fix: in the mouth, read `N`/`D` as `Chr(c, Other)` only if every digit
  it stands for has category 12 now. Otherwise return the unknown-text
  token with `written = None`. The `written_class` branch (tex.rs:1693)
  has the same gap for letters and others (`char_class` assumes default
  catcodes) and needs the same check.

### R2-4. `\ifx` of a one-of token with an empty member compares macros (unsound, open in HEAD)

- Code: exec.rs:2894-2905 (`ifx_operand` returns `(t, None)` if a member
  is empty). exec.rs:3104-3116 (`members` takes the slot's `may` =
  `text_meaning` macros). exec.rs:1933 (a macro is never a character).
- Rule: `ifX` compares one token (tex.web § 507). A one-of item is its
  members' texts (`AText.γ`), not a macro.
- MWE (the `\the` split must be refused, e.g. by a hot site or
  `limits.choices`; cor saw this effect in its first test draft):
  ```tex
  \ifodd\pdfrandomseed \toks0={ab}\else \toks0={}\fi
  \expandafter\ifx\expandafter a\the\toks0 \message{T}\else\message{F}\fi
  ```
  In TeX, an odd seed compares `a` with `a` and prints `T`. SaTeX gets
  `xs = [Char a]` and `ys = [Macro "ab", Macro ""]`. Every pair is
  `Some(false)`, so the test is decided false and prints only `F`.
- Fix: a one-of operand without firsts makes the test undecided (return
  `None`) unless it is split. Better: take its firsts by
  `first_of_either` (cor), with the next token for an empty member.

### R2-5. The top of a dimension is `±\maxdimen`, but TeX holds the whole word (unsound, D10 for dimensions)

- Code: mode.rs:232 (`Dim::ANY`), mode.rs:239-243 (`Dim::bounded` makes
  any sum past `\maxdimen` `ANY`), exec.rs:5037-5041 (`register_top`),
  value.rs:193 (`Num::full(true)`). Lean: `Itv.full true =
  ±maxDimen` (Abstract.lean:69), so the model has the same bug.
- Rule: `Itv.γ` of a top must hold every value. `\advance` of a
  dimension (§ 1238) and `hpack`/`vpack` sums (§§ 656, 668) do not check
  for overflow. "Dimension too large" is a scan error only (§ 460).
- MWE:
  ```tex
  \setbox0\hbox{\kern\maxdimen\kern\maxdimen}
  \ifdim\wd0>\maxdimen \message{big}\else\message{small}\fi
  ```
  In TeX, `\wd0` is 2147483646sp and the test prints `big`. In SaTeX,
  `along` goes past max, `bounded` gives `ANY = ±maxdimen`, and the test
  is decided false (`small`).
- Fix: make `Dim::ANY`, `register_top(Dimen)` and `Num::full(true)` the
  word. Do the same for `Itv.full` in Lean, which then also meets
  `itv_γ_widen`'s `inWord`.

### R2-6. A delimiter that straddles a one-of's end and unknown text after it is missed (unsound, new in 6188d18)

- Code: machine.rs:10597 (the new `!passes` guard skips `split_one_of`),
  machine.rs:10248-10266 (`scan_one_of` returns `Pass` when no member
  holds the whole delimiter), machine.rs:10727-10737 (`straddles` needs
  the tokens after it to equal the delimiter exactly).
  machine.rs:10512-10518 (`may_begin` needs the tokens before it exact,
  and a one-of token never is).
- Rule: `scanDelim` over `AItem.oneOf` (Abstract.lean:769-790). Every
  split of the concatenation must stay.
- MWE: an argument scan for `#1a1` over `<one-of {za, q}>` then typed
  digits of a count in [10,19] (for example
  `\edef\x{\m\the\count1}` with `\m` joined to `za`/`q` and the split
  refused, then `\expandafter\grab\x a1`). In TeX, `za12a1` ends at
  `a1` inside `za12`, so `#1 = z`. SaTeX gets `Pass` for the one-of,
  the digits are not short and cannot hold `a`, and `straddles` sees a
  typed token where it needs `1`. So the read goes on to the explicit
  `a1`, and `#1 = za12` on the `za` path.
- Fix: `scan_one_of` returns `Pass` only if no member ends in a proper
  prefix of the delimiter (the old `straddles` test without the
  `ahead` condition). Otherwise `Unknown`, or split as before
  `!passes`.

### R2-7. Pending cor: pair names are per (t, n), not per read (unsound if merged as is)

- Code (cor-on-26ef7ce.diff): `first_of_either` names A/R by `text_hash([t, n])`. `Env::pair` replaces their Corr rows.
- Rule: `Corr` rows must describe one choice. CHANGES item 10
  (`astate_join_sound`) keeps arms paired only within one join.
- Risk: a second read of the same `t` and `n` re-makes A/R with the same
  syms. If `t`'s copies stand for different choices (`carried.made`
  marks it made twice), an old A that is still live (kept in a macro) is
  paired with the new R. `split_one_of_groups` checks `made`, but
  `refine_may` from `\ifx` (exec.rs) reaches `correlate` without that
  check.
- MWE sketch (not built): a `\loop` reads `\p\u` twice at one site, keeps the first `#1` in `\edef\keep{#1}`, and `\ifx\keep<member>` then refines the second R.
- Fix: do not pair when `made.1` is set for `t`, or salt the tag with a
  per-path read counter. Also make `correlate` callers check `made`.

## Status (rv2, 2026-11-04, target/scratch/rv2.patch on 26ef7ce)

Tests in tests/it/semantics.rs `model_deviations`, each red on 26ef7ce's code (mutation run with the src diff reversed).

- R2-1 FIXED. `gives_back` refutes only where `fuzzy` is unset; past a refused fork it gives the text back (warning). The review's MWE was wrong (letters `xi` never match `\romannumeral`'s category-12 `i`, so TeX runs away too). Test `a_library_scan_past_a_refused_end_reads_the_document_on`: delimiter `x1`, typed digits of a count in [1,300], call in a texmf library file.
- R2-2 FIXED. `Machine::end_path` (explore.rs): ends the path without refuting it. No retraction, and `next_script` keeps the later arms of the last choice (`no_arm` dropped them too). Used by the cycle check and a failed `\satex@assume`. Test `a_cycle_on_one_arm_is_reported` (the cycle must be on the arm run while another waits: `\ifodd.. \else\r\fi`). Open: `cycle_after` on `rewritten` states still ends a path that may not cycle (now with its warning kept).
- R2-3 FIXED for digits. A written unknown digit reads as itself only where all its digits are of category 12 now. Otherwise it is one of: each digit of another category, as it reads now, plus the unknown digit for the rest (tex.rs `Mouth::digit`, machine.rs `written_digit`). Written classes keep their class only where digits, signs, space, braces and ASCII letters have their categories, else any text. Open (named in `reads_as_written`): OTHER may hold a `\string`-written escape character. Test `a_written_unknown_digit_reads_with_the_categories_of_the_read`.
- R2-4 FIXED. `ifx_operand` takes the next token as the first token of an empty member (each text plus that token, rests put back). Test `ifx_of_a_one_of_with_an_empty_member_compares_first_tokens` (split refused by `%\satex@configure{limits.choices=0}`).
- R2-5 PARTLY FIXED (Rust). `Dim::ANY` (typesetting results) is the 32-bit word; `Dim::bounded` widens only past the word. `register_top(Dimen)` stays `±\maxdimen`: as the word it broke 6 interval tests (`intervals_narrow_on_each_arm_and_decide_later_tests` and others). A register read through the § 461 clamp then counts as clamped, so no test narrows it any more. Open: a word top for registers needs the clamp to keep narrowing for values within `\maxdimen`. `Num::full(true)` stays `±\maxdimen`: it is a scanned value (§ 461 checks internal dimensions too). The review's MWE is wrong: `\ifdim\wd0>` raises "Dimension too large" and compares `\maxdimen` (pdftex: `small`). A count takes the width unchecked (`\count1=\wd0` gives 2147483646). Test `a_box_wider_than_maxdimen_stays_wider` (was both arms, now `big` only, as pdftex). Lean `Itv.full` is left as is (it models the scanned top); a stored-dimension top `word` is not yet in the model. Glue (`GlueHull::full`) keeps `±\maxdimen` per component: `\advance` of glue is unchecked as well (open).
- R2-6 FIXED. `scan_one_of` passes a one-of only where no member, after what was read, ends in a proper prefix of the delimiter. Else a path per member, or `Unknown` where no path may part. Test `a_delimiter_from_a_one_of_end_into_unknown_text_ends_there`.
- R2-7 not touched (pending cor patch).
- R2-8 NEW (seen while testing R2-3): a one-of of nine single-character members, one an active character, run unsplit does nothing and raises nothing (the active member never runs). Likely the set past `limits.meaning_set` widens to typed text whose class misses the active character. Not fixed. `written_digit` avoids it by grouping the category-12 digits into one member.

## Checked, no new soundness bug

- `join_streams`/`pin_stream` (machine.rs:1224, 4168): the union keeps every text and each line hull. Widening sends a moved `hi` to `CLOSED`, and `places` lists every line up to the end and then closed. Past `value_set`, `ANY_STREAM` or a refused pin, the stream is unknown. Sound. Only the relation to other names is lost (nc4).
- `give_back` (machine.rs:4792) rewinds only when exactly the tokens read since the mark came from the same file frame (checked by `source_progress`). Braces are uncounted, and the catcodes are those of the scan. Sound.
- `terminal-read-fatal` in `ENGINE_WARNINGS` (machine.rs:2218, 5501): only the severity changes. The path still halts, as TeX does.
- `settled_for_join` (mode.rs:5331): the hull includes kern 0. A ligature or an unknown font keeps the old widening. Sound.
- Own-rest fork (machine.rs:10503, 10559): only the fork key changes. A refused fork still sets `fuzzy` and `unknown_at`.
- `hides_delimiter` (machine.rs:10780) covers whole, ending, starting and inner matches. The only gap is a lone brace in a hole at depth > 0, which only the raw marker can hold (snd note).
- Retraction (machine.rs:5548-5600) counts per raise and keeps cached rows. Apart from R2-2 it hits only `infeasible` runs and shelved drops, which `report_dropped` names.
- Symbol order (vsp2): `Sym`-ordered sorts (explore.rs:1858) and hashed-set order change messages and which bound fires first. That is precision and determinism, not soundness. No set is truncated by `Sym` order without widening.

## Open lemmas against today's Rust

- `endGroup_sound`: P2 is a model-only mismatch. `pop_group` (env.rs:1045) returns the `\aftergroup` tokens in order as one list (§ 282).
- `scanDelim_sound` over holes: R2-1 and R2-6 are new counterexamples. D7 is still open.
- `weakAssign_sound`: `weaken_register_range` (exec.rs:3738) joins the old value with the new one. Its fallback `register_top` is too small for dimensions (R2-5).
- `AVal.widen_sound` needs `inWord`. Dimension tops are not in the word today (R2-5).

| # | Where | Lean rule | Severity | Since |
| --- | --- | --- | --- | --- |
| R2-1 | machine.rs:10407-10447 | `scanDelim`, `DropSound`, `branch` | unsound (silent drop) | 40f0ae8 (r2d) |
| R2-2 | explore.rs:812, machine.rs:9389 | `run_covered` | unsound report | 26ef7ce |
| R2-3 | tex.rs:1699 | `AItem.digits` | unsound | e35054c (flp) |
| R2-4 | exec.rs:2894-2905, 3104 | `ifX`, `AText.γ` | unsound | before 90d9ed8 |
| R2-5 | mode.rs:232-243, exec.rs:5037, Abstract.lean:69 | `Itv.full`, `Itv.γ` | unsound (model too) | before 90d9ed8 |
| R2-6 | machine.rs:10597, 10248-10266, 10727 | `scanDelim` (one-of) | unsound | 6188d18 |
| R2-7 | cor diff: `first_of_either`, `Env::pair` | `Corr` / `astate_join_sound` | unsound if merged | pending |
