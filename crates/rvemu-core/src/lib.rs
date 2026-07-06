//! rvemu-core: a RISC-V rv64imac_zicsr_zifencei(_zicntr_sstc) instruction-set
//! simulator. Pure interpreter, single hart. No host I/O happens in this
//! crate except through the [`platform::Platform`] trait.

pub mod bus;
pub mod cpu;
pub mod csr;
pub mod elf;
pub mod machine;
pub mod platform;
pub mod plic;
pub mod uart;
pub mod trap;
pub mod virtio;

pub use platform::Platform;
