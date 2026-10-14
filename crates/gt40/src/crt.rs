//! The GT40's monitor: a VR14 tube with a P39 phosphor, lit by the VT11's
//! beam. P39 is a long-persistence green: a spot keeps glowing for a good
//! part of a second, so a picture redrawn a few dozen times a second holds
//! still instead of flickering.
//!
//! The model keeps the phosphor's excitation per pixel. The beam adds to it
//! in proportion to how long it dwells on each pixel (a slowly drawn short
//! vector is brighter than a long fast one, a character dot brighter than
//! either) and to the intensity level; the excitation decays exponentially
//! with machine time, so what is on the screen at any moment depends only
//! on what the beam did and when, not on the frame rate of the window it is
//! shown in. A wide blur of the excitation adds the glow of the faceplate.
//!
//! The picture is the same at any window size: the spot and the glow are
//! sized in raster units, and brightness follows excitation per unit area
//! of the face, not per pixel.
//!
//! MODEL constants are marked. The spot size is from the GT40 manual. The
//! manual names the phosphor (P39) but gives no persistence, and the
//! sources reached so far call P39 "long" without a figure. The decay used
//! here, to 10% in 150 ms, is an assumption, and the first thing to change
//! if a datasheet turns up. The rest are judged by eye, not measured.

use crate::vt11::{Beam, FACE_H, FACE_W};
use rayon::prelude::*;

/// MODEL: P39 falls to 10% of its brightness in 150 ms (see above).
pub const P39_TAU_US: f64 = 150_000.0 / std::f64::consts::LN_10;
/// MODEL: dwell of a character dot or a point, microseconds.
const DOT_DWELL_US: f32 = 0.5;
/// MODEL: excitation (microseconds of dwell at full intensity, summed with
/// decay) that lights a pixel to 63% of full brightness.
const SATURATION: f32 = 0.5;
/// MODEL: strength of the faceplate glow.
const GLOW: f32 = 0.25;
/// The spot's spread, raster units (a Gaussian's sigma). The GT40 manual
/// gives the spot as under 20 mils across at half its peak brightness; the
/// face is 9 inches for 1024 raster units, so that is under 2.3 units. This
/// is 2.0.
const SPOT_SIGMA: f32 = 0.85;
/// Size of the glow's reduced image, raster units per cell.
const GLOW_CELL: f32 = 4.0;
/// MODEL: light from the faceplate itself, from room light and the
/// electron gun, linear units.
const FACE: f32 = 0.0012;

/// Relative beam current at each of the VT11's eight intensity levels
/// (MODEL; the VT11 manual gives no figures).
pub fn level(l: u8) -> f32 {
    0.30 + 0.70 * l as f32 / 7.0
}

pub struct Crt {
    pub w: usize,
    pub h: usize,
    energy: Vec<f32>,
    /// The excitation seen through the spot: the beam lights a round spot
    /// about two raster units across, not a single point.
    seen: Vec<f32>,
    seen_tmp: Vec<f32>,
    glow: Vec<f32>,
    glow_tmp: Vec<f32>,
    /// Pixels per glow cell.
    glow_down: usize,
    /// Machine time the excitation is current to, microseconds.
    now: f64,
    /// Raster units to pixels: pixel = origin + unit * scale (y flipped).
    scale: f32,
    ox: f32,
    oy: f32,
}

impl Crt {
    /// A tube drawn into a `w` x `h` pixel window, the 1024 x 768 face
    /// fitted inside it.
    pub fn new(w: usize, h: usize) -> Crt {
        let mut c = Crt {
            w: 0,
            h: 0,
            energy: Vec::new(),
            seen: Vec::new(),
            seen_tmp: Vec::new(),
            glow: Vec::new(),
            glow_tmp: Vec::new(),
            glow_down: 4,
            now: 0.0,
            scale: 1.0,
            ox: 0.0,
            oy: 0.0,
        };
        c.resize(w, h);
        c
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        if (w, h) == (self.w, self.h) {
            return;
        }
        self.w = w.max(1);
        self.h = h.max(1);
        self.energy = vec![0.0; self.w * self.h];
        self.seen = vec![0.0; self.w * self.h];
        self.seen_tmp = vec![0.0; self.w * self.h];
        self.scale = (self.w as f32 / FACE_W as f32).min(self.h as f32 / FACE_H as f32);
        self.glow_down = ((GLOW_CELL * self.scale).round() as usize).max(1);
        let (gw, gh) = self.glow_size();
        self.glow = vec![0.0; gw * gh];
        self.glow_tmp = vec![0.0; gw * gh];
        self.ox = (self.w as f32 - FACE_W as f32 * self.scale) / 2.0;
        self.oy = (self.h as f32 - FACE_H as f32 * self.scale) / 2.0;
    }

    fn glow_size(&self) -> (usize, usize) {
        (self.w / self.glow_down + 1, self.h / self.glow_down + 1)
    }

    /// Pixels per raster unit.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Machine time the phosphor is current to.
    pub fn now(&self) -> f64 {
        self.now
    }

    /// Forget everything drawn (the tube switched off and on).
    pub fn clear(&mut self) {
        self.energy.iter_mut().for_each(|e| *e = 0.0);
    }

    /// Window pixel for a raster position.
    pub fn to_pixel(&self, x: f32, y: f32) -> (f32, f32) {
        (self.ox + x * self.scale, self.oy + (FACE_H as f32 - y) * self.scale)
    }

    /// Raster position for a window pixel.
    pub fn to_raster(&self, px: f32, py: f32) -> (f32, f32) {
        ((px - self.ox) / self.scale, FACE_H as f32 - (py - self.oy) / self.scale)
    }

    /// Let the phosphor decay up to machine time `t`.
    pub fn advance(&mut self, t: f64) {
        if t <= self.now {
            return;
        }
        let k = (-(t - self.now) / P39_TAU_US).exp() as f32;
        self.energy.par_iter_mut().for_each(|e| *e *= k);
        self.now = t;
    }

    fn add(&mut self, x: i32, y: i32, e: f32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.energy[y as usize * self.w + x as usize] += e;
        }
    }

    /// A spot of excitation `e` at pixel (x, y), spread over the four
    /// pixels around it.
    fn spot(&mut self, x: f32, y: f32, e: f32) {
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (xi, yi) = (x0 as i32, y0 as i32);
        self.add(xi, yi, e * (1.0 - fx) * (1.0 - fy));
        self.add(xi + 1, yi, e * fx * (1.0 - fy));
        self.add(xi, yi + 1, e * (1.0 - fx) * fy);
        self.add(xi + 1, yi + 1, e * fx * fy);
    }

    /// Light the phosphor where the beam went. Beams later than the current
    /// time count as if at the current time.
    pub fn deposit(&mut self, b: &Beam) {
        let fade = |t: f64, now: f64| (-((now - t).max(0.0)) / P39_TAU_US).exp() as f32;
        match *b {
            Beam::Dot { x, y, t, level: l } => {
                let (px, py) = self.to_pixel(x, y);
                let e = DOT_DWELL_US * level(l) * fade(t, self.now);
                self.spot(px, py, e);
            }
            Beam::Line { x0, y0, x1, y1, t0, t1, level: l } => {
                let (ax, ay) = self.to_pixel(x0, y0);
                let (bx, by) = self.to_pixel(x1, y1);
                let len_px = (bx - ax).hypot(by - ay);
                let steps = (len_px.ceil() as usize).max(1);
                // Dwell per pixel along the line.
                let dwell = ((t1 - t0) as f32 / steps as f32).max(0.0);
                let f = fade(t1, self.now);
                let e = dwell * level(l) * f;
                for i in 0..=steps {
                    let s = i as f32 / steps as f32;
                    self.spot(ax + (bx - ax) * s, ay + (by - ay) * s, e);
                }
            }
        }
    }

    /// Spread the excitation over the spot, a Gaussian of SPOT_SIGMA
    /// raster units.
    fn compute_spot(&mut self) {
        let sigma = SPOT_SIGMA * self.scale;
        let r = (3.0 * sigma).ceil().max(1.0) as i32;
        let mut k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let sum: f32 = k.iter().sum();
        k.iter_mut().for_each(|v| *v /= sum);
        let (w, h) = (self.w as i32, self.h as i32);
        let energy = &self.energy;
        let k = &k;
        self.seen_tmp.par_chunks_mut(self.w).enumerate().for_each(|(y, row)| {
            let src = &energy[y * w as usize..(y + 1) * w as usize];
            for x in 0..w {
                let mut s = 0.0;
                for (i, kv) in k.iter().enumerate() {
                    let xx = x + i as i32 - r;
                    if xx >= 0 && xx < w {
                        s += src[xx as usize] * kv;
                    }
                }
                row[x as usize] = s;
            }
        });
        let tmp = &self.seen_tmp;
        self.seen.par_chunks_mut(self.w).enumerate().for_each(|(y, row)| {
            for (i, kv) in k.iter().enumerate() {
                let yy = y as i32 + i as i32 - r;
                if i == 0 {
                    row.iter_mut().for_each(|v| *v = 0.0);
                }
                if yy >= 0 && yy < h {
                    let src = &tmp[yy as usize * w as usize..(yy as usize + 1) * w as usize];
                    for (o, s) in row.iter_mut().zip(src) {
                        *o += s * kv;
                    }
                }
            }
        });
    }

    fn compute_glow(&mut self) {
        let (gw, gh) = self.glow_size();
        let gd = self.glow_down;
        self.glow.iter_mut().for_each(|g| *g = 0.0);
        for y in 0..self.h {
            let row = &self.energy[y * self.w..(y + 1) * self.w];
            let gy = y / gd;
            for (x, &e) in row.iter().enumerate() {
                self.glow[gy * gw + x / gd] += e;
            }
        }
        let norm = 1.0 / (gd * gd) as f32;
        // Separable blur, a binomial kernel of radius 4 at glow resolution.
        const K: [f32; 9] = [1.0, 8.0, 28.0, 56.0, 70.0, 56.0, 28.0, 8.0, 1.0];
        let ks: f32 = K.iter().sum();
        for y in 0..gh {
            for x in 0..gw {
                let mut s = 0.0;
                for (i, k) in K.iter().enumerate() {
                    let xx = (x as i32 + i as i32 - 4).clamp(0, gw as i32 - 1) as usize;
                    s += self.glow[y * gw + xx] * k;
                }
                self.glow_tmp[y * gw + x] = s / ks;
            }
        }
        for y in 0..gh {
            for x in 0..gw {
                let mut s = 0.0;
                for (i, k) in K.iter().enumerate() {
                    let yy = (y as i32 + i as i32 - 4).clamp(0, gh as i32 - 1) as usize;
                    s += self.glow_tmp[yy * gw + x] * k;
                }
                self.glow[y * gw + x] = s / ks * norm;
            }
        }
    }

    /// The picture: 0RGB pixels, the phosphor glowing on a dark faceplate.
    pub fn render(&mut self, out: &mut [u32]) {
        self.compute_spot();
        self.compute_glow();
        let (gw, gh) = self.glow_size();
        let gd = self.glow_down as f32;
        // Excitation per pixel to excitation per square raster unit.
        let density = self.scale * self.scale;
        let (cx, cy) = (self.w as f32 / 2.0, self.h as f32 / 2.0);
        let rmax = cx.hypot(cy);
        let w = self.w;
        let (energy, glowbuf) = (&self.seen, &self.glow);
        out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let i = y * w + x;
                // Glow, sampled bilinearly from the reduced image.
                let gx = x as f32 / gd;
                let gy = y as f32 / gd;
                let (gx0, gy0) = (gx.floor() as usize, gy.floor() as usize);
                let (fx, fy) = (gx - gx0 as f32, gy - gy0 as f32);
                let g = |xx: usize, yy: usize| glowbuf[yy.min(gh - 1) * gw + xx.min(gw - 1)];
                let glow = g(gx0, gy0) * (1.0 - fx) * (1.0 - fy)
                    + g(gx0 + 1, gy0) * fx * (1.0 - fy)
                    + g(gx0, gy0 + 1) * (1.0 - fx) * fy
                    + g(gx0 + 1, gy0 + 1) * fx * fy;
                let e = (energy[i] + GLOW * glow) * density;
                let b = 1.0 - (-e / SATURATION).exp();
                // P39's yellowish green, going towards white where the beam
                // is brightest; a faint grey-green faceplate under it, a
                // little darker towards the corners.
                let r = ((x as f32 - cx).hypot(y as f32 - cy) / rmax).min(1.0);
                let face = FACE * (1.0 - 0.5 * r * r);
                let hot = ((b - 0.85) / 0.15).clamp(0.0, 1.0) * 0.35;
                let red = face * 0.8 + b * 0.36 + hot * 0.6;
                let green = face * 1.0 + b * 1.00 + hot * 0.0;
                let blue = face * 0.85 + b * 0.30 + hot * 0.55;
                row[x] = pack(red, green, blue);
            }
        });
    }
}

/// How long the clean view keeps showing a beam, microseconds: fully for
/// CLEAN_FULL_US, then fading to nothing at CLEAN_SPAN_US. Most of the
/// picture is redrawn every 8 to 13 ms, but while the processor is busy,
/// parts of a long display file come round only every 40 to 60 ms.
pub const CLEAN_FULL_US: f64 = 20_000.0;
pub const CLEAN_SPAN_US: f64 = 70_000.0;

/// The beam's path without the phosphor: the beams of the last few
/// refreshes as crisp lines on black, brightness by intensity level, those
/// not redrawn lately fading out. `crt` gives the geometry and the time.
pub fn render_clean(crt: &Crt, beams: &[Beam], out: &mut [u32]) {
    let (w, h, now) = (crt.w, crt.h, crt.now);
    let weight = |t: f64| (1.0 - ((now - t - CLEAN_FULL_US) / (CLEAN_SPAN_US - CLEAN_FULL_US)).clamp(0.0, 1.0)) as f32;
    // Lines about a raster unit wide.
    let half = ((crt.scale - 1.0) / 2.0).max(0.0);
    let offsets: &[(f32, f32)] =
        if half < 0.25 { &[(0.0, 0.0)] } else { &[(-half, -half), (half, -half), (-half, half), (half, half)] };
    let mut acc = vec![0f32; w * h];
    let mut put = |x: f32, y: f32, v: f32| {
        for &(ox, oy) in offsets {
            let (x, y) = (x + ox, y + oy);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = (x - x0, y - y0);
            for (dx, dy, k) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
                let (xi, yi) = (x0 as i32 + dx, y0 as i32 + dy);
                if xi >= 0 && yi >= 0 && (xi as usize) < w && (yi as usize) < h {
                    let p = &mut acc[yi as usize * w + xi as usize];
                    *p = p.max(v * k * 1.6);
                }
            }
        }
    };
    for b in beams {
        match *b {
            Beam::Dot { x, y, t, level: l } => {
                let (px, py) = crt.to_pixel(x, y);
                put(px, py, level(l) * weight(t));
            }
            Beam::Line { x0, y0, x1, y1, t1, level: l, .. } => {
                let (ax, ay) = crt.to_pixel(x0, y0);
                let (bx, by) = crt.to_pixel(x1, y1);
                let v = level(l) * weight(t1);
                if v <= 0.0 {
                    continue;
                }
                let n = ((bx - ax).hypot(by - ay) * 2.0).ceil().max(1.0) as usize;
                for i in 0..=n {
                    let s = i as f32 / n as f32;
                    put(ax + (bx - ax) * s, ay + (by - ay) * s, v);
                }
            }
        }
    }
    for (o, &a) in out.iter_mut().zip(&acc) {
        let v = a.min(1.0);
        *o = pack(v * 0.92, v, v * 0.92);
    }
}

pub fn pack(r: f32, g: f32, b: f32) -> u32 {
    let c = |v: f32| (v.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u32;
    (c(r) << 16) | (c(g) << 8) | c(b)
}
