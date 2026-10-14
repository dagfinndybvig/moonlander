//! "THIS ROUTINE WILL DRAW THE SHIP RELATIVE TO IT'S CENTER. THE ANGLES
//! HAVE ALREADY BEEN GOTTEN FROM SOMEWHERE ELSE AND HAVE BEEN LEFT IN COSANG
//! AND SINANG. IN ADDITION, IN1 POINTS TO THE COMMANDS ON HOW TO DRAW THE
//! SHIP, AND IN2 POINTS TO WHERE TO LEAVE THEM."
//!
//! The command list is a little program of its own: each entry is the
//! address of the routine that handles it (DRAWIN copies a display word,
//! DRAWIS and DRAWVS rotate a point and emit an invisible or visible short
//! vector to it, DRAWDN ends the list), and DRAW runs it with
//! `MOV (TEMP)+,PC`. TEMP is left pointing just past the list, which is what
//! ROCKET later divides by (journal entry 007).

use super::minus;
use crate::generated::blocks as blk;
use crate::generated::symbols::*;
use crate::machine::{Escape, Flow, Moonlander};

impl Moonlander {
    /// SUB src from the word at (SP) / BIS into it, and the like.
    pub(crate) fn top(&mut self) -> u16 {
        let sp = self.sp;
        self.w(sp)
    }

    pub(crate) fn set_top(&mut self, v: u16) {
        let sp = self.sp;
        self.set(sp, v);
    }

    pub(crate) fn draw(&mut self) -> Flow {
        self.at(blk::DRAW)?;
        self.temp = self.in1; // MOVE OVER THE INPUT LIST POINTER.
        self.temp2 = self.in2; // AND THE OUTPUT LIST POINTER.
        self.set(LASTX, 0); // SET TO CENTER OF OBJECT.
        self.set(LASTY, 0);
        self.set(LOWEST, 0o77777); // LOWEST POINT WE HAVE DRAWN SO FAR.
        loop {
            // DRAWLP: GO TO FIRST INSTRUCTION.
            self.at(blk::DRAWLP)?;
            let routine = self.w(self.temp);
            self.temp = self.temp.wrapping_add(2);
            match routine {
                DRAWIS => {
                    // DRAW INVISIBLE SHORT.
                    self.at(blk::DRAWIS)?;
                    self.push(0);
                    self.call(blk::DRAWIS_1, Self::drawrt)?;
                    self.drawsc()?;
                }
                DRAWVS => {
                    // DRAW VISIBLE SHORT, UNLESS EITHER END IS BELOW LOWY.
                    self.at(blk::DRAWVS)?;
                    self.push(0o40000);
                    self.call(blk::DRAWVS_1, Self::drawrt)?;
                    let lowy = self.w(LOWY) as i16;
                    if self.w(LASTY) as i16 >= lowy {
                        self.at(blk::DRAWV1)?;
                        if self.ret2 as i16 >= self.w(LOWY) as i16 {
                            // IT'S ALSO OK.
                        } else {
                            self.at(blk::DRAWV1_1)?;
                            self.set_top(0); // ELSE CLEAR THE DISPLAY BIT.
                        }
                    } else {
                        self.at(blk::DRAWVS_2)?;
                        self.set_top(0); // NOPE. CLEAR OUT DISPLAY BIT.
                    }
                    self.drawsc()?;
                }
                DRAWIN => {
                    // THIS MOVES OVER ONE WORD.
                    self.at(blk::DRAWIN)?;
                    let v = self.w(self.temp);
                    self.temp = self.temp.wrapping_add(2);
                    self.set(self.temp2, v);
                    self.temp2 = self.temp2.wrapping_add(2);
                }
                DRAWDN => {
                    // THIS TERMINATES THE PICTURE AND WILL CAUSE THE
                    // SUBROUTINE TO EXIT.
                    self.at(blk::DRAWDN)?;
                    self.set(self.temp2, DISTOP);
                    self.temp2 = self.temp2.wrapping_add(2);
                    self.set(self.temp2, 0);
                    self.temp2 = self.temp2.wrapping_add(2);
                    return Ok(());
                }
                other => return Err(Escape::Wild(other)),
            }
        }
    }

    /// DRAWSC: make the short vector from the last point to the rotated one
    /// (X in RET1, Y in RET2), with the visibility bit already on the stack.
    fn drawsc(&mut self) -> Flow {
        self.at(blk::DRAWSC)?;
        self.in1 = self.ret1; // MOVE OVER X.
        self.ret1 = self.ret1.wrapping_sub(self.w(LASTX)); // FIGURE OUT DELTA X.
        if minus(self.ret1) {
            // REVERSE SIGN AND SET MINUS BITS IN THE NEW SHORT VECTOR COMMAND.
            self.at(blk::DRAWSC_1)?;
            self.ret1 = self.ret1.wrapping_neg();
            let v = self.top() | 0o20000;
            self.set_top(v);
        }
        self.at(blk::DRAWS1)?;
        // BIC #-100 (REMOVE STRAY BITS), SWAB, ROR: SWAB clears the carry,
        // so the rotate is a plain shift. Delta X lands in bits 12-7.
        self.ret1 = (self.ret1 & 0o77).swap_bytes() >> 1;
        self.set(LASTX, self.in1); // UPDATE THE NEW X POSITION.
        self.in2 = self.ret2; // AND NOW DO SAME THING WITH THE Y.
        self.ret2 = self.ret2.wrapping_sub(self.w(LASTY));
        if minus(self.ret2) {
            self.at(blk::DRAWS1_1)?;
            self.ret2 = self.ret2.wrapping_neg();
            let v = self.top() | 0o100;
            self.set_top(v);
        }
        self.at(blk::DRAWS2)?;
        self.ret2 &= 0o77;
        self.set(LASTY, self.in2); // UPDATE THE NEW Y NOW.
        if (self.in2 as i16) < self.w(LOWEST) as i16 {
            // IF LOWEST, REMEMBER IT.
            self.at(blk::DRAWS2_1)?;
            self.set(LOWEST, self.in2);
        }
        self.at(blk::DRAWNL)?;
        self.ret2 |= self.ret1; // NOW CREATE THE NEW INSTRUCTION.
        let bits = self.pop();
        self.ret2 |= bits;
        self.set(self.temp2, self.ret2); // AND STORE IT AWAY.
        self.temp2 = self.temp2.wrapping_add(2);
        Ok(())
    }

    /// DRAWRT: "THIS ROTATES THE X AND Y BYTE. (TEMP) POINTS TO THE X."
    /// Returns X*COS+Y*SIN in RET1 and Y*COS-X*SIN in RET2.
    pub(crate) fn drawrt(&mut self) -> Flow {
        self.at(blk::DRAWRT)?;
        self.in1 = self.bs(self.temp); // PICK UP THE X.
        self.temp = self.temp.wrapping_add(1);
        self.in2 = self.w(SINANG);
        self.call(blk::DRAWRT_1, Self::trgmul)?;
        self.push(self.ret1); // AND SAVE X*SINANG.
        self.in1 = self.bs(self.temp); // PICK UP THE Y.
        self.in2 = self.w(COSANG);
        self.call(blk::DRAWRT_2, Self::trgmul)?;
        let v = self.top().wrapping_sub(self.ret1); // X*SINANG-Y*COSANG
        self.set_top(v);
        self.in1 = self.bs(self.temp.wrapping_sub(1)); // PICK UP THE X AGAIN.
        self.in2 = self.w(COSANG);
        self.call(blk::DRAWRT_3, Self::trgmul)?;
        self.push(self.ret1); // X*COSANG
        self.in1 = self.bs(self.temp); // Y ONCE AGAIN.
        self.temp = self.temp.wrapping_add(1);
        self.in2 = self.w(SINANG);
        self.call(blk::DRAWRT_4, Self::trgmul)?;
        let v = self.pop();
        self.ret1 = self.ret1.wrapping_add(v); // X*COS+Y*SIN
        self.ret2 = self.pop().wrapping_neg(); // Y*COS-X*SIN
        Ok(())
    }
}
