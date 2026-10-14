"""Read and write DEC absolute loader (.LDA / .BIN) images.

Format (DEC paper-tape absolute loader): a sequence of blocks, each
  0x01 0x00  len_lo len_hi  addr_lo addr_hi  data...  checksum
where len counts the 6 header bytes plus data, and the checksum makes the
byte sum of the block (header, data, checksum) zero mod 256. A block with
len == 6 carries no data; its address is the start address (odd means
"halt, do not start"). Leading zero bytes (tape leader) are skipped.
"""

import sys


def read_lda(path):
    data = open(path, "rb").read()
    mem = {}
    blocks = []
    start = None
    i = 0
    while i < len(data):
        if data[i] == 0:
            i += 1
            continue
        if data[i] != 1 or data[i + 1] != 0:
            raise ValueError(f"bad block header at byte {i}: {data[i]:#x} {data[i+1]:#x}")
        n = data[i + 2] | (data[i + 3] << 8)
        addr = data[i + 4] | (data[i + 5] << 8)
        payload = data[i + 6:i + n]
        csum = data[i + n]
        if (sum(data[i:i + n + 1]) & 0xFF) != 0:
            raise ValueError(f"checksum error in block at byte {i}")
        if n == 6:
            start = addr
            break
        for k, b in enumerate(payload):
            mem[addr + k] = b
        blocks.append((addr, len(payload)))
        i += n + 1
    return mem, blocks, start


def write_lda(path, mem, start, chunk=64):
    out = bytearray(b"\0" * 8)
    addrs = sorted(mem)
    runs = []
    for a in addrs:
        if runs and runs[-1][1] == a and runs[-1][1] - runs[-1][0] < chunk:
            runs[-1][1] = a + 1
        else:
            runs.append([a, a + 1])
    for lo, hi in runs:
        body = bytes([1, 0]) + (hi - lo + 6).to_bytes(2, "little") + lo.to_bytes(2, "little")
        body += bytes(mem[a] for a in range(lo, hi))
        out += body + bytes([(-sum(body)) & 0xFF])
    body = bytes([1, 0, 6, 0]) + start.to_bytes(2, "little")
    out += body + bytes([(-sum(body)) & 0xFF]) + b"\0" * 8
    open(path, "wb").write(out)


def words(mem):
    """Word view: {even_addr: value} for every word with at least one byte loaded."""
    w = {}
    for a in mem:
        e = a & ~1
        w[e] = mem.get(e, 0) | (mem.get(e + 1, 0) << 8)
    return w


if __name__ == "__main__":
    mem, blocks, start = read_lda(sys.argv[1])
    lo, hi = min(mem), max(mem)
    print(f"{len(blocks)} blocks, {len(mem)} bytes, range {lo:06o}-{hi:06o}, start {start:06o}")
    # coalesce contiguous ranges
    rng = []
    for a in sorted(mem):
        if rng and rng[-1][1] == a:
            rng[-1][1] = a + 1
        else:
            rng.append([a, a + 1])
    for lo, hi in rng:
        print(f"  {lo:06o}-{hi - 1:06o}  ({hi - lo} bytes)")
