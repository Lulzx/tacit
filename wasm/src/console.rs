//! Persistent Tacit store session for the browser console.

use std::cell::RefCell;
use std::collections::BTreeMap;

use compile::{Compiler, ConstValue};
use store::session::{Outcome, Session};
use uir::{DTYPE_F32, DTYPE_I64, DTYPE_U8};

use crate::stepper;

thread_local! {
    static SESS: RefCell<Session> = RefCell::new(fresh());
    static CONSTS: RefCell<BTreeMap<String, ConstValue>> = const { RefCell::new(BTreeMap::new()) };
}

fn fresh() -> Session {
    let mut s = Session::new();
    let _ = s.seed_system();
    s
}

pub fn reset() {
    SESS.with(|s| *s.borrow_mut() = fresh());
    CONSTS.with(|c| c.borrow_mut().clear());
}

pub fn pwd() -> String {
    SESS.with(|s| s.borrow().pwd())
}

pub fn graph() -> String {
    SESS.with(|s| s.borrow().graph_text())
}

pub fn eval(src: &str) -> Result<String, String> {
    let src = src.trim();
    if src.is_empty() || src.starts_with('#') {
        return Ok(String::new());
    }
    let store_out = SESS.with(|s| s.borrow_mut().eval(src));
    match store_out {
        Ok(Outcome::Handled(t)) => Ok(t),
        Ok(Outcome::Unknown) => eval_uiua(src),
        Err(e) => Err(e),
    }
}

fn eval_uiua(src: &str) -> Result<String, String> {
    let is_binding = binding_name(src).is_some();
    let bytes = CONSTS.with(|c| {
        let mut comp = Compiler::with_consts(&c.borrow());
        comp.compile_line(src)?;
        Ok::<_, String>(comp.bytes())
    })?;
    let prog = uir::decode(&bytes).map_err(|_| "bad program".to_string())?;
    let v = stepper::run(&prog)?;
    if is_binding {
        if let Some(name) = binding_name(src) {
            CONSTS.with(|c| {
                c.borrow_mut().insert(name, snapshot(&v));
            });
        }
        return Ok(String::new());
    }
    Ok(fmt_value(&v))
}

fn snapshot(v: &stepper::Value) -> ConstValue {
    let mut shape = [1u32; 4];
    for i in 0..v.rank as usize {
        shape[i] = v.shape[i] as u32;
    }
    ConstValue { dtype: v.dtype, rank: v.rank, shape, payload: v.data.clone() }
}

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

fn fmt_value(v: &stepper::Value) -> String {
    if v.dtype == DTYPE_U8 && v.rank == 1 {
        return String::from_utf8_lossy(&v.data).into_owned();
    }
    let n = v.elems();
    let mut out = String::from("[");
    for i in 0..n {
        if i > 0 {
            out.push(' ');
        }
        match v.dtype {
            DTYPE_I64 => {
                let mut b = [0u8; 8];
                b.copy_from_slice(&v.data[i * 8..i * 8 + 8]);
                out.push_str(&i64::from_le_bytes(b).to_string());
            }
            DTYPE_F32 => {
                let mut b = [0u8; 4];
                b.copy_from_slice(&v.data[i * 4..i * 4 + 4]);
                out.push_str(&format!("{}", f32::from_le_bytes(b)));
            }
            _ => out.push_str(&v.data.get(i).copied().unwrap_or(0).to_string()),
        }
    }
    out.push(']');
    out
}
