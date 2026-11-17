#!/usr/bin/env python3
"""The spacing cycle over real documents: at each site a rule asks for, put in
the known mistake, lint, apply the rule's own fix, and compare with the
original. Prints per rule how many mistakes were found (recall), how many fixes
gave the original back, and which other findings the mistake raised.

    scripts/spacing-sweep.py [--per N] [--satex BIN] DOC.tex ...

Defaults to doc/site/examples/*.tex. Runs `satex lint --format json` once per
mutation in a scratch copy of each document's folder (target/scratch/sweep).
"""
import argparse, json, re, shutil, subprocess, sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# rule, the clean form as a pattern whose group 1 the mistake replaces
CLASSES = [
    ("missing-nonbreaking-space", r"\w(~)\\(?:ref|eqref|pageref|autoref|cite)\b", " "),
    ("sentence-spacing", r"\b(?:e\.g|i\.e|cf|vs)\.(\\ )", " "),
    ("sentence-spacing", r"[A-Z][A-Z](\\@)\.", ""),
    ("space-before-punctuation", r"[a-z]()[,;] [a-z]", " "),
    ("space-before-command", r"[a-z]()\\footnote\b", " "),
    ("command-terminated-space", r"\\(?:LaTeX|TeX)(\{\}) [a-z]", ""),
    ("parenthesis-spacing", r"[a-z]( )\([a-z]", ""),
]


def lint(satex, folder, name, root):
    """The findings in `name`, from a run of `root` (the document that reads it)."""
    out = subprocess.run([satex, "lint", "-f", root, "--format", "json", "--set", "cache.document=false"],
                         cwd=folder, capture_output=True, text=True, timeout=600).stdout
    try:
        return [r for r in json.loads(out) if r.get("file", name) == name]
    except json.JSONDecodeError:
        return []


def root_of(doc):
    """`doc` itself, or the document beside it that inputs it."""
    if "\\documentclass" in doc.read_text():
        return doc.name
    for other in sorted(doc.parent.glob("*.tex")):
        if re.search(r"\\(?:input|include)\{" + re.escape(doc.stem) + r"(?:\.tex)?\}", other.read_text()):
            return other.name
    return doc.name


def apply(text, edits):
    lines = text.split("\n")
    offset = lambda p: sum(len(l) + 1 for l in lines[: p["line"] - 1]) + p["col"] - 1
    for e in sorted(edits, key=lambda e: offset(e["start"]), reverse=True):
        a, b = offset(e["start"]), offset(e["end"])
        text = text[:a] + e["replacement"] + text[b:]
    return text


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("docs", nargs="*")
    ap.add_argument("--per", type=int, default=3, help="sites per rule and document")
    ap.add_argument("--satex", default=str(ROOT / "target/fast/satex"))
    args = ap.parse_args()
    docs = [Path(d).resolve() for d in args.docs] or sorted((ROOT / "doc/site/examples").glob("*.tex"))
    work = ROOT / "target/scratch/sweep"
    stats = defaultdict(Counter)
    extra = defaultdict(Counter)
    misses = []
    for doc in docs:
        clean = doc.read_text()
        folder = work / doc.parent.name
        if not folder.exists():
            shutil.copytree(doc.parent, folder)
        target = folder / doc.name
        base, root = None, root_of(doc)
        for code, pattern, wrong in CLASSES:
            for m in list(re.finditer(pattern, clean))[: args.per]:
                if base is None:
                    target.write_text(clean)
                    base = Counter(r["code"] for r in lint(args.satex, folder, doc.name, root))
                a, b = m.span(1)
                line = clean.count("\n", 0, a) + 1
                # a comment is no text: no rule reads it
                if re.search(r"(?<!\\)%", clean[clean.rfind("\n", 0, a) + 1 : a]):
                    continue
                mutated = clean[:a] + wrong + clean[b:]
                target.write_text(mutated)
                records = lint(args.satex, folder, doc.name, root)
                hits = [r for r in records if r["code"] == code and r.get("line") == line and r.get("edits")]
                key = f"{code} ({pattern[:28]})"
                stats[key]["sites"] += 1
                new = Counter(r["code"] for r in records) - base
                new[code] -= 1
                for c, n in (+new).items():
                    extra[key][c] += n
                if not hits:
                    misses.append(f"{doc.name}:{line} {code}: not found")
                    continue
                stats[key]["found"] += 1
                if apply(mutated, hits[0]["edits"]) == clean:
                    stats[key]["restored"] += 1
                else:
                    misses.append(f"{doc.name}:{line} {code}: fix gives other text")
        target.write_text(clean)
    for key, s in stats.items():
        others = ", ".join(f"{c} {n}" for c, n in extra[key].most_common(3))
        print(f"{key:58} sites {s['sites']:3}  found {s['found']:3}  restored {s['restored']:3}{'  also: ' + others if others else ''}")
    for m in misses:
        print("  ", m)
    return 1 if misses else 0


if __name__ == "__main__":
    sys.exit(main())
