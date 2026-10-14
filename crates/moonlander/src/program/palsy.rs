//! After a good landing: PALSY. The man climbs out and walks. If a
//! McDonald's is on the screen he walks to it, orders, walks back, climbs in
//! and the ship takes off again, a little more fuel aboard; otherwise he
//! plants a flag. Burness's name for the man's walk is in the comment at
//! MAYBEP: "WITH THE SPASTIC MAN."
//!
//! The walk is done one step at a time by an instruction MOVMAN writes into
//! PALSYI, INC R0 or DEC R0, and then calls.

use super::dec_bgt;
use crate::generated::blocks as blk;
use crate::generated::symbols::*;
use crate::machine::{Escape, Flow, Moonlander};

/// The instructions MOVMAN generates: `#INC` and `#DEC` in the source,
/// which assemble as INC R0 and DEC R0.
const INC_IN1: u16 = 0o005200;
const DEC_IN1: u16 = 0o005300;

impl Moonlander {
    pub(crate) fn palsy(&mut self) -> Flow {
        self.at(blk::PALSY)?;
        self.call_delay(blk::PALSY, Some(blk::PALSY_1))?; // WAIT FOUR SECONDS.
        let x = self.w(SHOWX);
        self.set(MANX, x); // POSITION MAN CORRECTLY.
        let y = self.w(SHOWY);
        self.set(MANY, y);
        self.set(DUSTON, MAN); // SHOW THE MAN NOW.
        // WAIT FOR CLOCK TO LINE UP, WHICH SHOULD TAKE A RANDOM AMOUNT OF TIME.
        self.at(blk::PALSY1)?;
        while self.w(TIME) & 0o177 != 0 {
            self.at(blk::PALSY1)?;
        }
        self.at(blk::PALSY1_1)?;
        if self.w(MACON) == 0 {
            return self.palnor(); // NONE. NORMAL PLANT.
        }
        self.at(blk::PALSY1_2)?;
        self.ret2 = self.w(MACX).wrapping_sub(self.w(SHOWX)); // WHICH WAY TO GO.
        self.ret1 = self.w(MACY).wrapping_sub(self.w(MANY)).wrapping_add(3); // LIFT HIM OFF THE FLOOR.
        self.push(self.ret1);
        if self.ret1 != 0 {
            self.at(blk::PALSY1_3)?;
            self.call(blk::PALMX1, Self::movman)?;
        } else {
            self.at(blk::PALMX1)?; // IF NO Y, THEN ONLY MOVE X.
        }
        self.ret2 = self.w(MACX).wrapping_sub(self.w(MANX)).wrapping_add(25); // INTO THE PLACE.
        self.push(self.ret2); // REMEMBER WHERE WE WERE.
        self.ret1 = 0;
        self.call(blk::PALMX1_1, Self::movman)?;
        self.set(SYSMES, ORDER); // ORDER THE FOOD NOW.
        self.call_delay(blk::PALMX1_1, Some(blk::PALMX1_2))?;
        self.set(SYSMES, 0);
        self.call_delay(blk::PALMX1_2, Some(blk::PALMX1_3))?;
        let back = self.top().wrapping_neg(); // THE NEGATIVE DISTANCE.
        self.set_top(back);
        self.ret2 = back;
        self.ret1 = 0;
        self.call(blk::PALMX1_4, Self::movman)?;
        self.ret2 = self.pop(); // AND THEN GO UP INTO THE SHIP.
        self.ret1 = self.pop().wrapping_neg();
        if self.ret1 != 0 {
            self.at(blk::PALMX1_5)?;
            self.call(blk::PALMX2, Self::movman)?; // UP HE GOES NOW.
        } else {
            self.at(blk::PALMX2)?;
        }
        self.call_delay(blk::PALMX2, Some(blk::PALMX2_1))?; // HOLD HIM IN SHIP A FEW SECONDS.
        // THEN LIFT HIM UP, AND TAKE HIM OFF, WITH A LITTLE MORE FUEL.
        let v = self.w(VERDIS).wrapping_add(4);
        self.set(VERDIS, v);
        let f = self.w(FUEL).wrapping_add(2000);
        self.set(FUEL, f);
        self.set(VERVEL, 0);
        self.set(TICKS, 0);
        // "AND SET SP TO TOP OF CORE <AGAIN>": every frame below this one is
        // abandoned. This routine never returns; it ends by restarting.
        self.sp = STACK;
        loop {
            self.at(blk::PALOFF)?;
            self.set(SYSMES, 0); // LOOP WITHOUT GIVING ANY MESSAGES.
            self.set(ANGLE, 0); // STRAIGHT UP.
            self.set(HORVEL, 0);
            self.set(PERCNT, 30); // ENOUGH THRUST TO RISE UP.
            self.call(blk::PALOFF_1, Self::eidle)?; // AND DO EVERYTHING RIGHT.
            if self.w(MOON) == 0 {
                break; // WAIT TILL MOON GETS BIG.
            }
        }
        self.at(blk::PALOFF_2)?;
        self.set(SHIPDP, 0); // REMOVE SHIP AND FLAME.
        self.set(FSUBC, 0);
        self.call_delay(blk::PALOFF_2, None)
    }

    /// PALNOR: no McDonald's. The man walks out and plants a flag, and both
    /// are remembered in the feature table.
    fn palnor(&mut self) -> Flow {
        self.at(blk::PALNOR)?;
        self.push(1); // THE SHIP HAS LANDED HERE.
        let i = self.w(INDEXL);
        self.push(i);
        self.call(blk::PALNOR_1, Self::putfet)?;
        self.ret1 = (-24i16) as u16; // PREPARE TO MOVE THE MAN OUT NOW.
        self.ret2 = 48;
        // ROR TEMP2: the low bit of TIME picks the direction. The carry
        // rotated in is from PUTFET's ADD #4,SP.
        let time = self.w(TIME);
        let c_in = (self.sp.wrapping_sub(6) as u32 + 4) > 0xFFFF;
        self.temp2 = (time >> 1) | ((c_in as u16) << 15);
        if time & 1 != 0 {
            self.at(blk::PALNOR_2)?;
            self.ret2 = self.ret2.wrapping_neg(); // IF ON, GO LEFT.
        }
        self.at(blk::PALN1)?;
        self.push(self.ret2);
        self.call(blk::PALN1_1, Self::movman)?;
        self.ret2 = self.pop(); // AND THEN THE FINAL DISTANCE.
        self.ret1 = 0;
        self.call(blk::PALN1_2, Self::movman)?;
        let x = self.w(MANX); // DISPLAY THE FLAG NOW.
        self.set(FLAGX, x);
        let y = self.w(MANY);
        self.set(FLAGY, y);
        let x = self.w(FLAGX).wrapping_add(20); // OFFSET IT A LITTLE BIT.
        self.set(FLAGX, x);
        self.set(FSUBC, FLAGL); // PLACE FLAG IN THE FLAME LIST.
        self.in1 = self.w(INDEXL);
        // Two more of the man's steps, applied to the index: where the flag is.
        self.call(blk::PALN1_3, Self::palsyi)?;
        self.call(blk::PALN1_4, Self::palsyi)?;
        self.push(2);
        self.push(self.in1);
        self.call(blk::PALN1_5, Self::putfet)?;
        self.set(SYSMES, MANMSG);
        self.call_delay(blk::PALN1_5, None) // WAIT TEN MORE SECONDS, AND START AGAIN.
    }

    /// MOVMAN: "THIS ROUTINE MOVES THE MAN ALONG THE SURFACE OF THE MOON.
    /// RET1=DELTA Y; RET2=DELTA X. IF Y=0, THEN ONLY X IS USED. IF Y<>0, THEN
    /// ONLY THE SIGN OF THE X IS USED." One step every eight clock ticks.
    pub(crate) fn movman(&mut self) -> Flow {
        self.at(blk::MOVMAN)?;
        self.temp2 = INC_IN1; // "INC IN1" TO INDICATE X TO RIGHT.
        if self.ret2 & 0x8000 != 0 {
            self.at(blk::MOVMAN_1)?;
            self.temp2 = DEC_IN1;
            self.ret2 = self.ret2.wrapping_neg(); // SET THE COUNTER TO GO DOWN.
        }
        self.at(blk::MOVM1)?;
        self.set(PALSYI, self.temp2); // SAVE AWAY THE GENERATED INSTRUCTION NOW.
        self.temp2 = 0; // THE DELTA Y ADD.
        if self.ret1 != 0 {
            self.at(blk::MOVM1_1)?;
            self.temp2 = 1; // SOME Y. SET TO UP.
            self.ret2 = self.ret1; // USE THE Y AS THE COUNTER NOW.
            if self.ret2 & 0x8000 != 0 {
                self.at(blk::MOVM1_2)?;
                self.ret2 = self.ret2.wrapping_neg();
                self.temp2 = self.temp2.wrapping_neg();
            }
        }
        self.at(blk::MOVJX)?;
        self.in1 = self.w(MANX); // THE PRESENT POSITION.
        loop {
            self.at(blk::MOVLUP)?;
            self.call(blk::MOVLUP_1, Self::palsyi)?; // MOVE OVER THE X.
            self.set(MANX, self.in1);
            let y = self.w(MANY).wrapping_add(self.temp2); // UPDATE THE Y ALSO.
            self.set(MANY, y);
            self.call(blk::MOVLUP_2, Self::palsyw)?; // WAIT FOR 8 CLOCK TICKS TO GO BY.
            let more = dec_bgt(self.ret2);
            self.ret2 = self.ret2.wrapping_sub(1);
            if !more {
                break;
            }
        }
        self.at(blk::MOVLUP_3) // RTS
    }

    /// PALSYI: run the instruction MOVMAN wrote here, then RTS.
    fn palsyi(&mut self) -> Flow {
        self.at(blk::PALSYI)?;
        match self.w(PALSYI) {
            INC_IN1 => self.in1 = self.in1.wrapping_add(1),
            DEC_IN1 => self.in1 = self.in1.wrapping_sub(1),
            0 => return Err(Escape::Halt(PALSYI)), // as assembled: HALT
            _ => return Err(Escape::Wild(PALSYI)),
        }
        Ok(())
    }

    /// PALSYW: wait for the clock to leave a multiple of 8, then to reach
    /// the next one, keeping the readouts going.
    fn palsyw(&mut self) -> Flow {
        self.at(blk::PALSYW)?;
        while self.w(TIME) & 7 == 0 {
            self.at(blk::PALSYW)?;
        }
        self.at(blk::PALSYW_1)?;
        self.call(blk::PALSYV, Self::dial)?;
        while self.w(TIME) & 7 != 0 {
            self.at(blk::PALSYV)?;
        }
        self.at(blk::PALSYV_1)?;
        self.call(blk::PALSYV_2, Self::dial)?;
        Ok(()) // RTS
    }
}
