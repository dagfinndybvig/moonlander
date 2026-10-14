//! Fly the translated program with the autopilot and print what happens.
//! Usage: fly PLAN SECONDS   (PLAN: land, hard, tilted, sliding, climb, readout)
use moonlander::generated::symbols::*;
use moonlander::pilot::{Pilot, Plan};
use moonlander::Moonlander;
use std::sync::{Arc, Mutex};

fn message(a: u16) -> &'static str {
    let names = [
        (LFTMSG, "LFTMSG"), (RGTMSG, "RGTMSG"), (TOPMSG, "TOPMSG"), (VFAST, "VFAST"), (FAST, "FAST"),
        (N2FAST, "N2FAST"), (GREATM, "GREATM"), (GOODM, "GOODM"), (ROUGHM, "ROUGHM"), (CRIPM, "CRIPM"),
        (DEADM, "DEADM"), (SIDEM, "SIDEM"), (ANGLEM, "ANGLEM"), (BUMPYM, "BUMPYM"), (ROCKMS, "ROCKMS"),
        (OLDMS, "OLDMS"), (FLAGMS, "FLAGMS"), (OLDTLT, "OLDTLT"), (MACDED, "MACDED"), (ORDER, "ORDER"),
        (MANMSG, "MANMSG"), (0, "-"),
    ];
    names.iter().find(|n| n.0 == a).map(|n| n.1).unwrap_or("?")
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let plan = match a.get(1).map(|s| s.as_str()).unwrap_or("land") {
        "hard" => Plan::Hard { fps: 20 },
        "tilted" => Plan::Tilted { degrees: 25 },
        "sliding" => Plan::Sliding { fps: 15 },
        "climb" => Plan::Climb,
        "readout" => Plan::Readout { item: 4, slot: 1 },
        "left" => Plan::Drift { fps: -150 },
        "mac" => Plan::McDonalds,
        "flag" => Plan::Flag,
        "bumpy" => Plan::Bumpy,
        "intomac" => Plan::IntoMcDonalds,
        "rough" => Plan::Hard { fps: 20 },
        "cripple" => Plan::Hard { fps: 40 },
        "dead" => Plan::Hard { fps: 70 },
        "right" => Plan::Drift { fps: 150 },
        _ => Plan::Land,
    };
    let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(150.0);
    let mut m = Moonlander::new(60.0);
    m.limit = secs * 1e6;
    let pilot = Arc::new(Mutex::new(Pilot::new(plan)));
    let p2 = pilot.clone();
    let mut next_print = 30e6;
    m.pacer = Some(Box::new(move |bus| {
        p2.lock().unwrap().fly(bus);
        if bus.t >= next_print {
            next_print += 1e6;
            let w = |x: u16| bus.mem[(x >> 1) as usize] as i16;
            println!(
                "t={:5.1} angle={:4} hv={:6} vv={:6} alt={:6} x={:4} pct={:3} fuel={:5} clock={} hitcnt={:6} msg={} sysmes={}",
                bus.t / 1e6, w(ANGLE), w(HORVEL), w(VERVEL), w(RADARY), w(BIGXCT), w(PERCNT), w(FUEL), w(CLOCK), w(HITCNT),
                message(w(FSUBC) as u16), message(w(SYSMES) as u16)
            );
        }
        true
    }));
    let end = m.run();
    println!("stopped: {end:?} after {} blocks; {} pen moves", m.blocks, pilot.lock().unwrap().script.len());
    if std::env::var("MOVES").is_ok() {
        for (t, p) in &pilot.lock().unwrap().script {
            println!("  {:7.3} {:?}", t / 1e6, p.map(|p| (p.x, p.y)));
        }
    }
}
