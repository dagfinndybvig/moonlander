//! The keyboard, as a hand moving the light pen. Burness's program reads
//! nothing but the pen, so each key puts the pen on the control a player
//! would have pointed at, holds it there until the program has acted, and
//! takes it away. Everything the keyboard does could be done with the pen,
//! and goes through the same light pen hits, the same sixteen-hit count in
//! `LIGHT`, and the same smoothing of the throttle bar.

use gt40::terminal::Terminal;
use gt40::vt11::LightPen;
use moonlander::generated::symbols::{LPFLG1, MINTRS, PERCNT};
use moonlander::pilot::Spot;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// Move the throttle by this many percent.
    Throttle(i16),
    /// Point at the next (1) or previous (-1) readout choice.
    Choice(i8),
    /// Point at a readout slot, 0 to 3.
    Slot(u8),
}

/// How long to hold the pen on the throttle at most, microseconds.
const THROTTLE_HOLD: f64 = 1_500_000.0;
/// How long to hold it at least: sixteen hits, and a few to smooth over.
const THROTTLE_SETTLE: f64 = 60_000.0;
/// How often to look at the reading and move the pen up or down.
const THROTTLE_LOOK: f64 = 40_000.0;
/// The top of the bar, in percent.
const BAR_TOP: i16 = 119;
/// How long to hold the pen on a readout choice or slot at most.
const TOUCH_HOLD: f64 = 1_500_000.0;
/// How long to hold it on a slot when no choice is blinking (it does
/// nothing but end the run of hits on whatever was touched before).
const TOUCH_EMPTY: f64 = 400_000.0;

#[derive(Default)]
pub struct Keyboard {
    /// A turning key is held: direction and whether fast.
    pub turning: Option<(i8, bool)>,
    /// The throttle setting wanted, if the pen is on its way to it.
    throttle: Option<Throttle>,
    /// How far from the setting wanted the pen has to be put for the
    /// reading to show it, in percent: learned by watching the reading.
    offset: i16,
    /// A choice or slot being touched: where, the blinking choice when the
    /// touch began, and when to give up.
    touch: Option<(Spot, u16, f64)>,
    /// The readout choice last pointed at.
    choice: Option<u8>,
}

#[derive(Clone, Copy, Debug)]
struct Throttle {
    want: u16,
    since: f64,
    /// When to look at the reading next.
    look: f64,
}

fn word(bus: &Terminal, a: u16) -> u16 {
    bus.mem[(a >> 1) as usize]
}

impl Keyboard {
    pub fn press(&mut self, c: Command, bus: &Terminal) {
        match c {
            Command::Throttle(d) => {
                let from = self.throttle.map(|t| t.want).unwrap_or(word(bus, PERCNT)) as i16;
                let want = (from + d).clamp(MINTRS as i16, 100) as u16;
                self.throttle = Some(Throttle { want, since: bus.t, look: bus.t + THROTTLE_LOOK });
            }
            Command::Choice(d) => {
                let c = match self.choice {
                    None if d > 0 => 0,
                    None => 11,
                    Some(c) => (c as i8 + d).rem_euclid(12) as u8,
                };
                self.choice = Some(c);
                self.touch = Some((Spot::Item(c), word(bus, LPFLG1), bus.t + TOUCH_HOLD));
            }
            Command::Slot(s) => {
                let blinking = word(bus, LPFLG1);
                let hold = if blinking != 0 { TOUCH_HOLD } else { TOUCH_EMPTY };
                self.touch = Some((Spot::Slot(s), blinking, bus.t + hold));
            }
        }
    }

    /// Where the keyboard's pen is now.
    pub fn pen(&mut self, bus: &Terminal) -> Option<LightPen> {
        // A touch on a readout choice or slot ends when the blinking
        // choice changes: the choice was taken up, or put in its slot.
        if let Some((spot, blinking, until)) = self.touch {
            if bus.t < until && word(bus, LPFLG1) == blinking {
                return spot.pen();
            }
            self.touch = None;
        }
        if let Some((dir, fast)) = self.turning {
            let rate = if fast { 100 } else { 15 };
            return Spot::Turn(dir as i16 * rate).pen();
        }
        if let Some(mut th) = self.throttle {
            let held = bus.t - th.since;
            let now = word(bus, PERCNT);
            if (now == th.want && held >= THROTTLE_SETTLE) || held >= THROTTLE_HOLD {
                self.throttle = None;
                return None;
            }
            // LPBARH reads where on the bar the pen saw the beam, and
            // averages; where that is depends on the pen. So the keyboard
            // does what a hand does: points, watches the percentage, and
            // moves the pen up or down until it shows what was wanted.
            if bus.t >= th.look {
                self.offset = (self.offset + (th.want as i16 - now as i16).signum()).clamp(-10, 10);
                th.look = bus.t + THROTTLE_LOOK;
                self.throttle = Some(th);
            }
            let aim = (th.want as i16 + self.offset).clamp(0, BAR_TOP);
            return Spot::Throttle(aim as u16).pen();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonlander::Moonlander;
    use std::sync::{Arc, Mutex};

    /// Run the game to `start` seconds, then press each key in turn,
    /// a second apart, and report the throttle reading a second after each.
    fn readings(start: f64, presses: &[Command]) -> Vec<u16> {
        let mut m = Moonlander::new(60.0);
        m.limit = (start + presses.len() as f64 + 0.1) * 1e6;
        let out = Arc::new(Mutex::new(Vec::new()));
        let o = out.clone();
        let presses = presses.to_vec();
        let mut kb = Keyboard::default();
        let mut pen = None;
        let mut next = 0;
        m.pacer = Some(Box::new(move |bus| {
            let due = start * 1e6 + next as f64 * 1e6;
            if bus.t >= due && next <= presses.len() {
                if next > 0 {
                    o.lock().unwrap().push(word(bus, PERCNT));
                }
                if next < presses.len() {
                    kb.press(presses[next], bus);
                }
                next += 1;
            }
            let p = kb.pen(bus);
            if p != pen {
                bus.move_pen(bus.t, p);
                pen = p;
            }
            true
        }));
        m.run();
        let r = out.lock().unwrap().clone();
        r
    }

    #[test]
    fn the_keyboard_sets_the_throttle_exactly() {
        use Command::Throttle;
        let r = readings(31.0, &[Throttle(5), Throttle(5), Throttle(-1), Throttle(-50), Throttle(-50), Throttle(20)]);
        // The game starts at 75%; the floor is 10%.
        assert_eq!(r, vec![80, 85, 84, 34, 10, 30]);
    }
}
