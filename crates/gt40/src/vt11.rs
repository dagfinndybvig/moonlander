//! The DEC VT11 graphic display processor (DPU) of the GT40.
//!
//! The DPU fetches a display file from memory by DMA and drives the CRT beam.
//! This model executes display files word by word, keeps the DPU's own clock
//! in microseconds, and reports everything the beam does as [`Beam`] events
//! with time stamps, so a renderer can reproduce phosphor persistence and a
//! test can check geometry exactly.
//!
//! Sources, in `reference/hardware-docs`:
//! - GT40/GT42 User's Guide (EK-GT40-OP-002, Feb 1975), chapter 4: registers,
//!   stop interrupt, light pen interaction, edge violations, flicker.
//! - VT11 Graphic Display Processor manual (EK-VT11-TM-001, 1974), chapter 2:
//!   vector normalization and drawing rate, character clock, blink, sync,
//!   light pen timing.
//! - Al Kossow's VT11 instruction set summary (1993).
//!
//! Modelling decisions are marked MODEL in the comments.

use std::collections::VecDeque;
use crate::chargen;

/// Visible face of the GT40's VR14: 1024 x 768 raster units, origin bottom left.
pub const FACE_W: i32 = 1024;
pub const FACE_H: i32 = 768;
/// Character pitch and line pitch at the GT40's normal jumper settings.
pub const CHAR_PITCH: i32 = 14;
pub const LINE_PITCH: i32 = 24;

/// MODEL: one DMA word fetch, core DATI cycle 900 ns plus decode.
const T_FETCH: f64 = 1.2;
/// MODEL: absolute positioning, DAC load (0.8 us) and settle (about 2 us).
const T_POSITION: f64 = 3.0;
/// MODEL: one character, 48 dots at the 400 ns character clock plus the
/// 0.98-1.4 us scope delay.
const T_CHAR: f64 = 20.6;
/// MODEL: a non-printing character code.
const T_CONTROL_CHAR: f64 = 1.0;
/// Light pen amplifier inhibit after a hit (VT11 manual, appendix B).
const T_LP_INHIBIT: f64 = 70.0;
/// Blink flip-flop half period (VT11 manual 2.2.9).
const T_BLINK_HALF: f64 = 266_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Char,
    ShortVector,
    LongVector,
    Point,
    GraphX,
    GraphY,
    RelativePoint,
    Spare,
}

impl Mode {
    fn from_bits(b: u16) -> Mode {
        [Mode::Char, Mode::ShortVector, Mode::LongVector, Mode::Point, Mode::GraphX,
         Mode::GraphY, Mode::RelativePoint, Mode::Spare][(b & 7) as usize]
    }
    fn bits(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineType {
    Solid,
    LongDash,
    ShortDash,
    DotDash,
}

/// What the beam did. Coordinates are raster units, time is DPU microseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Beam {
    /// An intensified straight run, drawn at uniform speed from t0 to t1.
    Line { x0: f32, y0: f32, x1: f32, y1: f32, t0: f64, t1: f64, level: u8 },
    /// A single intensified spot: a point, a character dot, a light pen flash.
    Dot { x: f32, y: f32, t: f64, level: u8 },
}

/// The light pen, held over the face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightPen {
    pub x: f32,
    pub y: f32,
    /// Radius of the pen's field of view, raster units.
    pub radius: f32,
}

/// Display memory as the DPU sees it over the Unibus.
pub trait DisplayBus {
    /// A word read by DMA, or None for a non-existent address (time-out).
    fn dma_read(&self, addr: u16) -> Option<u16>;
}

impl DisplayBus for [u16] {
    fn dma_read(&self, addr: u16) -> Option<u16> {
        self.get((addr >> 1) as usize).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Run {
    /// Not started since reset.
    Idle,
    Running,
    /// Stopped by a stop instruction, a light pen hit, or a time-out.
    Stopped,
    /// Waiting for the next line-frequency pulse (status A sync).
    Sync,
}

/// Work interrupted by a light pen hit, finished on resume.
#[derive(Clone, Copy, Debug)]
enum Pending {
    /// Rest of a vector: remaining displacement and whether it is intensified.
    Vector { dx: i32, dy: i32, lit: bool, steps_total: i32, steps_left: i32 },
    /// Second character of a character word.
    Char(u8),
}

#[derive(Clone, Debug)]
pub struct Vt11 {
    // Display program counter and run state.
    dpc: u16,
    run: Run,
    pending: Option<Pending>,
    // Graphic mode register and parameters.
    mode: Mode,
    intensity: u8,
    lp_enable: bool,
    blink: bool,
    line_type: LineType,
    // Status register A and B.
    stop_int_enable: bool,
    lp_intensify: bool,
    italics: bool,
    graphplot_inc: u16,
    // Beam position counters, 12 bits each (the "paper" is 4096 square).
    x: u16,
    y: u16,
    edge: bool,
    shift_out: bool,
    char_reg: u8,
    lp_flag: bool,
    stop_flag: bool,
    line_counter: u8,
    // Interrupt requests to the CPU (all at BR4).
    pub stop_irq: bool,
    pub lp_irq: bool,
    pub timeout_irq: bool,
    /// DPU time at which the most recent interrupt request was raised.
    pub irq_time: Option<f64>,
    // Light pen.
    pub pen: Option<LightPen>,
    /// Pen movements to come, in time order: at each DPU time the pen goes
    /// to the given place, or away. The pen is looked up by the time of each
    /// point the beam draws, so a movement takes effect at its moment however
    /// the display's running is divided up.
    pub pen_moves: VecDeque<(f64, Option<LightPen>)>,
    lp_inhibit_until: f64,
    // DPU clock and the next line-frequency pulse, microseconds.
    t: f64,
    next_line_pulse: f64,
    line_period: f64,
    /// Beam events since the last drain.
    pub out: Vec<Beam>,
}

impl Vt11 {
    /// A VT11 synchronised to a mains supply of `line_hz`.
    pub fn new(line_hz: f64) -> Vt11 {
        let mut v = Vt11 {
            dpc: 0,
            run: Run::Idle,
            pending: None,
            mode: Mode::Char,
            intensity: 4,
            lp_enable: false,
            blink: false,
            line_type: LineType::Solid,
            stop_int_enable: false,
            lp_intensify: true,
            italics: false,
            graphplot_inc: 0,
            x: 0,
            y: 0,
            edge: false,
            shift_out: false,
            char_reg: 0,
            lp_flag: false,
            stop_flag: false,
            line_counter: 0,
            stop_irq: false,
            lp_irq: false,
            timeout_irq: false,
            irq_time: None,
            pen: None,
            pen_moves: VecDeque::new(),
            lp_inhibit_until: 0.0,
            t: 0.0,
            next_line_pulse: 1_000_000.0 / line_hz,
            line_period: 1_000_000.0 / line_hz,
            out: Vec::new(),
        };
        v.reset();
        v
    }

    /// Unibus INIT (the RESET instruction, or power up).
    pub fn reset(&mut self) {
        self.run = Run::Idle;
        self.pending = None;
        self.mode = Mode::Char;
        self.stop_int_enable = false;
        self.lp_enable = false;
        self.lp_intensify = true;
        self.italics = false;
        self.blink = false;
        self.line_type = LineType::Solid;
        self.shift_out = false;
        self.stop_irq = false;
        self.lp_irq = false;
        self.timeout_irq = false;
        self.lp_flag = false;
        self.stop_flag = false;
        self.edge = false;
    }

    /// DPU clock, microseconds.
    pub fn time(&self) -> f64 {
        self.t
    }

    /// Where the display processor is: (DPC, X, Y, its clock). Two models
    /// given the same display file and the same light pen agree on this.
    pub fn whereabouts(&self) -> (u16, u16, u16, f64) {
        (self.dpc, self.x, self.y, self.t)
    }

    pub fn is_running(&self) -> bool {
        matches!(self.run, Run::Running | Run::Sync)
    }

    // ---- Unibus registers ------------------------------------------------

    /// 172000, display program counter.
    pub fn read_dpc(&self) -> u16 {
        self.dpc
    }

    /// 172000 written. An even value starts the display there; an odd value
    /// (bit 0 set) resumes a stopped display where it left off.
    pub fn write_dpc(&mut self, v: u16, now: f64) {
        self.catch_up(now);
        if v & 1 != 0 {
            if self.run == Run::Stopped {
                self.run = Run::Running;
            }
        } else {
            self.dpc = v;
            self.pending = None;
            self.run = Run::Running;
        }
        self.stop_flag = false;
        self.lp_flag = false;
    }

    /// 172002, display status register.
    pub fn read_dsr(&self) -> u16 {
        let lt = self.line_type as u16;
        ((self.stop_flag as u16) << 15)
            | (self.mode.bits() << 11)
            | ((self.intensity as u16 & 7) << 8)
            | ((self.lp_flag as u16) << 7)
            | ((self.shift_out as u16) << 6)
            | ((self.edge as u16) << 5)
            | ((self.italics as u16) << 4)
            | ((self.blink as u16) << 3)
            | lt
    }

    /// 172004, X position (bits 9-0) and graphplot increment (bits 15-10).
    pub fn read_xsr(&self) -> u16 {
        (self.x & 0o1777) | ((self.graphplot_inc & 0o77) << 10)
    }

    /// 172006, Y position (bits 9-0) and character register (bits 15-10).
    pub fn read_ysr(&self) -> u16 {
        (self.y & 0o1777) | (((self.char_reg as u16) & 0o77) << 10)
    }

    /// Highest-priority pending interrupt vector, if any. All VT11
    /// interrupts are requested at BR4.
    pub fn interrupt_vector(&self) -> Option<u16> {
        if self.stop_irq {
            Some(0o320)
        } else if self.lp_irq {
            Some(0o324)
        } else if self.timeout_irq {
            Some(0o330)
        } else {
            None
        }
    }

    /// The CPU has taken the interrupt at `vector`.
    pub fn acknowledge(&mut self, vector: u16) {
        match vector {
            0o320 => self.stop_irq = false,
            0o324 => self.lp_irq = false,
            0o330 => self.timeout_irq = false,
            _ => {}
        }
    }

    // ---- execution ---------------------------------------------------------

    fn catch_up(&mut self, now: f64) {
        if self.t < now && !self.is_running() {
            self.t = now;
        }
        while self.next_line_pulse <= self.t {
            self.next_line_pulse += self.line_period;
        }
    }

    /// Run the display processor until its clock reaches `until` or it stops.
    pub fn run_until<B: DisplayBus + ?Sized>(&mut self, bus: &B, until: f64) {
        self.settle_pen();
        loop {
            while self.next_line_pulse <= self.t {
                self.next_line_pulse += self.line_period;
            }
            match self.run {
                // A stopped display keeps its clock at the moment it stopped;
                // the CPU's next write to the DPC catches it up.
                Run::Idle | Run::Stopped => return,
                Run::Sync => {
                    if self.next_line_pulse > until {
                        self.t = until;
                        return;
                    }
                    self.t = self.next_line_pulse;
                    self.run = Run::Running;
                }
                Run::Running => {
                    if self.t >= until {
                        return;
                    }
                    if let Some(p) = self.pending.take() {
                        self.finish_pending(p);
                    } else {
                        self.step(bus);
                    }
                }
            }
        }
    }

    fn fetch<B: DisplayBus + ?Sized>(&mut self, bus: &B) -> Option<u16> {
        self.t += T_FETCH;
        match bus.dma_read(self.dpc) {
            Some(w) => {
                self.dpc = self.dpc.wrapping_add(2);
                Some(w)
            }
            None => {
                self.timeout_irq = true;
                self.irq_time = Some(self.t);
                self.run = Run::Stopped;
                self.stop_flag = true;
                None
            }
        }
    }

    /// Execute one instruction or data element.
    fn step<B: DisplayBus + ?Sized>(&mut self, bus: &B) {
        let Some(w) = self.fetch(bus) else { return };
        if w & 0o100000 != 0 {
            self.control(bus, w);
        } else {
            self.data(bus, w);
        }
    }

    fn control<B: DisplayBus + ?Sized>(&mut self, bus: &B, w: u16) {
        let op = (w >> 11) & 0o17;
        match op {
            0..=7 => {
                self.mode = Mode::from_bits(op);
                if w & 0o2000 != 0 {
                    self.intensity = ((w >> 7) & 7) as u8;
                }
                if w & 0o100 != 0 {
                    self.lp_enable = w & 0o40 != 0;
                }
                if w & 0o20 != 0 {
                    self.blink = w & 0o10 != 0;
                }
                if w & 0o4 != 0 {
                    self.line_type = [LineType::Solid, LineType::LongDash, LineType::ShortDash,
                                      LineType::DotDash][(w & 3) as usize];
                }
            }
            0o14 => {
                // Jump: the next word is the new display program counter.
                if let Some(target) = self.fetch(bus) {
                    self.dpc = target & !1;
                }
            }
            0o15 => {} // no-op
            0o16 => {
                // Load status register A.
                if w & 0o1000 != 0 {
                    self.stop_int_enable = w & 0o400 != 0;
                }
                if w & 0o200 != 0 {
                    // bit 6 = 0: intensify the point of light pen interaction
                    self.lp_intensify = w & 0o100 == 0;
                }
                if w & 0o40 != 0 {
                    self.italics = w & 0o20 != 0;
                }
                if w & 0o2000 != 0 {
                    // Stop. If sync is also requested, sync overrides stop
                    // (GT40 User's Guide 4.7.1).
                    if w & 0o4 != 0 {
                        self.run = Run::Sync;
                    } else {
                        self.run = Run::Stopped;
                        self.stop_flag = true;
                        if self.stop_int_enable {
                            self.stop_irq = true;
                            self.irq_time = Some(self.t);
                        }
                    }
                } else if w & 0o4 != 0 {
                    self.run = Run::Sync;
                }
            }
            0o17 => {
                // Load status register B.
                if w & 0o100 != 0 {
                    self.graphplot_inc = w & 0o77;
                }
            }
            _ => {} // 10-13: undefined, treated as no-op (MODEL)
        }
    }

    fn data<B: DisplayBus + ?Sized>(&mut self, bus: &B, w: u16) {
        match self.mode {
            Mode::Char => {
                let c1 = (w & 0o177) as u8;
                let c2 = ((w >> 8) & 0o177) as u8;
                if self.character(c1) {
                    self.pending = Some(Pending::Char(c2));
                    return;
                }
                self.character(c2);
            }
            Mode::ShortVector | Mode::RelativePoint => {
                let lit = w & 0o40000 != 0;
                let dx = ((w >> 7) & 0o77) as i32 * if w & 0o20000 != 0 { -1 } else { 1 };
                let dy = (w & 0o77) as i32 * if w & 0o100 != 0 { -1 } else { 1 };
                if self.mode == Mode::ShortVector {
                    self.vector(dx, dy, lit);
                } else {
                    self.relative_point(dx, dy, lit);
                }
            }
            Mode::LongVector => {
                let Some(w2) = self.fetch(bus) else { return };
                let lit = w & 0o40000 != 0;
                let dx = (w & 0o1777) as i32 * if w & 0o20000 != 0 { -1 } else { 1 };
                let dy = (w2 & 0o1777) as i32 * if w2 & 0o20000 != 0 { -1 } else { 1 };
                self.vector(dx, dy, lit);
            }
            Mode::Point => {
                let Some(w2) = self.fetch(bus) else { return };
                self.x = w & 0o1777;
                self.y = w2 & 0o1777;
                self.t += T_POSITION;
                if w & 0o40000 != 0 {
                    self.spot();
                }
            }
            Mode::GraphX => {
                // Data is a Y coordinate; X steps by the graphplot increment.
                self.x = (self.x + self.graphplot_inc) & 0o7777;
                self.y = w & 0o1777;
                self.t += T_POSITION;
                self.spot();
            }
            Mode::GraphY => {
                self.y = (self.y + self.graphplot_inc) & 0o7777;
                self.x = w & 0o1777;
                self.t += T_POSITION;
                self.spot();
            }
            Mode::Spare => {}
        }
    }

    // ---- drawing -----------------------------------------------------------

    fn on_face(x: i32, y: i32) -> bool {
        (0..FACE_W).contains(&x) && (0..FACE_H).contains(&y)
    }

    fn blink_off(&self) -> bool {
        self.blink_off_at(self.t)
    }

    /// The blink flip-flop gates intensification while blink is enabled.
    fn blink_off_at(&self, t: f64) -> bool {
        self.blink && ((t / T_BLINK_HALF) as u64) % 2 == 1
    }

    /// Level reported to the renderer for the current intensity register.
    fn level(&self) -> u8 {
        self.intensity
    }

    /// Light up the beam at the current position.
    fn spot(&mut self) {
        let (x, y) = (self.x as i32, self.y as i32);
        if !Self::on_face(x, y) || self.blink_off() {
            return;
        }
        let t = self.t;
        self.out.push(Beam::Dot { x: x as f32, y: y as f32, t, level: self.level() });
        if self.pen_sees(x as f32, y as f32, t) {
            self.light_pen_hit(t);
        }
    }

    fn relative_point(&mut self, dx: i32, dy: i32, lit: bool) {
        self.move_by(dx, dy);
        self.t += T_POSITION;
        if lit {
            self.spot();
        }
    }

    fn move_by(&mut self, dx: i32, dy: i32) {
        let nx = (self.x as i32 + dx).rem_euclid(4096);
        let ny = (self.y as i32 + dy).rem_euclid(4096);
        let was = Self::on_face(self.x as i32, self.y as i32);
        if was != Self::on_face(nx, ny) || !Self::on_face(nx, ny) {
            self.edge = true;
        }
        self.x = nx as u16;
        self.y = ny as u16;
    }

    /// Drawing time of a vector whose larger component is `m` raster units.
    /// The VT11 normalizes the components so the controlling one lies in
    /// 01000-01777, and clocks it at 5 MHz times normalized/02000: anything
    /// from 512 to 1023 units takes about 205 us, and in general a vector
    /// takes 0.2 us times the next power of two above its length (VT11
    /// manual 2.2.10.3).
    pub fn vector_time(m: i32) -> f64 {
        if m <= 0 {
            return 0.0;
        }
        let bits = 32 - (m as u32).leading_zeros();
        0.2 * (1u32 << bits) as f64
    }

    fn vector(&mut self, dx: i32, dy: i32, lit: bool) {
        let steps = dx.abs().max(dy.abs());
        self.draw_vector(dx, dy, lit, steps, steps);
    }

    /// Draw `steps_left` of a vector of `steps_total` steps whose remaining
    /// displacement is (dx, dy).
    fn draw_vector(&mut self, dx: i32, dy: i32, lit: bool, steps_total: i32, steps_left: i32) {
        let duration = Self::vector_time(steps_total);
        if steps_left <= 0 {
            return;
        }
        let dt_step = if steps_total > 0 { duration / steps_total as f64 } else { 0.0 };
        let (x0, y0) = (self.x as i32, self.y as i32);
        let t0 = self.t;
        // Walk the vector one major-axis step at a time, as the hardware does,
        // so dash patterns, edges and light pen hits fall where they would.
        let mut run_start: Option<(f32, f32, f64)> = None;
        let mut last = (x0 as f32, y0 as f32);
        let mut hit: Option<i32> = None;
        for i in 1..=steps_left {
            let fx = x0 as f32 + dx as f32 * i as f32 / steps_left as f32;
            let fy = y0 as f32 + dy as f32 * i as f32 / steps_left as f32;
            let t = t0 + dt_step * i as f64;
            let rx = (fx.round() as i32).rem_euclid(4096);
            let ry = (fy.round() as i32).rem_euclid(4096);
            self.line_counter = self.line_counter.wrapping_sub(1) & 0o37;
            let visible = lit && Self::on_face(rx, ry) && self.dash_on() && !self.blink_off_at(t);
            if visible {
                if run_start.is_none() {
                    run_start = Some((last.0, last.1, t - dt_step));
                }
                if hit.is_none() && self.pen_sees(fx, fy, t) {
                    hit = Some(i);
                }
            } else if let Some((sx, sy, st)) = run_start.take() {
                self.emit_line(sx, sy, last.0, last.1, st, t - dt_step);
            }
            if !Self::on_face(rx, ry) {
                self.edge = true;
            }
            last = (fx, fy);
            if hit == Some(i) {
                break;
            }
        }
        if let Some((sx, sy, st)) = run_start.take() {
            let end_t = t0 + dt_step * hit.unwrap_or(steps_left) as f64;
            self.emit_line(sx, sy, last.0, last.1, st, end_t);
        }
        match hit {
            Some(i) => {
                let done_x = (dx as f32 * i as f32 / steps_left as f32).round() as i32;
                let done_y = (dy as f32 * i as f32 / steps_left as f32).round() as i32;
                self.x = ((x0 + done_x).rem_euclid(4096)) as u16;
                self.y = ((y0 + done_y).rem_euclid(4096)) as u16;
                self.t = t0 + dt_step * i as f64;
                self.pending = Some(Pending::Vector {
                    dx: dx - done_x,
                    dy: dy - done_y,
                    lit,
                    steps_total,
                    steps_left: steps_left - i,
                });
                let t = self.t;
                self.light_pen_hit(t);
            }
            None => {
                self.x = ((x0 + dx).rem_euclid(4096)) as u16;
                self.y = ((y0 + dy).rem_euclid(4096)) as u16;
                self.t = t0 + dt_step * steps_left as f64;
            }
        }
    }

    fn emit_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, t0: f64, t1: f64) {
        self.out.push(Beam::Line { x0, y0, x1, y1, t0, t1, level: self.level() });
    }

    /// Dash generator (VT11 line counter, as decoded in simh's vt11.c).
    fn dash_on(&self) -> bool {
        let lc = self.line_counter;
        match self.line_type {
            LineType::Solid => true,
            LineType::LongDash => lc & 0o20 != 0,
            LineType::ShortDash => lc & 0o10 != 0,
            LineType::DotDash => (lc & 0o6 == 0o6 && lc & 0o10 == 0) || lc & 0o20 == 0,
        }
    }

    fn finish_pending(&mut self, p: Pending) {
        match p {
            Pending::Vector { dx, dy, lit, steps_total, steps_left } => {
                self.draw_vector(dx, dy, lit, steps_total, steps_left)
            }
            Pending::Char(c) => {
                self.character(c);
            }
        }
    }

    /// Display one character code. Returns true if the light pen stopped
    /// the display on it.
    fn character(&mut self, c: u8) -> bool {
        self.char_reg = c;
        if self.shift_out {
            if c == 0o17 {
                self.shift_out = false;
                self.t += T_CONTROL_CHAR;
                return false;
            }
            if c >= 0o40 {
                // Illegal code in shift-out mode: interrupt the PDP-11.
                self.timeout_irq = true;
                self.irq_time = Some(self.t);
                self.run = Run::Stopped;
                self.stop_flag = true;
                return false;
            }
            return self.glyph(c);
        }
        match c {
            0o00 => {
                return false;
            }
            0o10 => {
                self.move_by(-CHAR_PITCH, 0);
                self.t += T_CONTROL_CHAR;
            }
            0o12 => {
                self.move_by(0, -LINE_PITCH);
                self.t += T_CONTROL_CHAR;
            }
            0o15 => {
                self.x = 0;
                self.t += T_CONTROL_CHAR;
            }
            0o16 => {
                self.shift_out = true;
                self.t += T_CONTROL_CHAR;
            }
            0o40 => {
                self.t += T_CHAR;
                self.move_by(CHAR_PITCH, 0);
            }
            1..=0o37 => {
                self.t += T_CONTROL_CHAR;
            }
            _ => return self.glyph(c),
        }
        false
    }

    /// Draw a glyph at the beam position and advance. Returns true on a
    /// light pen hit.
    fn glyph(&mut self, c: u8) -> bool {
        let (bx, by) = (self.x as i32, self.y as i32);
        // MODEL: the VT11 draws nothing of a character whose cell is not
        // wholly on the face (simh vt11.c notes the same behaviour).
        let shows = Self::on_face(bx, by) && Self::on_face(bx + CHAR_PITCH - 1, by) && !self.blink_off();
        let t0 = self.t;
        let mut hit = false;
        if shows {
            let (dots, drop) = chargen::glyph(c);
            let level = self.level();
            for (col, row) in dots {
                // Dots sit on a 2-unit grid; italics shear the cell to the
                // right by about 5/8 unit per row (MODEL, after simh).
                let shear = if self.italics { row as f32 * 0.625 } else { 0.0 };
                let x = bx as f32 + 2.0 * col as f32 + shear;
                let y = by as f32 + 2.0 * row as f32 + drop as f32;
                let t = t0 + T_CHAR * (col as f64 * 8.0 + row as f64) / 48.0;
                self.out.push(Beam::Dot { x, y, t, level });
                if !hit && self.pen_sees(x, y, t) {
                    hit = true;
                }
            }
        }
        self.t = t0 + T_CHAR;
        self.move_by(CHAR_PITCH, 0);
        if hit {
            let t = self.t;
            self.light_pen_hit(t);
        }
        hit
    }

    // ---- light pen -----------------------------------------------------------

    /// Where the pen is at DPU time `t`.
    fn pen_at(&self, t: f64) -> Option<LightPen> {
        let mut p = self.pen;
        for &(at, q) in &self.pen_moves {
            if at > t {
                break;
            }
            p = q;
        }
        p
    }

    /// Apply the pen movements whose time has passed.
    fn settle_pen(&mut self) {
        while let Some(&(at, q)) = self.pen_moves.front() {
            if at > self.t {
                break;
            }
            self.pen = q;
            self.pen_moves.pop_front();
        }
    }

    fn pen_sees(&self, x: f32, y: f32, t: f64) -> bool {
        if !self.lp_enable || t < self.lp_inhibit_until {
            return false;
        }
        match self.pen_at(t) {
            Some(p) => {
                let (ddx, ddy) = (x - p.x, y - p.y);
                ddx * ddx + ddy * ddy <= p.radius * p.radius
            }
            None => false,
        }
    }

    /// A light pen hit: the DPU stops with the DPC pointing past the data
    /// word being drawn and the position registers at the beam position.
    fn light_pen_hit(&mut self, t: f64) {
        self.lp_inhibit_until = t + T_LP_INHIBIT;
        self.lp_flag = true;
        self.lp_irq = true;
        self.irq_time = Some(t);
        self.run = Run::Stopped;
        if self.lp_intensify {
            self.out.push(Beam::Dot { x: self.x as f32, y: self.y as f32, t, level: 7 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(words: &[(u16, u16)]) -> Vec<u16> {
        let mut m = vec![0u16; 4096];
        for &(a, w) in words {
            m[(a >> 1) as usize] = w;
        }
        m
    }

    #[test]
    fn point_then_long_vector() {
        // MODE POINT int=4; at (100,200); MODE LVEC; draw dx=+300 dy=-50; STOP
        let m = mem(&[(0o1000, 0o116000 | (4 << 7)), (0o1002, 100), (0o1004, 200),
                      (0o1006, 0o110000), (0o1010, 0o40000 | 300), (0o1012, 0o20000 | 50),
                      (0o1014, 0o173400)]);
        let mut v = Vt11::new(60.0);
        v.write_dpc(0o1000, 0.0);
        v.run_until(m.as_slice(), 1000.0);
        assert!(v.stop_irq, "stop with interrupt enabled must request an interrupt");
        assert_eq!(v.read_dpc(), 0o1016, "DPC points past the stop instruction");
        assert_eq!((v.read_xsr() & 0o1777, v.read_ysr() & 0o1777), (400, 150));
        let lines: Vec<_> = v.out.iter().filter(|b| matches!(b, Beam::Line { .. })).collect();
        assert_eq!(lines.len(), 1);
        if let Beam::Line { x0, y0, x1, y1, .. } = lines[0] {
            assert_eq!((x0.round(), y0.round(), x1.round(), y1.round()), (100.0, 200.0, 400.0, 150.0));
        }
    }

    #[test]
    fn vector_timing_matches_the_manual() {
        assert!((Vt11::vector_time(0o1777) - 204.8).abs() < 0.01);
        assert!((Vt11::vector_time(0o1000) - 204.8).abs() < 0.01);
        assert!((Vt11::vector_time(1) - 0.4).abs() < 0.01);
    }

    #[test]
    fn light_pen_stops_mid_vector_and_resume_finishes_it() {
        // A vertical bar drawn downward from y=700 to y=450 at x=947, light
        // pen enabled, as Burness's throttle bar is.
        let m = mem(&[(0o1000, 0o116000 | 0o100 | 0o40 | (7 << 7)), (0o1002, 947), (0o1004, 700),
                      (0o1006, 0o110000), (0o1010, 0o40000), (0o1012, 0o20000 | 250),
                      (0o1014, 0o173400)]);
        let mut v = Vt11::new(60.0);
        v.pen = Some(LightPen { x: 947.0, y: 600.0, radius: 6.0 });
        v.write_dpc(0o1000, 0.0);
        v.run_until(m.as_slice(), 1000.0);
        assert!(v.lp_irq && !v.stop_irq);
        assert_eq!(v.read_dpc(), 0o1014, "DPC points past the vector's data words");
        let y = v.read_ysr() & 0o1777;
        assert!((600 - 6..=600 + 6).contains(&(y as i32)), "YSR {y} should be near the pen");
        // Resume, as LPRESM does with BIS #1,DPC.
        v.lp_irq = false;
        v.pen = None;
        v.write_dpc(v.read_dpc() | 1, v.time());
        v.run_until(m.as_slice(), 2000.0);
        assert!(v.stop_irq);
        assert_eq!(v.read_ysr() & 0o1777, 450, "the rest of the vector was drawn");
    }

    #[test]
    fn characters_advance_and_carriage_return() {
        // MODE POINT at (0,650), MODE CHAR, "AB", CR LF, "C", STOP
        let m = mem(&[(0o1000, 0o114000), (0o1002, 0), (0o1004, 650), (0o1006, 0o100000),
                      (0o1010, (b'B' as u16) << 8 | b'A' as u16),
                      (0o1012, 0o12 << 8 | 0o15), (0o1014, b'C' as u16), (0o1016, 0o173400)]);
        let mut v = Vt11::new(60.0);
        v.write_dpc(0o1000, 0.0);
        v.run_until(m.as_slice(), 10_000.0);
        assert_eq!(v.read_xsr() & 0o1777, CHAR_PITCH as u16, "one character after the CR");
        assert_eq!(v.read_ysr() & 0o1777, (650 - LINE_PITCH) as u16);
    }

    #[test]
    fn sync_waits_for_the_line_pulse() {
        // STATUS-A SYNC then STOP: the stop must not happen before 1/60 s.
        let m = mem(&[(0o1000, 0o170004), (0o1002, 0o173400)]);
        let mut v = Vt11::new(60.0);
        v.write_dpc(0o1000, 0.0);
        v.run_until(m.as_slice(), 10_000.0);
        assert!(!v.stop_irq);
        v.run_until(m.as_slice(), 20_000.0);
        assert!(v.stop_irq);
    }
}
