"""Parse a macro11 listing into source lines, emitted words and symbols.

Listing layout (fixed columns): line number in columns 0-7, location in 9-14,
up to three assembled words (or bytes) in 16-39, source text from column 40.
A trailing apostrophe on a word means it was relocated by the linker. Lines
with a blank line-number field continue the previous source line's output.
"""

import re
from dataclasses import dataclass, field

ITEM = re.compile(r"([0-7]{3,6})('?)")


@dataclass
class Line:
    lineno: int
    addr: int = None
    items: list = field(default_factory=list)  # (value, nbytes, relocated)
    source: str = ""

    @property
    def label(self):
        m = re.match(r"\s*([A-Z0-9$.]+):", self.source)
        return m.group(1) if m else None

    @property
    def statement(self):
        """Source text without label or comment."""
        s = self.source.split(";", 1)[0]
        s = re.sub(r"^\s*[A-Z0-9$.]+:\s*", "", s)
        return s.strip()

    @property
    def nbytes(self):
        return sum(n for _, n, _ in self.items)


def parse(path, base=0):
    """Return (lines, labels, constants). Addresses are relocated by base."""
    lines = []
    labels = {}
    constants = {}
    text = open(path, encoding="latin-1").read().split("\n")
    in_symtab = False
    scope = None
    for raw in text:
        raw = raw.replace("\f", "")
        if raw.startswith("Symbol table"):
            in_symtab = True
            continue
        if raw.startswith("Program sections"):
            break
        if in_symtab:
            for k in range(0, len(raw), 25):
                chunk = raw[k:k + 25]
                m = re.match(r"(\S+)\s*=\s*%?([0-7]{6})", chunk)
                if m:
                    constants[m.group(1)] = int(m.group(2), 8)
            continue
        if len(raw) < 9:
            continue
        lnfield = raw[0:8].strip()
        loc = raw[9:15].strip()
        words = raw[16:40]
        src = raw[40:]
        if lnfield.isdigit():
            ln = Line(int(lnfield), source=src)
            lines.append(ln)
        elif loc and lines:
            ln = lines[-1]
        else:
            continue
        if loc and re.fullmatch(r"[0-7]{6}", loc):
            if ln.addr is None:
                ln.addr = int(loc, 8) + base
            for m in ITEM.finditer(words):
                v = m.group(1)
                ln.items.append((int(v, 8), 2 if len(v) == 6 else 1, m.group(2) == "'"))
        lab = ln.label if lnfield.isdigit() else None
        if lab:
            if lab[0].isdigit():          # local label, scoped to last global
                lab = f"{scope}:{lab}"
            else:
                scope = lab
            # a label on a line with no output takes the next location
            labels[lab] = ln
    # resolve label addresses: a label line without a location gets the
    # address of the next line that has one
    resolved = {}
    for name, ln in labels.items():
        if ln.addr is not None:
            resolved[name] = ln.addr
        else:
            i = lines.index(ln)
            nxt = next((l.addr for l in lines[i:] if l.addr is not None), None)
            resolved[name] = nxt
    return lines, resolved, constants
