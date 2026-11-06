#!/usr/bin/env python3
"""Builds the project site and its playground from the repository.

Nothing here is copied text: pages come from the Markdown in the tree, the
navigation from the wiki's _Sidebar.md, the version from Cargo.toml, the logo
from doc/logo/logo.tex, the playground's TeX files from what `satex query
files` says the samples and the offered packages read, and its caches from `satex cache export`.

    build.py --satex target/release/satex --wasm target/wasm32-unknown-emscripten/release --out site-out
"""

import argparse, math, unicodedata, base64, concurrent.futures, datetime, gzip, hashlib, html, inspect, json, os, pickle, re, shutil, signal, subprocess, sys, tempfile, threading, time, tomllib, urllib.parse
from pathlib import Path

import semantics as sem

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "doc" / "site"


def proc(cmd, **kw):
    """`subprocess.run`, with a satex run holding one of the JOBS slots: every step's pool may be
    as wide as it likes, never more than JOBS satex processes run at once. Its seconds go to RUNS."""
    if not (SATEX and cmd and str(cmd[0]) == str(SATEX)):
        return subprocess.run(cmd, **kw)
    with SLOTS:
        start = time.monotonic()
        try:
            return subprocess.run(cmd, **kw)
        finally:
            RUNS.append((time.monotonic() - start, " ".join(map(str, cmd[1:]))))


def run(*cmd, **kw):
    return proc(cmd, check=True, text=True, capture_output=True, **kw).stdout


def digest(data):
    return hashlib.sha256(data).hexdigest()[:12]


def available_memory():
    """The memory free for new work (Linux's MemAvailable), else half of all of it."""
    try:
        return int(re.search(r"MemAvailable:\s+(\d+) kB", Path("/proc/meminfo").read_text())[1]) * 1024
    except (OSError, TypeError):
        return os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES") // 2 if hasattr(os, "sysconf") else 0


# How many satex runs go at once: each takes a core and up to 2 GB of the memory free now;
# SATEX_SITE_JOBS overrides.
JOBS = int(os.environ.get("SATEX_SITE_JOBS") or max(1, min(os.cpu_count() or 2, available_memory() // 2**31)))
SLOTS, RUNS = threading.BoundedSemaphore(JOBS), []  # see proc()


BOARD = {"lines": {}, "drawn": 0, "lock": threading.Lock()}  # the steps running side by side, a bar each


def board(label, line, final=False):
    """Redraw every running step's bar in place, one line each; a finished step's line stays above them."""
    width = shutil.get_terminal_size().columns - 1
    with BOARD["lock"]:
        out = f"\x1b[{BOARD['drawn']}F" if BOARD["drawn"] else "\r"
        if final:
            BOARD["lines"].pop(label, None)
            out += f"\x1b[K{line[:width]}\n"
        else:
            BOARD["lines"][label] = line
        out += "".join(f"\x1b[K{l[:width]}\n" for l in BOARD["lines"].values()) + "\x1b[J"
        BOARD["drawn"] = len(BOARD["lines"])
        print(out, end="", file=sys.stderr, flush=True)


def parallel(label, jobs, f, workers=None):
    """`f` of each job, in order, JOBS at a time. A terminal gets a bar naming the running jobs;
    a log gets a line per tenth."""
    jobs, out, done, running, lock = list(jobs), {}, 0, {}, threading.Lock()
    tty, start = sys.stderr.isatty(), time.monotonic()
    name = lambda job: " ".join(dict.fromkeys(getattr(x, "name", x) for x in job if isinstance(x, (str, Path)))) \
        if isinstance(job, tuple) else str(job)

    def show():
        took = int(time.monotonic() - start)
        bar = "#" * (20 * done // max(1, len(jobs)))
        line = f"playground: {label} [{bar:·<20}] {done}/{len(jobs)} {took // 60}m{took % 60:02d}s"
        if tty:
            names = sorted(running.values())
            if names and done < len(jobs):
                line += " · " + ", ".join(names[:3]) + (f" +{len(names) - 3}" if len(names) > 3 else "")
            board(label, line, final=done == len(jobs))
        elif done == len(jobs) or done % max(1, len(jobs) // 10) == 0:
            print(line, file=sys.stderr, flush=True)

    def tracked(i):
        with lock:
            running[i] = name(jobs[i])
            if tty:
                show()
        try:
            return f(jobs[i])
        finally:
            with lock:
                del running[i]

    stop = threading.Event()

    def tick():  # the clock moves while long jobs run
        while not stop.wait(1):
            with lock:
                if done < len(jobs):
                    show()

    if tty:
        threading.Thread(target=tick, daemon=True).start()
    with concurrent.futures.ThreadPoolExecutor(workers or JOBS) as pool:
        futures = {pool.submit(tracked, i): i for i in range(len(jobs))}
        for finished in concurrent.futures.as_completed(futures):
            out[futures[finished]] = finished.result()
            with lock:
                done += 1
                show()
    stop.set()
    return [out[i] for i in range(len(jobs))]
# What `satex query files` listed, by what it depends on: the document, the
# flags, the project config and the installation (its ls-R files). A listing
# does not depend on the satex build, so a rebuild reuses it; --fresh drops it.
LISTINGS = {}
LISTINGS_FILE = None  # where they are kept between builds: written after each step, so a stopped build keeps what it listed


def save_listings():
    if LISTINGS_FILE:
        tmp = LISTINGS_FILE.with_suffix(".tmp")
        tmp.write_text(json.dumps(LISTINGS))
        tmp.replace(LISTINGS_FILE)
# The satex binary's digest, its cache key (what the analysis is: reporting code
# aside) and whether to ignore remembered results (--fresh): set by main().
BINARY, KEY, FRESH, SATEX = "", "", False, None
MEMO_USED = set()  # the memo files this build asked for


def prune_memo():
    """Drop the memo files this build superseded: those of a label it asked for under other parts.
    A label it did not ask for (a step it reused whole) keeps its files."""
    for d in {p.parent for p in MEMO_USED}:
        asked = {p.name.rsplit("-", 1)[0] for p in MEMO_USED if p.parent == d}
        for p in d.glob("*.pkl"):
            if p not in MEMO_USED and p.name.rsplit("-", 1)[0] in asked:
                p.unlink(missing_ok=True)


def memo(work, label, parts, make):
    """`make()`, remembered in WORK/memo under `label` while `parts` (what it depends on) stay the
    same: a build that changes only the page's code reuses every satex run."""
    path = work / "memo" / f"{label}-{digest(json.dumps(parts, default=str).encode())}.pkl"
    MEMO_USED.add(path)
    if path.exists() and not FRESH:
        return pickle.loads(path.read_bytes())
    value = make()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(pickle.dumps(value))
    return value


def binary_digest(satex):
    """The binary's digest without the build time and commit it names (`satex --version`):
    relinking the same code keeps every run keyed on it."""
    data = satex.read_bytes()
    about = json.loads(subprocess.run([str(satex), "--format", "json", "--version"], capture_output=True, text=True).stdout or "{}")
    for stamp in filter(None, (about.get("built"), about.get("commit"))):
        data = data.replace(stamp.encode(), b"")
    return digest(data)


def tree_digest(root):
    """What the text files under `root` say."""
    return digest(b"".join(p.name.encode() + p.read_bytes() for p in sorted(root.rglob("*"))
                           if p.is_file() and p.suffix in (".tex", ".bib", ".sty", ".cls", ".yaml", ".cfg", ".def", ".txt")))


def installation_stamp():
    lsr = run("kpsewhich", "--all", "ls-R").split()
    return ";".join(f"{p}:{os.stat(p).st_mtime_ns}" for p in sorted(lsr))


def files_read(satex, flags, doc, cwd, env, config=b"", absent=False):
    """The paths `satex query files` lists for `doc` (read, or found by `\\openin`), reused while nothing
    it depends on changed; with `absent`, the names it looked for and did not find instead."""
    # the paths it lists are absolute: another checkout's listing names another checkout's files
    key = digest(json.dumps([flags, str(doc), str(cwd), doc.read_bytes().decode(errors="replace"), config.decode(errors="replace"),
                             LISTINGS.get("@stamp", "")]).encode())
    if key not in LISTINGS:
        done = proc([str(satex), *flags, "query", "files", "-f", str(doc), "--format", "json"],
                              cwd=cwd, env=env, capture_output=True, text=True)
        LISTINGS[key] = [entry.get("path") or f"?{entry['file']}" for entry in json.loads(done.stdout or "[]")
                         if "path" in entry or entry.get("kind") == "absent"]
    return [e[1:] for e in LISTINGS[key] if e.startswith("?")] if absent else [e for e in LISTINGS[key] if not e.startswith("?")]


def kpsewhich(names, cwd):
    """Where TeX Live has each of `names`: name -> path, for those it has."""
    # a name spelled from unknown text holds marker characters no file name has
    names = [n for n in names if n and n.isprintable()]
    found = subprocess.run(["kpsewhich", *names], cwd=cwd, capture_output=True, text=True).stdout.split() if names else []
    return {Path(p).name: Path(p).resolve() for p in found}


MINIFY, SCRATCH = True, None
ESBUILD = None


def esbuild():
    """The esbuild binary already installed (ESBUILD, PATH or the VS Code extension's node_modules), or None."""
    global ESBUILD
    if ESBUILD is None:
        found = os.environ.get("ESBUILD") or shutil.which("esbuild")
        local = ROOT / "editors/vscode/node_modules/.bin/esbuild"
        ESBUILD = found or (str(local) if local.is_file() else "")
        if not ESBUILD:
            print("minify: no esbuild found, scripts and styles stay as written", file=sys.stderr)
    return ESBUILD or None


def minify_js(code, name):
    """(minified code, source map) of a script, or (code, None) without esbuild. The map embeds the source."""
    tool = esbuild()
    if not tool:
        return code, None
    with tempfile.TemporaryDirectory(dir=SCRATCH) as tmp:
        src, dst = Path(tmp) / name, Path(tmp) / "o.js"
        src.write_bytes(code)
        subprocess.run([tool, "--minify", "--sourcemap=external", "--legal-comments=none", "--log-level=error", f"--outfile={dst}", str(src)],
                       capture_output=True, check=True)
        out = dst.read_bytes()
        out = re.sub(rb"\n?//# sourceMappingURL=[^\n]*\n?$", b"", out)
        return out, (Path(str(dst) + ".map")).read_bytes()


def minify_css(code):
    tool = esbuild()
    if not tool:
        return code
    return subprocess.run([tool, "--minify", "--loader=css", "--log-level=error"], input=code, capture_output=True, check=True).stdout


HTML_TOKEN = re.compile(r"(<(pre|textarea|script|style|code)\b.*?</\2\s*>|<!--.*?-->|<[a-zA-Z/!](?:\"[^\"]*\"|'[^']*'|[^>\"'])*>)", re.S | re.I)


def minify_html(text):
    """Whitespace runs between tags become one space, comments go. Attributes, <pre>, <code>, <textarea>, <script> and <style> stay as written."""
    if not MINIFY:
        return text
    parts = HTML_TOKEN.split(text)
    out = []
    for i in range(0, len(parts), 3):
        out.append(re.sub(r"\s+", " ", parts[i]))
        if i + 1 < len(parts):
            out.append("" if parts[i + 1].startswith("<!--") else parts[i + 1])
    return "".join(out)


def minify_script(code):
    """A script embedded in a manifest: minified, no map."""
    return minify_js(code.encode(), "worker.js")[0].decode() if MINIFY else code


def emit(out, name, data):
    """Writes `data` under a content-addressed name, so it can be cached forever.
    Scripts and styles are minified first, the hash is of the shipped bytes, a script gets a source map beside it."""
    stem, ext = name.split(".", 1)
    smap = None
    if MINIFY and ext == "js":
        data, smap = minify_js(data, Path(name).name)
    elif MINIFY and ext == "css":
        data = minify_css(data)
    path = f"{stem}-{digest(data)}.{ext}"
    if smap:
        data += f"\n//# sourceMappingURL={Path(path).name}.map\n".encode()
        (out / path).parent.mkdir(parents=True, exist_ok=True)
        (out / f"{path}.map").write_bytes(smap)
    (out / path).parent.mkdir(parents=True, exist_ok=True)
    (out / path).write_bytes(data)
    return path


def emit_local(out, name, data):
    """emit(), plus a classic script setting the file as base64: a page opened
    from disk can neither fetch it nor hand its URL to a worker."""
    path = emit(out, name, data)
    (out / f"{path}.js").write_text(f'(self.SATEX_FILES ||= {{}})["{path}"] = "{base64.b64encode(data).decode()}";\n')
    return path


def version(cargo):
    """The crate's version, starred when the build is not that release: a later or changed tree."""
    tag = subprocess.run(["git", "describe", "--tags", "--dirty"], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    if tag == f"v{cargo['version']}":
        return cargo["version"], "this release"
    return f"{cargo['version']}*", f"pre-release: built from {tag or 'an untagged tree'}, not the release itself"


def pages():
    """Repository Markdown -> site path: the README is the guide, wiki pages live under wiki/."""
    table = {"README.md": "guide.html", "SEMANTICS.md": "SEMANTICS.html",
             "doc/chktex-coverage.md": "chktex-coverage.html", "doc/developing.md": "developing.html", "doc/credits.md": "credits.html", "CHANGELOG.md": "changelog.html"}
    for md in sorted((ROOT / "doc/wiki").glob("*.md")):
        if not md.name.startswith("_"):
            table[f"doc/wiki/{md.name}"] = f"wiki/{md.stem}.html"
    return table


def wiki(table):
    """GitHub wiki page names, as `Chktex-Coverage` or `Home`, to site pages."""
    return {Path(src).stem.lower(): dst for src, dst in table.items()} | {"home": "guide.html"}


def sidebar(table):
    """The wiki's _Sidebar.md, its GitHub wiki names resolved to site pages."""
    by_name = wiki(table)
    text = (ROOT / "doc/wiki/_Sidebar.md").read_text()
    text = re.sub(r"<img[^>]*>\s*", "", text)

    def target(m):
        name, _, anchor = m.group(2).partition("#")
        dst = by_name.get(name.lower())
        if dst is None:
            return m.group(0)
        return f"[{m.group(1)}](@ROOT@{dst}{'#' + anchor if anchor else ''})"
    return re.sub(r"\[([^\]]*)\]\(([^)\s]+)\)", target, text)


def chapters(table):
    """The Semantics section of the sidebar in reading order: (title, site page) of its top-level entries."""
    text = (ROOT / "doc/wiki/_Sidebar.md").read_text()
    block = text.split("[📖 Semantics]", 1)[1].split("\n\n", 1)[0]
    by_name = wiki(table)
    return [(m[1], by_name[m[2].split("#")[0].lower()]) for m in re.finditer(r"^- \[([^\]]+)\]\(([^)]+)\)", block, flags=re.M)]


def chapter_nav(dst, order):
    """The strip above and below a chapter of the semantics: where it stands, the previous and the next one."""
    pages_ = [d for _, d in order]
    if dst not in pages_:
        return ""
    i, root = pages_.index(dst), "../" * dst.count("/")
    link = lambda j, arrow: f'<a href="{root}{order[j][1]}">{arrow}</a>' if 0 <= j < len(order) else ""
    return (f'<nav class="chapters" aria-label="chapters of the semantics"><span>Chapter {i + 1} of {len(order)}</span>'
            f'{link(i - 1, "&larr; " + html.escape(order[i - 1][0].split(":")[0]) if i else "")}'
            f'{link(i + 1, html.escape(order[i + 1][0].split(":")[0]) + " &rarr;" if i + 1 < len(order) else "")}</nav>')


MERMAID = ('<script type="module">import m from "https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.esm.min.mjs";'
           'm.initialize({ startOnLoad: false, theme: "neutral", flowchart: { nodeSpacing: 14, rankSpacing: 26, padding: 6, curve: "basis", useMaxWidth: false } }); m.run();</script>')


def markdown(text):
    """The page's HTML; a Mermaid block becomes a diagram drawn in the browser."""
    body = run("pandoc", "-f", "gfm", "-t", "html5", "--syntax-highlighting=none", "--wrap=none", input=text)
    body, charts = re.subn(r'<pre class="mermaid"><code>(.*?)</code></pre>', r'<pre class="mermaid">\1</pre>', body, flags=re.S)
    return body + MERMAID if charts else body


def rewrite(body, source, table, homepage, repo, out, root):
    """Points links at the site: Markdown pages to their HTML, the site's own
    absolute URLs to relative ones, repository files to copies or GitHub."""
    base = Path(source).parent
    site_pages = {dst.removesuffix(".html"): dst for dst in table.values()}
    wiki_names = wiki(table)

    def fix(m):
        attr, url = m.group(1), html.unescape(m.group(2))
        if url.startswith(homepage):
            rest, _, anchor = url[len(homepage):].partition("#")
            dst = site_pages.get(rest.strip("/"), "index.html" if not rest.strip("/") else rest)
            return f'{attr}="{root}{dst}{"#" + anchor if anchor else ""}"'
        if re.match(r"^[a-z]+:|^#|^/", url):
            return m.group(0)
        path, _, anchor = url.partition("#")
        resolved = os.path.normpath(base / path).replace(os.sep, "/")
        anchor = "#" + anchor if anchor else ""
        if resolved in table:
            return f'{attr}="{root}{table[resolved]}{anchor}"'
        if "." not in path and "/" not in path and path.lower() in wiki_names:  # a GitHub wiki link
            return f'{attr}="{root}{wiki_names[path.lower()]}{anchor}"'
        if (ROOT / resolved).is_file() and not resolved.endswith(".md"):
            (out / resolved).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / resolved, out / resolved)
            return f'{attr}="{root}{resolved}{anchor}"'
        kind = "tree" if (ROOT / resolved).is_dir() else "blob"
        if resolved.startswith(".."):  # GitHub-relative, as `../../releases`
            return f'{attr}="{repo}/{url.lstrip("./")}"'
        return f'{attr}="{repo}/{kind}/main/{resolved}{anchor}"'
    return re.sub(r'\b(href|src)="([^"]*)"', fix, body)


def strip_banner(text):
    """Drops the generated marker and the centered wordmark the header already shows."""
    text = re.sub(r"^<!--.*?-->\s*", "", text, flags=re.S)
    return re.sub(r'^<p align="center"><a href="[^"]*"><img [^>]*></a></p>\s*', "", text)


def title_of(text):
    m = re.search(r"^# (.+)$", text, flags=re.M)
    return m.group(1).strip() if m else "satex"


def intro(text):
    """The README's opening paragraphs, up to its first rule, with its link definitions."""
    refs = "\n".join(re.findall(r"^\[[^\]]+\]: .*$", text, flags=re.M))
    return strip_banner(text).split("\n----", 1)[0] + "\n\n" + refs


def section(text, heading):
    """The README section under `## heading`, up to the next heading or `<details>`."""
    body = text.split(f"\n## {heading}\n", 1)[1]
    return re.split(r"\n(?:#+ |<details>)", body, maxsplit=1)[0].strip()


def install(text):
    """The README's install and editor sections, for the landing page."""
    return section(text, "Install") + "\n\n" + section(text, "Editor integration")


def logo(work):
    """doc/logo/logo.tex, typeset and converted to SVG without its white page.
    Pages: 3 the wordmark, 4 and 5 the tagline's lines (shown side by side, as page 2), 6 the plain name."""
    shutil.copy2(ROOT / "doc/logo/logo.tex", work / "logo.tex")
    run("pdflatex", "-interaction=nonstopmode", "-halt-on-error", "logo.tex", cwd=work)
    svgs = {}
    for page, name in (("3", "mark"), ("4", "line1"), ("5", "line2"), ("6", "name")):
        run("pdftocairo", "-svg", "-f", page, "-l", page, "logo.pdf", f"logo-{name}.svg", cwd=work)
        text = (work / f"logo-{name}.svg").read_text()
        # the white page and the line's near-white backdrop go: the logo sits on the page's own background
        svgs[name] = re.sub(r'<path[^>]*fill="rgb\(9[5-9][.\d]*%, 9[5-9][.\d]*%, (?:9[5-9][.\d]*|100)%\)"[^>]*/>|<path[^>]*fill="rgb\(100%, 100%, 100%\)"[^>]*/>', "", text).encode()
    run("pdftocairo", "-png", "-f", "6", "-l", "6", "-singlefile", "-transp", "-scale-to", "64", "logo.pdf", "favicon", cwd=work)
    # the favicon follows the browser's scheme: dark text on light, light on dark
    style = "<style>*{fill:#14213d}@media (prefers-color-scheme:dark){*{fill:#f2f4f8}}</style>"
    svgs["favicon"] = re.sub(rb"(<svg[^>]*>)", lambda m: m.group(1) + style.encode(), svgs["name"], count=1)
    svgs["favpng"] = (work / "favicon.png").read_bytes()
    return svgs


EXAMPLES = SITE / "examples"


def roots(samples):
    # a LaTeX document, or one whose magic comment names its engine (a plain TeX file)
    root = lambda t: "\\documentclass" in t or re.search(r"^%\s*!T[eE]X\s+(?:TS-)?program\s*=", t, re.M)
    return sorted(p.name for p in samples.glob("*.tex") if root(p.read_text(errors="replace")))


def sources():
    """The documents the playground offers: the site's examples, then the repository's samples."""
    out = [EXAMPLES / n for n in roots(EXAMPLES)] + [ROOT / "samples" / n for n in roots(ROOT / "samples")]
    # the playground keys its documents by name
    twice = sorted({d.name for d in out if [e.name for e in out].count(d.name) > 1})
    if twice:
        sys.exit(f"build.py: an example and a sample share a name: {', '.join(twice)}")
    return out


# The kernels the playground offers: a name, a label, the flags that select it.
# pdfTeX comes with the page; each other one is a bundle of its own, fetched when chosen.
KERNELS = [("pdftex", "pdfTeX", ["--set", "engine=pdftex"]), ("xetex", "XeTeX", ["--set", "engine=xetex"]),
           ("plain", "plain TeX", ["--set", "engine=tex", "--set", "kernel=plain"]),
           ("luatex", "LuaTeX", ["--set", "engine=luatex"])]
# Engines offered on another kernel's bundle, never built or run on their own: (id, label, the kernel they use).
# satex has no ConTeXt kernel to read, so ConTeXt runs on LuaTeX's.
ALIASES = [("context", "ConTeXt", "luatex")]
# Kernels that are offered but flagged, with the badge and the plain statement shown in the menu.
EXPERIMENTAL = {"luatex": ("experimental", "Experimental. LaTeX documents run on the LuaTeX engine model. Lua code (\\directlua, luacode) is not executed, only modelled, "
                           "and fontspec and other LuaTeX-only packages may report spurious findings."),
                "context": ("experimental", "Experimental. Runs on the LuaTeX bundle. satex does not interpret the ConTeXt kernel: it recognizes \\starttext documents "
                            "and models the LuaTeX primitives, so ConTeXt commands are unknown and findings may be spurious or missing.")}
# Listed, not offered: why each one is missing.
UNAVAILABLE = []


SAMPLE = "hello.tex"  # the document the playground opens with: the first download holds what it reads


def file_of(name):
    """The file a package (`name`) or class (`name.cls`) is."""
    return name if name.endswith(".cls") else f"{name}.sty"


def first_download():
    """packages.txt's first section: fetched with the first download, not on demand."""
    section = re.search(r"^# with the first download.*\n((?:[^#\n].*\n)+)", (SITE / "packages.txt").read_text(), re.M)
    return section.group(1).split() if section else []


def probe_doc(probes, name):
    stem = name.removesuffix(".cls")
    head = f"\\documentclass{{{stem}}}" if name.endswith(".cls") else f"\\documentclass{{article}}\n\\usepackage{{{stem}}}"
    doc = probes / f"{stem}.tex"
    doc.write_text(head + "\n\\begin{document}\nText.\n\\end{document}\n")
    return doc


def bundle(satex, work):
    """What the browser downloads, by the same steps for every kernel and package.
    A kernel's bundle: the TeX files the default sample reads, laid out like texmf-dist,
    and the cache entries (`satex cache export`) running it builds. A package's or class's
    bundle, for each kernel: the files and cache entries a document loading it adds to those,
    less what the bundles of the packages it loads itself hold (its `needs`).
    Returns the kernels' file tables, the packages' entries, the sample names and the project."""
    docs = sources()
    dist = Path(run("kpsewhich", "-var-value", "TEXMFDIST").strip()).resolve()
    tree, project, probes = work / "texmf.bundle", work / "work", work / "probes"
    for d in (tree, project, probes):
        shutil.rmtree(d, ignore_errors=True)
    probes.mkdir(parents=True)
    env = dict(os.environ, SATEX_CACHE=str(work / "cache-install"))
    bundled = lambda p: f"/texmf.bundle/{p.relative_to(dist).as_posix()}"

    def listed(job):
        flags, doc, cwd = job
        read = {(cwd / e).resolve() for e in files_read(satex, flags, doc, cwd, env) if (cwd / e).resolve().is_relative_to(dist)}
        # a file it looked for that TeX Live has goes with it too: without it the playground's run takes another path
        read |= {p for p in kpsewhich(files_read(satex, flags, doc, cwd, env, absent=True), work).values() if p.is_relative_to(dist)}
        # What another option would read instead (`[11pt]`: size11.clo for size10.clo, `\Large\bfseries`:
        # cmbx12.tfm for cmr10.tfm): the files of the same kind beside each one read. Classes, packages
        # and documents load by name.
        option = lambda p: (p.relative_to(dist).parts[0] == "tex" and p.suffix not in (".sty", ".cls", ".tex", ".ltx")) or p.suffix in (".tfm", ".vf")
        # A package's own images (mwe's example-image): \includegraphics finds them on the tree's ls-R.
        images = {q for p in read if p.suffix == ".sty" for q in p.parent.iterdir() if q.suffix in (".pdf", ".png", ".jpg")}
        return read | images | {q for p in read if option(p) for q in p.parent.glob(f"*{p.suffix}")}

    absent = {}
    reads = {}  # (kernel, document or package) -> the distribution files it reads
    # Names the installation has (test fixtures' names are not); plain TeX loads no LaTeX package.
    installed = lambda names: [n for n in names if file_of(n) in set(Path(p).name for p in subprocess.run(
        ["kpsewhich", *map(file_of, names)], capture_output=True, text=True).stdout.split())] if names else []
    probe = lambda names: [(kernel, flags, name) for kernel, _, flags in KERNELS if kernel != "plain" for name in names]

    def listing(job):
        kernel, flags, it = job
        return listed((flags, it, ROOT) if isinstance(it, Path) else (flags, probe_doc(probes, it), probes))

    def take(jobs, label):
        done = parallel(label, jobs, listing)
        save_listings()
        for (kernel, _, it), paths in zip(jobs, done):
            if isinstance(it, Path):
                reads[(kernel, it.name)] = paths
            elif file_of(it) in {p.name for p in paths}:
                reads[(kernel, it)] = paths
            else:
                absent.setdefault(kernel, []).append(it)

    # The listed packages go beside the examples; the packages an example reads besides go after.
    listed_names = installed(package_names())
    take([(kernel, flags, doc) for kernel, _, flags in KERNELS for doc in docs] + probe(listed_names), "[4/4] files read by examples and packages")
    more = installed(list(dict.fromkeys(sorted(f"{p.stem}.cls" if p.suffix == ".cls" else p.stem
                                               for (_, it), paths in list(reads.items()) if it in {d.name for d in docs}
                                               for p in paths if p.suffix in (".sty", ".cls")))))
    more = [n for n in more if n not in listed_names]
    take(probe(more), "[4/4] files read by their packages")
    # tikz's libraries are named in \usetikzlibrary, so every one is in its bundle, not only those a probe reads
    libraries = {q for p in dist.glob("tex/*/pgf") for q in p.rglob("*") if re.fullmatch(r"(?:tikz|pgf)library.*\.code\.tex", q.name)}
    for key in reads:
        if key[1] == "tikz":
            reads[key] = reads[key] | libraries
    every = listed_names + more
    for kernel, names in absent.items():
        print(f"playground: {len(names)} packages not offered under {kernel} (they do not load there)", file=sys.stderr)
    # the code points the typeset text reads encodings through (`query.rs` `encoding_slots`)
    unicode = Path(run("kpsewhich", "tuenc.def").strip())
    for path in set().union(*reads.values()) | ({unicode.resolve()} if unicode.is_file() else set()):  # the build tree: every file any of them reads
        target = tree / path.relative_to(dist)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)
    if (dist.parent / "release-texlive.txt").is_file():  # names the TeX Live year
        shutil.copy2(dist.parent / "release-texlive.txt", tree / "release-texlive.txt")
    # Files no listing finds (an engine or Lua reads them): bundle-extra.txt names them by tree and glob.
    extra = {}  # path in the bundle -> file
    for line in re.sub(r"#.*", "", (SITE / "bundle-extra.txt").read_text()).splitlines():
        if line.split():
            var, pattern = line.split(maxsplit=1)
            root = Path(run("kpsewhich", "-var-value", var).strip()).resolve()
            extra.update({path.relative_to(root).as_posix(): path for path in sorted(root.glob(pattern.strip())) if path.is_file()})
    for rel, path in extra.items():
        (tree / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, tree / rel)
    config = (SITE / "playground.yaml").read_text().replace("/texmf.bundle", str(tree))
    shutil.copytree(ROOT / "samples", project, ignore=shutil.ignore_patterns("satex.yaml"))
    for doc in docs[:len(roots(EXAMPLES))]:
        shutil.copy2(doc, project / doc.name)
    for doc in sources():
        for companion in projects()[doc.name]:
            shutil.copy2(doc.parent / companion, project / companion)
    (project / "satex.yaml").write_text(config)
    (probes / "satex.yaml").write_text(config)
    # The check: each example, run over the bundle tree as the playground runs it, looks for no file TeX Live has
    # and the tree lacks (the playground's Files view calls what it looks for "absent in TeX Live too").
    looked = parallel("[4/4] examples over the bundle", [d for d in docs if d.suffix == ".tex"],
                      lambda d: files_read(satex, KERNELS[0][2], project / d.name, project, env, config=config.encode(), absent=True))
    # a gap is a warning, never a failed build: the playground then says the file is not in the playground
    lacking = sorted({f"{d.name}: {n}" for d, names in zip([d for d in docs if d.suffix == ".tex"], looked) for n in kpsewhich(names, work)})
    if lacking:
        print(f"playground: warning: examples look for TeX Live files the bundle lacks: {', '.join(lacking)}", file=sys.stderr)

    # Kernels: the sample's files and the cache entries of running it. A kernel's
    # cache directory stays from the last build: satex rebuilds what this binary cannot use, and
    # `cache export` writes only the valid entries.
    def kernel_caches(kernel, flags):
        # The kernel's archive holds the format and what the sample loads, nothing else: the shared
        # directory gathers every package any run met, which would ship with the first download.
        shared, alone = work / f"cache-{kernel}", work / f"cache-{kernel}-sample"
        shutil.rmtree(alone, ignore_errors=True)
        alone.mkdir(parents=True)
        shared.mkdir(parents=True, exist_ok=True)
        for f in shared.glob("format-*"):
            shutil.copy2(f, alone / f.name)
        kenv = dict(os.environ, SATEX_CACHE=str(alone))
        proc([str(satex), *flags, "lint", "-f", SAMPLE], cwd=project, env=kenv, capture_output=True, check=False)
        archive = work / f"satex-cache-{kernel}.bin"
        run(str(satex), "cache", "export", str(archive), env=kenv)
        # a format built here keeps the shared directory warm for the next build
        for f in alone.glob("format-*"):
            if not (shared / f.name).exists():
                shutil.copy2(f, shared / f.name)
        return archive

    # Packages: the files a probe document reads. The cache entries are those the examples' runs, from a
    # copy of the kernel's caches, made for a package, whichever document loaded it: a package's entry
    # holds the state of everything loaded before it, so a probe's own would rarely match a document's.
    def fresh(kernel, flags):
        """One cache archive per package or class held by the entries running every example made,
        from a copy of the kernel's caches (`satex cache export --only`)."""
        return memo(work, f"fresh-{kernel}", [KEY, kernel, flags, tree_digest(project), first_download()], lambda: fresh_run(kernel, flags))

    def fresh_run(kernel, flags):
        cache = work / f"cache-docs-{kernel}"
        shutil.rmtree(cache, ignore_errors=True)
        shutil.copytree(work / f"cache-{kernel}", cache, copy_function=os.link, ignore=shutil.ignore_patterns("*.lock"))
        penv = dict(os.environ, SATEX_CACHE=str(cache))
        # the first download's packages get entries even when no example loads them: a probe document's.
        # Not its classes: a KOMA class's entries are about 9 MiB gz each, too much for the first download.
        first = [probe_doc(probes, n) for n in installed(first_download()) if kernel != "plain" and not n.endswith(".cls")]
        # all kernels' runs share the JOBS slots (proc), so a kernel done early leaves no core idle
        with concurrent.futures.ThreadPoolExecutor(JOBS) as inner:
            list(inner.map(lambda job: proc([str(satex), *flags, "lint", "-f", job[1].name], cwd=job[0], env=penv,
                                            capture_output=True, check=False), [(project, d) for d in docs] + [(probes, d) for d in first]))
        out = work / f"export-{kernel}.bin"
        held = set(json.loads(run(str(satex), "--format", "json", "cache", "export", str(out), env=penv))["holds"].values())
        archives = {}
        for stem in sorted(held):
            run(str(satex), "cache", "export", str(out), "--only", stem, env=penv)
            archives[stem] = out.read_bytes()
        out.unlink()
        shutil.rmtree(cache, ignore_errors=True)
        return archives

    # Per kernel, its caches and then the packages' from a copy of them: kernels need not wait for each other.
    both = parallel("[4/4] kernel and package caches", KERNELS, lambda k: (kernel_caches(k[0], k[2]), fresh(k[0], k[2])))
    archives = [a for a, _ in both]
    ran = {k: r for (k, _, _), (_, r) in zip(KERNELS, both)}
    tables, have, names = [], set(), {d.name for d in docs}
    for (kernel, _, _), archive in zip(KERNELS, archives):
        # the sample's files, and what any example reads that no package's bundle holds (a heading's font)
        packaged = set().union(*(paths for (k, n), paths in reads.items() if k == kernel and n not in names))
        shown = set(reads[(kernel, SAMPLE)]).union(*(reads.get((kernel, d.name), set()) for d in docs)) - (packaged - set(reads[(kernel, SAMPLE)]))
        files = {bundled(p): p.read_bytes() for p in sorted(shown) if bundled(p) not in have}
        have |= set(files)
        tables.append({**files, f"/kernels/{kernel}/satex-cache.bin": archive.read_bytes()})
    kernels = [k for k, _, _ in KERNELS]

    parts = {}  # (kernel, name) -> [files, cache archive, direct needs]
    for (kernel, name) in reads:
        if name in {d.name for d in docs}:
            continue
        k = kernels.index(kernel)
        held = set(tables[0]) | set(tables[k])
        seen = {p.name for p in reads[(kernel, name)]}
        parts[(kernel, name)] = [
            {bundled(p): p for p in reads[(kernel, name)] if bundled(p) not in held},
            ran[kernel].get(name),
            # a package loads no class: the probe document's class is no need of it
            {x for x in every if x != name and (kernel, x) in reads and file_of(x) in seen and (name.endswith(".cls") or not x.endswith(".cls"))}]
    # needs: the packages it loads, transitively. Their items are not repeated here, unless they need this one back.
    need = {}
    for (kernel, name), (_, _, direct) in parts.items():
        reach, stack = set(), list(direct)
        while stack:
            x = stack.pop()
            if x not in reach:
                reach.add(x)
                stack += parts[(kernel, x)][2]
        need[(kernel, name)] = reach
    packages = []
    for (kernel, name), (files, cache, _) in sorted(parts.items()):
        covered = [parts[(kernel, x)] for x in need[(kernel, name)] if name not in need[(kernel, x)]]
        files = {f: p.read_bytes() for f, p in files.items() if not any(f in c[0] for c in covered)}
        parts[(kernel, name)][:2] = [files, cache]
    for (kernel, name), (files, cache, _) in sorted(parts.items()):
        if cache:
            files = {**files, f"/kernels/{kernel}/{name}/satex-cache.bin": cache}
        if files:
            packages.append((kernel, name, files, sorted(x for x in need[(kernel, name)] if parts[(kernel, x)][0] or parts[(kernel, x)][1])))
    for path in sorted(project.iterdir()):
        if path.is_file() and path.name != "satex.yaml":
            tables[0][f"/work/{path.name}"] = path.read_bytes()
    # The tree's file list (kpathsea's ls-R) names every file a bundle holds, fetched or not: completion
    # offers each package, and loading one the page has not fetched is a missing file it then fetches.
    for rel, path in extra.items():
        tables[0][f"/texmf.bundle/{rel}"] = path.read_bytes()
    held = sorted({p for files in [*tables, *(f for _, _, f, _ in packages)] for p in files if p.startswith("/texmf.bundle/")})
    listing = {}
    for p in held:
        d, _, f = p.removeprefix("/texmf.bundle/").rpartition("/")
        listing.setdefault(d, []).append(f)
    tables[0]["/texmf.bundle/ls-R"] = "".join(f"./{d}:\n" + "".join(f"{f}\n" for f in fs) + "\n" for d, fs in sorted(listing.items())).encode()
    if (tree / "release-texlive.txt").is_file():
        tables[0]["/texmf.bundle/release-texlive.txt"] = (tree / "release-texlive.txt").read_bytes()
    tables[0]["/work/satex.yaml"] = config.replace(str(tree), "/texmf.bundle").encode()
    tables[0]["/cache/previews.json"] = previews(satex, work, tree)
    return tables, packages, [p.name for p in docs], project


def previews(satex, work, tree):
    """Completion's pictures of the arrow tips and patterns the bundled TikZ libraries declare,
    drawn by TeX here (`satex query previews`): the browser has no TeX to draw them."""
    libs = sorted(p.name.removeprefix("tikzlibrary").removesuffix(".code.tex") for p in tree.rglob("tikzlibrary*.code.tex"))
    data = memo(work, "previews", [BINARY, libs], lambda: previews_run(satex, work, libs))
    PREVIEWS.update(json.loads(data))
    return data


# The pictures of arrow tips and patterns (`kind:name` -> SVG), for the playground's completion and the wiki's hover cards.
PREVIEWS = {}


def PREVIEW_NAMES():
    """The names (without their kind) that have a picture."""
    return {k.split(":", 1)[1] for k in PREVIEWS}


def previews_run(satex, work, libs):
    doc = work / "previews" / "previews.tex"
    doc.parent.mkdir(parents=True, exist_ok=True)
    doc.write_text("\\documentclass{article}\n\\usepackage{tikz}\n" + "".join(f"\\usetikzlibrary{{{l}}}\n" for l in libs)
                   + "\\begin{document}\n\\end{document}\n")
    env = dict(os.environ, SATEX_CACHE=str(work / "cache-previews"))
    rows = json.loads(run(str(satex), "--format", "json", "query", "previews", "-f", str(doc), cwd=doc.parent, env=env))
    return json.dumps({r["name"]: r["svg"] for r in rows if r["svg"]}).encode()


def providers(satex, work, missing):
    """The packages that define the control sequences `missing`, by `satex query`'s `providers`: the installed files whose
    text defines each name through a definer the run saw: `name.sty` for a package, `name.cls` for a class."""
    if not missing:
        return set()
    doc = work / "cards" / "providers.tex"
    doc.write_text("\\documentclass{article}\n\\begin{document}\n\\end{document}\n")
    request = json.dumps({"file": doc.name, "queries": [{"type": "providers", "names": sorted(missing)}]})
    env = dict(os.environ, SATEX_CACHE=str(work / "cache-pdftex"))
    done = proc([str(satex), *KERNELS[0][2], "query", "--request", "-", "-f", doc.name], input=request, cwd=doc.parent,
                          env=env, capture_output=True, text=True)
    try:
        rows = json.loads(done.stdout)[0]["records"]
    except (json.JSONDecodeError, IndexError, KeyError):
        return set()
    return {r["package"] for r in rows}


def scope_cards(satex, work, names):
    """What `satex scope --all` knows of the control sequences the pages name, for their hover cards: the records
    by name from a document loading the packages the pages use, and each engine's primitives. Memoized."""
    found = re.compile(r"\\(?:usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^{}#\\]*)\}")
    texts = [p.read_text() for p in [ROOT / "README.md", ROOT / "SEMANTICS.md", *(ROOT / "doc/wiki").glob("*.md")]]
    wanted = sorted({n.strip() for t in texts for g in found.findall(t) for n in g.split(",") if re.fullmatch(r"[A-Za-z][\w-]*", n.strip())})
    wanted = [n for n in wanted if subprocess.run(["kpsewhich", f"{n}.sty"], capture_output=True, text=True).stdout.strip()]
    tmp = work / "cards"
    tmp.mkdir(exist_ok=True)

    def scope(kernel, flags, text, extra=()):
        doc = tmp / f"{kernel}.tex"
        doc.write_text(text)
        env = dict(os.environ, SATEX_CACHE=str(work / f"cache-{kernel}"))
        done = proc([str(satex), *flags, "--format", "json", "scope", "--all", *extra, "-f", doc.name], cwd=tmp, env=env, capture_output=True, text=True)
        try:
            return json.loads(done.stdout)
        except json.JSONDecodeError:
            return []

    def probe(text):
        doc = tmp / "shapes.tex"
        doc.write_text(text)
        done = proc([str(satex), *KERNELS[0][2], "--format", "json", "query", "diagnostics", "-f", doc.name], cwd=tmp,
                              env=dict(os.environ, SATEX_CACHE=str(work / "cache-pdftex")), capture_output=True, text=True)
        try:
            return json.loads(done.stdout)
        except json.JSONDecodeError:
            return []

    def make():
        text = "\\documentclass{article}\n" + "".join(f"\\usepackage{{{n}}}\n" for n in wanted) + "\\begin{document}\n\\end{document}\n"
        records = {r["name"]: r for r in scope("pdftex", KERNELS[0][2], text)}
        # a name no loaded package defines: the installed packages that define it, each loaded alone
        for n in sorted(providers(satex, work, [x for x in names if x not in records and re.fullmatch(r"\\[A-Za-z@]+", x)])):
            # a class (beamer's \mode, \uncover) is the document's class, a package is loaded by article
            head = f"\\documentclass{{{n[:-4]}}}\n" if n.endswith(".cls") else f"\\documentclass{{article}}\n\\usepackage{{{n.removesuffix('.sty')}}}\n"
            more = scope("pdftex", KERNELS[0][2], head + "\\begin{document}\n\\end{document}\n")
            for r in more:
                if r["name"] in names:
                    records.setdefault(r["name"], r)
        # the usage lines by a probe of each card's name, its arguments named by role (`scope` reads parameter texts only)
        doc = tmp / "pdftex.tex"
        doc.write_text(text)
        asked = sorted(n for n in names if n in records and records[n].get("tag") != "primitive")
        env = dict(os.environ, SATEX_CACHE=str(work / "cache-pdftex"))
        done = proc([str(satex), *KERNELS[0][2], "--format", "json", "explain", *asked, "-f", doc.name], cwd=tmp, env=env,
                              capture_output=True, text=True) if asked else None
        try:
            seen = set()
            for r in json.loads(done.stdout) if done else []:
                # the first record is the meaning in force
                # the manual's names (`scope`'s usage) win over the probe's places
                if r.get("usage") and r["name"] in records and r["name"] not in seen and "⟨" not in records[r["name"]].get("usage", ""):
                    records[r["name"]]["usage"] = r["usage"]
                seen.add(r["name"])
        except json.JSONDecodeError:
            pass
        engines = {label: {r["name"] for r in scope(kernel, flags, "\\documentclass{article}\n\\begin{document}\n\\end{document}\n", ["--filter", "tag=primitive"])}
                   for kernel, label, flags in KERNELS if kernel != "plain"}
        return records, engines, templates(scope, probe, names)
    return memo(work, "cards-scope-keyed3", [BINARY, wanted, sorted(names)], make)


# Declarations whose names are made from a key: the probe declares one key, a record naming it is a template for any key.
KEYED = ["\\newcounter{KEY}", "\\newglossaryentry{KEY}{name={n},description={d}}", "\\newlabel{KEY}{{1}{1}}", "\\bibcite{KEY}{1}", "\\newif\\ifKEY"]


def templates(scope, probe, names):
    """[(regex, record, declaration)]: the names the run of each `KEYED` declaration defines, with the key a hole. `scope` lists the
    named definitions. A name a `\\csname` makes (`\\r@key`) is no definition there, so each shape `\\x@key` among `names` is asserted
    defined after the declaration in a probe, and kept where the assertion holds."""
    key = "qzk"
    uses = KEYED
    text = "\\documentclass{article}\n\\usepackage{glossaries}\n\\begin{document}\n" + "".join(u.replace("KEY", key) + "\n" for u in uses) + "\\end{document}\n"
    out = []
    for r in scope("pdftex", KERNELS[0][2], text):
        if r.get("package") == "document" and key in r["name"] and 4 <= r["line"] < 4 + len(uses):
            rx = re.compile("[A-Za-z@]+".join(map(re.escape, r["name"].split(key))))
            out.append((rx, r, uses[r["line"] - 4].replace("KEY", "⟨key⟩")))
    shapes = sorted({re.sub(r"[A-Za-z]+$", key, n) for n in names if re.fullmatch(r"\\[A-Za-z]@[A-Za-z]+", n)})
    for u in KEYED:
        body = "".join(f"\\@ifundefined{{{x[1:]}}}{{\\count@=0 }}{{\\count@=1 }}\n%\\satex@assert{{\\count@ = 1}}\n" for x in shapes)
        for d in probe("\\documentclass{article}\n\\makeatletter\n" + u.replace("KEY", key) + "\n" + body + "\\begin{document}\n\\end{document}\n"):
            if d.get("code") == "assert-holds" and d["line"] >= 5 and d["line"] % 2 == 1:
                x = shapes[(d["line"] - 5) // 2]
                out.append((re.compile("[A-Za-z]+".join(map(re.escape, x.split(key)))), {"tag": "macro"}, u.replace("KEY", "⟨key⟩")))
    return out


def source_fallback():
    """name -> card for a name no run defines, from the installed sources of what the text around it names: the files of
    the packages and formats its page mentions (`word.sty`, `.ltx`, `.cls`), the files it cites, `latex.ltx`, and satex's own
    sources. A definer there gives its place. A name only ever a macro's delimiter in `latex.ltx` says so. A string in satex's sources says that."""
    pages = [ROOT / "README.md", ROOT / "SEMANTICS.md", *(ROOT / "doc/wiki").glob("*.md")]
    texts = {p: p.read_text() for p in pages}
    kernel = Path(subprocess.run(["kpsewhich", "latex.ltx"], capture_output=True, text=True).stdout.strip())
    seen = {}

    def defined(path):
        if path not in seen:
            text = path.read_text(errors="replace")
            seen[path] = {}
            for n, k, at in sem.definitions(text):
                seen[path].setdefault(n, (k, text.count("\n", 0, at) + 1))
        return seen[path]

    def card(name):
        around = [t for t in texts.values() if re.search(re.escape(name) + r"(?![\w@])", t)]
        words = sorted({w for t in around for w in re.findall(r"[a-z][a-z0-9-]{2,}", t)})
        found = [Path(l) for i in range(0, len(words), 400) for ext in ("sty", "ltx", "cls")
                 for l in subprocess.run(["kpsewhich", *(f"{w}.{ext}" for w in words[i:i + 400])], capture_output=True, text=True).stdout.split()]
        cited = [ROOT / m for t in around for m in re.findall(r"[\w/.-]+\.tex", t) if (ROOT / m).is_file()]
        for path in [kernel]:
            if (hit := defined(path).get(name)):
                at = f"{path.relative_to(ROOT) if path.is_relative_to(ROOT) else path.name}:{hit[1]}"
                return {"k": hit[0], "at": at, "doc": f"No documentation found. Defined at `{at}`."}
        text = kernel.read_text(errors="replace")
        use = re.search(r"#\d" + re.escape(name) + r"(?![\w@])", text)
        if use and not re.search(r"\\(?:[gex]?def|let)\s*" + re.escape(name) + r"(?![\w@])", text):
            line = text.count("\n", 0, use.start()) + 1
            return {"k": "delimiter", "at": f"latex.ltx:{line}", "doc": f"A delimiter: the kernel ends macro parameters with it (`latex.ltx:{line}`) and never defines it."}
        for path in [*found, *cited]:
            if (hit := defined(path).get(name)):
                at = f"{path.relative_to(ROOT) if path.is_relative_to(ROOT) else path.name}:{hit[1]}"
                return {"k": hit[0], "at": at, "doc": f"No documentation found. Defined at `{at}`."}
        write = re.search(r"\\string" + re.escape(name) + r"(?![\w@])", text)
        if write:
            line = text.count("\n", 0, write.start()) + 1
            return {"k": "file entry", "at": f"latex.ltx:{line}", "doc": f"Not a macro: the kernel writes `{name}` into a file (`latex.ltx:{line}`) and never defines it. The program that reads the file does."}
        for path in sorted((ROOT / "src").rglob("*.rs")):
            src = path.read_text()
            if (at := src.find('"' + name.replace("\\", "\\\\") + '"')) >= 0:
                line = src.count("\n", 0, at) + 1
                above = [l.strip().removeprefix("///").strip() for l in src[:at].splitlines()[:-1][-8:] if l.strip().startswith("///")]
                return {"k": "marker", "at": f"{path.relative_to(ROOT)}:{line}", "doc": f"A marker satex looks for in a source, never run: {' '.join(above)}"}
        return None
    return card


def mine(satex, work, names):
    """The manuals' records for the packages `names`, mined from the installation (a `satex query`'s `docs` each)."""
    probes, flags = work / "manuals", KERNELS[0][2]
    probes.mkdir(exist_ok=True)
    env = dict(os.environ, SATEX_CACHE=str(work / "cache-install"))

    def mined(name):
        doc = probe_doc(probes, name)
        request = json.dumps({"file": doc.name, "queries": [{"type": "docs"}]})
        done = proc([str(satex), *flags, "query", "--request", "-", "-f", doc.name], input=request, cwd=probes, env=env,
                              capture_output=True, text=True)
        try:
            return json.loads(done.stdout)[0]["records"]
        except (json.JSONDecodeError, IndexError, KeyError):
            return []
    # beside the bundle's own steps: half the workers each
    return memo(work, "manuals", [BINARY, names], lambda: [r for rs in parallel("[4/4] package manuals", names, mined) for r in rs])


def manuals(satex, work, tables, packages, offered, records):
    """What the bundled packages' manuals say about their commands (`satex query`'s `docs`), mined here
    because the bundle has no documented sources: a `.satex-docs` file in each package's documentation
    directory, in the bundle (a kernel's or a package's) that holds the file it was mined for. `records`
    were mined for the `offered` packages, the rest are mined now. Returns the stems of the packages with a manual."""
    dist = Path(run("kpsewhich", "-var-value", "TEXMFDIST").strip()).resolve()
    # the installation itself, not the bundle's tree: its packages' documented sources are beside them
    rest = sorted({name for _, name, _, _ in packages if not name.endswith(".cls")} - set(offered))
    records = records + (mine(satex, work, rest) if rest else [])
    by_file = {}
    for r in records:
        by_file.setdefault(str(Path(r.pop("package_file")).resolve()), {}).setdefault(r.pop("dir"), {})[r["name"]] = r
    documented = set()
    for files in [*tables, *(f for _, _, f, _ in packages)]:
        for held in [p for p in files if p.startswith("/texmf.bundle/") and p.endswith((".sty", ".cls"))]:
            native = str(dist / held.removeprefix("/texmf.bundle/"))
            for d, rows in by_file.get(native, {}).items():
                if Path(d).resolve().is_relative_to(dist):
                    target = f"/texmf.bundle/{Path(d).resolve().relative_to(dist).as_posix()}/{Path(native).stem}.satex-docs"
                    files[target] = json.dumps(list(rows.values()), ensure_ascii=False).encode()
                    documented.add(Path(native).stem)
    return documented


def package_names():
    """The packages and classes to offer: those the tests' fixtures, the examples and samples
    load, satex.yaml's `cache.index.common`, then doc/site/packages.txt, each once.
    A class is `name.cls`; a name no installation file backs is dropped when probed."""
    name = r"[A-Za-z][\w.-]*"
    found = re.compile(r"\\+(documentclass|usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^{}#\\]*)\}")
    texts = [p.read_text(errors="replace") for pattern in ("tests/**/*.tex", "tests/**/*.rs", "tests/**/*.sty", "samples/*.tex", "doc/site/examples/*.tex")
             for p in sorted(ROOT.glob(pattern))]
    scanned = []
    for text in texts:
        for kind, names in found.findall(text):
            scanned += [n.strip() + (".cls" if kind == "documentclass" else "") for n in names.split(",") if re.fullmatch(name, n.strip())]
    listed = re.sub(r"#.*", "", (SITE / "packages.txt").read_text()).split()
    config = (ROOT / "satex.yaml").read_text()
    common = re.search(r"^\s*common:\s*\[([^\]]*)\]", config, flags=re.M)
    if not common and re.search(r"^\s*common:", config, flags=re.M):
        sys.exit("build.py: satex.yaml's cache.index.common must stay a [a, b, ...] flow list")
    configured = [n.strip() for n in (common.group(1).split(",") if common else []) if n.strip()]
    return list(dict.fromkeys(sorted(scanned) + configured + listed))


def commands():
    """`examples/commands.txt`: the command each example runs, `lint` unless named there."""
    named = dict(line.split(": ", 1) for line in (EXAMPLES / "commands.txt").read_text().splitlines() if line and not line.startswith("#"))
    return {name: named.get(name, "lint") for name in roots(EXAMPLES)}


def projects():
    """The files each document reads beside itself (`satex query files`): tabs next to it,
    analyzed together with it. Its own build files (`thesis.aux`) are not among them."""
    out = {}
    for doc in sources():
        read = {(doc.parent / e).resolve() for e in files_read(SATEX, KERNELS[0][2], doc, doc.parent, dict(os.environ))}
        here = doc.parent.resolve()
        out[doc.name] = sorted(p.name for p in read if p.parent == here and p.stem != doc.stem and p.exists())
    return out


TOKEN = re.compile(r"(?P<com>%.*)|(?P<cmd>\\(?:[A-Za-z@]+|.))|(?P<br>[{}\[\]])|(?P<num>\d+(?:\.\d+)?)")
# Findings about the typeset text itself: marked on their few characters, not only underlined.
TYPOGRAPHIC = re.compile(r"dash|spac|quote|ellips|ligature|hyphen|tie|punct")


def tex_html(line, n, lints=()):
    """Source line `n` (0-based) with the token classes of the editor's tokenizer (CodeMirror's
    stex mode), so it is colored without JS. A command carries its name (`data-cs`), the text
    a finding covers the finding's indices (`data-lint`)."""
    cover = []
    for k, d in enumerate(lints):
        (sl, sc), (el, ec) = d["start"], d["end"]
        if not sl <= n <= el:
            continue
        a, b = (sc if sl == n else 0), (ec if el == n else len(line))
        if b <= a:  # an empty range marks one character
            a, b = (a, a + 1) if a < len(line) else (max(a - 1, 0), a)
        if b > a:
            cover.append((a, b, k))
    tokens = [(m.start(), m.end(), m.lastgroup, m.group()) for m in TOKEN.finditer(line)]
    cuts = sorted({0, len(line), *(x for t in tokens for x in t[:2]), *(x for c in cover for x in c[:2])})
    out = []
    for a, b in zip(cuts, cuts[1:]):
        tok = next((t for t in tokens if t[0] <= a < t[1]), None)
        ks = [k for x, y, k in cover if x <= a < y]
        classes, attrs = [], ""
        if tok:
            classes.append(f"tok-{tok[2]}")
            if tok[2] == "cmd":
                classes.append("src-cs")
                attrs += f' data-cs="{html.escape(tok[3])}"'
        if ks:
            worst = min(ks, key=lambda k: lints[k]["rank"])
            classes += ["lint", f"f-{lints[worst]['severity']}"] + (["typo"] if any(lints[k]["typo"] for k in ks) else [])
            attrs += f' data-lint="{" ".join(map(str, ks))}"'
        text = html.escape(line[a:b])
        out.append(f'<span class="{" ".join(classes)}"{attrs}>{text}</span>' if classes else text)
    return "".join(out)


def explained(satex, project, env, name, names, dist):
    """`satex explain` of each of `names` in example `name`, records by name. Paths are the
    playground's: a distribution file under /texmf.bundle, a project file by its name."""
    if not names:
        return {}
    return memo(work_of(env), "explained", [BINARY, name, sorted(names), tree_digest(project), str(dist)],
                lambda: explained_run(satex, project, env, name, names, dist))


def explained_run(satex, project, env, name, names, dist):
    done = proc([str(satex), "explain", *sorted(names), "--format", "json", "-f", name], cwd=project, env=env,
                          capture_output=True, text=True)
    by = {}
    for r in json.loads(done.stdout or "[]"):
        path = r.get("path")
        if path and Path(path).is_absolute():
            p = Path(path).resolve()
            r["path"] = f"/texmf.bundle/{p.relative_to(dist).as_posix()}" if p.is_relative_to(dist) else p.name
        by.setdefault(r["name"], []).append(r)
    return by


def work_of(env):
    """The scratch directory the cache directory of `env` lies in."""
    return Path(env["SATEX_CACHE"]).parent


def findings(satex, project, env, name, source):
    """The example's findings: ranges as the language server reports them, with the text
    each fix replaces."""
    return memo(work_of(env), "findings", [BINARY, name, source, tree_digest(project)],
                lambda: findings_run(satex, project, env, name, source))


def findings_run(satex, project, env, name, source):
    lsp = json.loads(proc([str(satex), "lint", "--format", "lsp", "-f", name], cwd=project, env=env,
                                    capture_output=True, text=True).stdout or "[]")
    rows = json.loads(proc([str(satex), "lint", "--format", "json", "-f", name], cwd=project, env=env,
                                     capture_output=True, text=True).stdout or "[]")
    lines = source.splitlines()
    text = lambda p: lines[p["line"] - 1] if p["line"] - 1 < len(lines) else ""
    out = []
    for d in (d for f in lsp for d in f.get("diagnostics", [])):
        start, end = d["range"]["start"], d["range"]["end"]
        # the language server's code is the rule's number, its name in `data.rule`
        code = (d.get("data") or {}).get("rule") or d["code"]
        row = next((r for r in rows if r["code"] == code and r["message"] == d["message"]), {})
        edits = [{"from": text(e["start"])[e["start"]["col"] - 1:e["end"]["col"] - 1] if e["start"]["line"] == e["end"]["line"] else "",
                  "to": e["replacement"]} for e in row.get("edits", []) if e.get("path", name) == name]
        sev = ["", "error", "warning", "info", "info"][d.get("severity", 3)]
        data = d.get("data", {})
        out.append({"code": code, "number": d["code"], "message": d["message"], "severity": sev, "rank": ["error", "warning", "info"].index(sev),
                    "start": (start["line"], start["character"]), "end": (end["line"], end["character"]),
                    "fix": data.get("fix", ""), "applicability": data.get("applicability", "unsafe"), "edits": edits,
                    "typo": bool(TYPOGRAPHIC.search(code)) and start["line"] == end["line"] and end["character"] - start["character"] <= 3})
    return sorted(out, key=lambda d: (d["rank"], d["start"]))


def typeset(satex, project, env, name, source):
    """The paragraphs the example sets (`satex query typeset`), as the playground's Typeset panel shows them."""
    def make():
        # a plain TeX file (its magic comment names `tex`) runs on the plain kernel, as the playground runs it
        plain = re.search(r"^%\s*!T[eE]X\s+(?:TS-)?program\s*=\s*tex\b", source, re.M)
        flags = next(f for k, _, f in KERNELS if k == "plain") if plain else []
        done = proc([str(satex), *flags, "query", "typeset", "--format", "json", "-f", name], cwd=project, env=env, capture_output=True, text=True)
        try:
            rows = json.loads(done.stdout or "[]")
        except json.JSONDecodeError:
            return []
        keep = ("line", "marked", "detail", "keys", "fonts", "align", "ligatures", "heading", "cut")
        # the project's own files: an \\input chapter's text too, not a package's
        own = lambda path: not Path(path).is_absolute() or Path(path).resolve().is_relative_to(project.resolve())
        return [{k: r[k] for k in keep if k in r} for r in rows if r.get("kind") == "paragraph" and own(str(r.get("path", "")))]
    return memo(work_of(env), "typeset", [BINARY, name, source, tree_digest(project), inspect.getsource(typeset)], make)


def examples(satex, work, project):
    """Each example as a tab, analyzed at build time: its findings underlined in the source with
    a marker at the line's end, what it typesets beside it, and what `satex explain` says of every
    command it uses, shown on hover. A last tab is the reader's own text, analyzed in the page.
    Also the showcase's live windows as they stand before the page's engine starts: name -> (source HTML, data)."""
    env = dict(os.environ, SATEX_CACHE=str(work / f"cache-{KERNELS[0][0]}"))
    dist = Path(run("kpsewhich", "-var-value", "TEXMFDIST").strip()).resolve()

    def rendered(project, name, source):
        """The source as HTML, its findings underlined and marked at the line's end, and the data site.js reads."""
        lints = findings(satex, project, env, name, source)
        used = {m.group() for line in source.splitlines() for m in TOKEN.finditer(line) if m.lastgroup == "cmd"}
        cs = explained(satex, project, env, name, used, dist)
        called = {x for rs in cs.values() for r in rs for x in r.get("expands", []) + [r.get("by")] if x and x not in cs}
        cs |= explained(satex, project, env, name, called, dist)
        data = json.dumps({"name": name, "source": source, "cs": cs, "lints": lints, "ts": typeset(satex, project, env, name, source)}).replace("</", "<\\/")
        # a finding's marker at the end of the line it starts on: its severity and rule
        # a finding's marker at the end of its line: its rule's number, the name on hover; one rule met on
        # lines in a row is one marker on the first, with how often
        marks = {}
        for k, d in enumerate(lints):
            n = d["start"][0]
            run = next((m for m in marks.get(n - 1, []) if m[0] == d["code"]), None)
            if run:
                run[1].append(k)
                marks.setdefault(n, []).append(run)
            else:
                marks.setdefault(n, []).append([d["code"], [k], d, n])
        def ends(n):
            out = []
            for code, ks, d, first in marks.get(n, []):
                if first != n:
                    continue
                times = f" ×{len(ks)}" if len(ks) > 1 else ""
                out.append(f'<span class="eol f-{d["severity"]}" data-lint="{" ".join(map(str, ks))}" title="{html.escape(code)}{times}">{html.escape(d.get("number") or code)}{times}</span>')
            return "".join(out)
        return "\n".join(f'<span class="ln" data-l="{n + 1}">{tex_html(l, n, lints)}{ends(n)}</span>' for n, l in enumerate(source.splitlines())), data

    def one(job):
        i, name = job
        lines, data = rendered(project, name, (EXAMPLES / name).read_text())
        sel = "true" if i == 0 else "false"
        tab = (f'<button role="tab" aria-selected="{sel}" aria-controls="ex-{i}" id="ex-tab-{i}">{html.escape(name)}</button>')
        return tab, (
            f'<figure class="example" role="tabpanel" id="ex-{i}" aria-labelledby="ex-tab-{i}"{"" if i == 0 else " hidden"}>\n'
            f'<figcaption><span class="muted live-status" aria-live="polite">Click into the source to edit it.</span>'
            f'<span class="ex-acts"><button type="button" class="ghost small ex-reset" hidden>reset</button>'
            f'<a class="button ghost small" href="play/index.html#{name}">open in the playground →</a></span></figcaption>\n'
            f'<pre class="src"><code>{lines}</code></pre>\n'
            f'<div class="ex-side typeset live-ts" aria-label="what SaTeX computes it typesets"></div>\n'
            f'<script type="application/json" class="ex-data">{data}</script></figure>')
    shown = [l.strip() for l in (EXAMPLES / "landing.txt").read_text().splitlines() if l.strip() and not l.startswith("#")]
    jobs = list(enumerate(n for n in shown if n in roots(EXAMPLES)))
    tabs, panels = zip(*parallel("[4/4] examples", jobs, one))
    own = len(jobs)
    tab = f'<button role="tab" aria-selected="false" aria-controls="ex-{own}" id="ex-tab-{own}">your own</button>'
    panel = (f'<figure class="example live" role="tabpanel" id="ex-{own}" aria-labelledby="ex-tab-{own}" hidden>\n'
             f'<figcaption><span class="muted live-status" aria-live="polite">Type LaTeX, even a tikzpicture: SaTeX reads it again as you type, in this page (about 10&nbsp;MB, loaded once).</span>'
             f'<a class="button ghost small" href="play/index.html">open the playground →</a></figcaption>\n'
             f'<div class="src live-src"></div>\n<div class="ex-side typeset live-ts" aria-label="what SaTeX computes it typesets"></div></figure>')
    def live(s):
        # its own directory: the text is the showcase's, no example's
        project = work / "showcase-live"
        project.mkdir(exist_ok=True)
        (project / s["file"]).write_text(s["text"])
        return s["name"], rendered(project, s["file"], s["text"])
    spec = [s for s in json.loads((SITE / "showcase/shots.json").read_text())["shots"] if s.get("live")]
    return f'<div class="tabs" role="tablist">{"".join(tabs)}{tab}</div>' + "\n".join([*panels, panel]), dict(map(live, spec))


def playground(satex, wasm, work, out):
    offered = package_names()
    # mining the manuals needs no bundle: it runs beside the caches and the rest
    side = concurrent.futures.ThreadPoolExecutor(1)
    mining = side.submit(mine, satex, work, sorted(n for n in offered if not n.endswith(".cls")))
    sources_ = [(d.name, d.read_bytes()) for d in sources()] + [(c, (d.parent / c).read_bytes()) for d in sources() for c in projects()[d.name]]
    tables, packages, samples, project = memo(
        work, "bundle", [KEY, LISTINGS.get("@stamp"), offered, sources_, tree_digest(ROOT / "samples"), (SITE / "playground.yaml").read_text(),
                         inspect.getsource(bundle)],
        lambda: bundle(satex, work))
    PREVIEWS.update(json.loads(tables[0].get("/cache/previews.json") or b"{}"))
    documented = manuals(satex, work, tables, packages, offered, mining.result())
    side.shutdown()
    # `cargo playground` builds the wasm meanwhile and names it done here.
    ready = os.environ.get("SATEX_WASM_READY")
    while ready and not Path(ready).exists():
        time.sleep(1)
    if ready and Path(ready).read_text() != "ok":
        sys.exit("playground: the wasm build failed")
    wasm_bytes = (wasm / "satex.wasm").read_bytes()
    kernels = []
    for (kernel, label, flags), files in zip(KERNELS, tables):
        packed = gzip.compress(b"".join(files.values()), 9, mtime=0)
        kernels.append({"id": kernel, "label": label, "args": flags, "size": len(packed),
                        **({"badge": EXPERIMENTAL[kernel][0], "note": EXPERIMENTAL[kernel][1]} if kernel in EXPERIMENTAL else {}),
                        "bundle": emit_local(out, f"play/{kernel}.bin.gz", packed),
                        "files": [[path, len(data)] for path, data in files.items()]})
    for engine, label, uses in ALIASES:
        base = next(k for k in kernels if k["id"] == uses)
        kernels.append({**base, "engine": engine, "label": label,
                        **({"badge": EXPERIMENTAL[engine][0], "note": EXPERIMENTAL[engine][1]} if engine in EXPERIMENTAL else {})})
    extra = []
    for kernel, name, files, needs in packages:
        packed = gzip.compress(b"".join(files.values()), 9, mtime=0)
        extra.append({"id": f"{kernel}/{name}", "name": name.removesuffix(".cls"), "kernel": kernel, "needs": needs, "label": name,
                      "size": len(packed), "bundle": emit_local(out, f"play/pkg/{kernel}-{name}.bin.gz", packed),
                      "manual": name.removesuffix(".cls") in documented,
                      "files": [[path, len(data)] for path, data in files.items()]})
    def facts():
        """What satex says about itself: its queries, which of them name their file, its rules, its completions."""
        listed = json.loads(run(str(satex), "query", "--list", "--format", "json"))
        queries = [q["name"] for q in listed]
        about = {q["name"]: q["about"] for q in listed}
        env = dict(os.environ, SATEX_CACHE=str(work / f"cache-{KERNELS[0][0]}"))
        sample = (EXAMPLES / roots(EXAMPLES)[0]).name

        def per_file(query):
            """Whether the query's rows name their file: the playground then shows the open file's rows only."""
            done = proc([str(satex), "query", query, "--filter", f"file={sample}", "--format", "json", "-f", sample],
                                  cwd=project, env=env, capture_output=True, text=True)
            try:
                return done.returncode == 0 and len(json.loads(done.stdout or "[]")) > 0
            except json.JSONDecodeError:
                return False
        rules = {r["code"]: {**r, **json.loads(run(str(satex), "lint", "--explain", r["code"], "--format", "json"))}
                 for r in json.loads(run(str(satex), "lint", "--rules", "--format", "json"))}
        # each example's type plugins: where a plugin shows on it (`places`) makes it the plugin's demo
        plugged = {ex: json.loads(run(str(satex), "query", "type-plugins", "--format", "json", "-f", ex, cwd=project, env=env)) for ex in roots(EXAMPLES)}
        return queries, [q for q in queries if per_file(q)], rules, json.loads(run(str(satex), "completions", "json")), about, plugged
    queries, filtered, rules, completion, query_about, plugged = memo(work, "facts", [BINARY, tree_digest(project), inspect.getsource(facts)], facts)
    table, files = memo(work, "symbols", [BINARY, ICON_PACKAGES, inspect.getsource(icons), inspect.getsource(emoji), inspect.getsource(symbols), inspect.getsource(comprehensive), inspect.getsource(categories), GLYPH],
                        lambda: symbols(satex, project, dict(os.environ, SATEX_CACHE=str(work / f"cache-{KERNELS[0][0]}"))))
    # every rule's card on any page (site.js), from the same records the playground's findings show
    (out / "rules.json").write_text(json.dumps(rules))
    tiles = plugin_tiles(plugged)
    (out / "type-plugins.json").write_text(json.dumps(tiles))
    manifest = {
        "js": emit_local(out, "play/satex.js", (wasm / "satex.js").read_bytes()),
        "wasm": emit_local(out, "play/satex.wasm", wasm_bytes), "wasmSize": len(wasm_bytes),
        "kernels": kernels, "packages": extra, "preload": first_download(), "unavailable": UNAVAILABLE, "queries": queries, "queryAbout": query_about, "filtered": filtered, "rules": rules,
        "completion": completion, "samples": samples, "sample": "hello.tex", "root": "../",
        "commands": {name: command for name, command in commands().items() if command != "lint"}, "projects": projects(), "plugins": plugins(), "typePlugins": tiles,
        "symbols": emit(out, "play/symbols.json", json.dumps({**table, "glyphs": emit(out, "play/glyphs.bin.gz", gzip.compress(table["glyphs"], 9, mtime=0)),
                                                                "fonts": [emit(out, f"play/fonts/{f.name}", f.read_bytes()) for f in files]}).encode()),
        "schema": emit(out, "play/satex.schema.json", (ROOT / "satex.schema.json").read_bytes()),
    }
    sizes = {k["id"]: f"{k['size'] / 2**20:.1f} MiB gz, {len(k['files'])} files" for k in kernels}
    print(f"playground: wasm {len(wasm_bytes) / 2**20:.1f} MiB, {sizes}; {len(extra)} package bundles on demand, "
          f"{sum(p['size'] for p in extra) / 2**20:.1f} MiB gz in all", file=sys.stderr)
    return manifest, *examples(satex, work, project)


# Icon packages whose own files map each command to a character of their own font.
ICON_PACKAGES = ["fontawesome", "fontawesome5", "fontawesome6", "fontawesome7", "academicons"]


def icons(dist, package):
    r"""[command, codepoint, font file, icon name] of each icon `package` defines, read off its own files: fontawesome5's
    and later's `\__fontawesome_def_icon:nnnnn`, else a `\def\faX{\faicon{name}}` with the name's `\symbol{"…}`.
    The font is the package's own OpenType or TrueType file, the brands one for a brands icon."""
    sty = run("kpsewhich", f"{package}.sty").strip()
    if not sty:
        return []
    texts = "\n".join(p.read_text(errors="replace") for p in Path(sty).parent.iterdir() if p.suffix in (".sty", ".def", ".tex"))
    fonts = sorted(p for d in ("opentype", "truetype") for p in (dist / "fonts" / d / "public" / package).glob("*.[ot]tf"))
    if not fonts:
        return []
    def font(part):
        brands = part.startswith("brands")
        # a brand's one face, else the solid one: the package's default style
        return next((f for f in fonts if ("Brands" in f.name) == brands and (brands or "Solid" in f.name)), fonts[0])
    out = [[cmd, int(code, 16), font(part), name] for cmd, name, part, code in
           re.findall(r'\\__fontawesome_def_icon:nnnnn\{(\\\w+)\}\{([\w-]+)\}\{(\w+)\}\{\d+\}\{"([0-9A-Fa-f]+)\}', texts)]
    codes = dict(re.findall(r'\\csname \w+@([\w-]+)\\endcsname\{+(?:\\\w+)?\\symbol\{"([0-9A-Fa-f]+)\}', texts))
    out += [[cmd, int(codes[name], 16), fonts[0], name] for cmd, name in re.findall(r'\\def(\\\w+)\{+\\\w+icon\{([\w-]+)\}\}', texts) if name in codes]
    return out


def emoji(dist):
    r"""[codepoint, name, description] of each emoji of the `emoji` package that is one character (a variation selector aside):
    `\emoji{name}` sets it, drawn here in the monochrome Noto Emoji font. Empty without the package or the font."""
    table, font = run("kpsewhich", "emoji-table.def").strip(), dist / "fonts/truetype/google/noto-emoji/NotoEmoji-Regular.ttf"
    if not table or not font.is_file():
        return []
    found = re.findall(r"\\__emoji_def:nnnnn \{([^}]*)\} \{([^}]*)\} \{([^}]*)\} \{([^}]*)\}", Path(table).read_text(errors="replace"))
    out = []
    for chars, name, aliases, about in found:
        codes = [int(c, 16) for c in re.findall(r"\^+([0-9a-fA-F]+)", chars) if int(c, 16) != 0xFE0F]
        if len(codes) == 1:
            out.append([codes[0], name, " ".join([about.replace("~", " ").lower(), *re.split(r",\s*", aliases)]).strip()])
    return out


def plugins():
    """satex's plugins, each folder of src/plugins with the first sentence of its module docs: the
    playground's Settings lists them with the keys each one reads."""
    out = []
    for path in sorted((ROOT / "src/plugins").iterdir()):
        source = path / "mod.rs" if path.is_dir() else path
        if source.suffix != ".rs" or source.stem == "mod" and not path.is_dir() or not source.is_file():
            continue
        doc = " ".join(l.removeprefix("//!").strip() for l in source.read_text().splitlines() if l.startswith("//!"))
        out.append({"name": path.stem, "about": re.split(r"(?<=\.)\s", doc, maxsplit=1)[0] if doc else ""})
    return out


def plugin_tiles(plugged):
    """The type plugins' tiles (site.js `pluginTiles`): `satex query type-plugins` by category, each plugin with
    its switch and default from the schema, its section of the wiki page and, as its demo, the example whose
    run gives it the most places."""
    records = next(iter(plugged.values()), [])
    text = (ROOT / "doc/wiki/type-plugins.md").read_text()
    heads = [(m.start(), m[1].strip()) for m in re.finditer(r"^#{2,3} (.+)$", text, re.M)]
    spans = [(start, end, head) for (start, head), (end, _) in zip(heads, heads[1:] + [(len(text), "")])]
    slug = lambda h: re.sub(r"\s", "-", re.sub(r"[^\w\s-]", "", h.lower()))
    def docs(name):
        head = next((h for _, _, h in spans if h == name), None) or next((h for a, b, h in spans if f"`{name}`" in text[a:b]), None)
        return "wiki/type-plugins.html" + (f"#{slug(head)}" if head else "")
    schema = json.loads((ROOT / "satex.schema.json").read_text())["properties"]
    def switch(name):
        key = f"type_plugins.{name}" if name in schema["type_plugins"]["properties"] else name
        node = {"properties": schema}
        for part in key.split("."):
            node = node.get("properties", {}).get(part, {})
        return f"{key}.enabled", node.get("properties", {}).get("enabled", {}).get("default", True)
    def demo(name):
        places = {ex: next((r.get("places", 0) for r in rs if r["name"] == name), 0) for ex, rs in plugged.items()}
        best = max(places, key=places.get, default=None)
        return best if best and places[best] else None
    out = []
    for r in records:
        if not out or out[-1]["name"] != r["category"]:
            out.append({"name": r["category"], "about": r.get("purpose") or "", "plugins": []})
        key, default = switch(r["name"])
        out[-1]["plugins"].append({"name": r["name"], "about": r["about"], "types": r["types"], "features": r["features"],
                                   "key": key, "default": default, "docs": docs(r["name"]), "demo": demo(r["name"])})
    return out


GLYPH = 32


def comprehensive(dist):
    r"""The Comprehensive LaTeX Symbol List's inventory, read off its typeset PDF, which shows every symbol
    whatever font or package sets it: [command, package, kind, table title, glyph] of each row of its
    tables, the glyph a bitmap, the bytes width and height then the pixels, 1 bit each (at most GLYPH wide) cut from the page's
    cell left of the monospaced command. The tables' packages are those of the list of tables (`AMS` is
    what the list's introduction says it is), none for the kernel's own. A package that is not installed
    leaves its rows out; a command is kept with its arguments unless they are the examples `{A}`."""
    pdf = dist / "doc/latex/comprehensive/symbols-a4.pdf"
    source = dist / "doc/latex/comprehensive/source/symbols.tex"
    if not pdf.is_file() or not shutil.which("pdftohtml") or not shutil.which("pdftoppm"):
        return []
    import io
    import numpy
    from PIL import Image
    xml = run("pdftohtml", "-xml", "-i", "-stdout", str(pdf))
    fonts = {i: re.sub(r"^\w+\+", "", name) for i, name in re.findall(r'<fontspec id="(\d+)"[^>]* family="([^"]+)"', xml)}
    pages = []
    for chunk in xml.split('<page number="')[1:]:
        its = [{"top": int(t), "left": int(l), "w": int(w), "h": int(h), "font": fonts[f], "txt": html.unescape(re.sub(r"</?[a-z][^>]*>", "", x))}
               for t, l, w, h, f, x in re.findall(r'<text top="(-?\d+)" left="(-?\d+)" width="(\d+)" height="(\d+)" font="(\d+)">(.*?)</text>', chunk)]
        pages.append((int(chunk[:chunk.index('"')]), its))
    size = re.search(r'<page number="1" position="absolute" top="0" left="0" height="(\d+)" width="(\d+)"', xml)
    height, width = int(size[1]), int(size[2])
    ams = re.search(r"means to use the \\AMS\{\} packages, viz\.\\ \\pkgname\{(\w+)\}", source.read_text(errors="replace")) if source.is_file() else None
    # the list of tables: each table's packages and title
    toc = {}
    for _, its in pages:
        for it in its:
            m = re.match(r"Table (\d+):", it["txt"])
            if m and it["left"] < 140 and "SFRM" in it["font"]:
                line = [x for x in its if abs(x["top"] - it["top"]) <= 3 and 150 < x["left"] < 770]
                pkgs = [x["txt"].strip() for x in line if "SFRM" not in x["font"] and re.fullmatch(r"[\w-]+", x["txt"].strip())]
                title = " ".join([it["txt"][len(m[0]):]] + [x["txt"] for x in line if "SFRM" in x["font"]])
                title = re.sub(r"(\s*\.)+\s*\d*\s*$", "", re.sub(r"L A TEX 2\S*", "LaTeX2e", title)).strip()
                toc.setdefault(int(m[1]), ([ams[1] if p == "AMS" and ams else p for p in pkgs], title))
    wanted = sorted({p for ps, _ in toc.values() for p in ps})
    have = {Path(p).stem for p in subprocess.run(["kpsewhich", *[f"{w}.sty" for w in wanted]], capture_output=True, text=True).stdout.split()}
    scale = 3

    def page(number, its, chapter, table):
        """The rows of one page as [table, kind, command, example, glyph], and the table its end is in."""
        caps = sorted((it["top"], int(its[i + 1]["txt"].split(":")[0])) for i, it in enumerate(its[:-1])
                      if it["txt"] == "Table" and "CSC" in it["font"] and re.match(r"\d+:", its[i + 1]["txt"]))
        cells = [it for it in its if re.search(r"TT|Mono", it["font"]) and it["top"] > 130]
        for it in its:
            m = re.fullmatch(r"Chapter (\d+)\.", it["txt"])
            if m and it["top"] < 130:
                chapter = int(m[1])
        out, shot = [], None
        for c in cells:
            tab = table
            for top, n in caps:
                if c["top"] >= top - 2:
                    tab = n
            m = re.match(r"\\(?:[A-Za-z@]+|.)((?:\{[^{}]*\}|\[[^\]]*\])*)", c["txt"])
            name = (m[0] if m[1] not in ("", "{A}", "{}") else m[0][:len(m[0]) - len(m[1])]) if m else c["txt"] if len(c["txt"]) <= 3 and c["txt"].strip() else None
            if not name or tab not in toc or chapter < 2:
                continue
            # prose mentioning a command is no table row: a word right before it
            if any(abs(d["top"] - c["top"]) < 8 and 0 <= c["left"] - d["left"] - d["w"] < 5 and re.search(r"[A-Za-z]{3}", d["txt"]) and d not in cells for d in its):
                continue
            column = sorted((d for d in cells if abs(d["left"] - c["left"]) <= 6), key=lambda d: d["top"])
            i = column.index(c)
            mid = lambda a, b: (a["top"] + a["h"] / 2 + b["top"] + b["h"] / 2) / 2
            y0 = mid(column[i - 1], c) if i else c["top"] - 40
            y1 = mid(c, column[i + 1]) if i + 1 < len(column) else c["top"] + c["h"] + 40
            row = [d["left"] + d["w"] for d in cells if d is not c and d["left"] + d["w"] <= c["left"] + 2 and d["top"] < y1 and d["top"] + d["h"] > y0]
            x0 = max(row, default=c["left"] - 260)
            if shot is None:
                shot = numpy.asarray(Image.open(io.BytesIO(subprocess.run(
                    ["pdftoppm", "-gray", "-f", str(number), "-l", str(number), "-scale-to-x", str(width * scale), "-scale-to-y", str(height * scale), str(pdf)],
                    capture_output=True).stdout)).convert("L"))
            cell = shot[int(max(y0, 0) * scale):int(y1 * scale), int(max(x0, 0) * scale):int((c["left"] - 3) * scale)]
            # any colour off the page's white is ink: a flag's yellow stripe as much as its black
            ink = cell < 240
            cols = numpy.flatnonzero(ink.any(0))
            if not len(cols):
                continue
            # the leftmost piece of ink: a symbol, not the second column of `\Big` versions or a description
            end = next((k for k in range(1, len(cols)) if cols[k] - cols[k - 1] > 9 * scale), len(cols))
            ink = ink[:, cols[0]:cols[end - 1] + 1]
            rows = numpy.flatnonzero(ink.any(1))
            if ink.sum() < 4:
                continue
            ink = ink[rows[0]:rows[-1] + 1]
            k = GLYPH / max(ink.shape)
            img = Image.fromarray((ink * 255).astype("uint8")).resize((max(1, round(ink.shape[1] * k)), max(1, round(ink.shape[0] * k))), Image.LANCZOS)
            mask = numpy.asarray(img) > 90
            # a sliver or a speck is a cut that missed the symbol: no glyph beats a blank one
            if mask.sum() < 12 or (min(mask.shape) <= 2 and mask.sum() < max(mask.shape)):
                continue
            out.append([tab, ("text", "math")[chapter == 3] if chapter in (2, 3) else "other", name, c["txt"],
                        bytes(mask.shape[::-1]) + numpy.packbits(mask).tobytes()])
        return out, (caps[-1][1] if caps else table), chapter

    state, found = (None, 0), []
    for number, its in pages:
        if number >= 14:
            out, *state = page(number, its, state[1], state[0])
            found += out
    seen, result = set(), []
    for tab, kind, name, example, glyph in found:
        pkgs, title = toc[tab]
        for pkg in [p for p in pkgs if p in have] or ([] if pkgs else ["kernel"]):
            if (name, pkg) not in seen:
                seen.add((name, pkg))
                result.append([name, pkg, kind, title, glyph])
    return result


def categories(listing):
    """The symbol list's table titles as categories, without the packages they name (`lilyglyphs Named
    Noteheads`: Named Noteheads): the titles, and each name's category as an index into them, from the first table showing it."""
    pkgs = {pkg for _, pkg, *_ in listing}
    # a package's name leads in lowercase (`lilyglyphs Clefs`) or stands between slashes
    clean = lambda t: re.sub(r"^(?:[a-z][\w-]*\s+)+", "", " ".join(w for w in t.split() if w not in pkgs and w != "/")).strip(" ,")
    titles, kinds = [], {}
    for name, _, _, title, _ in listing:
        t = clean(title)
        if t and name not in kinds:
            if t not in titles:
                titles.append(t)
            kinds[name] = titles.index(t)
    return titles, kinds


def established(dist, packages):
    """How much each of `packages` is relied on, 0 to 1: the log of how many files of the installation's
    packages and classes load it, against the most loaded one."""
    names, loads = set(packages), {}
    pattern = re.compile(rb"\\(?:RequirePackage|usepackage)(?:\[[^\]]*\])?\{([^}]*)\}")
    for path in (dist / "tex").rglob("*"):
        if path.suffix in (".sty", ".cls") and path.is_file():
            for group in set(pattern.findall(path.read_bytes())):
                for name in group.decode(errors="replace").split(","):
                    if name.strip() in names:
                        loads[name.strip()] = loads.get(name.strip(), 0) + 1
    top = math.log1p(max(loads.values(), default=1))
    return {p: round(math.log1p(loads.get(p, 0)) / top, 2) for p in packages}


def symbols(satex, project, env):
    """The symbols the playground's drawing pad matches against, each drawn in the font that sets it:
    unicode-math's table of the installation in unicode-math's default font, and the icon packages'
    commands in their own fonts, then what the symbol list shows beside them. Rows are
    `[codepoint, name, class, description, font, package]`; a font is an index into `fonts`, a package the
    one to load for the name (`satex` `providers`, else the table's own package). A symbol no font here
    sets has font -1 and a seventh field, its index into `glyphs`: the bitmaps (see `comprehensive`) one after the other."""
    dist = Path(run("kpsewhich", "-var-value", "TEXMFDIST").strip()).resolve()
    fonts, rows = [], []
    def font(path):
        if path not in fonts:
            fonts.append(path)
        return fonts.index(path)
    table = run("kpsewhich", "unicode-math-table.tex").strip()
    xetex = run("kpsewhich", "unicode-math-xetex.sty").strip()
    default = re.search(r"\\setmathfont\{([^}]+\.otf)\}", Path(xetex).read_text()) if xetex else None
    math_font = default and run("kpsewhich", default[1]).strip()
    if table and math_font:
        found = re.findall(r'\\UnicodeMathSymbol\{"([0-9A-Fa-f]+)\}\{(\\[A-Za-z]+)\s*\}\{\\math(\w+)\}\{([^}]*)\}', Path(table).read_text())
        math = [[int(c, 16), name, cls, about] for c, name, cls, about in found]
        # the kernel's `\alpha` is math italic, as TeX sets a small Greek letter (`\mitalpha`), a capital
        # `\Gamma` upright (`\mupGamma`)
        names = {n for _, n, *_ in math}
        italic = {n[4:]: c for c, n, cls, _ in math if n.startswith("\\mit") and cls == "alpha"}
        math += [[italic.get(n[4:], c) if "small" in about else c, "\\" + n[4:], cls, about] for c, n, cls, about in list(math)
                 if n.startswith("\\mup") and cls == "alpha" and "\\" + n[4:] not in names]
        request = json.dumps({"file": SAMPLE, "queries": [{"type": "providers", "names": sorted({n for _, n, *_ in math})}]})
        done = proc([str(satex), "query", "--request", "-", "-f", SAMPLE], input=request, cwd=project, env=env, capture_output=True, text=True)
        try:
            provider = {r["name"]: r["package"] for r in json.loads(done.stdout)[0]["records"]}
        except (json.JSONDecodeError, IndexError, KeyError):
            provider = {}
        f = font(Path(math_font))
        rows += [[c, name, cls, about, f, provider.get(name, "unicode-math")] for c, name, cls, about in math]
        # the plain text characters, which no list of commands shows: `?` is typed as it is, the special ones as TeX spells them
        spelled = {"#": r"\#", "$": r"\$", "%": r"\%", "&": r"\&", "_": r"\_", "{": r"\{", "}": r"\}", "~": r"\textasciitilde", "^": r"\textasciicircum", "\\": r"\textbackslash"}
        rows += [[ord(c), spelled.get(c, c), "text", unicodedata.name(c).lower(), f, "kernel"] for c in map(chr, range(33, 127))]
    for package in ICON_PACKAGES:
        # the icon's own name (`skull-crossbones`) is what a search finds it by
        rows += [[code, cmd, "icon", name.replace("-", " "), font(path), package] for cmd, code, path, name in icons(dist, package)]
    # the emoji package's, a set of its own that the drawing pad leaves out unless asked (draw.js `DRAW.off`)
    noto = dist / "fonts/truetype/google/noto-emoji/NotoEmoji-Regular.ttf"
    rows += [[code, f"\\emoji{{{name}}}", "icon", about, font(noto), "emoji"] for code, name, about in emoji(dist)]
    # the symbol list's rows: a command a font already sets only gains the packages the list gives it; the
    # rest (text characters and commands, accents, other fonts' symbols) are drawn from the list's own page
    known, taken, glyphs = {}, {(r[1], r[5]) for r in rows}, {}
    for r in rows:
        known.setdefault(r[1], r)
    # a cell of these tables shows a sample (an accent on letters, `\arg` as a word), not a symbol to draw
    sample = re.compile(r"(?<!Musical )Accents|Log-like|Alphabets")
    listing = comprehensive(dist)
    titles, kinds = categories(listing)
    for name, pkg, kind, title, bitmap in listing:
        if (name, pkg) in taken or sample.search(title):
            continue
        taken.add((name, pkg))
        base = known.get(name)
        rows.append([*base[:5], pkg] if base and base[4] >= 0 else [0, name, kind, title, -1, pkg, glyphs.setdefault(bitmap, len(glyphs))])
    return {"fonts": [p.name for p in fonts], "rows": rows, "glyphs": b"".join(glyphs), "categories": titles, "kinds": kinds,
            "weight": {**established(dist, sorted({r[5] for r in rows})), "kernel": 1, "unicode-math": 1}}, fonts


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--satex", type=Path, required=True, help="the native satex binary")
    ap.add_argument("--wasm", type=Path, default=Path("."), help="directory with the Emscripten satex.js and satex.wasm")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--work", type=Path, help="scratch directory (default: OUT/../site-work)")
    ap.add_argument("--precompress", action="store_true",
                    help="write a .gz beside each compressible file, for a server that serves those as GitHub Pages compresses")
    ap.add_argument("--no-minify", action="store_true", help="ship scripts, styles and pages as written (no source maps), for debugging")
    ap.add_argument("--docs-only", action="store_true", help="only the documentation pages (the wiki, the semantics and the hover cards): no playground, home, draw or bench pages")
    ap.add_argument("--fresh", action="store_true", help="redo every satex run instead of reusing WORK/listings.json and WORK/memo")
    ap.add_argument("--keep-listings", action="store_true",
                    help="reuse the files each package was listed to read across satex builds (a development loop: "
                         "a file a changed interpreter reads besides is missing from the bundle until --fresh)")
    args = ap.parse_args()
    # Ctrl+C stops the build: the terminal signals the satex runs too, and worker threads must not
    # start more of them, so the process ends at once instead of unwinding the main thread only.
    for stop in (signal.SIGINT, signal.SIGTERM):
        signal.signal(stop, lambda number, _: os._exit(128 + number))
    global MINIFY, SCRATCH, BINARY, KEY, FRESH, SATEX, SITE, EXAMPLES, LISTINGS_FILE
    SATEX, MINIFY = args.satex.resolve(), not args.no_minify
    BINARY, FRESH = binary_digest(args.satex.resolve()), args.fresh
    KEY = BINARY if args.docs_only else json.loads(proc([str(args.satex), "cache", "--format", "json"], capture_output=True, text=True).stdout or "{}").get("key") or BINARY
    out, work = args.out.resolve(), (args.work or args.out.resolve().parent / "site-work").resolve()
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    work.mkdir(parents=True, exist_ok=True)
    SCRATCH = work
    # One state of the site's sources for the whole build: an edit made while it runs reaches the next one,
    # never half of this one (a page from after the edit with a script from before it).
    snapshot = work / "site-src"
    shutil.rmtree(snapshot, ignore_errors=True)
    shutil.copytree(SITE, snapshot, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    SITE, EXAMPLES = snapshot, snapshot / "examples"
    listings = LISTINGS_FILE = work / "listings.json"
    stamp = installation_stamp() + ("" if args.keep_listings else ";" + KEY)
    if listings.exists() and not args.fresh:
        LISTINGS.update(json.loads(listings.read_text()))
    if LISTINGS.get("@stamp") != stamp:
        LISTINGS.clear()
        LISTINGS["@stamp"] = stamp

    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]
    homepage, repo = cargo["homepage"].rstrip("/") + "/", cargo["repository"].rstrip("/")
    release, release_title = version(cargo)
    template = (SITE / "template.html").read_text()
    (out / "play").mkdir()
    svgs = logo(work)
    assets = {name: emit(out, "favicon.png" if name == "favpng" else f"logo-{name}.svg", data) for name, data in svgs.items()}
    assets |= {
        "css": emit(out, "style.css", (SITE / "style.css").read_bytes()),
        "sitejs": emit(out, "site.js", (SITE / "site.js").read_bytes()),
        "clijs": emit(out, "cli.js", (SITE / "cli.js").read_bytes()),
        "cmjs": emit(out, "codemirror.js", (SITE / "codemirror.js").read_bytes()),
        "playjs": emit(out, "play/playground.js", (SITE / "playground.js").read_bytes()),
        "drawjs": emit(out, "draw.js", (SITE / "draw.js").read_bytes()),
        "viewsjs": emit(out, "views.js", (SITE / "views.js").read_bytes()),
        "dialogsjs": emit(out, "dialogs.js", (SITE / "dialogs.js").read_bytes()),
        "layersjs": emit(out, "layers.js", (SITE / "layers.js").read_bytes()),
    }
    table = pages()
    nav = sidebar(table)
    order = chapters(table)
    # The Lean model: names and types from the environment, memoized while the sources stay the same.
    lean_src = ROOT / "semantics/lean"
    lean_files = sorted(lean_src.rglob("*.lean"), key=str) + [SITE / "export.lean"]
    lean_files = [p for p in lean_files if ".lake" not in p.parts]
    decls = memo(work, "lean-decls", [digest(b"".join(p.read_bytes() for p in lean_files))], lambda: sem.export(lean_src, SITE / "export.lean"))
    commit = run("git", "-C", str(ROOT), "rev-parse", "HEAD").strip()
    sem_ids = {m[1]: m[1] + "-" + re.sub(r"\s", "-", re.sub(r"[^\w\s-]", "", m[2].replace("`", "").lower())) for m in re.finditer(r"^## (\d+)\. (.+)$", (ROOT / "SEMANTICS.md").read_text(), flags=re.M)}
    lean_env, refs, lean, lean_pages, _ = sem.build(ROOT, repo, commit, decls, sem_ids)
    lean_table = {f"semantics/lean/Satex/{s}.lean": f"lean/{s.lower()}.html" for s in sem.LEAN}
    stamp = os.environ.get("SOURCE_DATE_EPOCH")
    built = (datetime.datetime.fromtimestamp(int(stamp), datetime.UTC) if stamp else datetime.datetime.now(datetime.UTC)).date().isoformat()

    def fill(text, values):
        return re.sub(r"\{\{(\w+)\}\}", lambda m: values.get(m.group(1), m.group(0)), text)

    def page(dst, title, body, side=True, kind=None):
        root = "../" * dst.count("/")
        side_html = markdown(nav).replace("@ROOT@", root) if side else ""
        values = {"title": html.escape(title), "root": root, "body": body, "sidebar": side_html,
                  "version": release, "versiontitle": release_title, "repository": repo, "description": html.escape(cargo["description"]),
                  "class": kind or ("doc" if side else "home"), "date": built, "crate": cargo["name"], **assets}
        text = fill(template, values)
        (out / dst).parent.mkdir(parents=True, exist_ok=True)
        (out / dst).write_text(minify_html(text))

    def docs():
        done = []
        for src, dst in table.items():
            text = strip_banner((ROOT / src).read_text())
            body = rewrite(markdown(text), src, table | lean_table, homepage, repo, out, "../" * dst.count("/"))
            if dst == "changelog.html":
                body = re.sub(r"<li>(?:<p>)?<code>(feat|fix|perf|docs|build)</code>", lambda m: f'<li><span class="badge b-{m[1]}">{m[1]}</span>', body)
            if dst == "wiki/formal-semantics.html":
                body = formal(body)
            nav_html = chapter_nav(dst, order)
            body = body.replace("</h1>", "</h1>" + nav_html, 1) + nav_html if nav_html else body
            done.append((dst, title_of(text), body))
        return done

    def formal(body):
        """The formal page: its two placeholders filled from semantics/mapping.md and primitives.tsv."""
        lean.prefix, lean.here, lean.used = "../lean/", None, {}
        mapped = sem.mapping((ROOT / "semantics/mapping.md").read_text().split("\n", 1)[1], lean, refs, lambda t: rewrite(markdown(t), "semantics/mapping.md", table | lean_table, homepage, repo, out, "../"))
        prims, rows = sem.primitives((ROOT / "semantics/primitives.tsv").read_text(), lean, refs, sem.builtin_lines(ROOT))
        formal.rows = rows
        index = {n: sem.hover(d) for n, d in lean.used.items()}
        return body.replace("<!--satex-site:mapping-->", mapped).replace("<!--satex-site:primitives-->", prims) + \
            f'<script type="application/json" id="lean-decls">{json.dumps(index, ensure_ascii=False).replace("</", "<\\/")}</script>'

    # The pages beside the playground's satex runs: they need none of them.
    with concurrent.futures.ThreadPoolExecutor(1) as side:
        pages_done = side.submit(docs)
        manifest, shown, live = ({"symbols": ""}, "", {}) if args.docs_only else playground(args.satex.resolve(), args.wasm.resolve(), work, out)
        written = pages_done.result()
    save_listings()
    manifest["worker"] = minify_script((SITE / "worker.js").read_text())
    readme = (ROOT / "README.md").read_text()
    md = lambda text: rewrite(markdown(text), "README.md", table, homepage, repo, out, "")
    parts = {"intro": md(intro(readme)), "install": md(install(readme)), "examples": shown,
             "manifest": json.dumps(manifest).replace("</", "<\\/"), "root": "../"}
    # Hover cards: every code span naming a control sequence, on every page; the data of those names in one JSON.
    names, pending = set(), []
    for dst, title, body in written:
        body, found = sem.mark_cards(body, PREVIEW_NAMES())
        names |= found
        pending.append((dst, title, body))
    for key in ("intro", "install"):
        parts[key], found = sem.mark_cards(parts[key], PREVIEW_NAMES())
        names |= found
    card_data = sem.cards(names, formal.rows, lean, refs, *scope_cards(args.satex.resolve(), work, names), {k.split(":", 1)[1]: v for k, v in PREVIEWS.items()}, sem.builtin_lines(ROOT), sem.example_defs(pending), source_fallback())
    # satex's own comment directives: no package defines them
    card_data.update((k, v) for k, v in sem.directive_cards(ROOT).items() if k in card_data)
    assets["cards"] = emit(out, "cs-cards.json", json.dumps(card_data, ensure_ascii=False, separators=(",", ":")).encode())
    # the command line's help on every page's `satex …` lines (cli.js), from clap's command tree
    assets["cli"] = emit(out, "cli.json", run(str(args.satex.resolve()), "completions", "json").encode())
    print(f"cards: {len(card_data)} of {len(names)} control sequences named in code spans", file=sys.stderr)
    left = {k: v.get("k", "") for k, v in card_data.items() if v.get("k") == "undefined" or v.get("at", "").startswith(("the example", "the text", "the pattern"))}
    print(f"cards: {sum(v == 'undefined' for v in left.values())} names without a definition, {len(left) - sum(v == 'undefined' for v in left.values())} defined by an example: "
          + " ".join(sorted(k for k, v in left.items() if v == "undefined")), file=sys.stderr)
    for dst, title, body in pending:
        page(dst, title, body)
    for stem, body in lean_pages.items():
        page(f"lean/{stem.lower()}.html", f"{stem}.lean", body)
    if args.docs_only:
        sem.check_links(out, refs.used)
        return
    def home():
        parts["showcase"] = showcase(out, live)
        page("index.html", "Static Analysis for (La)TeX and Friends", fill((SITE / "home.html").read_text(), {
            **assets, **parts, "version": release, "versiontitle": release_title, "repository": repo, "description": html.escape(cargo["description"])}), side=False)
    home()
    page("play/index.html", "Playground", fill((SITE / "play.html").read_text(), {**assets, **parts}), side=False, kind="play")
    # Draw alone, to embed: its own page without the site's chrome
    (out / "draw").mkdir(exist_ok=True)
    (out / "draw/index.html").write_text(minify_html(fill((SITE / "draw.html").read_text(), {**assets, "root": "../", "symbols": manifest["symbols"]})))
    data = SITE / "bench/data.json"
    (out / "bench").mkdir(exist_ok=True)
    if data.exists():
        shutil.copy2(data, out / "bench/data.json")
    else:
        (out / "bench/data.json").write_text("[]\n")
    page("bench/index.html", "Benchmarks", fill((SITE / "bench.html").read_text(), {"root": "../", "repository": repo}), side=False)
    (out / ".nojekyll").write_text("")
    sem.check_links(out, refs.used)
    prune_memo()
    offline(out)
    # the showcase's captures follow the UI they show: stale ones are taken again from this build
    if recapture(out):
        home()
    if args.precompress:
        precompress(out)


def showcase(out, live):
    """The landing page's showcase: per entry of showcase/shots.json its light and dark images
    (showcase/capture.mjs takes them from a built playground), sized so the page never shifts, in a
    window frame; a `before` image beside them, a looping video over them where the capture made one.
    The small entries go into one card grid after the rest."""
    spec, src = json.loads((SITE / "showcase/shots.json").read_text()), SITE / "showcase/img"
    (out / "showcase").mkdir(exist_ok=True)
    size = lambda f: tuple(int.from_bytes(f.read_bytes()[o:o + 4], "big") for o in (16, 20))

    def pictures(stem, alt):
        pics = []
        for scheme in ("light", "dark"):
            width, height = size(src / f"{stem}-{scheme}.png")
            for ext in ("avif", "webp", "png", "webm", "mp4"):
                if (src / f"{stem}-{scheme}.{ext}").exists():
                    shutil.copy2(src / f"{stem}-{scheme}.{ext}", out / "showcase" / f"{stem}-{scheme}.{ext}")
            pics.append(f'<picture class="shot-{scheme}"><source type="image/avif" srcset="showcase/{stem}-{scheme}.avif">'
                        f'<source type="image/webp" srcset="showcase/{stem}-{scheme}.webp">'
                        f'<img src="showcase/{stem}-{scheme}.png" width="{width}" height="{height}" alt="{alt if scheme == "light" else ""}" loading="lazy" decoding="async"></picture>')
            # the video plays over its poster, the still: site.js loads it near the viewport, never under reduced motion
            if (src / f"{stem}-{scheme}.webm").exists():
                pics.append(f'<video class="shot-video shot-{scheme}" width="{width}" height="{height}" muted playsinline preload="none" aria-hidden="true" '
                            f'data-src="showcase/{stem}-{scheme}"></video>')
        return "".join(pics)

    # the capture as cut out, no title bar around it
    def window(stem, alt):
        return f'<div class="shot-win">{pictures(stem, alt)}</div>'

    figures, cards = [], []
    for s in spec["shots"]:
        name, alt = s["name"], html.escape(f'{s["title"]}: {s["caption"]}')
        if s.get("live"):
            # a live editor in the same frame: until site.js starts the engine in view, the build's run of
            # its text stands there, findings underlined and the typeset text beside
            lines, data = live[name]
            figures.append(f'<figure class="shot shot-big shot-live" id="shot-{name}" data-name="{html.escape(s["file"])}"><div class="shot-stage"><div class="shot-win">'
                           f'<div class="live-grid"><pre class="src live-src live-poster"><code>{lines}</code></pre><div class="ex-side typeset live-ts" aria-label="what SaTeX computes it typesets"></div></div>'
                           f'<p class="muted live-status" aria-live="polite">{html.escape(s["status"])}</p></div></div>'
                           f'<script type="application/json" class="ex-data">{data}</script>'
                           f'<figcaption><h3>{html.escape(s["title"])}</h3><p>{html.escape(s["caption"])}</p>'
                           f'<a class="shot-try" href="play/index.html">The full playground →</a></figcaption></figure>')
            continue
        if not all((src / f"{name}-{scheme}.png").exists() for scheme in ("light", "dark")):
            continue
        # the link opens the example and, once its run is in, the widget the shot shows (playground.js `satexOpen`)
        w = s.get("open")
        hash = s["file"] + "".join(f"&{k}={urllib.parse.quote(str(v), safe='')}" for k, v in zip(("widget", "at", "k"), w or []))
        try_it = f"play/index.html#{hash}"
        tags = s.get("tags", ["Before", "After"])
        # two captures of one dialog, the first behind the second and offset: its two pages at once
        if s.get("stack") and (src / f"{name}-back-light.png").exists():
            stage = f'<div class="shot-stack">{window(f"{name}-back", "")}{window(name, alt)}</div>'
        else:
            stage = (f'<div class="shot-pair"><div><span class="shot-tag">{tags[0]}</span>{window(f"{name}-before", "")}</div>'
                     f'<div><span class="shot-tag">{tags[1]}</span>{window(name, alt)}</div></div>'
                     if (src / f"{name}-before-light.png").exists() else window(name, alt))
        (figures if s.get("big") or s.get("wide") else cards).append(
            f'<figure class="shot{" shot-big" if s.get("big") or s.get("wide") else ""}{" shot-wide" if s.get("wide") else ""}"><a class="shot-stage" href="{html.escape(try_it)}" tabindex="-1">{stage}</a>'
            f'<figcaption><h3>{html.escape(s["title"])}</h3><p>{html.escape(s["caption"])}</p>'
            f'<a class="shot-try" href="{html.escape(try_it)}">Try it in {html.escape(s["file"])} →</a></figcaption></figure>')
    return "\n".join(figures + [f'<div class="cards">{"".join(cards)}</div>'])


# what a showcase capture shows: the playground's code and style, besides its example and its entry
SHOT_SOURCES = ("playground.js", "views.js", "dialogs.js", "layers.js", "draw.js", "site.js", "style.css", "play.html", "showcase/capture.mjs")


def recapture(out):
    """Takes again, from the built site in `out`, every showcase capture whose inputs changed since it was taken
    (showcase/img/keys.json); the rest stay. Without Chromium or the encoders the last captures stay, with a warning.
    SATEX_SHOTS=off skips it. Whether any capture is new."""
    if os.environ.get("SATEX_SHOTS") == "off":
        return False
    # the captures land in the repository, where the next build finds them, and in this build's snapshot
    spec, img = json.loads((SITE / "showcase/shots.json").read_text()), ROOT / "doc/site/showcase/img"
    keys_file = img / "keys.json"
    keys = json.loads(keys_file.read_text()) if keys_file.exists() else {}
    ui = b"".join((SITE / f).read_bytes() for f in SHOT_SOURCES)
    source = lambda s: next((d / s["file"] for d in (EXAMPLES, ROOT / "samples") if (d / s["file"]).exists()), None)
    key = lambda s: digest(ui + json.dumps(s, sort_keys=True).encode() + source(s).read_bytes())
    # an entry whose example is not there yet (a feature still to come) waits
    due = {s["name"]: key(s) for s in spec["shots"] if not s.get("live") and source(s)}
    # entries that need the local compile server (`cargo playground`) keep their last images: the static server here has none
    needs_server = [n for n, k in due.items() if keys.get(n) != k and next(s for s in spec["shots"] if s["name"] == n).get("server")]
    due = {n: k for n, k in due.items() if next(s for s in spec["shots"] if s["name"] == n).get("server") is None}
    # a capture that failed on these inputs waits for them to change: no retry on every build
    due = {n: k for n, k in due.items() if keys.get(n) not in (k, f"failed {k}") or keys[n] == k and not all((img / f"{n}-{c}.png").exists() for c in ("light", "dark"))}
    if needs_server:
        print(f"showcase: kept (needs the compile server): {', '.join(needs_server)}", file=sys.stderr)
    if not due:
        return False
    chrome = os.environ.get("CHROME") or next(iter(sorted(Path.home().glob(".cache/ms-playwright/chromium-*/chrome-linux64/chrome"), reverse=True)), None)
    missing = [t for t in ("node", "magick", "avifenc", "pngquant", "ffmpeg") if not shutil.which(t)] + ([] if chrome else ["chromium"])
    if missing:
        print(f"showcase: {len(due)} captures out of date, kept: no {', '.join(missing)}", file=sys.stderr)
        return False
    import http.server, functools
    http.server.SimpleHTTPRequestHandler.extensions_map[".wasm"] = "application/wasm"
    quiet = type("Quiet", (http.server.SimpleHTTPRequestHandler,), {"log_message": lambda *a: None})
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(quiet, directory=str(out)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    began = time.monotonic()
    # two browsers at most (each takes about 3 GB with the engine), one per entry, the next starting as one ends
    names, url = list(due), f"http://127.0.0.1:{server.server_address[1]}/"
    print(f"showcase: retaking {len(names)} shots whose page code, example or entry changed: {', '.join(names)}", file=sys.stderr)
    finished = [0]

    def run(part):
        t0 = time.monotonic()
        out = subprocess.run(["node", str(SITE / "showcase/capture.mjs"), url, str(img)],
                             env=dict(os.environ, ONLY=",".join(part), CHROME=str(chrome)), capture_output=True, text=True).stdout
        finished[0] += 1
        got = sorted(set(re.findall(r"^ok +\S+ (light|dark)", out, re.M)))
        print(f"showcase: [{finished[0]}/{len(names)}] {part[0]}: {' and '.join(got) or 'failed'} in {time.monotonic() - t0:.0f} s", file=sys.stderr)
        return out
    try:
        with concurrent.futures.ThreadPoolExecutor(2) as pool:
            outputs = list(pool.map(run, [[n] for n in names]))
    finally:
        server.shutdown()
    done = type("Done", (), {"stdout": "\n".join(outputs)})
    ok = re.findall(r"^ok +(\S+) (light|dark)", done.stdout, re.M)
    taken = [n for n in due if (n, "light") in ok and (n, "dark") in ok]
    keys.update((n, due[n] if n in taken else f"failed {due[n]}") for n in due)
    for f in img.iterdir():
        if f.name.split("-")[0] in taken:
            if f.resolve() != (SITE / "showcase/img" / f.name).resolve():
                shutil.copy2(f, SITE / "showcase/img" / f.name)
    keys_file.write_text(json.dumps(keys, indent=1, sort_keys=True) + "\n")
    failed = [line for line in done.stdout.splitlines() if line.startswith("FAIL")]
    print(f"showcase: {len(taken)} of {len(due)} captures taken again in {time.monotonic() - began:.0f} s" + "".join(f"\n  {f}" for f in failed), file=sys.stderr)
    return bool(taken)


def offline(out):
    """`play/offline.json` (what the service worker keeps: `core` to start, `more` in the background, with sizes)
    and `play/sw.js`, versioned by what they list."""
    page = (out / "play/index.html").read_text()
    manifest = json.loads(re.search(r'id="satex-play" type="application/json">(.*?)</script>', page, re.S).group(1).replace("<\\/", "</"))
    symbols = json.loads((out / manifest["symbols"]).read_text())
    core = sorted({m for m in re.findall(r"(?:\.\./)+([\w./-]+-[0-9a-f]{12}\.[\w.]+)", page)} | {
        manifest["js"], manifest["wasm"], manifest["schema"], manifest["symbols"], manifest["kernels"][0]["bundle"], symbols["glyphs"], *symbols["fonts"], "rules.json"})
    first = set(manifest.get("preload") or [])
    packages = sorted(manifest["packages"], key=lambda p: (p["id"] not in first, p["id"]))
    more = [[e["bundle"], e["size"]] for e in [*manifest["kernels"][1:], *packages]]
    listing = {"core": core, "more": more}
    version = digest(json.dumps(listing).encode() + page.encode() + (SITE / "sw.js").read_bytes())
    (out / "play/offline.json").write_text(json.dumps({"version": version, **listing}))
    (out / "play/sw.js").write_text(minify_script((SITE / "sw.js").read_text().replace("__VERSION__", version)))
    print(f"playground: offline {len(core)} files to start, {len(more)} bundles, {sum(m[1] for m in more) / 2**20:.0f} MiB gz more", file=sys.stderr)


COMPRESSIBLE = {".html", ".js", ".css", ".json", ".svg", ".wasm", ".txt", ".xml", ".map"}


def precompress(out):
    """`name.gz` beside each compressible file where it is smaller: what a server
    that compresses on the fly, as GitHub Pages does, would send."""
    for path in out.rglob("*"):
        if path.is_file() and path.suffix in COMPRESSIBLE and path.stat().st_size > 1024:
            packed = gzip.compress(path.read_bytes(), compresslevel=9, mtime=0)
            if len(packed) < path.stat().st_size:
                path.with_name(path.name + ".gz").write_bytes(packed)


if __name__ == "__main__":
    began = time.monotonic()
    main()
    # where the time went: the satex runs' seconds against the build's, and the slowest runs
    took = sum(t for t, _ in RUNS)
    print(f"playground: {len(RUNS)} satex runs, {took:.0f} s of {JOBS} slots' time in {time.monotonic() - began:.0f} s; slowest: "
          + "; ".join(f"{t:.0f} s {c[:90]}" for t, c in sorted(RUNS, reverse=True)[:5]), file=sys.stderr)
