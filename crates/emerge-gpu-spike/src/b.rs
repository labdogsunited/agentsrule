//! B (proposals/gpu-spike.md): the cost of running varied minds with a slot loop that reads
//! each slot's kind as data.
//!
//! One thread per entity runs the mind the way `emerge-ref` does: sense genes read once,
//! then `mind_steps = 3` steps of `keep·s` plus the contributions of the sense and
//! regulation genes in gene order, then sanitize; then the action and reaction genes read
//! one signal each. Kinds 0 to 6 are the real ones (`census::*`); kinds 7 and up are
//! synthetic templates of similar cost, to scale the number of kinds `K`.

use crate::census::{pack, PRODUCT, READER, REAL_KINDS, SENSE, SLOT_WORDS, WEIGH_LINEAR};
use crate::gpu::Gpu;

const N: usize = 100_000;
const SIGNALS: usize = 32;
const STEPS: u32 = 3;

/// One entity's mind: `keep` per signal and its packed slots.
#[derive(Clone)]
pub struct Mind {
    pub keep: Vec<f32>,
    pub slots: Vec<[u32; SLOT_WORDS]>,
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    fn unit(&mut self) -> f32 {
        self.next() as f32 / (1u32 << 31) as f32
    }
}

fn shader(k: u32) -> String {
    // Synthetic templates: kind 7.. each get one formula, cycling through eight.
    let mut cases = String::new();
    for kind in REAL_KINDS..k {
        let c = 0.013 * (kind as f32);
        let body = match (kind - REAL_KINDS) % 8 {
            0 => format!("nx[o] += abs(s[a] - s[b]) * wt + {c:?};"),
            1 => format!("nx[o] += max(s[a], s[b] * wt) - {c:?};"),
            2 => format!("nx[o] += sigmoid_d(s[a] * wt + s[b] + {c:?});"),
            3 => format!("nx[o] += tanh_d(wt * s[a] - s[b] - {c:?});"),
            4 => format!("nx[o] += min(s[a], wt + {c:?}) * s[b];"),
            5 => format!("nx[o] += select(s[a], s[b], s[a] > wt + {c:?});"),
            6 => format!("nx[o] += s[a] * s[a] * wt + {c:?};"),
            _ => format!("nx[o] += clamp(s[a] + s[b] * wt, -{c:?} - 1.0, 1.0 + {c:?});"),
        };
        cases.push_str(&format!("            case {kind}u: {{ let a = words[so + 3u] & 31u; let b = words[so + 5u] & 31u; let wt = bitcast<f32>(words[so + 2u]); {body} }}\n"));
    }
    format!(
        r#"
@group(0) @binding(0) var<storage, read> offs: array<u32>;
@group(0) @binding(1) var<storage, read> words: array<u32>;
@group(0) @binding(2) var<storage, read_write> sig: array<f32>;
@group(0) @binding(3) var<storage, read> cells: array<f32>;
@group(0) @binding(4) var<storage, read_write> res: array<f32>;

const NSIG = {SIGNALS}u;
const SLOT = {SLOT_WORDS}u;

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
    let n = words[base];
    let ns = min(words[base + 1u], NSIG);
    let slots = base + 2u + words[base + 1u];
    var s: array<f32, {SIGNALS}>;
    var nx: array<f32, {SIGNALS}>;
    var keep: array<f32, {SIGNALS}>;
    var rd1: array<f32, {SIGNALS}>;
    var rd2: array<f32, {SIGNALS}>;
    for (var j = 0u; j < ns; j++) {{
        s[j] = sig[e * NSIG + j];
        keep[j] = bitcast<f32>(words[base + 2u + j]);
    }}
    let cell = (e * 2654435761u) >> 16u;
    // Senses read once per tick.
    for (var g = 0u; g < min(n, NSIG); g++) {{
        let w0 = words[slots + g * SLOT];
        if ((w0 & 15u) == {SENSE}u) {{
            let r = cells[(cell + ((w0 >> 4u) & 15u) * 977u) & 65535u] * bitcast<f32>(words[slots + g * SLOT + 1u]);
            rd1[g] = r;
            rd2[g] = r * 0.5;
        }}
    }}
    for (var step = 0u; step < {STEPS}u; step++) {{
        for (var j = 0u; j < ns; j++) {{ nx[j] = keep[j] * s[j]; }}
        for (var g = 0u; g < n; g++) {{
            let so = slots + g * SLOT;
            let w0 = words[so];
            let ext = (w0 >> 24u) & 63u;
            let kind = select(w0 & 15u, ext, ext != 0u);
            let o = (w0 >> 8u) & 255u;
            if (o >= NSIG) {{ continue; }}
            switch kind {{
                case {SENSE}u: {{
                    nx[o] += rd1[g & 31u];
                    let o2 = (w0 >> 16u) & 255u;
                    if (o2 < NSIG) {{ nx[o2] += rd2[g & 31u]; }}
                }}
                case 1u, 2u, 3u, 4u: {{
                    var x = bitcast<f32>(words[so + 1u]);
                    let n_in = (w0 >> 4u) & 15u;
                    for (var k = 0u; k < n_in; k++) {{
                        x = x + bitcast<f32>(words[so + 2u + 2u * k]) * s[words[so + 3u + 2u * k] & 31u];
                    }}
                    var y = x;
                    if (kind == 2u) {{ y = select(0.0, 1.0, x > 0.0); }}
                    if (kind == 3u) {{ y = sigmoid_d(x); }}
                    if (kind == 4u) {{ y = tanh_d(x); }}
                    nx[o] += y;
                }}
                case {PRODUCT}u: {{
                    nx[o] += s[words[so + 3u] & 31u] * s[words[so + 5u] & 31u];
                }}
                case {READER}u: {{}}
{cases}                default: {{}}
            }}
        }}
        for (var j = 0u; j < ns; j++) {{ s[j] = sanitize(nx[j]); }}
    }}
    // Action and reaction genes read one signal.
    var acc = 0.0;
    for (var g = 0u; g < n; g++) {{
        let so = slots + g * SLOT;
        if ((words[so] & 15u) == {READER}u) {{
            let sg = words[so + 3u];
            var v = bitcast<f32>(words[so + 2u]);
            if (sg < NSIG) {{ v = s[sg]; }}
            acc += clamp(v, 0.0, 1.0);
        }}
    }}
    for (var j = 0u; j < ns; j++) {{ sig[e * NSIG + j] = s[j]; acc += s[j] * 0.001; }}
    res[e] = acc;
}}
"#
    )
}

/// Packs minds into the buffers the shader reads: per-entity word offsets and the words.
fn upload(minds: &[Mind]) -> (Vec<u32>, Vec<u32>) {
    let mut offs = Vec::with_capacity(minds.len());
    let mut words: Vec<u32> = Vec::new();
    for m in minds {
        offs.push(words.len() as u32);
        words.push(m.slots.len() as u32);
        words.push(m.keep.len() as u32);
        words.extend(m.keep.iter().map(|k| k.to_bits()));
        words.extend(m.slots.iter().flatten());
    }
    (offs, words)
}

/// A random mind of `g` slots over `k` kinds: kind 0 senses, 1 to 4 `weigh` with a curve,
/// 5 `product`, 7 and up synthetic.
fn random_mind(rng: &mut Rng, g: usize, k: u32) -> Mind {
    let n_sig = 16;
    let keep = (0..n_sig).map(|_| rng.unit() * 0.9).collect();
    let slots = (0..g)
        .map(|_| {
            let pick = rng.below(k - 1);
            random_slot(rng, pick, k)
        })
        .collect();
    Mind { keep, slots }
}

fn random_slot(rng: &mut Rng, pick: u32, k: u32) -> [u32; SLOT_WORDS] {
    // Kinds 0..=5 and 7..k-1; 6 (reader) is not drawn in the synthetic minds.
    let kind = if pick >= READER { pick + 1 } else { pick };
    let kind = kind.min(k - 1);
    let mut w = [0u32; SLOT_WORDS];
    let out = rng.below(16);
    match kind {
        SENSE => {
            w[0] = SENSE | (rng.below(8) << 4) | (out << 8) | (255 << 16);
            w[1] = 1.0f32.to_bits();
        }
        1..=4 => {
            let n_in = 1 + rng.below(4);
            w[0] = kind | (n_in << 4) | (out << 8);
            w[1] = ((rng.unit() - 0.5) * 4.0).to_bits();
            for i in 0..n_in as usize {
                w[2 + 2 * i] = ((rng.unit() - 0.5) * 4.0).to_bits();
                w[3 + 2 * i] = rng.below(16);
            }
        }
        PRODUCT => {
            w[0] = PRODUCT | (2 << 4) | (out << 8);
            w[3] = rng.below(16);
            w[5] = rng.below(16);
        }
        _ => {
            w[0] = (kind << 24) | (2 << 4) | (out << 8);
            w[2] = ((rng.unit() - 0.5) * 4.0).to_bits();
            w[3] = rng.below(16);
            w[5] = rng.below(16);
        }
    }
    w
}

fn kind_of(slot: &[u32; SLOT_WORDS]) -> u32 {
    match slot[0] >> 24 {
        0 => slot[0] & 15,
        ext => ext,
    }
}

/// A cheap key from the first four kinds, to see whether partial sorting recovers coherence.
fn key(m: &Mind) -> u32 {
    m.slots.iter().take(4).fold(0, |a, s| a * 16 + kind_of(s))
}

struct Case {
    name: String,
    minds: Vec<Mind>,
    k: u32,
}

fn measure(gpu: &Gpu, case: &Case) {
    let (offs, words) = upload(&case.minds);
    let n = case.minds.len();
    let src = shader(case.k.max(REAL_KINDS + 1));
    let t = std::time::Instant::now();
    let pipeline = gpu.pipeline(&src, "main");
    let compile_ms = t.elapsed().as_secs_f64() * 1e3;
    let offs_b = gpu.storage_with(&offs);
    let words_b = gpu.storage_with(&words);
    let sig_b = gpu.storage(n * SIGNALS * 4);
    let cells: Vec<f32> = (0..65536).map(|i| ((i * 37 % 101) as f32) * 0.01).collect();
    let cells_b = gpu.storage_with(&cells);
    let res_b = gpu.storage(n * 4);
    let wg = ((n as u32).div_ceil(64), 1, 1);
    let bufs = [&offs_b, &words_b, &sig_b, &cells_b, &res_b];
    gpu.bench(&pipeline, &bufs, wg, 3); // warm-up
    let (g, w) = gpu.bench(&pipeline, &bufs, wg, 10);
    let res: Vec<f32> = gpu.read(&res_b, n);
    let bad = res.iter().filter(|v| !v.is_finite()).count();
    let mean = res.iter().sum::<f32>() / n as f32;
    let slots: usize = case.minds.iter().map(|m| m.slots.len()).sum();
    println!(
        "{:<44} {:>8.2} ms GPU  {:>8.2} ms wall   ({:.1} slots/entity, {:.0} KB of genomes, pipeline {:.0} ms, checksum mean {:.3}, non-finite {})",
        case.name,
        g.unwrap_or(f64::NAN),
        w,
        slots as f64 / n as f64,
        words.len() as f64 * 4.0 / 1024.0,
        compile_ms,
        mean,
        bad
    );
}

fn load_real(path: &str) -> Vec<Mind> {
    let bytes = std::fs::read(path).expect("read packed genomes");
    let words: Vec<u32> = bytemuck::pod_collect_to_vec(&bytes);
    let mut minds = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let (n, ns) = (words[i] as usize, words[i + 1] as usize);
        let keep = words[i + 2..i + 2 + ns].iter().map(|&b| f32::from_bits(b)).collect();
        let mut slots = Vec::new();
        for g in 0..n {
            let at = i + 2 + ns + g * SLOT_WORDS;
            slots.push(words[at..at + SLOT_WORDS].try_into().unwrap());
        }
        minds.push(Mind { keep, slots });
        i += 2 + ns + n * SLOT_WORDS;
    }
    minds
}

pub fn run(args: &[String]) {
    let gpu = Gpu::new();
    println!("B: {} (timestamps: {})", gpu.name, gpu.timestamps);
    let mut rng = Rng(12345);
    let mut cases: Vec<Case> = Vec::new();

    // Variants 1, 2b and 3, on synthetic minds.
    for &g in &[16usize, 32] {
        for &k in &[8u32, 16, 32] {
            let one = random_mind(&mut rng, g, k);
            cases.push(Case { name: format!("1  uniform        G={g} K={k}"), minds: vec![one; N], k });
            let drifted: Vec<Mind> = (0..N).map(|_| random_mind(&mut rng, g, k)).collect();
            let mut sorted = drifted.clone();
            sorted.sort_by_key(key);
            cases.push(Case { name: format!("2b random         G={g} K={k}"), minds: drifted, k });
            cases.push(Case { name: format!("3  random sorted  G={g} K={k}"), minds: sorted, k });
        }
    }

    // Variant 2, on the evolved population, sampled with repetition up to N.
    if let Some(path) = args.first() {
        let real = load_real(path);
        println!("real population: {} minds, mean {:.1} slots", real.len(), real.iter().map(|m| m.slots.len()).sum::<usize>() as f64 / real.len() as f64);
        let sampled: Vec<Mind> = (0..N).map(|_| real[rng.below(real.len() as u32) as usize].clone()).collect();
        let mut sorted = sampled.clone();
        sorted.sort_by_key(key);
        cases.push(Case { name: "2  real genomes".into(), minds: sampled.clone(), k: REAL_KINDS + 1 });
        cases.push(Case { name: "2  real genomes, sorted".into(), minds: sorted, k: REAL_KINDS + 1 });
        let one = real[0].clone();
        cases.push(Case { name: "1  uniform (first real genome)".into(), minds: vec![one; N], k: REAL_KINDS + 1 });
    }

    for case in &cases {
        measure(&gpu, case);
    }
    let _ = WEIGH_LINEAR;
    let _ = pack;
}
