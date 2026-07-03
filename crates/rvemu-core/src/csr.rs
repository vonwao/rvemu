//! Control and status registers for rv64imac_zicsr_zifencei_zicntr_sstc with
//! M/S/U modes. WARL legalization aims to match the pinned Spike exactly;
//! where the spec allows latitude, lockstep against Spike is the arbiter and
//! the emulator adapts.

use crate::trap::Exception;

// Addresses (the ones the targets and test suites touch).
pub const FFLAGS: u16 = 0x001; // absent (no F) -> illegal
pub const CYCLE: u16 = 0xc00;
pub const TIME: u16 = 0xc01;
pub const INSTRET: u16 = 0xc02;
pub const SSTATUS: u16 = 0x100;
pub const SIE: u16 = 0x104;
pub const STVEC: u16 = 0x105;
pub const SCOUNTEREN: u16 = 0x106;
pub const SENVCFG: u16 = 0x10a;
pub const SSCRATCH: u16 = 0x140;
pub const SEPC: u16 = 0x141;
pub const SCAUSE: u16 = 0x142;
pub const STVAL: u16 = 0x143;
pub const SIP: u16 = 0x144;
pub const STIMECMP: u16 = 0x14d;
pub const SATP: u16 = 0x180;
pub const MSTATUS: u16 = 0x300;
pub const MISA: u16 = 0x301;
pub const MEDELEG: u16 = 0x302;
pub const MIDELEG: u16 = 0x303;
pub const MIE: u16 = 0x304;
pub const MTVEC: u16 = 0x305;
pub const MCOUNTEREN: u16 = 0x306;
pub const MENVCFG: u16 = 0x30a;
pub const MCOUNTINHIBIT: u16 = 0x320;
pub const MSCRATCH: u16 = 0x340;
pub const MEPC: u16 = 0x341;
pub const MCAUSE: u16 = 0x342;
pub const MTVAL: u16 = 0x343;
pub const MIP: u16 = 0x344;
pub const PMPCFG0: u16 = 0x3a0;
pub const PMPADDR0: u16 = 0x3b0;
pub const MCYCLE: u16 = 0xb00;
pub const MINSTRET: u16 = 0xb02;
pub const MVENDORID: u16 = 0xf11;
pub const MARCHID: u16 = 0xf12;
pub const MIMPID: u16 = 0xf13;
pub const MHARTID: u16 = 0xf14;

// mstatus fields
pub const MSTATUS_SIE: u64 = 1 << 1;
pub const MSTATUS_MIE: u64 = 1 << 3;
pub const MSTATUS_SPIE: u64 = 1 << 5;
pub const MSTATUS_MPIE: u64 = 1 << 7;
pub const MSTATUS_SPP: u64 = 1 << 8;
pub const MSTATUS_MPP_MASK: u64 = 3 << 11;
pub const MSTATUS_MPRV: u64 = 1 << 17;
pub const MSTATUS_SUM: u64 = 1 << 18;
pub const MSTATUS_MXR: u64 = 1 << 19;
pub const MSTATUS_TVM: u64 = 1 << 20;
pub const MSTATUS_TW: u64 = 1 << 21;
pub const MSTATUS_TSR: u64 = 1 << 22;
pub const MSTATUS_UXL_SXL: u64 = (2 << 32) | (2 << 34); // read-only 64-bit

// mip/mie bits
pub const IRQ_SSIP: u64 = 1 << 1;
pub const IRQ_MSIP: u64 = 1 << 3;
pub const IRQ_STIP: u64 = 1 << 5;
pub const IRQ_MTIP: u64 = 1 << 7;
pub const IRQ_SEIP: u64 = 1 << 9;
pub const IRQ_MEIP: u64 = 1 << 11;

pub const MISA_VALUE: u64 = (2 << 62) | 0x141105; // RV64 IMAC + S + U

const MSTATUS_WMASK: u64 = MSTATUS_SIE
    | MSTATUS_MIE
    | MSTATUS_SPIE
    | MSTATUS_MPIE
    | MSTATUS_SPP
    | MSTATUS_MPP_MASK
    | MSTATUS_MPRV
    | MSTATUS_SUM
    | MSTATUS_MXR
    | MSTATUS_TVM
    | MSTATUS_TW
    | MSTATUS_TSR;
const SSTATUS_MASK: u64 =
    MSTATUS_SIE | MSTATUS_SPIE | MSTATUS_SPP | MSTATUS_SUM | MSTATUS_MXR | (2 << 32) /*UXL ro*/;
const MEDELEG_WMASK: u64 = 0xb3ff; // delegatable exceptions (no M-ecall bit 11)
const MIDELEG_WMASK: u64 = IRQ_SSIP | IRQ_STIP | IRQ_SEIP;
const MIE_WMASK: u64 = IRQ_SSIP | IRQ_MSIP | IRQ_STIP | IRQ_MTIP | IRQ_SEIP | IRQ_MEIP;
const SIE_MASK: u64 = IRQ_SSIP | IRQ_STIP | IRQ_SEIP;
// Software-writable mip bits (MTIP/MSIP come from the CLINT; STIP from Sstc).
const MIP_WMASK: u64 = IRQ_SSIP | IRQ_SEIP;
const MENVCFG_WMASK: u64 = 1 << 63; // STCE only (no PBMT/CBIE for this target)

pub struct Csrs {
    pub mstatus: u64,
    pub medeleg: u64,
    pub mideleg: u64,
    pub mie: u64,
    pub mtvec: u64,
    pub mcounteren: u64,
    pub menvcfg: u64,
    pub mcountinhibit: u64,
    pub mscratch: u64,
    pub mepc: u64,
    pub mcause: u64,
    pub mtval: u64,
    /// Software-writable mip bits only; effective mip is composed in Cpu.
    pub mip_sw: u64,
    pub pmpcfg0: u64,
    pub pmpaddr: [u64; 16],
    pub stvec: u64,
    pub scounteren: u64,
    pub senvcfg: u64,
    pub sscratch: u64,
    pub sepc: u64,
    pub scause: u64,
    pub stval: u64,
    pub stimecmp: u64,
    pub satp: u64,
    /// Architectural counters: both advance one per retired instruction
    /// (like Spike counting steps) but are independently writable.
    pub instret: u64,
    pub cycle: u64,
}

impl Csrs {
    pub fn new() -> Self {
        Csrs {
            mstatus: MSTATUS_UXL_SXL,
            medeleg: 0,
            mideleg: 0,
            mie: 0,
            mtvec: 0,
            mcounteren: 0,
            menvcfg: 0,
            mcountinhibit: 0,
            mscratch: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
            mip_sw: 0,
            pmpcfg0: 0,
            pmpaddr: [0; 16],
            stvec: 0,
            scounteren: 0,
            senvcfg: 0,
            sscratch: 0,
            sepc: 0,
            scause: 0,
            stval: 0,
            stimecmp: u64::MAX,
            satp: 0,
            instret: 0,
            cycle: 0,
        }
    }
}

impl Default for Csrs {
    fn default() -> Self {
        Self::new()
    }
}

/// Legalize an mstatus write (also used for sstatus via mask).
pub fn legalize_mstatus(old: u64, val: u64) -> u64 {
    let mut new = (old & !MSTATUS_WMASK) | (val & MSTATUS_WMASK);
    // MPP is WARL over {U, S, M}; an illegal write (2) keeps the old value
    // (matches Spike's legalization).
    let mpp = (new >> 11) & 3;
    if mpp == 2 {
        new = (new & !MSTATUS_MPP_MASK) | (old & MSTATUS_MPP_MASK);
    }
    new | MSTATUS_UXL_SXL
}

pub fn sstatus_view(mstatus: u64) -> u64 {
    mstatus & SSTATUS_MASK
}

pub fn legalize_mtvec(val: u64) -> u64 {
    // Spike's tvec_csr_t::unlogged_write: clear bit 1, keep bit 0.
    val & !2
}

/// Effective privilege-independent write masks etc. are applied in
/// Cpu::csr_write, which owns composition with device state (mip) and
/// existence/permission checks. This module only holds storage and masks.
pub struct CsrMasks;

impl CsrMasks {
    pub const MSTATUS_WMASK: u64 = MSTATUS_WMASK;
    pub const SSTATUS_MASK: u64 = SSTATUS_MASK;
    pub const MEDELEG_WMASK: u64 = MEDELEG_WMASK;
    pub const MIDELEG_WMASK: u64 = MIDELEG_WMASK;
    pub const MIE_WMASK: u64 = MIE_WMASK;
    pub const SIE_MASK: u64 = SIE_MASK;
    pub const MIP_WMASK: u64 = MIP_WMASK;
    pub const MENVCFG_WMASK: u64 = MENVCFG_WMASK;
}

/// CSR access outcome used by the CPU.
pub type CsrResult = Result<u64, Exception>;
