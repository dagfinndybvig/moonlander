# FORTH on the GT40

Moonlander fills the GT40's 8K words of memory almost exactly: the game, the display list, the stacks, and the little monitor that loads it. That is what a 1973 trade-show demonstration had to do. But the same board — a PDP-11/05, a VT11 vector display, a keyboard — was sold as a general small computer, and the question this fork asks is what else that computer could be. Not with an operating system; the GT40 never really had one, just a bootstrap in ROM and programs that arrived on paper tape. Bare metal, and a high-level language small enough to leave room to work in.

In that era there was one obvious answer: Forth.

## Forth, briefly, from that era

Charles Moore was a computer pioneer who spent the 1960s writing control programs for observatories, and kept rewriting the same kind of program — read an instrument, move a telescope, record a number — in whatever language each new machine offered. His response was to carry the language with him instead. He distilled the parts he kept reimplementing into a tiny system with a dictionary of words, each word calling the others through a thread of addresses, and grew it machine by machine. The name came from "fourth-generation language", shortened to FORTH because the IBM 1130 he was then using allowed five characters in a file name.

In 1971 Moore and Elizabeth Rather brought the system to the National Radio Astronomy Observatory, where it ran the 36-foot radio telescope on Kitt Peak. Astronomers sat at a terminal, defined new words on the spot, and ran them; the language was the operating system, the compiler, and the application all at once. In 1973 the two of them founded FORTH, Inc. to sell the idea to laboratories that had small computers and too little memory.

What made Forth the right language for a machine like this was arithmetic, not romance. A Forth system is a dictionary of words, and a word is a few bytes of threaded code plus a short header. The interpreter is a handful of instructions. The compiler is the interpreter. Everything — the editor's job, the debugger's job, even the assembler's job — can be done by defining words, because the language extends itself in itself. A complete, interactive programming environment fit in 4K words on machines smaller than this one. In 1976 the Forth Interest Group published fig-Forth, a public-domain model of the language, and hobbyists typed its assembly listings into 8080s and 6502s and PDP-11s; by the Forth-79 standard the language was arguably the most portable high-level language small machines had.

The GT40 came out in 1972 and Moonlander was written in February 1973, so a PDP-11 Forth of exactly this period is not a fantasy — NRAO and FORTH, Inc. were running Forth on PDP-11s that year. Ours is not a port of any of them. It is written from scratch, in PDP-11 assembly, to fit this machine and to be readable alongside the Moonlander source in this repository.

## The system

The kernel is `forth.a11`, about 1,100 words of machine code, assembled by `asm11.py` — a small two-pass assembler written for this project because the period tools are not on hand — and packaged by `build-forth.py` into the same absolute paper-tape format the GT40's loader used, and that this repository's emulator reads:

```
python build-forth.py forth.a11 forth.lda
```

Run it in the desktop program, which gives the image the keyboard as console input, as the LK40 would have:

```
cargo run --release -p moonlander-desktop -- --image forth.lda
```

You get a banner and a prompt on the vector screen, 24 rows of 32 characters drawn straight into the VT11's display list. Type at it.

For testing without a window, `forthtest` boots the image headlessly, types lines at it, and prints the screen:

```
cargo build --release --example forthtest -p kd11b
./target/release/examples/forthtest.exe forth.lda out.png "1 2 + .,: SQ DUP * ;,5 SQ ."
```

## A first session

```
GT40 FORTH
1 2 + .
3 
: SQ DUP * ;
5 SQ .
25 
```

Everything on the stack. `1 2 +` pushes 1, pushes 2, adds; `.` prints and drops. A colon definition compiles the words between `:` and `;` into a new dictionary entry — here `SQ`, which squares the number on top of the stack — and from then on `SQ` is a word like any other. There is nothing else to learn about defining words, because that is all a definition is.

Numbers are 16-bit signed integers, printed in decimal. There is no floating point and no division word; Moonlander did not need one and neither, for a demonstration, do we.

## The dictionary

| | |
|---|---|
| `DUP DROP SWAP OVER ROT` | the stack |
| `+ - * AND OR` | arithmetic and logic |
| `@ ! C@ C!` | fetch and store, word and byte |
| `>R R>` | the return stack |
| `0= 1+ 1- +!` | flags and counters |
| `HERE ALLOT ,` | the dictionary as an allocation arena |
| `EMIT KEY CR .` | the console and the screen |
| `: ;` | define a word |
| `VARIABLE CONSTANT CREATE` | defining words |
| `IMMEDIATE LITERAL` | compilation control |
| `IF ELSE THEN` | conditionals |
| `BEGIN UNTIL` `BEGIN WHILE REPEAT` | general loops |
| `DO LOOP` | counted loops, with `I` as the index |

A few sessions' worth, in one place:

```
VARIABLE X
3 X !
X @ .
3 

10 CONSTANT TEN
TEN 2 * .
20 

: CNT 5 0 DO I . LOOP ;
CNT
0 1 2 3 4 

: COUNTDOWN 5 BEGIN DUP . 1 - DUP 0= UNTIL DROP ;
COUNTDOWN
5 4 3 2 1 

: T 0 0= IF 111 . ELSE 222 . THEN ;
T
111 
```

`DO` runs its body at least once and steps until the index reaches the limit. `BEGIN ... UNTIL` repeats until the flag is true; `BEGIN ... WHILE ... REPEAT` tests at the top. `IF` consumes a flag and compiles a conditional branch — inside a definition, `IF`, `ELSE`, `THEN`, `BEGIN`, `DO` and their partners are immediate words that execute at compile time and patch the branch offsets into the definition as it is being built. That mechanism, more than any other, is what makes a Forth compiler small: control flow is just words that compile other words.

A word the system does not know — inside a definition or out — prints `?` and returns to the prompt. If it happened inside a definition, the half-built definition is discarded, so a mistyped name costs nothing but the typing.

`CREATE` and `HERE` extend the language to data:

```
CREATE TABLE 3 , 1 , 4 , 1 , 5 ,
TABLE @ .
3 
TABLE 2 + @ .
1 
```

`CREATE` makes a word whose body is whatever you allot next; `,` compiles a number into the dictionary at `HERE`, and the next number lives two bytes further on, since a cell is a word. This is the whole story of data structures in Forth, and in 1973 it was a perfectly ordinary way to build a table.

## Inside

For the curious, and for anyone comparing this with the Moonlander source: the kernel is indirect-threaded. Each register has one job — R4 is the instruction pointer, R3 the current word, R5 the return stack, R6 the data stack — and the entire inner interpreter is two instructions:

```
NEXT:   MOV (R4)+, R3
        JMP @(R3)+
```

A colon definition's code field holds `DOCOL`, which pushes the instruction pointer on the return stack and follows the thread. Machine words end in `NEXT` and pick the thread up where it left off. The outer interpreter runs each word through a small trampoline so that machine words need not know whether they were called from a definition or from the keyboard.

Memory, in octal:

| | |
|---|---|
| `01000-03777` | kernel code |
| `04000` | kernel dictionary |
| `10000` | the runtime dictionary, growing up |
| `30000` | the VT11 display list — the text screen |
| `32000` | input buffer and kernel cells |
| `33000` | return stack, growing down |
| `34000` | data stack, growing down |

Everything above `34000` is stack headroom, and the dictionary can grow for another 4K words before anything collides — though on a real GT40 there was only 8K words in total, and this map is sized to it: the kernel, the screen, the buffers, the stacks, and room left over for your definitions, all inside the same 8K that Moonlander flew in.

## What it is not

It is a demonstration, not a distribution. There is no disk, no file system, no editor beyond backspace, no division, and no floating point; word names are capped at 17 characters and input lines at 64, the size of their buffers; definitions are lost when the power goes off, as they would have been on a paper-tape machine. What it does show is the claim this fork was made to test: the same emulated hardware that runs Moonlander instruction for instruction can boot a high-level language, give you a prompt, and compile your words — bare metal, in 8K words, on a machine whose only output device is a vector display.
