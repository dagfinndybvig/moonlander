//! One step of the game: SHOWSP ("THIS SECTION WILL FIGURE OUT THE POSITION
//! OF THE LUNAR MODULE AND WHICH VIEW TO USE"), the close-up view CLSEUP,
//! INTEL ("FIGURE OUT IF ANYTHING GREAT IS TO HAPPEN"): the landing, the
//! crashes and the features underneath, and DUST.
//!
//! SHOWSP is called from EIDLE each time the clock has ticked, with CLOCK
//! holding how many ticks have passed since the last step.

use super::{asr, dec_bgt, minus};
use crate::generated::blocks as blk;
use crate::generated::symbols::*;
use crate::machine::{Escape, Flow, Moonlander};

/// Where SHOWSP's position checks send the ship.
enum View {
    /// Off an edge of the world: OFFCOM with this X (in 32-foot raster
    /// units) and message.
    Off(u16, u16),
    /// Low enough for the close-up.
    Closeup,
    /// The far view of the whole moon.
    Far,
}

/// INTEL's landing verdicts.
enum Verdict {
    /// Look at what is underneath (AHAHC).
    Features,
    /// DEAD: crashed.
    Dead,
    /// Down, with this message: MAYBE checks the attitude and the ground.
    Down(u16),
}

impl Moonlander {
    pub(crate) fn showsp(&mut self) -> Flow {
        loop {
            self.at(blk::SHOWSP)?;
            self.set(LOWY, (-64i16) as u16); // SET UP ABORT LIMIT OUT OF RANGE.
            self.temp2 = self.w(TURN); // PICK UP THE RATE OF TURN.
            self.set(TURN, 0); // AND TURN IT OFF NOW.
            self.in1 = self.w(CLOCK); // THE NUMBER OF CLOCK TICKS.
            self.in2 = self.temp2;
            if minus(self.in2) {
                self.at(blk::SHOWSP_1)?;
                self.in2 = self.in2.wrapping_neg();
            }
            self.at(blk::SHOWP1)?;
            self.call(blk::SHOWP1_1, Self::multwo)?; // MULTIPLY THEM TOGETHER.
            self.in1 = CLKFRQ; // THEN GET ACTUAL # OF DEGREES OF TURN.
            let (lo, c) = self.ret2.overflowing_add(self.w(ANGLER)); // ADD IN THE REMAINDER,
            self.ret2 = lo;
            self.ret1 = self.ret1.wrapping_add(c as u16); // AND THE CARRY, ALSO.
            self.call(blk::SHOWP1_2, Self::divtwo)?;
            if minus(self.temp2) {
                self.at(blk::SHOWP1_3)?;
                self.ret2 = self.ret2.wrapping_neg(); // SET TO THE CORRECT SIGN.
            }
            self.at(blk::SHOWP2)?;
            let a = self.w(ANGLE).wrapping_add(self.ret2); // UPDATE THE ANGLE NOW.
            self.set(ANGLE, a);
            self.set(ANGLER, self.ret1); // SAVE THE REMAINDER NOW.
            self.call(blk::SHOWP2_1, Self::trig)?; // NEW SINES AND COSINES.
            // Draw the ship into whichever of the two buffers is not being
            // displayed.
            let t = self.w(SHIPTP).wrapping_add(2);
            self.set(SHIPTP, t);
            self.in2 = t & 2;
            self.in2 = self.w(SHIPLC.wrapping_add(self.in2));
            self.in1 = DESIGN; // SET UP SHIP'S DESIGN.
            self.push(self.in2); // REMEMBER THE BUFFER FOR LATER USE.
            self.call(blk::SHOWP2_2, Self::draw)?; // AND DRAW IT NOW.
            let b = self.pop();
            self.set(SHIPDP, b); // MOVE OVER BUFFER POINTER TO CALL.
            let low = self.w(LOWEST);
            self.set(SAVEY, low); // SAVE THE LOWEST Y DRAWN.
            // SHOWNT. DRAW leaves TEMP pointing past the ship's design list,
            // and ROCKET divides by it.
            self.at(blk::SHOWNT)?;
            let p = self.w(PERCNT);
            self.set(PERTRS, p); // THE PRESENT FUEL SETTING.
            self.call(blk::SHOWNT_1, Self::rocket)?;
            self.call(blk::SHOWNT_2, Self::r#where)?;
            // 32 FEET PER RASTOR ON BIG SCALE.
            self.temp = self.w(HORDIS).wrapping_add(22400); // FUDGE UP DOWNRANGE CALCULATION.
            for _ in 0..5 {
                self.temp = asr(self.temp);
            }
            self.set(BIGXCT, self.temp);
            self.temp2 = self.temp;
            // NOW GET THE CURRENT RADAR HEIGHT.
            self.temp = (self.temp << 1).wrapping_add(TERAIN);
            self.in1 = self.w(self.temp); // LEFT TERAIN.
            self.temp = self.temp.wrapping_add(2);
            self.in1 = self.in1.wrapping_add(self.w(self.temp)); // RIGHT TERAIN.
            self.in1 = asr(self.in1); // AVERAGE IT OUT.
            self.set(AVERY, self.in1);
            self.in1 = self.in1.wrapping_sub(self.w(VERDIS)).wrapping_neg();
            self.set(RADARY, self.in1); // RADAR HEIGHT IS NOW CALCULATED.
            if self.w(THRUST) as i16 > 0 {
                self.at(blk::SHOWNT_3)?;
                self.in1 = FLAMIN; // WHERE TO LEAVE THE FLAME.
                self.call(blk::SHOWNT_4, Self::flame)?;
                self.set(FSUBC, FLAMIN); // AND TURN ON THE FLAME AGAIN.
            } else {
                self.at(blk::NOFLAM)?;
                self.set(FSUBC, 0); // NO THRUST=NO FLAME.
            }
            let view = self.where_to_look()?;
            match view {
                View::Off(x, msg) => {
                    self.offcom(x, msg)?;
                    continue; // JMP SHOWSP
                }
                View::Closeup => return self.clseup(),
                View::Far => return self.farview(),
            }
        }
    }

    /// YSFLAM: decide between the far view, the close-up, and falling off
    /// the world.
    fn where_to_look(&mut self) -> Flow<View> {
        self.at(blk::YSFLAM)?;
        self.temp = self.w(BIGXCT); // PICK UP X POSITION NOW.
        if minus(self.temp) {
            return self.offlft();
        }
        self.at(blk::YSFLAM_1)?;
        if self.temp as i16 <= 10 {
            return self.offlft(); // WE ARE STILL OFF TO THE LEFT.
        }
        self.at(blk::YSFLAM_2)?;
        if self.temp as i16 >= 890 {
            // TOO FAR TO THE RIGHT.
            self.at(blk::OFFRGT)?;
            self.in1 = 887;
            self.in2 = RGTMSG;
            return Ok(View::Off(self.in1, self.in2));
        }
        self.at(blk::YSFLAM_3)?;
        self.temp = self.w(VERDIS); // HOW ABOUT HEIGHT.
        if minus(self.temp) {
            return Ok(View::Closeup);
        }
        self.at(blk::YSFLAM_4)?;
        if self.temp as i16 >= 25000 {
            // OFF THE TOP. "PRESENT X VALUE", says the comment; TEMP holds
            // the height, so the ship comes back at a distance worked out
            // from its height.
            self.at(blk::OFFTOP)?;
            self.in1 = self.temp;
            self.in2 = TOPMSG;
            return Ok(View::Off(self.in1, self.in2));
        }
        self.at(blk::YSFLAM_5)?;
        if self.temp as i16 <= 450 {
            return Ok(View::Closeup); // YES. MAGNIFY.
        }
        Ok(View::Far)
    }

    fn offlft(&mut self) -> Flow<View> {
        self.at(blk::OFFLFT)?;
        self.in1 = 13; // PICK UP A NEW X NOW.
        self.in2 = LFTMSG;
        Ok(View::Off(self.in1, self.in2))
    }

    /// OFFCOM: put the ship back over the edge it left by, with a message,
    /// no fuel, no sideways motion, and falling.
    fn offcom(&mut self, x: u16, msg: u16) -> Flow {
        self.at(blk::OFFCOM)?;
        self.in1 = (x << 5).wrapping_sub(22400); // GET A NEW HORDISTANCE.
        self.set(HORDIS, self.in1);
        self.set(SYSMES, msg);
        self.set(FUEL, 0); // TURN OFF FUEL NOW.
        self.set(HORVEL, 0); // CLEAR SIDEWAYS MOTION.
        self.ret2 = asr(asr(self.w(VERDIS))); // DIVIDE BY 4 TO GET A REASONABLE SPEED.
        if !minus(self.ret2) {
            self.at(blk::OFFCOM_1)?;
            self.ret2 = self.ret2.wrapping_neg(); // ELSE MAKE US GO DOWN.
        }
        self.at(blk::OFFCMD)?;
        self.set(VERVEL, self.ret2);
        Ok(())
    }

    /// The far view: the whole moon, 32 feet to a raster unit.
    fn farview(&mut self) -> Flow {
        self.at(blk::YSFLAM_6)?;
        let x = self.w(BIGXCT);
        self.set(SHOWX, x);
        for _ in 0..5 {
            self.temp = asr(self.temp); // SCALE DOWN THE VERTICAL DISTANCE.
        }
        self.temp = self.temp.wrapping_add(43);
        self.set(SHOWY, self.temp);
        if self.w(MOON) != 0 {
            // NOT DRAWN.
            self.at(blk::YSFLAM_7)?;
            self.call(blk::YSFLAM_8, Self::drawm1)?;
            self.set(MOON, 0); // RESET FLAG.
        }
        self.at(blk::SHWMN1)?;
        self.call(blk::SHWMN1_1, Self::dial)?; // DISPLAY THE DIALS NOW.
        self.temp = self.w(RADARY).wrapping_sub(16); // FIX FOR CENTER OF THE SHIP.
        if minus(self.temp) {
            // WE'VE CRASHED. BIG DISTORTION.
            self.at(blk::SHWMN1_2)?;
            self.in1 = 640;
            self.call(blk::SHWMN1_3, Self::alter)?;
            self.set(MOON, self.sp); // INDICATE THAT WE'VE ALTERED THE MOON.
            self.call(blk::SHWMN1_4, Self::drawm1)?;
            self.set(MOON, 0); // REMEMBER THAT WE'VE REDRAWN IT.
            self.jsr(blk::SHWOUT, Self::explod)?;
            return Err(Escape::Wild(blk::SHWOUT.addr)); // EXPLOD restarts the game.
        }
        self.at(blk::SHWOUT) // RTS
    }

    /// CLSEUP: "THIS ROUTINE WILL DRAW THE CLOSEUP PICTURES OF THE MOON, WHEN
    /// THEY BECOME NECESSARY. RETURN GOES BACK TO IDLE VIA THE OLD PC ON THE
    /// STACK <OR IT SHOULD, ANYWAY.>." MOON holds that return address while
    /// the close-up is on the screen.
    fn clseup(&mut self) -> Flow {
        self.at(blk::CLSEUP)?;
        let ret = self.top();
        let redraw = if self.w(MOON) != ret {
            // NOT IN SIGHT. GET CURRENT RASTOR POSITION AND SET A GOOD LEFT EDGE.
            self.at(blk::CLSEUP_1)?;
            self.in1 = self.w(BIGXCT).wrapping_sub(9);
            true
        } else {
            // MOON ALREADY BEING DISPLAYED. SEE IF IT'S STILL ON THE SCREEN.
            self.at(blk::CLSEC1)?;
            self.in1 = self.w(HORDIS).wrapping_sub(self.w(LEFEET));
            if self.in1 as i16 <= 30 {
                // TOO CLOSE TO LEFT. SHIFT EVERYTHING RIGHT.
                self.at(blk::CLOL)?;
                self.in1 = self.w(BIGXCT).wrapping_sub(17);
                true
            } else {
                self.at(blk::CLSEC1_1)?;
                if (self.in1 as i16) < 580 {
                    false // DISPLAY AS IS.
                } else {
                    // SHIFT EVERYTHING LEFT NOW.
                    self.at(blk::CLSEC1_2)?;
                    self.in1 = self.w(BIGXCT).wrapping_sub(1);
                    true
                }
            }
        };
        if redraw {
            self.at(blk::CLSEFG)?;
            self.set(LEFTED, self.in1); // LEFT EDGE ESTABLISHED NOW.
            self.in1 = (self.in1 << 5).wrapping_sub(22400); // CONVERT EDGE TO FEET.
            self.set(LEFEET, self.in1);
            self.call(blk::CLSEFG_1, Self::drawm2)?; // DRAW A CLOSE-UP OF THE MOON.
            let ret = self.top();
            self.set(MOON, ret); // SET FLAG TO INDICATE WE HAVE DONE SO.
            self.in1 = self.w(HORDIS).wrapping_sub(self.w(LEFEET)); // FEET FROM LEFT EDGE.
        }
        // CLSEOK: 3/2 OF FEET=RASTOR<X>
        self.at(blk::CLSEOK)?;
        self.ret2 = self.in1;
        self.in1 <<= 1;
        self.ret2 = asr(self.ret2.wrapping_add(self.in1));
        self.set(SHOWX, self.ret2);
        self.ret1 = 0; // NOW CONVERT BACK TO A TERAIN INDEX.
        self.in1 = 48;
        self.call(blk::CLSEOK_1, Self::divtwo)?;
        self.ret2 = self.ret2.wrapping_add(self.w(LEFTED)); // A TERAIN AND FEATURE INDEX.
        self.set(INDEXL, self.ret2); // INDEX TO LEFT POSITION OF SHIP.
        self.ret2 <<= 1;
        self.temp = self.ret2;
        // HOW MUCH SHIP IS TO LEFT OR RIGHT OF CENTER OF TERAIN: THE
        // DIVISION REMAINDER WEIGHTS THE TWO HEIGHTS AROUND IT.
        self.in1 = 48u16.wrapping_sub(self.ret1);
        self.temp2 = self.ret1;
        self.in2 = self.w(TERAIN.wrapping_add(self.temp));
        self.call(blk::CLSEOK_2, Self::sgnmul)?;
        self.push(self.ret2);
        self.in1 = self.temp2; // DO RIGHT EDGE OF "TERRAIN".
        self.in2 = self.w((TERAIN + 2).wrapping_add(self.temp));
        self.call(blk::CLSEOK_3, Self::sgnmul)?;
        self.ret1 = 0; // TERAIN HEIGHT*48 TO DISPLAY FUDGED HEIGHT.
        self.in1 = 48;
        let v = self.pop();
        self.ret2 = self.ret2.wrapping_add(v); // OLD FUDGED WEIGHTED HEIGHT.
        if !minus(self.ret2) {
            self.at(blk::CLSEF1)?;
            self.call(blk::CLSEF2, Self::divtwo)?;
        } else {
            self.at(blk::CLSEOK_4)?;
            self.ret2 = self.ret2.wrapping_neg(); // AVERAGE IS NEGATIVE.
            self.call(blk::CLSEOK_5, Self::divtwo)?;
            self.ret2 = self.ret2.wrapping_neg();
            self.at(blk::CLSEF2)?;
        }
        self.temp = self.ret2; // HEIGHT FOR DFAKE TO USE.
        self.ret2 = asr(asr(self.ret2));
        self.set(AVERY, self.ret2); // AVERAGE TERAIN HEIGHT.
        self.call(blk::CLSEF2_1, Self::dfake)?; // ACTUAL RASTER HEIGHT ABOVE THE MOON.
        self.set(AVERT, self.temp);
        self.in1 = self.w(VERDIS);
        self.ret2 = self.in1; // CONVERT TO RASTORS: THE OLD 3/2 GAMBIT.
        self.ret2 <<= 1;
        self.ret2 = asr(self.ret2.wrapping_add(self.in1));
        self.ret2 = self.ret2.wrapping_add(23); // TO CONVERT TO VIEWING RASTORS.
        self.set(SHOWY, self.ret2);
        let y = self.w(SHOWY).wrapping_add(24); // EXCEPT FOR THE FUDGE.
        self.set(SHOWY, y);
        self.ret2 = self.ret2.wrapping_sub(self.temp); // # OF RASTORS TO GO.
        self.temp = self.ret2;
        if minus(self.ret2) {
            self.at(blk::CLSEF2_2)?;
            self.ret2 = self.ret2.wrapping_neg();
        }
        self.at(blk::CLSEF3)?;
        self.ret1 = 0;
        self.in1 = 3; // MAKE IT THREE HALFS.
        self.ret2 <<= 1;
        self.call(blk::CLSEF3_1, Self::divtwo)?;
        if minus(self.temp) {
            self.at(blk::CLSEF3_2)?;
            self.ret2 = self.ret2.wrapping_neg(); // THE ORIGINAL SIGN.
        }
        self.at(blk::CLSEF4)?;
        self.set(RADARY, self.ret2); // THIS IS THE RADAR HEIGHT.
        self.call(blk::CLSEF4_1, Self::intel)?;
        self.call(blk::AHNONE, Self::dust)?;
        Ok(()) // AHNONE: RTS
    }

    /// INTEL: speed warnings while descending, and the verdict on contact.
    pub(crate) fn intel(&mut self) -> Flow {
        self.at(blk::INTEL)?;
        self.call(blk::INTEL_1, Self::dial)?;
        self.temp2 = self.w(RADARY); // PICK UP ALTITUDE NOW.
        let verdict = if minus(self.temp2) {
            self.at(blk::INTELM)?;
            if self.temp2 as i16 <= -10 {
                Verdict::Dead // TOO FAR DOWN. HE'S DEAD.
            } else {
                self.verylo()?
            }
        } else {
            self.at(blk::INTEL_2)?;
            if self.temp2 as i16 <= 3 {
                self.verylo()? // WE CERTAINLY ARE CLOSE.
            } else {
                self.speed_warning()?;
                Verdict::Features
            }
        };
        match verdict {
            Verdict::Features => self.ahahc(),
            Verdict::Dead => self.dead(),
            Verdict::Down(msg) => self.maybe(msg),
        }
    }

    /// INTEL_3 to INTEL_7: a message for coming down too fast.
    fn speed_warning(&mut self) -> Flow {
        self.at(blk::INTEL_3)?;
        self.temp = self.w(VERVEL); // GET VELOCITY NOW.
        if (self.temp as i16) < -600 {
            self.at(blk::AHAH)?;
            self.set(SYSMES, VFAST);
            return Ok(());
        }
        self.at(blk::INTEL_4)?;
        if (self.temp as i16) < -300 {
            self.at(blk::AHAH2)?;
            self.set(SYSMES, FAST);
            return Ok(());
        }
        self.at(blk::INTEL_5)?;
        if (self.temp as i16) < -150 {
            self.at(blk::AHAH3)?;
            self.set(SYSMES, N2FAST); // A "TAKE IT EASY MESSAGE".
            return Ok(());
        }
        self.at(blk::INTEL_6)?;
        if self.w(SYSMES) == N2FAST {
            self.at(blk::INTEL_7)?;
            self.set(SYSMES, 0); // DELETE IT NOW.
        }
        Ok(())
    }

    /// VERYLO: within three feet: engine off. VD: on the ground; how hard?
    fn verylo(&mut self) -> Flow<Verdict> {
        self.at(blk::VERYLO)?;
        self.set(PERCNT, 0); // TURN OFF FUEL NOW.
        self.set(LPBARY, BARBY); // AND INDICATE IT TO HIM.
        self.set(FSUBC, 0); // TERMINATE FLAMES.
        self.set(DUSTON, 0); // AND THE DUST.
        if self.temp2 != 0 {
            self.at(blk::VERYLO_1)?;
            if !minus(self.temp2) {
                return Ok(Verdict::Features); // POSITIVE: LOOK FOR FEATURES.
            }
        }
        self.at(blk::VD)?;
        self.temp = self.w(VERVEL);
        let v = self.temp as i16;
        if v <= -600 {
            return Ok(Verdict::Dead); // GREATER THAN 60 FPS. SPLAT
        }
        self.at(blk::VD_1)?;
        if v <= -300 {
            self.at(blk::CRIPPL)?;
            self.in1 = CRIPM;
            return Ok(Verdict::Down(self.in1));
        }
        self.at(blk::VD_2)?;
        if v <= -150 {
            self.at(blk::ROUGH)?;
            self.in1 = ROUGHM;
            return Ok(Verdict::Down(self.in1));
        }
        self.at(blk::VD_3)?;
        if v <= -80 {
            self.at(blk::GOOD)?;
            self.in1 = GOODM;
            return Ok(Verdict::Down(self.in1));
        }
        self.at(blk::VD_4)?;
        self.in1 = GREATM; // UNDER 8 FPS.
        Ok(Verdict::Down(self.in1))
    }

    /// AHAHC: what is underneath? GETFET looks it up in the feature table
    /// and AHTAB says what to do about it.
    fn ahahc(&mut self) -> Flow {
        self.at(blk::AHAHC)?;
        self.in1 = self.w(INDEXL); // PICK UP FEATURE INDEX NOW.
        self.push(self.in1);
        self.call(blk::AHAHC_1, Self::getfet)?;
        self.in2 = self.pop() << 1;
        match self.w(AHTAB.wrapping_add(self.in2)) {
            AHNONE => self.at(blk::AHNONE),
            AHSP1 => self.ahsp1(),
            AHFLG => self.ahflg(),
            AHOLDS => self.aholds(),
            AHROCK => self.ahrock(),
            AHMAC => self.ahmac(),
            other => Err(Escape::Wild(other)),
        }
    }

    /// AHMAC: over the McDonald's.
    fn ahmac(&mut self) -> Flow {
        self.at(blk::AHMAC)?;
        // TST #MACON: the flag is this instruction's own operand.
        if self.w(MACON) == 0 {
            return self.at(blk::AHNONE); // NOT DRAWN. EXIT NOW.
        }
        self.at(blk::AHMAC_1)?;
        if self.temp2 as i16 > 30 {
            return self.at(blk::AHNONE); // NOT TOO CLOSE. ALL IS WELL.
        }
        self.at(blk::AHMAC_2)?;
        // CLR #MACTHR: TOO CLOSE. WE'VE CRASHED INTO IT. The McDonald's is
        // gone until the program is loaded again.
        self.set(MACTHR, 0);
        self.set(SYSMES, MACDED);
        self.quick()
    }

    /// AHROCK: over a boulder.
    fn ahrock(&mut self) -> Flow {
        self.at(blk::AHROCK)?;
        if self.temp2 as i16 > 15 {
            return self.at(blk::AHNONE); // NOT TOO LOW OVER THE ROCK.
        }
        self.at(blk::AHROCK_1)?;
        if self.w(VERVEL) as i16 <= -600 {
            return self.godead(true); // GREATER THAN CRASH SPEED.
        }
        self.at(blk::AHROCK_2)?;
        self.set(SYSMES, ROCKMS);
        self.set(SHIPDP, 0); // STOP DISPLAYING THE SHIP NOW.
        self.jsr(blk::AHSP1, Self::explod)?;
        Err(Escape::Wild(blk::AHSP1.addr))
    }

    /// AHSP1: over a ship landed in an earlier game.
    fn ahsp1(&mut self) -> Flow {
        self.at(blk::AHSP1)?;
        if self.temp2 as i16 > 26 {
            return self.at(blk::AHNONE); // TOO HIGH: IT'S ALRIGHT.
        }
        self.at(blk::AHSP1_1)?;
        // TEMP is VERVEL when INTEL came by its speed checks; from VERYLO it
        // is what CLSEUP left in it.
        if self.temp as i16 <= -600 {
            return self.godead(true); // TOO FAST.
        }
        self.at(blk::AHSP1_2)?;
        self.set(SYSMES, OLDMS);
        self.push(3); // SET UP A LEFT SHIP
        if !minus(self.w(HORVEL)) {
            self.at(blk::AHSP1_3)?;
            let v = self.top().wrapping_add(1); // MAKE IT RIGHT TILTING.
            self.set_top(v);
        }
        self.at(blk::AHSP1M)?;
        self.push(self.in1); // PUSH THE INDEX ONTO THE STACK.
        self.call(blk::AHSP1M_1, Self::putfet)?;
        let y = self.w(SHOWY).wrapping_sub(16); // BRING DOWN THE OLD SHIP.
        self.set(SHOWY, y);
        self.call(blk::AHSP1M_2, Self::drawm2)?;
        self.jsr(blk::AHFLG, Self::explod)?;
        Err(Escape::Wild(blk::AHFLG.addr))
    }

    /// AHFLG: over a flag planted in an earlier game. With the engine on,
    /// the flag is vaporised.
    fn ahflg(&mut self) -> Flow {
        self.at(blk::AHFLG)?;
        if self.temp2 as i16 > 26 {
            return self.at(blk::AHNONE);
        }
        self.at(blk::AHFLG_1)?;
        if self.w(THRUST) == 0 {
            return self.at(blk::AHNONE); // ENGINE OFF.
        }
        self.at(blk::AHFLG_2)?;
        self.push(0);
        self.push(self.in1);
        self.call(blk::AHFLG_3, Self::putfet)?; // CLEAR INDICATOR FLAG NOW.
        self.set(SYSMES, FLAGMS);
        self.call(blk::AHFLG_4, Self::drawm2)?;
        self.at(blk::AHNONE) // AND CONTINUE CHECKING CRAP.
    }

    /// AHOLDS: over a ship crashed in an earlier game.
    fn aholds(&mut self) -> Flow {
        self.at(blk::AHOLDS)?;
        if self.temp2 as i16 > 17 {
            return self.at(blk::AHNONE);
        }
        self.at(blk::AHOLDS_1)?;
        let le = self.temp as i16 <= -600; // SEE IF TOO FAST.
        self.godead(le)
    }

    /// GODEAD is a BLE with a label on it. Branches from AHROCK and AHSP1
    /// arrive with the condition already true; AHOLDS falls into it with
    /// its own comparison. `le` is the condition the branch sees.
    fn godead(&mut self, le: bool) -> Flow {
        self.at(blk::GODEAD)?;
        if le {
            return self.dead();
        }
        self.at(blk::GODEAD_1)?;
        self.set(SYSMES, OLDTLT);
        self.jsr(blk::INTEL, Self::explod)?;
        Err(Escape::Wild(blk::INTEL.addr))
    }

    /// DEAD and QUICK: a crater, and the explosion.
    fn dead(&mut self) -> Flow {
        self.at(blk::DEAD)?;
        self.set(SYSMES, DEADM);
        self.quick()
    }

    fn quick(&mut self) -> Flow {
        self.at(blk::QUICK)?;
        self.in1 = 32; // ALTER THE SURFACE NOW.
        self.call(blk::QUICK_1, Self::alter)?;
        self.call(blk::QUICK_2, Self::drawm2)?;
        self.set(SHIPDP, 0); // STOP DISPLAYING THE SHIP.
        self.jsr(blk::MAYBE, Self::explod)?;
        Err(Escape::Wild(blk::MAYBE.addr))
    }

    /// MAYBE: down in one piece, so far. Too fast sideways, tilted, or on
    /// ground too steep, and the ship falls over; otherwise the man gets
    /// out.
    fn maybe(&mut self, msg: u16) -> Flow {
        self.at(blk::MAYBE)?;
        self.set(FSUBC, 0); // TURN OFF FLAME IMAGE NOW.
        self.set(SYSMES, msg); // DISPLAY THE CALLER'S MESSAGE NOW.
        if self.w(HORVEL) as i16 > 100 {
            return self.maybeb(); // TOO FAST SIDEWAYS.
        }
        self.at(blk::MAYBE_1)?;
        if (self.w(HORVEL) as i16) < -100 {
            return self.maybeb();
        }
        self.at(blk::MAYBE_2)?;
        if (self.w(ANGLE) as i16) < -15 {
            return self.maybec(); // TOO FAR LEFT.
        }
        self.at(blk::MAYBE_3)?;
        if self.w(ANGLE) as i16 > 15 {
            return self.maybec();
        }
        // NOW FIGURE OUT IF TERAIN IS TOO ROUGH.
        self.at(blk::MAYBE_4)?;
        self.in2 = self.w(INDEXL) << 1;
        self.in1 = self.w((TERAIN + 2).wrapping_add(self.in2)); // RIGHT TERAIN.
        self.in1 = self.in1.wrapping_sub(self.w(TERAIN.wrapping_add(self.in2))); // LEFT TERAIN.
        self.ret1 = self.in1;
        if minus(self.ret1) {
            self.at(blk::MAYBE_5)?;
            self.ret1 = self.ret1.wrapping_neg(); // ABSOLUTE VALUE.
        }
        self.at(blk::MAYBEP)?;
        if self.ret1 as i16 >= 48 {
            // A 48 FOOT DROP: A BADDY.
            self.at(blk::MAYBED)?;
            self.in2 = BUMPYM; // IN1=CORRECT SIGN.
            return self.tilt();
        }
        self.at(blk::MAYBEP_1)?;
        self.jsr(blk::MAYBEB, Self::palsy)?; // WITH THE SPASTIC MAN.
        Err(Escape::Wild(blk::MAYBEB.addr))
    }

    fn maybeb(&mut self) -> Flow {
        self.at(blk::MAYBEB)?;
        self.in1 = self.w(HORVEL); // DIRECTION TO TIP SHIP.
        self.in2 = SIDEM;
        self.tilt()
    }

    fn maybec(&mut self) -> Flow {
        self.at(blk::MAYBEC)?;
        self.in1 = self.w(ANGLE); // DIRECTION TO TILT THE SHIP.
        self.in2 = ANGLEM;
        self.tilt()
    }

    /// TILT: the ship falls over, the way IN1's sign says, onto the slope
    /// of the ground under it; it is remembered as a wreck.
    fn tilt(&mut self) -> Flow {
        self.at(blk::TILT)?;
        self.set(FSUBC, self.in2); // DISPLAY MESSAGE FROM FLAME POINT.
        self.in2 = self.w(INDEXL); // TERAIN INDEX AGAIN.
        self.push(3); // TIPPED TO THE LEFT.
        if !minus(self.in1) {
            self.at(blk::TILT_1)?;
            let v = self.top().wrapping_add(1);
            self.set_top(v);
        }
        self.at(blk::TILT1)?;
        self.push(self.in2); // UPDATE THE FEATURE TABLE.
        self.call(blk::TILT1_1, Self::putfet)?;
        // CONVERT FEET DIFFERENCE TO AN ANGLE.
        self.in2 <<= 1;
        self.ret1 = self.w((TERAIN + 2).wrapping_add(self.in2)); // RIGHT TERAIN.
        self.ret1 = self.ret1.wrapping_sub(self.w(TERAIN.wrapping_add(self.in2))); // LEFT.
        self.ret2 = self.ret1 << 1; // MULTIPLY IT BY 3
        self.ret2 = self.ret2.wrapping_add(self.ret1);
        self.ret2 = asr(asr(self.ret2)); // NOW DIVIDE BY FOUR.
        self.ret1 = self.ret1.wrapping_sub(self.ret2);
        self.ret2 = asr(self.ret1); // NOW HALF ANSWER AGAIN.
        self.ret1 = self.ret1.wrapping_add(self.ret2);
        if !minus(self.ret1) {
            self.at(blk::TILT2)?;
            if self.ret1 as i16 > 45 {
                self.at(blk::TILT2_1)?;
                self.ret1 = 45; // GREATER THAN 45 DEGREES.
            }
        } else {
            self.at(blk::TILT1_2)?;
            if (self.ret1 as i16) < -45 {
                self.at(blk::TILT1_3)?;
                self.ret1 = (-45i16) as u16;
            }
        }
        self.at(blk::TILT3)?;
        self.ret2 = 90; // FALLEN SHIP ANGLE.
        if minus(self.in1) {
            self.at(blk::TILT3_1)?;
            self.ret2 = self.ret2.wrapping_neg();
        }
        self.at(blk::TILT4)?;
        self.ret1 = self.ret1.wrapping_add(self.ret2); // THE NEW SHIP ANGLE.
        self.set(ANGLE, self.ret1);
        self.in2 = self.w(SHIPTP).wrapping_add(2) & 2; // TOGGLE SHIP SWITCH.
        self.set(SHIPTP, self.in2);
        self.in2 = self.w(SHIPLC.wrapping_add(self.in2));
        self.push(self.in2);
        self.call(blk::TILT4_1, Self::trig)?;
        self.in1 = DESIGN;
        self.call(blk::TILT4_2, Self::draw)?;
        let b = self.pop();
        self.set(SHIPDP, b); // SHOW NEW SHIP NOW.
        let y = self.w(SHOWY).wrapping_sub(7); // BRING IT DOWN A LITTLE BIT.
        self.set(SHOWY, y);
        self.call_delay(blk::TILT4_2, None) // DELAY 10, AND START AGAIN.
    }

    /// DUST: when the flame reaches the ground, dust flies from where it
    /// hits, more of it the lower and harder the engine is firing.
    pub(crate) fn dust(&mut self) -> Flow {
        self.at(blk::DUST)?;
        if self.w(RADARY) as i16 >= 150 {
            return self.nodust();
        }
        self.at(blk::DUST_1)?;
        self.temp2 = EXLIST; // WHERE TO LEAVE DUST.
        self.temp = self.w(PERTRS);
        let carry = self.temp < 63; // from CMP TEMP,#63.; MOV leaves it.
        if self.temp as i16 > 63 {
            self.at(blk::DUST_2)?;
            self.temp = 63; // SET TO 63% FOR SCALING.
        }
        self.at(blk::DUSTB1)?;
        // ROL x4: MAGNITUDE CONTROLS INTENSITY. The bits rotated in end up
        // below the intensity field and are cleared.
        let mut c = carry;
        for _ in 0..4 {
            let out = minus(self.temp);
            self.temp = (self.temp << 1) | c as u16;
            c = out;
        }
        self.temp &= !0o176177;
        self.temp |= 0o116120; // THE POSITIONING MASK.
        self.set(self.temp2, self.temp);
        self.temp2 = self.temp2.wrapping_add(2);
        if self.w(ANGLE) as i16 > 45 {
            return self.nodust(); // ANGLE TOO GREAT.
        }
        self.at(blk::DUSTB1_1)?;
        if (self.w(ANGLE) as i16) < -45 {
            return self.nodust();
        }
        // THE PLACE TO PUT THE DUST, BY THE TANGENT OF THE ANGLE.
        self.at(blk::DUSTB1_2)?;
        self.in2 = self.w(SINANG);
        if minus(self.in2) {
            self.at(blk::DUSTB1_3)?;
            self.in2 = self.in2.wrapping_neg();
        }
        self.at(blk::DUSTP1)?;
        self.in1 = self.w(SHOWY).wrapping_sub(self.w(AVERT)); // HEIGHT ABOVE THE TERAIN.
        self.temp = self.in1;
        self.call(blk::DUSTP1_1, Self::multwo)?;
        self.in1 = self.w(COSANG);
        self.call(blk::DUSTP1_2, Self::divtwo)?; // RET2=TAN(ANGLE)*DELTAY
        self.temp = self.temp.wrapping_add(self.ret2); // TOTAL FLAME DISTANCE.
        if !minus(self.w(SINANG)) {
            self.at(blk::DUSTP1_3)?;
            self.ret2 = self.ret2.wrapping_neg();
        }
        self.at(blk::DUSTP2)?;
        self.ret2 = self.ret2.wrapping_add(self.w(SHOWX)); // CENTER OF DUST NOW.
        self.set(self.temp2, self.ret2);
        self.temp2 = self.temp2.wrapping_add(2);
        let y = self.w(AVERT); // INSERT THE Y VALUE NOW.
        self.set(self.temp2, y);
        self.temp2 = self.temp2.wrapping_add(2);
        self.set(self.temp2, 0o130000); // RELATIVE POINT COMMAND.
        self.temp2 = self.temp2.wrapping_add(2);
        self.temp = self.temp.wrapping_sub(150); // DISTANCE TO GROUND.
        if !minus(self.temp) {
            return self.nodust(); // TOO FAR AWAY.
        }
        self.at(blk::DUSTP2_1)?;
        self.temp = self.temp.wrapping_neg();
        self.in1 = self.temp; // MULTIPLY IT BY THE % THROTTLE.
        self.in2 = self.w(PERTRS);
        self.call(blk::DUSTP2_2, Self::multwo)?;
        for _ in 0..4 {
            self.ret2 = asr(self.ret2); // DIVIDE BY 16.
        }
        if self.ret2 == 0 {
            return self.nodust();
        }
        self.at(blk::DUSTP2_3)?;
        self.push(self.ret2); // THE COUNTER.
        self.ret1 = asr(asr(STACK - 200 - EXLIST - 12)); // SIZE OF DUST AREA, IN PAIRS.
        if self.ret1 < self.top() {
            self.at(blk::DUSTP2_4)?;
            self.set_top(self.ret1); // SET A FINITE LIMIT NOW.
        }
        self.at(blk::DUSTWF)?;
        self.ret1 = self.w(DUSTX); // A RANDOM DUST GENERATING WORD.
        self.ret2 = 0o177700; // AND A GOOD MASK.
        loop {
            self.at(blk::DUSTL)?;
            self.ret1 = self.ret1.wrapping_add(self.w(TIME)).wrapping_add(1);
            self.ret1 &= !self.ret2; // JUST LOW SIX BITS.
            self.in1 = self.bs(YTHRST.wrapping_add(self.ret1)); // PICK UP AN X NOW.
            self.ret1 = self.ret1.wrapping_add(self.w(VERACC)) & !self.ret2;
            self.in1 &= !self.ret2;
            self.in1 = self.in1.swap_bytes() >> 1; // SWAB clears the carry for ROR.
            self.temp = !self.temp & 0o20000; // FLIP X DIRECTION, KEEP THE SIGN BIT.
            self.temp |= 0o40000; // THE INTENSITY BIT.
            self.in1 |= self.temp;
            self.in2 = self.bs(YTHRST.wrapping_add(self.ret1)) & !self.ret2; // Y.
            self.in1 |= self.in2; // MAKE THE COMMAND NOW.
            self.set(self.temp2, self.in1);
            self.temp2 = self.temp2.wrapping_add(2);
            self.in1 = self.in1.wrapping_add(0o20100) & !0o140000; // FLIP SIGNS, INVISIBLE.
            self.set(self.temp2, self.in1);
            self.temp2 = self.temp2.wrapping_add(2);
            let n = self.top();
            self.set_top(n.wrapping_sub(1));
            if !dec_bgt(n) {
                break;
            }
        }
        self.at(blk::DUSTL_1)?;
        self.set(self.temp2, DISTOP); // TERMINATE THE LIST NOW.
        self.temp2 = self.temp2.wrapping_add(2);
        self.set(self.temp2, 0);
        self.set(DUSTX, self.ret1);
        self.pop();
        self.set(DUSTON, EXLIST); // TURN ON THE DUST NOW.
        Ok(())
    }

    fn nodust(&mut self) -> Flow {
        self.at(blk::NODUST)?;
        self.set(DUSTON, 0); // TURN OFF THE DUST DISPLAY NOW.
        Ok(())
    }
}
