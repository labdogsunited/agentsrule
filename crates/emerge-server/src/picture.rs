//! Pictures of a run: the view to draw (the run's own views, then any given as source), and the
//! PNG of a frame. `emerge-server render` and the live viewer both use these, so a live frame and
//! an exported one are the same bytes (`proposals/live-viewer.md`).

use crate::run::Run;
use emerge_lang::ast::{Chart, Item, View};

/// The charts to draw under a picture: `default` is the default panel, any other name a `chart`
/// block of the run's source or of `extra`. Empty when nothing is asked for.
#[derive(Debug, Clone)]
pub enum Charts {
    Default,
    Block(Chart),
}

pub fn pick_charts(run: &Run, extra: Option<&str>, names: &[String]) -> Result<Vec<Charts>, String> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let mut file = emerge_lang::parse(&run.live.source).map_err(|d| d.message)?;
    if let Some(src) = extra {
        file.items.extend(emerge_lang::parse(src).map_err(|d| format!("views:{}:{}: {}", d.line, d.col, d.message))?.items);
    }
    let blocks: Vec<Chart> = file.items.into_iter().filter_map(|i| match i.node {
        Item::Chart(c) => Some(c),
        _ => None,
    }).collect();
    names.iter().map(|n| match n.as_str() {
        "default" => Ok(Charts::Default),
        n => blocks.iter().rev().find(|c| c.name == n).cloned().map(Charts::Block).ok_or(format!("no chart `{n}` (use `default` for the default panel)")),
    }).collect()
}

/// The view named `name` (the last one by that name), or the last view, among the run's own views
/// followed by those of `extra`; all checked against the run's world.
pub fn pick_view(run: &Run, extra: Option<&str>, name: Option<&str>) -> Result<View, String> {
    let mut file = emerge_lang::parse(&run.live.source).map_err(|d| d.message)?;
    if let Some(src) = extra {
        let more = emerge_lang::parse(src).map_err(|d| format!("views:{}:{}: {}", d.line, d.col, d.message))?;
        file.items.extend(more.items);
    }
    let (errors, _) = emerge_lang::view::check(&file);
    if let Some(e) = errors.first() {
        return Err(format!("{}: {} {}", e.path, e.code, e.message));
    }
    let views: Vec<View> = file.items.into_iter().filter_map(|i| match i.node {
        Item::View(v) => Some(v),
        _ => None,
    }).collect();
    match name {
        Some(n) => views.into_iter().rev().find(|v| v.name == n).ok_or(format!("no view `{n}`")),
        None => views.into_iter().last().ok_or("the world has no view: write one, or pass views".to_string()),
    }
}

/// The run's current frame drawn with `view`, as PNG bytes, and the image's size. `cell_px` 0
/// picks a size near 1024 pixels across.
pub fn png(run: &Run, view: &View, cell_px: u32) -> Result<(Vec<u8>, u32, u32), String> {
    png_with(run, view, cell_px, &[])
}

/// `png`, with `charts` drawn under the picture from the run's history (none: exactly `png`).
pub fn png_with(run: &Run, view: &View, cell_px: u32, charts: &[Charts]) -> Result<(Vec<u8>, u32, u32), String> {
    png_map(run, view, cell_px, charts).map(|(b, w, h, _)| (b, w, h))
}

/// Where the map sits in a picture: its size in pixels (it starts at the top left) and the world it
/// shows, so a point in the picture can be turned into a point of the world (the viewer's info picker).
#[derive(Debug, Clone, Copy)]
pub struct MapInfo {
    pub w: u32,
    pub h: u32,
    pub grid_w: usize,
    pub grid_h: usize,
    pub world_w: f32,
    pub world_h: f32,
}

/// `png_with`, and the map's place in the picture.
pub fn png_map(run: &Run, view: &View, cell_px: u32, charts: &[Charts]) -> Result<(Vec<u8>, u32, u32, MapInfo), String> {
    shot(run, view, cell_px, charts, false).map(|s| (s.png, s.w, s.h, s.map))
}

/// A picture with what it shows (`proposals/agent-tracking.md` §12): the PNG, the map's place in it,
/// and the frame it was drawn from, with the agents the view draws (`hits`, empty unless asked for).
/// The PNG, the hits and any overlay all come from this one frame, so a click on the picture names
/// the creature that was where the picture shows it.
pub struct Shot {
    pub png: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub map: MapInfo,
    pub frame: emerge_view::Frame,
    pub hits: Vec<emerge_view::render::Hit>,
}

pub fn shot(run: &Run, view: &View, cell_px: u32, charts: &[Charts], hits: bool) -> Result<Shot, String> {
    shot_of(run.frame(), view, cell_px, charts, hits)
}

fn panels(charts: &[Charts]) -> Vec<emerge_view::chart::Panel<'_>> {
    charts.iter().map(|c| match c {
        Charts::Default => emerge_view::chart::Panel::Default,
        Charts::Block(b) => emerge_view::chart::Panel::Block(b),
    }).collect()
}

/// A live frame drawn for the viewer: the cell layers as a PNG (charts under them, no legend), the creatures as vector
/// glyphs the page draws itself (so they stay sharp at any zoom), and the legend as data. `None` for a view that draws a
/// cell layer over an agents layer: the caller sends a raster `Shot` instead.
pub struct VectorShot {
    pub shot: Shot,
    pub glyphs: Vec<emerge_view::render::Glyph>,
    pub layers: Vec<(String, String, f32)>,
    pub keys: Vec<emerge_view::render::LayerKey>,
}

pub fn vector_of(frame: emerge_view::Frame, view: &View, cell_px: u32, charts: &[Charts], hits: bool) -> Result<Option<VectorShot>, String> {
    let Some(v) = emerge_view::render::render_vector(view, &frame, cell_px, &panels(charts)) else { return Ok(None) };
    let png = v.picture.encode_png().map_err(|e| e.to_string())?;
    let k = if cell_px > 0 { cell_px } else { (1024 / frame.grid_w.max(1) as u32).max(1) };
    let map = MapInfo { w: frame.grid_w as u32 * k, h: frame.grid_h as u32 * k, grid_w: frame.grid_w, grid_h: frame.grid_h, world_w: frame.world_w, world_h: frame.world_h };
    let hits = if hits { emerge_view::render::agent_hits(view, &frame, cell_px) } else { Vec::new() };
    let (w, h) = (v.picture.width(), v.picture.height());
    Ok(Some(VectorShot { shot: Shot { png, w, h, map, frame, hits }, glyphs: v.glyphs, layers: v.layers, keys: v.keys }))
}

/// `shot` of a frame already read (a live one, or one kept for the timeline).
pub fn shot_of(frame: emerge_view::Frame, view: &View, cell_px: u32, charts: &[Charts], hits: bool) -> Result<Shot, String> {
    let panels = panels(charts);
    let image = emerge_view::render_with(view, &frame, emerge_view::Options { cell_px, legend: true }, &panels);
    let png = image.encode_png().map_err(|e| e.to_string())?;
    // The renderer's rule: `cell_px` pixels a cell, or a size near 1024 pixels across.
    let k = if cell_px > 0 { cell_px } else { (1024 / frame.grid_w.max(1) as u32).max(1) };
    let map = MapInfo { w: frame.grid_w as u32 * k, h: frame.grid_h as u32 * k, grid_w: frame.grid_w, grid_h: frame.grid_h, world_w: frame.world_w, world_h: frame.world_h };
    let hits = if hits { emerge_view::render::agent_hits(view, &frame, cell_px) } else { Vec::new() };
    Ok(Shot { png, w: image.width(), h: image.height(), map, frame, hits })
}
