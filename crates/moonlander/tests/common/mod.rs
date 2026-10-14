//! Running the original binary and the translation side by side.
#![allow(dead_code)]

use kd11b::Machine;
use moonlander::generated::blocks::ALL;
use moonlander::{Flow, Moonlander};
use std::sync::{Arc, Mutex};

/// One flag per word address: does a basic block start there?
pub fn block_starts() -> Vec<bool> {
    let mut v = vec![false; 0o100000];
    for b in ALL {
        v[(b.addr >> 1) as usize] = true;
    }
    v
}

/// The original binary on the emulated GT40, in block-synchronous mode.
pub fn emulator() -> Machine {
    let mut m = Machine::new(0o20000, 60.0);
    m.load(&gt40::lda::parse(moonlander::IMAGE).unwrap());
    m.block_starts = Some(block_starts());
    m
}

/// A tiny deterministic generator for test inputs.
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// A 16-bit value, biased towards the edges where arithmetic goes wrong.
    pub fn word(&mut self) -> u16 {
        match self.next() % 8 {
            0 => [0, 1, 2, 0o77777, 0o100000, 0o177777, 0o177776, 0o100001][(self.next() % 8) as usize],
            1 => 1 << (self.next() % 16),
            2 => (self.next() % 256) as u16,
            _ => self.next() as u16,
        }
    }
}

/// State both sides start a routine from.
#[derive(Clone)]
pub struct Start {
    /// R0-R5, SP (before the return address is pushed).
    pub regs: [u16; 7],
    /// Words to place in memory first.
    pub mem: Vec<(u16, u16)>,
    /// Where the routine is entered in the original.
    pub entry: u16,
    /// The return address its caller pushes.
    pub ret: u16,
}

/// What a routine left behind.
#[derive(Debug, PartialEq)]
pub struct End {
    pub regs: [u16; 7],
    pub t: f64,
    pub trace: Vec<u16>,
    pub mem: Vec<u16>,
}

/// Run a routine in the original binary, interrupts held off, until it
/// returns to `ret`.
pub fn run_original(s: &Start) -> End {
    let mut m = emulator();
    for &(a, v) in &s.mem {
        m.bus.mem[(a >> 1) as usize] = v;
    }
    m.r[..6].copy_from_slice(&s.regs[..6]);
    m.r[6] = s.regs[6];
    m.psw = 0o340;
    m.push(s.ret).unwrap();
    m.r[7] = s.entry;
    m.bus.rehash();
    let starts = block_starts();
    let mut trace = Vec::new();
    let mut steps = 0;
    // Returned: at the return address, with the stack back at or above where
    // the caller had it (PUTFET also removes its two arguments).
    while !(m.r[7] == s.ret && m.r[6] >= s.regs[6]) {
        if starts[(m.r[7] >> 1) as usize] {
            trace.push(m.r[7]);
        }
        m.step();
        steps += 1;
        assert!(!m.halted && steps < 10_000_000, "original did not return (pc {:06o})", m.r[7]);
    }
    let mut regs = [0; 7];
    regs.copy_from_slice(&m.r[..7]);
    End { regs, t: m.bus.t, trace, mem: m.bus.mem.clone() }
}

/// Run the translated routine from the same state.
pub fn run_translation(s: &Start, routine: fn(&mut Moonlander) -> Flow) -> End {
    let mut tr = Moonlander::new(60.0);
    for &(a, v) in &s.mem {
        tr.bus.mem[(a >> 1) as usize] = v;
    }
    tr.bus.rehash();
    tr.set_regs(s.regs);
    tr.psw = 0o340;
    tr.push(s.ret);
    let trace = Arc::new(Mutex::new(Vec::new()));
    let t2 = trace.clone();
    tr.observer = Some(Box::new(move |snap| {
        t2.lock().unwrap().push(snap.addr);
        true
    }));
    routine(&mut tr).expect("translation escaped");
    tr.finish();
    let back = tr.pop();
    assert_eq!(back, s.ret, "translation returned to {back:06o}");
    let trace = trace.lock().unwrap().clone();
    End { regs: tr.regs(), t: tr.bus.t, trace, mem: tr.bus.mem.clone() }
}

/// Octal, the way the listing shows it.
pub fn oct(v: &[u16]) -> String {
    v.iter().map(|x| format!("{x:06o}")).collect::<Vec<_>>().join(" ")
}

/// Compare a routine on both sides; describe the first difference.
pub fn compare(s: &Start, routine: fn(&mut Moonlander) -> Flow) -> Result<(), String> {
    let a = run_original(s);
    let b = run_translation(s, routine);
    if a.trace != b.trace {
        let i = a.trace.iter().zip(&b.trace).take_while(|(x, y)| x == y).count();
        return Err(format!(
            "blocks differ at step {i}: original [{}], translation [{}]",
            oct(&a.trace[i..(i + 4).min(a.trace.len())]),
            oct(&b.trace[i..(i + 4).min(b.trace.len())])
        ));
    }
    if a.regs != b.regs {
        return Err(format!("registers: original [{}], translation [{}]", oct(&a.regs), oct(&b.regs)));
    }
    if a.t != b.t {
        return Err(format!("time: original {}, translation {}", a.t, b.t));
    }
    if let Some(i) = (0..a.mem.len()).find(|&i| a.mem[i] != b.mem[i]) {
        return Err(format!("memory at {:06o}: original {:06o}, translation {:06o}", i * 2, a.mem[i], b.mem[i]));
    }
    Ok(())
}
