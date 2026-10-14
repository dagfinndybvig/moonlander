//! The multiply and divide routines against the original binary, on inputs
//! chosen to reach every bit pattern's path, and the edges.
mod common;
use common::*;
use moonlander::generated::symbols::*;
use moonlander::routine;

fn check(name: &str, cases: usize, seed: u64, regs: impl Fn(&mut Rng) -> [u16; 6]) {
    let r = routine(name);
    let mut rng = Rng(seed);
    for n in 0..cases {
        let g = regs(&mut rng);
        let s = Start {
            regs: [g[0], g[1], g[2], g[3], g[4], g[5], STACK],
            mem: vec![],
            entry: r.addr,
            ret: EXIDLE,
        };
        if let Err(e) = compare(&s, r.run) {
            panic!("{name} case {n} with R0-R5 [{}]: {e}", oct(&g));
        }
    }
}

#[test]
fn multwo() {
    check("MULTWO", 3000, 1, |r| [r.word(), r.word(), r.word(), r.word(), r.word(), r.word()]);
}

#[test]
fn sgnmul() {
    check("SGNMUL", 2000, 2, |r| [r.word(), r.word(), r.word(), r.word(), r.word(), r.word()]);
}

#[test]
fn trgmul() {
    check("TRGMUL", 2000, 3, |r| [r.word(), r.word(), r.word(), r.word(), r.word(), r.word()]);
}

#[test]
fn divtwo() {
    // Every kind of dividend and divisor, including quotients that overflow.
    check("DIVTWO", 3000, 4, |r| [r.word(), r.word(), r.word(), r.word(), r.word(), r.word()]);
}
