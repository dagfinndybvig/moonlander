# 007 The ship's weight is an address

Date: 2026-10-14

Before translating, I read the whole program, routine by routine, against the matching reconstruction. This entry records the one finding that changes how the game plays, and the places where the program keeps state somewhere other than in its variables. Both decide how the translation has to be built.

## Acceleration is divided by a table address

`ROCKET` (source line 438) computes the engine's thrust, burns fuel, works out the ship's weight, and then the acceleration, which its comment gives as thrust times g over weight:

```
FUELKO: MOV     RET2,FUEL               ;AND SAVE IT AWAY.
        CLR     RET1                    ;NOW DIVIDE BY 10 TO CONVERT
        MOV     #10.,IN1                ;TENTHS OF POUNDS TO POUNDS.
        JSR     PC,DIVTWO               ;DO DIVISION NOW.
        ADD     #EMPTY,RET2             ;ADD IN EMPTY WEIGHT OF SHIP.
        MOV     RET2,WEIGHT             ;AND SAVE AWAY NOW.
        MOV     THRUST,IN1              ;FIGURE OUT ACCELERATION NOW.
        MOV     #G1,IN2
        JSR     PC,MULTWO
        MOV     TEMP,IN1                ;NOW DIVIDE BY WEIGHT.
        JSR     PC,DIVTWO
        MOV     RET2,ACCEL              ;AND SAVE THIS AWAY. A=TRS*G/W
```

The weight is in `RET2` (R3) and in `WEIGHT`. The divide uses `TEMP` (R4), which nothing in `ROCKET`, `MULTWO` or `DIVTWO` sets, and which no interrupt routine changes (`TIMER` uses no registers; `DISPLY` and `LIGHT` save and restore R0 and R1 only).

`ROCKET` has one caller, `SHOWNT`, which is reached only by falling through from `SHOWSP`, which has just drawn the ship with `DRAW`. `DRAW` walks the ship's command list with R4 (`DRAWLP: MOV (TEMP)+,PC`), so on return R4 points one word past the list's final `DRAWDN`. The next thing assembled after the ship's design is `YTHRST`, the table of flame lengths. In the Mare Assabet build that is address 023546, or 10,086.

So the ship always accelerates as if it weighed 10,086 lb. The `WEIGHT` readout a player can select is computed correctly and used for nothing.

### Checked in the running original

`crates/kd11b/examples/probe.rs` runs a program in the emulator and reports the registers each time the PC reaches an address, without stopping. On the original binary, at the divide (001572), over 90 seconds of flight:

```
pc 001572  r0 017303 r1 037327 r2 003615 r3 007705 r4 023546 r5 032452  seen 494 times
pc 001572  r0 017303 r1 037327 r2 003615 r3 007705 r4 023546 r5 032576  seen 495 times
```

R4 is 023546 every time. R2:R3 is 7,875 × 16,087, the thrust at the starting 75% throttle times `G1`; the two lines differ only in R5, which alternates between the two ship buffers. As fuel burns the registers do not change, which is the point.

### What it does to the game

Accelerations in ft/s², from the program's own constants (`MAXTRS` 10,500 lb, `G1` = 32.174 × 500, `G2` = 5.34 × 500, `EMPTY` 14,300 lb, 3,000 lb of fuel):

| | full tanks, as written | empty tanks, as written | as built |
|---|---|---|---|
| divisor (lb) | 17,300 | 14,300 | 10,086 |
| full throttle | 19.5 (3.7 lunar g) | 23.6 (4.4 lunar g) | 33.5 (6.3 lunar g) |
| starting 75% throttle | 14.6 | 17.7 | 25.1 |
| throttle to hover | 27% | 23% | 16% |

The engine is 1.4 to 1.7 times stronger than the comments intend, and does not get stronger as the tanks empty. Fuel still runs out on schedule: `ROCKET` burns it from thrust and elapsed ticks, independent of weight.

The divisor is wherever `YTHRST` happens to be assembled. All three surviving sources contain the same `MOV TEMP,IN1`. In the 1973 listing `YTHRST` is at 023550 from the start of a relocatable section, so a build of it flies as if the ship weighed 10,088 lb if loaded at 0, or 10,600 lb at RT-11's usual 001000. Any edit that moved the flame table moved the ship's weight. A translation that "fixed" this, or that laid out its data differently, would fly a different ship.

## State that lives outside the variables

The program keeps information in places a translation that only carried its named variables across would lose. Each is listed with its source line in `reconstruction/lunar-mare-assabet.mac`.

| Where | What it does | Consequence |
|---|---|---|
| `ROCKET` 472 | divides by R4, left by `DRAW` | registers are part of the state between routines |
| `MOV SP,MOON` 707, `CMP MOON,(SP)` 748, `MOV (SP),MOON` 761 | `MOON` is 0 when the far view is drawn, the return address on the stack (`EXIDLE`, 000474) when the close-up is drawn, and anything else, including the stack pointer, when neither is | return addresses must be the original addresses, on a stack in memory |
| `TST #MACON` 842, `CLR #MACTHR` 848 | two flags are the immediate operands of instructions; `MACTHR` starts as its own address and is cleared when the ship hits the McDonald's, which then stays destroyed until the program is reloaded, because restarting does not reinitialise code | the code area is data and must be kept |
| `DRAW22` 1525 | an instruction rewritten between `INC` and `DEC` by the code around it; the last setting survives a restart | as above |
| `PALSYI` 1227, 1245 | an instruction generated at run time (`INC R0` or `DEC R0`) and then called | as above |
| `EXGEN` 1449 | `MOV -6(SP),IN1` takes a word from below the stack pointer, left by an earlier call or interrupt, as a random number | the stack's dead area matters; with interrupts it is timing dependent |
| `ASCDO` 2287-2294 | the leading-zero flag is a word in the caller's frame, set non-zero with `MOV SP,2(SP)` or `MOV PC,(SP)` | stack layout must match |
| `DELAY` 2030-2046 | reads its argument from the word after the call, skips it, and either returns or jumps to `RESTART` from any depth | non-local exits |
| `DRAWMC` 1337 | `JMP @10.(SP)` returns through a frame further up the stack | as above |
| `PALMX2` 1172 | resets the stack pointer to the top of core and carries on | as above |
| `BIS #0,DSR` 462, 1423, 1434, 1440 | rings the bell: any write to the display status register does | I/O must reach the hardware model |
| `LPTAB` 2193 | light pen hits on the ship, flame, messages, dust and moon dispatch to `DSUBRT`, which would jump through whatever R0 points at. Those lists switch light pen interrupts off (`DRWSHP` starts with 114120), so it cannot happen | unreachable; the translation stops with a diagnostic if it ever does |

## What this means for the translation

- The translation keeps the original 8K-word memory image, with the same layout, initialised from the matching reconstruction. Variables, display lists, tables and the instruction words that double as data all sit where the original has them, and the display processor model reads its display lists from that memory exactly as the VT11 did.
- The six general registers are kept, under Burness's names (`IN1`, `IN2`, `RET1`, `RET2`, `TEMP`, `TEMP2`). Each routine is a Rust function that reads and writes them as the original does, so values left in registers between routines, like R4 above, carry across.
- Calls push the original return addresses onto a stack in that memory, so `MOON`, `ASCDO`, `DRAWMC` and `EXGEN` see what they saw on the GT40.
- Jumps out of nested calls (`RESTART`, `PALMX2`) become early returns that unwind to the routine that catches them.
- Time is charged per basic block of the original, with costs computed from the processor timing model, so the number of clock ticks per calculation (`CLOCK`) comes out the same as on the emulated machine. Interrupts are taken at block boundaries.

The verification follows from this. With interrupts held off, one calculation step of the translation and of the original binary, from the same memory and registers, must leave identical memory, identical registers, the same sequence of basic blocks and the same elapsed time. With interrupts live they are taken at block boundaries rather than between any two instructions, which moves an interrupt by at most one block; long runs are compared for outcome, not for every word.
