//! The machine thread. It runs the program (the translation, or the
//! original binary on the emulated processor) against the wall clock, and
//! trades with the window through a [`Link`]: the beam and the bell go out,
//! the light pen comes in.
//!
//! Real time is kept loosely. The machine runs until its clock is a little
//! ahead of the wall clock and then sleeps; if it ever falls far behind (the
//! window was dragged, the computer slept), it starts counting from where it
//! is rather than racing to catch up. Everything the program computes depends
//! only on machine time, so how well the host keeps up changes when things
//! appear, never what happens.

use crate::keys::Keyboard;
use gt40::terminal::Terminal;
use gt40::vt11::{Beam, LightPen};
use moonlander::pilot::{Pilot, Plan, PEN_RADIUS};
use moonlander::{Escape, Moonlander};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cpu {
    /// The Rust translation, running on the terminal model.
    Translation,
    /// The original binary, on the emulated KD11-B processor.
    Original,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub cpu: Cpu,
    /// Mains frequency: the line clock and the display's sync.
    pub line_hz: f64,
    /// Fly by the autopilot instead of the player's pen.
    pub demo: Option<Plan>,
    /// Run this .lda image instead of the game's own.
    pub image: Option<std::path::PathBuf>,
}

/// What the window and the machine thread share.
#[derive(Default)]
pub struct Link {
    // From the window.
    /// The mouse button is down with the cursor here (raster units).
    pub mouse: Option<(f32, f32)>,
    /// Keyboard commands not yet acted on.
    pub keys: Vec<crate::keys::Command>,
    /// Console characters from the keyboard, for programs that read them.
    pub text: Vec<u8>,
    /// A turning key is held: direction (-1 left, 1 right) and whether fast.
    pub turning: Option<(i8, bool)>,
    pub paused: bool,
    /// Load the program again and start it.
    pub reload: bool,
    pub quit: bool,

    // From the machine.
    /// Machine time, microseconds.
    pub t: f64,
    /// Everything the beam drew since the window last looked.
    pub beams: Vec<Beam>,
    /// Bells rung since the window last looked.
    pub bells: u32,
    /// Where the light pen is, for the window to show.
    pub pen: Option<LightPen>,
    /// Why the processor stopped, if it has.
    pub halted: Option<String>,
    /// Counts loads of the program; the window starts a fresh tube on each.
    pub generation: u64,
}

pub type Shared = Arc<Mutex<Link>>;

pub fn spawn(cfg: Config, link: Shared) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new().name("gt40".into()).spawn(move || machine(cfg, link)).expect("machine thread")
}

/// Load and run the program, again each time the window asks, until it
/// asks to quit.
fn machine(cfg: Config, link: Shared) {
    loop {
        {
            let mut l = link.lock().unwrap();
            if l.quit {
                return;
            }
            l.reload = false;
            l.halted = None;
            l.beams.clear();
            l.bells = 0;
            l.t = 0.0;
            l.generation += 1;
        }
        let host = Arc::new(Mutex::new(Host::new(&cfg, link.clone())));
        let (stopped, mut bus) = match cfg.cpu {
            Cpu::Translation => run_translation(&cfg, host.clone()),
            Cpu::Original => run_original(&cfg, host.clone()),
        };
        // The processor has stopped. The display and the clock go on: a
        // display file that loops keeps its picture up, one that waits for
        // the processor fades.
        if let Some(why) = stopped {
            link.lock().unwrap().halted = Some(why);
            let mut h = host.lock().unwrap();
            loop {
                bus.t += 1000.0;
                bus.sync();
                if !h.tend(&mut bus) {
                    break;
                }
            }
        }
    }
}

/// Run the translation; returns why the processor stopped (None if the
/// window stopped it) and the terminal.
fn run_translation(cfg: &Config, host: Arc<Mutex<Host>>) -> (Option<String>, Terminal) {
    let mut m = Moonlander::new(cfg.line_hz);
    let h = host.clone();
    m.pacer = Some(Box::new(move |bus| h.lock().unwrap().tend(bus)));
    let why = match m.run() {
        Escape::Stop => None,
        Escape::Restart => unreachable!("run() restarts by itself"),
        Escape::Halt(a) => Some(format!("HALT AT {:06o}", a)),
        Escape::BusError(a) => Some(format!("BUS ERROR AT {:06o}. THE TRAP VECTOR HALTS", a)),
        Escape::Wild(a) => Some(format!("CONTROL WENT TO {:06o}", a)),
    };
    m.pacer = None;
    (why, std::mem::replace(&mut m.bus, Terminal::new(0, cfg.line_hz)))
}

/// Run the original binary on the emulated processor, which takes
/// interrupts between any two instructions, as the real one did.
fn run_original(cfg: &Config, host: Arc<Mutex<Host>>) -> (Option<String>, Terminal) {
    let mut m = kd11b::machine::Machine::new(0o20000, cfg.line_hz);
    let bytes = match &cfg.image {
        Some(path) => std::fs::read(path).expect("cannot read the image file"),
        None => moonlander::IMAGE.to_vec(),
    };
    let img = gt40::lda::parse(&bytes).expect("memory image");
    m.load(&img);
    let mut h = host.lock().unwrap();
    loop {
        let until = m.bus.t + 1000.0;
        m.run_until(until);
        if m.halted {
            let why = format!("HALT AT {:06o}", m.r[7].wrapping_sub(2));
            return (Some(why), std::mem::replace(&mut m.bus, Terminal::new(0, cfg.line_hz)));
        }
        if !h.tend(&mut m.bus) {
            return (None, std::mem::replace(&mut m.bus, Terminal::new(0, cfg.line_hz)));
        }
    }
}

/// The host side of the terminal: what a person at the GT40 would see and
/// do, at the pace they would see it.
struct Host {
    link: Shared,
    keyboard: Keyboard,
    pilot: Option<Pilot>,
    pen: Option<LightPen>,
    clock: Pace,
}

impl Host {
    fn new(cfg: &Config, link: Shared) -> Host {
        Host { link, keyboard: Keyboard::default(), pilot: cfg.demo.map(Pilot::new), pen: None, clock: Pace::new(0.0) }
    }

    /// Called at least once a millisecond of machine time. Returns false to
    /// stop the machine.
    fn tend(&mut self, bus: &mut Terminal) -> bool {
        let mut l = self.link.lock().unwrap();
        if l.quit || l.reload {
            return false;
        }
        // The picture and the bell. If the window is not taking the beams
        // (it is hidden), drop the oldest: the phosphor would have faded.
        if l.beams.len() > 1 << 20 {
            l.beams.clear();
        }
        l.beams.append(&mut bus.vt11.out);
        l.bells += bus.bells.len() as u32;
        bus.bells.clear();
        l.t = bus.t;

        // Console input: one character at a time into the keyboard
        // buffer, with the done bit set, as the LK40 would have.
        if !l.text.is_empty() && bus.console.tks & 0o200 == 0 {
            bus.console.tkb = l.text.remove(0) as u16;
            bus.console.tks |= 0o200;
        }

        // The light pen: the autopilot's, the mouse, or the keyboard's.
        if let Some(p) = &mut self.pilot {
            p.fly(bus);
            l.pen = bus.vt11.pen_moves.back().map(|m| m.1).unwrap_or(bus.vt11.pen);
        } else {
            for k in std::mem::take(&mut l.keys) {
                self.keyboard.press(k, bus);
            }
            self.keyboard.turning = l.turning;
            let pen = match l.mouse {
                Some((x, y)) => Some(LightPen { x, y, radius: PEN_RADIUS }),
                None => self.keyboard.pen(bus),
            };
            if pen != self.pen {
                bus.move_pen(bus.t, pen);
                self.pen = pen;
            }
            l.pen = pen;
        }

        let paused = l.paused;
        drop(l);
        if paused {
            loop {
                std::thread::sleep(Duration::from_millis(15));
                let l = self.link.lock().unwrap();
                if l.quit || l.reload {
                    return false;
                }
                if !l.paused {
                    break;
                }
            }
            self.clock = Pace::new(bus.t);
        }
        self.clock.pace(bus.t);
        true
    }
}

/// Machine time against the wall clock.
struct Pace {
    origin: Instant,
    /// Machine time at `origin`.
    t0: f64,
}

impl Pace {
    fn new(t: f64) -> Pace {
        Pace { origin: Instant::now(), t0: t }
    }

    fn wall(&self) -> f64 {
        self.t0 + self.origin.elapsed().as_secs_f64() * 1e6
    }

    /// Sleep while the machine is ahead; start again from here if it has
    /// fallen a long way behind.
    fn pace(&mut self, t: f64) {
        let ahead = t - self.wall();
        if ahead > 2000.0 {
            std::thread::sleep(Duration::from_micros(ahead as u64));
        } else if ahead < -100_000.0 {
            *self = Pace::new(t);
        }
    }
}
