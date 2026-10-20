//! Variation on the GPU (milestone M4): the batch test of the mutation kernel.

use crate::gpu::Gpu;
use crate::pack::*;
use crate::shader;

const OUTW: usize = 1100;

pub fn shader_source() -> String {
    format!("{}\n{}\n{}", shader::constants(), shader::contract(), include_str!("mutate.wgsl"))
}

/// Mutates each job's parent genome on the GPU. `masters` are the parents' master words, laid
/// end to end; a job is `[offset of the parent, lineage, child id (low word), tick]`.
pub fn run_batch(gpu: &Gpu, menus: &[u32], rates: &[u32], seed: u64, masters: &[u32], jobs: &[[u32; 4]]) -> Vec<Vec<u32>> {
    let module = gpu.module(&shader_source());
    let (bgl, layout) = gpu.storage_layout(6);
    let pipeline = gpu.pipeline_in(&module, "mutate_batch", &layout);
    let mut params = vec![0u32; P_COUNT];
    params[P_SEEDLO] = seed as u32;
    params[P_SEEDHI] = (seed >> 32) as u32;
    params[53] = jobs.len() as u32;
    let p = gpu.storage_with(&params);
    let m = gpu.storage_with(menus);
    let r = gpu.storage_with(rates);
    let mw = gpu.storage_with(masters);
    let j = gpu.storage_with(&jobs.concat());
    let out = gpu.storage(jobs.len() * OUTW * 4);
    let group = gpu.group(&bgl, &[&p, &m, &r, &mw, &j, &out]);
    gpu.dispatch(&[(&pipeline, &group, (jobs.len() as u32).div_ceil(64))]);
    let words: Vec<u32> = gpu.read(&out, jobs.len() * OUTW);
    words
        .chunks(OUTW)
        .map(|c| {
            let (ns, nk) = (c[0] as usize, c[2] as usize);
            let mut v = c[..MASTER_HDR + nk + ns * SLOT].to_vec();
            v.extend_from_slice(&c[1000..1058]);
            v
        })
        .collect()
}
