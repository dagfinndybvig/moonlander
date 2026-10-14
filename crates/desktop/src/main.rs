//! Moonlander on a modelled GT40, in a window.
//!
//! The program runs on its own thread against the wall clock (`host`); the
//! window shows what the beam drew, through the model of the GT40's P39
//! phosphor or as clean vectors, and turns the mouse and the keyboard into
//! the light pen (`keys`).

// A release build on Windows opens no console window beside the game.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod host;
mod keys;
mod overlay;
mod sound;

use gt40::crt::{render_clean, Crt, CLEAN_SPAN_US};
use gt40::vt11::Beam;
use host::{Config, Cpu, Shared};
use keys::Command;
use moonlander::pilot::{Plan, PEN_RADIUS};
use overlay::Canvas;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorIcon, Fullscreen, Window, WindowId};

const USAGE: &str = "\
Moonlander, by Jack Burness (DEC GT40, 1973), on a modelled GT40.

usage: moonlander [options]

  --original        run the original binary on the emulated PDP-11/05
                    instead of the Rust translation
  --clean           start with clean vectors instead of the phosphor
  --50hz            a 50 Hz mains supply (the default is 60 Hz)
  --demo [PLAN]     let the autopilot fly: land (the default), mcdonalds,
                    flag, crash, spin
  --mute            no bell
  --help            this

In the window, H shows the controls.";

const HELP: &[&str] = &[
    "MOONLANDER",
    "~JACK BURNESS, DEC GT40, 1973. THE MARE ASSABET BUILD.",
    "",
    "THE MOUSE IS THE LIGHT PEN. HOLD THE BUTTON DOWN TO PUT",
    "THE PEN ON THE SCREEN.",
    "~  THROTTLE   POINT AT THE BAR WHERE YOU WANT IT",
    "~  TURN       HOLD THE PEN ON AN ARROW. THE BIG ONES",
    "~             TURN FASTER",
    "~  READOUTS   POINT AT A WORD DOWN THE RIGHT, THEN AT",
    "~             ONE OF THE FOUR READOUTS ACROSS THE TOP",
    "",
    "THE KEYBOARD MOVES THE SAME PEN.",
    "~  UP DOWN       THROTTLE 5%, WITH SHIFT 1%",
    "~  LEFT RIGHT    TURN, WITH SHIFT FASTER",
    "~  TAB           NEXT READOUT WORD, WITH SHIFT BACK",
    "~  1 2 3 4       PUT IT IN THAT READOUT",
    "",
    "~  V  PHOSPHOR OR CLEAN VECTORS     P  PAUSE",
    "~  F  FULL SCREEN                   R  LOAD AGAIN",
    "~  H  THIS HELP",
];

struct Options {
    cfg: Config,
    clean: bool,
    sound: bool,
}

fn options() -> Result<Options, String> {
    let mut o = Options { cfg: Config { cpu: Cpu::Translation, line_hz: 60.0, demo: None }, clean: false, sound: true };
    let mut args = std::env::args().skip(1).peekable();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--original" => o.cfg.cpu = Cpu::Original,
            "--clean" => o.clean = true,
            "--50hz" => o.cfg.line_hz = 50.0,
            "--60hz" => o.cfg.line_hz = 60.0,
            "--mute" => o.sound = false,
            "--demo" => {
                let plan = match args.peek().map(|s| s.as_str()) {
                    Some(p) if !p.starts_with("--") => {
                        let p = args.next().unwrap();
                        match p.as_str() {
                            "land" => Plan::Land,
                            "mcdonalds" => Plan::McDonalds,
                            "flag" => Plan::Flag,
                            "crash" => Plan::Hard { fps: 40 },
                            "spin" => Plan::Spin,
                            other => return Err(format!("no such plan: {other}")),
                        }
                    }
                    _ => Plan::Land,
                };
                o.cfg.demo = Some(plan);
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(o)
}

struct App {
    cfg: Config,
    link: Shared,
    machine: Option<std::thread::JoinHandle<()>>,
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    crt: Crt,
    /// The beams recent enough for the clean view.
    recent: Vec<Beam>,
    generation: u64,
    clean: bool,
    help: bool,
    notice: Option<(String, Instant)>,
    started: Instant,
    cursor: Option<(f64, f64)>,
    button: bool,
    mods: ModifiersState,
    left: bool,
    right: bool,
    bell: sound::Bell,
    next_frame: Instant,
}

impl App {
    fn new(o: Options) -> App {
        App {
            cfg: o.cfg,
            link: Shared::default(),
            machine: None,
            window: None,
            surface: None,
            crt: Crt::new(1024, 768),
            recent: Vec::new(),
            generation: 0,
            clean: o.clean,
            help: false,
            notice: None,
            started: Instant::now(),
            cursor: None,
            button: false,
            mods: ModifiersState::empty(),
            left: false,
            right: false,
            bell: sound::Bell::new(o.sound),
            next_frame: Instant::now(),
        }
    }

    fn title(&self) -> String {
        match self.cfg.cpu {
            Cpu::Translation => "Moonlander".to_string(),
            Cpu::Original => "Moonlander (the original binary, emulated)".to_string(),
        }
    }

    fn notify(&mut self, s: &str) {
        self.notice = Some((s.to_string(), Instant::now()));
    }

    fn command(&self, c: Command) {
        self.link.lock().unwrap().keys.push(c);
    }

    /// The mouse pen: on the screen while the button is down.
    fn update_pen(&self) {
        let at = match (self.button, self.cursor) {
            (true, Some((x, y))) => Some(self.crt.to_raster(x as f32, y as f32)),
            _ => None,
        };
        self.link.lock().unwrap().mouse = at;
    }

    fn update_turning(&self) {
        let fast = self.mods.shift_key();
        let t = match (self.left, self.right) {
            (true, false) => Some((-1, fast)),
            (false, true) => Some((1, fast)),
            _ => None,
        };
        self.link.lock().unwrap().turning = t;
    }

    fn quit(&mut self, el: &ActiveEventLoop) {
        self.link.lock().unwrap().quit = true;
        if let Some(m) = self.machine.take() {
            let _ = m.join();
        }
        el.exit();
    }

    fn key(&mut self, el: &ActiveEventLoop, code: KeyCode, pressed: bool, repeat: bool) {
        let shift = self.mods.shift_key();
        let fresh = pressed && !repeat;
        match code {
            KeyCode::ArrowUp if pressed => self.command(Command::Throttle(if shift { 1 } else { 5 })),
            KeyCode::ArrowDown if pressed => self.command(Command::Throttle(if shift { -1 } else { -5 })),
            KeyCode::ArrowLeft => {
                self.left = pressed;
                self.update_turning();
            }
            KeyCode::ArrowRight => {
                self.right = pressed;
                self.update_turning();
            }
            KeyCode::Tab if pressed => self.command(Command::Choice(if shift { -1 } else { 1 })),
            KeyCode::Digit1 if fresh => self.command(Command::Slot(0)),
            KeyCode::Digit2 if fresh => self.command(Command::Slot(1)),
            KeyCode::Digit3 if fresh => self.command(Command::Slot(2)),
            KeyCode::Digit4 if fresh => self.command(Command::Slot(3)),
            KeyCode::KeyQ | KeyCode::KeyW if fresh && (self.mods.super_key() || self.mods.control_key()) => {
                self.quit(el)
            }
            KeyCode::KeyV if fresh => {
                self.clean = !self.clean;
                self.notify(if self.clean { "CLEAN VECTORS" } else { "P39 PHOSPHOR" });
            }
            KeyCode::KeyF | KeyCode::F11 if fresh => {
                if let Some(w) = &self.window {
                    let full = w.fullscreen().is_some();
                    w.set_fullscreen(if full { None } else { Some(Fullscreen::Borderless(None)) });
                }
            }
            KeyCode::KeyP if fresh => {
                let mut l = self.link.lock().unwrap();
                l.paused = !l.paused;
            }
            KeyCode::KeyR if fresh => {
                {
                    let mut l = self.link.lock().unwrap();
                    l.paused = false;
                    l.reload = true;
                }
                self.notify("LOADING THE PROGRAM AGAIN");
            }
            KeyCode::KeyH | KeyCode::F1 | KeyCode::Slash if fresh => self.help = !self.help,
            KeyCode::Escape if fresh => {
                if self.help {
                    self.help = false;
                } else if let Some(w) = &self.window {
                    w.set_fullscreen(None);
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else { return };
        let size = window.inner_size();
        let (Some(nw), Some(nh)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        let (w, h) = (size.width as usize, size.height as usize);
        if surface.resize(nw, nh).is_err() {
            return;
        }

        // Take what the machine has drawn.
        let (t, beams, bells, pen, halted, generation, paused) = {
            let mut l = self.link.lock().unwrap();
            (l.t, std::mem::take(&mut l.beams), std::mem::take(&mut l.bells), l.pen, l.halted.clone(), l.generation, l.paused)
        };
        if generation != self.generation {
            self.crt = Crt::new(w, h);
            self.recent.clear();
            self.generation = generation;
        }
        self.crt.resize(w, h);
        self.crt.advance(t);
        for b in &beams {
            self.crt.deposit(b);
        }
        self.recent.extend(beams);
        let cut = t - CLEAN_SPAN_US;
        self.recent.retain(|b| match *b {
            Beam::Dot { t, .. } => t >= cut,
            Beam::Line { t1, .. } => t1 >= cut,
        });
        self.bell.ring(bells);

        let Ok(mut buffer) = surface.buffer_mut() else { return };
        if self.clean {
            render_clean(&self.crt, &self.recent, &mut buffer);
        } else {
            self.crt.render(&mut buffer);
        }

        // Over the tube.
        let scale = self.crt.scale();
        let dot = ((scale * 2.0).round() as i32).max(2);
        let mut c = Canvas { px: &mut buffer, w, h };
        if let Some(p) = pen {
            let (x, y) = self.crt.to_pixel(p.x, p.y);
            let r = (PEN_RADIUS * scale).max(5.0) + 2.0;
            c.ring(x, y, r, overlay::INK, 0.55);
        }
        if let Some(why) = &halted {
            c.banner(dot, &format!("THE PROCESSOR STOPPED: {why}.  R LOADS THE PROGRAM AGAIN"));
        } else if paused {
            c.banner(dot, "PAUSED.  P GOES ON");
        }
        if self.help {
            c.panel(HELP, dot);
        } else {
            let mut row = 0;
            if let Some((s, at)) = &self.notice {
                let age = at.elapsed().as_secs_f32();
                if age < 2.5 {
                    c.note(row, dot, (2.5 - age).min(1.0), s);
                    row += 1;
                }
            }
            let age = self.started.elapsed().as_secs_f32();
            if age < 10.0 {
                let what = match (self.cfg.demo.is_some(), self.cfg.cpu) {
                    (true, _) => "THE AUTOPILOT IS FLYING.  H  HELP",
                    (false, Cpu::Original) => "THE ORIGINAL BINARY, EMULATED.  H  HELP",
                    (false, Cpu::Translation) => "H  HELP",
                };
                c.note(row, dot, (10.0 - age).min(1.0), what);
            }
        }
        window.pre_present_notify();
        let _ = buffer.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(self.title())
            .with_inner_size(LogicalSize::new(1024.0, 768.0))
            .with_min_inner_size(LogicalSize::new(400.0, 300.0));
        let window = Rc::new(el.create_window(attrs).expect("cannot open a window"));
        window.set_cursor(CursorIcon::Crosshair);
        let context = softbuffer::Context::new(window.clone()).expect("cannot draw in the window");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("cannot draw in the window");
        let size = window.inner_size();
        self.crt = Crt::new(size.width.max(1) as usize, size.height.max(1) as usize);
        self.window = Some(window);
        self.surface = Some(surface);
        if self.machine.is_none() {
            self.machine = Some(host::spawn(self.cfg.clone(), self.link.clone()));
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.quit(el),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                self.update_turning();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.key(el, code, event.state == ElementState::Pressed, event.repeat);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
                if self.button {
                    self.update_pen();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.update_pen();
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.button = state == ElementState::Pressed;
                self.update_pen();
            }
            WindowEvent::Focused(false) => {
                // Let go of everything.
                self.button = false;
                self.left = false;
                self.right = false;
                self.update_pen();
                self.update_turning();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_frame {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
            self.next_frame = now + Duration::from_micros(16_667);
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
    }
}

fn main() {
    let o = match options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("moonlander: {e}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    let el = EventLoop::new().expect("no display to open a window on");
    let mut app = App::new(o);
    el.run_app(&mut app).expect("event loop");
}
