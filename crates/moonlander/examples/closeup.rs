//! Draw the close-up moon list for a left edge, alone, and save it.
//! Usage: closeup LEFTED OUT.png
use gt40::raster::Raster;
use gt40::vt11::Vt11;
use moonlander::generated::symbols::*;
use moonlander::Moonlander;
use std::io::BufWriter;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let left: u16 = a[1].parse().unwrap();
    let mut m = Moonlander::new(60.0);
    m.bus.mem[(LEFTED >> 1) as usize] = left;
    m.sp = STACK;
    m.psw = 0o340;
    m.push(EXIDLE);
    (moonlander::routine("DRAWM2").run)(&mut m).unwrap();
    let mut v = Vt11::new(60.0);
    v.write_dpc(MOONST, 0.0);
    v.run_until(&m.bus.mem[..], 100_000.0);
    let mut r = Raster::new();
    for b in &v.out {
        r.draw(b);
    }
    let f = std::fs::File::create(&a[2]).unwrap();
    let mut e = png::Encoder::new(BufWriter::new(f), r.w as u32, r.h as u32);
    e.set_color(png::ColorType::Rgb);
    e.write_header().unwrap().write_image_data(&r.rgb()).unwrap();
    println!("{} beams", v.out.len());
}
