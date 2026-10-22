//! A1 (archive/gpu-spike.md): the division-free curves on every f32 bit pattern.

use crate::gpu::Gpu;

const SHADER: &str = r#"
struct P { base: u32, }
@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var<storage, read_write> out: array<u32>;

fn tanh_p(x: f32) -> f32 {
    let u = clamp(x * 0.5, -1.0, 1.0);
    return u * (2.0 - abs(u));
}
fn sigmoid_p(x: f32) -> f32 {
    let v = clamp(x * 0.25, -1.0, 1.0);
    return 0.5 + 0.5 * (v * (2.0 - abs(v)));
}

// The defined forms: an input below 2^-100 in magnitude is zero, so no intermediate is denormal.
fn tiny(x: f32) -> f32 {
    return select(x, 0.0, abs(x) < 7.888609052210118e-31);
}
fn tanh_d(x: f32) -> f32 { return tanh_p(tiny(x)); }
fn sigmoid_d(x: f32) -> f32 { return sigmoid_p(tiny(x)); }

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let x = bitcast<f32>(p.base + i);
    out[4u * i] = bitcast<u32>(tanh_p(x));
    out[4u * i + 1u] = bitcast<u32>(sigmoid_p(x));
    out[4u * i + 2u] = bitcast<u32>(tanh_d(x));
    out[4u * i + 3u] = bitcast<u32>(sigmoid_d(x));
}
"#;

fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    x.max(lo).min(hi)
}

pub fn tanh_p(x: f32) -> f32 {
    let u = clamp(x * 0.5, -1.0, 1.0);
    u * (2.0 - u.abs())
}

pub fn sigmoid_p(x: f32) -> f32 {
    let v = clamp(x * 0.25, -1.0, 1.0);
    0.5 + 0.5 * (v * (2.0 - v.abs()))
}

/// An input below 2^-100 in magnitude is zero.
fn tiny(x: f32) -> f32 {
    if x.abs() < 7.888_609e-31 { 0.0 } else { x }
}

pub fn tanh_d(x: f32) -> f32 {
    tanh_p(tiny(x))
}

pub fn sigmoid_d(x: f32) -> f32 {
    sigmoid_p(tiny(x))
}

#[derive(Clone, Copy)]
enum Class {
    Zero,
    Denormal,
    Normal,
    Inf,
    Nan,
}
const NAMES: [&str; 4] = ["tanh_p", "sigmoid_p", "tanh_d", "sigmoid_d"];
const CLASSES: [&str; 5] = ["zero", "denormal", "normal", "infinity", "nan"];

fn class(bits: u32) -> Class {
    let (e, m) = ((bits >> 23) & 0xff, bits & 0x7f_ffff);
    match (e, m) {
        (0, 0) => Class::Zero,
        (0, _) => Class::Denormal,
        (255, 0) => Class::Inf,
        (255, _) => Class::Nan,
        _ => Class::Normal,
    }
}

pub fn run() {
    let gpu = Gpu::new();
    println!("A1: {}", gpu.name);
    const CHUNK: u32 = 1 << 23;
    let pipeline = gpu.pipeline(SHADER, "main");
    let out = gpu.storage(CHUNK as usize * 4 * 4);
    // [function][class] = (inputs, mismatches)
    let mut tally = [[(0u64, 0u64); 5]; 4];
    let mut examples: Vec<String> = Vec::new();
    let mut gpu_time = std::time::Duration::ZERO;
    let chunks: u64 = std::env::var("SPIKE_CHUNKS").ok().and_then(|v| v.parse().ok()).unwrap_or((1u64 << 32) / CHUNK as u64);
    for chunk in 0..chunks {
        let base = (chunk * CHUNK as u64) as u32;
        let uniform = gpu.uniform_with(&[base]);
        gpu_time += gpu.run(&pipeline, &[&uniform, &out], (CHUNK / 256, 1, 1));
        let got: Vec<u32> = gpu.read(&out, CHUNK as usize * 4);
        let threads = 16usize;
        let per = CHUNK as usize / threads;
        let results: Vec<([[(u64, u64); 5]; 4], Vec<String>)> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let got = &got;
                    s.spawn(move || {
                        let mut tally = [[(0u64, 0u64); 5]; 4];
                        let mut ex = Vec::new();
                        for i in t * per..(t + 1) * per {
                            let bits = base.wrapping_add(i as u32);
                            let x = f32::from_bits(bits);
                            let c = class(bits) as usize;
                            let want = [tanh_p(x).to_bits(), sigmoid_p(x).to_bits(), tanh_d(x).to_bits(), sigmoid_d(x).to_bits()];
                            for f in 0..4 {
                                tally[f][c].0 += 1;
                                if got[4 * i + f] != want[f] {
                                    tally[f][c].1 += 1;
                                    if (f == 0 && c == 2 || f >= 2) && ex.len() < 6 {
                                        ex.push(format!("{} x={bits:#010x} gpu={:#010x} cpu={:#010x}", NAMES[f], got[4 * i + f], want[f]));
                                    }
                                }
                            }
                        }
                        (tally, ex)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for (t, ex) in results {
            for f in 0..4 {
                for c in 0..5 {
                    tally[f][c].0 += t[f][c].0;
                    tally[f][c].1 += t[f][c].1;
                }
            }
            for e in ex {
                if examples.len() < 12 {
                    examples.push(e);
                }
            }
        }
    }
    for f in 0..4 {
        for c in 0..5 {
            println!("{:14} {:9}: {:>10} inputs, {:>10} mismatches", NAMES[f], CLASSES[c], tally[f][c].0, tally[f][c].1);
        }
    }
    for e in &examples {
        println!("  e.g. {e}");
    }
    println!("GPU time for 2^32 evaluations of both functions (dispatch only, wall): {:.2} s", gpu_time.as_secs_f64());
}
