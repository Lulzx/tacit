//! A pipeline is a value-transform graph.  `|` is not a byte pipe.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::session::GraphNode;
use crate::{Store, TreeCap};

#[derive(Clone, Debug)]
pub enum PipeVal {
    Text(String),
    Lines(Vec<String>),
    Nums(Vec<f64>),
    Scalar(f64),
}

pub fn run(
    store: &mut Store,
    cwd: &mut TreeCap,
    tokens: &[String],
) -> Result<(String, Vec<GraphNode>, Option<String>), String> {
    let (stages, dest) = split_stages(tokens)?;
    if stages.is_empty() {
        return Err("empty pipeline".to_string());
    }
    let mut graph: Vec<GraphNode> = Vec::new();
    let mut val = eval_source(store, cwd, &stages[0], &mut graph)?;
    for stage in &stages[1..] {
        val = apply_transform(store, cwd, val, stage, &mut graph)?;
    }
    if dest.is_some() {
        graph.push(node("Bind"));
        if let Some(name) = dest.as_deref() {
            graph.push(node(name));
        }
    }
    Ok((format_val(&val), graph, dest))
}

fn split_stages(tokens: &[String]) -> Result<(Vec<Vec<String>>, Option<String>), String> {
    if tokens.iter().any(|t| t == ">>") {
        return Err(">> is in-place mutation; write a new value instead".to_string());
    }
    let mut dest = None;
    let mut body = tokens;
    if let Some(i) = tokens.iter().position(|t| t == ">") {
        if i + 1 >= tokens.len() || i + 2 != tokens.len() {
            return Err("usage: … > name".to_string());
        }
        dest = Some(tokens[i + 1].clone());
        body = &tokens[..i];
    }
    let mut stages = Vec::new();
    let mut cur = Vec::new();
    for t in body {
        if t == "|" {
            if cur.is_empty() {
                return Err("empty pipeline stage".to_string());
            }
            stages.push(core::mem::take(&mut cur));
        } else {
            cur.push(t.clone());
        }
    }
    if cur.is_empty() {
        return Err("empty pipeline stage".to_string());
    }
    stages.push(cur);
    Ok((stages, dest))
}

fn eval_source(
    store: &Store,
    cwd: &TreeCap,
    stage: &[String],
    graph: &mut Vec<GraphNode>,
) -> Result<PipeVal, String> {
    match stage.first().map(String::as_str) {
        Some("cat") => {
            if stage.len() != 2 {
                return Err("usage: cat <name>".to_string());
            }
            let name = &stage[1];
            let blob = store.read_file(cwd, name).map_err(|e| e.as_str().to_string())?;
            let text = core::str::from_utf8(&blob.bytes)
                .map_err(|_| "not text".to_string())?
                .to_string();
            graph.push(node(name));
            Ok(PipeVal::Text(text))
        }
        Some("echo") => {
            let text = unescape(&join(&stage[1..]));
            graph.push(node("Value"));
            Ok(PipeVal::Text(text))
        }
        Some("ls") => {
            let kids = store.list(cwd).map_err(|e| e.as_str().to_string())?;
            graph.push(node("CurrentTree"));
            graph.push(node("Children"));
            Ok(PipeVal::Lines(kids.into_iter().map(|(n, _, _)| n).collect()))
        }
        Some("seq") => {
            graph.push(node("Seq"));
            Ok(PipeVal::Lines(seq_lines(&stage[1..])?))
        }
        Some("printf") => {
            graph.push(node("Printf"));
            Ok(PipeVal::Text(printf_fmt(&stage[1..])?))
        }
        Some(other) => Err(format!("not a source: {}", other)),
        None => Err("empty pipeline stage".to_string()),
    }
}

fn apply_transform(
    store: &mut Store,
    cwd: &mut TreeCap,
    val: PipeVal,
    stage: &[String],
    graph: &mut Vec<GraphNode>,
) -> Result<PipeVal, String> {
    let cmd = stage.first().map(String::as_str).unwrap_or("");
    match cmd {
        "lines" => {
            if stage.len() != 1 {
                return Err("usage: lines".to_string());
            }
            graph.push(node("Lines"));
            Ok(PipeVal::Lines(into_lines(val)))
        }
        "grep" | "filter" => {
            if stage.len() != 2 {
                return Err(format!("usage: {} <pattern>", cmd));
            }
            let pat = &stage[1];
            let lines = into_lines(maybe_lines(val, graph));
            graph.push(node(&format!("Filter({})", pat)));
            Ok(PipeVal::Lines(lines.into_iter().filter(|l| l.contains(pat.as_str())).collect()))
        }
        "sort" => {
            if stage.len() != 1 {
                return Err("usage: sort".to_string());
            }
            graph.push(node("Sort"));
            match val {
                PipeVal::Nums(mut n) => {
                    n.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
                    Ok(PipeVal::Nums(n))
                }
                other => {
                    let mut lines = into_lines(maybe_lines(other, graph));
                    lines.sort();
                    Ok(PipeVal::Lines(lines))
                }
            }
        }
        "uniq" | "unique" => {
            if stage.len() != 1 {
                return Err("usage: uniq".to_string());
            }
            graph.push(node("Unique"));
            let lines = into_lines(maybe_lines(val, graph));
            let mut out = Vec::new();
            for l in lines {
                if out.last() != Some(&l) && !out.iter().any(|x| x == &l) {
                    out.push(l);
                }
            }
            Ok(PipeVal::Lines(out))
        }
        "parse" => {
            if stage.len() != 1 {
                return Err("usage: parse".to_string());
            }
            graph.push(node("Parse"));
            Ok(PipeVal::Nums(into_nums(val)?))
        }
        "square" => {
            if stage.len() != 1 {
                return Err("usage: square".to_string());
            }
            graph.push(node("Square"));
            Ok(PipeVal::Nums(into_nums(val)?.into_iter().map(|x| x * x).collect()))
        }
        "sum" => {
            if stage.len() != 1 {
                return Err("usage: sum".to_string());
            }
            graph.push(node("Sum"));
            let n = into_nums(val)?;
            Ok(PipeVal::Scalar(n.iter().copied().sum()))
        }
        "rev" | "reverse" | "tac" => {
            graph.push(node("Reverse"));
            match val {
                PipeVal::Nums(mut n) => {
                    n.reverse();
                    Ok(PipeVal::Nums(n))
                }
                PipeVal::Lines(mut l) => {
                    l.reverse();
                    Ok(PipeVal::Lines(l))
                }
                PipeVal::Text(t) => {
                    let mut l = into_lines(PipeVal::Text(t));
                    l.reverse();
                    Ok(PipeVal::Lines(l))
                }
                PipeVal::Scalar(s) => Ok(PipeVal::Scalar(s)),
            }
        }
        "head" => {
            let n = flag_n(&stage[1..], 10)?;
            graph.push(node("Head"));
            let mut lines = into_lines(maybe_lines(val, graph));
            lines.truncate(n);
            Ok(PipeVal::Lines(lines))
        }
        "tail" => {
            let n = flag_n(&stage[1..], 10)?;
            graph.push(node("Tail"));
            let lines = into_lines(maybe_lines(val, graph));
            let start = lines.len().saturating_sub(n);
            Ok(PipeVal::Lines(lines[start..].to_vec()))
        }
        "wc" => {
            graph.push(node("Count"));
            Ok(PipeVal::Text(wc_text(&val, &stage[1..])))
        }
        "nl" => {
            graph.push(node("NumberLines"));
            let lines = into_lines(maybe_lines(val, graph));
            let numbered = lines
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{}\t{}", i + 1, l))
                .collect();
            Ok(PipeVal::Lines(numbered))
        }
        "cut" => {
            graph.push(node("Cut"));
            Ok(PipeVal::Lines(cut_lines(&into_lines(maybe_lines(val, graph)), &stage[1..])?))
        }
        "tr" => {
            if stage.len() != 3 {
                return Err("usage: tr <from> <to>".to_string());
            }
            graph.push(node("Translate"));
            Ok(PipeVal::Text(tr_text(&format_val(&val), &stage[1], &stage[2])))
        }
        "tee" => {
            if stage.len() != 2 {
                return Err("usage: tee <name>".to_string());
            }
            let name = &stage[1];
            let text = format_val(&val);
            store
                .write_file(cwd, name, text.as_bytes(), "text/plain")
                .map_err(|e| e.as_str().to_string())?;
            graph.push(node("Tee"));
            Ok(val)
        }
        other => Err(format!("unknown transform: {}", other)),
    }
}

pub fn seq_lines(args: &[String]) -> Result<Vec<String>, String> {
    let nums: Result<Vec<i64>, _> = args.iter().map(|s| s.parse::<i64>()).collect();
    let nums = nums.map_err(|_| "usage: seq [first [incr]] last".to_string())?;
    let (mut cur, step, last) = match nums.as_slice() {
        [last] => (1i64, 1i64, *last),
        [first, last] => (*first, 1i64, *last),
        [first, incr, last] => (*first, *incr, *last),
        _ => return Err("usage: seq [first [incr]] last".to_string()),
    };
    if step == 0 {
        return Err("seq: increment is 0".to_string());
    }
    let mut out = Vec::new();
    if step > 0 {
        while cur <= last {
            out.push(format!("{}", cur));
            cur += step;
        }
    } else {
        while cur >= last {
            out.push(format!("{}", cur));
            cur += step;
        }
    }
    Ok(out)
}

pub fn printf_fmt(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("usage: printf <format> [args]".to_string());
    }
    let fmt = unescape(&args[0]);
    let mut vals = args[1..].iter();
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.next() {
                Some('%') => out.push('%'),
                Some('s') | Some('d') | Some('i') => {
                    if let Some(v) = vals.next() {
                        out.push_str(v);
                    }
                }
                Some(other) => {
                    out.push('%');
                    out.push(other);
                }
                None => out.push('%'),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

pub fn flag_n(args: &[String], default: usize) -> Result<usize, String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-n" {
            let n = args.get(i + 1).ok_or("usage: -n <count>")?;
            return n.parse().map_err(|_| "bad -n count".to_string());
        }
        if args[i].starts_with('-') && args[i].len() > 1 && args[i].as_bytes()[1].is_ascii_digit() {
            return args[i][1..].parse().map_err(|_| "bad count".to_string());
        }
        i += 1;
    }
    Ok(default)
}

pub fn wc_text(val: &PipeVal, flags: &[String]) -> String {
    let text = format_val(val);
    let lines = text.lines().count();
    let words = text.split_whitespace().count();
    let bytes = text.len();
    let show_l = flags.iter().any(|f| f == "-l") || flags.is_empty();
    let show_w = flags.iter().any(|f| f == "-w") || flags.is_empty();
    let show_c = flags.iter().any(|f| f == "-c") || flags.is_empty();
    let mut parts = Vec::new();
    if show_l {
        parts.push(format!("{}", lines));
    }
    if show_w {
        parts.push(format!("{}", words));
    }
    if show_c {
        parts.push(format!("{}", bytes));
    }
    parts.join(" ")
}

fn cut_lines(lines: &[String], args: &[String]) -> Result<Vec<String>, String> {
    let mut delim = "\t".to_string();
    let mut field = 1usize;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-d" {
            delim = args.get(i + 1).ok_or("usage: cut -d <delim> -f <n>")?.clone();
            i += 2;
            continue;
        }
        if args[i] == "-f" {
            field = args
                .get(i + 1)
                .ok_or("usage: cut -d <delim> -f <n>")?
                .parse()
                .map_err(|_| "bad field".to_string())?;
            i += 2;
            continue;
        }
        i += 1;
    }
    if field == 0 {
        return Err("cut: fields start at 1".to_string());
    }
    Ok(lines
        .iter()
        .map(|l| {
            l.split(&delim)
                .nth(field - 1)
                .unwrap_or("")
                .to_string()
        })
        .collect())
}

fn tr_text(text: &str, from: &str, to: &str) -> String {
    let from: Vec<char> = from.chars().collect();
    let to: Vec<char> = to.chars().collect();
    text.chars()
        .map(|c| {
            if let Some(i) = from.iter().position(|x| *x == c) {
                *to.get(i).unwrap_or(&c)
            } else {
                c
            }
        })
        .collect()
}

fn maybe_lines(val: PipeVal, graph: &mut Vec<GraphNode>) -> PipeVal {
    match val {
        PipeVal::Text(_) => {
            graph.push(node("Lines"));
            PipeVal::Lines(into_lines(val))
        }
        other => other,
    }
}

fn into_lines(val: PipeVal) -> Vec<String> {
    match val {
        PipeVal::Lines(l) => l,
        PipeVal::Text(t) => t
            .lines()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        PipeVal::Nums(n) => n.iter().map(|x| fmt_num(*x)).collect(),
        PipeVal::Scalar(s) => alloc::vec![fmt_num(s)],
    }
}

fn into_nums(val: PipeVal) -> Result<Vec<f64>, String> {
    match val {
        PipeVal::Nums(n) => Ok(n),
        PipeVal::Scalar(s) => Ok(alloc::vec![s]),
        PipeVal::Text(t) => parse_nums(t.split_whitespace()),
        PipeVal::Lines(l) => parse_nums(l.iter().flat_map(|s| s.split_whitespace())),
    }
}

fn parse_nums<'a, I: IntoIterator<Item = &'a str>>(it: I) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for tok in it {
        if tok.is_empty() {
            continue;
        }
        match tok.parse::<f64>() {
            Ok(n) => out.push(n),
            Err(_) => return Err(format!("parse: not a number: {}", tok)),
        }
    }
    Ok(out)
}

pub fn format_val(val: &PipeVal) -> String {
    match val {
        PipeVal::Text(t) => t.clone(),
        PipeVal::Lines(l) => l.join("\n"),
        PipeVal::Nums(n) => {
            let mut out = String::new();
            for (i, x) in n.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&fmt_num(*x));
            }
            out
        }
        PipeVal::Scalar(s) => fmt_num(*s),
    }
}

fn fmt_num(x: f64) -> String {
    if x.is_finite() && x.abs() < 1e15 {
        let r = x as i64;
        if (x - r as f64).abs() < 1e-9 {
            return format!("{}", r);
        }
    }
    format!("{}", x)
}

fn join(args: &[String]) -> String {
    let mut out = String::new();
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(a);
    }
    out
}

pub fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn node(label: &str) -> GraphNode {
    GraphNode { label: label.to_string() }
}
