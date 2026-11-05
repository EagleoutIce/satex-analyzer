#!/usr/bin/env python3
"""Runs the benchmark documents (benches/docs.txt) under every kernel and
records one entry of doc/site/bench/data.json.

  python3 scripts/bench.py [--build] [--satex target/release/satex] [--version V]
                           [--docs benches/docs.txt] [--data doc/site/bench/data.json]
  python3 scripts/bench.py --merge NEW.json [--data doc/site/bench/data.json]

With no options it benchmarks target/release/satex as the Cargo.toml version
and the current commit. `--build` builds the release binary first. An entry
replaces the one of the same version in the data file. `--merge` only folds
the entries of another file in (CI does this on main).

Per run: a cold run builds the caches, the second run is measured."""
import argparse, json, re, subprocess, sys, time
from pathlib import Path

KERNELS = ["tex", "pdftex", "xetex", "luatex"]
UNITS = {"k": 1e3, "M": 1e6, "G": 1e9}


def count(text):
    return round(float(text[:-1]) * UNITS[text[-1]]) if text[-1] in UNITS else int(text)


def grab(pattern, text, convert=count):
    m = re.search(pattern, text)
    return convert(m.group(1)) if m else None


def findings(out):
    try:
        data = json.loads(out)
    except ValueError:
        return None
    items = data if isinstance(data, list) else next((v for v in data.values() if isinstance(v, list)), [])
    return len(items)


def stats_of(err):
    """The `--stats-json` object on stderr, or None for a binary without it."""
    for line in reversed(err.splitlines()):
        if line.startswith("{"):
            try:
                return json.loads(line)
            except ValueError:
                return None
    return None


def run(satex, doc, kernel, timeout):
    cmd = [satex, "lint", "--stats", "--stats-json", "--format", "json", "--color", "never", "--set", f"engine={kernel}", "-f", str(doc)]
    started = time.perf_counter()
    try:
        done = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return {"wall": timeout, "end": False, "timeout": True}
    wall = time.perf_counter() - started
    err = done.stderr
    stats = stats_of(err)
    result = {
        "wall": round(wall, 3),
        "peak": grab(r"(\d+) MiB peak", err, int),
        "steps": grab(r"([\d.]+[kMG]?) tokens", err),
        "forks": grab(r"([\d.]+[kMG]?) forks", err),
        "end": "budget exhausted" not in err and done.returncode in (0, 1),
        "findings": findings(done.stdout),
        "code": done.returncode,
    }
    if stats:
        # exact counts replace the rounded ones of the text line
        result.update(peak=stats["peak_mib"], steps=stats["steps"], forks=stats["forks"], stats=stats)
    return result


def merge(data, new):
    old = json.loads(data.read_text()) if data.exists() else []
    keep = {e["version"] for e in new}
    data.parent.mkdir(parents=True, exist_ok=True)
    data.write_text(json.dumps([e for e in old if e["version"] not in keep] + new, indent=1) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--satex", default="target/release/satex")
    ap.add_argument("--build", action="store_true", help="cargo build --release --bin satex first")
    ap.add_argument("--merge", help="fold the entries of this file into --data and stop")
    ap.add_argument("--version", default=None)
    ap.add_argument("--date", default=time.strftime("%Y-%m-%d"))
    ap.add_argument("--commit", default=None)
    ap.add_argument("--docs", default="benches/docs.txt")
    ap.add_argument("--data", default="doc/site/bench/data.json")
    ap.add_argument("--kernels", nargs="*", default=KERNELS)
    ap.add_argument("--timeout", type=int, default=120)
    args = ap.parse_args()
    data = Path(args.data)
    if args.merge:
        return merge(data, json.loads(Path(args.merge).read_text()))
    if args.build:
        subprocess.run(["cargo", "build", "--locked", "--release", "--bin", "satex"], check=True)
    if not Path(args.satex).exists():
        sys.exit(f"{args.satex} not found, build it or pass --build")
    if args.version is None:
        args.version = re.search(r'^version\s*=\s*"([^"]+)"', Path("Cargo.toml").read_text(), re.M).group(1)
    if args.commit is None:
        git = subprocess.run(["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True)
        args.commit = git.stdout.strip()

    # a line is a document, then `-kernel` for each kernel it is not run under
    lines = [l.split() for l in Path(args.docs).read_text().splitlines() if l.strip()]
    docs = [(l[0], {k[1:] for k in l[1:] if k.startswith("-")}) for l in lines if Path(l[0]).exists()]
    runs = []
    for doc, skipped in docs:
        for kernel in (k for k in args.kernels if k not in skipped):
            run(args.satex, doc, kernel, args.timeout * 8)
            r = {"doc": doc, "kernel": kernel, **run(args.satex, doc, kernel, args.timeout)}
            print(json.dumps(r), file=sys.stderr, flush=True)
            runs.append(r)

    merge(data, [{"version": args.version, "date": args.date, "commit": args.commit, "runs": runs}])
    print(f"wrote {data}", file=sys.stderr)


if __name__ == "__main__":
    main()
