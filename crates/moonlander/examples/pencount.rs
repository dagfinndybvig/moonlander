//! Hold the light pen at (X, Y) from 31 s to 36 s and count what happens.
//! Usage: pencount X Y
use gt40::vt11::LightPen;
use moonlander::generated::blocks as blk;
use moonlander::Moonlander;
use std::sync::{Arc, Mutex};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (x, y): (f32, f32) = (a[1].parse().unwrap(), a[2].parse().unwrap());
    let mut m = Moonlander::new(60.0);
    m.bus.move_pen(31e6, Some(LightPen { x, y, radius: 5.0 }));
    m.bus.move_pen(36e6, None);
    m.limit = 36e6;
    let counts = Arc::new(Mutex::new(std::collections::BTreeMap::<&'static str, u64>::new()));
    let c2 = counts.clone();
    m.observer = Some(Box::new(move |s| {
        if s.t >= 31e6 {
            for b in [blk::LIGHT, blk::LIGHTG_1, blk::ARROW, blk::LPBARH, blk::DTOP2_1, blk::SHOWSP, blk::LPIT1, blk::LPIT2] {
                if s.addr == b.addr {
                    *c2.lock().unwrap().entry(b.name).or_insert(0) += 1;
                }
            }
        }
        true
    }));
    m.run();
    println!("{:?}", counts.lock().unwrap());
}
