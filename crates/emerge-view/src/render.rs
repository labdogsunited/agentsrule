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
            let name = p.root.as_str();
            let f = cx.frame;
            if let Some(a) = cx.agent {
                match name {
                    "heading" => Val::Vec2(a.heading),
                    "position" => Val::Vec2(a.pos),
                    "id" => Val::Cat(a.id),
                    "lineage_id" => Val::Cat(a.lineage as u64),
                    "age" => Val::Int(a.age as i64),
                    "expressed_genes" => Val::Int(a.expressed_genes as i64),
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
                match (name, cx.substance) {
                    ("value", Some(s)) => Val::Num(f.cell(cx.cell, s) as f64 / 1000.0),
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
        "palette" => {
            let name = rest.first().and_then(|e| name_of(e)).unwrap_or("viridis");
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
        "arrow" => Val::Glyph(Shape::Arrow),
        "dot" => Val::Glyph(Shape::Dot),
        _ => Val::Later,
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
    let substance = match positional(&layer.source).first() {
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
    let n = frame.grid_w * frame.grid_h;
    let mut out = Vec::with_capacity(n);
    for cell in 0..n {
        let cx = Ctx { frame, cell, substance, agent: None };
        let cd = match (color, substance) {
            (Some(e), Some(_)) => match eval(e, &cx, &mut key) {
                Val::Color(c) => c,
                _ => UNAVAILABLE,
            },
            _ => UNAVAILABLE,
        };
        out.push((cd, 1.0));
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
    for a in &frame.agents {
        let cx = Ctx { frame, cell: 0, substance: None, agent: Some(a) };
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
        out.push((a.pos, at));
    }
    key.categories = seen.into_iter().map(|(l, c)| (frame.lineages.get(l).cloned().unwrap_or(format!("lineage {l}")), c)).collect();
    key.count = out.len();
    (out, key)
}

/// Renders `view` of `frame`. The picture is the grid at `cell_px` pixels per cell, with the
/// legend to its right.
pub fn render(view: &View, frame: &Frame, opt: Options) -> Pixmap {
    let k = if opt.cell_px > 0 { opt.cell_px } else { (1024 / frame.grid_w.max(1) as u32).max(1) };
    let (w, h) = (frame.grid_w as u32 * k, frame.grid_h as u32 * k);
    let total_w = w + if opt.legend { LEGEND_W } else { 0 };
    let mut pm = Pixmap::new(total_w, h.max(if opt.legend { 520 } else { 1 })).expect("an image of this size");
    pm.fill(tiny_skia::Color::from_rgba8(EMPTY[0], EMPTY[1], EMPTY[2], 255));
    let mut keys = Vec::new();
    // World units to pixels.
    let (sx, sy) = (w as f32 / frame.world_w, h as f32 / frame.world_h);
    for layer in &view.layers {
        let l = &layer.node;
        match l.source.name.as_str() {
            "field" => {
                let (cells, key) = field_layer(l, frame);
                for (c, (cd, alpha)) in cells.iter().enumerate() {
                    let (x, y) = ((c % frame.grid_w) as u32 * k, (c / frame.grid_w) as u32 * k);
                    let mut p = paint(*cd, *alpha);
                    p.anti_alias = false; // exact cells
                    pm.fill_rect(Rect::from_xywh(x as f32, y as f32, k as f32, k as f32).unwrap(), &p, Transform::identity(), None);
                }
                keys.push(key);
            }
            "agents" => {
                let (agents, key) = agents_layer(l, frame);
                for (pos, at) in &agents {
                    draw_agent(&mut pm, [pos[0] * sx, pos[1] * sy], at, sx.min(sy), w as f32, h as f32, frame.wrap);
                }
                keys.push(key);
            }
            _ => {}
        }
    }
    if opt.legend {
        legend(&mut pm, w, view, frame, &keys);
    }
    pm
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
