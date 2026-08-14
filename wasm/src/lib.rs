//! Minimal WASM proof: expose the Uiua -> UIR compiler (the self-hosted
//! compiler's front end) so it can run in a browser / Node.
//!
//! This is the *language machine* — pure logic, no AArch64. The microkernel
//! machine layer (MMU, NEON/SME, PACGA) is not here; WASM is a second
//! machine layer for the same language machine.

use std::cell::RefCell;

use compile::{compile_file, fuse};

mod stepper;

thread_local! {
    static LAST_ERR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static SESSION: RefCell<Option<stepper::Session>> = const { RefCell::new(None) };
}

fn set_err(msg: &str) {
    LAST_ERR.with(|e| {
        *e.borrow_mut() = msg.as_bytes().to_vec();
    });
}

fn clear_err() {
    LAST_ERR.with(|e| e.borrow_mut().clear());
}

fn leak_vec(v: Vec<u8>, out_len: *mut usize) -> *mut u8 {
    if v.is_empty() {
        unsafe {
            *out_len = 0;
        }
        return std::ptr::null_mut();
    }
    let boxed = v.into_boxed_slice();
    unsafe {
        *out_len = boxed.len();
    }
    let ptr = boxed.as_ptr() as *mut u8;
    std::mem::forget(boxed);
    ptr
}

fn encode_value(v: &stepper::Value) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(1 + 1 + 16 + v.data.len());
    out.push(v.dtype);
    out.push(v.rank);
    for i in 0..4 {
        out.extend_from_slice(&(v.shape[i] as u32).to_le_bytes());
    }
    out.extend_from_slice(&v.data);
    out
}

/// Compile a Uiua source string to UIR bytes.
///
/// Returns a pointer to a heap-allocated buffer; `out_len` receives its
/// length. On error returns null and `out_len` is set to 0. The caller must
/// free the buffer with `uiua_free`.
#[no_mangle]
pub extern "C" fn uiua_compile(src_ptr: *const u8, src_len: usize, out_len: *mut usize) -> *mut u8 {
    if src_ptr.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let src = unsafe { std::slice::from_raw_parts(src_ptr, src_len) };
    let src = String::from_utf8_lossy(src);
    match compile_file(&src) {
        Ok(bytes) => {
            clear_err();
            leak_vec(bytes, out_len)
        }
        Err(e) => {
            set_err(&e);
            unsafe {
                *out_len = 0;
            }
            std::ptr::null_mut()
        }
    }
}

/// Last compile/run error as UTF-8. Empty if the last call succeeded.
#[no_mangle]
pub extern "C" fn uiua_last_error(out_len: *mut usize) -> *mut u8 {
    if out_len.is_null() {
        return std::ptr::null_mut();
    }
    LAST_ERR.with(|e| leak_vec(e.borrow().clone(), out_len))
}

/// Apply the Add→Multiply fusion pass to a UIR buffer.
#[no_mangle]
pub extern "C" fn uiua_fuse(uir_ptr: *const u8, uir_len: usize, out_len: *mut usize) -> *mut u8 {
    if uir_ptr.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let uir = unsafe { std::slice::from_raw_parts(uir_ptr, uir_len) };
    leak_vec(fuse(uir), out_len)
}

/// Free a buffer returned by `uiua_compile` or `uiua_run`.
#[no_mangle]
pub extern "C" fn uiua_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        unsafe {
            let _ = Vec::from_raw_parts(ptr, len, len);
        }
    }
}

/// Run a UIR program (as produced by `uiua_compile`) to completion.
///
/// Returns a heap buffer: [dtype u8, rank u8, shape 4x u32, then the value
/// bytes]. `out_len` receives the total length. Returns null on error.
#[no_mangle]
pub extern "C" fn uiua_run(uir_ptr: *const u8, uir_len: usize, out_len: *mut usize) -> *mut u8 {
    if uir_ptr.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let uir = unsafe { std::slice::from_raw_parts(uir_ptr, uir_len) };
    let prog = match uir::decode(uir) {
        Ok(p) => p,
        Err(_) => return std::ptr::null_mut(),
    };
    let v = match stepper::run(&prog) {
        Ok(v) => {
            clear_err();
            v
        }
        Err(e) => {
            set_err(&e);
            return std::ptr::null_mut();
        }
    };
    leak_vec(encode_value(&v), out_len)
}

/// Load a UIR program for stepping. Returns the node count, or -1 on error.
#[no_mangle]
pub extern "C" fn uiua_load(uir_ptr: *const u8, uir_len: usize) -> i32 {
    if uir_ptr.is_null() {
        set_err("null uir");
        return -1;
    }
    let uir = unsafe { std::slice::from_raw_parts(uir_ptr, uir_len) };
    let prog = match uir::decode(uir) {
        Ok(p) => p,
        Err(_) => {
            set_err("invalid UIR");
            return -1;
        }
    };
    let n = prog.nodes.len() as i32;
    SESSION.with(|s| {
        *s.borrow_mut() = Some(stepper::Session::load(prog));
    });
    clear_err();
    n
}

/// Fire the next node. Buffer: u8 status (0=done, 1=ok, 2=err), then
/// u32 id, u8 op, u8 dtype, u8 rank, u8 engine, 4×u32 shape, u32 bytes,
/// u32 name_len, name, u32 preview_len, preview.
#[no_mangle]
pub extern "C" fn uiua_step(out_len: *mut usize) -> *mut u8 {
    if out_len.is_null() {
        return std::ptr::null_mut();
    }
    SESSION.with(|s| {
        let mut slot = s.borrow_mut();
        let session = match slot.as_mut() {
            Some(sess) => sess,
            None => {
                set_err("no program loaded");
                return leak_vec(vec![2u8], out_len);
            }
        };
        match session.step() {
            Ok(None) => {
                clear_err();
                leak_vec(vec![0u8], out_len)
            }
            Ok(Some(info)) => {
                clear_err();
                let mut out = Vec::new();
                out.push(1u8);
                out.extend_from_slice(&info.id.to_le_bytes());
                out.push(info.op);
                out.push(info.dtype);
                out.push(info.rank);
                out.push(info.engine);
                for d in info.shape {
                    out.extend_from_slice(&d.to_le_bytes());
                }
                out.extend_from_slice(&info.bytes.to_le_bytes());
                out.extend_from_slice(&(info.name.len() as u32).to_le_bytes());
                out.extend_from_slice(&info.name);
                out.extend_from_slice(&(info.preview.len() as u32).to_le_bytes());
                out.extend_from_slice(&info.preview);
                leak_vec(out, out_len)
            }
            Err(e) => {
                set_err(&e);
                leak_vec(vec![2u8], out_len)
            }
        }
    })
}

/// Value of the last fired node in the loaded session (same layout as `uiua_run`).
#[no_mangle]
pub extern "C" fn uiua_result(out_len: *mut usize) -> *mut u8 {
    if out_len.is_null() {
        return std::ptr::null_mut();
    }
    SESSION.with(|s| {
        let slot = s.borrow();
        match slot.as_ref().and_then(|sess| sess.last_value()) {
            Some(v) => {
                clear_err();
                leak_vec(encode_value(v), out_len)
            }
            None => {
                set_err("no value produced");
                unsafe {
                    *out_len = 0;
                }
                std::ptr::null_mut()
            }
        }
    })
}

/// Return the dataflow graph of a UIR program as a binary table for the
/// browser to draw: [u32 count, u32 version=2, then per node: u32 id, u8 op,
/// u8 dtype, u8 rank, u8 engine, u8 pure, 4×u32 shape, u32 name_len, name,
/// u32 in0, u32 in1, u32 in2].
#[no_mangle]
pub extern "C" fn uiua_graph(uir_ptr: *const u8, uir_len: usize, out_len: *mut usize) -> *mut u8 {
    if uir_ptr.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let uir = unsafe { std::slice::from_raw_parts(uir_ptr, uir_len) };
    let prog = match uir::decode(uir) {
        Ok(p) => p,
        Err(_) => return std::ptr::null_mut(),
    };
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&(prog.nodes.len() as u32).to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    for nd in prog.nodes.iter() {
        out.extend_from_slice(&nd.id.to_le_bytes());
        out.push(nd.op);
        out.push(nd.dtype);
        out.push(nd.rank);
        out.push(nd.engine);
        out.push(if nd.pure { 1 } else { 0 });
        for d in nd.shape {
            out.extend_from_slice(&d.to_le_bytes());
        }
        let name = &prog.names[nd.id as usize];
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&nd.in0.to_le_bytes());
        out.extend_from_slice(&nd.in1.to_le_bytes());
        out.extend_from_slice(&nd.in2.to_le_bytes());
    }
    leak_vec(out, out_len)
}
