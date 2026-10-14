//! Run generated CPU test cases (oracle/cputest/gen.py) on the KD11-B model
//! and print final state in the same format as oracle/cputest/run_simh.py.
//! Usage: cpucheck CASES.txt > OUT.txt
use kd11b::Machine;

struct Case {
    n: u32,
    r: Vec<u16>,
    psw: u16,
    mem: Vec<(u16, Vec<u16>)>,
    vec: Vec<(u16, u16)>,
}

fn oct(s: &str) -> u16 {
    u16::from_str_radix(s, 8).unwrap()
}

fn parse(path: &str) -> Vec<Case> {
    let mut out = Vec::new();
    let mut cur: Option<Case> = None;
    for line in std::fs::read_to_string(path).unwrap().lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        match p.first().copied() {
            Some("CASE") => {
                cur = Some(Case { n: p[1].parse().unwrap(), r: vec![], psw: 0, mem: vec![], vec: vec![] })
            }
            Some("R") => cur.as_mut().unwrap().r = p[1..].iter().map(|s| oct(s)).collect(),
            Some("PSW") => cur.as_mut().unwrap().psw = oct(p[1]),
            Some("MEM") => cur.as_mut().unwrap().mem.push((oct(p[1]), p[2..].iter().map(|s| oct(s)).collect())),
            Some("VEC") => {
                cur.as_mut().unwrap().vec = p[1..]
                    .iter()
                    .map(|s| {
                        let (a, b) = s.split_once(':').unwrap();
                        (oct(a), oct(b))
                    })
                    .collect()
            }
            Some("END") => out.push(cur.take().unwrap()),
            _ => {}
        }
    }
    out
}

fn main() {
    let path = std::env::args().nth(1).unwrap();
    for c in parse(&path) {
        let mut m = Machine::new(0o40000 / 2, 60.0); // 8K words, a stock GT40
        // CPU-only configuration: no DMA or interrupt sources besides the
        // clock and console, matching run_simh.py's simh configuration.
        m.bus.has_vt11 = false;
        m.bus.host = None;
        for &(v, h) in &c.vec {
            m.bus.mem[(v >> 1) as usize] = h;
            m.bus.mem[(v >> 1) as usize + 1] = 0;
        }
        for (base, words) in &c.mem {
            for (i, &w) in words.iter().enumerate() {
                m.bus.mem[(*base >> 1) as usize + i] = w;
            }
        }
        m.r.copy_from_slice(&c.r);
        m.psw = c.psw;
        let mut steps = 0;
        let trace = std::env::var("TRACE").ok().and_then(|v| v.parse::<u32>().ok()) == Some(c.n);
        if trace {
            m.bus.access_log = Some(Vec::new());
        }
        while !m.halted && steps < 20_000 {
            let (pc, psw) = (m.r[7], m.psw);
            m.step();
            if trace {
                eprintln!("{pc:06o} {:06o}  psw {psw:03o} -> {:03o}  regs {:?}", m.last_insn, m.psw,
                          m.r.iter().map(|r| format!("{r:o}")).collect::<Vec<_>>());
                for (w, a, v) in m.bus.access_log.as_mut().unwrap().drain(..) {
                    eprintln!("        {} {a:06o} {v:06o}", if w { "W" } else { "R" });
                }
            }
            steps += 1;
        }
        println!("CASE {}", c.n);
        println!("R {}", m.r.iter().map(|r| format!("{r:o}")).collect::<Vec<_>>().join(" "));
        println!("PSW {:o}", m.psw);
        println!("HALT {:o}", m.r[7]);
        println!("FAULTS {}", m.faults.iter().map(|a| format!("{a:o}")).collect::<Vec<_>>().join(" "));
        let dump = |base: u16| {
            (0..0o400u16).map(|i| format!("{:o}", m.bus.mem[(base >> 1) as usize + i as usize])).collect::<Vec<_>>().join(" ")
        };
        println!("MEM 2000 {}", dump(0o2000));
        println!("MEM 3000 {}", dump(0o3000));
    }
}
