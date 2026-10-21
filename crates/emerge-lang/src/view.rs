//! Checks of views and charts (`language.md` §9.2, `proposals/visual-language.md` V1): every name a
//! view uses resolves, and every value has the kind its place needs. Errors join `check::check`;
//! bindings that are valid but not computed yet (`species_id`, `metrics.*`) are warnings (`W1201`),
//! so a file that uses them stays valid.

use crate::ast::*;
use crate::check::Problem;

/// The palettes a view may name, by kind. A renderer must define every one.
pub const PALETTES: &[(&str, PaletteKind)] = &[
    ("greens", PaletteKind::Sequential),
    ("blues", PaletteKind::Sequential),
    ("reds", PaletteKind::Sequential),
    ("grays", PaletteKind::Sequential),
    ("viridis", PaletteKind::Sequential),
    ("magma", PaletteKind::Sequential),
    ("inferno", PaletteKind::Sequential),
    ("cividis", PaletteKind::Sequential),
    ("rdbu", PaletteKind::Diverging),
    ("phase", PaletteKind::Cyclic),
    ("categorical", PaletteKind::Categorical),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    /// Low to high, for an amount or a rate.
    Sequential,
    /// Two sides of an explicit zero.
    Diverging,
    /// Wraps around, for a heading or a phase.
    Cyclic,
    /// Distinct colours for distinct values (lineages, states).
    Categorical,
}

/// Agent properties a view may read, with their kinds; `species_id` is not computed yet.
const AGENT: &[(&str, Ty)] = &[
    ("heading", Ty::Vec2),
    ("position", Ty::Vec2),
    ("id", Ty::Category),
    ("lineage_id", Ty::Category),
    ("age", Ty::Int),
    ("expressed_genes", Ty::Int),
    // µ its own reactions made in the frame's window (exit test 9's producers: `made > 0mu`).
    ("made", Ty::Amount),
];
const NOT_YET: &[&str] = &["species_id"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ty {
    F32,
    Int,
    Bool,
    Vec2,
    /// An identity: a lineage, an entity, a state.
    Category,
    /// µ of a substance: `units(...)` makes it a number.
    Amount,
    /// A number mapped into `[0, 1]` by `scale`.
    Scaled,
    Color,
    Glyph,
    /// A binding that is valid but not computed yet; already warned about.
    Later,
}

impl Ty {
    fn text(self) -> &'static str {
        match self {
            Ty::F32 => "a number",
            Ty::Int => "a whole number",
            Ty::Bool => "a condition",
            Ty::Vec2 => "a direction",
            Ty::Category => "an identity",
            Ty::Amount => "an amount (use `units(...)` for a number)",
            Ty::Scaled => "a scaled value",
            Ty::Color => "a colour",
            Ty::Glyph => "a glyph",
            Ty::Later => "a binding not computed yet",
        }
    }
    fn number(self) -> bool {
        matches!(self, Ty::F32 | Ty::Int | Ty::Later)
    }
}

/// What a layer's expressions see.
enum Scope<'a> {
    /// A field layer: `value` is the cell's value (in units for a held substance).
    Field,
    /// An agents layer: entity properties and inventory amounts by substance name.
    Agents { substances: &'a [String] },
}

struct Checker {
    errors: Vec<Problem>,
    warnings: Vec<Problem>,
    path: String,
    pos: Pos,
}

fn lit_number(l: &Lit) -> Option<f64> {
    match l {
        Lit::Int(i) => Some(*i as f64),
        Lit::Float(f) => f.parse().ok(),
        _ => None,
    }
}

impl Checker {
    fn error(&mut self, code: &'static str, message: String) {
        self.errors.push(Problem { code, path: self.path.clone(), pos: self.pos, message, fix: None });
    }
    fn later(&mut self, what: &str) {
        let message = format!("`{what}` is not computed yet: a renderer shows it as unavailable");
        self.warnings.push(Problem { code: "W1201", path: self.path.clone(), pos: self.pos, message, fix: None });
    }
    /// `want` or report what was found; `Later` passes (it was warned about where it was named).
    fn expect(&mut self, got: Ty, want: &[Ty], what: &str) {
        if got != Ty::Later && !want.contains(&got) {
            let w: Vec<&str> = want.iter().map(|t| t.text()).collect();
            self.error("E0205", format!("{what} must be {}, not {}", w.join(" or "), got.text()));
        }
    }

    fn expr(&mut self, e: &Expr, scope: &Scope) -> Ty {
        match e {
            Expr::Lit(Lit::Int(_)) => Ty::Int,
            Expr::Lit(Lit::Float(_)) => Ty::F32,
            Expr::Lit(Lit::Amount(_)) => Ty::Amount,
            Expr::Lit(Lit::Bool(_)) => Ty::Bool,
            Expr::Lit(_) => {
                self.error("E0205", "a range or rate is not a value here".into());
                Ty::F32
            }
            Expr::Paren(x) => self.expr(x, scope),
            Expr::Neg(x) => {
                let t = self.expr(x, scope);
                self.expect(t, &[Ty::F32, Ty::Int], "a negated value");
                t
            }
            Expr::Not(x) => {
                let t = self.expr(x, scope);
                self.expect(t, &[Ty::Bool], "`not`'s operand");
                Ty::Bool
            }
            Expr::Binary(op, a, b) => {
                let (ta, tb) = (self.expr(a, scope), self.expr(b, scope));
                match op {
                    BinOp::Or | BinOp::And => {
                        self.expect(ta, &[Ty::Bool], "a condition");
                        self.expect(tb, &[Ty::Bool], "a condition");
                        Ty::Bool
                    }
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        let same = ta == tb || (ta.number() && tb.number()) || ta == Ty::Later || tb == Ty::Later;
                        if !same {
                            self.error("E0205", format!("cannot compare {} with {}", ta.text(), tb.text()));
                        }
                        Ty::Bool
                    }
                    _ => {
                        self.expect(ta, &[Ty::F32, Ty::Int], &format!("`{}`'s left side", op.text()));
                        self.expect(tb, &[Ty::F32, Ty::Int], &format!("`{}`'s right side", op.text()));
                        if ta == Ty::Int && tb == Ty::Int && *op != BinOp::Div { Ty::Int } else { Ty::F32 }
                    }
                }
            }
            Expr::Path(p) => self.path_ty(p, scope),
            Expr::Call(c) => self.call(c, None, scope),
            Expr::Pipe(x, c) => {
                let t = self.expr(x, scope);
                self.call(c, Some(t), scope)
            }
            Expr::List(_) | Expr::Capture(_) => {
                self.error("E0205", "a list or capture is not a value in a view".into());
                Ty::F32
            }
        }
    }

    fn path_ty(&mut self, p: &Path, scope: &Scope) -> Ty {
        let name = p.root.as_str();
        if !p.segs.is_empty() {
            self.error("E0114", format!("unknown name `{name}.…` in a view"));
            return Ty::F32;
        }
        match scope {
            Scope::Field if name == "value" => Ty::F32,
            Scope::Agents { substances } => {
                if let Some((_, t)) = AGENT.iter().find(|(n, _)| *n == name) {
                    *t
                } else if NOT_YET.contains(&name) {
                    self.later(name);
                    Ty::Later
                } else if substances.iter().any(|s| s == name) {
                    Ty::Amount
                } else {
                    self.error("E0114", format!("unknown name `{name}`: an agents layer reads {}, and inventory amounts by substance name", AGENT.iter().map(|(n, _)| format!("`{n}`")).collect::<Vec<_>>().join(", ")));
                    Ty::F32
                }
            }
            Scope::Field => {
                self.error("E0114", format!("unknown name `{name}`: a field layer reads `value`"));
                Ty::F32
            }
        }
    }

    fn arg<'c>(c: &'c Call, key: &str) -> Option<&'c Value> {
        c.args.iter().find(|a| a.name.as_deref() == Some(key)).map(|a| &a.value)
    }

    fn subject(c: &Call) -> Option<&Expr> {
        Self::positional(c).first().copied()
    }

    /// The unnamed arguments, in order.
    fn positional(c: &Call) -> Vec<&Expr> {
        c.args.iter().filter(|a| a.name.is_none()).filter_map(|a| match &a.value {
            Value::Expr(e) => Some(e),
            _ => None,
        }).collect()
    }

    /// The name that comes after the subject (`scale(value, log, …)`, or `value |> scale(log, …)`).
    fn word(c: &Call, piped: bool) -> Option<String> {
        match Self::positional(c).get(usize::from(!piped)) {
            Some(Expr::Path(p)) if p.segs.is_empty() => Some(p.root.clone()),
            _ => None,
        }
    }

    /// A call, with the piped-in value as its subject when there is one.
    fn call(&mut self, c: &Call, piped: Option<Ty>, scope: &Scope) -> Ty {
        let subject = |me: &mut Self| -> Option<Ty> { piped.or_else(|| Self::subject(c).map(|e| me.expr(e, scope))) };
        match c.name.as_str() {
            "units" => {
                let t = subject(self);
                self.expect(t.unwrap_or(Ty::F32), &[Ty::Amount], "`units`'s subject");
                Ty::F32
            }
            "float" => {
                let t = subject(self);
                self.expect(t.unwrap_or(Ty::Int), &[Ty::Int], "`float`'s subject");
                Ty::F32
            }
            "scale" => {
                let t = subject(self).unwrap_or(Ty::F32);
                self.expect(t, &[Ty::F32, Ty::Int], "what `scale` maps");
                let kind = Self::word(c, piped.is_some());
                match kind.as_deref() {
                    Some("linear") | Some("log") => {}
                    other => self.error("E0114", format!("unknown scale `{}`: `linear` or `log`", other.unwrap_or("?"))),
                }
                match Self::arg(c, "range") {
                    Some(Value::Expr(Expr::Lit(Lit::Range(a, b)))) => match (lit_number(a), lit_number(b)) {
                        (Some(lo), Some(hi)) if lo >= hi => self.error("E0204", format!("the range {lo}..{hi} is empty")),
                        (Some(lo), Some(_)) if kind.as_deref() == Some("log") && lo <= 0.0 => self.error("E0204", "a log scale needs a range above 0".into()),
                        (Some(_), Some(_)) => {}
                        _ => self.error("E0205", "`range` must be numbers, `low..high`".into()),
                    },
                    Some(_) => self.error("E0205", "`range` must be `low..high`".into()),
                    None => self.error("E0116", "`scale` needs `range`".into()),
                }
                Ty::Scaled
            }
            "palette" => {
                let t = subject(self).unwrap_or(Ty::Scaled);
                let name = Self::word(c, piped.is_some());
                match name.as_deref().and_then(|n| PALETTES.iter().find(|(p, _)| *p == n)) {
                    Some((_, PaletteKind::Categorical)) => self.expect(t, &[Ty::Category, Ty::Int], "what a categorical palette colours"),
                    Some((_, PaletteKind::Cyclic)) => self.expect(t, &[Ty::Scaled, Ty::F32, Ty::Vec2], "what a cyclic palette colours"),
                    Some(_) => self.expect(t, &[Ty::Scaled, Ty::F32], "what this palette colours (map it with `scale` first)"),
                    None => self.error("E0114", format!("unknown palette `{}`: {}", name.unwrap_or_default(), PALETTES.iter().map(|(p, _)| *p).collect::<Vec<_>>().join(", "))),
                }
                Ty::Color
            }
            "arrow" => {
                match Self::arg(c, "dir") {
                    Some(Value::Expr(e)) => {
                        let t = self.expr(e, scope);
                        self.expect(t, &[Ty::Vec2], "`arrow`'s `dir`");
                    }
                    _ => self.error("E0116", "`arrow` needs `dir`".into()),
                }
                Ty::Glyph
            }
            "dot" => Ty::Glyph,
            other => {
                self.error("E0114", format!("unknown function `{other}` in a view"));
                Ty::F32
            }
        }
    }

    fn value_expr<'v>(&mut self, v: &'v Value) -> Option<&'v Expr> {
        match v {
            Value::Expr(e) => Some(e),
            _ => {
                self.error("E0205", "expected an expression".into());
                None
            }
        }
    }
}

fn substances(w: &World) -> Vec<String> {
    w.decls.iter().filter_map(|d| match &d.node {
        WDecl::Substance { name, .. } => Some(name.clone()),
        _ => None,
    }).collect()
}

fn view(v: &View, pos: Pos, worlds: &[&World], errors: &mut Vec<Problem>, warnings: &mut Vec<Problem>) {
    let Some(world) = worlds.iter().find(|w| w.name == v.world) else {
        errors.push(Problem { code: "E0114", path: v.name.clone(), pos, message: format!("view `{}` is for `{}`, which is not a world in this file", v.name, v.world), fix: None });
        return;
    };
    let subs = substances(world);
    for layer in &v.layers {
        let mut ck = Checker { errors: Vec::new(), warnings: Vec::new(), path: format!("{}.layers.{}", v.name, layer.node.name), pos: layer.pos };
        let l = &layer.node;
        let (scope, keys): (Scope, &[&str]) = match l.source.name.as_str() {
            "field" => {
                match Checker::subject(&l.source) {
                    Some(Expr::Path(p)) => {
                        let field = world.decls.iter().find_map(|d| match &d.node {
                            WDecl::Field { name, holds, .. } if *name == p.root => Some(holds),
                            _ => None,
                        });
                        let kind = if world.decls.iter().any(|d| matches!(&d.node, WDecl::Field { name, states: Some(_), .. } if *name == p.root)) { "state" } else { "signal" };
                        match (field, p.segs.as_slice()) {
                            (None, _) => ck.error("E0114", format!("unknown field `{}`", p.root)),
                            (Some(Some(holds)), [Seg::Field(s)]) if !holds.iter().any(|(h, _)| h == s) => ck.error("E0114", format!("field `{}` holds no `{s}`", p.root)),
                            (Some(Some(_)), [Seg::Field(_)]) | (Some(None), []) => {}
                            (Some(Some(_)), _) => ck.error("E0116", format!("say which substance of `{}` to show: `field({}.NAME)`", p.root, p.root)),
                            (Some(None), _) => ck.error("E0114", format!("`{}` is a {kind} field: show it as `field({})`", p.root, p.root)),
                        }
                    }
                    _ => ck.error("E0116", "`field` needs a field, such as `field(cells.sugar)`".into()),
                }
                (Scope::Field, &["color"])
            }
            "agents" => {
                let scope = Scope::Agents { substances: &subs };
                if let Some(Value::Expr(e)) = Checker::arg(&l.source, "where") {
                    let t = ck.expr(e, &scope);
                    ck.expect(t, &[Ty::Bool], "`where`");
                }
                (scope, &["glyph", "color", "size"])
            }
            other => {
                ck.error("E0114", format!("unknown layer source `{other}`: `field` or `agents`"));
                errors.append(&mut ck.errors);
                continue;
            }
        };
        for entry in &l.entries {
            ck.pos = entry.pos;
            let key = entry.node.key.as_str();
            if !keys.contains(&key) {
                ck.error("E0115", format!("unknown entry `{key}` in a {} layer: {}", l.source.name, keys.join(", ")));
                continue;
            }
            let Some(e) = ck.value_expr(&entry.node.value) else { continue };
            let t = ck.expr(e, &scope);
            match key {
                "color" => ck.expect(t, &[Ty::Color], "`color`"),
                "glyph" => ck.expect(t, &[Ty::Glyph], "`glyph`"),
                _ => ck.expect(t, &[Ty::F32, Ty::Int], "`size`"),
            }
        }
        errors.append(&mut ck.errors);
        warnings.append(&mut ck.warnings);
    }
}

fn chart(c: &Chart, pos: Pos, worlds: &[&World], errors: &mut Vec<Problem>, warnings: &mut Vec<Problem>) {
    if !worlds.iter().any(|w| w.name == c.world) {
        errors.push(Problem { code: "E0114", path: c.name.clone(), pos, message: format!("chart `{}` is for `{}`, which is not a world in this file", c.name, c.world), fix: None });
        return;
    }
    for s in &c.series {
        let path = format!("{}.{}", c.name, s.node.name);
        let mut push = |code: &'static str, message: String, warn: bool| {
            let p = Problem { code, path: path.clone(), pos: s.pos, message, fix: None };
            if warn { warnings.push(p) } else { errors.push(p) }
        };
        let subject = Checker::subject(&s.node);
        match s.node.name.as_str() {
            "line" => {
                match subject {
                    Some(Expr::Path(p)) if p.root == "metrics" => push("W1201", format!("`metrics{}` is not computed yet: a renderer shows it as unavailable", p.segs.iter().map(|g| match g { Seg::Field(f) | Seg::Label(f) => format!(".{f}") }).collect::<String>()), true),
                    Some(Expr::Path(p)) if p.segs.is_empty() && ["population", "births", "deaths"].contains(&p.root.as_str()) => {}
                    _ => push("E0114", "a line plots `population`, `births`, `deaths` or `metrics.…`".into(), false),
                }
                match Checker::arg(&s.node, "by") {
                    Some(Value::Expr(Expr::Path(p))) if p.root == "lineage" => {}
                    Some(Value::Expr(Expr::Path(p))) if p.root == "species" => push("W1201", "`by: species` is not computed yet (species are not defined); `by: lineage` is".into(), true),
                    Some(_) => push("E0114", "`by` is `lineage` or `species`".into(), false),
                    None => {}
                }
            }
            "ledger" => match subject {
                Some(Expr::Path(p)) if p.segs.is_empty() && (p.root == "elements" || p.root == "substances") => {}
                _ => push("E0114", "a ledger shows `elements` or `substances`".into(), false),
            },
            other => push("E0114", format!("unknown series `{other}`: `line` or `ledger`"), false),
        }
    }
}

/// Every view and chart of `file`: errors, and warnings for bindings not computed yet.
pub fn check(file: &File) -> (Vec<Problem>, Vec<Problem>) {
    let worlds: Vec<&World> = file.items.iter().filter_map(|i| match &i.node {
        Item::World(w) => Some(w),
        _ => None,
    }).collect();
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    for item in &file.items {
        match &item.node {
            Item::View(v) => view(v, item.pos, &worlds, &mut errors, &mut warnings),
            Item::Chart(c) => chart(c, item.pos, &worlds, &mut errors, &mut warnings),
            _ => {}
        }
    }
    (errors, warnings)
}
