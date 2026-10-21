//! Pictures of a run: the view to draw (the run's own views, then any given as source), and the
//! PNG of a frame. `emerge-server render` and the live viewer both use these, so a live frame and
//! an exported one are the same bytes (`proposals/live-viewer.md`).

use crate::run::Run;
use emerge_lang::ast::{Item, View};

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
    let image = emerge_view::render(view, &run.frame(), emerge_view::Options { cell_px, legend: true });
    let bytes = image.encode_png().map_err(|e| e.to_string())?;
    Ok((bytes, image.width(), image.height()))
}
