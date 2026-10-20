//! B, variant 4 (proposals/gpu-spike.md): what binning entities by shape would cost, by
//! measuring its parts. A shape is a kernel specialised for one sequence of kinds. This
//! measures the time to create one such pipeline and the cost of dispatching many small bins.

use crate::census::SLOT_WORDS;
use crate::gpu::Gpu;

/// Straight-line WGSL for one shape: `(kind, n_in)` per slot, kinds 0 to 5 only.
fn shape_shader(shape: &[(u32, u32)], signals: usize) -> String {
    let mut body = String::new();
    for step in 0..3 {
        body.push_str(&format!("    // step {step}\n    for (var j = 0u; j < ns; j++) {{ nx[j] = keep[j] * s[j]; }}\n"));
        for (g, &(kind, n_in)) in shape.iter().enumerate() {
            let so = format!("(slots + {g}u * {SLOT_WORDS}u)");
            let o = format!("((words[{so}] >> 8u) & 255u)");
            match kind {
                0 => body.push_str(&format!("    nx[{o}] += rd1[{g}];\n")),
                1..=4 => {
                    let mut x = format!("bitcast<f32>(words[{so} + 1u])");
                    for k in 0..n_in {
                        x = format!("({x} + bitcast<f32>(words[{so} + {}u]) * s[words[{so} + {}u] & 31u])", 2 + 2 * k, 3 + 2 * k);
                    }
                    let y = match kind {
                        1 => x,
                        2 => format!("select(0.0, 1.0, {x} > 0.0)"),
                        3 => format!("sigmoid_d({x})"),
                        _ => format!("tanh_d({x})"),
                    };
                    body.push_str(&format!("    nx[{o}] += {y};\n"));
                }
                _ => body.push_str(&format!("    nx[{o}] += s[words[{so} + 3u] & 31u] * s[words[{so} + 5u] & 31u];\n")),
            }
        }
        body.push_str("    for (var j = 0u; j < ns; j++) { s[j] = sanitize(nx[j]); }\n");
    }
    format!(
        r#"
@group(0) @binding(0) var<storage, read> offs: array<u32>;
@group(0) @binding(1) var<storage, read> words: array<u32>;
@group(0) @binding(2) var<storage, read_write> sig: array<f32>;
@group(0) @binding(3) var<storage, read> cells: array<f32>;
@group(0) @binding(4) var<storage, read_write> res: array<f32>;
fn tanh_d(x: f32) -> f32 {{
    let t = select(x, 0.0, abs(x) < 7.888609052210118e-31);
    let u = clamp(t * 0.5, -1.0, 1.0);
    return u * (2.0 - abs(u));
}}
fn sigmoid_d(x: f32) -> f32 {{
    let t = select(x, 0.0, abs(x) < 7.888609052210118e-31);
    let v = clamp(t * 0.25, -1.0, 1.0);
    return 0.5 + 0.5 * (v * (2.0 - abs(v)));
}}
fn sanitize(x: f32) -> f32 {{
    var y = x;
    if (!(abs(y) <= 3.0e38)) {{ y = 0.0; }}
    return clamp(y, -1.0e4, 1.0e4);
}}
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
    let e = id.x;
    if (e >= arrayLength(&offs)) {{ return; }}
    let base = offs[e];
    let ns = min(words[base + 1u], {signals}u);
    let slots = base + 2u + words[base + 1u];
    var s: array<f32, {signals}>;
    var nx: array<f32, {signals}>;
    var keep: array<f32, {signals}>;
    var rd1: array<f32, {signals}>;
    for (var j = 0u; j < ns; j++) {{ s[j] = sig[e * {signals}u + j]; keep[j] = bitcast<f32>(words[base + 2u + j]); }}
    let cell = (e * 2654435761u) >> 16u;
    for (var g = 0u; g < {n}u; g++) {{ rd1[g] = cells[(cell + g * 977u) & 65535u]; }}
{body}    var acc = 0.0;
    for (var j = 0u; j < ns; j++) {{ sig[e * {signals}u + j] = s[j]; acc += s[j] * 0.001; }}
    res[e] = acc;
}}
"#,
        n = shape.len().min(signals)
    )
}

pub fn run(shape_counts: Option<(usize, usize)>) {
    let gpu = Gpu::new();
    println!("variant 4 (binning cost by parts): {}", gpu.name);
    let mut rng = 7u64;
    let mut next = |n: u32| {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((rng >> 33) as u32) % n
    };
    // 24 distinct shapes of 12 mind genes each (the real mean is about 12 active genes).
    let shapes: Vec<Vec<(u32, u32)>> = (0..24)
        .map(|_| (0..12).map(|_| {
            let kind = next(6);
            (kind, if (1..=4).contains(&kind) { 1 + next(4) } else { 0 })
        }).collect())
        .collect();
    let mut create = Vec::new();
    let mut pipelines = Vec::new();
    for sh in &shapes {
        let src = shape_shader(sh, 32);
        let t = std::time::Instant::now();
        pipelines.push(gpu.pipeline(&src, "main"));
        create.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let mean = create.iter().sum::<f64>() / create.len() as f64;
    println!("pipeline creation for a 12-gene shape: mean {mean:.1} ms, min {:.1}, max {:.1} (24 shapes, unrolled WGSL)", create.iter().cloned().fold(f64::MAX, f64::min), create.iter().cloned().fold(0.0, f64::max));

    // Small bins: one workgroup each, 2000 dispatches cycling over the pipelines, in one pass.
    let n = 64usize;
    let words = vec![0u32; 2 + 32 + 12 * 12];
    let offs = vec![0u32; n];
    let (offs_b, words_b) = (gpu.storage_with(&offs), gpu.storage_with(&words));
    let sig_b = gpu.storage(n * 32 * 4);
    let cells_b = gpu.storage_with(&vec![0f32; 65536]);
    let res_b = gpu.storage(n * 4);
    let bufs = [&offs_b, &words_b, &sig_b, &cells_b, &res_b];
    let t_one = gpu.bench(&pipelines[0], &bufs, (1, 1, 1), 2000);
    println!("2000 one-workgroup dispatches of one pipeline: {:.2} us each on the GPU, {:.2} us wall", t_one.0.unwrap_or(f64::NAN) * 1e3, t_one.1 * 1e3);
    // Switching pipelines: one pass per pipeline is the same as switching; measure 24 passes of 100.
    let t = std::time::Instant::now();
    let mut gpu_ms = 0.0;
    for p in &pipelines {
        let r = gpu.bench(p, &bufs, (1, 1, 1), 100);
        gpu_ms += r.0.unwrap_or(f64::NAN) * 100.0;
    }
    let wall = t.elapsed().as_secs_f64() * 1e3;
    println!("24 pipelines x 100 dispatches, one submission each: {:.1} ms wall in total ({:.1} us per dispatch incl. submit and wait), {:.2} us per dispatch on the GPU", wall, wall * 1e3 / 2400.0, gpu_ms * 1e3 / 2400.0);

    if let Some((mind_shapes, full_shapes)) = shape_counts {
        // Dispatch cost in one pass (the first measurement) times the number of bins.
        let per = t_one.0.unwrap_or(f64::NAN);
        println!("binned tick estimate for the real population: {mind_shapes} mind shapes x {per:.4} ms = {:.1} ms of dispatch overhead alone ({full_shapes} full shapes: {:.1} ms)", mind_shapes as f64 * per, full_shapes as f64 * per);
        println!("compiling every shape once: {mind_shapes} x {mean:.0} ms = {:.1} s", mind_shapes as f64 * mean / 1e3);
    }
}
