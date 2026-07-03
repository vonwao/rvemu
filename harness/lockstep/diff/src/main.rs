//! lockstep-diff: streaming comparator for reference (Spike) vs DUT (rvemu)
//! instruction traces.
//!
//! Usage: lockstep-diff <reference-trace> <dut-trace> [--ref-format spike|canonical] [--context N]
//!
//! The reference input is Spike's `--log-commits` output (default
//! `--ref-format spike`); the DUT input is rvemu's `--trace` output, which is
//! already canonical. Every line is normalized to the canonical form defined
//! in harness/README.md and compared for exact string equality. A shadow
//! integer register file is maintained per side (from the write annotations)
//! so the divergence report shows full architectural state.
//!
//! Exit codes:
//!   0 = both traces ended together with no divergence
//!   1 = divergence (first differing instruction reported)
//!   2 = usage/IO error
//!   3 = clean common prefix, but one trace ended before the other (counts
//!       reported; expected when the DUT halts on tohost while Spike commits
//!       a few more loop iterations before HTIF stops it, or when one side
//!       hits an instruction budget)

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};

/// CSRs excluded from comparison: free-running counters that legitimately
/// differ between reference and DUT. Fixed, frozen list.
fn is_ignored_csr(name: &str) -> bool {
    matches!(name, "cycle" | "time" | "instret" | "mcycle" | "minstret")
        || name.starts_with("hpmcounter")
        || name.starts_with("mhpmcounter")
        || name.starts_with("mhpmevent")
}

enum RefFormat {
    Spike,
    Canonical,
}

/// Convert one Spike commit-log line to canonical form.
/// Spike:  `core   0: 3 0x0000000080000000 (0x00000093) x1  0x00...0 mem 0x... [0x...] c773_mtvec 0x...`
/// Canon:  `p3 0000000080000000 00000093 x1=0x0000000000000000 m:0x...[=0x...] c:mtvec=0x...`
/// Returns None for non-commit lines (Spike prints nothing else with
/// --log-commits, but be tolerant of blank lines).
fn spike_to_canonical(line: &str) -> Option<String> {
    let rest = line.strip_prefix("core")?.trim_start();
    let rest = rest.strip_prefix(|c: char| c.is_ascii_digit())?;
    let rest = rest.strip_prefix(':')?.trim_start();
    let mut tok = rest.split_whitespace().peekable();
    let priv_lvl = tok.next()?;
    let pc = tok.next()?.strip_prefix("0x")?;
    let insn = tok.next()?.strip_prefix("(0x")?.strip_suffix(')')?;
    let insn_val = u32::from_str_radix(insn, 16).ok()?;
    let mut out = format!("p{} {:0>16} {:08x}", priv_lvl, pc.to_lowercase(), insn_val);
    while let Some(t) = tok.next() {
        if let Some(reg) = t.strip_prefix('x') {
            if let Ok(n) = reg.parse::<u8>() {
                let val = tok.next()?;
                if n != 0 {
                    out.push_str(&format!(" x{}={}", n, val));
                }
                continue;
            }
        }
        if t == "mem" {
            let addr = tok.next()?;
            // Store: a value token follows; load: next token is a new kind.
            if let Some(next) = tok.peek() {
                if next.starts_with("0x") {
                    let val = tok.next()?;
                    out.push_str(&format!(" m:{}={}", addr, val));
                    continue;
                }
            }
            out.push_str(&format!(" m:{}", addr));
            continue;
        }
        if t.starts_with('c') {
            // c<addr>_<name>
            if let Some((_, name)) = t.split_once('_') {
                let val = tok.next()?;
                if !is_ignored_csr(name) {
                    out.push_str(&format!(" c:{}={}", name, val));
                }
                continue;
            }
        }
        // Unknown token kind: keep verbatim so a mismatch is visible rather
        // than silently dropped.
        out.push_str(" ?");
        out.push_str(t);
    }
    Some(out)
}

/// Filter a canonical DUT line: drop ignored-CSR tokens so the DUT does not
/// have to special-case counters.
fn filter_canonical(line: &str) -> String {
    line.split_whitespace()
        .filter(|t| match t.strip_prefix("c:") {
            Some(rest) => match rest.split_once('=') {
                Some((name, _)) => !is_ignored_csr(name),
                None => true,
            },
            None => true,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct Side {
    name: &'static str,
    reader: BufReader<Box<dyn Read>>,
    regs: [u64; 32],
    format: RefFormat,
}

impl Side {
    fn open(name: &'static str, path: &str, format: RefFormat) -> std::io::Result<Side> {
        let inner: Box<dyn Read> = Box::new(std::fs::File::open(path)?);
        Ok(Side {
            name,
            reader: BufReader::new(inner),
            regs: [0; 32],
            format,
        })
    }

    fn next_canonical(&mut self) -> Option<String> {
        let mut s = String::new();
        loop {
            s.clear();
            match self.reader.read_line(&mut s) {
                Ok(0) => return None,
                Ok(_) => {
                    let t = s.trim_end();
                    if t.is_empty() {
                        continue;
                    }
                    match self.format {
                        RefFormat::Spike => {
                            if let Some(c) = spike_to_canonical(t) {
                                return Some(c);
                            }
                            // Non-commit line (e.g. spike warnings): skip.
                        }
                        RefFormat::Canonical => return Some(filter_canonical(t)),
                    }
                }
                Err(e) => {
                    eprintln!("lockstep-diff: read error on {}: {}", self.name, e);
                    std::process::exit(2);
                }
            }
        }
    }

    fn apply(&mut self, canonical: &str) {
        for t in canonical.split_whitespace().skip(3) {
            if let Some(rest) = t.strip_prefix('x') {
                if let Some((n, v)) = rest.split_once('=') {
                    if let (Ok(n), Ok(v)) = (
                        n.parse::<usize>(),
                        u64::from_str_radix(v.trim_start_matches("0x"), 16),
                    ) {
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
        out.push_str(&format!(
            "x{:<2}=0x{:016x}{}",
            i,
            side.regs[i],
            if i % 4 == 3 { "\n" } else { "  " }
        ));
    }
    out
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // --canonicalize <spike-log>: print the canonical form of a Spike commit
    // log (used by the self-test and for manual inspection).
    if args.first().map(|s| s.as_str()) == Some("--canonicalize") {
        let path = args.get(1).unwrap_or_else(|| {
            eprintln!("usage: lockstep-diff --canonicalize <spike-log>");
            std::process::exit(2);
        });
        let f = std::fs::File::open(path).unwrap_or_else(|e| {
            eprintln!("lockstep-diff: cannot open {}: {}", path, e);
            std::process::exit(2);
        });
        for line in BufReader::new(f).lines() {
            let line = line.unwrap_or_default();
            if let Some(c) = spike_to_canonical(line.trim_end()) {
                println!("{}", c);
            }
        }
        std::process::exit(0);
    }
    if args.len() < 2 {
        eprintln!("usage: lockstep-diff <reference-trace> <dut-trace> [--ref-format spike|canonical] [--context N]");
        std::process::exit(2);
    }
    let mut context = 8usize;
    let mut ref_format = RefFormat::Spike;
    if let Some(i) = args.iter().position(|a| a == "--context") {
        context = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(8);
    }
    if let Some(i) = args.iter().position(|a| a == "--ref-format") {
        ref_format = match args.get(i + 1).map(|s| s.as_str()) {
            Some("canonical") => RefFormat::Canonical,
            _ => RefFormat::Spike,
        };
    }

    let mut reference = Side::open("reference", &args[0], ref_format).unwrap_or_else(|e| {
        eprintln!("lockstep-diff: cannot open reference {}: {}", args[0], e);
        std::process::exit(2);
    });
    let mut dut = Side::open("dut", &args[1], RefFormat::Canonical).unwrap_or_else(|e| {
        eprintln!("lockstep-diff: cannot open dut {}: {}", args[1], e);
        std::process::exit(2);
    });

    let mut history: VecDeque<String> = VecDeque::with_capacity(context + 1);
    let mut count: u64 = 0;

    loop {
        let r = reference.next_canonical();
        let d = dut.next_canonical();
        match (r, d) {
            (None, None) => {
                println!("LOCKSTEP OK: {} instructions, traces ended together, no divergence", count);
                std::process::exit(0);
            }
            (Some(rl), None) => {
                println!(
                    "LOCKSTEP PREFIX-CLEAN: dut ended after {} instructions; reference continues:",
                    count
                );
                println!("  {}", rl);
                print_context(&history);
                std::process::exit(3);
            }
            (None, Some(dl)) => {
                println!(
                    "LOCKSTEP PREFIX-CLEAN: reference ended after {} instructions; dut continues:",
                    count
                );
                println!("  {}", dl);
                print_context(&history);
                std::process::exit(3);
            }
            (Some(rl), Some(dl)) => {
                count += 1;
                if rl != dl {
                    println!("DIVERGENCE at instruction {}:", count);
                    println!("  reference: {}", rl);
                    println!("  dut:       {}", dl);
                    print_context(&history);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_commit() {
        assert_eq!(
            spike_to_canonical("core   0: 3 0x0000000080000050 (0x00000093) x1  0x0000000000000000").unwrap(),
            "p3 0000000080000050 00000093 x1=0x0000000000000000"
        );
    }

    #[test]
    fn parses_load_and_store() {
        assert_eq!(
            spike_to_canonical("core   0: 3 0x000000000000100c (0x0182b283) x5  0x0000000080000000 mem 0x0000000000001018").unwrap(),
            "p3 000000000000100c 0182b283 x5=0x0000000080000000 m:0x0000000000001018"
        );
        assert_eq!(
            spike_to_canonical("core   0: 3 0x0000000080000040 (0xfc3f2223) mem 0x0000000080001000 0x00000001").unwrap(),
            "p3 0000000080000040 fc3f2223 m:0x0000000080001000=0x00000001"
        );
    }

    #[test]
    fn parses_csr_write_and_filters_counters() {
        assert_eq!(
            spike_to_canonical("core   0: 3 0x00000000800000dc (0x30529073) c773_mtvec 0x00000000800000e4").unwrap(),
            "p3 00000000800000dc 30529073 c:mtvec=0x00000000800000e4"
        );
        assert_eq!(
            spike_to_canonical("core   0: 3 0x0000000080000000 (0xb00027f3) x15 0x0000000000001234 c2816_mcycle 0x0000000000001234").unwrap(),
            "p3 0000000080000000 b00027f3 x15=0x0000000000001234"
        );
    }

    #[test]
    fn compressed_insn_normalized_to_8_digits() {
        assert_eq!(
            spike_to_canonical("core   0: 3 0x0000000080000004 (0x1141) x2  0x0000000080000000").unwrap(),
            "p3 0000000080000004 00001141 x2=0x0000000080000000"
        );
    }

    #[test]
    fn x0_write_dropped() {
        assert_eq!(
            spike_to_canonical("core   0: 3 0x0000000080000000 (0x00000033) x0  0x0000000000000000").unwrap(),
            "p3 0000000080000000 00000033"
        );
    }

    #[test]
    fn dut_counter_csr_filtered() {
        assert_eq!(
            filter_canonical("p3 0000000080000000 b00027f3 x15=0x1 c:mcycle=0x1"),
            "p3 0000000080000000 b00027f3 x15=0x1"
        );
    }
}
