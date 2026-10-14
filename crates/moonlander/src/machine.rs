//! What the translated program runs on: a GT40 without its processor
//! (`gt40::terminal::Terminal`), plus the processor state the original
//! program depends on: its six general registers, its stack pointer, its
//! priority, and the time its instructions take.
//!
//! The translation is written in basic blocks of the original program. At
//! the start of each block it calls [`Moonlander::at`] with that block (from
//! `generated::blocks`), which
//!
//! - charges the time of the block before it, instruction by instruction,
//!   in the order and with the values the processor model uses, so machine
//!   time stays equal to the emulator's to the last bit;
//! - brings the display and clock up to that time and takes any interrupt
//!   that is due, by running the translated interrupt routine;
//! - reports the block to an observer, if one is attached, and gives the
//!   host a chance to pace or stop the run.
//!
//! The emulator in block-synchronous mode does the same things at the same
//! addresses, which is what makes the two comparable word for word.

use crate::generated::blocks;
use gt40::terminal::{Terminal, PSW_ADDR};

/// A basic block of the original program.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    /// Address of its first instruction.
    pub addr: u16,
    /// Address after its last instruction.
    pub end: u16,
    /// Number of instructions.
    pub n: u8,
    pub name: &'static str,
    /// Times to add, in order: each instruction's time, and after RESET the
    /// bus reset time.
    pub cost: &'static [f64],
}

/// Ways control leaves the translated routines other than by returning.
#[derive(Clone, Debug, PartialEq)]
pub enum Escape {
    /// `JMP RESTART`: abandon everything and start the game again.
    Restart,
    /// The host stopped the run (a time limit, an observer, a closed window).
    Stop,
    /// The original would have taken a bus error at this address. Every trap
    /// vector in this build leads to a HALT.
    BusError(u16),
    /// The original would have halted at this address.
    Halt(u16),
    /// Control went to an address the translation does not cover. In the
    /// original this would be a jump into the weeds.
    Wild(u16),
}

pub type Flow<T = ()> = Result<T, Escape>;

/// The processor state at the start of a block, as seen by an observer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub addr: u16,
    pub t: f64,
    /// R0-R5, SP.
    pub r: [u16; 7],
    /// Priority bits of the PSW.
    pub priority: u16,
    pub mem_hash: u64,
}

pub struct Moonlander {
    pub bus: Terminal,
    // The general registers, under the names Burness gave them.
    pub in1: u16,
    pub in2: u16,
    pub ret1: u16,
    pub ret2: u16,
    pub temp: u16,
    pub temp2: u16,
    pub sp: u16,
    /// Processor status word. Only the priority (bits 7-5) is meaningful:
    /// the translation tests results directly and keeps no condition codes.
    pub psw: u16,
    /// Start of the block being executed: the PC an interrupt would stack.
    pub pc: u16,
    /// End of the block being executed: where a WAIT in it resumes.
    block_end: u16,
    /// Time of the current block, charged when the next one starts.
    pending: &'static [f64],
    /// A bus error not yet acted on (the block in which it happened runs to
    /// its end; the next block start stops the run).
    fault: Option<u16>,
    /// Stop when machine time reaches this.
    pub limit: f64,
    /// Called at every block start; return false to stop.
    pub observer: Option<Box<dyn FnMut(&Snapshot) -> bool + Send>>,
    /// Called at least once per millisecond of machine time, and at every
    /// WAIT, with the terminal: the host paces the run against the wall
    /// clock, moves the light pen, collects the picture. Return false to stop.
    pub pacer: Option<Box<dyn FnMut(&mut Terminal) -> bool + Send>>,
    next_pace: f64,
    /// Blocks executed.
    pub blocks: u64,
}

impl Moonlander {
    /// A GT40 with 8K words of core, as the Mare Assabet build ran on,
    /// loaded with the program's memory image.
    pub fn new(line_hz: f64) -> Moonlander {
        let mut bus = Terminal::new(0o20000, line_hz);
        let img = gt40::lda::parse(crate::IMAGE).expect("memory image");
        bus.load(&img);
        Moonlander {
            bus,
            in1: 0,
            in2: 0,
            ret1: 0,
            ret2: 0,
            temp: 0,
            temp2: 0,
            sp: 0,
            psw: 0,
            pc: img.start.unwrap_or(0),
            block_end: 0,
            pending: &[],
            fault: None,
            limit: f64::INFINITY,
            observer: None,
            pacer: None,
            next_pace: 0.0,
            blocks: 0,
        }
    }

    /// Registers R0-R5 and SP, in PDP-11 order.
    pub fn regs(&self) -> [u16; 7] {
        [self.in1, self.in2, self.ret1, self.ret2, self.temp, self.temp2, self.sp]
    }

    pub fn set_regs(&mut self, r: [u16; 7]) {
        [self.in1, self.in2, self.ret1, self.ret2, self.temp, self.temp2, self.sp] = r;
    }

    fn priority(&self) -> u16 {
        (self.psw >> 5) & 7
    }

    // ---- memory --------------------------------------------------------------

    /// Read a word (core, a device register, or the PSW).
    pub fn w(&mut self, a: u16) -> u16 {
        if a == PSW_ADDR {
            return self.psw;
        }
        match self.bus.read_word(a) {
            Ok(v) => v,
            Err(f) => {
                self.fault.get_or_insert(f.0);
                0
            }
        }
    }

    /// Write a word.
    pub fn set(&mut self, a: u16, v: u16) {
        if self.fault.is_some() {
            return;
        }
        if a == PSW_ADDR {
            self.psw = v & 0o357;
            return;
        }
        if let Err(f) = self.bus.write_word(a, v) {
            self.fault = Some(f.0);
        }
    }

    /// Read a byte.
    pub fn b(&mut self, a: u16) -> u8 {
        let w = self.w(a & !1);
        if a & 1 != 0 {
            (w >> 8) as u8
        } else {
            w as u8
        }
    }

    /// Read a byte, sign-extended, as MOVB into a register does.
    pub fn bs(&mut self, a: u16) -> u16 {
        self.b(a) as i8 as i16 as u16
    }

    /// Write a byte.
    pub fn setb(&mut self, a: u16, v: u8) {
        if self.fault.is_some() {
            return;
        }
        if let Err(f) = self.bus.write_byte(a, v) {
            self.fault = Some(f.0);
        }
    }

    pub fn push(&mut self, v: u16) {
        self.sp = self.sp.wrapping_sub(2);
        let sp = self.sp;
        self.set(sp, v);
    }

    pub fn pop(&mut self) -> u16 {
        let sp = self.sp;
        let v = self.w(sp);
        self.sp = sp.wrapping_add(2);
        v
    }

    // ---- time, interrupts and blocks -------------------------------------------

    /// Charge the time of the block in progress, as if it had just ended.
    /// For hosts and tests that stop the translation between blocks.
    pub fn finish(&mut self) {
        self.charge();
    }

    fn charge(&mut self) {
        for &c in self.pending {
            self.bus.t += c;
        }
        self.pending = &[];
    }

    fn pace(&mut self) -> Flow {
        if self.bus.t >= self.limit {
            return Err(Escape::Stop);
        }
        if self.bus.t >= self.next_pace {
            self.next_pace = self.bus.t + 1000.0;
            if let Some(p) = &mut self.pacer {
                if !p(&mut self.bus) {
                    return Err(Escape::Stop);
                }
            }
        }
        Ok(())
    }

    /// Start of block `b`: charge the previous block, take interrupts that
    /// are due, report, and go on.
    pub fn at(&mut self, b: Block) -> Flow {
        self.charge();
        if let Some(a) = self.fault {
            return Err(Escape::BusError(a));
        }
        self.pc = b.addr;
        self.take_interrupts()?;
        self.pc = b.addr;
        self.blocks += 1;
        if let Some(o) = &mut self.observer {
            let s = Snapshot {
                addr: b.addr,
                t: self.bus.t,
                r: [self.in1, self.in2, self.ret1, self.ret2, self.temp, self.temp2, self.sp],
                priority: (self.psw >> 5) & 7,
                mem_hash: self.bus.mem_hash,
            };
            if !o(&s) {
                return Err(Escape::Stop);
            }
        }
        self.pace()?;
        self.pending = b.cost;
        self.block_end = b.end;
        Ok(())
    }

    fn take_interrupts(&mut self) -> Flow {
        loop {
            self.bus.sync();
            match self.bus.pending_interrupt(self.priority()) {
                Some((_, vector)) => self.interrupt(vector)?,
                None => return Ok(()),
            }
        }
    }

    /// Take the interrupt at `vector`: stack the PSW and the PC, run the
    /// routine the vector names, and return from it.
    fn interrupt(&mut self, vector: u16) -> Flow {
        let interrupted = self.pc;
        self.bus.acknowledge(vector);
        // As the emulator does in block-synchronous mode, the stacked PSW
        // carries no condition codes.
        let psw = self.psw & !0o17;
        self.push(psw);
        self.push(interrupted);
        let handler = self.w(vector);
        self.psw = self.w(vector + 2) & 0o357;
        self.bus.t += blocks::INTERRUPT;
        self.dispatch(handler)?;
        // RTI, whose time is in the handler's last block.
        self.charge();
        let pc = self.pop();
        let psw = self.pop();
        self.psw = psw & 0o357;
        if pc != interrupted {
            return Err(Escape::Wild(pc));
        }
        self.pc = interrupted;
        Ok(())
    }

    /// WAIT: sleep until an interrupt, take it, and go on. The WAIT
    /// instruction's own time is in the current block.
    pub fn wait(&mut self) -> Flow {
        self.charge();
        // An interrupt ends the WAIT; the PC stacked is the instruction
        // after it, which starts the next block.
        self.pc = self.block_end;
        loop {
            self.bus.sync();
            if let Some((_, vector)) = self.bus.pending_interrupt(self.priority()) {
                return self.interrupt(vector);
            }
            self.bus.sleep();
            self.pace()?;
        }
    }

    /// RESET: Unibus INIT. Its time is in the current block.
    pub fn reset(&mut self) {
        self.bus.unibus_init();
    }

    /// JSR PC: push the return address, run the routine, and take its RTS,
    /// whose time is in the routine's last block. Returns where the RTS
    /// went, which a routine can change by editing its return address.
    pub fn jsr(&mut self, ret: Block, routine: fn(&mut Self) -> Flow) -> Flow<u16> {
        self.jsr_at(ret.addr, routine)
    }

    /// JSR PC with the return address given as an address: for calls
    /// followed by inline data, where the address pushed is the data's.
    pub fn jsr_at(&mut self, ret: u16, routine: fn(&mut Self) -> Flow) -> Flow<u16> {
        self.push(ret);
        routine(self)?;
        Ok(self.pop())
    }

    /// A subroutine call that returns to the instruction after it, which
    /// starts block `ret`.
    pub fn call(&mut self, ret: Block, routine: fn(&mut Self) -> Flow) -> Flow {
        let to = self.jsr(ret, routine)?;
        if to != ret.addr {
            return Err(Escape::Wild(to));
        }
        self.at(ret)
    }

    /// Read a word where the original expects a bus error to be possible:
    /// None if there is nothing at that address.
    pub fn probe(&mut self, a: u16) -> Option<u16> {
        self.bus.read_word(a).ok()
    }

    /// The first instruction of the current block took a bus error after
    /// costing `cost`, with the PC at `stacked_pc`: the rest of the block
    /// does not run; the PSW and PC are stacked and the vector at 4 taken.
    /// Returns the address the vector holds.
    pub fn bus_error_trap(&mut self, cost: &'static [f64], stacked_pc: u16) -> u16 {
        self.pending = &[];
        for &c in cost {
            self.bus.t += c;
        }
        let psw = self.psw & !0o17;
        self.push(psw);
        self.push(stacked_pc);
        let handler = self.w(0o4);
        self.psw = self.w(0o6) & 0o357;
        handler
    }
}
