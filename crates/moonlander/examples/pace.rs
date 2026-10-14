//! How fast the program runs on the modelled processor, which is what the
//! timing model decides about the game. Holds the light pen on the
//! throttle bar from 31 to 36 s and on the small left arrow from 37 to
//! 42 s, then leaves the ship alone to 60 s, and reports
//!
//! - physics steps a second, and the clock ticks each one covers (the
//!   value EIDLE stores in CLOCK), which sets the size of each step;
//! - LPBARH runs a second with the pen on the bar, and ARROW runs a second
//!   with it on an arrow;
//! - how long the display takes to come back to the same vector (median
//!   and 90th percentile), over the untouched stretch.
//!
//! Usage: pace
use gt40::vt11::{Beam, LightPen};
use moonlander::generated::blocks as blk;
use moonlander::Moonlander;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Tally {
    steps: u64,
    clock: BTreeMap<u16, u64>,
    lpbarh: u64,
    arrow: u64,
    seen: HashMap<(i32, i32, i32, i32), f64>,
    gaps: Vec<f64>,
}

const QUIET: std::ops::Range<f64> = 45e6..60e6;

fn main() {
    let mut m = Moonlander::new(60.0);
    let pen = |x: f32, y: f32| Some(LightPen { x, y, radius: 5.0 });
    m.bus.move_pen(31e6, pen(950.0, 522.0));
    m.bus.move_pen(36e6, None);
    m.bus.move_pen(37e6, pen(935.0, 375.0));
    m.bus.move_pen(42e6, None);
    m.limit = QUIET.end;
    let tally = Arc::new(Mutex::new(Tally::default()));
    let (t1, t2) = (tally.clone(), tally.clone());
    m.observer = Some(Box::new(move |s| {
        let mut t = t1.lock().unwrap();
        if s.addr == blk::LPBARH.addr && (31e6..36e6).contains(&s.t) {
            t.lpbarh += 1;
        }
        if s.addr == blk::ARROW.addr && (37e6..42e6).contains(&s.t) {
            t.arrow += 1;
        }
        // EIDLE_2 is CLR TICKS; MOV IN1,CLOCK; JSR PC,SHOWSP: IN1 holds
        // the ticks this step will cover.
        if s.addr == blk::EIDLE_2.addr && QUIET.contains(&s.t) {
            t.steps += 1;
            *t.clock.entry(s.r[0]).or_insert(0) += 1;
        }
        true
    }));
    m.pacer = Some(Box::new(move |bus| {
        let mut t = t2.lock().unwrap();
        let window = QUIET.contains(&bus.t);
        for b in bus.vt11.out.drain(..) {
            if let (true, Beam::Line { x0, y0, x1, y1, t1, .. }) = (window, b) {
                let k = (x0 as i32, y0 as i32, x1 as i32, y1 as i32);
                if let Some(prev) = t.seen.insert(k, t1) {
                    t.gaps.push(t1 - prev);
                }
            }
        }
        true
    }));
    m.run();
    let t = tally.lock().unwrap();
    let mut g = t.gaps.clone();
    g.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: usize| g[g.len() * p / 100] / 1000.0;
    let mean = t.clock.iter().map(|(k, v)| *k as f64 * *v as f64).sum::<f64>() / t.steps as f64;
    let secs = (QUIET.end - QUIET.start) / 1e6;
    println!("physics steps a second    {:.2}", t.steps as f64 / secs);
    println!("ticks per step            mean {:.2}  {:?}", mean, t.clock);
    println!("LPBARH a second, on bar   {:.0}", t.lpbarh as f64 / 5.0);
    println!("ARROW a second, on arrow  {:.1}", t.arrow as f64 / 5.0);
    println!("vector repeat gap (ms)    median {:.2}  p90 {:.2}", pct(50), pct(90));
}
