//! The interrupt routines: the line clock (TIMER), the display processor's
//! stops (DISPLY, "THE START OF THE DISPLAY SECTION. PLEASE BE MERICFUL.")
//! and bus errors (DBUSE), and the light pen (LIGHT).
//!
//! The VT11 has no display subroutines, so Burness made them: a display
//! list calls another by a stop instruction followed by the address to go
//! to (zero means return). The stop interrupts the processor, and DISPLY
//! keeps the return addresses on a stack of its own in memory, running down
//! from STACKD. Each entry on that stack is a pair: the address of the code
//! to run on return (GOTSUB, or DTOP at the bottom) and the display address
//! to resume at. DTOP steps through the master list DLIST, one entry per
//! thing on the screen; which entry is being drawn (DNUM) is how the light
//! pen routine knows what was hit.

use super::{asl, asr, minus};
use crate::generated::blocks as blk;
use crate::generated::symbols::*;
use crate::machine::{Escape, Flow, Moonlander};

/// Where the display routines' shared code is entered.
enum Display {
    /// DISRT: start the display at IN2, unless it begins with a call or a
    /// return of its own.
    Run,
    /// DSUBRT: return through the display stack: MOV (IN1)+,PC.
    Return,
    /// DTOPOK: the next entry of the master list, at IN2.
    Next,
}

impl Moonlander {
    /// TIMER: THE CLOCK INTERUPT ROUTINE.
    pub(crate) fn timer(&mut self) -> Flow {
        self.at(blk::TIMER)?;
        let v = self.w(LKS) | 0o100; // JUST MAKE SURE CLOCK IS STILL GOING.
        self.set(LKS, v);
        let ticks = self.w(TICKS).wrapping_add(1); // UPDATE CLOCKS.
        self.set(TICKS, ticks);
        let time = self.w(TIME).wrapping_add(1);
        self.set(TIME, time);
        if time & 0o17 == 0 {
            // TIME TO CLEAR ARROWS: every 16 ticks the brightened arrow
            // goes back to normal.
            self.at(blk::TIMER_1)?;
            self.set(DLIST1, 0);
            self.set(DLIST2, 0);
        }
        self.at(blk::TIMERT) // RTI
    }

    /// DISPLY: the display stopped. If the word after the stop is zero it
    /// is a display subroutine return; otherwise a call to that address.
    pub(crate) fn disply(&mut self) -> Flow {
        self.at(blk::DISPLY)?;
        self.push(self.in1);
        self.push(self.in2);
        self.in1 = self.w(DSTACK); // PICK UP OLD STACK.
        let dpc = self.w(DPC); // FIND OUT WHY WE STOPPED.
        self.in2 = self.w(dpc);
        if self.in2 == 0 {
            return self.display(Display::Return); // DISPLAY SUBROUTINE RETURN.
        }
        // SUBROUTINE CALL. PUSH RETURN DPC ON STACK, FUDGED PAST THE
        // POINTER, AND THE RETURN ADDRESS.
        self.at(blk::DISPLY_1)?;
        self.in1 = self.in1.wrapping_sub(2);
        let dpc = self.w(DPC);
        self.set(self.in1, dpc);
        let v = self.w(self.in1).wrapping_add(2);
        self.set(self.in1, v);
        self.in1 = self.in1.wrapping_sub(2);
        self.set(self.in1, GOTSUB);
        self.display(Display::Run)
    }

    /// DISRT, DSUBRT, GOTSUB, DTOP and DTOPOK, which jump among themselves
    /// until one starts the display and returns from the interrupt.
    fn display(&mut self, mut at: Display) -> Flow {
        loop {
            at = match at {
                Display::Run => {
                    self.at(blk::DISRT)?;
                    if self.w(self.in2) != DISTOP {
                        // NOT ANOTHER SUBROUTINE CALL: START THE DISPLAY.
                        self.at(blk::DISRST)?;
                        self.set(DSTACK, self.in1); // SAVE THE STACK NOW.
                        self.at(blk::DISRST_1)?;
                        self.set(DPC, self.in2); // START THE DISPLAY GOING NOW.
                        self.in2 = self.pop(); // RESTORE SAVED REGISTERS.
                        self.in1 = self.pop();
                        return Ok(()); // RTI
                    }
                    // FIGURE OUT WHETHER TO RETURN OR GO DOWN ONE LEVEL.
                    self.at(blk::DISRT_1)?;
                    if self.w(self.in2.wrapping_add(2)) == 0 {
                        Display::Return
                    } else {
                        self.at(blk::DISRT_2)?;
                        self.in1 = self.in1.wrapping_sub(2); // PUSH NEXT DPC ONTO STACK.
                        self.set(self.in1, self.in2);
                        let v = self.w(self.in1).wrapping_add(4); // AND FUDGE IT UP A BIT.
                        self.set(self.in1, v);
                        self.in1 = self.in1.wrapping_sub(2);
                        self.set(self.in1, GOTSUB);
                        self.in2 = self.w(self.in2.wrapping_add(2)); // UPDATE NEW PC NOW.
                        Display::Run
                    }
                }
                Display::Return => {
                    self.at(blk::DSUBRT)?;
                    let to = self.w(self.in1); // GO TO SUBROUTINE NOW.
                    self.in1 = self.in1.wrapping_add(2);
                    match to {
                        GOTSUB => {
                            self.at(blk::GOTSUB)?;
                            self.in2 = self.w(self.in1); // PICK UP OLD DPC
                            self.in1 = self.in1.wrapping_add(2);
                            Display::Run
                        }
                        DTOP => {
                            // GO DOWN THE MASTER LIST.
                            self.at(blk::DTOP)?;
                            let n = self.w(DNUM).wrapping_add(1);
                            self.set(DNUM, n);
                            loop {
                                self.at(blk::DTOP2)?;
                                self.in2 = self.w(DNUM) << 1; // MAKE INTO A GOOD INDEX.
                                self.in2 = self.w(DLIST.wrapping_add(self.in2));
                                if self.in2 != 0 {
                                    break;
                                }
                                self.at(blk::DTOP2_1)?;
                                self.set(DNUM, 0); // AT BOTTOM. LOOP.
                            }
                            Display::Next
                        }
                        other => return Err(Escape::Wild(other)),
                    }
                }
                Display::Next => {
                    self.at(blk::DTOPOK)?;
                    self.set(STACKD, DTOP);
                    self.in1 = STACKD;
                    Display::Run
                }
            }
        }
    }

    /// DBUSE: the display took a bus error. Start it again from the top.
    pub(crate) fn dbuse(&mut self) -> Flow {
        self.at(blk::DBUSE)?;
        self.set(DNUM, 0); // RECYCLE DISPLAY TO THE TOP OF THE LIST.
        self.push(self.in1); // MAKE IT LOOK LIKE A NORMAL INTERUPT.
        self.push(self.in2);
        self.in2 = DITEM1;
        self.display(Display::Next)
    }

    /// LIGHT: "LIGHT PEN HIT ROUTINE." A hit counts only after sixteen in a
    /// row on the same master list entry; then the entry's routine in LPTAB
    /// handles it.
    pub(crate) fn light(&mut self) -> Flow {
        self.at(blk::LIGHT)?;
        self.push(self.in1);
        self.push(self.in2); // SAVE REGISTERS NOW.
        self.in2 = self.w(DNUM); // THE INDEXED NUMBER.
        if self.in2 != self.w(OLDHIT) {
            // NOT SAME. SET UP HIT COUNTER AND THE OLD REGISTER.
            self.at(blk::LIGHT_1)?;
            self.set(HITCNT, 15);
            self.set(OLDHIT, self.in2);
            return self.lpresm();
        }
        self.at(blk::LIGHTG)?;
        let n = self.w(HITCNT).wrapping_sub(1); // HAVE WE BEEN HIT ENOUGH?
        self.set(HITCNT, n);
        if !minus(n) {
            return self.lpresm(); // NO. GET OUT NOW.
        }
        self.at(blk::LIGHTG_1)?;
        self.in2 = asl(self.in2).0; // SHIFT THE INDEX OVER
        let to = self.w(LPTAB.wrapping_add(self.in2)); // AND GO PROCESS INTERUPT.
        match to {
            LPBARH => self.lpbarh(),
            LPIT1 => self.lpit1(),
            LPIT2 => self.lpit2(),
            ARROW => self.arrow(),
            // Hits on the ship, flame, messages, dust and moon. Their lists
            // switch light pen interrupts off, so this does not happen; if it
            // did, the return through the display stack would jump through
            // whatever R0 of the interrupted program points at.
            DSUBRT => self.display(Display::Return),
            other => Err(Escape::Wild(other)),
        }
    }

    /// LPBARH: "THIS ROUTINE IS CALLED WHENEVER THE LIGHT PEN HITS THE
    /// THROTTLE BAR." The pen's height on the bar, smoothed (7/8 old, 1/8
    /// new), sets the throttle between MINTRS and 100 percent.
    fn lpbarh(&mut self) -> Flow {
        self.at(blk::LPBARH)?;
        self.in2 = self.w(YSR); // GET THE Y COORDINATE.
        self.in2 &= 0o1777; // MAKE IT JUST 10 BITS.
        self.in2 = self.in2.wrapping_add(BARADD); // ADD FUDGE FACTOR NOW.
        self.in1 = self.w(LPBARY) << 3; // OLD Y, MULTIPLIED BY 8.
        self.in1 = self.in1.wrapping_sub(self.w(LPBARY)); // WE REALLY ONLY WANTED 7.
        self.in2 = self.in2.wrapping_add(self.in1); // NOW MAKE IT 7*OLD+NEW Y.
        self.in2 = asr(asr(asr(self.in2))); // NOW AVERAGE IT OUT.
        self.set(LPBARY, self.in2);
        self.in2 = self.in2.wrapping_sub(BARFDG); // SUBTRACT BASE Y NOW.
        self.in2 = asr(self.in2); // DIVIDE BY TWO.
        // CMP IN2,#MINTRS / BPL: the sign of the difference, not a signed
        // comparison.
        if minus(self.in2.wrapping_sub(MINTRS)) {
            self.at(blk::LPBARH_1)?;
            self.in2 = MINTRS; // SET TO THE MINIMUM THRUST.
        }
        self.at(blk::LPBARP)?;
        if 100 < self.in2 as i16 {
            self.at(blk::LPBARP_1)?;
            self.in2 = 100; // IF GREATER THEN 100, MAKE IT ONLY 100.
        }
        self.at(blk::LPBARQ)?;
        self.set(PERCNT, self.in2); // SAVE AWAY THE VALUE NOW.
        self.lpresm()
    }

    /// LPRESM: resume the display where the hit stopped it, and return.
    fn lpresm(&mut self) -> Flow {
        self.at(blk::LPRESM)?;
        let v = self.w(DPC) | 1; // RESUME THE DISPLAY NOW.
        self.set(DPC, v);
        self.in2 = self.pop(); // RESTORE THE REGISTERS NOW.
        self.in1 = self.pop();
        Ok(()) // RTI
    }

    /// LPIT1: a hit on one of the twelve items: make it blink, as the one to
    /// move into a readout slot.
    fn lpit1(&mut self) -> Flow {
        self.at(blk::LPIT1)?;
        self.in2 = self.w(DLIST.wrapping_add(self.in2)); // POINTER TO USER'S DISPLAY ITEM.
        self.in1 = self.w(LPFLG1); // AND PICK UP PREVIOUS ITEM NOW.
        if self.in1 != 0 {
            self.at(blk::LPIT1_1)?;
            let a = self.in1.wrapping_add(2);
            let v = self.w(a) & !0o10; // ELSE CLEAR OUT THE BLINK BITS.
            self.set(a, v);
        }
        self.at(blk::LPIT1P)?;
        self.set(LPFLG1, self.in2); // SAVE IT AWAY FOR DISPLAY TRANSFER.
        let a = self.in2.wrapping_add(2);
        let v = self.w(a) | 0o30; // SET IT BLINKING NOW.
        self.set(a, v);
        self.lpresm()
    }

    /// LPIT2: a hit on a readout slot: put the blinking item there.
    fn lpit2(&mut self) -> Flow {
        self.at(blk::LPIT2)?;
        self.in1 = self.w(LPFLG1); // GET HIS POINTER.
        if self.in1 == 0 {
            return self.lpresm(); // NONE SPECIFIED.
        }
        self.at(blk::LPIT2_1)?;
        self.set(LPFLG1, 0); // CLEAR POINTER NOW.
        // CHANGE OUR POINTER NOW: DIALTB-LPBASE+DLIST(IN2) is DIALTB(IN2).
        let slot = (DIALTB.wrapping_sub(LPBASE).wrapping_add(DLIST)).wrapping_add(self.in2);
        self.set(slot, self.in1);
        let a = self.in1.wrapping_add(2);
        let v = self.w(a) & !0o10; // AND CLEAR THE BLINK FLAG NOW.
        self.set(a, v);
        self.lpresm()
    }

    /// ARROW: a hit on one of the four turning arrows. The rate of turn is
    /// the word before the arrow's display list; the arrow is drawn again,
    /// brighter, until TIMER clears it.
    fn arrow(&mut self) -> Flow {
        self.at(blk::ARROW)?;
        self.in2 = self.w(DLIST.wrapping_add(self.in2)); // PICK UP ADDRESS.
        let rate = self.w(self.in2.wrapping_sub(2)); // MOVE OVER RATE OF TURN.
        self.set(TURN, rate);
        self.set(DLIST1, self.in2); // AND ALSO MAKE IT BRIGHTER.
        self.set(DLIST2, self.in2);
        self.lpresm()
    }
}
