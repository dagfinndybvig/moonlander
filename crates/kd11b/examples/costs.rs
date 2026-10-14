//! Instruction lengths and times for the translation's block table.
//! Reads lines "ADDR W0 [W1 [W2]]" (octal) on stdin; writes
//! "ADDR LENGTH INSTRUCTION EXTRA" with the times in microseconds, printed
//! so they read back as exactly the same f64.
use kd11b::cost::{cost, length};
use kd11b::timing::Timing;
use std::io::{BufRead, Write};

fn main() {
    let timing = Timing::provisional();
    let out = std::io::stdout();
    let mut out = out.lock();
    writeln!(out, "# timing: {}", timing.source).unwrap();
    writeln!(out, "# interrupt: {:?}", timing.interrupt()).unwrap();
    // TST @#a on a missing device: fetch, extension word, failed read.
    let f = kd11b::cost::faulted(&timing, 3);
    writeln!(out, "# fault3: {:?} {:?}", f.instruction, f.extra).unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let v: Vec<u16> = line.split_whitespace().map(|s| u16::from_str_radix(s, 8).unwrap()).collect();
        if v.is_empty() {
            continue;
        }
        let (addr, words) = (v[0], &v[1..]);
        let n = length(words[0]);
        assert_eq!(n as usize, words.len(), "length disagrees with the listing at {addr:06o}");
        let c = cost(&timing, words);
        writeln!(out, "{addr:06o} {n} {:?} {:?}", c.instruction, c.extra).unwrap();
    }
}
