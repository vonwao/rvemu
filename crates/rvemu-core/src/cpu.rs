//! The interpreter core: fetch, decode, execute, trap, one instruction at a
//! time. Behavior is calibrated against the pinned Spike via the lockstep
//! harness; where the spec leaves latitude, Spike's choice wins.

use crate::bus::Bus;
use crate::csr::{self, Csrs};
use crate::trap::Exception;

pub const PRV_U: u8 = 0;
pub const PRV_S: u8 = 1;
pub const PRV_M: u8 = 3;

/// Spike ticks mtime once per this many retired instructions.
const INSNS_PER_RTC_TICK: u64 = 100;

/// What the CPU reports back to the driving loop after one step.
pub enum StepResult {
    Retired,
    /// Instruction trapped (no retirement); trap already taken.
    Trapped,
    /// WFI with no pending interrupt: nothing retired, waiting.
    WaitingForInterrupt,
}

pub struct Cpu {
    pub regs: [u64; 32],
    pub pc: u64,
    pub prv: u8,
    pub csrs: Csrs,
    pub bus: Bus,
    reservation: Option<u64>,
    /// Retired-instruction count for the run budget (independent of the
    /// writable minstret CSR).
    pub retired: u64,
    /// Trigger module: minimal mcontrol address-match triggers like Spike's.
    pub tselect: u64,
    pub tdata1: [u64; 4],
    pub tdata2: [u64; 4],
    /// Set when minstret/mcycle written by the current instruction: the
    /// increment at retirement is suppressed (Spike's behavior).
    instret_written: bool,
    cycle_written: bool,
    /// tohost address if the loaded image has the HTIF symbol.
    pub tohost: Option<u64>,
    /// Last nonzero value written to tohost, if any.
    pub tohost_value: Option<u64>,
    /// Canonical trace line for the last retired instruction (built only
    /// when `trace_enabled`).
    pub trace_enabled: bool,
    pub trace_line: String,
}

impl Cpu {
    pub fn new(bus: Bus, entry: u64) -> Self {
        Cpu {
            regs: [0; 32],
            pc: entry,
            prv: PRV_M,
            csrs: Csrs::new(),
            bus,
            reservation: None,
            retired: 0,
            tselect: 0,
            tdata1: [0; 4],
            tdata2: [0; 4],
            instret_written: false,
            cycle_written: false,
            tohost: None,
            tohost_value: None,
            trace_enabled: false,
            trace_line: String::new(),
        }
    }

    fn x(&self, r: u32) -> u64 {
        self.regs[r as usize]
    }

    fn set_x(&mut self, r: u32, v: u64) {
        if r != 0 {
            self.regs[r as usize] = v;
            if self.trace_enabled {
                self.trace_line.push_str(&format!(" x{}=0x{:016x}", r, v));
            }
        }
    }

    fn trace_load(&mut self, addr: u64) {
        if self.trace_enabled {
            self.trace_line.push_str(&format!(" m:0x{:016x}", addr));
        }
    }

    fn trace_store(&mut self, addr: u64, val: u64, size: u64) {
        if self.trace_enabled {
            let digits = (size * 2) as usize;
            self.trace_line
                .push_str(&format!(" m:0x{:016x}=0x{:0digits$x}", addr, val & mask_bytes(size)));
        }
    }

    fn trace_csr(&mut self, name: &str, val: u64) {
        if self.trace_enabled {
            self.trace_line.push_str(&format!(" c:{}=0x{:016x}", name, val));
        }
    }

    // ---- timers / interrupt composition -------------------------------

    fn mtime(&self) -> u64 {
        self.bus.clint.mtime
    }

    fn stce(&self) -> bool {
        self.csrs.menvcfg >> 63 & 1 != 0
    }

    /// Effective mip: software bits + CLINT-driven MTIP/MSIP + Sstc STIP.
    pub fn mip(&self) -> u64 {
        let mut v = self.csrs.mip_sw;
        if self.bus.clint.mtime >= self.bus.clint.mtimecmp {
            v |= csr::IRQ_MTIP;
        }
        if self.bus.clint.msip {
            v |= csr::IRQ_MSIP;
        }
        if self.stce() && self.mtime() >= self.csrs.stimecmp {
            v |= csr::IRQ_STIP;
        }
        v
    }

    /// Take the highest-priority enabled pending interrupt, if any is
    /// actionable at the current privilege. Returns true if a trap was taken.
    pub fn take_interrupt(&mut self) -> bool {
        let pending = self.mip() & self.csrs.mie;
        if pending == 0 {
            return false;
        }
        let mie = self.csrs.mstatus & csr::MSTATUS_MIE != 0;
        let sie = self.csrs.mstatus & csr::MSTATUS_SIE != 0;

        // M-mode interrupts: not delegated, taken if prv < M or (prv == M and MIE).
        let m_enabled = self.prv < PRV_M || (self.prv == PRV_M && mie);
        let m_pending = pending & !self.csrs.mideleg;
        // S-mode interrupts: delegated, taken if prv < S or (prv == S and SIE).
        let s_enabled = self.prv < PRV_S || (self.prv == PRV_S && sie);
        let s_pending = pending & self.csrs.mideleg;

        // Priority: MEI, MSI, MTI, SEI, SSI, STI.
        const ORDER: [u64; 6] = [11, 3, 7, 9, 1, 5];
        let take = |set: u64| ORDER.iter().copied().find(|b| set >> b & 1 != 0);

        if m_enabled {
            if let Some(cause) = take(m_pending) {
                self.trap_to_m(cause | (1 << 63), 0, true);
                return true;
            }
        }
        if s_enabled {
            if let Some(cause) = take(s_pending) {
                self.trap_to_s(cause | (1 << 63), 0, true);
                return true;
            }
        }
        false
    }

    fn trap_to_m(&mut self, cause: u64, tval: u64, interrupt: bool) {
        let epc = self.pc;
        self.csrs.mepc = epc & !1;
        self.csrs.mcause = cause;
        self.csrs.mtval = tval;
        let mut ms = self.csrs.mstatus;
        // MPIE <- MIE, MIE <- 0, MPP <- prv
        let mie = (ms >> 3) & 1;
        ms = (ms & !csr::MSTATUS_MPIE) | (mie << 7);
        ms &= !csr::MSTATUS_MIE;
        ms = (ms & !csr::MSTATUS_MPP_MASK) | ((self.prv as u64) << 11);
        self.csrs.mstatus = ms;
        self.prv = PRV_M;
        let base = self.csrs.mtvec & !3;
        self.pc = if self.csrs.mtvec & 3 == 1 && interrupt {
            base + 4 * (cause & 0xff)
        } else {
            base
        };
    }

    fn trap_to_s(&mut self, cause: u64, tval: u64, interrupt: bool) {
        let epc = self.pc;
        self.csrs.sepc = epc & !1;
        self.csrs.scause = cause;
        self.csrs.stval = tval;
        let mut ms = self.csrs.mstatus;
        let sie = (ms >> 1) & 1;
        ms = (ms & !csr::MSTATUS_SPIE) | (sie << 5);
        ms &= !csr::MSTATUS_SIE;
        ms = (ms & !csr::MSTATUS_SPP) | ((self.prv as u64 & 1) << 8);
        self.csrs.mstatus = ms;
        self.prv = PRV_S;
        let base = self.csrs.stvec & !3;
        self.pc = if self.csrs.stvec & 3 == 1 && interrupt {
            base + 4 * (cause & 0xff)
        } else {
            base
        };
    }

    /// Route a synchronous exception per medeleg.
    fn take_exception(&mut self, e: Exception) {
        let cause = e.cause();
        let deleg = self.csrs.medeleg >> cause & 1 != 0 && self.prv < PRV_M;
        if deleg {
            self.trap_to_s(cause, e.tval(), false);
        } else {
            self.trap_to_m(cause, e.tval(), false);
        }
    }

    // ---- CSR access ----------------------------------------------------

    fn csr_name(addr: u16) -> &'static str {
        match addr {
            csr::CYCLE => "cycle",
            csr::TIME => "time",
            csr::INSTRET => "instret",
            csr::SSTATUS => "sstatus",
            csr::SIE => "sie",
            csr::STVEC => "stvec",
            csr::SCOUNTEREN => "scounteren",
            csr::SENVCFG => "senvcfg",
            csr::SSCRATCH => "sscratch",
            csr::SEPC => "sepc",
            csr::SCAUSE => "scause",
            csr::STVAL => "stval",
            csr::SIP => "sip",
            csr::STIMECMP => "stimecmp",
            csr::SATP => "satp",
            csr::MSTATUS => "mstatus",
            csr::MISA => "misa",
            csr::MEDELEG => "medeleg",
            csr::MIDELEG => "mideleg",
            csr::MIE => "mie",
            csr::MTVEC => "mtvec",
            csr::MCOUNTEREN => "mcounteren",
            csr::MENVCFG => "menvcfg",
            csr::MCOUNTINHIBIT => "mcountinhibit",
            csr::MSCRATCH => "mscratch",
            csr::MEPC => "mepc",
            csr::MCAUSE => "mcause",
            csr::MTVAL => "mtval",
            csr::MIP => "mip",
            csr::PMPCFG0 => "pmpcfg0",
            0x7a0 => "tselect",
            0x7a1 => "tdata1",
            0x7a2 => "tdata2",
            0x7a3 => "tdata3",
            0x7a4 => "tinfo",
            csr::MCYCLE => "mcycle",
            csr::MINSTRET => "minstret",
            csr::MVENDORID => "mvendorid",
            csr::MARCHID => "marchid",
            csr::MIMPID => "mimpid",
            csr::MHARTID => "mhartid",
            _ => {
                if (csr::PMPADDR0..csr::PMPADDR0 + 16).contains(&addr) {
                    const NAMES: [&str; 16] = [
                        "pmpaddr0", "pmpaddr1", "pmpaddr2", "pmpaddr3", "pmpaddr4", "pmpaddr5",
                        "pmpaddr6", "pmpaddr7", "pmpaddr8", "pmpaddr9", "pmpaddr10", "pmpaddr11",
                        "pmpaddr12", "pmpaddr13", "pmpaddr14", "pmpaddr15",
                    ];
                    NAMES[(addr - csr::PMPADDR0) as usize]
                } else {
                    "unknown"
                }
            }
        }
    }

    fn check_counter(&self, addr: u16) -> Result<(), ()> {
        // cycle/time/instret gating via mcounteren (S/U) and scounteren (U).
        let bit = match addr {
            csr::CYCLE => 0,
            csr::TIME => 1,
            csr::INSTRET => 2,
            _ => return Ok(()),
        };
        if self.prv < PRV_M && self.csrs.mcounteren >> bit & 1 == 0 {
            return Err(());
        }
        if self.prv == PRV_U && self.csrs.scounteren >> bit & 1 == 0 {
            return Err(());
        }
        Ok(())
    }

    fn csr_read(&mut self, addr: u16, insn: u64) -> Result<u64, Exception> {
        // Privilege check from address bits [9:8].
        if (addr >> 8 & 3) as u8 > self.prv {
            return Err(Exception::IllegalInstruction(insn));
        }
        let v = match addr {
            csr::CYCLE => {
                self.check_counter(addr).map_err(|_| Exception::IllegalInstruction(insn))?;
                self.csrs.cycle
            }
            csr::INSTRET => {
                self.check_counter(addr).map_err(|_| Exception::IllegalInstruction(insn))?;
                self.csrs.instret
            }
            csr::TIME => {
                self.check_counter(addr).map_err(|_| Exception::IllegalInstruction(insn))?;
                self.mtime()
            }
            csr::MCYCLE => self.csrs.cycle,
            csr::MINSTRET => self.csrs.instret,
            csr::SSTATUS => csr::sstatus_view(self.csrs.mstatus),
            csr::SIE => self.csrs.mie & csr::CsrMasks::SIE_MASK,
            csr::STVEC => self.csrs.stvec,
            csr::SCOUNTEREN => self.csrs.scounteren,
            csr::SENVCFG => self.csrs.senvcfg,
            csr::SSCRATCH => self.csrs.sscratch,
            csr::SEPC => self.csrs.sepc,
            csr::SCAUSE => self.csrs.scause,
            csr::STVAL => self.csrs.stval,
            csr::SIP => self.mip() & csr::CsrMasks::SIE_MASK,
            csr::STIMECMP => {
                if self.prv < PRV_M && (!self.stce() || self.csrs.mcounteren >> 1 & 1 == 0) {
                    return Err(Exception::IllegalInstruction(insn));
                }
                self.csrs.stimecmp
            }
            csr::SATP => {
                if self.prv == PRV_S && self.csrs.mstatus & csr::MSTATUS_TVM != 0 {
                    return Err(Exception::IllegalInstruction(insn));
                }
                self.csrs.satp
            }
            csr::MSTATUS => self.csrs.mstatus,
            csr::MISA => csr::MISA_VALUE,
            csr::MEDELEG => self.csrs.medeleg,
            csr::MIDELEG => self.csrs.mideleg,
            csr::MIE => self.csrs.mie,
            csr::MTVEC => self.csrs.mtvec,
            csr::MCOUNTEREN => self.csrs.mcounteren,
            csr::MENVCFG => self.csrs.menvcfg,
            csr::MCOUNTINHIBIT => self.csrs.mcountinhibit,
            csr::MSCRATCH => self.csrs.mscratch,
            csr::MEPC => self.csrs.mepc,
            csr::MCAUSE => self.csrs.mcause,
            csr::MTVAL => self.csrs.mtval,
            csr::MIP => self.mip(),
            csr::PMPCFG0 => self.csrs.pmpcfg0,
            0x7a0 => self.tselect,
            0x7a1 => self.tdata1[self.tselect as usize & 3],
            0x7a2 => self.tdata2[self.tselect as usize & 3],
            0x7a3 => 0,
            0x7a4 => 1 << 2, // tinfo: mcontrol (type 2) supported
            csr::MVENDORID | csr::MARCHID | csr::MIMPID => 0,
            csr::MHARTID => 0,
            a if (csr::PMPADDR0..csr::PMPADDR0 + 16).contains(&a) => {
                self.csrs.pmpaddr[(a - csr::PMPADDR0) as usize]
            }
            _ => return Err(Exception::IllegalInstruction(insn)),
        };
        Ok(v)
    }

    /// Write a CSR; returns the legalized stored value for tracing.
    fn csr_write(&mut self, addr: u16, val: u64, insn: u64) -> Result<u64, Exception> {
        if (addr >> 8 & 3) as u8 > self.prv || addr >> 10 == 3 {
            // read-only region (top two bits 11) or insufficient privilege
            return Err(Exception::IllegalInstruction(insn));
        }
        let stored = match addr {
            csr::SSTATUS => {
                let m = csr::CsrMasks::SSTATUS_MASK & csr::CsrMasks::MSTATUS_WMASK;
                self.csrs.mstatus = (self.csrs.mstatus & !m) | (val & m);
                csr::sstatus_view(self.csrs.mstatus)
            }
            csr::SIE => {
                let m = csr::CsrMasks::SIE_MASK & self.csrs.mideleg;
                // Spike: sie writes affect only delegated bits.
                self.csrs.mie = (self.csrs.mie & !m) | (val & m);
                self.csrs.mie & csr::CsrMasks::SIE_MASK
            }
            csr::STVEC => {
                self.csrs.stvec = csr::legalize_mtvec(val);
                self.csrs.stvec
            }
            csr::SCOUNTEREN => {
                self.csrs.scounteren = val & 7;
                self.csrs.scounteren
            }
            csr::SENVCFG => {
                self.csrs.senvcfg = val & 1; // FIOM only
                self.csrs.senvcfg
            }
            csr::SSCRATCH => {
                self.csrs.sscratch = val;
                val
            }
            csr::SEPC => {
                self.csrs.sepc = val & !1;
                self.csrs.sepc
            }
            csr::SCAUSE => {
                self.csrs.scause = val;
                val
            }
            csr::STVAL => {
                self.csrs.stval = val;
                val
            }
            csr::SIP => {
                // Only SSIP writable through sip, and only if delegated.
                let m = csr::IRQ_SSIP & self.csrs.mideleg;
                self.csrs.mip_sw = (self.csrs.mip_sw & !m) | (val & m);
                self.mip() & csr::CsrMasks::SIE_MASK
            }
            csr::STIMECMP => {
                if self.prv < PRV_M && (!self.stce() || self.csrs.mcounteren >> 1 & 1 == 0) {
                    return Err(Exception::IllegalInstruction(insn));
                }
                self.csrs.stimecmp = val;
                val
            }
            csr::SATP => {
                if self.prv == PRV_S && self.csrs.mstatus & csr::MSTATUS_TVM != 0 {
                    return Err(Exception::IllegalInstruction(insn));
                }
                let mode = val >> 60;
                if mode == 0 || mode == 8 {
                    // ASID: WARL, we support 0 bits -> hardwire to 0.
                    self.csrs.satp = val & 0x8000_0fff_ffff_ffff & !(0xffff << 44);
                }
                self.csrs.satp
            }
            csr::MSTATUS => {
                self.csrs.mstatus = csr::legalize_mstatus(self.csrs.mstatus, val);
                self.csrs.mstatus
            }
            csr::MISA => csr::MISA_VALUE, // writes ignored
            csr::MEDELEG => {
                self.csrs.medeleg = val & csr::CsrMasks::MEDELEG_WMASK;
                self.csrs.medeleg
            }
            csr::MIDELEG => {
                self.csrs.mideleg = val & csr::CsrMasks::MIDELEG_WMASK;
                self.csrs.mideleg
            }
            csr::MIE => {
                self.csrs.mie = val & csr::CsrMasks::MIE_WMASK;
                self.csrs.mie
            }
            csr::MTVEC => {
                self.csrs.mtvec = csr::legalize_mtvec(val);
                self.csrs.mtvec
            }
            csr::MCOUNTEREN => {
                self.csrs.mcounteren = val & 7;
                self.csrs.mcounteren
            }
            csr::MENVCFG => {
                self.csrs.menvcfg = val & csr::CsrMasks::MENVCFG_WMASK;
                self.csrs.menvcfg
            }
            csr::MCOUNTINHIBIT => {
                self.csrs.mcountinhibit = val & 5;
                self.csrs.mcountinhibit
            }
            csr::MSCRATCH => {
                self.csrs.mscratch = val;
                val
            }
            csr::MEPC => {
                self.csrs.mepc = val & !1;
                self.csrs.mepc
            }
            csr::MCAUSE => {
                self.csrs.mcause = val;
                val
            }
            csr::MTVAL => {
                self.csrs.mtval = val;
                val
            }
            csr::MIP => {
                let m = csr::CsrMasks::MIP_WMASK | if self.stce() { 0 } else { csr::IRQ_STIP };
                self.csrs.mip_sw = (self.csrs.mip_sw & !m) | (val & m);
                self.mip()
            }
            csr::PMPCFG0 => {
                self.csrs.pmpcfg0 = val;
                val
            }
            0x7a0 => {
                // 4 triggers: writes of higher values are clamped like Spike.
                self.tselect = val.min(3);
                self.tselect
            }
            0x7a1 => {
                let i = self.tselect as usize & 3;
                // mcontrol only: type forced to 2, dmode 0; writable bits:
                // action[15:12] (0 only), match[10:7] (0 only), m/s/u,
                // execute/store/load.
                let wmask: u64 = (1 << 6) | (1 << 4) | (1 << 3) | 7;
                self.tdata1[i] = (2u64 << 60) | (val & wmask);
                self.tdata1[i]
            }
            0x7a2 => {
                let i = self.tselect as usize & 3;
                self.tdata2[i] = val;
                val
            }
            0x7a3 => 0,
            csr::MCYCLE => {
                self.csrs.cycle = val;
                self.cycle_written = true;
                val
            }
            csr::MINSTRET => {
                self.csrs.instret = val;
                self.instret_written = true;
                val
            }
            a if (csr::PMPADDR0..csr::PMPADDR0 + 16).contains(&a) => {
                self.csrs.pmpaddr[(a - csr::PMPADDR0) as usize] = val & 0x3f_ffff_ffff_ffff;
                self.csrs.pmpaddr[(a - csr::PMPADDR0) as usize]
            }
            _ => return Err(Exception::IllegalInstruction(insn)),
        };
        Ok(stored)
    }

    /// mcontrol trigger check. kind: 2=execute, 1=store, 0=load.
    fn trigger_hit(&self, kind: u32, addr: u64) -> bool {
        for i in 0..4 {
            let t = self.tdata1[i];
            if t >> 60 != 2 || t >> kind & 1 == 0 {
                continue;
            }
            let mode_ok = match self.prv {
                PRV_M => t >> 6 & 1 != 0,
                PRV_S => t >> 4 & 1 != 0,
                _ => t >> 3 & 1 != 0,
            };
            if mode_ok && self.tdata2[i] == addr {
                return true;
            }
        }
        false
    }

    // ---- memory (Gate A: physical only) --------------------------------

    // The pinned Spike supports misaligned data accesses in hardware
    // (rv64ui-p-ma_data expects them to succeed), so plain loads/stores do
    // not take misalignment exceptions; only AMO/LR/SC do.
    // Note: loads are traced by the caller after the register writeback, so
    // the canonical token order matches Spike (x then m).
    fn load(&mut self, vaddr: u64, size: u64) -> Result<u64, Exception> {
        if self.trigger_hit(0, vaddr) {
            return Err(Exception::Breakpoint(vaddr));
        }
        self.bus.load(vaddr, size)
    }

    fn store(&mut self, vaddr: u64, val: u64, size: u64) -> Result<(), Exception> {
        if self.trigger_hit(1, vaddr) {
            return Err(Exception::Breakpoint(vaddr));
        }
        self.bus.store(vaddr, val, size)?;
        self.trace_store(vaddr, val, size);
        if self.tohost == Some(vaddr) && val != 0 {
            self.tohost_value = Some(val);
        }
        Ok(())
    }

    // ---- fetch/step -----------------------------------------------------

    fn fetch(&mut self) -> Result<(u32, u32), Exception> {
        // Returns (raw bits as fetched, expanded 32-bit instruction).
        let pc = self.pc;
        let lo = self
            .bus
            .load(pc, 2)
            .map_err(|_| Exception::InstructionAccessFault(pc))? as u32;
        if lo & 3 == 3 {
            let hi = self
                .bus
                .load(pc + 2, 2)
                .map_err(|_| Exception::InstructionAccessFault(pc))? as u32;
            let insn = lo | (hi << 16);
            Ok((insn, insn))
        } else {
            let expanded = expand_compressed(lo as u16).ok_or(Exception::IllegalInstruction(lo as u64))?;
            Ok((lo, expanded))
        }
    }

    /// Execute one instruction (or take one trap / wait).
    pub fn step(&mut self) -> StepResult {
        if self.take_interrupt() {
            return StepResult::Trapped;
        }
        if self.trace_enabled {
            self.trace_line.clear();
        }
        if self.trigger_hit(2, self.pc) {
            self.take_exception(Exception::Breakpoint(self.pc));
            return StepResult::Trapped;
        }
        let (raw, insn) = match self.fetch() {
            Ok(v) => v,
            Err(e) => {
                self.take_exception(e);
                return StepResult::Trapped;
            }
        };
        if self.trace_enabled {
            self.trace_line = format!("p{} {:016x} {:08x}", self.prv, self.pc, raw);
        }
        let ilen = if raw & 3 == 3 { 4 } else { 2 };
        match self.execute(insn, ilen) {
            Ok(next) => {
                self.pc = next;
                self.retire();
                StepResult::Retired
            }
            Err(ExecOutcome::Exception(e)) => {
                self.take_exception(e);
                StepResult::Trapped
            }
            Err(ExecOutcome::Wfi) => {
                // Retire the wfi itself; the run loop advances time until an
                // interrupt is pending (matching Spike's idle fast-forward).
                self.pc += ilen;
                self.retire();
                StepResult::WaitingForInterrupt
            }
        }
    }

    fn retire(&mut self) {
        self.retired = self.retired.wrapping_add(1);
        if !self.instret_written {
            self.csrs.instret = self.csrs.instret.wrapping_add(1);
        }
        if !self.cycle_written {
            self.csrs.cycle = self.csrs.cycle.wrapping_add(1);
        }
        self.instret_written = false;
        self.cycle_written = false;
        if self.retired % INSNS_PER_RTC_TICK == 0 {
            self.bus.clint.mtime = self.bus.clint.mtime.wrapping_add(1);
        }
    }

    /// Advance time while idle in WFI (no instructions retiring).
    pub fn idle_tick(&mut self) {
        self.bus.clint.mtime = self.bus.clint.mtime.wrapping_add(1);
    }

    pub fn has_pending_interrupt(&self) -> bool {
        self.mip() & self.csrs.mie != 0
    }

    // ---- execute ---------------------------------------------------------

    fn execute(&mut self, insn: u32, ilen: u64) -> Result<u64, ExecOutcome> {
        let pc = self.pc;
        let next = pc.wrapping_add(ilen);
        let op = insn & 0x7f;
        let rd = (insn >> 7) & 31;
        let rs1 = (insn >> 15) & 31;
        let rs2 = (insn >> 20) & 31;
        let f3 = (insn >> 12) & 7;
        let f7 = insn >> 25;
        let i_imm = (insn as i32 >> 20) as i64 as u64;

        match op {
            0x37 => {
                // lui
                self.set_x(rd, (insn & 0xffff_f000) as i32 as i64 as u64);
                Ok(next)
            }
            0x17 => {
                // auipc
                self.set_x(rd, pc.wrapping_add((insn & 0xffff_f000) as i32 as i64 as u64));
                Ok(next)
            }
            0x6f => {
                // jal
                let imm = ((insn >> 31) as u64) << 20
                    | ((insn >> 12 & 0xff) as u64) << 12
                    | ((insn >> 20 & 1) as u64) << 11
                    | ((insn >> 21 & 0x3ff) as u64) << 1;
                let imm = sext(imm, 21);
                self.set_x(rd, next);
                Ok(pc.wrapping_add(imm))
            }
            0x67 => {
                // jalr
                let t = self.x(rs1).wrapping_add(i_imm) & !1;
                self.set_x(rd, next);
                Ok(t)
            }
            0x63 => {
                // branches
                let imm = ((insn >> 31) as u64) << 12
                    | ((insn >> 7 & 1) as u64) << 11
                    | ((insn >> 25 & 0x3f) as u64) << 5
                    | ((insn >> 8 & 0xf) as u64) << 1;
                let imm = sext(imm, 13);
                let a = self.x(rs1);
                let b = self.x(rs2);
                let taken = match f3 {
                    0 => a == b,
                    1 => a != b,
                    4 => (a as i64) < (b as i64),
                    5 => (a as i64) >= (b as i64),
                    6 => a < b,
                    7 => a >= b,
                    _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
                };
                Ok(if taken { pc.wrapping_add(imm) } else { next })
            }
            0x03 => {
                // loads
                let addr = self.x(rs1).wrapping_add(i_imm);
                let v = match f3 {
                    0 => self.load(addr, 1)? as i8 as i64 as u64,
                    1 => self.load(addr, 2)? as i16 as i64 as u64,
                    2 => self.load(addr, 4)? as i32 as i64 as u64,
                    3 => self.load(addr, 8)?,
                    4 => self.load(addr, 1)?,
                    5 => self.load(addr, 2)?,
                    6 => self.load(addr, 4)?,
                    _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
                };
                self.set_x(rd, v);
                self.trace_load(addr);
                Ok(next)
            }
            0x23 => {
                // stores
                let imm = sext(((f7 as u64) << 5) | rd as u64, 12);
                let addr = self.x(rs1).wrapping_add(imm);
                let v = self.x(rs2);
                match f3 {
                    0 => self.store(addr, v, 1)?,
                    1 => self.store(addr, v, 2)?,
                    2 => self.store(addr, v, 4)?,
                    3 => self.store(addr, v, 8)?,
                    _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
                }
                Ok(next)
            }
            0x13 => {
                // op-imm
                let a = self.x(rs1);
                let sh = (insn >> 20 & 0x3f) as u32;
                let v = match f3 {
                    0 => a.wrapping_add(i_imm),
                    1 if f7 >> 1 == 0 => a << sh,
                    2 => ((a as i64) < (i_imm as i64)) as u64,
                    3 => (a < i_imm) as u64,
                    4 => a ^ i_imm,
                    5 if f7 >> 1 == 0 => a >> sh,
                    5 if f7 >> 1 == 0x10 => ((a as i64) >> sh) as u64,
                    6 => a | i_imm,
                    7 => a & i_imm,
                    _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
                };
                self.set_x(rd, v);
                Ok(next)
            }
            0x1b => {
                // op-imm-32
                let a = self.x(rs1) as u32;
                let sh = rs2;
                let v: u64 = match (f3, f7) {
                    (0, _) => (a.wrapping_add(i_imm as u32) as i32) as i64 as u64,
                    (1, 0) => ((a << sh) as i32) as i64 as u64,
                    (5, 0) => ((a >> sh) as i32) as i64 as u64,
                    (5, 0x20) => ((a as i32) >> sh) as i64 as u64,
                    _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
                };
                self.set_x(rd, v);
                Ok(next)
            }
            0x33 => {
                // op
                let a = self.x(rs1);
                let b = self.x(rs2);
                let v = match (f3, f7) {
                    (0, 0) => a.wrapping_add(b),
                    (0, 0x20) => a.wrapping_sub(b),
                    (1, 0) => a << (b & 63),
                    (2, 0) => ((a as i64) < (b as i64)) as u64,
                    (3, 0) => (a < b) as u64,
                    (4, 0) => a ^ b,
                    (5, 0) => a >> (b & 63),
                    (5, 0x20) => ((a as i64) >> (b & 63)) as u64,
                    (6, 0) => a | b,
                    (7, 0) => a & b,
                    // M
                    (0, 1) => a.wrapping_mul(b),
                    (1, 1) => (((a as i64 as i128) * (b as i64 as i128)) >> 64) as u64,
                    (2, 1) => (((a as i64 as i128) * (b as u128 as i128)) >> 64) as u64,
                    (3, 1) => (((a as u128) * (b as u128)) >> 64) as u64,
                    (4, 1) => {
                        if b == 0 {
                            u64::MAX
                        } else if a as i64 == i64::MIN && b as i64 == -1 {
                            a
                        } else {
                            ((a as i64) / (b as i64)) as u64
                        }
                    }
                    (5, 1) => {
                        if b == 0 {
                            u64::MAX
                        } else {
                            a / b
                        }
                    }
                    (6, 1) => {
                        if b == 0 {
                            a
                        } else if a as i64 == i64::MIN && b as i64 == -1 {
                            0
                        } else {
                            ((a as i64) % (b as i64)) as u64
                        }
                    }
                    (7, 1) => {
                        if b == 0 {
                            a
                        } else {
                            a % b
                        }
                    }
                    _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
                };
                self.set_x(rd, v);
                Ok(next)
            }
            0x3b => {
                // op-32
                let a = self.x(rs1) as u32;
                let b = self.x(rs2) as u32;
                let v: u64 = match (f3, f7) {
                    (0, 0) => (a.wrapping_add(b) as i32) as i64 as u64,
                    (0, 0x20) => (a.wrapping_sub(b) as i32) as i64 as u64,
                    (1, 0) => ((a << (b & 31)) as i32) as i64 as u64,
                    (5, 0) => ((a >> (b & 31)) as i32) as i64 as u64,
                    (5, 0x20) => ((a as i32) >> (b & 31)) as i64 as u64,
                    (0, 1) => (a.wrapping_mul(b) as i32) as i64 as u64,
                    (4, 1) => {
                        let (a, b) = (a as i32, b as i32);
                        (if b == 0 {
                            -1
                        } else if a == i32::MIN && b == -1 {
                            a
                        } else {
                            a / b
                        }) as i64 as u64
                    }
                    (5, 1) => (if b == 0 { u32::MAX as i32 } else { (a / b) as i32 }) as i64 as u64,
                    (6, 1) => {
                        let (a, b) = (a as i32, b as i32);
                        (if b == 0 {
                            a
                        } else if a == i32::MIN && b == -1 {
                            0
                        } else {
                            a % b
                        }) as i64 as u64
                    }
                    (7, 1) => (if b == 0 { a as i32 } else { (a % b) as i32 }) as i64 as u64,
                    _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
                };
                self.set_x(rd, v);
                Ok(next)
            }
            0x0f => {
                // fence / fence.i: no-ops for a single in-order hart.
                Ok(next)
            }
            0x2f => self.execute_amo(insn, next),
            0x73 => self.execute_system(insn, next),
            _ => Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
        }
    }

    fn execute_amo(&mut self, insn: u32, next: u64) -> Result<u64, ExecOutcome> {
        let rd = (insn >> 7) & 31;
        let rs1 = (insn >> 15) & 31;
        let rs2 = (insn >> 20) & 31;
        let f3 = (insn >> 12) & 7;
        let f5 = insn >> 27;
        let size: u64 = match f3 {
            2 => 4,
            3 => 8,
            _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
        };
        let addr = self.x(rs1);
        if addr % size != 0 {
            // AMOs raise store/AMO misaligned.
            return Err(ExecOutcome::Exception(Exception::StoreAddressMisaligned(addr)));
        }
        match f5 {
            0x02 => {
                // lr
                if rs2 != 0 {
                    return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64)));
                }
                let v = self.bus.load(addr, size).map_err(ExecOutcome::Exception)?;
                let v = if size == 4 { v as i32 as i64 as u64 } else { v };
                self.reservation = Some(addr);
                self.set_x(rd, v);
                self.trace_load(addr);
                Ok(next)
            }
            0x03 => {
                // sc
                if self.reservation == Some(addr) {
                    self.bus
                        .store(addr, self.x(rs2), size)
                        .map_err(ExecOutcome::Exception)?;
                    self.set_x(rd, 0);
                    self.trace_store(addr, self.x(rs2), size);
                } else {
                    self.set_x(rd, 1);
                }
                self.reservation = None;
                Ok(next)
            }
            _ => {
                let old = self.bus.load(addr, size).map_err(ExecOutcome::Exception)?;
                let old_sx = if size == 4 { old as i32 as i64 as u64 } else { old };
                let b = self.x(rs2);
                let newv = match f5 {
                    0x00 => old_sx.wrapping_add(b),
                    0x01 => b,
                    0x04 => old_sx ^ b,
                    0x08 => old_sx | b,
                    0x0c => old_sx & b,
                    0x10 => {
                        if size == 4 {
                            ((old_sx as i64).min(b as i32 as i64)) as u64
                        } else {
                            ((old_sx as i64).min(b as i64)) as u64
                        }
                    }
                    0x14 => {
                        if size == 4 {
                            ((old_sx as i64).max(b as i32 as i64)) as u64
                        } else {
                            ((old_sx as i64).max(b as i64)) as u64
                        }
                    }
                    0x18 => {
                        if size == 4 {
                            (old & 0xffff_ffff).min(b & 0xffff_ffff)
                        } else {
                            old.min(b)
                        }
                    }
                    0x1c => {
                        if size == 4 {
                            (old & 0xffff_ffff).max(b & 0xffff_ffff)
                        } else {
                            old.max(b)
                        }
                    }
                    _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
                };
                self.bus
                    .store(addr, newv, size)
                    .map_err(ExecOutcome::Exception)?;
                self.set_x(rd, old_sx);
                // Spike logs AMO memory as load addr then store addr=val.
                self.trace_load(addr);
                self.trace_store(addr, newv, size);
                Ok(next)
            }
        }
    }

    fn execute_system(&mut self, insn: u32, next: u64) -> Result<u64, ExecOutcome> {
        let rd = (insn >> 7) & 31;
        let rs1 = (insn >> 15) & 31;
        let f3 = (insn >> 12) & 7;
        let csr_addr = (insn >> 20) as u16;

        if f3 == 0 {
            return match insn {
                0x0000_0073 => {
                    // ecall
                    Err(ExecOutcome::Exception(match self.prv {
                        PRV_M => Exception::EcallFromM,
                        PRV_S => Exception::EcallFromS,
                        _ => Exception::EcallFromU,
                    }))
                }
                0x0010_0073 => Err(ExecOutcome::Exception(Exception::Breakpoint(self.pc))),
                0x3020_0073 => {
                    // mret
                    if self.prv != PRV_M {
                        return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64)));
                    }
                    let ms = self.csrs.mstatus;
                    let mpp = ((ms >> 11) & 3) as u8;
                    let mpie = (ms >> 7) & 1;
                    let mut new = ms;
                    new = (new & !csr::MSTATUS_MIE) | (mpie << 3);
                    new |= csr::MSTATUS_MPIE;
                    new &= !csr::MSTATUS_MPP_MASK; // MPP <- U
                    if mpp != PRV_M {
                        new &= !csr::MSTATUS_MPRV;
                    }
                    self.csrs.mstatus = new;
                    self.prv = mpp;
                    self.trace_csr("mstatus", new);
                    Ok(self.csrs.mepc)
                }
                0x1020_0073 => {
                    // sret
                    if self.prv < PRV_S
                        || (self.prv == PRV_S && self.csrs.mstatus & csr::MSTATUS_TSR != 0)
                    {
                        return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64)));
                    }
                    let ms = self.csrs.mstatus;
                    let spp = ((ms >> 8) & 1) as u8;
                    let spie = (ms >> 5) & 1;
                    let mut new = ms;
                    new = (new & !csr::MSTATUS_SIE) | (spie << 1);
                    new |= csr::MSTATUS_SPIE;
                    new &= !csr::MSTATUS_SPP;
                    if spp != PRV_M {
                        new &= !csr::MSTATUS_MPRV;
                    }
                    self.csrs.mstatus = new;
                    self.prv = spp;
                    self.trace_csr("mstatus", new);
                    Ok(self.csrs.sepc)
                }
                0x1050_0073 => {
                    // wfi
                    if self.prv == PRV_U
                        || (self.prv == PRV_S && self.csrs.mstatus & csr::MSTATUS_TW != 0)
                    {
                        return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64)));
                    }
                    Err(ExecOutcome::Wfi)
                }
                _ if insn >> 25 == 0x09 => {
                    // sfence.vma: no TLB caching in Gate A/B design -> no-op,
                    // but privilege/TVM check applies.
                    if self.prv == PRV_U
                        || (self.prv == PRV_S && self.csrs.mstatus & csr::MSTATUS_TVM != 0)
                    {
                        return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64)));
                    }
                    Ok(next)
                }
                _ => Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
            };
        }

        // Zicsr
        let uimm = rs1 as u64;
        let (do_read, do_write) = match f3 {
            1 | 5 => (rd != 0, true),           // csrrw[i]: read only if rd != 0
            2 | 6 => (true, rs1 != 0),          // csrrs[i]: write only if rs1 != 0
            3 | 7 => (true, rs1 != 0),          // csrrc[i]
            _ => return Err(ExecOutcome::Exception(Exception::IllegalInstruction(insn as u64))),
        };
        let old = if do_read || matches!(f3, 2 | 3 | 6 | 7) {
            self.csr_read(csr_addr, insn as u64).map_err(ExecOutcome::Exception)?
        } else {
            // csrrw with rd=0 still needs write permission checks; probe via
            // write path below.
            0
        };
        let src = if f3 >= 5 { uimm } else { self.x(rs1) };
        let newv = match f3 {
            1 | 5 => src,
            2 | 6 => old | src,
            _ => old & !src,
        };
        if do_write {
            let stored = self
                .csr_write(csr_addr, newv, insn as u64)
                .map_err(ExecOutcome::Exception)?;
            self.set_x(rd, old);
            let name = Self::csr_name(csr_addr);
            // Counter CSRs are excluded from comparison anyway; still emit.
            self.trace_csr(name, stored);
        } else {
            self.set_x(rd, old);
        }
        Ok(next)
    }
}

enum ExecOutcome {
    Exception(Exception),
    Wfi,
}

impl From<Exception> for ExecOutcome {
    fn from(e: Exception) -> Self {
        ExecOutcome::Exception(e)
    }
}

fn sext(v: u64, bits: u32) -> u64 {
    let shift = 64 - bits;
    ((v << shift) as i64 >> shift) as u64
}

fn mask_bytes(size: u64) -> u64 {
    match size {
        1 => 0xff,
        2 => 0xffff,
        4 => 0xffff_ffff,
        _ => u64::MAX,
    }
}

/// Expand a 16-bit compressed instruction to its 32-bit equivalent.
/// Returns None for illegal/reserved encodings (including all-zeros).
pub fn expand_compressed(c: u16) -> Option<u32> {
    let op = c & 3;
    let f3 = (c >> 13) & 7;
    let r = |n: u16| n as u32; // full register number
    let rc = |n: u16| (n & 7) as u32 + 8; // compressed register x8..x15

    let rd = r((c >> 7) & 31);
    let rs2 = r((c >> 2) & 31);
    let rd_c = rc(c >> 2);
    let rs1_c = rc(c >> 7);

    // Helpers to assemble 32-bit encodings.
    let itype = |imm: i32, rs1: u32, f3: u32, rd: u32, op: u32| {
        Some(((imm as u32) << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | op)
    };
    let rtype = |f7: u32, rs2: u32, rs1: u32, f3: u32, rd: u32, op: u32| {
        Some((f7 << 25) | (rs2 << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | op)
    };
    let stype = |imm: u32, rs2: u32, rs1: u32, f3: u32, op: u32| {
        Some(((imm >> 5) << 25) | (rs2 << 20) | (rs1 << 15) | (f3 << 12) | ((imm & 31) << 7) | op)
    };

    match (op, f3) {
        (0, 0) => {
            // c.addi4spn -> addi rd', x2, nzuimm
            let imm = ((c >> 5 & 1) as u32) << 3
                | ((c >> 6 & 1) as u32) << 2
                | ((c >> 7 & 0xf) as u32) << 6
                | ((c >> 11 & 3) as u32) << 4;
            if imm == 0 {
                return None;
            }
            itype(imm as i32, 2, 0, rd_c, 0x13)
        }
        (0, 2) => {
            // c.lw
            let imm = ((c >> 5 & 1) as u32) << 6 | ((c >> 6 & 1) as u32) << 2 | ((c >> 10 & 7) as u32) << 3;
            itype(imm as i32, rs1_c, 2, rd_c, 0x03)
        }
        (0, 3) => {
            // c.ld
            let imm = ((c >> 5 & 3) as u32) << 6 | ((c >> 10 & 7) as u32) << 3;
            itype(imm as i32, rs1_c, 3, rd_c, 0x03)
        }
        (0, 6) => {
            // c.sw
            let imm = ((c >> 5 & 1) as u32) << 6 | ((c >> 6 & 1) as u32) << 2 | ((c >> 10 & 7) as u32) << 3;
            stype(imm, rd_c, rs1_c, 2, 0x23)
        }
        (0, 7) => {
            // c.sd
            let imm = ((c >> 5 & 3) as u32) << 6 | ((c >> 10 & 7) as u32) << 3;
            stype(imm, rd_c, rs1_c, 3, 0x23)
        }
        (1, 0) => {
            // c.addi (incl. c.nop)
            let imm = (((c >> 12 & 1) as u32) << 5 | ((c >> 2 & 31) as u32)) as i32;
            let imm = (imm << 26) >> 26;
            itype(imm, rd, 0, rd, 0x13)
        }
        (1, 1) => {
            // c.addiw
            let imm = (((c >> 12 & 1) as u32) << 5 | ((c >> 2 & 31) as u32)) as i32;
            let imm = (imm << 26) >> 26;
            if rd == 0 {
                return None;
            }
            itype(imm, rd, 0, rd, 0x1b)
        }
        (1, 2) => {
            // c.li
            let imm = (((c >> 12 & 1) as u32) << 5 | ((c >> 2 & 31) as u32)) as i32;
            let imm = (imm << 26) >> 26;
            itype(imm, 0, 0, rd, 0x13)
        }
        (1, 3) => {
            if rd == 2 {
                // c.addi16sp
                let imm = ((c >> 12 & 1) as u32) << 9
                    | ((c >> 6 & 1) as u32) << 4
                    | ((c >> 5 & 1) as u32) << 6
                    | ((c >> 3 & 3) as u32) << 7
                    | ((c >> 2 & 1) as u32) << 5;
                let imm = ((imm as i32) << 22) >> 22;
                if imm == 0 {
                    return None;
                }
                itype(imm, 2, 0, 2, 0x13)
            } else {
                // c.lui
                let imm = (((c >> 12 & 1) as u32) << 17 | ((c >> 2 & 31) as u32) << 12) as i32;
                let imm = (imm << 14) >> 14;
                if imm == 0 || rd == 0 {
                    return None;
                }
                Some(((imm as u32) & 0xffff_f000) | (rd << 7) | 0x37)
            }
        }
        (1, 4) => {
            let f2 = (c >> 10) & 3;
            match f2 {
                0 => {
                    // c.srli
                    let sh = ((c >> 12 & 1) << 5 | (c >> 2 & 31)) as u32;
                    itype(sh as i32, rs1_c, 5, rs1_c, 0x13)
                }
                1 => {
                    // c.srai
                    let sh = ((c >> 12 & 1) << 5 | (c >> 2 & 31)) as u32;
                    itype((sh | 0x400) as i32, rs1_c, 5, rs1_c, 0x13)
                }
                2 => {
                    // c.andi
                    let imm = (((c >> 12 & 1) as u32) << 5 | ((c >> 2 & 31) as u32)) as i32;
                    let imm = (imm << 26) >> 26;
                    itype(imm, rs1_c, 7, rs1_c, 0x13)
                }
                _ => {
                    let f2b = (c >> 5) & 3;
                    let w = (c >> 12) & 1;
                    match (w, f2b) {
                        (0, 0) => rtype(0x20, rd_c, rs1_c, 0, rs1_c, 0x33), // c.sub
                        (0, 1) => rtype(0, rd_c, rs1_c, 4, rs1_c, 0x33),    // c.xor
                        (0, 2) => rtype(0, rd_c, rs1_c, 6, rs1_c, 0x33),    // c.or
                        (0, 3) => rtype(0, rd_c, rs1_c, 7, rs1_c, 0x33),    // c.and
                        (1, 0) => rtype(0x20, rd_c, rs1_c, 0, rs1_c, 0x3b), // c.subw
                        (1, 1) => rtype(0, rd_c, rs1_c, 0, rs1_c, 0x3b),    // c.addw
                        _ => None,
                    }
                }
            }
        }
        (1, 5) => {
            // c.j
            let imm = ((c >> 12 & 1) as u32) << 11
                | ((c >> 11 & 1) as u32) << 4
                | ((c >> 9 & 3) as u32) << 8
                | ((c >> 8 & 1) as u32) << 10
                | ((c >> 7 & 1) as u32) << 6
                | ((c >> 6 & 1) as u32) << 7
                | ((c >> 3 & 7) as u32) << 1
                | ((c >> 2 & 1) as u32) << 5;
            let imm = ((imm as i32) << 20) >> 20;
            let u = imm as u32;
            Some(
                ((u >> 20 & 1) << 31)
                    | ((u >> 1 & 0x3ff) << 21)
                    | ((u >> 11 & 1) << 20)
                    | ((u >> 12 & 0xff) << 12)
                    | 0x6f,
            )
        }
        (1, 6) | (1, 7) => {
            // c.beqz / c.bnez
            let imm = ((c >> 12 & 1) as u32) << 8
                | ((c >> 10 & 3) as u32) << 3
                | ((c >> 5 & 3) as u32) << 6
                | ((c >> 3 & 3) as u32) << 1
                | ((c >> 2 & 1) as u32) << 5;
            let imm = ((imm as i32) << 23) >> 23;
            let u = imm as u32;
            let f3b = if f3 == 6 { 0 } else { 1 };
            Some(
                ((u >> 12 & 1) << 31)
                    | ((u >> 5 & 0x3f) << 25)
                    | (rs1_c << 15)
                    | (f3b << 12)
                    | ((u >> 1 & 0xf) << 8)
                    | ((u >> 11 & 1) << 7)
                    | 0x63,
            )
        }
        (2, 0) => {
            // c.slli
            let sh = ((c >> 12 & 1) << 5 | (c >> 2 & 31)) as u32;
            itype(sh as i32, rd, 1, rd, 0x13)
        }
        (2, 2) => {
            // c.lwsp
            if rd == 0 {
                return None;
            }
            let imm = ((c >> 12 & 1) as u32) << 5 | ((c >> 4 & 7) as u32) << 2 | ((c >> 2 & 3) as u32) << 6;
            itype(imm as i32, 2, 2, rd, 0x03)
        }
        (2, 3) => {
            // c.ldsp
            if rd == 0 {
                return None;
            }
            let imm = ((c >> 12 & 1) as u32) << 5 | ((c >> 5 & 3) as u32) << 3 | ((c >> 2 & 7) as u32) << 6;
            itype(imm as i32, 2, 3, rd, 0x03)
        }
        (2, 4) => {
            let bit12 = (c >> 12) & 1;
            if bit12 == 0 {
                if rs2 == 0 {
                    // c.jr
                    if rd == 0 {
                        return None;
                    }
                    itype(0, rd, 0, 0, 0x67)
                } else {
                    // c.mv
                    rtype(0, rs2, 0, 0, rd, 0x33)
                }
            } else if rs2 == 0 {
                if rd == 0 {
                    // c.ebreak
                    Some(0x0010_0073)
                } else {
                    // c.jalr
                    itype(0, rd, 0, 1, 0x67)
                }
            } else {
                // c.add
                rtype(0, rs2, rd, 0, rd, 0x33)
            }
        }
        (2, 6) => {
            // c.swsp
            let imm = ((c >> 9 & 0xf) as u32) << 2 | ((c >> 7 & 3) as u32) << 6;
            stype(imm, rs2, 2, 2, 0x23)
        }
        (2, 7) => {
            // c.sdsp
            let imm = ((c >> 10 & 7) as u32) << 3 | ((c >> 7 & 7) as u32) << 6;
            stype(imm, rs2, 2, 3, 0x23)
        }
        _ => None,
    }
}
