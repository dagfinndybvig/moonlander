# The GT40, as modelled

This is the reference for the machine Moonlander runs on in this repository: what each part of the GT40 does in the model, which figures come from DEC's documents, which are my readings of them, and which are assumptions. The journal tells how the model was built and checked; this page is for looking things up.

Every figure is tagged:

- **[doc]**: stated in one of DEC's manuals, cited by section.
- **[READING]**: a choice where a manual is unclear or silent, explained where it is made.
- **[MODEL]**: an assumption, judged rather than documented.
- **[measured]**: measured on this model, not on hardware.

The same tags mark the constants in the code (`MODEL` and `READING` comments), so a figure here can be found there.

## Sources

Links to public copies, and checksums of the copies used, are in [`reference/hardware-docs/`](../reference/hardware-docs/).

| Short name | Document |
|---|---|
| GT40 guide | GT40/GT42 User's Guide, EK-GT40-OP-002 (February 1975) |
| 11/05 manual | PDP-11/05, 11/10 Computer Manual, DEC-11-H05AA-A-D |
| VT11 manual | VT11 Graphic Display Processor, EK-VT11-TM-001 (1974) |
| VR14 manual | VR14/VR17 CRT Display Monitor User's Manual, DEC-12-HVCRT-D-D |
| VT11 summary | Al Kossow's VT11 instruction set summary (1993) |

Two things come from simh rather than a manual, and are marked where they are used: the VT11 character shapes, copied verbatim with their licence from open-simh's `display/vt11.c` into [`chargen.rs`](../crates/gt40/src/chargen.rs), and a few details of the VT11 that the manuals leave to the circuit diagrams.

## The machine

A GT40 is a PDP-11/05 (the KD11-B processor) with 8K words of core, a VT11 display processor that reads its display file from that core by DMA, a VR14 monitor with a light pen, an LK40 keyboard, a line clock, and a serial line to a host computer. The GT40 guide (4.2) puts it this way: "two separate, programmed processors: a PDP-11/10 computer (CPU) and a special display processor (DPU)". The 11/05 and 11/10 are the same processor.

| Part | Model | Code |
|---|---|---|
| Processor | KD11-B: basic PDP-11 instruction set, interrupts, traps, DEC's instruction times | [`crates/kd11b`](../crates/kd11b/) |
| Memory | 8K words of MM11 core | [`terminal.rs`](../crates/gt40/src/terminal.rs) |
| Line clock | the KD11-B's built-in KW11-L | `terminal.rs` |
| Display processor | VT11: display file execution, timing, light pen, interrupts | [`vt11.rs`](../crates/gt40/src/vt11.rs) |
| Characters | VT11 character generator, 6 × 8 dots | [`chargen.rs`](../crates/gt40/src/chargen.rs) |
| Monitor | VR14 with P39 phosphor, for the window | [`crt.rs`](../crates/gt40/src/crt.rs) |
| Host line | DL11 | `terminal.rs` |
| Keyboard | LK40 registers | `terminal.rs` |
| Bell | a write to the display status register | `terminal.rs`, [`sound.rs`](../crates/desktop/src/sound.rs) |

Everything runs on one clock, in microseconds. The processor (or the translation standing in for it) advances it by instruction times; the display processor is brought up to the same time before every interrupt check and every write to its program counter; the line clock ticks at fixed times.

## Address map

| Address (octal) | What | Notes |
|---|---|---|
| 000000–037777 | core, 8K words | Anything above, up to the I/O page, is non-existent and a reference to it is a bus error. |
| 000004 | vector: bus error | Odd address on a word access, or no device. |
| 000010 | vector: reserved instruction | Anything outside the 11/05's instruction set, and JMP or JSR to a register. |
| 000014 | vector: BPT | |
| 000020 | vector: IOT | |
| 000024 | vector: power fail | Not modelled (see below). |
| 000030 | vector: EMT | |
| 000034 | vector: TRAP | |
| 000060 | vector: keyboard | BR4. |
| 000100 | vector: line clock | BR6. |
| 000320 | vector: VT11 stop | BR4. |
| 000324 | vector: VT11 light pen | BR4. |
| 000330 | vector: VT11 time-out and shift-out | BR4. |
| 172000 | VT11 DPC, display program counter | Write an even address to start the display there, an odd value to resume a stopped display. |
| 172002 | VT11 DSR, display status | Reading gives stop flag, mode, intensity, light pen flag, shift out, edge, italics, blink and line type. Any write rings the bell. |
| 172004 | VT11 XSR | X position (bits 9–0), graphplot increment (15–10). |
| 172006 | VT11 YSR | Y position (bits 9–0), character register (15–10). |
| 175610–175616 | DL11 host line: RCSR, RBUF, XCSR, XBUF | [doc] GT40 guide 3.4. |
| 177546 | LKS, line clock status | Bit 7 the tick flag, bit 6 interrupt enable. |
| 177560–177566 | keyboard status and buffer, console transmitter | |
| 177570 | console switches (read), lights (write) | |
| 177776 | PSW | Answered by the processor. |

The vectors and registers are [doc] from the GT40 guide (chapters 3 and 4) and the 11/05 manual. The priority levels are [doc] GT40 guide Table 5, which puts the display processor on BR4, and 3.2.3.1: "in the KD11-B, the internal line clock is logically the last device on BR6, and the keyboard or Teletype interface is logically the last device on BR4". The 11/05 manual says the same of its serial line.

## Processor: KD11-B

**Instruction set.** The basic PDP-11 set. Everything the KD11-B lacks (EIS, floating point, SOB, XOR, SXT, MARK, RTT, MFPI, MTPI, MFPS, MTPS, SPL) takes the reserved instruction trap. Condition codes follow DEC's definitions. The model was compared instruction by instruction with simh's PDP-11 configured as an 11/05, on randomly generated programs (journal 006).

**11/05 behaviour.** [doc] 11/05 manual Table 2-8 lists where the 11/05 differs from the 11/20. The model follows the 11/05 column: an instruction whose source register is also its autoincremented or autodecremented destination uses the register's value before the change, `MOV PC,LOC` stores the address of the instruction plus 2, and SWAB clears V.

**Interrupts.** Taken between instructions when a request's level is above the processor's priority (PSW bits 7–5). [doc] 11/05 manual Table 4-5 gives the order: T-bit trap, stack overflow, power fail, BR7, BR6, internal line clock, BR5, BR4, then the serial line (the keyboard). Interrupt entry pushes the PSW and PC and loads both from the vector. [doc] 11/05 manual Table 2-8: on the 11/05, "the first instruction in an interrupt routine will not be executed if another interrupt occurs at a higher priority level than was assumed by the first interrupt". The model checks for interrupts before every instruction, so it does the same.

**WAIT** stops the processor until an interrupt; the model moves time straight to the next display interrupt or clock tick. **RESET** sends INIT, which resets the VT11, the clock's status and the serial lines. **HALT** stops the processor.

**Instruction times.** [doc] 11/05 manual Part II, section 2.2.2 and Tables 2-2 to 2-7: a basic time per instruction plus source and destination times by addressing mode, with adjustments for non-modifying instructions (TST, CMP, BIT, JMP, JSR, per section 4.7.2.2) and for byte operands at odd addresses. The tables are for MM11-K/L core, the GT40's memory. The full tables, and what they do to the game, are in [journal 010](../journal/010-decs-timings.md) and [`timing.rs`](../crates/kd11b/src/timing.rs).

| | µs |
|---|---|
| MOV R,R | 3.1 |
| ADD R,R | 3.7 |
| MOV #n,R | 4.0 |
| MOV @#a,@#b | 9.5 |
| CMP #n,@#a | 6.8 |
| CLR @#a | 6.8 |
| conditional branch, not taken / taken | 1.9 / 2.5 |
| JSR PC,@#a / RTS PC | 6.0 / 3.8 |
| RTI | 4.4 |
| EMT, TRAP, IOT, BPT | 8.2 |
| RESET | 20,000 |

- [READING] JMP and JSR add the destination time less 1.2 µs, as non-modifying instructions.
- [READING] MOV's "3.1 µs mode 0" applies to a register destination.
- [READING] Interrupt entry takes 8.2 µs, the time of a trap instruction, since both use the same microroutine after one bus transfer (11/05 manual 5.5.2).
- [READING] A bus error trap takes the faulting instruction's time plus 8.2 µs.
- [doc] Every time is "subject to ±10% variation". The model uses the stated values.

**Not modelled.**

- The T-bit trace trap, the stack overflow trap and power fail. Moonlander never sets the T bit and keeps its stack at the top of core. Its power fail vector points at a routine that restarts the game when power comes back, which the model never calls.
- The console's functions, beyond the switch and light registers.
- [doc] 11/05 manual Table 2-8: on the 11/05 the general registers "are valid program addresses" at 177700–177717. The model treats those addresses as non-existent. Moonlander never uses them.
- The processor waiting for the bus while the display processor's DMA has it (see Memory).

## Memory: MM11

8K words of core, the size the program assumes: its stack starts at 037776. [doc] GT40 guide 1.5: 400 ns access and 900 ns cycle for a read. Memory speed enters the model only through the processor's instruction times and the display processor's fetch time.

**Not modelled: DMA contention.** [doc] GT40 guide 4.2: the display processor reads core "by way of non-processor requests (NPR, i.e., DMA requests)". On the real machine each display fetch holds the bus for a memory cycle, and a processor that wants the bus then waits. The model runs the two independently, so the real processor would be a little slower than modelled, by the share of bus cycles the display takes.

## Line clock

[doc] 11/05 manual 4.12: the KD11-B's line clock is "program compatible with the KW11-L" and takes its input from the power supply. It ticks at the mains frequency, 60 Hz by default and 50 Hz as an option; the surviving binary is built for 60 (`CLKFRQ=60.`). Each tick sets bit 7 of LKS and, if bit 6 is set, requests an interrupt at BR6 through vector 100.

- [MODEL] One flag serves as both the tick flag and the interrupt request: taking the interrupt clears bit 7, and so does writing 0 to it. A KW11-L keeps bit 7 until the program clears it. Moonlander sets bit 6 and never tests bit 7, so it cannot tell the difference.

## Display processor: VT11

The VT11 executes a display file from core: control words that set the mode and parameters, and data words that draw in the current mode. The model follows the GT40 guide chapter 4 and the VT11 manual chapter 2, and reports every point and line the beam draws, with times, for the monitor model and for tests.

**Face and coordinates.** [doc] GT40 guide 1.5: 1024 × 768 raster units visible, from 12-bit position registers (a "paper" 4096 square). The origin is at the bottom left, and positions wrap at 4096. Drawing outside the face sets the edge flag and lights nothing.

**Modes.** Character, short vector, long vector, point, graphplot X, graphplot Y and relative point, set by control words that can also set the intensity (8 levels), light pen enable, blink and line type (solid, long dash, short dash, dot dash). Load status A sets the stop interrupt enable, light pen intensify, italics, stop and sync; load status B sets the graphplot increment. A jump takes the next word as the new DPC. [doc] GT40 guide 4.3 and 4.5, VT11 manual 2.2.8, VT11 summary.

- [MODEL] Undefined control codes 10–13 do nothing.
- [doc] When stop and sync are given together, sync wins: "The display will stop but will resume in sync with the line frequency" (GT40 guide 4.7.1). Sync waits for the next mains pulse.

**Timing.**

| | µs | |
|---|---|---|
| vector | 0.2 × the next power of two above its longer component | [doc] VT11 manual 2.2.10.2 and 2.2.10.3: the controlling component is normalized into 1000–1777 octal and clocked at 5 MHz, so "the longest possible vector [...] takes about 200 µs to draw", and every vector is drawn at between half and all of that rate |
| character | 20.6 | [MODEL] 48 dots at the 400 ns character clock, plus the 0.98–1.4 µs scope delay ([doc] VT11 manual 2.2.12) |
| fetch of a display word | 1.2 | [MODEL] a core cycle plus decode |
| absolute or relative positioning | 3.0 | [MODEL] DAC load and settling. [doc] VR14 manual: full screen deflection settles in 18 µs or less, a 0.1 inch step in about 1 µs, so the real time depends on the distance; the model's does not. |
| non-printing character | 1.0 | [MODEL] |
| blink half period | 266,000 | [doc] VT11 manual 2.2.8.5 and Figure 2-16 |
| light pen inhibit after a hit | 70 | [doc] VT11 manual appendix B |

Because a vector's time goes by the power of two above its length, a vector of 513 units takes as long as one of 1023, and the beam dwells longer on each point of the shorter one. The monitor model shows this as brightness.

**Characters.** 6 × 8 dots on a 2-unit grid. Shift out selects the special characters 000–037; an illegal code in shift-out mode stops the display and interrupts through 330.

- [READING] Characters 14 units apart and lines 24 apart. [doc] The GT40 guide (1.5) gives 73 characters to a line and 31 lines, which the VT11 manual's Table 2-12 makes jumper W5. The 14 and 24 units for W5 are simh's (`vt11.h`).
- [MODEL] Italics shear each dot row right by 5/8 unit, after simh.
- [MODEL] A character is drawn only if the beam position and the end of its cell, 13 units to the right, are both on the face, as simh also does.

**Dashes.** [MODEL] The dash patterns come from the VT11's line counter, decoded as simh's `vt11.c` decodes it.

**Interrupts.** All at BR4 ([doc] GT40 guide Table 5): a stop instruction with the stop interrupt enabled (vector 320), a light pen hit (324), and a time-out on a display fetch from non-existent memory or an illegal shift-out code (330).

## Light pen

The pen sees the beam when it draws within the pen's field of view while light pen interaction is enabled for what is being drawn. A hit stops the display at that point, with the DPC past the word being drawn and the position registers at the beam, and requests an interrupt. Writing an odd value to the DPC resumes, finishing the interrupted vector or character pair. If light pen intensify is on, the point of the hit is drawn at full intensity. After a hit the pen is blind for 70 µs ([doc] VT11 manual appendix B).

- [MODEL] The field of view is a circle 5 raster units in radius, about 1.1 mm on the face. The autopilot, the mouse and the keyboard all use this value.
- [MODEL] A hit on a character is reported after the character is drawn, with the position registers at the next character.

The pen's movements are scheduled by time, and the model looks up where the pen is at the time of each point it draws. So a pen movement takes effect at its own moment however the display's running is divided up (journal 008).

**What the field of view decides.** How many hits a held pen produces, and so how quickly Moonlander's controls respond and how fast `HITCNT` counts. [measured] With the pen on the throttle bar, the program's throttle handler runs about 2,250 times a second; held on an arrow, the ship turns about 42 times a second (journal 010). With the pen held at height Y on the bar, the program's reading settles at about (Y − 466) / 2 percent (journal 009).

## Monitor: VR14 with P39 phosphor

| | | |
|---|---|---|
| viewable area | 9 × 6.75 in | [doc] GT40 guide 1.5 |
| phosphor | P39 | [doc] GT40 guide |
| spot size | under 20 mils (FWHM) at 30 fL, so under 2.3 raster units | [doc] GT40 guide |
| brightness | 30 fL or more | [doc] GT40 guide, VR14 manual |
| contrast ratio | over 10:1 | [doc] GT40 guide |
| intensity levels | 8, program controlled | [doc] GT40 guide, VR14 manual 3.5 |

The window's monitor model ([`crt.rs`](../crates/gt40/src/crt.rs), journal 009):

| | | |
|---|---|---|
| spot | Gaussian, 2.0 raster units across at half maximum | from the spot size above |
| persistence | exponential, to 10% in 150 ms | [MODEL] the manuals name P39 but give no persistence |
| excitation | proportional to the beam's dwell on each pixel and to the intensity level | [MODEL] |
| relative beam current by level | 0.30 + 0.70 × level / 7 | [MODEL] the VR14 manual (3.5, Figure 3-6) leaves the spread between levels to the operator's contrast and brightness controls |
| character dot dwell | 0.5 µs | [MODEL] |
| saturation, faceplate glow, faceplate light | set by eye | [MODEL] |

The model's picture depends only on what the beam did and when, not on the window's size or frame rate.

## Host line: DL11

Present by default at 175610–175616 ([doc] GT40 guide 3.4). Moonlander writes to it once, at start, to log the user off a PDP-10: a null, two ^Cs, and `K/F` with a carriage return, the fast logout.

- [MODEL] The transmitter is always ready: characters go out at once.
- [MODEL] The DL11's interrupts are not modelled; Moonlander does not enable them.
- With the line absent, touching its registers is a bus error. Moonlander points the bus error vector at a routine of its own before it tries, which skips the logoff, as on a GT40 without a host.

## Keyboard and bell

The LK40 keyboard's registers are at 177560–177566, interrupting through 060 at BR4. Moonlander reads nothing from the keyboard.

[doc] GT40 guide 4.3: "If an attempt is made to write to address 172002, the effect is to ring the BELL in the GT40/42". Moonlander rings it with `BIS #0,DSR`: once when the fuel falls under 200 pounds, and repeatedly while the ship explodes.

- [MODEL] The guide describes the bell only as a "tone generator". The desktop program plays 150 ms of an 880 Hz tone.

## For comparisons: block-synchronous mode

Not part of the GT40. To compare the translation with the original word for word, the processor model has a mode in which it brings the devices up to time, and takes interrupts, only at the start of the original program's basic blocks, as the translation does. It also stacks PSWs without condition codes, which the translation does not keep, and restores the real codes on RTI from a shadow copy. The original program runs exactly as it would otherwise. Outside the tests the processor model runs in its ordinary mode, as the desktop program's `--original` option uses it.
