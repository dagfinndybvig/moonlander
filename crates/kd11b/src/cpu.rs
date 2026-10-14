//! KD11-B processor (PDP-11/05, 11/10): the basic PDP-11 instruction set.
//!
//! Instructions the KD11-B lacks (EIS, FIS, SOB, XOR, SXT, MARK, RTT,
//! MFPI/MTPI, MFPS/MTPS, SPL) take the reserved instruction trap at 010, as
//! they would on a GT40. Semantics follow the PDP-11 processor handbooks;
//! condition codes are set exactly as DEC specifies for each instruction.

use crate::machine::{Fault, Machine};

pub const N: u16 = 0o10;
pub const Z: u16 = 0o4;
pub const V: u16 = 0o2;
pub const C: u16 = 0o1;

const SP: usize = 6;
const PC: usize = 7;

/// Where an operand lives.
#[derive(Clone, Copy, Debug)]
enum Loc {
    Reg(usize),
    Mem(u16),
}

/// What happened when an instruction ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Normal,
    Halt,
    Wait,
    Reset,
    /// A trap to the given vector (EMT, TRAP, IOT, BPT, reserved instruction,
    /// bus error).
    Trap(u16),
}

/// Instruction groups, as the manual divides the instruction set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    DoubleOperand,
    SingleOperand,
    Branch,
    Jump,
    Jsr,
    Rts,
    Rti,
    Trap,
    ConditionCode,
    Misc,
}

pub struct Executed {
    pub outcome: Outcome,
    pub class: Class,
    /// A branch that branched.
    pub taken: bool,
    /// A byte instruction whose source, or destination, is at an odd
    /// address (the KD11-B takes longer over those).
    pub odd_src: bool,
    pub odd_dst: bool,
}

impl Executed {
    fn new(outcome: Outcome, class: Class) -> Executed {
        Executed { outcome, class, taken: false, odd_src: false, odd_dst: false }
    }
}

/// A byte operand at an odd address in memory.
fn odd(l: Loc, byte: bool) -> bool {
    matches!(l, Loc::Mem(a) if byte && a & 1 != 0)
}

impl Machine {
    fn ea(&mut self, spec: u16, byte: bool) -> Result<Loc, Fault> {
        let mode = (spec >> 3) & 7;
        let r = (spec & 7) as usize;
        let step = if byte && r < SP { 1 } else { 2 };
        Ok(match mode {
            0 => Loc::Reg(r),
            1 => Loc::Mem(self.r[r]),
            2 => {
                let a = self.r[r];
                self.r[r] = a.wrapping_add(step);
                Loc::Mem(a)
            }
            3 => {
                let a = self.r[r];
                self.r[r] = a.wrapping_add(2);
                Loc::Mem(self.read_word(a)?)
            }
            4 => {
                self.r[r] = self.r[r].wrapping_sub(step);
                Loc::Mem(self.r[r])
            }
            5 => {
                self.r[r] = self.r[r].wrapping_sub(2);
                let a = self.r[r];
                Loc::Mem(self.read_word(a)?)
            }
            6 => {
                let x = self.fetch()?;
                Loc::Mem(self.r[r].wrapping_add(x))
            }
            _ => {
                let x = self.fetch()?;
                let a = self.r[r].wrapping_add(x);
                Loc::Mem(self.read_word(a)?)
            }
        })
    }

    fn fetch(&mut self) -> Result<u16, Fault> {
        let pc = self.r[PC];
        let w = self.read_word(pc)?;
        self.r[PC] = pc.wrapping_add(2);
        Ok(w)
    }

    fn get_w(&mut self, l: Loc) -> Result<u16, Fault> {
        match l {
            Loc::Reg(r) => Ok(self.r[r]),
            Loc::Mem(a) => self.read_word(a),
        }
    }

    fn put_w(&mut self, l: Loc, v: u16) -> Result<(), Fault> {
        match l {
            Loc::Reg(r) => {
                self.r[r] = v;
                Ok(())
            }
            Loc::Mem(a) => self.write_word(a, v),
        }
    }

    fn get_b(&mut self, l: Loc) -> Result<u8, Fault> {
        match l {
            Loc::Reg(r) => Ok(self.r[r] as u8),
            Loc::Mem(a) => self.read_byte(a),
        }
    }

    fn put_b(&mut self, l: Loc, v: u8) -> Result<(), Fault> {
        match l {
            Loc::Reg(r) => {
                self.r[r] = (self.r[r] & 0xFF00) | v as u16;
                Ok(())
            }
            Loc::Mem(a) => self.write_byte(a, v),
        }
    }

    pub fn push(&mut self, v: u16) -> Result<(), Fault> {
        self.r[SP] = self.r[SP].wrapping_sub(2);
        let sp = self.r[SP];
        self.write_word(sp, v)
    }

    pub fn pop(&mut self) -> Result<u16, Fault> {
        let sp = self.r[SP];
        let v = self.read_word(sp)?;
        self.r[SP] = sp.wrapping_add(2);
        Ok(v)
    }

    fn set_nz(&mut self, v: u16, byte: bool) {
        let (neg, zero) = if byte { (v & 0x80 != 0, v & 0xFF == 0) } else { (v & 0x8000 != 0, v == 0) };
        self.psw &= !(N | Z);
        if neg {
            self.psw |= N;
        }
        if zero {
            self.psw |= Z;
        }
    }

    fn set_flag(&mut self, f: u16, on: bool) {
        if on {
            self.psw |= f;
        } else {
            self.psw &= !f;
        }
    }

    fn flag(&self, f: u16) -> bool {
        self.psw & f != 0
    }

    /// Fetch and execute one instruction.
    pub(crate) fn execute(&mut self) -> Result<Executed, Fault> {
        let w = self.fetch()?;
        self.last_insn = w;
        let ok = |class| Ok(Executed::new(Outcome::Normal, class));
        let byte = w & 0o100000 != 0;
        let mask: u16 = if byte { 0xFF } else { 0xFFFF };
        let sign: u16 = if byte { 0x80 } else { 0x8000 };

        // Double operand group.
        let op = (w >> 12) & 7;
        if (1..=6).contains(&op) && !(op == 6 && byte) || (w >> 12) == 0o16 {
            let src_spec = (w >> 6) & 0o77;
            let dst_spec = w & 0o77;
            let opcode = w >> 12; // includes byte bit
            let sub = opcode == 0o16;
            let is_byte = byte && !sub;
            let sl = self.ea(src_spec, is_byte)?;
            let s = if is_byte { self.get_b(sl)? as u16 } else { self.get_w(sl)? };
            let dl = self.ea(dst_spec, is_byte)?;
            let (odd_src, odd_dst) = (odd(sl, is_byte), odd(dl, is_byte));
            match opcode & 0o7 {
                1 => {
                    // MOV, MOVB (MOVB to a register sign-extends)
                    if is_byte {
                        if let Loc::Reg(r) = dl {
                            self.r[r] = (s as u8 as i8) as i16 as u16;
                        } else {
                            self.put_b(dl, s as u8)?;
                        }
                    } else {
                        self.put_w(dl, s)?;
                    }
                    self.set_nz(s, is_byte);
                    self.set_flag(V, false);
                }
                2 => {
                    // CMP: src - dst, operands unchanged
                    let d = if is_byte { self.get_b(dl)? as u16 } else { self.get_w(dl)? };
                    let r = s.wrapping_sub(d) & mask;
                    self.set_nz(r, is_byte);
                    self.set_flag(V, (s ^ d) & sign != 0 && (d ^ r) & sign == 0);
                    self.set_flag(C, s & mask < d & mask);
                }
                3 => {
                    // BIT
                    let d = if is_byte { self.get_b(dl)? as u16 } else { self.get_w(dl)? };
                    self.set_nz(s & d, is_byte);
                    self.set_flag(V, false);
                }
                4 | 5 => {
                    // BIC, BIS
                    let d = if is_byte { self.get_b(dl)? as u16 } else { self.get_w(dl)? };
                    let r = if opcode & 7 == 4 { d & !s } else { d | s } & mask;
                    if is_byte {
                        self.put_b(dl, r as u8)?;
                    } else {
                        self.put_w(dl, r)?;
                    }
                    self.set_nz(r, is_byte);
                    self.set_flag(V, false);
                }
                _ => {
                    // ADD (op 6) or SUB (op 016): dst = dst +/- src, words only
                    let d = self.get_w(dl)?;
                    let (r, v, c) = if sub {
                        let r = d.wrapping_sub(s);
                        (r, (s ^ d) & 0x8000 != 0 && (s ^ r) & 0x8000 == 0, d < s)
                    } else {
                        let (r, c) = d.overflowing_add(s);
                        (r, (s ^ d) & 0x8000 == 0 && (s ^ r) & 0x8000 != 0, c)
                    };
                    self.put_w(dl, r)?;
                    self.set_nz(r, false);
                    self.set_flag(V, v);
                    self.set_flag(C, c);
                }
            }
            return Ok(Executed { odd_src, odd_dst, ..Executed::new(Outcome::Normal, Class::DoubleOperand) });
        }

        // Branches: high byte 001-007 and 200-207.
        let hb = w >> 8;
        if (0o1..=0o7).contains(&hb) || (0o200..=0o207).contains(&hb) {
            let (n, z, v, c) = (self.flag(N), self.flag(Z), self.flag(V), self.flag(C));
            let take = match hb {
                0o1 => true,
                0o2 => !z,
                0o3 => z,
                0o4 => n == v,
                0o5 => n != v,
                0o6 => !z && n == v,
                0o7 => z || n != v,
                0o200 => !n,
                0o201 => n,
                0o202 => !c && !z,
                0o203 => c || z,
                0o204 => !v,
                0o205 => v,
                0o206 => !c,
                _ => c,
            };
            if take {
                let off = (w & 0o377) as u8 as i8 as i16;
                self.r[PC] = self.r[PC].wrapping_add((off * 2) as u16);
            }
            return Ok(Executed { taken: take, ..Executed::new(Outcome::Normal, Class::Branch) });
        }

        match w {
            0o000000 => return Ok(Executed::new(Outcome::Halt, Class::Misc)),
            0o000001 => return Ok(Executed::new(Outcome::Wait, Class::Misc)),
            0o000002 => {
                self.r[PC] = self.pop()?;
                let at = self.r[SP];
                let psw = self.pop()?;
                self.psw = self.unstack_psw(at, psw);
                return ok(Class::Rti);
            }
            0o000003 => return Ok(Executed::new(Outcome::Trap(0o14), Class::Trap)),
            0o000004 => return Ok(Executed::new(Outcome::Trap(0o20), Class::Trap)),
            0o000005 => return Ok(Executed::new(Outcome::Reset, Class::Misc)),
            _ => {}
        }

        if (0o000100..=0o000177).contains(&w) {
            // JMP; register mode is illegal and traps to 4.
            if w & 0o70 == 0 {
                return Ok(Executed::new(Outcome::Trap(0o4), Class::Jump));
            }
            if let Loc::Mem(a) = self.ea(w & 0o77, false)? {
                self.r[PC] = a;
            }
            return ok(Class::Jump);
        }
        if (0o000200..=0o000207).contains(&w) {
            let r = (w & 7) as usize;
            self.r[PC] = self.r[r];
            self.r[r] = self.pop()?;
            return ok(Class::Rts);
        }
        if (0o000240..=0o000277).contains(&w) {
            let bits = w & 0o17;
            if w & 0o20 != 0 {
                self.psw |= bits;
            } else {
                self.psw &= !bits;
            }
            return ok(Class::ConditionCode);
        }
        if (0o000300..=0o000377).contains(&w) {
            // SWAB
            let l = self.ea(w & 0o77, false)?;
            let v = self.get_w(l)?;
            let r = v.rotate_left(8);
            self.put_w(l, r)?;
            self.set_nz(r & 0xFF, true);
            self.set_flag(V, false);
            self.set_flag(C, false);
            return ok(Class::SingleOperand);
        }
        if (0o004000..=0o004777).contains(&w) {
            // JSR; register-mode destination traps to 4.
            if w & 0o70 == 0 {
                return Ok(Executed::new(Outcome::Trap(0o4), Class::Jsr));
            }
            let reg = ((w >> 6) & 7) as usize;
            if let Loc::Mem(a) = self.ea(w & 0o77, false)? {
                let v = self.r[reg];
                self.push(v)?;
                self.r[reg] = self.r[PC];
                self.r[PC] = a;
            }
            return ok(Class::Jsr);
        }
        if (0o104000..=0o104377).contains(&w) {
            return Ok(Executed::new(Outcome::Trap(0o30), Class::Trap));
        }
        if (0o104400..=0o104777).contains(&w) {
            return Ok(Executed::new(Outcome::Trap(0o34), Class::Trap));
        }

        // Single operand group: 0050DD-0063DD and byte forms 1050DD-1063DD.
        let sop = (w >> 6) & 0o777;
        if (0o050..=0o063).contains(&sop) {
            let l = self.ea(w & 0o77, byte)?;
            let done = Ok(Executed { odd_dst: odd(l, byte), ..Executed::new(Outcome::Normal, Class::SingleOperand) });
            let d = if byte { self.get_b(l)? as u16 } else { self.get_w(l)? };
            let c_in = self.flag(C);
            let r = match sop {
                0o050 => {
                    self.psw &= !(N | V | C);
                    self.psw |= Z;
                    0
                }
                0o051 => {
                    let r = !d & mask;
                    self.set_nz(r, byte);
                    self.set_flag(V, false);
                    self.set_flag(C, true);
                    r
                }
                0o052 => {
                    let r = d.wrapping_add(1) & mask;
                    self.set_nz(r, byte);
                    self.set_flag(V, d == sign - 1);
                    r
                }
                0o053 => {
                    let r = d.wrapping_sub(1) & mask;
                    self.set_nz(r, byte);
                    self.set_flag(V, d == sign);
                    r
                }
                0o054 => {
                    let r = d.wrapping_neg() & mask;
                    self.set_nz(r, byte);
                    self.set_flag(V, r == sign);
                    self.set_flag(C, r != 0);
                    r
                }
                0o055 => {
                    let r = d.wrapping_add(c_in as u16) & mask;
                    self.set_nz(r, byte);
                    self.set_flag(V, d == sign - 1 && c_in);
                    self.set_flag(C, d == mask && c_in);
                    r
                }
                0o056 => {
                    let r = d.wrapping_sub(c_in as u16) & mask;
                    self.set_nz(r, byte);
                    self.set_flag(V, d == sign && c_in);
                    self.set_flag(C, d == 0 && c_in);
                    r
                }
                0o057 => {
                    self.set_nz(d, byte);
                    self.set_flag(V, false);
                    self.set_flag(C, false);
                    return done;
                }
                0o060 | 0o061 | 0o062 | 0o063 => {
                    let (r, c_out) = match sop {
                        0o060 => (((d >> 1) | if c_in { sign } else { 0 }) & mask, d & 1 != 0),
                        0o061 => (((d << 1) | c_in as u16) & mask, d & sign != 0),
                        0o062 => (((d >> 1) | (d & sign)) & mask, d & 1 != 0),
                        _ => ((d << 1) & mask, d & sign != 0),
                    };
                    self.set_nz(r, byte);
                    self.set_flag(C, c_out);
                    let n = self.flag(N);
                    self.set_flag(V, n != c_out);
                    r
                }
                _ => unreachable!(),
            };
            if byte {
                self.put_b(l, r as u8)?;
            } else {
                self.put_w(l, r)?;
            }
            return done;
        }

        // Everything else is reserved on the KD11-B: RTT, SPL, MARK, MFPI,
        // MTPI, SXT, MUL, DIV, ASH, ASHC, XOR, FIS, SOB, MTPS, MFPS, FPP.
        Ok(Executed::new(Outcome::Trap(0o10), Class::Trap))
    }
}
