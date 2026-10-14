//! The physics: the engine (ROCKET), the flame (FLAME), the sines and
//! cosines (TRIG) and the motion (WHERE).
//!
//! Units, from the source: distances in feet, velocities in tenths of a foot
//! per second, accelerations in 1/500 ft/s^2, fuel in tenths of a pound,
//! thrust and weight in pounds, angles in degrees, sines and cosines scaled
//! by 2^14, time in line clock ticks (CLOCK, the ticks since the last step).

use super::{asr, dec_bgt, minus};
use crate::generated::blocks as blk;
use crate::generated::symbols::*;
use crate::machine::{Flow, Moonlander};

impl Moonlander {
    /// ROCKET: "THIS ROUTINE FIGURES OUT THE ACCELERATION OF THE SHIP AND
    /// THE AMOUNT OF FUEL REMAINING."
    pub(crate) fn rocket(&mut self) -> Flow {
        self.at(blk::ROCKET)?;
        if self.w(FUEL) == 0 {
            return self.rockof(); // NONE. TURN OFF THE ENGINE.
        }
        self.at(blk::ROCKET_1)?;
        self.in1 = self.w(PERTRS); // PICK UP THE PERCENT THROTTLE NOW.
        self.in2 = MAXTRS; // GET THE MAXIMUM THRUST NOW.
        self.call(blk::ROCKET_2, Self::multwo)?; // PER CENT * MAXTHRUST.
        self.in1 = 100; // %*MAXIMUM THRUST /100.
        self.call(blk::ROCKET_3, Self::divtwo)?;
        self.set(THRUST, self.ret2);
        self.in2 = self.ret2;
        self.in1 = self.w(CLOCK); // NUMBER OF CLOCK TICKS.
        self.call(blk::ROCKET_4, Self::multwo)?;
        self.in1 = CLKFDG; // FREQUENCY*25.
        self.call(blk::ROCKET_5, Self::divtwo)?; // APPROX. 250LBS FUEL PER LB THRUST.
        self.ret2 = self.ret2.wrapping_sub(self.w(FUEL)); // SUBTRACT OFF REMAINING FUEL.
        if !minus(self.ret2) {
            // HE'S GOT NONE LEFT.
            self.at(blk::ROCKET_6)?;
            self.ret2 = 0;
            self.set(LPSW, DISTOP); // STOP THE LOW FUEL MESSAGE NOW.
        } else {
            self.at(blk::FUELOK)?;
            self.ret2 = self.ret2.wrapping_neg(); // GET FUEL LEFT
            if self.w(LPSW) != 0 {
                // FUEL NOT ALREADY LOW.
                self.at(blk::FUELOK_1)?;
                if (self.ret2 as i16) < 2000 {
                    // UNDER 200 POUNDS: CLEAR THE SWITCH, RING THE BELL ONCE.
                    self.at(blk::FUELOK_2)?;
                    self.set(LPSW, 0);
                    self.at(blk::FUELOK_3)?;
                    let v = self.w(DSR);
                    self.set(DSR, v);
                }
            }
        }
        self.at(blk::FUELKO)?;
        self.set(FUEL, self.ret2);
        self.ret1 = 0; // DIVIDE BY 10 TO CONVERT TENTHS OF POUNDS TO POUNDS.
        self.in1 = 10;
        self.call(blk::FUELKO_1, Self::divtwo)?;
        self.ret2 = self.ret2.wrapping_add(EMPTY); // ADD IN EMPTY WEIGHT OF SHIP.
        self.set(WEIGHT, self.ret2);
        self.in1 = self.w(THRUST); // FIGURE OUT ACCELERATION NOW.
        self.in2 = G1;
        self.call(blk::FUELKO_2, Self::multwo)?;
        // "NOW DIVIDE BY WEIGHT." The weight is in RET2 and WEIGHT; TEMP is
        // whatever the caller left there, which is the end of the ship's
        // drawing list (journal entry 007). Kept as written.
        self.in1 = self.temp;
        self.call(blk::FUELKO_3, Self::divtwo)?;
        self.set(ACCEL, self.ret2); // A=TRS*G/W
        self.in1 = self.ret2;
        self.in2 = self.w(SINANG); // HORIZONTAL ACCELERATION.
        self.call(blk::FUELKO_4, Self::trgmul)?;
        self.set(HORACC, self.ret1);
        self.in1 = self.w(ACCEL); // AND THE VERTICAL ACCELERATION.
        self.in2 = self.w(COSANG);
        self.call(blk::FUELKO_5, Self::trgmul)?;
        self.ret1 = self.ret1.wrapping_sub(G2); // DON'T FORGET ABOUT THE LUNAR GRAVITY.
        self.set(VERACC, self.ret1);
        Ok(())
    }

    /// ROCKOF: out of fuel, the engine stops.
    fn rockof(&mut self) -> Flow {
        self.at(blk::ROCKOF)?;
        self.set(LPSW, DISTOP); // TURN OFF LOW FUEL MESSAGE WHEN OUT.
        self.set(THRUST, 0);
        self.set(WEIGHT, EMPTY); // UPDATE WEIGHT OF THE ROCKET NOW.
        let w = self.w(WEIGHT).wrapping_add(self.w(FUEL));
        self.set(WEIGHT, w);
        self.set(ACCEL, 0);
        self.set(HORACC, 0);
        self.set(VERACC, G2.wrapping_neg()); // SET ROCKET TO FREE FALL.
        Ok(())
    }

    /// FLAME: "THIS ROUTINE WILL DRAW THE THRUST OF THE ROCKET, BASED ON
    /// THE AMOUNT OF FUEL THAT IS BEING CONSUMED." IN1 says where to leave
    /// it. The flame's length comes from YTHRST by throttle, made to flicker
    /// by YUPDWN; its foot wanders across FLAMBT; its line type and
    /// intensity cycle every frame.
    pub(crate) fn flame(&mut self) -> Flow {
        self.at(blk::FLAME)?;
        self.in2 = self.in1; // WHERE TO LEAVE FLAME.
        self.temp = asr(asr(asr(self.w(PERTRS)))); // DIVIDE BY EIGHT FOR THE TABLE LOOKUP.
        self.ret1 = self.bs(YTHRST.wrapping_add(self.temp)); // THE Y LENGTH.
        let frand = self.w(FRAND).wrapping_add(1); // NEXT FLAME DISPLACEMENT.
        self.set(FRAND, frand);
        self.ret2 = frand & 0o37; // JUST THIRTY TWO VALUES POSSIBLE.
        self.ret2 = self.bs(YUPDWN.wrapping_add(self.ret2));
        self.ret1 = self.ret1.wrapping_add(self.ret2);
        let fshift = self.w(FSHIFT).wrapping_add(self.ret1); // HORIZONTAL FLICKER.
        self.set(FSHIFT, fshift);
        self.ret2 = fshift & 3; // JUST 0 TO 3.
        self.temp = FLEN; // NUMBER OF POINTS TO DO.
        self.temp2 = FLAMXS; // WHERE TO INSERT.
        self.ret2 = self.ret2.wrapping_add(FLAMBT); // THE BYTE X TABLE.
        loop {
            self.at(blk::FLAMLP)?;
            let x = self.b(self.ret2); // INSERT X VALUE NOW.
            self.ret2 = self.ret2.wrapping_add(1);
            self.setb(self.temp2, x);
            self.temp2 = self.temp2.wrapping_add(1);
            self.setb(self.temp2, self.ret1 as u8); // INSERT THE Y VALUE.
            self.temp2 = self.temp2.wrapping_add(1);
            self.temp2 = self.temp2.wrapping_add(6); // UPDATE PAST COMMANDS.
            let more = dec_bgt(self.temp); // AND LOOP TILL DONE.
            self.temp = self.temp.wrapping_sub(1);
            if !more {
                break;
            }
        }
        self.at(blk::FLAMLP_1)?;
        let fline = self.w(FLINE).wrapping_add(1) & 3; // THE LINE TYPE.
        self.set(FLINE, fline);
        let fint = self.w(FINT).wrapping_add(0o600) & !0o176177; // AND THE INTENSITY.
        self.set(FINT, fint);
        self.set(FLAMEX, 0o106124); // SET UP PROTOTYPE COMMAND.
        let v = self.w(FLAMEX) | self.w(FLINE); // AND MOVE OVER THE DATA.
        self.set(FLAMEX, v);
        let v = self.w(FLAMEX) | self.w(FINT);
        self.set(FLAMEX, v);
        self.in1 = FLAMDO; // FINALLY SET UP THE POINTER.
        self.draw() // JMP DRAW: DRAW THE FIGURE, AND RETURN.
    }

    /// TRIG: "THIS ROUTINE GETS THE SINE AND COSINE FROM THE TABLES." Also
    /// puts ANGLE back in -180..180, nearly: the loop that should bring a
    /// large angle down repeats only while the result is negative, so an
    /// angle of 360 or more comes out reduced once and left as it is.
    pub(crate) fn trig(&mut self) -> Flow {
        self.at(blk::TRIG)?;
        self.in1 = self.w(ANGLE);
        if !minus(self.in1) {
            loop {
                // TRIG2: POSITIVE. SEE IF IT'S >180.
                self.at(blk::TRIG2)?;
                if self.in1 as i16 <= 180 {
                    break;
                }
                self.at(blk::TRIG2_1)?;
                self.in1 = self.in1.wrapping_sub(360); // DECREMENT BY ONE REVOLUTION
                if !minus(self.in1) {
                    break;
                }
            }
        } else {
            loop {
                // TRIG1: NEGATIVE. SEE IF BELOW -180.
                self.at(blk::TRIG1)?;
                if self.in1 as i16 > -180 {
                    break;
                }
                self.at(blk::TRIG1_1)?;
                self.in1 = self.in1.wrapping_add(360); // TOO SMALL. FUDGE IT AND LOOP.
                if !minus(self.in1) {
                    break;
                }
                self.at(blk::TRIG1_2)?;
            }
        }
        self.at(blk::TRIG3)?;
        self.set(ANGLE, self.in1); // AND NOW SAVE IT AWAY AGAIN.
        if minus(self.in1) {
            self.at(blk::TRIG3_1)?;
            self.in1 = self.in1.wrapping_add(360); // CONVERT TO A GOOD COMPASS NOTATION.
        }
        self.at(blk::TRIG4)?;
        self.in1 <<= 1;
        let s = self.w(SINTAB.wrapping_add(self.in1));
        self.set(SINANG, s);
        if self.in1 as i16 >= 540 {
            // GREATER THAN 269 DEGREES: WRAP IT AROUND.
            self.at(blk::TRIG4_1)?;
            self.in1 = self.in1.wrapping_sub(720);
        }
        self.at(blk::TRIG5)?;
        let c = self.w(COSTAB.wrapping_add(self.in1));
        self.set(COSANG, c);
        Ok(())
    }

    /// WHERE: "THIS ROUTINE FIGURES OUT WHERE THE SHIP IS IN RELATIONSHIP
    /// TO THE MOON." For each axis: v1 = v0 + a*t; the distance moves by
    /// (v0 + a*t/2) * t, with the remainder of that division carried to the
    /// next step so that nothing is lost to rounding.
    pub(crate) fn r#where(&mut self) -> Flow {
        self.at(blk::WHERE)?;
        self.in2 = self.w(HORACC); // PICK UP THE X ACCEL.
        if minus(self.in2) {
            self.at(blk::WHERE_1)?;
            self.in2 = self.in2.wrapping_neg(); // NEGATE IT FOR A SECOND.
        }
        self.at(blk::WH1)?;
        self.in1 = self.w(CLOCK); // ELAPSED TIME.
        self.call(blk::WH1_1, Self::multwo)?;
        self.in1 = CLKFG2; // TIMER FUDGE.
        self.call(blk::WH1_2, Self::divtwo)?;
        if minus(self.w(HORACC)) {
            self.at(blk::WH1_3)?;
            self.ret2 = self.ret2.wrapping_neg();
        }
        self.at(blk::WH2)?;
        self.ret1 = self.ret2; // A*T
        self.ret2 = asr(self.ret2).wrapping_add(self.w(HORVEL)); // V0+.5*A*T
        let v = self.w(HORVEL).wrapping_add(self.ret1); // UPDATE THE HORIZONTAL VELOCITY.
        self.set(HORVEL, v);
        self.push(self.ret2); // SAVE SIGN OF VELOCITY FOR TEST LATER.
        self.in2 = self.ret2; // VEL IN FPS*10
        if minus(self.in2) {
            self.at(blk::WH2_1)?;
            self.in2 = self.in2.wrapping_neg();
        }
        self.at(blk::WH3)?;
        self.in1 = self.w(CLOCK); // NOW GET THE DISTANCE COVERED.
        self.call(blk::WH3_1, Self::multwo)?;
        self.in1 = CLKFG3;
        // BRING BACK THE REMAINDER FROM PREVIOUS OPERATIONS, AND THE CARRY.
        let (lo, c) = self.ret2.overflowing_add(self.w(HORREM));
        self.ret2 = lo;
        self.ret1 = self.ret1.wrapping_add(c as u16);
        self.call(blk::WH3_2, Self::divtwo)?; // ACTUAL DISTANCE COVERED.
        if minus(self.pop()) {
            // RECALL OLD SIGN OF VELOCITY.
            self.at(blk::WH3_3)?;
            self.ret2 = self.ret2.wrapping_neg();
        }
        self.at(blk::WH4)?;
        let d = self.w(HORDIS).wrapping_add(self.ret2); // UPDATE THE DISTANCE NOW.
        self.set(HORDIS, d);
        self.set(HORREM, self.ret1); // AND SAVE THE REMAINDER NOW.
        self.in2 = self.w(VERACC); // NOW DO THE UP AND DOWN PART.
        if minus(self.in2) {
            self.at(blk::WH4_1)?;
            self.in2 = self.in2.wrapping_neg();
        }
        self.at(blk::WH5)?;
        self.in1 = self.w(CLOCK);
        self.call(blk::WH5_1, Self::multwo)?;
        self.in1 = CLKFG2;
        self.call(blk::WH5_2, Self::divtwo)?;
        if minus(self.w(VERACC)) {
            self.at(blk::WH5_3)?;
            self.ret2 = self.ret2.wrapping_neg();
        }
        self.at(blk::WH6)?;
        self.ret1 = self.ret2;
        self.ret2 = asr(self.ret2).wrapping_add(self.w(VERVEL));
        let v = self.w(VERVEL).wrapping_add(self.ret1);
        self.set(VERVEL, v);
        self.push(self.ret2); // SAVE SIGN FOR LATER USE.
        self.in2 = self.ret2;
        if minus(self.in2) {
            self.at(blk::WH6_1)?;
            self.in2 = self.in2.wrapping_neg();
        }
        self.at(blk::WH7)?;
        self.in1 = self.w(CLOCK);
        self.call(blk::WH7_1, Self::multwo)?;
        self.in1 = CLKFG3;
        let (lo, c) = self.ret2.overflowing_add(self.w(VERREM)); // ADD IN PREVIOUS REMAINDER.
        self.ret2 = lo;
        self.ret1 = self.ret1.wrapping_add(c as u16);
        self.call(blk::WH7_2, Self::divtwo)?;
        if minus(self.pop()) {
            self.at(blk::WH7_3)?;
            self.ret2 = self.ret2.wrapping_neg();
        }
        self.at(blk::WH8)?;
        let d = self.w(VERDIS).wrapping_add(self.ret2);
        self.set(VERDIS, d);
        self.set(VERREM, self.ret1); // AND SAVE THE REMAINDER NOW.
        Ok(())
    }
}
