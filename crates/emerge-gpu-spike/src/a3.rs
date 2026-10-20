//! A3 (proposals/gpu-runtime.md, M0): the contract-mode float functions of
//! `emerge_ref::contract`, written in WGSL with explicit `fma`, against the CPU, bit for bit.

use crate::gpu::Gpu;
use emerge_ref::contract as c;

fn lit(x: f32) -> String {
    format!("bitcast<f32>({:#x}u)", x.to_bits())
}

fn prelude() -> String {
    let sin: Vec<String> = c::SIN.iter().map(|&x| lit(x)).collect();
    let cos: Vec<String> = c::COS.iter().map(|&x| lit(x)).collect();
    format!(
        r#"
struct P {{ base: u32, }}
@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var<storage, read_write> out: array<u32>;

fn tiny_c() -> f32 {{ return {tiny}; }}
fn flush(x: f32) -> f32 {{ return select(x, 0.0, abs(x) < tiny_c()); }}
fn fm(a: f32, b: f32, c: f32) -> f32 {{ return flush(fma(a, b, c)); }}
fn hash(v: u32) -> u32 {{
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}}
fn unit(i: u32, k: u32) -> f32 {{ return f32(hash(i * 16u + k) >> 8u) * {unit}; }}
// lo + u * (hi - lo), fused, without the flush
fn between(i: u32, k: u32, lo: f32, hi: f32) -> f32 {{ return fma(unit(i, k), hi - lo, lo); }}
fn tiny_in(x: f32) -> f32 {{ return select(x, 0.0, abs(x) < tiny_c()); }}
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
fn rotate(h: vec2<f32>, s: f32, c: f32) -> vec2<f32> {{
    let nx = fm(h.x, c, -(h.y * s));
    let ny = fm(h.x, s, h.y * c);
    let q = fm(nx, nx, ny * ny);
    let t = fm(q, -0.5, 1.5);
    return vec2<f32>(nx * t, ny * t);
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
        unit = lit(1.0 / 16_777_216.0),
        pi = lit(c::PI),
        tau = lit(c::TAU),
        half_pi = lit(c::HALF_PI),
        sin0 = sin[0], sin1 = sin[1], sin2 = sin[2], sin3 = sin[3], sin4 = sin[4], sin5 = sin[5],
        cos0 = cos[0], cos1 = cos[1], cos2 = cos[2], cos3 = cos[3], cos4 = cos[4], cos5 = cos[5], cos6 = cos[6],
    )
}

// ---- the same inputs on the CPU ----

fn hash(v: u32) -> u32 {
    let s = v.wrapping_mul(747796405).wrapping_add(2891336453);
    let w = ((s >> ((s >> 28) + 4)) ^ s).wrapping_mul(277803737);
    (w >> 22) ^ w
}
fn unit(i: u32, k: u32) -> f32 {
    (hash(i.wrapping_mul(16).wrapping_add(k)) >> 8) as f32 * (1.0 / 16_777_216.0)
}
fn between(i: u32, k: u32, lo: f32, hi: f32) -> f32 {
    unit(i, k).mul_add(hi - lo, lo)
}

struct Test {
    name: &'static str,
    /// WGSL body of `fn test(i: u32) -> vec2<u32>`.
    wgsl: &'static str,
    count: u64,
    cpu: fn(u32) -> [u32; 2],
}

const ENDPOINT: u32 = 0x40E6_6666; // 7.2f32

fn sincos_x(i: u32) -> f32 {
    f32::from_bits(if i <= ENDPOINT { i } else { 0x8000_0000 | (i - ENDPOINT - 1) })
}

fn tests() -> Vec<Test> {
    vec![
        Test {
            name: "sincos on every f32 with |x| <= 7.2",
            wgsl: "let x = select(bitcast<f32>(i), bitcast<f32>(0x80000000u | (i - 0x40E66667u)), i > 0x40E66666u); let r = sincos(x); return vec2<u32>(bitcast<u32>(r.x), bitcast<u32>(r.y));",
            count: 2 * (ENDPOINT as u64 + 1),
            cpu: |i| {
                let (s, co) = c::sincos(sincos_x(i));
                [s.to_bits(), co.to_bits()]
            },
        },
        Test {
            name: "rotate (turn and renormalise)",
            wgsl: "let h = vec2<f32>(between(i, 0u, -1.0, 1.0), between(i, 1u, -1.0, 1.0)); let r = rotate(h, between(i, 2u, -1.0, 1.0), between(i, 3u, -1.0, 1.0)); return vec2<u32>(bitcast<u32>(r.x), bitcast<u32>(r.y));",
            count: 1 << 26,
            cpu: |i| {
                let r = c::rotate([between(i, 0, -1.0, 1.0), between(i, 1, -1.0, 1.0)], between(i, 2, -1.0, 1.0), between(i, 3, -1.0, 1.0));
                [r[0].to_bits(), r[1].to_bits()]
            },
        },
        Test {
            name: "advance and wrap (fused step, one wrap)",
            wgsl: "let px = between(i, 0u, -10.0, 530.0); let py = between(i, 1u, -10.0, 530.0); let dist = between(i, 4u, 0.0, 8.0); let x = fm(between(i, 2u, -1.0, 1.0), dist, px); let y = fm(between(i, 3u, -1.0, 1.0), dist, py); return vec2<u32>(bitcast<u32>(fit(x, 512.0, true)), bitcast<u32>(fit(y, 512.0, true)));",
            count: 1 << 26,
            cpu: |i| {
                let p = c::advance([between(i, 0, -10.0, 530.0), between(i, 1, -10.0, 530.0)], [between(i, 2, -1.0, 1.0), between(i, 3, -1.0, 1.0)], between(i, 4, 0.0, 8.0));
                [c::fit(p[0], 512.0, true).to_bits(), c::fit(p[1], 512.0, true).to_bits()]
            },
        },
        Test {
            name: "clamped position (no wrap)",
            wgsl: "let v = between(i, 0u, -50.0, 600.0); return vec2<u32>(bitcast<u32>(fit(v, 512.0, false)), 0u);",
            count: 1 << 24,
            cpu: |i| [c::fit(between(i, 0, -50.0, 600.0), 512.0, false).to_bits(), 0],
        },
        Test {
            name: "contact distance (dx*dx + dy*dy fused)",
            wgsl: "let dx = between(i, 0u, 0.0, 4.0); let dy = between(i, 1u, 0.0, 4.0); return vec2<u32>(bitcast<u32>(fm(dx, dx, dy * dy)), 0u);",
            count: 1 << 26,
            cpu: |i| [c::distance2(between(i, 0, 0.0, 4.0), between(i, 1, 0.0, 4.0)).to_bits(), 0],
        },
        Test {
            name: "normal draw (twelve uniforms)",
            wgsl: "var s = 0.0; for (var k = 0u; k < 12u; k++) { s += unit(i, k); } return vec2<u32>(bitcast<u32>(s - 6.0), 0u);",
            count: 1 << 26,
            cpu: |i| {
                let mut k = 0;
                let n = c::normal(|| {
                    let u = unit(i, k);
                    k += 1;
                    u
                });
                [n.to_bits(), 0]
            },
        },
        Test {
            name: "tanh_into and sigmoid_d",
            wgsl: "let x = between(i, 0u, -4.0, 4.0); let a = between(i, 1u, -2.0, 2.0); return vec2<u32>(bitcast<u32>(tanh_into(x, a)), bitcast<u32>(sigmoid_d(x)));",
            count: 1 << 26,
            cpu: |i| {
                let x = between(i, 0, -4.0, 4.0);
                [c::tanh_into(x, between(i, 1, -2.0, 2.0)).to_bits(), c::sigmoid_d(x).to_bits()]
            },
        },
        Test {
            name: "gradient projection (two fused forms)",
            wgsl: "let gx = between(i, 0u, -1.0, 1.0); let gy = between(i, 1u, -1.0, 1.0); let hx = between(i, 2u, -1.0, 1.0); let hy = between(i, 3u, -1.0, 1.0); return vec2<u32>(bitcast<u32>(fm(gx, hx, gy * hy)), bitcast<u32>(fm(gy, hx, -(gx * hy))));",
            count: 1 << 26,
            cpu: |i| {
                let (gx, gy, hx, hy) = (between(i, 0, -1.0, 1.0), between(i, 1, -1.0, 1.0), between(i, 2, -1.0, 1.0), between(i, 3, -1.0, 1.0));
                [c::fma(gx, hx, gy * hy).to_bits(), c::fma(gy, hx, -(gx * hy)).to_bits()]
            },
        },
    ]
}

pub fn run() {
    let gpu = Gpu::new();
    println!("A3: {}", gpu.name);
    const CHUNK: u32 = 1 << 23;
    let out = gpu.storage(CHUNK as usize * 2 * 4);
    let mut all_ok = true;
    for t in tests() {
        let src = format!(
            "{}\nfn test(i: u32) -> vec2<u32> {{ {} }}\n@compute @workgroup_size(256)\nfn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n    let r = test(p.base + id.x);\n    out[2u * id.x] = r.x;\n    out[2u * id.x + 1u] = r.y;\n}}\n",
            prelude(),
            t.wgsl
        );
        let pipeline = gpu.pipeline(&src, "main");
        let (mut mismatches, mut first) = (0u64, Vec::new());
        let chunks = t.count.div_ceil(CHUNK as u64);
        for chunk in 0..chunks {
            let base = (chunk * CHUNK as u64) as u32;
            let uniform = gpu.uniform_with(&[base]);
            gpu.run(&pipeline, &[&uniform, &out], (CHUNK / 256, 1, 1));
            let got: Vec<u32> = gpu.read(&out, CHUNK as usize * 2);
            let valid = (t.count - chunk * CHUNK as u64).min(CHUNK as u64) as usize;
            let threads = 16usize;
            let per = valid.div_ceil(threads);
            let parts: Vec<(u64, Vec<String>)> = std::thread::scope(|s| {
                let handles: Vec<_> = (0..threads)
                    .map(|k| {
                        let got = &got;
                        let cpu = t.cpu;
                        s.spawn(move || {
                            let (mut m, mut ex) = (0u64, Vec::new());
                            for i in k * per..((k + 1) * per).min(valid) {
                                let want = cpu(base + i as u32);
                                if got[2 * i] != want[0] || got[2 * i + 1] != want[1] {
                                    m += 1;
                                    if ex.len() < 2 {
                                        ex.push(format!("i={} gpu={:#x},{:#x} cpu={:#x},{:#x}", base + i as u32, got[2 * i], got[2 * i + 1], want[0], want[1]));
                                    }
                                }
                            }
                            (m, ex)
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for (m, ex) in parts {
                mismatches += m;
                first.extend(ex);
            }
        }
        println!("{:<48} {:>13} inputs, {:>9} mismatches", t.name, t.count, mismatches);
        for e in first.iter().take(3) {
            println!("    e.g. {e}");
        }
        all_ok &= mismatches == 0;
    }
    println!("{}", if all_ok { "A3: all identical" } else { "A3: MISMATCHES" });
}
