//! Patches applied to a source file (`language.md` §10.3): a variant of a world or lineage
//! is the base file plus a small patch, applied to the tree before `prepare`.
//!
//! Supported: `set` (a setting or one argument of it, an entry of a config block, a
//! constant, a menu entry's cost or one parameter, an argument of a labelled rule
//! statement, a reaction's `catalyze`, a lineage's `mutation` or profile entry) and `add`
//! (a world declaration, optionally `after`/`before` a rule or other named declaration; a
//! menu entry `after`/`before` another entry of that menu). `replace`, `remove`, `rename`
//! and adding genes are `E0904`: not supported yet (genes are added with `extends`).
//!
//! A path names the item as written in the file: a patch on `Builder` does not reach
//! what `Builder` takes from the lineage it extends.

use crate::ast::*;
use crate::check::Problem;

fn problem(code: &'static str, path: &str, pos: Pos, message: String) -> Problem {
    Problem { code, path: path.to_string(), pos, message, fix: None }
}

/// One step of a patch path: a field name or a `@label`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Key<'a> {
    Field(&'a str),
    Label(&'a str),
}

fn keys(p: &Path) -> Vec<Key<'_>> {
    let mut v = vec![Key::Field(p.root.as_str())];
    v.extend(p.segs.iter().map(|s| match s {
        Seg::Field(f) => Key::Field(f),
        Seg::Label(l) => Key::Label(l),
    }));
    v
}

fn text(p: &Path) -> String {
    let mut s = p.root.clone();
    for seg in &p.segs {
        match seg {
            Seg::Field(f) => s.push_str(&format!(".{f}")),
            Seg::Label(l) => s.push_str(&format!("@{l}")),
        }
    }
    s
}

/// Where a problem is reported: the patch's target and path, at the operation.
struct At<'a> {
    target: &'a str,
    pos: Pos,
}

impl At<'_> {
    fn err(&self, code: &'static str, path: &Path, message: String) -> Problem {
        problem(code, &format!("{}.{}", self.target, text(path)), self.pos, message)
    }
}

/// The file with every `patch` item of `patches` applied, in order. Items of `patches`
/// that are not patches are `E0903`.
pub fn apply(file: &File, patches: &File) -> Result<File, Problem> {
    let mut out = file.clone();
    for item in &patches.items {
        let Item::Patch(p) = &item.node else {
            return Err(problem("E0903", "", item.pos, "a patch file holds `patch` items only".into()));
        };
        let Some(slot) = out.items.iter_mut().find(|i| match &i.node {
            Item::World(w) => w.name == p.target,
            Item::Lineage(l) => l.name == p.target,
            _ => false,
        }) else {
            return Err(problem("E0114", &p.target, item.pos, format!("no world or lineage `{}` to patch", p.target)));
        };
        for op in &p.ops {
            let at = At { target: &p.target, pos: op.pos };
            match &mut slot.node {
                Item::World(w) => world_op(w, &op.node, &at)?,
                Item::Lineage(l) => lineage_op(l, &op.node, &at)?,
                _ => unreachable!(),
            }
        }
    }
    Ok(out)
}

/// `--set Dish.rules.rotting@react_rot.rate=500bp` as a patch file: the first name is the
/// target, the rest is the path, and everything after the first `=` is the value.
pub fn parse_set(spec: &str) -> Result<File, crate::Diag> {
    let bad = |m: &str| crate::Diag { code: "E0001", line: 1, col: 1, message: format!("`--set {spec}`: {m}") };
    let (lhs, value) = spec.split_once('=').ok_or_else(|| bad("expected PATH=VALUE"))?;
    let (target, path) = lhs.split_once('.').ok_or_else(|| bad("expected TARGET.PATH=VALUE"))?;
    crate::parse(&format!("patch {target} {{\n    set {path} = {value};\n}}\n")).map_err(|d| bad(&d.message))
}

// ---------- worlds ----------

fn world_op(w: &mut World, op: &PatchOp, at: &At) -> Result<(), Problem> {
    match op {
        PatchOp::Set { path, value } => world_set(w, path, value, at),
        PatchOp::Add { position, decl } => world_add(w, position.as_ref(), decl, at),
        PatchOp::Replace { path, .. } | PatchOp::Remove { path, .. } | PatchOp::Rename { path, .. } => {
            Err(at.err("E0904", path, "`replace`, `remove` and `rename` are not supported by `--patch` yet".into()))
        }
    }
}

fn unknown(at: &At, path: &Path, what: &str) -> Problem {
    at.err("E0114", path, format!("no {what}"))
}

fn world_set(w: &mut World, path: &Path, value: &Value, at: &At) -> Result<(), Problem> {
    let ks = keys(path);
    let Key::Field(head) = ks[0] else { unreachable!() };
    match (head, &ks[1..]) {
        ("consts", [Key::Field(name)]) => {
            let found = w.decls.iter_mut().find_map(|d| match &mut d.node {
                WDecl::Const { name: n, value } if n == name => Some(value),
                _ => None,
            });
            let slot = found.ok_or_else(|| unknown(at, path, &format!("constant `{name}`")))?;
            *slot = expr_of(value, path, at)?;
            Ok(())
        }
        ("senses" | "actions" | "regulation", [Key::Field(item), Key::Field(field)]) => {
            let items = w.decls.iter_mut().find_map(|d| match &mut d.node {
                WDecl::Menu { kind, items } if kind == head => Some(items),
                _ => None,
            });
            let items = items.ok_or_else(|| unknown(at, path, &format!("`{head}` menu")))?;
            let found = items.iter_mut().find(|m| menu_name(&m.node) == *item);
            let m = found.ok_or_else(|| unknown(at, path, &format!("entry `{item}` in `{head}`")))?;
            if *field == "cost" {
                let MenuItem::Action { cost, .. } = &mut m.node else {
                    return Err(unknown(at, path, &format!("cost on `{item}` (only actions have one)")));
                };
                *cost = match lit_of(value) {
                    Some(l @ Lit::Amount(_)) => l,
                    _ => return Err(at.err("E0205", path, "a cost is an amount such as `20mu`".into())),
                };
                return Ok(());
            }
            let params = match &mut m.node {
                MenuItem::Sense { params, .. } | MenuItem::Action { params, .. } | MenuItem::Gene { params, .. } => params,
            };
            let slot = params.iter_mut().find(|(n, _)| n == field);
            let (_, param) = slot.ok_or_else(|| unknown(at, path, &format!("parameter `{field}` of `{item}`")))?;
            *param = match (lit_of(value), value) {
                (Some(l @ Lit::Range(..)), _) => Param::Range(l),
                (_, Value::Expr(Expr::List(items))) => {
                    let names: Option<Vec<String>> = items
                        .iter()
                        .map(|v| match v {
                            Value::Expr(Expr::Path(p)) if p.segs.is_empty() => Some(p.root.clone()),
                            _ => None,
                        })
                        .collect();
                    Param::Choices(names.ok_or_else(|| at.err("E0205", path, "a list of choices holds names".into()))?)
                }
                _ => return Err(at.err("E0205", path, "a parameter is set to a range (`0u..1u`) or a list of names".into())),
            };
            Ok(())
        }
        ("rules", [Key::Field(name), Key::Label(label), Key::Field(arg)]) => {
            let body = w.decls.iter_mut().find_map(|d| match &mut d.node {
                WDecl::Rule { name: n, body, .. } if n == name => Some(body),
                _ => None,
            });
            let body = body.ok_or_else(|| unknown(at, path, &format!("rule `{name}`")))?;
            let call = find_statement(body, label).ok_or_else(|| unknown(at, path, &format!("statement `@{label}` in rule `{name}`")))?;
            let a = call.args.iter_mut().find(|a| a.name.as_deref() == Some(*arg));
            let a = a.ok_or_else(|| unknown(at, path, &format!("argument `{arg}` of `@{label}`")))?;
            a.value = value.clone();
            Ok(())
        }
        ("reactions", [Key::Field(name), Key::Field("catalyze")]) => {
            let found = w.decls.iter_mut().find_map(|d| match &mut d.node {
                WDecl::Reaction { name: n, catalyze, .. } if n == name => Some(catalyze),
                _ => None,
            });
            let slot = found.ok_or_else(|| unknown(at, path, &format!("reaction `{name}`")))?;
            let Value::Expr(Expr::Call(c)) = value else {
                return Err(at.err("E0205", path, "`catalyze` is set to a call such as `nitrate(half: 1u, boost: 4.0)`".into()));
            };
            *slot = Some(c.clone());
            Ok(())
        }
        (_, [Key::Field(key)]) => {
            // A config block (`upkeep.per_gene`) or the argument of a setting's call (`space.wrap`).
            for d in &mut w.decls {
                match &mut d.node {
                    WDecl::Config { name, entries } if name == head => return set_entry(entries, key, value, path, at),
                    WDecl::Setting(e) if e.key == head => return set_argument(e, key, value, path, at),
                    _ => {}
                }
            }
            Err(unknown(at, path, &format!("`{head}`")))
        }
        (_, []) => {
            for d in &mut w.decls {
                if let WDecl::Setting(e) = &mut d.node {
                    if e.key == head {
                        e.value = value.clone();
                        return Ok(());
                    }
                }
            }
            Err(unknown(at, path, &format!("setting `{head}` (add it with `add {head}: ...;`)")))
        }
        _ => Err(unknown(at, path, "item at this path")),
    }
}

fn menu_name(m: &MenuItem) -> &str {
    match m {
        MenuItem::Sense { name, .. } | MenuItem::Action { name, .. } | MenuItem::Gene { name, .. } => name,
    }
}

fn find_statement<'a>(body: &'a mut Block, label: &str) -> Option<&'a mut Call> {
    for s in body.iter_mut() {
        match &mut s.node {
            Stmt::Call { call, label: Some(l) } if l == label => return Some(call),
            Stmt::When { then, otherwise, .. } => {
                if let Some(c) = find_statement(then, label) {
                    return Some(c);
                }
                if let Some(c) = otherwise.as_mut().and_then(|o| find_statement(o, label)) {
                    return Some(c);
                }
            }
            Stmt::For { body, .. } => {
                if let Some(c) = find_statement(body, label) {
                    return Some(c);
                }
            }
            _ => {}
        }
    }
    None
}

fn set_entry(entries: &mut [Node<Entry>], key: &str, value: &Value, path: &Path, at: &At) -> Result<(), Problem> {
    match entries.iter_mut().find(|e| e.node.key == key) {
        Some(e) => {
            e.node.value = value.clone();
            Ok(())
        }
        None => Err(unknown(at, path, &format!("entry `{key}`"))),
    }
}

fn set_argument(e: &mut Entry, arg: &str, value: &Value, path: &Path, at: &At) -> Result<(), Problem> {
    let Value::Expr(Expr::Call(c)) = &mut e.value else {
        return Err(unknown(at, path, &format!("argument `{arg}` (`{}` is not a call)", e.key)));
    };
    match c.args.iter_mut().find(|a| a.name.as_deref() == Some(arg)) {
        Some(a) => {
            a.value = value.clone();
            Ok(())
        }
        None => Err(unknown(at, path, &format!("argument `{arg}` of `{}`", e.key))),
    }
}

fn lit_of(v: &Value) -> Option<Lit> {
    match v {
        Value::Expr(Expr::Lit(l)) => Some(l.clone()),
        _ => None,
    }
}

fn expr_of(v: &Value, path: &Path, at: &At) -> Result<Expr, Problem> {
    match v {
        Value::Expr(e) => Ok(e.clone()),
        _ => Err(at.err("E0205", path, "a constant is set to an expression".into())),
    }
}

/// The name an `add` declares, for the duplicate check (`E0111`).
fn declared(d: &WDecl) -> Option<(&'static str, &str)> {
    Some(match d {
        WDecl::Element(n) => ("element", n),
        WDecl::Const { name, .. } => ("constant", name),
        WDecl::Substance { name, .. } => ("substance", name),
        WDecl::Field { name, .. } => ("field", name),
        WDecl::Source { name, .. } => ("source", name),
        WDecl::Sink { name, .. } => ("sink", name),
        WDecl::Reaction { name, .. } => ("reaction", name),
        WDecl::Rule { name, .. } => ("rule", name),
        WDecl::Setting(e) => ("setting", &e.key),
        WDecl::Config { name, .. } => ("block", name),
        WDecl::Menu { .. } | WDecl::Waste { .. } | WDecl::Genesis(_) => return None,
    })
}

fn world_add(w: &mut World, position: Option<&(bool, Path)>, item: &AddItem, at: &At) -> Result<(), Problem> {
    match item {
        AddItem::Gene(_) => {
            let p = position.map(|(_, p)| p.clone()).unwrap_or(Path { root: "seed".into(), segs: vec![] });
            Err(at.err("E0904", &p, "a gene is added to a lineage, which `--patch` cannot do yet (use `extends`)".into()))
        }
        AddItem::MenuItem(m) => {
            let Some((after, p)) = position else { unreachable!("the parser reads a menu entry only after a position") };
            let ks = keys(p);
            let (Key::Field(kind), [Key::Field(neighbour)]) = (ks[0], &ks[1..]) else {
                return Err(at.err("E0114", p, "a menu entry goes after or before an entry, as `actions.steal`".into()));
            };
            let items = w.decls.iter_mut().find_map(|d| match &mut d.node {
                WDecl::Menu { kind: k, items } if k == kind => Some(items),
                _ => None,
            });
            let items = items.ok_or_else(|| unknown(at, p, &format!("`{kind}` menu")))?;
            if items.iter().any(|x| menu_name(&x.node) == menu_name(m)) {
                return Err(at.err("E0111", p, format!("`{}` is already in `{kind}`", menu_name(m))));
            }
            let i = items.iter().position(|x| menu_name(&x.node) == *neighbour);
            let i = i.ok_or_else(|| unknown(at, p, &format!("entry `{neighbour}` in `{kind}`")))?;
            let node = Node { trivia: Trivia::default(), pos: at.pos, node: m.clone() };
            items.insert(if *after { i + 1 } else { i }, node);
            Ok(())
        }
        AddItem::Decl(d) => {
            if let Some((kind, name)) = declared(d) {
                let clash = w.decls.iter().find_map(|x| declared(&x.node).filter(|(_, n)| *n == name));
                if let Some((other, _)) = clash {
                    let p = Path { root: name.to_string(), segs: vec![] };
                    return Err(at.err("E0111", &p, format!("the {kind} `{name}` repeats the name of a {other}; use `set`")));
                }
            }
            let node = Node { trivia: Trivia::default(), pos: at.pos, node: d.clone() };
            let index = match position {
                None => w.decls.len(),
                Some((after, p)) => {
                    let ks = keys(p);
                    let i = match (ks[0], &ks[1..]) {
                        // A setting or block is named alone: `after remains`.
                        (Key::Field(name), []) => {
                            let i = w.decls.iter().position(|x| matches!(declared(&x.node), Some(("setting" | "block", n)) if n == name));
                            i.ok_or_else(|| unknown(at, p, &format!("setting or block `{name}`")))?
                        }
                        (Key::Field(kind), [Key::Field(name)]) => {
                            let want = match kind {
                                "rules" => "rule",
                                "reactions" => "reaction",
                                "substances" => "substance",
                                "consts" => "constant",
                                _ => return Err(at.err("E0114", p, format!("`{kind}` is not a kind of declaration"))),
                            };
                            let i = w.decls.iter().position(|x| declared(&x.node) == Some((want, name)));
                            i.ok_or_else(|| unknown(at, p, &format!("{want} `{name}`")))?
                        }
                        _ => return Err(at.err("E0114", p, "a declaration goes after or before one, as `rules.weather` or `remains`".into())),
                    };
                    if *after { i + 1 } else { i }
                }
            };
            w.decls.insert(index, node);
            Ok(())
        }
    }
}

// ---------- lineages ----------

fn lineage_op(l: &mut Lineage, op: &PatchOp, at: &At) -> Result<(), Problem> {
    match op {
        PatchOp::Set { path, value } => {
            let ks = keys(path);
            let Key::Field(head) = ks[0] else { unreachable!() };
            match (head, &ks[1..]) {
                ("mutation", [Key::Field(key)]) => {
                    let entries = l.decls.iter_mut().find_map(|d| match &mut d.node {
                        LDecl::Mutation(e) => Some(e),
                        _ => None,
                    });
                    let entries = entries.ok_or_else(|| unknown(at, path, "mutation block"))?;
                    set_entry(entries, key, value, path, at)
                }
                ("profiles", [Key::Field(name), Key::Field(key)]) => {
                    let entries = l.decls.iter_mut().find_map(|d| match &mut d.node {
                        LDecl::Profile { name: n, entries } if n == name => Some(entries),
                        _ => None,
                    });
                    let entries = entries.ok_or_else(|| unknown(at, path, &format!("profile `{name}`")))?;
                    set_entry(entries, key, value, path, at)
                }
                _ => Err(unknown(at, path, "item at this path (lineages take `mutation.KEY` and `profiles.NAME.KEY`)")),
            }
        }
        PatchOp::Add { position, .. } => {
            let p = position.as_ref().map(|(_, p)| p.clone()).unwrap_or(Path { root: "seed".into(), segs: vec![] });
            Err(at.err("E0904", &p, "`--patch` cannot add to a lineage yet (use `extends`)".into()))
        }
        PatchOp::Replace { path, .. } | PatchOp::Remove { path, .. } | PatchOp::Rename { path, .. } => {
            Err(at.err("E0904", path, "`replace`, `remove` and `rename` are not supported by `--patch` yet".into()))
        }
    }
}
