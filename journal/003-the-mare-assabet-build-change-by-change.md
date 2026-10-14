# 003 The Mare Assabet build, change by change

Date: 2026-10-14

The source-less binary (`lunar-later-nosource.lda`) is referred to from here on as the **Mare Assabet build**, after its greeting. Rick Naro described it as "of later vintage". The evidence below does not establish its age relative to the listing, so it gets a neutral name.

## Method

1. **A decoder validated against ground truth.** `oracle/tools/pdp11.py` decodes PDP-11 instructions and classifies every operand word (immediate, absolute, PC-relative, index). It was run over every instruction in the 1973 build and compared with the assembler's own listing: 1,737 of 1,737 agree. It flags opcodes a PDP-11/05 cannot execute, which helps separate code from data.
2. **A tokenised view of the 1973 program.** Each emitted byte becomes either a fixed value, a reference to a program location, or a reference to a fixed hardware address. The listing's relocation markers say which words are addresses.
3. **Block search.** The program is cut at its 462 global labels. Each block is searched for in the binary using only its fixed values. Unique matches anchor the map; ambiguous blocks are settled by adjacency; blocks with no fixed content (the all-zero variable area, address tables) are placed from the references that point into them.
4. **Independent verification.** Only after placement are the reference words checked: each must hold the new address of the thing it pointed to in 1973. This is a test the placement did not use.

Result: 434 of 462 blocks placed, 91.5% of the 1973 program's bytes located, 682 references consistent, 2 inconsistent (both genuine changes, below), 53 unverifiable because they point into storage that is reserved but never loaded.

Two bugs in my own tooling surfaced along the way and were fixed before any finding was recorded: the listing prints relocatable words relative to the program section (so values are now read from the linked image), and labels that sit alone before a storage reservation were being given the wrong address (they now come from the assembler's symbol table).

## How it was built

**Absolute-mode addressing.** Every PC-relative operand in the 1973 build is absolute in the binary: `TST FUEL` (opcode 005767, a displacement) became `TST @#150` (005737, an address); `JSR PC,DELAY` became `JSR PC,@#...`. This is what MACRO-11's `.ENABL AMA` does. The binary was reassembled from source with that option, not patched.

**Layout.** Apart from the edited routines, the entire program sits in the binary at a constant offset of -1002 (octal) from the 1973 build. The variables moved from 001000 to 000104. Every byte of the shift is accounted for by the edits listed below.

**Memory map**, with buffer addresses recovered from the instructions that reference them (several independent references agree in each case):

| Range | Contents |
|---|---|
| 000000 | `JMP @#000334`, restart |
| 000004-000033 | trap vectors (see trap catcher) |
| 000100 | line clock vector, to `TIMER` |
| 000104-000261 | variables, `TURN` through `DUSTX` |
| 000262-000317 | display-subroutine stack, top at 000316 |
| 000320-000333 | VT11 vectors: display stop, light pen, display bus error (static, set at run time in 1973) |
| 000334-014051 | program, tables, display lists |
| 014052-017774 | moon display buffer, reserved, not loaded |
| 017774-032337 | messages, terrain, features, trig tables, display control words |
| 032340-037776 | ship buffers, flame, explosion and dust list, CPU stack |

The startup code and the greeting are loaded at 032340-033737, inside the area that becomes the ship buffers and explosion list. The restart path clears that area to display-stop instructions, so the run-once code and text are overwritten by the game's own buffers after use.

## What differs

| # | Site | 1973 listing | Mare Assabet build |
|---|---|---|---|
| 1 | Startup | `START`: two RESETs, vectors installed at run time, keyboard and clock interrupts enabled, 10 s greeting | five RESETs; probes the serial line with the bus-error vector pointed at a handler that discards the trap; transmits `^C ^C K/F <CR>` to the host; enables the clock; 30 s greeting |
| 2 | Restart | `RESTAR` clears the moon buffer | entry at 000334 begins with `RESET`, clears the moon buffer, then a second loop clears 032340-037776 |
| 3 | Clock | `CLKFRQ=50.` | 60 (two sites: turn rate in `SHOWP1`, `DELAY1`) |
| 4 | Idle loop | waits until 5 ticks accumulate, "FOR CLOCK ON THE 40 AND 45" | no such loop; computes on every tick |
| 5 | Keyboard | `KBDINT: RESET; RTI`, keyboard interrupts enabled | keyboard not used; vector not loaded |
| 6 | Traps | none | trap catcher: vectors 004, 010, 014, 020, 030 each point at their own `HALT` (001000-001010) |
| 7 | Readouts | values at intensity 6, menu items at intensity 6 | values at 5, menu items at 3 (`DITEM1-4`, `ITEME1-ES`, and the twelve matching `INIT` entries) |
| 8 | Greeting | "RT-11 LUNAR LANDER ... ALL INTERNAL NAVIGATIONAL SYSTEMS HAVE DIED ON YOU" | "GREETINGS ... Mare Assabet ... AN INFERIOR BRAND OF MINICOMPUTER" |
| 9 | Dust | `#STACK-200.-EXLIST-12.` = 2490 | 2400; the same expression over the new layout, not a code change |

Everything else is identical, including:

- the feature table: 407 boulders and the MacDonald's at terrain positions 702-704 in both;
- the take-off logic after planting the flag or visiting the MacDonald's (`ADD #2000.,FUEL`, then forcing angle, sideways velocity and throttle while climbing).

## Corrections to earlier statements

- Entry 002 said the binary introduced the typo "SURVIRORS". It did not. Every copy of the source spells it that way (Al Kossow's 1980 RT-11 adaptation, Lars Brinkhoff's 2018 PALX port, all of Rick Naro's files, the copies on the RT-11 disk) except the file we adopted as the reference, which has it corrected. The typo is Burness's. Our reference copy was edited by someone, at least at this point.
- A working note said a keypress restarts the binary. It does not: the binary never enables keyboard interrupts and does not load the keyboard vector.

## Provenance, as far as the evidence goes

- **Our listing is an adaptation.** It sets `CLKFRQ=50.` but its hand-computed constants (`FIVESECONDS=300`, `CLKFDG=1500 ;CLKFRQ*25`, `CLKFG2=3000 ;CLKFRQ*50`) assume 60 Hz, so the clock was changed after they were written. It also carries a wait loop for the faster PDP-11/40 and 11/45 and an RT-11 greeting. Al Kossow's adaptation, dated 22 January 1980, has `CLKFRQ=60.` and the same wait loop.
- **The binary matches Burness's account in one respect.** He told Kate Willaert that "Mare Assabet" was in every version he modified himself and that he did not remember the "inferior brand of minicomputer" joke. This build contains both.
- **Its host logout points at a PDP-10.** `K` abbreviates `KJOB` (log out) and `/F` requests a fast logout on PDP-10 time-sharing systems of the period (a 1974 Stanford AI Lab help file documents both).
- **Order is not established.** Both builds share the post-fix take-off code, so neither is one of the early versions with the relanding bug Burness described. The binary lacks the 11/40-45 adaptation, which is consistent with it being closer to the GT40 original, but it also has features (trap catcher, host logout, memory reuse) the listing lacks. Recorded as open.

## Open questions

- Who made the listing's 50 Hz and RT-11 adaptation, and where.
- Which site ran the binary on a PDP-10-hosted GT40.
- Whether the readout intensity difference reflects a different display or taste.

Sources for external facts: Kate Willaert, "Moonlander: One Giant Leap for Game Design" (A Critical Hit, 2023), mirrored at cs.brandeis.edu/~storer; SAILDART `LOGOUT.BH[UP,DOC]`, 1974.
