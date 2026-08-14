//! Tacit host compiler: lowers a first-milestone subset of **Uiua** to UIR.
//!
//! Uiua is a tacit array language: functions appear to the left of their
//! arguments and code is read right-to-left (`+ 2 3` is 2+3, `× 2 + 3 5` is
//! 2*(3+5)).  The subset covers numeric scalars/arrays, character vectors,
//! bindings (`Name ← value`), elementwise arithmetic, reduce (`/+`), reshape
//! (`↯`), reverse (`⇌`), length (`⧻`), plus the OS system functions (`&name`)
//! for effects, graph/machine queries, counters, and placement.  Anything
//! else is a compile error with a source location.

use std::collections::HashMap;
use uir::*;

#[derive(Clone, Copy)]
struct SValue {
    node: u32,
    dtype: u8,
    rank: u8,
    shape: [u32; 4],
}

struct Compiler {
    enc: Encoder,
    stack: Vec<SValue>,
    bindings: HashMap<String, SValue>,
    fuse: bool,
    line: usize,
    next_name: Option<String>,
    parallel_axis: u8,
}

fn shape_elems(rank: u8, shape: &[u32; 4]) -> u64 {
    let mut n = 1u64;
    for i in 0..rank as usize {
        n *= shape[i] as u64;
    }
    n
}

impl Compiler {
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
        self.parallel_axis = 0;
        let d = NodeDesc {
            id: self.enc.count,
            op,
            dtype,
            rank,
            shape: *shape,
            pure,
            parallel_axis: par,
            engine: ENGINE_PCORE,
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
            let s0 = chars[ci].0;
            while ci < chars.len() && (chars[ci].1.is_ascii_alphanumeric() || chars[ci].1 == '-') {
                ci += 1;
            }
            let name = src[s0..chars[ci].0].to_string();
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
            let name = &src[s0..chars[ci].0];
            let tok = match name {
                "add" => Tok::Add,
                "sub" | "subtract" => Tok::Sub,
                "mul" | "multiply" => Tok::Mul,
                "div" | "divide" => Tok::Div,
                "sum" => Tok::Sum,
                "reverse" => Tok::Reverse,
                "length" => Tok::Length,
                "reshape" => Tok::Reshape { rank: 0, dims: [1, 1, 1, 1] },
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
    let text = &src[s0..chars[i].0];
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

/// Rewrite special forms (`&fill [shape] v`, `↯ [shape]`, `&fmt "s"`,
/// `&provenance n`) into composite tokens.
fn desugar(words: &[(usize, Tok)]) -> Result<Vec<(usize, Tok)>, String> {
    let mut out = Vec::new();
    let mut i = 0;
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
                    out.push((*ln, Tok::Reshape { rank: dims.len() as u8, dims: d }));
                    i += 2;
                    continue;
                }
                return Err(format!("line {}: ↯ needs a shape list", ln));
            }
            Tok::Sys(name) if name == "fill" => {
                if let (Some((_, Tok::List { payload, .. })), Some((_, val))) =
                    (words.get(i + 1), words.get(i + 2))
                {
                    let dims = dims_from_payload(payload);
                    let (dtype, value) = match val {
                        Tok::Float(v) => (DTYPE_F32, *v),
                        Tok::Int(v) => (DTYPE_I64, *v as f64),
                        _ => return Err(format!("line {}: &fill value must be a number", ln)),
                    };
                    let mut shape = [1u32; 4];
                    for k in 0..dims.len() {
                        shape[k] = dims[k];
                    }
                    out.push((*ln, Tok::Fill { dtype, rank: dims.len() as u8, shape, value }));
                    i += 3;
                    continue;
                }
                return Err(format!("line {}: &fill needs [shape] value", ln));
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
    Ok(out)
}

fn compile(src: &str, fuse: bool) -> Result<Vec<u8>, String> {
    let toks = tokenize(src)?;
    let mut c = Compiler {
        enc: Encoder::new(),
        stack: Vec::new(),
        bindings: HashMap::new(),
        fuse,
        line: 1,
        next_name: None,
        parallel_axis: 0,
    };

    // Split into lines.
    let mut lines: Vec<Vec<(usize, Tok)>> = Vec::new();
    let mut cur: Vec<(usize, Tok)> = Vec::new();
    for (ln, t) in toks {
        if t == Tok::Newline {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push((ln, t));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }

    for stmt in lines {
        if stmt.is_empty() {
            continue;
        }
        c.line = stmt[0].0;
        let is_binding = stmt.len() >= 2 && matches!(stmt[0].1, Tok::Ident(_)) && stmt[1].1 == Tok::Arrow;
        if is_binding {
            let name = match &stmt[0].1 {
                Tok::Ident(n) => n.clone(),
                _ => unreachable!(),
            };
            c.next_name = Some(name.clone());
            let words = desugar(&stmt[2..])?;
            c.eval_words(&words)?;
            let v = c.stack.pop().ok_or_else(|| c.error("binding has no value"))?;
            c.bindings.insert(name, v);
        } else {
            let words = desugar(&stmt)?;
            c.eval_words(&words)?;
        }
    }

    let mut bytes = c.enc.finish();
    if fuse {
        bytes = fuse_pass(&bytes);
    }
    Ok(bytes)
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
                let v = self.bindings.get(name).cloned().ok_or_else(|| self.error(&format!("unknown name '{}'", name)))?;
                self.stack.push(v);
                Ok(())
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
            "filter-pure" => {
                let a = self.pop()?;
                let payload = filter_payload(2, 1);
                let n = self.emit(OP_FILTER, DTYPE_I64, 2, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "Filter", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: a.shape });
                Ok(())
            }
            "filter-effectful" => {
                let a = self.pop()?;
                let payload = filter_payload(2, 0);
                let n = self.emit(OP_FILTER, DTYPE_I64, 2, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "Filter", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: a.shape });
                Ok(())
            }
            "sort-asc" => {
                let a = self.pop()?;
                let payload = [0u8, 0u8];
                let n = self.emit(OP_SORT_BY, DTYPE_I64, 2, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "SortBy", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: a.shape });
                Ok(())
            }
            "sort-desc" => {
                let a = self.pop()?;
                let payload = [0u8, 1u8];
                let n = self.emit(OP_SORT_BY, DTYPE_I64, 2, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "SortBy", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: a.shape });
                Ok(())
            }
            "order" => self.unop(OP_ORDER, "Order", true, CAP_NONE),
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
            other => Err(self.error(&format!("unknown system function '&{}'", other))),
        }
    }
}

/// Extract dims from a rank-1 i64 list value (a shape literal).
fn filter_payload(col: u8, val: i64) -> Vec<u8> {
    let mut p = Vec::new();
    p.push(col);
    p.extend_from_slice(&val.to_le_bytes());
    p
}

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
    match compile(&src, fuse) {
        Ok(uir_bytes) => {
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
