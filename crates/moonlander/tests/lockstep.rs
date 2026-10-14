//! The whole program, from power-up, in the original binary and in the
//! translation side by side: at every basic block both must be at the same
//! address at the same machine time, with the same registers, priority and
//! memory. Interrupts are live on both sides; the emulator takes them only at
//! block starts, as the translation does.
mod common;
use common::*;
use gt40::vt11::LightPen;
use kd11b::Machine;
use moonlander::generated::blocks::ALL;
use moonlander::{Escape, Moonlander, Snapshot};
use std::sync::{Arc, Mutex};

struct Run {
    blocks: u64,
    end: Escape,
    /// How many times each block ran.
    visits: std::collections::BTreeMap<u16, u64>,
    host: Vec<u8>,
}

fn name(addr: u16) -> String {
    ALL.iter().find(|b| b.addr == addr).map(|b| b.name.to_string()).unwrap_or(format!("{addr:06o}"))
}

/// Advance the original to its next point of executing an instruction at
/// the start of a block.
fn to_next_block(m: &mut Machine, starts: &[bool]) -> Result<(), String> {
    let mut n = 0u64;
    loop {
        if m.halted {
            return Err(format!("original halted at {:06o}", m.r[7]));
        }
        if !m.waiting && starts[(m.r[7] >> 1) as usize] {
            m.bus.sync();
            if m.bus.pending_interrupt((m.psw >> 5) & 7).is_none() {
                return Ok(());
            }
        }
        m.step();
        n += 1;
        if n > 50_000_000 {
            return Err("original ran on without reaching a block start".into());
        }
    }
}

fn lockstep(seconds: f64, pen: Vec<(f64, Option<LightPen>)>) -> Result<Run, String> {
    let starts = block_starts();
    let emu = Arc::new(Mutex::new(emulator()));
    emu.lock().unwrap().bus.pen_script = pen.clone().into();
    let failure: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mut tr = Moonlander::new(60.0);
    tr.bus.pen_script = pen.into();
    tr.limit = seconds * 1e6;
    let (e2, f2) = (emu.clone(), failure.clone());
    let mut last: Option<Snapshot> = None;
    let visits = Arc::new(Mutex::new(std::collections::BTreeMap::new()));
    let v2 = visits.clone();
    tr.observer = Some(Box::new(move |s: &Snapshot| {
        *v2.lock().unwrap().entry(s.addr).or_insert(0u64) += 1;
        let mut m = e2.lock().unwrap();
        if let Err(e) = to_next_block(&mut m, &starts) {
            *f2.lock().unwrap() = Some(format!("at {} (t={}): {e}", name(s.addr), s.t));
            return false;
        }
        let o = Snapshot {
            addr: m.r[7],
            t: m.bus.t,
            r: [m.r[0], m.r[1], m.r[2], m.r[3], m.r[4], m.r[5], m.r[6]],
            priority: (m.psw >> 5) & 7,
            mem_hash: m.bus.mem_hash,
        };
        if o != *s {
            let prev = last.map(|p| name(p.addr)).unwrap_or_default();
            *f2.lock().unwrap() = Some(format!(
                "after {prev}:\n  original    {} t={} r=[{}] pri={} hash={:016x}\n  translation {} t={} r=[{}] pri={} hash={:016x}",
                name(o.addr), o.t, oct(&o.r), o.priority, o.mem_hash,
                name(s.addr), s.t, oct(&s.r), s.priority, s.mem_hash
            ));
            return false;
        }
        last = Some(*s);
        m.step(); // execute the block's first instruction
        true
    }));
    let end = tr.run();
    if let Some(f) = failure.lock().unwrap().take() {
        let m = emu.lock().unwrap();
        let diffs: Vec<String> = (0..m.bus.mem.len())
            .filter(|&i| m.bus.mem[i] != tr.bus.mem[i])
            .take(12)
            .map(|i| format!("{:06o}: original {:06o} translation {:06o}", i * 2, m.bus.mem[i], tr.bus.mem[i]))
            .collect();
        return Err(format!("{f}\n  memory: {}", if diffs.is_empty() { "same".into() } else { diffs.join("; ") }));
    }
    let visits = visits.lock().unwrap().clone();
    let host = tr.bus.host.as_ref().map(|h| h.sent.clone()).unwrap_or_default();
    Ok(Run { blocks: tr.blocks, end, visits, host })
}

#[test]
fn greeting_flight_and_crash_with_no_hands() {
    // No light pen: the greeting for 30 s, then the ship falls at the
    // starting 75% throttle until it hits the moon, explodes, and the game
    // starts again.
    match lockstep(120.0, vec![]) {
        Ok(r) => {
            assert_eq!(r.end, Escape::Stop);
            // It logged the user off the host before anything else.
            assert_eq!(r.host, vec![0, 0o203, 0o203, b'K', b'/', b'F', 0o15]);
            for b in ["DEAD", "EXPLOD", "EXGEN", "RESTAR", "DRAWM1", "DRAWM2", "TIMER", "DISPLY"] {
                let addr = ALL.iter().find(|x| x.name == b).unwrap().addr;
                assert!(r.visits.contains_key(&addr), "never reached {b}");
            }
            eprintln!("{} blocks in step; {} of {} blocks reached", r.blocks, r.visits.len(), ALL.len());
        }
        Err(e) => panic!("{e}"),
    }
}
