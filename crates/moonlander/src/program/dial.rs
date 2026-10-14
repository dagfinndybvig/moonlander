//! The readouts: DIAL ("THIS ROUTINE WILL CALCULATE THE VALUES THAT THE
//! USER WISHES TO HAVE DISPLAYED"), the number to text conversions ASCII
//! and SASCII, and DELAY, which keeps the readouts going while it waits.
//!
//! The four readout slots are display lists DITEM1-4, each ending in a
//! display jump to the label text of whichever of the twelve items is
//! chosen there (DIALTB). The light pen moves items between slots; DIAL
//! rewrites the jump and the six digits in place every time it runs.

use super::{dec_bgt, minus};
use crate::generated::blocks as blk;
use crate::generated::symbols::*;
use crate::machine::{Block, Escape, Flow, Moonlander};

impl Moonlander {
    /// MOVB #c,(IN2)+
    fn put_char(&mut self, c: u8) {
        self.setb(self.in2, c);
        self.in2 = self.in2.wrapping_add(1);
    }

    pub(crate) fn dial(&mut self) -> Flow {
        self.at(blk::DIAL)?;
        // SAVE ALL THE IMPORTANT REGISTERS NOW.
        for r in [self.in1, self.in2, self.ret1, self.ret2, self.temp, self.temp2] {
            self.push(r);
        }
        self.temp = 0; // SET POINTER TO FIRST ITEM.
        loop {
            self.at(blk::DIALL)?;
            self.temp2 = self.w(DIALTB.wrapping_add(self.temp)); // POINTER TO AN ITEM ENTRY.
            if self.temp2 == 0 {
                break; // IF ZERO, THEN END OF LIST.
            }
            self.at(blk::DIALL_1)?;
            self.in2 = self.w(DIALTC.wrapping_add(self.temp)); // WHERE TO DISPLAY IT.
            // CALCULATE "ITEMFX" ENTRY ADDRESS AND INSERT IT IN THE DISJMP
            // OF THE DITEMX TABLE.
            self.ret2 = self.temp2.wrapping_add(10);
            self.set(self.in2.wrapping_add(18), self.ret2);
            let var = self.w(self.temp2.wrapping_sub(4)); // PICK UP USER'S NUMBER NOW.
            self.ret2 = self.w(var);
            self.in1 = self.w(self.temp2.wrapping_sub(2)); // THE DIVISOR <IF ANY>.
            if self.in1 != 0 {
                self.at(blk::DIALL_2)?;
                if minus(self.ret2) {
                    self.at(blk::DIALL_3)?;
                    self.ret2 = self.ret2.wrapping_neg(); // MAKE ANSWER POSITIVE.
                }
                self.at(blk::DIALLP)?;
                self.ret1 = 0; // GET RID OF THE HIGH PART.
                self.call(blk::DIALLP_1, Self::divtwo)?;
                let var = self.w(self.temp2.wrapping_sub(4)); // NOW RESTORE THE SIGN.
                if minus(self.w(var)) {
                    self.at(blk::DIALLP_2)?;
                    self.ret2 = self.ret2.wrapping_neg();
                }
            }
            self.at(blk::DIALND)?;
            self.temp2 = self.w(DIALTC.wrapping_add(self.temp));
            self.in1 = self.ret2; // MOVE OVER NUMBER NOW.
            self.in2 = 10u16.wrapping_add(self.temp2); // WHERE TO LEAVE THE ANSWER.
            self.call(blk::DIALND_1, Self::sascii)?;
            // TST (TEMP)+: the read of low memory it makes has no effect.
            self.temp = self.temp.wrapping_add(2); // NEXT ITEM.
        }
        self.at(blk::DIALD1)?;
        if self.w(PERCNT) != self.w(OLDPER) {
            // CONVERT PERCENTAGE NOW.
            self.at(blk::DIALD1_1)?;
            self.in2 = LPBARC; // WHERE TO PLACE STRING.
            self.in1 = self.w(PERCNT);
            self.set(OLDPER, self.in1);
            self.call(blk::DIALRT, Self::sascii)?;
        } else {
            self.at(blk::DIALRT)?;
        }
        // RESTORE ALL THE ACTIVE REGISTERS NOW.
        self.temp2 = self.pop();
        self.temp = self.pop();
        self.ret2 = self.pop();
        self.ret1 = self.pop();
        self.in2 = self.pop();
        self.in1 = self.pop();
        Ok(())
    }

    /// ASCII: "THIS ROUTINE CONVERT THE BINARY NUMBER (UNSIGNED) IN THE
    /// REGISTER "IN1" INTO AN ASCII STRING SIX CHARACTERS LONG AND LEAVES THE
    /// ANSWER TO WHERE "IN2" POINTS TO. THE ANSWER IS PUT RIGHT JUSTIFIED
    /// WITH LEADING BLANKS." The digits come two at a time from TENTAB; the
    /// leading-zero flag is a word on the stack that ASCDO reads in its
    /// caller's frame.
    pub(crate) fn ascii(&mut self) -> Flow {
        self.at(blk::ASCII)?;
        self.put_char(b' '); // FIRST CHARACTER WILL BE BLANK ALWAYS.
        if self.in1 >= 10000 {
            // GREATER THAN 9999: DO SOMETHING SPECIAL.
            self.at(blk::ASCIIG)?;
            // MOV PC,-(SP): the flag becomes the PC, which is not zero.
            self.push(blk::ASCIIG.addr + 2);
            self.put_char(b'0');
            loop {
                self.at(blk::ASCIIM)?;
                if self.in1 < 10000 {
                    break;
                }
                self.at(blk::ASCIIM_1)?;
                let a = self.in2.wrapping_sub(1); // CHANGE DIGIT AND UPDATE.
                let d = self.b(a).wrapping_add(1);
                self.setb(a, d);
                self.in1 = self.in1.wrapping_sub(10000);
            }
        } else {
            self.at(blk::ASCII_1)?;
            self.put_char(b' ');
            self.push(0); // AND REMEMBER TO DELETE LEADING ZEROS.
        }
        self.at(blk::ASCIIL)?;
        if (self.in1 as i16) < 100 {
            // NO DIVIDE IS NECESSARY.
            self.at(blk::ASCIID)?;
            self.ret1 = self.in1; // SET UP REMAINDER.
            self.ret2 = TENTAB; // AND ADDRESS FOR THE FIRST 0.
        } else {
            self.at(blk::ASCIIL_1)?;
            self.ret2 = self.in1; // GREATER THAN 99. DIVIDE.
            self.ret1 = 0;
            self.in1 = 100;
            self.call(blk::ASCIIL_2, Self::divtwo)?;
            // LOOK UP ANSWER IN TABLE: TWO BYTES WHICH COMPOSE NEXT TWO DIGITS.
            self.ret2 = (self.ret2 << 1).wrapping_add(TENTAB);
        }
        self.at(blk::ASCIIB)?;
        self.call(blk::ASCIIB_1, Self::ascdo)?; // INSERT FIRST.
        self.call(blk::ASCIIB_2, Self::ascdo)?; // INSERT SECOND.
        self.ret2 = (self.ret1 << 1).wrapping_add(TENTAB); // THE REMAINDER.
        self.call(blk::ASCIIB_3, Self::ascdo)?;
        // MOV PC,(SP): BUT MAKE SURE WE GET A GOOD ZERO.
        self.set_top(blk::ASCIIB_3.addr + 2);
        self.call(blk::ASCIIB_4, Self::ascdo)?;
        self.sp = self.sp.wrapping_add(2); // BUMP THE STACK NOW.
        Ok(())
    }

    /// ASCDO: insert the digit at (RET2), or a blank while still in the
    /// leading zeros.
    fn ascdo(&mut self) -> Flow {
        self.at(blk::ASCDO)?;
        let flag = self.w(self.sp.wrapping_add(2)); // INSERT ANYTHING?
        if flag == 0 {
            self.at(blk::ASCDO_1)?;
            if self.b(self.ret2) == b'0' {
                // STILL ZERO. INSERT A BLANK.
                self.at(blk::ASCDO_2)?;
                self.put_char(b' ');
                self.ret2 = self.ret2.wrapping_add(1);
                return Ok(());
            }
            // GOOD CHARACTER. CLEAR ZERO FLAG (MOV SP,2(SP)).
            self.at(blk::ASCDO1)?;
            self.set(self.sp.wrapping_add(2), self.sp);
        }
        self.at(blk::ASCDBP)?;
        let c = self.b(self.ret2); // MOVE OVER THE CHARACTER NOW.
        self.ret2 = self.ret2.wrapping_add(1);
        self.put_char(c);
        Ok(())
    }

    /// SASCII: "THIS ROUTINE WILL CONVERT A SIGNED NUMBER TO AN ASCII STRING
    /// ACCORDING TO THE RULES USED IN "ASCII"."
    pub(crate) fn sascii(&mut self) -> Flow {
        self.at(blk::SASCII)?;
        if !minus(self.in1) {
            return self.ascii(); // IF POSITIVE, NOTHING SPECIAL.
        }
        self.at(blk::SASCII_1)?;
        self.in1 = self.in1.wrapping_neg();
        self.call(blk::SASCII_2, Self::ascii)?;
        self.in1 = self.in2; // MOV BACK LAST POINTER.
        loop {
            // LOOK FOR THE FIRST BACKWARD BLANK.
            self.at(blk::SASCIL)?;
            self.in1 = self.in1.wrapping_sub(1);
            if self.b(self.in1) == b' ' {
                break;
            }
        }
        self.at(blk::SASCIL_1)?;
        self.setb(self.in1, b'-'); // WHEN FOUND, CHANGE IT.
        Ok(())
    }

    /// DELAY: "IT WILL WAIT "N" SECONDS BEFORE RESTARTING THE SYSTEM." N is
    /// the word after the call. Negative N waits and returns past that
    /// word; positive N waits and restarts the game. While waiting it keeps
    /// the readouts up to date.
    pub(crate) fn delay(&mut self) -> Flow {
        self.at(blk::DELAY)?;
        self.set(LPSW, DISTOP); // TURN OFF LOW FUEL MESSAGE.
        let param = self.top();
        self.in1 = self.w(param); // PICK UP THE TIME.
        self.in2 = self.w(TIME); // GET CURRENT TIME.
        self.ret1 = self.in1;
        if minus(self.in1) {
            self.at(blk::DELAY_1)?;
            self.in1 = self.in1.wrapping_neg();
        }
        loop {
            // CALCULATE FUTURE TIME.
            self.at(blk::DELAY1)?;
            self.in2 = self.in2.wrapping_add(CLKFRQ);
            let more = dec_bgt(self.in1);
            self.in1 = self.in1.wrapping_sub(1);
            if !more {
                break;
            }
        }
        loop {
            self.at(blk::DELAY2)?;
            self.wait()?; // WAIT FOR AN INTERUPT TO OCCUR.
            self.at(blk::DELAY2_1)?;
            self.call(blk::DELAY2_2, Self::dial)?; // DISPLAY THE USER'S DATA.
            if self.in2 <= self.w(TIME) {
                break; // BHI: unsigned.
            }
        }
        self.at(blk::DELAY2_3)?;
        let r = self.top().wrapping_add(2); // UPDATE THE RETURN PC NOW.
        self.set_top(r);
        if minus(self.ret1) {
            return self.at(blk::DVOKX); // IF NEGATIVE, THEN RETURN.
        }
        self.at(blk::DELAY2_4)?;
        Err(Escape::Restart) // ELSE RESTART THE SYSTEM.
    }

    /// JSR PC,DELAY followed by the word holding N. `call` is the block
    /// ending in the JSR (N is the word after it); `next` is where a
    /// negative N returns to.
    pub(crate) fn call_delay(&mut self, call: Block, next: Option<Block>) -> Flow {
        let to = self.jsr_at(call.end, Self::delay)?;
        match next {
            Some(b) if to == b.addr => self.at(b),
            _ => Err(Escape::Wild(to)),
        }
    }
}
