// Native macOS counterpart to Tacit's three in-image benches.
// Mirrors the same logical work so the two sides can be compared:
//   1. fusion:  C = (A + B) * D  over 2048x2048 f32, fused vs unfused
//   2. matmul:  64x64 f32 matrix product
//   3. send:    copy of 8388608 f32 (Tacit's zero-copy share vs copy)
//
// Reports BOTH wall-clock time and a byte-traffic proxy (bytes read+written
// from the arrays), so the metric Tacit prints in-image has a native twin.
//
// Build:  rustc -O bench_native.rs -o bench_native
// Run:    ./bench_native

use std::time::Instant;

const N: usize = 2048; // fusion shape (2048x2048 f32)
const M: usize = 64;   // matmul shape (64x64 f32)
const SEND: usize = 8_388_608; // send/copy element count (f32)

fn main() {
    println!("== native macOS (Apple M4 Pro, rustc -O) ==");

    // ---- 1. fusion: C = (A + B) * D ----
    let a = vec![1.0f32; N * N];
    let b = vec![2.0f32; N * N];
    let d = vec![3.0f32; N * N];
    let mut c = vec![0.0f32; N * N];

    // unfused: C = A + B, then C = C * D  (two passes over memory)
    let t = Instant::now();
    for i in 0..N * N {
        c[i] = a[i] + b[i];
    }
    for i in 0..N * N {
        c[i] = c[i] * d[i];
    }
    let unfused = t.elapsed();
    // pass1 reads a,b writes c (2*16+16 MiB); pass2 reads c,d writes c (2*16+16 MiB)
    let unfused_bytes = 2 * (2 * N * N * 4 + N * N * 4);

    // fused: C = (A + B) * D in one pass (single AddMul kernel)
    let t = Instant::now();
    for i in 0..N * N {
        c[i] = (a[i] + b[i]) * d[i];
    }
    let fused = t.elapsed();
    // reads a,b,d (3*16 MiB) + writes c (16 MiB), once
    let fused_bytes = 3 * N * N * 4 + N * N * 4;

    println!("-- fusion 2048x2048 f32 --");
    println!("  unfused: {:>10.3} ms   byte-proxy {} bytes", ms(unfused), unfused_bytes);
    println!("  fused:   {:>10.3} ms   byte-proxy {} bytes", ms(fused), fused_bytes);
    println!("  fused < unfused: {}", fused < unfused);
    println!("  checksum C[0] = {}", c[0]); // (1+2)*3 = 9

    // ---- 2. matmul 64x64 ----
    let ma = vec![1.5f32; M * M];
    let mb = vec![2.0f32; M * M];
    let mut mc = vec![0.0f32; M * M];
    let t = Instant::now();
    for i in 0..M {
        for k in 0..M {
            let aik = ma[i * M + k];
            for j in 0..M {
                mc[i * M + j] += aik * mb[k * M + j];
            }
        }
    }
    let matmul = t.elapsed();
    println!("-- matmul 64x64 f32 --");
    println!("  {:>10.3} ms   C00 = {}", ms(matmul), mc[0]); // 64 * 1.5 * 2.0 = 192

    // ---- 3. send/copy 8388608 f32 ----
    let src = vec![1.0f32; SEND];
    let mut dst = vec![0.0f32; SEND];
    let t = Instant::now();
    dst.copy_from_slice(&src);
    let copy = t.elapsed();
    println!("-- copy 8388608 f32 (32 MiB) --");
    println!("  {:>10.3} ms   copied {} bytes", ms(copy), SEND * 4);
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1e3
}
