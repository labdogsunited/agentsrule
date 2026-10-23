//! The library of the node editor (`proposals/reusable-operators.md` §2, §4): templates (rules with bindings and controls,
//! no link to where they came from) and operator definitions, each version kept as it was published. Kept in
//! `<state dir>/library/` when the server has a state directory (`templates/NAME.json`, `operators/NAME@VERSION.em`), and
//! in memory otherwise. The caller never names a path: names are checked to be plain words.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct Library {
    dir: Option<PathBuf>,
    templates: BTreeMap<String, Value>,
    operators: BTreeMap<(String, i64), String>,
}

/// A name a library entry may have: a letter, then letters, digits and `_`.
pub fn plain(name: &str) -> Result<(), String> {
    let ok = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && name.len() <= 64;
    if ok { Ok(()) } else { Err(format!("`{name}` is not a library name: a letter, then letters, digits and `_`")) }
}

/// A definition's text with its version set to 0 and in canonical form: two versions are the same content when these are.
fn content(def: &str) -> Option<String> {
    let f = emerge_lang::parse(&format!("world T {{ {def} }}")).ok()?;
    let d = f.items.into_iter().find_map(|i| match i.node { emerge_lang::ast::Item::World(w) => w.decls.into_iter().next(), _ => None })?;
    match d.node {
        emerge_lang::ast::WDecl::Operator { name, params, rules, .. } => Some(emerge_lang::fmt::decl_text(&emerge_lang::ast::WDecl::Operator { name, version: 0, params, rules })),
        _ => None,
    }
}

/// A definition's text with `version`.
pub fn with_version(def: &str, version: i64) -> Option<String> {
    let f = emerge_lang::parse(&format!("world T {{ {def} }}")).ok()?;
    let d = f.items.into_iter().find_map(|i| match i.node { emerge_lang::ast::Item::World(w) => w.decls.into_iter().next(), _ => None })?;
    match d.node {
        emerge_lang::ast::WDecl::Operator { name, params, rules, .. } => Some(emerge_lang::fmt::decl_text(&emerge_lang::ast::WDecl::Operator { name, version, params, rules })),
        _ => None,
    }
}

impl Library {
    pub fn new(dir: Option<PathBuf>) -> Library {
        let mut l = Library { dir, ..Library::default() };
        l.load();
        l
    }

    fn load(&mut self) {
        let Some(dir) = &self.dir else { return };
        if let Ok(rd) = std::fs::read_dir(dir.join("templates")) {
            for e in rd.flatten() {
                let p = e.path();
                if let (Some(name), Ok(t)) = (p.file_stem().and_then(|s| s.to_str()).map(str::to_string), std::fs::read_to_string(&p)) {
                    if let Ok(v) = serde_json::from_str::<Value>(&t) {
                        self.templates.insert(name, v);
                    }
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(dir.join("operators")) {
            for e in rd.flatten() {
                let p = e.path();
                let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else { continue };
                let Some((name, v)) = stem.split_once('@') else { continue };
                if let (Ok(v), Ok(t)) = (v.parse::<i64>(), std::fs::read_to_string(&p)) {
                    self.operators.insert((name.to_string(), v), t);
                }
            }
        }
    }

    fn write(&self, sub: &str, file: &str, body: &str) -> Result<(), String> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let d = dir.join(sub);
        std::fs::create_dir_all(&d).and_then(|_| std::fs::write(d.join(file), body)).map_err(|e| format!("could not keep it in the library: {e}"))
    }

    pub fn templates(&self) -> &BTreeMap<String, Value> {
        &self.templates
    }

    pub fn save_template(&mut self, name: &str, t: Value, replace: bool) -> Result<(), String> {
        plain(name)?;
        if self.templates.contains_key(name) && !replace {
            return Err(format!("the library has a template `{name}` already (pass replace: true to put this one in its place)"));
        }
        self.write("templates", &format!("{name}.json"), &serde_json::to_string_pretty(&t).unwrap_or_default())?;
        self.templates.insert(name.to_string(), t);
        Ok(())
    }

    /// The versions of operator `name` kept, oldest first.
    pub fn versions(&self, name: &str) -> Vec<i64> {
        self.operators.keys().filter(|(n, _)| n == name).map(|(_, v)| *v).collect()
    }

    pub fn operator(&self, name: &str, version: i64) -> Option<&String> {
        self.operators.get(&(name.to_string(), version))
    }

    /// Keeps `def` (an operator's text) as `version` of `name`: refused if that version is kept with other content, a
    /// no-op if it is kept with the same.
    pub fn publish(&mut self, name: &str, version: i64, def: &str, about: &str) -> Result<i64, String> {
        plain(name)?;
        let mine = content(def).ok_or("not an operator definition")?;
        if let Some(have) = self.operator(name, version) {
            if content(have) == Some(mine) {
                return Ok(version);
            }
            return Err(format!("the library's `{name}` version {version} is other content: a version, once kept, does not change (edit it as a new version)"));
        }
        // The same content kept under another version is that version.
        if let Some(v) = self.versions(name).into_iter().find(|v| self.operator(name, *v).and_then(|t| content(t)) == Some(mine.clone())) {
            return Ok(v);
        }
        let about = about.lines().filter(|l| !l.trim().is_empty()).map(|l| format!("// {}\n", l.trim())).collect::<String>();
        let text = format!("{about}{}\n", with_version(def, version).ok_or("not an operator definition")?);
        self.write("operators", &format!("{name}@{version}.em"), &text)?;
        self.operators.insert((name.to_string(), version), text);
        Ok(version)
    }

    pub fn list(&self) -> Value {
        let mut ops: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
        for ((name, v), text) in &self.operators {
            let about: Vec<&str> = text.lines().take_while(|l| l.starts_with("//")).map(|l| l.trim_start_matches('/').trim()).collect();
            let params = emerge_lang::parse(&format!("world T {{ {text} }}")).ok().and_then(|f| f.items.into_iter().find_map(|i| match i.node {
                emerge_lang::ast::Item::World(w) => w.decls.into_iter().find_map(|d| match d.node {
                    emerge_lang::ast::WDecl::Operator { params, .. } => Some(params.into_iter().map(|p| json!({"name": p.node.name, "kind": p.node.kind, "default": p.node.default.as_ref().map(emerge_lang::fmt::value_text)})).collect::<Vec<_>>()),
                    _ => None,
                }),
                _ => None,
            })).unwrap_or_default();
            ops.entry(name).or_default().push(json!({"version": v, "about": about.join(" "), "params": params}));
        }
        json!({
            "templates": self.templates.values().collect::<Vec<_>>(),
            "operators": ops.into_iter().map(|(n, v)| json!({"name": n, "versions": v})).collect::<Vec<_>>(),
            "kept": self.dir.as_ref().map(|d| d.display().to_string()),
        })
    }
}
