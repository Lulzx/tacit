//! Minimal WASM proof: expose the Uiua -> UIR compiler (the self-hosted
//! compiler's front end) so it can run in a browser / Node.
//!
//! This is the *language machine* — pure logic, no AArch64. The microkernel
//! machine layer (MMU, NEON/SME, PACGA) is not here; WASM is a second
//! machine layer for the same language machine.

use compile::compile_file;

mod stepper;

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
            let boxed = bytes.into_boxed_slice();
            let ptr = boxed.as_ptr() as *mut u8;
            unsafe { *out_len = boxed.len(); }
            std::mem::forget(boxed);
            ptr
        }
        Err(_) => {
            unsafe { *out_len = 0; }
            std::ptr::null_mut()
        }
    }
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
        Ok(v) => v,
        Err(_) => return std::ptr::null_mut(),
    };

    // header: dtype, rank, shape (4x u32), then data
    let mut out: Vec<u8> = Vec::with_capacity(1 + 1 + 16 + v.data.len());
    out.push(v.dtype);
    out.push(v.rank);
    for i in 0..4 {
        out.extend_from_slice(&(v.shape[i] as u32).to_le_bytes());
    }
    out.extend_from_slice(&v.data);

    let boxed = out.into_boxed_slice();
    let ptr = boxed.as_ptr() as *mut u8;
    unsafe { *out_len = boxed.len(); }
    std::mem::forget(boxed);
    ptr
}

/// Return the dataflow graph of a UIR program as a binary table for the
/// browser to draw: [u32 count, then per node: u32 id, u8 op, u32 name_len,
/// name bytes, u32 in0, u32 in1, u32 in2].
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
    for nd in prog.nodes.iter() {
        out.extend_from_slice(&nd.id.to_le_bytes());
        out.push(nd.op);
        let name = &prog.names[nd.id as usize];
        out.extend_from_slice(&(name.len() as u32).to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&nd.in0.to_le_bytes());
        out.extend_from_slice(&nd.in1.to_le_bytes());
        out.extend_from_slice(&nd.in2.to_le_bytes());
    }
    let boxed = out.into_boxed_slice();
    let ptr = boxed.as_ptr() as *mut u8;
    unsafe { *out_len = boxed.len(); }
    std::mem::forget(boxed);
    ptr
}
