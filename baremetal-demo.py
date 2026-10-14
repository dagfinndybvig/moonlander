# Bare-metal GT40 demo, hand-assembled: a pulsing square and a bouncing dot,
# animated from the 60 Hz line-clock interrupt while the VT11 loops over the
# display list by DMA. The CPU spends its life in WAIT.
#
# Written in the Mare Assabet build's own style: absolute addressing
# (@#LABEL, mode 37) for every memory operand, no PC-relative references --
# the kd11b model resolves index/relative words as byte displacements, which
# is right for X(Rn) but never doubled for mode 67, and the original binary
# never uses PC-relative to notice.
#
# Memory layout (octal):
#   0100    clock vector -> ISR at 0200, PSW 0300 (priority 6)
#   1000    main: enable clock, point the VT11 at the list, WAIT forever
#   1100    variables: SIZE DELTA X Y DX DY
#   1200    display list (the VT11 re-fetches it every pass, so edits show)
#   2000    clock ISR: oscillate SIZE, bounce X/Y, patch the list, RTI
#   3000    top of stack

import sys, os
sys.path.insert(0, r"C:\Users\dybvig\Reverse\moonlander\oracle\tools")
import lda as ldamod

SIZE  = 0o1100
DELTA = 0o1102
X     = 0o1104
Y     = 0o1106
DX    = 0o1110
DY    = 0o1112

LIST  = 0o1200
P1    = LIST + 2    # point word 1 (x, lit)
P2    = LIST + 4    # point word 2 (y)
V1    = LIST + 6    # long vector: dx=+S (lit)
V4    = LIST + 14   # dy=+S
V5    = LIST + 16   # dx=-S (lit)
V8    = LIST + 22   # dy=-S

SMAX = 300
SMIN = 40

# --- two-pass assembler -----------------------------------------------------

mem = {}
cur = [0]
labels = {}
fixups = []  # (addr, 'abs', label) or (addr, 'br', label, opcode)

def org(a): cur[0] = a
def label(name): labels[name] = cur[0]
def word(w): mem[cur[0]] = w & 0o177777; cur[0] += 2
def abs_ref(target_label):       # absolute address operand word (mode 37 data)
    fixups.append((cur[0], 'abs', target_label)); word(0)
def br(op, target_label):
    fixups.append((cur[0], 'br', target_label, op)); word(op)

def resolve():
    for f in fixups:
        addr, kind, lab = f[0], f[1], f[2]
        target = labels[lab]
        if kind == 'abs':
            mem[addr] = target
        else:
            off = (target - addr - 2) // 2
            assert -128 <= off <= 127, (addr, lab, off)
            mem[addr] = (f[3] | (off & 0o377)) & 0o177777

# --- main program at 01000 ---------------------------------------------------

org(0o1000)
label('MAIN')
word(0o12706); word(0o3000)                  # MOV #3000, SP
word(0o12737); word(0o300); word(0o177546)   # MOV #300, @#177546 (clock IE+MONITOR)
word(0o12737); word(LIST); word(0o172000)    # MOV #LIST, @#172000 (DPC: load+run)
label('LOOP')
word(0o000001)                               # WAIT
br(0o000400, 'LOOP')                         # BR LOOP

# --- clock interrupt service at 02000 ----------------------------------------

org(0o2000)
label('ISR')
word(0o010046)                               # MOV R0, -(SP)

# square size oscillates between SMIN and SMAX
word(0o13700); abs_ref('SIZE')               # MOV @#SIZE, R0
word(0o63700); abs_ref('DELTA')              # ADD @#DELTA, R0
word(0o10037); abs_ref('SIZE')               # MOV R0, @#SIZE
word(0o20027); word(SMAX)                    # CMP R0, #SMAX
br(0o003400, 'OKH')                          # BLE OKH
word(0o12737); word(SMAX); abs_ref('SIZE')   # MOV #SMAX, @#SIZE
word(0o5437); abs_ref('DELTA')               # NEG @#DELTA
br(0o000400, 'SQD')                          # BR SQD
label('OKH')
word(0o20027); word(SMIN)                    # CMP R0, #SMIN
br(0o002000, 'SQD')                          # BGE SQD
word(0o12737); word(SMIN); abs_ref('SIZE')   # MOV #SMIN, @#SIZE
word(0o5437); abs_ref('DELTA')               # NEG @#DELTA
label('SQD')
# patch the four long vectors of the square
word(0o13737); abs_ref('SIZE'); abs_ref('V1')     # MOV @#SIZE, @#V1
word(0o52737); word(0o040000); abs_ref('V1')      # BIS #040000, @#V1 (lit)
word(0o10037); abs_ref('V4')                      # MOV R0, @#V4 (dy=+S)
word(0o10037); abs_ref('V5')                      # MOV R0, @#V5
word(0o52737); word(0o060000); abs_ref('V5')      # BIS #060000, @#V5 (lit, -dx)
word(0o10037); abs_ref('V8')                      # MOV R0, @#V8
word(0o52737); word(0o020000); abs_ref('V8')      # BIS #020000, @#V8 (-dy)

# bounce the dot horizontally, 0..1023
word(0o13700); abs_ref('X')                  # MOV @#X, R0
word(0o63700); abs_ref('DX')                 # ADD @#DX, R0
word(0o5700)                                 # TST R0
br(0o100000, 'XOK')                          # BPL XOK
word(0o5437); abs_ref('DX')                  # NEG @#DX
word(0o5000)                                 # CLR R0
br(0o000400, 'XST')                          # BR XST
label('XOK')
word(0o20027); word(1023)                    # CMP R0, #1023
br(0o003400, 'XST')                          # BLE XST
word(0o5437); abs_ref('DX')                  # NEG @#DX
word(0o12700); word(1023)                    # MOV #1023, R0
label('XST')
word(0o10037); abs_ref('X')                  # MOV R0, @#X
word(0o10037); abs_ref('P1')                 # MOV R0, @#P1
word(0o52737); word(0o040000); abs_ref('P1') # BIS #040000, @#P1 (lit)

# bounce the dot vertically, 0..767
word(0o13700); abs_ref('Y')                  # MOV @#Y, R0
word(0o63700); abs_ref('DY')                 # ADD @#DY, R0
word(0o5700)                                 # TST R0
br(0o100000, 'YOK')                          # BPL YOK
word(0o5437); abs_ref('DY')                  # NEG @#DY
word(0o5000)                                 # CLR R0
br(0o000400, 'YST')                          # BR YST
label('YOK')
word(0o20027); word(767)                     # CMP R0, #767
br(0o003400, 'YST')                          # BLE YST
word(0o5437); abs_ref('DY')                  # NEG @#DY
word(0o12700); word(767)                     # MOV #767, R0
label('YST')
word(0o10037); abs_ref('Y')                  # MOV R0, @#Y
word(0o10037); abs_ref('P2')                 # MOV R0, @#P2

word(0o12600)                                # MOV (SP)+, R0
word(0o000002)                               # RTI

# --- display list at 01200 ----------------------------------------------------

org(LIST)
word(0o114000)                               # Point mode
label('P1W'); word(0o040000 | 512)           # lit point, x
label('P2W'); word(384)                      # y
word(0o110000)                               # LongVector mode
label('V1W'); word(0o040000 | 100)           # dx=+S, lit
word(0)                                      # dy=0
word(0o040000)                               # dx=0, lit
label('V4W'); word(100)                      # dy=+S
label('V5W'); word(0o060000 | 100)           # dx=-S, lit
word(0)                                      # dy=0
word(0o040000)                               # dx=0, lit
label('V8W'); word(0o020000 | 100)           # dy=-S
word(0o160000)                               # JMP
word(LIST)                                   # -> list start

labels['P1'] = labels['P1W']; labels['P2'] = labels['P2W']
labels['V1'] = labels['V1W']; labels['V4'] = labels['V4W']
labels['V5'] = labels['V5W']; labels['V8'] = labels['V8W']
labels['SIZE'] = SIZE; labels['DELTA'] = DELTA
labels['X'] = X; labels['Y'] = Y
labels['DX'] = DX; labels['DY'] = DY

# --- variables and vectors ----------------------------------------------------

org(SIZE);  word(100)    # initial square size
org(DELTA); word(2)      # grow by 2 per tick
org(X);     word(512)
org(Y);     word(384)
org(DX);    word(5)
org(DY);    word(3)

org(0o100); word(0o2000); word(0o300)   # clock vector: ISR, priority 6

resolve()

# --- emit .lda ----------------------------------------------------------------

bytemem = {}
for a, w in mem.items():
    bytemem[a] = w & 0xFF
    bytemem[a + 1] = (w >> 8) & 0xFF
out = sys.argv[1] if len(sys.argv) > 1 else 'demo.lda'
ldamod.write_lda(out, bytemem, 0o1000)
print(f"assembled {len(mem)} words -> {out}, start 0100")
