#!/usr/bin/env python3
"""Builds the project site and its playground from the repository.

Nothing here is copied text: pages come from the Markdown in the tree, the
navigation from the wiki's _Sidebar.md, the version from Cargo.toml, the logo
from doc/logo/logo.tex, the playground's TeX files from what `satex query
files` says the samples and the offered packages read, and its caches from `satex cache export`.

    build.py --satex target/release/satex --wasm target/wasm32-unknown-emscripten/release --out site-out
"""

import argparse, base64, concurrent.futures, datetime, gzip, hashlib, html, inspect, json, os, pickle, re, shutil, subprocess, sys, time, tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "doc" / "site"


def run(*cmd, **kw):
    return subprocess.run(cmd, check=True, text=True, capture_output=True, **kw).stdout


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


def parallel(label, jobs, f):
    """`f` of each job, in order, JOBS at a time, counting the finished ones on stderr."""
    jobs, out, done = list(jobs), {}, 0
    tty = sys.stderr.isatty()
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        running = {pool.submit(f, job): i for i, job in enumerate(jobs)}
        for finished in concurrent.futures.as_completed(running):
            out[running[finished]] = finished.result()
            done += 1
            line = f"playground: {label}: {done}/{len(jobs)}"
            if tty:
                print(f"\r{line}", end="\n" if done == len(jobs) else "", file=sys.stderr, flush=True)
            elif done == len(jobs) or done % max(1, len(jobs) // 10) == 0:
                print(line, file=sys.stderr)
    return [out[i] for i in range(len(jobs))]
# What `satex query files` listed, by what it depends on: the document, the
# flags, the project config and the installation (its ls-R files). A listing
# does not depend on the satex build, so a rebuild reuses it; --fresh drops it.
LISTINGS = {}
# The satex binary's digest, its cache key (what the analysis is: reporting code
# aside) and whether to ignore remembered results (--fresh): set by main().
BINARY, KEY, FRESH, SATEX = "", "", False, None


def memo(work, label, parts, make):
    """`make()`, remembered in WORK/memo under `label` while `parts` (what it depends on) stay the
    same: a build that changes only the page's code reuses every satex run."""
    path = work / "memo" / f"{label}-{digest(json.dumps(parts, default=str).encode())}.pkl"
    if path.exists() and not FRESH:
        return pickle.loads(path.read_bytes())
    value = make()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(pickle.dumps(value))
    return value


def tree_digest(root):
    """What the text files under `root` say."""
    return digest(b"".join(p.name.encode() + p.read_bytes() for p in sorted(root.rglob("*"))
                           if p.is_file() and p.suffix in (".tex", ".bib", ".sty", ".cls", ".yaml", ".cfg", ".def", ".txt")))


def installation_stamp():
    lsr = run("kpsewhich", "--all", "ls-R").split()
    return ";".join(f"{p}:{os.stat(p).st_mtime_ns}" for p in sorted(lsr))


def files_read(satex, flags, doc, cwd, env, config=b""):
    """The paths `satex query files` lists for `doc`, reused while nothing it depends on changed."""
    key = digest(json.dumps([flags, doc.read_bytes().decode(errors="replace"), config.decode(errors="replace"),
                             LISTINGS.get("@stamp", "")]).encode())
    if key not in LISTINGS:
        done = subprocess.run([str(satex), *flags, "query", "files", "-f", str(doc), "--format", "json"],
                              cwd=cwd, env=env, capture_output=True, text=True)
        LISTINGS[key] = [entry["path"] for entry in json.loads(done.stdout or "[]")]
    return LISTINGS[key]


def emit(out, name, data):
    """Writes `data` under a content-addressed name, so it can be cached forever."""
    stem, ext = name.split(".", 1)
    path = f"{stem}-{digest(data)}.{ext}"
    (out / path).parent.mkdir(parents=True, exist_ok=True)
    (out / path).write_bytes(data)
    return path


def emit_local(out, name, data):
    """emit(), plus a classic script setting the file as base64: a page opened
    from disk can neither fetch it nor hand its URL to a worker."""
    path = emit(out, name, data)
    (out / f"{path}.js").write_text(f'(self.SATEX_FILES ||= {{}})["{path}"] = "{base64.b64encode(data).decode()}";\n')
    return path


# ---------------------------------------------------------------- pages

def pages():
    """Repository Markdown -> site path: the README is the guide, wiki pages live under wiki/."""
    table = {"README.md": "guide.html", "SEMANTICS.md": "SEMANTICS.html",
             "doc/chktex-coverage.md": "chktex-coverage.html", "doc/developing.md": "developing.html"}
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


def markdown(text):
    return run("pandoc", "-f", "gfm", "-t", "html5", "--syntax-highlighting=none", "--wrap=none", input=text)


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


# ---------------------------------------------------------------- logo

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


# ---------------------------------------------------------------- playground

EXAMPLES = SITE / "examples"


def roots(samples):
    return sorted(p.name for p in samples.glob("*.tex") if "\\documentclass" in p.read_text(errors="replace"))


def sources():
    """The documents the playground offers: the site's examples, then the repository's samples."""
    return [EXAMPLES / n for n in roots(EXAMPLES)] + [ROOT / "samples" / n for n in roots(ROOT / "samples")]


# The kernels the playground offers: a name, a label, the flags that select it.
# pdfTeX comes with the page; each other one is a bundle of its own, fetched when chosen.
KERNELS = [("pdftex", "pdfTeX", ["--set", "engine=pdftex"]), ("xetex", "XeTeX", ["--set", "engine=xetex"]),
           ("plain", "plain TeX", ["--set", "engine=tex", "--set", "kernel=plain"])]
# Listed, not offered: why each one is missing.
UNAVAILABLE = [("LuaTeX", "Not available yet: the LuaTeX kernel is cut short at the format budget, so it is never cached. "
                "Every check rebuilds it: 7 s natively, about half a minute in the browser.")]


SAMPLE = "hello.tex"  # the document the playground opens with: the first download holds what it reads


def file_of(name):
    """The file a package (`name`) or class (`name.cls`) is."""
    return name if name.endswith(".cls") else f"{name}.sty"


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
        # What another option would read instead (`[11pt]`: size11.clo for size10.clo): the TeX
        # inputs of the same kind beside each one read. Classes, packages and documents load by
        # name; fonts are no option's input.
        option = lambda p: "tex" in p.relative_to(dist).parts[:1] and p.suffix not in (".sty", ".cls", ".tex", ".ltx")
        return read | {q for p in read if option(p) for q in p.parent.glob(f"*{p.suffix}")}

    absent = {}
    reads = {}  # (kernel, document or package) -> the distribution files it reads
    jobs = [(kernel, flags, doc) for kernel, _, flags in KERNELS for doc in docs]
    for (kernel, _, doc), paths in zip(jobs, parallel("the files the examples read", jobs, lambda j: listed((j[1], j[2], ROOT)))):
        reads[(kernel, doc.name)] = paths
    # The names: the list's, then every package or class an example reads.
    every = list(dict.fromkeys(package_names() + sorted(
        f"{p.stem}.cls" if p.suffix == ".cls" else p.stem for paths in reads.values() for p in paths if p.suffix in (".sty", ".cls"))))
    # Names the installation has (test fixtures' names are not); plain TeX loads no LaTeX package.
    have = set(Path(p).name for p in subprocess.run(["kpsewhich", *map(file_of, every)], capture_output=True, text=True).stdout.split())
    every = [n for n in every if file_of(n) in have]
    pjobs = [(kernel, flags, name) for kernel, _, flags in KERNELS if kernel != "plain" for name in every]
    for (kernel, _, name), paths in zip(pjobs, parallel("the files each package reads", pjobs, lambda j: listed((j[1], probe_doc(probes, j[2]), probes)))):
        if file_of(name) in {p.name for p in paths}:
            reads[(kernel, name)] = paths
        else:
            absent.setdefault(kernel, []).append(name)
    for kernel, names in absent.items():
        print(f"playground: {len(names)} packages not offered under {kernel} (they do not load there)", file=sys.stderr)
    for path in set().union(*reads.values()):  # the build tree: every file any of them reads
        target = tree / path.relative_to(dist)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)
    config = (SITE / "playground.yaml").read_text().replace("/texmf.bundle", str(tree))
    shutil.copytree(ROOT / "samples", project, ignore=shutil.ignore_patterns("satex.yaml"))
    for doc in docs[:len(roots(EXAMPLES))]:
        shutil.copy2(doc, project / doc.name)
    for doc in sources():
        for companion in projects()[doc.name]:
            shutil.copy2(doc.parent / companion, project / companion)
    (project / "satex.yaml").write_text(config)
    (probes / "satex.yaml").write_text(config)

    # Kernels: the sample's files and the cache entries of running it, all kernels at once. A kernel's
    # cache directory stays from the last build: satex rebuilds what this binary cannot use, and
    # `cache export` writes only the valid entries.
    def kernel_caches(kernel, flags):
        kenv = dict(os.environ, SATEX_CACHE=str(work / f"cache-{kernel}"))
        subprocess.run([str(satex), *flags, "lint", "-f", SAMPLE], cwd=project, env=kenv, capture_output=True, check=False)
        archive = work / f"satex-cache-{kernel}.bin"
        run(str(satex), "cache", "export", str(archive), env=kenv)
        return archive

    archives = parallel("kernel caches", KERNELS, lambda k: kernel_caches(k[0], k[2]))
    tables, have = [], set()
    for (kernel, _, _), archive in zip(KERNELS, archives):
        files = {bundled(p): p.read_bytes() for p in sorted(reads[(kernel, SAMPLE)]) if bundled(p) not in have}
        have |= set(files)
        tables.append({**files, f"/kernels/{kernel}/satex-cache.bin": archive.read_bytes()})
    kernels = [k for k, _, _ in KERNELS]

    # Packages: the files a probe document reads. The cache entries are those the examples' runs, from a
    # copy of the kernel's caches, made for a package, whichever document loaded it: a package's entry
    # holds the state of everything loaded before it, so a probe's own would rarely match a document's.
    def fresh(kernel, flags):
        """One cache archive per package or class held by the entries running every example made,
        from a copy of the kernel's caches (`satex cache export --only`)."""
        return memo(work, f"fresh-{kernel}", [KEY, kernel, flags, tree_digest(project)], lambda: fresh_run(kernel, flags))

    def fresh_run(kernel, flags):
        cache = work / f"cache-docs-{kernel}"
        shutil.rmtree(cache, ignore_errors=True)
        shutil.copytree(work / f"cache-{kernel}", cache, copy_function=os.link, ignore=shutil.ignore_patterns("*.lock"))
        penv = dict(os.environ, SATEX_CACHE=str(cache))
        # the kernels' runs go at once (`ran`), so each takes its share of the jobs
        with concurrent.futures.ThreadPoolExecutor(max(1, JOBS // len(KERNELS))) as inner:
            list(inner.map(lambda d: subprocess.run([str(satex), *flags, "lint", "-f", d.name], cwd=project, env=penv,
                                                    capture_output=True, check=False), docs))
        out = work / f"export-{kernel}.bin"
        held = set(json.loads(run(str(satex), "--format", "json", "cache", "export", str(out), env=penv))["holds"].values())
        archives = {}
        for stem in sorted(held):
            run(str(satex), "cache", "export", str(out), "--only", stem, env=penv)
            archives[stem] = out.read_bytes()
        out.unlink()
        shutil.rmtree(cache, ignore_errors=True)
        return archives

    parts = {}  # (kernel, name) -> [files, cache archive, direct needs]
    ran = dict(zip((k for k, _, _ in KERNELS), parallel("package caches from the examples", KERNELS, lambda k: fresh(k[0], k[2]))))
    for (kernel, name) in reads:
        if name in {d.name for d in docs}:
            continue
        k = kernels.index(kernel)
        held = set(tables[0]) | set(tables[k])
        seen = {p.name for p in reads[(kernel, name)]}
        parts[(kernel, name)] = [
            {bundled(p): p for p in reads[(kernel, name)] if bundled(p) not in held},
            ran[kernel].get(name.removesuffix(".cls")),
            {x for x in every if x != name and (kernel, x) in reads and file_of(x) in seen}]
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
    tables[0]["/work/satex.yaml"] = config.replace(str(tree), "/texmf.bundle").encode()
    return tables, packages, [p.name for p in docs], project


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
    done = subprocess.run([str(satex), "explain", *sorted(names), "--format", "json", "-f", name], cwd=project, env=env,
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
    lsp = json.loads(subprocess.run([str(satex), "lint", "--format", "lsp", "-f", name], cwd=project, env=env,
                                    capture_output=True, text=True).stdout or "[]")
    rows = json.loads(subprocess.run([str(satex), "lint", "--format", "json", "-f", name], cwd=project, env=env,
                                     capture_output=True, text=True).stdout or "[]")
    lines = source.splitlines()
    text = lambda p: lines[p["line"] - 1] if p["line"] - 1 < len(lines) else ""
    out = []
    for d in (d for f in lsp for d in f.get("diagnostics", [])):
        start, end = d["range"]["start"], d["range"]["end"]
        row = next((r for r in rows if r["code"] == d["code"] and r["message"] == d["message"]), {})
        edits = [{"from": text(e["start"])[e["start"]["col"] - 1:e["end"]["col"] - 1] if e["start"]["line"] == e["end"]["line"] else "",
                  "to": e["replacement"]} for e in row.get("edits", []) if e.get("path", name) == name]
        sev = ["", "error", "warning", "info", "info"][d.get("severity", 3)]
        data = d.get("data", {})
        out.append({"code": d["code"], "message": d["message"], "severity": sev, "rank": ["error", "warning", "info"].index(sev),
                    "start": (start["line"], start["character"]), "end": (end["line"], end["character"]),
                    "fix": data.get("fix", ""), "applicability": data.get("applicability", "unsafe"), "edits": edits,
                    "typo": bool(TYPOGRAPHIC.search(d["code"])) and start["line"] == end["line"] and end["character"] - start["character"] <= 3})
    return sorted(out, key=lambda d: (d["rank"], d["start"]))


def examples(satex, work, project):
    """Each example as a tab, analyzed at build time: its findings underlined in the source
    and listed beside it, and what `satex explain` says of every command it uses, shown on
    hover. The panel starts with the command `commands()` names, else the example's own."""
    env = dict(os.environ, SATEX_CACHE=str(work / f"cache-{KERNELS[0][0]}"))
    dist = Path(run("kpsewhich", "-var-value", "TEXMFDIST").strip()).resolve()
    tabs, panels = [], []
    for i, name in enumerate(roots(EXAMPLES)):
        source = (EXAMPLES / name).read_text()
        lints = findings(satex, project, env, name, source)
        used = {m.group() for line in source.splitlines() for m in TOKEN.finditer(line) if m.lastgroup == "cmd"}
        cs = explained(satex, project, env, name, used, dist)
        called = {x for rs in cs.values() for r in rs for x in r.get("expands", []) + [r.get("by")] if x and x not in cs}
        cs |= explained(satex, project, env, name, called, dist)
        argv = commands()[name].split()
        order = [m.group() for line in source.splitlines() for m in TOKEN.finditer(line) if m.group() in cs and m.group() != "\\documentclass"]
        own = [n for n in order if any(r.get("path") == name for r in cs[n])]
        initial = argv[1] if argv[0] == "explain" and len(argv) > 1 else (own or order or [""])[0]
        data = json.dumps({"cs": cs, "lints": lints, "initial": initial}).replace("</", "<\\/")
        lines = "\n".join(f'<span class="ln" data-l="{n + 1}">{tex_html(l, n, lints)}</span>' for n, l in enumerate(source.splitlines()))
        items = "".join(f'<li class="f-{d["severity"]}" data-k="{k}"><span class="sev sev-{d["severity"]}">{d["severity"]}</span> '
                        f'<b>{d["start"][0] + 1}:{d["start"][1] + 1}</b> <code>{html.escape(d["code"])}</code> {re.sub(r"`([^`]+)`", r"<code>\1</code>", html.escape(d["message"]))}</li>'
                        for k, d in enumerate(lints))
        found = f'<h4>Findings <b>{len(lints)}</b></h4><ul class="diagnostics">{items}</ul>' if lints else '<h4>Findings</h4><p class="muted">none</p>'
        sel = "true" if i == 0 else "false"
        tabs.append(f'<button role="tab" aria-selected="{sel}" aria-controls="ex-{i}" id="ex-tab-{i}">{html.escape(name)}</button>')
        panels.append(
            f'<figure class="example" role="tabpanel" id="ex-{i}" aria-labelledby="ex-tab-{i}"{"" if i == 0 else " hidden"}>\n'
            f'<figcaption><span class="muted">Hover a command to explain it, an underline to see its finding.</span>'
            f'<a class="button ghost small" href="play/index.html#{name}">open in the playground →</a></figcaption>\n'
            f'<pre class="src"><code>{lines}</code></pre>\n'
            f'<div class="ex-side"><div class="ex-findings">{found}</div>'
            f'<div class="ex-explain"><h4>Explain <code class="xname"></code></h4><div class="xbody"><p class="muted">Hover a command.</p></div></div></div>\n'
            f'<script type="application/json" class="ex-data">{data}</script></figure>')
    return f'<div class="tabs" role="tablist">{"".join(tabs)}</div>' + "\n".join(panels)


def playground(satex, wasm, work, out):
    offered = package_names()
    sources_ = [(d.name, d.read_bytes()) for d in sources()] + [(c, (d.parent / c).read_bytes()) for d in sources() for c in projects()[d.name]]
    tables, packages, samples, project = memo(
        work, "bundle", [KEY, LISTINGS.get("@stamp"), offered, sources_, tree_digest(ROOT / "samples"), (SITE / "playground.yaml").read_text(),
                         inspect.getsource(bundle)],
        lambda: bundle(satex, work))
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
                        "bundle": emit_local(out, f"play/{kernel}.bin.gz", packed),
                        "files": [[path, len(data)] for path, data in files.items()]})
    extra = []
    for kernel, name, files, needs in packages:
        packed = gzip.compress(b"".join(files.values()), 9, mtime=0)
        extra.append({"id": f"{kernel}/{name}", "name": name.removesuffix(".cls"), "kernel": kernel, "needs": needs, "label": name,
                      "size": len(packed), "bundle": emit_local(out, f"play/pkg/{kernel}-{name}.bin.gz", packed),
                      "files": [[path, len(data)] for path, data in files.items()]})
    def facts():
        """What satex says about itself: its queries, which of them name their file, its rules, its completions."""
        queries = [line.split()[0] for line in run(str(satex), "query", "--list").splitlines()[1:] if line.strip()]
        env = dict(os.environ, SATEX_CACHE=str(work / f"cache-{KERNELS[0][0]}"))
        sample = (EXAMPLES / roots(EXAMPLES)[0]).name

        def per_file(query):
            """Whether the query's rows name their file: the playground then shows the open file's rows only."""
            done = subprocess.run([str(satex), "query", query, "--filter", f"file={sample}", "--format", "json", "-f", sample],
                                  cwd=project, env=env, capture_output=True, text=True)
            try:
                return done.returncode == 0 and len(json.loads(done.stdout or "[]")) > 0
            except json.JSONDecodeError:
                return False
        rules = {r["code"]: {**r, **json.loads(run(str(satex), "lint", "--explain", r["code"], "--format", "json"))}
                 for r in json.loads(run(str(satex), "lint", "--rules", "--format", "json"))}
        return queries, [q for q in queries if per_file(q)], rules, json.loads(run(str(satex), "completions", "json"))
    queries, filtered, rules, completion = memo(work, "facts", [BINARY, tree_digest(project)], facts)
    manifest = {
        "js": emit_local(out, "play/satex.js", (wasm / "satex.js").read_bytes()),
        "wasm": emit_local(out, "play/satex.wasm", wasm_bytes), "wasmSize": len(wasm_bytes),
        "kernels": kernels, "packages": extra, "unavailable": UNAVAILABLE, "queries": queries, "filtered": filtered, "rules": rules,
        "completion": completion, "samples": samples, "sample": "hello.tex", "root": "../",
        "commands": {name: command for name, command in commands().items() if command != "lint"}, "projects": projects(),
    }
    sizes = {k["id"]: f"{k['size'] / 2**20:.1f} MiB gz, {len(k['files'])} files" for k in kernels}
    print(f"playground: wasm {len(wasm_bytes) / 2**20:.1f} MiB, {sizes}; {len(extra)} package bundles on demand, "
          f"{sum(p['size'] for p in extra) / 2**20:.1f} MiB gz in all", file=sys.stderr)
    return manifest, examples(satex, work, project)


# ---------------------------------------------------------------- site

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--satex", type=Path, required=True, help="the native satex binary")
    ap.add_argument("--wasm", type=Path, required=True, help="directory with the Emscripten satex.js and satex.wasm")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--work", type=Path, help="scratch directory (default: OUT/../site-work)")
    ap.add_argument("--precompress", action="store_true",
                    help="write a .gz beside each compressible file, for a server that serves those as GitHub Pages compresses")
    ap.add_argument("--fresh", action="store_true", help="redo every satex run instead of reusing WORK/listings.json and WORK/memo")
    args = ap.parse_args()
    global BINARY, KEY, FRESH, SATEX
    SATEX = args.satex.resolve()
    BINARY, FRESH = digest(args.satex.resolve().read_bytes()), args.fresh
    KEY = json.loads(subprocess.run([str(args.satex), "cache", "--format", "json"], capture_output=True, text=True).stdout or "{}").get("key") or BINARY
    out, work = args.out.resolve(), (args.work or args.out.resolve().parent / "site-work").resolve()
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    work.mkdir(parents=True, exist_ok=True)
    listings = work / "listings.json"
    stamp = installation_stamp() + ";" + KEY
    if listings.exists() and not args.fresh:
        LISTINGS.update(json.loads(listings.read_text()))
    if LISTINGS.get("@stamp") != stamp:
        LISTINGS.clear()
        LISTINGS["@stamp"] = stamp

    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]
    homepage, repo = cargo["homepage"].rstrip("/") + "/", cargo["repository"].rstrip("/")
    template = (SITE / "template.html").read_text()
    (out / "play").mkdir()
    svgs = logo(work)
    assets = {name: emit(out, "favicon.png" if name == "favpng" else f"logo-{name}.svg", data) for name, data in svgs.items()}
    assets |= {
        "css": emit(out, "style.css", (SITE / "style.css").read_bytes()),
        "sitejs": emit(out, "site.js", (SITE / "site.js").read_bytes()),
        "playjs": emit(out, "play/playground.js", (SITE / "playground.js").read_bytes()),
    }
    table = pages()
    nav = sidebar(table)
    stamp = os.environ.get("SOURCE_DATE_EPOCH")
    built = (datetime.datetime.fromtimestamp(int(stamp), datetime.UTC) if stamp else datetime.datetime.now(datetime.UTC)).date().isoformat()

    def fill(text, values):
        return re.sub(r"\{\{(\w+)\}\}", lambda m: values.get(m.group(1), m.group(0)), text)

    def page(dst, title, body, side=True, kind=None):
        root = "../" * dst.count("/")
        side_html = markdown(nav).replace("@ROOT@", root) if side else ""
        values = {"title": html.escape(title), "root": root, "body": body, "sidebar": side_html,
                  "version": cargo["version"], "repository": repo, "description": html.escape(cargo["description"]),
                  "class": kind or ("doc" if side else "home"), "date": built, "crate": cargo["name"], **assets}
        text = fill(template, values)
        (out / dst).parent.mkdir(parents=True, exist_ok=True)
        (out / dst).write_text(text)

    for src, dst in table.items():
        text = strip_banner((ROOT / src).read_text())
        body = rewrite(markdown(text), src, table, homepage, repo, out, "../" * dst.count("/"))
        page(dst, title_of(text), body)

    manifest, shown = playground(args.satex.resolve(), args.wasm.resolve(), work, out)
    listings.write_text(json.dumps(LISTINGS))
    manifest["worker"] = (SITE / "worker.js").read_text()
    readme = (ROOT / "README.md").read_text()
    md = lambda text: rewrite(markdown(text), "README.md", table, homepage, repo, out, "")
    parts = {"intro": md(intro(readme)), "install": md(install(readme)), "examples": shown,
             "manifest": json.dumps(manifest).replace("</", "<\\/"), "root": "../"}
    home = fill((SITE / "home.html").read_text(), {**assets, **parts, "version": cargo["version"], "repository": repo,
                                                       "description": html.escape(cargo["description"])})
    page("index.html", "satex", home, side=False)
    page("play/index.html", "Playground", fill((SITE / "play.html").read_text(), {**assets, **parts}), side=False, kind="play")
    (out / ".nojekyll").write_text("")
    if args.precompress:
        precompress(out)


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
    main()
