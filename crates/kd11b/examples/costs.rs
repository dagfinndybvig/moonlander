//! Instruction lengths and times for the translation's block table.
//! Reads lines "ADDR W0 [W1 [W2]]" (octal) on stdin; writes one line per
//! instruction, the times in microseconds printed so they read back as
//! exactly the same f64:
//!
//!     ADDR LENGTH T TIME
//!     ADDR LENGTH B NOT_TAKEN TAKEN               (conditional branch)
//!     ADDR LENGTH Y SRC DST T00 T10 T01 T11       (byte operand in memory)
//!
//! where SRC is 1 if the source is in memory, DST is N, R, W or M (none,
//! read, written, read then written), and Tsd the time with an odd source
//! (s) and an odd destination (d).
use kd11b::cost::{length, step, Access, Step};
use kd11b::timing::Timing;
use std::io::{BufRead, Write};

fn main() {
    let timing = Timing::dec();
    let out = std::io::stdout();
    let mut out = out.lock();
    writeln!(out, "# timing: {}", timing.source).unwrap();
    writeln!(out, "# interrupt: {:?}", timing.interrupt()).unwrap();
    // TST @#a on a missing device: its own time, then the trap.
    let (own, trap) = timing.bus_error(Some(0o005737));
    writeln!(out, "# fault3: {own:?} {trap:?}").unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let v: Vec<u16> = line.split_whitespace().map(|s| u16::from_str_radix(s, 8).unwrap()).collect();
        if v.is_empty() {
            continue;
        }
        let (addr, words) = (v[0], &v[1..]);
        let n = length(words[0]);
        assert_eq!(n as usize, words.len(), "length disagrees with the listing at {addr:06o}");
        match step(&timing, words[0]) {
            Step::Time(t) => writeln!(out, "{addr:06o} {n} T {t:?}"),
            Step::Branch { not_taken, taken } => writeln!(out, "{addr:06o} {n} B {not_taken:?} {taken:?}"),
            Step::Byte { times, src, dst } => {
                let d = match dst {
                    Access::None => 'N',
                    Access::Read => 'R',
                    Access::Write => 'W',
                    Access::ReadWrite => 'M',
                };
                let t = times.map(|t| format!("{t:?}")).join(" ");
                writeln!(out, "{addr:06o} {n} Y {} {d} {t}", src as u8)
            }
        }
        .unwrap();
    }
}
