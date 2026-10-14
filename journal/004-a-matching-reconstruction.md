# 004 A matching reconstruction

Date: 2026-10-14

## Result

`reconstruction/lunar-mare-assabet.mac` assembles to exactly the memory image of the surviving Mare Assabet binary: the same 12,235 bytes loaded at the same addresses, with the same contents and the same start address (032340).

```
$ MACRO11=macro11 reconstruction/build.sh
candidate: 12235 bytes loaded, start 032340
target:    12235 bytes loaded, start 032340

EXACT MATCH
```

The file is Burness's LEM.090 listing with the changes from entry 003 applied. 138 lines are marked `;RECON`; a whitespace-normalised diff against the listing touches 249 lines, most of them the new start-up code and greeting. It matched on the first assembly.

## Checks on the result

A first-time exact match is a reason for suspicion, so:

- **Negative control.** Changing a single character ("Mare Assabet," to "Mare Assabet.") produces a one-byte mismatch at 033246, reported as such. The comparison is not vacuous.
- **Independent linker.** The object was also converted with Don North's `obj2bin.pl` from the macro11 distribution instead of this project's `link.py`. Every one of the binary's 12,235 bytes is identical. That tool additionally zero-fills the unused gaps (034-077, 262-317, the moon buffer, one padding byte), so its file loads more addresses than the original; the original was written by a tool that, like `link.py`, emits only what the assembler produced.
- **The comparison is exit-status gated**, so the build script fails on any difference.

## What the binary determines, and what it does not

The binary fixes every assembled word. It does not fix spelling. Where the reconstruction had to choose:

| Choice | Constraint from the binary | Reconstruction |
|---|---|---|
| Names of new labels | none | `TRAP04`..`TRAP30`, `NOLINE`, `LINEOK`, `XWAIT`, `RESTL2` |
| Which clearing loop is `RESTAR` | `DELAY2` jumps to the restart at 000334, the `RESET` | `RESTAR` is the `RESET` and moon loop; the buffer loop is new |
| Top of stack | 037776; no word is loaded there | `STACK=37776` as a symbol, replacing the listing's reserved `STACK: .WORD` |
| Display-subroutine stack | top at 000316, nothing loaded 000262-000317 | `.=316`, `STACKD:`, `.=320` |
| Comments | none | Burness's comments kept; new code commented in his register |

The source text is therefore one of many that produce this binary. The claim is about the machine code, not the keystrokes.

## The loader file itself

Reproducing the file, not just the memory it loads, would need the original tool's packing rules. What the file shows:

- 24 zero bytes of leader, then blocks separated by 6 zero bytes, then a start block; the file is padded to 27 blocks of 512 bytes, matching the length in the RT-11 directory. The padding is zero except one 377 byte.
- Blocks follow the order the assembler emitted bytes, not address order. Each trap vector is its own 4-byte block, and the display vectors at 000320 come before the variables at 000104, so the original source set up all vectors first, each with its own location assignment.
- Every `.=.-1` after a message and the `.=.-2` at `MACON` appears as a block starting one or two bytes before the previous one ended.
- Long runs are split at 350 bytes, but contiguous messages are also split where no address discontinuity exists, so the writer flushed at points that cannot be recovered from the binary.

Not reproduced. The memory image is what the GT40 runs; the container's rules belong to a tool we do not have. Given the start-up logs off a PDP-10, a PDP-10-hosted cross-assembler is plausible, and unproven.

## Two observations about the reconstructed code

- **The start-up never restores the bus-error vector.** It points vector 4 at `NOLINE` (032506) to survive probing a missing serial line, and leaves it there. After the restart clears the buffers, 032506 lies in the second ship buffer. Until a ship is drawn there it holds a zero word, so a bus error halts, by accident much as the trap catcher intended; once the buffer holds a ship, a bus error would run display data as instructions.
- **Address 000000 holds `JMP @#RESTAR`.** The convention of restarting a PDP-11 program by starting it at location 0 works here, where in the listing location 0 is never loaded.
