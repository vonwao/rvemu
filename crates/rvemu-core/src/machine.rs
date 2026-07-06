//! Machine assembly: bus + cpu + boot ROM, and the run loop used by both
//! the native CLI and (later) the wasm target.

use crate::bus::{Bus, RAM_BASE};
use crate::cpu::{Cpu, StepResult};
use crate::elf::LoadedElf;
use crate::platform::Platform;

/// Spike-compatible reset ROM at 0x1000: sets a0=mhartid, a1=&dtb (0x1020),
/// loads the entry address from 0x1018 and jumps to it.
fn boot_rom(entry: u64, dtb: &[u8]) -> Vec<u8> {
    let mut rom = Vec::new();
    let insns: [u32; 5] = [
        0x0000_0297, // auipc t0, 0
        0x0202_8593, // addi  a1, t0, 32
        0xf140_2573, // csrr  a0, mhartid
        0x0182_b283, // ld    t0, 24(t0)
        0x0002_8067, // jr    t0
    ];
    for i in insns {
        rom.extend_from_slice(&i.to_le_bytes());
    }
    rom.extend_from_slice(&0u32.to_le_bytes()); // pad to 0x1018
    rom.extend_from_slice(&entry.to_le_bytes()); // 0x1018: entry
    rom.extend_from_slice(dtb); // 0x1020: dtb
    rom
}

pub struct Machine {
    pub cpu: Cpu,
    pub begin_signature: Option<u64>,
    pub end_signature: Option<u64>,
}

pub enum RunExit {
    /// tohost written: value per the HTIF test convention.
    Tohost(u64),
    /// Instruction budget exhausted.
    Budget,
}

impl Machine {
    pub fn new(elf: &LoadedElf, ram_mib: usize, dtb: &[u8]) -> Self {
        let mut bus = Bus::new(ram_mib * 1024 * 1024);
        bus.set_boot_rom(boot_rom(elf.entry, dtb));
        for (paddr, data) in &elf.segments {
            if *paddr >= RAM_BASE {
                bus.write_ram(*paddr, data);
            }
        }
        let mut cpu = Cpu::new(bus, 0x1000);
        cpu.tohost = elf.tohost;
        Machine {
            cpu,
            begin_signature: elf.begin_signature,
            end_signature: elf.end_signature,
        }
    }

    /// Run until tohost is written or `max_insns` instructions retire.
    /// Calls `on_trace` with the canonical line after each retirement when
    /// tracing is enabled.
    pub fn run(
        &mut self,
        max_insns: u64,
        platform: &mut dyn Platform,
        on_trace: impl FnMut(&str),
    ) -> RunExit {
        self.run_paced(max_insns, u64::MAX, platform, on_trace)
    }

    /// Like `run`, but also stops once the guest RTC reaches `max_mtime`.
    /// This is how the browser page paces guest time to wall time: WFI
    /// fast-forwarding otherwise skips whole timing waits inside one call.
    pub fn run_paced(
        &mut self,
        max_insns: u64,
        max_mtime: u64,
        platform: &mut dyn Platform,
        mut on_trace: impl FnMut(&str),
    ) -> RunExit {
        // The budget bounds steps (attempted instructions), not retirements:
        // a trap loop retires nothing but must still terminate, matching
        // Spike's --instructions semantics.
        let mut steps: u64 = 0;
        while steps < max_insns && self.cpu.bus.clint.mtime < max_mtime {
            steps += 1;
            // Console plumbing every RTC-tick-ish interval: cheap check.
            if steps % 128 == 0 {
                self.pump_console(platform);
            }
            match self.cpu.step() {
                StepResult::Retired => {
                    if self.cpu.trace_enabled {
                        on_trace(&self.cpu.trace_line);
                    }
                    if let Some(v) = self.cpu.tohost_value.take() {
                        self.pump_console(platform);
                        return RunExit::Tohost(v);
                    }
                }
                StepResult::Trapped => {}
                StepResult::WaitingForInterrupt => {
                    if self.cpu.trace_enabled {
                        on_trace(&self.cpu.trace_line);
                    }
                    // Idle in whole quanta until an interrupt is pending. A
                    // budget guard avoids spinning forever with interrupts
                    // disabled or masked.
                    let mut guard = 0u64;
                    while !self.cpu.has_pending_interrupt() && self.cpu.bus.clint.mtime < max_mtime {
                        self.cpu.idle_slice();
                        guard += 1;
                        if guard % 64 == 0 {
                            self.pump_console(platform);
                        }
                        if guard > 100_000_000 {
                            self.pump_console(platform);
                            return RunExit::Budget;
                        }
                    }
                }
            }
        }
        self.pump_console(platform);
        RunExit::Budget
    }

    fn pump_console(&mut self, platform: &mut dyn Platform) {
        for b in self.cpu.bus.uart.tx_out.drain(..) {
            platform.console_write(b);
        }
        while self.cpu.bus.input_queue.len() < 64 {
            match platform.console_read() {
                Some(b) => self.cpu.bus.input_queue.push_back(b),
                None => break,
            }
        }
    }

    /// Read the signature region (RISCOF contract).
    pub fn signature(&mut self) -> Option<Vec<u32>> {
        let (b, e) = (self.begin_signature?, self.end_signature?);
        let mut words = Vec::new();
        let mut a = b;
        while a < e {
            words.push(self.cpu.bus.load(a, 4).ok()? as u32);
            a += 4;
        }
        Some(words)
    }
}
