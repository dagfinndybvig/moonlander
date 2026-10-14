//! The translated program, one module per section of Burness's source.

mod arith;
mod dial;
mod draw;
mod explode;
mod interrupts;
mod moon;
mod palsy;
mod rocket;
mod show;
mod start;

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

/// DEC then BGT: whether the branch is taken after decrementing `old`.
/// DEC sets V when it steps from 100000 to 077777, and BGT then fails.
pub(crate) fn dec_bgt(old: u16) -> bool {
    old != 0x8000 && (old.wrapping_sub(1) as i16) > 0
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
    Routine { name: "TRIG", addr: TRIG, run: Moonlander::trig },
    Routine { name: "DRAW", addr: DRAW, run: Moonlander::draw },
    Routine { name: "DRAWRT", addr: DRAWRT, run: Moonlander::drawrt },
    Routine { name: "ROCKET", addr: ROCKET, run: Moonlander::rocket },
    Routine { name: "FLAME", addr: FLAME, run: Moonlander::flame },
    Routine { name: "WHERE", addr: WHERE, run: Moonlander::r#where },
    Routine { name: "DIAL", addr: DIAL, run: Moonlander::dial },
    Routine { name: "ASCII", addr: ASCII, run: Moonlander::ascii },
    Routine { name: "SASCII", addr: SASCII, run: Moonlander::sascii },
    Routine { name: "DFAKE", addr: DFAKE, run: Moonlander::dfake },
    Routine { name: "PUTFET", addr: PUTFET, run: Moonlander::putfet },
    Routine { name: "GETFET", addr: GETFET, run: Moonlander::getfet },
    Routine { name: "ALTER", addr: ALTER, run: Moonlander::alter },
    Routine { name: "DRAWM1", addr: DRAWM1, run: Moonlander::drawm1 },
    Routine { name: "DRAWM2", addr: DRAWM2, run: Moonlander::drawm2 },
    Routine { name: "DUST", addr: DUST, run: Moonlander::dust },
    Routine { name: "INTEL", addr: INTEL, run: Moonlander::intel },
    Routine { name: "SHOWSP", addr: SHOWSP, run: Moonlander::showsp },
    Routine { name: "MOVMAN", addr: MOVMAN, run: Moonlander::movman },
];

/// Look up a routine by its name in the source.
pub fn routine(name: &str) -> &'static Routine {
    ROUTINES.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no routine {name}"))
}

impl Moonlander {
    /// Run the interrupt routine at `handler`, as the vector names it.
    pub(crate) fn dispatch(&mut self, handler: u16) -> Flow {
        match handler {
            TIMER => self.timer(),
            DISPLY => self.disply(),
            LIGHT => self.light(),
            DBUSE => self.dbuse(),
            POWERF => self.powerf(),
            // The trap catcher: each unexpected trap halts at its own address.
            TRAP04 | TRAP10 | TRAP14 | TRAP20 | TRAP30 => {
                let b = crate::generated::blocks::ALL.iter().find(|b| b.addr == handler).copied();
                self.at(b.expect("trap catcher block"))?;
                Err(Escape::Halt(handler))
            }
            _ => Err(Escape::Wild(handler)),
        }
    }
}
