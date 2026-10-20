//! `emerge fmt`: the canonical text of a program (`language.md` §2.2).
//!
//! Each statement or declaration is built as one line of pieces, where a piece is text or
//! a bracketed group. A line longer than 100 columns is broken at one outermost group (the
//! one with the most characters inside, the first on a tie), one item per line, and the
//! pieces are laid out again until every line fits or no group is left.

use crate::ast::*;

const WIDTH: usize = 100;
const INDENT: &str = "    ";

#[derive(Debug, Clone)]
enum P {
    T(String),
    G(&'static str, &'static str, Vec<Vec<P>>),
}

fn t(s: impl Into<String>) -> P {
    P::T(s.into())
}

fn flat(pieces: &[P]) -> String {
    let mut s = String::new();
    for p in pieces {
        match p {
            P::T(x) => s.push_str(x),
            P::G(open, close, items) => {
                s.push_str(open);
                s.push_str(&items.iter().map(|i| flat(i)).collect::<Vec<_>>().join(", "));
                s.push_str(close);
            }
        }
    }
    s
}

struct Out {
    lines: Vec<String>,
}

impl Out {
    fn layout(&mut self, indent: usize, line: Vec<P>) {
        let text = flat(&line);
        let width = indent * INDENT.len() + text.chars().count();
        let widest = line
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p {
                P::G(_, _, items) if !items.is_empty() => {
                    Some((i, items.iter().map(|x| flat(x)).collect::<Vec<_>>().join(", ").chars().count()))
                }
                _ => None,
            })
            .fold(None, |best: Option<(usize, usize)>, (i, n)| match best {
                Some((_, m)) if m >= n => best,
                _ => Some((i, n)),
            });
        match widest {
            Some((g, _)) if width > WIDTH => {
                let P::G(open, close, items) = line[g].clone() else { unreachable!() };
                let mut head = line[..g].to_vec();
                head.push(t(open));
                self.layout(indent, head);
                let n = items.len();
                for (k, mut item) in items.into_iter().enumerate() {
                    if k + 1 < n {
                        item.push(t(","));
                    }
                    self.layout(indent + 1, item);
                }
                let mut tail = vec![t(close)];
                tail.extend_from_slice(&line[g + 1..]);
                self.layout(indent, tail);
            }
            _ => self.lines.push(format!("{}{}", INDENT.repeat(indent), text)),
        }
    }

    fn trivia_before(&mut self, indent: usize, tr: &Trivia, first: bool) {
        if tr.blank_before && !first {
            self.lines.push(String::new());
        }
        for c in &tr.comments {
            if c.is_empty() {
                self.lines.push(String::new());
            } else {
                self.lines.push(format!("{}{}", INDENT.repeat(indent), c));
            }
        }
    }

    fn trailing(&mut self, tr: &Trivia) {
        if let Some(c) = &tr.trailing {
            let last = self.lines.last_mut().unwrap();
            last.push(' ');
            last.push_str(c);
        }
    }

    /// A node: its comments and blank line, its lines, and its trailing comment.
    fn node<T>(&mut self, indent: usize, n: &Node<T>, first: bool, f: impl FnOnce(&mut Self, usize, &T)) {
        self.trivia_before(indent, &n.trivia, first);
        f(self, indent, &n.node);
        self.trailing(&n.trivia);
    }

    fn nodes<T>(&mut self, indent: usize, ns: &[Node<T>], mut f: impl FnMut(&mut Self, usize, &T)) {
        for (i, n) in ns.iter().enumerate() {
            self.node(indent, n, i == 0, &mut f);
        }
    }

    /// `header {` children `}`.
    fn block<T>(&mut self, indent: usize, mut header: Vec<P>, ns: &[Node<T>], f: impl FnMut(&mut Self, usize, &T)) {
        if ns.is_empty() {
            header.push(t(" {}"));
            self.layout(indent, header);
            return;
        }
        header.push(t(" {"));
        self.layout(indent, header);
        self.nodes(indent + 1, ns, f);
        self.lines.push(format!("{}}}", INDENT.repeat(indent)));
    }
}

pub fn format(file: &File) -> String {
    let file = canonical_reactions(file);
    let mut out = Out { lines: Vec::new() };
    out.nodes(0, &file.items, item);
    for c in &file.tail {
        out.lines.push(c.clone());
    }
    let mut s = out.lines.join("\n");
    s.push('\n');
    s
}

/// The file with every world reaction in canonical form (spec §4.4), so `fmt` fixes what
/// `check` reports as `E0404`. A formula naming an undeclared substance, or one that would
/// cancel to nothing, is left as written; `check` reports those.
fn canonical_reactions(file: &File) -> File {
    let mut file = file.clone();
    for item in &mut file.items {
        let Item::World(w) = &mut item.node else { continue };
        let order: Vec<String> = w.decls.iter().filter_map(|d| match &d.node {
            WDecl::Substance { name, .. } => Some(name.clone()),
            _ => None,
        }).collect();
        for d in &mut w.decls {
            let WDecl::Reaction { formula: f, .. } = &mut d.node else { continue };
            let known = f.inputs.iter().chain(&f.outputs).all(|t| order.contains(&t.name));
            if !known {
                continue;
            }
            let c = crate::check::canonical(f, &|n| order.iter().position(|o| o == n).unwrap_or(usize::MAX));
            if !c.inputs.is_empty() && !c.outputs.is_empty() {
                *f = c;
            }
        }
    }
    file
}

fn item(out: &mut Out, indent: usize, it: &Item) {
    match it {
        Item::World(w) => {
            let mut decls = w.decls.clone();
            decls.sort_by_key(|d| decl_order(&d.node));
            out.block(indent, vec![t(format!("world {}", w.name))], &decls, wdecl);
        }
        Item::Lineage(l) => {
            let extends = l.base.as_ref().map(|b| format!(" extends {b}")).unwrap_or_default();
            out.block(indent, vec![t(format!("lineage {}{extends} in {}", l.name, l.world))], &l.decls, ldecl)
        }
        Item::Ruleset(r) => out.block(indent, vec![t(format!("ruleset {}", r.name))], &r.rules, |o, i, r| {
            let mut line = vec![t(format!("rule {} weight {} : ", r.name, float(&r.weight)))];
            line.extend(pattern(&r.from));
            if let Some(g) = &r.guard {
                line.push(t(" where "));
                line.extend(expr(g));
            }
            line.push(t(" -> "));
            line.extend(pattern(&r.to));
            line.push(t(";"));
            o.layout(i, line);
        }),
        Item::View(v) => out.block(indent, vec![t(format!("view {} for {}", v.name, v.world))], &v.layers, |o, i, l| {
            let mut header = vec![t(format!("layer {} = ", l.name))];
            header.extend(call(&l.source));
            o.block(i, header, &l.entries, entry);
        }),
        Item::Chart(c) => out.block(indent, vec![t(format!("chart {} for {}", c.name, c.world))], &c.series, |o, i, c| {
            let mut line = call(c);
            line.push(t(";"));
            o.layout(i, line);
        }),
        Item::Explore(x) => out.block(indent, vec![t(format!("explore {}", x.name))], &x.items, |o, i, it| match it {
            ExploreItem::Vary { path: p, values } => {
                let mut line = vec![t(format!("vary {} in ", path(p)))];
                line.extend(expr(values));
                line.push(t(";"));
                o.layout(i, line);
            }
            ExploreItem::Entry(e) => entry(o, i, e),
        }),
        Item::Patch(p) => out.block(indent, vec![t(format!("patch {} base {}", p.target, int(p.base)))], &p.ops, patch_op),
    }
}

/// The order of `language.md` §6, with rules before genesis. Substances, reactions and
/// rules keep their relative order (the sort is stable).
fn decl_order(d: &WDecl) -> u8 {
    match d {
        WDecl::Setting(e) if e.key == "space" => 0,
        WDecl::Element(_) | WDecl::Const { .. } => 1,
        WDecl::Config { name, .. } if name == "second_law" => 2,
        WDecl::Substance { .. } => 3,
        WDecl::Field { .. } => 4,
        WDecl::Source { .. } => 5,
        WDecl::Sink { .. } => 6,
        WDecl::Waste { .. } => 7,
        WDecl::Setting(e) if e.key == "remains" => 8,
        WDecl::Reaction { .. } => 9,
        WDecl::Config { name, .. } if name == "reaction_space" => 10,
        WDecl::Config { name, .. } if name == "upkeep" => 11,
        WDecl::Config { name, .. } if name == "limits" => 12,
        WDecl::Config { .. } | WDecl::Setting(_) => 13,
        WDecl::Menu { kind, .. } if kind == "senses" => 14,
        WDecl::Menu { kind, .. } if kind == "actions" => 15,
        WDecl::Menu { .. } => 16,
        WDecl::Rule { .. } => 17,
        WDecl::Genesis(_) => 18,
    }
}

fn wdecl(out: &mut Out, i: usize, d: &WDecl) {
    let line = |pieces: Vec<P>| {
        let mut pieces = pieces;
        pieces.push(t(";"));
        pieces
    };
    match d {
        WDecl::Element(e) => out.layout(i, line(vec![t(format!("element {e}"))])),
        WDecl::Substance { name, comp, potential } => out.layout(
            i,
            line(vec![t(format!("substance {name} = {} potential {}", terms(comp), int(*potential)))]),
        ),
        WDecl::Field { name, kind, holds, combine } => {
            let mut pieces = vec![t(format!("field {name} : "))];
            pieces.extend(call(kind));
            match holds {
                Some(h) => {
                    pieces.push(t(" holds "));
                    pieces.push(P::G("[", "]", h.iter().map(|(s, a)| vec![t(format!("{s}: {}", lit(a)))]).collect()));
                }
                None => pieces.push(t(match combine {
                    Some(c) => format!(" signal combine {c}"),
                    None => " signal".into(),
                })),
            }
            out.layout(i, line(pieces));
        }
        WDecl::Source { name, emits } => {
            out.layout(i, line(vec![t(format!("source {name} : ")), names(emits)]));
        }
        WDecl::Sink { name, accepts } => {
            let mut pieces = vec![t(format!("sink {name} : "))];
            match accepts {
                SinkAccepts::Any => pieces.push(t("any")),
                SinkAccepts::List(l) => pieces.push(names(l)),
            }
            out.layout(i, line(pieces));
        }
        WDecl::Waste { substance, sink } => out.layout(i, line(vec![t(format!("waste {substance} -> {sink}"))])),
        WDecl::Reaction { name, formula: f, catalyze } => {
            let mut pieces = vec![t(format!("reaction {name} : {}", formula(f)))];
            if let Some(c) = catalyze {
                pieces.push(t(" catalyze "));
                pieces.extend(call(c));
            }
            out.layout(i, line(pieces));
        }
        WDecl::Menu { kind, items } => out.block(i, vec![t(kind.clone())], items, menu_item),
        WDecl::Rule { name, on, body } => out.block(i, vec![t(format!("rule {name} on {on}"))], body, stmt),
        WDecl::Genesis(body) => out.block(i, vec![t("genesis")], body, stmt),
        WDecl::Config { name, entries } => {
            // A config block with one entry, and no comments in it, is written on one line.
            if let [only] = entries.as_slice() {
                if only.trivia.comments.is_empty() && only.trivia.trailing.is_none() {
                    let mut pieces = vec![t(format!("{name} {{ "))];
                    pieces.extend(entry_pieces(&only.node));
                    pieces.push(t(" }"));
                    if i * INDENT.len() + flat(&pieces).chars().count() <= WIDTH {
                        out.layout(i, pieces);
                        return;
                    }
                }
            }
            out.block(i, vec![t(name.clone())], entries, entry);
        }
        WDecl::Setting(e) => entry(out, i, e),
        WDecl::Const { name, value } => {
            let mut pieces = vec![t(format!("const {name} = "))];
            pieces.extend(expr(value));
            out.layout(i, line(pieces));
        }
    }
}

fn names(list: &[String]) -> P {
    P::G("[", "]", list.iter().map(|n| vec![t(n.clone())]).collect())
}

fn params(ps: &[(String, Param)]) -> P {
    P::G(
        "(",
        ")",
        ps.iter()
            .map(|(name, p)| {
                let v = match p {
                    Param::Range(l) => lit(l),
                    Param::Any(k) => format!("any {k}"),
                    Param::Choices(c) => format!("[{}]", c.join(", ")),
                    Param::Name(n) => n.clone(),
                };
                vec![t(format!("{name}: {v}"))]
            })
            .collect(),
    )
}

fn menu_item(out: &mut Out, i: usize, m: &MenuItem) {
    let mut pieces = Vec::new();
    match m {
        MenuItem::Sense { name, params: ps } => {
            pieces.push(t(format!("sense {name}")));
            pieces.push(params(ps));
        }
        MenuItem::Action { name, params: ps, cost, per, requires } => {
            pieces.push(t(format!("action {name}")));
            pieces.push(params(ps));
            pieces.push(t(format!(" cost {}", lit(cost))));
            if let Some(p) = per {
                pieces.push(t(format!(" per {p}")));
            }
            if let Some(r) = requires {
                pieces.push(t(" requires "));
                pieces.extend(expr(r));
            }
        }
        MenuItem::Gene { name, params: ps, inputs, weight } => {
            pieces.push(t(format!("gene {name}")));
            pieces.push(params(ps));
            pieces.push(t(format!(" inputs {}", lit(inputs))));
            if let Some(w) = weight {
                pieces.push(t(format!(" weight {}", lit(w))));
            }
        }
    }
    pieces.push(t(";"));
    out.layout(i, pieces);
}

fn entry_pieces(e: &Entry) -> Vec<P> {
    let mut pieces = vec![t(format!("{}: ", e.key))];
    pieces.extend(value(&e.value));
    pieces.push(t(";"));
    pieces
}

fn entry(out: &mut Out, i: usize, e: &Entry) {
    out.layout(i, entry_pieces(e));
}

fn stmt(out: &mut Out, i: usize, s: &Stmt) {
    match s {
        Stmt::Let { name, value } => {
            let mut line = vec![t(format!("let {name} = "))];
            line.extend(expr(value));
            line.push(t(";"));
            out.layout(i, line);
        }
        Stmt::When { cond, then, otherwise } => {
            let mut header = vec![t("when ")];
            header.extend(expr(cond));
            out.block(i, header, then, stmt);
            if let Some(o) = otherwise {
                // `} else {` on one line.
                let close = out.lines.pop().unwrap();
                out.block(i, vec![t(format!("{} else", close.trim_start()))], o, stmt);
            }
        }
        Stmt::For { var, field, body } => out.block(i, vec![t(format!("for {var} in {field}"))], body, stmt),
        Stmt::Assign { target, value } => {
            let mut line = vec![t(format!("{} = ", path(target)))];
            line.extend(expr(value));
            line.push(t(";"));
            out.layout(i, line);
        }
        Stmt::Call { call: c, label } => {
            let mut line = call(c);
            if let Some(l) = label {
                line.push(t(format!(" @{l}")));
            }
            line.push(t(";"));
            out.layout(i, line);
        }
        Stmt::Setting { key, value } => {
            let mut line = vec![t(format!("{key}: "))];
            line.extend(expr(value));
            line.push(t(";"));
            out.layout(i, line);
        }
    }
}

fn ldecl(out: &mut Out, i: usize, d: &LDecl) {
    match d {
        LDecl::Profile { name, entries } => out.block(i, vec![t(format!("profile {name}"))], entries, entry),
        LDecl::Mutation(entries) => out.block(i, vec![t("mutation")], entries, entry),
        LDecl::Genesis(body) => out.block(i, vec![t("genesis")], body, stmt),
        LDecl::Seed { signals, modules } => {
            out.layout(i, vec![t("seed genome {")]);
            out.nodes(i + 1, signals, |o, j, (name, keep)| o.layout(j, vec![t(format!("signal {name} keep {};", float(keep)))]));
            for (k, m) in modules.iter().enumerate() {
                out.node(i + 1, m, k == 0 && signals.is_empty(), module);
            }
            out.lines.push(format!("{}}}", INDENT.repeat(i)));
        }
    }
}

fn module(out: &mut Out, i: usize, m: &Module) {
    let header = format!("{}module {}", if m.dormant { "dormant " } else { "" }, m.name);
    out.block(i, vec![t(header)], &m.genes, gene);
}

fn source(s: &Source) -> String {
    match s {
        Source::Signal(n) => n.clone(),
        Source::Const(f) => float(f),
    }
}

fn gene_pieces(g: &GeneDecl) -> Vec<P> {
    let mut line = vec![t(if g.dormant { "dormant " } else { "" })];
    match &g.body {
        GeneBody::Sense { call: c, outs } => {
            line.push(t("sense "));
            line.extend(call(c));
            line.push(t(format!(" -> {}", outs.join(", "))));
        }
        GeneBody::Regulate { call: c, inputs, out } => {
            line.push(t("regulate "));
            line.extend(call(c));
            line.push(t(" <- "));
            line.push(P::G(
                "[",
                "]",
                inputs
                    .iter()
                    .map(|inp| {
                        vec![t(match &inp.weight {
                            Some(w) => format!("{} * {}", float(w), inp.signal),
                            None => inp.signal.clone(),
                        })]
                    })
                    .collect(),
            ));
            line.push(t(format!(" -> {out}")));
        }
        GeneBody::Reaction { name, rate, source: s } => {
            line.push(t(format!("reaction {name} rate {}", lit(rate))));
            if let Some(s) = s {
                line.push(t(format!(" <- {}", source(s))));
            }
        }
        GeneBody::Action { call: c, source: s, threshold } => {
            line.push(t("action "));
            line.extend(call(c));
            if let Some(s) = s {
                line.push(t(format!(" <- {}", source(s))));
            }
            if let Some(th) = threshold {
                line.push(t(format!(" threshold {}", float(th))));
            }
        }
    }
    if let Some(l) = &g.label {
        line.push(t(format!(" @{l}")));
    }
    if let Some(a) = &g.after {
        line.push(t(format!(" after @{a}")));
    }
    line.push(t(";"));
    line
}

fn gene(out: &mut Out, i: usize, g: &GeneDecl) {
    out.layout(i, gene_pieces(g));
}

fn pattern(p: &Pattern) -> Vec<P> {
    let item = if p.capture { format!("${}", p.item) } else { p.item.clone() };
    vec![
        t(format!("{} {item}", p.kind)),
        P::G(
            "(",
            ")",
            p.args
                .iter()
                .map(|(n, e)| {
                    let mut v = vec![t(format!("{n}: "))];
                    v.extend(expr(e));
                    v
                })
                .collect(),
        ),
    ]
}

fn patch_op(out: &mut Out, i: usize, op: &PatchOp) {
    match op {
        PatchOp::Set { path: p, value: v } => {
            let mut line = vec![t(format!("set {} = ", path(p)))];
            line.extend(value(v));
            line.push(t(";"));
            out.layout(i, line);
        }
        PatchOp::Add { position, decl } => {
            let prefix = match position {
                Some((after, p)) => format!("add {} {}: ", if *after { "after" } else { "before" }, path(p)),
                None => "add ".to_string(),
            };
            let start = out.lines.len();
            match decl.as_ref() {
                AddItem::Decl(d) => wdecl(out, i, d),
                AddItem::Gene(g) => gene(out, i, g),
            }
            let first = &mut out.lines[start];
            let body = first.trim_start().to_string();
            *first = format!("{}{prefix}{body}", INDENT.repeat(i));
        }
        PatchOp::Replace { path: p, body } => out.block(i, vec![t(format!("replace {}", path(p)))], body, stmt),
        PatchOp::Remove { path: p, move_to } => {
            let m = move_to.as_ref().map(|s| format!(" move_to: {s}")).unwrap_or_default();
            out.layout(i, vec![t(format!("remove {}{m};", path(p)))]);
        }
        PatchOp::Rename { path: p, to } => out.layout(i, vec![t(format!("rename {} to {to};", path(p)))]),
    }
}

// ---------- expressions and literals ----------

fn value(v: &Value) -> Vec<P> {
    match v {
        Value::Expr(e) => expr(e),
        Value::Formula(f) => vec![t(formula(f))],
        Value::Any => vec![t("any")],
    }
}

fn call(c: &Call) -> Vec<P> {
    let args = c
        .args
        .iter()
        .map(|a| {
            let mut v = Vec::new();
            if let Some(n) = &a.name {
                v.push(t(format!("{n}: ")));
            }
            v.extend(value(&a.value));
            v
        })
        .collect();
    vec![t(c.name.clone()), P::G("(", ")", args)]
}

fn expr(e: &Expr) -> Vec<P> {
    match e {
        Expr::Lit(l) => vec![t(lit(l))],
        Expr::Path(p) => vec![t(path(p))],
        Expr::Call(c) => call(c),
        Expr::List(items) => vec![P::G("[", "]", items.iter().map(value).collect())],
        Expr::Capture(c) => vec![t(format!("${c}"))],
        Expr::Paren(inner) => vec![P::G("(", ")", vec![expr(inner)])],
        Expr::Neg(inner) => {
            let mut v = vec![t("-")];
            v.extend(expr(inner));
            v
        }
        Expr::Not(inner) => {
            let mut v = vec![t("not ")];
            v.extend(expr(inner));
            v
        }
        Expr::Binary(op, l, r) => {
            let mut v = expr(l);
            v.push(t(format!(" {} ", op.text())));
            v.extend(expr(r));
            v
        }
        Expr::Pipe(l, c) => {
            let mut v = expr(l);
            v.push(t(" |> "));
            v.extend(call(c));
            v
        }
    }
}

fn path(p: &Path) -> String {
    let mut s = p.root.clone();
    for seg in &p.segs {
        match seg {
            Seg::Field(f) => {
                s.push('.');
                s.push_str(f);
            }
            Seg::Label(l) => {
                s.push('@');
                s.push_str(l);
            }
        }
    }
    s
}

fn terms(ts: &[Term]) -> String {
    ts.iter()
        .map(|x| match x.coeff {
            Some(c) => format!("{} {}", int(c), x.name),
            None => x.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// A statement as source text on one line (blocks are joined with spaces).
pub fn stmt_text(s: &Stmt) -> String {
    let mut out = Out { lines: Vec::new() };
    stmt(&mut out, 0, s);
    out.lines.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ")
}

/// A setting's value as source text: `rot`, `void`.
pub fn value_text(v: &Value) -> String {
    flat(&value(v))
}

/// A formula as source text: `light + carbon -> sugar`.
pub fn formula_text(f: &Formula) -> String {
    formula(f)
}

fn formula(f: &Formula) -> String {
    format!("{} -> {}", terms(&f.inputs), terms(&f.outputs))
}

/// Digits grouped in threes with `_`, for numbers of four digits or more.
fn group(digits: &str) -> String {
    if digits.len() < 4 {
        return digits.to_string();
    }
    let mut out = String::new();
    for (k, c) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k).is_multiple_of(3) {
            out.push('_');
        }
        out.push(c);
    }
    out
}

fn int(i: i64) -> String {
    let g = group(&i.unsigned_abs().to_string());
    if i < 0 { format!("-{g}") } else { g }
}

fn float(f: &str) -> String {
    let (sign, rest) = match f.strip_prefix('-') {
        Some(r) => ("-", r),
        None => ("", f),
    };
    let (mantissa, exp) = match rest.find(['e', 'E']) {
        Some(k) => (&rest[..k], Some(&rest[k + 1..])),
        None => (rest, None),
    };
    let (whole, frac) = match mantissa.find('.') {
        Some(k) => (&mantissa[..k], Some(&mantissa[k + 1..])),
        None => (mantissa, None),
    };
    let mut s = format!("{sign}{}", group(whole));
    if let Some(fr) = frac {
        s.push('.');
        s.push_str(fr);
    }
    if let Some(e) = exp {
        s.push('e');
        s.push_str(e);
    }
    s
}

pub fn lit(l: &Lit) -> String {
    match l {
        Lit::Int(i) => int(*i),
        Lit::Float(f) => float(f),
        Lit::Amount(mu) if mu % 1_000 == 0 => format!("{}u", group(&(mu / 1_000).to_string())),
        Lit::Amount(mu) => format!("{}mu", group(&mu.to_string())),
        Lit::Rate(bp) => format!("{}bp", group(&bp.to_string())),
        Lit::Bool(b) => b.to_string(),
        Lit::Range(a, b) => format!("{}..{}", lit(a), lit(b)),
    }
}
