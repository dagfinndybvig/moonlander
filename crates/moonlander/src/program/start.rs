//! Starting and restarting: START (from power-up; it runs once, from the
//! buffers it is assembled over), RESTAR, the main loop IDLE and EIDLE,
//! the power fail routine, and the trap catchers.

use super::dec_bgt;
use crate::generated::blocks as blk;
use crate::generated::symbols::*;
use crate::machine::{Escape, Flow, Moonlander};

impl Moonlander {
    /// Run the program from its start address until something stops it:
    /// the host, a halt, or control leaving the program. Restarts (from
    /// DELAY, after every landing or crash) begin again at RESTAR.
    pub fn run(&mut self) -> Escape {
        let mut r = self.start();
        loop {
            match r {
                Err(Escape::Restart) => r = self.restar(),
                Err(e) => return e,
                Ok(()) => return Escape::Wild(self.pc),
            }
        }
    }

    /// START: reset the bus, log the user off the host if there is a line
    /// to one, put up the greeting, and after 30 seconds start the game.
    pub(crate) fn start(&mut self) -> Flow {
        self.at(blk::START)?; // DO A FEW HEAVY RESETS
        self.reset();
        for b in [blk::START_1, blk::START_2, blk::START_3] {
            self.at(b)?;
            self.reset();
        }
        self.at(blk::START_4)?;
        self.reset();
        self.sp = STACK;
        self.at(blk::START_5)?;
        self.set(STATUS, 0);
        self.set(0o4, NOLINE); // A BUS ERROR GOES TO NOLINE,
        self.set(0o6, 0); // AT PRIORITY 0. IT IS NOT PUT BACK AFTERWARDS.
        self.at(blk::START_6)?;
        if self.probe(XCSR).is_some() {
            // IS THERE A LINE? THEN ^C ^C K/F <CR>: A FAST LOGOUT ON A PDP-10.
            self.at(blk::START_7)?;
            self.set(XBUF, 0);
            let sends = [
                (blk::START_8, 0o203), // ^C, EIGHTH BIT SET.
                (blk::START_9, 0o203), // ^C AGAIN.
                (blk::START_10, b'K' as u16), // KJOB
                (blk::START_11, b'/' as u16),
                (blk::START_12, b'F' as u16), // /F, FAST.
                (blk::START_13, 0o15),
            ];
            for (b, c) in sends {
                self.call(b, Self::xwait)?;
                self.set(XBUF, c);
            }
            self.call(blk::START_14, Self::xwait)?; // BR LINEOK
        } else {
            // NO LINE: TOUCHING IT TRAPS, AND THE TRAP SKIPS ALL THIS.
            let handler = self.bus_error_trap(blk::FAULT3, blk::START_6.addr + 4);
            if handler != NOLINE {
                return Err(Escape::Wild(handler));
            }
            self.at(blk::NOLINE)?;
            self.sp = self.sp.wrapping_add(4); // CMP (SP)+,(SP)+: DROP THE TRAP.
        }
        self.at(blk::LINEOK)?;
        self.set(LKS, 0o100); // ENABLE LINE CLOCK
        self.at(blk::LINEOK_1)?;
        self.set(DPC, STARTM); // THE GREETING.
        self.call_delay(blk::LINEOK_1, None) // WAIT 30 SECS AND GET STARTED
    }

    /// XWAIT: wait for the line to the host to be ready.
    fn xwait(&mut self) -> Flow {
        loop {
            self.at(blk::XWAIT)?;
            if self.b(XCSR) & 0x80 != 0 {
                break;
            }
        }
        self.at(blk::XWAIT_1) // RTS
    }

    /// RESTAR: clear the moon list and the buffers to display stops, and
    /// set every variable from the table INIT. What is not in INIT keeps its
    /// value from the last game: the terrain and its craters, the features,
    /// the readouts chosen, and the flags and instructions kept in code.
    pub(crate) fn restar(&mut self) -> Flow {
        self.at(blk::RESTAR)?;
        self.reset(); // CLEAR THE I/O PAGE FIRST.
        self.in1 = MOONST; // SET THE MOON AREA TO ALL DISTOP'S FOLLOWED BY A ZERO.
        self.in2 = (MOONEN.wrapping_sub(self.in1) as i16 >> 2) as u16; // DOUBLE WORDS.
        self.fill_stops(blk::RESTL1)?;
        self.at(blk::RESTL1_1)?;
        self.in1 = SHIPB1; // THE SAME FOR THE SHIP, FLAME AND EXPLOSION BUFFERS.
        self.in2 = (STACK.wrapping_sub(self.in1) as i16 >> 2) as u16;
        self.fill_stops(blk::RESTL2)?;
        self.at(blk::RESTL2_1)?;
        self.set(STATUS, 0); // CLEAR CPU STUFF.
        self.sp = STACK;
        self.in1 = INIT; // INITIALIZE THE SYSTEM NOW.
        loop {
            self.at(blk::STARTL)?;
            self.in2 = self.w(self.in1); // PICK UP NEXT COMMAND.
            self.in1 = self.in1.wrapping_add(2);
            if self.in2 == 0 {
                break; // ZERO. ALL DONE.
            }
            self.at(blk::STARTL_1)?;
            let v = self.w(self.in1); // DO COMMAND NOW.
            self.in1 = self.in1.wrapping_add(2);
            self.set(self.in2, v);
        }
        self.idle()
    }

    /// RESTL1 and RESTL2: IN2 pairs of DISTOP, 0 from IN1.
    fn fill_stops(&mut self, b: crate::machine::Block) -> Flow {
        loop {
            self.at(b)?;
            self.set(self.in1, DISTOP);
            self.in1 = self.in1.wrapping_add(2);
            self.set(self.in1, 0);
            self.in1 = self.in1.wrapping_add(2);
            let more = dec_bgt(self.in2);
            self.in2 = self.in2.wrapping_sub(1);
            if !more {
                return Ok(());
            }
        }
    }

    /// IDLE: THE MAIN WAIT POINT.
    pub(crate) fn idle(&mut self) -> Flow {
        loop {
            self.at(blk::IDLE)?;
            self.call(blk::IDLE_1, Self::eidle)?; // WAIT FOR AN EVENT.
        }
    }

    /// EIDLE: wait for an interrupt; if the clock has ticked since the last
    /// step, run a step with CLOCK set to the number of ticks.
    pub(crate) fn eidle(&mut self) -> Flow {
        self.at(blk::EIDLE)?;
        self.wait()?;
        self.at(blk::EIDLE_1)?;
        self.in1 = self.w(TICKS);
        if self.in1 != 0 {
            self.at(blk::EIDLE_2)?;
            self.set(TICKS, 0); // RESET THE LITTLE TICK COUNTER.
            self.set(CLOCK, self.in1); // THE # OF TICKS FOR CALCULATIONS.
            self.call(blk::EXIDLE, Self::showsp)?; // DO CALCULATIONS.
            return Ok(());
        }
        self.at(blk::EXIDLE) // RTS
    }

    /// POWERF: "POWER FAIL ROUTINES HERE." Restart the display and the clock
    /// and go back to IDLE with a fresh stack. Reached only through the
    /// power fail vector, which nothing here raises.
    pub(crate) fn powerf(&mut self) -> Flow {
        self.at(blk::POWERF)?;
        self.set(DSTACK, STACKD); // SET UP ALL THE STACKS NOW.
        self.set(STACKD, DTOP);
        self.set(DNUM, 0);
        self.at(blk::POWERF_1)?;
        self.set(DPC, ITEME1); // THE DISPLAY IS GOING NOW.
        self.at(blk::POWERF_2)?;
        self.set(LKS, 0o100); // SO IS THE CLOCK.
        self.sp = STACK; // NOW SO IS THE PROGRAM.
        self.at(blk::POWERF_3)?;
        self.set(STATUS, 0);
        self.idle()
    }
}
