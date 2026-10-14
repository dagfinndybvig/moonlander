"""Disassembler for DEC VT11 display files (the GT40's display processor).

Control words have bit 15 set. Bits 14-11 select the instruction:
  0mmm  set graphic mode m (0 char, 1 short vector, 2 long vector, 3 point,
        4 graphplot X, 5 graphplot Y, 6 relative point), with optional
        intensity (bit 10 enables bits 9-7), light pen interrupt enable
        (bit 6 enables bit 5), blink (bit 4 enables bit 3), line type
        (bit 2 enables bits 1-0)
  1100  jump; the next word is the target address
  1101  no-op
  1110  load status register A: stop (bit 10), stop interrupt (9 enables 8),
        light pen intensify (7 enables 6), italics (5 enables 4),
        sync to line frequency (bit 2)
  1111  load status register B: graphplot increment (6 enables 5-0)
Data words (bit 15 clear) are interpreted according to the current mode.

Source: Al Kossow's VT11 instruction set summary (1993), listed in
reference/hardware-docs/README.md.
"""

MODES = ["CHAR", "SVEC", "LVEC", "POINT", "GRAPHX", "GRAPHY", "RELPT", "SPARE7"]
LINES = ["solid", "long-dash", "short-dash", "dot-dash"]


def control(w):
    op = (w >> 11) & 0o17
    if op < 8:
        parts = [f"MODE {MODES[op]}"]
        if w & 0o2000:
            parts.append(f"int={(w >> 7) & 7}")
        if w & 0o100:
            parts.append("lp=" + ("on" if w & 0o40 else "off"))
        if w & 0o20:
            parts.append("blink=" + ("on" if w & 0o10 else "off"))
        if w & 0o4:
            parts.append("line=" + LINES[w & 3])
        return op, " ".join(parts)
    if op == 0o14:
        return "JUMP", "JUMP"
    if op == 0o15:
        return "NOP", "NOP"
    if op == 0o16:
        parts = ["STATUS-A"]
        if w & 0o2000:
            parts.append("STOP")
        if w & 0o1000:
            parts.append("stop-int=" + ("on" if w & 0o400 else "off"))
        if w & 0o200:
            parts.append("lp-intensify=" + ("off" if w & 0o100 else "on"))
        if w & 0o40:
            parts.append("italics=" + ("on" if w & 0o20 else "off"))
        if w & 0o4:
            parts.append("SYNC")
        return "SA", " ".join(parts)
    parts = ["STATUS-B"]
    if w & 0o100:
        parts.append(f"graphplot-incr={w & 0o77}")
    return "SB", " ".join(parts)


def sgn(v, bit):
    return -v if bit else v


def disassemble(read_word, lo, hi, mode=None, names=None):
    """Yield (addr, words, text) for the display file between lo and hi."""
    names = names or {}
    a = lo
    while a <= hi:
        w = read_word(a)
        if w & 0o100000:
            op, text = control(w)
            if op == "JUMP":
                t = read_word(a + 2)
                yield a, [w, t], f"JUMP {names.get(t, f'{t:06o}')}"
                a += 4
                continue
            if isinstance(op, int):
                mode = op
            yield a, [w], text
            a += 2
            continue
        if mode is None:
            yield a, [w], f"data {w:06o} (mode unknown)"
            a += 2
            continue
        m = MODES[mode]
        if m == "CHAR":
            c1, c2 = w & 0o177, (w >> 8) & 0o177
            show = lambda c: repr(chr(c)) if 32 <= c < 127 else f"\\{c:03o}"
            yield a, [w], f"  chars {show(c1)} {show(c2)}"
            a += 2
        elif m in ("SVEC", "RELPT"):
            dx = sgn((w >> 7) & 0o77, w & 0o20000)
            dy = sgn(w & 0o77, w & 0o100)
            vis = "draw" if w & 0o40000 else "move"
            yield a, [w], f"  {vis} dx={dx:+d} dy={dy:+d}"
            a += 2
        elif m == "LVEC":
            w2 = read_word(a + 2)
            dx = sgn(w & 0o1777, w & 0o20000)
            dy = sgn(w2 & 0o1777, w2 & 0o20000)
            vis = "draw" if w & 0o40000 else "move"
            yield a, [w, w2], f"  {vis} dx={dx:+d} dy={dy:+d}"
            a += 4
        elif m == "POINT":
            w2 = read_word(a + 2)
            vis = "dot" if w & 0o40000 else "at"
            yield a, [w, w2], f"  {vis} x={w & 0o1777} y={w2 & 0o1777}"
            a += 4
        else:
            yield a, [w], f"  {m.lower()} {w & 0o1777}"
            a += 2


if __name__ == "__main__":
    import os
    import sys
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import lda
    root = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
    if sys.argv[1] == "--old":
        img = open(os.path.join(root, "oracle/build/gtlem-1973.bin"), "rb").read()
        rd = lambda a: img[a] | (img[a + 1] << 8)
        args = sys.argv[2:]
    else:
        mem, _, _ = lda.read_lda(os.path.join(root, "reference/original/lunar-later-nosource.lda"))
        rd = lambda a: mem.get(a, 0) | (mem.get(a + 1, 0) << 8)
        args = sys.argv[1:]
    lo, hi = int(args[0], 8), int(args[1], 8)
    for a, ws, t in disassemble(rd, lo, hi):
        print(f"{a:06o}  {' '.join(f'{x:06o}' for x in ws):14s}  {t}")
