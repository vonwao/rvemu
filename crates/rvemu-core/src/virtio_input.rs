//! virtio-input device (virtio-mmio v2, device id 18): carries evdev events
//! (keyboard + relative mouse) from the page's canvas into the guest.
//! Extras only — exists only when `Bus::enable_input` was called; certified
//! targets and the lockstep platform never see it. Shares ring helpers with
//! the net device (virtio.rs); the small register block is deliberately
//! duplicated rather than refactoring the already-certified net device.

use crate::virtio::{
    push_used, ram_slice_mut, read_u16, read_u32, read_u64, Queue, DESC_F_NEXT, DESC_F_WRITE,
    MAGIC, VENDOR_ID, VERSION, VIRTIO_F_VERSION_1,
};

const DEVICE_ID_INPUT: u32 = 18;
const QUEUE_NUM_MAX: u32 = 64;

// virtio_input_config selects.
const CFG_ID_NAME: u8 = 0x01;
const CFG_ID_DEVIDS: u8 = 0x03;
const CFG_EV_BITS: u8 = 0x11;

const DEV_NAME: &str = "rvemu virtio input";

pub struct VirtioInput {
    status: u32,
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: u64,
    queue_sel: u32,
    queues: [Queue; 2], // 0 = eventq, 1 = statusq
    interrupt_status: u32,
    cfg_select: u8,
    cfg_subsel: u8,
    /// Host events (type, code, value) awaiting guest event buffers.
    pending: std::collections::VecDeque<(u16, u16, u32)>,
}

impl VirtioInput {
    pub fn new() -> Self {
        VirtioInput {
            status: 0,
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: 0,
            queue_sel: 0,
            queues: [Queue::default(), Queue::default()],
            interrupt_status: 0,
            cfg_select: 0,
            cfg_subsel: 0,
            pending: std::collections::VecDeque::new(),
        }
    }

    pub fn irq_level(&self) -> bool {
        self.interrupt_status != 0
    }

    /// Queue one evdev event from the host (page). The page sends its own
    /// EV_SYN(0,0,0) after each batch.
    pub fn push_event(&mut self, etype: u16, code: u16, value: u32) {
        if self.pending.len() < 1024 {
            self.pending.push_back((etype, code, value));
        }
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn reset(&mut self) {
        let (sel, subsel) = (self.cfg_select, self.cfg_subsel);
        *self = VirtioInput::new();
        self.cfg_select = sel;
        self.cfg_subsel = subsel;
    }

    /// The 128-byte config payload for the current select/subsel, or None
    /// when unsupported (size then reads 0).
    fn cfg_payload(&self) -> Option<Vec<u8>> {
        match (self.cfg_select, self.cfg_subsel) {
            (CFG_ID_NAME, _) => Some(DEV_NAME.as_bytes().to_vec()),
            (CFG_ID_DEVIDS, _) => {
                // bustype BUS_VIRTUAL (0x06), vendor/product/version zero.
                let mut v = vec![0u8; 8];
                v[0] = 0x06;
                Some(v)
            }
            (CFG_EV_BITS, 1) => {
                // EV_KEY bitmap: all keyboard codes 1..=255 plus the mouse
                // buttons BTN_LEFT/RIGHT/MIDDLE (0x110..=0x112).
                let mut v = vec![0u8; 128];
                for code in 1..=255usize {
                    v[code / 8] |= 1 << (code % 8);
                }
                for code in 0x110..=0x112usize {
                    v[code / 8] |= 1 << (code % 8);
                }
                Some(v)
            }
            (CFG_EV_BITS, 2) => {
                // EV_REL bitmap: REL_X, REL_Y, REL_WHEEL.
                Some(vec![0x03, 0x01])
            }
            _ => None,
        }
    }

    pub fn load(&mut self, offset: u64, size: u64) -> Result<u64, ()> {
        if offset >= 0x100 {
            // Config space, byte-addressable: select(0) subsel(1) size(2)
            // reserved(3..8) payload(8..136).
            let payload = self.cfg_payload();
            let byte = |o: usize| -> u8 {
                match o {
                    0 => self.cfg_select,
                    1 => self.cfg_subsel,
                    2 => payload.as_ref().map_or(0, |p| p.len().min(128) as u8),
                    8..=135 => payload.as_ref().and_then(|p| p.get(o - 8).copied()).unwrap_or(0),
                    _ => 0,
                }
            };
            let off = (offset - 0x100) as usize;
            let mut v = 0u64;
            for i in 0..size as usize {
                v |= (byte(off + i) as u64) << (8 * i);
            }
            return Ok(v);
        }
        if size != 4 {
            return Err(());
        }
        let q = &self.queues[(self.queue_sel as usize) & 1];
        let v: u32 = match offset {
            0x000 => MAGIC,
            0x004 => VERSION,
            0x008 => DEVICE_ID_INPUT,
            0x00c => VENDOR_ID,
            0x010 => ((VIRTIO_F_VERSION_1) >> (32 * (self.device_features_sel & 1))) as u32,
            0x034 => QUEUE_NUM_MAX,
            0x044 => q.ready as u32,
            0x060 => self.interrupt_status,
            0x070 => self.status,
            0x0fc => 0,
            _ => 0,
        };
        Ok(v as u64)
    }

    /// Returns true on QueueNotify (bus then runs processing against RAM).
    pub fn store(&mut self, offset: u64, val: u64, size: u64) -> Result<bool, ()> {
        if offset >= 0x100 {
            // Config: select/subsel are the only writable bytes.
            let off = offset - 0x100;
            if off == 0 {
                self.cfg_select = val as u8;
                if size >= 2 {
                    self.cfg_subsel = (val >> 8) as u8;
                }
            } else if off == 1 {
                self.cfg_subsel = val as u8;
            }
            return Ok(false);
        }
        if size != 4 {
            return Err(());
        }
        let val = val as u32;
        let qi = (self.queue_sel as usize) & 1;
        match offset {
            0x014 => self.device_features_sel = val,
            0x020 => {
                let shift = 32 * (self.driver_features_sel & 1);
                self.driver_features =
                    (self.driver_features & !(0xffff_ffffu64 << shift)) | ((val as u64) << shift);
            }
            0x024 => self.driver_features_sel = val,
            0x030 => self.queue_sel = val,
            0x038 => self.queues[qi].num = val.min(QUEUE_NUM_MAX),
            0x044 => self.queues[qi].ready = val & 1 != 0,
            0x050 => return Ok(true),
            0x064 => self.interrupt_status &= !val,
            0x070 => {
                if val == 0 {
                    self.reset();
                } else {
                    self.status = val;
                }
            }
            0x080 => self.queues[qi].desc = (self.queues[qi].desc & !0xffff_ffff) | val as u64,
            0x084 => self.queues[qi].desc = (self.queues[qi].desc & 0xffff_ffff) | ((val as u64) << 32),
            0x090 => self.queues[qi].driver = (self.queues[qi].driver & !0xffff_ffff) | val as u64,
            0x094 => self.queues[qi].driver = (self.queues[qi].driver & 0xffff_ffff) | ((val as u64) << 32),
            0x0a0 => self.queues[qi].device = (self.queues[qi].device & !0xffff_ffff) | val as u64,
            0x0a4 => self.queues[qi].device = (self.queues[qi].device & 0xffff_ffff) | ((val as u64) << 32),
            _ => {}
        }
        Ok(false)
    }

    /// Deliver pending events into posted eventq buffers and drain statusq.
    pub fn process(&mut self, ram: &mut [u8], ram_base: u64) {
        if self.status & 0x4 == 0 {
            return;
        }
        self.process_eventq(ram, ram_base);
        self.drain_statusq(ram, ram_base);
    }

    fn process_eventq(&mut self, ram: &mut [u8], ram_base: u64) {
        let q = self.queues[0].clone();
        if !q.ready || q.num == 0 {
            return;
        }
        let mut last = q.last_avail;
        let mut used = false;
        while let Some(&(etype, code, value)) = self.pending.front() {
            let avail_idx = match read_u16(ram, ram_base, q.driver + 2) {
                Some(v) => v,
                None => break,
            };
            if last == avail_idx {
                break;
            }
            let slot = (last as u32 % q.num) as u64;
            let Some(head) = read_u16(ram, ram_base, q.driver + 4 + 2 * slot) else { break };
            let mut ev = [0u8; 8];
            ev[0..2].copy_from_slice(&etype.to_le_bytes());
            ev[2..4].copy_from_slice(&code.to_le_bytes());
            ev[4..8].copy_from_slice(&value.to_le_bytes());
            // Write into the first writable descriptor of the chain.
            let mut written = 0usize;
            let mut di = head;
            for _ in 0..q.num {
                let base = q.desc + 16 * (di as u64 % q.num as u64);
                let (Some(addr), Some(len), Some(flags), Some(next)) = (
                    read_u64(ram, ram_base, base),
                    read_u32(ram, ram_base, base + 8),
                    read_u16(ram, ram_base, base + 12),
                    read_u16(ram, ram_base, base + 14),
                ) else {
                    break;
                };
                if flags & DESC_F_WRITE != 0 && written < ev.len() {
                    let n = (ev.len() - written).min(len as usize);
                    if let Some(dst) = ram_slice_mut(ram, ram_base, addr, n) {
                        dst.copy_from_slice(&ev[written..written + n]);
                        written += n;
                    }
                }
                if flags & DESC_F_NEXT == 0 {
                    break;
                }
                di = next;
            }
            push_used(ram, ram_base, &q, head, written as u32);
            self.pending.pop_front();
            last = last.wrapping_add(1);
            used = true;
        }
        self.queues[0].last_avail = last;
        if used {
            self.interrupt_status |= 1;
        }
    }

    fn drain_statusq(&mut self, ram: &mut [u8], ram_base: u64) {
        let q = self.queues[1].clone();
        if !q.ready || q.num == 0 {
            return;
        }
        let mut last = q.last_avail;
        let mut used = false;
        loop {
            let avail_idx = match read_u16(ram, ram_base, q.driver + 2) {
                Some(v) => v,
                None => break,
            };
            if last == avail_idx {
                break;
            }
            let slot = (last as u32 % q.num) as u64;
            let Some(head) = read_u16(ram, ram_base, q.driver + 4 + 2 * slot) else { break };
            push_used(ram, ram_base, &q, head, 0); // LED/repeat events: ignored
            last = last.wrapping_add(1);
            used = true;
        }
        self.queues[1].last_avail = last;
        if used {
            self.interrupt_status |= 1;
        }
    }
}

impl Default for VirtioInput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x8000_0000;
    const DESC: u64 = BASE + 0x1000;
    const AVAIL: u64 = BASE + 0x2000;
    const USED: u64 = BASE + 0x3000;
    const BUF: u64 = BASE + 0x4000;

    #[test]
    fn config_and_event_delivery() {
        let mut d = VirtioInput::new();
        let mut ram = vec![0u8; 0x10000];
        // Config: name select.
        d.store(0x100, CFG_ID_NAME as u64, 1).unwrap();
        assert_eq!(d.load(0x102, 1).unwrap() as usize, DEV_NAME.len());
        assert_eq!(d.load(0x108, 1).unwrap() as u8, b'r');
        // EV_BITS for EV_REL.
        d.store(0x100, (2 << 8 | CFG_EV_BITS as u64) as u64, 2).unwrap();
        assert_eq!(d.load(0x102, 1).unwrap(), 2);
        assert_eq!(d.load(0x108, 1).unwrap(), 0x03);
        // Queue 0 setup: num 4, one 8-byte writable buffer posted.
        d.store(0x70, 0x0f, 4).unwrap();
        d.store(0x30, 0, 4).unwrap();
        d.store(0x38, 4, 4).unwrap();
        d.store(0x80, DESC & 0xffff_ffff, 4).unwrap();
        d.store(0x84, DESC >> 32, 4).unwrap();
        d.store(0x90, AVAIL & 0xffff_ffff, 4).unwrap();
        d.store(0x94, AVAIL >> 32, 4).unwrap();
        d.store(0xa0, USED & 0xffff_ffff, 4).unwrap();
        d.store(0xa4, USED >> 32, 4).unwrap();
        d.store(0x44, 1, 4).unwrap();
        let doff = (DESC - BASE) as usize;
        ram[doff..doff + 8].copy_from_slice(&BUF.to_le_bytes());
        ram[doff + 8..doff + 12].copy_from_slice(&8u32.to_le_bytes());
        ram[doff + 12..doff + 14].copy_from_slice(&DESC_F_WRITE.to_le_bytes());
        let aoff = (AVAIL - BASE) as usize;
        ram[aoff + 2..aoff + 4].copy_from_slice(&1u16.to_le_bytes());
        // ring[0] = 0 already.
        d.push_event(1, 30, 1); // EV_KEY KEY_A down
        d.process(&mut ram, BASE);
        assert_eq!(read_u16(&ram, BASE, USED + 2).unwrap(), 1);
        let ev = &ram[(BUF - BASE) as usize..(BUF - BASE) as usize + 8];
        assert_eq!(u16::from_le_bytes([ev[0], ev[1]]), 1);
        assert_eq!(u16::from_le_bytes([ev[2], ev[3]]), 30);
        assert_eq!(u32::from_le_bytes([ev[4], ev[5], ev[6], ev[7]]), 1);
        assert!(d.irq_level());
    }
}
