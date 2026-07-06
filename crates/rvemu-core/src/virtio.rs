//! virtio-mmio (spec 1.1, "modern" version 2) transport carrying a
//! virtio-net device. Extras only: the device exists only when
//! `Bus::enable_net` was called (the wasm demo build), so certified targets
//! and the lockstep platform never see it. The host side exchanges raw
//! ethernet frames: guest->host frames accumulate in `tx_frames`; the host
//! injects frames with `rx_push` and they are delivered into posted rx
//! buffers on the next device tick.

pub(crate) const MAGIC: u32 = 0x7472_6976; // "virt"
pub(crate) const VERSION: u32 = 2;
const DEVICE_ID_NET: u32 = 1;
pub(crate) const VENDOR_ID: u32 = 0x726d_7665; // "rvem"

pub(crate) const VIRTIO_F_VERSION_1: u64 = 1 << 32;
const VIRTIO_NET_F_MAC: u64 = 1 << 5;

pub(crate) const DESC_F_NEXT: u16 = 1;
pub(crate) const DESC_F_WRITE: u16 = 2;

/// Fixed, documented MAC for the guest NIC.
pub const GUEST_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

const QUEUE_NUM_MAX: u32 = 256;
const VNET_HDR_LEN: usize = 12; // virtio_net_hdr_v1 (VERSION_1 layout)
const MAX_FRAME: usize = 1600;

#[derive(Default, Clone)]
pub(crate) struct Queue {
    pub(crate) ready: bool,
    pub(crate) num: u32,
    pub(crate) desc: u64,
    pub(crate) driver: u64, // avail ring
    pub(crate) device: u64, // used ring
    pub(crate) last_avail: u16,
}

pub struct VirtioNet {
    status: u32,
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: u64,
    queue_sel: u32,
    queues: [Queue; 2], // 0 = rx, 1 = tx
    interrupt_status: u32,
    /// Frames from the guest, each length-prefixed (u16 LE) — drained by the
    /// host gateway.
    pub tx_frames: Vec<u8>,
    /// Frames from the host awaiting free guest rx buffers.
    rx_pending: std::collections::VecDeque<Vec<u8>>,
    // Diagnostics (debug_line).
    dbg_rx_calls: u64,
    dbg_rx_delivered: u64,
    dbg_rx_break: u64,
    pub dbg_ticks: u64,
    pub dbg_tick_pending: u64,
}

impl VirtioNet {
    pub fn new() -> Self {
        VirtioNet {
            status: 0,
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: 0,
            queue_sel: 0,
            queues: [Queue::default(), Queue::default()],
            interrupt_status: 0,
            tx_frames: Vec::new(),
            rx_pending: std::collections::VecDeque::new(),
            dbg_rx_calls: 0,
            dbg_rx_delivered: 0,
            dbg_rx_break: 0,
            dbg_ticks: 0,
            dbg_tick_pending: 0,
        }
    }

    pub fn irq_level(&self) -> bool {
        self.interrupt_status != 0
    }

    /// Host injects one ethernet frame toward the guest.
    pub fn rx_push(&mut self, frame: &[u8]) {
        if frame.len() <= MAX_FRAME && self.rx_pending.len() < 256 {
            self.rx_pending.push_back(frame.to_vec());
        }
    }

    pub fn has_rx_pending(&self) -> bool {
        !self.rx_pending.is_empty()
    }

    /// One-line state dump for the wasm debug export.
    pub fn debug_line(&self, ram: &[u8], ram_base: u64) -> String {
        let q = |i: usize| {
            let q = &self.queues[i];
            let avail = read_u16(ram, ram_base, q.driver + 2).map_or(-1i32, |v| v as i32);
            let used = read_u16(ram, ram_base, q.device + 2).map_or(-1i32, |v| v as i32);
            format!(
                "q{i}[ready={} num={} desc={:#x} last_avail={} avail_idx={} used_idx={}]",
                q.ready, q.num, q.desc, q.last_avail, avail, used
            )
        };
        format!(
            "status={:#x} isr={:#x} feat={:#x} rx_pending={} rx_calls={} rx_delivered={} rx_break={} ticks={} tick_pending={} {} {}",
            self.status,
            self.interrupt_status,
            self.driver_features,
            self.rx_pending.len(),
            self.dbg_rx_calls,
            self.dbg_rx_delivered,
            self.dbg_rx_break,
            self.dbg_ticks,
            self.dbg_tick_pending,
            q(0),
            q(1)
        )
    }

    fn device_features(&self) -> u64 {
        VIRTIO_F_VERSION_1 | VIRTIO_NET_F_MAC
    }

    fn reset(&mut self) {
        self.status = 0;
        self.driver_features = 0;
        self.queue_sel = 0;
        self.queues = [Queue::default(), Queue::default()];
        self.interrupt_status = 0;
        self.rx_pending.clear();
    }

    pub fn load(&mut self, offset: u64, size: u64) -> Result<u64, ()> {
        // Config space: byte-addressable.
        if offset >= 0x100 {
            let cfg_off = (offset - 0x100) as usize;
            // virtio_net_config: mac[6], then status/max_pairs/mtu (unused
            // without their feature bits; read as zero).
            let mut v = 0u64;
            for i in 0..size as usize {
                let b = GUEST_MAC.get(cfg_off + i).copied().unwrap_or(0);
                v |= (b as u64) << (8 * i);
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
            0x008 => DEVICE_ID_NET,
            0x00c => VENDOR_ID,
            0x010 => (self.device_features() >> (32 * (self.device_features_sel & 1))) as u32,
            0x034 => QUEUE_NUM_MAX,
            0x044 => q.ready as u32,
            0x060 => self.interrupt_status,
            0x070 => self.status,
            0x0fc => 0, // ConfigGeneration
            _ => 0,
        };
        Ok(v as u64)
    }

    /// Register write. Returns true when the driver notified a queue (the
    /// bus then runs queue processing against RAM).
    pub fn store(&mut self, offset: u64, val: u64, size: u64) -> Result<bool, ()> {
        if offset >= 0x100 {
            return Ok(false); // config space read-only here
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
            0x050 => return Ok(true), // QueueNotify
            0x064 => self.interrupt_status &= !val,
            0x070 => {
                if val == 0 {
                    self.reset();
                } else {
                    self.status = val;
                }
            }
            0x080 => set_lo(&mut self.queues[qi].desc, val),
            0x084 => set_hi(&mut self.queues[qi].desc, val),
            0x090 => set_lo(&mut self.queues[qi].driver, val),
            0x094 => set_hi(&mut self.queues[qi].driver, val),
            0x0a0 => set_lo(&mut self.queues[qi].device, val),
            0x0a4 => set_hi(&mut self.queues[qi].device, val),
            _ => {}
        }
        Ok(false)
    }

    /// Drain guest tx buffers and fill guest rx buffers. `ram` is guest RAM
    /// (base RAM_BASE). Called after QueueNotify and per device tick while
    /// host frames are pending.
    pub fn process(&mut self, ram: &mut [u8], ram_base: u64) {
        if self.status & 0x4 == 0 {
            return; // DRIVER_OK not set
        }
        self.process_tx(ram, ram_base);
        self.process_rx(ram, ram_base);
    }

    fn process_tx(&mut self, ram: &mut [u8], ram_base: u64) {
        let q = self.queues[1].clone();
        if !q.ready || q.num == 0 {
            return;
        }
        let mut last = q.last_avail;
        let avail_idx = match read_u16(ram, ram_base, q.driver + 2) {
            Some(v) => v,
            None => return,
        };
        let mut used = false;
        while last != avail_idx {
            let slot = (last as u32 % q.num) as u64;
            let Some(head) = read_u16(ram, ram_base, q.driver + 4 + 2 * slot) else { break };
            // Gather the whole descriptor chain (header + frame).
            let mut buf: Vec<u8> = Vec::new();
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
                if flags & DESC_F_WRITE == 0 {
                    if let Some(bytes) = ram_slice(ram, ram_base, addr, len as usize) {
                        buf.extend_from_slice(bytes);
                    }
                }
                if flags & DESC_F_NEXT == 0 {
                    break;
                }
                di = next;
            }
            if buf.len() > VNET_HDR_LEN && buf.len() <= VNET_HDR_LEN + MAX_FRAME {
                let frame = &buf[VNET_HDR_LEN..];
                self.tx_frames.extend_from_slice(&(frame.len() as u16).to_le_bytes());
                self.tx_frames.extend_from_slice(frame);
            }
            push_used(ram, ram_base, &q, head, 0);
            last = last.wrapping_add(1);
            used = true;
        }
        self.queues[1].last_avail = last;
        if used {
            self.interrupt_status |= 1;
        }
    }

    fn process_rx(&mut self, ram: &mut [u8], ram_base: u64) {
        self.dbg_rx_calls += 1;
        let q = self.queues[0].clone();
        if !q.ready || q.num == 0 {
            return;
        }
        let mut last = q.last_avail;
        let mut used = false;
        while let Some(frame) = self.rx_pending.front() {
            let avail_idx = match read_u16(ram, ram_base, q.driver + 2) {
                Some(v) => v,
                None => {
                    self.dbg_rx_break = 1;
                    break;
                }
            };
            if last == avail_idx {
                self.dbg_rx_break = 2;
                break; // no free buffers; retry on a later tick
            }
            let slot = (last as u32 % q.num) as u64;
            let Some(head) = read_u16(ram, ram_base, q.driver + 4 + 2 * slot) else { break };
            // Payload = virtio_net_hdr_v1 (zeroed, num_buffers=1) + frame.
            let mut payload = vec![0u8; VNET_HDR_LEN];
            payload[10] = 1; // num_buffers (LE u16 at offset 10)
            payload.extend_from_slice(frame);
            // Scatter into the writable descriptor chain.
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
                if flags & DESC_F_WRITE != 0 && written < payload.len() {
                    let n = (payload.len() - written).min(len as usize);
                    if let Some(dst) = ram_slice_mut(ram, ram_base, addr, n) {
                        dst.copy_from_slice(&payload[written..written + n]);
                        written += n;
                    }
                }
                if flags & DESC_F_NEXT == 0 {
                    break;
                }
                di = next;
            }
            if written < payload.len() {
                // Buffer too small for this frame: drop the frame (no
                // MRG_RXBUF), recycle the buffer with len 0.
                push_used(ram, ram_base, &q, head, 0);
            } else {
                push_used(ram, ram_base, &q, head, written as u32);
            }
            self.rx_pending.pop_front();
            last = last.wrapping_add(1);
            used = true;
            self.dbg_rx_delivered += 1;
        }
        self.queues[0].last_avail = last;
        if used {
            self.interrupt_status |= 1;
        }
    }
}

impl Default for VirtioNet {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn set_lo(v: &mut u64, val: u32) {
    *v = (*v & !0xffff_ffff) | val as u64;
}
pub(crate) fn set_hi(v: &mut u64, val: u32) {
    *v = (*v & 0xffff_ffff) | ((val as u64) << 32);
}

pub(crate) fn ram_slice<'a>(ram: &'a [u8], base: u64, addr: u64, len: usize) -> Option<&'a [u8]> {
    let off = addr.checked_sub(base)? as usize;
    ram.get(off..off + len)
}
pub(crate) fn ram_slice_mut<'a>(ram: &'a mut [u8], base: u64, addr: u64, len: usize) -> Option<&'a mut [u8]> {
    let off = addr.checked_sub(base)? as usize;
    ram.get_mut(off..off + len)
}
pub(crate) fn read_u16(ram: &[u8], base: u64, addr: u64) -> Option<u16> {
    ram_slice(ram, base, addr, 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}
pub(crate) fn read_u32(ram: &[u8], base: u64, addr: u64) -> Option<u32> {
    ram_slice(ram, base, addr, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
pub(crate) fn read_u64(ram: &[u8], base: u64, addr: u64) -> Option<u64> {
    ram_slice(ram, base, addr, 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
}
pub(crate) fn write_u16(ram: &mut [u8], base: u64, addr: u64, v: u16) {
    if let Some(b) = ram_slice_mut(ram, base, addr, 2) {
        b.copy_from_slice(&v.to_le_bytes());
    }
}
pub(crate) fn write_u32(ram: &mut [u8], base: u64, addr: u64, v: u32) {
    if let Some(b) = ram_slice_mut(ram, base, addr, 4) {
        b.copy_from_slice(&v.to_le_bytes());
    }
}

/// Append one used-ring element and bump used.idx.
pub(crate) fn push_used(ram: &mut [u8], base: u64, q: &Queue, head: u16, len: u32) {
    let idx = read_u16(ram, base, q.device + 2).unwrap_or(0);
    let slot = (idx as u32 % q.num) as u64;
    let elem = q.device + 4 + 8 * slot;
    write_u32(ram, base, elem, head as u32);
    write_u32(ram, base, elem + 4, len);
    write_u16(ram, base, q.device + 2, idx.wrapping_add(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x8000_0000;
    const DESC: u64 = BASE + 0x1000;
    const AVAIL: u64 = BASE + 0x2000;
    const USED: u64 = BASE + 0x3000;
    const BUF: u64 = BASE + 0x4000;

    fn setup(qsel: u32) -> (VirtioNet, Vec<u8>) {
        let mut d = VirtioNet::new();
        let ram = vec![0u8; 0x10000];
        d.store(0x70, 0x0f, 4).unwrap(); // ACK|DRIVER|DRIVER_OK|FEATURES_OK
        d.store(0x30, qsel as u64, 4).unwrap();
        d.store(0x38, 4, 4).unwrap(); // queue size 4
        d.store(0x80, DESC & 0xffff_ffff, 4).unwrap();
        d.store(0x84, DESC >> 32, 4).unwrap();
        d.store(0x90, AVAIL & 0xffff_ffff, 4).unwrap();
        d.store(0x94, AVAIL >> 32, 4).unwrap();
        d.store(0xa0, USED & 0xffff_ffff, 4).unwrap();
        d.store(0xa4, USED >> 32, 4).unwrap();
        d.store(0x44, 1, 4).unwrap(); // ready
        (d, ram)
    }

    fn wr_desc(ram: &mut [u8], i: u64, addr: u64, len: u32, flags: u16, next: u16) {
        let off = (DESC - BASE + 16 * i) as usize;
        ram[off..off + 8].copy_from_slice(&addr.to_le_bytes());
        ram[off + 8..off + 12].copy_from_slice(&len.to_le_bytes());
        ram[off + 12..off + 14].copy_from_slice(&flags.to_le_bytes());
        ram[off + 14..off + 16].copy_from_slice(&next.to_le_bytes());
    }

    fn set_avail(ram: &mut [u8], idx: u16, ring: &[u16]) {
        let off = (AVAIL - BASE) as usize;
        ram[off + 2..off + 4].copy_from_slice(&idx.to_le_bytes());
        for (i, h) in ring.iter().enumerate() {
            ram[off + 4 + 2 * i..off + 6 + 2 * i].copy_from_slice(&h.to_le_bytes());
        }
    }

    fn used_idx(ram: &[u8]) -> u16 {
        read_u16(ram, BASE, USED + 2).unwrap()
    }

    #[test]
    fn rx_delivery_two_desc_chain() {
        let (mut d, mut ram) = setup(0);
        // Kernel-style small rx buffer: hdr desc chained to data desc.
        wr_desc(&mut ram, 0, BUF, VNET_HDR_LEN as u32, DESC_F_WRITE | DESC_F_NEXT, 1);
        wr_desc(&mut ram, 1, BUF + VNET_HDR_LEN as u64, 2048, DESC_F_WRITE, 0);
        set_avail(&mut ram, 1, &[0]);
        d.rx_push(&[0xaa; 60]);
        d.process(&mut ram, BASE);
        assert_eq!(used_idx(&ram), 1, "used ring advanced");
        let len = read_u32(&ram, BASE, USED + 4 + 4).unwrap();
        assert_eq!(len as usize, VNET_HDR_LEN + 60, "written length");
        let hdr = ram_slice(&ram, BASE, BUF, VNET_HDR_LEN).unwrap();
        assert_eq!(hdr[10], 1, "num_buffers = 1");
        let body = ram_slice(&ram, BASE, BUF + VNET_HDR_LEN as u64, 60).unwrap();
        assert!(body.iter().all(|&b| b == 0xaa), "frame bytes in place");
        assert!(d.irq_level(), "interrupt asserted");
    }

    #[test]
    fn rx_single_desc_buffer() {
        let (mut d, mut ram) = setup(0);
        wr_desc(&mut ram, 0, BUF, 2048, DESC_F_WRITE, 0);
        set_avail(&mut ram, 1, &[0]);
        d.rx_push(&[0x55; 100]);
        d.process(&mut ram, BASE);
        assert_eq!(used_idx(&ram), 1);
        let len = read_u32(&ram, BASE, USED + 4 + 4).unwrap();
        assert_eq!(len as usize, VNET_HDR_LEN + 100);
    }

    #[test]
    fn rx_waits_for_buffers() {
        let (mut d, mut ram) = setup(0);
        d.rx_push(&[0xaa; 60]);
        d.process(&mut ram, BASE); // no avail buffers yet
        assert_eq!(used_idx(&ram), 0);
        assert!(d.has_rx_pending());
        wr_desc(&mut ram, 0, BUF, 2048, DESC_F_WRITE, 0);
        set_avail(&mut ram, 1, &[0]);
        d.process(&mut ram, BASE);
        assert_eq!(used_idx(&ram), 1);
        assert!(!d.has_rx_pending());
    }

    #[test]
    fn tx_collects_frame() {
        let (mut d, mut ram) = setup(1);
        // hdr+frame in one read-only descriptor.
        let frame = [0x11u8; 42];
        let mut payload = vec![0u8; VNET_HDR_LEN];
        payload.extend_from_slice(&frame);
        let off = (BUF - BASE) as usize;
        ram[off..off + payload.len()].copy_from_slice(&payload);
        wr_desc(&mut ram, 0, BUF, payload.len() as u32, 0, 0);
        set_avail(&mut ram, 1, &[0]);
        d.process(&mut ram, BASE);
        assert_eq!(used_idx(&ram), 1);
        assert_eq!(d.tx_frames.len(), 2 + 42);
        assert_eq!(u16::from_le_bytes([d.tx_frames[0], d.tx_frames[1]]), 42);
        assert!(d.tx_frames[2..].iter().all(|&b| b == 0x11));
    }
}
