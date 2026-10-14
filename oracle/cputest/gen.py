"""Generate random PDP-11/05 test programs for differential testing.

Each case: random registers and condition codes, a scratch area filled with
even addresses inside itself (so deferred modes mostly land somewhere legal,
while arithmetic on that data eventually produces odd addresses and bus
errors), and a straight-line program of basic-instruction-set instructions in
every addressing mode, with forward branches. Every trap vector points at its
own HALT, so traps end the case at a known place.

Memory map: 000700-000716 HALTs for traps, 002000-002776 scratch,
003000-003776 stack (SP starts at 003776), 004000 up: the program.

Usage: gen.py SEED COUNT OUT.txt
"""

import random
import sys

SCRATCH_LO, SCRATCH_HI = 0o2000, 0o2776
PROG = 0o4000
VECTORS = [0o4, 0o10, 0o14, 0o20, 0o30, 0o34]

DOUBLE = [0o01, 0o02, 0o03, 0o04, 0o05, 0o06, 0o11, 0o12, 0o13, 0o14, 0o15, 0o16]
SINGLE = [0o050, 0o051, 0o052, 0o053, 0o054, 0o055, 0o056, 0o057, 0o060, 0o061, 0o062, 0o063, 0o003]
BRANCH = [0o001, 0o002, 0o003, 0o004, 0o005, 0o006, 0o007,
          0o200, 0o201, 0o202, 0o203, 0o204, 0o205, 0o206, 0o207]


def operand(rng, pc_after):
    """An operand spec plus its extension words. pc_after(n) gives the
    address the PC will have after the n-th extension word is fetched."""
    choice = rng.random()
    if choice < 0.70:
        mode = rng.randrange(8)
        reg = rng.randrange(6)
        ext = []
        if mode in (6, 7):
            ext = [rng.randrange(0, 0o40, 2)]
        return (mode << 3) | reg, ext
    if choice < 0.80:
        return 0o27, [rng.randrange(0x10000)]                     # #imm
    if choice < 0.90:
        return 0o37, [rng.randrange(SCRATCH_LO, SCRATCH_HI, 2)]   # @#abs
    # PC-relative to a scratch word: displacement from the updated PC
    target = rng.randrange(SCRATCH_LO, SCRATCH_HI, 2)
    return 0o67, [None, target]   # displacement fixed up by the caller


def gen_case(rng, n_insns=24):
    words = []
    addr = PROG
    starts = []
    fixups = []
    for _ in range(n_insns):
        starts.append(addr)
        k = rng.random()
        if k < 0.50:
            op = rng.choice(DOUBLE)
            insn = [0]
            spec_s, ext_s = operand(rng, None)
            spec_d, ext_d = operand(rng, None)
            insn[0] = (op << 12) | (spec_s << 6) | spec_d
            body = []
            for spec, ext in ((spec_s, ext_s), (spec_d, ext_d)):
                if spec == 0o67:
                    body.append(("rel", ext[1]))
                else:
                    body.extend(ext)
            words_this = [insn[0]] + body
        elif k < 0.85:
            op = rng.choice(SINGLE)
            byte = rng.random() < 0.3 and op != 0o003
            spec, ext = operand(rng, None)
            w = ((0o1 if byte else 0) << 15) | (op << 6) | spec
            words_this = [w] + ([("rel", ext[1])] if spec == 0o67 else ext)
        elif k < 0.93:
            words_this = [("branch", rng.choice(BRANCH))]
        elif k < 0.97:
            cc = rng.randrange(0o240, 0o300)
            words_this = [cc]
        else:
            words_this = [rng.choice([0o104000 + rng.randrange(0o400), 0o104400 + rng.randrange(0o400),
                                      0o000003, 0o000004])]
        for w in words_this:
            words.append(w)
            addr += 2
    end = addr
    words.append(0)   # HALT at the end
    # resolve relative operands and branches
    out = []
    a = PROG
    for i, w in enumerate(words):
        if isinstance(w, tuple) and w[0] == "rel":
            out.append((w[1] - (a + 2)) & 0xFFFF)
        elif isinstance(w, tuple) and w[0] == "branch":
            later = [s for s in starts if s > a] + [end]
            target = rng.choice(later[:4])
            off = (target - (a + 2)) // 2
            out.append((w[1] << 8) | (off & 0o377))
        else:
            out.append(w)
        a += 2
    regs = [rng.randrange(0o2200, 0o2600, 2) for _ in range(6)] + [0o3776, PROG]
    psw = 0o340 | rng.randrange(16)
    scratch = [rng.randrange(SCRATCH_LO, SCRATCH_HI, 2) if rng.random() < 0.8 else rng.randrange(0x10000)
               for _ in range((SCRATCH_HI - SCRATCH_LO) // 2 + 1)]
    return regs, psw, scratch, out


def main():
    seed, count, path = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
    rng = random.Random(seed)
    with open(path, "w") as f:
        for n in range(count):
            regs, psw, scratch, prog = gen_case(rng)
            f.write(f"CASE {n}\n")
            f.write("R " + " ".join(f"{r:o}" for r in regs) + "\n")
            f.write(f"PSW {psw:o}\n")
            f.write(f"MEM {SCRATCH_LO:o} " + " ".join(f"{w:o}" for w in scratch) + "\n")
            f.write(f"MEM {PROG:o} " + " ".join(f"{w:o}" for w in prog) + "\n")
            vec = []
            for k, v in enumerate(VECTORS):
                vec.append((v, 0o700 + 2 * k))
            f.write("VEC " + " ".join(f"{v:o}:{h:o}" for v, h in vec) + "\n")
            f.write("END\n")


if __name__ == "__main__":
    main()
