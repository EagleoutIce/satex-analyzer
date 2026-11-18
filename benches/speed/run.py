#!/usr/bin/env python3
"""Speed tables: satex against pdfTeX on the same inputs.
  benches/speed/run.py SATEX_BINARY [REPEATS] [--heavy] [--only NAME,...]
Single-path table: ini_* run under `pdftex -ini` and satex with kernel none, the others under
pdflatex and satex's latex2e kernel (warm caches), minus their *_empty baseline. Times are
the least CPU seconds of REPEATS runs. ini_para.tex is generated: one paragraph, 20k lines.
--heavy: whole documents, one run each (pdflatex one pass): generated article/book/scrbook of
50/200/1000 pages, a TikZ-heavy and an expl3-heavy one, and the largest site examples.
Columns: wall and CPU seconds, steps/s, peak resident memory. SPEED_TIMEOUT (600 s) caps a run."""
import json, os, shutil, subprocess, sys, time

here = os.path.dirname(os.path.abspath(__file__))
args = [a for a in sys.argv[1:] if not a.startswith("--")]
satex, reps = args[0], int(args[1]) if len(args) > 1 else 3
heavy = "--heavy" in sys.argv
only = next((sys.argv[i + 1].split(",") for i, a in enumerate(sys.argv) if a == "--only"), None)
timeout = float(os.environ.get("SPEED_TIMEOUT", 600))
out = os.path.join(here, "..", "..", "target", "speed-out")
examples = os.path.join(here, "..", "..", "doc", "site", "examples")
# pdflatex writes its .aux apart: satex must not read files LaTeX wrote
texout = os.path.join(out, "pdflatex")
os.makedirs(texout, exist_ok=True)

def write(name, text):
    with open(os.path.join(out, name + ".tex"), "w") as f:
        f.write(text)

write("ini_para", "% a long paragraph of text: 20k lines of words (pdftex -ini)\n\\catcode`\\{=1 \\catcode`\\}=2\n"
      + "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.\n" * 20000 + "\\par\n\\end\n")

# Deterministic text: word k of paragraph i.
WORDS = ("lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut "
         "labore et dolore magna aliqua enim ad minim veniam quis nostrud exercitation ullamco").split()

def words(i, n):
    return " ".join(WORDS[(i * 7 + k * 3 + k * k) % len(WORDS)] for k in range(n))

def mixed(pages, cls):
    """About one page per unit: three paragraphs, a section every second page, a chapter every
    tenth (book classes), and one of list, table, equation, cross-reference in turn."""
    book = cls != "article"
    s = [f"\\documentclass{{{cls}}}\n\\usepackage{{amsmath,amssymb}}\n\\begin{{document}}\n"]
    for p in range(pages):
        if book and p % 10 == 0:
            s.append(f"\\chapter{{Chapter {p // 10}}}\\label{{c:{p}}}\n")
        if p % 2 == 0:
            s.append(f"\\section{{{words(p, 3)}}}\\label{{s:{p}}}\n")
        for i in range(3):
            s.append(words(p * 3 + i, 110) + f" \\cite{{b{(p + i) % 40}}}.\n\n")
        s.append([
            "\\begin{itemize}\n" + "".join(f"\\item {words(p + k, 12)}\n" for k in range(4)) + "\\end{itemize}\n",
            "\\begin{tabular}{lrr}\\hline\n" + "".join(f"{words(p + k, 2)} & {p * k} & {k}.5\\\\\n" for k in range(5))
            + "\\hline\\end{tabular}\n\n",
            f"\\begin{{equation}}\\label{{e:{p}}}\n\\sum_{{k=1}}^{{{p + 2}}} \\frac{{x_k^2}}{{k+1}} = \\int_0^1 f(t)\\,dt\n\\end{{equation}}\n",
            f"See Section~\\ref{{s:{p - p % 2}}} and page~\\pageref{{s:{p - p % 2}}}.\n\n",
        ][p % 4])
    s.append("\\begin{thebibliography}{99}\n" + "".join(f"\\bibitem{{b{k}}} {words(k, 8)}.\n" for k in range(40)))
    return "".join(s) + "\\end{thebibliography}\n\\end{document}\n"

def tikz(pictures):
    s = ["\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{arrows.meta,positioning}\n\\begin{document}\n"]
    for p in range(pictures):
        s.append(f"\\begin{{tikzpicture}}[node distance=1cm]\n\\node[draw] (a) {{{words(p, 1)}}};\n"
                 f"\\node[draw, right=of a] (b) {{{p}}};\n\\node[circle, draw, below=of a] (c) {{$x$}};\n"
                 "\\draw[-Stealth] (a) -- (b); \\draw[-Stealth] (b) to[bend left] (c);\n"
                 f"\\foreach \\x in {{0,...,{4 + p % 4}}} {{ \\draw (\\x*0.3, -2) -- ++(0.2, {p % 3 + 1}*0.1); }}\n"
                 f"\\fill[blue!{10 + p % 80}] (3,0) circle ({1 + p % 5}pt);\n\\end{{tikzpicture}}\n\n")
    return "".join(s) + "\\end{document}\n"

def expl3(calls):
    s = ["\\documentclass{article}\n\\ExplSyntaxOn\n\\seq_new:N \\l_b_seq \\prop_new:N \\l_b_prop \\int_new:N \\l_b_int\n"
         "\\NewDocumentCommand \\bench { m O{3} } {\n"
         "  \\seq_set_split:Nnn \\l_b_seq { , } {#1}\n"
         "  \\seq_map_inline:Nn \\l_b_seq { \\prop_put:Nnn \\l_b_prop {##1} {#2} \\int_add:Nn \\l_b_int {#2} }\n"
         "  \\str_case:nnF {#1} { {a,b} {A} {c} {C} } { \\tl_count:n {#1} }\n"
         "  \\int_step_inline:nn {#2} { \\int_incr:N \\l_b_int }\n"
         "  \\prop_get:NnN \\l_b_prop {x} \\l_tmpa_tl \\fp_eval:n { #2 * 1.5 + \\int_use:N \\l_b_int / 7 }\n"
         "}\n\\ExplSyntaxOff\n\\begin{document}\n"]
    s += [f"\\bench{{{','.join(WORDS[(c + k) % len(WORDS)] for k in range(1 + c % 5))},x}}[{c % 7 + 1}]\n" for c in range(calls)]
    return "".join(s) + "\\end{document}\n"

generated = {f"{cls}_{n}p": (mixed, n, cls) for cls in ("article", "book", "scrbook") for n in (50, 200, 1000)}
generated |= {"tikz_200": (tikz, 200), "expl3_2000": (expl3, 2000)}

def run(cmd, n=None):
    """Least CPU seconds of n runs as (cpu, wall, peak RSS MB), and the last JSON line on stderr.
    (None, None) on timeout."""
    best_run, stats, err = None, None, os.path.join(out, "stderr.txt")
    for _ in range(n or reps):
        with open(err, "w") as e:
            t0 = time.monotonic()
            p = subprocess.Popen(cmd, cwd=out, stdout=subprocess.DEVNULL, stderr=e)
            while (w := os.wait4(p.pid, os.WNOHANG))[0] == 0:
                if time.monotonic() - t0 > timeout:
                    p.kill()
                    os.wait4(p.pid, 0)
                    return None, None
                time.sleep(0.01)
            wall, ru = time.monotonic() - t0, w[2]
        p.returncode = 0  # reaped by wait4 above
        best_run = min(best_run or (9e9,), (ru.ru_utime + ru.ru_stime, wall, ru.ru_maxrss / 1024))
        stats = next((json.loads(l) for l in open(err) if l.startswith("{")), stats)
    return best_run, stats

def best(cmd, n=None):
    r, stats = run(cmd, n)
    return (r[0] if r else None), stats

def source(name):
    if name in generated or name == "ini_para":
        return os.path.join(out, name + ".tex")
    if name.startswith("ex_"):
        return os.path.join(examples, name[3:] + ".tex")
    return os.path.join(here, name + ".tex")

def tex_cmd(name):
    ini = name.startswith("ini_")
    return ["pdftex", "-ini", "-interaction=batchmode", source(name)] if ini else \
        ["pdflatex", "-interaction=batchmode", "-output-directory", texout, source(name)]

def sat_cmd(name, stats=False):
    extra = ["--set", "kernel=none"] if name.startswith("ini_") else []
    return [satex, "--no-config", "--set", "cache.document=false", *extra, *(["--stats-json"] if stats else []),
            "-f", source(name), "lint", "--format", "json"]

def tex(name):
    return best(tex_cmd(name))[0]

def sat(name):
    # --stats-json turns on per-line timings, which cost time: count with it, time without
    t, _ = best(sat_cmd(name))
    s = best(sat_cmd(name, True), 1)[1]
    return t, (s or {}).get("steps"), (s or {}).get("peak_paths")

def single():
    work = [("ini_loop", None), ("ini_csname", None), ("ini_para", None),
            ("expl3_step", "latex_empty"), ("pgfmath_parse", "pgfmath_empty")]
    print(f"{'workload':15} {'pdftex s':>9} {'satex s':>9} {'ratio':>6} {'steps':>10} {'Msteps/s':>9} {'paths':>5}")
    for name, base in work:
        if only and name not in only:
            continue
        pt, (st, steps, paths) = tex(name), sat(name)
        if base:
            pt -= tex(base)
            bt, bsteps, _ = sat(base)
            st -= bt
            steps = steps - bsteps if steps and bsteps else steps
        rate = steps / st / 1e6 if steps and st > 0 else 0
        print(f"{name:15} {pt:9.3f} {st:9.3f} {st / pt if pt > 0 else 0:6.1f} {steps or 0:10} {rate:9.2f} {paths or 0:5}")

def big():
    """One run each: whole documents, pdflatex one pass."""
    names = list(generated) + ["ex_" + n for n in ("thesis", "chapter", "drawing", "slides", "proofs")]
    print(f"{'workload':14} {'KB':>6} {'pdflatex':>8} {'wall s':>8} {'cpu s':>8} {'ratio':>6} {'Msteps':>8} "
          f"{'Msteps/s':>8} {'peak MB':>8} {'paths':>5}")
    for name in names:
        if only and name not in only:
            continue
        if name in generated:
            gen, *a = generated[name]
            write(name, gen(*a))
        if name == "ex_thesis":
            shutil.copy(os.path.join(examples, "refs.bib"), out)
        kb = os.path.getsize(source(name)) / 1024
        pt = (run(tex_cmd(name), 1)[0] or (None,))[0]
        # the counting run first: it also warms the package caches for the timed run
        counted, s = run(sat_cmd(name, True), 1)
        r = counted and run(sat_cmd(name), 1)[0]
        s = s or {}
        if r is None:
            print(f"{name:14} {kb:6.0f} {pt or 0:8.2f}  timeout after {timeout:.0f} s")
            continue
        steps, (cpu, wall, rss) = s.get("steps") or 0, r
        ratio = cpu / pt if pt else 0
        print(f"{name:14} {kb:6.0f} {pt or 0:8.2f} {wall:8.2f} {cpu:8.2f} {ratio:6.1f} {steps / 1e6:8.2f} "
              f"{steps / cpu / 1e6 if cpu else 0:8.2f} {rss:8.0f} {s.get('peak_paths') or 0:5}", flush=True)

big() if heavy else single()
