//! A2 (archive/gpu-spike.md): the arithmetic around the curves, as `emerge-ref/src/mind.rs`
//! (`think`) writes it: `x = fold(bias, acc + w·s)`, the curve, then `keep·s` plus the
//! contributions in gene order, then sanitize.
//!
//! Two GPU variants (plain `a*b + c`; WGSL `fma`) against two CPU references (unfused;
//! fused with `mul_add`), on random tuples drawn identically on both sides.

use crate::gpu::Gpu;

/// `MULADD(a, b, c)` is `a*b + c` written inline (variant 0) or `fma(a, b, c)` (variant 1).
const SHADER: &str = r#"
struct P { base: u32, mode: u32, }
@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var<storage, read_write> out: array<u32>;

fn hash(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

// Exponent range by mode: 0 = the runner's ranges, 1 = tiny values near the denormal edge.
fn value(d: u32, elo: i32, ehi: i32, signed: bool) -> f32 {
    let e = elo + i32((d >> 24u) % u32(ehi - elo + 1));
    var bits = (u32(e + 127) << 23u) | (d & 0x7fffffu);
    if (signed && ((d >> 23u) & 1u) == 1u) { bits = bits | 0x80000000u; }
    return bitcast<f32>(bits);
}

fn tanh_d(x: f32) -> f32 {
    let t = select(x, 0.0, abs(x) < 7.888609052210118e-31);
    let u = clamp(t * 0.5, -1.0, 1.0);
    return u * (2.0 - abs(u));
}
fn sigmoid_d(x: f32) -> f32 {
    let t = select(x, 0.0, abs(x) < 7.888609052210118e-31);
    let v = clamp(t * 0.25, -1.0, 1.0);
    return 0.5 + 0.5 * (v * (2.0 - abs(v)));
}

// Mode 2 is mode 1 with the defined rule: every multiply-add result below 2^-100 in magnitude is 0.
fn flush(x: f32) -> f32 {
    if (p.mode == 2u && abs(x) < 7.888609052210118e-31) { return 0.0; }
    return x;
}

fn sanitize(x: f32, flush: bool) -> f32 {
    var y = x;
    if (!(abs(y) <= 3.0e38)) { y = 0.0; }
    y = clamp(y, -1.0e4, 1.0e4);
    if (flush && abs(y) < 7.888609052210118e-31) { y = 0.0; }
    return y;
}

fn tuple(i: u32, slot: u32, variant: u32) {
    let h = hash(i * 16u);
    let n_in = 1u + (h & 3u);
    let curve = (h >> 2u) & 3u;
    let n_c = (h >> 4u) & 3u;
    var w: array<f32, 4>;
    var s: array<f32, 4>;
    let tiny = p.mode >= 1u;
    let bias = value(hash(i * 16u + 1u), select(-6, -70, tiny), select(4, -60, tiny), true);
    for (var k = 0u; k < 4u; k++) {
        w[k] = value(hash(i * 16u + 2u + k), select(-6, -70, tiny), select(4, 4, tiny) , true);
        s[k] = value(hash(i * 16u + 6u + k), select(-10, -70, tiny), select(13, 13, tiny), true);
    }
    let keep = value(hash(i * 16u + 10u), select(-8, -70, tiny), 0, false);
    var x = bias;
    for (var k = 0u; k < n_in; k++) {
        x = MULADD(w[k], s[k], x);
    }
    var y = x;
    if (curve == 1u) { y = select(0.0, 1.0, x > 0.0); }
    if (curve == 2u) { y = sigmoid_d(x); }
    if (curve == 3u) { y = tanh_d(x); }
    var next = MULADD(keep, s[0], y);
    for (var j = 0u; j < n_c; j++) {
        next = MULADD(s[j], w[3u - j], next);
    }
    let o = (slot * 2u + variant) * 3u;
    out[o] = bitcast<u32>(x);
    out[o + 1u] = bitcast<u32>(y);
    out[o + 2u] = bitcast<u32>(sanitize(next, true));
}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    tuple(p.base + id.x, id.x, VARIANT);
}
"#;

fn hash(v: u32) -> u32 {
    let s = v.wrapping_mul(747796405).wrapping_add(2891336453);
    let w = ((s >> ((s >> 28) + 4)) ^ s).wrapping_mul(277803737);
    (w >> 22) ^ w
}

fn value(d: u32, elo: i32, ehi: i32, signed: bool) -> f32 {
    let e = elo + ((d >> 24) % (ehi - elo + 1) as u32) as i32;
    let mut bits = (((e + 127) as u32) << 23) | (d & 0x7f_ffff);
    if signed && (d >> 23) & 1 == 1 {
        bits |= 0x8000_0000;
    }
    f32::from_bits(bits)
}

fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    x.max(lo).min(hi)
}

const TINY: f32 = 7.888_609e-31;

fn tanh_d(x: f32) -> f32 {
    let t = if x.abs() < TINY { 0.0 } else { x };
    let u = clamp(t * 0.5, -1.0, 1.0);
    u * (2.0 - u.abs())
}

fn sigmoid_d(x: f32) -> f32 {
    let t = if x.abs() < TINY { 0.0 } else { x };
    let v = clamp(t * 0.25, -1.0, 1.0);
    0.5 + 0.5 * (v * (2.0 - v.abs()))
}

fn sanitize(x: f32) -> f32 {
    let mut y = if x.abs() <= 3.0e38 { x } else { 0.0 };
    y = clamp(y, -1.0e4, 1.0e4);
    if y.abs() < TINY {
        y = 0.0;
    }
    y
}

/// `[x, y, next]` bits, with every multiply-add fused or none.
fn reference(i: u32, mode: u32, fused: bool) -> [u32; 3] {
    let h = hash(i.wrapping_mul(16));
    let n_in = 1 + (h & 3) as usize;
    let curve = (h >> 2) & 3;
    let n_c = ((h >> 4) & 3) as usize;
    let tiny = mode >= 1;
    let pick = |a, b| if tiny { b } else { a };
    let bias = value(hash(i.wrapping_mul(16) + 1), pick(-6, -70), pick(4, -60), true);
    let (mut w, mut s) = ([0f32; 4], [0f32; 4]);
    for k in 0..4u32 {
        w[k as usize] = value(hash(i.wrapping_mul(16) + 2 + k), pick(-6, -70), 4, true);
        s[k as usize] = value(hash(i.wrapping_mul(16) + 6 + k), pick(-10, -70), 13, true);
    }
    let keep = value(hash(i.wrapping_mul(16) + 10), pick(-8, -70), 0, false);
    let ma = |a: f32, b: f32, c: f32| {
        let r = if fused { a.mul_add(b, c) } else { a * b + c };
        if mode == 2 && r.abs() < TINY { 0.0 } else { r }
    };
    let mut x = bias;
    for k in 0..n_in {
        x = ma(w[k], s[k], x);
    }
    let y = match curve {
        1 => if x > 0.0 { 1.0 } else { 0.0 },
        2 => sigmoid_d(x),
        3 => tanh_d(x),
        _ => x,
    };
    let mut next = ma(keep, s[0], y);
    for j in 0..n_c {
        next = ma(s[j], w[3 - j], next);
    }
    [x.to_bits(), y.to_bits(), sanitize(next).to_bits()]
}

pub fn run() {
    let gpu = Gpu::new();
    println!("A2: {}", gpu.name);
    const CHUNK: u32 = 1 << 22;
    // 2^28 tuples per mode, or SPIKE_CHUNKS chunks of 2^22 (for slow adapters).
    let chunks: u32 = std::env::var("SPIKE_CHUNKS").ok().and_then(|v| v.parse().ok()).unwrap_or(64);
    let names = ["plain a*b+c", "wgsl fma"];
    let refs = ["unfused", "fused"];
    let pipelines: Vec<_> = (0..2)
        .map(|v| {
            let ma = |a: &str, b: &str, c: &str| if v == 0 { format!("flush(({a}) * ({b}) + ({c}))") } else { format!("flush(fma({a}, {b}, {c}))") };
            let src = SHADER
                .replace("MULADD(w[k], s[k], x)", &ma("w[k]", "s[k]", "x"))
                .replace("MULADD(keep, s[0], y)", &ma("keep", "s[0]", "y"))
                .replace("MULADD(s[j], w[3u - j], next)", &ma("s[j]", "w[3u - j]", "next"))
                .replace("VARIANT", &v.to_string());
            gpu.pipeline(&src, "main")
        })
        .collect();
    let out = gpu.storage(CHUNK as usize * 2 * 3 * 4);
    for mode in 0..3u32 {
        // [variant][reference][value] mismatches
        let mut mism = [[[0u64; 3]; 2]; 2];
        let mut total = 0u64;
        let mut examples: Vec<String> = Vec::new();
        for chunk in 0..chunks {
            let base = chunk * CHUNK;
            let uniform = gpu.uniform_with(&[base, mode]);
            for v in 0..2 {
                gpu.run(&pipelines[v], &[&uniform, &out], (CHUNK / 256, 1, 1));
            }
            let got: Vec<u32> = gpu.read(&out, CHUNK as usize * 6);
            let threads = 16usize;
            let per = CHUNK as usize / threads;
            let parts: Vec<([[[u64; 3]; 2]; 2], Vec<String>)> = std::thread::scope(|s| {
                let handles: Vec<_> = (0..threads)
                    .map(|t| {
                        let got = &got;
                        s.spawn(move || {
                            let mut m = [[[0u64; 3]; 2]; 2];
                            let mut ex = Vec::new();
                            for i in t * per..(t + 1) * per {
                                let r = [reference(base + i as u32, mode, false), reference(base + i as u32, mode, true)];
                                for v in 0..2 {
                                    let g = &got[(i * 2 + v) * 3..(i * 2 + v) * 3 + 3];
                                    for (k, rr) in r.iter().enumerate() {
                                        for j in 0..3 {
                                            if g[j] != rr[j] {
                                                m[v][k][j] += 1;
                                                if ex.len() < 4 && j == 2 && v == 0 && k == 1 {
                                                    ex.push(format!("tuple {} next gpu={:#010x} unfused={:#010x} fused={:#010x}", base + i as u32, g[j], r[0][j], r[1][j]));
                                                }
                                            }
                                        }
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
                for v in 0..2 {
                    for k in 0..2 {
                        for j in 0..3 {
                            mism[v][k][j] += m[v][k][j];
                        }
                    }
                }
                examples.extend(ex);
            }
            total += CHUNK as u64;
        }
        println!("mode {} ({}): {total} tuples; mismatches [x before the curve, y after it, next after sanitize]", mode, ["runner ranges", "tiny values", "tiny values, multiply-adds flushed below 2^-100"][mode as usize]);
        for v in 0..2 {
            for k in 0..2 {
                println!("  gpu {:12} vs cpu {:8}: {:?}", names[v], refs[k], mism[v][k]);
            }
        }
        for e in examples.iter().take(4) {
            println!("  e.g. {e}");
        }
    }
}
