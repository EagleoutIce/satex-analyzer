"""The formal chapter of the site: the Lean model rendered as linked code, the Lean <-> Rust mapping and the
primitive table (semantics/), and the hover cards of control sequences named in code spans.

Names, types and doc comments come from the Lean environment (`export.lean`, run by `lake env lean --run`);
the sources give the text, the namespaces and where each declaration is. A link whose target is missing
fails the build (`Missing`)."""
import html, json, re, subprocess, sys
from pathlib import Path
from urllib.parse import quote

LEAN = ["Core", "Concrete", "Abstract", "Soundness"]
KEYWORDS = set("""def theorem lemma inductive structure abbrev instance class opaque axiom example namespace end section open in
where deriving with fun match if then else let have show from by do return at using mutual variable universe private protected
partial unsafe noncomputable attribute macro syntax notation infix infixl infixr prefix postfix set_option import export termination_by
decreasing_by calc obtain rcases intro intros exact apply simp rfl cases induction unfold rw constructor omega decide refine
exists fun· λ ∀ ∃ extends Type Prop Sort suffices nomatch nofun for unless mut deriving_handler local scoped""".split())
TACTICS_ONLY = {"exact", "apply", "simp", "rfl", "cases", "induction", "unfold", "rw", "constructor", "omega", "decide", "intro", "intros",
                "refine", "obtain", "rcases", "exists", "using", "simp_all", "contradiction", "assumption", "specialize", "subst", "split"}


class Missing(Exception):
    """A link target that does not exist."""


def esc(s):
    return html.escape(s, quote=False)


def attr(s):
    return html.escape(s, quote=True)


# ---------------------------------------------------------------- Lean environment

def export(lean_dir, script):
    """Every Satex declaration (`export.lean`): name, kind, type, doc, module, line."""
    done = subprocess.run(["bash", "-c", f"source ~/.elan/env 2>/dev/null; cd '{lean_dir}' && lake build -q Satex >&2 && lake env lean --run '{script}'"],
                          capture_output=True, text=True)
    if done.returncode:
        raise Missing(f"lean export failed: {done.stderr[-600:]}")
    return json.loads(done.stdout)


class Env:
    """The declarations by full name, and the lookups the renderer needs."""

    def __init__(self, decls):
        self.by = {d["name"]: d for d in decls}
        self.last = {}
        for n, d in self.by.items():
            if d["kind"] == "constructor" or d["kind"] in ("def", "theorem", "inductive", "opaque", "axiom"):
                self.last.setdefault(n.rsplit(".", 1)[-1], []).append(n)

    def resolve(self, tok, ns, opens, recent=()):
        """The declaration a source token names inside namespace `ns` (a list of components) with `opens`, or None.
        `.name` is the constructor or definition of that name: the one of the only type mentioned so far in the
        declaration (`recent`), else the only one there is."""
        if tok.startswith("."):
            c = self.last.get(tok[1:], [])
            near = [n for n in c if n.rsplit(".", 1)[0] in recent]
            c = near if len(near) == 1 else c
            return c[0] if len(c) == 1 else None
        spaces = [".".join(ns[:i]) for i in range(len(ns), -1, -1)]
        for pre in spaces + [f"{s}.{o}" if s else o for o in opens for s in spaces] + list(opens):
            full = f"{pre}.{tok}" if pre else tok
            if full in self.by:
                return full
        return None

    def field(self, name):
        """`x.field` on a local `x`: the unique structure field or definition of that name in a type's namespace."""
        c = [n for n in self.last.get(name, []) if "." in n and n.rsplit(".", 1)[0] in self.by and self.by[n.rsplit(".", 1)[0]]["kind"] == "inductive"]
        return c[0] if len(c) == 1 else None


# ---------------------------------------------------------------- tokens

IDENT = r"[^\W\d][\w'!?]*(?:\.(?:[^\W\d]|\{)[\w'!?]*)*"
TOKEN = re.compile(r"""
  (?P<line>--[^\n]*)
 |(?P<str>"(?:[^"\\\n]|\\.)*")
 |(?P<chr>(?<![\w'])'(?:[^'\\\n]|\\.[^'\n]*)')
 |(?P<num>\b\d[\d_]*(?:\.\d+)?\b|0x[0-9A-Fa-f]+)
 |(?P<id>\.?""" + IDENT + r""")
 |(?P<other>.)""", re.X | re.S)


def block_comment_end(text, i):
    """Index after the `-/` that closes the (nested) comment opened at `i` (`/-`)."""
    depth, j = 0, i
    while j < len(text):
        if text.startswith("/-", j):
            depth, j = depth + 1, j + 2
        elif text.startswith("-/", j):
            depth, j = depth - 1, j + 2
            if depth == 0:
                return j
        else:
            j += 1
    return len(text)


def tokens(text):
    """(kind, text) pieces covering `text`: comment, doc (/-- and /-!), line, str, chr, num, id, other."""
    i = 0
    while i < len(text):
        if text.startswith("/-", i):
            j = block_comment_end(text, i)
            yield ("doc" if text.startswith(("/--", "/-!"), i) else "comment"), text[i:j]
            i = j
            continue
        m = TOKEN.match(text, i)
        yield m.lastgroup, m.group()
        i = m.end()


DECL = re.compile(r"^\s*(?:@\[[^\]]*\]\s*)*(?:(?:private|protected|noncomputable|partial|unsafe|nonrec)\s+)*"
                  r"(def|theorem|lemma|inductive|structure|abbrev|class|opaque|axiom|instance)\s+(?P<n>[^\s:({\[]+)?")


class Source:
    """A Lean file: its declarations (name, line, anchor) found from the text and checked against the environment."""

    def __init__(self, path, env):
        self.path, self.env = path, env
        self.text = path.read_text()
        self.lines = self.text.split("\n")
        self.anchors = {}      # line index -> list of full names declared there (before that line's tokens)
        self.scan()

    def scan(self):
        ns, kind_block, scopes = [], None, []
        env = self.env
        inside, row = set(), 0           # lines that start inside a block comment
        for kind, tok in tokens(self.text):
            if kind in ("comment", "doc"):
                inside.update(range(row + 1, row + tok.count("\n") + 1))
            row += tok.count("\n")
        for i, line in enumerate(self.lines):
            if i in inside:
                continue
            m = re.match(r"\s*(namespace|section|mutual)\b\s*(\S*)", line)
            if m:
                comps = m.group(2).split(".") if m.group(1) == "namespace" else []
                scopes.append(comps)
                ns += comps
                continue
            if re.match(r"end\b", line):
                if scopes:
                    k = len(scopes.pop())
                    if k:
                        del ns[-k:]
                continue
            if kind_block and (line.strip() and not line[0].isspace()) and not line.lstrip().startswith(("--", "/-")):
                kind_block = None
            m = DECL.match(line)
            if m and m.group("n") and not line.lstrip().startswith(("--", "/-")):
                name = m.group("n")
                if m.group(1) in ("inductive", "structure", "class"):
                    kind_block = None
                full = self.full(ns, name)
                if full:
                    self.anchors.setdefault(i, []).append(full)
                    if m.group(1) in ("inductive", "structure", "class"):
                        kind_block = (m.group(1), full)
                continue
            if kind_block:
                kind, owner = kind_block
                s = line.strip()
                if kind == "inductive" and s.startswith("|"):
                    parts = [p.strip() for p in s.split("|") if p.strip()]
                    first = lambda p: (re.match(r"[\w'.]+", p) or [None])[0]
                    names = [first(p) for p in parts] if all(re.fullmatch(r"[\w']+(\s*[({\[⟨].*)?", p) for p in parts) else [first(parts[0])] if parts else []
                    for n in filter(None, names):
                        if f"{owner}.{n}" in env.by:
                            self.anchors.setdefault(i, []).append(f"{owner}.{n}")
                elif kind != "inductive":
                    f = re.match(r"\s+(?:\(?([\w']+)\)?)\s*(?:\([^)]*\)\s*)*:", line)
                    if f and not s.startswith(("deriving", "--", "/-")) and f"{owner}.{f.group(1)}" in env.by:
                        self.anchors.setdefault(i, []).append(f"{owner}.{f.group(1)}")

    def full(self, ns, name):
        """The declaration's full name: inside `ns`, `Foo.bar` is `ns.Foo.bar`; a `_root_` prefix or an absent one falls back."""
        for k in range(len(ns), -1, -1):
            f = ".".join(ns[:k] + [name])
            if f in self.env.by:
                return f
        return None


# ---------------------------------------------------------------- § links

SECTION = re.compile(r"(§§?|\bSEM §§?)\s*(\d+(?:\.\d+)?)((?:\s*[–-]\s*\d+|(?:\s*,\s*|\s+and\s+)(?:§\s*)?\d+(?:[–-]\d+)?)*)")


class Refs:
    """Link targets outside the Lean model: tex.web sections (by line), SEMANTICS sections, Rust files, the commit."""

    def __init__(self, root, repo, commit, semantics_ids, site_root=""):
        self.root, self.repo, self.commit = root, repo, commit
        self.web = {int(a): int(b) for a, b in (l.split("\t") for l in (root / "doc/src/tex-web-sections.tsv").read_text().split("\n") if l)}
        self.sha = (root / "doc/src/tex-web-sha.txt").read_text().strip()
        self.sem = semantics_ids
        self.files = {}
        for p in (root / "src").rglob("*.rs"):
            self.files.setdefault(p.name, []).append(p.relative_to(root).as_posix())
        self.lengths = {}
        self.used = set()      # targets (page, anchor) the generated pages link to

    def web_url(self, n):
        if n not in self.web:
            raise Missing(f"tex.web section {n} does not exist")
        return f"https://github.com/TeX-Live/texlive-source/blob/{self.sha}/texk/web2c/tex.web#L{self.web[n]}"

    def link_sections(self, text, semantics_page):
        """`text` (already HTML-escaped) with every `§ n` a link: SEM § n to SEMANTICS, the rest to tex.web.
        etex_man and pdfTeX manual references stay text (they have no section map)."""
        def one(m):
            tag, first, rest = m.group(1), m.group(2), m.group(3)
            sem = tag.startswith("SEM")
            nums = [first] + re.findall(r"\d+(?:\.\d+)?", rest)
            if not sem and "." in first:
                return m.group(0)
            out = m.group(0)
            if sem:
                n = first
                if n not in self.sem:
                    raise Missing(f"SEMANTICS has no section {n}")
                self.used.add((semantics_page.removeprefix("../"), self.sem[n]))
                return f'<a href="{semantics_page}#{self.sem[n]}">{m.group(0)}</a>'
            # each number its own link, ranges to their start
            def num(mm):
                a = int(mm.group(1))
                b = mm.group(2)
                title = f"tex.web §{a}" + (f"–{b}" if b else "")
                return f'<a class="web" title="{title}" href="{self.web_url(a)}">{mm.group(0)}</a>'
            body = re.sub(r"(\d+)(?:\s*[–-]\s*(\d+))?", num, m.group(0)[len(tag):])
            return tag + body
        return SECTION.sub(one, text)

    def rust(self, path, a, b=None):
        """(url, shown) of `path:a-b`: path resolved (`value.rs` alone when it is unique), the lines checked."""
        full = path if path.startswith("src/") else None
        if not full:
            c = [f for f in self.files.get(path.rsplit("/", 1)[-1], []) if f.endswith(path)]
            if len(c) > 1:
                c = [f for f in c if "/engine/" in f] or c
            if len(c) != 1:
                raise Missing(f"Rust file {path}: {len(c)} candidates")
            full = c[0]
        if not (self.root / full).is_file():
            raise Missing(f"Rust file {full} does not exist")
        n = self.lengths.setdefault(full, len((self.root / full).read_text().split("\n")))
        if int(a) > n or (b and int(b) > n):
            raise Missing(f"{full}:{a} is past the end of the file ({n} lines)")
        frag = f"#L{a}" + (f"-L{b}" if b else "")
        return f"{self.repo}/blob/{self.commit}/{full}{frag}", full


# ---------------------------------------------------------------- code rendering

def render_comment(text, ctx, kind, refs, page):
    """A comment: escaped, `code` spans linked when they name a declaration, § numbers linked."""
    body = esc(text)

    def code(m):
        name = html.unescape(m.group(1))
        full = ctx.resolve_name(name)
        if full:
            return f'<code>{ctx.link(full, name)}</code>'
        return f"<code>{m.group(1)}</code>"
    body = re.sub(r"`([^`\n]+)`", code, body)
    body = refs.link_sections(body, ctx.semantics_page)
    return f'<span class="lc {kind}">{body}</span>'


class Ctx:
    """Rendering state of one page: the files' namespaces, the used declarations, the page of each file."""

    def __init__(self, env, files, refs, semantics_page="../SEMANTICS.html"):
        self.env, self.files, self.refs, self.semantics_page = env, files, refs, semantics_page
        self.prefix = ""               # from the page being written to the Lean pages
        self.home = {}                 # full name -> file stem that declares it
        for stem, src in files.items():
            for names in src.anchors.values():
                for n in names:
                    self.home[n] = stem
        self.ns, self.opens, self.here = [], [], None
        self.recent = []               # the types named so far in the declaration being read
        self.used = {}                 # names linked on this page, for the hover index

    def resolve_name(self, tok):
        full = self.env.resolve(tok, self.ns, self.opens, self.recent)
        return full if full in self.home else None

    def href(self, full):
        stem = self.home.get(full)
        if stem is None:
            raise Missing(f"no source anchor for {full}")
        page = f"{self.prefix}{stem.lower()}.html"
        self.refs.used.add((f"lean/{stem.lower()}.html", full))
        return f"{page if stem != self.here else ''}#{quote(full)}"

    def link(self, full, shown):
        self.used[full] = self.env.by[full]
        if self.env.by[full]["kind"] == "inductive":
            self.recent.append(full)
        d = self.env.by[full]
        return f'<a class="ld d-{d["kind"]}" href="{self.href(full)}" data-lean="{attr(full)}">{esc(shown)}</a>'


def render(stem, ctx, refs):
    """The highlighted, linked HTML of Lean file `stem`; `ctx.used` collects the names linked."""
    src = ctx.files[stem]
    ctx.here, ctx.ns, ctx.opens, ctx.prefix = stem, [], [], ""
    out, line = [], 0
    stack = []                       # (kind, name, opens-before)
    declared = {i: list(v) for i, v in src.anchors.items()}   # the declarations of a line not yet met in its text
    prev_kw = None
    for kind, tok in tokens(src.text):
        if kind in ("comment", "doc"):
            parts = tok.split("\n")
            html_parts = []
            for k, part in enumerate(parts):
                html_parts.append(render_comment(part, ctx, kind, refs, ctx.semantics_page) if part else "")
            out.append("\n".join(html_parts))
            line += len(parts) - 1
            continue
        if kind == "line":
            out.append(render_comment(tok, ctx, "line", refs, ctx.semantics_page))
            continue
        if kind == "other" and tok == "\n":
            line += 1
            out.append("\n")
            continue
        if kind == "str" or kind == "chr":
            out.append(f'<span class="ls">{esc(tok)}</span>')
        elif kind == "num":
            out.append(f'<span class="ln">{esc(tok)}</span>')
        elif kind == "id":
            word = tok
            own = next((n for n in declared.get(line, []) if n == word or n.endswith("." + word)), None)
            if own:
                declared[line].remove(own)
                ctx.used[own] = ctx.env.by[own]
                if ctx.env.by[own]["kind"] == "inductive":
                    ctx.recent.append(own)
                out.append(f'<span id="{attr(own)}" class="dn d-{ctx.env.by[own]["kind"]}" data-lean="{attr(own)}">{esc(word)}</span>')
                continue
            if word in KEYWORDS:
                if word in ("def", "theorem", "lemma", "inductive", "structure", "abbrev", "class", "opaque", "axiom", "instance"):
                    ctx.recent = []
                out.append(f'<span class="lk">{esc(word)}</span>')
                if word == "namespace":
                    prev_kw = "namespace"
                elif word == "open":
                    prev_kw = "open"
                elif word == "end":
                    prev_kw = "end"
                elif word in ("section",):
                    stack.append(("section", None, list(ctx.opens)))
                    prev_kw = None
                else:
                    prev_kw = None
                continue
            if prev_kw == "namespace":
                ctx.ns += word.split(".")
                stack.append(("namespace", word, list(ctx.opens)))
                prev_kw = None
                out.append(esc(word))
                continue
            if prev_kw == "end":
                if stack:
                    k, nm, old = stack.pop()
                    if k == "namespace":
                        del ctx.ns[-len(nm.split(".")):]
                    ctx.opens[:] = old
                prev_kw = None
                out.append(esc(word))
                continue
            if prev_kw == "open":
                full = None
                for pre in [".".join(ctx.ns[:i]) for i in range(len(ctx.ns), -1, -1)]:
                    f = f"{pre}.{word}" if pre else word
                    if any(n.startswith(f + ".") for n in ctx.env.by):
                        full = f
                        break
                if full:
                    ctx.opens.append(full)
                out.append(esc(word))
                continue
            full = ctx.resolve_name(word)
            if full:
                out.append(ctx.link(full, word))
                continue
            if "." in word:
                head, _, last = word.rpartition(".")
                f = ctx.env.field(last) if not word.startswith(".") else None
                if f and f in ctx.home:
                    out.append(esc(head + "."))
                    out.append(ctx.link(f, last))
                    continue
            out.append(esc(word))
        else:
            if tok.strip() and tok not in "()[]{},:⟨⟩":
                prev_kw = None
            out.append(esc(tok))
    return "".join(out)


def hover(d):
    """What the hover of a Lean name shows: kind, type, doc comment, file and line."""
    return {"k": d["kind"], "t": re.sub(r"\s+", " ", d["type"]).replace("Satex.", ""), "d": d["doc"].strip(), "f": d["module"].rsplit(".", 1)[-1], "l": d["line"]}


def lean_page(stem, ctx, refs, repo, commit):
    """The page's HTML body and the JSON index of the names it hovers."""
    code = render(stem, ctx, refs)
    n = len(ctx.files[stem].lines)
    index = {n_: hover(d) for n_, d in ctx.used.items()}
    prev = LEAN[LEAN.index(stem) - 1] if LEAN.index(stem) else None
    nxt = LEAN[LEAN.index(stem) + 1] if LEAN.index(stem) + 1 < len(LEAN) else None
    nav = " · ".join(filter(None, [f'<a href="{prev.lower()}.html">&larr; {prev}</a>' if prev else "", '<a href="../wiki/formal-semantics.html">The formal model</a>',
                                   f'<a href="{nxt.lower()}.html">{nxt} &rarr;</a>' if nxt else ""]))
    return (f'<h1>{stem}.lean</h1><p class="chapter-nav">{nav}</p>'
            f'<p class="lean-note">{n} lines. Every name is a link to its definition, hover shows its type and doc comment. '
            f'<a href="{repo}/blob/{commit}/semantics/lean/Satex/{stem}.lean">The file on GitHub</a>. '
            'A name that is a local variable, or a field of a value whose type is not named, stays plain.</p>'
            f'<pre class="lean"><code>{code}</code></pre>'
            f'<script type="application/json" id="lean-decls">{json.dumps(index, ensure_ascii=False).replace("</", "<\\/")}</script>')


# ---------------------------------------------------------------- mapping and primitives

def rustify(frag, refs):
    """Rust references in the text of `frag` (HTML) as links: `file.rs:a-b`. Text inside a link stays."""
    def one(m):
        url, full = refs.rust(m.group(1), m.group(2), m.group(3))
        return f'<a class="rs" href="{url}" title="{full}">{m.group(0)}</a>'
    out, depth = [], 0
    for piece in re.split(r"(<[^>]*>)", frag):
        if piece.startswith("<"):
            depth += 1 if re.match(r"<a\b", piece) else -1 if piece.startswith("</a") else 0
            out.append(piece)
        else:
            out.append(piece if depth else re.sub(r"(?<![\w/.-])((?:[\w-]+/)*[\w-]+\.rs):(\d+)(?:[-–](\d+))?", one, piece))
    return "".join(out)


def mapping(md_text, ctx, refs, runmd):
    """semantics/mapping.md as HTML: Lean names in the first column linked to their anchors, Rust references to GitHub."""
    body = runmd(md_text)

    def cell(m):
        cells = re.findall(r"<td[^>]*>.*?</td>", m.group(0), flags=re.S)
        if len(cells) < 2:
            return m.group(0)
        row = m.group(0)
        first = cells[0]
        last_base = [None]

        def name(mm):
            tok = html.unescape(mm.group(1))
            full = None
            if tok.startswith(".") and last_base[0]:
                full = ctx.env.resolve(f"{last_base[0]}{tok}", ["Satex", "Abstract"], []) or ctx.env.resolve(f"{last_base[0]}{tok}", ["Satex"], [])
            else:
                for ns in (["Satex", "Abstract"], ["Satex", "Concrete"], ["Satex"]):
                    full = ctx.env.resolve(tok, ns, [])
                    if full and full in ctx.home:
                        break
                    full = None
                if not full and last_base[0]:        # a sibling of the row's first name: `Itv.widen`, `wlo`
                    full = ctx.env.resolve(f"{last_base[0]}.{tok}", ["Satex", "Abstract"], [])
                elif not last_base[0]:
                    last_base[0] = tok.split(".")[0] if tok[0].isalpha() else None
            if not full or full not in ctx.home:
                if re.match(r"^[\w.γ'·]+$", tok) and not tok.startswith("."):
                    raise Missing(f"mapping.md: Lean name {tok} not found")
                return mm.group(0)
            return f'<code>{ctx.link(full, tok)}</code>'
        new_first = re.sub(r"<code>([^<]+)</code>", name, first)
        row = row.replace(first, new_first, 1)
        return row
    body = re.sub(r"<tr>.*?</tr>", cell, body, flags=re.S)
    return rustify(body, refs)


def primitives(tsv_text, ctx, refs, builtins):
    """semantics/primitives.tsv as a filterable table: rules linked to their Lean anchors, sources to tex.web, the Rust line of the name."""
    rows = []
    for line in tsv_text.split("\n"):
        if not line or line.startswith("#") or line.startswith("name\t"):
            continue
        name, rule, source, kind = (line.split("\t") + ["", "", ""])[:4]
        rows.append((name, rule, source, kind))
    out = [
           '<table class="prim wrap"><thead><tr><th>Primitive</th><th>Semantics</th><th>Rule</th><th>Rust</th><th>Source</th></tr></thead><tbody>']
    rule_ns = (["Satex", "Concrete"], ["Satex", "Abstract"], ["Satex"])
    ok = {"exact": "exact", "over": "over-approximated", "open": "no rule yet"}
    for name, rule, source, kind in rows:
        rules = []
        for r in filter(lambda x: x not in ("", "-"), (x.strip() for x in rule.split(","))):
            full = next((f for ns in rule_ns for f in [ctx.env.resolve(r, ns, [])] if f and f in ctx.home), None)
            if not full:
                raise Missing(f"primitives.tsv: rule {r} of {name!r} not found")
            rules.append(ctx.link(full, r))
        line = builtins.get(name.removeprefix("\\"))
        rs = ""
        if line:
            url, full = refs.rust("src/engine/builtins.rs", str(line))
            rs = f'<a class="rs" href="{url}">builtins.rs:{line}</a>'
        src = refs.link_sections(esc(source).replace("tex.web ", "tex.web "), ctx.semantics_page)
        out.append(f'<tr id="{prim_id(name)}" data-kind="{kind}"><td><code>{esc(cs_of(name))}</code></td>'
                   f'<td><span class="sem sem-{kind}">{ok.get(kind, kind)}</span></td><td>{" ".join(rules)}</td><td>{rs}</td><td>{src}</td></tr>')
    out.append("</tbody></table>")
    return "\n".join(out), rows


def cs_of(name):
    """The control sequence a primitives.tsv name stands for: `csname` is `\\csname`, `-` is `\\-`, `\\ ` is itself."""
    return name if name.startswith("\\") else "\\" + name


def prim_id(name):
    return "p-" + re.sub(r"[^A-Za-z0-9]", lambda m: f"_{ord(m.group()):02x}", name.removeprefix("\\") or name)


def builtin_lines(root):
    """Name -> first line of src/engine/builtins.rs that spells it as a string."""
    out = {}
    for i, line in enumerate((root / "src/engine/builtins.rs").read_text().split("\n"), 1):
        for s in re.findall(r'"([A-Za-z@]+|[^A-Za-z"\\])"', line):
            out.setdefault(s, i)
    return out


# ---------------------------------------------------------------- the page

def build(root, repo, commit, env_decls, semantics_ids):
    """Writes the four Lean pages; returns (formal body HTML with the placeholders filled, the primitive rows, ctx, refs)."""
    env = Env(env_decls)
    refs = Refs(root, repo, commit, semantics_ids)
    files = {s: Source(root / f"semantics/lean/Satex/{s}.lean", env) for s in LEAN}
    ctx = Ctx(env, files, refs)
    pages = {}
    for stem in LEAN:
        ctx.used = {}
        pages[stem] = lean_page(stem, ctx, refs, repo, commit)
    return env, refs, ctx, pages, files


def check_links(out, used):
    """Every (page, anchor) the generated pages link to must exist in the written site."""
    seen, bad = {}, []
    for page, anchor in sorted(used):
        f = out / page
        if page not in seen:
            seen[page] = set(re.findall(r'\bid="([^"]+)"', html.unescape(f.read_text()))) if f.is_file() else None
        if seen[page] is None or anchor not in seen[page]:
            bad.append(f"{page}#{anchor}")
    if bad:
        raise Missing(f"{len(bad)} link targets are missing, e.g. " + ", ".join(bad[:8]))


# ---------------------------------------------------------------- cards of control sequences

# A name ends in a letter or colon, else it is a fragment (`\\l_`, `\\KV@`); a one-character name is ASCII punctuation.
CS_TOKEN = re.compile(r"\\(?:[A-Za-z@:_]*[A-Za-z:]|[,;:!\-/_^\"'`=.~&%$#{}()\[\]|@\\])(?![A-Za-z@:_])")
ENV_USE = re.compile(r"\\(?:begin|end)\{([A-Za-z@*][\w@*-]*)\}")


def cs_keys(text):
    """The card keys a code span's text names, in order: its control sequences, `\\begin{env}` as one key. A span
    that does not start with a control sequence names none. Fragments like `\\begin{` key `\\begin`."""
    text = text.strip()
    if not CS_TOKEN.match(text):
        return []
    out, i = [], 0
    while (m := CS_TOKEN.search(text, i)):
        e = ENV_USE.match(text, m.start())
        out.append((m.start(), (e or m).end(), e.group(0) if e else m.group()))
        i = out[-1][1]
    return out


def mark_cards(body, extra=()):
    """`body` with every inline code span that names control sequences (or a name in `extra`, e.g. an arrow tip with a preview)
    marked `class="cs" data-cs="name"`, the name its first one. A span naming several marks each in a `span`.
    Returns the HTML and the names marked."""
    names, parts = set(), re.split(r"(<pre\b.*?</pre>)", body, flags=re.S)

    def one(m):
        text = html.unescape(m.group(1))
        ks = cs_keys(text)
        if not ks:
            if text.strip() not in extra:
                return m.group(0)
            names.add(text.strip())
            return f'<code class="cs" data-cs="{attr(text.strip())}">{m.group(1)}</code>'
        names.update(k for _, _, k in ks)
        inner, at = m.group(1), 0
        if len(ks) > 1:
            inner = ""
            for s, e, k in ks:
                inner += html.escape(text[at:s], quote=False) + f'<span data-cs="{attr(k)}">{html.escape(text[s:e], quote=False)}</span>'
                at = e
            inner += html.escape(text[at:], quote=False)
        return f'<code class="cs" data-cs="{attr(ks[0][2])}">{inner}</code>'
    return "".join(p if i % 2 else re.sub(r"<code>([^<]*)</code>", one, p) for i, p in enumerate(parts)), names


def cards(names, rows, ctx, refs, scope, engines, previews, builtins, defs=None):
    """name -> card: kind (primitive / kernel macro / package macro), engines, the semantics row with links to its
    Lean rule and Rust site, signature, documentation, a preview picture. `scope` is `satex scope --all`'s records by name,
    `engines` the primitives of each engine (label -> names), `rows` primitives.tsv's, `defs` example_defs'.
    Every name gets a card: an environment's from its start macro, an example's own from its definition, else a minimal one."""
    defs = defs or {}
    by_row = {cs_of(r[0]): r for r in rows}
    every = set.intersection(*engines.values()) if engines else set()
    out = {}
    for name in sorted(names):
        env = ENV_USE.fullmatch(name)
        card, rec, row = {}, scope.get("\\" + env.group(1) if env else name), by_row.get(name)
        pv = previews.get(name)
        if pv:
            card["pv"] = pv
        if env:
            card["k"] = "environment"
            if rec and (rec.get("provider") or rec.get("package")):
                card["pkg"] = rec.get("provider") or rec["package"]
            if rec and rec.get("file"):
                card["at"] = f'{rec["file"]}:{rec["line"]}'
            card["doc"] = (rec or {}).get("does") or no_doc(card)
        elif rec or row:
            tag = rec.get("tag") if rec else None
            # the provider defined it first; the package in force may only redefine it
            pkg = (rec.get("provider") or rec.get("package")) if rec else None
            again = rec.get("package") if rec else None
            whose = "class" if rec and str(rec.get("file", "")).endswith(".cls") and pkg == again else "package"
            if row or tag == "primitive":
                card["k"] = "primitive"
            elif pkg in ("kernel", "latex.ltx"):
                card["k"] = "kernel macro" if tag != "alias" else "kernel alias"
            else:
                card["k"] = f"{whose} macro" if tag != "alias" else f"{whose} alias"
            if pkg and card["k"] != "primitive":
                card["pkg"] = pkg
                if again and again not in (pkg, "document"):
                    card["re"] = again
            if card["k"] == "primitive":
                have = [e for e, ns in engines.items() if name in ns]
                if have:
                    card["e"] = "all engines" if name in every else ", ".join(have)
            if rec:
                # the usage line with its arguments named by the manual or the run, never numbered
                if rec.get("usage") and rec["usage"] != name:
                    card["sig"] = rec["usage"]
                if rec.get("file"):
                    card["at"] = f'{rec["file"]}:{rec["line"]}'
                if rec.get("does"):
                    card["doc"] = rec["does"]
                elif card["k"] != "primitive":
                    card["doc"] = no_doc(card)
        if row:
            _, rule, source, kind = row
            sem = {"kind": {"exact": "exact", "over": "over-approximated", "open": "no rule yet"}.get(kind, kind), "row": f"wiki/formal-semantics.html#{prim_id(row[0])}"}
            links = []
            for r in filter(lambda x: x not in ("", "-"), (x.strip() for x in rule.split(","))):
                full = next((f for ns in (["Satex", "Concrete"], ["Satex", "Abstract"], ["Satex"]) for f in [ctx.env.resolve(r, ns, [])] if f and f in ctx.home), None)
                if not full:
                    raise Missing(f"primitives.tsv: rule {r} of {name!r} not found")
                links.append([r, f"lean/{ctx.home[full].lower()}.html#{quote(full)}"])
                refs.used.add((f"lean/{ctx.home[full].lower()}.html", full))
            sem["rules"] = links
            line = builtins.get(row[0].removeprefix("\\"))
            if line:
                url, full = refs.rust("src/engine/builtins.rs", str(line))
                sem["rust"] = [f"builtins.rs:{line}", url]
            if source:
                sem["src"] = source
            card["sem"] = sem
            refs.used.add(("wiki/formal-semantics.html", prim_id(row[0])))
            if not card.get("doc"):
                card["doc"] = primitive_doc(rule, source, card.get("e"))
        if not card and name in defs:
            title, kind = defs[name]
            card.update(k=kind, at=f"the example on {title}")
        out[name] = card or {"k": "undefined", "doc": "No definition found: the probe document, which loads the pages' packages, "
                                                      "does not define it at its top level."}
    return out


def no_doc(card):
    """What a card says when no documentation source speaks for its name: where it is defined, never a guess."""
    return f'No documentation found. Defined at `{card["at"]}`.' if card.get("at") else "No documentation found."


# What a primitive is, by the semantics rule primitives.tsv gives it.
RULE_SAYS = {"intPar": "An integer parameter", "dimPar": "A dimension parameter", "gluePar": "A glue parameter",
             "specialTable": "A per-character table", "convertOther": "Expands to text computed from its operands",
             "ifTrue": "A conditional", "constant": "A constant", "unknowable": "A quantity known only while the engine runs",
             "lua": "Runs Lua code", "operands": "A typesetting or output command", "illegal": "An internal quantity"}


def primitive_doc(rule, source, engines):
    """A primitive's line from its semantics row: what its rule makes it and where it is documented."""
    said = next((RULE_SAYS[r.split(".")[-1]] for r in rule.split(",") if r.split(".")[-1] in RULE_SAYS), None)
    whose = f" of {engines}" if engines and engines != "all engines" else ""
    where = f", documented in the {source}" if source else ""
    return f"{said or 'A primitive'}{whose}{where}."


DEFINERS = [(r"(?:\\(?:gdef|edef|xdef|def)|\\(?:re)?newcommand\*?|\\(?:Provide|Declare)(?:Robust)?Command\*?|\\(?:New|Renew|Provide|Declare)(?:Expandable)?DocumentCommand)\s*\{?\s*(\\[A-Za-z@]+)", "macro"),
            (r"\\let\s*(\\[A-Za-z@]+)", "alias"),
            (r"\\new(count|dimen|skip|muskip|toks|box)\s*(\\[A-Za-z@]+)", None),
            (r"\\newif\s*\\if([A-Za-z@]+)", "conditional"),
            (r"\\newcounter\s*\{([A-Za-z@]+)\}", "counter"),
            (r"\\(?:new|renew)environment\*?\s*\{([A-Za-z@]+)\}", "environment"),
            (r"\\(?:cs|bool|int|dim|tl|seq|clist|prop|box)_(?:new|set|gset)[A-Za-z_]*:[A-Za-z]*\s*(\\[A-Za-z_:@]+)", "expl3 name")]


def example_defs(pages):
    """name -> (page title, kind) for the names the pages' code blocks define: `\\def`, `\\newcommand`, `\\newcount`, `\\newif`
    (its three macros), `\\newcounter` (`\\c@name`), environments (`\\name`, `\\endname`). `pages` are (dst, title, body)."""
    out = {}
    for _, title, body in pages:
        for pre in re.findall(r"<pre\b.*?</pre>", body, flags=re.S):
            text = html.unescape(re.sub(r"<[^>]*>", "", pre))
            for rx, kind in DEFINERS:
                for m in re.finditer(rx, text):
                    n = m.group(m.lastindex)
                    k = kind or {"count": "count register", "dimen": "dimen register", "skip": "skip register", "muskip": "muskip register", "toks": "token register", "box": "box register"}[m.group(1)]
                    names = {"conditional": [f"\\if{n}", f"\\{n}true", f"\\{n}false"], "counter": [f"\\c@{n}", f"\\the{n}"], "environment": [f"\\{n}", f"\\end{n}"]}.get(kind, [n])
                    for x in names:
                        out.setdefault(x, (title, k if kind != "conditional" else "conditional"))
    return out
