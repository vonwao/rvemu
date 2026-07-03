//! lockstep-diff: streaming comparator for two normalized instruction traces.
//!
//! Usage: lockstep-diff <reference-trace> <dut-trace> [--context N]
//!
//! Both inputs are files or FIFOs in the normalized trace format defined in
//! harness/README.md: one line per retired instruction,
//!   <pc-hex-16> <insn-hex-8> [xN=<hex16>]... [f:<csr>=<hex16>]...
//! Lines are compared for exact equality. The comparator also maintains a
//! shadow integer register file per side (applied from the write annotations)
//! so a divergence report can show full architectural state, plus a ring
//! buffer of preceding instructions for context.
//!
//! Exit codes: 0 = traces identical to common end, 1 = divergence found,
//! 2 = usage/IO error.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};

struct Side {
    name: &'static str,
    reader: BufReader<std::fs::File>,
    regs: [u64; 32],
    line_no: u64,
}

impl Side {
    fn open(name: &'static str, path: &str) -> std::io::Result<Side> {
        Ok(Side {
            name,
            reader: BufReader::new(std::fs::File::open(path)?),
            regs: [0; 32],
            line_no: 0,
        })
    }

    fn next_line(&mut self) -> Option<String> {
        let mut s = String::new();
        loop {
            s.clear();
            match self.reader.read_line(&mut s) {
                Ok(0) => return None,
                Ok(_) => {
                    let t = s.trim_end();
                    if !t.is_empty() {
                        self.line_no += 1;
                        return Some(t.to_string());
                    }
                }
                Err(e) => {
                    eprintln!("lockstep-diff: read error on {}: {}", self.name, e);
                    std::process::exit(2);
                }
            }
        }
    }

    /// Apply the xN= writebacks of a normalized line to the shadow regfile.
    fn apply(&mut self, line: &str) {
        for tok in line.split_whitespace().skip(2) {
            if let Some(rest) = tok.strip_prefix('x') {
                if let Some((n, v)) = rest.split_once('=') {
                    if let (Ok(n), Ok(v)) = (n.parse::<usize>(), u64::from_str_radix(v.trim_start_matches("0x"), 16)) {
                        if n > 0 && n < 32 {
                            self.regs[n] = v;
                        }
                    }
                }
            }
        }
    }
}

fn dump_regs(side: &Side) -> String {
    let mut out = String::new();
    for i in 0..32 {
        out.push_str(&format!("x{:<2}=0x{:016x}{}", i, side.regs[i], if i % 4 == 3 { "\n" } else { "  " }));
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: lockstep-diff <reference-trace> <dut-trace> [--context N]");
        std::process::exit(2);
    }
    let mut context = 8usize;
    if let Some(i) = args.iter().position(|a| a == "--context") {
        context = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(8);
    }
    let mut reference = Side::open("reference", &args[0]).unwrap_or_else(|e| {
        eprintln!("lockstep-diff: cannot open reference {}: {}", args[0], e);
        std::process::exit(2);
    });
    let mut dut = Side::open("dut", &args[1]).unwrap_or_else(|e| {
        eprintln!("lockstep-diff: cannot open dut {}: {}", args[1], e);
        std::process::exit(2);
    });

    let mut history: VecDeque<String> = VecDeque::with_capacity(context + 1);
    let mut count: u64 = 0;

    loop {
        let r = reference.next_line();
        let d = dut.next_line();
        match (r, d) {
            (None, None) => {
                println!("LOCKSTEP OK: {} instructions, no divergence", count);
                std::process::exit(0);
            }
            (Some(rl), None) => {
                println!("DIVERGENCE at instruction {}: dut trace ended early", count + 1);
                println!("reference continues with:\n  {}", rl);
                print_context(&history);
                std::process::exit(1);
            }
            (None, Some(dl)) => {
                println!("DIVERGENCE at instruction {}: reference trace ended early", count + 1);
                println!("dut continues with:\n  {}", dl);
                print_context(&history);
                std::process::exit(1);
            }
            (Some(rl), Some(dl)) => {
                count += 1;
                if rl != dl {
                    println!("DIVERGENCE at instruction {}:", count);
                    println!("  reference: {}", rl);
                    println!("  dut:       {}", dl);
                    print_context(&history);
                    // Shadow state BEFORE this instruction (writes not applied).
                    println!("reference regs before divergent instruction:\n{}", dump_regs(&reference));
                    println!("dut regs before divergent instruction:\n{}", dump_regs(&dut));
                    std::process::exit(1);
                }
                reference.apply(&rl);
                dut.apply(&dl);
                history.push_back(rl);
                if history.len() > context {
                    history.pop_front();
                }
            }
        }
    }
}

fn print_context(history: &VecDeque<String>) {
    if history.is_empty() {
        println!("(no preceding instructions)");
        return;
    }
    println!("preceding {} instructions (both sides identical):", history.len());
    for h in history {
        println!("  {}", h);
    }
}
