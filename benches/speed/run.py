#!/usr/bin/env python3
"""Single-path speed table: satex against pdfTeX on the same inputs.
  benches/speed/run.py SATEX_BINARY [REPEATS]
ini_* run under `pdftex -ini` and satex with kernel none, the others under pdflatex
and satex's latex2e kernel (warm caches), minus their *_empty baseline. Times are
the least CPU seconds of REPEATS runs. ini_para.tex is generated: one paragraph, 20k lines."""
import json, os, subprocess, sys

here = os.path.dirname(os.path.abspath(__file__))
satex, reps = sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 3
out = os.path.join(here, "..", "..", "target", "speed-out")
os.makedirs(out, exist_ok=True)
with open(os.path.join(out, "ini_para.tex"), "w") as f:
    f.write("% a long paragraph of text: 20k lines of words (pdftex -ini)\n\\catcode`\\{=1 \\catcode`\\}=2\n")
    f.write("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.\n" * 20000 + "\\par\n\\end\n")
work = [("ini_loop", None), ("ini_csname", None), ("ini_para", None),
        ("expl3_step", "latex_empty"), ("pgfmath_parse", "pgfmath_empty")]

def best(cmd, n=None):
    times, stats = [], None
    for _ in range(n or reps):
        p = subprocess.Popen(cmd, cwd=out, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
        err = p.stderr.read()
        _, _, ru = os.wait4(p.pid, 0)
        times.append(ru.ru_utime + ru.ru_stime)
        for line in err.splitlines():
            if line.startswith("{"):
                stats = json.loads(line)
    return min(times), stats

def source(name):
    return os.path.join(out if name == "ini_para" else here, name + ".tex")

def tex(name):
    f = source(name)
    ini = name.startswith("ini_")
    return best(["pdftex", "-ini", "-interaction=batchmode", f] if ini else ["pdflatex", "-interaction=batchmode", f])[0]

def sat(name):
    f = source(name)
    extra = ["--set", "kernel=none"] if name.startswith("ini_") else []
    cmd = [satex, "--no-config", "--set", "cache.document=false", *extra, "-f", f, "lint", "--format", "json"]
    # --stats-json turns on per-line timings, which cost time: count with it, time without
    t, _ = best(cmd)
    s = best(cmd[:1] + ["--stats-json"] + cmd[1:], 1)[1]
    return t, (s or {}).get("steps"), (s or {}).get("peak_paths")

print(f"{'workload':15} {'pdftex s':>9} {'satex s':>9} {'ratio':>6} {'steps':>10} {'Msteps/s':>9} {'paths':>5}")
for name, base in work:
    pt, (st, steps, paths) = tex(name), sat(name)
    if base:
        pt -= tex(base)
        bt, bsteps, _ = sat(base)
        st -= bt
        steps = steps - bsteps if steps and bsteps else steps
    rate = steps / st / 1e6 if steps and st > 0 else 0
    print(f"{name:15} {pt:9.3f} {st:9.3f} {st / pt if pt > 0 else 0:6.1f} {steps or 0:10} {rate:9.2f} {paths or 0:5}")
