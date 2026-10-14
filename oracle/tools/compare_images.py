"""Compare two absolute loader images byte for byte.

A matching reconstruction must load exactly the same set of addresses with
exactly the same contents, and give the same start address. Differences are
reported as contiguous runs, with the nearest preceding label from an
optional macro11 listing of the candidate.

Usage: compare_images.py CANDIDATE.lda TARGET.lda [CANDIDATE.lst]
Exit status 0 only for an exact match.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lda  # noqa: E402
import listing  # noqa: E402


def runs(addrs):
    out = []
    for a in sorted(addrs):
        if out and out[-1][1] == a:
            out[-1][1] = a + 1
        else:
            out.append([a, a + 1])
    return out


def main():
    cand, _, cstart = lda.read_lda(sys.argv[1])
    targ, _, tstart = lda.read_lda(sys.argv[2])
    names = {}
    if len(sys.argv) > 3:
        _, labels, _ = listing.parse(sys.argv[3], base=0)
        names = sorted((v, k) for k, v in labels.items() if v is not None and ":" not in k)

    def where(a):
        best = None
        for v, k in names:
            if v <= a:
                best = (k, a - v)
            else:
                break
        return f"{best[0]}+{best[1]:o}" if best else ""

    only_c = set(cand) - set(targ)
    only_t = set(targ) - set(cand)
    diff = {a for a in set(cand) & set(targ) if cand[a] != targ[a]}
    print(f"candidate: {len(cand)} bytes loaded, start {cstart:06o}")
    print(f"target:    {len(targ)} bytes loaded, start {tstart:06o}")
    ok = not only_c and not only_t and not diff and cstart == tstart
    if cstart != tstart:
        print(f"START ADDRESS DIFFERS: {cstart:06o} vs {tstart:06o}")
    for title, s in (("loaded only by candidate", only_c), ("loaded only by target", only_t),
                     ("different contents", diff)):
        if s:
            print(f"\n{title}: {len(s)} bytes")
            for lo, hi in runs(s)[:40]:
                extra = ""
                if title == "different contents":
                    extra = "  cand " + " ".join(f"{cand[a]:03o}" for a in range(lo, min(hi, lo + 8)))
                    extra += "  targ " + " ".join(f"{targ[a]:03o}" for a in range(lo, min(hi, lo + 8)))
                print(f"  {lo:06o}-{hi - 1:06o} ({hi - lo:4d})  {where(lo):14s}{extra}")
    print("\nEXACT MATCH" if ok else "\nno match")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
