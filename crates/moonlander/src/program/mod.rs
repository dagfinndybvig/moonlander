//! The translated program, one module per section of Burness's source.

mod arith;

use crate::generated::symbols::*;
use crate::machine::{Escape, Flow, Moonlander};

/// ASL: shift left; the carry is the bit shifted out.
pub(crate) fn asl(v: u16) -> (u16, bool) {
    (v << 1, v & 0x8000 != 0)
}

/// ROL: rotate left through the carry.
pub(crate) fn rol(v: u16, c: bool) -> (u16, bool) {
    ((v << 1) | c as u16, v & 0x8000 != 0)
}

/// ASR: arithmetic shift right.
pub(crate) fn asr(v: u16) -> u16 {
    ((v as i16) >> 1) as u16
}

/// The N condition code a result would set.
pub(crate) fn minus(v: u16) -> bool {
    v & 0x8000 != 0
}

/// A translated routine: its name in the source, its address in the
/// original, and the translation.
pub struct Routine {
    pub name: &'static str,
    pub addr: u16,
    pub run: fn(&mut Moonlander) -> Flow,
}

/// Every routine that is entered by JSR, so tests can call each one in the
/// original and in the translation from the same state.
pub static ROUTINES: &[Routine] = &[
    Routine { name: "MULTWO", addr: MULTWO, run: Moonlander::multwo },
    Routine { name: "SGNMUL", addr: SGNMUL, run: Moonlander::sgnmul },
    Routine { name: "TRGMUL", addr: TRGMUL, run: Moonlander::trgmul },
    Routine { name: "DIVTWO", addr: DIVTWO, run: Moonlander::divtwo },
];

/// Look up a routine by its name in the source.
pub fn routine(name: &str) -> &'static Routine {
    ROUTINES.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no routine {name}"))
}

impl Moonlander {
    /// Run the interrupt routine at `handler`, as the vector names it.
    pub(crate) fn dispatch(&mut self, handler: u16) -> Flow {
        match handler {
            _ => Err(Escape::Wild(handler)),
        }
    }
}
