//! Which blocks of the original do the test flights not reach? Mirrors the
//! flights in tests/lockstep.rs (the translation alone, which those tests
//! show runs block for block as the original does).
use moonlander::generated::blocks::ALL;
use moonlander::generated::symbols::*;
use moonlander::pilot::{Pilot, Plan};
use moonlander::Moonlander;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

fn earlier_games() -> Vec<(f64, u16, u16)> {
    let img = gt40::lda::parse(moonlander::IMAGE).unwrap();
    let mut mem = std::collections::BTreeMap::new();
    for &(a, b) in &img.bytes {
        mem.insert(a, b);
    }
    for (i, f) in [(655u16, 2u8), (658, 1), (662, 3), (664, 4)] {
        let a = FEATUR + i / 2;
        let b = mem.get(&a).copied().unwrap_or(0);
        mem.insert(a, if i & 1 != 0 { (b & 0x0F) | (f << 4) } else { (b & 0xF0) | f });
    }
    let mut words: Vec<u16> = [655u16, 658, 662, 664].iter().map(|i| (FEATUR + i / 2) & !1).collect();
    words.dedup();
    words.iter().map(|&a| (35.0, a, mem[&a] as u16 | (mem.get(&(a + 1)).copied().unwrap_or(0) as u16) << 8)).collect()
}

fn main() {
    let mut flights: Vec<(Option<Plan>, f64, Vec<(f64, u16, u16)>, bool)> = vec![
        (None, 120.0, vec![], false),
        (None, 40.0, vec![], true),
        (Some(Plan::Land), 200.0, vec![(40.0, FUEL, 2100)], false),
        (Some(Plan::McDonalds), 260.0, vec![(35.0, TERAIN + 2 * 704, 90)], false),
    ];
    for (p, s) in [
        (Plan::McDonalds, 420.0), (Plan::Flag, 270.0), (Plan::Hard { fps: 10 }, 240.0), (Plan::Hard { fps: 20 }, 240.0),
        (Plan::Hard { fps: 40 }, 240.0), (Plan::Brake, 360.0), (Plan::Sliding { fps: -20 }, 240.0),
        (Plan::LandAt { index: 581 }, 300.0), (Plan::LandAt { index: 583 }, 300.0), (Plan::LowDrift { fps: 30, alt: 300 }, 320.0),
        (Plan::Tilted { degrees: -25 }, 240.0), (Plan::Bumpy, 270.0), (Plan::IntoMcDonalds, 240.0), (Plan::Climb, 280.0),
        (Plan::Drift { fps: -150 }, 260.0), (Plan::Drift { fps: 150 }, 200.0), (Plan::Readout { item: 4, slot: 1 }, 220.0),
        (Plan::Spin, 280.0), (Plan::LowDrift { fps: -30, alt: 300 }, 300.0),
    ] {
        flights.push((Some(p), s, vec![], false));
    }
    for i in [655, 658, 662] {
        flights.push((Some(Plan::LandAt { index: i }), 250.0, earlier_games(), false));
    }
    let seen = Arc::new(Mutex::new(BTreeSet::new()));
    for (plan, secs, deposits, no_host) in flights {
        let mut m = Moonlander::new(60.0);
        m.limit = secs * 1e6;
        for (t, a, v) in deposits {
            m.bus.deposits.push_back((t * 1e6, a, v));
        }
        if no_host {
            m.bus.host = None;
        }
        let s2 = seen.clone();
        m.observer = Some(Box::new(move |s| {
            s2.lock().unwrap().insert(s.addr);
            true
        }));
        let mut pilot = plan.map(Pilot::new);
        m.pacer = Some(Box::new(move |bus| {
            bus.vt11.out.clear();
            if let Some(p) = &mut pilot {
                p.fly(bus);
            }
            true
        }));
        m.run();
    }
    let seen = seen.lock().unwrap();
    let missing: Vec<&str> = ALL.iter().filter(|b| !seen.contains(&b.addr)).map(|b| b.name).collect();
    println!("{} of {} reached; not reached ({}):", seen.len(), ALL.len(), missing.len());
    println!("{}", missing.join(" "));
}
