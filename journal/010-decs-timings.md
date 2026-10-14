# 010 DEC's timings

Date: 2026-10-14

Until now the processor's instruction times came from a first-order model of my own: a cost per memory reference plus a little per instruction class. Entries 005 and 008 flagged it as provisional. The PDP-11/05, 11/10 Computer Manual (DEC-11-H05AA-A-D) has now replaced it. This entry records what the manual gives, the readings where it leaves a choice, what had to change in the translation to charge the new times exactly, and what the change does to the game.

## The source

The manual arrived as a 336-page scan with no text layer. Its PDF names its producer as `tumble`, Eric Smith's scanning tool. The pages were run through OCR to find the timing material, and every number below was then read off the page images, not the OCR.

The timing is in Part II, section 2.2.2 and Tables 2-2 to 2-7:

> The execution time for an instruction is the sum of a basic instruction time and the time to determine and fetch the source and/or destination operands. [...] All times stated are subject to ±10% variation.

The tables are for the KD11-B with MM11-K/L core. The GT40 user's guide gives its memory the same figures (900 ns cycle, 400 ns access), so they apply as printed. The model uses the stated values, not the ±10%.

**Operand times (Table 2-2), µs**

| Mode | | Source | Destination |
|---|---|---|---|
| 0 | register | 0 | 0 |
| 1 | register deferred | 0.9 | 2.4 |
| 2 | autoincrement | 0.9 | 2.4 |
| 3 | autoincrement deferred | 2.4 | 3.4 |
| 4 | autodecrement | 0.9 | 2.4 |
| 5 | autodecrement deferred | 2.4 | 3.4 |
| 6 | indexed | 2.4 | 3.4 |
| 7 | indexed deferred | 3.4 | 4.7 |

The notes under the table: add 1.3 µs for a source byte at an odd address; add 1.3 µs for an odd destination byte with a non-modifying instruction and 2.4 µs with a modifying one; subtract 1.2 µs from the destination time for a non-modifying instruction.

**Basic times (Tables 2-3 to 2-7), µs**

| Instructions | Basic time |
|---|---|
| CLR COM INC DEC NEG ADC SBC TST ROR ROL ASR ASL (and byte forms) | 3.4 |
| SWAB | 4.3 |
| MOV, MOVB | 3.7, or 3.1 "mode 0" |
| CMP BIT BIC BIS ADD SUB (and byte forms) | 3.7 |
| JMP | 1.0 |
| JSR, RTS | 3.8 |
| conditional branches | 1.9 not taken, 2.5 taken |
| BR | 2.5 |
| EMT TRAP IOT BPT | 8.2 |
| RTI | 4.4 |
| HALT, WAIT | 1.8 |
| RESET | 20 ms |
| condition code operators, NOP | 2.5 |

Section 4.7.2.2 says which instructions are non-modifying, where it explains when the processor reads a destination with DATIP, the read that holds the memory for a write back: "The instructions TST, CMP, BIT, JMP, and JSR can never result in the modification of the destination by the processor." MOV is not on the list. The KD11-B reads a MOV's destination before writing it, as it does for ADD.

## Readings

The manual leaves five things open. Each choice is marked READING in `crates/kd11b/src/timing.rs`.

- **JMP and JSR.** Their tables give a basic time only. The model adds the destination time less 1.2 µs, treating them like the other instructions on the non-modifying list. JMP @#a comes to 3.2 µs and JSR PC,@#a to 6.0.
- **MOV's "mode 0".** Read as a register destination. MOV R0,R1 takes 3.1 µs, MOV #n,R0 4.0, and MOV to memory 3.7 plus both operand times.
- **Interrupt entry.** Not given. It is taken as the 8.2 µs of a trap instruction, because section 5.5.2 has interrupts and traps join the same microroutine (ET-2), which stacks the PS and PC and loads the new pair. Both reach it after one bus transfer: the instruction fetch for a trap, the vector read for an interrupt.
- **Bus error traps.** The faulting instruction's own time plus 8.2 µs. The reserved instruction trap, and JMP or JSR to a register, take 8.2 µs.
- **RESET.** The table's 20 ms, which is the length of the INIT pulse.

The cycle-by-cycle answer is in the microprogram listing, but that is in DEC's print set (K-MP-KD11-B), not this manual. Chapter 5 shows only excerpts.

Table 2-8 lists the 11/05's differences from the 11/20. The emulator follows the 11/05 column, because it was checked against simh configured as an 11/05 (entry 006). None of the differences touches this program, though:

- **Shared registers.** No instruction uses the same register as its source and as its autoincremented or autodecremented destination.
- **MOV PC.** Its two `MOV PC` instructions, both in `ASCII`, use no extension word, so they store the same address on either machine.
- **SWAB.** Its three `SWAB`s, whose V bit differs between the machines, are each followed by a `ROR`, which sets V without reading it.

## Times that depend on what happened

The provisional model charged the same time for an instruction however it went. DEC's figures do not, in two places:

- **A conditional branch** takes 1.9 µs if it falls through and 2.5 µs if it branches. The program has 232 of them.
- **A byte operand at an odd address** costs 1.3 or 2.4 µs more. The program has 22 byte instructions with operands in memory: the ship and flame tables in `DRAWRT`, `FLAME`, `DUST` and `EXGEN`, the feature nibbles in `PUTFET` and `GETFET` (two features to a byte, so `GETFET`'s `MOVB` below reads an odd address for half of them), the digit strings in `ASCII` and `SASCII`, and `TSTB XCSR`.

The translation charges time per block, from generated tables, so it needs a way to charge what happened. Tables in `blocks.rs` now give each instruction one of three kinds of step. A block of `GETFET`, which picks a terrain feature out of a table of nibbles:

```
GETFET: MOV     TEMP,-(SP)              ;SAVE A SCRATCH REGISTER NOW.
        MOV     4(SP),TEMP              ;PICK UP THE INDEX NOW.
        ASR     TEMP                    ;BYTE ADDRESS IT.
        MOVB    FEATUR(TEMP),TEMP       ;PICK UP THE FEATUR NOW.
        BCC     GETDUN                  ;IF RIGHT BYTE, NO SHIFTING IS NECESSARY.
```

```rust
pub const GETFET: Block = Block { ..., cost: &[T(6.1), T(5.5), T(3.4),
    Y([5.5, 6.8, 7.9, 9.2], true, Access::None), B(1.9, 2.5)] };
```

- `T(t)` is a fixed time.
- `B(not_taken, taken)` is a branch. A conditional branch always ends its block, so the next block to run settles it: the branch was not taken if that block starts where the branch's block ends.
- `Y(times, src, dst)` is a byte instruction with an operand in memory. `times` holds the four combinations of odd source and odd destination. The translation's byte reads and writes are logged as it makes them, and when the block is charged the log is matched against what each byte instruction of the original does: whether it reads a source, and whether it reads, writes, or reads and writes back its destination. The addresses in the log give the variant.

If the log does not match the original's instructions, the translation stops with the block's name. Any byte access in the wrong place, or one too many or too few, is caught on the spot. In a flight to the McDonald's, all four variants come up: 271,431 times even, 185,612 with an odd source, 65,509 with an odd destination, and 22,630 with both.

Every time still comes from one function, `Timing::instruction`. The emulator calls it as it executes; the generator calls it for every variant. Both sides add the same `f64` values in the same order, so machine time is still compared exactly.

## The checks, again

Entry 008 expected the tests to "rerun unchanged". They did not, quite.

- **Routine tests.** All 11 routine tests and 4 arithmetic tests pass, with exact time, as they stand.
- **Lockstep.** The emulator and the translation still agree at every block of every flight. But three flights now end differently, because the autopilot is flying a ship whose physics steps are larger:
  - The flight meant to tip over by sliding sideways at 15 ft/s now comes down on a rock. At 20 ft/s it tips over as intended.
  - The one meant to tip over on its tilt at +25 degrees now has too much sideways speed first. At -25 degrees it gets the tilt message.
  - The landing at terrain index 582, meant for ground too steep to stand on, now ends on a rock. Index 581 gives the rough-ground message.
- **Coverage.** The rock landings had reached `AHROCK` for the first time, so a flight that comes down on a rock at index 583 was added to keep it covered.

There are now 26 flights, and **511,650,217 blocks ran in lockstep with no difference**. Coverage is as before: the flights reach 590 of the 608 blocks and the routine tests 5 more.

## What it does to the game

`crates/moonlander/examples/pace.rs` measures how fast the program runs. It holds the pen on the throttle bar, then on an arrow, then leaves the ship alone.

| | Provisional model | DEC's tables |
|---|---|---|
| physics steps a second, ship left alone, overview | 12.1 | 10.5 |
| clock ticks per step, the same | 4.98 | 5.68 |
| LPBARH runs a second, pen on the throttle | 2,317 | 2,254 |
| ARROW actions a second, pen on an arrow | 39.6 | 41.8 |

In a flight to the McDonald's, with the autopilot's pen at work all the way down, the ticks per step went from 6.9 to 7.6 under the provisional model to 7.8 to 8.8 under DEC's. By DEC's figures the 11/05 runs this program about 15% slower than my estimate did, so each physics step covers more time and moves the ship further.

Two consequences:

- **The coasting band narrows.** Entry 008 found that `WHERE` drops the remainder when it updates velocities, so a net acceleration under 6/CLOCK ft/s² never changes the speed. At 5 ticks that was 1.2 ft/s². The ship left alone now runs at 5 and 6 ticks, so the band is 1.0 to 1.2 ft/s², and while flying it is nearer 0.7.
- **Entry 005's observation still stands, on firmer ground.** The 1973 listing has a loop, removed in the Mare Assabet build, that makes the program wait for at least 5 ticks "FOR THE 40 AND 45", the faster PDP-11s. With DEC's own times, the GT40 left alone runs at 5 or 6 ticks a step. Burness's floor sits just under the GT40's natural pace, which is what you would choose to make a faster machine play like a GT40.

## Limits

- **±10%.** The manual gives every time to ±10%. The model has no variation. A real GT40 would differ from it, and from another GT40, by that much.
- **The readings above.** They decide 8.2 µs per interrupt, and the clock and display interrupts are frequent. If the print set turns up, the microprogram listing would settle them.
- **Interrupts at block starts.** The equivalence still holds under entry 008's rule that interrupts are taken at block starts. The emulator in its ordinary mode, as `--original` runs it, takes them between any two instructions, as the 11/05 did.
