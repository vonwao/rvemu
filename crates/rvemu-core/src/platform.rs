/// Host-services abstraction. Everything the emulator core needs from the
/// outside world goes through this trait, so the same core logic compiles
/// unchanged for the native CLI and (later) a WebAssembly target.
///
/// The core itself is deterministic: the timer (CLINT `mtime`) is derived
/// from executed instruction count, not wall-clock time, so lockstep
/// comparison against a reference simulator is reproducible. The trait only
/// covers genuinely host-dependent services.
pub trait Platform {
    /// Write one byte of console output (UART transmit).
    fn console_write(&mut self, byte: u8);

    /// Non-blocking console input (UART receive). `None` when no byte is
    /// pending.
    fn console_read(&mut self) -> Option<u8>;
}

/// Native stdin/stdout implementation used by the CLI. Stdin is drained by
/// a background thread so `console_read` never blocks (works for pipes and
/// ptys alike).
pub struct StdioPlatform {
    rx: std::sync::mpsc::Receiver<u8>,
}

impl StdioPlatform {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut stdin = std::io::stdin().lock();
            let mut buf = [0u8; 1];
            while let Ok(n) = stdin.read(&mut buf) {
                if n == 0 {
                    break; // EOF: stop feeding input
                }
                if tx.send(buf[0]).is_err() {
                    break;
                }
            }
        });
        Self { rx }
    }
}

impl Default for StdioPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for StdioPlatform {
    fn console_write(&mut self, byte: u8) {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(&[byte]);
        let _ = out.flush();
    }

    fn console_read(&mut self) -> Option<u8> {
        self.rx.try_recv().ok()
    }
}
