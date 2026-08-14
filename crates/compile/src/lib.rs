//! The shared Uiua-subset compiler, usable from the host (`hostc`) and from
//! the guest shell (`tacit`).  It lowers a first-milestone subset of **Uiua**
//! to UIR.
//!
//! Uiua is a tacit array language: functions appear to the left of their
//! arguments and code is read right-to-left (`+ 2 3` is 2+3, `× 2 + 3 5` is
//! 2*(3+5)).  The subset covers numeric scalars/arrays, character vectors,
//! bindings (`Name ← value`), elementwise arithmetic, reduce (`/+`), reshape
//! (`↯`, including scalar-fill), reverse (`⇌`), length (`⧻`), grade (`⍏`/`⍖`),
//! select (`⊏`), keep (`▽`), pick (`⊡`), equals (`=`, for masks), plus the OS
//! system functions (`&name`) for effects, graph/machine queries, counters,
//! and placement.  Pure elementwise nodes are placed on `engine = neon` (the
//! boot CPU's SIMD unit); everything else stays on `engine = p-core`.  Both
//! are `home = uma`.  Anything else is a compile error with a source location.
//!
//! The guest shell keeps a session table of named *values*: after a line
//! runs, its result can be stored via [`Compiler::set_const`] and referenced
//! by later lines, where it is re-emitted as a fresh constant node.

#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use uir::*;

#[derive(Clone, Copy)]
struct SValue {
    node: u32,
    dtype: u8,
    rank: u8,
    shape: [u32; 4],
}

/// A materialized value that outlives one compiled program: the shell stores
/// a line's result here so a later line can re-emit it as a constant.
#[derive(Clone)]
pub struct ConstValue {
    pub dtype: u8,
    pub rank: u8,
    pub shape: [u32; 4],
    pub payload: Vec<u8>,
}

pub struct Compiler {
    enc: Encoder,
    stack: Vec<SValue>,
    bindings: BTreeMap<String, SValue>,
    consts: BTreeMap<String, ConstValue>,
    line: usize,
    next_name: Option<String>,
    parallel_axis: u8,
    engine: u8,
}

fn shape_elems(rank: u8, shape: &[u32; 4]) -> u64 {
    let mut n = 1u64;
    for i in 0..rank as usize {
        n *= shape[i] as u64;
    }
    n
}

impl Compiler {
    pub fn new() -> Self {
        Compiler {
            enc: Encoder::new(),
            stack: Vec::new(),
            bindings: BTreeMap::new(),
            consts: BTreeMap::new(),
            line: 1,
            next_name: None,
            parallel_axis: 0,
            engine: ENGINE_PCORE,
        }
    }

    /// A compiler seeded with session constants (the shell's binding table).
    pub fn with_consts(consts: &BTreeMap<String, ConstValue>) -> Self {
        let mut c = Compiler::new();
        c.consts = consts.clone();
        c
    }

    /// Store a session constant under `name`; later lines re-emit it.
    pub fn set_const(&mut self, name: String, v: ConstValue) {
        self.consts.insert(name, v);
    }

    /// Snapshot the program compiled so far (per-line for the shell).
    pub fn bytes(&self) -> Vec<u8> {
        self.enc.snapshot()
    }

    pub fn finish(self) -> Vec<u8> {
        self.enc.finish()
    }

    /// Compile `src` (one or more newline-separated statements) into the
    /// current program.
    pub fn compile_line(&mut self, src: &str) -> Result<(), String> {
        let toks = tokenize(src)?;
        let mut cur: Vec<(usize, Tok)> = Vec::new();
        for (ln, t) in toks {
            if t == Tok::Newline {
                if !cur.is_empty() {
                    self.compile_statement(&cur)?;
                    cur.clear();
                }
            } else {
                cur.push((ln, t));
            }
        }
        if !cur.is_empty() {
            self.compile_statement(&cur)?;
        }
        Ok(())
    }

    fn compile_statement(&mut self, stmt: &[(usize, Tok)]) -> Result<(), String> {
        self.line = stmt[0].0;
        let is_binding = stmt.len() >= 2 && matches!(stmt[0].1, Tok::Ident(_)) && stmt[1].1 == Tok::Arrow;
        if is_binding {
            let name = match &stmt[0].1 {
                Tok::Ident(n) => n.clone(),
                _ => unreachable!(),
            };
            self.next_name = Some(name.clone());
            let words = desugar(&stmt[2..])?;
            self.eval_words(&words)?;
            let v = self.stack.pop().ok_or_else(|| self.error("binding has no value"))?;
            self.bindings.insert(name, v);
        } else {
            let words = desugar(&stmt)?;
            self.eval_words(&words)?;
        }
        Ok(())
    }

    fn emit(
        &mut self,
        op: u8,
        dtype: u8,
        rank: u8,
        shape: &[u32; 4],
        pure: bool,
        cap_need: u8,
        in0: u32,
        in1: u32,
        in2: u32,
        name: &str,
        const_payload: &[u8],
    ) -> u32 {
        let effective = self.next_name.take().unwrap_or_else(|| name.to_string());
        let par = self.parallel_axis;
        let eng = self.engine;
        self.parallel_axis = 0;
        self.engine = ENGINE_PCORE;
        let d = NodeDesc {
            id: self.enc.count,
            op,
            dtype,
            rank,
            shape: *shape,
            pure,
            parallel_axis: par,
            engine: eng,
            home: HOME_UMA,
            cap_need,
            in0,
            in1,
            in2,
            name_len: effective.len() as u32,
            const_len: const_payload.len() as u32,
        };
        self.enc.node(&d, effective.as_bytes(), const_payload)
    }

    fn error(&self, msg: &str) -> String {
        format!("line {}: {}", self.line, msg)
    }

    fn pop(&mut self) -> Result<SValue, String> {
        self.stack.pop().ok_or_else(|| self.error("stack underflow"))
    }

    fn push_const(&mut self, dtype: u8, rank: u8, shape: &[u32; 4], payload: &[u8]) {
        let n = self.emit(OP_CONST, dtype, rank, shape, true, CAP_NONE, NONE, NONE, NONE, "const", payload);
        self.stack.push(SValue { node: n, dtype, rank, shape: *shape });
    }

    fn binop(&mut self, op: u8, name: &str) -> Result<(), String> {
        // Right-to-left: the top of stack is the left argument.
        let a = self.pop()?;
        let b = self.pop()?;
        let (dtype, rank, shape) = if a.rank == 0 {
            (b.dtype, b.rank, b.shape)
        } else if b.rank == 0 {
            (a.dtype, a.rank, a.shape)
        } else {
            if a.rank != b.rank || a.shape != b.shape {
                return Err(self.error("shape mismatch"));
            }
            (a.dtype, a.rank, a.shape)
        };
        self.parallel_axis = if rank > 0 { 1 } else { 0 };
        // Placement rule 1: pure elementwise work goes to the NEON engine
        // (the boot CPU's SIMD unit); the machine description marks it online.
        self.engine = ENGINE_NEON;
        let n = self.emit(op, dtype, rank, &shape, true, CAP_NONE, a.node, b.node, NONE, name, &[]);
        self.stack.push(SValue { node: n, dtype, rank, shape });
        Ok(())
    }

    fn unop(&mut self, op: u8, name: &str, pure: bool, cap: u8) -> Result<(), String> {
        let a = self.pop()?;
        let n = self.emit(op, a.dtype, a.rank, &a.shape, pure, cap, a.node, NONE, NONE, name, &[]);
        self.stack.push(SValue { node: n, dtype: a.dtype, rank: a.rank, shape: a.shape });
        Ok(())
    }

    fn source(&mut self, op: u8, name: &str) {
        let n = self.emit(op, DTYPE_I64, 2, &[0, 7, 1, 1], true, CAP_NONE, NONE, NONE, NONE, name, &[]);
        self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: [0, 7, 1, 1] });
    }
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Int(i64),
    Float(f64),
    Str(Vec<u8>),
    List { dtype: u8, rank: u8, shape: [u32; 4], payload: Vec<u8> },
    Ident(String),
    Sys(String),
    Add,
    Sub,
    Mul,
    Div,
    Sum,
    Reverse,
    Length,
    Reshape { rank: u8, dims: [u32; 4] },
    Fill { dtype: u8, rank: u8, shape: [u32; 4], value: f64 },
    GradeUp,
    GradeDown,
    Select,
    Keep,
    Pick,
    Eq,
    Fmt { template: Vec<u8> },
    Provenance(u32),
    Arrow,
    Newline,
}

fn tokenize(src: &str) -> Result<Vec<(usize, Tok)>, String> {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut toks = Vec::new();
    let mut ci = 0usize;
    let mut line = 1;
    while ci < chars.len() {
        let (bi, c) = chars[ci];
        if c == '\n' {
            toks.push((line, Tok::Newline));
            line += 1;
            ci += 1;
            continue;
        }
        if c == '#' {
            ci += 1;
            while ci < chars.len() && chars[ci].1 != '\n' {
                ci += 1;
            }
            continue;
        }
        if c.is_whitespace() {
            ci += 1;
            continue;
        }
        if c == '[' {
            let (list, next) = parse_list(src, &chars, ci)?;
            ci = next;
            toks.push((line, list));
            continue;
        }
        if c == ']' {
            return Err(format!("line {}: unexpected ']'", line));
        }
        if c == '←' {
            toks.push((line, Tok::Arrow));
            ci += 1;
            continue;
        }
        if c == '"' {
            let start_line = line;
            ci += 1;
            let mut s = Vec::new();
            while ci < chars.len() && chars[ci].1 != '"' {
                s.push(chars[ci].1 as u8);
                ci += 1;
            }
            if ci >= chars.len() {
                return Err(format!("line {}: unterminated string", start_line));
            }
            ci += 1;
            toks.push((start_line, Tok::Str(s)));
            continue;
        }
        if c == '&' {
            let start_line = line;
            ci += 1;
            if ci >= chars.len() {
                return Err(format!("line {}: '&' needs a name", start_line));
            }
            let s0 = chars[ci].0;
            while ci < chars.len() && (chars[ci].1.is_ascii_alphanumeric() || chars[ci].1 == '-') {
                ci += 1;
            }
            let end = chars.get(ci).map(|(b, _)| *b).unwrap_or(src.len());
            let name = src[s0..end].to_string();
            toks.push((start_line, Tok::Sys(name)));
            continue;
        }
        if c == '¯' {
            let start_line = line;
            ci += 1;
            let (t, next) = read_number(src, &chars, ci)?;
            ci = next;
            match t {
                Tok::Int(v) => toks.push((start_line, Tok::Int(-v))),
                Tok::Float(v) => toks.push((start_line, Tok::Float(-v))),
                _ => return Err(format!("line {}: '¯' must precede a number", line)),
            }
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && ci + 1 < chars.len() && chars[ci + 1].1.is_ascii_digit()) {
            let start_line = line;
            let (t, next) = read_number(src, &chars, ci)?;
            ci = next;
            toks.push((start_line, t));
            continue;
        }
        if c == '+' {
            toks.push((line, Tok::Add));
            ci += 1;
            continue;
        }
        if c == '-' {
            toks.push((line, Tok::Sub));
            ci += 1;
            continue;
        }
        if c == '×' || c == '*' {
            toks.push((line, Tok::Mul));
            ci += 1;
            continue;
        }
        if c == '÷' || c == '%' {
            toks.push((line, Tok::Div));
            ci += 1;
            continue;
        }
        if c == '/' {
            // `/` is Uiua's reduce modifier; the subset supports `/+` (sum).
            if ci + 1 < chars.len() && chars[ci + 1].1 == '+' {
                toks.push((line, Tok::Sum));
                ci += 2;
            } else {
                return Err(format!("line {}: reduce '/' with a non-'+' function is out of the first-milestone subset", line));
            }
            continue;
        }
        if c == '⇌' {
            toks.push((line, Tok::Reverse));
            ci += 1;
            continue;
        }
        if c == '⧻' {
            toks.push((line, Tok::Length));
            ci += 1;
            continue;
        }
        if c == '⍏' {
            toks.push((line, Tok::GradeUp));
            ci += 1;
            continue;
        }
        if c == '⍖' {
            toks.push((line, Tok::GradeDown));
            ci += 1;
            continue;
        }
        if c == '⊏' {
            toks.push((line, Tok::Select));
            ci += 1;
            continue;
        }
        if c == '▽' {
            toks.push((line, Tok::Keep));
            ci += 1;
            continue;
        }
        if c == '⊡' {
            toks.push((line, Tok::Pick));
            ci += 1;
            continue;
        }
        if c == '=' {
            toks.push((line, Tok::Eq));
            ci += 1;
            continue;
        }
        if c == '↯' {
            toks.push((line, Tok::Reshape { rank: 0, dims: [1, 1, 1, 1] }));
            ci += 1;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start_line = line;
            let s0 = bi;
            while ci < chars.len() && (chars[ci].1.is_ascii_alphanumeric() || chars[ci].1 == '_') {
                ci += 1;
            }
            let end = chars.get(ci).map(|(b, _)| *b).unwrap_or(src.len());
            let name = &src[s0..end];
            let tok = match name {
                "add" => Tok::Add,
                "sub" | "subtract" => Tok::Sub,
                "mul" | "multiply" => Tok::Mul,
                "div" | "divide" => Tok::Div,
                "sum" => Tok::Sum,
                "reverse" => Tok::Reverse,
                "length" => Tok::Length,
                "reshape" => Tok::Reshape { rank: 0, dims: [1, 1, 1, 1] },
                "up" => Tok::GradeUp,
                "down" => Tok::GradeDown,
                "sel" => Tok::Select,
                "keep" => Tok::Keep,
                "pick" => Tok::Pick,
                "eq" => Tok::Eq,
                "open" | "read" | "write" | "close" | "seek" | "fork" | "exec" | "thread"
                | "spawn" | "pthread" | "socket" | "listen" | "accept" | "metal" | "cuda"
                | "coreml" | "accelerate" | "file" | "ioctl" => {
                    return Err(format!("line {}: '{}' is out of the first-milestone subset", start_line, name));
                }
                _ => Tok::Ident(name.to_string()),
            };
            toks.push((start_line, tok));
            continue;
        }
        return Err(format!("line {}: unexpected character '{}'", line, c));
    }
    Ok(toks)
}

fn read_number(src: &str, chars: &[(usize, char)], ci: usize) -> Result<(Tok, usize), String> {
    let s0 = chars[ci].0;
    let mut i = ci;
    let mut is_float = false;
    while i < chars.len() && (chars[i].1.is_ascii_digit() || chars[i].1 == '.') {
        if chars[i].1 == '.' {
            is_float = true;
        }
        i += 1;
    }
    // The number may run to the very end of `src` (a shell line has no
    // trailing newline), so fall back to the source length.
    let end = chars.get(i).map(|(b, _)| *b).unwrap_or(src.len());
    let text = &src[s0..end];
    if is_float {
        Ok((Tok::Float(text.parse().map_err(|_| format!("bad float '{}'", text))?), i))
    } else {
        Ok((Tok::Int(text.parse().map_err(|_| format!("bad int '{}'", text))?), i))
    }
}

fn parse_list(src: &str, chars: &[(usize, char)], open: usize) -> Result<(Tok, usize), String> {
    // chars[open] == '['
    let mut i = open + 1;
    let mut elems: Vec<(u8, f64, i64)> = Vec::new();
    let mut dtype = DTYPE_I64;
    let mut nested = false;
    let mut rows = 0usize;
    let mut depth = 1usize;
    loop {
        if i >= chars.len() {
            return Err("unterminated list".to_string());
        }
        let c = chars[i].1;
        if c == ']' {
            depth -= 1;
            i += 1;
            if depth == 0 {
                break;
            }
            continue;
        }
        if c == '[' {
            nested = true;
            rows += 1;
            depth += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() || c == ',' {
            i += 1;
            continue;
        }
        if c == '¯' {
            i += 1;
            let (t, n) = read_number(src, chars, i)?;
            i = n;
            match t {
                Tok::Float(v) => {
                    dtype = DTYPE_F32;
                    elems.push((DTYPE_F32, -v, -(v as i64)));
                }
                Tok::Int(v) => elems.push((DTYPE_I64, -(v as f64), -v)),
                _ => unreachable!(),
            }
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].1.is_ascii_digit()) {
            let (t, n) = read_number(src, chars, i)?;
            i = n;
            match t {
                Tok::Float(v) => {
                    dtype = DTYPE_F32;
                    elems.push((DTYPE_F32, v, v as i64));
                }
                Tok::Int(v) => elems.push((DTYPE_I64, v as f64, v)),
                _ => unreachable!(),
            }
            continue;
        }
        return Err(format!("unexpected '{}' in list", c));
    }
    if nested {
        let cols = if rows > 0 { elems.len() / rows } else { 0 };
        let shape = [rows as u32, cols as u32, 1, 1];
        let payload = list_payload(dtype, &elems);
        Ok((Tok::List { dtype, rank: 2, shape, payload }, i))
    } else {
        let shape = [elems.len() as u32, 1, 1, 1];
        let payload = list_payload(dtype, &elems);
        Ok((Tok::List { dtype, rank: 1, shape, payload }, i))
    }
}

fn list_payload(dtype: u8, elems: &[(u8, f64, i64)]) -> Vec<u8> {
    if dtype == DTYPE_F32 {
        let mut w = VecWriter::new();
        for e in elems {
            w.u32((e.1 as f32).to_bits());
        }
        w.buf
    } else {
        let mut w = VecWriter::new();
        for e in elems {
            w.u64(e.2 as u64);
        }
        w.buf
    }
}

/// Extract the dims from a shape list's i64 payload.
fn dims_from_payload(payload: &[u8]) -> Vec<u32> {
    let mut dims = Vec::new();
    for i in (0..payload.len()).step_by(8) {
        if i + 8 <= payload.len() {
            let v = i64::from_le_bytes([payload[i], payload[i + 1], payload[i + 2], payload[i + 3], payload[i + 4], payload[i + 5], payload[i + 6], payload[i + 7]]);
            dims.push(v as u32);
        }
    }
    dims
}

/// Rewrite special forms (`↯ [shape]` reshape or fill, `⊏⍏`/`⊏⍖` sort trains,
/// `&fmt "s"`, `&provenance n`) into composite tokens, and lower Uiua's
/// dyadic-train rule: `▽ = 1 ⊡ 2 G` is `▽ (= 1 (⊡ 2 G)) G`, so a dyadic train
/// head (`▽`/`⊏` over a composition) duplicates the line's final operand.
fn desugar(words: &[(usize, Tok)]) -> Result<Vec<(usize, Tok)>, String> {
    let mut out = Vec::new();
    let mut i = 0;

    // The final operand of the line, if it is a single token: the value a
    // dyadic train applies its head to on both sides.
    let line_operand: Option<Tok> = match words.last() {
        Some((_, t)) if is_operand(t) => Some(t.clone()),
        _ => None,
    };

    // A dyadic train head: `▽` (keep) or `⊏` (select) followed by a
    // composition rather than an operand (`▽ = 1 ⊡ 2 G`, `⊏ ⍏ T`).
    let is_train_head = |k: usize, t: &Tok| -> bool {
        match t {
            Tok::Keep => matches!(words.get(k + 1).map(|(_, n)| n), Some(n) if !is_operand(n)),
            Tok::Select => matches!(
                words.get(k + 1).map(|(_, n)| n),
                Some(n) if matches!(n, Tok::GradeUp | Tok::GradeDown)
            ),
            _ => false,
        }
    };
    let mut train_heads = 0usize;
    for (k, (_, t)) in words.iter().enumerate() {
        if is_train_head(k, t) {
            train_heads += 1;
        }
    }

    while i < words.len() {
        let (ln, t) = &words[i];
        match t {
            Tok::Reshape { .. } => {
                if let Some((_, Tok::List { payload, .. })) = words.get(i + 1) {
                    let dims = dims_from_payload(payload);
                    let mut d = [1u32; 4];
                    for k in 0..dims.len() {
                        d[k] = dims[k];
                    }
                    let rank = dims.len() as u8;
                    // Uiua: reshaping a scalar fills it out — `↯ [shape] value`.
                    if let Some((_, val)) = words.get(i + 2) {
                        match val {
                            Tok::Float(v) => {
                                out.push((*ln, Tok::Fill { dtype: DTYPE_F32, rank, shape: d, value: *v }));
                                i += 3;
                                continue;
                            }
                            Tok::Int(v) => {
                                out.push((*ln, Tok::Fill { dtype: DTYPE_I64, rank, shape: d, value: *v as f64 }));
                                i += 3;
                                continue;
                            }
                            _ => {}
                        }
                    }
                    out.push((*ln, Tok::Reshape { rank, dims: d }));
                    i += 2;
                    continue;
                }
                return Err(format!("line {}: ↯ needs a shape list", ln));
            }
            Tok::Sys(name) if name == "fmt" => {
                if let Some((_, Tok::Str(tpl))) = words.get(i + 1) {
                    out.push((*ln, Tok::Fmt { template: tpl.clone() }));
                    i += 2;
                    continue;
                }
                // bare &fmt (no template)
                out.push((*ln, Tok::Fmt { template: Vec::new() }));
            }
            Tok::Sys(name) if name == "provenance" => {
                if let Some((_, Tok::Int(n))) = words.get(i + 1) {
                    out.push((*ln, Tok::Provenance(*n as u32)));
                    i += 2;
                    continue;
                }
                return Err(format!("line {}: &provenance needs a node id", ln));
            }
            _ => out.push(words[i].clone()),
        }
        i += 1;
    }

    if train_heads > 0 {
        match &line_operand {
            Some(op) => {
                for _ in 0..train_heads {
                    out.push((out.last().map(|(l, _)| *l).unwrap_or(0), op.clone()));
                }
            }
            None => return Err("a ▽/⊏ train sorts or keeps a single operand in this subset".to_string()),
        }
    }
    Ok(out)
}

/// Tokens that can be a value operand (a train's duplicated argument).
fn is_operand(t: &Tok) -> bool {
    matches!(
        t,
        Tok::Int(_)
            | Tok::Float(_)
            | Tok::Str(_)
            | Tok::List { .. }
            | Tok::Ident(_)
            | Tok::Sys(_)
    )
}

impl Compiler {
    fn eval_words(&mut self, words: &[(usize, Tok)]) -> Result<(), String> {
        // Uiua reads right-to-left.
        for (ln, tok) in words.iter().rev() {
            self.line = *ln;
            self.eval_token(tok)?;
        }
        Ok(())
    }

    fn eval_token(&mut self, tok: &Tok) -> Result<(), String> {
        match tok {
            Tok::Int(v) => {
                let payload = const_i64(&[*v]);
                self.push_const(DTYPE_I64, 0, &[1, 1, 1, 1], &payload);
                Ok(())
            }
            Tok::Float(v) => {
                let payload = const_f32(&[*v as f32]);
                self.push_const(DTYPE_F32, 0, &[1, 1, 1, 1], &payload);
                Ok(())
            }
            Tok::Str(s) => {
                let payload = const_u8(s);
                self.push_const(DTYPE_U8, 1, &[s.len() as u32, 1, 1, 1], &payload);
                Ok(())
            }
            Tok::List { dtype, rank, shape, payload } => {
                self.push_const(*dtype, *rank, shape, payload);
                Ok(())
            }
            Tok::Ident(name) => {
                if let Some(v) = self.bindings.get(name) {
                    self.stack.push(*v);
                    return Ok(());
                }
                if let Some(cv) = self.consts.get(name).cloned() {
                    // A session constant from an earlier line: re-emit it.
                    let n = self.emit(OP_CONST, cv.dtype, cv.rank, &cv.shape, true, CAP_NONE, NONE, NONE, NONE, "const", &cv.payload);
                    self.stack.push(SValue { node: n, dtype: cv.dtype, rank: cv.rank, shape: cv.shape });
                    return Ok(());
                }
                Err(self.error(&format!("unknown name '{}'", name)))
            }
            Tok::Add => self.binop(OP_ADD, "Add"),
            Tok::Sub => self.binop(OP_SUB, "Sub"),
            Tok::Mul => self.binop(OP_MUL, "Multiply"),
            Tok::Div => self.binop(OP_DIV, "Div"),
            Tok::Sum => {
                let a = self.pop()?;
                let (rank, shape) = if a.rank == 0 {
                    (0, [1u32, 1, 1, 1])
                } else if a.rank == 1 {
                    (0, [1u32, 1, 1, 1])
                } else {
                    (1, [a.shape[0], 1, 1, 1])
                };
                let n = self.emit(OP_REDUCE_SUM, a.dtype, rank, &shape, true, CAP_NONE, a.node, NONE, NONE, "ReduceSum", &[]);
                self.stack.push(SValue { node: n, dtype: a.dtype, rank, shape });
                Ok(())
            }
            Tok::Reverse => self.unop(OP_REVERSE, "Reverse", true, CAP_NONE),
            Tok::Length => self.unop(OP_COUNT, "Count", true, CAP_NONE),
            Tok::GradeUp | Tok::GradeDown => {
                let a = self.pop()?;
                let rows = if a.rank >= 1 { a.shape[0] } else { 1 };
                let op = if *tok == Tok::GradeUp { OP_GRADE_UP } else { OP_GRADE_DOWN };
                let n = self.emit(op, DTYPE_I64, 1, &[rows, 1, 1, 1], true, CAP_NONE, a.node, NONE, NONE, if *tok == Tok::GradeUp { "GradeUp" } else { "GradeDown" }, &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 1, shape: [rows, 1, 1, 1] });
                Ok(())
            }
            Tok::Select => {
                let idx = self.pop()?;
                let arr = self.pop()?;
                let n = self.emit(OP_SELECT, arr.dtype, arr.rank, &arr.shape, true, CAP_NONE, idx.node, arr.node, NONE, "Select", &[]);
                self.stack.push(SValue { node: n, dtype: arr.dtype, rank: arr.rank, shape: arr.shape });
                Ok(())
            }
            Tok::Keep => {
                let mask = self.pop()?;
                let arr = self.pop()?;
                // Kept rows are known only at run time; the stepper computes
                // the actual shape from the mask.
                let mut shape = [1u32; 4];
                if arr.rank >= 2 {
                    shape[0] = 0;
                    shape[1] = arr.shape[1];
                } else {
                    shape[0] = 0;
                }
                let n = self.emit(OP_KEEP, DTYPE_I64, if arr.rank >= 2 { 2 } else { 1 }, &shape, true, CAP_NONE, mask.node, arr.node, NONE, "Keep", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: if arr.rank >= 2 { 2 } else { 1 }, shape });
                Ok(())
            }
            Tok::Pick => {
                let idx = self.pop()?;
                let arr = self.pop()?;
                let (rank, shape) = if arr.rank >= 2 {
                    (1, [arr.shape[1], 1, 1, 1])
                } else {
                    (0, [1, 1, 1, 1])
                };
                let n = self.emit(OP_PICK, arr.dtype, rank, &shape, true, CAP_NONE, idx.node, arr.node, NONE, "Pick", &[]);
                self.stack.push(SValue { node: n, dtype: arr.dtype, rank, shape });
                Ok(())
            }
            Tok::Eq => self.binop(OP_EQ, "Equal"),
            Tok::Reshape { rank, dims } => {
                let data = self.pop()?;
                let mut shape = [1u32; 4];
                for d in 0..*rank as usize {
                    shape[d] = dims[d];
                }
                if shape_elems(*rank, &shape) != shape_elems(data.rank, &data.shape) {
                    return Err(self.error("reshape must preserve element count"));
                }
                let n = self.emit(OP_RESHAPE, data.dtype, *rank, &shape, true, CAP_NONE, data.node, NONE, NONE, "Reshape", &[]);
                self.stack.push(SValue { node: n, dtype: data.dtype, rank: *rank, shape });
                Ok(())
            }
            Tok::Fill { dtype, rank, shape, value } => {
                let payload = const_fill(*dtype, *rank, shape, *value);
                self.parallel_axis = if *rank > 0 { 1 } else { 0 };
                let n = self.emit(OP_FILL, *dtype, *rank, shape, true, CAP_NONE, NONE, NONE, NONE, "Fill", &payload);
                self.stack.push(SValue { node: n, dtype: *dtype, rank: *rank, shape: *shape });
                Ok(())
            }
            Tok::Fmt { template } => {
                let a = self.pop()?;
                let n = self.emit(OP_FORMAT, DTYPE_U8, 1, &[0, 1, 1, 1], true, CAP_NONE, a.node, NONE, NONE, "Format", template);
                self.stack.push(SValue { node: n, dtype: DTYPE_U8, rank: 1, shape: [0, 1, 1, 1] });
                Ok(())
            }
            Tok::Provenance(id) => {
                let payload = id.to_le_bytes().to_vec();
                let n = self.emit(OP_PROVENANCE, DTYPE_U8, 1, &[0, 1, 1, 1], true, CAP_NONE, NONE, NONE, NONE, "Provenance", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_U8, rank: 1, shape: [0, 1, 1, 1] });
                Ok(())
            }
            Tok::Sys(name) => self.eval_sys(name),
            Tok::Arrow | Tok::Newline => Ok(()),
        }
    }

    fn eval_sys(&mut self, name: &str) -> Result<(), String> {
        match name {
            "display" => self.unop(OP_DISPLAY, "Display", false, CAP_DISPLAY),
            "keys" => {
                let n = self.emit(OP_KEYBOARD, DTYPE_U8, 1, &[0, 1, 1, 1], false, CAP_KEYBOARD, NONE, NONE, NONE, "Keyboard", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_U8, rank: 1, shape: [0, 1, 1, 1] });
                Ok(())
            }
            "graph-nodes" => {
                self.source(OP_GRAPH_NODES, "GraphNodes");
                Ok(())
            }
            "graph-edges" => {
                self.source(OP_GRAPH_EDGES, "GraphEdges");
                Ok(())
            }
            "machine" => {
                self.source(OP_MACHINE_DESC, "MachineDesc");
                Ok(())
            }
            "ready" => {
                self.source(OP_READY_SET, "ReadySet");
                Ok(())
            }
            "caps" => {
                self.source(OP_CAPS, "Caps");
                Ok(())
            }
            "names" => {
                self.source(OP_NAMES, "Names");
                Ok(())
            }
            "rows" => {
                let a = self.pop()?;
                self.parallel_axis = 1;
                let n = self.emit(OP_ROWS, a.dtype, a.rank, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "Rows", &[]);
                self.stack.push(SValue { node: n, dtype: a.dtype, rank: a.rank, shape: a.shape });
                Ok(())
            }
            "send" => self.unop(OP_SEND, "Send", true, CAP_NONE),
            "copy" => {
                // dyadic: &copy trigger array (the trigger orders the copy)
                let a = self.pop()?; // array
                let trigger = self.pop()?; // ordering trigger
                let n = self.emit(OP_COPY, a.dtype, a.rank, &a.shape, true, CAP_NONE, trigger.node, a.node, NONE, "Copy", &[]);
                self.stack.push(SValue { node: n, dtype: a.dtype, rank: a.rank, shape: a.shape });
                Ok(())
            }
            "copied" => {
                let a = self.pop()?;
                let n = self.emit(OP_COUNTER_COPIED, DTYPE_I64, 0, &[1, 1, 1, 1], true, CAP_NONE, a.node, NONE, NONE, "BytesCopied", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                Ok(())
            }
            "bytes" => {
                let a = self.pop()?;
                let n = self.emit(OP_COUNTER_BYTES, DTYPE_I64, 0, &[1, 1, 1, 1], true, CAP_NONE, a.node, NONE, NONE, "BytesMoved", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                Ok(())
            }
            "entries" => {
                let a = self.pop()?;
                let n = self.emit(OP_COUNTER_ENTRIES, DTYPE_I64, 0, &[1, 1, 1, 1], true, CAP_NONE, a.node, NONE, NONE, "KernelEntries", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                Ok(())
            }
            "zero" => {
                let n = self.emit(OP_ZERO, DTYPE_I64, 0, &[1, 1, 1, 1], true, CAP_NONE, NONE, NONE, NONE, "Zero", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                Ok(())
            }
            "fmt-machine" => {
                let n = self.emit(OP_FMT_MACHINE, DTYPE_U8, 1, &[0, 1, 1, 1], true, CAP_NONE, NONE, NONE, NONE, "FmtMachine", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_U8, rank: 1, shape: [0, 1, 1, 1] });
                Ok(())
            }
            "stats" => {
                let a = self.pop()?;
                let n = self.emit(OP_STATS, DTYPE_U8, 1, &[0, 1, 1, 1], true, CAP_NONE, a.node, NONE, NONE, "Stats", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_U8, rank: 1, shape: [0, 1, 1, 1] });
                Ok(())
            }
            "hash" | "store" => {
                let a = self.pop()?;
                let op = if name == "hash" { OP_HASH } else { OP_STORE };
                let n = self.emit(op, DTYPE_I64, 0, &[1, 1, 1, 1], true, CAP_NONE, a.node, NONE, NONE, if name == "hash" { "Hash" } else { "Store" }, &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                Ok(())
            }
            "load" => {
                let a = self.pop()?;
                // The loaded value's shape is known only at run time.
                let n = self.emit(OP_LOAD, DTYPE_I64, 1, &[0, 1, 1, 1], true, CAP_NONE, a.node, NONE, NONE, "Load", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 1, shape: [0, 1, 1, 1] });
                Ok(())
            }
            "clock" | "replay-clock" => {
                let op = if name == "clock" { OP_CLOCK } else { OP_REPLAY_CLOCK };
                let n = self.emit(op, DTYPE_I64, 0, &[1, 1, 1, 1], false, CAP_NONE, NONE, NONE, NONE, if name == "clock" { "Clock" } else { "ReplayClock" }, &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                Ok(())
            }
            "replay-keys" => {
                let n = self.emit(OP_REPLAY_KEYS, DTYPE_U8, 1, &[0, 1, 1, 1], false, CAP_NONE, NONE, NONE, NONE, "ReplayKeys", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_U8, rank: 1, shape: [0, 1, 1, 1] });
                Ok(())
            }
            "trace" => {
                self.source(OP_TRACE, "Trace");
                Ok(())
            }
            other => Err(self.error(&format!("unknown system function '&{}'", other))),
        }
    }
}

/// Compile a whole Uiua source file to UIR bytes (no fusion; that is a host
/// optimization applied by `hostc` after this returns).
pub fn compile_file(src: &str) -> Result<Vec<u8>, String> {
    let mut c = Compiler::new();
    c.compile_line(src)?;
    Ok(c.finish())
}
