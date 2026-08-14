//! Tacit host compiler driver: reads a Uiua source file, lowers it to UIR
//! with the shared `compile` crate, optionally applies the host fusion pass,
//! and writes the image payload (or dumps the graph).

use compile::compile_file;
use uir::*;

/// Fusion pass: replace a single-consumer pure Add→Multiply chain with one
/// AddMul kernel so the intermediate T = A+B is never materialized.
fn fuse_pass(buf: &[u8]) -> Vec<u8> {
    let prog = match uir::decode(buf) {
        Ok(p) => p,
        Err(_) => return buf.to_vec(),
    };
    let n = prog.nodes.len();
    let mut dead = vec![false; n];
    let mut consumers = vec![0u32; n];
    for nd in &prog.nodes {
        for inp in [nd.in0, nd.in1, nd.in2] {
            if inp != NONE {
                consumers[inp as usize] += 1;
            }
        }
    }
    let mut nodes = prog.nodes.clone();
    for i in 0..n {
        let nd = nodes[i];
        if nd.op == OP_MUL && nd.pure {
            // The Add may be either input of Multiply (× D + B A vs + B A × D).
            for (add_slot, other_slot) in [(nd.in0, nd.in1), (nd.in1, nd.in0)] {
                if add_slot == NONE || other_slot == NONE {
                    continue;
                }
                let add_idx = add_slot as usize;
                let add = nodes[add_idx];
                if add.op == OP_ADD && add.pure && add.in2 == NONE && consumers[add_idx] == 1 {
                    nodes[i].op = OP_ADD_MUL;
                    nodes[i].in0 = add.in0;
                    nodes[i].in1 = add.in1;
                    nodes[i].in2 = other_slot;
                    dead[add_idx] = true;
                    break;
                }
            }
        }
    }

    let mut enc = Encoder::new();
    let mut remap = vec![NONE; n];
    for (i, nd) in nodes.iter().enumerate() {
        if !dead[i] {
            remap[i] = enc.count;
            let map = |x: u32| if x == NONE { NONE } else { remap[x as usize] };
            let mut d = *nd;
            d.in0 = map(d.in0);
            d.in1 = map(d.in1);
            d.in2 = map(d.in2);
            let name: &[u8] = if d.op == OP_ADD_MUL { b"AddMul" } else { &prog.names[i] };
            enc.node(&d, name, &prog.consts[i]);
        }
    }
    enc.finish()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut fuse = true;
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut dump = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--no-fuse" => fuse = false,
            "--fuse" => fuse = true,
            "--dump" => dump = true,
            "-o" => {
                i += 1;
                output = Some(args[i].clone());
            }
            a if a.starts_with("--") => {
                eprintln!("unknown flag {}", a);
                std::process::exit(2);
            }
            a => input = Some(a.to_string()),
        }
        i += 1;
    }
    let input = match input {
        Some(i) => i,
        None => {
            eprintln!("usage: hostc [--fuse|--no-fuse] -o OUT.uir IN.ua");
            std::process::exit(2);
        }
    };
    let src = std::fs::read_to_string(&input).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {}", input, e);
        std::process::exit(1);
    });
    match compile_file(&src) {
        Ok(mut uir_bytes) => {
            if fuse {
                uir_bytes = fuse_pass(&uir_bytes);
            }
            if dump {
                let prog = uir::decode(&uir_bytes).unwrap();
                for (i, nd) in prog.nodes.iter().enumerate() {
                    println!(
                        "#{} {} dtype={} rank={} shape={:?} pure={} in=({},{},{}) name={}",
                        i,
                        uir::op_name(nd.op),
                        nd.dtype,
                        nd.rank,
                        &nd.shape[..nd.rank as usize],
                        nd.pure,
                        nd.in0,
                        nd.in1,
                        nd.in2,
                        prog.name(i)
                    );
                }
            }
            if let Some(out) = output {
                std::fs::write(&out, &uir_bytes).unwrap_or_else(|e| {
                    eprintln!("cannot write {}: {}", out, e);
                    std::process::exit(1);
                });
            } else if !dump {
                std::io::Write::write_all(&mut std::io::stdout(), &uir_bytes).unwrap();
            }
        }
        Err(e) => {
            eprintln!("{}: error: {}", input, e);
            std::process::exit(1);
        }
    }
}
