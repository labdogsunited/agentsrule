//! The Transform phase on the GPU (milestone M3, first part).

use crate::gpu::Gpu;
use crate::pack::*;
use crate::shader;

pub struct TransformOut {
    pub amounts: Vec<u32>,
    pub camt: Vec<u32>,
}

pub fn shader_source() -> String {
    format!("{}\n{}", shader::constants(), include_str!("transform.wgsl"))
}

pub fn run_transform(gpu: &Gpu, p: &Packed, rec: &[u32], dying: &[u32]) -> TransformOut {
    let (n, ns) = (p.n, p.ns);
    let nc = p.camt.len() / ns;
    let module = gpu.module(&shader_source());
    let (bgl, layout) = gpu.storage_layout(14);
    let params = gpu.storage_with(&p.params);
    let ent = gpu.storage_with(&p.ent);
    let amounts = gpu.storage_with(&p.amounts);
    let camt = gpu.storage_with(&p.camt);
    let ccap = gpu.storage_with(&p.ccap);
    let ecap = gpu.storage_with(&p.ecap);
    let rec = gpu.storage_with(rec);
    let gw = gpu.storage_with(&p.gw);
    let dying = gpu.storage_with(dying);
    let reactions = gpu.storage_with(&p.reactions);
    let rlaws = gpu.storage_with(&p.rlaws.iter().chain([&0u32, &0]).copied().collect::<Vec<_>>());
    let waste = gpu.storage_with(&p.waste.iter().chain([&0u32]).copied().collect::<Vec<_>>());
    let stats = gpu.storage(p.reactions.len() / RX * 4 + 4);
    let flows = gpu.storage(3 * ns * 8 + 8);
    let group = gpu.group(&bgl, &[&params, &ent, &amounts, &camt, &ccap, &ecap, &rec, &gw, &dying, &reactions, &rlaws, &waste, &stats, &flows]);
    let wg = |count: usize| (count.max(1) as u32).div_ceil(64);
    let (re, rc, vent) = (gpu.pipeline_in(&module, "react_entities", &layout), gpu.pipeline_in(&module, "react_cells", &layout), gpu.pipeline_in(&module, "vent", &layout));
    gpu.dispatch(&[(&re, &group, wg(n))]);
    for l in 0..p.params[P_NRLAWS] {
        gpu.queue.write_buffer(&params, (P_LAWIDX * 4) as u64, bytemuck::bytes_of(&l));
        gpu.dispatch(&[(&rc, &group, wg(nc))]);
    }
    gpu.dispatch(&[(&vent, &group, wg(n))]);
    TransformOut { amounts: gpu.read(&amounts, n * ns), camt: gpu.read(&camt, nc * ns) }
}
