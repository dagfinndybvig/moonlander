//! An autopilot that flies the game the way a player did: by holding the
//! light pen over the throttle bar and the turning arrows. It is not part of
//! Burness's program. It exists to drive the program through landings,
//! crashes and the rest for testing, and as a demonstration.
//!
//! It reads the program's variables and moves the pen through the
//! terminal's light pen script. The script it builds can be replayed into
//! another run (the original binary, in the emulator) to make the same moves
//! at the same moments.
//!
//! Flying by light pen is slow to respond. LIGHT acts only after sixteen
//! hits in a row on the same thing, and the pen is hit once each time the
//! display passes under it, so every move from one control to another costs
//! a noticeable fraction of a second before anything happens. The pilot
//! therefore stays on a control until its job is done, rather than hopping
//! between them. Staying too long has its own problem; see `decide`.

use crate::generated::symbols::*;
use gt40::terminal::Terminal;
use gt40::vt11::LightPen;

/// What the pilot is trying to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Plan {
    /// Land gently, upright, on whatever is below.
    Land,
    /// Come down at `fps` feet per second for the last 300 feet.
    Hard { fps: i16 },
    /// Touch down tilted by `degrees` (the ship tips over).
    Tilted { degrees: i16 },
    /// Touch down still moving sideways at `fps`.
    Sliding { fps: i16 },
    /// Climb until the ship leaves the top of the screen.
    Climb,
    /// Fly sideways at `fps` (negative is left) until off the edge.
    Drift { fps: i16 },
    /// Before flying, move readout item `item` (0-11) into slot `slot` (0-3)
    /// with the light pen, then land.
    Readout { item: u8, slot: u8 },
    /// Land beside the McDonald's.
    McDonalds,
    /// Land well away from the McDonald's, where the man plants a flag.
    Flag,
    /// Land on the slope at terrain index 625, too steep to stand on.
    Bumpy,
    /// Come down on the McDonald's itself.
    IntoMcDonalds,
    /// Land at terrain index `index`, whatever is there.
    LandAt { index: i16 },
    /// Hold the big left arrow until the ship has turned right round.
    Spin,
    /// Fall fast to 30 feet, then brake at full throttle.
    Brake,
    /// Hold `alt` feet and drift sideways at `fps` (negative is left).
    LowDrift { fps: i16, alt: i16 },
}

/// Where the pen can be put: the controls on the face. The desktop front
/// end's keyboard puts the pen in the same places.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spot {
    Away,
    /// The throttle bar at this percentage.
    Throttle(u16),
    /// A turning arrow: -100, -15, 15 or 100 degrees a second.
    Turn(i16),
    /// One of the twelve readout choices down the right, 0 (HEIGHT) to 11.
    Item(u8),
    /// One of the four readouts across the top, 0 to 3.
    Slot(u8),
}

/// Field of view of the pen the pilot holds, raster units.
pub const PEN_RADIUS: f32 = 5.0;

impl Spot {
    /// The pen over this control, or away from the face.
    pub fn pen(self) -> Option<LightPen> {
        let p = |x: f32, y: f32| Some(LightPen { x, y, radius: PEN_RADIUS });
        match self {
            Spot::Away => None,
            // The bar's three lines are at x 947, 950 and 953. With this
            // pen its smoothed reading settles at about (Y - 466) / 2
            // percent, a little under the (Y - 462) / 2 used here; the
            // pilot steers by the reading, so the difference only means it
            // asks for a little more than it gets.
            Spot::Throttle(pct) => p(950.0, 462.0 + 2.0 * pct as f32),
            // Small arrows 20 units long at y 375, big ones 40 long at 330.
            Spot::Turn(r) if r <= -100 => p(925.0, 330.0),
            Spot::Turn(r) if r < 0 => p(935.0, 375.0),
            Spot::Turn(r) if r >= 100 => p(975.0, 330.0),
            Spot::Turn(_) => p(965.0, 375.0),
            // The twelve choices down the right-hand side, from HEIGHT at the
            // top, and the four readout slots across the top: the pen goes
            // on the first letter of the name (characters are 14 units apart
            // and 16 high, and blanks show nothing to see).
            Spot::Item(i) => p(921.0, ITEMY1 as f32 + 8.0 - i as f32 * (ITEMY1 - ITEMY2) as f32),
            Spot::Slot(s) => p(105.0 + 250.0 * s as f32, DY1 as f32 + 8.0),
        }
    }
}

pub struct Pilot {
    pub plan: Plan,
    /// Every pen movement scheduled, in order: (machine time, pen).
    pub script: Vec<(f64, Option<LightPen>)>,
    next: f64,
    spot: Spot,
    since: f64,
    /// Where to land, in feet (HORDIS), once chosen.
    site: Option<f64>,
    /// When the ship came to rest; the pilot lets go then.
    down: Option<f64>,
}

/// How often the pilot looks, machine microseconds.
const LOOK_EVERY: f64 = 20_000.0;
/// The least time to stay on the throttle once there.
const THROTTLE_DWELL: f64 = 300_000.0;

fn word(bus: &Terminal, a: u16) -> i16 {
    bus.mem[(a >> 1) as usize] as i16
}

impl Pilot {
    pub fn new(plan: Plan) -> Pilot {
        Pilot { plan, script: Vec::new(), next: 0.0, spot: Spot::Away, since: 0.0, site: None, down: None }
    }

    /// Call with the terminal at least every millisecond of machine time.
    pub fn fly(&mut self, bus: &mut Terminal) {
        if bus.t < self.next {
            return;
        }
        self.next = bus.t + LOOK_EVERY;
        let spot = self.decide(bus);
        if spot != self.spot {
            if std::mem::discriminant(&spot) != std::mem::discriminant(&self.spot) {
                self.since = bus.t;
            }
            let pen = spot.pen();
            let at = bus.t + 1000.0;
            bus.move_pen(at, pen);
            self.script.push((at, pen));
            self.spot = spot;
        }
    }

    /// A place to land near terrain index `near`: the feature table empty
    /// two segments either side, and the ground level.
    fn pick_site(&self, bus: &Terminal, near: i32) -> Option<f64> {
        let feature = |i: i32| {
            let b = bus.mem[((FEATUR as i32 + i / 2) >> 1) as usize];
            let byte = if (FEATUR as i32 + i / 2) & 1 != 0 { b >> 8 } else { b & 0xFF };
            if i & 1 != 0 { (byte >> 4) & 15 } else { byte & 15 }
        };
        let height = |i: i32| word(bus, (TERAIN as i32 + 2 * i) as u16) as i32;
        let ok = |i: i32| (i - 2..=i + 2).all(|j| feature(j) == 0) && (height(i + 1) - height(i)).abs() < 16;
        match self.plan {
            Plan::Bumpy => return Some(625.0 * 32.0 - 22400.0 + 16.0),
            Plan::IntoMcDonalds => return Some(703.0 * 32.0 - 22400.0 + 16.0),
            Plan::LandAt { index } => return Some(index as f64 * 32.0 - 22400.0 + 16.0),
            Plan::LowDrift { .. } => return None,
            _ => {}
        }
        let (lo, hi) = match self.plan {
            Plan::McDonalds => (690, 700),
            Plan::Flag | Plan::Tilted { .. } | Plan::Sliding { .. } | Plan::Hard { .. } | Plan::Brake => (600, 682),
            _ => (20, 880),
        };
        (0..400)
            .flat_map(|d| [near + d, near - d])
            .find(|&i| (lo..=hi).contains(&i) && ok(i))
            .map(|i| i as f64 * 32.0 - 22400.0 + 16.0)
    }

    fn decide(&mut self, bus: &Terminal) -> Spot {
        // Nothing to fly during the thirty-second greeting.
        if bus.t < 30.5e6 {
            return Spot::Away;
        }
        if let Plan::Readout { item, slot } = self.plan {
            // Two seconds on the item (it starts blinking), two on the slot.
            let since = bus.t - 30.5e6;
            if since < 2e6 {
                return Spot::Item(item);
            }
            if since < 4e6 {
                return Spot::Slot(slot);
            }
        }
        // LIGHT counts hits on the same control down in HITCNT, and after
        // 32,768 of them the count wraps to +32,767 and the control stops
        // responding until it has counted back down. A player would have
        // lifted the pen now and then; the pilot touches an empty readout
        // slot, a hit elsewhere that does nothing but reset the count.
        if word(bus, HITCNT) < -20000 && word(bus, LPFLG1) == 0 {
            return Spot::Slot(0);
        }
        if matches!(self.spot, Spot::Slot(_)) && bus.t - self.since < 100_000.0 {
            return self.spot;
        }
        // Once down, let go of the pen until the next game is under way.
        let landed = [GREATM, GOODM, ROUGHM, CRIPM, SIDEM, ANGLEM, BUMPYM]
            .contains(&(word(bus, SYSMES) as u16));
        if landed && self.down.is_none() {
            self.down = Some(bus.t);
        }
        if let Some(t) = self.down {
            if bus.t - t < 100e6 && word(bus, VERDIS) < 20000 {
                return Spot::Away;
            }
            self.down = None;
            self.site = None;
        }
        if self.site.is_none() && word(bus, RADARY) < 6000 && word(bus, HORVEL).abs() < 400 {
            self.site = self.pick_site(bus, word(bus, BIGXCT) as i32);
        }
        if self.plan == Plan::Spin && bus.t > 40e6 && bus.t < 55e6 {
            return Spot::Turn(-100); // round and round, for fifteen seconds
        }
        let (theta, percent) = self.wanted(bus);
        let err = theta - word(bus, ANGLE) as f64;
        let throttle_off = (percent as f64 - word(bus, PERCNT) as f64).abs();
        let turn = |e: f64| Spot::Turn(if e.abs() > 12.0 { 100 } else { 15 } * e.signum() as i16);
        match self.spot {
            Spot::Turn(r) => {
                // Stay on the arrow until the angle is nearly right; leave
                // the big one early, since the turn already asked for is
                // still to come.
                let close = if r.abs() >= 100 { 12.0 } else { 2.0 };
                if err.abs() > close && err.signum() == r.signum() as f64 && throttle_off < 30.0 {
                    return self.spot;
                }
                if err.abs() > 4.0 && throttle_off < 10.0 {
                    return turn(err);
                }
                Spot::Throttle(percent)
            }
            Spot::Throttle(_) if bus.t - self.since < THROTTLE_DWELL || err.abs() <= 4.0 || throttle_off > 5.0 => {
                Spot::Throttle(percent)
            }
            _ if throttle_off > 10.0 => Spot::Throttle(percent),
            _ if err.abs() > 4.0 => turn(err),
            _ => Spot::Throttle(percent),
        }
    }

    /// The attitude and throttle that would steer the velocity towards the
    /// one the plan wants.
    fn wanted(&self, bus: &Terminal) -> (f64, u16) {
        let vx = word(bus, HORVEL) as f64 / 10.0;
        let vy = word(bus, VERVEL) as f64 / 10.0;
        let alt = word(bus, RADARY) as f64;
        let g = G2 as f64 / 500.0;
        // Acceleration per percent of throttle, as the program computes it
        // (journal entry 007: thrust times g over the flame table's address).
        let per_percent = MAXTRS as f64 * G1 as f64 / YTHRST as f64 / 500.0 / 100.0;
        let a_max = 100.0 * per_percent;
        let profile = -(alt * 0.06 + 3.0).clamp(3.0, 120.0);
        // Below 6,000 feet, head for a clear, level place to land.
        let x = word(bus, HORDIS) as f64;
        let (to_site, far) = match self.site {
            Some(site) if alt < 6000.0 && alt > 60.0 => ((0.05 * (site - x)).clamp(-60.0, 60.0), (site - x).abs() > 200.0),
            _ => (0.0, false),
        };
        // Hold altitude while still a long way from the site.
        let profile = if far && alt < 1500.0 { 0.0 } else { profile };
        let (want_vy, want_vx, tilt) = match self.plan {
            Plan::Climb => (40.0, 0.0, None),
            Plan::Drift { fps } => (0.0, fps as f64, None),
            Plan::Brake if alt > 30.0 => (-60.0, to_site, None),
            Plan::Brake => (0.0, 0.0, None),
            Plan::LowDrift { fps, alt: hold } if alt < 2.0 * hold as f64 => {
                (((hold as f64 - alt) * 0.2).clamp(-30.0, 30.0), fps as f64, None)
            }
            Plan::LowDrift { .. } => (profile, 0.0, None),
            // Come in moving gently the other way, so that tipping the ship
            // over at the last moment takes the sideways speed to nothing.
            Plan::Tilted { degrees } if alt < 80.0 && alt >= 40.0 => (profile, -0.25 * degrees as f64, None),
            Plan::Hard { fps } if alt < 300.0 => (-(fps as f64), 0.0, None),
            Plan::Tilted { degrees } if alt < 40.0 => (profile, 0.0, Some(degrees as f64)),
            Plan::Sliding { fps } if alt < 300.0 => (profile, fps as f64, None),
            _ => (profile, to_site, None),
        };
        // The engine can only push up and sideways: when the ship should
        // come down faster than it falls, the most it can do is idle.
        // The program drops the remainder when it turns acceleration into a
        // change of velocity, so with CLOCK at 5 a net acceleration under
        // 1.2 ft/s/s changes nothing: small corrections must be firm.
        let ex = want_vx - vx;
        let ax = if ex.abs() < 1.0 { 0.0 } else { ex.signum() * (0.12 * ex.abs()).max(2.0) };
        let ay = (g + 0.5 * (want_vy - vy)).max(0.0);
        let mut theta = if ax == 0.0 { 0.0 } else { ax.atan2(ay).to_degrees().clamp(-75.0, 75.0) };
        if alt < 80.0 && matches!(self.plan, Plan::Land | Plan::McDonalds | Plan::Readout { .. } | Plan::Hard { .. }) {
            theta = theta.clamp(-8.0, 8.0); // upright for the touchdown
        }
        if let Some(t) = tilt {
            theta = t;
        }
        let mut percent = ((ax * ax + ay * ay).sqrt().min(a_max) / per_percent).clamp(10.0, 100.0);
        match self.plan {
            // Pen above the top of the bar: the program clamps it to 100.
            Plan::Climb => percent = 110.0,
            Plan::Brake if alt <= 30.0 => percent = 100.0,
            Plan::Tilted { .. } if alt < 40.0 => percent = 10.0,
            _ => {}
        }
        (theta, percent.round() as u16)
    }
}
