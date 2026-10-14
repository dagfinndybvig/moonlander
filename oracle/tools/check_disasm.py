"""Validate the decoder against the 1973 listing, which is ground truth.

For every source line that is an instruction, decode the assembled image at
that address and check: the mnemonic agrees (allowing MACRO-11 synonyms), the
instruction length equals the words the assembler emitted, and every word the
linker relocated is an extension word the decoder classified as an operand.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import listing
import pdp11

SYNONYMS = {"BHIS": "BCC", "BLO": "BCS"}
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "build")


def main():
    img = open(os.path.join(ROOT, "gtlem-1973.bin"), "rb").read()

    def rw(a):
        return img[a] | (img[a + 1] << 8)

    lines, labels, consts = listing.parse(os.path.join(ROOT, "gtlem-1973.lst"), base=0o1000)
    known = set(pdp11.DOUBLE.values()) | set(pdp11.SINGLE.values()) | set(pdp11.BRANCH.values())
    known |= {m + "B" for m in pdp11.SINGLE.values()} | set(pdp11.ZERO_OP.values())
    known |= {"JMP", "JSR", "RTS", "EMT", "TRAP", "NOP", "CLC", "CLV", "CLZ", "CLN",
              "SEC", "SEV", "SEZ", "SEN", "CCC", "SCC"} | set(SYNONYMS)
    n = bad = 0
    mnem_count = {}
    idiom = []
    for ln in lines:
        st = ln.statement
        if not st or ln.addr is None:
            continue
        op = st.split()[0]
        if op not in known:
            continue
        n += 1
        ins = pdp11.decode(rw, ln.addr)
        want = SYNONYMS.get(op, op)
        problems = []
        if ins.mnemonic != want:
            problems.append(f"mnemonic {ins.mnemonic} != {want}")
        if ins.length * 2 != ln.nbytes:
            if "(PC)+" in st and ln.nbytes == 2:
                # The source spells this instruction's extension words as the
                # following statements (Burness's self-modifying idiom at
                # DRAW22). The decoder is right to consume them.
                idiom.append(f"{ln.lineno:5d} {ln.addr:06o}  {st:28s} = {ins.text}")
            else:
                problems.append(f"length {ins.length * 2} bytes != {ln.nbytes}")
        if not ins.on_1105:
            problems.append("not a PDP-11/05 instruction")
        ext_kinds = {o.word: o.kind for o in ins.operands if o.word}
        for k, (_, _, reloc) in enumerate(ln.items[1:], start=1):
            if reloc and k not in ext_kinds:
                problems.append(f"relocated word {k} not seen as operand")
        mnem_count[want] = mnem_count.get(want, 0) + 1
        if problems:
            bad += 1
            print(f"{ln.lineno:5d} {ln.addr:06o}  {st:40s} -> {ins.text}: {'; '.join(problems)}")
    print(f"\n{n} instructions checked, {bad} disagreements")
    print(f"{len(idiom)} lines where the source spells extension words as separate statements:")
    for s in idiom:
        print("   ", s)
    print(f"{len(labels)} labels, {len(consts)} constants")
    print("mnemonics used:", " ".join(f"{k}:{v}" for k, v in sorted(mnem_count.items())))


if __name__ == "__main__":
    main()
