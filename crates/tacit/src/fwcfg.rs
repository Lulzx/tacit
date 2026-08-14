//! fw_cfg (QEMU firmware config) access.  Used only to configure the ramfb
//! display device.  QEMU removed MMIO/PIO *writes* to fw_cfg long ago, so the
//! config is written through the DMA write channel.

pub const FW_CFG_BASE: usize = 0x0902_0000;
const DATA: usize = FW_CFG_BASE + 0x00;
const SELECTOR: usize = FW_CFG_BASE + 0x08;
const DMA: usize = FW_CFG_BASE + 0x10;

const FW_CFG_FILE_DIR: u16 = 0x0019;

const DMA_CTL_WRITE: u32 = 0x10;
const DMA_CTL_SELECT: u32 = 0x08;

unsafe fn rd_data() -> u8 {
    core::ptr::read_volatile(DATA as *const u8)
}

unsafe fn wr_selector(sel: u16) {
    core::ptr::write_volatile(SELECTOR as *mut u16, sel.to_be());
}

unsafe fn wr_dma(addr: u64) {
    core::ptr::write_volatile(DMA as *mut u64, addr.to_be());
}

/// Select an fw_cfg item.
pub fn select(sel: u16) {
    unsafe { wr_selector(sel) }
}

/// Read the next byte of the selected item (big-endian byte stream).
pub fn read_byte() -> u8 {
    unsafe { rd_data() }
}

fn read_u32() -> u32 {
    let a = read_byte() as u32;
    let b = read_byte() as u32;
    let c = read_byte() as u32;
    let d = read_byte() as u32;
    (a << 24) | (b << 16) | (c << 8) | d
}

fn read_u16() -> u16 {
    let a = read_byte() as u16;
    let b = read_byte() as u16;
    (a << 8) | b
}

/// Find the selector for the named file (e.g. "etc/ramfb"), or None.
///
/// The fw_cfg file directory is: u32 count, then per entry
/// { u32 size, u16 select, u16 reserved, char name[56] } (name NUL-terminated).
pub fn find_file(name: &[u8]) -> Option<u16> {
    select(FW_CFG_FILE_DIR);
    let count = read_u32();
    if count > 4096 {
        return None;
    }
    for _ in 0..count {
        let _size = read_u32();
        let sel = read_u16();
        let _reserved = read_u16();
        let mut buf = [0u8; 56];
        for i in 0..56 {
            buf[i] = read_byte();
        }
        let mut nlen = 0usize;
        while nlen < 56 && buf[nlen] != 0 {
            nlen += 1;
        }
        if nlen == name.len() && &buf[..nlen] == name {
            return Some(sel);
        }
    }
    None
}

/// Write `data` (already big-endian) to the named file via the DMA channel.
/// Returns true on success.
pub fn write_file(sel: u16, data: &[u8]) -> bool {
    // Descriptor: control BE32, length BE32, address BE64
    #[repr(C, align(8))]
    struct DmaAccess {
        control: u32,
        length: u32,
        address: u64,
    }

    // Statics live in RAM (identity-mapped, cache-off), readable by QEMU DMA.
    static mut DESC: DmaAccess = DmaAccess { control: 0, length: 0, address: 0 };
    static mut PAYLOAD: [u8; 64] = [0; 64];

    if data.len() > 64 {
        return false;
    }
    unsafe {
        for (i, b) in data.iter().enumerate() {
            PAYLOAD[i] = *b;
        }
        DESC.control = (DMA_CTL_WRITE | DMA_CTL_SELECT | ((sel as u32) << 16)).to_be();
        DESC.length = (data.len() as u32).to_be();
        DESC.address = (&PAYLOAD[0] as *const u8 as u64).to_be();

        // QEMU reads the descriptor and payload from RAM via DMA; clean the
        // data cache so it sees the writes.
        crate::mmu::flush_dcache(&DESC as *const DmaAccess as usize, core::mem::size_of::<DmaAccess>());
        crate::mmu::flush_dcache(&PAYLOAD[0] as *const u8 as usize, data.len());

        wr_dma(&DESC as *const DmaAccess as u64);
    }
    true
}

// ---------------------------------------------------------------------------
// ramfb
// ---------------------------------------------------------------------------

pub struct FramebufferInfo {
    pub addr: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

const RAMFB_NAME: &[u8] = b"etc/ramfb";
const FOURCC_XRGB8888: u32 = 0x3432_5258;

/// Configure the ramfb device to display the framebuffer at `addr`.
pub fn configure_ramfb(addr: u64, width: u32, height: u32) -> Option<FramebufferInfo> {
    let sel = find_file(RAMFB_NAME)?;
    let stride = width * 4;
    let mut cfg = [0u8; 28];
    cfg[0..8].copy_from_slice(&addr.to_be_bytes());
    cfg[8..12].copy_from_slice(&FOURCC_XRGB8888.to_be_bytes());
    cfg[12..16].copy_from_slice(&0u32.to_be_bytes()); // flags
    cfg[16..20].copy_from_slice(&width.to_be_bytes());
    cfg[20..24].copy_from_slice(&height.to_be_bytes());
    cfg[24..28].copy_from_slice(&stride.to_be_bytes());
    if !write_file(sel, &cfg) {
        return None;
    }
    Some(FramebufferInfo { addr, width, height, stride })
}
