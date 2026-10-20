//! The palettes a view may name (`emerge_lang::view::PALETTES`), as colour stops interpolated
//! linearly in sRGB. viridis, magma, inferno and cividis follow matplotlib's maps; the sequential
//! and diverging ones follow ColorBrewer; `categorical` is Tableau 10; `phase` is computed (OKLCH
//! at constant lightness and chroma, the hue going round), so a heading reads the same all round.

type Rgb = [u8; 3];

const fn hex(v: u32) -> Rgb {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

const VIRIDIS: [Rgb; 10] = [hex(0x440154), hex(0x482878), hex(0x3e4989), hex(0x31688e), hex(0x26828e), hex(0x1f9e89), hex(0x35b779), hex(0x6ece58), hex(0xb5de2b), hex(0xfde725)];
const MAGMA: [Rgb; 10] = [hex(0x000004), hex(0x180f3d), hex(0x440f76), hex(0x721f81), hex(0x9e2f7f), hex(0xcd4071), hex(0xf1605d), hex(0xfd9668), hex(0xfeca8d), hex(0xfcfdbf)];
const INFERNO: [Rgb; 10] = [hex(0x000004), hex(0x1b0c41), hex(0x4a0c6b), hex(0x781c6d), hex(0xa52c60), hex(0xcf4446), hex(0xed6925), hex(0xfb9b06), hex(0xf7d13d), hex(0xfcffa4)];
const CIVIDIS: [Rgb; 10] = [hex(0x00224e), hex(0x123570), hex(0x3b496c), hex(0x575d6d), hex(0x707173), hex(0x8a8779), hex(0xa69d75), hex(0xc4b56c), hex(0xe4cf5b), hex(0xfee838)];
const GREENS: [Rgb; 9] = [hex(0xf7fcf5), hex(0xe5f5e0), hex(0xc7e9c0), hex(0xa1d99b), hex(0x74c476), hex(0x41ab5d), hex(0x238b45), hex(0x006d2c), hex(0x00441b)];
const BLUES: [Rgb; 9] = [hex(0xf7fbff), hex(0xdeebf7), hex(0xc6dbef), hex(0x9ecae1), hex(0x6baed6), hex(0x4292c6), hex(0x2171b5), hex(0x08519c), hex(0x08306b)];
const REDS: [Rgb; 9] = [hex(0xfff5f0), hex(0xfee0d2), hex(0xfcbba1), hex(0xfc9272), hex(0xfb6a4a), hex(0xef3b2c), hex(0xcb181d), hex(0xa50f15), hex(0x67000d)];
const GRAYS: [Rgb; 2] = [hex(0xffffff), hex(0x000000)];
const RDBU: [Rgb; 11] = [hex(0x67001f), hex(0xb2182b), hex(0xd6604d), hex(0xf4a582), hex(0xfddbc7), hex(0xf7f7f7), hex(0xd1e5f0), hex(0x92c5de), hex(0x4393c3), hex(0x2166ac), hex(0x053061)];
const TABLEAU10: [Rgb; 10] = [hex(0x4e79a7), hex(0xf28e2b), hex(0xe15759), hex(0x76b7b2), hex(0x59a14f), hex(0xedc948), hex(0xb07aa1), hex(0xff9da7), hex(0x9c755f), hex(0xbab0ac)];

/// The colour of a cell with nothing to show (a zero under a log scale, an empty field), and the
/// colour of a binding that is not computed yet. Both are named in the legend.
pub const EMPTY: Rgb = hex(0x15181e);
pub const UNAVAILABLE: Rgb = hex(0x8a8f98);

fn stops(name: &str) -> Option<&'static [Rgb]> {
    Some(match name {
        "viridis" => &VIRIDIS,
        "magma" => &MAGMA,
        "inferno" => &INFERNO,
        "cividis" => &CIVIDIS,
        "greens" => &GREENS,
        "blues" => &BLUES,
        "reds" => &REDS,
        "grays" => &GRAYS,
        "rdbu" => &RDBU,
        _ => return None,
    })
}

/// A continuous palette at `t` in `[0, 1]` (clamped).
pub fn sample(name: &str, t: f64) -> Rgb {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    if name == "phase" {
        return phase(t);
    }
    let s = stops(name).unwrap_or(&VIRIDIS);
    let x = t * (s.len() - 1) as f64;
    let i = (x.floor() as usize).min(s.len() - 2);
    let f = x - i as f64;
    let mix = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * f).round() as u8;
    [mix(s[i][0], s[i + 1][0]), mix(s[i][1], s[i + 1][1]), mix(s[i][2], s[i + 1][2])]
}

/// The categorical palette for an identity: the same identity always gets the same colour.
pub fn category(index: u64) -> Rgb {
    TABLEAU10[(index % TABLEAU10.len() as u64) as usize]
}

/// OKLCH (L 0.72, C 0.12, hue `t` × 360°) to sRGB.
fn phase(t: f64) -> Rgb {
    let h = t * std::f64::consts::TAU;
    let (l, a, b) = (0.72, 0.12 * h.cos(), 0.12 * h.sin());
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
    let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
    let bl = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701 * s3;
    let enc = |c: f64| {
        let c = c.clamp(0.0, 1.0);
        let v = if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
        (v * 255.0).round() as u8
    };
    [enc(r), enc(g), enc(bl)]
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_palette_a_view_may_name_is_defined() {
        for (name, kind) in emerge_lang::view::PALETTES {
            if *kind != emerge_lang::view::PaletteKind::Categorical && *name != "phase" {
                assert!(super::stops(name).is_some(), "{name} has no stops");
            }
        }
    }
}
