//! Run a GT40 program and report the registers each time the PC reaches a
//! given address: a breakpoint that does not stop the machine.
//! Usage: probe IMAGE.lda LINE_HZ WORDS SECONDS PC [PC...]
//! WORDS and PC are octal. Prints one line per distinct register state,
//! with how many times it was seen and the first time it was seen.
use kd11b::Machine;
use std::collections::BTreeMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let img = gt40::lda::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let hz: f64 = a[2].parse().unwrap();
    let words = usize::from_str_radix(&a[3], 8).unwrap() / 2;
    let until = a[4].parse::<f64>().unwrap() * 1e6;
    let pcs: Vec<u16> = a[5..].iter().map(|s| u16::from_str_radix(s, 8).unwrap()).collect();
    let mut m = Machine::new(words, hz);
    m.load(&img);
    let mut seen: BTreeMap<(u16, [u16; 6]), (u64, f64)> = BTreeMap::new();
    while m.t < until && !m.halted {
        let pc = m.r[7];
        if pcs.contains(&pc) {
            let mut r = [0u16; 6];
            r.copy_from_slice(&m.r[..6]);
            let e = seen.entry((pc, r)).or_insert((0, m.t / 1e6));
            e.0 += 1;
        }
        m.step();
    }
    for ((pc, r), (n, t)) in &seen {
        println!(
            "pc {pc:06o}  r0 {:06o} r1 {:06o} r2 {:06o} r3 {:06o} r4 {:06o} r5 {:06o}  seen {n} times, first at {t:.2}s",
            r[0], r[1], r[2], r[3], r[4], r[5]
        );
    }
}
