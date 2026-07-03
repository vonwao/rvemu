//! rvemu-core: a RISC-V rv64imac_zicsr_zifencei instruction-set simulator.
//!
//! Pure interpreter, single hart, Sv39 translation. No host I/O happens in
//! this crate except through the [`platform::Platform`] trait.

pub mod platform;

pub use platform::Platform;
