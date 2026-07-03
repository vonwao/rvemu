//! ns16550 UART modeled register-for-register on the pinned Spike's
//! riscv/ns16550.cc (reg_shift=0, io_width=1, PLIC source 1). Behavioral
//! quirks preserved deliberately: level-triggered THR-empty interrupt,
//! LSR TEMT|THRE forced when THRI is disabled, FCR clear bits consumed in
//! update_interrupt, and the 16-tick RX poll backoff.

use std::collections::VecDeque;

const UART_QUEUE_SIZE: usize = 64;
const MAX_BACKOFF: u32 = 16;

pub const IER_RDI: u8 = 0x01;
pub const IER_THRI: u8 = 0x02;
const IIR_NO_INT: u8 = 0x01;
const IIR_THRI: u8 = 0x02;
const IIR_RDI: u8 = 0x04;
const IIR_TYPE_BITS: u8 = 0xc0;
const FCR_CLEAR_RCVR: u8 = 0x02;
const FCR_CLEAR_XMIT: u8 = 0x04;
const LCR_DLAB: u8 = 0x80;
const MCR_LOOP: u8 = 0x10;
const MCR_OUT2: u8 = 0x08;
const LSR_DR: u8 = 0x01;
const LSR_BI: u8 = 0x10;
const LSR_THRE: u8 = 0x20;
const LSR_TEMT: u8 = 0x40;
const MSR_DCD: u8 = 0x80;
const MSR_DSR: u8 = 0x20;
const MSR_CTS: u8 = 0x10;

pub struct Uart {
    ier: u8,
    iir: u8,
    fcr: u8,
    lcr: u8,
    mcr: u8,
    lsr: u8,
    msr: u8,
    dll: u8,
    dlm: u8,
    scr: u8,
    rx_queue: VecDeque<u8>,
    backoff_counter: u32,
    /// Level presented to the PLIC (source 1).
    pub irq_level: bool,
    /// Bytes transmitted (drained by the platform layer each step).
    pub tx_out: Vec<u8>,
}

impl Uart {
    pub fn new() -> Self {
        Uart {
            ier: 0,
            iir: IIR_NO_INT,
            fcr: 0,
            lcr: 0,
            mcr: MCR_OUT2,
            lsr: LSR_TEMT | LSR_THRE,
            msr: MSR_DCD | MSR_DSR | MSR_CTS,
            dll: 0x0c,
            dlm: 0,
            scr: 0,
            rx_queue: VecDeque::new(),
            backoff_counter: 0,
            irq_level: false,
            tx_out: Vec::new(),
        }
    }

    fn update_interrupt(&mut self) {
        let mut interrupts: u8 = 0;
        if self.fcr & FCR_CLEAR_RCVR != 0 {
            self.fcr &= !FCR_CLEAR_RCVR;
            self.rx_queue.clear();
            self.lsr &= !LSR_DR;
        }
        if self.fcr & FCR_CLEAR_XMIT != 0 {
            self.fcr &= !FCR_CLEAR_XMIT;
            self.lsr |= LSR_TEMT | LSR_THRE;
        }
        if self.ier & IER_RDI != 0 && self.lsr & LSR_DR != 0 {
            interrupts |= IIR_RDI;
        }
        if self.ier & IER_THRI != 0 && self.lsr & LSR_TEMT != 0 {
            interrupts |= IIR_THRI;
        }
        if interrupts == 0 {
            self.iir = IIR_NO_INT;
            self.irq_level = false;
        } else {
            self.iir = interrupts;
            self.irq_level = true;
        }
        if self.ier & IER_THRI == 0 {
            self.lsr |= LSR_TEMT | LSR_THRE;
        }
    }

    fn rx_byte(&mut self) -> u8 {
        if self.rx_queue.is_empty() {
            self.lsr &= !LSR_DR;
            return 0;
        }
        if self.lsr & LSR_BI != 0 {
            self.lsr &= !LSR_BI;
            return 0;
        }
        let ret = self.rx_queue.pop_front().unwrap();
        if self.rx_queue.is_empty() {
            self.lsr &= !LSR_DR;
        }
        ret
    }

    /// Byte-register load at `offset` (0..7); size must be 1.
    pub fn load(&mut self, offset: u64, size: u64) -> Result<u64, ()> {
        if size != 1 {
            return Err(());
        }
        let mut update = false;
        let val = match offset & 7 {
            0 => {
                update = true;
                if self.lcr & LCR_DLAB != 0 {
                    self.dll
                } else {
                    self.rx_byte()
                }
            }
            1 => {
                if self.lcr & LCR_DLAB != 0 {
                    self.dlm
                } else {
                    self.ier
                }
            }
            2 => self.iir | IIR_TYPE_BITS,
            3 => self.lcr,
            4 => self.mcr,
            5 => self.lsr,
            6 => self.msr,
            _ => self.scr,
        };
        if update {
            self.update_interrupt();
        }
        Ok(val as u64)
    }

    pub fn store(&mut self, offset: u64, val: u64, size: u64) -> Result<(), ()> {
        if size != 1 {
            return Err(());
        }
        let val = val as u8;
        let mut update = false;
        match offset & 7 {
            0 => {
                update = true;
                if self.lcr & LCR_DLAB != 0 {
                    self.dll = val;
                } else if self.mcr & MCR_LOOP != 0 {
                    if self.rx_queue.len() < UART_QUEUE_SIZE {
                        self.rx_queue.push_back(val);
                        self.lsr |= LSR_DR;
                    }
                } else {
                    self.lsr |= LSR_TEMT | LSR_THRE;
                    self.tx_out.push(val);
                }
            }
            1 => {
                if self.lcr & LCR_DLAB == 0 {
                    self.ier = val & 0x0f;
                } else {
                    self.dlm = val;
                }
                update = true;
            }
            2 => {
                self.fcr = val;
                update = true;
            }
            3 => {
                self.lcr = val;
                update = true;
            }
            4 => {
                self.mcr = val;
                update = true;
            }
            5 | 6 => {}
            _ => self.scr = val,
        }
        if update {
            self.update_interrupt();
        }
        Ok(())
    }

    /// Per-RTC-tick input poll, mirroring Spike's tick(): FIFO must be
    /// enabled, no loopback, queue not full; backoff after an empty poll.
    pub fn tick(&mut self, mut input: impl FnMut() -> Option<u8>) {
        if self.fcr & 0x01 == 0 || self.mcr & MCR_LOOP != 0 || self.rx_queue.len() >= UART_QUEUE_SIZE {
            return;
        }
        if self.backoff_counter > 0 && self.backoff_counter < MAX_BACKOFF {
            self.backoff_counter += 1;
            return;
        }
        match input() {
            None => {
                self.backoff_counter = 1;
            }
            Some(b) => {
                self.backoff_counter = 0;
                self.rx_queue.push_back(b);
                self.lsr |= LSR_DR;
                self.update_interrupt();
            }
        }
    }
}

impl Default for Uart {
    fn default() -> Self {
        Self::new()
    }
}
