//! Tacit host compiler driver: reads a Uiua source file, lowers it to UIR
//! with the shared `compile` crate, optionally applies the host fusion pass,
//! and writes the image payload (or dumps the graph).

use compile::compile_file;

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
                uir_bytes = compile::fuse(&uir_bytes);
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
