"""Recover where the Mare Assabet build put its RAM buffers.

Reserved storage is never loaded, so the binary itself does not contain these
areas. But code refers to them: every relocated reference in a placed 1973
routine names a 1973 label, and the corresponding word in the binary gives
that label's new address. Several independent references to the same label
must agree.
"""
import json, os, sys
from collections import defaultdict
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
import lda, listing
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
old = open(os.path.join(ROOT, "oracle/build/gtlem-1973.bin"), "rb").read()
new, _, _ = lda.read_lda(os.path.join(ROOT, "reference/original/lunar-later-nosource.lda"))
rn = lambda a: new.get(a, 0) | (new.get(a + 1, 0) << 8)
lines, labels, consts = listing.parse(os.path.join(ROOT, "oracle/build/gtlem-1973.lst"), base=0o1000)
amap = json.load(open(os.path.join(ROOT, "oracle/align/map.json"))); placed = amap["placed"]
def shift_of(a):
    for n, s, e in amap["blocks"]:
        if s <= a < e:
            return placed.get(n)
by_target = defaultdict(lambda: defaultdict(int))
for ln in lines:
    if ln.addr is None:
        continue
    for k, (v, n, reloc) in enumerate(ln.items):
        if not reloc or n != 2:
            continue
        wa = ln.addr + 2 * k
        d = shift_of(wa)
        if d is None:
            continue
        by_target[old[wa] | (old[wa + 1] << 8)][rn(wa + d)] += 1
want = sys.argv[1:] or ["MOONST", "MOONEN", "EXLIST", "STACK", "STACKD", "SHIPB1", "SHIPB2",
                        "FLAMIN", "SHIPDP", "FSUBC", "SYSMES", "DUSTON", "MOONGO", "STARTM"]
for name in want:
    o = labels[name]
    seen = by_target.get(o, {})
    txt = ", ".join(f"{v:06o} (x{c})" for v, c in sorted(seen.items())) or "no references from placed code"
    print(f"{name:7s} 1973 {o:06o}  ->  {txt}")
