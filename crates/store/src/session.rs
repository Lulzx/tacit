//! Bash-shaped surface over the namespace.  Commands are sugar for
//! tree/blob transforms, not Unix programs.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::{hash_hex, Kind, Store, TreeCap, RIGHT_READ, RIGHT_WRITE};

/// A recorded transform so `graph` can show the last command as a value graph.
#[derive(Clone, Debug)]
pub struct GraphNode {
    pub label: String,
}

pub struct Session {
    pub store: Store,
    pub cwd: TreeCap,
    last_graph: Vec<GraphNode>,
}

pub enum Outcome {
    /// Command handled.  Empty string means no output (mkdir, cd, …).
    Handled(String),
    /// Not a store command; the guest may try Uiua.
    Unknown,
}

impl Session {
    pub fn new() -> Self {
        let store = Store::new();
        let cwd = store.root_cap(RIGHT_READ | RIGHT_WRITE);
        Session { store, cwd, last_graph: Vec::new() }
    }

    /// Seed `/system` as an empty tree.  `/home` is created by the user.
    pub fn seed_system(&mut self) -> Result<(), crate::Error> {
        self.store.mkdir(&mut self.cwd, "system")?;
        Ok(())
    }

    pub fn eval(&mut self, line: &str) -> Result<Outcome, String> {
        let src = line.trim();
        if src.is_empty() || src.starts_with('#') {
            return Ok(Outcome::Handled(String::new()));
        }
        let tokens = tokenize(src)?;
        if tokens.is_empty() {
            return Ok(Outcome::Handled(String::new()));
        }
        if tokens.iter().any(|t| t == "|") {
            return self.eval_pipeline(&tokens);
        }
        let cmd = tokens[0].as_str();
        match cmd {
            "pwd" => self.cmd_pwd(),
            "ls" => self.cmd_ls(&tokens[1..]),
            "cd" => self.cmd_cd(&tokens[1..]),
            "cat" => self.cmd_cat(&tokens[1..]),
            "echo" => self.cmd_echo(&tokens[1..]),
            "mkdir" => self.cmd_mkdir(&tokens[1..]),
            "cp" => self.cmd_cp(&tokens[1..]),
            "mv" => self.cmd_mv(&tokens[1..]),
            "rm" => self.cmd_rm(&tokens[1..]),
            "history" => self.cmd_history(&tokens[1..]),
            "undo" => self.cmd_undo(&tokens[1..]),
            "inspect" => self.cmd_inspect(&tokens[1..]),
            "why" => self.cmd_why(&tokens[1..]),
            "explain" => self.cmd_explain(&tokens[1..]),
            "graph" => self.cmd_graph(),
            "help" => self.cmd_help(),
            "clear" => Ok(Outcome::Handled("\x0c".to_string())),
            _ => Ok(Outcome::Unknown),
        }
    }

    fn cmd_pwd(&mut self) -> Result<Outcome, String> {
        self.set_graph(&["CurrentTree", "RenderPath", "Display"]);
        Ok(Outcome::Handled(Store::pwd(&self.cwd)))
    }

    fn cmd_ls(&mut self, args: &[String]) -> Result<Outcome, String> {
        if !args.is_empty() {
            return Err("usage: ls".to_string());
        }
        self.set_graph(&["CurrentTree", "Children", "Display"]);
        let kids = self.store.list(&self.cwd).map_err(err)?;
        let mut out = String::new();
        for (i, (name, _, _)) in kids.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(name);
        }
        Ok(Outcome::Handled(out))
    }

    fn cmd_cd(&mut self, args: &[String]) -> Result<Outcome, String> {
        let spec = match args {
            [] => "/",
            [one] => one.as_str(),
            _ => return Err("usage: cd [path]".to_string()),
        };
        self.cwd = self.store.enter(&self.cwd, spec).map_err(err)?;
        self.set_graph(&["CurrentTree", "Enter", "Display"]);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_cat(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "cat")?;
        let (text, nbytes) = {
            let blob = self.store.read_file(&self.cwd, name).map_err(err)?;
            match core::str::from_utf8(&blob.bytes) {
                Ok(s) => (s.to_string(), blob.bytes.len()),
                Err(_) => (String::new(), blob.bytes.len()),
            }
        };
        self.set_graph(&[name, "Read", "Display"]);
        if text.is_empty() && nbytes > 0 {
            Ok(Outcome::Handled(format!("<{} bytes>", nbytes)))
        } else {
            Ok(Outcome::Handled(text))
        }
    }

    fn cmd_echo(&mut self, args: &[String]) -> Result<Outcome, String> {
        if args.iter().any(|a| a == ">>") {
            return Err(">> is in-place mutation; write a new value instead".to_string());
        }
        if let Some(i) = args.iter().position(|a| a == ">") {
            if i + 1 >= args.len() || i + 2 != args.len() {
                return Err("usage: echo [text] > name".to_string());
            }
            let name = args[i + 1].as_str();
            let text = crate::pipe::unescape(&join_words(&args[..i]));
            self.store
                .write_file(&mut self.cwd, name, text.as_bytes(), "text/plain")
                .map_err(err)?;
            self.note(name, &["Value", "Bind"], &[]);
            self.set_graph(&["Value", "Bind", name]);
            return Ok(Outcome::Handled(String::new()));
        }
        self.set_graph(&["Value", "Display"]);
        Ok(Outcome::Handled(crate::pipe::unescape(&join_words(args))))
    }

    fn cmd_mkdir(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "mkdir")?;
        self.store.mkdir(&mut self.cwd, name).map_err(err)?;
        self.note(name, &["EmptyTree", "Bind"], &[]);
        self.set_graph(&["EmptyTree", "Bind", name]);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_cp(&mut self, args: &[String]) -> Result<Outcome, String> {
        let (src, dst) = two_names(args, "cp")?;
        self.store.copy(&mut self.cwd, src, dst).map_err(err)?;
        self.note(dst, &["Bind"], &[src]);
        self.set_graph(&[src, "Bind", dst]);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_mv(&mut self, args: &[String]) -> Result<Outcome, String> {
        let (src, dst) = two_names(args, "mv")?;
        self.store.rename(&mut self.cwd, src, dst).map_err(err)?;
        self.note(dst, &["Rebind"], &[src]);
        self.set_graph(&[src, "Rebind", dst]);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_rm(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "rm")?;
        self.store.remove(&mut self.cwd, name).map_err(err)?;
        self.set_graph(&["CurrentTree", "RemoveBinding", name]);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_history(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "history")?;
        let hist = self.store.history(&self.cwd, name).map_err(err)?;
        let mut out = String::new();
        for (i, e) in hist.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&format!("{}", i + 1));
            out.push_str("  ");
            out.push_str(e.action.as_str());
            out.push_str("  ");
            let hx = hash_hex(e.hash);
            out.push_str(core::str::from_utf8(&hx).unwrap_or("????????"));
        }
        self.set_graph(&[name, "History", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_undo(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "undo")?;
        self.store.undo(&mut self.cwd, name).map_err(err)?;
        self.set_graph(&[name, "RestorePrev", "Bind"]);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_inspect(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "inspect")?;
        let (kind, id, size, typ) = self.store.inspect(&self.cwd, name).map_err(err)?;
        let hx = hash_hex(id);
        let hash = core::str::from_utf8(&hx).unwrap_or("????????");
        let kind_s = match kind {
            Kind::Blob => "blob",
            Kind::Tree => "tree",
        };
        let out = format!("type: {}\nkind: {}\nsize: {}\nhash: {}", typ, kind_s, size, hash);
        self.set_graph(&[name, "Inspect", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_graph(&mut self) -> Result<Outcome, String> {
        Ok(Outcome::Handled(render_graph(&self.last_graph)))
    }

    fn cmd_help(&mut self) -> Result<Outcome, String> {
        Ok(Outcome::Handled(
            "pwd ls cd cat echo mkdir cp mv rm history undo inspect why explain graph help clear\n\
             names bind immutable values; undo repoints a name\n\
             | is a transform graph: cat n | parse | square | sum\n\
             why / explain print how a name was produced\n\
             Uiua lines still compile"
                .to_string(),
        ))
    }

    pub fn pwd(&self) -> String {
        Store::pwd(&self.cwd)
    }

    pub fn graph_text(&self) -> String {
        render_graph(&self.last_graph)
    }

    fn eval_pipeline(&mut self, tokens: &[String]) -> Result<Outcome, String> {
        let (out, graph, dest) = crate::pipe::run(&self.store, &self.cwd, tokens)?;
        self.last_graph = graph;
        if let Some(name) = dest {
            self.store
                .write_file(&mut self.cwd, &name, out.as_bytes(), "text/plain")
                .map_err(err)?;
            let labels: Vec<String> = self.last_graph.iter().map(|n| n.label.clone()).collect();
            let inputs = pipeline_inputs(tokens);
            let _ = self.store.note(&self.cwd, &name, labels, inputs);
            return Ok(Outcome::Handled(String::new()));
        }
        Ok(Outcome::Handled(out))
    }

    fn note(&mut self, name: &str, produced_by: &[&str], inputs: &[&str]) {
        let _ = self.store.note(
            &self.cwd,
            name,
            produced_by.iter().map(|s| (*s).to_string()).collect(),
            inputs.iter().map(|s| (*s).to_string()).collect(),
        );
    }

    fn cmd_why(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "why")?;
        let _ = self.store.inspect(&self.cwd, name).map_err(err)?;
        let mut out = String::new();
        let mut seen = alloc::collections::BTreeSet::new();
        self.render_why(&mut out, name, &mut seen, 0);
        self.set_graph(&[name, "Why", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn render_why(
        &self,
        out: &mut String,
        name: &str,
        seen: &mut alloc::collections::BTreeSet<String>,
        indent: usize,
    ) {
        if !seen.insert(name.to_string()) {
            return;
        }
        let pad = " ".repeat(indent);
        if indent == 0 {
            out.push_str(name);
        }
        if let Ok(Some(p)) = self.store.last_prov(&self.cwd, name) {
            let produced = p.produced_by.clone();
            let inputs = p.inputs.clone();
            for step in produced.iter().rev() {
                out.push('\n');
                out.push_str(&pad);
                out.push_str("← ");
                out.push_str(step);
            }
            for inp in &inputs {
                if produced.iter().any(|s| s == inp) {
                    out.push('\n');
                    self.render_why(out, inp, seen, indent + 2);
                    continue;
                }
                out.push('\n');
                out.push_str(&pad);
                out.push_str("← ");
                out.push_str(inp);
                out.push('\n');
                self.render_why(out, inp, seen, indent + 2);
            }
            return;
        }
        if let Ok(h) = self.store.history(&self.cwd, name) {
            if let Some(last) = h.last() {
                out.push('\n');
                out.push_str(&pad);
                out.push_str("← ");
                out.push_str(last.action.as_str());
                out.push(' ');
                let hx = hash_hex(last.hash);
                out.push_str(core::str::from_utf8(&hx).unwrap_or("????????"));
            }
        }
    }

    fn cmd_explain(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "explain")?;
        let (kind, id, size, typ) = self.store.inspect(&self.cwd, name).map_err(err)?;
        let hx = hash_hex(id);
        let hash = core::str::from_utf8(&hx).unwrap_or("????????");
        let kind_s = match kind {
            Kind::Blob => "blob",
            Kind::Tree => "tree",
        };
        let mut out = String::new();
        out.push_str(name);
        out.push('\n');
        if let Ok(Some(p)) = self.store.last_prov(&self.cwd, name) {
            out.push_str("\nproduced by:\n");
            out.push_str(&render_graph(
                &p.produced_by
                    .iter()
                    .map(|s| GraphNode { label: s.clone() })
                    .collect::<Vec<_>>(),
            ));
            out.push_str("\n\ninputs:\n");
            if p.inputs.is_empty() {
                out.push_str("  (none)\n");
            } else {
                for inp in &p.inputs {
                    out.push_str("  ");
                    out.push_str(inp);
                    out.push('\n');
                }
            }
        } else {
            out.push('\n');
        }
        out.push_str(&format!(
            "\ntype: {}\nkind: {}\nsize: {}\nhash: {}\n",
            typ, kind_s, size, hash
        ));
        out.push_str("\nauthority:\n");
        if self.cwd.rights & RIGHT_READ != 0 {
            out.push_str("  read\n");
        }
        if self.cwd.rights & RIGHT_WRITE != 0 {
            out.push_str("  write\n");
        }
        out.push_str("\nreproducible: yes");
        self.set_graph(&[name, "Explain", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn set_graph(&mut self, labels: &[&str]) {
        self.last_graph = labels
            .iter()
            .map(|s| GraphNode { label: (*s).to_string() })
            .collect();
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

fn render_graph(nodes: &[GraphNode]) -> String {
    if nodes.is_empty() {
        return "no pipeline".to_string();
    }
    let mut out = String::new();
    for (i, n) in nodes.iter().enumerate() {
        if i == 0 {
            out.push_str(&n.label);
        } else {
            out.push_str("\n  |\n ");
            out.push_str(&n.label);
        }
    }
    out
}

fn err(e: crate::Error) -> String {
    e.as_str().to_string()
}

fn one_name<'a>(args: &'a [String], cmd: &str) -> Result<&'a str, String> {
    match args {
        [one] => Ok(one.as_str()),
        _ => Err(format!("usage: {} <name>", cmd)),
    }
}

fn two_names<'a>(args: &'a [String], cmd: &str) -> Result<(&'a str, &'a str), String> {
    match args {
        [a, b] => Ok((a.as_str(), b.as_str())),
        _ => Err(format!("usage: {} <src> <dst>", cmd)),
    }
}

fn pipeline_inputs(tokens: &[String]) -> Vec<String> {
    let mut stage = Vec::new();
    for t in tokens {
        if t == "|" || t == ">" {
            break;
        }
        stage.push(t.clone());
    }
    match stage.first().map(String::as_str) {
        Some("cat") if stage.len() == 2 => alloc::vec![stage[1].clone()],
        _ => Vec::new(),
    }
}

fn join_words(args: &[String]) -> String {
    let mut out = String::new();
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(a);
    }
    out
}

fn tokenize(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                cur.push(c);
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '|' => {
                if !cur.is_empty() {
                    out.push(core::mem::take(&mut cur));
                }
                out.push("|".to_string());
            }
            '>' => {
                if !cur.is_empty() {
                    out.push(core::mem::take(&mut cur));
                }
                if chars.peek() == Some(&'>') {
                    chars.next();
                    out.push(">>".to_string());
                } else {
                    out.push(">".to_string());
                }
            }
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(core::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if quote.is_some() {
        return Err("unterminated quote".to_string());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handled(s: &mut Session, line: &str) -> String {
        match s.eval(line).unwrap() {
            Outcome::Handled(t) => t,
            Outcome::Unknown => panic!("unknown: {line}"),
        }
    }

    #[test]
    fn milestone_transcript() {
        let mut s = Session::new();
        assert_eq!(handled(&mut s, "pwd"), "/");
        assert_eq!(handled(&mut s, "mkdir home"), "");
        assert_eq!(handled(&mut s, "cd home"), "");
        assert_eq!(handled(&mut s, "echo \"hello tacit\" > hello.txt"), "");
        assert_eq!(handled(&mut s, "ls"), "hello.txt");
        assert_eq!(handled(&mut s, "cat hello.txt"), "hello tacit");

        let h1 = handled(&mut s, "history hello.txt");
        assert!(h1.starts_with("1  create  "), "{h1}");

        assert_eq!(handled(&mut s, "echo \"hello world\" > hello.txt"), "");
        let h2 = handled(&mut s, "history hello.txt");
        let lines: Vec<&str> = h2.lines().collect();
        assert_eq!(lines.len(), 2, "{h2}");
        assert!(lines[0].starts_with("1  create  "));
        assert!(lines[1].starts_with("2  replace  "));

        assert_eq!(handled(&mut s, "undo hello.txt"), "");
        assert_eq!(handled(&mut s, "cat hello.txt"), "hello tacit");

        let g = handled(&mut s, "graph");
        assert!(g.contains("hello.txt") || g.contains("RestorePrev"), "{g}");
    }

    #[test]
    fn echo_without_redirect_prints() {
        let mut s = Session::new();
        assert_eq!(handled(&mut s, "echo hello tacit"), "hello tacit");
    }

    #[test]
    fn tokenize_redirect_tight() {
        let t = tokenize("echo hi>x").unwrap();
        assert_eq!(t, ["echo", "hi", ">", "x"]);
    }

    #[test]
    fn cp_then_cat() {
        let mut s = Session::new();
        handled(&mut s, "echo hi > a");
        handled(&mut s, "cp a b");
        assert_eq!(handled(&mut s, "cat b"), "hi");
    }

    #[test]
    fn pipeline_parse_square_sum() {
        let mut s = Session::new();
        handled(&mut s, "echo \"1 2 3 4\" > numbers");
        assert_eq!(handled(&mut s, "cat numbers | parse | square | sum"), "30");
        let g = handled(&mut s, "graph");
        assert!(g.contains("numbers"), "{g}");
        assert!(g.contains("Parse"), "{g}");
        assert!(g.contains("Square"), "{g}");
        assert!(g.contains("Sum"), "{g}");
    }

    #[test]
    fn pipeline_grep_sort_uniq() {
        let mut s = Session::new();
        handled(&mut s, "echo \"ok\\nerror b\\nerror a\\nerror b\\nok\" > log.txt");
        let out = handled(&mut s, "cat log.txt | grep error | sort | uniq");
        assert_eq!(out, "error a\nerror b");
        let g = handled(&mut s, "graph");
        assert!(g.contains("Filter(error)"), "{g}");
        assert!(g.contains("Sort"), "{g}");
        assert!(g.contains("Unique"), "{g}");
    }

    #[test]
    fn pipeline_bind() {
        let mut s = Session::new();
        handled(&mut s, "echo \"1 2 3 4\" > numbers");
        handled(&mut s, "cat numbers | parse | square | sum > total");
        assert_eq!(handled(&mut s, "cat total"), "30");
    }

    #[test]
    fn why_follows_production() {
        let mut s = Session::new();
        handled(&mut s, "echo \"1 2 3 4\" > numbers");
        handled(&mut s, "cat numbers | parse | square | sum > total");
        let w = handled(&mut s, "why total");
        assert!(w.starts_with("total"), "{w}");
        assert!(w.contains("← Sum"), "{w}");
        assert!(w.contains("← Parse"), "{w}");
        assert!(w.contains("← numbers") || w.contains("numbers"), "{w}");
        assert!(w.contains("← Value") || w.contains("← Bind"), "{w}");
    }

    #[test]
    fn explain_names_inputs_and_authority() {
        let mut s = Session::new();
        handled(&mut s, "echo hi > a");
        let e = handled(&mut s, "explain a");
        assert!(e.contains("produced by:"), "{e}");
        assert!(e.contains("Bind"), "{e}");
        assert!(e.contains("reproducible: yes"), "{e}");
        assert!(e.contains("write"), "{e}");
        assert!(e.contains("type: text/plain"), "{e}");
    }
}
