//! Boot a GT40 program from an absolute loader image, run it, and save
//! snapshots of the screen.
//! Usage: boot IMAGE.lda LINE_HZ WORDS SECONDS[,SECONDS...] OUT_PREFIX
use gt40::raster::Raster;
use gt40::vt11::Beam;
use kd11b::Machine;
use std::io::BufWriter;

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
    let hz: f64 = a[2].parse().unwrap();
    let words: usize = usize::from_str_radix(&a[3], 8).unwrap() / 2;
    let times: Vec<f64> = a[4].split(',').map(|s| s.parse::<f64>().unwrap()).collect();
    let prefix = &a[5];
    let mut m = Machine::new(words, hz);
    m.load(&img);
    let window = 1_000_000.0 / 30.0; // one 1/30 s slice of beam activity
    for (i, &secs) in times.iter().enumerate() {
        let until = secs * 1e6;
        while m.t < until && !m.halted {
            m.run_until((m.t + 50_000.0).min(until));
            // keep only recent beam history
            let cut = m.t - window;
            m.vt11.out.retain(|b| match *b {
                Beam::Dot { t, .. } => t >= cut,
                Beam::Line { t1, .. } => t1 >= cut,
            });
        }
        let path = format!("{prefix}-{i}.png");
        save(&path, &m.vt11.out);
        let w = |a: u16| m.mem[(a >> 1) as usize] as i16;
        eprintln!(
            "t={:.2}s pc={:06o} halted={} instr={} ticks={} | host<-{:?} | FUEL={} VERDIS={} HORDIS={} VERVEL={} HORVEL={} CLOCK={} -> {path}",
            m.t / 1e6, m.r[7], m.halted, m.instructions, m.clock.ticks,
            m.host.as_ref().map(|h| String::from_utf8_lossy(&h.sent).into_owned()),
            w(0o150), w(0o136), w(0o134), w(0o130), w(0o124), w(0o156)
        );
    }
}
