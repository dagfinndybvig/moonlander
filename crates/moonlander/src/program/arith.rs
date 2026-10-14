//! "THESE ARE SOME MISC MULTIPLY ROUTINES (AND DIVIDE) WHICH ARE NECESSARY
//! FOR THE SMOOTH OPERATION OF THE SYSTEM."
//!
//! The KD11-B has no multiply or divide instructions. Burness's routines are
//! fully unrolled shift-and-add and non-restoring shift-and-subtract loops,
//! so their time depends on the bits of the operands; the translation walks
//! the same blocks the original would.

use super::{asl, minus, rol};
use crate::generated::blocks::*;
use crate::machine::{Block, Flow, Moonlander};

/// MULTCM and its unlabelled successors: one rotate of the multiplier each.
const FIND: [Block; 15] = [
    MULTCM, MULTCM_1, MULTCM_2, MULTCM_3, MULTCM_4, MULTCM_5, MULTCM_6, MULTCM_7, MULTCM_8, MULTCM_9,
    MULTCM_10, MULTCM_11, MULTCM_12, MULTCM_13, MULTCM_14,
];
/// ADDk adds the multiplicand for multiplier bit k; NADDk shifts. Index k.
const ADD: [Option<Block>; 17] = [
    None, None, Some(ADD2), Some(ADD3), Some(ADD4), Some(ADD5), Some(ADD6), Some(ADD7), Some(ADD8),
    Some(ADD9), Some(ADD10), Some(ADD11), Some(ADD12), Some(ADD13), Some(ADD14), Some(ADD15), None,
];
const NADD: [Option<Block>; 17] = [
    None, None, Some(NADD2), Some(NADD3), Some(NADD4), Some(NADD5), Some(NADD6), Some(NADD7),
    Some(NADD8), Some(NADD9), Some(NADD10), Some(NADD11), Some(NADD12), Some(NADD13), Some(NADD14),
    Some(NADD15), None,
];
/// DIVTWO's sixteen steps, A to P: a quotient bit of 1 (DVOKx) or 0 (DVBADx).
const DVOK: [Block; 16] = [
    DVOKA, DVOKB, DVOKC, DVOKD, DVOKE, DVOKF, DVOKG, DVOKH, DVOKI, DVOKJ, DVOKK, DVOKL, DVOKM, DVOKN,
    DVOKO, DVOKP,
];
const DVBAD: [Block; 16] = [
    DVBADA, DVBADB, DVBADC, DVBADD, DVBADE, DVBADF, DVBADG, DVBADH, DVBADI, DVBADJ, DVBADK, DVBADL,
    DVBADM, DVBADN, DVBADO, DVBADP,
];

impl Moonlander {
    /// Shift RET1:RET2 left one place (ASL RET2, ROL RET1); the carry out
    /// of RET1 is the next multiplier bit.
    fn shift_ret(&mut self) -> bool {
        let (lo, c) = asl(self.ret2);
        let (hi, out) = rol(self.ret1, c);
        self.ret2 = lo;
        self.ret1 = hi;
        out
    }

    /// ADD IN2,RET2 / ADC RET1.
    fn add_multiplicand(&mut self) {
        let (lo, c) = self.ret2.overflowing_add(self.in2);
        self.ret2 = lo;
        self.ret1 = self.ret1.wrapping_add(c as u16);
    }

    /// MULTWO: RET1:RET2 = IN1 * IN2, unsigned. The smaller operand becomes
    /// the multiplier, in RET1, where the high half of the product
    /// accumulates as its bits are shifted out.
    pub(crate) fn multwo(&mut self) -> Flow {
        self.at(MULTWO)?;
        self.ret2 = 0; // CLEAR LOW ORDER RETURN VALUE.
        if self.in2 >= self.in1 {
            // BHIS: IN1 IS SMALLER (AS EXPECTED).
            self.at(MULT1L)?;
            self.ret1 = self.in1;
            if self.ret1 == 0 {
                return self.at(MULTDN); // IF IT'S ZERO, THEN WE ARE DONE.
            }
        } else {
            self.at(MULTWO_1)?;
            self.ret1 = self.in2;
            if self.ret1 == 0 {
                return self.at(MULTDN);
            }
            self.at(MULTWO_2)?;
            self.in2 = self.in1; // <INVERT>. CLC, BR MULTCM.
        }
        // ROTATE MULTIPLIER ONE PLACE, AND START MULTIPLY WHEN SET. The
        // carry going in is clear each time: from the CMP whose BHIS was
        // taken, from CLC, or from the last rotate, whose BCS was not.
        let mut first = None;
        for (i, &b) in FIND.iter().enumerate() {
            self.at(b)?;
            let (v, c) = rol(self.ret1, false);
            self.ret1 = v;
            if c {
                first = Some(16 - i);
                break;
            }
        }
        let Some(k) = first else {
            // RET1 MUST BE 100000: A MULTIPLY BY 1.
            self.at(MULTCM_15)?;
            self.ret1 = 0;
            self.ret2 = self.in2;
            return self.at(MULTDN);
        };
        // The multiplier's leading 1 has been shifted out at bit k. From
        // here each step adds the multiplicand if the bit just shifted out
        // was a 1 (ADDk), then shifts the product and multiplier (NADDk).
        let (mut bit, mut k) = if k == 16 {
            self.at(ADD16)?;
            self.ret2 = self.in2; // IF A BIGGY, JUST MOVE OVER CRAP.
            (self.shift_ret(), 15)
        } else {
            (true, k)
        };
        while k > 1 {
            if bit {
                self.at(ADD[k].unwrap())?;
                self.add_multiplicand(); // BIT ON. ADD IN MULTIPLICAND.
            }
            self.at(NADD[k].unwrap())?;
            bit = self.shift_ret(); // DO NEXT BIT NOW. OK TO MULT?
            k -= 1;
        }
        if bit {
            self.at(NADD2_1)?;
            self.add_multiplicand();
        }
        self.at(NADD1) // AND RETURN WHEN DONE.
    }

    /// SGNMUL: "THIS ROUTINE WILL MULTIPLY TWO SIGNED NUMBERS." Leaves both
    /// inputs made positive.
    pub(crate) fn sgnmul(&mut self) -> Flow {
        self.at(SGNMUL)?;
        if !minus(self.in1) {
            self.at(SGNMP1)?;
            if !minus(self.in2) {
                return self.sgnmp3(); // IT'S ALSO POSITIVE. DO MULTIPLY.
            }
            self.at(SGNMP1_1)?;
            self.in2 = self.in2.wrapping_neg(); // ELSE NEGATE IT, AND INVERT ANSWER.
            return self.sgnmp2();
        }
        self.at(SGNMUL_1)?;
        self.in1 = self.in1.wrapping_neg();
        if !minus(self.in2) {
            return self.sgnmp2(); // IT'S POSITIVE.
        }
        self.at(SGNMUL_2)?;
        self.in2 = self.in2.wrapping_neg(); // BOTH NEGATIVE=POSITIVE.
        self.sgnmp3()
    }

    fn sgnmp3(&mut self) -> Flow {
        self.at(SGNMP3)?;
        self.call(SGNMP3_1, Self::multwo) // and RTS PC
    }

    fn sgnmp2(&mut self) -> Flow {
        self.at(SGNMP2)?;
        self.call(SGNMP2_1, Self::multwo)?;
        // AND DO A DOUBLE PRECISION NEGATE: NEG RET2, ADC RET1, NEG RET1.
        self.ret2 = self.ret2.wrapping_neg();
        let c = self.ret2 != 0;
        self.ret1 = self.ret1.wrapping_add(c as u16).wrapping_neg();
        Ok(())
    }

    /// TRGMUL: a signed multiply by a sine or cosine (scaled by 2^14), with
    /// the result in RET1 scaled back: RET1:RET2 shifted left twice.
    pub(crate) fn trgmul(&mut self) -> Flow {
        self.at(TRGMUL)?;
        self.call(TRGMUL_1, Self::sgnmul)?;
        self.shift_ret(); // FUDGE THE RETURN ANSWER NOW.
        self.shift_ret();
        Ok(())
    }

    /// DIVTWO: "THIS ROUTINE WILL DIVIDE RET1-RET2 BY IN1 AND LEAVE THE
    /// ANSWER IN RET2. DOES IT BY 16 SUBTRACTIONS. CRUDE BUT FAST." The
    /// remainder is left in RET1. A non-restoring division: after a
    /// subtraction that goes negative it adds instead on the next step.
    pub(crate) fn divtwo(&mut self) -> Flow {
        self.at(DIVTWO)?;
        self.shift_ret(); // WE FORBID LEFT-MOST BIT TO BE ON.
        self.ret1 = self.ret1.wrapping_sub(self.in1); // SEE IF IT FITS.
        let mut fits = !minus(self.ret1);
        for s in 0..16 {
            if fits {
                // SUBTRACTION IS GOOD. INSERT A "1".
                self.at(DVOK[s])?;
                self.ret2 = self.ret2.wrapping_add(1);
                if s == 15 {
                    return self.at(DVOKX);
                }
                self.shift_ret();
                self.ret1 = self.ret1.wrapping_sub(self.in1);
            } else {
                // NOT YET. PUT IN A ZERO.
                self.at(DVBAD[s])?;
                if s == 15 {
                    // ON LAST FAIL, ADD BACK IN TO GET A CORRECT REMAINDER.
                    self.ret1 = self.ret1.wrapping_add(self.in1);
                    return Ok(());
                }
                self.shift_ret();
                self.ret1 = self.ret1.wrapping_add(self.in1); // 2*(Y-X)+X=2*Y-X.
            }
            fits = !minus(self.ret1);
        }
        unreachable!()
    }
}
