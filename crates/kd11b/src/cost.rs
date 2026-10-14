//! Instruction lengths and times for code that stands in for PDP-11
//! instructions without executing them: the translation charges the time
//! the original instructions would have taken, instruction by instruction,
//! with the same values `Machine::step` uses.
//!
//! An instruction's time is fixed by its first word, except in two cases
//! (see `timing`): a conditional branch takes longer if it branches, and a
//! byte operand at an odd address takes longer. For those the translation
//! is given every possible time and picks the one that happened.

use crate::timing::Timing;

/// Extension words used by one operand specifier.
fn ext(spec: u16) -> u16 {
    let mode = (spec >> 3) & 7;
    let reg = spec & 7;
    match mode {
        6 | 7 => 1,
        2 | 3 if reg == 7 => 1,
        _ => 0,
    }
}

/// Length in words of the instruction whose first word is `w`, as the
/// KD11-B decodes it.
pub fn length(w: u16) -> u16 {
    let op = (w >> 12) & 7;
    let byte = w & 0o100000 != 0;
    // double operand: MOV CMP BIT BIC BIS (word and byte), ADD, SUB
    if (1..=6).contains(&op) && !(op == 6 && byte) || (w >> 12) == 0o16 {
        return 1 + ext((w >> 6) & 0o77) + ext(w & 0o77);
    }
    // JMP, SWAB, JSR, and the single operand group
    let sop = (w >> 6) & 0o777;
    if (0o000100..=0o000177).contains(&w)
        || (0o000300..=0o000377).contains(&w)
        || (0o004000..=0o004777).contains(&w)
        || (0o050..=0o063).contains(&sop)
    {
        return 1 + ext(w & 0o77);
    }
    1
}

/// How the translation touches a byte instruction's destination: the reads
/// and writes it makes, in order, which are what it can observe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// A register or an immediate: no memory access.
    None,
    /// Read only (CMPB, BITB, TSTB).
    Read,
    /// Written only (MOVB, CLRB).
    Write,
    /// Read, then written (the rest).
    ReadWrite,
}

/// The time to charge for one instruction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Time(f64),
    /// A conditional branch: the time if it does not branch, and if it does.
    Branch { not_taken: f64, taken: f64 },
    /// A byte instruction with an operand in memory: the time for each
    /// combination of an odd source address (bit 0) and an odd destination
    /// address (bit 1); whether the source is in memory, and how the
    /// destination is touched.
    Byte { times: [f64; 4], src: bool, dst: Access },
}

/// An operand specifier that addresses memory (not a register, and not an
/// immediate in the instruction stream).
fn in_memory(spec: u16) -> bool {
    let (mode, reg) = ((spec >> 3) & 7, spec & 7);
    mode != 0 && !(mode == 2 && reg == 7)
}

/// The time to charge for the instruction whose first word is `w`.
pub fn step(timing: &Timing, w: u16) -> Step {
    let t = |taken, s, d| timing.instruction(w, taken, s, d);
    let hb = w >> 8;
    if matches!(hb, 0o002..=0o007 | 0o200..=0o207) {
        return Step::Branch { not_taken: t(false, false, false), taken: t(true, false, false) };
    }
    let top = w >> 12;
    let sop = (w >> 6) & 0o777;
    let (src, dst) = if matches!(top, 0o11..=0o15) {
        let dst = match top {
            0o11 => Access::Write,
            0o12 | 0o13 => Access::Read,
            _ => Access::ReadWrite,
        };
        (in_memory(w >> 6), if in_memory(w) { dst } else { Access::None })
    } else if w & 0o100000 != 0 && (0o050..=0o063).contains(&sop) {
        let dst = match sop {
            0o050 => Access::Write,
            0o057 => Access::Read,
            _ => Access::ReadWrite,
        };
        (false, if in_memory(w) { dst } else { Access::None })
    } else {
        (false, Access::None)
    };
    if !src && dst == Access::None {
        return Step::Time(t(false, false, false));
    }
    let times = [t(false, false, false), t(false, true, false), t(false, false, true), t(false, true, true)];
    Step::Byte { times, src, dst }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths() {
        assert_eq!(length(0o010001), 1); // MOV R0,R1
        assert_eq!(length(0o012700), 2); // MOV #n,R0
        assert_eq!(length(0o013737), 3); // MOV @#a,@#b
        assert_eq!(length(0o016001), 2); // MOV x(R0),R1
        assert_eq!(length(0o004737), 2); // JSR PC,@#a
        assert_eq!(length(0o000137), 2); // JMP @#a
        assert_eq!(length(0o105737), 2); // TSTB @#a
        assert_eq!(length(0o000207), 1); // RTS PC
        assert_eq!(length(0o001001), 1); // BNE
        assert_eq!(length(0o060227), 2); // ADD #n,R2 (word only, op 6)
        assert_eq!(length(0o162737), 3); // SUB #n,@#a
    }

    #[test]
    fn steps() {
        let d = Timing::dec();
        assert_eq!(step(&d, 0o010001), Step::Time(3.1));
        assert_eq!(step(&d, 0o000403), Step::Time(2.5)); // BR always branches
        assert_eq!(step(&d, 0o001003), Step::Branch { not_taken: 1.9, taken: 2.5 });
        // MOVB (R5)+,(R2)+ reads a byte and writes one.
        match step(&d, 0o112522) {
            Step::Byte { src: true, dst: Access::Write, .. } => {}
            s => panic!("{s:?}"),
        }
        // CMPB (R3),#'0: the destination is an immediate.
        match step(&d, 0o121327) {
            Step::Byte { src: true, dst: Access::None, .. } => {}
            s => panic!("{s:?}"),
        }
        // INCB -1(R1): read, then written.
        match step(&d, 0o105261) {
            Step::Byte { src: false, dst: Access::ReadWrite, times } => assert_eq!(times[2], 9.2),
            s => panic!("{s:?}"),
        }
        // MOVB R1,R2: no memory operand at all.
        assert_eq!(step(&d, 0o110102), Step::Time(3.1));
    }
}
