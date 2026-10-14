//! A GT40: KD11-B processor, MM11 core memory, KW11-L line clock, VT11
//! display processor, DL11 line to the host and the LK40 keyboard, all on one
//! Unibus with one time base in microseconds.

use crate::cpu::{Class, Outcome};
use crate::timing::Timing;
use gt40::vt11::{DisplayBus, Vt11};

/// A bus error: odd address on a word access, or no device at the address.
/// It becomes a trap to 004.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault(pub u16);

/// KW11-L line clock at 177546.
#[derive(Clone, Debug)]
pub struct LineClock {
    pub hz: f64,
    pub csr: u16,
    pub next_tick: f64,
    pub ticks: u64,
}

impl LineClock {
    const MONITOR: u16 = 0o200;
    const IE: u16 = 0o100;
    fn period(&self) -> f64 {
        1_000_000.0 / self.hz
    }
    fn wants_interrupt(&self) -> bool {
        self.csr & Self::MONITOR != 0 && self.csr & Self::IE != 0
    }
}

/// DL11 asynchronous line to the host computer (175610-175616).
#[derive(Clone, Debug, Default)]
pub struct HostLine {
    pub rcsr: u16,
    pub xcsr: u16,
    /// Everything the program has sent to the host.
    pub sent: Vec<u8>,
}

/// LK40 keyboard and console registers (177560-177566).
#[derive(Clone, Debug, Default)]
pub struct Console {
    pub tks: u16,
    pub tkb: u16,
    pub tps: u16,
}

pub struct Machine {
    /// Core memory, one entry per word.
    pub mem: Vec<u16>,
    pub r: [u16; 8],
    pub psw: u16,
    /// Machine time, microseconds.
    pub t: f64,
    pub halted: bool,
    pub waiting: bool,
    pub vt11: Vt11,
    pub clock: LineClock,
    pub host: Option<HostLine>,
    pub console: Console,
    pub timing: Timing,
    /// Instructions executed.
    pub instructions: u64,
    /// The last instruction word fetched (for diagnostics).
    pub last_insn: u16,
    /// Memory references made by the instruction in progress.
    pub(crate) refs: u32,
}

struct Mem<'a>(&'a [u16]);

impl DisplayBus for Mem<'_> {
    fn dma_read(&self, addr: u16) -> Option<u16> {
        self.0.get((addr >> 1) as usize).copied()
    }
}

impl Machine {
    /// A GT40 with `words` words of core and a mains supply of `line_hz`.
    pub fn new(words: usize, line_hz: f64) -> Machine {
        let mut m = Machine {
            mem: vec![0; words],
            r: [0; 8],
            psw: 0,
            t: 0.0,
            halted: false,
            waiting: false,
            vt11: Vt11::new(line_hz),
            clock: LineClock { hz: line_hz, csr: 0, next_tick: 1_000_000.0 / line_hz, ticks: 0 },
            host: Some(HostLine { xcsr: 0o200, ..Default::default() }),
            console: Console { tps: 0o200, ..Default::default() },
            timing: Timing::provisional(),
            instructions: 0,
            last_insn: 0,
            refs: 0,
        };
        m.unibus_init();
        m
    }

    /// Load an absolute loader image and set the PC to its start address.
    pub fn load(&mut self, img: &gt40::lda::Image) {
        img.load_into(&mut self.mem);
        if let Some(s) = img.start {
            self.r[7] = s;
        }
    }

    fn unibus_init(&mut self) {
        self.vt11.reset();
        self.clock.csr = 0;
        if let Some(h) = &mut self.host {
            h.rcsr = 0;
            h.xcsr = 0o200;
        }
        self.console.tks = 0;
        self.console.tps = 0o200;
    }

    // ---- bus -----------------------------------------------------------------

    pub fn read_word(&mut self, a: u16) -> Result<u16, Fault> {
        if a & 1 != 0 {
            return Err(Fault(a));
        }
        self.refs += 1;
        if (a as usize) < self.mem.len() * 2 {
            return Ok(self.mem[(a >> 1) as usize]);
        }
        self.io_read(a)
    }

    pub fn write_word(&mut self, a: u16, v: u16) -> Result<(), Fault> {
        if a & 1 != 0 {
            return Err(Fault(a));
        }
        self.refs += 1;
        if (a as usize) < self.mem.len() * 2 {
            self.mem[(a >> 1) as usize] = v;
            return Ok(());
        }
        self.io_write(a, v, None)
    }

    pub fn read_byte(&mut self, a: u16) -> Result<u8, Fault> {
        let w = self.read_word(a & !1)?;
        Ok(if a & 1 != 0 { (w >> 8) as u8 } else { w as u8 })
    }

    pub fn write_byte(&mut self, a: u16, v: u8) -> Result<(), Fault> {
        let wa = a & !1;
        self.refs += 1;
        if (wa as usize) < self.mem.len() * 2 {
            let w = &mut self.mem[(wa >> 1) as usize];
            *w = if a & 1 != 0 { (*w & 0x00FF) | ((v as u16) << 8) } else { (*w & 0xFF00) | v as u16 };
            return Ok(());
        }
        self.io_write(wa, 0, Some((a & 1 != 0, v)))
    }

    fn io_read(&mut self, a: u16) -> Result<u16, Fault> {
        Ok(match a {
            0o172000 => self.vt11.read_dpc(),
            0o172002 => self.vt11.read_dsr(),
            0o172004 => self.vt11.read_xsr(),
            0o172006 => self.vt11.read_ysr(),
            0o177546 => self.clock.csr,
            0o177560 => self.console.tks,
            0o177562 => {
                self.console.tks &= !0o200;
                self.console.tkb
            }
            0o177564 => self.console.tps,
            0o177566 => 0,
            0o177776 => self.psw,
            0o175610..=0o175616 => match &mut self.host {
                Some(h) => match a {
                    0o175610 => h.rcsr,
                    0o175612 => 0,
                    0o175614 => h.xcsr,
                    _ => 0,
                },
                None => return Err(Fault(a)),
            },
            _ => return Err(Fault(a)),
        })
    }

    /// A write to the I/O page. `byte` carries (high byte?, value) for MOVB
    /// and friends; device registers here are written as words.
    fn io_write(&mut self, a: u16, v: u16, byte: Option<(bool, u8)>) -> Result<(), Fault> {
        let cur = if byte.is_some() { self.io_read(a)? } else { 0 };
        let v = match byte {
            Some((true, b)) => (cur & 0x00FF) | ((b as u16) << 8),
            Some((false, b)) => (cur & 0xFF00) | b as u16,
            None => v,
        };
        match a {
            0o172000 => {
                self.sync_display();
                self.vt11.write_dpc(v, self.t);
            }
            0o172002 => {} // rings the bell
            0o172004 | 0o172006 => {}
            0o177546 => {
                // Bit 6 is the interrupt enable; writing 0 to bit 7 clears
                // the monitor flag.
                self.clock.csr = (self.clock.csr & !0o100) | (v & 0o100);
                if v & 0o200 == 0 {
                    self.clock.csr &= !0o200;
                }
            }
            0o177560 => self.console.tks = (self.console.tks & 0o200) | (v & 0o100),
            0o177564 => self.console.tps = (self.console.tps & 0o200) | (v & 0o100),
            0o177566 => {}
            0o177776 => self.psw = v & 0o357,
            0o175610..=0o175616 => match &mut self.host {
                Some(h) => match a {
                    0o175610 => h.rcsr = v & 0o100,
                    0o175614 => h.xcsr = (h.xcsr & 0o200) | (v & 0o100),
                    0o175616 => h.sent.push(v as u8),
                    _ => {}
                },
                None => return Err(Fault(a)),
            },
            _ => return Err(Fault(a)),
        }
        Ok(())
    }

    /// Bring the display processor up to machine time.
    pub fn sync_display(&mut self) {
        let mem = Mem(&self.mem);
        self.vt11.run_until(&mem, self.t);
    }

    // ---- interrupts and time -------------------------------------------------

    fn advance_clock(&mut self) {
        while self.clock.next_tick <= self.t {
            self.clock.next_tick += self.clock.period();
            self.clock.ticks += 1;
            self.clock.csr |= LineClock::MONITOR;
        }
    }

    /// Highest-priority interrupt the processor will accept now.
    fn pending_interrupt(&self) -> Option<(u16, u16)> {
        let prio = (self.psw >> 5) & 7;
        if self.clock.wants_interrupt() && 6 > prio {
            return Some((6, 0o100));
        }
        if 4 > prio {
            if let Some(v) = self.vt11.interrupt_vector() {
                return Some((4, v));
            }
            if self.console.tks & 0o300 == 0o300 {
                return Some((4, 0o60));
            }
        }
        None
    }

    fn take_interrupt(&mut self, vector: u16) -> Result<(), Fault> {
        match vector {
            0o100 => self.clock.csr &= !LineClock::MONITOR,
            0o320 | 0o324 | 0o330 => self.vt11.acknowledge(vector),
            _ => {}
        }
        self.trap(vector)
    }

    fn trap(&mut self, vector: u16) -> Result<(), Fault> {
        let (psw, pc) = (self.psw, self.r[7]);
        self.push(psw)?;
        self.push(pc)?;
        self.r[7] = self.read_word(vector)?;
        self.psw = self.read_word(vector + 2)?;
        Ok(())
    }

    /// Advance the machine by one step: take an interrupt, execute one
    /// instruction, or sleep in WAIT until the next event.
    pub fn step(&mut self) {
        if self.halted {
            return;
        }
        self.sync_display();
        self.advance_clock();
        if let Some((_, vector)) = self.pending_interrupt() {
            self.waiting = false;
            self.refs = 0;
            if self.take_interrupt(vector).is_err() {
                self.halted = true; // double bus error
            }
            self.t += self.timing.interrupt();
            return;
        }
        if self.waiting {
            // Sleep until the display raises an interrupt or the clock ticks.
            let mem = Mem(&self.mem);
            self.vt11.irq_time = None;
            self.vt11.run_until(&mem, self.clock.next_tick);
            self.t = match self.vt11.irq_time.take() {
                Some(t) if t < self.clock.next_tick => t.max(self.t),
                _ => self.clock.next_tick,
            };
            return;
        }
        self.refs = 0;
        let start = self.t;
        let result = self.execute();
        self.instructions += 1;
        let (outcome, class) = match result {
            Ok(e) => (e.outcome, e.class),
            Err(_) => (Outcome::Trap(0o4), Class::Trap),
        };
        let mut extra = 0.0;
        match outcome {
            Outcome::Normal => {}
            Outcome::Halt => self.halted = true,
            Outcome::Wait => self.waiting = true,
            Outcome::Reset => {
                self.unibus_init();
                extra = self.timing.reset();
            }
            Outcome::Trap(v) => {
                if self.trap(v).is_err() {
                    self.halted = true;
                }
                extra = self.timing.interrupt();
            }
        }
        self.t = start + self.timing.instruction(class, self.refs) + extra;
    }

    /// Run until machine time reaches `until` or the processor halts.
    pub fn run_until(&mut self, until: f64) {
        while self.t < until && !self.halted {
            self.step();
        }
        self.sync_display();
    }
}
