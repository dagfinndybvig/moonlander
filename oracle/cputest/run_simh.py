"""Run generated test cases in simh's PDP-11 configured as an 11/05.

Each case is written as an absolute loader image covering low memory (traps
and HALT handlers), the scratch area, the stack and the program, so nothing
from a previous case survives. simh loads it, runs from 004000 to a HALT, and
the final registers, PSW and memory are recorded in the same format the Rust
checker prints.

Usage: run_simh.py SIMH_BINARY CASES.txt OUT.txt
"""

import os
import re
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools"))
import lda  # noqa: E402


def parse_cases(path):
    cases, cur = [], None
    for line in open(path):
        p = line.split()
        if not p:
            continue
        if p[0] == "CASE":
            cur = {"n": int(p[1]), "mem": []}
        elif p[0] == "R":
            cur["r"] = [int(x, 8) for x in p[1:]]
        elif p[0] == "PSW":
            cur["psw"] = int(p[1], 8)
        elif p[0] == "MEM":
            cur["mem"].append((int(p[1], 8), [int(x, 8) for x in p[2:]]))
        elif p[0] == "VEC":
            cur["vec"] = [tuple(int(y, 8) for y in x.split(":")) for x in p[1:]]
        elif p[0] == "END":
            cases.append(cur)
    return cases


def image(case):
    mem = {}

    def put(a, w):
        mem[a] = w & 0xFF
        mem[a + 1] = (w >> 8) & 0xFF

    for a in range(0, 0o1000, 2):
        put(a, 0)
    for a in range(0o3000, 0o4000, 2):
        put(a, 0)
    for v, h in case["vec"]:
        put(v, h)
        put(v + 2, 0)
    for base, words in case["mem"]:
        for i, w in enumerate(words):
            put(base + 2 * i, w)
    return mem


def main():
    simh, cases_path, out_path = sys.argv[1:4]
    cases = parse_cases(cases_path)
    tmp = tempfile.mkdtemp(prefix="cputest-")
    # A GT40 without its display processor and host line: CPU, 16 KB, line
    # clock, console. Every other simh default peripheral is removed.
    script = ["set cpu 11/05", "set cpu 16k", "set vt disable", "set dli disable",
              "set rha disable", "set ptr disable", "set ptp disable", "set lpt disable",
              "set dz disable", "set rk disable", "set rl disable", "set hk disable",
              "set rx disable", "set rq disable", "set tm disable", "set tq disable",
              "set throttle 0"]
    for c in cases:
        f = os.path.join(tmp, f"c{c['n']}.lda")
        lda.write_lda(f, image(c), 0o4000)
        script.append(f"echo ===CASE {c['n']}")
        script.append("reset all")
        script.append("dep 0-37776 0")     # no memory survives from the previous case
        script.append(f"load {f}")
        for i, name in enumerate(["R0", "R1", "R2", "R3", "R4", "R5", "SP", "PC"]):
            script.append(f"dep {name} {c['r'][i]:o}")
        script.append(f"dep PSW {c['psw']:o}")
        script.append("go 4000")
        script.append("ex R0,R1,R2,R3,R4,R5,SP,PC,PSW")
        script.append("ex 2000-2776")
        script.append("ex 3000-3776")
    script.append("exit")
    ini = os.path.join(tmp, "run.ini")
    open(ini, "w").write("\n".join(script) + "\n")
    res = subprocess.run([simh, ini], capture_output=True, text=True, timeout=600)
    text = res.stdout
    out = []
    for block in text.split("===CASE ")[1:]:
        n = int(block.split()[0])
        regs = {}
        for m in re.finditer(r"^(R[0-5]|SP|PC|PSW):\s+([0-7]+)", block, re.M):
            regs[m.group(1)] = int(m.group(2), 8)
        memv = {}
        for m in re.finditer(r"^([0-7]{1,6}):\s+([0-7]{1,6})\s*$", block, re.M):
            memv[int(m.group(1), 8)] = int(m.group(2), 8)
        halt = re.search(r"HALT instruction, PC: ([0-7]+)", block)
        out.append(f"CASE {n}")
        out.append("R " + " ".join(f"{regs.get(k, 0):o}" for k in ["R0", "R1", "R2", "R3", "R4", "R5", "SP", "PC"]))
        out.append(f"PSW {regs.get('PSW', 0):o}")
        out.append(f"HALT {int(halt.group(1), 8):o}" if halt else "HALT ?")
        out.append(f"MEM 2000 " + " ".join(f"{memv.get(a, 0):o}" for a in range(0o2000, 0o3000, 2)))
        out.append(f"MEM 3000 " + " ".join(f"{memv.get(a, 0):o}" for a in range(0o3000, 0o4000, 2)))
    open(out_path, "w").write("\n".join(out) + "\n")
    print(f"{len(out) // 6} cases run in simh; stderr: {res.stderr.strip()[:200]}")


if __name__ == "__main__":
    main()
