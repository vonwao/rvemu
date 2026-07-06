//! virtio-mmio (spec 1.1, "modern" version 2) transport carrying a
//! virtio-net device. Extras only: the device exists only when
//! `Bus::enable_net` was called (the wasm demo build), so certified targets
//! and the lockstep platform never see it. The host side exchanges raw
//! ethernet frames: guest->host frames accumulate in `tx_frames`; the host
//! injects frames with `rx_push` and they are delivered into posted rx
//! buffers on the next device tick.

const MAGIC: u32 = 0x7472_6976; // "virt"
const VERSION: u32 = 2;
const DEVICE_ID_NET: u32 = 1;
const VENDOR_ID: u32 = 0x726d_7665; // "rvem"

const VIRTIO_F_VERSION_1: u64 = 1 << 32;
const VIRTIO_NET_F_MAC: u64 = 1 << 5;

const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;

/// Fixed, documented MAC for the guest NIC.
pub const GUEST_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

const QUEUE_NUM_MAX: u32 = 256;
const VNET_HDR_LEN: usize = 12; // virtio_net_hdr_v1 (VERSION_1 layout)
const MAX_FRAME: usize = 1600;

#[derive(Default, Clone)]
struct Queue {
    ready: bool,
    num: u32,
    desc: u64,
    driver: u64, // avail ring
    device: u64, // used ring
    last_avail: u16,
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
        let q = self.queues[0].clone();
        if !q.ready || q.num == 0 {
            return;
        }
        let mut last = q.last_avail;
        let mut used = false;
        while let Some(frame) = self.rx_pending.front() {
            let avail_idx = match read_u16(ram, ram_base, q.driver + 2) {
                Some(v) => v,
                None => break,
            };
            if last == avail_idx {
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

fn set_lo(v: &mut u64, val: u32) {
    *v = (*v & !0xffff_ffff) | val as u64;
}
fn set_hi(v: &mut u64, val: u32) {
    *v = (*v & 0xffff_ffff) | ((val as u64) << 32);
}

fn ram_slice<'a>(ram: &'a [u8], base: u64, addr: u64, len: usize) -> Option<&'a [u8]> {
    let off = addr.checked_sub(base)? as usize;
    ram.get(off..off + len)
}
fn ram_slice_mut<'a>(ram: &'a mut [u8], base: u64, addr: u64, len: usize) -> Option<&'a mut [u8]> {
    let off = addr.checked_sub(base)? as usize;
    ram.get_mut(off..off + len)
}
fn read_u16(ram: &[u8], base: u64, addr: u64) -> Option<u16> {
    ram_slice(ram, base, addr, 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}
fn read_u32(ram: &[u8], base: u64, addr: u64) -> Option<u32> {
    ram_slice(ram, base, addr, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
fn read_u64(ram: &[u8], base: u64, addr: u64) -> Option<u64> {
    ram_slice(ram, base, addr, 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
}
fn write_u16(ram: &mut [u8], base: u64, addr: u64, v: u16) {
    if let Some(b) = ram_slice_mut(ram, base, addr, 2) {
        b.copy_from_slice(&v.to_le_bytes());
    }
}
fn write_u32(ram: &mut [u8], base: u64, addr: u64, v: u32) {
    if let Some(b) = ram_slice_mut(ram, base, addr, 4) {
        b.copy_from_slice(&v.to_le_bytes());
    }
}

/// Append one used-ring element and bump used.idx.
fn push_used(ram: &mut [u8], base: u64, q: &Queue, head: u16, len: u32) {
    let idx = read_u16(ram, base, q.device + 2).unwrap_or(0);
    let slot = (idx as u32 % q.num) as u64;
    let elem = q.device + 4 + 8 * slot;
    write_u32(ram, base, elem, head as u32);
    write_u32(ram, base, elem + 4, len);
    write_u16(ram, base, q.device + 2, idx.wrapping_add(1));
}
