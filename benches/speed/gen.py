"""Generated benchmark documents, shared by run.py and scripts/bench_compile.py."""

# Deterministic text: word k of paragraph i.
WORDS = ("lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut "
         "labore et dolore magna aliqua enim ad minim veniam quis nostrud exercitation ullamco").split()

def words(i, n):
    return " ".join(WORDS[(i * 7 + k * 3 + k * k) % len(WORDS)] for k in range(n))

def mixed(pages, cls):
    """About one page per unit: three paragraphs, a section every second page, a chapter every
    tenth (book classes), and one of list, table, equation, cross-reference in turn."""
    book = cls != "article"
    # `thesis`: a report with what theses carry: figures drawn in TikZ and pgfplots every fifth page, booktabs, hyperref
    thesis = cls == "thesis"
    pre = "\\usepackage{tikz,pgfplots,booktabs,graphicx}\n\\pgfplotsset{compat=1.18}\n\\usepackage{hyperref}\n" if thesis else ""
    s = [f"\\documentclass{{{'report' if thesis else cls}}}\n\\usepackage{{amsmath,amssymb}}\n{pre}\\begin{{document}}\n"]
    for p in range(pages):
        if thesis and p % 5 == 4:
            s.append(f"\\begin{{figure}}[t]\\centering\n" + ("\\begin{tikzpicture}\\begin{axis}[width=8cm,height=5cm]\\addplot{x^2};\\addplot{%d*x};\\end{axis}\\end{tikzpicture}" % (p % 7 + 1) if p % 10 == 4 else
                     "\\begin{tikzpicture}\\draw[->] (0,0) -- (%d,0);\\foreach \\i in {1,...,%d} \\node[circle,draw] at (\\i,1) {\\i};\\end{tikzpicture}" % (p % 5 + 3, p % 5 + 2))
                     + f"\n\\caption{{{words(p, 6)}}}\\label{{f:{p}}}\n\\end{{figure}}\nFigure~\\ref{{f:{p}}} shows it.\n\n")
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

