"""Generate the translation's tables from the assembled reconstruction.

Inputs:  reconstruction/out/lunar.lst and lunar.lda (from reconstruction/build.sh)
         the KD11-B timing model, through `cargo run --example costs -p kd11b`
Outputs: crates/moonlander/src/generated/symbols.rs  every label and constant
         crates/moonlander/src/generated/blocks.rs   the basic blocks of the
                                                     original program, each with
                                                     the times of its instructions
         crates/moonlander/data/lunar.lda            the memory image

A basic block starts at every label on an instruction, after every
instruction that transfers control (branches, JMP, JSR, RTS, RTI, HALT, WAIT,
and moves into the PC), and at every instruction that touches the I/O page
or resets the bus. The translation calls `at(BLOCK)` at the start of each
block it executes; that is where it charges time, takes interrupts and
records its trace, and the emulator, in block-synchronous mode, does the
same at the same addresses.

Usage: python3 -I crates/moonlander/gen/gen.py  (from the repository root)
"""

import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "oracle", "tools"))
import lda  # noqa: E402
import listing  # noqa: E402
import pdp11  # noqa: E402

LST = os.path.join(ROOT, "reconstruction", "out", "lunar.lst")
LDA = os.path.join(ROOT, "reconstruction", "out", "lunar.lda")
OUT = os.path.join(ROOT, "crates", "moonlander", "src", "generated")
DATA = os.path.join(ROOT, "crates", "moonlander", "data")

MNEMONICS = set("""MOV MOVB CMP CMPB BIT BITB BIC BICB BIS BISB ADD SUB CLR CLRB COM COMB
INC INCB DEC DECB NEG NEGB ADC ADCB SBC SBCB TST TSTB ROR RORB ROL ROLB ASR ASRB
ASL ASLB SWAB JMP JSR RTS RTI BR BNE BEQ BGE BLT BGT BLE BPL BMI BHI BLOS BVC BVS
BCC BHIS BCS BLO HALT WAIT RESET NOP CLC CLV CLZ CLN SEC SEV SEZ SEN CCC SCC EMT
TRAP IOT BPT""".split())

# Words assembled as data that the program executes. PALSYI holds an
# instruction generated at run time, INC R0 or DEC R0 (the same time).
EXECUTED_DATA = {"PALSYI": 0o005200}

WRITES_DST = set("MOV MOVB ADD SUB BIC BICB BIS BISB CLR CLRB COM COMB INC INCB DEC DECB "
                 "NEG NEGB ADC ADCB SBC SBCB ROR RORB ROL ROLB ASR ASRB ASL ASLB SWAB".split())
REGISTER_ALIASES = {"IN1", "IN2", "RET1", "RET2", "TEMP", "TEMP2"}


def main():
    lines, labels, constants = listing.parse(LST)
    # In an absolute assembly the symbol table gives every label as a plain
    # value; it is authoritative (a label alone on a line takes the location
    # counter, which the listing does not show).
    labels = {name: constants.get(name, a) for name, a in labels.items()}
    mem, _, start = lda.read_lda(LDA)
    words = lda.words(mem)

    def rw(a):
        return words.get(a, 0)

    # Register aliases are listed as %00000n in the symbol table.
    aliases = set(re.findall(r"(\S+)\s*=%0000", open(LST, encoding="latin-1").read()))

    # Instructions: listing lines whose statement is an instruction, decoded
    # from the image, skipping lines that are another instruction's operands
    # (as in MOV (PC)+,@(PC)+ followed by an instruction word used as data).
    cands = []
    for ln in lines:
        st = ln.statement
        if ln.addr is None or not st:
            continue
        if st.split()[0] in MNEMONICS:
            cands.append(ln)
    by_label = {name: a for name, a in labels.items()}
    for name, w in EXECUTED_DATA.items():
        a = by_label[name]
        assert rw(a) == 0, f"{name} should assemble as 0"
    src_at = {}
    for ln in lines:
        if ln.addr is not None and ln.addr not in src_at:
            src_at[ln.addr] = ln.source.rstrip()
    insns = []
    own_label = {}  # the label written on an instruction's own line
    end = -1
    for ln in sorted(cands, key=lambda l: l.addr):
        if ln.addr < end:
            continue  # an operand of the previous instruction
        if ln.label:
            own_label[ln.addr] = ln.label
        ins = pdp11.decode(rw, ln.addr)
        insns.append(ins)
        end = ln.addr + 2 * ins.length
    for name, w in EXECUTED_DATA.items():
        a = by_label[name]
        ins = pdp11.decode(lambda x, a=a, w=w: w if x == a else rw(x), a)
        ins.mnemonic += " (generated at run time)"
        insns.append(ins)
    insns.sort(key=lambda i: i.addr)
    at = {i.addr: i for i in insns}

    # Times, from the processor model.
    req = "".join(f"{i.addr:06o} " + " ".join(f"{w:06o}" for w in i.words) + "\n" for i in insns)
    res = subprocess.run(["cargo", "run", "-q", "--release", "-p", "kd11b", "--example", "costs"],
                         input=req, capture_output=True, text=True, cwd=ROOT, check=True)
    costs = {}
    timing_source = None
    for line in res.stdout.splitlines():
        if line.startswith("# timing:"):
            timing_source = line.split(":", 1)[1].strip()
            continue
        if line.startswith("# interrupt:"):
            interrupt = line.split(":", 1)[1].strip()
            continue
        if line.startswith("# fault3:"):
            fault3 = line.split(":", 1)[1].split()
            continue
        f = line.split()
        a, n, kind = f[0], f[1], f[2]
        if kind == "T":
            step = f"T({f[3]})"
        elif kind == "B":
            step = f"B({f[3]}, {f[4]})"
        else:
            access = {"N": "None", "R": "Read", "W": "Write", "M": "ReadWrite"}[f[4]]
            src = "true" if f[3] == "1" else "false"
            step = f"Y([{', '.join(f[5:9])}], {src}, Access::{access})"
        costs[int(a, 8)] = (int(n), step)
    for i in insns:
        assert costs[i.addr][0] == i.length, f"length mismatch at {i.addr:06o}"
        # The translation tells a taken branch from one not taken by the
        # block that runs next, which needs the two to differ.
        w, hb = i.words[0], i.words[0] >> 8
        conditional = 0o2 <= hb <= 0o7 or 0o200 <= hb <= 0o207
        assert not (conditional and w & 0o377 == 0), f"branch to the next instruction at {i.addr:06o}"

    label_at = {}
    for name, a in labels.items():
        if a in at and name not in REGISTER_ALIASES:
            label_at.setdefault(a, []).append(name)

    def transfers(i):
        if i.flow in ("branch", "jump", "call", "return", "stop"):
            return True
        if i.mnemonic in ("WAIT", "BR"):
            return True
        if i.operands and i.operands[-1].text == "PC" and i.mnemonic.split()[0] in WRITES_DST:
            return True
        return False

    def touches_io(i):
        if i.mnemonic == "RESET":
            return True
        return any(o.kind == "abs" and o.value >= 0o160000 for o in i.operands)

    # Split into blocks.
    starts = set(label_at)
    prev = None
    for i in insns:
        if prev is not None and (transfers(prev) or prev.addr + 2 * prev.length != i.addr):
            starts.add(i.addr)
        if touches_io(i):
            starts.add(i.addr)
        prev = i
    blocks = []
    cur = None
    for i in insns:
        if i.addr in starts:
            cur = {"addr": i.addr, "insns": []}
            blocks.append(cur)
        if cur is None:
            continue
        cur["insns"].append(i)
        if transfers(i):
            cur = None
    # Names: the label, or the last label and a count.
    last, k = None, 0
    names = set()
    for b in blocks:
        if b["addr"] in label_at:
            names_here = sorted(label_at[b["addr"]])
            if own_label.get(b["addr"]) in names_here:
                names_here.remove(own_label[b["addr"]])
                names_here.insert(0, own_label[b["addr"]])
            label_at[b["addr"]] = names_here
            last = names_here[0]
            k = 0
            b["name"] = last
        else:
            k += 1
            b["name"] = f"{last}_{k}"
        assert b["name"] not in names
        names.add(b["name"])
        b["end"] = b["insns"][-1].addr + 2 * b["insns"][-1].length
        b["aliases"] = label_at.get(b["addr"], [])[1:]

    os.makedirs(OUT, exist_ok=True)
    os.makedirs(DATA, exist_ok=True)
    shutil.copyfile(LDA, os.path.join(DATA, "lunar.lda"))

    with open(os.path.join(OUT, "blocks.rs"), "w") as f:
        f.write("// Generated by crates/moonlander/gen/gen.py from reconstruction/out/lunar.lst.\n")
        f.write("// Do not edit. Instruction times: " + (timing_source or "?") + ".\n\n")
        f.write("use crate::machine::{Access, Block, Step::*};\n\n")
        for b in blocks:
            f.write("/// ```text\n")
            for i in b["insns"]:
                f.write(f"/// {src_at.get(i.addr, i.text)}\n".replace("\t", "    "))
            f.write("/// ```\n")
            cost = [costs[i.addr][1] for i in b["insns"]]
            f.write(f"pub const {b['name']}: Block = Block {{ addr: 0o{b['addr']:06o}, end: 0o{b['end']:06o}, "
                    f"n: {len(b['insns'])}, name: \"{b['name']}\", cost: &[{', '.join(cost)}] }};\n")
            for al in b["aliases"]:
                f.write(f"pub const {al}: Block = {b['name']};\n")
        f.write(f"\n/// Interrupt entry: stacking the PS and PC and loading the vector.\n"
                f"pub const INTERRUPT: f64 = {interrupt};\n")
        f.write(f"/// An absolute-mode TST that takes a bus error: the instruction's time,\n"
                f"/// then the trap's.\n"
                f"pub const FAULT3: &[f64] = &[{fault3[0]}, {fault3[1]}];\n")
        f.write("\n/// Every block, in address order.\npub static ALL: &[Block] = &[\n")
        for b in blocks:
            f.write(f"    {b['name']},\n")
        f.write("];\n")

    syms = {}
    for name, v in constants.items():
        if name == "." or name in aliases or "$" in name:
            continue
        syms[name] = v
    for name, v in labels.items():
        if name in REGISTER_ALIASES:
            continue
        if name in syms and syms[name] != v:
            raise SystemExit(f"{name}: label {v:o} and symbol table {syms[name]:o} disagree")
        syms[name] = v
    with open(os.path.join(OUT, "symbols.rs"), "w") as f:
        f.write("// Generated by crates/moonlander/gen/gen.py from reconstruction/out/lunar.lst.\n")
        f.write("// Do not edit. Every label and assembly-time constant of the original.\n\n")
        for name in sorted(syms):
            f.write(f"pub const {name}: u16 = 0o{syms[name] & 0xFFFF:06o};\n")
        f.write(f"\n/// The start address in the image's transfer block.\npub const TRANSFER: u16 = 0o{start:06o};\n")

    print(f"{len(insns)} instructions, {len(blocks)} blocks, {len(syms)} symbols")


if __name__ == "__main__":
    main()
