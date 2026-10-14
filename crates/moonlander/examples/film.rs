//! Film a flight on the modelled phosphor. The CRT model is fed every beam
//! the display processor draws, and photographed at the machine times an
//! edit list asks for. Frames go to stdout as raw RGB for ffmpeg:
//!
//!     film PLAN FPS WIDTH HEIGHT EDIT [DETAIL] | ffmpeg -f rawvideo -pix_fmt rgb24 \
//!         -s WIDTHxHEIGHT -r FPS -i - ...
//!
//! The edit list is a text file. Lines starting with `#` are comments.
//!
//! - `VIDEO MACHINE` pairs (seconds) map video time to machine time. Between
//!   them machine time follows a monotone cubic through the points, so the
//!   speed changes smoothly. Machine time must keep advancing.
//! - `cut` ends a shot: the next pair jumps to a new machine time. The
//!   machine runs through the gap, so after a cut the phosphor is what the
//!   tube showed at that moment.
//! - `camera VIDEO X Y WIDTH` frames the picture: the point (X, Y) of the
//!   face, in raster units, at the centre, and WIDTH raster units across.
//!   Between camera lines the framing moves with an ease in and out, the
//!   width changing geometrically. Without camera lines the whole face is
//!   shown, 1024 units across.
//!
//! The phosphor is modelled at DETAIL pixels per raster unit (an optional
//! sixth argument, 2 by default) and resampled into the frame, so a closer
//! framing has the detail the tube had.
//!
//! The times at which each flight starts (`RESTAR`) are printed to stderr.
use gt40::crt::Crt;
use moonlander::generated::blocks as blk;
use moonlander::pilot::{Pilot, Plan};
use moonlander::Moonlander;
use rayon::prelude::*;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// One shot: video seconds to machine seconds, a monotone cubic
/// (Fritsch and Carlson) through the points.
struct Shot {
    v: Vec<f64>,
    s: Vec<f64>,
    m: Vec<f64>,
}

impl Shot {
    fn new(points: &[(f64, f64)]) -> Shot {
        let (v, s): (Vec<f64>, Vec<f64>) = points.iter().copied().unzip();
        let n = v.len();
        assert!(n >= 2, "a shot needs two points");
        let d: Vec<f64> = (0..n - 1).map(|i| (s[i + 1] - s[i]) / (v[i + 1] - v[i])).collect();
        assert!(d.iter().all(|&x| x > 0.0), "machine time must advance within a shot");
        let mut m = vec![0.0; n];
        m[0] = d[0];
        m[n - 1] = d[n - 2];
        for i in 1..n - 1 {
            m[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) / 2.0 };
        }
        for i in 0..n - 1 {
            let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
            let h = a * a + b * b;
            if h > 9.0 {
                let t = 3.0 / h.sqrt();
                m[i] = t * a * d[i];
                m[i + 1] = t * b * d[i];
            }
        }
        Shot { v, s, m }
    }

    fn at(&self, x: f64) -> f64 {
        let n = self.v.len();
        let i = (0..n - 1).find(|&i| x <= self.v[i + 1]).unwrap_or(n - 2);
        let h = self.v[i + 1] - self.v[i];
        let t = ((x - self.v[i]) / h).clamp(0.0, 1.0);
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * self.s[i]
            + (t3 - 2.0 * t2 + t) * h * self.m[i]
            + (-2.0 * t3 + 3.0 * t2) * self.s[i + 1]
            + (t3 - t2) * h * self.m[i + 1]
    }
}

/// A framing: centre and width, in raster units.
#[derive(Clone, Copy)]
struct Frame {
    x: f64,
    y: f64,
    w: f64,
}

/// The framing at video time `v`.
fn camera(keys: &[(f64, Frame)], v: f64) -> Frame {
    let full = Frame { x: 512.0, y: 384.0, w: 1024.0 };
    let Some(first) = keys.first() else { return full };
    if v <= first.0 {
        return first.1;
    }
    for k in keys.windows(2) {
        let ((v0, a), (v1, b)) = (k[0], k[1]);
        if v <= v1 {
            let t = (v - v0) / (v1 - v0);
            let e = t * t * (3.0 - 2.0 * t);
            let lerp = |p: f64, q: f64| p + (q - p) * e;
            return Frame { x: lerp(a.x, b.x), y: lerp(a.y, b.y), w: (a.w.ln() + (b.w.ln() - a.w.ln()) * e).exp() };
        }
    }
    keys.last().unwrap().1
}

struct Edit {
    shots: Vec<Shot>,
    camera: Vec<(f64, Frame)>,
}

fn read_edit(path: &str) -> Edit {
    let text = std::fs::read_to_string(path).unwrap();
    let mut shots = Vec::new();
    let mut points = Vec::new();
    let mut cam = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "cut" {
            shots.push(Shot::new(&points));
            points.clear();
            continue;
        }
        let f: Vec<f64> = line.trim_start_matches("camera").split_whitespace().map(|x| x.parse().unwrap()).collect();
        if line.starts_with("camera") {
            cam.push((f[0], Frame { x: f[1], y: f[2], w: f[3] }));
        } else {
            points.push((f[0], f[1]));
        }
    }
    shots.push(Shot::new(&points));
    Edit { shots, camera: cam }
}

/// Resample the model's picture through the framing `f` into `out` (RGB),
/// averaging four samples per output pixel, then dither to 8 bits.
fn shoot(crt: &Crt, model: &[u32], mw: usize, mh: usize, f: Frame, w: usize, h: usize, out: &mut [u8]) {
    let unit = f.w / w as f64;
    let fetch = |px: f32, py: f32| -> [f32; 3] {
        let x = px.clamp(0.0, mw as f32 - 1.001);
        let y = py.clamp(0.0, mh as f32 - 1.001);
        let (x0, y0) = (x as usize, y as usize);
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let mut c = [0.0f32; 3];
        for (dx, dy, wgt) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
            let p = model[(y0 + dy) * mw + x0 + dx];
            c[0] += wgt * ((p >> 16) & 255) as f32;
            c[1] += wgt * ((p >> 8) & 255) as f32;
            c[2] += wgt * (p & 255) as f32;
        }
        c
    };
    out.par_chunks_mut(w * 3).enumerate().for_each(|(j, row)| {
        for i in 0..w {
            let mut c = [0.0f32; 3];
            for (sx, sy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                let rx = f.x - f.w / 2.0 + (i as f64 + sx) * unit;
                let ry = f.y + unit * h as f64 / 2.0 - (j as f64 + sy) * unit;
                let (px, py) = crt.to_pixel(rx as f32, ry as f32);
                let s = fetch(px - 0.5, py - 0.5);
                for k in 0..3 {
                    c[k] += s[k] / 4.0;
                }
            }
            // The same triangular noise on all three channels, as the
            // model's own output uses, so dark gradients do not band.
            let n = noise(i as u32, j as u32);
            for k in 0..3 {
                row[i * 3 + k] = (c[k] + n).round().clamp(0.0, 255.0) as u8;
            }
        }
    });
}

fn noise(x: u32, y: u32) -> f32 {
    let mut a = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    a ^= a >> 15;
    a = a.wrapping_mul(0x2C1B_3C6D);
    a ^= a >> 12;
    let u = (a & 0xFFFF) as f32 / 65536.0;
    let v = (a >> 16) as f32 / 65536.0;
    u + v - 1.0
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let plan = match a[1].as_str() {
        "land" => Plan::Land,
        "mcdonalds" => Plan::McDonalds,
        "flag" => Plan::Flag,
        other => panic!("plan {other}"),
    };
    let fps: f64 = a[2].parse().unwrap();
    let (w, h): (usize, usize) = (a[3].parse().unwrap(), a[4].parse().unwrap());
    let edit = read_edit(&a[5]);
    let detail: f32 = a.get(6).map(|d| d.parse().unwrap()).unwrap_or(2.0);

    // Machine time and framing of every frame.
    let mut frames = Vec::new();
    for (k, shot) in edit.shots.iter().enumerate() {
        let (v0, v1) = (shot.v[0], *shot.v.last().unwrap());
        let last = k == edit.shots.len() - 1;
        let mut i = (v0 * fps).ceil() as usize;
        while (i as f64) / fps < v1 || (last && (i as f64) / fps <= v1 - 0.5 / fps) {
            let v = i as f64 / fps;
            frames.push((shot.at(v) * 1e6, camera(&edit.camera, v)));
            i += 1;
        }
    }
    assert!(frames.windows(2).all(|p| p[0].0 <= p[1].0), "shots must follow each other in machine time");
    eprintln!("{} frames, {:.2} s", frames.len(), frames.len() as f64 / fps);

    // The model's canvas covers the widest framing asked for.
    let widest = frames.iter().map(|f| f.1.w).fold(1024.0, f64::max) as f32;
    let mw = (widest * detail).ceil() as usize;
    let mh = ((widest * h as f32 / w as f32).max(768.0) * detail).ceil() as usize;

    let mut m = Moonlander::new(60.0);
    m.limit = frames.last().unwrap().0 + 20_000.0;
    let pilot = Arc::new(Mutex::new(Pilot::new(plan)));
    let restarts = Arc::new(Mutex::new(Vec::new()));
    let r = restarts.clone();
    m.observer = Some(Box::new(move |s| {
        if s.addr == blk::RESTAR.addr {
            r.lock().unwrap().push(s.t);
        }
        true
    }));
    let mut crt = Crt::new(mw, mh);
    let mut model = vec![0u32; mw * mh];
    let mut bytes = vec![0u8; w * h * 3];
    let mut out = std::io::BufWriter::new(std::io::stdout());
    let mut next = 0;
    m.pacer = Some(Box::new(move |bus| {
        pilot.lock().unwrap().fly(bus);
        if next >= frames.len() {
            return false;
        }
        // Beams more than a second before the next frame have faded to
        // nothing by then; drop them unseen.
        if bus.t < frames[next].0 - 1e6 {
            bus.vt11.out.clear();
            return true;
        }
        crt.advance(bus.t);
        for b in bus.vt11.out.drain(..) {
            crt.deposit(&b);
        }
        while next < frames.len() && bus.t >= frames[next].0 {
            crt.render(&mut model);
            shoot(&crt, &model, mw, mh, frames[next].1, w, h, &mut bytes);
            out.write_all(&bytes).unwrap();
            next += 1;
            if next % 100 == 0 {
                eprintln!("frame {next}");
            }
        }
        true
    }));
    m.run();
    for t in restarts.lock().unwrap().iter() {
        eprintln!("RESTAR at {:.3} s", t / 1e6);
    }
}
