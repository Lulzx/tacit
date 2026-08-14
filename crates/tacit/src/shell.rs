//! Tacit shell: a bash-shaped surface over the content-addressed store,
//! with the in-guest Uiua compiler as a fallback.
//!
//! Store commands (`pwd`, `ls`, `cd`, `cat`, `echo`, …) are syntactic sugar
//! for transforms over trees and blobs.  They do not invoke Unix programs.
//! A line that is not a store command is compiled with the shared `compile`
//! crate and stepped — bindings persist as session constants.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;

use compile::{Compiler, ConstValue};
use store::session::{Outcome, Session};

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

fn write_line(s: &str) {
    crate::console_write_str(s);
    crate::console_write_str("\n");
}

fn eval_store(sess: &mut Session, src: &str) -> bool {
    match sess.eval(src) {
        Ok(Outcome::Handled(out)) => {
            if out == "\x0c" {
                crate::console_clear();
            } else if !out.is_empty() {
                write_line(&out);
            }
            true
        }
        Ok(Outcome::Unknown) => false,
        Err(e) => {
            crate::console_write_str("store: ");
            write_line(&e);
            true
        }
    }
}

fn eval_uiua(consts: &mut BTreeMap<String, ConstValue>, src: &str) {
    let is_binding = binding_name(src).is_some();

    let mut c = Compiler::with_consts(consts);
    if let Err(e) = c.compile_line(src) {
        crate::console_write_str("compile error: ");
        crate::console_write_str(&e);
        crate::console_write_str("\n");
        return;
    }
    let bytes = c.bytes();
    let prog = match uir::decode(&bytes) {
        Ok(p) => p,
        Err(_) => {
            crate::console_write_str("error: bad program\n");
            return;
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
        return;
    }

    let last = g.last;
    let result = last.and_then(|i| g.vals[i].clone());
    let last_is_display = match last {
        Some(i) => prog.nodes[i].op == uir::OP_DISPLAY,
        None => false,
    };
    if let Some(v) = &result {
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

fn handle_line(sess: &mut Session, consts: &mut BTreeMap<String, ConstValue>, src: &str) {
    if eval_store(sess, src) {
        return;
    }
    eval_uiua(consts, src);
}

/// Scripted first-milestone session.  Prints as if typed, checks the undo.
pub fn demo() {
    crate::console_write_str("store: content-addressed namespace (root tree)\n");
    crate::console_write_str("store: data is immutable; names are refs\n");
    crate::console_write_str("\n--- store + shell (scripted) ---\n");

    let mut sess = Session::new();
    if let Err(e) = sess.seed_system() {
        crate::console_write_str("store seed failed: ");
        write_line(e.as_str());
        return;
    }

    let script: &[&str] = &[
        "pwd",
        "mkdir home",
        "cd home",
        "echo \"hello tacit\" > hello.txt",
        "ls",
        "cat hello.txt",
        "history hello.txt",
        "echo \"hello world\" > hello.txt",
        "history hello.txt",
        "undo hello.txt",
        "cat hello.txt",
        "echo \"1 2 3 4\" > numbers",
        "cat numbers | parse | square | sum",
        "graph",
    ];

    let mut ok = true;
    for line in script {
        crate::console_write_str("tacit> ");
        write_line(line);
        match sess.eval(line) {
            Ok(Outcome::Handled(out)) => {
                if !out.is_empty() {
                    write_line(&out);
                }
            }
            Ok(Outcome::Unknown) => {
                write_line("store: unknown command");
                ok = false;
            }
            Err(e) => {
                crate::console_write_str("store: ");
                write_line(&e);
                ok = false;
            }
        }
    }

    match sess.eval("cat hello.txt") {
        Ok(Outcome::Handled(out)) if out == "hello tacit" => {}
        _ => ok = false,
    }
    match sess.eval("cat numbers | parse | square | sum") {
        Ok(Outcome::Handled(out)) if out == "30" => {}
        _ => ok = false,
    }

    write_line(if ok {
        "store session: values + names + history + undo + pipeline (ok)"
    } else {
        "store session: FAILED"
    });
}

pub fn run() -> ! {
    let mut sess = Session::new();
    if let Err(e) = sess.seed_system() {
        crate::console_write_str("store seed failed: ");
        write_line(e.as_str());
    }
    let mut consts: BTreeMap<String, ConstValue> = BTreeMap::new();
    loop {
        crate::console_write_str("\ntacit> ");
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
        handle_line(&mut sess, &mut consts, src);
    }
}
