//! Minimal number/text formatting into byte buffers (no `core::fmt`).

pub fn append_u64(out: &mut alloc::vec::Vec<u8>, mut v: u64) {
    if v == 0 {
        out.push(b'0');
        return;
    }
    let mut tmp = [0u8; 20];
    let mut i = 20;
    while v > 0 {
        i -= 1;
        tmp[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    out.extend_from_slice(&tmp[i..]);
}

pub fn append_i64(out: &mut alloc::vec::Vec<u8>, v: i64) {
    if v < 0 {
        out.push(b'-');
        append_u64(out, (v as i128).unsigned_abs() as u64);
    } else {
        append_u64(out, v as u64);
    }
}

pub fn append_f32(out: &mut alloc::vec::Vec<u8>, v: f32) {
    if v.is_nan() {
        out.extend_from_slice(b"nan");
        return;
    }
    if v == f32::INFINITY {
        out.extend_from_slice(b"inf");
        return;
    }
    if v == f32::NEG_INFINITY {
        out.extend_from_slice(b"-inf");
        return;
    }
    if v < 0.0 {
        out.push(b'-');
        append_f32(out, -v);
        return;
    }
    let whole = v as u64;
    append_u64(out, whole);
    out.push(b'.');
    let frac = (v - whole as f32) * 1000.0;
    let f = ((frac + 0.5) as u64) % 1000;
    if f < 10 {
        out.extend_from_slice(b"00");
    } else if f < 100 {
        out.push(b'0');
    }
    append_u64(out, f);
}

pub fn append_str(out: &mut alloc::vec::Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
}

/// Format an array of values as "[a b c ...]" into `out`.
pub fn append_array(out: &mut alloc::vec::Vec<u8>, dtype: u8, data: *const u8, n: usize) {
    out.push(b'[');
    for i in 0..n {
        if i > 0 {
            out.push(b' ');
        }
        unsafe {
            match dtype {
                uir::DTYPE_I64 => append_i64(out, *(data.add(i * 8) as *const i64)),
                uir::DTYPE_F32 => append_f32(out, *(data.add(i * 4) as *const f32)),
                _ => append_u64(out, *(data.add(i) as *const u8) as u64),
            }
        }
    }
    out.push(b']');
}
