//! Content-addressed namespace: immutable values, mutable names.
//!
//! Four objects: Blob, Tree, Ref, Capability.  Data is never updated in
//! place — a write is `Transform(input_hash) → output_hash` plus an atomic
//! Ref update.  Paths are a human projection; authority is a
//! [`TreeCap`] (`object_id` + rights).
//!
//! No inodes, file descriptors, `open`, `seek`, or in-place mutation.

#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

pub mod session;
pub mod pipe;

pub type Hash = u64;

pub const RIGHT_READ: u8 = 1;
pub const RIGHT_WRITE: u8 = 2;

const OBJECT_LIMIT: usize = 64 * 1024;
const MAX_OBJECTS: usize = 256;
const MAX_NAME: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Blob,
    Tree,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Create,
    Replace,
    Remove,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub hash: Hash,
    pub kind: Kind,
}

#[derive(Clone, Debug)]
pub struct Blob {
    pub typ: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Tree {
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Clone, Debug)]
enum Object {
    Blob(Blob),
    Tree(Tree),
}

#[derive(Clone, Debug)]
pub struct HistEntry {
    pub action: Action,
    pub hash: Hash,
}

/// How a name was produced.  Stacked with history so undo pops both.
#[derive(Clone, Debug)]
pub struct Prov {
    pub produced_by: Vec<String>,
    pub inputs: Vec<String>,
}

/// Authority over a tree object.  The path is only a projection for `pwd`.
#[derive(Clone, Debug)]
pub struct TreeCap {
    pub object_id: Hash,
    pub rights: u8,
    pub path: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NotFound,
    NotTree,
    NotBlob,
    Exists,
    NeedRead,
    NeedWrite,
    BadName,
    EmptyName,
    Limit,
    NothingToUndo,
    NotEmpty,
}

pub struct Store {
    objects: BTreeMap<Hash, Object>,
    /// Absolute path → history of the name binding (last entry is current).
    refs: BTreeMap<String, Vec<HistEntry>>,
    /// Absolute path → production notes, aligned with `refs`.
    provs: BTreeMap<String, Vec<Prov>>,
    root: Hash,
}

impl Store {
    pub fn new() -> Self {
        let mut s = Store {
            objects: BTreeMap::new(),
            refs: BTreeMap::new(),
            provs: BTreeMap::new(),
            root: 0,
        };
        let root = s.insert(Object::Tree(Tree { entries: BTreeMap::new() })).expect("empty tree");
        s.root = root;
        s
    }

    pub fn root_hash(&self) -> Hash {
        self.root
    }

    pub fn root_cap(&self, rights: u8) -> TreeCap {
        TreeCap { object_id: self.root, rights, path: Vec::new() }
    }

    pub fn pwd(cap: &TreeCap) -> String {
        if cap.path.is_empty() {
            return "/".to_string();
        }
        let mut out = String::new();
        for p in &cap.path {
            out.push('/');
            out.push_str(p);
        }
        out
    }

    pub fn list(&self, cap: &TreeCap) -> Result<Vec<(String, Kind, Hash)>, Error> {
        self.require(cap, RIGHT_READ)?;
        let t = self.tree(cap.object_id)?;
        Ok(t.entries.iter().map(|(n, e)| (n.clone(), e.kind, e.hash)).collect())
    }

    pub fn enter(&self, cap: &TreeCap, spec: &str) -> Result<TreeCap, Error> {
        self.require(cap, RIGHT_READ)?;
        let path = join_path(&cap.path, spec)?;
        let id = self.resolve_tree(&path)?;
        Ok(TreeCap { object_id: id, rights: cap.rights, path })
    }

    pub fn mkdir(&mut self, cap: &mut TreeCap, name: &str) -> Result<Hash, Error> {
        self.require(cap, RIGHT_WRITE)?;
        check_name(name)?;
        if self.lookup(cap, name).is_ok() {
            return Err(Error::Exists);
        }
        let empty = self.insert(Object::Tree(Tree { entries: BTreeMap::new() }))?;
        self.bind(cap, name, empty, Kind::Tree, true)?;
        Ok(empty)
    }

    pub fn write_file(
        &mut self,
        cap: &mut TreeCap,
        name: &str,
        bytes: &[u8],
        typ: &str,
    ) -> Result<Hash, Error> {
        self.require(cap, RIGHT_WRITE)?;
        check_name(name)?;
        if let Ok((_, Kind::Tree)) = self.lookup(cap, name) {
            return Err(Error::Exists);
        }
        let id = self.put_blob(typ, bytes)?;
        self.bind(cap, name, id, Kind::Blob, true)?;
        Ok(id)
    }

    pub fn read_file(&self, cap: &TreeCap, name: &str) -> Result<&Blob, Error> {
        self.require(cap, RIGHT_READ)?;
        let (id, kind) = self.lookup(cap, name)?;
        if kind != Kind::Blob {
            return Err(Error::NotBlob);
        }
        match self.objects.get(&id) {
            Some(Object::Blob(b)) => Ok(b),
            _ => Err(Error::NotFound),
        }
    }

    pub fn remove(&mut self, cap: &mut TreeCap, name: &str) -> Result<(), Error> {
        self.require(cap, RIGHT_WRITE)?;
        check_name(name)?;
        let (id, _) = self.lookup(cap, name)?;
        self.unbind(cap, name)?;
        self.record(&abs_path(&cap.path, name), Action::Remove, id);
        Ok(())
    }

    pub fn copy(&mut self, cap: &mut TreeCap, src: &str, dst: &str) -> Result<Hash, Error> {
        self.require(cap, RIGHT_READ | RIGHT_WRITE)?;
        check_name(src)?;
        check_name(dst)?;
        if src == dst {
            let (id, _) = self.lookup(cap, src)?;
            return Ok(id);
        }
        if self.lookup(cap, dst).is_ok() {
            return Err(Error::Exists);
        }
        let (id, kind) = self.lookup(cap, src)?;
        self.bind(cap, dst, id, kind, true)?;
        Ok(id)
    }

    pub fn rename(&mut self, cap: &mut TreeCap, src: &str, dst: &str) -> Result<(), Error> {
        self.require(cap, RIGHT_WRITE)?;
        check_name(src)?;
        check_name(dst)?;
        if src == dst {
            return Ok(());
        }
        if self.lookup(cap, dst).is_ok() {
            return Err(Error::Exists);
        }
        let (id, kind) = self.lookup(cap, src)?;
        self.bind(cap, dst, id, kind, true)?;
        self.unbind(cap, src)?;
        self.record(&abs_path(&cap.path, src), Action::Remove, id);
        Ok(())
    }

    pub fn history(&self, cap: &TreeCap, name: &str) -> Result<&[HistEntry], Error> {
        self.require(cap, RIGHT_READ)?;
        check_name(name)?;
        let key = abs_path(&cap.path, name);
        match self.refs.get(&key) {
            Some(h) if !h.is_empty() => Ok(h.as_slice()),
            _ => Err(Error::NotFound),
        }
    }

    pub fn undo(&mut self, cap: &mut TreeCap, name: &str) -> Result<(), Error> {
        self.require(cap, RIGHT_WRITE)?;
        check_name(name)?;
        let key = abs_path(&cap.path, name);
        let hist = self.refs.get(&key).ok_or(Error::NotFound)?;
        if hist.is_empty() {
            return Err(Error::NothingToUndo);
        }
        if hist.len() == 1 {
            match hist[0].action {
                Action::Create => {
                    self.unbind(cap, name)?;
                    self.refs.remove(&key);
                    self.provs.remove(&key);
                    return Ok(());
                }
                Action::Remove => {
                    let id = hist[0].hash;
                    self.refs.remove(&key);
                    self.provs.remove(&key);
                    self.bind(cap, name, id, Kind::Blob, true)?;
                    return Ok(());
                }
                Action::Replace => return Err(Error::NothingToUndo),
            }
        }
        // Drop the last action and restore the previous binding.
        let hist = self.refs.get_mut(&key).ok_or(Error::NotFound)?;
        hist.pop();
        if let Some(p) = self.provs.get_mut(&key) {
            p.pop();
        }
        let prev = hist.last().cloned().ok_or(Error::NothingToUndo)?;
        match prev.action {
            Action::Remove => {
                self.unbind(cap, name)?;
            }
            Action::Create | Action::Replace => {
                self.bind(cap, name, prev.hash, Kind::Blob, false)?;
            }
        }
        Ok(())
    }

    pub fn inspect(&self, cap: &TreeCap, name: &str) -> Result<(Kind, Hash, usize, &str), Error> {
        self.require(cap, RIGHT_READ)?;
        let (id, kind) = self.lookup(cap, name)?;
        match kind {
            Kind::Blob => {
                let b = match self.objects.get(&id) {
                    Some(Object::Blob(b)) => b,
                    _ => return Err(Error::NotFound),
                };
                Ok((Kind::Blob, id, b.bytes.len(), b.typ.as_str()))
            }
            Kind::Tree => {
                let t = self.tree(id)?;
                Ok((Kind::Tree, id, t.entries.len(), "tree"))
            }
        }
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// Record how the latest binding of `name` was produced.
    pub fn note(
        &mut self,
        cap: &TreeCap,
        name: &str,
        produced_by: Vec<String>,
        inputs: Vec<String>,
    ) -> Result<(), Error> {
        self.require(cap, RIGHT_WRITE)?;
        check_name(name)?;
        let key = abs_path(&cap.path, name);
        self.provs.entry(key).or_default().push(Prov { produced_by, inputs });
        Ok(())
    }

    pub fn last_prov(&self, cap: &TreeCap, name: &str) -> Result<Option<&Prov>, Error> {
        self.require(cap, RIGHT_READ)?;
        check_name(name)?;
        let key = abs_path(&cap.path, name);
        Ok(self.provs.get(&key).and_then(|v| v.last()))
    }

    pub fn object_limit() -> usize {
        MAX_OBJECTS
    }

    pub fn look(&self, cap: &TreeCap, spec: &str) -> Result<Look, Error> {
        self.require(cap, RIGHT_READ)?;
        let path = join_path(&cap.path, spec)?;
        if path.is_empty() {
            return Ok(Look::Tree(TreeCap {
                object_id: self.root,
                rights: cap.rights,
                path: Vec::new(),
            }));
        }
        if let Ok(id) = self.resolve_tree(&path) {
            return Ok(Look::Tree(TreeCap { object_id: id, rights: cap.rights, path }));
        }
        let name = path[path.len() - 1].clone();
        let parent_path = path[..path.len() - 1].to_vec();
        let parent_id = self.resolve_tree(&parent_path)?;
        let parent = TreeCap { object_id: parent_id, rights: cap.rights, path: parent_path };
        let (hash, kind) = self.lookup(&parent, &name)?;
        match kind {
            Kind::Blob => Ok(Look::Blob { parent, name, hash }),
            Kind::Tree => Ok(Look::Tree(TreeCap {
                object_id: hash,
                rights: cap.rights,
                path: {
                    let mut p = parent.path;
                    p.push(name);
                    p
                },
            })),
        }
    }

    pub fn touch(&mut self, cap: &mut TreeCap, name: &str) -> Result<Hash, Error> {
        self.require(cap, RIGHT_WRITE)?;
        check_name(name)?;
        match self.lookup(cap, name) {
            Ok((id, Kind::Blob)) => Ok(id),
            Ok((_, Kind::Tree)) => Err(Error::Exists),
            Err(Error::NotFound) => self.write_file(cap, name, b"", "text/plain"),
            Err(e) => Err(e),
        }
    }

    pub fn rmdir(&mut self, cap: &mut TreeCap, name: &str) -> Result<(), Error> {
        self.require(cap, RIGHT_WRITE)?;
        check_name(name)?;
        let (id, kind) = self.lookup(cap, name)?;
        if kind != Kind::Tree {
            return Err(Error::NotTree);
        }
        if !self.tree(id)?.entries.is_empty() {
            return Err(Error::NotEmpty);
        }
        self.remove(cap, name)
    }

    pub fn mkdir_p(&mut self, cap: &mut TreeCap, spec: &str) -> Result<(), Error> {
        self.require(cap, RIGHT_WRITE)?;
        let path = join_path(&cap.path, spec)?;
        let mut cur = Vec::new();
        for name in &path {
            let parent_id = self.resolve_tree(&cur)?;
            let mut parent = TreeCap { object_id: parent_id, rights: cap.rights, path: cur.clone() };
            match self.lookup(&parent, name) {
                Ok((_, Kind::Tree)) => {}
                Ok((_, Kind::Blob)) => return Err(Error::Exists),
                Err(Error::NotFound) => {
                    self.mkdir(&mut parent, name)?;
                }
                Err(e) => return Err(e),
            }
            cur.push(name.clone());
        }
        Ok(())
    }

    pub fn walk(&self, cap: &TreeCap) -> Result<Vec<WalkEnt>, Error> {
        self.require(cap, RIGHT_READ)?;
        let mut out = Vec::new();
        self.walk_into(cap.object_id, "", &mut out)?;
        Ok(out)
    }

    fn walk_into(&self, id: Hash, prefix: &str, out: &mut Vec<WalkEnt>) -> Result<(), Error> {
        let t = self.tree(id)?;
        for (name, e) in &t.entries {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                let mut p = String::from(prefix);
                p.push('/');
                p.push_str(name);
                p
            };
            let size = match e.kind {
                Kind::Blob => match self.objects.get(&e.hash) {
                    Some(Object::Blob(b)) => b.bytes.len(),
                    _ => 0,
                },
                Kind::Tree => match self.objects.get(&e.hash) {
                    Some(Object::Tree(t)) => t.entries.len(),
                    _ => 0,
                },
            };
            out.push(WalkEnt { path: path.clone(), kind: e.kind, hash: e.hash, size });
            if e.kind == Kind::Tree {
                self.walk_into(e.hash, &path, out)?;
            }
        }
        Ok(())
    }

    pub fn refresh(&self, cap: &mut TreeCap) -> Result<(), Error> {
        cap.object_id = self.resolve_tree(&cap.path)?;
        Ok(())
    }

    pub fn blob_len(&self, id: Hash) -> Option<usize> {
        match self.objects.get(&id) {
            Some(Object::Blob(b)) => Some(b.bytes.len()),
            _ => None,
        }
    }
}

/// Result of resolving a path projection.
pub enum Look {
    Tree(TreeCap),
    Blob { parent: TreeCap, name: String, hash: Hash },
}

#[derive(Clone, Debug)]
pub struct WalkEnt {
    pub path: String,
    pub kind: Kind,
    pub hash: Hash,
    pub size: usize,
}

impl Store {
    fn require(&self, cap: &TreeCap, need: u8) -> Result<(), Error> {
        if cap.rights & RIGHT_READ == 0 && need & RIGHT_READ != 0 {
            return Err(Error::NeedRead);
        }
        if cap.rights & RIGHT_WRITE == 0 && need & RIGHT_WRITE != 0 {
            return Err(Error::NeedWrite);
        }
        // The path is a projection: the live object at that path must match
        // the capability's object_id, or the cap is stale / forged.
        let live = self.resolve_tree(&cap.path)?;
        if live != cap.object_id {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    fn tree(&self, id: Hash) -> Result<&Tree, Error> {
        match self.objects.get(&id) {
            Some(Object::Tree(t)) => Ok(t),
            Some(Object::Blob(_)) => Err(Error::NotTree),
            None => Err(Error::NotFound),
        }
    }

    fn resolve_tree(&self, path: &[String]) -> Result<Hash, Error> {
        let mut id = self.root;
        for name in path {
            let t = self.tree(id)?;
            match t.entries.get(name) {
                Some(e) if e.kind == Kind::Tree => id = e.hash,
                Some(_) => return Err(Error::NotTree),
                None => return Err(Error::NotFound),
            }
        }
        Ok(id)
    }

    fn lookup(&self, cap: &TreeCap, name: &str) -> Result<(Hash, Kind), Error> {
        let t = self.tree(cap.object_id)?;
        match t.entries.get(name) {
            Some(e) => Ok((e.hash, e.kind)),
            None => Err(Error::NotFound),
        }
    }

    fn put_blob(&mut self, typ: &str, bytes: &[u8]) -> Result<Hash, Error> {
        if bytes.len() > OBJECT_LIMIT {
            return Err(Error::Limit);
        }
        self.insert(Object::Blob(Blob { typ: typ.to_string(), bytes: bytes.to_vec() }))
    }

    fn insert(&mut self, obj: Object) -> Result<Hash, Error> {
        let id = hash_object(&obj);
        if self.objects.contains_key(&id) {
            return Ok(id);
        }
        if self.objects.len() >= MAX_OBJECTS {
            return Err(Error::Limit);
        }
        self.objects.insert(id, obj);
        Ok(id)
    }

    fn bind(
        &mut self,
        cap: &mut TreeCap,
        name: &str,
        id: Hash,
        kind: Kind,
        record: bool,
    ) -> Result<(), Error> {
        let existed = self.lookup(cap, name).is_ok();
        let mut child_id = id;
        let mut child_kind = kind;
        let full = {
            let mut p = cap.path.clone();
            p.push(name.to_string());
            p
        };
        for i in (0..full.len()).rev() {
            let parent_path = &full[..i];
            let bind_name = &full[i];
            let parent_id = self.resolve_tree(parent_path)?;
            let mut entries = self.tree(parent_id)?.entries.clone();
            entries.insert(bind_name.clone(), Entry { hash: child_id, kind: child_kind });
            child_id = self.insert(Object::Tree(Tree { entries }))?;
            child_kind = Kind::Tree;
        }
        self.root = child_id;
        cap.object_id = self.resolve_tree(&cap.path)?;
        if record {
            let action = if existed { Action::Replace } else { Action::Create };
            self.record(&abs_path(&cap.path, name), action, id);
        }
        Ok(())
    }

    fn unbind(&mut self, cap: &mut TreeCap, name: &str) -> Result<(), Error> {
        let _ = self.lookup(cap, name)?;
        let mut full = cap.path.clone();
        full.push(name.to_string());
        // Remove the leaf, then rewrite ancestors.
        let parent_path = &full[..full.len() - 1];
        let parent_id = self.resolve_tree(parent_path)?;
        let mut entries = self.tree(parent_id)?.entries.clone();
        entries.remove(name);
        let mut child_id = self.insert(Object::Tree(Tree { entries }))?;
        for i in (0..parent_path.len()).rev() {
            let anc = &parent_path[..i];
            let bind_name = &parent_path[i];
            let anc_id = self.resolve_tree(anc)?;
            let mut e = self.tree(anc_id)?.entries.clone();
            e.insert(bind_name.clone(), Entry { hash: child_id, kind: Kind::Tree });
            child_id = self.insert(Object::Tree(Tree { entries: e }))?;
        }
        self.root = child_id;
        cap.object_id = self.resolve_tree(&cap.path)?;
        Ok(())
    }

    fn record(&mut self, path: &str, action: Action, hash: Hash) {
        self.refs.entry(path.to_string()).or_default().push(HistEntry { action, hash });
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Error {
    pub fn as_str(self) -> &'static str {
        match self {
            Error::NotFound => "no such name",
            Error::NotTree => "not a tree",
            Error::NotBlob => "not a blob",
            Error::Exists => "name exists",
            Error::NeedRead => "need read",
            Error::NeedWrite => "need write",
            Error::BadName => "bad name",
            Error::EmptyName => "empty name",
            Error::Limit => "store limit",
            Error::NothingToUndo => "nothing to undo",
            Error::NotEmpty => "tree not empty",
        }
    }
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Create => "create",
            Action::Replace => "replace",
            Action::Remove => "remove",
        }
    }
}

pub fn hash_hex(id: Hash) -> [u8; 8] {
    const DIG: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 8];
    for i in 0..8 {
        out[7 - i] = DIG[((id >> (i * 4)) & 0xf) as usize];
    }
    out
}

pub fn hash_hex16(id: Hash) -> [u8; 16] {
    const DIG: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 16];
    for i in 0..16 {
        out[15 - i] = DIG[((id >> (i * 4)) & 0xf) as usize];
    }
    out
}

fn fnv1a(bytes: &[u8]) -> Hash {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn hash_object(obj: &Object) -> Hash {
    let mut buf = Vec::new();
    match obj {
        Object::Blob(b) => {
            buf.push(0);
            let tb = b.typ.as_bytes();
            buf.push(tb.len() as u8);
            buf.extend_from_slice(tb);
            let n = b.bytes.len() as u32;
            buf.extend_from_slice(&n.to_le_bytes());
            buf.extend_from_slice(&b.bytes);
        }
        Object::Tree(t) => {
            buf.push(1);
            let n = t.entries.len() as u32;
            buf.extend_from_slice(&n.to_le_bytes());
            for (name, e) in &t.entries {
                let nb = name.as_bytes();
                buf.push(nb.len() as u8);
                buf.extend_from_slice(nb);
                buf.extend_from_slice(&e.hash.to_le_bytes());
                buf.push(match e.kind {
                    Kind::Blob => 0,
                    Kind::Tree => 1,
                });
            }
        }
    }
    fnv1a(&buf)
}

fn check_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::EmptyName);
    }
    if name.len() > MAX_NAME || name == "." || name == ".." || name.contains('/') {
        return Err(Error::BadName);
    }
    Ok(())
}

fn abs_path(cwd: &[String], name: &str) -> String {
    if cwd.is_empty() {
        let mut s = String::from("/");
        s.push_str(name);
        return s;
    }
    let mut s = String::new();
    for p in cwd {
        s.push('/');
        s.push_str(p);
    }
    s.push('/');
    s.push_str(name);
    s
}

fn join_path(cwd: &[String], spec: &str) -> Result<Vec<String>, Error> {
    if spec.is_empty() {
        return Ok(Vec::new());
    }
    let mut path = if spec.starts_with('/') { Vec::new() } else { cwd.to_vec() };
    for part in spec.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            path.pop();
            continue;
        }
        check_name(part)?;
        path.push(part.to_string());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_pwd() {
        let s = Store::new();
        let cap = s.root_cap(RIGHT_READ | RIGHT_WRITE);
        assert_eq!(Store::pwd(&cap), "/");
        assert!(s.list(&cap).unwrap().is_empty());
    }

    #[test]
    fn same_bytes_same_hash() {
        let mut s = Store::new();
        let mut cap = s.root_cap(RIGHT_READ | RIGHT_WRITE);
        let a = s.write_file(&mut cap, "a.txt", b"hello", "text/plain").unwrap();
        let b = s.write_file(&mut cap, "b.txt", b"hello", "text/plain").unwrap();
        assert_eq!(a, b);
        // two names, one blob object plus the trees
        assert!(s.object_count() <= 4);
    }

    #[test]
    fn copy_is_rebind() {
        let mut s = Store::new();
        let mut cap = s.root_cap(RIGHT_READ | RIGHT_WRITE);
        let a = s.write_file(&mut cap, "a.txt", b"x", "text/plain").unwrap();
        let b = s.copy(&mut cap, "a.txt", "c.txt").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rights_are_checked() {
        let mut s = Store::new();
        let mut ro = s.root_cap(RIGHT_READ);
        assert_eq!(s.mkdir(&mut ro, "x").unwrap_err(), Error::NeedWrite);
        let rw = s.root_cap(RIGHT_READ | RIGHT_WRITE);
        assert!(s.list(&rw).is_ok());
    }

    #[test]
    fn first_milestone_session() {
        let mut s = Store::new();
        let mut cwd = s.root_cap(RIGHT_READ | RIGHT_WRITE);
        assert_eq!(Store::pwd(&cwd), "/");

        s.mkdir(&mut cwd, "home").unwrap();
        cwd = s.enter(&cwd, "home").unwrap();
        assert_eq!(Store::pwd(&cwd), "/home");

        s.write_file(&mut cwd, "hello.txt", b"hello tacit", "text/plain").unwrap();
        let names = s.list(&cwd).unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].0, "hello.txt");
        assert_eq!(s.read_file(&cwd, "hello.txt").unwrap().bytes, b"hello tacit");

        let h1 = s.history(&cwd, "hello.txt").unwrap();
        assert_eq!(h1.len(), 1);
        assert_eq!(h1[0].action, Action::Create);

        s.write_file(&mut cwd, "hello.txt", b"hello world", "text/plain").unwrap();
        let h2 = s.history(&cwd, "hello.txt").unwrap();
        assert_eq!(h2.len(), 2);
        assert_eq!(h2[0].action, Action::Create);
        assert_eq!(h2[1].action, Action::Replace);
        assert_eq!(s.read_file(&cwd, "hello.txt").unwrap().bytes, b"hello world");

        s.undo(&mut cwd, "hello.txt").unwrap();
        assert_eq!(s.read_file(&cwd, "hello.txt").unwrap().bytes, b"hello tacit");
        let h3 = s.history(&cwd, "hello.txt").unwrap();
        assert_eq!(h3.len(), 1);
        assert_eq!(h3[0].action, Action::Create);
    }

    #[test]
    fn cd_dotdot_and_absolute() {
        let mut s = Store::new();
        let mut cwd = s.root_cap(RIGHT_READ | RIGHT_WRITE);
        s.mkdir(&mut cwd, "home").unwrap();
        cwd = s.enter(&cwd, "home").unwrap();
        s.mkdir(&mut cwd, "notes").unwrap();
        cwd = s.enter(&cwd, "notes").unwrap();
        assert_eq!(Store::pwd(&cwd), "/home/notes");
        cwd = s.enter(&cwd, "..").unwrap();
        assert_eq!(Store::pwd(&cwd), "/home");
        cwd = s.enter(&cwd, "/").unwrap();
        assert_eq!(Store::pwd(&cwd), "/");
    }

    #[test]
    fn undo_create_unbinds() {
        let mut s = Store::new();
        let mut cwd = s.root_cap(RIGHT_READ | RIGHT_WRITE);
        s.write_file(&mut cwd, "a.txt", b"x", "text/plain").unwrap();
        s.undo(&mut cwd, "a.txt").unwrap();
        assert_eq!(s.read_file(&cwd, "a.txt").unwrap_err(), Error::NotFound);
    }
}
