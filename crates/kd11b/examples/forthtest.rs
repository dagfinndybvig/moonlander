//! Headless test for GT40 FORTH: boot forth.lda, type lines at it through
//! the console, and dump the screen both as decoded display-list text and
//! as a PNG.
//!
//! Usage: forthtest IMAGE.lda OUT.png LINE[,LINE...]
//! Lines are sent with \r as the return; characters are deposited one at a
//! time whenever the console done bit is clear.

use gt40::raster::Raster;
use gt40::vt11::Beam;
use kd11b::Machine;
use std::collections::VecDeque;
use std::io::BufWriter;

const DLIST: u16 = 0o30000;

fn screen_text(mem: &[u16]) -> Vec<String> {
    let word = |a: u16| mem[(a >> 1) as usize];
    let mut rows = Vec::new();
    for r in 0..24u16 {
        let base = DLIST + r * 40 + 8;
        let mut s = String::new();
        for c in 0..32u16 {
            let b = if c & 1 == 0 {
                word(base + c) & 0o177
            } else {
                (word(base + c - 1) >> 8) & 0o177
            };
            s.push(if b >= 0o40 && b < 0o177 { b as u8 as char } else { ' ' });
        }
        rows.push(s.trim_end().to_string());
    }
    rows
}

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
    let img = gt40::lda::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let out = &a[2];
    let lines: Vec<Vec<u8>> = a[3]
        .replace("\\,", "\u{0}")
        .split(',')
        .map(|l| l.replace("\u{0}", ",").replace("\\r", "\r").into_bytes())
        .collect();
    let mut m = Machine::new(0o20000, 60.0);
    m.load(&img);
    let mut queue: VecDeque<u8> = VecDeque::new();
    let mut sent = 0;
    let deadline = 30.0; // seconds of machine time
    while m.bus.t < deadline * 1e6 && !m.halted {
        m.run_until((m.bus.t + 20_000.0).min(deadline * 1e6));
        if m.bus.console.tks & 0o200 == 0 {
            if queue.is_empty() && sent < lines.len() {
                queue.extend(lines[sent].iter().copied());
                queue.push_back(b'\r');
                sent += 1;
            }
            if let Some(ch) = queue.pop_front() {
                m.bus.console.tkb = ch as u16;
                m.bus.console.tks |= 0o200;
            }
        }
        m.bus.vt11.out.retain(|b| match *b {
            Beam::Dot { t, .. } => t >= m.bus.t - 1_000_000.0 / 30.0,
            Beam::Line { t1, .. } => t1 >= m.bus.t - 1_000_000.0 / 30.0,
        });
    }
    for (i, row) in screen_text(&m.bus.mem).iter().enumerate() {
        if !row.is_empty() {
            println!("{:2}|{}", i, row);
        }
    }
    eprintln!(
        "t={:.2}s pc={:06o} halted={} instr={} sent={}/{} lines faults={:?}",
        m.bus.t / 1e6,
        m.r[7],
        m.halted,
        m.instructions,
        sent,
        lines.len(),
        m.faults
    );
    if m.halted {
        eprintln!(
            "regs: {}",
            m.r.iter().map(|v| format!("{:06o}", v)).collect::<Vec<_>>().join(" ")
        );
    }
    save(out, &m.bus.vt11.out);
}
