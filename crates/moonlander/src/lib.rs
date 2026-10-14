//! Moonlander, by Jack Burness, for the DEC GT40 graphic display terminal
//! (1973), as it was built for the "Mare Assabet" greeting: translated into
//! Rust routine by routine from the matching reconstruction of its source
//! (`reconstruction/lunar-mare-assabet.mac`).
//!
//! # How it is translated
//!
//! The program depends on more than its variables (journal entry 007). It
//! divides by a register left over from another routine, keeps flags in
//! the immediate operands of instructions, rewrites one of its own
//! instructions, compares a variable with its return address, and draws
//! random numbers from below its stack pointer. A translation that kept only
//! the variables would fly a different ship. So this one keeps everything
//! the original's behaviour can depend on:
//!
//! - The original memory image, laid out exactly as the Mare Assabet binary
//!   was, initialised from the assembled reconstruction. Variables, tables,
//!   display lists and the instruction words that double as data all live
//!   where they did. The VT11 display processor model draws the picture from
//!   this memory, as the real one did by DMA.
//! - The six general registers, under Burness's names, and a stack in that
//!   memory holding the original return addresses.
//! - Time. Each routine is written in the basic blocks of the original, and
//!   at the start of each block calls [`Moonlander::at`], which charges the
//!   time the original instructions take on a KD11-B, takes interrupts, and
//!   reports the block to any observer. How many clock ticks pass while a
//!   frame is computed sets the size of each physics step, so this matters.
//!
//! What it does not keep is the processor's condition codes between blocks:
//! each branch tests the value it depends on directly.
//!
//! # How it is checked
//!
//! The emulator in crate `kd11b` runs the original binary on the same
//! hardware model, in a mode where it too takes interrupts only at the start
//! of blocks. Run side by side from the same state with the same light pen
//! movements, the two must agree at every block: same address, same machine
//! time, same registers, same contents of all of memory.
//!
//! # Layout
//!
//! The routines are grouped as Burness grouped them, one module per section
//! of the source; each keeps his comments.

pub mod generated;
pub mod machine;
mod program;

pub use machine::{Block, Escape, Flow, Moonlander, Snapshot};
pub use program::{routine, Routine, ROUTINES};

/// The program's memory image: the reconstruction, assembled. It matches the
/// surviving Mare Assabet binary in every loaded byte.
pub static IMAGE: &[u8] = include_bytes!("../data/lunar.lda");
