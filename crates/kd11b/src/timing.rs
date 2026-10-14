//! Instruction timing for the KD11-B with MM11 core memory.
//!
//! PROVISIONAL. The table below is a first-order model, not DEC's figures:
//! each memory reference costs one core cycle plus Unibus overhead, and each
//! instruction class adds microcode time. It will be replaced by the timing
//! tables of the PDP-11/05, 11/10 Computer Manual (DEC-11-H05AA) once that
//! document is in hand. Timing only affects pacing: how many line-clock ticks
//! pass while the program computes a frame. It never affects what an
//! instruction does.

use crate::cpu::Class;

#[derive(Clone, Debug)]
pub struct Timing {
    /// Microseconds per memory reference (instruction fetch, extension word,
    /// operand read or write, stack push or pop).
    pub per_ref: f64,
    /// Microcode overhead per instruction class, microseconds.
    pub double_operand: f64,
    pub single_operand: f64,
    pub branch: f64,
    pub jump: f64,
    pub jsr: f64,
    pub rts: f64,
    pub rti: f64,
    pub trap: f64,
    pub condition_code: f64,
    pub misc: f64,
    /// Interrupt or trap service, beyond its memory references.
    pub interrupt: f64,
    /// RESET: the Unibus INIT pulse.
    pub reset: f64,
    pub source: &'static str,
}

impl Timing {
    pub fn provisional() -> Timing {
        Timing {
            per_ref: 1.5,
            double_operand: 1.2,
            single_operand: 1.0,
            branch: 0.9,
            jump: 0.9,
            jsr: 1.5,
            rts: 1.2,
            rti: 1.5,
            trap: 2.0,
            condition_code: 0.9,
            misc: 1.0,
            interrupt: 6.0,
            reset: 20_000.0,
            source: "provisional first-order model",
        }
    }

    pub fn instruction(&self, class: Class, refs: u32) -> f64 {
        let k = match class {
            Class::DoubleOperand => self.double_operand,
            Class::SingleOperand => self.single_operand,
            Class::Branch => self.branch,
            Class::Jump => self.jump,
            Class::Jsr => self.jsr,
            Class::Rts => self.rts,
            Class::Rti => self.rti,
            Class::Trap => self.trap,
            Class::ConditionCode => self.condition_code,
            Class::Misc => self.misc,
        };
        k + self.per_ref * refs as f64
    }

    pub fn interrupt(&self) -> f64 {
        self.interrupt + 4.0 * self.per_ref
    }

    pub fn reset(&self) -> f64 {
        self.reset
    }
}
