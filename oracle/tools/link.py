#!/usr/bin/env python3
"""Tiny RT-11 object linker for a single-module, single-psect MACRO-11 object.

Does what RT-11 LINK does for gtlem-1973.obj: places the unnamed relocatable
program section at a base address (default 001000, RT-11 LINK's default),
applies the relocation directory (RLD) entries, and writes:

  * an absolute loader image (.lda, paper-tape "absolute binary" format),
  * a raw memory image (.bin, 64 KiB minus the I/O page, little endian words),
  * a JSON summary.

Only the RLD entry types that MACRO-11 emits for a self-contained module are
supported: internal (01), internal displaced (03), location counter
definition (07) and modification (010). Anything else aborts.

Usage: link.py OBJ --base 1000 --lda OUT.lda --bin OUT.bin
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lda  # noqa: E402


def read_formatted_binary(path):
    """Yield the data bytes of each formatted-binary block in an RT-11 .OBJ."""
    data = open(path, "rb").read()
    i = 0
    while i < len(data):
        if data[i] == 0:
            i += 1
            continue
        if data[i] != 1 or data[i + 1] != 0:
            raise ValueError("bad block header at %d" % i)
        n = data[i + 2] | (data[i + 3] << 8)
        block = data[i:i + n]
        chk = data[i + n]
        if (sum(block) + chk) & 0xFF:
            raise ValueError("bad checksum at %d" % i)
        yield block[4:]
        i += n + 1


def w(b, o):
    return b[o] | (b[o + 1] << 8)


def link(objpath, base):
    mem = {}            # absolute byte address -> value
    xfer = None
    psect_len = None
    last_txt = None     # (load_addr_rel, bytearray) of the preceding TXT record
    loc_base = 0        # current location counter base (relative)
    for rec in read_formatted_binary(objpath):
        rtype = w(rec, 0)
        body = rec[2:]
        if rtype == 1:  # GSD
            for k in range(0, len(body), 8):
                ent = body[k:k + 8]
                etype = ent[5]
                val = w(ent, 6)
                if etype == 3:      # transfer address
                    xfer = val
                elif etype == 5:    # psect
                    if ent[4] & 0o40:   # REL
                        psect_len = val
        elif rtype == 2:    # ENDGSD
            pass
        elif rtype == 3:    # TXT
            if last_txt is not None:
                flush(mem, base, last_txt)
            addr = w(body, 0)
            last_txt = (addr, bytearray(body[2:]))
        elif rtype == 4:    # RLD applies to the preceding TXT record
            k = 0
            while k < len(body):
                cmd = body[k] & 0x7F
                is_byte = body[k] & 0x80
                disp = body[k + 1]
                if is_byte:
                    raise ValueError("byte relocation not supported")
                if cmd in (1, 3):
                    const = w(body, k + 2)
                    off = disp - 4
                    t_addr, t_data = last_txt
                    loc = base + t_addr + off
                    if cmd == 1:
                        v = (const + base) & 0xFFFF
                    else:
                        v = (const - (loc + 2)) & 0xFFFF
                    t_data[off] = v & 0xFF
                    t_data[off + 1] = v >> 8
                    k += 4
                elif cmd == 7:      # location counter definition: name(4) + const(2)
                    loc_base = w(body, k + 6)
                    k += 8
                elif cmd == 8:      # location counter modification
                    loc_base = w(body, k + 2)
                    k += 4
                else:
                    raise ValueError("unsupported RLD type %o" % cmd)
        elif rtype == 6:    # ENDMOD
            if last_txt is not None:
                flush(mem, base, last_txt)
                last_txt = None
        else:
            raise ValueError("unsupported record type %d" % rtype)
    if last_txt is not None:
        flush(mem, base, last_txt)
    return mem, (xfer + base) & 0xFFFF, psect_len


def flush(mem, base, txt):
    addr, data = txt
    for i, b in enumerate(data):
        mem[(base + addr + i) & 0xFFFF] = b


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("obj")
    ap.add_argument("--base", default="1000", help="octal load base (default 1000)")
    ap.add_argument("--lda")
    ap.add_argument("--bin")
    ap.add_argument("--json")
    a = ap.parse_args()
    base = int(a.base, 8)
    mem, start, plen = link(a.obj, base)
    lo, hi = min(mem), max(mem)
    print("loaded bytes %d, range %06o-%06o, psect length %06o, start %06o"
          % (len(mem), lo, hi, plen, start))
    if a.lda:
        lda.write_lda(a.lda, mem, start)
    if a.bin:
        img = bytearray(0o160000)
        for k, v in mem.items():
            img[k] = v
        open(a.bin, "wb").write(bytes(img))
    if a.json:
        json.dump({"base_octal": "%06o" % base, "start_octal": "%06o" % start,
                   "start": start, "psect_length_octal": "%06o" % plen,
                   "low_octal": "%06o" % lo, "high_octal": "%06o" % hi,
                   "loaded_bytes": len(mem)}, open(a.json, "w"), indent=1)


if __name__ == "__main__":
    main()
