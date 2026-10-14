"""PDP-11 instruction decoder for reverse engineering GT40 programs.

decode(read_word, addr) returns an Insn describing the instruction at addr:
its length in words, MACRO-11 style text, and, for every extension word, what
kind of value it holds. The operand kinds matter more than the text: they say
which words in a program image depend on where things were placed in memory.

Operand word kinds:
  "imm"    #n      immediate (mode 2 on PC). May or may not be an address.
  "abs"    @#n     absolute address (mode 3 on PC).
  "rel"    label   PC-relative (mode 6 on PC); target = word address + 2 + n.
  "relind" @label  PC-relative deferred (mode 7 on PC).
  "index"  n(Rx)   index offset (modes 6 and 7 on R0-R5, SP). May be an address.

The GT40 processor is a KD11-B (PDP-11/05). It lacks EIS, FIS, FPP, SOB, XOR,
SXT, MARK, RTT, MFPI/MTPI and MFPS/MTPS. Such opcodes decode with
`on_1105=False`, which is a strong hint that the words are data, not code.
"""

from dataclasses import dataclass, field

REGS = ["R0", "R1", "R2", "R3", "R4", "R5", "SP", "PC"]


@dataclass
class Operand:
    text: str
    word: int = None     # index of the extension word within the instruction, if any
    kind: str = None     # imm, abs, rel, relind, index
    value: int = None    # raw extension word
    target: int = None   # effective address for rel/relind (absolute)


@dataclass
class Insn:
    addr: int
    words: list
    mnemonic: str
    operands: list = field(default_factory=list)
    on_1105: bool = True
    branch_target: int = None
    flow: str = "seq"    # seq, branch, jump, call, return, stop

    @property
    def length(self):
        return len(self.words)

    @property
    def text(self):
        ops = ",".join(o.text for o in self.operands)
        return f"{self.mnemonic}\t{ops}" if ops else self.mnemonic


def _operand(spec, read_word, addr, ext):
    """Decode a 6-bit operand spec. ext is the list of extension words so far."""
    mode, reg = (spec >> 3) & 7, spec & 7
    r = REGS[reg]
    if mode == 0:
        return Operand(r)
    if mode == 1:
        return Operand(f"({r})")
    if mode == 4:
        return Operand(f"-({r})")
    if mode == 5:
        return Operand(f"@-({r})")
    # modes with an extension word when reg is PC, or modes 6/7 always
    if reg == 7 and mode in (2, 3):
        waddr = addr + 2 * (1 + len(ext))
        v = read_word(waddr)
        ext.append(v)
        if mode == 2:
            return Operand(f"#{v:o}", len(ext), "imm", v)
        return Operand(f"@#{v:o}", len(ext), "abs", v)
    if mode == 2:
        return Operand(f"({r})+")
    if mode == 3:
        return Operand(f"@({r})+")
    waddr = addr + 2 * (1 + len(ext))
    v = read_word(waddr)
    ext.append(v)
    if reg == 7:
        tgt = (waddr + 2 + v) & 0xFFFF
        if mode == 6:
            return Operand(f"{tgt:o}", len(ext), "rel", v, tgt)
        return Operand(f"@{tgt:o}", len(ext), "relind", v, tgt)
    pre = "@" if mode == 7 else ""
    return Operand(f"{pre}{v:o}({r})", len(ext), "index", v)


DOUBLE = {0o01: "MOV", 0o02: "CMP", 0o03: "BIT", 0o04: "BIC", 0o05: "BIS", 0o06: "ADD",
          0o11: "MOVB", 0o12: "CMPB", 0o13: "BITB", 0o14: "BICB", 0o15: "BISB", 0o16: "SUB"}

SINGLE = {0o050: "CLR", 0o051: "COM", 0o052: "INC", 0o053: "DEC", 0o054: "NEG",
          0o055: "ADC", 0o056: "SBC", 0o057: "TST", 0o060: "ROR", 0o061: "ROL",
          0o062: "ASR", 0o063: "ASL", 0o003: "SWAB",
          0o065: "MFPI", 0o066: "MTPI", 0o067: "SXT"}
SINGLE_NOT_1105 = {"MFPI", "MTPI", "SXT"}

# Keyed by the high byte of the instruction word (opcode bits 15-8).
BRANCH = {0o001: "BR", 0o002: "BNE", 0o003: "BEQ", 0o004: "BGE", 0o005: "BLT",
          0o006: "BGT", 0o007: "BLE", 0o200: "BPL", 0o201: "BMI", 0o202: "BHI",
          0o203: "BLOS", 0o204: "BVC", 0o205: "BVS", 0o206: "BCC", 0o207: "BCS"}

ZERO_OP = {0: "HALT", 1: "WAIT", 2: "RTI", 3: "BPT", 4: "IOT", 5: "RESET", 6: "RTT"}

CC_NAMES = {0o240: "NOP", 0o257: "CCC", 0o277: "SCC"}


def _cc(w):
    if w in CC_NAMES:
        return CC_NAMES[w]
    s = "SE" if w & 0o20 else "CL"
    parts = [s + f for bit, f in ((8, "N"), (4, "Z"), (2, "V"), (1, "C")) if w & bit]
    return "!".join(parts)


def decode(read_word, addr):
    w = read_word(addr)
    ext = []

    def done(mn, ops=(), on=True, **kw):
        return Insn(addr, [w] + ext, mn, list(ops), on, **kw)

    if w < 0o10:
        mn = ZERO_OP[w]
        flow = {"HALT": "stop", "RTI": "return", "RTT": "return"}.get(mn, "seq")
        return done(mn, on=(mn != "RTT"), flow=flow)
    if 0o000100 <= w <= 0o000177:
        op = _operand(w & 0o77, read_word, addr, ext)
        return done("JMP", [op], flow="jump")
    if 0o000200 <= w <= 0o000207:
        return done("RTS", [Operand(REGS[w & 7])], flow="return")
    if 0o000240 <= w <= 0o000277:
        return done(_cc(w))
    if 0o000300 <= w <= 0o000377:
        return done("SWAB", [_operand(w & 0o77, read_word, addr, ext)])
    name = BRANCH.get(w >> 8)
    if name:
        off = w & 0o377
        if off & 0o200:
            off -= 0o400
        tgt = (addr + 2 + 2 * off) & 0xFFFF
        flow = "jump" if name == "BR" else "branch"
        return done(name, [Operand(f"{tgt:o}")], branch_target=tgt, flow=flow)
    if 0o004000 <= w <= 0o004777:
        r = REGS[(w >> 6) & 7]
        op = _operand(w & 0o77, read_word, addr, ext)
        return done("JSR", [Operand(r), op], flow="call")
    if 0o006400 <= w <= 0o006477:
        return done("MARK", [Operand(f"{w & 0o77:o}")], on=False)
    sop = (w >> 6) & 0o1777          # bits 15-6
    if sop in SINGLE:                # word single-operand group, 0050DD-0067DD
        mn = SINGLE[sop]
        op = _operand(w & 0o77, read_word, addr, ext)
        return done(mn, [op], on=mn not in SINGLE_NOT_1105)
    if (sop & 0o777) in SINGLE and w & 0o100000:   # byte group, 1050DD-1067DD
        word_mn = SINGLE[sop & 0o777]
        byte_mn = {"MFPI": "MFPD", "MTPI": "MTPD", "SXT": "MFPS"}.get(word_mn, word_mn + "B")
        op = _operand(w & 0o77, read_word, addr, ext)
        return done(byte_mn, [op], on=word_mn not in SINGLE_NOT_1105)
    if 0o106400 <= w <= 0o106477:
        return done("MTPS", [_operand(w & 0o77, read_word, addr, ext)], on=False)
    top = w >> 12
    if top in DOUBLE:
        src = _operand((w >> 6) & 0o77, read_word, addr, ext)
        dst = _operand(w & 0o77, read_word, addr, ext)
        return done(DOUBLE[top], [src, dst])
    if 0o070000 <= w <= 0o074777:
        mn = ["MUL", "DIV", "ASH", "ASHC", "XOR"][(w >> 9) & 7]
        r = REGS[(w >> 6) & 7]
        op = _operand(w & 0o77, read_word, addr, ext)
        return done(mn, [op, Operand(r)] if mn != "XOR" else [Operand(r), op], on=False)
    if 0o077000 <= w <= 0o077777:
        tgt = (addr + 2 - 2 * (w & 0o77)) & 0xFFFF
        return done("SOB", [Operand(REGS[(w >> 6) & 7]), Operand(f"{tgt:o}")],
                    on=False, branch_target=tgt, flow="branch")
    if 0o104000 <= w <= 0o104377:
        return done("EMT", [Operand(f"{w & 0o377:o}")])
    if 0o104400 <= w <= 0o104777:
        return done("TRAP", [Operand(f"{w & 0o377:o}")])
    return done(".WORD", [Operand(f"{w:o}")], on=False)


if __name__ == "__main__":
    # Self-test on hand-checked encodings from the 1973 listing.
    cases = {
        (0o000005,): "RESET",
        (0o012767, 0o001014, 0o000024): None,   # MOV #n,rel
        (0o004767, 0o011270): None,             # JSR PC,rel
        (0o003373,): "BGT",
        (0o000207,): "RTS",
        (0o103005,): "BCC",
        (0o006102,): "ROL",
    }
    for words, mn in cases.items():
        mem = {0o1000 + 2 * i: v for i, v in enumerate(words)}
        ins = decode(lambda a: mem.get(a, 0), 0o1000)
        print(f"{' '.join(f'{x:06o}' for x in words):24s} -> {ins.text:28s} len={ins.length}")
