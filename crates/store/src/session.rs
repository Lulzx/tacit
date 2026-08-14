//! Bash-shaped surface over the namespace.  Commands are sugar for
//! tree/blob transforms, not Unix programs.

use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

use alloc::collections::BTreeMap;

use crate::{hash_hex, Kind, Look, Store, TreeCap, RIGHT_READ, RIGHT_WRITE};

/// A recorded transform so `graph` can show the last command as a value graph.
#[derive(Clone, Debug)]
pub struct GraphNode {
    pub label: String,
}

pub struct Session {
    pub store: Store,
    pub cwd: TreeCap,
    last_graph: Vec<GraphNode>,
    cmds: Vec<String>,
    aliases: BTreeMap<String, String>,
    dirstack: Vec<TreeCap>,
    env: BTreeMap<String, String>,
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
        Session {
            store,
            cwd,
            last_graph: Vec::new(),
            cmds: Vec::new(),
            aliases: BTreeMap::new(),
            dirstack: Vec::new(),
            env: BTreeMap::new(),
        }
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
        let tokens = expand_vars(tokenize(src)?, &self.env);
        if tokens.is_empty() {
            return Ok(Outcome::Handled(String::new()));
        }
        if tokens.iter().any(|t| t == "|") {
            self.cmds.push(src.to_string());
            return self.eval_pipeline(&tokens);
        }
        let cmd = tokens[0].clone();
        if let Some(exp) = self.aliases.get(&cmd).cloned() {
            let mut rest = tokenize(&exp)?;
            rest.extend(tokens[1..].iter().cloned());
            return self.eval(&rest.join(" "));
        }
        self.cmds.push(src.to_string());
        match cmd.as_str() {
            "pwd" => self.cmd_pwd(),
            "ls" => self.cmd_ls(&tokens[1..]),
            "cd" => self.cmd_cd(&tokens[1..]),
            "cat" | "less" | "more" => self.cmd_cat(&tokens[1..]),
            "echo" => self.cmd_echo(&tokens[1..]),
            "printf" => self.cmd_printf(&tokens[1..]),
            "mkdir" => self.cmd_mkdir(&tokens[1..]),
            "cp" | "ln" => self.cmd_cp(&tokens[1..]),
            "mv" => self.cmd_mv(&tokens[1..]),
            "rm" => self.cmd_rm(&tokens[1..]),
            "rmdir" => self.cmd_rmdir(&tokens[1..]),
            "touch" => self.cmd_touch(&tokens[1..]),
            "history" => self.cmd_history(&tokens[1..]),
            "undo" => self.cmd_undo(&tokens[1..]),
            "inspect" | "stat" | "file" => self.cmd_inspect(&tokens[1..]),
            "why" => self.cmd_why(&tokens[1..]),
            "explain" => self.cmd_explain(&tokens[1..]),
            "graph" => self.cmd_graph(),
            "help" | "man" => self.cmd_help(),
            "neofetch" | "fastfetch" | "screenfetch" => self.cmd_neofetch(),
            "clear" => Ok(Outcome::Handled("\x0c".to_string())),
            "head" => self.cmd_head_tail(&tokens[1..], true),
            "tail" => self.cmd_head_tail(&tokens[1..], false),
            "wc" => self.cmd_wc(&tokens[1..]),
            "nl" => self.cmd_nl(&tokens[1..]),
            "tac" => self.cmd_tac(&tokens[1..]),
            "cut" => self.cmd_cut(&tokens[1..]),
            "tr" => self.cmd_tr(&tokens[1..]),
            "tee" => self.cmd_tee(&tokens[1..]),
            "seq" => self.cmd_seq(&tokens[1..]),
            "tree" => self.cmd_tree(&tokens[1..]),
            "find" => self.cmd_find(&tokens[1..]),
            "du" => self.cmd_du(&tokens[1..]),
            "df" => self.cmd_df(),
            "basename" => self.cmd_basename(&tokens[1..]),
            "dirname" => self.cmd_dirname(&tokens[1..]),
            "realpath" | "readlink" => self.cmd_realpath(&tokens[1..]),
            "true" => Ok(Outcome::Handled(String::new())),
            "false" => Err("false".to_string()),
            "test" => self.cmd_test(&tokens[1..], false),
            "[" => self.cmd_test(&tokens[1..], true),
            "exit" | "logout" => Ok(Outcome::Handled(String::new())),
            "alias" => self.cmd_alias(&tokens[1..]),
            "unalias" => self.cmd_unalias(&tokens[1..]),
            "type" | "which" | "command" => self.cmd_type(&tokens[1..]),
            "pushd" => self.cmd_pushd(&tokens[1..]),
            "popd" => self.cmd_popd(),
            "dirs" => self.cmd_dirs(),
            "export" | "set" => self.cmd_export(&tokens[1..]),
            "unset" => self.cmd_unset(&tokens[1..]),
            "env" => self.cmd_env(),
            "date" => Ok(Outcome::Handled("clock is a capability; this shell is deterministic".to_string())),
            "whoami" => Ok(Outcome::Handled("shell".to_string())),
            "hostname" => Ok(Outcome::Handled("tacit".to_string())),
            "uname" => Ok(Outcome::Handled("Tacit array-transformation machine".to_string())),
            "chmod" | "chown" | "umask" => Err("authority is a capability, not a mode".to_string()),
            "sudo" => Err("no ambient authority".to_string()),
            "ps" | "top" | "kill" | "jobs" | "fg" | "bg" => {
                Err("no processes. try graph".to_string())
            }
            "bash" | "sh" => Err("this is the Tacit shell".to_string()),
            "vim" | "vi" | "nano" | "ed" => {
                Err("edit is a transform: echo text > name, or undo name".to_string())
            }
            "ssh" | "curl" | "wget" | "apt" | "git" => {
                Err("not a Tacit transform (no network, no packages)".to_string())
            }
            _ => {
                self.cmds.pop();
                let _ = cmd;
                Ok(Outcome::Unknown)
            }
        }
    }

    fn cmd_pwd(&mut self) -> Result<Outcome, String> {
        self.set_graph(&["CurrentTree", "RenderPath", "Display"]);
        Ok(Outcome::Handled(Store::pwd(&self.cwd)))
    }

    fn cmd_ls(&mut self, args: &[String]) -> Result<Outcome, String> {
        let (flags, rest) = take_flags(args);
        let long = flags.iter().any(|f| f.contains('l'));
        let spec = rest.first().map(String::as_str).unwrap_or(".");
        let cap = match self.store.look(&self.cwd, spec).map_err(err)? {
            Look::Tree(c) => c,
            Look::Blob { name, .. } => {
                self.set_graph(&["Children", "Display"]);
                return Ok(Outcome::Handled(name));
            }
        };
        self.set_graph(&["CurrentTree", "Children", "Display"]);
        let kids = self.store.list(&cap).map_err(err)?;
        let mut out = String::new();
        for (i, (name, kind, hash)) in kids.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            if long {
                let mark = match kind {
                    Kind::Tree => "tree",
                    Kind::Blob => "blob",
                };
                let size = match kind {
                    Kind::Blob => self.store.blob_len(*hash).unwrap_or(0),
                    Kind::Tree => 0,
                };
                let hx = hash_hex(*hash);
                out.push_str(&format!(
                    "{} {:>5} {} {}",
                    mark,
                    size,
                    core::str::from_utf8(&hx).unwrap_or("????????"),
                    name
                ));
            } else {
                out.push_str(name);
            }
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
        if args.is_empty() {
            return Err("usage: cat <name>…".to_string());
        }
        let mut out = String::new();
        for (i, spec) in args.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&self.read_text(spec)?);
        }
        self.set_graph(&[args[0].as_str(), "Read", "Display"]);
        Ok(Outcome::Handled(out))
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
        let (flags, rest) = take_flags(args);
        if rest.is_empty() {
            return Err("usage: mkdir [-p] <name>…".to_string());
        }
        let parents = flags.iter().any(|f| f.contains('p'));
        for spec in &rest {
            if parents || spec.contains('/') {
                self.store.mkdir_p(&mut self.cwd, spec).map_err(err)?;
            } else {
                self.store.mkdir(&mut self.cwd, spec).map_err(err)?;
                self.note(spec, &["EmptyTree", "Bind"], &[]);
            }
        }
        self.sync_cwd();
        self.set_graph(&["EmptyTree", "Bind"]);
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
        let (flags, rest) = take_flags(args);
        if rest.is_empty() {
            return Err("usage: rm [-r] <name>…".to_string());
        }
        let rec = flags.iter().any(|f| f.contains('r') || f.contains('R'));
        for spec in &rest {
            match self.store.look(&self.cwd, spec).map_err(err)? {
                Look::Tree(t) => {
                    if t.path.is_empty() {
                        return Err("cannot remove /".to_string());
                    }
                    if !rec {
                        return Err("is a tree (use rm -r)".to_string());
                    }
                    let name = t.path.last().unwrap().clone();
                    let mut parent = parent_cap(&self.store, &self.cwd, &t.path)?;
                    self.store.remove(&mut parent, &name).map_err(err)?;
                }
                Look::Blob { mut parent, name, .. } => {
                    self.store.remove(&mut parent, &name).map_err(err)?;
                }
            }
        }
        self.sync_cwd();
        self.set_graph(&["CurrentTree", "RemoveBinding"]);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_history(&mut self, args: &[String]) -> Result<Outcome, String> {
        if args.is_empty() {
            let mut out = String::new();
            for (i, c) in self.cmds.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                out.push_str(&format!("{:>4}  {}", i + 1, c));
            }
            self.set_graph(&["CmdLog", "Display"]);
            return Ok(Outcome::Handled(out));
        }
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
            "nav     pwd ls cd pushd popd dirs tree find du df\n\
             names   cat less more echo printf touch mkdir cp ln mv rm rmdir\n\
             path    basename dirname realpath readlink\n\
             text    head tail wc nl tac cut tr tee seq grep filter sort uniq unique\n\
             pipe    parse square sum lines rev reverse\n\
             meta    history undo inspect stat file why explain graph help man neofetch\n\
             shell   alias unalias type which command env export set unset\n\
             more    test true false clear date whoami hostname uname exit"
                .to_string(),
        ))
    }

    pub fn pwd(&self) -> String {
        Store::pwd(&self.cwd)
    }

    pub fn graph_text(&self) -> String {
        render_graph(&self.last_graph)
    }

    /// Completions for the last token of `line`.  Commands after `|`/`>`,
    /// names from the current tree otherwise.
    pub fn complete(&self, line: &str) -> Vec<String> {
        let (prefix, want_cmd) = complete_prefix(line);
        let mut out = Vec::new();
        if want_cmd {
            for c in COMMANDS {
                if c.starts_with(prefix) {
                    out.push((*c).to_string());
                }
            }
            for a in self.aliases.keys() {
                if a.starts_with(prefix) && !out.iter().any(|x| x == a) {
                    out.push(a.clone());
                }
            }
        }
        out.extend(self.complete_names(prefix));
        out.sort();
        out.dedup();
        out
    }

    fn complete_names(&self, prefix: &str) -> Vec<String> {
        let (dir, pre) = match prefix.rfind('/') {
            Some(i) => (&prefix[..=i], &prefix[i + 1..]),
            None => ("", prefix),
        };
        let cap = if dir.is_empty() {
            self.cwd.clone()
        } else {
            match self.store.look(&self.cwd, dir.trim_end_matches('/')) {
                Ok(Look::Tree(c)) => c,
                _ => return Vec::new(),
            }
        };
        let Ok(kids) = self.store.list(&cap) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (name, kind, _) in kids {
            if name.starts_with(pre) {
                let mut s = String::from(dir);
                s.push_str(&name);
                if kind == Kind::Tree {
                    s.push('/');
                }
                out.push(s);
            }
        }
        out
    }

    fn eval_pipeline(&mut self, tokens: &[String]) -> Result<Outcome, String> {
        let (out, graph, dest) = crate::pipe::run(&mut self.store, &mut self.cwd, tokens)?;
        self.last_graph = graph;
        if let Some(name) = dest {
            self.store
                .write_file(&mut self.cwd, &name, out.as_bytes(), "text/plain")
                .map_err(err)?;
            let labels: Vec<String> = self
                .last_graph
                .iter()
                .map(|n| n.label.clone())
                .filter(|l| l.as_str() != name && l.as_str() != "Bind")
                .collect();
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

    fn cmd_printf(&mut self, args: &[String]) -> Result<Outcome, String> {
        let text = crate::pipe::printf_fmt(args)?;
        self.set_graph(&["Printf", "Display"]);
        Ok(Outcome::Handled(text))
    }

    fn cmd_rmdir(&mut self, args: &[String]) -> Result<Outcome, String> {
        if args.is_empty() {
            return Err("usage: rmdir <name>…".to_string());
        }
        for spec in args {
            let (mut parent, name) = leaf_parent(&self.store, &self.cwd, spec)?;
            self.store.rmdir(&mut parent, &name).map_err(err)?;
        }
        self.sync_cwd();
        self.set_graph(&["RemoveBinding"]);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_touch(&mut self, args: &[String]) -> Result<Outcome, String> {
        if args.is_empty() {
            return Err("usage: touch <name>…".to_string());
        }
        for spec in args {
            let (mut parent, name) = leaf_parent(&self.store, &self.cwd, spec)?;
            self.store.touch(&mut parent, &name).map_err(err)?;
            self.note(&name, &["Touch", "Bind"], &[]);
        }
        self.sync_cwd();
        self.set_graph(&["Touch", "Bind"]);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_head_tail(&mut self, args: &[String], head: bool) -> Result<Outcome, String> {
        let n = crate::pipe::flag_n(args, 10)?;
        let spec = {
            let mut i = 0;
            let mut found = None;
            while i < args.len() {
                if args[i] == "-n" {
                    i += 2;
                    continue;
                }
                if args[i].starts_with('-') {
                    i += 1;
                    continue;
                }
                found = Some(args[i].as_str());
                break;
            }
            found.ok_or("usage: head [-n N] <name>")?
        };
        let text = self.read_text(spec)?;
        let mut lines: Vec<&str> = text.lines().collect();
        if head {
            lines.truncate(n);
        } else {
            let start = lines.len().saturating_sub(n);
            lines = lines[start..].to_vec();
        }
        self.set_graph(&[if head { "Head" } else { "Tail" }, "Display"]);
        Ok(Outcome::Handled(lines.join("\n")))
    }

    fn cmd_wc(&mut self, args: &[String]) -> Result<Outcome, String> {
        let (flags, rest) = take_flags(args);
        let spec = rest.first().ok_or("usage: wc [-lwc] <name>")?;
        let text = self.read_text(spec)?;
        let val = crate::pipe::PipeVal::Text(text);
        self.set_graph(&["Count", "Display"]);
        Ok(Outcome::Handled(crate::pipe::wc_text(&val, &flags)))
    }

    fn cmd_nl(&mut self, args: &[String]) -> Result<Outcome, String> {
        let spec = one_name(args, "nl")?;
        let text = self.read_text(spec)?;
        let out = text
            .lines()
            .enumerate()
            .map(|(i, l)| format!("{}\t{}", i + 1, l))
            .collect::<Vec<_>>()
            .join("\n");
        self.set_graph(&["NumberLines", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_tac(&mut self, args: &[String]) -> Result<Outcome, String> {
        let spec = one_name(args, "tac")?;
        let text = self.read_text(spec)?;
        let mut lines: Vec<&str> = text.lines().collect();
        lines.reverse();
        self.set_graph(&["Reverse", "Display"]);
        Ok(Outcome::Handled(lines.join("\n")))
    }

    fn cmd_cut(&mut self, args: &[String]) -> Result<Outcome, String> {
        let mut delim = "\t".to_string();
        let mut field = 1usize;
        let mut spec = None;
        let mut i = 0;
        while i < args.len() {
            if args[i] == "-d" {
                delim = args.get(i + 1).ok_or("usage: cut -d <delim> -f <n> <name>")?.clone();
                i += 2;
                continue;
            }
            if args[i] == "-f" {
                field = args
                    .get(i + 1)
                    .ok_or("usage: cut -d <delim> -f <n> <name>")?
                    .parse()
                    .map_err(|_| "bad field")?;
                i += 2;
                continue;
            }
            spec = Some(args[i].clone());
            i += 1;
        }
        let spec = spec.ok_or("usage: cut -d <delim> -f <n> <name>")?;
        if field == 0 {
            return Err("cut: fields start at 1".to_string());
        }
        let text = self.read_text(&spec)?;
        let out = text
            .lines()
            .map(|l| l.split(&delim).nth(field - 1).unwrap_or("").to_string())
            .collect::<Vec<_>>()
            .join("\n");
        self.set_graph(&["Cut", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_tr(&mut self, args: &[String]) -> Result<Outcome, String> {
        // tr from to  — needs stdin; require a name: tr from to <name> or use pipe
        if args.len() < 2 {
            return Err("usage: tr <from> <to>  (in a pipeline) or tr <from> <to> <name>".to_string());
        }
        if args.len() == 2 {
            return Err("tr needs input: cat f | tr a b".to_string());
        }
        let text = self.read_text(&args[2])?;
        let from: Vec<char> = args[0].chars().collect();
        let to: Vec<char> = args[1].chars().collect();
        let out: String = text
            .chars()
            .map(|c| {
                if let Some(i) = from.iter().position(|x| *x == c) {
                    *to.get(i).unwrap_or(&c)
                } else {
                    c
                }
            })
            .collect();
        self.set_graph(&["Translate", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_tee(&mut self, _args: &[String]) -> Result<Outcome, String> {
        Err("tee is a pipeline stage: cat f | tee copy".to_string())
    }

    fn cmd_seq(&mut self, args: &[String]) -> Result<Outcome, String> {
        let lines = crate::pipe::seq_lines(args)?;
        self.set_graph(&["Seq", "Display"]);
        Ok(Outcome::Handled(lines.join("\n")))
    }

    fn cmd_tree(&mut self, args: &[String]) -> Result<Outcome, String> {
        let spec = args.first().map(String::as_str).unwrap_or(".");
        let cap = match self.store.look(&self.cwd, spec).map_err(err)? {
            Look::Tree(c) => c,
            Look::Blob { name, .. } => return Ok(Outcome::Handled(name)),
        };
        let ents = self.store.walk(&cap).map_err(err)?;
        let mut out = String::from(".");
        for e in &ents {
            out.push('\n');
            let depth = e.path.bytes().filter(|b| *b == b'/').count();
            for _ in 0..depth {
                out.push_str("    ");
            }
            let leaf = e.path.rsplit('/').next().unwrap_or(&e.path);
            out.push_str(leaf);
            if e.kind == Kind::Tree {
                out.push('/');
            }
        }
        self.set_graph(&["Tree", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_find(&mut self, args: &[String]) -> Result<Outcome, String> {
        let spec = args.iter().find(|a| !a.starts_with('-')).map(String::as_str).unwrap_or(".");
        let cap = match self.store.look(&self.cwd, spec).map_err(err)? {
            Look::Tree(c) => c,
            Look::Blob { name, .. } => return Ok(Outcome::Handled(name)),
        };
        let pat = {
            let mut p = None;
            let mut i = 0;
            while i + 1 < args.len() {
                if args[i] == "-name" {
                    p = Some(args[i + 1].as_str());
                }
                i += 1;
            }
            p
        };
        let ents = self.store.walk(&cap).map_err(err)?;
        let mut out = String::new();
        for e in &ents {
            let leaf = e.path.rsplit('/').next().unwrap_or(&e.path);
            if let Some(p) = pat {
                if !glob_match(leaf, p) {
                    continue;
                }
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&e.path);
        }
        self.set_graph(&["Find", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_du(&mut self, args: &[String]) -> Result<Outcome, String> {
        let spec = args.first().map(String::as_str).unwrap_or(".");
        let cap = match self.store.look(&self.cwd, spec).map_err(err)? {
            Look::Tree(c) => c,
            Look::Blob { hash, name, .. } => {
                let n = self.store.blob_len(hash).unwrap_or(0);
                return Ok(Outcome::Handled(format!("{}\t{}", n, name)));
            }
        };
        let ents = self.store.walk(&cap).map_err(err)?;
        let mut total = 0usize;
        let mut out = String::new();
        for e in &ents {
            if e.kind == Kind::Blob {
                total += e.size;
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("{}\t{}", e.size, e.path));
            }
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("{}\t.", total));
        self.set_graph(&["Size", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_neofetch(&mut self) -> Result<Outcome, String> {
        let term_name = if cfg!(target_arch = "wasm32") {
            "xterm.js"
        } else if cfg!(target_os = "none") {
            "uart"
        } else {
            "ghostty"
        };
        let user = "shell";
        let host = "tacit";
        let title = format!("{user}@{host}");
        let bar: String = core::iter::repeat('-').take(title.len()).collect();
        let info = [
            format!("\x1b[1;32m{user}\x1b[0m@\x1b[1;32m{host}\x1b[0m"),
            format!("\x1b[2m{bar}\x1b[0m"),
            nf_kv("OS", "Tacit"),
            nf_kv("Host", "Mac16,8"),
            nf_kv("Kernel", "UIR"),
            nf_kv("Uptime", "this session"),
            nf_kv(
                "Packages",
                &format!("{} (store)", self.store.object_count()),
            ),
            nf_kv("Shell", "tacit"),
            nf_kv("Resolution", "1512x982"),
            nf_kv("DE", "Aqua"),
            nf_kv("WM", "Quartz Compositor"),
            nf_kv("WM Theme", "Blue (Light)"),
            nf_kv("Terminal", term_name),
            nf_kv("CPU", "Apple M4 Pro"),
            nf_kv("GPU", "Apple M4 Pro"),
            nf_kv(
                "Memory",
                &format!(
                    "{} / {} objects",
                    self.store.object_count(),
                    Store::object_limit()
                ),
            ),
        ];
        let pad = NF_LOGO.iter().map(|s| s.chars().count()).max().unwrap_or(0) + 4;
        let rows = NF_LOGO.len().max(info.len());
        let mut out = String::new();
        for i in 0..rows {
            if i > 0 {
                out.push('\n');
            }
            let raw = if i < NF_LOGO.len() { NF_LOGO[i] } else { "" };
            let mut left = String::from(raw);
            while left.chars().count() < pad {
                left.push(' ');
            }
            out.push_str("\x1b[32m");
            out.push_str(&left);
            out.push_str("\x1b[0m");
            if i < info.len() {
                out.push_str(&info[i]);
            }
        }
        self.set_graph(&["Machine", "Store", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_df(&mut self) -> Result<Outcome, String> {
        self.set_graph(&["Store", "Display"]);
        Ok(Outcome::Handled(format!(
            "{} / {} objects",
            self.store.object_count(),
            Store::object_limit()
        )))
    }

    fn cmd_basename(&mut self, args: &[String]) -> Result<Outcome, String> {
        let spec = one_name(args, "basename")?;
        let name = spec.rsplit('/').next().unwrap_or(spec);
        self.set_graph(&["Basename", "Display"]);
        Ok(Outcome::Handled(name.to_string()))
    }

    fn cmd_dirname(&mut self, args: &[String]) -> Result<Outcome, String> {
        let spec = one_name(args, "dirname")?;
        let out = match spec.rsplit_once('/') {
            Some(("", _)) => "/",
            Some((d, _)) => d,
            None => ".",
        };
        self.set_graph(&["Dirname", "Display"]);
        Ok(Outcome::Handled(out.to_string()))
    }

    fn cmd_realpath(&mut self, args: &[String]) -> Result<Outcome, String> {
        let spec = args.first().map(String::as_str).unwrap_or(".");
        let look = self.store.look(&self.cwd, spec).map_err(err)?;
        let path = match look {
            Look::Tree(c) => Store::pwd(&c),
            Look::Blob { parent, name, .. } => {
                let mut p = Store::pwd(&parent);
                if p != "/" {
                    p.push('/');
                }
                p.push_str(&name);
                p
            }
        };
        self.set_graph(&["RenderPath", "Display"]);
        Ok(Outcome::Handled(path))
    }

    fn cmd_test(&mut self, args: &[String], bracket: bool) -> Result<Outcome, String> {
        let args = if bracket {
            if args.last().map(String::as_str) != Some("]") {
                return Err("[: missing ]".to_string());
            }
            &args[..args.len() - 1]
        } else {
            args
        };
        let ok = match args {
            [op, name] if op == "-e" => self.store.look(&self.cwd, name).is_ok(),
            [op, name] if op == "-f" => matches!(self.store.look(&self.cwd, name), Ok(Look::Blob { .. })),
            [op, name] if op == "-d" => matches!(self.store.look(&self.cwd, name), Ok(Look::Tree(_))),
            [op, s] if op == "-z" => s.is_empty(),
            [op, s] if op == "-n" => !s.is_empty(),
            [a, op, b] if op == "=" || op == "==" => a == b,
            [a, op, b] if op == "!=" => a != b,
            _ => return Err("usage: test -e|-f|-d|-z|-n name  or  test a = b".to_string()),
        };
        if ok {
            Ok(Outcome::Handled(String::new()))
        } else {
            Err("false".to_string())
        }
    }

    fn cmd_alias(&mut self, args: &[String]) -> Result<Outcome, String> {
        if args.is_empty() {
            let mut out = String::new();
            for (k, v) in &self.aliases {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("alias {}='{}'", k, v));
            }
            return Ok(Outcome::Handled(out));
        }
        let spec = join_words(args);
        if let Some((name, val)) = spec.split_once('=') {
            let val = val.trim_matches('\'').trim_matches('"').to_string();
            self.aliases.insert(name.trim().to_string(), val);
            return Ok(Outcome::Handled(String::new()));
        }
        Err("usage: alias name=command".to_string())
    }

    fn cmd_unalias(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "unalias")?;
        self.aliases.remove(name);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_type(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "type")?;
        if let Some(exp) = self.aliases.get(name) {
            return Ok(Outcome::Handled(format!("{} is aliased to `{}`", name, exp)));
        }
        let meaning = command_meaning(name);
        Ok(Outcome::Handled(format!("{} is {}", name, meaning)))
    }

    fn cmd_pushd(&mut self, args: &[String]) -> Result<Outcome, String> {
        self.dirstack.push(self.cwd.clone());
        if !args.is_empty() {
            self.cwd = self.store.enter(&self.cwd, &args[0]).map_err(err)?;
        }
        self.cmd_dirs()
    }

    fn cmd_popd(&mut self) -> Result<Outcome, String> {
        self.cwd = self.dirstack.pop().ok_or("dir stack empty")?;
        self.cmd_dirs()
    }

    fn cmd_dirs(&mut self) -> Result<Outcome, String> {
        let mut out = Store::pwd(&self.cwd);
        for c in self.dirstack.iter().rev() {
            out.push(' ');
            out.push_str(&Store::pwd(c));
        }
        self.set_graph(&["DirStack", "Display"]);
        Ok(Outcome::Handled(out))
    }

    fn cmd_export(&mut self, args: &[String]) -> Result<Outcome, String> {
        if args.is_empty() {
            return self.cmd_env();
        }
        let spec = join_words(args);
        if let Some((k, v)) = spec.split_once('=') {
            self.env.insert(k.trim().to_string(), v.trim_matches('"').trim_matches('\'').to_string());
            return Ok(Outcome::Handled(String::new()));
        }
        Err("usage: export NAME=value".to_string())
    }

    fn cmd_unset(&mut self, args: &[String]) -> Result<Outcome, String> {
        let name = one_name(args, "unset")?;
        self.env.remove(name);
        Ok(Outcome::Handled(String::new()))
    }

    fn cmd_env(&mut self) -> Result<Outcome, String> {
        let mut out = String::new();
        for (k, v) in &self.env {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("{}={}", k, v));
        }
        Ok(Outcome::Handled(out))
    }

    fn sync_cwd(&mut self) {
        let _ = self.store.refresh(&mut self.cwd);
    }

    fn read_text(&self, spec: &str) -> Result<String, String> {
        match self.store.look(&self.cwd, spec).map_err(err)? {
            Look::Blob { parent, name, .. } => {
                let blob = self.store.read_file(&parent, &name).map_err(err)?;
                core::str::from_utf8(&blob.bytes)
                    .map(|s| s.to_string())
                    .map_err(|_| "not text".to_string())
            }
            Look::Tree(_) => Err("not a blob".to_string()),
        }
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

const NF_LOGO: &[&str] = &[
    r#"                    'c."#,
    r#"                 ,xNMM."#,
    r#"               .OMMMMo"#,
    r#"               OMMM0,"#,
    r#"     .;loddo:' loolloddol;."#,
    r#"   cKMMMMMMMMMMNWMMMMMMMMMM0:"#,
    r#" .KMMMMMMMMMMMMMMMMMMMMMMMWd."#,
    r#" XMMMMMMMMMMMMMMMMMMMMMMMX."#,
    r#";MMMMMMMMMMMMMMMMMMMMMMMM:"#,
    r#":MMMMMMMMMMMMMMMMMMMMMMMM:"#,
    r#".MMMMMMMMMMMMMMMMMMMMMMMMX."#,
    r#" kMMMMMMMMMMMMMMMMMMMMMMMMWd."#,
    r#" .XMMMMMMMMMMMMMMMMMMMMMMMMMMk"#,
    r#"  .XMMMMMMMMMMMMMMMMMMMMMMMMK."#,
    r#"    kMMMMMMMMMMMMMMMMMMMMMMd"#,
    r#"     ;KMMMMMMMWXXWMMMMMMMk."#,
    r#"       .cooc,.    .,coo:."#,
];

fn nf_kv(key: &str, val: &str) -> String {
    format!("\x1b[1;32m{key}:\x1b[0m {val}")
}

const COMMANDS: &[&str] = &[
    "pwd", "ls", "cd", "cat", "less", "more", "echo", "printf", "mkdir", "cp", "ln", "mv", "rm",
    "rmdir", "touch", "history", "undo", "inspect", "stat", "file", "why", "explain", "graph",
    "help", "man", "neofetch", "fastfetch", "screenfetch", "clear", "head", "tail", "wc", "nl",
    "tac", "cut", "tr", "tee", "seq", "tree", "find", "du", "df", "basename", "dirname",
    "realpath", "readlink", "true", "false", "test", "exit", "logout", "alias", "unalias", "type",
    "which", "command", "pushd", "popd", "dirs", "export", "set", "unset", "env", "date", "whoami",
    "hostname", "uname", "parse", "square", "sum", "grep", "filter", "sort", "uniq", "unique",
    "lines", "rev", "reverse",
];

fn complete_prefix(line: &str) -> (&str, bool) {
    let stage = match line.rfind(|c| c == '|' || c == '>') {
        Some(i) => &line[i + 1..],
        None => line,
    };
    let trimmed = stage.trim_start();
    let want_cmd = !trimmed.contains(char::is_whitespace);
    let prefix = match line.rsplit_once(char::is_whitespace) {
        Some((_, last)) => last,
        None => line,
    };
    let prefix = prefix.trim_start_matches(|c| c == '|' || c == '>');
    (prefix, want_cmd)
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

fn take_flags(args: &[String]) -> (Vec<String>, Vec<String>) {
    let mut flags = Vec::new();
    let mut rest = Vec::new();
    for a in args {
        if a.starts_with('-') && a.len() > 1 && !a.as_bytes()[1].is_ascii_digit() {
            flags.push(a.clone());
        } else {
            rest.push(a.clone());
        }
    }
    (flags, rest)
}

fn expand_vars(tokens: Vec<String>, env: &BTreeMap<String, String>) -> Vec<String> {
    tokens
        .into_iter()
        .map(|t| {
            if let Some(name) = t.strip_prefix('$') {
                if name.is_empty() {
                    t
                } else {
                    env.get(name).cloned().unwrap_or_default()
                }
            } else {
                t
            }
        })
        .collect()
}

fn parent_cap(store: &Store, cwd: &TreeCap, path: &[String]) -> Result<TreeCap, String> {
    if path.is_empty() {
        return Err("cannot remove /".to_string());
    }
    if path.len() == 1 {
        return store.enter(cwd, "/").map_err(err);
    }
    let mut spec = String::from("/");
    spec.push_str(&path[..path.len() - 1].join("/"));
    store.enter(cwd, &spec).map_err(err)
}

fn leaf_parent(store: &Store, cwd: &TreeCap, spec: &str) -> Result<(TreeCap, String), String> {
    match store.look(cwd, spec) {
        Ok(Look::Blob { parent, name, .. }) => Ok((parent, name)),
        Ok(Look::Tree(t)) => {
            let name = t.path.last().cloned().ok_or_else(|| "bad path".to_string())?;
            let parent = parent_cap(store, cwd, &t.path)?;
            Ok((parent, name))
        }
        Err(_) => {
            if let Some((dir, name)) = spec.rsplit_once('/') {
                match store.look(cwd, dir) {
                    Ok(Look::Tree(c)) => Ok((c, name.to_string())),
                    _ => Err("no such name".to_string()),
                }
            } else {
                Ok((cwd.clone(), spec.to_string()))
            }
        }
    }
}

fn glob_match(name: &str, pat: &str) -> bool {
    if pat == "*" {
        return true;
    }
    if let Some(suf) = pat.strip_prefix('*') {
        return name.ends_with(suf);
    }
    if let Some(pre) = pat.strip_suffix('*') {
        return name.starts_with(pre);
    }
    name == pat
}

fn command_meaning(name: &str) -> &'static str {
    match name {
        "ls" => "a projection of the current tree",
        "cd" => "a change of tree capability",
        "cat" | "less" | "more" => "resolve → value → display",
        "cp" | "ln" => "a new binding of the same value",
        "mv" => "a rebind",
        "rm" | "rmdir" => "remove a binding",
        "echo" | "printf" => "create a value",
        "mkdir" => "bind an empty tree",
        "touch" => "bind an empty blob",
        "graph" => "the last transform chain",
        "why" | "explain" => "provenance of a name",
        "neofetch" | "fastfetch" | "screenfetch" => "a projection of the machine and store",
        "pwd" => "the path projection of the current tree cap",
        "head" | "tail" | "wc" | "grep" | "sort" | "uniq" | "cut" | "tr" => {
            "a transform over a value"
        }
        "test" | "[" => "a predicate over names/values",
        _ => "not a known transform",
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
        match s.eval(line) {
            Ok(Outcome::Handled(t)) => t,
            Ok(Outcome::Unknown) => panic!("unknown: {line}"),
            Err(e) => panic!("{line} => {e}"),
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
        assert!(!w.contains("← total"), "{w}");
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

    #[test]
    fn top_bash_commands() {
        let mut s = Session::new();
        handled(&mut s, "touch a");
        assert_eq!(handled(&mut s, "cat a"), "");
        handled(&mut s, "echo \"one\\ntwo\\nthree\\nfour\" > nums");
        assert_eq!(handled(&mut s, "head -n 2 nums"), "one\ntwo");
        assert_eq!(handled(&mut s, "tail -n 1 nums"), "four");
        assert_eq!(handled(&mut s, "wc -l nums"), "4");
        assert_eq!(handled(&mut s, "seq 3"), "1\n2\n3");
        handled(&mut s, "mkdir -p p/q");
        assert_eq!(handled(&mut s, "realpath p/q"), "/p/q");
        assert_eq!(handled(&mut s, "basename p/q"), "q");
        assert_eq!(handled(&mut s, "dirname p/q"), "p");
        handled(&mut s, "export FOO=bar");
        assert_eq!(handled(&mut s, "echo $FOO"), "bar");
        assert_eq!(handled(&mut s, "test -f a"), "");
        assert!(s.eval("test -d a").is_err());
        handled(&mut s, "alias ll=ls");
        let t = handled(&mut s, "type ll");
        assert!(t.contains("aliased"), "{t}");
        let tr = handled(&mut s, "tree");
        assert!(tr.contains("p/") || tr.contains("q"), "{tr}");
        let df = handled(&mut s, "df");
        assert!(df.contains("objects"), "{df}");
        handled(&mut s, "echo \"a,b\\nc,d\" > t.csv");
        assert_eq!(handled(&mut s, "cut -d , -f 2 t.csv"), "b\nd");
        assert_eq!(handled(&mut s, "cat nums | tr o 0"), "0ne\ntw0\nthree\nf0ur");
    }

    #[test]
    fn neofetch_projects_machine_and_store() {
        let mut s = Session::new();
        let out = handled(&mut s, "neofetch");
        let plain: String = {
            let mut p = String::new();
            let mut it = out.chars().peekable();
            while let Some(c) = it.next() {
                if c == '\x1b' {
                    if it.peek() == Some(&'[') {
                        it.next();
                        while let Some(x) = it.next() {
                            if x.is_ascii_alphabetic() {
                                break;
                            }
                        }
                    }
                    continue;
                }
                p.push(c);
            }
            p
        };
        let cols: Vec<usize> = plain
            .lines()
            .filter_map(|line| line.find("OS:").or_else(|| line.find("shell@")))
            .collect();
        assert!(cols.len() >= 2, "{plain}");
        assert!(cols.windows(2).all(|w| w[0] == w[1]), "{plain}");
        assert!(out.contains("'c."), "{out}");
        assert!(out.contains(",xNMM."), "{out}");
        assert!(out.contains(".cooc,."), "{out}");
        assert!(out.contains("shell"), "{out}");
        assert!(out.contains("OS:"), "{out}");
        assert!(out.contains("Tacit"), "{out}");
        assert!(out.contains("Mac16,8"), "{out}");
        assert!(out.contains("Apple M4 Pro"), "{out}");
        assert!(out.contains("Quartz Compositor"), "{out}");
        assert!(handled(&mut s, "type neofetch").contains("projection"),);
    }

    #[test]
    fn complete_commands_and_names() {
        let mut s = Session::new();
        let cmds = s.complete("c");
        assert!(cmds.iter().any(|c| c == "cat"), "{cmds:?}");
        assert!(cmds.iter().any(|c| c == "cd"), "{cmds:?}");
        handled(&mut s, "echo hi > hello.txt");
        handled(&mut s, "mkdir home");
        let names = s.complete("cat h");
        assert!(names.iter().any(|c| c == "hello.txt"), "{names:?}");
        assert!(names.iter().any(|c| c == "home/"), "{names:?}");
        let pipe = s.complete("cat hello.txt | g");
        assert!(pipe.iter().any(|c| c == "grep"), "{pipe:?}");
        handled(&mut s, "cd home");
        handled(&mut s, "echo x > note.txt");
        handled(&mut s, "cd /");
        let nested = s.complete("cat home/n");
        assert!(nested.iter().any(|c| c == "home/note.txt"), "{nested:?}");
    }
}
