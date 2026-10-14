# 006 Checking the processor against an independent implementation

Date: 2026-10-14

The emulator in entry 005 ran the game correctly, which is good evidence but not a test. This entry compares the KD11-B model, instruction by instruction, with simh's PDP-11 configured as an 11/05: an independent implementation by other people.

## Method

`oracle/cputest/gen.py` generates random programs from the 11/05 instruction set: every double- and single-operand instruction in word and byte form, all eight addressing modes on R0-R5, immediate, absolute and PC-relative operands, forward branches on every condition, condition-code instructions, and EMT, TRAP, IOT and BPT. Registers start inside a scratch area whose words are mostly addresses within itself, so deferred modes usually land somewhere legal and arithmetic on that data eventually produces odd and non-existent addresses, exercising bus errors. Every trap vector points at its own HALT.

Both emulators run each program from identical state on a GT40-sized machine (8K words) with no display processor and no host line, so nothing but the program can change memory. simh is stripped of every peripheral its default 11/05 configuration adds and its memory is cleared before each case. Final registers, PSW, halt address, scratch area and stack are compared. `oracle/cputest/lockstep.py` finds the first instruction at which a case diverges.

## Result

9,000 programs over three seeds (20261014, 7, 1973):

| Outcome | Count |
|---|---|
| identical in every register, flag and compared memory word | 8,698 |
| differ only in the PSW stacked by a bus error | 242 |
| touched 177700-177717 | 8 |
| touched another I/O address where simh has a device and a GT40 has none | 52 |
| unexplained | 0 |

The three explained categories:

- **Stacked PSW after a bus error.** When an operand access faults partway through an instruction, simh has sometimes already applied that instruction's condition codes (for example `CLR` sets Z before its write faults) and stacks those; this model stacks the condition codes from before the instruction. Which a real KD11-B does depends on its microcode order, which the documents in hand do not give. Not changed to match simh: copying another emulator's choice is not evidence.
- **177700-177717.** simh lets an 11/05 program reach the general registers at these addresses (a `BIC` on 177700 rewrote R0). This model treats them as non-existent. Model-specific and unverified either way.
- **Other I/O page addresses.** Configuration, not semantics.

None of these can affect Moonlander: its only bus error is the start-up probe of the serial line, whose handler discards the stacked PSW unread, and it touches no I/O addresses outside the GT40's devices.

## Problems found in my own harness along the way

Each was found by a disagreement and fixed before the result above:

1. simh kept memory between cases while the model started from zero; a program reading an unloaded address saw a previous case's data in simh.
2. `set cpu 28k` is not a valid simh command; simh had silently stayed at 56 KB. Both sides now run at 16 KB, the GT40's size.
3. simh's default 11/05 carries a dozen peripherals a GT40 never had.
4. simh services a trap at the start of the following step, so lock-step traces must be re-aligned after traps.

## Limits

- The category rules are applied by pattern; the first cause was traced by hand for a sample of cases in each category, not for all of them.
- Interrupt timing, WAIT, RESET and the devices are not covered here; the game exercises them, and the translation will be checked against the running original.
