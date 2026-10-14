//! A DEC GT40 as a running machine: the KD11-B (PDP-11/05) processor with
//! its core memory, line clock, host line and keyboard, driving the VT11
//! display processor model from the `gt40` crate.

pub mod cpu;
pub mod machine;
pub mod timing;

pub use machine::Machine;
