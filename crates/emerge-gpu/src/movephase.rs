//! The Move phase on the GPU (milestone M2): upkeep, fees, requests, two-stage arbitration,
//! commit and movement.

use crate::gpu::Gpu;
use crate::pack::*;
use crate::shader;

pub struct MoveOut {
    pub amounts: Vec<u32>,
    pub camt: Vec<u32>,
    pub epos: Vec<f32>,
    pub dying: Vec<u32>,
    /// The Set records, with word 7 the live flag of each intent.
    pub rec: Vec<u32>,
    /// Per request: origin, dest, s, overflow, requested, steal, stage 1, stage 2.
    pub rq: Vec<u32>,
    /// Requests in the oversubscribed lists of stage 1 and stage 2.
    pub listed: [u32; 2],
}

pub fn shader_source() -> String {
    format!("{}\n{}\n{}", shader::constants(), shader::contract(), include_str!("move.wgsl"))
}

const BINDINGS: u32 = 22;

pub fn run_move(gpu: &Gpu, p: &Packed, rec_in: &[u32]) -> MoveOut {
    let n = p.n;
    let ns = p.ns;
    let nc = p.camt.len() / ns;
    let r = p.params[P_RTOT] as usize;
    let module = gpu.module(&shader_source());
    let (bgl, layout) = gpu.storage_layout(BINDINGS);
    let pipe = |name: &str| gpu.pipeline_in(&module, name, &layout);
    let keys = (nc + n) * ns;

    let params = gpu.storage_with(&p.params);
    let ent = gpu.storage_with(&p.ent);
    let epos = gpu.storage_with(&p.epos);
    let amounts = gpu.storage_with(&p.amounts);
    let camt = gpu.storage_with(&p.camt);
    let ccap = gpu.storage_with(&p.ccap);
    let rec = gpu.storage_with(rec_in);
    let rec_ent = gpu.storage_with(&p.rec_ent);
    let laws = gpu.storage_with(&p.laws.iter().chain([&0u32]).copied().collect::<Vec<_>>());
    let sinkacc = gpu.storage_with(&p.sinkacc);
    let nbr = gpu.storage_with(&p.nbr);
    let dying = gpu.storage(n.max(1) * 4);
    let guards = gpu.storage(n.max(1) * 4);
    let rq = gpu.storage(r.max(1) * 8 * 4);
    let sum_lo = gpu.storage(keys.max(1) * 4);
    let sum_hi = gpu.storage(keys.max(1) * 4);
    let list = gpu.storage(r.next_power_of_two().max(2) * 8);
    let counters = gpu.storage(16);
    let gw = gpu.storage_with(&p.gw);
    let ecap = gpu.storage_with(&p.ecap);
    let scr = gpu.storage(r.next_power_of_two().max(2) * 8);
    let sortp0 = gpu.storage_with(&[0u32, 0, 0, 0]);
    let all = [&params, &ent, &epos, &amounts, &camt, &ccap, &rec, &rec_ent, &laws, &sinkacc, &nbr, &dying, &guards, &rq, &sum_lo, &sum_hi, &list, &counters, &gw, &ecap, &scr];
    let with_sortp = |sortp: &wgpu::Buffer| {
        let mut b: Vec<&wgpu::Buffer> = all.to_vec();
        b.push(sortp);
        gpu.group(&bgl, &b)
    };
    let group = with_sortp(&sortp0);

    let (upkeep, guards_k, gen, sums, flag, pad, bitonic, share, init2, commit, movement) = (
        pipe("upkeep_fees"), pipe("collect_guards"), pipe("gen_requests"), pipe("sums"), pipe("flag_over"), pipe("pad_list"),
        pipe("bitonic"), pipe("share"), pipe("init_stage2"), pipe("commit"), pipe("movement"),
    );
    let wg = |count: usize| (count.max(1) as u32).div_ceil(64);
    gpu.dispatch(&[(&upkeep, &group, wg(n))]);
    gpu.dispatch(&[(&guards_k, &group, wg(n))]);
    gpu.dispatch(&[(&gen, &group, wg(r))]);

    let mut listed = [0u32; 2];
    for stage in 0..2u32 {
        if stage == 1 {
            gpu.queue.write_buffer(&params, (P_STAGE * 4) as u64, bytemuck::bytes_of(&1u32));
            gpu.dispatch(&[(&init2, &group, wg(r))]);
            // Fresh sums and an empty list.
            gpu.queue.write_buffer(&sum_lo, 0, &vec![0u8; keys.max(1) * 4]);
            gpu.queue.write_buffer(&sum_hi, 0, &vec![0u8; keys.max(1) * 4]);
            gpu.queue.write_buffer(&counters, 0, &[0u8; 16]);
        }
        gpu.dispatch(&[(&sums, &group, wg(r)), (&flag, &group, wg(r))]);
        let m: Vec<u32> = gpu.read(&counters, 4);
        let m = m[0] as usize;
        listed[stage as usize] = m as u32;
        if m > 0 {
            let size = m.next_power_of_two().max(2);
            let mut keep = Vec::new(); // the buffers behind the per-step bind groups
            let mut groups = Vec::new();
            let mut k = 2;
            while k <= size {
                let mut j = k / 2;
                while j > 0 {
                    keep.push(gpu.storage_with(&[k as u32, j as u32, size as u32, 0]));
                    j /= 2;
                }
                k *= 2;
            }
            for b in &keep {
                groups.push(with_sortp(b));
            }
            let pad_group = &groups[0];
            let mut steps: Vec<(&wgpu::ComputePipeline, &wgpu::BindGroup, u32)> = vec![(&pad, pad_group, wg(size))];
            for g in &groups {
                steps.push((&bitonic, g, wg(size)));
            }
            steps.push((&share, &group, wg(m)));
            // `share` reads the sort parameters only through the group it is bound with.
            gpu.dispatch(&steps);
        }
    }
    gpu.dispatch(&[(&commit, &group, wg(r)), (&movement, &group, wg(n))]);

    MoveOut {
        amounts: gpu.read(&amounts, n * ns),
        camt: gpu.read(&camt, nc * ns),
        epos: gpu.read(&epos, n * 4),
        dying: gpu.read(&dying, n),
        rec: gpu.read(&rec, rec_in.len()),
        rq: gpu.read(&rq, r * 8),
        listed,
    }
}
