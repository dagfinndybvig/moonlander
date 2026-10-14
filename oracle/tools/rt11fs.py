"""Minimal read-only RT-11 file system reader.

RT-11 volumes use 512-byte blocks. The directory starts at the block named by
the home block (block 1, word at byte 0724), normally block 6, and is a chain
of two-block segments. Each segment has a five-word header (segments total,
next segment, highest segment in use, extra bytes per entry, first data block
of the segment) followed by entries of 7 words plus the extra bytes:
status, filename (two RAD50 words), extension (one RAD50 word), length in
blocks, job/channel, creation date. Status bits: 0400 tentative, 01000 empty,
02000 permanent, 04000 end of segment.

Usage:
  rt11fs.py IMAGE                list files
  rt11fs.py IMAGE NAME.EXT OUT   extract one file
"""

import sys

RAD50 = " ABCDEFGHIJKLMNOPQRSTUVWXYZ$.%0123456789"


def rad50(w):
    return RAD50[w // 1600] + RAD50[(w // 40) % 40] + RAD50[w % 40]


def word(b, o):
    return b[o] | (b[o + 1] << 8)


def rt11_date(d):
    if d == 0:
        return ""
    year = (d & 0o37) + 1972 + ((d >> 14) & 3) * 32
    day = (d >> 5) & 0o37
    month = (d >> 10) & 0o17
    return f"{year:04d}-{month:02d}-{day:02d}"


def listing(img):
    home = img[512:1024]
    dirblk = word(home, 0o724) or 6
    seg = 1
    out = []
    while seg:
        off = (dirblk + (seg - 1) * 2) * 512
        s = img[off:off + 1024]
        extra = word(s, 6)
        block = word(s, 8)
        nxt = word(s, 2)
        p = 10
        while p + 14 <= len(s):
            st = word(s, p)
            if st & 0o4000:
                break
            name = rad50(word(s, p + 2)) + rad50(word(s, p + 4))
            ext = rad50(word(s, p + 6))
            length = word(s, p + 8)
            date = word(s, p + 12)
            if st & 0o2000:
                out.append((name.strip() + "." + ext.strip(), block, length, rt11_date(date)))
            block += length
            p += 14 + extra
        seg = nxt
    return out


if __name__ == "__main__":
    img = open(sys.argv[1], "rb").read()
    files = listing(img)
    if len(sys.argv) == 2:
        for name, start, length, date in files:
            print(f"{name:12s} block {start:6d} len {length:5d}  {date}")
    else:
        want = sys.argv[2].upper()
        for name, start, length, date in files:
            if name == want:
                open(sys.argv[3], "wb").write(img[start * 512:(start + length) * 512])
                print(f"extracted {name}: {length} blocks, dated {date}")
                break
        else:
            sys.exit(f"{want} not found")
