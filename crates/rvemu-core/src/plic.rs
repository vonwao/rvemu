//! PLIC modeled on the pinned Spike's riscv/plic.cc: 32 ids (ndev=31),
//! 4 priority bits, two contexts (0 = hart0 M-mode, 1 = hart0 S-mode).
//! Spike quirks preserved: a level change is delivered only to the FIRST
//! context that has the source enabled; enable-writes recompute pending
//! from the current level; claim clears pending via the claimed mask.

const NUM_IDS: u32 = 32;
const PRIO_MASK: u32 = 0xf;

const PRIORITY_BASE: u64 = 0x0;
const PENDING_BASE: u64 = 0x1000;
const ENABLE_BASE: u64 = 0x2000;
const ENABLE_PER_HART: u64 = 0x80;
const CONTEXT_BASE: u64 = 0x200000;
const CONTEXT_PER_HART: u64 = 0x1000;
const CONTEXT_THRESHOLD: u64 = 0x0;
const CONTEXT_CLAIM: u64 = 0x4;

#[derive(Default)]
struct Context {
    enable: u32,
    pending: u32,
    pending_priority: [u8; NUM_IDS as usize],
    claimed: u32,
    threshold: u32,
}

pub struct Plic {
    priority: [u32; NUM_IDS as usize],
    level: u32,
    ctx: [Context; 2], // 0 = M, 1 = S
    /// mip levels computed from context state.
    pub meip: bool,
    pub seip: bool,
}

impl Plic {
    pub fn new() -> Self {
        Plic {
            priority: [0; NUM_IDS as usize],
            level: 0,
            ctx: [Context::default(), Context::default()],
            meip: false,
            seip: false,
        }
    }

    fn best_pending(c: &Context) -> u32 {
        let mut best_id = 0u32;
        let mut best_prio = 0u8;
        for id in 0..NUM_IDS {
            let mask = 1u32 << id;
            if c.pending & mask == 0 || c.claimed & mask != 0 {
                continue;
            }
            if best_id == 0 || best_prio < c.pending_priority[id as usize] {
                best_id = id;
                best_prio = c.pending_priority[id as usize];
            }
        }
        if u32::from(best_prio) <= c.threshold {
            return 0;
        }
        best_id
    }

    fn update(&mut self, i: usize) {
        let best = Self::best_pending(&self.ctx[i]);
        if i == 0 {
            self.meip = best != 0;
        } else {
            self.seip = best != 0;
        }
    }

    fn claim(&mut self, i: usize) -> u32 {
        let best = Self::best_pending(&self.ctx[i]);
        if best != 0 {
            self.ctx[i].claimed |= 1 << best;
        }
        self.update(i);
        best
    }

    /// Device level change (Spike's set_interrupt_level, incl. the
    /// first-enabled-context-only delivery).
    pub fn set_interrupt_level(&mut self, id: u32, lvl: bool) {
        if id == 0 || id >= NUM_IDS {
            return;
        }
        let mask = 1u32 << id;
        let prio = self.priority[id as usize] as u8;
        if lvl {
            self.level |= mask;
        } else {
            self.level &= !mask;
        }
        for i in 0..2 {
            if self.ctx[i].enable & mask != 0 {
                if lvl {
                    self.ctx[i].pending |= mask;
                    self.ctx[i].pending_priority[id as usize] = prio;
                } else {
                    self.ctx[i].pending &= !mask;
                    self.ctx[i].pending_priority[id as usize] = 0;
                    self.ctx[i].claimed &= !mask;
                }
                self.update(i);
                break;
            }
        }
    }

    fn word_read(&mut self, offset: u64) -> u32 {
        match offset {
            o if o < PENDING_BASE => {
                let id = (o - PRIORITY_BASE) >> 2;
                if id > 0 && id < NUM_IDS as u64 {
                    self.priority[id as usize]
                } else {
                    0
                }
            }
            o if o < ENABLE_BASE => {
                if (o - PENDING_BASE) >> 2 == 0 {
                    self.ctx[0].pending | self.ctx[1].pending
                } else {
                    0
                }
            }
            o if o < CONTEXT_BASE => {
                let i = ((o - ENABLE_BASE) / ENABLE_PER_HART) as usize;
                let word = (o - ENABLE_BASE) % ENABLE_PER_HART >> 2;
                if i < 2 && word == 0 {
                    self.ctx[i].enable
                } else {
                    0
                }
            }
            o => {
                let i = ((o - CONTEXT_BASE) / CONTEXT_PER_HART) as usize;
                let reg = (o - CONTEXT_BASE) % CONTEXT_PER_HART;
                if i >= 2 {
                    return 0;
                }
                match reg {
                    CONTEXT_THRESHOLD => self.ctx[i].threshold,
                    CONTEXT_CLAIM => self.claim(i),
                    _ => 0,
                }
            }
        }
    }

    fn word_write(&mut self, offset: u64, val: u32) {
        match offset {
            o if o < PENDING_BASE => {
                let id = (o - PRIORITY_BASE) >> 2;
                if id > 0 && id < NUM_IDS as u64 {
                    self.priority[id as usize] = val & PRIO_MASK;
                }
            }
            o if o < ENABLE_BASE => {} // pending is read-only
            o if o < CONTEXT_BASE => {
                let i = ((o - ENABLE_BASE) / ENABLE_PER_HART) as usize;
                let word = (o - ENABLE_BASE) % ENABLE_PER_HART >> 2;
                if i >= 2 || word != 0 {
                    return;
                }
                let old = self.ctx[i].enable;
                let new = val & !1u32; // id 0 not enableable
                let xor = old ^ new;
                self.ctx[i].enable = new;
                for id in 0..NUM_IDS {
                    let mask = 1u32 << id;
                    if xor & mask == 0 {
                        continue;
                    }
                    if new & mask != 0 && self.level & mask != 0 {
                        self.ctx[i].pending |= mask;
                        self.ctx[i].pending_priority[id as usize] = self.priority[id as usize] as u8;
                    } else if new & mask == 0 {
                        self.ctx[i].pending &= !mask;
                        self.ctx[i].pending_priority[id as usize] = 0;
                        self.ctx[i].claimed &= !mask;
                    }
                }
                self.update(i);
            }
            o => {
                let i = ((o - CONTEXT_BASE) / CONTEXT_PER_HART) as usize;
                let reg = (o - CONTEXT_BASE) % CONTEXT_PER_HART;
                if i >= 2 {
                    return;
                }
                match reg {
                    CONTEXT_THRESHOLD => {
                        self.ctx[i].threshold = val & PRIO_MASK;
                        self.update(i);
                    }
                    CONTEXT_CLAIM => {
                        // complete
                        if (val as u64) < NUM_IDS as u64 && self.ctx[i].enable & (1 << val) != 0 {
                            self.ctx[i].claimed &= !(1 << val);
                            self.update(i);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn load(&mut self, offset: u64, size: u64) -> Result<u64, ()> {
        match size {
            4 => Ok(self.word_read(offset) as u64),
            8 => {
                let lo = self.word_read(offset) as u64;
                let hi = self.word_read(offset + 4) as u64;
                Ok(lo | (hi << 32))
            }
            _ => Err(()),
        }
    }

    pub fn store(&mut self, offset: u64, val: u64, size: u64) -> Result<(), ()> {
        match size {
            4 => {
                self.word_write(offset, val as u32);
                Ok(())
            }
            8 => {
                self.word_write(offset, val as u32);
                self.word_write(offset + 4, (val >> 32) as u32);
                Ok(())
            }
            _ => Err(()),
        }
    }
}

impl Default for Plic {
    fn default() -> Self {
        Self::new()
    }
}
