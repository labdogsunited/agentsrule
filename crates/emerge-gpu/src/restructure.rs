//! The Restructure phase on the GPU (milestone M3, second part), without mutation.

use crate::gpu::Gpu;
use crate::pack::*;
use crate::shader;

pub struct RestructureOut {
    pub ent: Vec<u32>,
    pub epos: Vec<f32>,
    pub amounts: Vec<u32>,
    pub ecap: Vec<u32>,
    pub sig: Vec<f32>,
    pub camt: Vec<u32>,
    pub alive: usize,
    pub children: usize,
    pub removed: usize,
}

pub fn shader_source() -> String {
    format!("{}\n{}\n{}\n{}\n{}", shader::constants(), shader::contract(), include_str!("restructure.wgsl"), crate::scan::kernels("alive", "params[P_N]", "{name}_in[i]"), crate::scan::kernels("cand", "params[P_N]", "{name}_in[i]"))
}

pub fn run_restructure(gpu: &Gpu, p: &Packed, rec: &[u32], dying: &[u32]) -> RestructureOut {
    let (n, ns) = (p.n, p.ns);
    let nc = p.camt.len() / ns;
    let module = gpu.module(&shader_source());
    let (bgl, layout) = gpu.storage_layout(29);
    let cap = 2 * n.max(1);
    let blocks = n.div_ceil(256).max(1);
    let params = gpu.storage_with(&p.params);
    let ent = gpu.storage_with(&p.ent);
    let epos = gpu.storage_with(&p.epos);
    let amounts = gpu.storage_with(&p.amounts);
    let ecap = gpu.storage_with(&p.ecap);
    let sig = gpu.storage_with(&p.sig);
    let camt = gpu.storage_with(&p.camt);
    let ccap = gpu.storage_with(&p.ccap);
    let rec = gpu.storage_with(rec);
    let dying = gpu.storage_with(dying);
    let gw = gpu.storage_with(&p.gw);
    let reactions = gpu.storage_with(&p.reactions);
    let ent2 = gpu.storage(cap * ENT * 4);
    let epos2 = gpu.storage(cap * 4 * 4);
    let amounts2 = gpu.storage(cap * ns * 4);
    let ecap2 = gpu.storage(cap * ns * 4);
    let sig2 = gpu.storage(cap * NSIG * 4);
    let (alive_in, alive_out, alive_blocks) = (gpu.storage(n.max(1) * 4), gpu.storage(n.max(1) * 4), gpu.storage(blocks * 4));
    let (cand_in, cand_out, cand_blocks) = (gpu.storage(n.max(1) * 4), gpu.storage(n.max(1) * 4), gpu.storage(blocks * 4));
    let list = gpu.storage(n.next_power_of_two().max(2) * 8);
    let counters = gpu.storage(16);
    let sortp0 = gpu.storage_with(&[0u32, 0, 0, 0]);
    let work = gpu.storage(n.max(1) * 4);
    let prev_src = gpu.storage(n.max(1) * ns * 4);
    let prev_dst = gpu.storage(cap * ns * 4);
    let all = [&params, &ent, &epos, &amounts, &ecap, &sig, &camt, &ccap, &rec, &dying, &gw, &reactions, &ent2, &epos2, &amounts2, &ecap2, &sig2, &alive_in, &alive_out, &alive_blocks, &cand_in, &cand_out, &cand_blocks, &list, &counters];
    let with_sortp = |sortp: &wgpu::Buffer| {
        let mut b: Vec<&wgpu::Buffer> = all.to_vec();
        b.push(sortp);
        b.push(&work);
        b.push(&prev_src);
        b.push(&prev_dst);
        gpu.group(&bgl, &b)
    };
    let group = with_sortp(&sortp0);
    let pipe = |name: &str| gpu.pipeline_in(&module, name, &layout);
    let wg = |count: usize| (count.max(1) as u32).div_ceil(64);
    let wg256 = (n.max(1) as u32).div_ceil(256);

    gpu.dispatch(&[(&pipe("mark"), &group, wg(n))]);
    for name in ["alive", "cand"] {
        gpu.dispatch(&[
            (&pipe(&format!("{name}_local")), &group, wg256),
            (&pipe(&format!("{name}_scan_blocks")), &group, 1),
            (&pipe(&format!("{name}_add")), &group, wg256),
        ]);
    }
    let last = |buf_in: &wgpu::Buffer, buf_out: &wgpu::Buffer| -> usize {
        if n == 0 {
            return 0;
        }
        let a: Vec<u32> = gpu.read(buf_in, n);
        let b: Vec<u32> = gpu.read(buf_out, n);
        (a[n - 1] + b[n - 1]) as usize
    };
    let alive = last(&alive_in, &alive_out);
    gpu.queue.write_buffer(&params, (P_NALIVE * 4) as u64, bytemuck::bytes_of(&(alive as u32)));

    // Disposal: the removed entities sorted by cell and then by index.
    let removed = gpu.read::<u32>(&counters, 4)[0] as usize;
    if removed > 0 {
        let size = removed.next_power_of_two().max(2);
        let mut keep = Vec::new();
        let mut k = 2;
        while k <= size {
            let mut j = k / 2;
            while j > 0 {
                keep.push(gpu.storage_with(&[k as u32, j as u32, size as u32, 0]));
                j /= 2;
            }
            k *= 2;
        }
        let groups: Vec<_> = keep.iter().map(|b| with_sortp(b)).collect();
        let (pad, bit, dispose) = (pipe("pad_list"), pipe("bitonic"), pipe("dispose"));
        let mut steps: Vec<(&wgpu::ComputePipeline, &wgpu::BindGroup, u32)> = vec![(&pad, &groups[0], wg(size))];
        for g in &groups {
            steps.push((&bit, g, wg(size)));
        }
        steps.push((&dispose, &group, wg(removed)));
        gpu.dispatch(&steps);
    }

    let (divide, compact) = (pipe("divide"), pipe("compact"));
    gpu.dispatch(&[(&divide, &group, wg(n))]);
    gpu.dispatch(&[(&compact, &group, wg(n))]);
    let candidates = last(&cand_in, &cand_out);
    let children = candidates.min(p.params[P_MAXENT] as usize - alive.min(p.params[P_MAXENT] as usize));
    let total = alive + children;
    RestructureOut {
        ent: gpu.read(&ent2, total * ENT),
        epos: gpu.read(&epos2, total * 4),
        amounts: gpu.read(&amounts2, total * ns),
        ecap: gpu.read(&ecap2, total * ns),
        sig: gpu.read(&sig2, total * NSIG),
        camt: gpu.read(&camt, nc * ns),
        alive,
        children,
        removed,
    }
}
