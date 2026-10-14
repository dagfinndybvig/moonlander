//! A GT40 without its processor: core memory, the VT11 display processor,
//! the KW11-L line clock, the DL11 line to the host and the LK40 keyboard,
//! on one Unibus with one time base in microseconds.
//!
//! Two things drive it. The KD11-B processor model (crate `kd11b`) runs the
//! original binary on it; the translation (crate `moonlander`) runs Rust in
//! place of that binary on it. Both see the same memory, devices, interrupt
//! requests and display timing, because both use this code.

use crate::vt11::{DisplayBus, LightPen, Vt11};
use std::collections::VecDeque;

/// A bus error: odd address on a word access, or no device at the address.
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
    pub const MONITOR: u16 = 0o200;
    pub const IE: u16 = 0o100;

    pub fn period(&self) -> f64 {
        1_000_000.0 / self.hz
    }

    pub fn wants_interrupt(&self) -> bool {
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

/// The processor status word lives in the processor, not here; the
/// processor answers 177776 itself before asking the terminal.
pub const PSW_ADDR: u16 = 0o177776;

pub struct Terminal {
    /// Core memory, one entry per word.
    pub mem: Vec<u16>,
    /// Machine time, microseconds.
    pub t: f64,
    pub vt11: Vt11,
    pub clock: LineClock,
    pub host: Option<HostLine>,
    pub console: Console,
    /// Console switch register (177570, read) and display register (write).
    pub switches: u16,
    pub console_lights: u16,
    /// Whether the VT11 answers on the bus (off for processor-only tests).
    pub has_vt11: bool,
    /// Bus references made since this counter was last cleared; the
    /// processor model times its instructions by them.
    pub refs: u32,
    /// When set, every bus access is logged here: (write?, address, value).
    pub access_log: Option<Vec<(bool, u16, u16)>>,
    /// Times at which the bell was rung (any write to the display status
    /// register rings it).
    pub bells: Vec<f64>,
    /// Scheduled light pen movements: at each time, the pen goes to the
    /// given place (or away from the screen). Applied in time order as the
    /// display runs, so two runs given the same script see the same pen.
    pub pen_script: VecDeque<(f64, Option<LightPen>)>,
    /// Order-independent hash of all of core, kept up to date on every
    /// write. Two machines with equal hashes almost surely hold equal core.
    pub mem_hash: u64,
}

struct Core<'a>(&'a [u16]);

impl DisplayBus for Core<'_> {
    fn dma_read(&self, addr: u16) -> Option<u16> {
        self.0.get((addr >> 1) as usize).copied()
    }
}

fn mix(addr: u16, v: u16) -> u64 {
    // splitmix64 finaliser over (address, value)
    let mut z = ((addr as u64) << 16 | v as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Terminal {
    /// A GT40 with `words` words of core and a mains supply of `line_hz`.
    pub fn new(words: usize, line_hz: f64) -> Terminal {
        let mut t = Terminal {
            mem: vec![0; words],
            t: 0.0,
            vt11: Vt11::new(line_hz),
            clock: LineClock { hz: line_hz, csr: 0, next_tick: 1_000_000.0 / line_hz, ticks: 0 },
            host: Some(HostLine { xcsr: 0o200, ..Default::default() }),
            console: Console { tps: 0o200, ..Default::default() },
            switches: 0,
            console_lights: 0,
            has_vt11: true,
            refs: 0,
            access_log: None,
            bells: Vec::new(),
            pen_script: VecDeque::new(),
            mem_hash: 0,
        };
        t.unibus_init();
        t.rehash();
        t
    }

    /// Load an absolute loader image; returns its start address, if any.
    pub fn load(&mut self, img: &crate::lda::Image) -> Option<u16> {
        img.load_into(&mut self.mem);
        self.rehash();
        img.start
    }

    /// Recompute the core hash after writing `mem` directly.
    pub fn rehash(&mut self) {
        self.mem_hash = self.mem.iter().enumerate().fold(0, |h, (i, &v)| h ^ mix((i * 2) as u16, v));
    }

    /// Unibus INIT: the RESET instruction, or power up.
    pub fn unibus_init(&mut self) {
        self.vt11.reset();
        self.clock.csr = 0;
        if let Some(h) = &mut self.host {
            h.rcsr = 0;
            h.xcsr = 0o200;
        }
        self.console.tks = 0;
        self.console.tps = 0o200;
    }

    fn in_core(&self, a: u16) -> bool {
        (a as usize) < self.mem.len() * 2
    }

    fn store(&mut self, a: u16, v: u16) {
        let w = &mut self.mem[(a >> 1) as usize];
        self.mem_hash ^= mix(a, *w) ^ mix(a, v);
        *w = v;
    }

    // ---- bus -----------------------------------------------------------------

    pub fn read_word(&mut self, a: u16) -> Result<u16, Fault> {
        if a & 1 != 0 {
            return Err(Fault(a));
        }
        self.refs += 1;
        let v = if self.in_core(a) { Ok(self.mem[(a >> 1) as usize]) } else { self.io_read(a) };
        if let (Some(log), Ok(v)) = (&mut self.access_log, &v) {
            log.push((false, a, *v));
        }
        v
    }

    pub fn write_word(&mut self, a: u16, v: u16) -> Result<(), Fault> {
        if a & 1 != 0 {
            return Err(Fault(a));
        }
        if let Some(log) = &mut self.access_log {
            log.push((true, a, v));
        }
        self.refs += 1;
        if self.in_core(a) {
            self.store(a, v);
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
        if let Some(log) = &mut self.access_log {
            log.push((true, a, v as u16));
        }
        self.refs += 1;
        if self.in_core(wa) {
            let w = self.mem[(wa >> 1) as usize];
            let nw = if a & 1 != 0 { (w & 0x00FF) | ((v as u16) << 8) } else { (w & 0xFF00) | v as u16 };
            self.store(wa, nw);
            return Ok(());
        }
        self.io_write(wa, 0, Some((a & 1 != 0, v)))
    }

    fn io_read(&mut self, a: u16) -> Result<u16, Fault> {
        if !self.has_vt11 && (0o172000..=0o172006).contains(&a) {
            return Err(Fault(a));
        }
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
            0o177570 => self.switches,
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
        if !self.has_vt11 && (0o172000..=0o172006).contains(&a) {
            return Err(Fault(a));
        }
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
            0o172002 => self.bells.push(self.t), // any write rings the bell
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
            0o177570 => self.console_lights = v,
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

    // ---- display and time ----------------------------------------------------

    /// Run the display processor to `until`, moving the light pen as
    /// scripted on the way.
    fn run_display(&mut self, until: f64) {
        while let Some(&(at, pen)) = self.pen_script.front() {
            if at > until {
                break;
            }
            self.vt11.run_until(&Core(&self.mem), at);
            self.vt11.pen = pen;
            self.pen_script.pop_front();
        }
        self.vt11.run_until(&Core(&self.mem), until);
    }

    /// Bring the display processor up to machine time.
    pub fn sync_display(&mut self) {
        let t = self.t;
        self.run_display(t);
    }

    /// Count line clock ticks up to machine time.
    pub fn advance_clock(&mut self) {
        while self.clock.next_tick <= self.t {
            self.clock.next_tick += self.clock.period();
            self.clock.ticks += 1;
            self.clock.csr |= LineClock::MONITOR;
        }
    }

    /// Bring every device up to machine time.
    pub fn sync(&mut self) {
        self.sync_display();
        self.advance_clock();
    }

    /// The highest-priority interrupt a processor at `priority` will accept
    /// now, as (bus request level, vector).
    pub fn pending_interrupt(&self, priority: u16) -> Option<(u16, u16)> {
        if self.clock.wants_interrupt() && 6 > priority {
            return Some((6, 0o100));
        }
        if 4 > priority {
            if let Some(v) = self.vt11.interrupt_vector() {
                return Some((4, v));
            }
            if self.console.tks & 0o300 == 0o300 {
                return Some((4, 0o60));
            }
        }
        None
    }

    /// The processor has taken the interrupt at `vector`.
    pub fn acknowledge(&mut self, vector: u16) {
        match vector {
            0o100 => self.clock.csr &= !LineClock::MONITOR,
            0o320 | 0o324 | 0o330 => self.vt11.acknowledge(vector),
            _ => {}
        }
    }

    /// A processor in WAIT sleeps until the display raises an interrupt or
    /// the clock ticks. Machine time moves to that moment.
    pub fn sleep(&mut self) {
        self.vt11.irq_time = None;
        let tick = self.clock.next_tick;
        self.run_display(tick);
        self.t = match self.vt11.irq_time.take() {
            Some(t) if t < tick => t.max(self.t),
            _ => tick,
        };
    }
}
