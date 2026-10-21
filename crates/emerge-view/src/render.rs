//! Renders a `.em` view of a frame to an image (`proposals/visual-language.md` V2).
//!
//! As in Houdini's VEX, a layer is evaluated once per element (a cell of the field, or an agent)
//! into attributes: `@Cd` (colour), `@Alpha`, `@pscale` (size, in world units), `@shape` and `@N`
//! (direction). Drawing reads only those attributes, so any front end that sets them (the current
//! `color:`/`size:`/`glyph:` entries, or later VEX-like wrangles) renders the same way.
//!
//! A field is drawn as exact cells: every pixel of a cell has that cell's colour, with no
//! smoothing. A legend beside the picture says what every colour and size means, including what
//! is empty and what is not computed yet.

use crate::frame::{Agent, Frame};
use crate::palette::{self, EMPTY, UNAVAILABLE};
use crate::text::Text;
use emerge_lang::ast::*;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

type Rgb = [u8; 3];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Dot,
    Arrow,
}

/// The attributes of one element, VEX style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attributes {
    pub cd: Rgb,
    pub alpha: f32,
    pub pscale: f32,
    pub shape: Shape,
    pub n: [f32; 2],
}

/// A value while evaluating a view expression.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Val {
    Num(f64),
    Int(i64),
    Bool(bool),
    Vec2([f32; 2]),
    /// An identity (a lineage, an entity).
    Cat(u64),
    /// µ of a substance.
    Amount(u64),
    /// In `[0, 1]`, or `None` for nothing to show (zero under a log scale).
    Scaled(Option<f64>),
    Color(Rgb),
    Glyph(Shape),
    /// A binding not computed yet.
    Later,
}

impl Val {
    fn num(self) -> f64 {
        match self {
            Val::Num(x) => x,
            Val::Int(i) => i as f64,
            Val::Amount(a) => a as f64 / 1000.0,
            Val::Bool(b) => f64::from(u8::from(b)),
            // In arithmetic a scaled value is its number; nothing to show (below a log scale) is 0.
            Val::Scaled(t) => t.unwrap_or(0.0),
            _ => 0.0,
        }
    }
}

/// What a legend says about one layer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayerKey {
    pub name: String,
    /// What it shows, in words.
    pub source: String,
    /// The palette and the scale's range and kind, if the colour is a scaled number.
    pub ramp: Option<(String, f64, f64, bool)>,
    /// The identities shown, with their names and colours.
    pub categories: Vec<(String, Rgb)>,
    /// Entries quoted from the view (`size: …`).
    pub entries: Vec<String>,
    /// Bindings not computed yet, shown in `UNAVAILABLE`.
    pub unavailable: Vec<String>,
    /// Cells with nothing to show, drawn in `EMPTY`.
    pub empty: bool,
    pub count: usize,
}

struct Ctx<'a> {
    frame: &'a Frame,
    /// The cell, for a field layer.
    cell: usize,
    /// The substance shown by a field layer.
    substance: Option<usize>,
    /// The state field shown by a field layer (an index into `Frame::states`).
    state: Option<usize>,
    /// `value` itself, for drawing a legend's swatch of one state.
    value: Option<f64>,
    /// A layer body's locals.
    locals: Option<&'a std::collections::HashMap<String, Val>>,
    agent: Option<&'a Agent>,
}

fn name_of(e: &Expr) -> Option<&str> {
    match e {
        Expr::Path(p) if p.segs.is_empty() => Some(&p.root),
        _ => None,
    }
}

fn positional(c: &Call) -> Vec<&Expr> {
    c.args.iter().filter(|a| a.name.is_none()).filter_map(|a| match &a.value {
        Value::Expr(e) => Some(e),
        _ => None,
    }).collect()
}

fn named<'c>(c: &'c Call, key: &str) -> Option<&'c Expr> {
    c.args.iter().find(|a| a.name.as_deref() == Some(key)).and_then(|a| match &a.value {
        Value::Expr(e) => Some(e),
        _ => None,
    })
}

fn range(e: Option<&Expr>) -> (f64, f64) {
    let n = |l: &Lit| match l {
        Lit::Int(i) => *i as f64,
        Lit::Float(f) => f.parse().unwrap_or(0.0),
        _ => 0.0,
    };
    match e {
        Some(Expr::Lit(Lit::Range(a, b))) => (n(a), n(b)),
        _ => (0.0, 1.0),
    }
}

/// `scale`: a number into `[0, 1]`, clamped; a log scale gives nothing for zero and below.
pub fn scale(x: f64, log: bool, lo: f64, hi: f64) -> Option<f64> {
    if log {
        if x <= 0.0 {
            return None;
        }
        Some(((x.ln() - lo.ln()) / (hi.ln() - lo.ln())).clamp(0.0, 1.0))
    } else {
        Some(((x - lo) / (hi - lo)).clamp(0.0, 1.0))
    }
}

fn eval(e: &Expr, cx: &Ctx, key: &mut LayerKey) -> Val {
    match e {
        Expr::Lit(Lit::Int(i)) => Val::Int(*i),
        Expr::Lit(Lit::Float(f)) => Val::Num(f.parse().unwrap_or(0.0)),
        Expr::Lit(Lit::Amount(a)) => Val::Amount(*a),
        Expr::Lit(Lit::Bool(b)) => Val::Bool(*b),
        // A string is a shape's name (`@shape = "arrow";`); `ramp` reads its palette's name itself.
        Expr::Lit(Lit::Str(s)) => match s.as_str() {
            "dot" => Val::Glyph(Shape::Dot),
            "arrow" => Val::Glyph(Shape::Arrow),
            _ => Val::Later,
        },
        Expr::Lit(_) | Expr::List(_) | Expr::Capture(_) => Val::Later,
        Expr::Paren(x) => eval(x, cx, key),
        Expr::Neg(x) => match eval(x, cx, key) {
            Val::Int(i) => Val::Int(-i),
            v => Val::Num(-v.num()),
        },
        Expr::Not(x) => Val::Bool(eval(x, cx, key) != Val::Bool(true)),
        Expr::Binary(op, a, b) => {
            let (x, y) = (eval(a, cx, key), eval(b, cx, key));
            if x == Val::Later || y == Val::Later {
                return Val::Later;
            }
            let (p, q) = (x.num(), y.num());
            match op {
                BinOp::Or => Val::Bool(x == Val::Bool(true) || y == Val::Bool(true)),
                BinOp::And => Val::Bool(x == Val::Bool(true) && y == Val::Bool(true)),
                BinOp::Eq => Val::Bool(x == y || p == q),
                BinOp::Ne => Val::Bool(x != y && p != q),
                BinOp::Lt => Val::Bool(p < q),
                BinOp::Le => Val::Bool(p <= q),
                BinOp::Gt => Val::Bool(p > q),
                BinOp::Ge => Val::Bool(p >= q),
                BinOp::Add | BinOp::Sub | BinOp::Mul if matches!((x, y), (Val::Int(_), Val::Int(_))) => {
                    let (i, j) = (p as i64, q as i64);
                    Val::Int(match op {
                        BinOp::Add => i + j,
                        BinOp::Sub => i - j,
                        _ => i * j,
                    })
                }
                BinOp::Add => Val::Num(p + q),
                BinOp::Sub => Val::Num(p - q),
                BinOp::Mul => Val::Num(p * q),
                BinOp::Div => Val::Num(if q == 0.0 { 0.0 } else { p / q }),
            }
        }
        Expr::Path(p) => {
            if let Some(v) = cx.locals.and_then(|l| l.get(&p.root)) {
                return *v;
            }
            // `@energy` reads what `energy` reads; `@P` is the position; a cell also has `@ix`, `@iy`.
            let name = match p.root.strip_prefix('@') {
                Some("P") => "position",
                Some(a) => a,
                None => p.root.as_str(),
            };
            let f = cx.frame;
            if cx.agent.is_none() && cx.value.is_none() {
                match name {
                    "ix" => return Val::Int((cx.cell % f.grid_w) as i64),
                    "iy" => return Val::Int((cx.cell / f.grid_w) as i64),
                    other if p.root.starts_with('@') && other != "value" => {
                        if let Some(s) = f.substance(other) {
                            return Val::Amount(f.cell(cx.cell, s) as u64);
                        }
                    }
                    _ => {}
                }
            }
            if let Some(a) = cx.agent {
                match name {
                    "heading" => Val::Vec2(a.heading),
                    "position" => Val::Vec2(a.pos),
                    "id" => Val::Cat(a.id),
                    "lineage_id" => Val::Cat(a.lineage as u64),
                    "age" => Val::Int(a.age as i64),
                    "expressed_genes" => Val::Int(a.expressed_genes as i64),
                    "made" => match &f.made {
                        Some(m) => {
                            // The legend names the window, so a picture of producers says what it counted.
                            if !key.entries.iter().any(|e| e.starts_with("made:")) {
                                key.entries.push(format!("made: {} made by its own reactions, ticks {}–{}", m.substance, m.from_tick, f.tick));
                            }
                            Val::Amount(a.made)
                        }
                        None => {
                            if !key.unavailable.iter().any(|u| u == "made") {
                                key.unavailable.push("made".into());
                            }
                            Val::Later
                        }
                    },
                    other => match f.substance(other) {
                        Some(s) => Val::Amount(a.amounts[s] as u64),
                        None => {
                            if !key.unavailable.iter().any(|u| u == other) {
                                key.unavailable.push(other.to_string());
                            }
                            Val::Later
                        }
                    },
                }
            } else {
                match (name, cx.value, cx.state, cx.substance) {
                    ("value", Some(v), _, _) => Val::Num(v),
                    ("value", None, Some(k), _) => Val::Num(f.states[k].cells[cx.cell] as f64),
                    ("value", None, None, Some(s)) => Val::Num(f.cell(cx.cell, s) as f64 / 1000.0),
                    _ => Val::Later,
                }
            }
        }
        Expr::Call(c) => call(c, None, cx, key),
        Expr::Pipe(x, c) => {
            let v = eval(x, cx, key);
            call(c, Some(v), cx, key)
        }
    }
}

fn call(c: &Call, piped: Option<Val>, cx: &Ctx, key: &mut LayerKey) -> Val {
    let pos = positional(c);
    let (subject, rest) = match piped {
        Some(v) => (v, &pos[..]),
        None => match pos.split_first() {
            Some((s, rest)) => (eval(s, cx, key), rest),
            None => (Val::Later, &pos[..]),
        },
    };
    match c.name.as_str() {
        "units" => match subject {
            Val::Amount(a) => Val::Num(a as f64 / 1000.0),
            v => v,
        },
        "float" => Val::Num(subject.num()),
        "scale" => {
            if subject == Val::Later {
                return Val::Later;
            }
            let log = rest.first().and_then(|e| name_of(e)) == Some("log");
            let (lo, hi) = range(named(c, "range"));
            Val::Scaled(scale(subject.num(), log, lo, hi))
        }
        "palette" => call_palette(rest.first().and_then(|e| name_of(e)).unwrap_or("viridis"), subject, key),
        "arrow" => Val::Glyph(Shape::Arrow),
        "dot" => Val::Glyph(Shape::Dot),
        "ramp" => {
            // `ramp("magma", t)`: the palette's name is the first argument, a string.
            let name = match pos.first() {
                Some(Expr::Lit(Lit::Str(n))) => n.clone(),
                _ => return Val::Color(UNAVAILABLE),
            };
            let t = match pos.get(1) {
                Some(e) => eval(e, cx, key),
                None => Val::Later,
            };
            call_palette(&name, t, key)
        }
        "fit" | "fit_log" | "clamp" | "lerp" | "min" | "max" | "abs" | "log" | "sqrt" => {
            let args: Vec<Val> = pos.iter().map(|e| eval(e, cx, key)).collect();
            if args.contains(&Val::Later) {
                return Val::Later;
            }
            let n = |k: usize| args.get(k).map_or(0.0, |v| v.num());
            match c.name.as_str() {
                // VEX's fit: clamp to the old range, then map; `fit(x, a, b, 0.0, 1.0)` is `scale(linear)`.
                "fit" => {
                    let (a, b, lo, hi) = (n(1), n(2), n(3), n(4));
                    let t = if b == a { 0.0 } else { (n(0).clamp(a.min(b), a.max(b)) - a) / (b - a) };
                    Val::Num(t * (hi - lo) + lo)
                }
                // `scale(log)`: nothing to show at or below zero.
                "fit_log" => Val::Scaled(scale(n(0), true, n(1), n(2))),
                "clamp" => Val::Num(n(0).clamp(n(1).min(n(2)), n(1).max(n(2)))),
                "lerp" => Val::Num(n(0) + (n(1) - n(0)) * n(2)),
                "min" => Val::Num(n(0).min(n(1))),
                "max" => Val::Num(n(0).max(n(1))),
                "abs" => Val::Num(n(0).abs()),
                "log" => Val::Num(if n(0) > 0.0 { n(0).ln() } else { 0.0 }),
                _ => Val::Num(n(0).max(0.0).sqrt()),
            }
        }
        _ => Val::Later,
    }
}

/// A palette sampled at a value, as `palette` and `ramp` do.
fn call_palette(name: &str, subject: Val, key: &mut LayerKey) -> Val {
    match subject {
        Val::Later => Val::Color(UNAVAILABLE),
        Val::Scaled(None) => {
            key.empty = true;
            Val::Color(EMPTY)
        }
        Val::Scaled(Some(t)) => Val::Color(palette::sample(name, t)),
        Val::Cat(i) => Val::Color(palette::category(i)),
        Val::Int(i) if name == "categorical" => Val::Color(palette::category(i.max(0) as u64)),
        Val::Vec2([x, y]) => Val::Color(palette::sample(name, (f64::from(y).atan2(f64::from(x)) / std::f64::consts::TAU).rem_euclid(1.0))),
        v => Val::Color(palette::sample(name, v.num())),
    }
}

/// The legend's lines for a layer body: each write to an attribute, as written.
fn body_lines(body: &[Node<VStmt>], key: &mut LayerKey) {
    for s in body {
        match &s.node {
            VStmt::Set { attr, value } => key.entries.push(format!("@{attr} = {}", emerge_lang::fmt::value_text(&Value::Expr(value.clone())))),
            VStmt::Local { ty, name, value } => key.entries.push(format!("{ty} {name} = {}", emerge_lang::fmt::value_text(&Value::Expr(value.clone())))),
            VStmt::If { cond, then, otherwise } => {
                key.entries.push(format!("if ({}) …", emerge_lang::fmt::value_text(&Value::Expr(cond.clone()))));
                body_lines(then, key);
                if let Some(o) = otherwise {
                    body_lines(o, key);
                }
            }
        }
    }
}

/// Runs a layer body for one element: locals, writes to its visual attributes, `if`.
fn run_body(body: &[Node<VStmt>], base: &Ctx, at: &mut Attributes, locals: &mut std::collections::HashMap<String, Val>, key: &mut LayerKey) {
    for s in body {
        let value = |e: &Expr, locals: &std::collections::HashMap<String, Val>, key: &mut LayerKey| {
            let cx = Ctx { frame: base.frame, cell: base.cell, substance: base.substance, state: base.state, value: base.value, agent: base.agent, locals: Some(locals) };
            eval(e, &cx, key)
        };
        match &s.node {
            VStmt::Local { name, value: e, .. } => {
                let v = value(e, locals, key);
                locals.insert(name.clone(), v);
            }
            VStmt::Set { attr, value: e } => {
                let v = value(e, locals, key);
                match (attr.as_str(), v) {
                    ("Cd", Val::Color(c)) => at.cd = c,
                    ("Cd", _) => at.cd = UNAVAILABLE,
                    ("Alpha", v) if v != Val::Later => at.alpha = (match v { Val::Scaled(Some(t)) => t, _ => v.num() }).clamp(0.0, 1.0) as f32,
                    ("pscale", v) if v != Val::Later => at.pscale = v.num() as f32,
                    ("shape", Val::Glyph(sh)) => at.shape = sh,
                    ("N", Val::Vec2(n)) => at.n = n,
                    _ => {}
                }
            }
            VStmt::If { cond, then, otherwise } => {
                if value(cond, locals, key) == Val::Bool(true) {
                    run_body(then, base, at, locals, key);
                } else if let Some(o) = otherwise {
                    run_body(o, base, at, locals, key);
                }
            }
        }
    }
}

/// How large the picture is: pixels per cell, and the legend's width.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Pixels on a side of one cell; 0 picks about 1_024 pixels across.
    pub cell_px: u32,
    pub legend: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { cell_px: 0, legend: true }
    }
}

pub const LEGEND_W: u32 = 360;
const PANEL: Rgb = [0x0e, 0x11, 0x16];
const INK: Rgb = [0xe8, 0xea, 0xee];
const MUTED: Rgb = [0x9a, 0xa0, 0xaa];

fn paint(c: Rgb, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], (alpha.clamp(0.0, 1.0) * 255.0).round() as u8);
    p.anti_alias = true;
    p
}

/// The colours of every cell for a field layer, and what its legend says.
pub fn field_layer(layer: &Layer, frame: &Frame) -> (Vec<(Rgb, f32)>, LayerKey) {
    let mut key = LayerKey { name: layer.name.clone(), ..LayerKey::default() };
    let state = match positional(&layer.source).first() {
        Some(Expr::Path(p)) if p.segs.is_empty() => frame.states.iter().position(|s| s.name == p.root),
        _ => None,
    };
    let substance = match positional(&layer.source).first() {
        Some(Expr::Path(_)) if state.is_some() => None,
        // `context(cells)`: the conserved field's cells, read by substance in the body.
        Some(Expr::Path(p)) if layer.source.name == "context" && p.segs.is_empty() && p.root == frame.field => {
            key.source = format!("{} (cells, by substance)", p.root);
            None
        }
        Some(Expr::Path(p)) => match p.segs.as_slice() {
            [Seg::Field(s)] => {
                key.source = format!("{}.{s}, in units per cell", p.root);
                frame.substance(s)
            }
            _ => {
                key.source = format!("{} (not run yet)", p.root);
                key.unavailable.push(p.root.clone());
                None
            }
        },
        _ => None,
    };
    let color = layer.entries.iter().find(|e| e.node.key == "color").and_then(|e| match &e.node.value {
        Value::Expr(x) => Some(x),
        _ => None,
    });
    if let Some(Expr::Pipe(inner, pal)) = color {
        let pname = positional(pal).first().and_then(|e| name_of(e)).unwrap_or("viridis").to_string();
        if let Expr::Pipe(_, sc) = inner.as_ref() {
            if sc.name == "scale" {
                let (lo, hi) = range(named(sc, "range"));
                let log = positional(sc).first().and_then(|e| name_of(e)) == Some("log");
                key.ramp = Some((pname, lo, hi, log));
            }
        } else {
            key.ramp = Some((pname, 0.0, 1.0, false));
        }
    }
    if let Some(k) = state {
        // A state field: `value` is the state's index, and the legend shows each state's colour.
        let st = &frame.states[k];
        key.source = format!("{}, a state field (value: {})", st.name, st.names.iter().enumerate().map(|(i, n)| format!("{n} {i}")).collect::<Vec<_>>().join(", "));
        key.ramp = None;
        if let Some(e) = color {
            for (i, name) in st.names.iter().enumerate() {
                let cx = Ctx { frame, cell: 0, substance: None, state, value: Some(i as f64), agent: None, locals: None };
                if let Val::Color(c) = eval(e, &cx, &mut key) {
                    key.categories.push((name.clone(), c));
                }
            }
        }
    }
    body_lines(&layer.body, &mut key);
    let n = frame.grid_w * frame.grid_h;
    let mut out = Vec::with_capacity(n);
    let mut locals = std::collections::HashMap::new();
    for cell in 0..n {
        let cx = Ctx { frame, cell, substance, state, value: None, agent: None, locals: None };
        let cd = match (color, substance.or(state)) {
            (Some(e), Some(_)) => match eval(e, &cx, &mut key) {
                Val::Color(c) => c,
                _ => UNAVAILABLE,
            },
            _ if !layer.body.is_empty() => EMPTY,
            _ => UNAVAILABLE,
        };
        let mut at = Attributes { cd, alpha: 1.0, pscale: 1.0, shape: Shape::Dot, n: [1.0, 0.0] };
        if !layer.body.is_empty() {
            locals.clear();
            run_body(&layer.body, &cx, &mut at, &mut locals, &mut key);
        }
        out.push((at.cd, at.alpha));
    }
    key.count = n;
    (out, key)
}

/// The attributes of every agent an agents layer shows (after its `where`), and its legend.
pub fn agents_layer(layer: &Layer, frame: &Frame) -> (Vec<([f32; 2], Attributes)>, LayerKey) {
    let mut key = LayerKey { name: layer.name.clone(), source: "agents".into(), ..LayerKey::default() };
    let entry = |k: &str| layer.entries.iter().find(|e| e.node.key == k).and_then(|e| match &e.node.value {
        Value::Expr(x) => Some(x),
        _ => None,
    });
    let filter = named(&layer.source, "where");
    if let Some(f) = filter {
        key.source = format!("agents where {}", emerge_lang::fmt::value_text(&Value::Expr(f.clone())));
    }
    for k in ["color", "size", "glyph"] {
        if let Some(e) = entry(k) {
            key.entries.push(format!("{k}: {}", emerge_lang::fmt::value_text(&Value::Expr(e.clone()))));
        }
    }
    let mut seen = std::collections::BTreeMap::new();
    let mut out = Vec::new();
    body_lines(&layer.body, &mut key);
    let mut locals = std::collections::HashMap::new();
    // A body that colours by `ramp("categorical", @lineage_id)` gets the lineages in its legend.
    let by_lineage = layer.body.iter().any(|s| matches!(&s.node, VStmt::Set { attr, value: Expr::Call(c) } if attr == "Cd" && c.name == "ramp"
        && matches!(positional(c).as_slice(), [Expr::Lit(Lit::Str(n)), Expr::Path(p)] if n == "categorical" && p.root == "@lineage_id")));
    for a in &frame.agents {
        let cx = Ctx { frame, cell: 0, substance: None, state: None, value: None, agent: Some(a), locals: None };
        if let Some(f) = filter {
            if eval(f, &cx, &mut key) != Val::Bool(true) {
                continue;
            }
        }
        let mut at = Attributes { cd: [0xf2, 0xf2, 0xf2], alpha: 1.0, pscale: 1.0, shape: Shape::Dot, n: a.heading };
        if let Some(e) = entry("color") {
            at.cd = match eval(e, &cx, &mut key) {
                Val::Color(c) => c,
                _ => UNAVAILABLE,
            };
            if let Expr::Pipe(x, _) = e {
                if matches!(x.as_ref(), Expr::Path(p) if p.root == "lineage_id") {
                    seen.insert(a.lineage, at.cd);
                }
            }
        }
        if let Some(e) = entry("size") {
            let v = eval(e, &cx, &mut key);
            at.pscale = if v == Val::Later { 1.0 } else { v.num() as f32 };
        }
        if let Some(Val::Glyph(s)) = entry("glyph").map(|e| eval(e, &cx, &mut key)) {
            at.shape = s;
        }
        if !layer.body.is_empty() {
            locals.clear();
            run_body(&layer.body, &cx, &mut at, &mut locals, &mut key);
            if by_lineage {
                seen.insert(a.lineage, at.cd);
            }
        }
        out.push((a.pos, at));
    }
    key.categories = seen.into_iter().map(|(l, c)| (frame.lineages.get(l).cloned().unwrap_or(format!("lineage {l}")), c)).collect();
    key.count = out.len();
    (out, key)
}

/// Renders `view` of `frame`. The picture is the grid at `cell_px` pixels per cell, with the
/// legend to its right.
pub fn render(view: &View, frame: &Frame, opt: Options) -> Pixmap {
    render_into(view, frame, opt, 0).0
}

/// `render`, with a strip of charts (`chart::Panel`s, drawn from the frame's history) under it.
pub fn render_with(view: &View, frame: &Frame, opt: Options, panels: &[crate::chart::Panel]) -> Pixmap {
    if panels.is_empty() {
        return render(view, frame, opt);
    }
    let (mut pm, main_h) = render_into(view, frame, opt, crate::chart::STRIP_H);
    crate::chart::draw_strip(&mut pm, main_h, panels, frame);
    pm
}

/// The picture, `extra` pixels taller than it needs, and the height of the picture itself.
fn render_into(view: &View, frame: &Frame, opt: Options, extra: u32) -> (Pixmap, u32) {
    let k = if opt.cell_px > 0 { opt.cell_px } else { (1024 / frame.grid_w.max(1) as u32).max(1) };
    let (w, h) = (frame.grid_w as u32 * k, frame.grid_h as u32 * k);
    let total_w = w + if opt.legend { LEGEND_W } else { 0 };
    let main_h = h.max(if opt.legend { 520 } else { 1 });
    let mut pm = Pixmap::new(total_w, main_h + extra).expect("an image of this size");
    pm.fill(tiny_skia::Color::from_rgba8(EMPTY[0], EMPTY[1], EMPTY[2], 255));
    let mut keys = Vec::new();
    // World units to pixels.
    let (sx, sy) = (w as f32 / frame.world_w, h as f32 / frame.world_h);
    for layer in &view.layers {
        let l = &layer.node;
        let (blend, opacity) = blend_of(l);
        // The usual layer draws straight onto the picture; any other blend or opacity draws into
        // its own transparent layer, composited per pixel.
        let direct = blend == "over" && opacity >= 1.0;
        let mut own = if direct { None } else { Some(Pixmap::new(w.max(1), h.max(1)).expect("a layer of this size")) };
        let target = own.as_mut().unwrap_or(&mut pm);
        let kind = match (l.source.name.as_str(), positional(&l.source).first()) {
            ("context", Some(Expr::Path(p))) if p.root == "agents" => "agents",
            ("context", _) => "field",
            (other, _) => other,
        };
        match kind {
            "field" => {
                let (cells, key) = field_layer(l, frame);
                for (c, (cd, alpha)) in cells.iter().enumerate() {
                    let (x, y) = ((c % frame.grid_w) as u32 * k, (c / frame.grid_w) as u32 * k);
                    let mut p = paint(*cd, *alpha);
                    p.anti_alias = false; // exact cells
                    target.fill_rect(Rect::from_xywh(x as f32, y as f32, k as f32, k as f32).unwrap(), &p, Transform::identity(), None);
                }
                keys.push(key);
            }
            "agents" => {
                let (agents, key) = agents_layer(l, frame);
                for (pos, at) in &agents {
                    draw_agent(target, [pos[0] * sx, pos[1] * sy], at, sx.min(sy), w as f32, h as f32, frame.wrap);
                }
                keys.push(key);
            }
            _ => {}
        }
        if let Some(layer_pm) = own {
            composite(&mut pm, &layer_pm, blend, opacity);
        }
    }
    if opt.legend {
        legend(&mut pm, w, view, frame, &keys);
    }
    (pm, main_h)
}

/// A layer's `blend` (default `over`) and `opacity` (default 1).
fn blend_of(l: &Layer) -> (&str, f32) {
    let entry = |k: &str| l.entries.iter().find(|e| e.node.key == k).and_then(|e| match &e.node.value {
        Value::Expr(x) => Some(x),
        _ => None,
    });
    let blend = match entry("blend") {
        Some(Expr::Path(p)) => p.root.as_str(),
        _ => "over",
    };
    let opacity = match entry("opacity") {
        Some(Expr::Lit(Lit::Float(f))) => f.parse().unwrap_or(1.0),
        Some(Expr::Lit(Lit::Int(i))) => *i as f32,
        _ => 1.0,
    };
    (blend, opacity.clamp(0.0, 1.0))
}

/// One channel of a blend, on values in [0, 1]: `s` the layer's colour premultiplied by its
/// alpha `a` (already scaled by the opacity), `d` the opaque picture below.
pub fn blend_channel(mode: &str, s: f32, a: f32, d: f32) -> f32 {
    match mode {
        "add" => (d + s).min(1.0),
        "subtract" => (d - s).max(0.0),
        "multiply" => s * d + d * (1.0 - a),
        "screen" => s + d - s * d,
        _ => s + d * (1.0 - a),
    }
}

/// Composites `layer` (premultiplied RGBA) onto the opaque top-left of `pm`.
fn composite(pm: &mut Pixmap, layer: &Pixmap, mode: &str, opacity: f32) {
    let (w, h, pw) = (layer.width() as usize, layer.height() as usize, pm.width() as usize);
    let (src, dst) = (layer.data(), pm.data_mut());
    for y in 0..h {
        for x in 0..w {
            let (i, j) = (4 * (y * w + x), 4 * (y * pw + x));
            let a = src[i + 3] as f32 / 255.0 * opacity;
            if a == 0.0 && mode != "multiply" {
                continue;
            }
            for c in 0..3 {
                let s = src[i + c] as f32 / 255.0 * opacity;
                let d = dst[j + c] as f32 / 255.0;
                dst[j + c] = (blend_channel(mode, s, a, d) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn draw_agent(pm: &mut Pixmap, p: [f32; 2], at: &Attributes, px_per_unit: f32, w: f32, h: f32, wrap: bool) {
    let size = (at.pscale * px_per_unit).max(1.5);
    let paint = paint(at.cd, at.alpha);
    // A glyph across a wrapping edge is drawn on both sides.
    let shifts: Vec<(f32, f32)> = if wrap {
        let xs = [0.0, if p[0] < size { w } else if p[0] > w - size { -w } else { 0.0 }];
        let ys = [0.0, if p[1] < size { h } else if p[1] > h - size { -h } else { 0.0 }];
        let mut v = vec![(0.0, 0.0), (xs[1], 0.0), (0.0, ys[1]), (xs[1], ys[1])];
        v.dedup();
        v
    } else {
        vec![(0.0, 0.0)]
    };
    for (dx, dy) in shifts {
        let (x, y) = (p[0] + dx, p[1] + dy);
        let path = match at.shape {
            Shape::Dot => PathBuilder::from_circle(x, y, size * 0.5),
            Shape::Arrow => {
                let (nx, ny) = (at.n[0], at.n[1]);
                let len = (nx * nx + ny * ny).sqrt().max(1e-6);
                let (ux, uy) = (nx / len, ny / len);
                let mut pb = PathBuilder::new();
                pb.move_to(x + ux * size * 0.6, y + uy * size * 0.6);
                pb.line_to(x - ux * size * 0.4 - uy * size * 0.32, y - uy * size * 0.4 + ux * size * 0.32);
                pb.line_to(x - ux * size * 0.2, y - uy * size * 0.2);
                pb.line_to(x - ux * size * 0.4 + uy * size * 0.32, y - uy * size * 0.4 - ux * size * 0.32);
                pb.close();
                pb.finish()
            }
        };
        if let Some(path) = path {
            pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
}

fn swatch(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Rgb) {
    let mut p = paint(c, 1.0);
    p.anti_alias = false;
    pm.fill_rect(Rect::from_xywh(x, y, w, h).unwrap(), &p, Transform::identity(), None);
}

fn number(x: f64) -> String {
    if x.abs() >= 1000.0 && x.fract() == 0.0 {
        let s = format!("{}", x as i64);
        let mut out = String::new();
        for (i, ch) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 && ch != '-' {
                out.push('_');
            }
            out.push(ch);
        }
        out
    } else if x.fract() == 0.0 {
        format!("{x:.0}")
    } else {
        format!("{x}")
    }
}

fn legend(pm: &mut Pixmap, x0: u32, view: &View, frame: &Frame, keys: &[LayerKey]) {
    let x = x0 as f32;
    swatch(pm, x, 0.0, LEGEND_W as f32, pm.height() as f32, PANEL);
    let t = Text::new();
    let (lx, wrap_w) = (x + 22.0, LEGEND_W as f32 - 44.0);
    let mut y = 34.0;
    t.draw(pm, lx, y, 22.0, &format!("{} · {}", view.name, view.world), INK);
    y += 24.0;
    t.draw(pm, lx, y, 13.0, &format!("tick {}   revision {}   {} agents", frame.tick, frame.revision, frame.agents.len()), MUTED);
    y += 16.0;
    t.draw(pm, lx, y, 13.0, &format!("{} × {} cells{}", frame.grid_w, frame.grid_h, if frame.wrap { ", wrapping" } else { "" }), MUTED);
    y += 30.0;
    for key in keys {
        t.draw(pm, lx, y, 15.0, &key.name, INK);
        y += 18.0;
        y = t.wrapped(pm, (lx, y), 12.5, &key.source, MUTED, wrap_w);
        y += 6.0;
        if let Some((pal, lo, hi, log)) = &key.ramp {
            let bw = wrap_w;
            for i in 0..bw as u32 {
                let c = palette::sample(pal, i as f64 / (bw - 1.0) as f64);
                swatch(pm, lx + i as f32, y, 1.0, 14.0, c);
            }
            y += 30.0;
            t.draw(pm, lx, y, 12.0, &number(*lo), MUTED);
            let hi_text = number(*hi);
            t.draw(pm, lx + bw - t.width(12.0, &hi_text), y, 12.0, &hi_text, MUTED);
            let mid = format!("{} {}", pal, if *log { "log" } else { "linear" });
            t.draw(pm, lx + bw * 0.5 - t.width(12.0, &mid) * 0.5, y, 12.0, &mid, MUTED);
            y += 14.0;
            t.draw(pm, lx, y, 11.0, "values outside the range take the end colours", MUTED);
            y += 16.0;
        }
        if key.empty {
            swatch(pm, lx, y - 10.0, 14.0, 14.0, EMPTY);
            swatch(pm, lx, y - 10.0, 14.0, 1.0, MUTED);
            t.draw(pm, lx + 22.0, y + 1.0, 12.0, "none (zero, below a log scale)", MUTED);
            y += 20.0;
        }
        for (name, c) in &key.categories {
            swatch(pm, lx, y - 10.0, 14.0, 14.0, *c);
            t.draw(pm, lx + 22.0, y + 1.0, 12.5, name, INK);
            y += 20.0;
        }
        for e in &key.entries {
            y = t.wrapped(pm, (lx, y), 12.0, e, MUTED, wrap_w);
        }
        for u in &key.unavailable {
            swatch(pm, lx, y - 10.0, 14.0, 14.0, UNAVAILABLE);
            y = t.wrapped(pm, (lx + 22.0, y + 1.0), 12.0, &format!("{u}: not computed yet, drawn grey"), MUTED, wrap_w - 22.0);
        }
        if key.count > 0 && key.source.starts_with("agents") {
            t.draw(pm, lx, y, 12.0, &format!("{} shown", key.count), MUTED);
            y += 16.0;
        }
        y += 18.0;
    }
}
