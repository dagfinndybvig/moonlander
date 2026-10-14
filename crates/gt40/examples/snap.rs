//! Run a display file from a loader image through the VT11 model and save
//! what the beam drew as a PNG (additive spots, no persistence).
//! Usage: snap IMAGE.lda START_OCTAL OUT.png [MICROSECONDS]
use gt40::vt11::{Beam, Vt11, FACE_H, FACE_W};
use std::io::BufWriter;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let img = gt40::lda::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let start = u16::from_str_radix(&a[2], 8).unwrap();
    let until: f64 = a.get(4).map(|s| s.parse().unwrap()).unwrap_or(40_000.0);
    let mut mem = vec![0u16; 0o40000 / 2];
    img.load_into(&mut mem);
    let mut v = Vt11::new(60.0);
    v.write_dpc(start, 0.0);
    v.run_until(mem.as_slice(), until);
    let (w, h) = (FACE_W as usize, FACE_H as usize);
    let mut buf = vec![0f32; w * h];
    let mut plot = |x: f32, y: f32, k: f32| {
        let (xi, yi) = (x.round() as i64, (FACE_H as f32 - 1.0 - y).round() as i64);
        for (ox, oy, f) in [(0, 0, 1.0), (1, 0, 0.35), (-1, 0, 0.35), (0, 1, 0.35), (0, -1, 0.35)] {
            let (px, py) = (xi + ox, yi + oy);
            if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                buf[py as usize * w + px as usize] += k * f;
            }
        }
    };
    let lvl = |l: u8| 0.35 + 0.65 * l as f32 / 7.0;
    let mut n = (0, 0);
    for b in &v.out {
        match *b {
            Beam::Dot { x, y, level, .. } => { plot(x, y, lvl(level)); n.0 += 1; }
            Beam::Line { x0, y0, x1, y1, level, .. } => {
                let len = ((x1 - x0).hypot(y1 - y0)).ceil().max(1.0);
                for i in 0..=len as i32 { let s = i as f32 / len; plot(x0 + (x1 - x0) * s, y0 + (y1 - y0) * s, lvl(level)); }
                n.1 += 1;
            }
        }
    }
    let px: Vec<u8> = buf.iter().flat_map(|&v| {
        let g = (v.min(1.6) / 1.6).powf(0.7);
        [(g * 120.0) as u8, (g * 255.0) as u8, (g * 140.0) as u8]
    }).collect();
    let f = std::fs::File::create(&a[3]).unwrap();
    let mut e = png::Encoder::new(BufWriter::new(f), w as u32, h as u32);
    e.set_color(png::ColorType::Rgb);
    e.write_header().unwrap().write_image_data(&px).unwrap();
    eprintln!("{} dots, {} lines, DPU time {:.0} us, stopped at {:06o}", n.0, n.1, v.time(), v.read_dpc());
}
