"""Compare KD11-B model results with simh results and classify differences.

Known, documented divergences (not CPU semantics a program can rely on):
  stacked-psw   only the PSW pushed by a bus-error trap differs: which
                condition codes an aborted instruction has already set is
                microcode-order dependent and not settled by DEC documents
                in hand
  regfile       the program touched 177700-177717; simh maps these to the
                general registers for an 11/05, this model treats them as
                non-existent (a bus error)
  iopage        the program touched another I/O page address where simh's
                11/05 configuration still has a device and a GT40 has none
Anything else is a failure.

Usage: compare.py MINE.txt SIMH.txt
"""
import sys


def load(p):
    cases, cur = {}, None
    for line in open(p):
        k, _, v = line.strip().partition(" ")
        if k == "CASE":
            cur = int(v)
            cases[cur] = {}
        elif cur is not None:
            cases[cur].setdefault(k, []).append(v)
    return cases


a, b = load(sys.argv[1]), load(sys.argv[2])
kinds = {}
for n in sorted(a):
    faults = [int(x, 8) for x in a[n].get("FAULTS", [""])[0].split()]
    mine = {k: v for k, v in a[n].items() if k != "FAULTS"}
    if mine == b[n]:
        kinds.setdefault("identical", []).append(n)
        continue
    if any(0o177700 <= f <= 0o177717 for f in faults):
        kinds.setdefault("regfile", []).append(n)
        continue
    if any(f >= 0o160000 for f in faults):
        kinds.setdefault("iopage", []).append(n)
        continue
    memd = []
    for ma, mb in zip(a[n]["MEM"], b[n]["MEM"]):
        wa, wb = ma.split(), mb.split()
        base = int(wa[0], 8)
        memd += [base + 2 * i for i, (x, y) in enumerate(zip(wa[1:], wb[1:])) if x != y]
    sp = int(a[n]["R"][0].split()[6], 8)
    regs_same = all(a[n][k] == b[n][k] for k in ("R", "PSW", "HALT"))
    if regs_same and memd == [sp + 2] and faults:
        kinds.setdefault("stacked-psw", []).append(n)
    else:
        kinds.setdefault("FAIL", []).append(n)
for k in ("identical", "stacked-psw", "regfile", "iopage", "FAIL"):
    v = kinds.get(k, [])
    print(f"{k:12s} {len(v):5d}  {v[:10] if k == 'FAIL' else ''}")
sys.exit(1 if kinds.get("FAIL") else 0)
