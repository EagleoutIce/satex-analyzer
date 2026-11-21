#!/usr/bin/env python3
"""Compile benchmark: `satex compile` against latexmk and the plain engine on the same machine.
Records one entry of doc/site/bench/compile.json (the analysis data stays in data.json).

  python3 scripts/bench_compile.py [--satex target/fast/satex] [--list benches/compile.txt]
        [--only NAME,...] [--big article-200,book-1000] [--tools ...] [--scenarios cold,warm,edit]
        [--repeats 5] [--big-repeats 3] [--work target/bench-compile] [--data doc/site/bench/compile.json]

benches/compile.txt names a document and its engine per line. --big adds generated documents
(benches/speed/gen.py, `CLASS-PAGES`). Cells of the same version merge into its entry, a cell
measured again replaces the old one.

Tools (all with the clock pinned by SOURCE_DATE_EPOCH, FORCE_SOURCE_DATE=1):
  latexmk      latexmk -pdf|-lualatex|-xelatex
  engine       the engine, rerun as latexmk would: BibTeX/Biber/makeindex on changed input,
               again while the build files change or the log asks, at most 5 passes
  satex        satex compile --system satex, explaining the log (the analysis where it needs one)
  satex-build  the same with compile.explain=false: the PDF only
  satex-warm   satex-build with compile.warm=true: a pdfTeX waits on its format for the next build
Scenarios:
  cold  a fresh copy of the document's directory, no build files. Caches that exist: the OS file
        cache, luaotfload's font cache, satex's package caches (SATEX_CACHE under --work), each
        primed by one unmeasured build per document and tool.
  warm  a rebuild with nothing changed, after a full build of the same tool in the same dir.
  edit  a rebuild after one body word changed (another word each repeat), after a full build.
Per run: wall, the time to the PDF's last write (pdf_s, before satex's explanation; none when
the build left the PDF as it was), CPU (user+sys of the whole process tree, warm engines
included: this script is their subreaper), peak RSS, engine passes, BibTeX/Biber/makeindex
runs. The PDF of each tool's last run is compared with latexmk's of the same scenario: page count and pdftotext text.
A run starts only while the 1-minute load is below 3/4 of the cores (waits up to 2 min)."""
import argparse, ctypes, json, os, re, shutil, signal, statistics, subprocess, sys, threading, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "benches" / "speed"))
from gen import mixed  # noqa: E402

EPOCH = "1795176000"  # 2026-11-20T12:00Z
CLOCK = "2026-11-20T12:00"
ENV = {"SOURCE_DATE_EPOCH": EPOCH, "FORCE_SOURCE_DATE": "1", "SATEX_NO_SETUP": "1"}
PROGRAM = {"pdftex": "pdflatex", "luatex": "lualatex", "xetex": "xelatex"}
LATEXMK = {"pdftex": "-pdf", "luatex": "-lualatex", "xetex": "-xelatex"}
TOOLS = ["latexmk", "engine", "satex", "satex-build", "satex-warm"]
SCENARIOS = ["cold", "warm", "edit"]
REST = ["aux", "toc", "lof", "lot", "out", "bbl", "ind", "nav", "snm", "bcf", "idx", "glo"]
TIMEOUT = 900


def load():
    return os.getloadavg()[0]


def quiet(seen):
    """Waits for a quiet machine (load below 3/4 of the cores, 2 min at most), records the load."""
    for _ in range(24):
        if load() < 0.75 * os.cpu_count():
            break
        time.sleep(5)
    seen.append(round(load(), 1))


# CPU seconds and peak KiB of the runs since Meter's start
ACC = [0.0, 0]


def account(ru):
    ACC[0] += ru.ru_utime + ru.ru_stime
    ACC[1] = max(ACC[1], ru.ru_maxrss)


def reap():
    """Accounts finished orphans (warm engines) this script adopted as their subreaper."""
    while True:
        try:
            pid, _, ru = os.wait4(-1, os.WNOHANG)
        except ChildProcessError:
            return
        if pid == 0:
            return
        account(ru)


def run(cmd, cwd, env):
    """One command: wall, exit code, output. Its tree's rusage goes to ACC (wait4 includes the
    children it waited for). Killed as a group past TIMEOUT. Output goes to a file: a warm engine
    it leaves behind may hold its descriptors."""
    log = Path(cwd).parent / f".{Path(cwd).name}.out"
    with open(log, "wb") as f:
        started = time.perf_counter()
        p = subprocess.Popen(cmd, cwd=cwd, env=env, stdout=f, stderr=subprocess.STDOUT,
                             stdin=subprocess.DEVNULL, start_new_session=True)
        timer = threading.Timer(TIMEOUT, lambda: os.killpg(p.pid, signal.SIGKILL))
        timer.start()
        _, status, ru = os.wait4(p.pid, 0)
        wall = time.perf_counter() - started
        timer.cancel()
    p.returncode = code = os.waitstatus_to_exitcode(status)
    account(ru)
    if code == -signal.SIGKILL and wall >= TIMEOUT:
        code = "timeout"
    return wall, code, log.read_text(errors="replace")


class Meter:
    """CPU and peak RSS of everything run between start and stop, adopted orphans included."""

    def __enter__(self):
        reap()
        ACC[:] = [0.0, 0]
        return self

    def stop(self):
        reap()
        self.cpu, self.peak = ACC[0], ACC[1] // 1024

    def __exit__(self, *_):
        pass


def changed(dir, job, was):
    now = {e: (dir / f"{job}.{e}").read_bytes() if (dir / f"{job}.{e}").exists() else b"" for e in REST}
    return now != was, now


def engine_build(dir, file, job, engine, env):
    """The engine as latexmk would run it. Gives passes, tool runs, wall, code, output."""
    prog = [PROGRAM[engine], "-interaction=nonstopmode", "-file-line-error", "-synctex=1", file]
    _, was = changed(dir, job, {})
    passes = tools = 0
    wall, outs, code = 0.0, [], 0
    inputs = {}
    for _ in range(5):
        w, code, out = run(prog, dir, env)
        wall, passes = wall + w, passes + 1
        outs.append(out)
        ran = False
        for tool, src, cmd in (("bibtex", "aux", ["bibtex", job]), ("biber", "bcf", ["biber", job]),
                               ("makeindex", "idx", ["makeindex", job])):
            f = dir / f"{job}.{src}"
            if not f.exists() or (tool == "bibtex" and b"\\bibdata" not in f.read_bytes()):
                continue
            key = f.read_bytes() if tool != "bibtex" else b"".join(
                l for l in f.read_bytes().splitlines() if l.startswith((b"\\citation", b"\\bibdata", b"\\bibstyle")))
            if inputs.get(tool) == key:
                continue
            inputs[tool] = key
            w, _, out = run(cmd, dir, env)
            wall, tools, ran = wall + w, tools + 1, True
            outs.append(out)
        moved, was = changed(dir, job, was)
        log = (dir / f"{job}.log").read_text(errors="replace") if (dir / f"{job}.log").exists() else ""
        if not (ran or moved or "Rerun to get" in log or "Label(s) may have changed" in log):
            break
    return passes, tools, wall, code, "\n".join(outs)


def build(tool, satex, dir, file, job, engine, env):
    """One build of `tool`. Gives wall, code, passes, tool runs, analysis ran (satex only)."""
    if tool == "engine":
        passes, tools, wall, code, out = engine_build(dir, file, job, engine, env)
        return wall, code, passes, tools, None, out
    if tool == "latexmk":
        cmd = ["latexmk", LATEXMK[engine], "-interaction=nonstopmode", "-file-line-error", "-synctex=1", "-f", file]
        wall, code, out = run(cmd, dir, env)
        passes = len(re.findall(r"Run number \d+ of rule '(?:pdf|lua|xe)?latex", out))
        tools = len(re.findall(r"Run number \d+ of rule '(?:bibtex|biber|makeindex)", out))
        return wall, code, passes, tools, None, out
    sets = [f"engine={engine}", f"clock={CLOCK}"]
    sets += {"satex": [], "satex-build": ["compile.explain=false"],
             "satex-warm": ["compile.explain=false", "compile.warm=true"]}[tool]
    cmd = [satex, "compile", "--system", "satex", "--stats", "--format", "json", "--color", "never"]
    cmd += [a for s in sets for a in ("--set", s)] + ["-f", file]
    wall, code, out = run(cmd, dir, env)
    m = re.search(r"satex: (\d+) pass", out)
    passes = int(m.group(1)) if m else 0
    analysis = bool(re.search(r"\b[1-9][\d.]*[kMG]? tokens", out)) and "explained as the last build" not in out
    return wall, code, passes, None, analysis if tool == "satex" else False, out


def stop_warm(dir):
    rec = dir / ".satex-build" / "mirror" / ".satex-build" / "warm" / "engine"
    if rec.exists():
        try:
            os.kill(int(rec.read_text().split()[0]), signal.SIGTERM)
        except (ValueError, ProcessLookupError, PermissionError):
            pass
    time.sleep(0.2)
    reap()


EDIT = re.compile(r"(?<=[ \t\n])([a-z]{4,})(?=[ .,;\n])")
# code, not text: verbatim, pictures, math
SKIP = re.compile(r"\\begin\{(verbatim|Verbatim|lstlisting|minted|comment|filecontents|tikzpicture|axis|circuitikz"
                  r"|equation|align|gather|multline|displaymath|forest)\*?\}.*?\\end\{\1\*?\}", re.S)


def text_at(line):
    """Whether a word after `line` (its line up to the word) is running text: no comment, math,
    brace or bracket open before it."""
    if "%" in line or line.count("$") % 2 or line.count("[") != line.count("]"):
        return False
    return line.count("{") == line.count("}")


def edit(path, rep):
    """Changes one body word: the first word of running text after \\begin{document} ends in one
    q, then two, in turn: each repeat is a change. With none, a word goes before \\end{document}."""
    text = path.read_text()
    code = [m.span() for m in SKIP.finditer(text)]
    for m in EDIT.finditer(text, max(text.find("\\begin{document}"), 0)):
        if any(a <= m.start() < b for a, b in code) or not text_at(text[text.rfind("\n", 0, m.start()) + 1:m.start()]):
            continue
        word = m.group(1).rstrip("q") + "q" * (rep % 2 + 1)
        path.write_text(text[:m.start()] + word + text[m.end():])
        return
    path.write_text(text.replace("\\end{document}", f"Edit {rep}.\n\\end{{document}}", 1))


def pdf_facts(pdf):
    if not pdf.exists():
        return None, None
    info = subprocess.run(["pdfinfo", str(pdf)], capture_output=True, text=True).stdout
    pages = re.search(r"Pages:\s+(\d+)", info)
    text = subprocess.run(["pdftotext", "-q", str(pdf), "-"], capture_output=True, text=True).stdout
    return (int(pages.group(1)) if pages else None), text


def fresh(src_dir, dst, names):
    if dst.exists():
        stop_warm(dst)
        shutil.rmtree(dst)
    if names is None:
        shutil.copytree(src_dir, dst, ignore=shutil.ignore_patterns(".satex-build", "*.aux", "*.log", "*.pdf"))
    else:
        dst.mkdir(parents=True)
        for name, text in names.items():
            (dst / name).write_text(text)


def cell(args, doc, engine, group, tools, env, loads):
    """Every scenario and tool of one document: its runs."""
    if doc.startswith("gen:"):
        cls, pages = doc[4:].rsplit("-", 1)
        src_dir, names, file = None, {"main.tex": mixed(int(pages), cls)}, "main.tex"
    else:
        src_dir, names, file = Path(doc).parent, None, Path(doc).name
    job = Path(file).stem
    reps = args.big_repeats if group == "big" else args.repeats
    tag = re.sub(r"[^A-Za-z0-9_.-]", "_", f"{doc}@{engine}")
    work = Path(args.work) / tag
    runs = []
    for tool in tools:  # prime the caches: one unmeasured build each
        d = work / "prime" / tool
        fresh(src_dir, d, names)
        build(tool, args.satex, d, file, job, engine, env)
        stop_warm(d)
    for scenario in args.scenarios:
        samples = {t: [] for t in tools}
        dirs = {t: work / scenario / t for t in tools}
        if scenario != "cold":
            for t in tools:
                fresh(src_dir, dirs[t], names)
                build(t, args.satex, dirs[t], file, job, engine, env)
        for rep in range(reps):
            for t in tools:  # interleaved, so a noisy minute hits every tool
                d = dirs[t]
                if scenario == "cold":
                    fresh(src_dir, d, names)
                if scenario == "edit":
                    edit(d / file, rep)
                quiet(loads)
                began = time.time()
                with Meter() as m:
                    wall, code, passes, ntools, analysis, out = build(t, args.satex, d, file, job, engine, env)
                    m.stop()
                pdf = d / f"{job}.pdf"
                ok = code != "timeout" and pdf.exists()
                # the PDF's last write: satex explains the log after it, the editor shows it at once
                at = pdf.stat().st_mtime - began if pdf.exists() else None
                at = None if at is None or at < 0 else min(at, wall)
                samples[t].append(dict(wall=wall, pdf=at, cpu=m.cpu, peak=m.peak, passes=passes, tools=ntools,
                                       analysis=analysis, ok=ok, code=code, out=out[-2000:]))
        ref_pages, ref_text = pdf_facts(dirs["latexmk"] / f"{job}.pdf") if "latexmk" in tools else (None, None)
        for t in tools:
            s = samples[t]
            pages, text = pdf_facts(dirs[t] / f"{job}.pdf")
            walls = [x["wall"] for x in s]
            med = lambda k: statistics.median(x[k] for x in s) if all(x[k] is not None for x in s) else None
            bad = [x for x in s if not x["ok"]]
            r = {"doc": doc, "group": group, "engine": engine, "pages": ref_pages, "scenario": scenario, "tool": t,
                 "n": len(s), "wall": {"median": round(statistics.median(walls), 3), "min": round(min(walls), 3),
                                       "max": round(max(walls), 3)},
                 "pdf_s": med("pdf") and round(med("pdf"), 3), "cpu": round(med("cpu"), 3),
                 "peak_mib": max(x["peak"] for x in s), "engine_runs": med("passes"),
                 "tool_runs": med("tools"), "analysis": any(x["analysis"] for x in s) if t == "satex" else None,
                 "ok": not bad, "same_pages": None if t == "latexmk" else pages == ref_pages,
                 "same_text": None if t == "latexmk" else text == ref_text,
                 "note": f"exit {bad[0]['code']}: {bad[0]['out'][-300:]}" if bad else None}
            runs.append(r)
            if t != "latexmk" and text != ref_text and text is not None and ref_text is not None:
                keep = Path(args.work) / "diffs" / f"{tag}-{scenario}-{t}"
                keep.mkdir(parents=True, exist_ok=True)
                (keep / "tool.txt").write_text(text)
                (keep / "latexmk.txt").write_text(ref_text)
            print(json.dumps({k: v for k, v in r.items() if k != "note"}), file=sys.stderr, flush=True)
        for t in tools:
            stop_warm(dirs[t])
    return runs


def machine(loads):
    cpu = next((l.split(":", 1)[1].strip() for l in Path("/proc/cpuinfo").read_text().splitlines()
                if l.startswith("model name")), "?")
    mem = int(re.search(r"MemTotal:\s+(\d+)", Path("/proc/meminfo").read_text()).group(1)) // 1048576
    tl = re.search(r"\(TeX Live (\d+)", subprocess.run(["pdftex", "--version"], capture_output=True, text=True).stdout)
    mk = re.search(r"Version (\S+)", subprocess.run(["latexmk", "-v"], capture_output=True, text=True).stdout)
    u = os.uname()
    return {"cpu": cpu, "cores": os.cpu_count(), "ram_gib": mem, "os": f"{u.sysname} {u.release.split('-')[0]}",
            "texlive": f"TeX Live {tl.group(1)}" if tl else "?", "latexmk": mk.group(1) if mk else "?",
            "load": [min(loads), max(loads)] if loads else None}


def geomean(xs):
    xs = [x for x in xs if x and x > 0]
    return round(statistics.geometric_mean(xs), 2) if xs else None


def summary(runs, tool="satex-build"):
    by = {(r["doc"], r["engine"], r["scenario"], r["tool"]): r for r in runs}

    def speedup(of):
        """Geometric mean over the docs group of latexmk's median wall over `of`'s, per scenario."""
        return {sc: geomean([by[(d, e, s, "latexmk")]["wall"]["median"] / r["wall"]["median"]
                             for (d, e, s, t), r in by.items() if s == sc and t == of and r["group"] == "docs"
                             and (d, e, s, "latexmk") in by and r["ok"]]) for sc in SCENARIOS}
    big = [r for r in runs if r["group"] == "big" and r["scenario"] == "edit" and r["tool"] == tool]
    big = max(big, key=lambda r: r["pages"] or 0, default=None)
    cmp = [r for r in runs if r["same_text"] is not None]
    tools = sorted({r["tool"] for r in runs} - {"latexmk"})
    return {"tool": tool, "speedup": speedup(tool), "by_tool": {t: speedup(t) for t in tools},
            "big_edit": big and {"doc": big["doc"], "pages": big["pages"], "satex": big["wall"]["median"],
                                 "latexmk": by.get((big["doc"], big["engine"], "edit", "latexmk"), {}).get("wall", {}).get("median")},
            "identical": f"{sum(r['same_pages'] and r['same_text'] for r in cmp)}/{len(cmp)}"}


def merge(data, entry):
    old = json.loads(data.read_text()) if data.exists() else []
    prev = next((e for e in old if e["version"] == entry["version"]), None)
    if prev:
        key = lambda r: (r["doc"], r["engine"], r["scenario"], r["tool"])
        fresh_keys = {key(r) for r in entry["runs"]}
        entry["runs"] = [r for r in prev["runs"] if key(r) not in fresh_keys] + entry["runs"]
        loads = [x for e in (prev, entry) for x in (e["machine"]["load"] or [])]
        entry["machine"]["load"] = [min(loads), max(loads)] if loads else None
    entry["summary"] = summary(entry["runs"])
    data.parent.mkdir(parents=True, exist_ok=True)
    data.write_text(json.dumps([e for e in old if e["version"] != entry["version"]] + [entry], indent=1) + "\n")


def main():
    global TIMEOUT
    ap = argparse.ArgumentParser()
    ap.add_argument("--satex", default="target/fast/satex")
    ap.add_argument("--list", default="benches/compile.txt")
    ap.add_argument("--only", help="comma-separated substrings of the documents to run")
    ap.add_argument("--big", default="", help="generated documents, CLASS-PAGES, comma-separated")
    ap.add_argument("--big-engine", default="pdftex")
    ap.add_argument("--tools", default=",".join(TOOLS))
    ap.add_argument("--scenarios", default=",".join(SCENARIOS))
    ap.add_argument("--repeats", type=int, default=5)
    ap.add_argument("--big-repeats", type=int, default=3)
    ap.add_argument("--timeout", type=int, default=TIMEOUT)
    ap.add_argument("--work", default="target/bench-compile")
    ap.add_argument("--data", default="doc/site/bench/compile.json")
    ap.add_argument("--version", default=None)
    ap.add_argument("--commit", default=None)
    args = ap.parse_args()
    TIMEOUT = args.timeout
    args.scenarios = args.scenarios.split(",")
    args.satex = str(Path(args.satex).resolve())
    args.work = str(Path(args.work).resolve())
    tools = args.tools.split(",")
    # adopt orphans (warm engines), so their CPU is counted and they are reaped
    ctypes.CDLL(None).prctl(36, 1, 0, 0, 0)
    env = {**os.environ, **ENV, "SATEX_CACHE": str(Path(args.work) / "satex-cache")}
    if args.version is None:
        args.version = re.search(r'^version\s*=\s*"([^"]+)"', Path("Cargo.toml").read_text(), re.M).group(1)
    if args.commit is None:
        sha = subprocess.run(["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
        dirty = subprocess.run(["git", "status", "--porcelain", "--untracked-files=no"], capture_output=True, text=True).stdout
        args.commit = sha + ("+dirty" if dirty.strip() else "")
    lines = [l.split() for l in Path(args.list).read_text().splitlines() if l.strip() and not l.startswith("#")]
    docs = [(l[0], l[1], "docs") for l in lines if not args.only or any(o in l[0] for o in args.only.split(","))]
    docs += [(f"gen:{b}", args.big_engine, "big") for b in args.big.split(",") if b]
    loads, runs = [], []
    for doc, engine, group in docs:
        runs += cell(args, doc, engine, group, tools, env, loads)
    entry = {"version": args.version, "date": time.strftime("%Y-%m-%d"), "commit": args.commit,
             "machine": machine(loads), "clock": f"SOURCE_DATE_EPOCH={EPOCH} FORCE_SOURCE_DATE=1",
             "repeats": args.repeats, "tools": {
                 "latexmk": "latexmk -pdf|-lualatex|-xelatex -interaction=nonstopmode",
                 "engine": "the engine rerun as latexmk would: BibTeX/Biber/makeindex on changed input, again while the build files change",
                 "satex": "satex compile --system satex: own fixpoint, preamble format, no pass when nothing read changed; explains the log, with the analysis where a message needs it",
                 "satex-build": "satex compile --system satex, compile.explain=false: the PDF only",
                 "satex-warm": "satex-build with compile.warm=true: a pdfTeX waits on its format for the next build"},
             "scenarios": {
                 "cold": "fresh copy, no build files; OS file cache, font cache and satex package caches primed",
                 "warm": "rebuild with nothing changed, after a full build",
                 "edit": "rebuild after one body word changed, after a full build"},
             "runs": runs}
    merge(Path(args.data), entry)
    print(f"wrote {args.data}", file=sys.stderr)


if __name__ == "__main__":
    main()
