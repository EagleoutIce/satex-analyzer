#!/usr/bin/env python3
"""Maintains CHANGELOG.md from the git history.

  changelog.py draft            commits not yet in Unreleased, by area, to curate into one-liners
  changelog.py check            exit 1 when user-visible commits are missing from Unreleased
  changelog.py mark             record HEAD as reviewed (after curating)
  changelog.py release vX.Y.Z   Unreleased becomes "vX.Y.Z (date)", a fresh Unreleased on top
  changelog.py notes vX.Y.Z     print the section of a release
  changelog.py links            write bare `@abc1234` and `#12` in the file as links

Unreleased starts with `<!-- reviewed: <sha> -->`. Commits up to it count as curated.
"""
import datetime, os, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILE = ROOT / "CHANGELOG.md"
MARK = re.compile(r"<!-- reviewed: ([0-9a-f]+) -->")
VISIBLE = {"feat", "feat-fix", "lint-fix", "perf", "perf-fix", "fix"}
# commit scope -> area heading; anything else is Analysis (docs types go to the site)
AREAS = {
    "Playground": "playground draw",
    "Editor plugins and widgets": "lsp explain",
    "VS Code": "vscode",
    "Lints": "lint",
    "CLI": "cli config trace query explore completions output",
    "Site and docs": "site wiki docs doc readme",
    "Build and infrastructure": "build ci bench test tests deps dep meta fixtures citations",
}
SCOPE = {s: a for a, ss in AREAS.items() for s in ss.split()}
BADGE = {"feat": "feat", "perf": "perf", "docs": "docs", "doc": "docs"}


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()


def parse(subject):
    m = re.match(r"([a-z-]+?)(?:\(([a-z0-9-]+)\))?!?: (.*)", subject)
    return m.groups() if m else (None, None, subject)


def badge(kind):
    return "fix" if kind and kind.endswith("-fix") or kind == "fix" else BADGE.get(kind, kind or "misc")


def area(kind, scope):
    if kind in ("docs", "doc", "docs-fix", "doc-fix"):
        return "Site and docs"
    if kind and kind.startswith(("build", "ci", "test", "dep", "meta", "wip")):
        return "Build and infrastructure"
    return SCOPE.get(scope, "Analysis")


def unreleased():
    text = FILE.read_text()
    m = re.search(r"^## Unreleased\n(.*?)(?=^## |\Z)", text, re.S | re.M)
    return text, m


def last_tag():
    return git("describe", "--tags", "--abbrev=0", "--match", "v*")


def pending():
    """(sha, kind, scope, subject) of commits after the reviewed mark that no entry links."""
    _, m = unreleased()
    body = m.group(1) if m else ""
    mark = MARK.search(body)
    base = mark.group(1) if mark else last_tag()
    refs = set(re.findall(r"\b[0-9a-f]{7,40}\b", MARK.sub("", body)))
    out = []
    for line in git("log", "--no-merges", "--format=%H %s", f"{base}..HEAD").splitlines():
        sha, subject = line.split(" ", 1)
        if not any(sha.startswith(r) for r in refs):
            out.append((sha, *parse(subject)))
    return out


def draft():
    groups = {}
    for sha, kind, scope, subject in pending():
        groups.setdefault(area(kind, scope), []).append(f"- `{badge(kind)}` {subject} [`{sha[:7]}`]")
    for name in [*AREAS, "Analysis"]:
        if name in groups:
            print(f"### {name}\n" + "\n".join(groups[name]) + "\n")


def check():
    miss = [c for c in pending() if c[1] in VISIBLE]
    for sha, kind, scope, subject in miss:
        print(f"{sha[:7]} {kind}{f'({scope})' if scope else ''}: {subject}")
    if miss:
        print(f"\n{len(miss)} user-visible commits missing from Unreleased: run scripts/changelog.py draft", file=sys.stderr)
        sys.exit(1)


def mark():
    text, m = unreleased()
    head = git("rev-parse", "--short", "HEAD")
    body = MARK.sub("", m.group(1)).lstrip("\n")
    FILE.write_text(text[:m.start(1)] + f"\n<!-- reviewed: {head} -->\n\n" + body + text[m.end(1):])


def release(tag):
    if not re.fullmatch(r"v\d+\.\d+\.\d+(-[\w.]+)?", tag):
        sys.exit(f"not a version tag: {tag}")
    text, m = unreleased()
    if not m:
        sys.exit("no Unreleased section")
    if f"## {tag} " in text:
        sys.exit(f"{tag} is already released")
    body = MARK.sub("", m.group(1)).strip("\n")
    if not body:
        sys.exit("Unreleased is empty")
    day = os.environ.get("SOURCE_DATE_EPOCH")
    day = (datetime.datetime.fromtimestamp(int(day), datetime.UTC) if day else datetime.datetime.now(datetime.UTC)).date()
    FILE.write_text(text[:m.start()] + f"## Unreleased\n\n<!-- reviewed: {git('rev-parse', '--short', 'HEAD')} -->\n\n## {tag} ({day})\n\n{body}\n\n" + text[m.end():])


REPO = "https://github.com/EagleoutIce/satex-analyzer"


def links():
    text = FILE.read_text()
    text = re.sub(r"(?<![\w`/\[])@([0-9a-f]{7,40})\b", lambda m: f"[`{git('rev-parse', '--short=7', m[1])}`]({REPO}/commit/{git('rev-parse', '--short=7', m[1])})", text)
    text = re.sub(r"(?<![\w`/\[&])#(\d+)\b", lambda m: f"[#{m[1]}]({REPO}/pull/{m[1]})", text)
    FILE.write_text(text)


def notes(tag):
    m = re.search(rf"^## {re.escape(tag)} .*?\n(.*?)(?=^## |\Z)", FILE.read_text(), re.S | re.M)
    if not m:
        sys.exit(f"no section for {tag}")
    # links relative to the repository root, made absolute: the notes live elsewhere
    print(re.sub(r"\]\((?![a-z]+:|#)([^)]+)\)", rf"]({REPO}/blob/main/\1)", m.group(1).strip()))


if __name__ == "__main__":
    cmd, *rest = sys.argv[1:] or ["draft"]
    {"draft": draft, "check": check, "mark": mark, "release": release, "notes": notes, "links": links}[cmd](*rest)
