//! Tacit host compiler: lowers the documented Uiua subset to UIR.
//!
//! The subset: numeric scalars, rank-1/2 numeric arrays, character arrays,
//! bindings, elementwise arithmetic, reduce, reshape, reverse, fill, display
//! write, keyboard read, graph/machine query, table ops (count/filter/sort/
//! order), and format.  Anything else is a compile error with a source
//! location.  Files, threads, Metal, CUDA, and sockets are out of subset and
//! are rejected before any image is produced.

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
    src: String,
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
}

impl Compiler {
    fn error(&self, msg: &str) -> String {
        format!("{}:{}: {}", "line", self.line, msg)
    }

    fn push_value(&mut self, name: &str, v: SValue) {
        self.stack.push(v);
        let _ = name;
    }

    fn pop(&mut self) -> Result<SValue, String> {
        self.stack.pop().ok_or_else(|| self.error("stack underflow"))
    }

    fn binop(&mut self, op: u8, name: &str) -> Result<(), String> {
        let b = self.pop()?;
        let a = self.pop()?;
        // scalar broadcast or same shape
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
        let n = self.emit( op, dtype, rank, &shape, true, CAP_NONE, a.node, b.node, NONE, name, &[]);
        self.stack.push(SValue { node: n, dtype, rank, shape });
        Ok(())
    }

    fn unop(&mut self, op: u8, name: &str, pure: bool, cap: u8) -> Result<(), String> {
        let a = self.pop()?;
        let n = self.emit( op, a.dtype, a.rank, &a.shape, pure, cap, a.node, NONE, NONE, name, &[]);
        self.stack.push(SValue { node: n, dtype: a.dtype, rank: a.rank, shape: a.shape });
        Ok(())
    }

    fn source(&mut self, op: u8, name: &str) {
        let n = self.emit( op, DTYPE_I64, 2, &[0, 7, 1, 1], true, CAP_NONE, NONE, NONE, NONE, name, &[]);
        self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: [0, 7, 1, 1] });
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Int(i64),
    Float(f64),
    Str(Vec<u8>),
    Ident(String),
    Sys(String),
    Plus,
    Minus,
    Star,
    Slash,
    Arrow,
    LBracket,
    RBracket,
    Newline,
}

fn tokenize(src: &str) -> Result<Vec<(usize, Tok)>, String> {
    let mut toks = Vec::new();
    let bytes = src.as_bytes();
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut ci = 0usize;
    let mut line = 1;
    while ci < chars.len() {
        let (bi, c) = chars[ci];
        let i = bi;
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
            toks.push((line, Tok::LBracket));
            ci += 1;
            continue;
        }
        if c == ']' {
            toks.push((line, Tok::RBracket));
            ci += 1;
            continue;
        }
        if c == '←' {
            toks.push((line, Tok::Arrow));
            ci += 1;
            continue;
        }
        if c == '+' {
            toks.push((line, Tok::Plus));
            ci += 1;
            continue;
        }
        if c == '-' {
            toks.push((line, Tok::Minus));
            ci += 1;
            continue;
        }
        if c == '×' || c == '*' {
            toks.push((line, Tok::Star));
            ci += 1;
            continue;
        }
        if c == '÷' || c == '/' {
            toks.push((line, Tok::Slash));
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
            let s0 = if ci < chars.len() { chars[ci].0 } else { i + 1 };
            while ci < chars.len() && (chars[ci].1.is_ascii_alphanumeric() || chars[ci].1 == '-') {
                ci += 1;
            }
            let s1 = if ci < chars.len() { chars[ci].0 } else { bytes.len() };
            let name = src[s0..s1].to_string();
            toks.push((start_line, Tok::Sys(name)));
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && ci + 1 < chars.len() && chars[ci + 1].1.is_ascii_digit()) {
            let start_line = line;
            let s0 = i;
            let mut is_float = false;
            while ci < chars.len() && (chars[ci].1.is_ascii_digit() || chars[ci].1 == '.') {
                if chars[ci].1 == '.' {
                    is_float = true;
                }
                ci += 1;
            }
            let s1 = if ci < chars.len() { chars[ci].0 } else { bytes.len() };
            let text = &src[s0..s1];
            if is_float {
                toks.push((start_line, Tok::Float(text.parse().map_err(|_| format!("line {}: bad float", start_line))?)));
            } else {
                toks.push((start_line, Tok::Int(text.parse().map_err(|_| format!("line {}: bad int", start_line))?)));
            }
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start_line = line;
            let s0 = i;
            while ci < chars.len() && (chars[ci].1.is_ascii_alphanumeric() || chars[ci].1 == '_' || chars[ci].1 == '-') {
                ci += 1;
            }
            let s1 = if ci < chars.len() { chars[ci].0 } else { bytes.len() };
            let name = src[s0..s1].to_string();
            // reject out-of-subset nouns
            if matches!(name.as_str(), "open" | "read" | "write" | "close" | "seek" | "fork" | "exec" | "thread" | "spawn" | "pthread" | "socket" | "listen" | "accept" | "metal" | "cuda" | "coreml" | "accelerate" | "file" | "ioctl") {
                return Err(format!("line {}: '{}' is out of the first-milestone subset", start_line, name));
            }
            toks.push((start_line, Tok::Ident(name)));
            continue;
        }
        return Err(format!("line {}: unexpected character '{}'", line, c));
    }
    Ok(toks)
}

fn compile(src: &str, fuse: bool) -> Result<Vec<u8>, String> {
    let toks = tokenize(src)?;
    let mut c = Compiler {
        enc: Encoder::new(),
        stack: Vec::new(),
        bindings: HashMap::new(),
        fuse,
        src: src.to_string(),
        line: 1,
        next_name: None,
        parallel_axis: 0,
    };

    // Split into statements on newlines; within a statement, an Arrow marks a
    // binding (`name ← expr`).  The last non-empty statement is the program
    // body; earlier statements are bindings.
    let mut stmts: Vec<Vec<(usize, Tok)>> = Vec::new();
    let mut cur: Vec<(usize, Tok)> = Vec::new();
    for (ln, t) in &toks {
        if *t == Tok::Newline {
            if !cur.is_empty() {
                stmts.push(core::mem::take(&mut cur));
            }
        } else {
            cur.push((*ln, t.clone()));
        }
    }
    if !cur.is_empty() {
        stmts.push(cur);
    }

    let n_stmts = stmts.len();
    for (si, stmt) in stmts.iter().enumerate() {
        if stmt.is_empty() {
            continue;
        }
        let is_binding = stmt.len() >= 2 && stmt[1].1 == Tok::Arrow;
        if is_binding {
            // expect: name ← expr ...
            if stmt.len() < 2 || stmt[1].1 != Tok::Arrow {
                return Err(format!("line {}: expected binding `name ← expr`", stmt[0].0));
            }
            let name = match &stmt[0].1 {
                Tok::Ident(n) => n.clone(),
                _ => return Err(format!("line {}: expected binding name", stmt[0].0)),
            };
            c.line = stmt[0].0;
            c.next_name = Some(name.clone());
            c.eval_words(&stmt[2..])?;
            let v = c.stack.pop().ok_or_else(|| c.error("binding has no value"))?;
            c.bindings.insert(name, v);
        } else {
            c.line = stmt[0].0;
            c.eval_words(stmt)?;
        }
    }

    let mut bytes = c.enc.finish();
    if fuse {
        bytes = fuse_pass(&bytes);
    }
    Ok(bytes)
}

/// Fusion pass: replace a single-consumer pure Add→Multiply chain with one
/// AddMul kernel so the intermediate T = A+B is never materialized.  Does not
/// fuse across effectful nodes or fan-out (single-consumer rule).
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
        if nd.op == OP_MUL && nd.in0 != NONE && nd.pure {
            let add_idx = nd.in0 as usize;
            let add = nodes[add_idx];
            if add.op == OP_ADD
                && add.pure
                && add.in2 == NONE
                && consumers[add_idx] == 1
            {
                nodes[i].op = OP_ADD_MUL;
                nodes[i].in0 = add.in0;
                nodes[i].in1 = add.in1;
                nodes[i].in2 = nd.in1;
                dead[add_idx] = true;
            }
        }
    }

    // Re-encode with id remapping.
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
            d.name_len = if d.op == OP_ADD_MUL { 6 } else { d.name_len };
            let name: &[u8] = if d.op == OP_ADD_MUL {
                b"AddMul"
            } else {
                &prog.names[i]
            };
            enc.node(&d, name, &prog.consts[i]);
        }
    }
    enc.finish()
}

impl Compiler {
    fn eval_words(&mut self, words: &[(usize, Tok)]) -> Result<(), String> {
        let mut j = 0;
        while j < words.len() {
            let (ln, t) = &words[j];
            self.line = *ln;
            match t {
                Tok::Int(v) => {
                    let payload = const_i64(&[*v]);
                    let n = self.emit( OP_CONST, DTYPE_I64, 0, &[1, 1, 1, 1], true, CAP_NONE, NONE, NONE, NONE, "const", &payload);
                    self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                }
                Tok::Float(v) => {
                    let payload = const_f32(&[*v as f32]);
                    let n = self.emit( OP_CONST, DTYPE_F32, 0, &[1, 1, 1, 1], true, CAP_NONE, NONE, NONE, NONE, "const", &payload);
                    self.stack.push(SValue { node: n, dtype: DTYPE_F32, rank: 0, shape: [1, 1, 1, 1] });
                }
                Tok::Str(s) => {
                    let payload = const_u8(s);
                    let n = self.emit( OP_CONST, DTYPE_U8, 1, &[s.len() as u32, 1, 1, 1], true, CAP_NONE, NONE, NONE, NONE, "const", &payload);
                    self.stack.push(SValue { node: n, dtype: DTYPE_U8, rank: 1, shape: [s.len() as u32, 1, 1, 1] });
                }
                Tok::Ident(name) => {
                    let v = self.bindings.get(name).cloned().ok_or_else(|| self.error(&format!("unknown name '{}'", name)))?;
                    self.stack.push(v);
                }
                Tok::LBracket => {
                    // parse a numeric list: [ n n n ... ] (or matrix [[..][..]])
                    let (n, shape, rank, dtype) = self.parse_list(words, &mut j)?;
                    self.stack.push(SValue { node: n, dtype, rank, shape });
                }
                Tok::Plus => self.binop(OP_ADD, "Add")?,
                Tok::Minus => self.binop(OP_SUB, "Sub")?,
                Tok::Star => self.binop(OP_MUL, "Multiply")?,
                Tok::Slash => self.binop(OP_DIV, "Div")?,
                Tok::Sys(name) => self.eval_sys(name, words, &mut j)?,
                Tok::Arrow | Tok::RBracket | Tok::Newline => {}
            }
            j += 1;
        }
        Ok(())
    }

    fn parse_list(&mut self, words: &[(usize, Tok)], j: &mut usize) -> Result<(u32, [u32; 4], u8, u8), String> {
        // words[*j] == LBracket.  Collect ints/floats until RBracket.
        let start = *j;
        let mut k = *j + 1;
        let mut elems: Vec<(u8, f64, i64)> = Vec::new();
        let mut dtype = DTYPE_I64;
        let mut matrix = false;
        let mut inner_dims: Option<usize> = None;
        let mut row_count = 0usize;
        let mut nested = 0usize;
        while k < words.len() {
            match &words[k].1 {
                Tok::LBracket => {
                    nested += 1;
                    matrix = true;
                }
                Tok::RBracket => {
                    if nested == 0 {
                        break;
                    }
                    nested -= 1;
                    row_count += 1;
                }
                Tok::Int(v) => elems.push((DTYPE_I64, *v as f64, *v)),
                Tok::Float(v) => {
                    dtype = DTYPE_F32;
                    elems.push((DTYPE_F32, *v, *v as i64));
                }
                _ => {
                    return Err(self.error("unexpected token in list"));
                }
            }
            k += 1;
        }
        if k >= words.len() {
            return Err(self.error("unterminated list"));
        }
        *j = k; // advance to RBracket

        let _ = start;
        if matrix {
            // rank-2: row_count rows.  Infer cols.
            let total = elems.len();
            let cols = if row_count > 0 { total / row_count } else { 0 };
            let shape = [row_count as u32, cols as u32, 1, 1];
            let payload = list_payload(dtype, &elems);
            let n = self.emit( OP_CONST, dtype, 2, &shape, true, CAP_NONE, NONE, NONE, NONE, "const", &payload);
            Ok((n, shape, 2, dtype))
        } else {
            let shape = [elems.len() as u32, 1, 1, 1];
            let payload = list_payload(dtype, &elems);
            let n = self.emit( OP_CONST, dtype, 1, &shape, true, CAP_NONE, NONE, NONE, NONE, "const", &payload);
            Ok((n, shape, 1, dtype))
        }
    }

    fn eval_sys(&mut self, name: &str, words: &[(usize, Tok)], j: &mut usize) -> Result<(), String> {
        match name {
            "display" => self.unop(OP_DISPLAY, "Display", false, CAP_DISPLAY),
            "keys" => {
                // keyboard source (effectful)
                let n = self.emit( OP_KEYBOARD, DTYPE_U8, 1, &[0, 1, 1, 1], false, CAP_KEYBOARD, NONE, NONE, NONE, "Keyboard", &[]);
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
            "count" => self.unop(OP_COUNT, "Count", true, CAP_NONE),
            "filter-effectful" => {
                // Filter rows where pure == 0 (col 2 in node table)
                let a = self.pop()?;
                let payload = filter_payload(2, 0);
                let n = self.emit( OP_FILTER, DTYPE_I64, 2, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "Filter", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: a.shape });
                Ok(())
            }
            "filter-pure" => {
                // Filter rows where pure == 1 (col 2 in node table)
                let a = self.pop()?;
                let payload = filter_payload(2, 1);
                let n = self.emit( OP_FILTER, DTYPE_I64, 2, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "Filter", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: a.shape });
                Ok(())
            }
            "filter-ready" => {
                let a = self.pop()?;
                let payload = filter_payload(6, 1);
                let n = self.emit( OP_FILTER, DTYPE_I64, 2, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "Filter", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: a.shape });
                Ok(())
            }
            "sort-asc" => {
                let a = self.pop()?;
                let payload = [0u8, 0u8]; // col 0, ascending
                let n = self.emit( OP_SORT_BY, DTYPE_I64, 2, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "SortBy", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: a.shape });
                Ok(())
            }
            "sort-desc" => {
                let a = self.pop()?;
                let payload = [0u8, 1u8]; // col 0, descending
                let n = self.emit( OP_SORT_BY, DTYPE_I64, 2, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "SortBy", &payload);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 2, shape: a.shape });
                Ok(())
            }
            "reverse" => self.unop(OP_REVERSE, "Reverse", true, CAP_NONE),
            "order" => self.unop(OP_ORDER, "Order", true, CAP_NONE),
            "reduce-sum" => {
                let a = self.pop()?;
                let (rank, shape) = if a.rank == 0 {
                    (0, [1u32, 1, 1, 1])
                } else if a.rank == 1 {
                    (0, [1u32, 1, 1, 1])
                } else {
                    // reduce over the last axis: [r, c] -> [r]
                    (1, [a.shape[0], 1, 1, 1])
                };
                let n = self.emit(OP_REDUCE_SUM, a.dtype, rank, &shape, true, CAP_NONE, a.node, NONE, NONE, "ReduceSum", &[]);
                self.stack.push(SValue { node: n, dtype: a.dtype, rank, shape });
                Ok(())
            }
            "reshape" => {
                // &reshape [dims] : pop value, change shape (metadata)
                *j += 1;
                let (_, shape_tok) = words.get(*j).ok_or_else(|| self.error("&reshape needs a shape list"))?;
                let (rank, shape) = match shape_tok {
                    Tok::LBracket => {
                        let mut k = *j + 1;
                        let mut dims = Vec::new();
                        while k < words.len() {
                            match &words[k].1 {
                                Tok::RBracket => break,
                                Tok::Int(v) => dims.push(*v as u32),
                                _ => return Err(self.error("shape list must be ints")),
                            }
                            k += 1;
                        }
                        *j = k;
                        let mut s = [1u32; 4];
                        for (i, d) in dims.iter().enumerate() {
                            s[i] = *d;
                        }
                        (dims.len() as u8, s)
                    }
                    _ => return Err(self.error("&reshape needs a shape list")),
                };
                let a = self.pop()?;
                if shape_elems(rank, &shape) != shape_elems(a.rank, &a.shape) {
                    return Err(self.error("reshape must preserve element count"));
                }
                let n = self.emit(OP_RESHAPE, a.dtype, rank, &shape, true, CAP_NONE, a.node, NONE, NONE, "Reshape", &[]);
                self.stack.push(SValue { node: n, dtype: a.dtype, rank, shape });
                Ok(())
            }
            "rows" => {
                // rank-wise map: the leading axis is independent.
                let a = self.pop()?;
                self.parallel_axis = 1;
                let n = self.emit(OP_ROWS, a.dtype, a.rank, &a.shape, true, CAP_NONE, a.node, NONE, NONE, "Rows", &[]);
                self.stack.push(SValue { node: n, dtype: a.dtype, rank: a.rank, shape: a.shape });
                Ok(())
            }
            "caps" => {
                self.source(OP_CAPS, "Caps");
                Ok(())
            }
            "bytes" => {
                let n = self.emit( OP_COUNTER_BYTES, DTYPE_I64, 0, &[1, 1, 1, 1], true, CAP_NONE, NONE, NONE, NONE, "BytesMoved", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                Ok(())
            }
            "copied" => {
                let n = self.emit( OP_COUNTER_COPIED, DTYPE_I64, 0, &[1, 1, 1, 1], true, CAP_NONE, NONE, NONE, NONE, "BytesCopied", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                Ok(())
            }
            "entries" => {
                let n = self.emit( OP_COUNTER_ENTRIES, DTYPE_I64, 0, &[1, 1, 1, 1], true, CAP_NONE, NONE, NONE, NONE, "KernelEntries", &[]);
                self.stack.push(SValue { node: n, dtype: DTYPE_I64, rank: 0, shape: [1, 1, 1, 1] });
                Ok(())
            }
            "fmt" => {
                // &fmt "template" : pop value, push Format(value, template)
                // template is the next token (a string)
                *j += 1;
                let (_, tpl) = words.get(*j).ok_or_else(|| self.error("&fmt needs a template string"))?;
                let tpl = match tpl {
                    Tok::Str(s) => s.clone(),
                    _ => return Err(self.error("&fmt template must be a string")),
                };
                let a = self.pop()?;
                let n = self.emit( OP_FORMAT, DTYPE_U8, 1, &[0, 1, 1, 1], true, CAP_NONE, a.node, NONE, NONE, "Format", &tpl);
                self.stack.push(SValue { node: n, dtype: DTYPE_U8, rank: 1, shape: [0, 1, 1, 1] });
                Ok(())
            }
            "fill" => {
                // &fill [shape] value
                *j += 1;
                let (_, shape_tok) = words.get(*j).ok_or_else(|| self.error("&fill needs a shape list"))?;
                let shape = match shape_tok {
                    Tok::LBracket => {
                        // parse shape list of ints
                        let mut k = *j + 1;
                        let mut dims = Vec::new();
                        while k < words.len() {
                            match &words[k].1 {
                                Tok::RBracket => break,
                                Tok::Int(v) => dims.push(*v as u32),
                                _ => return Err(self.error("shape list must be ints")),
                            }
                            k += 1;
                        }
                        *j = k;
                        let mut s = [1u32; 4];
                        for (i, d) in dims.iter().enumerate() {
                            s[i] = *d;
                        }
                        (dims.len() as u8, s)
                    }
                    _ => return Err(self.error("&fill needs a shape list")),
                };
                let (rank, shape) = shape;
                *j += 1;
                let (_, val_tok) = words.get(*j).ok_or_else(|| self.error("&fill needs a value"))?;
                let (dtype, fill) = match val_tok {
                    Tok::Float(v) => (DTYPE_F32, *v),
                    Tok::Int(v) => (DTYPE_I64, *v as f64),
                    _ => return Err(self.error("&fill value must be numeric")),
                };
                let payload = const_fill(dtype, rank, &shape, fill);
                self.parallel_axis = if rank > 0 { 1 } else { 0 };
                let n = self.emit( OP_FILL, dtype, rank, &shape, true, CAP_NONE, NONE, NONE, NONE, "Fill", &payload);
                self.stack.push(SValue { node: n, dtype, rank, shape });
                Ok(())
            }
            other => Err(self.error(&format!("unknown system word '&{}'", other))),
        }
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

fn filter_payload(col: u8, val: i64) -> Vec<u8> {
    let mut p = Vec::new();
    p.push(col);
    p.extend_from_slice(&val.to_le_bytes());
    p
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
