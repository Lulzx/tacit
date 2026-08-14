//! Identity-mapped page tables at the 16 KiB granule, with caches enabled.
//!
//! 48-bit VA, 4 translation levels (16 KiB granule): L0 (VA[47], 2 entries),
//! L1 (VA[46:36]), L2 (VA[35:25], 32 MiB blocks).  RAM is normal WB memory,
//! MMIO is device-nGnRnE, and the framebuffer (in RAM) is flushed explicitly.

pub const GRANULE: usize = 16 * 1024;
const BLOCK: u64 = 32 * 1024 * 1024; // level-2 block size at 16 KiB granule

#[repr(align(16384))]
struct Tables {
    l0: [u64; 2048],
    l1: [u64; 2048],
    l2: [u64; 2048],
}

static mut TABLES: Tables = Tables { l0: [0; 2048], l1: [0; 2048], l2: [0; 2048] };

fn table_desc(next_pa: usize) -> u64 {
    // Table descriptor: type 0b11, next table PA in bits [47:14].
    (next_pa as u64) | 0b11
}

fn block_desc(pa: u64, attr_idx: u64, shareable: bool) -> u64 {
    // Level-2 block (32 MiB): type 0b01, AttrIndx[4:2], AP[7:6]=00 (EL1 RW),
    // SH[9:8], AF[10].
    let mut d = pa; // pa is 32 MiB aligned, so bits [24:0] are zero
    d |= 0b01;
    d |= attr_idx << 2;
    d |= (if shareable { 3 } else { 0 }) << 8;
    d |= 1 << 10;
    d
}

/// Build identity-mapped page tables and enable the MMU + caches.
pub fn init(mem_base: u64, mem_size: u64) {
    unsafe {
        TABLES.l0[0] = table_desc(TABLES.l1.as_ptr() as usize);
        TABLES.l1[0] = table_desc(TABLES.l2.as_ptr() as usize);

        // Device region: 0x0800_0000 .. mem_base (UART, GIC, fw_cfg, ...)
        let mut a = 0x0800_0000u64;
        while a < mem_base {
            let idx = (a >> 25) as usize;
            TABLES.l2[idx] = block_desc(a, 0, false);
            a += BLOCK;
        }
        // RAM: mem_base .. mem_base + mem_size (normal WB, inner shareable)
        let mut a = mem_base & !(BLOCK - 1);
        let end = (mem_base + mem_size + BLOCK - 1) & !(BLOCK - 1);
        while a < end {
            let idx = (a >> 25) as usize;
            TABLES.l2[idx] = block_desc(a, 1, true);
            a += BLOCK;
        }

        let ttbr0 = TABLES.l0.as_ptr() as u64;
        let mair: u64 = 0x0000_0000_0000_00FF; // attr0=device, attr1=WB-RA-WA
        let tcr: u64 = 16 | (1 << 8) | (1 << 10) | (3 << 12) | (2 << 14) | (5 << 32);
        let sctlr: u64 = (1 << 0) | (1 << 2) | (1 << 12); // M | C | I
        core::arch::asm!(
            "dsb sy",
            "msr ttbr0_el1, {0}",
            "msr mair_el1, {1}",
            "msr tcr_el1, {2}",
            "isb",
            "tlbi vmalle1",
            "dsb ish",
            "isb",
            "msr sctlr_el1, {3}",
            "isb",
            "ic iallu",
            "isb",
            in(reg) ttbr0,
            in(reg) mair,
            in(reg) tcr,
            in(reg) sctlr,
            options(nostack),
        );
    }
}

/// Clean the data cache for [addr, addr+len) so coherent RAM (read by QEMU's
/// ramfb / fw_cfg DMA) sees the writes.
pub fn flush_dcache(addr: usize, len: usize) {
    let line = crate::machine::CACHE_LINE;
    let start = addr & !(line - 1);
    let end = (addr + len + line - 1) & !(line - 1);
    let mut a = start;
    while a < end {
        unsafe {
            core::arch::asm!("dc civac, {0}", in(reg) a);
        }
        a += line;
    }
    unsafe {
        core::arch::asm!("dsb sy");
    }
}
