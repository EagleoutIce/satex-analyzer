#!/usr/bin/env python3
"""The prose rules against the contexts that are not prose: each rule's
trigger, set in plain prose (it must be found) and wrapped in math, code, a
key, a comment or a URL (it must not), or with the word the rule decides by
replaced by one of those. Prints per rule and context what was found where
nothing belongs (false positives) and what was missed.

    scripts/lint-sweep.py [--satex BIN] [--rule CODE] [--show]

One document per context in target/scratch/lint-sweep, one `satex lint
--format json` each. A finding counts for the block (trigger) its line is in.
"""
import argparse, json, subprocess, sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# rule, trigger in plain prose, and the word `@` stands for there: the word
# the rule decides by.
TRIGGERS = [tuple((line.split("\t") + [""])[:3])
            for line in (ROOT / "tests/it/lint_triggers.txt").read_text().splitlines()
            if line and not line.startswith("#")]

# Wrapping the whole trigger: `<T>` is the trigger with its word filled in.
WRAP = {
    "plain": "<T>",
    "footnote": r"Text.\footnote{<T>}",
    "verb": r"\verb|<T>|",
    "texttt": r"\texttt{<T>}",
    "listing": "\\begin{lstlisting}\n<T>\n\\end{lstlisting}",
    "verbatim": "\\begin{verbatim}\n<T>\n\\end{verbatim}",
    "comment": "% <T>",
    "math": "$<T>$",
    "display": r"\[<T>\]",
    "url": r"\url{<T>}",
    "label": r"\label{<T>}",
    "cite": r"\cite{<T>}",
    "cref": r"\cref{<T>}",
    "tikz": r"\tikz\node{<T>};",
    "foreign": r"\foreignlanguage{ngerman}{<T>}",
}
# Replacing the word the rule decides by.
BESIDE = {
    "w-math": "$x_{42}$",
    "w-paren": r"\(x\)",
    "w-verb": r"\verb|x|",
    "w-texttt": r"\texttt{x}",
    "w-url": r"\url{http://x.org}",
    "w-ref": r"\ref{sec:a}",
    "w-cite": r"\cite{knuth}",
    "w-cref": r"\cref{sec:a}",
}
MUST = {"plain", "footnote"}
# Seen but not judged: whether a TikZ node's text or another language's is
# prose for a rule depends on the rule.
OPEN = {"tikz", "foreign"}
# Rules that read mathematics: in math they are right to speak.  A quantity
# in math is the worst case, and a dash there sets minus signs.
MATH_RULES = {"math-operator-name", "math-punctuation", "dimension-times", "ellipsis-dots", "hand-set-quantity",
              "wrong-dash"}
# Rules that measure the font: an italic correction or an accent on a dotted
# letter is wrong in any font.
FONT_RULES = {"accent-on-dotted-letter", "missing-italic-correction", "stray-italic-correction", "zero-italic-correction"}
# Triggers that are a formula themselves: inside `\texttt` they are math still.
FORMULA = {"math-operator-name", "math-punctuation"}
# A context where the rule's own trigger is still the mistake it names:
# `\cref` sets its lowercase name at a sentence start.
# A filler that is itself the mistake (`\cref` at a sentence start, a `\ref`
# with a space in front), or sets a number the range rule rightly reads.
INTENDED = {("sentence-capital", "w-cref"), ("missing-nonbreaking-space", "w-ref"),
            ("missing-nonbreaking-space", "w-cite"), ("wrong-dash", "w-ref"), ("wrong-dash", "w-cref")}

PREAMBLE = r"""\documentclass{article}
\usepackage[LANGUAGES]{babel}
\usepackage{listings}
\usepackage{url}
\usepackage{tikz}
\usepackage{cleveref}
\begin{document}
\section{One}\label{sec:a}
"""


def lint(satex, folder, name):
    out = subprocess.run([satex, "lint", "-f", name, "--format", "json", "--set", "cache.document=false"],
                         cwd=folder, capture_output=True, text=True, timeout=600).stdout
    try:
        return [r for r in json.loads(out) if r.get("file", name) == name]
    except json.JSONDecodeError:
        print(f"{name}: no JSON: {out[:200]}", file=sys.stderr)
        return []


def document(context, triggers):
    """The document for `context`, and per trigger the lines its block covers."""
    text, blocks = PREAMBLE, []
    for rule, template, word in triggers:
        if context in WRAP:
            body = template.replace("@", word)
            if context == "verb":
                body = WRAP[context].replace("|", "!").replace("<T>", body) if "|" in body else WRAP[context].replace("<T>", body)
            else:
                body = WRAP[context].replace("<T>", body)
        else:
            body = template.replace("@", BESIDE[context])
        start = text.count("\n") + 1
        text += body + "\n\n"
        blocks.append((start, start + body.count("\n")))
    languages = "ngerman,english" if context == "foreign" else "english"
    return text.replace("LANGUAGES", languages) + "\\end{document}\n", blocks


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--satex", default=str(ROOT / "target/fast/satex"))
    ap.add_argument("--rule", help="only this rule's triggers")
    ap.add_argument("--jobs", type=int, default=3, help="lint runs at once")
    ap.add_argument("--show", action="store_true", help="print every false positive")
    args = ap.parse_args()
    triggers = [t for t in TRIGGERS if not args.rule or t[0] == args.rule]
    work = ROOT / "target/scratch/lint-sweep"
    work.mkdir(parents=True, exist_ok=True)
    prose = {t[0] for t in TRIGGERS}
    fp = defaultdict(lambda: defaultdict(int))  # rule -> context -> count
    missed = defaultdict(list)
    seen_open = defaultdict(list)
    shown = []
    runs = []
    for context in list(WRAP) + list(BESIDE):
        cases = [t for t in triggers if context in WRAP or "@" in t[1]]
        if cases:
            text, blocks = document(context, cases)
            (work / f"{context}.tex").write_text(text)
            runs.append((context, cases, blocks))
    with ThreadPoolExecutor(args.jobs) as pool:
        found = list(pool.map(lambda run: lint(args.satex, work, f"{run[0]}.tex"), runs))
    for (context, cases, blocks), records in zip(runs, found):
        name = f"{context}.tex"
        records = [r for r in records if r["code"] in prose]
        for (rule, template, word), (a, b) in zip(cases, blocks):
            here = [r for r in records if a <= r.get("line", 0) <= b]
            own = [r for r in here if r["code"] == rule]
            if context in MUST:
                if not own:
                    missed[rule].append(f"{context}: {template.replace('@', word)}")
                continue
            if context in BESIDE:
                # A filler such as `\ref` may rightly raise another rule.
                here = [] if (rule, context) in INTENDED else own
            for r in here:
                allowed = (r["code"] in MATH_RULES and context in {"math", "display"}
                           or r["code"] in FONT_RULES and context in {"math", "display", "texttt"}
                           or r["code"] == rule in FORMULA and context == "texttt")
                if allowed:
                    continue
                if context in OPEN:
                    seen_open[r["code"]].append(context)
                    continue
                fp[r["code"]][context] += 1
                shown.append(f"{name}:{r.get('line')} {r['code']}: {r.get('message', '')[:90]}")
    contexts = [c for c in list(WRAP) + list(BESIDE) if c not in MUST | OPEN]
    total = 0
    print(f"{'rule':28} {'FP':>3}  contexts")
    for rule in sorted(prose):
        n = sum(fp[rule].values())
        total += n
        where = ", ".join(f"{c} {fp[rule][c]}" for c in contexts if fp[rule][c])
        miss = f"  MISSED {len(missed[rule])}" if missed[rule] else ""
        extra = f"  (open: {', '.join(sorted(set(seen_open[rule])))})" if seen_open[rule] else ""
        print(f"{rule:28} {n:3}  {where}{miss}{extra}")
    print(f"total false positives: {total}, rules with one: {sum(1 for r in fp if sum(fp[r].values()))}, "
          f"missed: {sum(len(m) for m in missed.values())}")
    for rule, ms in missed.items():
        for m in ms:
            print(f"   missed {rule}: {m}")
    if args.show:
        for s in shown:
            print("  ", s)
    return 1 if total or missed else 0


if __name__ == "__main__":
    sys.exit(main())
