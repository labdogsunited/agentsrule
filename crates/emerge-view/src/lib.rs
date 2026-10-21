//! Renders `.em` views (`proposals/visual-language.md` V2): a frame of a world, evaluated layer by
//! layer into VEX-style attributes, drawn as exact cells and glyphs with a legend, saved as PNG.

pub mod chart;
pub mod frame;
pub mod palette;
pub mod render;
pub mod text;

pub use frame::{Agent, Frame};
pub use render::{render, render_with, Options};
