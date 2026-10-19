#!/usr/bin/env python3
"""Builds the project site and its playground from the repository.

Nothing here is copied text: pages come from the Markdown in the tree, the
navigation from the wiki's _Sidebar.md, the version from Cargo.toml, the logo
from doc/logo/logo.tex, the playground's TeX files from what `satex query
files` says the samples and the offered packages read, and its caches from `satex cache export`.

    build.py --satex target/release/satex --wasm target/wasm32-unknown-emscripten/release --out site-out
"""

import argparse, base64, concurrent.futures, datetime, gzip, hashlib, html, json, os, re, shutil, subprocess, sys, tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "doc" / "site"


def run(*cmd, **kw):
    return subprocess.run(cmd, check=True, text=True, capture_output=True, **kw).stdout


def digest(data):
    return hashlib.sha256(data).hexdigest()[:12]


# How many satex runs go at once: each takes a core and up to a few GB.
JOBS = max(1, min(8, (os.cpu_count() or 2) // 2))
# What `satex query files` listed, by what it depends on: the document, the
# flags, the project config and the installation (its ls-R files). A listing
# does not depend on the satex build, so a rebuild reuses it; --fresh drops it.
LISTINGS = {}


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


def bundle(satex, work):
    """The TeX files the samples read under each kernel, laid out like texmf-dist, with
    the caches a run against that tree alone builds: the browser's runs then replay them.
    Returns one file table per kernel: the first holds the samples, the others what they add."""
    samples = ROOT / "samples"
    docs = sources()
    names = [p.name for p in docs]
    dist = Path(run("kpsewhich", "-var-value", "TEXMFDIST").strip()).resolve()
    tree, project = work / "texmf.bundle", work / "work"
    for d in (tree, project):
        shutil.rmtree(d, ignore_errors=True)
    env = dict(os.environ, SATEX_CACHE=str(work / "cache-install"))
    read = {kernel: set() for kernel, _, _ in KERNELS}
    jobs = [(kernel, flags, doc) for kernel, _, flags in KERNELS for doc in docs]
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        listed = pool.map(lambda j: files_read(satex, j[1], j[2], ROOT, env), jobs)
        for (kernel, _, _), paths in zip(jobs, listed):
            for entry in paths:
                path = Path(entry) if Path(entry).is_absolute() else ROOT / entry
                if path.resolve().is_relative_to(dist):
                    read[kernel].add(path.resolve())
    for path in set().union(*read.values()):
        target = tree / path.relative_to(dist)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)
    config = (SITE / "playground.yaml").read_text()
    shutil.copytree(samples, project, ignore=shutil.ignore_patterns("satex.yaml"))
    for doc in docs[:len(roots(EXAMPLES))]:
        shutil.copy2(doc, project / doc.name)
    for database in EXAMPLES.glob("*.bib"):
        shutil.copy2(database, project / database.name)
    (project / "satex.yaml").write_text(config.replace("/texmf.bundle", str(tree)))
    tables, shared = [], read[KERNELS[0][0]]
    for i, (kernel, _, flags) in enumerate(KERNELS):
        cache = work / f"cache-{kernel}"
        env = dict(os.environ, SATEX_CACHE=str(cache))
        with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
            list(pool.map(lambda name: subprocess.run([str(satex), *flags, "lint", "-f", name], cwd=project, env=env,
                                                      capture_output=True, check=False), names))
        archive = work / f"satex-cache-{kernel}.bin"
        run(str(satex), "cache", "export", str(archive), env=env)
        own = sorted(read[kernel]) if i == 0 else sorted(read[kernel] - shared)
        files = {f"/texmf.bundle/{p.relative_to(dist).as_posix()}": p.read_bytes() for p in own}
        files[f"/kernels/{kernel}/satex-cache.bin"] = archive.read_bytes()
        tables.append(files)
    for path in sorted(project.iterdir()):
        if path.is_file() and path.name != "satex.yaml":
            tables[0][f"/work/{path.name}"] = path.read_bytes()
    tables[0]["/work/satex.yaml"] = config.encode()
    return tables, names, project


def package_names():
    """doc/site/packages.txt, then satex.yaml's `cache.index.common`, each once."""
    names = re.sub(r"#.*", "", (SITE / "packages.txt").read_text()).split()
    config = (ROOT / "satex.yaml").read_text()
    common = re.search(r"^\s*common:\s*\[([^\]]*)\]", config, flags=re.M)
    if not common and re.search(r"^\s*common:", config, flags=re.M):
        sys.exit("build.py: satex.yaml's cache.index.common must stay a [a, b, ...] flow list")
    return list(dict.fromkeys(names + [n.strip() for n in (common.group(1).split(",") if common else []) if n.strip()]))


def packages(satex, work, have):
    """One bundle per package of `package_names()`: the distribution files a document
    loading it reads under each kernel, but those the browser has with that kernel
    loaded (`have`, one set per kernel).
    A name satex finds no file for is left out, and said."""
    dist = Path(run("kpsewhich", "-var-value", "TEXMFDIST").strip()).resolve()
    probes = work / "probes"
    shutil.rmtree(probes, ignore_errors=True)
    probes.mkdir(parents=True)
    env = dict(os.environ, SATEX_CACHE=str(work / "cache-install"))

    def probe(name):
        cls = name.endswith(".cls")
        stem = name.removesuffix(".cls")
        doc = probes / f"{stem}.tex"
        head = f"\\documentclass{{{stem}}}" if cls else f"\\documentclass{{article}}\n\\usepackage{{{stem}}}"
        doc.write_text(head + "\n\\begin{document}\nText.\n\\end{document}\n")
        seen, files = set(), {}
        for (_, _, flags), loaded in zip(KERNELS, have):
            for entry in files_read(satex, flags, doc, probes, env):
                path = (probes / entry).resolve()
                if not path.is_relative_to(dist):
                    continue
                seen.add(path.name)
                bundled = f"/texmf.bundle/{path.relative_to(dist).as_posix()}"
                if bundled not in loaded:
                    files[bundled] = path
        if (f"{stem}.cls" if cls else f"{stem}.sty") not in seen:
            return name, None
        return name, {path: files[path].read_bytes() for path in sorted(files)}

    found = {}
    with concurrent.futures.ThreadPoolExecutor(JOBS) as pool:
        for name, files in pool.map(probe, package_names()):
            if files is None:
                print(f"playground: no {name} in this installation, not offered", file=sys.stderr)
            else:
                found[name.removesuffix(".cls")] = files
    return found


def commands():
    """`examples/commands.txt`: the command each example runs, `lint` unless named there."""
    named = dict(line.split(": ", 1) for line in (EXAMPLES / "commands.txt").read_text().splitlines() if line and not line.startswith("#"))
    return {name: named.get(name, "lint") for name in roots(EXAMPLES)}


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


def findings(satex, project, env, name, source):
    """The example's findings: ranges as the language server reports them, with the text
    each fix replaces."""
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
    tables, samples, project = bundle(satex, work)
    wasm_bytes = (wasm / "satex.wasm").read_bytes()
    kernels = []
    for (kernel, label, flags), files in zip(KERNELS, tables):
        packed = gzip.compress(b"".join(files.values()), 9, mtime=0)
        kernels.append({"id": kernel, "label": label, "args": flags, "size": len(packed),
                        "bundle": emit_local(out, f"play/{kernel}.bin.gz", packed),
                        "files": [[path, len(data)] for path, data in files.items()]})
    extra = []
    for name, files in packages(satex, work, [set(tables[0]) | set(t) for t in tables]).items():
        if not files:  # all in the first download already
            continue
        packed = gzip.compress(b"".join(files.values()), 9, mtime=0)
        extra.append({"id": name, "label": name, "size": len(packed), "bundle": emit_local(out, f"play/pkg/{name}.bin.gz", packed),
                      "files": [[path, len(data)] for path, data in files.items()]})
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
    manifest = {
        "js": emit_local(out, "play/satex.js", (wasm / "satex.js").read_bytes()),
        "wasm": emit_local(out, "play/satex.wasm", wasm_bytes), "wasmSize": len(wasm_bytes),
        "kernels": kernels, "packages": extra, "unavailable": UNAVAILABLE, "queries": queries, "filtered": [q for q in queries if per_file(q)], "rules": rules,
        "completion": json.loads(run(str(satex), "completions", "json")), "samples": samples, "sample": "hello.tex", "root": "../",
        "commands": {name: command for name, command in commands().items() if command != "lint"},
    }
    sizes = {k["id"]: f"{k['size'] / 2**20:.1f} MiB gz, {len(k['files'])} files" for k in kernels}
    print(f"playground: wasm {len(wasm_bytes) / 2**20:.1f} MiB, {sizes}; {len(extra)} packages on demand, "
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
    ap.add_argument("--fresh", action="store_true", help="list every file read anew instead of reusing WORK/listings.json")
    args = ap.parse_args()
    out, work = args.out.resolve(), (args.work or args.out.resolve().parent / "site-work").resolve()
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    work.mkdir(parents=True, exist_ok=True)
    listings = work / "listings.json"
    stamp = installation_stamp()
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
