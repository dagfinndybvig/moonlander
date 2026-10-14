//! Routines run alone, interrupts held off, in the original binary and in
//! the translation from the same state: same blocks, registers, time and
//! memory. The inputs are random within what the program can produce, and
//! reach paths the test flights do not: the edges of the screen, angles a
//! long way round, every readout and number width.
mod common;
use common::*;
use moonlander::generated::blocks::ALL;
use moonlander::generated::symbols::*;
use moonlander::routine;
use std::collections::BTreeSet;

fn run(name: &str, cases: usize, seed: u64, setup: impl Fn(&mut Rng) -> (Vec<u16>, Vec<(u16, u16)>)) -> BTreeSet<u16> {
    let r = routine(name);
    let mut rng = Rng(seed);
    let mut seen = BTreeSet::new();
    for n in 0..cases {
        let (regs, mem) = setup(&mut rng);
        let s = Start {
            regs: [regs[0], regs[1], regs[2], regs[3], regs[4], regs[5], STACK - 0o100],
            mem,
            entry: r.addr,
            ret: EXIDLE,
        };
        let a = run_original(&s);
        seen.extend(a.trace.iter().copied());
        if let Err(e) = compare(&s, r.run) {
            panic!("{name} case {n}: {e}");
        }
    }
    seen
}

fn reached(seen: &BTreeSet<u16>, names: &[&str]) {
    for n in names {
        let b = ALL.iter().find(|b| b.name == *n).unwrap();
        assert!(seen.contains(&b.addr), "{n} not reached");
    }
}

fn any(r: &mut Rng) -> Vec<u16> {
    (0..6).map(|_| r.word()).collect()
}

#[test]
fn arithmetic_reaches_every_path() {
    let s = run("MULTWO", 500, 11, |r| (any(r), vec![]));
    reached(&s, &["ADD16", "ADD15", "NADD15", "MULTCM_15", "MULTWO_1", "MULTWO_2"]);
    let s = run("DIVTWO", 500, 12, |r| (any(r), vec![]));
    reached(&s, &["DVOKA", "DVBADA", "DVOKP", "DVBADP"]);
}

#[test]
fn trig_at_any_angle() {
    let s = run("TRIG", 2000, 13, |r| {
        let angle = (r.next() % 2000) as i16 - 1000;
        (any(r), vec![(ANGLE, angle as u16)])
    });
    reached(&s, &["TRIG1_1", "TRIG1_2", "TRIG2_1", "TRIG3_1", "TRIG4_1"]);
}

#[test]
fn drawing_the_ship_at_any_angle() {
    // DRAW takes the sine and cosine from SINANG and COSANG; any pair will
    // do for comparing, including ones no angle has.
    run("DRAW", 300, 14, |r| {
        let mut regs = any(r);
        regs[0] = if r.next() % 2 == 0 { DESIGN } else { FLAMDO };
        regs[1] = SHIPB1;
        let lowy = (r.next() % 80) as i16 - 40;
        (regs, vec![(SINANG, r.word() >> 1), (COSANG, r.word() >> 1), (LOWY, lowy as u16)])
    });
}

#[test]
fn the_engine_at_any_throttle_and_fuel() {
    run("ROCKET", 2000, 15, |r| {
        let mut regs = any(r);
        regs[4] = r.word(); // TEMP: whatever DRAW left
        let fuel = if r.next() % 4 == 0 { 0 } else { (r.next() % 30001) as u16 };
        let lpsw = if r.next() % 2 == 0 { DISTOP } else { 0 };
        (regs, vec![(FUEL, fuel), (PERTRS, (r.next() % 101) as u16), (CLOCK, 1 + (r.next() % 12) as u16), (LPSW, lpsw)])
    });
}

#[test]
fn motion_in_any_state() {
    run("WHERE", 2000, 16, |r| {
        let v = |r: &mut Rng, m: u64| ((r.next() % (2 * m + 1)) as i64 - m as i64) as u16;
        (
            any(r),
            vec![
                (HORACC, v(r, 20000)), (VERACC, v(r, 20000)), (HORVEL, v(r, 12000)), (VERVEL, v(r, 12000)),
                (HORREM, (r.next() % 600) as u16), (VERREM, (r.next() % 600) as u16), (CLOCK, 1 + (r.next() % 12) as u16),
                (HORDIS, r.word()), (VERDIS, r.word()),
            ],
        )
    });
}

#[test]
fn the_flame_at_any_throttle() {
    run("FLAME", 500, 17, |r| {
        let mut regs = any(r);
        regs[0] = FLAMIN;
        (regs, vec![(PERTRS, (r.next() % 101) as u16), (FRAND, r.word()), (FSHIFT, r.word()), (FLINE, r.word()), (FINT, r.word())])
    });
}

#[test]
fn readouts_of_every_kind() {
    let items = [ITEME1, ITEME2, ITEME3, ITEME4, ITEME5, ITEME6, ITEME7, ITEME8, ITEME9, ITEMET, ITEMEE, ITEMES];
    let s = run("DIAL", 1500, 18, |r| {
        let mut mem = vec![];
        for k in 0..4 {
            mem.push((DIALTB + 2 * k, items[(r.next() % 12) as usize]));
        }
        for a in [RADARY, VERDIS, HORDIS, FUEL, WEIGHT, THRUST, ANGLE, VERVEL, HORVEL, VERACC, HORACC, TIME] {
            mem.push((a, r.word()));
        }
        mem.push((PERCNT, (r.next() % 101) as u16));
        mem.push((OLDPER, if r.next() % 2 == 0 { 0xFFFF } else { 50 }));
        (any(r), mem)
    });
    reached(&s, &["ASCIIG", "ASCIIM_1", "ASCIID", "ASCDO_2", "ASCDO1", "SASCII_1", "SASCIL_1", "DIALLP_2", "DIALD1_1"]);
}

#[test]
fn the_feature_table() {
    run("PUTFET", 500, 19, |r| {
        let i = (r.next() % 900) as u16;
        let v = (r.next() % 8) as u16;
        // PUTFET takes its index and value on the stack, above the return.
        (any(r), vec![(STACK - 0o100, i), (STACK - 0o76, v)])
    });
}

#[test]
fn the_close_up_anywhere_on_the_moon() {
    // Including over the deepest craters (594, 615) and the mountains past
    // index 300, where the ground leaves the bottom or the top of the screen.
    // DRAWI1_1 cannot be reached: DRAWIC's first test, CMP TEMP,#1024 with
    // BLO, is unsigned, so a point below the screen counts as above it and
    // is drawn at the top.
    let s = run("DRAWM2", 120, 20, |r| {
        let left = [300u16, 310, 420, 585, 590, 594, 605, 690, 700][(r.next() % 9) as usize] + (r.next() % 4) as u16;
        (any(r), vec![(LEFTED, left), (DFUDGE, ((r.next() % 7) as i16 - 3) as u16)])
    });
    reached(&s, &["DRAWM2_3", "DRW2L5_1", "DRAWIC_1", "MACDON_1"]);
}

#[test]
fn the_far_view() {
    run("DRAWM1", 10, 21, |r| (any(r), vec![]));
}

#[test]
fn craters() {
    run("ALTER", 300, 22, |r| {
        let mut regs = any(r);
        regs[0] = [32, 640][(r.next() % 2) as usize];
        (regs, vec![(BIGXCT, 20 + (r.next() % 860) as u16)])
    });
}
