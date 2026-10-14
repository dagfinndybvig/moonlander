//! A plain software rasterizer for beam events: additive spots on a
//! 1024 x 768 field, no persistence. Used for snapshots and tests; the
//! front end has its own phosphor renderer.

use crate::vt11::{Beam, FACE_H, FACE_W};

pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub energy: Vec<f32>,
}

impl Raster {
    pub fn new() -> Raster {
        Raster { w: FACE_W as usize, h: FACE_H as usize, energy: vec![0.0; (FACE_W * FACE_H) as usize] }
    }

    fn plot(&mut self, x: f32, y: f32, k: f32) {
        let (xi, yi) = (x.round() as i64, (FACE_H as f32 - 1.0 - y).round() as i64);
        for (ox, oy, f) in [(0, 0, 1.0), (1, 0, 0.35), (-1, 0, 0.35), (0, 1, 0.35), (0, -1, 0.35)] {
            let (px, py) = (xi + ox, yi + oy);
            if px >= 0 && py >= 0 && (px as usize) < self.w && (py as usize) < self.h {
                self.energy[py as usize * self.w + px as usize] += k * f;
            }
        }
    }

    /// Relative brightness of the VT11's eight intensity levels (MODEL).
    pub fn level(l: u8) -> f32 {
        0.35 + 0.65 * l as f32 / 7.0
    }

    pub fn draw(&mut self, b: &Beam) {
        match *b {
            Beam::Dot { x, y, level, .. } => self.plot(x, y, Self::level(level)),
            Beam::Line { x0, y0, x1, y1, level, .. } => {
                let len = (x1 - x0).hypot(y1 - y0).ceil().max(1.0);
                for i in 0..=len as i32 {
                    let s = i as f32 / len;
                    self.plot(x0 + (x1 - x0) * s, y0 + (y1 - y0) * s, Self::level(level));
                }
            }
        }
    }

    /// RGB bytes in a P39-like green.
    pub fn rgb(&self) -> Vec<u8> {
        self.energy
            .iter()
            .flat_map(|&v| {
                let g = (v.min(1.6) / 1.6).powf(0.7);
                [(g * 120.0) as u8, (g * 255.0) as u8, (g * 140.0) as u8]
            })
            .collect()
    }
}

impl Default for Raster {
    fn default() -> Self {
        Self::new()
    }
}
