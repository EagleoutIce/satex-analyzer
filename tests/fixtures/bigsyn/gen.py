#!/usr/bin/env python3
"""Synthetic big document: gen.py OUT --pages N [options].

A book of about N pages from the parts real theses have: \\include'd chapters,
sections, forward and backward \\ref/\\pageref/\\eqref, citations (biblatex,
natbib or plain bibtex), figure and table floats, tikz pictures, display math,
lists, footnotes, a glossary and an index. It needs three or more build runs
(aux, toc/lof/lot, bbl, glossary, index), like a latexmk build.

The text is seeded noise, so the same arguments give the same files.
Sizes used by the big-document benches: 40, 200 and 600 pages.
"""
import argparse
import os
import random

WORDS = ("analysis abstract domain lattice widening fixpoint token macro expansion group box glue "
         "penalty paragraph page float label reference counter register font kerning ligature "
         "hyphen category code catcode primitive engine format kernel package class option hook "
         "sound precise approximate interval join meet transfer state path branch loop summary "
         "the a of in on with for from by to and or that this which is are was were be can may "
         "result value input output number name file line column error warning note figure table "
         "equation section chapter theorem proof lemma example method approach system model").split()
TERMS = ["lattice", "widening", "fixpoint", "expansion", "catcode", "primitive", "kernel", "hook",
         "interval", "summary", "transfer", "register"]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("out")
    ap.add_argument("--pages", type=int, default=40)
    ap.add_argument("--chapter-pages", type=int, default=20)
    ap.add_argument("--section-pages", type=int, default=4)
    ap.add_argument("--cls", default="book")
    ap.add_argument("--bib", choices=["biblatex", "natbib", "bibtex", "none"], default="biblatex")
    ap.add_argument("--tikz-every", type=int, default=6, help="a tikz figure every N pages (0: none)")
    ap.add_argument("--no", default="", help="comma list of parts to leave out: glossary,index,hyperref,cleveref,lists,math,floats,refs")
    ap.add_argument("--seed", type=int, default=1)
    a = ap.parse_args()
    off = set(filter(None, a.no.split(",")))
    rnd = random.Random(a.seed)
    os.makedirs(os.path.join(a.out, "chapters"), exist_ok=True)

    def words(n):
        return " ".join(rnd.choice(WORDS) for _ in range(n))

    def sentence(n):
        s = words(n)
        return s[0].upper() + s[1:] + "."

    chapters = max(2, round(a.pages / a.chapter_pages))
    per_chapter = max(1, round(a.pages / chapters))
    sections = max(1, round(per_chapter / a.section_pages))
    units = max(1, round(per_chapter / sections))
    n_bib = 0 if a.bib == "none" else max(8, min(400, a.pages // 2))
    # Every label the document defines, so references can point forward and backward.
    labels = {"sec": [], "eq": [], "fig": [], "tab": [], "ch": []}
    plan = []
    for c in range(chapters):
        labels["ch"].append(f"ch:{c}")
        for s in range(sections):
            labels["sec"].append(f"sec:{c}:{s}")
            for u in range(units):
                k = (c * sections + s) * units + u
                kind = ("math", "list", "table", "figure", "align", "tikz" if a.tikz_every and k % a.tikz_every == 3 else "desc")[k % 6]
                if kind in ("math", "align") and "math" not in off:
                    labels["eq"].append(f"eq:{k}")
                if kind in ("figure", "tikz") and "floats" not in off:
                    labels["fig"].append(f"fig:{k}")
                if kind == "table" and "floats" not in off:
                    labels["tab"].append(f"tab:{k}")
                plan.append((c, s, u, k, kind))
    ref = "cref" if "cleveref" not in off and "hyperref" not in off else "ref"

    def refer():
        if "refs" in off:
            return ""
        kind = rnd.choice([k for k, v in labels.items() if v])
        key = rnd.choice(labels[kind])
        name = {"sec": "Section", "eq": "Equation", "fig": "Figure", "tab": "Table", "ch": "Chapter"}[kind]
        how = rnd.randrange(6)
        if how == 0:
            return f" (see page~\\pageref{{{key}}})"
        if kind == "eq" and how == 1:
            return f" by~\\eqref{{{key}}}"
        if ref == "cref" and how < 4:
            return f" (\\cref{{{key}}})"
        return f" ({name}~\\ref{{{key}}})"

    def cite():
        if not n_bib:
            return ""
        keys = ",".join(f"key{rnd.randrange(n_bib)}" for _ in range(rnd.choice([1, 1, 2])))
        cmd = {"biblatex": rnd.choice(["cite", "autocite", "textcite"]), "natbib": rnd.choice(["citep", "citet"]), "bibtex": "cite"}[a.bib]
        return f" \\{cmd}{{{keys}}}"

    def para():
        out = []
        for i in range(rnd.randrange(4, 7)):
            s = sentence(rnd.randrange(12, 22))
            if i == 1:
                s = s[:-1] + refer() + "."
            if i == 2:
                s = s[:-1] + cite() + "."
            if i == 3 and "glossary" not in off:
                s = s[:-1] + f" using the \\gls{{{rnd.choice(TERMS)}}}."
            if i == 3 and "index" not in off:
                s += f"\\index{{{rnd.choice(WORDS)}}}"
            if i == 4 and rnd.randrange(3) == 0:
                s = s[:-1] + f"\\footnote{{{sentence(8)}}}."
            if rnd.randrange(5) == 0:
                s += f" Let $x_{{{rnd.randrange(9)}}} \\in \\mathbb{{R}}$ and \\emph{{{words(2)}}}."
            out.append(s)
        return " ".join(out)

    def block(k, kind):
        if kind == "math" and "math" not in off:
            return ["\\begin{equation}\\label{eq:%d}" % k,
                    f"  f_{{{k}}}(x) = \\sum_{{i=0}}^{{n}} \\frac{{x^i}}{{i!}} + \\alpha_{{{k % 7}}} \\cdot \\sqrt{{x^2 + {k}}}",
                    "\\end{equation}"]
        if kind == "align" and "math" not in off:
            return ["\\begin{align}", f"  a_{{{k}}} &= b + {k} \\label{{eq:{k}}}\\\\", f"  c &\\le \\int_0^1 g_{{{k % 5}}}(t)\\,dt \\nonumber",
                    "\\end{align}"]
        if kind == "list" and "lists" not in off:
            return ["\\begin{enumerate}"] + [f"\\item {sentence(10)}" for _ in range(3)] + \
                   ["\\begin{itemize}"] + [f"\\item \\textbf{{{words(1)}}} {sentence(8)}" for _ in range(2)] + ["\\end{itemize}", "\\end{enumerate}"]
        if kind == "desc" and "lists" not in off:
            return ["\\begin{description}"] + [f"\\item[{words(1)}] {sentence(12)}" for _ in range(3)] + ["\\end{description}"]
        if kind == "table" and "floats" not in off:
            rows = [f"{words(2)} & {rnd.randrange(100)} & {rnd.randrange(1000) / 10} \\\\" for _ in range(5)]
            return ["\\begin{table}[tb]", "\\centering", "\\begin{tabular}{lrr}", "\\toprule", "Name & Count & Share \\\\", "\\midrule"] + rows + \
                   ["\\bottomrule", "\\end{tabular}", f"\\caption{{{sentence(7)}}}\\label{{tab:{k}}}", "\\end{table}"]
        if kind == "figure" and "floats" not in off:
            return ["\\begin{figure}[tb]", "\\centering", "\\rule{6cm}{3cm}", f"\\caption[{words(3)}]{{{sentence(9)}}}\\label{{fig:{k}}}", "\\end{figure}"]
        if kind == "tikz" and "floats" not in off:
            nodes = [f"  \\node[draw, rounded corners] (n{i}) at ({i * 2},{(i * k) % 3}) {{{words(1)}}};" for i in range(4)]
            edges = [f"  \\draw[->] (n{i}) -- (n{i + 1});" for i in range(3)]
            return ["\\begin{figure}[tb]", "\\centering", "\\begin{tikzpicture}"] + nodes + edges + \
                   ["\\end{tikzpicture}", f"\\caption{{{sentence(9)}}}\\label{{fig:{k}}}", "\\end{figure}"]
        return []

    files = {c: [f"\\chapter{{{words(3).title()}}}\\label{{ch:{c}}}", "", para(), ""] for c in range(chapters)}
    for c, s, u, k, kind in plan:
        out = files[c]
        if u == 0:
            out += [f"\\section{{{words(3).title()}}}\\label{{sec:{c}:{s}}}", ""]
        elif u % 2 == 1:
            out += [f"\\subsection{{{words(3).title()}}}", ""]
        out += [para(), "", para(), ""] + block(k, kind) + ["", para(), ""]
    for c, out in files.items():
        with open(os.path.join(a.out, "chapters", f"ch{c:02}.tex"), "w") as f:
            f.write("\n".join(out))

    pre = [f"\\documentclass[11pt,a4paper]{{{a.cls}}}", "\\usepackage[T1]{fontenc}", "\\usepackage{amsmath,amssymb}", "\\usepackage{graphicx,booktabs}"]
    if a.tikz_every and "floats" not in off:
        pre.append("\\usepackage{tikz}")
    if a.bib == "biblatex":
        pre += ["\\usepackage[backend=biber,style=numeric]{biblatex}", "\\addbibresource{refs.bib}"]
    if a.bib == "natbib":
        pre.append("\\usepackage[numbers]{natbib}")
    if "index" not in off:
        pre += ["\\usepackage{makeidx}", "\\makeindex"]
    if "hyperref" not in off:
        pre.append("\\usepackage[hidelinks]{hyperref}")
    if "glossary" not in off:
        pre += ["\\usepackage{glossaries}", "\\makeglossaries"]
        pre += [f"\\newglossaryentry{{{t}}}{{name={{{t}}}, description={{{sentence(9)}}}}}" for t in TERMS]
    if ref == "cref":
        pre.append("\\usepackage{cleveref}")
    body = ["\\begin{document}", "\\frontmatter", "\\tableofcontents", "\\listoffigures", "\\listoftables", "\\mainmatter"]
    if a.cls not in ("book", "scrbook", "memoir"):
        body = ["\\begin{document}", "\\tableofcontents", "\\listoffigures", "\\listoftables"]
    body += [f"\\include{{chapters/ch{c:02}}}" for c in range(chapters)]
    if "glossary" not in off:
        body.append("\\printglossaries")
    if a.bib == "biblatex":
        body.append("\\printbibliography")
    elif n_bib:
        body += ["\\bibliographystyle{%s}" % ("plainnat" if a.bib == "natbib" else "plain"), "\\bibliography{refs}"]
    if "index" not in off:
        body.append("\\printindex")
    body += ["\\end{document}", ""]
    with open(os.path.join(a.out, "main.tex"), "w") as f:
        f.write("\n".join(pre + body))
    if n_bib:
        with open(os.path.join(a.out, "refs.bib"), "w") as f:
            for i in range(n_bib):
                kind = ("article", "inproceedings", "book")[i % 3]
                venue = {"article": "journal", "inproceedings": "booktitle", "book": "publisher"}[kind]
                f.write(f"@{kind}{{key{i},\n  author = {{Author{i % 17}, A. and Writer{i % 5}, B.}},\n  title = {{{words(6).title()}}},\n"
                        f"  {venue} = {{{words(3).title()}}},\n  year = {{{1990 + i % 35}}},\n}}\n")
    print(f"{a.out}: {chapters} chapters x {sections} sections x {units} pages, {len(plan)} blocks, {n_bib} bib entries")


if __name__ == "__main__":
    main()
