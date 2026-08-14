//! The machine description: the Apple M4 Pro inventory Tacit is built toward,
//! recorded even though QEMU virt only wires the boot CPU.

pub const PAGE_SIZE: usize = 16 * 1024;
pub const CACHE_LINE: usize = 128;

#[derive(Clone, Copy)]
pub struct EngineDesc {
    pub name: &'static str,
    pub online: bool,
    pub kind: u8,
}

/// Ordered list of engines, matching UIR's engine code order.
pub static ENGINES: [EngineDesc; 8] = [
    EngineDesc { name: "p-core", online: true, kind: uir::ENGINE_PCORE },
    EngineDesc { name: "e-core", online: false, kind: uir::ENGINE_ECORE },
    EngineDesc { name: "neon", online: false, kind: uir::ENGINE_NEON },
    EngineDesc { name: "sme", online: false, kind: uir::ENGINE_SME },
    EngineDesc { name: "gpu", online: false, kind: uir::ENGINE_GPU },
    EngineDesc { name: "ane", online: false, kind: uir::ENGINE_ANE },
    EngineDesc { name: "media", online: false, kind: uir::ENGINE_MEDIA },
    EngineDesc { name: "display", online: false, kind: uir::ENGINE_DISPLAY },
];

pub const HOME: &str = "uma";

/// The machine description rendered as text.
pub fn description_text() -> alloc::vec::Vec<u8> {
    let mut s = alloc::vec::Vec::new();
    crate::fmt::append_str(&mut s, "machine: Apple M4 Pro (reference)\n");
    crate::fmt::append_str(&mut s, "  home = uma\n");
    crate::fmt::append_str(&mut s, "  pages = 16 KiB, cache line = 128 B\n");
    crate::fmt::append_str(&mut s, "  engines:\n");
    for e in ENGINES.iter() {
        crate::fmt::append_str(&mut s, "    ");
        crate::fmt::append_str(&mut s, e.name);
        crate::fmt::append_str(&mut s, if e.online { "  [online]" } else { "  [offline]" });
        crate::fmt::append_str(&mut s, "\n");
    }
    s
}

pub fn print() {
    let s = description_text();
    crate::console_write_bytes(&s);
}
