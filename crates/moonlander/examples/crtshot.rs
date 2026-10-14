//! Fly the translation and photograph the phosphor: the CRT model fed with
//! every beam as it is drawn, saved at the given moments.
//! Usage: crtshot PLAN SECONDS[,SECONDS...] PREFIX [WIDTH HEIGHT] [clean]
use gt40::crt::{render_clean, Crt, CLEAN_SPAN_US};
use gt40::vt11::Beam;
use moonlander::pilot::{Pilot, Plan};
use moonlander::Moonlander;
use std::io::BufWriter;
use std::sync::{Arc, Mutex};

fn save(path: &str, w: usize, h: usize, px: &[u32]) {
    let f = std::fs::File::create(path).unwrap();
    let mut e = png::Encoder::new(BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    let bytes: Vec<u8> = px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    e.write_header().unwrap().write_image_data(&bytes).unwrap();
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let plan = match a[1].as_str() {
        "none" => None,
        "land" => Some(Plan::Land),
        "mac" => Some(Plan::McDonalds),
        "flag" => Some(Plan::Flag),
        s if s.starts_with("at") => Some(Plan::LandAt { index: s[2..].parse().unwrap() }),
        other => panic!("plan {other}"),
    };
    let times: Vec<f64> = a[2].split(',').map(|s| s.parse::<f64>().unwrap() * 1e6).collect();
    let prefix = a[3].clone();
    let (w, h) = if a.len() > 5 { (a[4].parse().unwrap(), a[5].parse().unwrap()) } else { (1024usize, 768usize) };
    let clean = a.iter().any(|s| s == "clean");
    let mut m = Moonlander::new(60.0);
    m.limit = *times.last().unwrap() + 20_000.0;
    let pilot = Arc::new(Mutex::new(plan.map(Pilot::new)));
    let crt = Arc::new(Mutex::new(Crt::new(w, h)));
    let recent: Arc<Mutex<Vec<Beam>>> = Arc::new(Mutex::new(Vec::new()));
    let mut next = 0;
    let mut frame = 0.0;
    m.pacer = Some(Box::new(move |bus| {
        if let Some(p) = pilot.lock().unwrap().as_mut() {
            p.fly(bus);
        }
        // As a window would: the phosphor is brought up to time once a frame.
        let due = next < times.len() && bus.t >= times[next];
        if bus.t < frame && !due {
            return true;
        }
        frame = bus.t + 1e6 / 60.0;
        let mut c = crt.lock().unwrap();
        c.advance(bus.t);
        let mut r = recent.lock().unwrap();
        for b in bus.vt11.out.drain(..) {
            c.deposit(&b);
            r.push(b);
        }
        let cut = bus.t - CLEAN_SPAN_US;
        r.retain(|b| match *b {
            Beam::Dot { t, .. } => t >= cut,
            Beam::Line { t1, .. } => t1 >= cut,
        });
        while next < times.len() && bus.t >= times[next] {
            let mut px = vec![0u32; w * h];
            if clean {
                render_clean(&c, &r, &mut px);
            } else {
                c.render(&mut px);
            }
            let path = format!("{prefix}-{next}.png");
            save(&path, w, h, &px);
            eprintln!("{path}");
            next += 1;
        }
        true
    }));
    m.run();
}
