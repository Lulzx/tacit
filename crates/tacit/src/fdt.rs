//! Minimal flat-device-tree parser.  We only need the physical memory map
//! (the `memory` node's `reg`), which is the guest's only authority for what
//! RAM it owns.  Missing or malformed memory info is a boot fault.

pub struct MemoryMap {
    pub base: u64,
    pub size: u64,
}

const FDT_MAGIC: u32 = 0xd00d_feed;
const FDT_BEGIN_NODE: u32 = 0x1;
const FDT_END_NODE: u32 = 0x2;
const FDT_PROP: u32 = 0x3;
const FDT_NOP: u32 = 0x4;
const FDT_END: u32 = 0x9;

fn be32(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Parse the fdt; return Ok(Some(map)) on success, Ok(None) if no memory
/// node, Err on a structurally invalid blob.
pub fn parse(fdt: &[u8]) -> Result<Option<MemoryMap>, ()> {
    if fdt.len() < 40 || be32(fdt, 0) != FDT_MAGIC {
        return Err(());
    }
    let off_struct = be32(fdt, 8) as usize;
    let off_strings = be32(fdt, 12) as usize;

    if off_struct >= fdt.len() || off_strings >= fdt.len() {
        return Err(());
    }

    let strings = |nameoff: usize| -> Option<&[u8]> {
        let start = off_strings + nameoff;
        if start >= fdt.len() {
            return None;
        }
        let mut end = start;
        while end < fdt.len() && fdt[end] != 0 {
            end += 1;
        }
        Some(&fdt[start..end])
    };

    let mut p = off_struct;
    let mut mem_base: Option<u64> = None;
    let mut mem_size: Option<u64> = None;
    let mut is_memory_node = false;
    let mut node_name = [0u8; 64];
    let mut node_len = 0usize;

    loop {
        if p + 4 > fdt.len() {
            return Err(());
        }
        let token = be32(fdt, p);
        p += 4;
        match token {
            FDT_BEGIN_NODE => {
                is_memory_node = false;
                node_len = 0;
                while p < fdt.len() && fdt[p] != 0 && node_len < 64 {
                    node_name[node_len] = fdt[p];
                    node_len += 1;
                    p += 1;
                }
                while p < fdt.len() && fdt[p] != 0 {
                    p += 1;
                }
                p += 1; // NUL
                p = (p + 3) & !3; // align 4
            }
            FDT_END_NODE => {
                is_memory_node = false;
            }
            FDT_PROP => {
                if p + 8 > fdt.len() {
                    return Err(());
                }
                let len = be32(fdt, p) as usize;
                let nameoff = be32(fdt, p + 4) as usize;
                p += 8;
                let name = match strings(nameoff) {
                    Some(n) => n,
                    None => return Err(()),
                };
                let data_start = p;
                let data_end = p + len;
                if data_end > fdt.len() {
                    return Err(());
                }

                if name == b"device_type" && len == 7 && &fdt[data_start..data_end] == b"memory\0" {
                    is_memory_node = true;
                }
                let nm = &node_name[..node_len];
                if name == b"reg"
                    && (is_memory_node
                        || nm == b"memory"
                        || (node_len > 7 && &nm[..7] == b"memory@"))
                {
                    if len >= 16 {
                        let base_hi = be32(fdt, data_start) as u64;
                        let base_lo = be32(fdt, data_start + 4) as u64;
                        let size_hi = be32(fdt, data_start + 8) as u64;
                        let size_lo = be32(fdt, data_start + 12) as u64;
                        mem_base = Some((base_hi << 32) | base_lo);
                        mem_size = Some((size_hi << 32) | size_lo);
                    }
                }

                p = (data_end + 3) & !3;
            }
            FDT_END => break,
            FDT_NOP => {}
            _ => return Err(()),
        }
    }

    match (mem_base, mem_size) {
        (Some(b), Some(s)) => Ok(Some(MemoryMap { base: b, size: s })),
        _ => Ok(None),
    }
}

/// Debug: dump node names and device_type/reg properties to the UART.
pub fn dump(fdt: &[u8]) {
    if fdt.len() < 40 {
        return;
    }
    let off_struct = be32(fdt, 8) as usize;
    let off_strings = be32(fdt, 12) as usize;
    if off_struct >= fdt.len() || off_strings >= fdt.len() {
        return;
    }
    let strings = |nameoff: usize| -> Option<&[u8]> {
        let start = off_strings + nameoff;
        if start >= fdt.len() {
            return None;
        }
        let mut end = start;
        while end < fdt.len() && fdt[end] != 0 {
            end += 1;
        }
        Some(&fdt[start..end])
    };
    let mut p = off_struct;
    let mut depth = 0usize;
    loop {
        if p + 4 > fdt.len() {
            return;
        }
        let token = be32(fdt, p);
        p += 4;
        match token {
            FDT_BEGIN_NODE => {
                let start = p;
                while p < fdt.len() && fdt[p] != 0 {
                    p += 1;
                }
                let name = core::str::from_utf8(&fdt[start..p]).unwrap_or("?");
                for _ in 0..depth {
                    crate::uart::write_str("  ");
                }
                crate::uart::write_str("[node] ");
                crate::uart::write_str(name);
                crate::uart::write_str("\n");
                depth += 1;
                p += 1;
                p = (p + 3) & !3;
            }
            FDT_END_NODE => {
                depth = depth.saturating_sub(1);
            }
            FDT_PROP => {
                if p + 8 > fdt.len() {
                    return;
                }
                let len = be32(fdt, p) as usize;
                let nameoff = be32(fdt, p + 4) as usize;
                p += 8;
                let name = strings(nameoff).unwrap_or(b"?");
                let data_end = p + len;
                if data_end > fdt.len() {
                    return;
                }
                let nstr = core::str::from_utf8(name).unwrap_or("?");
                if nstr == "device_type" || nstr == "reg" {
                    for _ in 0..depth {
                        crate::uart::write_str("  ");
                    }
                    crate::uart::write_str("[prop] ");
                    crate::uart::write_str(nstr);
                    crate::uart::write_str(" len=");
                    crate::uart::write_bytes(&crate::dec(len));
                    crate::uart::write_str("\n");
                }
                p = (data_end + 3) & !3;
            }
            FDT_END => return,
            FDT_NOP => {}
            _ => return,
        }
    }
}
