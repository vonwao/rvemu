//! Exceptions and interrupts, RISC-V privileged spec numbering.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exception {
    InstructionAddressMisaligned(u64),
    InstructionAccessFault(u64),
    IllegalInstruction(u64), // tval = raw instruction bits
    Breakpoint(u64),
    LoadAddressMisaligned(u64),
    LoadAccessFault(u64),
    StoreAddressMisaligned(u64),
    StoreAccessFault(u64),
    EcallFromU,
    EcallFromS,
    EcallFromM,
    InstructionPageFault(u64),
    LoadPageFault(u64),
    StorePageFault(u64),
}

impl Exception {
    pub fn cause(&self) -> u64 {
        use Exception::*;
        match self {
            InstructionAddressMisaligned(_) => 0,
            InstructionAccessFault(_) => 1,
            IllegalInstruction(_) => 2,
            Breakpoint(_) => 3,
            LoadAddressMisaligned(_) => 4,
            LoadAccessFault(_) => 5,
            StoreAddressMisaligned(_) => 6,
            StoreAccessFault(_) => 7,
            EcallFromU => 8,
            EcallFromS => 9,
            EcallFromM => 11,
            InstructionPageFault(_) => 12,
            LoadPageFault(_) => 13,
            StorePageFault(_) => 15,
        }
    }

    pub fn tval(&self) -> u64 {
        use Exception::*;
        match self {
            InstructionAddressMisaligned(v) | InstructionAccessFault(v) | IllegalInstruction(v)
            | Breakpoint(v) | LoadAddressMisaligned(v) | LoadAccessFault(v)
            | StoreAddressMisaligned(v) | StoreAccessFault(v) | InstructionPageFault(v)
            | LoadPageFault(v) | StorePageFault(v) => *v,
            EcallFromU | EcallFromS | EcallFromM => 0,
        }
    }
}

/// Interrupt cause numbers (the bit index in mip/mie).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    SupervisorSoftware = 1,
    MachineSoftware = 3,
    SupervisorTimer = 5,
    MachineTimer = 7,
    SupervisorExternal = 9,
    MachineExternal = 11,
}
