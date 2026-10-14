"""Find the first instruction at which the KD11-B model and simh disagree.

Runs one generated case in simh one instruction at a time, recording R0-R7
and PSW after each step, and in the model with TRACE set, then reports the
first step where they differ, with simh's disassembly of that instruction.

Usage: lockstep.py SIMH_BINARY CASES.txt CASE_NUMBER [STEPS]
"""
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "tools"))
import lda  # noqa: E402
import run_simh  # noqa: E402

ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
REGS = ["R0", "R1", "R2", "R3", "R4", "R5", "SP", "PC"]


def simh_trace(simh, case, steps):
    tmp = tempfile.mkdtemp(prefix="lockstep-")
    f = os.path.join(tmp, "c.lda")
    lda.write_lda(f, run_simh.image(case), 0o4000)
    s = ["set cpu 11/05", "set cpu 16k", "set vt disable", "set dli disable", "set rha disable",
         "set ptr disable", "set ptp disable", "set lpt disable", "set dz disable", "set rk disable",
         "set rl disable", "set hk disable", "set rx disable", "set rq disable", "set tm disable",
         "set tq disable", "dep 0-37776 0", f"load {f}"]
    for i, name in enumerate(REGS):
        s.append(f"dep {name} {case['r'][i]:o}")
    s.append(f"dep PSW {case['psw']:o}")
    s.append("dep PC 4000")
    for _ in range(steps):
        s += ["step", "ex " + ",".join(REGS + ["PSW"]), "echo ---"]
    s.append("exit")
    ini = os.path.join(tmp, "s.ini")
    open(ini, "w").write("\n".join(s) + "\n")
    out = subprocess.run([simh, ini], capture_output=True, text=True, timeout=120).stdout
    states, disasm = [], []
    for block in out.split("---")[:-1]:
        regs = {m.group(1): int(m.group(2), 8) for m in re.finditer(r"^(R[0-5]|SP|PC|PSW):\s+([0-7]+)", block, re.M)}
        if len(regs) < 9:
            break
        states.append(tuple(regs[k] for k in REGS) + (regs["PSW"] & 0o377,))
        m = re.search(r"(?:Step expired|HALT instruction), PC: [0-7]+ \((.*?)\)", block)
        disasm.append(m.group(1) if m else "?")
    return states, disasm


def model_trace(cases_path, n):
    exe = os.path.join(ROOT, "target", "release", "examples", "cpucheck")
    env = dict(os.environ, TRACE=str(n))
    err = subprocess.run([exe, cases_path], capture_output=True, text=True, env=env).stderr
    states, info = [], []
    for line in err.splitlines():
        m = re.match(r"([0-7]{6}) ([0-7]{6})  psw ([0-7]+) -> ([0-7]+)  regs \[(.*)\]", line)
        if m:
            regs = tuple(int(x.strip().strip('"'), 8) for x in m.group(5).split(","))
            states.append(regs + (int(m.group(4), 8) & 0o377,))
            info.append(f"at {m.group(1)} word {m.group(2)}")
    return states, info


def main():
    simh, cases_path, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
    steps = int(sys.argv[4]) if len(sys.argv) > 4 else 40
    case = [c for c in run_simh.parse_cases(cases_path) if c["n"] == n][0]
    s_states, s_dis = simh_trace(simh, case, steps)
    m_states, m_info = model_trace(cases_path, n)
    # simh services a trap at the start of the following step, so its trace
    # can lag the model's by one state after a trap; re-align when it does.
    i = j = 0
    names = REGS + ["PSW"]
    while i < len(m_states) and j < len(s_states):
        x, y = m_states[i], s_states[j]
        if x == y:
            i += 1
            j += 1
            continue
        if j + 1 < len(s_states) and s_states[j + 1] == x:
            j += 1
            continue
        diff = [f"{names[k]} model {x[k]:o} simh {y[k]:o}" for k in range(9) if x[k] != y[k]]
        print(f"case {n}: first divergence at model step {i + 1}: {m_info[i]}")
        print(f"  simh executed: {s_dis[j - 1] if j else '?'} ; simh next: {s_dis[j]}")
        print("  " + "; ".join(diff))
        return
    print(f"case {n}: no divergence in {min(len(m_states), len(s_states))} steps")


if __name__ == "__main__":
    main()
