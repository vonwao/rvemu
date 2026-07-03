//! rvemu CLI, implementing exactly the harness contract in harness/README.md.
//! Exit codes: 0 = tohost PASS, 1 = tohost FAIL, 2 = budget exhausted /
//! usage error / load error.

use rvemu_core::platform::StdioPlatform;
use rvemu_core::{elf, machine};
use std::io::Write;

fn main() {
    let mut image: Option<String> = None;
    let mut max_insns: u64 = u64::MAX;
    let mut trace: Option<String> = None;
    let mut signature: Option<String> = None;
    let mut sig_granularity: u64 = 4;
    let mut ram_mib: usize = 256;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--max-insns" => max_insns = req(&mut args, "--max-insns").parse().unwrap_or_else(|_| usage()),
            "--trace" => trace = Some(req(&mut args, "--trace")),
            "--signature" => signature = Some(req(&mut args, "--signature")),
            "--signature-granularity" => {
                sig_granularity = req(&mut args, "--signature-granularity").parse().unwrap_or(4)
            }
            "--ram-mib" => ram_mib = req(&mut args, "--ram-mib").parse().unwrap_or_else(|_| usage()),
            _ if a.starts_with('-') => usage(),
            _ => image = Some(a),
        }
    }
    let Some(image) = image else { usage() };

    let bytes = match std::fs::read(&image) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("rvemu: cannot read {}: {}", image, e);
            std::process::exit(2);
        }
    };
    let loaded = match elf::load(&bytes) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("rvemu: {}: {}", image, e);
            std::process::exit(2);
        }
    };

    let mut m = machine::Machine::new(&loaded, ram_mib, &[]);

    let mut trace_file = trace.map(|p| {
        std::io::BufWriter::new(std::fs::File::create(p).unwrap_or_else(|e| {
            eprintln!("rvemu: cannot open trace file: {}", e);
            std::process::exit(2);
        }))
    });
    m.cpu.trace_enabled = trace_file.is_some();

    let mut platform = StdioPlatform::new();
    let exit = m.run(max_insns, &mut platform, |line| {
        if let Some(f) = trace_file.as_mut() {
            let _ = writeln!(f, "{}", line);
        }
    });
    if let Some(f) = trace_file.as_mut() {
        let _ = f.flush();
    }

    if let Some(sig_path) = signature {
        if sig_granularity != 4 {
            eprintln!("rvemu: only --signature-granularity 4 is supported");
            std::process::exit(2);
        }
        match m.signature() {
            Some(words) => {
                let mut out = String::new();
                for w in words {
                    out.push_str(&format!("{:08x}\n", w));
                }
                if let Err(e) = std::fs::write(&sig_path, out) {
                    eprintln!("rvemu: cannot write signature: {}", e);
                    std::process::exit(2);
                }
            }
            None => {
                eprintln!("rvemu: no begin_signature/end_signature symbols");
                std::process::exit(2);
            }
        }
    }

    match exit {
        machine::RunExit::Tohost(1) => std::process::exit(0),
        machine::RunExit::Tohost(v) => {
            eprintln!("FAIL test {} (tohost=0x{:x})", v >> 1, v);
            std::process::exit(1);
        }
        machine::RunExit::Budget => {
            eprintln!("rvemu: instruction budget exhausted");
            std::process::exit(2);
        }
    }
}

fn req(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("rvemu: {} needs a value", flag);
        std::process::exit(2);
    })
}

fn usage() -> ! {
    eprintln!("usage: rvemu [--max-insns N] [--trace FILE] [--signature FILE [--signature-granularity 4]] [--ram-mib N] <elf>");
    std::process::exit(2);
}
