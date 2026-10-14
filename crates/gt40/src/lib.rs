//! Hardware models of the DEC GT40 graphic display terminal, written from
//! DEC's manuals: the VT11 display processor and its character generator,
//! and the terminal around them (core, line clock, host line, keyboard).

pub mod chargen;
pub mod crt;
pub mod lda;
pub mod raster;
pub mod terminal;
pub mod vt11;
