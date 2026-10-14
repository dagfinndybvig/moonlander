"""Align the 1973 Moonlander build against the later, source-less build.

Every byte the 1973 listing emits is turned into a token:
  F  fixed value: opcodes, constants, text, table data. Must match exactly.
  A  absolute reference to a program location (a word the linker relocated).
     In the later build it must equal the new address of the same location.
  R  PC-relative reference to a program location (mode 67/77 operand).
     The new target, computed from the new word and its new address, must be
     the new address of the same location.
  X  PC-relative reference to a fixed absolute address (a vector or device
     register). The target must be identical in both builds.

The program is cut into blocks at its global labels. Each block is searched
for in the later image using its F tokens only. Unique matches become anchors;
ambiguous ones are settled by adjacency to their neighbours. Only then are the
A, R and X tokens checked against the resulting map, so every reference is an
independent test of the alignment rather than an input to it.

Writes oracle/align/report.md and oracle/align/map.json.
"""

import json
import os
import sys
from collections import defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import listing  # noqa: E402
import lda  # noqa: E402
import pdp11  # noqa: E402

ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
OLD_LST = os.path.join(ROOT, "oracle/build/gtlem-1973.lst")
OLD_BIN = os.path.join(ROOT, "oracle/build/gtlem-1973.bin")
NEW_LDA = os.path.join(ROOT, "reference/original/lunar-later-nosource.lda")
OUT = os.path.join(ROOT, "oracle/align")

KNOWN = (set(pdp11.DOUBLE.values()) | set(pdp11.SINGLE.values()) | set(pdp11.BRANCH.values())
         | {m + "B" for m in pdp11.SINGLE.values()} | set(pdp11.ZERO_OP.values())
         | {"JMP", "JSR", "RTS", "EMT", "TRAP", "NOP", "CLC", "CLV", "CLZ", "CLN",
            "SEC", "SEV", "SEZ", "SEN", "CCC", "SCC", "BHIS", "BLO"})


def ama_opcode(ins):
    """The opcode word as MACRO-11 emits it under .ENABL AMA: every operand
    field holding mode 67 (PC relative) becomes mode 37 (absolute)."""
    w = ins.words[0]
    fields = []
    if ins.mnemonic in pdp11.DOUBLE.values():
        fields = [6, 0]
    elif ins.mnemonic not in pdp11.BRANCH.values() and ins.mnemonic not in (
            "RTS", "EMT", "TRAP") and ins.length >= 1 and ins.operands:
        fields = [0]
    for sh in fields:
        if (w >> sh) & 0o77 == 0o67:
            w = (w & ~(0o77 << sh)) | (0o37 << sh)
    return w


def old_tokens():
    img = open(OLD_BIN, "rb").read()

    def rw(a):
        return img[a] | (img[a + 1] << 8)

    lines, labels, consts = listing.parse(OLD_LST, base=0o1000)
    toks = {}          # old byte address -> (kind, value, size)
    origin = {}        # old byte address -> listing line number
    for ln in lines:
        if ln.addr is None or not ln.items:
            continue
        st = ln.statement
        op = st.split()[0] if st else ""
        ins = None
        if op in KNOWN:
            ins = pdp11.decode(rw, ln.addr)
            if ins.length * 2 != ln.nbytes:
                ins = None      # self-modifying idiom: treat as data words
        # Values come from the linked image. The listing shows relocatable
        # words relative to the program section, so it is used only for the
        # relocation flag and the word/byte layout.
        a = ln.addr
        if ins:
            kinds = {o.word: o for o in ins.operands if o.word}
            for k, (_, n, reloc) in enumerate(ln.items):
                waddr = ln.addr + 2 * k
                v = rw(waddr)
                o = kinds.get(k)
                if o is None:
                    toks[waddr] = ("F", ama_opcode(ins) if k == 0 else v, 2)
                elif o.kind == "rel":
                    # mode 67 under .ENABL AMA becomes mode 37: the word holds
                    # the absolute target instead of a displacement
                    tgt = (waddr + 2 + v) & 0xFFFF
                    toks[waddr] = ("F", tgt, 2) if reloc else ("A", tgt, 2)
                elif o.kind == "relind":
                    tgt = (waddr + 2 + v) & 0xFFFF
                    toks[waddr] = ("X", tgt, 2) if reloc else ("R", tgt, 2)
                else:
                    toks[waddr] = ("A", v, 2) if reloc else ("F", v, 2)
                origin[waddr] = ln.lineno
        elif st.upper().startswith(".EVEN") or st.upper().startswith(".=.-1"):
            for _, n, reloc in ln.items:      # alignment padding: content is not meaningful
                toks[a] = ("P", 0, n)
                origin[a] = ln.lineno
                a += n
        else:
            for _, n, reloc in ln.items:
                v = rw(a) if n == 2 else img[a]
                toks[a] = ("A", v, 2) if reloc else ("F", v, n)
                origin[a] = ln.lineno
                a += n
    return toks, origin, lines, labels


def blocks_from(labels, toks):
    """Cut the token stream at global labels into (name, start, end) blocks."""
    starts = sorted({addr: name for name, addr in labels.items()
                     if addr is not None and ":" not in name}.items())
    addrs = sorted(toks)
    blocks = []
    for i, (addr, name) in enumerate(starts):
        end = starts[i + 1][0] if i + 1 < len(starts) else addrs[-1] + 2
        body = [a for a in addrs if addr <= a < end]
        if body:
            blocks.append((name, addr, end, body))
    return blocks


def main():
    os.makedirs(OUT, exist_ok=True)
    toks, origin, lines, labels = old_tokens()
    newmem, _, newstart = lda.read_lda(NEW_LDA)

    def nb(a):
        return newmem.get(a)

    def nw(a):
        lo, hi = newmem.get(a), newmem.get(a + 1)
        return None if lo is None or hi is None else lo | (hi << 8)

    blocks = blocks_from(labels, toks)
    newlo, newhi = min(newmem), max(newmem)

    def fits(body, base, at):
        for a in body:
            kind, v, n = toks[a]
            p = at + (a - base)
            if kind == "P":
                continue
            if n == 1:
                if nb(p) is None or (kind == "F" and nb(p) != v):
                    return False
            else:
                w = nw(p)
                if w is None:
                    return False
                if kind == "F" and w != v:
                    return False
                if kind == "X" and ((p + 2 + w) & 0xFFFF) != v:
                    return False
        return True

    cands = {}
    for name, start, end, body in blocks:
        fixed = sum(1 for a in body if toks[a][0] == "F")
        found = []
        if fixed:
            for at in range(newlo - (newlo % 2) + (start % 2), newhi + 1, 2):
                if fits(body, start, at):
                    found.append(at)
        cands[name] = found

    block_of = {}
    for name, start, end, body in blocks:
        for a in range(start, end):
            block_of[a] = name
    index = {b[0]: i for i, b in enumerate(blocks)}
    placed, how = {}, {}

    def mapped(old):
        name = block_of.get(old)
        if name is None or name not in placed:
            return None
        return old + placed[name]

    def new_target(kind, p):
        w = nw(p)
        if w is None:
            return None
        return w if kind == "A" else (p + 2 + w) & 0xFFFF

    # 1. blocks whose fixed content occurs exactly once
    for name, start, end, body in blocks:
        if len(cands[name]) == 1:
            placed[name] = cands[name][0] - start
            how[name] = "unique"
    while True:
        progress = False
        # 2. ambiguous blocks that sit flush against a placed neighbour
        for i, (name, start, end, body) in enumerate(blocks):
            if name in placed or not cands[name]:
                continue
            for j in (i - 1, i + 1):
                if 0 <= j < len(blocks) and blocks[j][0] in placed:
                    d = placed[blocks[j][0]]
                    if start + d in cands[name]:
                        placed[name] = d
                        how[name] = "adjacent"
                        progress = True
                        break
        # 3. blocks located by the references that point into them
        votes = defaultdict(lambda: defaultdict(int))
        for name, start, end, body in blocks:
            if name not in placed:
                continue
            d = placed[name]
            for a in body:
                kind, v, n = toks[a]
                if kind in ("A", "R"):
                    t = new_target(kind, a + d)
                    tb = block_of.get(v)
                    if t is not None and tb and tb not in placed:
                        votes[tb][t - v] += 1
        for tb, vs in votes.items():
            name, start, end, body = blocks[index[tb]]
            for d, cnt in sorted(vs.items(), key=lambda kv: -kv[1]):
                if fits(body, start, start + d):
                    placed[tb] = d
                    how[tb] = f"referenced x{cnt}"
                    progress = True
                    break
        if not progress:
            break

    # verify every reference token against the map
    stats = defaultdict(int)
    bad_refs = []
    for name, start, end, body in blocks:
        if name not in placed:
            continue
        d = placed[name]
        for a in body:
            kind, v, n = toks[a]
            if kind not in ("A", "R"):
                continue
            p = a + d
            w = nw(p)
            new_target = w if kind == "A" else (p + 2 + w) & 0xFFFF
            expect = mapped(v)
            if expect is None:
                stats["unverifiable (target block not placed)"] += 1
            elif expect == new_target:
                stats["consistent"] += 1
            else:
                stats["INCONSISTENT"] += 1
                bad_refs.append((name, a, p, kind, v, expect, new_target, origin.get(a)))

    covered = set()
    for name, start, end, body in blocks:
        if name in placed:
            for a in body:
                n = toks[a][2]
                for k in range(n):
                    covered.add(a + placed[name] + k)
    uncovered = sorted(a for a in newmem if a not in covered)
    gaps = []
    for a in uncovered:
        if gaps and gaps[-1][1] == a:
            gaps[-1][1] = a + 1
        else:
            gaps.append([a, a + 1])

    # report
    total_old = sum(sum(toks[a][2] for a in b[3]) for b in blocks)
    placed_old = sum(sum(toks[a][2] for a in b[3]) for b in blocks if b[0] in placed)
    out = ["# Alignment of the 1973 build against the later build", ""]
    out.append(f"- 1973 blocks: {len(blocks)}, placed: {len(placed)}")
    out.append(f"- 1973 bytes placed: {placed_old} of {total_old} ({100 * placed_old / total_old:.1f}%)")
    out.append(f"- later-build bytes explained: {len(covered & set(newmem))} of {len(newmem)}")
    out.append("- reference checks: " + ", ".join(f"{k} {v}" for k, v in sorted(stats.items())))
    out.append("")
    out.append("## Blocks")
    out.append("")
    out.append("| label | 1973 | later | shift | placed by |")
    out.append("|---|---|---|---|---|")
    for name, start, end, body in blocks:
        if name in placed:
            d = placed[name]
            out.append(f"| {name} | {start:06o}-{end - 1:06o} | {start + d:06o} | {d:+o} | {how[name]} |")
        else:
            out.append(f"| {name} | {start:06o}-{end - 1:06o} | **unplaced** | | {len(cands[name])} candidates |")
    out.append("")
    out.append("## Later-build bytes not explained by any placed block")
    out.append("")
    for lo, hi in gaps:
        out.append(f"- {lo:06o}-{hi - 1:06o} ({hi - lo} bytes)")
    if bad_refs:
        out.append("")
        out.append("## Inconsistent references")
        out.append("")
        for r in bad_refs[:200]:
            name, a, p, kind, v, expect, got, lno = r
            out.append(f"- {name} line {lno}: 1973 {a:06o} {kind}->{v:06o}, expected {expect:06o}, later has {got:06o}")
    open(os.path.join(OUT, "report.md"), "w").write("\n".join(out) + "\n")
    json.dump({"placed": {k: v for k, v in placed.items()},
               "blocks": [(b[0], b[1], b[2]) for b in blocks]},
              open(os.path.join(OUT, "map.json"), "w"), indent=0)
    print("\n".join(out[:8]))
    print(f"unplaced blocks: {sum(1 for b in blocks if b[0] not in placed)}; gaps: {len(gaps)}")


if __name__ == "__main__":
    main()
