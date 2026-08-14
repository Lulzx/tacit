//! The in-guest Uiua shell.
//!
//! Each typed line is compiled with the shared `compile` crate and stepped by
//! the UIR stepper on the boot CPU.  Bindings are *values*: a bound name is
//! snapshotted as a session constant and re-emitted by later lines, so a
//! shell session is `Name ← value` lines followed by expression lines.
//!
//! This is the beginning of the self-hosted compiler story: the guest compiles
//! its own subset, from the same source code the host compiler uses.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;

use compile::{Compiler, ConstValue};

/// Snapshot a stepped value into a session constant (materialized bytes).
fn snapshot(v: &crate::stepper::Value) -> ConstValue {
    let len = v.byte_len();
    let mut payload = alloc::vec![0u8; len];
    if len > 0 {
        unsafe {
            core::ptr::copy_nonoverlapping(v.data as *const u8, payload.as_mut_ptr(), len);
        }
    }
    let mut shape = [1u32; 4];
    for i in 0..v.rank as usize {
        shape[i] = v.shape[i] as u32;
    }
    ConstValue { dtype: v.dtype, rank: v.rank, shape, payload }
}

/// If `src` is a `Name ← value` binding, return the name.
fn binding_name(src: &str) -> Option<String> {
    let pos = src.find('←')?;
    let name = src[..pos].trim();
    if name.is_empty() {
        return None;
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(name.to_string())
}

pub fn run() -> ! {
    let mut consts: BTreeMap<String, ConstValue> = BTreeMap::new();
    loop {
        crate::console_write_str("\nuiua> ");
        let line = crate::devices::read_line();
        let src = match core::str::from_utf8(&line) {
            Ok(s) => s.trim(),
            Err(_) => {
                crate::console_write_str("error: non-utf8 line\n");
                continue;
            }
        };
        if src.is_empty() || src.starts_with('#') {
            continue;
        }
        let is_binding = binding_name(src).is_some();

        let mut c = Compiler::with_consts(&consts);
        if let Err(e) = c.compile_line(src) {
            crate::console_write_str("compile error: ");
            crate::console_write_str(&e);
            crate::console_write_str("\n");
            continue;
        }
        let bytes = c.bytes();
        let prog = match uir::decode(&bytes) {
            Ok(p) => p,
            Err(_) => {
                crate::console_write_str("error: bad program\n");
                continue;
            }
        };

        let mut g = crate::stepper::Graph::new(&prog);
        let opts = crate::stepper::RunOpts {
            realm: 0,
            live: None,
            policy: None,
            scheduler: None,
            interactive: false,
        };
        if let Err(e) = crate::stepper::run(&mut g, &opts) {
            crate::console_write_str("runtime error: ");
            crate::console_write_str(e);
            crate::console_write_str("\n");
            g.release_all();
            continue;
        }

        let last = g.last;
        let result = last.and_then(|i| g.vals[i].clone());
        let last_is_display = match last {
            Some(i) => prog.nodes[i].op == uir::OP_DISPLAY,
            None => false,
        };
        if let Some(v) = &result {
            // The REPL echoes the value unless the line already ended in an
            // explicit `&display` (a tee that printed it) or was a binding.
            if !last_is_display && !is_binding {
                let text = crate::stepper::predict_text(v);
                crate::console_write_bytes(&text);
                crate::console_write_str("\n");
            }
            if is_binding {
                if let Some(i) = last {
                    let name = prog.name(i).to_string();
                    consts.insert(name, snapshot(v));
                }
            }
        }
        g.release_all();
    }
}
