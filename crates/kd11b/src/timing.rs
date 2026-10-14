//! Instruction timing for the KD11-B with MM11-K/L core memory, from DEC's
//! PDP-11/05, 11/10 Computer Manual (DEC-11-H05AA-A-D), Part II, section
//! 2.2.2 and Tables 2-2 to 2-7. The GT40's memory is this MM11: the GT40
//! user's guide gives the same 900 ns cycle and 400 ns access time.
//!
//! The manual: "The execution time for an instruction is the sum of a basic
//! instruction time and the time to determine and fetch the source and/or
//! destination operands." Table 2-2 gives the operand times by addressing
//! mode, the instruction tables the basic times. "All times stated are
//! subject to ±10% variation"; the model uses the stated values.
//!
//! Two things in the tables depend on more than the instruction word: a
//! conditional branch takes 1.9 µs if it does not branch and 2.5 µs if it
//! does, and a byte operand at an odd address costs more (Table 2-2's notes).
//! [`Timing::instruction`] takes both as arguments.
//!
//! Where the manual leaves a choice, the choice made is marked READING:
//!
//! - READING: JMP (1.0 µs) and JSR (3.8 µs) add the destination time less
//!   1.2 µs, as for a non-modifying instruction. Their destination is an
//!   address, not an operand, and section 4.7.2.2 lists JMP and JSR with
//!   TST, CMP and BIT as the instructions that "can never result in the
//!   modification of the destination".
//! - READING: MOV's "3.1 µs mode 0" is for a register destination; any
//!   other destination takes the 3.7 µs basic time. MOV counts as modifying:
//!   the same section's list of non-modifying instructions leaves it out.
//! - READING: interrupt entry is not given. It is taken as the trap
//!   instructions' 8.2 µs: section 5.5.2 has interrupts and traps join the
//!   same microroutine (ET-2) that stacks the PS and PC and loads the new
//!   ones, after one bus transfer in each case (the instruction fetch for a
//!   trap, the vector read for an interrupt).
//! - READING: a bus error trap costs the instruction's own time plus the
//!   8.2 µs of a trap; the reserved instruction trap, and JMP or JSR to a
//!   register, 8.2 µs.
//! - RESET: the 20 ms the table gives, which is the length of INIT.
//!
//! Times are worked out in tenths of a microsecond, exactly, and converted
//! once.

/// Source operand time by addressing mode (Table 2-2), tenths of µs.
const SRC: [u32; 8] = [0, 9, 9, 24, 9, 24, 24, 34];
/// Destination operand time by addressing mode (Table 2-2), tenths of µs.
const DST: [u32; 8] = [0, 24, 24, 34, 24, 34, 34, 47];
/// "Subtract 1.2 µs for a non-modifying instruction."
const NON_MODIFYING: u32 = 12;
/// "Add 1.3 µs for odd byte addressing" (source, or a destination that is
/// only read).
const ODD_READ: u32 = 13;
/// "Add 2.4 µs for odd byte addressing with a modifying instruction."
const ODD_MODIFY: u32 = 24;
/// EMT, TRAP, IOT, BPT (Table 2-5); also interrupt entry (READING above).
const TRAP: u32 = 82;
/// RESET: INIT for 20 ms (Table 2-6).
const RESET: u32 = 200_000;

#[derive(Clone, Debug)]
pub struct Timing {
    pub source: &'static str,
}

impl Default for Timing {
    fn default() -> Self {
        Timing::dec()
    }
}

fn mode(spec: u16) -> usize {
    ((spec >> 3) & 7) as usize
}

/// Destination time for addressing mode `m`.
fn dst(m: usize, modifying: bool, odd: bool) -> u32 {
    let mut t = DST[m];
    if m != 0 && !modifying {
        t -= NON_MODIFYING;
    }
    if odd {
        t += if modifying { ODD_MODIFY } else { ODD_READ };
    }
    t
}

impl Timing {
    /// The figures of the PDP-11/05, 11/10 Computer Manual.
    pub fn dec() -> Timing {
        Timing { source: "DEC-11-H05AA-A-D, Part II, Tables 2-2 to 2-7" }
    }

    /// Time of the instruction whose first word is `w`, in microseconds:
    /// `taken` if it is a branch that branched, `odd_src` and `odd_dst` if
    /// it is a byte instruction whose operand is at an odd address.
    pub fn instruction(&self, w: u16, taken: bool, odd_src: bool, odd_dst: bool) -> f64 {
        tenths(w, taken, odd_src, odd_dst) as f64 / 10.0
    }

    /// Interrupt entry: stacking the PS and PC and loading the vector.
    pub fn interrupt(&self) -> f64 {
        TRAP as f64 / 10.0
    }

    /// An instruction whose first word is `w` takes a bus error: its own
    /// time (`None` if the fetch itself failed), and the trap's.
    pub fn bus_error(&self, w: Option<u16>) -> (f64, f64) {
        let own = w.map(|w| self.instruction(w, false, false, false)).unwrap_or(0.0);
        (own, self.interrupt())
    }
}

fn tenths(w: u16, taken: bool, odd_src: bool, odd_dst: bool) -> u32 {
    let top = w >> 12;
    let (sm, dm) = (mode(w >> 6), mode(w));
    // Double operand: MOV CMP BIT BIC BIS ADD and their byte forms, SUB.
    if matches!(top, 0o01..=0o06 | 0o11..=0o15 | 0o16) {
        let op = top & 7;
        let (mov, cmp_or_bit) = (op == 1 && top != 0o16, matches!(op, 2 | 3) && top != 0o16);
        let basic = if mov && dm == 0 { 31 } else { 37 };
        let src = SRC[sm] + if odd_src { ODD_READ } else { 0 };
        return basic + src + dst(dm, !cmp_or_bit, odd_dst);
    }
    // Branches: 1.9 µs not taken, 2.5 µs taken (BR always branches).
    let hb = w >> 8;
    if matches!(hb, 0o001..=0o007 | 0o200..=0o207) {
        return if taken || hb == 0o001 { 25 } else { 19 };
    }
    match w {
        0o000000 | 0o000001 => return 18, // HALT, WAIT
        0o000002 => return 44,            // RTI
        0o000003 | 0o000004 => return TRAP, // BPT, IOT
        0o000005 => return RESET,
        _ => {}
    }
    // JMP and JSR: register mode is illegal and traps.
    if (0o000100..=0o000177).contains(&w) {
        return if dm == 0 { TRAP } else { 10 + dst(dm, false, false) };
    }
    if (0o004000..=0o004777).contains(&w) {
        return if dm == 0 { TRAP } else { 38 + dst(dm, false, false) };
    }
    if (0o000200..=0o000207).contains(&w) {
        return 38; // RTS
    }
    if (0o000240..=0o000277).contains(&w) {
        return 25; // condition code operators, NOP
    }
    if (0o000300..=0o000377).contains(&w) {
        return 43 + dst(dm, true, false); // SWAB
    }
    if (0o104000..=0o104777).contains(&w) {
        return TRAP; // EMT, TRAP
    }
    // Single operand: CLR COM INC DEC NEG ADC SBC TST ROR ROL ASR ASL and
    // their byte forms. TST does not modify its destination.
    let sop = (w >> 6) & 0o777;
    if (0o050..=0o063).contains(&sop) {
        return 34 + dst(dm, sop != 0o057, odd_dst);
    }
    // Anything else is reserved on the KD11-B and traps to 10.
    TRAP
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(w: u16) -> f64 {
        Timing::dec().instruction(w, false, false, false)
    }

    #[test]
    fn worked_examples() {
        assert_eq!(t(0o010001), 3.1); // MOV R0,R1
        assert_eq!(t(0o012700), 4.0); // MOV #n,R0: 3.1 + 0.9
        assert_eq!(t(0o013737), 9.5); // MOV @#a,@#b: 3.7 + 2.4 + 3.4
        assert_eq!(t(0o060102), 3.7); // ADD R1,R2
        assert_eq!(t(0o022737), 6.8); // CMP #n,@#a: 3.7 + 0.9 + 3.4 - 1.2
        assert_eq!(t(0o005037), 6.8); // CLR @#a: 3.4 + 3.4
        assert_eq!(t(0o005737), 5.6); // TST @#a: 3.4 + 3.4 - 1.2
        assert_eq!(t(0o000137), 3.2); // JMP @#a: 1.0 + 3.4 - 1.2
        assert_eq!(t(0o004737), 6.0); // JSR PC,@#a: 3.8 + 3.4 - 1.2
        assert_eq!(t(0o000207), 3.8); // RTS PC
        assert_eq!(t(0o000002), 4.4); // RTI
        assert_eq!(t(0o104400), 8.2); // TRAP
        assert_eq!(t(0o000005), 20_000.0); // RESET
        assert_eq!(t(0o000241), 2.5); // CLC
        assert_eq!(t(0o070000), 8.2); // MUL: reserved on the 11/05
    }

    #[test]
    fn branches() {
        let d = Timing::dec();
        assert_eq!(d.instruction(0o001003, false, false, false), 1.9); // BNE
        assert_eq!(d.instruction(0o001003, true, false, false), 2.5);
        assert_eq!(d.instruction(0o000403, false, false, false), 2.5); // BR
    }

    #[test]
    fn odd_bytes() {
        let d = Timing::dec();
        // MOVB (R1)+,(R2)+ : 3.7 + 0.9 + 2.4, plus 1.3 odd source, 2.4 odd
        // destination (MOV modifies).
        assert_eq!(d.instruction(0o112122, false, false, false), 7.0);
        assert_eq!(d.instruction(0o112122, false, true, false), 8.3);
        assert_eq!(d.instruction(0o112122, false, false, true), 9.4);
        // TSTB (R1): 3.4 + 2.4 - 1.2, odd adds 1.3.
        assert_eq!(d.instruction(0o105711, false, false, true), 5.9);
    }
}
