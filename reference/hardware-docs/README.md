# DEC's manuals

The hardware model is written from these documents. They are DEC's and are not included here; each is listed with a public copy. [`docs/hardware.md`](../../docs/hardware.md) cites them by section, and the SHA-256 of the copy used is given so you can check you are reading the same scan.

To keep copies next to the code, put them in this folder. Git ignores everything here except this file.

## Cited in the model

| Document | Used for | Copy |
|---|---|---|
| GT40/GT42 User's Guide, EK-GT40-OP-002 (February 1975) | The machine, its address map, the VT11 registers and instructions, interrupt priorities, the bell, the monitor's specifications | [bitsavers](https://bitsavers.org/pdf/dec/graphics/VT11/EK-GT40-OP-002_GT40_GT42_Users_Guide_Feb75.pdf) |
| PDP-11/05, 11/10 Computer Manual, DEC-11-H05AA-A-D | The KD11-B processor: instruction times, the 11/05's differences from the 11/20, interrupt order, the line clock | [bitsavers](https://bitsavers.org/pdf/dec/pdp11/1105/DEC-11-H05AA-A-D_1105um.pdf) |
| VT11 Graphic Display Processor, EK-VT11-TM-001 (1974) | Vector and character timing, blink, character spacing jumpers, light pen timing | [Rick Naro's repository](https://github.com/sillygoose/moonlander/blob/master/Moonlander%20Classic/Documentation/PDP11/VT11%20Graphic%20Display%20Processor.pdf) |
| VR14/VR17 CRT Display Monitor User's Manual, DEC-12-HVCRT-D-D | Deflection settling, intensity levels | [bitsavers](https://bitsavers.org/www.computer.museum.uq.edu.au/pdf/DEC-12-HVCRT-D-D%20VR14%26VR17%20CRT%20Display%20Monitor%20User%27s%20Manual.pdf) |
| VT11 instruction set summary, Al Kossow (1993) | The display instruction set, and the display file disassembler in `oracle/tools/vt11dis.py` | [Rick Naro's repository](https://github.com/sillygoose/moonlander/blob/master/Moonlander%20Classic/Documentation/PDP11/VT-11%20Instruction%20Set.txt) |

## Background

These came with the Moonlander material in Rick Naro's repository and were read along the way, but nothing in the model rests on them.

| Document | Copy |
|---|---|
| GT44 User's Guide, DEC-11-HGT44-A-D | [Rick Naro's repository](https://github.com/sillygoose/moonlander/blob/master/Moonlander%20Classic/Documentation/PDP11/DEC%20GT44%20Users%20Guide.pdf) |
| PDP-11 MACRO-11 Language Reference Manual, AA-V027A-TC (1983) | [Rick Naro's repository](https://github.com/sillygoose/moonlander/blob/master/Moonlander%20Classic/Documentation/PDP11/macro.pdf) |
| RT-11 Commands Manual (1973) | [Rick Naro's repository](https://github.com/sillygoose/moonlander/blob/master/Moonlander%20Classic/Documentation/PDP11/RT-11%20Commands%20Manual.pdf) |
| PDP-11 instruction reference (CSC 258 course notes) | [Rick Naro's repository](https://github.com/sillygoose/moonlander/blob/master/Moonlander%20Classic/Documentation/PDP11/pdp11%20instruction%20reference.pdf) |
| PDP-11 Simulator Usage, simh (2008) | [Rick Naro's repository](https://github.com/sillygoose/moonlander/blob/master/Moonlander%20Classic/Documentation/PDP11/pdp11_doc.pdf) |
| The PDP-11 How-To Book | [Rick Naro's repository](https://github.com/sillygoose/moonlander/blob/master/Moonlander%20Classic/Documentation/PDP11/pdpbook.txt) |

## The copies used

```
5be14b3ca033eb0cb33aaf84b90fcedbb234c7bdffedac7bc132a5ba5573572f  GT40/GT42 User's Guide, EK-GT40-OP-002
e4ecf12e50f95a31b6f79aaca0cf0c6565c43e25b304e8debdacd4ae23d3bc26  PDP-11/05, 11/10 Computer Manual, DEC-11-H05AA-A-D
4857a8028f9be912a6fdec34502d47b3fb91bacd9994fce328e19ad2e76da96c  VT11 Graphic Display Processor, EK-VT11-TM-001
f04b947d91d7d8030e8563f06e794e385799228fe43bd675e9aabc839bae75a7  VR14/VR17 CRT Display Monitor User's Manual, DEC-12-HVCRT-D-D
45d5bc579078777ffd7ad2e71200221232190fec5b4286c954e0dff4136c6145  VT11 instruction set summary
c04205a1786ba8128c4a7258a45c2fbae84800a3f9ea54ff19b9d5613023cd99  GT44 User's Guide
5cae1efa1c63f0b05ad9036f3cd51ddb91dc9ea836d507bf1352efe1b7cde027  MACRO-11 Language Reference Manual
5667264fe2df62103ca5c75356d4f201f2d7db7ae961f811734b45a5cbcddac0  RT-11 Commands Manual
d3c13c06d37354b37f0a881c9f1e7c9b8adccf1354acafaa9cd651967dede329  PDP-11 instruction reference
0f56979fbb098da73d6eab3becdf6a675b768fc0ca62acadd924e1aeb3dff39b  PDP-11 Simulator Usage
db60c8103cebebbd474d645e118b39ebff0b559beca84ee97222ca441c8d2bf6  The PDP-11 How-To Book
```

The files in Rick Naro's repository are byte for byte the copies used. The three on bitsavers have not been compared with the copies used. If a scan differs, its page numbers may too, but DEC's section numbers, which the citations use, do not.
