# 002 The 1973 baseline, and a second Moonlander

Date: 2026-10-14

## Rebuilding the 1973 program from its listing

Toolchain:

- MACRO-11 cross-assembler: `Rhialto/macro11`, commit `0616816f8e81c24377dca6e191a82c406ab59394`, version 0.8.
- Linker: `oracle/tools/link.py`, a minimal RT-11 object linker. It applies the four relocation directory entry types MACRO-11 emits for a self-contained module and aborts on anything else.

The source assembles without a single error or edit. It contains no `.ASECT`, so it is relocatable. It was linked at octal 001000, the RT-11 LINK default. Two independent signals support that base: Rick Naro's annotations place `TURN`, the first data word, at 001000, and the source reserves its stack immediately below `START`.

```
macro11 -rt11 gtlem-1973.mac -o gtlem-1973.obj -l gtlem-1973.lst
link.py gtlem-1973.obj --base 1000 --lda gtlem-1973.lda --bin gtlem-1973.bin --json gtlem-1973.link.json
```

Result: 11,813 bytes loaded at 001000-041617, start address 001216 (`START`).

The build was repeated from a clean directory and compared: identical object file, identical memory image, identical load file.

```
1e63081fa0ab6985e36766b330e62078088a9b97aae4377c8c27ee12e6365061  gtlem-1973.obj
00df90f75732f0aa3997d0bd837aa6f424c6325ec7e0b6e3215c1ce0b5a502d8  gtlem-1973.lda
e9aeb935befc2904322e95e68836589ac31dcc8573d538c8d801127d6f7d1b39  gtlem-1973.bin
```

A note on fit: a base GT40 has 8K words (addresses up to 037777), and this build ends at 041617. The source carries a comment asking that no code be placed in 015000-020000 or 035000-037776. Either the machine it ran on had more memory, or it was linked differently. Open question.

## The listing is not purely February 1973

The header reads "LEM.090, FEBRUARY 25, 1973 5:55 PM" and "FIRST VERSION". The startup message, `STARTM` at the end of the file, reads:

```
            R T - 1 1  L U N A R  L A N D E R
    ALL INTERNAL NAVIGATIONAL SYSTEMS HAVE DIED ON YOU
       IT LOOKS AS THOUGH YOU'RE GOING TO HAVE TO
             LAND THIS THING ON YOUR OWN
       ***** GOOD BYE, GOOD LUCK, AND SORRY  *****
```

A greeting naming RT-11 sits uneasily with a February 1973 GT40 demo. At least this part of the file was touched after the header was written. Kate Willaert has separately established that "FIRST VERSION" is not literally true. Recorded as a provenance question, not resolved.

## The second Moonlander

`lunar-later-nosource.lda` is a different build of the game. It is consistent in name, format and size (13,824 bytes) with the `lunar.lda` (listed as 14K) that John Holden supplied to Eric Smith's GT40 page, though that page could not be fetched to compare bytes. A copy inside the RT-11 V5.3 disk image in Rick Naro's repository is byte-identical, so it is the same file, not independent corroboration.

Load map:

| Range | Bytes | Likely content |
|---|---|---|
| 000000-000033 | 28 | trap and interrupt vectors |
| 000100-000261 | 114 | more vectors (clock, display, light pen region) |
| 000320-014051 | 5,978 | program |
| 014052-017773 | not loaded | 1,986 bytes, close to the 1,000-word moon buffer the 1973 source reserves |
| 017774-033737 | 6,115 | program and tables |

Start address 032340. Unlike the 1973 build it loads from address 0, carries its own vectors, and leaves 015000-020000 free, which matches the request in the 1973 source's own comment.

A diff of the printable text in both images shows what changed on screen:

- The greeting is new. It reads, in part: "BECAUSE OF CHANGES IN THE NATIONAL POLICY, THE LUNAR EXPLORATORY PROGRAM IS ONCE AGAIN FUNCTIONING", sends the pilot to "Mare Assabet" (rendered in lower case, which the VT11 character generator supports through shift codes), and explains that "THE SPACE AGENCY CHOSE TO USE AN INFERIOR BRAND OF MINICOMPUTER IN THE LUNAR LANDER". Mare Assabet is a joke: the Assabet River runs past DEC's mill in Maynard. This is the version people remember.
- The crash message gained a typo: 1973 "NO SURVIVORS", later "NO SURVIRORS".
- The greeting has its own typo: "YOU MISSION".
- All other message strings are unchanged in content; their surroundings differ because the code moved.

## Prior art

Rick Naro produced a raw machine disassembly of this build (`lunar.dis`, also on the disk as `LUNAR.MAC`): absolute `.ASECT` code with generated labels (`L0`, `L1`, ...) and no names or comments. He notes that no source is available for it.

That defines the work still undone: a symbolic reconstruction that uses Burness's names and structure, reassembles byte for byte to the same image, and documents every difference between the two versions.

## An evidence trap

The RT-11 disk dates its files 1979 to 2012. `GT.MAC` is dated 1979-09-09 but its header contains "MODIFICATIONS BY RICK NARO TO RUN ON GT40 SIMULATOR". The dates reflect the simulator's clock when files were written, not history. None of the disk's dates are used as evidence.
