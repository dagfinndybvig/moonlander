//! The moon: the far view DRAWM1, the close-up DRAWM2 with the things
//! left on the surface by earlier games (wrecks, landed ships, flags, rocks
//! and the McDonald's), ALTER for craters, and the feature table routines
//! PUTFET and GETFET.
//!
//! The terrain TERAIN is one height per 48 feet. The far view draws every
//! fourth height at 32 feet to a raster unit; the close-up draws each 48
//! feet as twelve short vectors, jiggled by DFUDGE so the ground looks
//! rough. Both are built in MOONST as a graph-plot display list, at most
//! DRAWCT (225) vectors long.
//!
//! DRAWMC and DRAWRS save and restore registers in a way worth knowing about.
//! DRAWMC pushes five registers and then "returns" with `JMP @10.(SP)`,
//! leaving its return address on the stack under them. DRAWRS later copies
//! its own return address over that stale one, pops the registers, and
//! returns through the copy.

use super::{asr, dec_bgt, minus};
use crate::generated::blocks as blk;
use crate::machine::{Block, Escape, Flow, Moonlander};
use crate::generated::symbols::*;

impl Moonlander {
    /// DRAWM1: the whole moon, small.
    pub(crate) fn drawm1(&mut self) -> Flow {
        self.at(blk::DRAWM1)?;
        self.drawmc_call(blk::DRAWM1_1)?; // SET UP COMMON CRAP.
        self.temp = self.w(TERAIN); // FIRST Y POS, DIVIDED BY 32.
        for _ in 0..5 {
            self.temp = asr(self.temp);
        }
        self.temp = self.temp.wrapping_add(23);
        self.set(self.temp2, self.temp);
        self.temp2 = self.temp2.wrapping_add(2);
        self.set(LASTY, self.temp); // SAVE Y FOR TOP OF SCREEN CHECK.
        self.set(self.temp2, SETSVM); // THEN SET GRAPH PLOT MODE.
        self.temp2 = self.temp2.wrapping_add(2);
        self.in1 = TERAIN;
        loop {
            self.at(blk::DRAW1L)?;
            self.in1 = self.in1.wrapping_add(8); // POSITION TO THE NEXT Y.
            self.temp = self.w(self.in1);
            for _ in 0..5 {
                self.temp = asr(self.temp);
            }
            self.temp = self.temp.wrapping_add(23); // SCALE IT AROUND NOW.
            match self.jsr(blk::DRAW1L_1, Self::drawic)? {
                a if a == blk::DRAW1L_1.addr => self.at(blk::DRAW1L_1)?, // BR DRAW1L
                a if a == blk::DRAW1L_2.addr => break,
                other => return Err(Escape::Wild(other)),
            }
        }
        self.at(blk::DRAW1L_2)?;
        self.set(self.temp2, DISTOP); // TERMINATE THE LIST
        self.temp2 = self.temp2.wrapping_add(2);
        self.set(self.temp2, 0);
        self.set(MOONGO, MOONST); // START DISPLAYING THE MOON NOW.
        self.call(blk::DRAW1L_3, Self::drawrs)?; // RESTORE REGISTERS
        Ok(()) // RTS
    }

    /// JSR PC,DRAWMC, which comes back with JMP @10.(SP) to `back`.
    fn drawmc_call(&mut self, back: Block) -> Flow {
        self.push(back.addr);
        self.drawmc()?;
        let to = self.w(self.sp.wrapping_add(10));
        if to != back.addr {
            return Err(Escape::Wild(to));
        }
        self.at(back)
    }

    /// DRAWMC: save registers, start an empty moon list.
    fn drawmc(&mut self) -> Flow {
        self.at(blk::DRAWMC)?;
        for r in [self.ret1, self.ret2, self.in1, self.in2, self.temp] {
            self.push(r);
        }
        self.set(DRAWCT, 225); // NUMBER OF VECTORS TO DRAW.
        self.set(MOONGO, 0); // TURN OFF DRAWING OF MOON.
        self.set(MACON, 0); // "NO MACDONALD'S ON SCREEN".
        self.temp2 = MOONST;
        self.set(self.temp2, 0o114124); // PLOT POINT MODE.
        self.temp2 = self.temp2.wrapping_add(2);
        self.set(self.temp2, 0o170240); // SET STATUS MODE.
        self.temp2 = self.temp2.wrapping_add(2);
        self.set(self.temp2, 0); // SET X POSITION TO ZERO NOW.
        self.temp2 = self.temp2.wrapping_add(2);
        Ok(()) // JMP @10.(SP)
    }

    /// DRAWRS: restore what DRAWMC saved.
    fn drawrs(&mut self) -> Flow {
        self.at(blk::DRAWRS)?;
        let r = self.top(); // MOVE OVER RETURN ADDRESS.
        self.set(self.sp.wrapping_add(12), r);
        self.temp = self.pop(); // THROW AWAY TOP ITEM.
        self.temp = self.pop(); // AND RESTORE THE REGISTERS.
        self.in2 = self.pop();
        self.in1 = self.pop();
        self.ret2 = self.pop();
        self.ret1 = self.pop();
        Ok(()) // RTS
    }

    /// DRAWIC: one graph-plot Y to the list, invisible where it runs off
    /// the top or bottom, with the line type and intensity changed every few
    /// vectors. Returns past the word after the call once DRAWCT runs out.
    fn drawic(&mut self) -> Flow {
        self.at(blk::DRAWIC)?;
        self.push(0o1000); // MAKE THE VECTOR INVISIBLE FOR NOW.
        let mut skip_visible = false;
        if self.temp >= 1024 {
            // TOO BIG, SET TO TOP, AND TURN OFF.
            self.at(blk::DRAWIC_1)?;
            self.temp = 1023;
            if self.temp == self.w(LASTY) {
                skip_visible = true; // STILL ON TOP. DON'T DRAW THE MOUNTAIN.
            }
        }
        if !skip_visible {
            self.at(blk::DRAWI1)?;
            let visible = if !minus(self.temp) {
                true
            } else {
                self.at(blk::DRAWI1_1)?;
                self.temp = 0; // LESS THAN ZERO.
                self.w(LASTY) != 0 // WAS LAST RESULT 0? THEN NOT VISIBLE.
            };
            if visible {
                self.at(blk::DRAWI2)?;
                let v = self.top() | 0o40000; // TURN ON VISIBLE BIT.
                self.set_top(v);
            }
        }
        self.at(blk::DRAWI3)?;
        let ty = self.w(DRAWTY).wrapping_sub(1); // WHAT TYPE OF INTENSITY AND LINE TYPE.
        self.set(DRAWTY, ty);
        if minus(ty) {
            // COUNTER OVERFLOWED. CHANGE TYPE.
            self.at(blk::DRAWI3_1)?;
            let z = ((self.w(DRAWTZ).wrapping_add(1)) & 3).wrapping_add(1);
            self.set(DRAWTZ, z);
            self.set(DRAWTY, z); // RESET BIGGY AND LITTLY.
            let dint = self.w(DINT).wrapping_add(0o1200) & !0o176177; // RANDOMIZE INTENSITY
            self.set(DINT, dint);
            let dtype = self.w(DTYPE).wrapping_add(1) & 3; // AND TYPE.
            self.set(DTYPE, dtype);
            self.set(self.temp2, dint); // CREATE NEW PLOT POINT INSTRUCTION.
            let v = self.w(self.temp2) | dtype;
            self.set(self.temp2, v);
            let v = self.w(self.temp2) | 0o106004; // GRAPH Y.
            self.set(self.temp2, v);
            self.temp2 = self.temp2.wrapping_add(2);
        }
        self.at(blk::DRWIN)?;
        self.temp = self.temp.wrapping_sub(self.w(LASTY)); // GET THE DELTA Y.
        if minus(self.temp) {
            self.at(blk::DRWIN_1)?;
            self.temp = self.temp.wrapping_neg() & 0o77;
            let y = self.w(LASTY).wrapping_sub(self.temp); // UPDATE PREVIOUS Y NOW.
            self.set(LASTY, y);
            self.temp |= 0o100;
        } else {
            self.at(blk::DRWINP)?;
            self.temp &= 0o77; // CLEAN IT UP.
            let y = self.w(LASTY).wrapping_add(self.temp);
            self.set(LASTY, y);
        }
        self.at(blk::DRWINQ)?;
        let bits = self.pop();
        self.temp |= bits; // MAKE IT A GOOD INSTRUCTION NOW.
        self.set(self.temp2, self.temp);
        self.temp2 = self.temp2.wrapping_add(2);
        let n = self.w(DRAWCT); // DECREMENT MASTER COUNTER.
        self.set(DRAWCT, n.wrapping_sub(1));
        if !dec_bgt(n) {
            self.at(blk::DRWINQ_1)?;
            let r = self.top().wrapping_add(2); // END RETURN.
            self.set_top(r);
        }
        self.at(blk::DRAWRR) // RTS
    }

    /// DFAKE: feet to close-up raster units: (3*Y/2)/4 + 23.
    pub(crate) fn dfake(&mut self) -> Flow {
        self.at(blk::DFAKE)?;
        self.push(self.temp);
        self.temp <<= 1;
        let v = self.pop();
        self.temp = self.temp.wrapping_add(v);
        self.temp = asr(asr(asr(self.temp)));
        self.temp = self.temp.wrapping_add(23); // CORRECTS THE BASE SCREEN Y.
        Ok(())
    }

    /// DRAWM2: the close-up, starting at terrain index LEFTED.
    pub(crate) fn drawm2(&mut self) -> Flow {
        self.at(blk::DRAWM2)?;
        self.drawmc_call(blk::DRAWM2_1)?;
        self.set(DFUDGE, 0);
        self.in1 = (self.w(LEFTED) << 1).wrapping_add(TERAIN); // POINTER TO LEFT Y.
        self.temp = self.w(self.in1);
        self.call(blk::DRAWM2_2, Self::dfake)?;
        if minus(self.temp) {
            self.at(blk::DRAWM2_3)?;
            self.temp = 0; // OFF THE BOTTOM: ZERO IT OUT.
        } else {
            self.at(blk::DRW2L5)?;
            if self.temp >= 1024 {
                self.at(blk::DRW2L5_1)?;
                self.temp = 1023; // TOO BIG: FIX IT UP.
            }
        }
        self.at(blk::DRW2M)?;
        self.set(self.temp2, self.temp); // STARTING Y.
        self.temp2 = self.temp2.wrapping_add(2);
        self.set(LASTY, self.temp);
        self.set(self.temp2, SETSVM); // THE GRAPH PLOT MODE COMMAND <Y>.
        self.temp2 = self.temp2.wrapping_add(2);
        'terrain: loop {
            // DRW2L: Y(NEW) AND DELTA Y.
            self.at(blk::DRW2L)?;
            self.push(self.temp);
            self.temp = self.w(self.in1);
            self.in1 = self.in1.wrapping_add(2);
            self.call(blk::DRW2L_1, Self::dfake)?;
            self.in2 = self.temp;
            self.temp = self.pop();
            self.ret1 = 0;
            self.push(self.in1);
            self.in1 = 12; // TWELVE STEPS PER 48 FEET.
            self.in2 = self.in2.wrapping_sub(self.temp); // DELTA Y.
            if !minus(self.in2) {
                self.at(blk::DRAW2G)?;
                self.in2 = self.in2.wrapping_add(6); // ROUNDED,
                self.ret2 = self.in2;
                self.call(blk::DRAW2H, Self::divtwo)?; // AND DIVIDED BY TWELVE.
            } else {
                self.at(blk::DRW2L_2)?;
                self.in2 = self.in2.wrapping_sub(6).wrapping_neg();
                self.ret2 = self.in2;
                self.call(blk::DRW2L_3, Self::divtwo)?;
                self.ret2 = self.ret2.wrapping_neg();
                self.at(blk::DRAW2H)?;
            }
            self.in2 = self.ret2; // THE DELTA.
            self.ret1 = self.in1; // THE COUNT: TWELVE.
            self.in1 = self.pop();
            loop {
                // DRAW22 is INC @#DFUDGE or DEC @#DFUDGE, whichever the code
                // below last made it: the jiggle walks up to 3, then down to
                // -3, then up again. The setting survives a restart.
                self.at(blk::DRAW22)?;
                let d = match self.w(DRAW22) {
                    0o005237 => self.w(DFUDGE).wrapping_add(1),
                    0o005337 => self.w(DFUDGE).wrapping_sub(1),
                    _ => return Err(Escape::Wild(DRAW22)),
                };
                self.set(DFUDGE, d);
                if d as i16 >= 3 {
                    // MOV (PC)+,@(PC)+: store the word after it at the
                    // address after that, which is DEC @(PC)+ into DRAW22.
                    self.at(blk::DRAW22_1)?;
                    self.store_inline(blk::DRAW22_1);
                } else {
                    self.at(blk::DRAW2V)?;
                    if self.w(DFUDGE) as i16 <= -3 {
                        self.at(blk::DRAW2V_1)?;
                        self.store_inline(blk::DRAW2V_1); // INC @(PC)+ into DRAW22.
                    }
                }
                self.at(blk::DRAW2W)?;
                self.temp = self.temp.wrapping_add(self.w(DFUDGE)); // PERTURB SURFACE NOW.
                self.temp = self.temp.wrapping_add(self.in2);
                self.push(self.temp); // SAVE Y FOR A SECOND.
                match self.jsr(blk::DRAW2W_1, Self::drawic)? {
                    a if a == blk::DRAW2W_1.addr => {
                        self.at(blk::DRAW2W_1)?; // STILL GOING.
                        self.at(blk::DRAW2C)?;
                        self.temp = self.pop(); // RECALL THE Y NOW.
                        let more = dec_bgt(self.ret1);
                        self.ret1 = self.ret1.wrapping_sub(1);
                        if !more {
                            self.at(blk::DRAW2C_1)?;
                            continue 'terrain; // DONE WITH THE TWELVE.
                        }
                    }
                    a if a == blk::DRAW2W_2.addr => {
                        self.at(blk::DRAW2W_2)?; // GONE.
                        break 'terrain;
                    }
                    other => return Err(Escape::Wild(other)),
                }
            }
        }
        // DRAW2X: NOW THE FEATURES ON THE 19 SEGMENTS ON THE SCREEN.
        self.at(blk::DRAW2X)?;
        self.ret1 = self.w(LEFTED);
        self.set_top(19); // the saved Y becomes the counter.
        self.in1 = 24; // X OF THE FIRST SEGMENT'S CENTRE.
        self.ret2 = (self.ret1 << 1).wrapping_add(TERAIN);
        loop {
            self.at(blk::DRAW2Y)?;
            self.push(self.ret1); // FEATURE INDEX.
            self.ret1 = self.ret1.wrapping_add(1);
            self.call(blk::DRAW2Y_1, Self::getfet)?;
            self.temp = self.pop() << 1;
            // JSR PC,@DRAWTB(TEMP)
            let routine: fn(&mut Self) -> Flow = match self.w(DRAWTB.wrapping_add(self.temp)) {
                DRAWRR => Self::drawrr,
                OLDSHP => Self::oldshp,
                FLAG => Self::flag,
                LEFTSP => Self::leftsp,
                RIGHTS => Self::rights,
                ROCK => Self::rock,
                AHNONE => Self::ahnone,
                MACDON => Self::macdon,
                other => return Err(Escape::Wild(other)),
            };
            self.call(blk::DRAW2Y_2, routine)?;
            self.ret2 = self.ret2.wrapping_add(2); // TST (RET2)+: BYPASS
            self.in1 = self.in1.wrapping_add(48);
            let n = self.top();
            self.set_top(n.wrapping_sub(1));
            if !dec_bgt(n) {
                break;
            }
        }
        self.at(blk::DRAW2Y_3)?;
        self.pop();
        self.set(self.temp2, DISTOP); // TERMINATE THE DISPLAY NOW.
        self.temp2 = self.temp2.wrapping_add(2);
        self.set(self.temp2, 0);
        self.call(blk::DRAW2Y_4, Self::drawrs)?;
        self.set(MOONGO, MOONST); // START DISPLAYING THE MOON NOW.
        Ok(()) // RTS
    }

    /// MOV (PC)+,@(PC)+ at the start of `b`: the word after the instruction
    /// is stored at the address in the word after that.
    fn store_inline(&mut self, b: Block) {
        let v = self.w(b.addr.wrapping_add(2));
        let a = self.w(b.addr.wrapping_add(4));
        self.set(a, v);
    }

    fn drawrr(&mut self) -> Flow {
        self.at(blk::DRAWRR) // 0=NOTHING=RETURN.
    }

    pub(crate) fn ahnone(&mut self) -> Flow {
        self.at(blk::AHNONE)
    }

    fn oldshp(&mut self) -> Flow {
        self.dead_ship(blk::OLDSHP) // UPRIGHT, 23 ABOVE THE SURFACE.
    }

    fn leftsp(&mut self) -> Flow {
        self.dead_ship(blk::LEFTSP) // LEFT TILTED SHIP.
    }

    fn rights(&mut self) -> Flow {
        self.dead_ship(blk::RIGHTS)
    }

    /// OLDSHP, LEFTSP and RIGHTS: JSR PC,DEADSP followed by three words for
    /// it: the ship's angle, its height above the ground, and the lowest Y
    /// to draw. DEADSP normally drops this call and returns to our caller.
    fn dead_ship(&mut self, b: Block) -> Flow {
        self.at(b)?;
        self.push(b.end);
        if self.deadsp()? {
            return Ok(());
        }
        // No room: DEADBY's RTS returns into the three words, and the
        // processor runs them. For an upright ship the first is 0, a HALT;
        // for a left-tilted one, -90, a floating point instruction the
        // 11/05 lacks, which traps to TRAP10 and halts there; for a
        // right-tilted one, 90, JMP @(R2)+, a jump to a terrain height.
        let at = self.pop();
        match self.w(at) {
            0 => Err(Escape::Halt(at)),
            0o177646 => Err(Escape::Halt(TRAP10)),
            0o000132 => {
                let to = self.w(self.ret2);
                Err(Escape::Wild(to))
            }
            _ => Err(Escape::Wild(at)),
        }
    }

    /// DEADSP: draw a ship from an earlier game into the moon list, tilted
    /// to the ground. Returns whether it dropped its caller's call (it does
    /// unless there was no room).
    fn deadsp(&mut self) -> Flow<bool> {
        self.at(blk::DEADSP)?;
        if self.temp2 > MOONEN - 102 {
            self.at(blk::DEADBY)?; // NO ROOM. EXIT.
            return Ok(false);
        }
        self.at(blk::DEADSP_1)?;
        for r in [self.in2, self.ret1, self.ret2, self.temp, self.in1] {
            self.push(r);
        }
        self.in1 = self.w(self.sp.wrapping_add(10)); // PICK UP OLD PC: THE THREE WORDS.
        self.set(self.temp2, SETPNT); // SET POINT MODE.
        self.temp2 = self.temp2.wrapping_add(2);
        let x = self.top(); // MOV OVER THE X NOW.
        self.set(self.temp2, x);
        self.temp2 = self.temp2.wrapping_add(2);
        let a = self.w(ANGLE);
        self.push(a); // PUSH DOWN THE OLD ANGLE.
        self.temp = self.w(self.ret2); // PICK UP A Y.
        self.call(blk::DEADSP_2, Self::dfake)?;
        self.push(self.temp);
        self.temp = self.w(self.ret2.wrapping_add(2)); // NEXT Y.
        self.call(blk::DEADSP_3, Self::dfake)?;
        let v = self.pop();
        self.temp = asr(self.temp.wrapping_add(v)); // AVERAGE OF THE Y'S, WHICH IS CENTER.
        let a = self.w(self.in1); // THE CALLER'S ANGLE.
        self.in1 = self.in1.wrapping_add(2);
        self.set(ANGLE, a);
        self.temp = self.temp.wrapping_add(self.w(self.in1)); // CENTER THE SHIP.
        self.in1 = self.in1.wrapping_add(2);
        let low = self.w(self.in1);
        self.set(LOWY, low); // AND SET THE LOW Y.
        self.set(self.temp2, self.temp); // INSERT THE Y NOW.
        self.temp2 = self.temp2.wrapping_add(2);
        let (l, r) = (self.w(self.ret2), self.w(self.ret2.wrapping_add(2))); // WHICH Y IS BIGGER.
        if l != r {
            self.at(blk::DEADSP_4)?;
            if l > r {
                self.at(blk::DEADPL)?;
                let a = self.w(ANGLE).wrapping_add(22);
                self.set(ANGLE, a);
            } else {
                self.at(blk::DEADSP_5)?;
                let a = self.w(ANGLE).wrapping_add((-22i16) as u16);
                self.set(ANGLE, a);
            }
        }
        self.at(blk::DEADOK)?;
        self.call(blk::DEADOK_1, Self::trig)?;
        self.in2 = self.temp2;
        self.in1 = DESIGN;
        self.call(blk::DEADOK_2, Self::draw)?; // DRAW THE SHIP NOW.
        self.temp2 = self.temp2.wrapping_sub(2); // REMOVE THE DISPLAY STOP INSTRUCTION.
        self.set(self.temp2, 0);
        self.temp2 = self.temp2.wrapping_sub(2);
        self.set(self.temp2, 0);
        let a = self.pop();
        self.set(ANGLE, a); // RESTORE THE ANGLE,
        self.call(blk::DEADOK_3, Self::trig)?; // AND THE VALUES.
        self.in1 = self.pop();
        self.temp = self.pop();
        self.ret2 = self.pop();
        self.ret1 = self.pop();
        self.in2 = self.pop();
        self.pop(); // IGNORE ORIGINAL CALL.
        self.at(blk::DEADBY)?;
        Ok(true)
    }

    /// FLAG: a flag planted in an earlier game.
    fn flag(&mut self) -> Flow {
        self.at(blk::FLAG)?;
        if self.temp2 > MOONEN.wrapping_sub(FLAGEN).wrapping_add(FLAGL).wrapping_sub(6) {
            return self.at(blk::ROCKRT); // NO ROOM.
        }
        self.at(blk::FLAG_1)?;
        self.set(FLAGX, self.in1);
        self.temp = self.w(self.ret2);
        self.call(blk::FLAG_2, Self::dfake)?; // GET THE Y'S
        self.in2 = self.temp;
        self.temp = self.w(self.ret2.wrapping_add(2));
        self.call(blk::FLAG_3, Self::dfake)?;
        self.temp = asr(self.temp.wrapping_add(self.in2));
        self.set(FLAGY, self.temp);
        self.in2 = FLAGL; // NOW FAKE OUT A ROUTINE: ENTER ROCK PUTTER.
        self.rocklp()
    }

    /// ROCK: a boulder.
    fn rock(&mut self) -> Flow {
        self.at(blk::ROCK)?;
        if self.temp2 > MOONEN.wrapping_sub(ROCKEN).wrapping_add(ROCKL).wrapping_sub(6) {
            return self.at(blk::ROCKRT); // CAN'T FIT NOW.
        }
        self.at(blk::ROCK_1)?;
        self.set(ROCKX, self.in1);
        self.temp = self.w(self.ret2); // AFTER FUDGING Y.
        self.call(blk::ROCK_2, Self::dfake)?;
        self.in2 = self.temp;
        self.temp = self.w(self.ret2.wrapping_add(2));
        self.call(blk::ROCK_3, Self::dfake)?;
        self.temp = asr(self.temp.wrapping_add(self.in2));
        self.set(ROCKY, self.temp);
        self.in2 = ROCKL; // PROTOTYPE ROCK.
        self.rocklp()
    }

    /// ROCKLP: copy a display list into the moon list, up to its stop.
    fn rocklp(&mut self) -> Flow {
        loop {
            self.at(blk::ROCKLP)?;
            self.temp = self.w(self.in2);
            self.in2 = self.in2.wrapping_add(2);
            if self.temp == DISTOP {
                break;
            }
            self.at(blk::ROCKLP_1)?;
            self.set(self.temp2, self.temp);
            self.temp2 = self.temp2.wrapping_add(2);
        }
        self.at(blk::ROCKRT)
    }

    /// MACDON: the McDonald's, if it still exists and fits on the screen.
    fn macdon(&mut self) -> Flow {
        self.at(blk::MACDON)?;
        if self.w(MACTHR) == 0 {
            return self.at(blk::MACEX); // NO LONGER IN EXISTENCE.
        }
        self.at(blk::MACDON_1)?;
        if self.in1 as i16 <= 25 {
            return self.at(blk::MACEX); // TOO FAR LEFT.
        }
        self.at(blk::MACDON_2)?;
        if self.in1 as i16 >= 880 {
            return self.at(blk::MACEX); // TOO FAR TO THE RIGHT.
        }
        self.at(blk::MACDON_3)?;
        self.temp = self.w(self.ret2.wrapping_add(2)); // RIGHT Y.
        if (self.w(self.ret2) as i16) < self.temp as i16 {
            self.at(blk::MACDON_4)?;
            self.temp = self.w(self.ret2); // PICK SMALLEST Y NOW.
        }
        self.at(blk::MACB1)?;
        self.call(blk::MACB1_1, Self::dfake)?;
        self.set(MACY, self.temp);
        self.set(MACX, self.in1);
        self.set(MACON, self.sp); // SET THE MAC IS DRAWN FLAG.
        self.set(self.temp2, DISTOP); // PLACE CALL TO IT IN THE BUFFER.
        self.temp2 = self.temp2.wrapping_add(2);
        self.set(self.temp2, MACS);
        self.temp2 = self.temp2.wrapping_add(2);
        self.at(blk::MACEX) // "SIMPLE, ISN'T IT."
    }

    /// ALTER: dig a crater IN1 feet deep at the ship, halving and swapping
    /// sign outward, and remember a wreck there.
    pub(crate) fn alter(&mut self) -> Flow {
        self.at(blk::ALTER)?;
        self.in2 = self.w(BIGXCT); // PRESENT INDEX.
        self.temp = self.in2;
        self.in2 = (self.in2 << 1).wrapping_add(TERAIN);
        self.push(3); // A CRASHED SHIP.
        self.temp2 = self.w(self.in2); // PICK UP Y.
        self.in2 = self.in2.wrapping_add(2);
        self.ret2 = self.in2;
        self.temp2 = self.temp2.wrapping_sub(self.w(self.in2)); // WHICH WAY TO TILT SHIP.
        if !minus(self.temp2) {
            self.at(blk::ALTER_1)?;
            let v = self.top().wrapping_add(1);
            self.set_top(v);
        }
        self.at(blk::ALERTP)?;
        self.push(self.temp); // THE INDEX.
        self.call(blk::ALERTL, Self::putfet)?;
        loop {
            let v = self.w(self.in2).wrapping_sub(self.in1); // NOW ALTER BOTH WAYS.
            self.set(self.in2, v);
            self.in2 = self.in2.wrapping_add(2);
            self.ret2 = self.ret2.wrapping_sub(2);
            let v = self.w(self.ret2).wrapping_sub(self.in1);
            self.set(self.ret2, v);
            self.in1 = asr(self.in1).wrapping_neg(); // DIVIDE BY TWO AND NEGATE.
            if self.in1 == 0 {
                break;
            }
            self.at(blk::ALERTL)?;
        }
        self.at(blk::ALERTL_1) // RTS
    }

    /// PUTFET: store a feature (four bits) for a terrain index. Both are
    /// passed on the stack, value under index, and removed by PUTFET.
    pub(crate) fn putfet(&mut self) -> Flow {
        self.at(blk::PUTFET)?;
        self.push(self.temp); // SAVE TEMPORARY NOW.
        let index = self.w(self.sp.wrapping_add(4));
        self.temp = asr(index); // MAKE IT A BYTE INDEX.
        let at = FEATUR.wrapping_add(self.temp);
        if index & 1 != 0 {
            // LEFT BYTE: SHIFT OVER THE VALUE.
            self.at(blk::PUTFET_1)?;
            let a = self.sp.wrapping_add(6);
            let v = self.w(a) << 4;
            self.set(a, v);
            let b = self.b(at) & !0o360;
            self.setb(at, b);
        } else {
            self.at(blk::FETRGT)?;
            let b = self.b(at) & !0o17; // CLEAR OUT THE RIGHT BYTE.
            self.setb(at, b);
        }
        self.at(blk::PUTCOM)?;
        let v = self.b(self.sp.wrapping_add(6)); // INSERT THE BYTE NOW.
        let b = self.b(at) | v;
        self.setb(at, b);
        self.temp = self.pop(); // RESTORE THE REGISTER NOW.
        let r = self.top(); // MOVE OVER THE PC.
        self.set(self.sp.wrapping_add(4), r);
        self.sp = self.sp.wrapping_add(4);
        Ok(()) // RTS
    }

    /// GETFET: the feature for the terrain index on the stack, returned in
    /// its place.
    pub(crate) fn getfet(&mut self) -> Flow {
        self.at(blk::GETFET)?;
        self.push(self.temp); // SAVE A SCRATCH REGISTER NOW.
        let index = self.w(self.sp.wrapping_add(4));
        self.temp = asr(index); // BYTE ADDRESS IT.
        self.temp = self.bs(FEATUR.wrapping_add(self.temp));
        if index & 1 != 0 {
            self.at(blk::GETFET_1)?;
            for _ in 0..4 {
                self.temp = asr(self.temp); // SHIFT IT OVER.
            }
        }
        self.at(blk::GETDUN)?;
        self.temp &= 0o17; // JUST RETURN 4 BITS NOW.
        self.set(self.sp.wrapping_add(4), self.temp);
        self.temp = self.pop();
        Ok(()) // RTS
    }
}
