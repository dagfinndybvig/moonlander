"""Build GT40 Forth: assemble forth.a11 into forth.lda.

Usage: python build-forth.py [SOURCE] [OUT]
"""

import sys, re
from asm11 import Asm

def strip_comment(line):
    """Split on ';' outside double quotes (word names like ";" are quoted)."""
    out, q = [], False
    for ch in line:
        if ch == '"':
            q = not q
        if ch == ";" and not q:
            break
        out.append(ch)
    return "".join(out)

def parse_operands(s):
    """Split on commas that are not inside quotes."""
    out, cur, q = [], "", False
    for ch in s:
        if ch == "'" or ch == '"':
            q = not q
        if ch == "," and not q:
            out.append(cur.strip()); cur = ""
        else:
            cur += ch
    if cur.strip(): out.append(cur.strip())
    return out

def assemble_source(a: Asm, text, listing=None):
    for raw in text.splitlines():
        line = strip_comment(raw).rstrip()
        if not line.strip():
            continue
        if listing is not None:
            listing.append((a.here(), raw.rstrip()))
        # label(s) at line start
        while True:
            m = re.match(r"^(\w+):\s*(.*)$", line)
            if not m: break
            a.label(m.group(1))
            line = m.group(2)
        if not line.strip():
            continue
        # constant
        m = re.match(r"^(\w+)\s*=\s*(.+)$", line)
        if m:
            a.equ(m.group(1), a.value(m.group(2)))
            continue
        parts = line.split(None, 1)
        op = parts[0].upper()
        rest = parts[1] if len(parts) > 1 else ""
        if op == ".ORG":
            a.org(a.value(rest))
        elif op == ".EVEN":
            a.even()
        elif op == ".WORD":
            for e in parse_operands(rest):
                if e.startswith("\""):
                    continue
                a.emit_word(0)
                a.fix(a.here() - 2, "w", e)
        elif op == ".ASCII":
            s = rest.strip().strip('"')
            for ch in s:
                a.emit_byte(ord(ch))
            a.even()
        elif op == ".BYTE":
            for e in parse_operands(rest):
                a.emit_byte(a.value(e))
            a.even()
        elif op == ".BLKW":
            a.blk(a.value(rest))
        elif op == ".HEAD":
            # .HEAD "NAME",CODELABEL[,FLAGS]  -- dictionary header:
            #   link | len | flags | chars | pad | cfa
            args = parse_operands(rest)
            name = args[0].strip().strip('"')
            code = args[1].strip()
            flags = a.value(args[2]) if len(args) > 2 else 0
            a.even()
            head = a.here()
            a.emit_word(a.labels.get("LASTHD", 0))  # link to the previous header
            a.emit_byte(len(name))
            a.emit_byte(flags)
            for ch in name:
                a.emit_byte(ord(ch))
            a.even()
            a.emit_word(0); a.fix(a.here() - 2, "w", code)
            a.equ("LASTHD", head)
        else:
            a.instr(op, parse_operands(rest))

def main():
    src = sys.argv[1] if len(sys.argv) > 1 else "forth.a11"
    out = sys.argv[2] if len(sys.argv) > 2 else "forth.lda"
    a = Asm()
    listing = []
    assemble_source(a, open(src).read(), listing)
    a.resolve()
    if a.errors:
        for e in a.errors: print("ERROR:", e)
        sys.exit(1)
    start = a.labels.get("START", 0o1000)
    a.write(out, start)
    print(f"assembled {len(a.mem)} words -> {out}, start {start:06o}")
    hi = max(a.mem)
    print(f"memory used: 0{a.cur:05o} bytes of code/data, top address {hi:06o}")
    if "--list" in sys.argv:
        lst = sys.argv[sys.argv.index("--list") + 1]
        with open(lst, "w") as f:
            for addr, text in listing:
                f.write(f"{addr:06o}  {text}\n")

if __name__ == "__main__":
    main()
