//! EXPLOD: "THIS SECTION WILL CAUSE THE SKIP TO EXPLODE ON THE SCREEN BY
//! SHOOTING OUT A SERIES OF "DOTS" FOR A GIVEN PERIOD OF TIME." Then five
//! seconds, and the game starts again.
//!
//! The dots' spread comes from EXGEN's random numbers, and one ingredient of
//! those is `MOV -6(SP),IN1`: a word below the stack pointer, left there by
//! a deeper call or an interrupt ("RANDOM INTERUPT NUMBER").

use super::{dec_bgt, minus};
use crate::generated::blocks as blk;
use crate::generated::symbols::*;
use crate::machine::{Flow, Moonlander};

impl Moonlander {
    pub(crate) fn explod(&mut self) -> Flow {
        self.at(blk::EXPLOD)?;
        self.set(RADIUS, 0);
        self.set(FSUBC, 0); // TURN OFF THE ROCKET NOW.
        self.set(DUSTON, 0); // TURN OFF THE DUST, IF ANY.
        self.at(blk::EXPLOD_1)?;
        self.bell(); // "RING THE BELL".
        loop {
            self.at(blk::EXPLD1)?;
            self.temp2 = EXLIST; // WHERE TO SHOW EXPLOSION.
            self.temp = self.w(XTYPE).wrapping_add(1); // RANDOMIZING WORD.
            self.temp = self.temp.swap_bytes() >> 1; // SWAB, ROR (SWAB clears the carry).
            self.temp = self.temp.wrapping_add(self.w(self.temp2)); // ADD IN OLD COMMAND.
            self.temp &= !0o176177; // CLEAR OUT CRAP NOW.
            self.temp |= 0o116120; // MAKE IT A GOOD COMMAND NOW.
            self.set(self.temp2, self.temp);
            self.temp2 = self.temp2.wrapping_add(2);
            self.call(blk::EXPLD1_1, Self::exgen)?; // GENERATE SOME SMOKE NOW.
            self.bell(); // GIVE A GOOD BEEP NOW.
            let r = self.w(RADIUS).wrapping_sub(10); // NOW PRODUCE SOME MORE DOTS.
            self.set(RADIUS, r);
            self.call(blk::EXPLD1_2, Self::exgen)?;
            self.set(self.temp2, DISTOP); // TERMINATE THE PICTURE NOW.
            self.temp2 = self.temp2.wrapping_add(2);
            self.set(self.temp2, 0);
            self.set(FSUBC, EXLIST); // MAKE SURE WE'RE SHOWING IT.
            self.at(blk::EXPLD1_3)?;
            self.bell(); // GIVE A RANDOM BEEP, AT TIMES.
            let r = self.w(RADIUS).wrapping_add(33);
            self.set(RADIUS, r);
            if r as i16 > 0o300 {
                break; // DONE.
            }
        }
        self.at(blk::EXPLD1_4)?;
        self.call_delay(blk::EXPLD1_4, None) // DELAY NOW BEFORE RESTARTING.
    }

    /// BIS #0,DSR: any write to the display status register rings the bell.
    pub(crate) fn bell(&mut self) {
        let v = self.w(DSR);
        self.set(DSR, v);
    }

    /// EXGEN: a ring of dots at RADIUS round the ship, from -30 to 210
    /// degrees, each pushed in or out a little at random.
    fn exgen(&mut self) -> Flow {
        self.at(blk::EXGEN)?;
        self.set(ANGLE, (-30i16) as u16); // INITIALIZE THE ANGLE NOW.
        self.push(241); // COUNTER TO DO -30 TO 210 DEGREES.
        loop {
            self.at(blk::EXGENL)?;
            self.call(blk::EXGENL_1, Self::trig)?;
            self.in1 = self.w(self.sp.wrapping_sub(6)); // RANDOM INTERUPT NUMBER.
            self.in1 = (self.in1 as i16 >> 1) as u16; // JUST IN CASE IT WAS EVEN.
            self.in1 = self.in1.wrapping_add(1);
            self.in1 = self.in1.wrapping_add(self.w(TIME)); // ADD IN THE TIME ALSO.
            self.in1 = self.in1.wrapping_add(self.w(XTYPE)); // AND THE PREVIOUS NUMBER.
            self.set(XTYPE, self.in1);
            self.in1 &= 0o37; // BETWEEN 0 AND 31
            self.temp = self.bs(YUPDWN.wrapping_add(self.in1)); // USE THE FLAME TABLE.
            self.temp = self.temp.wrapping_add(self.w(RADIUS)); // A NEW RADIUS NOW.
            let mut placed = false;
            if !minus(self.temp) {
                self.at(blk::EXGENL_2)?;
                self.in1 = self.temp;
                self.in2 = self.w(COSANG);
                self.call(blk::EXGENL_3, Self::trgmul)?;
                self.ret1 = self.ret1.wrapping_add(self.w(SHOWX)); // OUR X NOW.
                if !minus(self.ret1) {
                    self.at(blk::EXGENL_4)?;
                    self.ret1 |= INT; // TURN ON INTENSIFY BIT.
                    self.set(self.temp2, self.ret1);
                    self.temp2 = self.temp2.wrapping_add(2);
                    self.in1 = self.temp; // NOW GET THE Y.
                    self.in2 = self.w(SINANG);
                    self.call(blk::EXGENL_5, Self::trgmul)?;
                    self.ret1 = self.ret1.wrapping_add(self.w(SHOWY));
                    if !minus(self.ret1) {
                        self.at(blk::EXGENL_6)?;
                        self.set(self.temp2, self.ret1);
                        self.temp2 = self.temp2.wrapping_add(2);
                        placed = true;
                    } else {
                        // IF X IS ALREADY THERE, TAKE IT BACK.
                        self.at(blk::EXGENO)?;
                        self.temp2 = self.temp2.wrapping_sub(2);
                        self.set(self.temp2, 0);
                    }
                }
            }
            if !placed {
                // INSERT DUMMIES SO DISPLAY WON'T SCREW UP.
                self.at(blk::EXGENP)?;
                self.set(self.temp2, 0);
                self.temp2 = self.temp2.wrapping_add(2);
                self.set(self.temp2, 0);
                self.temp2 = self.temp2.wrapping_add(2);
            }
            self.at(blk::EXGEND)?;
            let a = self.w(ANGLE).wrapping_add(1); // NEXT ANGLE.
            self.set(ANGLE, a);
            let n = self.top();
            self.set_top(n.wrapping_sub(1));
            if !dec_bgt(n) {
                break;
            }
        }
        self.at(blk::EXGEND_1)?;
        self.pop(); // AND THEN EXIT.
        Ok(()) // "THIS INSTRUCTION HELPS IF ONE LEAVES IT IN."
    }
}
