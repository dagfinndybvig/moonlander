//! Static instruction lengths and times, for code that stands in for
//! PDP-11 instructions without executing them (the translation charges the
//! time the original instructions would have taken).
//!
//! Times are not computed by a second copy of the timing rules: each
//! instruction is executed once, in a sandbox machine where every address
//! it can form is valid, and the same expression `Machine::step` uses is
//! applied to what it did. Nothing about an 11/05 instruction's time depends
//! on its data, only on its addressing modes, so one execution is enough.

use crate::cpu::{Class, Outcome};
use crate::timing::Timing;
use crate::Machine;

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

/// What an instruction costs: `instruction` is `Timing::instruction` for
/// it, `extra` the RESET or trap time added after it. `Machine::step`
/// computes `t = (t + instruction) + extra`; a translation reproducing its
/// time exactly must add the two in that order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cost {
    pub instruction: f64,
    pub extra: f64,
}

/// The cost of the instruction in `words` (first word, then any extension
/// words), when it completes without a bus error.
pub fn cost(timing: &Timing, words: &[u16]) -> Cost {
    // 28K words: every address below the I/O page exists. Registers and
    // memory hold 040000, so register, deferred, autoincrement,
    // autodecrement and indexed operands all land in core.
    let mut m = Machine::new(0o70000, 60.0);
    m.timing = timing.clone();
    m.bus.mem.iter_mut().for_each(|w| *w = 0o40000);
    let pc: u16 = 0o100000;
    for (i, &w) in words.iter().enumerate() {
        m.bus.mem[(pc >> 1) as usize + i] = w;
    }
    m.bus.rehash();
    m.r = [0o40000; 8];
    m.r[7] = pc;
    m.psw = 0o340;
    m.bus.refs = 0;
    let e = m.execute().expect("sandbox instruction faulted");
    let instruction = timing.instruction(e.class, m.bus.refs);
    let extra = match e.outcome {
        Outcome::Reset => timing.reset(),
        Outcome::Trap(_) => timing.interrupt(),
        _ => 0.0,
    };
    Cost { instruction, extra }
}

/// The cost of an instruction that takes a bus error after `refs` bus
/// references: `Machine::step` charges it as a trap.
pub fn faulted(timing: &Timing, refs: u32) -> Cost {
    Cost { instruction: timing.instruction(Class::Trap, refs), extra: timing.interrupt() }
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
    fn costs_follow_refs() {
        let t = Timing::provisional();
        let reg = cost(&t, &[0o010001]); // MOV R0,R1: fetch only
        assert_eq!(reg.instruction, t.instruction(Class::DoubleOperand, 1));
        let abs = cost(&t, &[0o013737, 0o1000, 0o1002]); // MOV @#a,@#b: 1+1+1+1+1
        assert_eq!(abs.instruction, t.instruction(Class::DoubleOperand, 5));
        let reset = cost(&t, &[0o000005]);
        assert_eq!(reset.extra, t.reset());
    }
}
