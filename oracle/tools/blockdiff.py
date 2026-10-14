"""Side-by-side comparison of a 1973 routine with its counterpart in the
Mare Assabet build.

The two instruction streams are aligned with difflib over a normalized form of
each instruction (mnemonic plus operands, with addresses replaced by the names
of what they point to, and PC-relative/absolute addressing treated as the same
thing). Matching instructions print once; differences print side by side.

Usage: blockdiff.py LABEL [LABEL2]   compare 1973 LABEL..LABEL2 (default: up to
                                     the next global label) with the binary.
       blockdiff.py --range OLDLO OLDHI NEWLO NEWHI   (octal)
"""

import difflib
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import lda  # noqa: E402
import listing  # noqa: E402
import pdp11  # noqa: E402
import symdis  # noqa: E402

ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))


def stream(read, lo, hi, names, devices):
    out = []
    a = lo
    while a <= hi:
        ins = pdp11.decode(read, a)
        mn, _, ops = ins.text.partition("\t")
        ops = symdis.symbolize(ops, names, devices) if ins.mnemonic != ".WORD" else ops
        # PC-relative and absolute addressing are equivalent here (.ENABL AMA)
        norm = re.sub(r"@#", "", f"{mn} {ops}")
        out.append((a, ins, f"{mn}\t{ops}" if ops else mn, norm))
        a += 2 * ins.length
    return out


def main():
    old_names, new_names, devices = symdis.load_names()
    oimg = open(os.path.join(ROOT, "oracle/build/gtlem-1973.bin"), "rb").read()
    nmem, _, _ = lda.read_lda(os.path.join(ROOT, "reference/original/lunar-later-nosource.lda"))
    ro = lambda a: oimg[a] | (oimg[a + 1] << 8)
    rn = lambda a: nmem.get(a, 0) | (nmem.get(a + 1, 0) << 8)

    if sys.argv[1] == "--range":
        olo, ohi, nlo, nhi = (int(x, 8) for x in sys.argv[2:6])
    else:
        _, labels, _ = listing.parse(os.path.join(ROOT, "oracle/build/gtlem-1973.lst"), base=0o1000)
        amap = json.load(open(os.path.join(ROOT, "oracle/align/map.json")))
        glob = sorted((v, k) for k, v in labels.items() if v is not None and ":" not in k)
        olo = labels[sys.argv[1]]
        if len(sys.argv) > 2:
            ohi = labels[sys.argv[2]] - 2
        else:
            ohi = next(v for v, k in glob if v > olo) - 2
        # estimate the new location from the nearest placed block before it
        placed = amap["placed"]
        prev = [(s, placed[n]) for n, s, e in amap["blocks"] if n in placed and s <= olo]
        shift = prev[-1][1] if prev else 0
        nxt = [(s, placed[n]) for n, s, e in amap["blocks"] if n in placed and s > ohi]
        nlo = olo + shift
        nhi = (nxt[0][0] + nxt[0][1] - 2) if nxt else ohi + shift
    a = stream(ro, olo, ohi, old_names, devices)
    b = stream(rn, nlo, nhi, new_names, devices)
    sm = difflib.SequenceMatcher(a=[x[3] for x in a], b=[x[3] for x in b], autojunk=False)
    print(f"1973 {olo:06o}-{ohi:06o}   vs   Mare Assabet build {nlo:06o}-{nhi:06o}\n")
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == "equal":
            for k in range(i2 - i1):
                oa, _, txt, _ = a[i1 + k]
                na = b[j1 + k][0]
                lab = old_names.get(oa, "")
                print(f"  {oa:06o} {na:06o}  {lab + ':' if lab else '':8s}{txt}")
        else:
            n = max(i2 - i1, j2 - j1)
            for k in range(n):
                left = a[i1 + k] if i1 + k < i2 else None
                right = b[j1 + k] if j1 + k < j2 else None
                lt = f"{left[0]:06o} {old_names.get(left[0], ''):7s} {left[2]}" if left else ""
                rt = f"{right[0]:06o} {right[2]}" if right else ""
                print(f"- {lt:52s} | + {rt}")


if __name__ == "__main__":
    main()
