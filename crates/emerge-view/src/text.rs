//! Text for legends, drawn with DejaVu Sans (in `fonts/`, with its licence), so a picture looks
//! the same on every machine.

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use tiny_skia::Pixmap;

static DEJAVU: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

pub struct Text {
    font: FontRef<'static>,
}

impl Default for Text {
    fn default() -> Self {
        Text::new()
    }
}

impl Text {
    pub fn new() -> Text {
        Text { font: FontRef::try_from_slice(DEJAVU).expect("the bundled font loads") }
    }

    /// The width of `s` at `size` pixels.
    pub fn width(&self, size: f32, s: &str) -> f32 {
        let f = self.font.as_scaled(PxScale::from(size));
        let mut w = 0.0;
        let mut last = None;
        for c in s.chars() {
            let g = f.glyph_id(c);
            if let Some(p) = last {
                w += f.kern(p, g);
            }
            w += f.h_advance(g);
            last = Some(g);
        }
        w
    }

    /// Draws `s` with its baseline at `y`, blended over what is there.
    pub fn draw(&self, pm: &mut Pixmap, x: f32, y: f32, size: f32, s: &str, color: [u8; 3]) {
        let f = self.font.as_scaled(PxScale::from(size));
        let (w, h) = (pm.width() as i32, pm.height() as i32);
        let data = pm.data_mut();
        let mut cx = x;
        let mut last = None;
        for c in s.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = last {
                cx += f.kern(p, id);
            }
            let glyph = id.with_scale_and_position(PxScale::from(size), point(cx, y));
            if let Some(o) = self.font.outline_glyph(glyph) {
                let b = o.px_bounds();
                o.draw(|gx, gy, cov| {
                    let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                    if px < 0 || py < 0 || px >= w || py >= h {
                        return;
                    }
                    let i = (py * w + px) as usize * 4;
                    for k in 0..3 {
                        let under = data[i + k] as f32;
                        data[i + k] = (under + (color[k] as f32 - under) * cov.clamp(0.0, 1.0)).round() as u8;
                    }
                    data[i + 3] = 255;
                });
            }
            cx += f.h_advance(id);
            last = Some(id);
        }
    }

    /// Draws `s` broken into lines no wider than `max`; returns the baseline after the last line.
    pub fn wrapped(&self, pm: &mut Pixmap, x: f32, y: f32, size: f32, s: &str, color: [u8; 3], max: f32) -> f32 {
        let mut line = String::new();
        let mut y = y;
        for word in s.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if self.width(size, &candidate) > max && !line.is_empty() {
                self.draw(pm, x, y, size, &line, color);
                y += size * 1.35;
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        if !line.is_empty() {
            self.draw(pm, x, y, size, &line, color);
            y += size * 1.35;
        }
        y
    }
}
