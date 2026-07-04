//! WebAssembly target: the same rvemu core, exported through hand-rolled
//! extern "C" functions (no bindings crates). JS owns the terminal; console
//! bytes flow through small ring buffers inside the Platform implementation.

use rvemu_core::platform::Platform;
use rvemu_core::{elf, machine};
use std::cell::RefCell;

/// Console bridge: output accumulates until JS drains it; input is queued
/// from JS key events.
struct WasmPlatform {
    out: Vec<u8>,
    inp: std::collections::VecDeque<u8>,
}

impl Platform for WasmPlatform {
    fn console_write(&mut self, byte: u8) {
        self.out.push(byte);
    }
    fn console_read(&mut self) -> Option<u8> {
        self.inp.pop_front()
    }
}

struct State {
    machine: machine::Machine,
    platform: WasmPlatform,
    done: bool,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static IMAGE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Reserve space for the guest image; returns a pointer JS writes into.
#[no_mangle]
pub extern "C" fn image_alloc(len: usize) -> *mut u8 {
    IMAGE.with(|img| {
        let mut img = img.borrow_mut();
        img.clear();
        img.resize(len, 0);
        img.as_mut_ptr()
    })
}

/// Parse the uploaded ELF and construct the machine. Returns 0 on success.
#[no_mangle]
pub extern "C" fn boot(ram_mib: usize) -> i32 {
    IMAGE.with(|img| {
        let img = img.borrow();
        let loaded = match elf::load(&img) {
            Ok(l) => l,
            Err(_) => return 1,
        };
        let machine = machine::Machine::new(&loaded, ram_mib, &[]);
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                machine,
                platform: WasmPlatform {
                    out: Vec::new(),
                    inp: std::collections::VecDeque::new(),
                },
                done: false,
            });
        });
        0
    })
}

/// Run up to `steps` instructions. Returns 1 when the guest halted via
/// tohost, 0 while running, -1 if not booted.
#[no_mangle]
pub extern "C" fn run(steps: u64) -> i32 {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(st) = s.as_mut() else { return -1 };
        if st.done {
            return 1;
        }
        match st.machine.run(steps, &mut st.platform, |_| {}) {
            machine::RunExit::Tohost(_) => {
                st.done = true;
                1
            }
            machine::RunExit::Budget => 0,
        }
    })
}

/// Queue one input byte (terminal keystroke).
#[no_mangle]
pub extern "C" fn console_in(byte: u8) {
    STATE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.platform.inp.push_back(byte);
        }
    });
}

/// Number of pending console-output bytes.
#[no_mangle]
pub extern "C" fn console_out_len() -> usize {
    STATE.with(|s| s.borrow().as_ref().map_or(0, |st| st.platform.out.len()))
}

/// Pointer to the pending console-output bytes (valid until next run call).
#[no_mangle]
pub extern "C" fn console_out_ptr() -> *const u8 {
    STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |st| st.platform.out.as_ptr())
    })
}

/// Discard drained console output.
#[no_mangle]
pub extern "C" fn console_out_clear() {
    STATE.with(|s| {
        if let Some(st) = s.borrow_mut().as_mut() {
            st.platform.out.clear();
        }
    });
}

/// Total retired instructions (for the on-page counter).
#[no_mangle]
pub extern "C" fn retired() -> u64 {
    STATE.with(|s| s.borrow().as_ref().map_or(0, |st| st.machine.cpu.retired))
}
