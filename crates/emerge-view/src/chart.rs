//! Charts beside the picture (`proposals/plan-next.md` item 12, V4): a run's history drawn as small
//! line charts in a strip under the picture and its legend. A chart reads only the recorded
//! history; nothing here changes the world.

use crate::frame::{Frame, History};
use crate::palette;
use crate::text::Text;
use emerge_lang::ast::{Chart, Expr, Seg, Value};
use tiny_skia::{Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

type Rgb = [u8; 3];

/// Height of the strip of charts, in pixels.
pub const STRIP_H: u32 = 190;
const BG: Rgb = [0x0b, 0x0d, 0x11];
const INK: Rgb = [0xe8, 0xea, 0xee];
const MUTED: Rgb = [0x9a, 0xa0, 0xaa];
const GRID: Rgb = [0x2a, 0x2f, 0x38];
const BIRTHS: Rgb = [0x7b, 0xc4, 0x7f];
const DEATHS: Rgb = [0xe5, 0x73, 0x73];

/// What a strip shows: the default panel, or a `chart` block of the source.
pub enum Panel<'a> {
    Default,
    Block(&'a Chart),
}

/// One line: its label, colour and value at each sample.
pub struct Series {
    pub label: String,
    pub color: Rgb,
    pub values: Vec<f64>,
}

/// One chart: a title, its lines, and notes for what it cannot draw.
pub struct Plot {
    pub title: String,
    pub series: Vec<Series>,
    pub notes: Vec<String>,
}

fn population(h: &History, frame: &Frame, by_lineage: bool) -> Vec<Series> {
    if by_lineage {
        (0..frame.lineages.len())
            .map(|l| Series { label: frame.lineages[l].clone(), color: palette::category(l as u64), values: h.samples.iter().map(|s| *s.by_lineage.get(l).unwrap_or(&0) as f64).collect() })
            .collect()
    } else {
        vec![Series { label: "population".into(), color: INK, values: h.samples.iter().map(|s| s.by_lineage.iter().sum::<u64>() as f64).collect() }]
    }
}

fn counts(h: &History, births: bool) -> Series {
    let (label, color) = if births { ("births", BIRTHS) } else { ("deaths", DEATHS) };
    let per = if h.every == 1 { "tick".to_string() } else { format!("{} ticks", h.every) };
    Series { label: format!("{label} per {per}"), color, values: h.samples.iter().map(|s| (if births { s.births } else { s.deaths }) as f64).collect() }
}

/// The ledger check over the history, in words.
fn ledger(h: &History) -> String {
    let audited: Vec<i128> = h.samples.iter().filter_map(|s| s.drift).collect();
    if audited.is_empty() {
        return "conservation: not audited".into();
    }
    let off = audited.iter().filter(|&&d| d != 0).count();
    let worst = audited.iter().map(|d| d.abs()).max().unwrap_or(0);
    if off == 0 {
        format!("conservation: exact (0 µ of any element) in all {} audited samples", audited.len())
    } else {
        format!("conservation: OFF in {off} of {} samples, at most {worst} µ", audited.len())
    }
}

/// The charts a panel draws from a history.
pub fn plots(panel: &Panel, h: &History, frame: &Frame) -> Vec<Plot> {
    match panel {
        Panel::Default => vec![
            Plot { title: "population by lineage".into(), series: population(h, frame, true), notes: Vec::new() },
            Plot { title: "births and deaths".into(), series: vec![counts(h, true), counts(h, false)], notes: Vec::new() },
            Plot { title: "ledger".into(), series: Vec::new(), notes: vec![ledger(h)] },
        ],
        Panel::Block(c) => {
            let mut p = Plot { title: c.name.clone(), series: Vec::new(), notes: Vec::new() };
            for s in &c.series {
                let call = &s.node;
                let subject = call.args.iter().find(|a| a.name.is_none()).and_then(|a| match &a.value {
                    Value::Expr(Expr::Path(p)) => Some(p),
                    _ => None,
                });
                let by_lineage = call.args.iter().any(|a| a.name.as_deref() == Some("by") && matches!(&a.value, Value::Expr(Expr::Path(p)) if p.root == "lineage"));
                let by_species = call.args.iter().any(|a| a.name.as_deref() == Some("by") && matches!(&a.value, Value::Expr(Expr::Path(p)) if p.root == "species"));
                match (call.name.as_str(), subject.map(|p| (p.root.as_str(), &p.segs))) {
                    ("line", Some(("population", _))) if by_species => p.notes.push("population by species: not computed yet".into()),
                    ("line", Some(("population", _))) => p.series.extend(population(h, frame, by_lineage)),
                    ("line", Some(("births", _))) => p.series.push(counts(h, true)),
                    ("line", Some(("deaths", _))) => p.series.push(counts(h, false)),
                    ("line", Some(("metrics", segs))) => {
                        let name: String = segs.iter().map(|g| match g { Seg::Field(f) | Seg::Label(f) => format!(".{f}") }).collect();
                        p.notes.push(format!("metrics{name}: not computed yet"));
                    }
                    ("ledger", _) => p.notes.push(ledger(h)),
                    _ => p.notes.push(format!("`{}` cannot be drawn", call.name)),
                }
            }
            vec![p]
        }
    }
}

fn fill(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Rgb) {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], 255);
    if let Some(r) = Rect::from_xywh(x, y, w.max(1.0), h.max(1.0)) {
        pm.fill_rect(r, &p, Transform::identity(), None);
    }
}

/// Where sample `i` of `n`, and value `v` of a range up to `top`, land in a plot area.
pub fn place(area: (f32, f32, f32, f32), i: usize, n: usize, v: f64, top: f64) -> (f32, f32) {
    let (x, y, w, h) = area;
    let fx = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 };
    let fy = if top > 0.0 { (v / top) as f32 } else { 0.0 };
    (x + fx * w, y + h - fy * h)
}

/// The plot area of chart `k` of `count` in a strip `width` wide starting at `y0`.
pub fn area(k: usize, count: usize, width: u32, y0: u32) -> (f32, f32, f32, f32) {
    let cw = width as f32 / count as f32;
    (k as f32 * cw + 52.0, y0 as f32 + 40.0, cw - 76.0, STRIP_H as f32 - 76.0)
}

/// Draws the charts of `panels` from the frame's history into the strip at `y0`.
pub fn draw_strip(pm: &mut Pixmap, y0: u32, panels: &[Panel], frame: &Frame) {
    let width = pm.width();
    fill(pm, 0.0, y0 as f32, width as f32, STRIP_H as f32, BG);
    let t = Text::new();
    let Some(h) = frame.history.as_ref().filter(|h| !h.samples.is_empty()) else {
        t.draw(pm, 20.0, y0 as f32 + 30.0, 13.0, "no history recorded for this frame", MUTED);
        return;
    };
    let all: Vec<Plot> = panels.iter().flat_map(|p| plots(p, h, frame)).collect();
    let (first, last) = (h.samples[0].tick, h.samples[h.samples.len() - 1].tick);
    for (k, plot) in all.iter().enumerate() {
        let (x, y, w, ph) = area(k, all.len(), width, y0);
        t.draw(pm, x - 32.0, y0 as f32 + 22.0, 14.0, &plot.title, INK);
        if plot.series.is_empty() {
            let mut ny = y + 14.0;
            for n in &plot.notes {
                ny = t.wrapped(pm, (x - 32.0, ny), 12.5, n, MUTED, w + 40.0) + 4.0;
            }
            continue;
        }
        let top = plot.series.iter().flat_map(|s| s.values.iter().copied()).fold(0.0f64, f64::max).max(1.0);
        fill(pm, x, y + ph, w, 1.0, GRID);
        fill(pm, x, y, 1.0, ph, GRID);
        t.draw(pm, x - 6.0 - t.width(11.0, &number(top)), y + 8.0, 11.0, &number(top), MUTED);
        t.draw(pm, x - 6.0 - t.width(11.0, "0"), y + ph + 4.0, 11.0, "0", MUTED);
        t.draw(pm, x, y + ph + 16.0, 11.0, &format!("tick {first}"), MUTED);
        let end = format!("tick {last}");
        t.draw(pm, x + w - t.width(11.0, &end), y + ph + 16.0, 11.0, &end, MUTED);
        let mut lx = x;
        for s in &plot.series {
            let mut pb = PathBuilder::new();
            for (i, &v) in s.values.iter().enumerate() {
                let (px, py) = place((x, y, w, ph), i, s.values.len(), v, top);
                if i == 0 { pb.move_to(px, py) } else { pb.line_to(px, py) }
            }
            if s.values.len() == 1 {
                let (px, py) = place((x, y, w, ph), 0, 1, s.values[0], top);
                pb.line_to(px + 0.5, py);
            }
            if let Some(path) = pb.finish() {
                let mut p = Paint::default();
                p.set_color_rgba8(s.color[0], s.color[1], s.color[2], 255);
                p.anti_alias = true;
                pm.stroke_path(&path, &p, &Stroke { width: 1.6, ..Stroke::default() }, Transform::identity(), None);
            }
            fill(pm, lx, y + ph + 24.0, 10.0, 10.0, s.color);
            t.draw(pm, lx + 14.0, y + ph + 33.0, 11.0, &s.label, MUTED);
            lx += 24.0 + t.width(11.0, &s.label);
        }
        let mut ny = y - 6.0;
        for n in &plot.notes {
            t.draw(pm, x + 6.0, ny + 14.0, 11.0, n, MUTED);
            ny += 14.0;
        }
    }
}

fn number(v: f64) -> String {
    if v >= 10_000.0 { format!("{:.0}k", v / 1000.0) } else { format!("{v:.0}") }
}
