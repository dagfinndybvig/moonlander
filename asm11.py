"""A two-pass assembler for the PDP-11 subset the GT40 needs.

Written for the kd11b emulator, which resolves mode-6/7 index words as byte
displacements (right for X(Rn), never doubled for PC-relative). Branch
offsets it does double, as the hardware does. So:

  * label / @label operands carry a BYTE displacement,
  * branch operands carry a WORD displacement (halved here, doubled by the
    machine), as on real hardware.

Syntax (one instruction per line, ';' comments, labels end with ':'):

  MOV R1, -(SP)        registers R0-R7, SP=R6, PC=R7
  MOV @#X, R0          absolute (mode 37)
  ADD #10, R1          immediate (mode 27)
  JMP (R3)+            all addressing modes
  CLR X(R4)            indexed
  BEQ THERE            branches (8-bit word displacement)
  JSR PC, SUBR
  .WORD a, b           data (expressions, labels, 'c character literals)
  .ASCII "TEXT"        string, high byte of the last word zeroed if odd
  .BYTE n, n           bytes (pads to even with 0)
  .BLKW n              reserve n words
  .ORG n               set location counter
  .EVEN                round up to even
  NAME = expr          constant

Output: a DEC absolute loader image (.lda), via the repo's oracle/tools/lda.py.
"""

import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "oracle", "tools"))
import lda as ldamod

REGS = {"R0": 0, "R1": 1, "R2": 2, "R3": 3, "R4": 4, "R5": 5, "SP": 6, "PC": 7}

# name -> (opcode, format)  format: 'd2' double, 'd1' single/branch-to-addr,
# 'br' branch, 'none' no operand, 'sob' (unused here)
DOUBLE = {"MOV": 0o01, "CMP": 0o02, "BIT": 0o03, "BIC": 0o04, "BIS": 0o05,
          "MOV B": 0o11, "CMPB": 0o12, "BITB": 0o13, "BICB": 0o14, "BISB": 0o15,
          "MOVB": 0o11, "ADD": 0o06, "SUB": 0o16, "XOR": 0o07}
SINGLE = {"CLR": 0o0050, "COM": 0o0051, "INC": 0o0052, "DEC": 0o0053,
          "NEG": 0o0054, "ADC": 0o0055, "SBC": 0o0056, "TST": 0o0057,
          "ROR": 0o0060, "ROL": 0o0061, "ASR": 0o0062, "ASL": 0o0063,
          # This repo's kd11b model dispatches JMP at 0o0001DD and SWAB at
          # 0o0003DD (see cpu.rs, cost.rs, timing.rs and the game binary),
          # not the DEC handbook's 0o0064DD / 0o0067DD.
          "JMP": 0o0001, "SWAB": 0o0003,
          "CLRB": 0o1050, "COMB": 0o1051,
          "INCB": 0o1052, "DECB": 0o1053, "NEGB": 0o1054, "TSTB": 0o1057,
          "RORB": 0o1060, "ROLB": 0o1061, "ASRB": 0o1062, "ASLB": 0o1063}
BRANCH = {"BR": 0o000400, "BNE": 0o001000, "BEQ": 0o001400, "BGE": 0o002000,
          "BLT": 0o002400, "BGT": 0o003000, "BLE": 0o003400, "BPL": 0o100000,
          "BMI": 0o100400, "BHI": 0o101000, "BLOS": 0o101400, "BVC": 0o102000,
          "BVS": 0o102400, "BCC": 0o103000, "BCS": 0o103400, "BLS": 0o103400}
NOOPERAND = {"WAIT": 0o000001, "RTI": 0o000002, "HALT": 0o000000, "RESET": 0o000005,
             "CLC": 0o000241, "SEC": 0o000242, "CLV": 0o000244, "SEV": 0o000245,
             "CLZ": 0o000250, "SEZ": 0o000251, "CLN": 0o000252, "SEN": 0o000253}


class Asm:
    def __init__(self):
        self.mem = {}          # word address -> value
        self.cur = 0o1000
        self.labels = {}
        self.fixups = []       # (addr, kind, label/expr, extra)
        self.errors = []

    # ---- pass 1 helpers ----------------------------------------------------

    def org(self, a): self.cur = a
    def here(self): return self.cur
    def even(self):
        if self.cur & 1: self.cur += 1
    def label(self, name):
        if name in self.labels: self.errors.append(f"duplicate label {name}")
        self.labels[name] = self.cur
    def equ(self, name, val): self.labels[name] = val

    def emit_word(self, w):
        self.mem[self.cur] = w & 0xFFFF; self.cur += 2
    def emit_byte(self, b):
        a = self.cur & ~1
        if self.cur & 1:
            self.mem[a] = (self.mem[a] & 0x00FF) | ((b & 0xFF) << 8)
        else:
            self.mem[a] = b & 0xFF
        self.cur += 1
    def blk(self, n): self.cur += 2 * n

    # ---- expression evaluation (pass 2) -------------------------------------

    def value(self, e):
        e = e.strip()
        if e.startswith("'") and len(e) >= 2:
            return ord(e[1])
        if e.startswith("0o"):
            return int(e[2:], 8)
        if e.startswith("0x"):
            return int(e[2:], 16)
        # simple a+b / a-b
        for op in ("+", "-"):
            i = e.rfind(op)
            if i > 0:
                try:
                    a, b = self.value(e[:i]), self.value(e[i+1:])
                    return a + b if op == "+" else a - b
                except Exception:
                    pass
        if e.endswith("."):
            return int(e[:-1])          # trailing dot: decimal
        if e.lstrip("-").isdigit():
            try:
                return int(e, 8)
            except ValueError:
                return int(e)  # digits 8/9: decimal
        if e in self.labels:
            return self.labels[e]
        raise KeyError(e)

    # ---- operands ------------------------------------------------------------

    def reg(self, t):
        t = t.strip()
        if t.upper() in REGS: return REGS[t.upper()]
        raise KeyError(t)

    def operand(self, t):
        """Return (mode*8+reg, [extra words as (kind, expr)])."""
        t = t.strip()
        u = t.upper()
        if u in REGS:
            return (REGS[u], [])
        if t.startswith("@#"):
            return (0o37, [("w", t[2:])])
        if t.startswith("#"):
            return (0o27, [("w", t[1:])])
        if t.startswith("@-"):
            r = self.reg(t[2:]); return (0o50 + r, [])
        if t.startswith("-("):
            r = self.reg(t[2:-1]); return (0o40 + r, [])
        if t.startswith("@(") and t.endswith("+"):
            r = self.reg(t[2:-2]); return (0o30 + r, [])
        if t.startswith("(") and t.endswith("+"):
            r = self.reg(t[1:-2]); return (0o20 + r, [])
        if t.startswith("@(") and t.endswith(")"):
            r = self.reg(t[2:-1]); return (0o10 + r, [])
        if t.startswith("(") and t.endswith(")"):
            r = self.reg(t[1:-1]); return (0o10 + r, [])
        # @label  (relative deferred, mode 77)
        if t.startswith("@"):
            return (0o77, [("b", t[1:])])
        # X(Rn) or @X(Rn)
        if t.endswith(")") and "(" in t:
            i = t.rindex("(")
            idx = t[i+1:-1]
            r = self.reg(idx)
            base = t[:i]
            if base.startswith("@"):
                return (0o70 + r, [("w", base[1:])])
            return (0o60 + r, [("w", base)])
        # plain label: PC-relative (mode 67), byte displacement
        return (0o67, [("b", t)])

    def fix(self, addr, kind, expr):
        self.fixups.append((addr, kind, expr))

    # ---- instruction assembly -------------------------------------------------

    def instr(self, mnem, ops):
        u = mnem.upper()
        if u in NOOPERAND:
            self.emit_word(NOOPERAND[u]); return
        if u in ("RTS",):
            self.emit_word(0o000200 | self.reg(ops[0])); return
        if u in BRANCH or u.rstrip() in BRANCH:
            op = BRANCH[u.rstrip()] if u not in BRANCH else BRANCH[u]
            self.emit_word(op)
            self.fix(self.cur - 2, "brw", ops[0]); return
        if u in DOUBLE:
            op = DOUBLE[u]
            if u in ("ADD", "SUB") or (op & 0o170000 == 0o010000 and False):
                pass
            if u in ("ADD", "SUB", "XOR"):
                # strict forms: ADD/SUB word-only, src any, dst any
                s = self.operand(ops[0]); d = self.operand(ops[1])
                self.emit_word((op << 12) | (s[0] << 6) | d[0])
                for k, e in s[1]: self.fix(self.cur, k, e); self.emit_word(0)
                for k, e in d[1]: self.fix(self.cur, k, e); self.emit_word(0)
                return
            s = self.operand(ops[0]); d = self.operand(ops[1])
            self.emit_word((op << 12) | (s[0] << 6) | d[0])
            for k, e in s[1]: self.fix(self.cur, k, e); self.emit_word(0)
            for k, e in d[1]: self.fix(self.cur, k, e); self.emit_word(0)
            return
        if u in SINGLE:
            d = self.operand(ops[0])
            self.emit_word((SINGLE[u] << 6) | d[0])
            for k, e in d[1]: self.fix(self.cur, k, e); self.emit_word(0)
            return
        if u == "JSR":
            r = self.reg(ops[0]); d = self.operand(ops[1])
            self.emit_word((0o004 << 9) | (r << 6) | d[0])
            for k, e in d[1]: self.fix(self.cur, k, e); self.emit_word(0)
            return
        if u == "SOB":
            raise ValueError("SOB traps on the KD11-B; use DEC/BNE")
        raise KeyError(mnem)

    # ---- pass 2 ----------------------------------------------------------------

    def resolve(self):
        for addr, kind, expr in self.fixups:
            v = self.value(expr)
            if kind == "w":
                self.mem[addr] = v & 0xFFFF
            elif kind == "b":
                # byte displacement for mode 6/7: target - (addr + 2)
                off = v - (addr + 2)
                self.mem[addr] = off & 0xFFFF
            elif kind == "brw":
                off = (v - (addr + 2)) // 2
                if not -128 <= off <= 127:
                    self.errors.append(f"branch out of range at {addr:06o} to {expr}")
                self.mem[addr] = (self.mem[addr] & 0o177400) | (off & 0o377)
            elif kind == "hw":
                self.mem[addr] = (self.mem[addr] & 0x00FF) | ((v & 0xFF) << 8)

    def write(self, path, start):
        bytemem = {}
        for a, w in self.mem.items():
            bytemem[a] = w & 0xFF
            bytemem[a + 1] = (w >> 8) & 0xFF
        ldamod.write_lda(path, bytemem, start)
