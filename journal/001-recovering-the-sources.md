# 001 Recovering the sources

Date: 2026-10-14

## What exists

Moonlander was written by Jack Burness for the DEC GT40 graphics terminal in February 1973, as a trade show demo. The GT40 is a PDP-11/05 with a VT11 vector display processor and a light pen. Burness wrote it in PDP-11 assembly (MACRO-11), and the display programs it builds in memory are VT11 instructions.

Recovered material, with provenance:

| File | Origin | Notes |
|---|---|---|
| `reference/original/gtlem-1973.mac` | Rick Naro's repository (`sillygoose/moonlander`, MIT, 2020), file `GTLEM Source Code.mac`, matching the copy Eric Smith has hosted on brouhaha.com since the 1990s | The untouched listing. Header: "LEM.090, FEBRUARY 25, 1973 5:55 PM". The lineage runs through Al Kossow, who held a copy when Burness himself no longer had one. |
| `reference/original/rtlem-rt11.mac` | Same repository | Al Kossow's RT-11 adaptation. 65 changed lines against the original after whitespace normalisation. |
| `reference/original/lunar-later-nosource.lda` | Same repository, `Binaries/Moonlander/Lunar` | A later build distributed with the GT40 simulator. No source survives for it. |
| `reference/original/lunar-1969.fcl` | Same repository | Jim Storer's 1969 FOCAL text game, the ancestor. |
| `reference/hardware-docs/*` | Same repository | DEC VT11 and GT44 documentation, PDP-11 references. |

SHA-256 at time of capture:

```
f74777ac8770def307abd3f45e905def3ad30583a81ea7b007267e979b5682ca  gtlem-1973.mac
5ac337a90b2306111b864a0b26897d84600ea6528cb2b33a0968036ce2e9717c  rtlem-rt11.mac
95f314570424a2d3a5e5e4684b78d5b5f519d6f16931854540dc736220340930  lunar-later-nosource.lda
f4b791c6ae667a2e03bccf18fc108d31ed6e551ef4967f03a7827137c1a9ae2f  lunar-1969.fcl
```

## First observations

- The source is complete: 5,101 lines, every routine labeled and commented in Burness's voice ("THIS IS THE START OF THE DISPLAY SECTION. PLEASE BE MERCIFUL.").
- Units are documented in the header: velocities in tenths of feet per second, accelerations at 500 times actual, distances in whole feet, fuel in tenths of pounds, angles as compass degrees.
- Arithmetic is all integer, through hand-written routines (`MULTWO`, `DIVTWO`, unrolled shift-and-add). Trig comes from 360-entry tables, not computation.
- Rick Naro's notes say the dust code "does not appear to be stable" and he disabled it in his build. This is the first candidate for a known defect in the original.
- The header calls itself "FIRST VERSION". Kate Willaert's research shows it is not; earlier builds existed.

## Method decided

1. Treat the 1973 source as the specification and the assembled 1973 binary, running in a PDP-11 emulator, as the oracle.
2. Translate routine by routine, keeping 16-bit integer semantics, so the translation can be compared to the oracle tick by tick.
3. Reconstruct the VT11 display processor and light pen from DEC's manuals, so the game's own display files and light pen handler run unchanged in spirit.
4. Use the source-less later binary as the pure reverse-engineering exercise: recover what changed, with only machine code to go on.
5. Record every deviation from the original here, with the reason.
