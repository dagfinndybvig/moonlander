//! What the window draws over the tube: the help, short notices, and where
//! the light pen is. It uses the VT11's own character shapes but a neutral
//! grey, so nothing here can be mistaken for the GT40's picture.

use gt40::chargen::glyph;

pub const INK: [f32; 3] = [0.80, 0.79, 0.74];
pub const FAINT: [f32; 3] = [0.50, 0.49, 0.46];

pub struct Canvas<'a> {
    pub px: &'a mut [u32],
    pub w: usize,
    pub h: usize,
}

fn unpack(p: u32) -> [f32; 3] {
    [((p >> 16) & 255) as f32 / 255.0, ((p >> 8) & 255) as f32 / 255.0, (p & 255) as f32 / 255.0]
}

fn pack(c: [f32; 3]) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    (b(c[0]) << 16) | (b(c[1]) << 8) | b(c[2])
}

impl Canvas<'_> {
    /// Blend `color` over the pixel at (x, y) with opacity `a`.
    fn blend(&mut self, x: i32, y: i32, color: [f32; 3], a: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h || a <= 0.0 {
            return;
        }
        let i = y as usize * self.w + x as usize;
        let o = unpack(self.px[i]);
        let a = a.min(1.0);
        self.px[i] = pack([0, 1, 2].map(|k| o[k] + (color[k] - o[k]) * a));
    }

    /// Darken a rectangle, to set text on.
    pub fn shade(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, keep: f32) {
        for y in y0.max(0)..y1.min(self.h as i32) {
            for x in x0.max(0)..x1.min(self.w as i32) {
                let i = y as usize * self.w + x as usize;
                self.px[i] = pack(unpack(self.px[i]).map(|v| v * keep));
            }
        }
    }

    /// Width of `s` in pixels with dots `dot` pixels apart.
    pub fn width(s: &str, dot: i32) -> i32 {
        s.chars().count() as i32 * 7 * dot
    }

    /// Text with its top left at (x, y), in the character generator's
    /// 6 x 8 dots, `dot` pixels apart.
    pub fn text(&mut self, x: i32, y: i32, dot: i32, color: [f32; 3], a: f32, s: &str) {
        let size = if dot >= 3 { dot - 1 } else { dot };
        for (n, ch) in s.chars().enumerate() {
            let code = if ch.is_ascii() { ch as u8 } else { b'?' };
            let (dots, drop) = glyph(code);
            let cx = x + n as i32 * 7 * dot;
            for (col, row) in dots {
                let px = cx + col as i32 * dot;
                let py = y + (7 - row as i32) * dot - drop as i32 / 2 * dot;
                for yy in 0..size {
                    for xx in 0..size {
                        self.blend(px + xx, py + yy, color, a);
                    }
                }
            }
        }
    }

    /// A thin circle, antialiased.
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, color: [f32; 3], a: f32) {
        let reach = (r + 2.0).ceil() as i32;
        for y in (cy as i32 - reach)..=(cy as i32 + reach) {
            for x in (cx as i32 - reach)..=(cx as i32 + reach) {
                let d = ((x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) - r).abs();
                self.blend(x, y, color, a * (1.2 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// A block of lines on a shaded panel, centred. A line starting with
    /// `~` is drawn fainter (the `~` is not shown).
    pub fn panel(&mut self, lines: &[&str], dot: i32) {
        let lh = 12 * dot;
        let pad = 8 * dot;
        let wmax = lines.iter().map(|l| Self::width(l.trim_start_matches('~'), dot)).max().unwrap_or(0);
        let (pw, ph) = (wmax + 2 * pad, lines.len() as i32 * lh + 2 * pad - 4 * dot);
        let (x0, y0) = ((self.w as i32 - pw) / 2, (self.h as i32 - ph) / 2);
        self.shade(x0, y0, x0 + pw, y0 + ph, 0.07);
        self.frame(x0, y0, x0 + pw, y0 + ph, FAINT, 0.6);
        for (i, l) in lines.iter().enumerate() {
            let (color, l) = match l.strip_prefix('~') {
                Some(rest) => (FAINT, rest),
                None => (INK, *l),
            };
            self.text(x0 + pad, y0 + pad + i as i32 * lh, dot, color, 1.0, l);
        }
    }

    /// A one-pixel rectangle.
    pub fn frame(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: [f32; 3], a: f32) {
        for x in x0..x1 {
            self.blend(x, y0, color, a);
            self.blend(x, y1 - 1, color, a);
        }
        for y in y0 + 1..y1 - 1 {
            self.blend(x0, y, color, a);
            self.blend(x1 - 1, y, color, a);
        }
    }

    /// One line on a shaded strip, at the bottom left (row 0 lowest).
    pub fn note(&mut self, row: i32, dot: i32, a: f32, s: &str) {
        let lh = 12 * dot;
        let y = self.h as i32 - (row + 1) * lh - 2 * dot;
        let x = 3 * dot;
        self.shade(x - 2 * dot, y - 2 * dot, x + Self::width(s, dot) + dot, y + 10 * dot, 1.0 - 0.7 * a);
        self.text(x, y, dot, INK, a, s);
    }

    /// One line on a shaded strip, centred across the middle.
    pub fn banner(&mut self, dot: i32, s: &str) {
        let wpx = Self::width(s, dot);
        let x = (self.w as i32 - wpx) / 2;
        let y = self.h as i32 / 2 - 4 * dot;
        self.shade(x - 4 * dot, y - 4 * dot, x + wpx + 3 * dot, y + 12 * dot, 0.25);
        self.text(x, y, dot, INK, 1.0, s);
    }
}
