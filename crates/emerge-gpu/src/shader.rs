//! WGSL shared by the kernels: the contract-mode functions of `emerge_ref::contract`, with
//! explicit `fma` wherever a multiply meets an add (checked against the CPU in
//! `emerge-gpu-spike a3`), and the constants of `pack.rs`.

use crate::pack::*;
use emerge_ref::contract as c;

fn lit(x: f32) -> String {
    format!("bitcast<f32>({:#x}u)", x.to_bits())
}

pub fn constants() -> String {
    let mut s = String::new();
    for (name, v) in [
        ("ENT", ENT as u32), ("SLOT", SLOT as u32), ("NSIG", NSIG as u32), ("MAXG", MAXG as u32), ("NONE", NONE), ("REC", REC as u32),
        ("SENSE_INVENTORY", SENSE_INVENTORY), ("SENSE_CELL", SENSE_CELL), ("SENSE_GRADIENT", SENSE_GRADIENT), ("SENSE_PROBE", SENSE_PROBE),
        ("SENSE_CROWD", SENSE_CROWD), ("SENSE_NEIGHBOUR", SENSE_NEIGHBOUR), ("SENSE_CLOCK", SENSE_CLOCK), ("SENSE_DELTA", SENSE_DELTA),
        ("WEIGH", WEIGH), ("PRODUCT", PRODUCT), ("REACT", REACT), ("MOVE", MOVE), ("TURN", TURN), ("TAKE", TAKE), ("DROP", DROP),
        ("GIVE", GIVE), ("STEAL", STEAL), ("GUARD", GUARD), ("DIVIDE", DIVIDE),
        ("R_MOVE", R_MOVE), ("R_TURN", R_TURN), ("R_TAKE", R_TAKE), ("R_DROP", R_DROP), ("R_GIVE", R_GIVE), ("R_STEAL", R_STEAL),
        ("R_GUARD", R_GUARD), ("R_DIVIDE", R_DIVIDE), ("R_REACT", R_REACT),
        ("P_N", P_N as u32), ("P_NS", P_NS as u32), ("P_TICK", P_TICK as u32), ("P_STEPS", P_STEPS as u32), ("P_ONE", P_ONE as u32),
        ("P_GW", P_GW as u32), ("P_GH", P_GH as u32), ("P_WRAP", P_WRAP as u32), ("P_NBX", P_NBX as u32), ("P_NBY", P_NBY as u32),
        ("P_W", P_W as u32), ("P_H", P_H as u32), ("P_R2", P_R2 as u32), ("P_RADIUS", P_RADIUS as u32), ("P_KX", P_KX as u32),
        ("P_KY", P_KY as u32), ("P_CKX", P_CKX as u32), ("P_CKY", P_CKY as u32), ("P_UNIT", P_UNIT as u32),
    ] {
        s.push_str(&format!("const {name} = {v}u;\n"));
    }
    s
}

/// The contract functions. `params` must be bound as `array<u32>` named `params`.
pub fn contract() -> String {
    let sin: Vec<String> = c::SIN.iter().map(|&x| lit(x)).collect();
    let cos: Vec<String> = c::COS.iter().map(|&x| lit(x)).collect();
    format!(
        r#"
fn tiny_c() -> f32 {{ return {tiny}; }}
fn flush(x: f32) -> f32 {{ return select(x, 0.0, abs(x) < tiny_c()); }}
fn fm(a: f32, b: f32, c: f32) -> f32 {{ return flush(fma(a, b, c)); }}
fn tiny_in(x: f32) -> f32 {{ return select(x, 0.0, abs(x) < tiny_c()); }}
fn pf(i: u32) -> f32 {{ return bitcast<f32>(params[i]); }}
// A u32 as an f32 with one rounding (to nearest even), whatever the converter does.
fn f32c(a: u32) -> f32 {{ return fma(f32(a >> 12u), 4096.0, f32(a & 4095u)); }}
fn sanitize(x: f32, limit: f32) -> f32 {{
    if (!(abs(x) <= {fmax})) {{ return 0.0; }}
    return flush(clamp(x, -limit, limit));
}}
fn amount_diff(a: u32, b: u32, k: f32) -> f32 {{
    if (a >= b) {{ return f32c(a - b) * k; }}
    return -(f32c(b - a) * k);
}}
fn tanh_into(x: f32, acc: f32) -> f32 {{
    let u = clamp(tiny_in(x) * 0.5, -1.0, 1.0);
    return fm(u, 2.0 - abs(u), acc);
}}
fn sigmoid_d(x: f32) -> f32 {{
    let v = clamp(tiny_in(x) * 0.25, -1.0, 1.0);
    return 0.5 + 0.5 * (v * (2.0 - abs(v)));
}}
fn sincos(x0: f32) -> vec2<f32> {{
    var x = tiny_in(x0);
    if (x > {pi}) {{ x = x - {tau}; }} else if (x < -{pi}) {{ x = x + {tau}; }}
    let a = abs(x);
    var r = a;
    var flip = false;
    if (a > {half_pi}) {{ r = {pi} - a; flip = true; }}
    let r2 = r * r;
    var ps = {sin5};
    ps = fm(ps, r2, {sin4}); ps = fm(ps, r2, {sin3}); ps = fm(ps, r2, {sin2}); ps = fm(ps, r2, {sin1}); ps = fm(ps, r2, {sin0});
    var pc = {cos6};
    pc = fm(pc, r2, {cos5}); pc = fm(pc, r2, {cos4}); pc = fm(pc, r2, {cos3}); pc = fm(pc, r2, {cos2}); pc = fm(pc, r2, {cos1}); pc = fm(pc, r2, {cos0});
    var s = flush(r * ps);
    if (x < 0.0) {{ s = -s; }}
    var cc = pc;
    if (flip) {{ cc = -cc; }}
    return vec2<f32>(s, cc);
}}
// The heading turned by an angle, without renormalising (for `probe`).
fn turned(h: vec2<f32>, angle_deg: f32) -> vec2<f32> {{
    let sc = sincos(angle_deg * {deg});
    return vec2<f32>(fm(h.x, sc.y, -(h.y * sc.x)), fm(h.x, sc.x, h.y * sc.y));
}}
fn fit(v: f32, size: f32, wrap: bool) -> f32 {{
    if (wrap) {{
        var r = v;
        if (v < 0.0) {{ r = v + size; }} else if (v >= size) {{ r = v - size; }}
        if (r >= size || r < 0.0) {{ return 0.0; }}
        return r;
    }}
    return clamp(v, 0.0, bitcast<f32>(bitcast<u32>(size) - 1u));
}}
"#,
        tiny = lit(c::TINY),
        fmax = lit(f32::MAX),
        pi = lit(c::PI),
        tau = lit(c::TAU),
        half_pi = lit(c::HALF_PI),
        deg = lit(c::DEG),
        sin0 = sin[0], sin1 = sin[1], sin2 = sin[2], sin3 = sin[3], sin4 = sin[4], sin5 = sin[5],
        cos0 = cos[0], cos1 = cos[1], cos2 = cos[2], cos3 = cos[3], cos4 = cos[4], cos5 = cos[5], cos6 = cos[6],
    )
}
