//! Fly the translation with the autopilot and save pictures of the screen.
//! Usage: shot PLAN SECONDS[,SECONDS...] PREFIX   (DEP as for `fly`)
//! Each picture is the last 1/30 s of beam activity before that moment.
use gt40::raster::Raster;
use gt40::vt11::Beam;
use moonlander::pilot::{Pilot, Plan};
use moonlander::Moonlander;
use std::io::BufWriter;
use std::sync::{Arc, Mutex};

fn save(path: &str, beams: &[Beam]) {
    let mut r = Raster::new();
    for b in beams {
        r.draw(b);
    }
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(BufWriter::new(f), r.w as u32, r.h as u32);
    e.set_color(png::ColorType::Rgb);
    e.write_header().unwrap().write_image_data(&r.rgb()).unwrap();
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let plan = match a[1].as_str() {
        "none" => None,
        "land" => Some(Plan::Land),
        "mac" => Some(Plan::McDonalds),
        "flag" => Some(Plan::Flag),
        "lowleft" => Some(Plan::LowDrift { fps: -30, alt: 150 }),
        s if s.starts_with("at") => Some(Plan::LandAt { index: s[2..].parse().unwrap() }),
        other => panic!("plan {other}"),
    };
    let times: Vec<f64> = a[2].split(',').map(|s| s.parse::<f64>().unwrap() * 1e6).collect();
    let prefix = a[3].clone();
    let mut m = Moonlander::new(60.0);
    m.limit = *times.last().unwrap() + 1.0;
    if let Ok(d) = std::env::var("DEP") {
        for item in d.split(',') {
            let f: Vec<&str> = item.split(':').collect();
            let t: f64 = f[0].parse().unwrap();
            m.bus.deposits.push_back((t * 1e6, u16::from_str_radix(f[1], 8).unwrap(), u16::from_str_radix(f[2], 8).unwrap()));
        }
    }
    let pilot = Arc::new(Mutex::new(plan.map(Pilot::new)));
    let mut next = 0;
    m.pacer = Some(Box::new(move |bus| {
        if let Some(p) = pilot.lock().unwrap().as_mut() {
            p.fly(bus);
        }
        let cut = bus.t - 1e6 / 30.0;
        bus.vt11.out.retain(|b| match *b {
            Beam::Dot { t, .. } => t >= cut,
            Beam::Line { t1, .. } => t1 >= cut,
        });
        while next < times.len() && bus.t >= times[next] {
            let path = format!("{prefix}-{next}.png");
            save(&path, &bus.vt11.out);
            let w = |x: u16| bus.mem[(x >> 1) as usize] as i16;
            use moonlander::generated::symbols::*;
            eprintln!("{path}: t={:.1} x={} alt={} verdis={} lefted={}", bus.t / 1e6, w(BIGXCT), w(RADARY), w(VERDIS), w(LEFTED));
            next += 1;
        }
        true
    }));
    m.run();
}
