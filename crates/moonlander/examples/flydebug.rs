//! Fly with the autopilot and count interrupt routine entries per second.
use moonlander::generated::blocks as blk;
use moonlander::pilot::{Pilot, Plan};
use moonlander::Moonlander;
use std::sync::{Arc, Mutex};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (from, to): (f64, f64) = (a[1].parse().unwrap(), a[2].parse().unwrap());
    let mut m = Moonlander::new(60.0);
    m.limit = to * 1e6;
    let pilot = Arc::new(Mutex::new(Pilot::new(Plan::Land)));
    let p2 = pilot.clone();
    m.pacer = Some(Box::new(move |bus| {
        p2.lock().unwrap().fly(bus);
        true
    }));
    let counts = Arc::new(Mutex::new(std::collections::BTreeMap::<(u64, &'static str), u64>::new()));
    let c2 = counts.clone();
    m.observer = Some(Box::new(move |s| {
        if s.t >= from * 1e6 {
            for b in [blk::LIGHT, blk::LIGHT_1, blk::LPBARH, blk::DTOP2_1, blk::SHOWSP, blk::DISPLY, blk::TIMER] {
                if s.addr == b.addr {
                    *c2.lock().unwrap().entry(((s.t / 1e6) as u64, b.name)).or_insert(0) += 1;
                }
            }
        }
        true
    }));
    m.run();
    let c = counts.lock().unwrap();
    let mut sec = 0;
    for ((s, n), v) in c.iter() {
        if *s != sec { println!(); print!("{s}:"); sec = *s; }
        print!(" {n}={v}");
    }
    println!();
    println!("vt11 running={} pen={:?}", m.bus.vt11.is_running(), m.bus.vt11.pen);
}
