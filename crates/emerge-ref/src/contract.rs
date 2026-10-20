//! Contract mode (`proposals/gpu-runtime.md`, M0): every float operation of a tick written
//! so that a GPU can reproduce it bit for bit.
//!
//! The rules (found by the GPU spike, `proposals/gpu-spike.md`):
//! - a multiply-add is always one fused operation (`mul_add`), never a rounded product
//!   followed by an add, because a GPU compiler may fuse the second form and may not;
//! - a multiply-add result below `2^-100` in magnitude is `0`, because a GPU may flush
//!   denormals;
//! - curves are the division-free `sigmoid_d` and `tanh_d`;
//! - there is no `sin`, `cos`, `sqrt`, division, `exp` or `ln` (their results differ between
//!   implementations): `sincos` is a polynomial, a heading is renormalised with one Newton
//!   step, and a normal draw is the sum of twelve uniform draws.
//!
//! Off by default (`World::contract`); the default runner is unchanged.

/// `2^-100`.
pub const TINY: f32 = 7.888_609e-31;
/// Degrees to radians, as an `f32`.
pub const DEG: f32 = 0.017_453_292;

pub fn flush(x: f32) -> f32 {
    if x.abs() < TINY { 0.0 } else { x }
}

/// `a·b + c` fused, with the flush rule.
pub fn fma(a: f32, b: f32, c: f32) -> f32 {
    flush(a.mul_add(b, c))
}

fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    x.max(lo).min(hi)
}

/// An input below `2^-100` in magnitude is `0`, so no intermediate of a curve is denormal.
fn tiny(x: f32) -> f32 {
    if x.abs() < TINY { 0.0 } else { x }
}

pub fn tanh_d(x: f32) -> f32 {
    let u = clamp(tiny(x) * 0.5, -1.0, 1.0);
    u * (2.0 - u.abs())
}

/// `acc + tanh_d(x)` with the product fused into the add, which a GPU compiler may do on its
/// own and is therefore written out.
pub fn tanh_into(x: f32, acc: f32) -> f32 {
    let u = clamp(tiny(x) * 0.5, -1.0, 1.0);
    fma(u, 2.0 - u.abs(), acc)
}

pub fn sigmoid_d(x: f32) -> f32 {
    let v = clamp(tiny(x) * 0.25, -1.0, 1.0);
    0.5 + 0.5 * (v * (2.0 - v.abs()))
}

/// Finite values are clamped to `±limit`, anything else is `0`.
pub fn sanitize(x: f32, limit: f32) -> f32 {
    if x.abs() <= f32::MAX { flush(clamp(x, -limit, limit)) } else { 0.0 }
}

pub const SIN: [f32; 6] = [1.0, -1.0 / 6.0, 1.0 / 120.0, -1.0 / 5040.0, 1.0 / 362_880.0, -1.0 / 39_916_800.0];
pub const COS: [f32; 7] = [1.0, -0.5, 1.0 / 24.0, -1.0 / 720.0, 1.0 / 40_320.0, -1.0 / 3_628_800.0, 1.0 / 479_001_600.0];
pub const PI: f32 = std::f32::consts::PI;
pub const TAU: f32 = std::f32::consts::TAU;
pub const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;

fn horner(k: &[f32], x2: f32) -> f32 {
    let mut p = k[k.len() - 1];
    for &c in k[..k.len() - 1].iter().rev() {
        p = fma(p, x2, c);
    }
    p
}

/// `(sin, cos)` of an angle in radians with `|x| <= 2π + 1`: reduced to `[-π, π]`, folded to
/// `[0, π/2]`, then a Taylor polynomial (error below `1e-7`).
pub fn sincos(x: f32) -> (f32, f32) {
    let x = tiny(x);
    let x = if x > PI { x - TAU } else if x < -PI { x + TAU } else { x };
    let a = x.abs();
    let (r, flip) = if a > HALF_PI { (PI - a, true) } else { (a, false) };
    let r2 = r * r;
    let s = flush(r * horner(&SIN, r2));
    let c = horner(&COS, r2);
    (if x < 0.0 { -s } else { s }, if flip { -c } else { c })
}

/// A heading turned by `(s, c)` (the sine and cosine of the turn), renormalised with one
/// Newton step for `1/sqrt`: no `sqrt`, no division.
pub fn rotate(h: [f32; 2], s: f32, c: f32) -> [f32; 2] {
    let nx = fma(h[0], c, -(h[1] * s));
    let ny = fma(h[0], s, h[1] * c);
    let q = fma(nx, nx, ny * ny);
    let t = fma(q, -0.5, 1.5);
    [nx * t, ny * t]
}

/// The heading `h` turned `angle_deg` to the left, without renormalising (for `probe`).
pub fn turned(h: [f32; 2], angle_deg: f32) -> [f32; 2] {
    let (s, c) = sincos(angle_deg * DEG);
    [fma(h[0], c, -(h[1] * s)), fma(h[0], s, h[1] * c)]
}

/// `p + d·dist` fused.
pub fn advance(p: [f32; 2], d: [f32; 2], dist: f32) -> [f32; 2] {
    [fma(d[0], dist, p[0]), fma(d[1], dist, p[1])]
}

/// A position wrapped (or clamped) into `[0, size)`; a step longer than the space is not
/// supported in contract mode.
pub fn fit(v: f32, size: f32, wrap: bool) -> f32 {
    if wrap {
        let r = if v < 0.0 { v + size } else if v >= size { v - size } else { v };
        if r >= size || r < 0.0 { 0.0 } else { r }
    } else {
        clamp(v, 0.0, f32::from_bits(size.to_bits() - 1))
    }
}

/// `(a - b) * k` for two amounts, where the difference is taken in integers first (a product
/// that met a subtraction could be fused either way) and converted once. The sign is split off
/// so that the conversion of a `u32` to `f32`, exact in no more than 24 bits, is one rounding.
pub fn amount_diff(a: u32, b: u32, k: f32) -> f32 {
    if a >= b { (a - b) as f32 * k } else { -(((b - a) as f32) * k) }
}

/// Squared distance for the contact index: `dx·dx + dy·dy` fused.
pub fn distance2(dx: f32, dy: f32) -> f32 {
    fma(dx, dx, dy * dy)
}

/// A standard normal as twelve uniform draws summed, minus 6 (mean 0, variance 1).
pub fn normal(units: impl FnMut() -> f32) -> f32 {
    let mut units = units;
    let mut sum = 0.0f32;
    for _ in 0..12 {
        sum += units();
    }
    sum - 6.0
}
