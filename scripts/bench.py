#!/usr/bin/env python3
"""Runs the benchmark documents (benches/docs.txt) under every kernel and
records one entry of doc/site/bench/data.json.

  scripts/bench.py --satex target/release/satex --version 0.1.1 [--date D]
                   [--docs benches/docs.txt] [--data doc/site/bench/data.json]

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


def run(satex, doc, kernel, timeout):
    cmd = [satex, "lint", "--stats", "--format", "json", "--color", "never", "--set", f"engine={kernel}", "-f", str(doc)]
    started = time.perf_counter()
    try:
        done = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return {"wall": timeout, "end": False, "timeout": True}
    wall = time.perf_counter() - started
    err = done.stderr
    return {
        "wall": round(wall, 3),
        "peak": grab(r"(\d+) MiB peak", err, int),
        "steps": grab(r"([\d.]+[kMG]?) tokens", err),
        "forks": grab(r"([\d.]+[kMG]?) forks", err),
        "end": "budget exhausted" not in err and done.returncode in (0, 1),
        "findings": findings(done.stdout),
        "code": done.returncode,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--satex", required=True)
    ap.add_argument("--version", required=True)
    ap.add_argument("--date", default=time.strftime("%Y-%m-%d"))
    ap.add_argument("--commit", default="")
    ap.add_argument("--docs", default="benches/docs.txt")
    ap.add_argument("--data", default="doc/site/bench/data.json")
    ap.add_argument("--kernels", nargs="*", default=KERNELS)
    ap.add_argument("--timeout", type=int, default=120)
    args = ap.parse_args()

    docs = [d for d in Path(args.docs).read_text().split() if Path(d).exists()]
    runs = []
    for doc in docs:
        for kernel in args.kernels:
            run(args.satex, doc, kernel, args.timeout * 8)
            r = {"doc": doc, "kernel": kernel, **run(args.satex, doc, kernel, args.timeout)}
            print(json.dumps(r), file=sys.stderr, flush=True)
            runs.append(r)

    data = Path(args.data)
    entries = json.loads(data.read_text()) if data.exists() else []
    entries = [e for e in entries if e["version"] != args.version]
    entries.append({"version": args.version, "date": args.date, "commit": args.commit, "runs": runs})
    data.parent.mkdir(parents=True, exist_ok=True)
    data.write_text(json.dumps(entries, indent=1) + "\n")


if __name__ == "__main__":
    main()
