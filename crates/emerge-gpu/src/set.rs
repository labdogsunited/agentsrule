//! The Set phase on the GPU (milestone M1).

use crate::gpu::Gpu;
use crate::pack::*;
use crate::shader;

pub struct SetOut {
    /// `NSIG` floats per entity, after the mind.
    pub sig: Vec<f32>,
    /// `REC` words per active gene.
    pub rec: Vec<u32>,
    /// `ns` words per entity (valid for entities with a `delta` gene).
    pub prev: Vec<u32>,
}

pub fn shader_source() -> String {
    format!("{}\n{}\n{}", shader::constants(), shader::contract(), include_str!("set.wgsl"))
}

pub fn run_set(gpu: &Gpu, p: &Packed) -> (SetOut, std::time::Duration) {
    let pipeline = gpu.pipeline(&shader_source(), "main");
    let params = gpu.storage_with(&p.params);
    let ent = gpu.storage_with(&p.ent);
    let epos = gpu.storage_with(&p.epos);
    let amounts = gpu.storage_with(&p.amounts);
    let prev = gpu.storage_with(&p.prev);
    let gw = gpu.storage_with(&p.gw);
    let sig = gpu.storage_with(&p.sig);
    let camt = gpu.storage_with(&p.camt);
    let ccap = gpu.storage_with(&p.ccap);
    let nstart = gpu.storage_with(&p.nstart);
    let norder = gpu.storage_with(&p.norder);
    let rec = gpu.storage(p.records.max(1) * REC * 4);
    let prev_out = gpu.storage((p.n * p.ns).max(1) * 4);
    let wg = ((p.n as u32).div_ceil(64), 1, 1);
    let t = gpu.run(&pipeline, &[&params, &ent, &epos, &amounts, &prev, &gw, &sig, &camt, &ccap, &nstart, &norder, &rec, &prev_out], wg);
    let out = SetOut {
        sig: gpu.read(&sig, p.n * NSIG),
        rec: gpu.read(&rec, p.records * REC),
        prev: gpu.read(&prev_out, p.n * p.ns),
    };
    (out, t)
}
