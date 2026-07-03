//! Physical memory bus: RAM, Spike-compatible boot ROM, and devices
//! (Gate A: CLINT only; PLIC and UART arrive with Gate B).

use crate::trap::Exception;

pub const BOOT_ROM_BASE: u64 = 0x1000;
pub const CLINT_BASE: u64 = 0x0200_0000;
pub const CLINT_SIZE: u64 = 0xc0000;
pub const RAM_BASE: u64 = 0x8000_0000;

pub const CLINT_MSIP: u64 = 0x0;
pub const CLINT_MTIMECMP: u64 = 0x4000;
pub const CLINT_MTIME: u64 = 0xbff8;

/// CLINT with Spike's timing model: mtime advances by 1 per
/// INSNS_PER_RTC_TICK (100) retired instructions, so interrupt timing is
/// deterministic and matches the pinned reference simulator exactly.
pub struct Clint {
    pub msip: bool,
    pub mtimecmp: u64,
    pub mtime: u64,
}

impl Clint {
    fn new() -> Self {
        Clint { msip: false, mtimecmp: 0, mtime: 0 }
    }

    fn read(&self, offset: u64, size: u64) -> Result<u64, ()> {
        // Natural alignment assumed; only the registers the targets use.
        let val = match offset & !7 {
            CLINT_MSIP if offset < 4 => self.msip as u64,
            CLINT_MTIMECMP => self.mtimecmp,
            CLINT_MTIME => self.mtime,
            _ => 0,
        };
        let shift = (offset & 7) * 8;
        Ok(match size {
            1 => (val >> shift) & 0xff,
            2 => (val >> shift) & 0xffff,
            4 => (val >> shift) & 0xffff_ffff,
            8 => val,
            _ => return Err(()),
        })
    }

    fn write(&mut self, offset: u64, val: u64, size: u64) -> Result<(), ()> {
        match (offset, size) {
            (CLINT_MSIP, 4) => self.msip = val & 1 != 0,
            (CLINT_MTIMECMP, 8) => self.mtimecmp = val,
            (CLINT_MTIME, 8) => self.mtime = val,
            (0x4000, 4) => self.mtimecmp = (self.mtimecmp & !0xffff_ffff) | (val & 0xffff_ffff),
            (0x4004, 4) => self.mtimecmp = (self.mtimecmp & 0xffff_ffff) | (val << 32),
            _ => {} // other CLINT space: ignore writes like Spike's model
        }
        Ok(())
    }
}

pub struct Bus {
    pub ram: Vec<u8>,
    boot_rom: Vec<u8>,
    pub clint: Clint,
}

impl Bus {
    pub fn new(ram_bytes: usize) -> Self {
        Bus {
            ram: vec![0; ram_bytes],
            boot_rom: Vec::new(),
            clint: Clint::new(),
        }
    }

    /// Install the reset ROM image at 0x1000 (Spike-compatible: 5
    /// instructions + entry word + DTB).
    pub fn set_boot_rom(&mut self, rom: Vec<u8>) {
        self.boot_rom = rom;
    }

    pub fn write_ram(&mut self, paddr: u64, data: &[u8]) {
        let off = (paddr - RAM_BASE) as usize;
        self.ram[off..off + data.len()].copy_from_slice(data);
    }

    /// Physical load. `size` in bytes (1/2/4/8). Returns zero-extended value.
    pub fn load(&mut self, paddr: u64, size: u64) -> Result<u64, Exception> {
        if paddr >= RAM_BASE {
            let off = (paddr - RAM_BASE) as usize;
            if off + size as usize <= self.ram.len() {
                let mut v = 0u64;
                for i in 0..size as usize {
                    v |= (self.ram[off + i] as u64) << (8 * i);
                }
                return Ok(v);
            }
        } else if paddr >= BOOT_ROM_BASE && (paddr - BOOT_ROM_BASE) as usize + size as usize <= self.boot_rom.len() {
            let off = (paddr - BOOT_ROM_BASE) as usize;
            let mut v = 0u64;
            for i in 0..size as usize {
                v |= (self.boot_rom[off + i] as u64) << (8 * i);
            }
            return Ok(v);
        } else if (CLINT_BASE..CLINT_BASE + CLINT_SIZE).contains(&paddr) {
            return self.clint.read(paddr - CLINT_BASE, size).map_err(|_| Exception::LoadAccessFault(paddr));
        }
        Err(Exception::LoadAccessFault(paddr))
    }

    pub fn store(&mut self, paddr: u64, val: u64, size: u64) -> Result<(), Exception> {
        if paddr >= RAM_BASE {
            let off = (paddr - RAM_BASE) as usize;
            if off + size as usize <= self.ram.len() {
                for i in 0..size as usize {
                    self.ram[off + i] = (val >> (8 * i)) as u8;
                }
                return Ok(());
            }
        } else if (CLINT_BASE..CLINT_BASE + CLINT_SIZE).contains(&paddr) {
            return self.clint.write(paddr - CLINT_BASE, val, size).map_err(|_| Exception::StoreAccessFault(paddr));
        }
        Err(Exception::StoreAccessFault(paddr))
    }
}
