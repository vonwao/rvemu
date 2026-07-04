//! Minimal ELF64 loader: PT_LOAD segments into physical RAM, entry point,
//! and the handful of symbols the harness contract needs (tohost,
//! begin_signature/end_signature).

pub struct LoadedElf {
    pub entry: u64,
    /// (paddr, bytes) pairs to place in memory.
    pub segments: Vec<(u64, Vec<u8>)>,
    pub tohost: Option<u64>,
    pub begin_signature: Option<u64>,
    pub end_signature: Option<u64>,
}

fn rd16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn rd32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn rd64(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

pub fn load(bytes: &[u8]) -> Result<LoadedElf, String> {
    if bytes.len() < 64 || &bytes[0..4] != b"\x7fELF" {
        return Err("not an ELF file".into());
    }
    if bytes[4] != 2 || bytes[5] != 1 {
        return Err("not a little-endian ELF64".into());
    }
    let e_machine = rd16(bytes, 18);
    if e_machine != 243 {
        return Err(format!("not a RISC-V ELF (machine {})", e_machine));
    }
    // Spike/fesvr rule: ET_EXEC (2) loads at its stated addresses; any other
    // type (e.g. OpenSBI's ET_DYN fw_payload) is loaded at DRAM_BASE.
    let e_type = rd16(bytes, 16);
    let load_offset: u64 = if e_type == 2 { 0 } else { 0x8000_0000 };
    let entry = rd64(bytes, 24).wrapping_add(load_offset);
    let phoff = rd64(bytes, 32) as usize;
    let shoff = rd64(bytes, 40) as usize;
    let phentsize = rd16(bytes, 54) as usize;
    let phnum = rd16(bytes, 56) as usize;
    let shentsize = rd16(bytes, 58) as usize;
    let shnum = rd16(bytes, 60) as usize;

    let mut segments = Vec::new();
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        if p + 56 > bytes.len() {
            return Err("truncated program headers".into());
        }
        let p_type = rd32(bytes, p);
        if p_type != 1 {
            continue; // PT_LOAD only
        }
        let p_offset = rd64(bytes, p + 8) as usize;
        let p_paddr = rd64(bytes, p + 24).wrapping_add(load_offset);
        let p_filesz = rd64(bytes, p + 32) as usize;
        let p_memsz = rd64(bytes, p + 40) as usize;
        if p_offset + p_filesz > bytes.len() {
            return Err("truncated segment".into());
        }
        let mut data = bytes[p_offset..p_offset + p_filesz].to_vec();
        data.resize(p_memsz, 0);
        segments.push((p_paddr, data));
    }

    // Symbol lookup via .symtab/.strtab.
    let mut tohost = None;
    let mut begin_signature = None;
    let mut end_signature = None;
    for i in 0..shnum {
        let s = shoff + i * shentsize;
        if s + 64 > bytes.len() {
            break;
        }
        let sh_type = rd32(bytes, s + 4);
        if sh_type != 2 {
            continue; // SHT_SYMTAB
        }
        let sym_off = rd64(bytes, s + 24) as usize;
        let sym_size = rd64(bytes, s + 32) as usize;
        let link = rd32(bytes, s + 40) as usize; // strtab section index
        let entsize = rd64(bytes, s + 56) as usize;
        if entsize == 0 || link >= shnum {
            continue;
        }
        let str_hdr = shoff + link * shentsize;
        let str_off = rd64(bytes, str_hdr + 24) as usize;
        let str_size = rd64(bytes, str_hdr + 32) as usize;
        let strtab = &bytes[str_off..(str_off + str_size).min(bytes.len())];
        for j in 0..sym_size / entsize {
            let e = sym_off + j * entsize;
            if e + 24 > bytes.len() {
                break;
            }
            let name_off = rd32(bytes, e) as usize;
            let value = rd64(bytes, e + 8);
            if name_off >= strtab.len() {
                continue;
            }
            let end = strtab[name_off..].iter().position(|&c| c == 0).unwrap_or(0) + name_off;
            match &strtab[name_off..end] {
                b"tohost" => tohost = Some(value),
                b"begin_signature" => begin_signature = Some(value),
                b"end_signature" => end_signature = Some(value),
                _ => {}
            }
        }
    }

    Ok(LoadedElf {
        entry,
        segments,
        tohost,
        begin_signature,
        end_signature,
    })
}
