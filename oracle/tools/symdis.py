"""Symbolized disassembly of the later build, using names recovered from 1973.

Every 1973 label inside a block the aligner placed is carried to its new
address. Operands that point at a named location print the name; anything
the alignment could not explain prints as a raw octal address.

Usage: symdis.py LO HI          (octal addresses in the later build)
       symdis.py --old LO HI    (same, for the 1973 build, names from listing)
"""

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import lda  # noqa: E402
import listing  # noqa: E402
import pdp11  # noqa: E402

ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))


def load_names():
    _, labels, consts = listing.parse(os.path.join(ROOT, "oracle/build/gtlem-1973.lst"), base=0o1000)
    amap = json.load(open(os.path.join(ROOT, "oracle/align/map.json")))
    placed = amap["placed"]
    blocks = amap["blocks"]

    def shift(old):
        for name, s, e in blocks:
            if s <= old < e:
                return placed.get(name)
        return None

    old_names = {}
    new_names = {}
    for name, addr in labels.items():
        if addr is None:
            continue
        old_names.setdefault(addr, name)
        d = shift(addr)
        if d is not None:
            new_names.setdefault(addr + d, name)
    devices = {v: k for k, v in consts.items() if v >= 0o160000}
    return old_names, new_names, devices


def symbolize(text, names, devices):
    import re

    def name_of(v):
        return names.get(v) or devices.get(v)

    def sub(m):
        return name_of(int(m.group(1), 8)) or m.group(1)
    # Absolute operands (@#n) and relative/branch targets are addresses and
    # get names. Immediates (#n) stay numeric: in the later build nothing says
    # which immediates are addresses, and guessing would invent meaning.
    text = re.sub(r"@#([0-7]{3,6})(?![0-7])", lambda m: "@#" + sub(m), text)
    text = text.replace("@#", "@#\x00")   # protect what was just handled
    text = re.sub(r"(?<![0-9A-Z$#\x00])([0-7]{3,6})(?![0-7])", sub, text)
    return text.replace("\x00", "")


def dump(read, lo, hi, names, devices, other=None):
    a = lo
    while a <= hi:
        ins = pdp11.decode(read, a)
        lab = names.get(a, "")
        words = " ".join(f"{w:06o}" for w in ins.words)
        txt = ins.text
        if ins.mnemonic not in (".WORD",):
            mn, _, ops = txt.partition("\t")
            # immediates stay numeric unless they hit a known label
            txt = f"{mn}\t{symbolize(ops, names, devices)}" if ops else mn
        flag = "" if ins.on_1105 else "   ; not an 11/05 instruction"
        print(f"{a:06o} {words:22s} {lab + ':' if lab else '':8s}{txt}{flag}")
        a += 2 * ins.length


def main():
    old_names, new_names, devices = load_names()
    if sys.argv[1] == "--old":
        img = open(os.path.join(ROOT, "oracle/build/gtlem-1973.bin"), "rb").read()
        read = lambda a: img[a] | (img[a + 1] << 8)
        dump(read, int(sys.argv[2], 8), int(sys.argv[3], 8), old_names, devices)
    else:
        mem, _, _ = lda.read_lda(os.path.join(ROOT, "reference/original/lunar-later-nosource.lda"))
        read = lambda a: mem.get(a, 0) | (mem.get(a + 1, 0) << 8)
        dump(read, int(sys.argv[1], 8), int(sys.argv[2], 8), new_names, devices)


if __name__ == "__main__":
    main()
