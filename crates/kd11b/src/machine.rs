//! A GT40: the KD11-B processor on the GT40's Unibus. Everything other than
//! the processor (core, VT11, line clock, host line, keyboard) is
//! `gt40::terminal::Terminal`, shared with the translation.

use crate::cpu::{Class, Outcome};
use crate::timing::Timing;
pub use gt40::terminal::Fault;
use gt40::terminal::{Terminal, PSW_ADDR};
use std::collections::HashMap;

pub struct Machine {
    /// Core, devices and machine time.
    pub bus: Terminal,
    pub r: [u16; 8],
    pub psw: u16,
    pub halted: bool,
    pub waiting: bool,
    pub timing: Timing,
    /// Instructions executed.
    pub instructions: u64,
    /// The last instruction word fetched (for diagnostics).
    pub last_insn: u16,
    /// Addresses of bus errors taken, for diagnostics.
    pub faults: Vec<u16>,
    /// Block-synchronous mode, for comparing the original binary with the
    /// translation, which can only be interrupted between basic blocks.
    /// When set (one flag per word of the address space), devices are
    /// brought up to time and interrupts taken only at block starts, and a
    /// stacked PSW is written with its condition codes cleared, since the
    /// translation keeps none between blocks. RTI restores the real codes
    /// from `shadow_cc`, so the program runs exactly as it would otherwise.
    pub block_starts: Option<Vec<bool>>,
    shadow_cc: HashMap<u16, u16>,
}

impl Machine {
    /// A GT40 with `words` words of core and a mains supply of `line_hz`.
    pub fn new(words: usize, line_hz: f64) -> Machine {
        Machine {
            bus: Terminal::new(words, line_hz),
            r: [0; 8],
            psw: 0,
            halted: false,
            waiting: false,
            timing: Timing::provisional(),
            instructions: 0,
            last_insn: 0,
            faults: Vec::new(),
            block_starts: None,
            shadow_cc: HashMap::new(),
        }
    }

    /// Load an absolute loader image and set the PC to its start address.
    pub fn load(&mut self, img: &gt40::lda::Image) {
        if let Some(s) = self.bus.load(img) {
            self.r[7] = s;
        }
    }

    /// Machine time, microseconds.
    pub fn t(&self) -> f64 {
        self.bus.t
    }

    // ---- bus, with the processor status word answered here ------------------

    pub fn read_word(&mut self, a: u16) -> Result<u16, Fault> {
        if a == PSW_ADDR {
            self.bus.refs += 1;
            if let Some(log) = &mut self.bus.access_log {
                log.push((false, a, self.psw));
            }
            return Ok(self.psw);
        }
        self.bus.read_word(a)
    }

    pub fn write_word(&mut self, a: u16, v: u16) -> Result<(), Fault> {
        if a == PSW_ADDR {
            self.bus.refs += 1;
            if let Some(log) = &mut self.bus.access_log {
                log.push((true, a, v));
            }
            self.psw = v & 0o357;
            return Ok(());
        }
        self.bus.write_word(a, v)
    }

    pub fn read_byte(&mut self, a: u16) -> Result<u8, Fault> {
        let w = self.read_word(a & !1)?;
        Ok(if a & 1 != 0 { (w >> 8) as u8 } else { w as u8 })
    }

    pub fn write_byte(&mut self, a: u16, v: u8) -> Result<(), Fault> {
        if a & !1 == PSW_ADDR {
            self.bus.refs += 1;
            if let Some(log) = &mut self.bus.access_log {
                log.push((true, a, v as u16));
            }
            if a & 1 == 0 {
                self.psw = ((self.psw & 0xFF00) | v as u16) & 0o357;
            }
            return Ok(());
        }
        self.bus.write_byte(a, v)
    }

    // ---- interrupts and time -------------------------------------------------

    fn at_block_start(&self) -> bool {
        match &self.block_starts {
            None => true,
            Some(b) => b.get((self.r[7] >> 1) as usize).copied().unwrap_or(false),
        }
    }

    fn take_interrupt(&mut self, vector: u16) -> Result<(), Fault> {
        self.bus.acknowledge(vector);
        self.trap(vector)
    }

    pub(crate) fn trap(&mut self, vector: u16) -> Result<(), Fault> {
        let (psw, pc) = (self.psw, self.r[7]);
        let stacked = if self.block_starts.is_some() {
            self.shadow_cc.insert(self.r[6].wrapping_sub(2), psw & 0o17);
            psw & !0o17
        } else {
            psw
        };
        self.push(stacked)?;
        self.push(pc)?;
        self.r[7] = self.read_word(vector)?;
        self.psw = self.read_word(vector + 2)?;
        Ok(())
    }

    /// The PSW popped by RTI from `addr`.
    pub(crate) fn unstack_psw(&mut self, addr: u16, v: u16) -> u16 {
        match self.shadow_cc.remove(&addr) {
            Some(cc) if self.block_starts.is_some() => (v & !0o17) | cc,
            _ => v,
        }
    }

    /// Advance the machine by one step: take an interrupt, execute one
    /// instruction, or sleep in WAIT until the next event.
    pub fn step(&mut self) {
        if self.halted {
            return;
        }
        if self.waiting || self.at_block_start() {
            self.bus.sync();
            if let Some((_, vector)) = self.bus.pending_interrupt((self.psw >> 5) & 7) {
                self.waiting = false;
                self.bus.refs = 0;
                if self.take_interrupt(vector).is_err() {
                    self.halted = true; // double bus error
                }
                self.bus.t += self.timing.interrupt();
                return;
            }
        }
        if self.waiting {
            self.bus.sleep();
            return;
        }
        self.bus.refs = 0;
        let start = self.bus.t;
        let result = self.execute();
        self.instructions += 1;
        let (outcome, class) = match result {
            Ok(e) => (e.outcome, e.class),
            Err(Fault(a)) => {
                self.faults.push(a);
                (Outcome::Trap(0o4), Class::Trap)
            }
        };
        let mut extra = 0.0;
        match outcome {
            Outcome::Normal => {}
            Outcome::Halt => self.halted = true,
            Outcome::Wait => self.waiting = true,
            Outcome::Reset => {
                self.bus.unibus_init();
                extra = self.timing.reset();
            }
            Outcome::Trap(v) => {
                if self.trap(v).is_err() {
                    self.halted = true;
                }
                extra = self.timing.interrupt();
            }
        }
        self.bus.t = start + self.timing.instruction(class, self.bus.refs) + extra;
    }

    /// Run until machine time reaches `until` or the processor halts.
    pub fn run_until(&mut self, until: f64) {
        while self.bus.t < until && !self.halted {
            self.step();
        }
        self.bus.sync_display();
    }
}
