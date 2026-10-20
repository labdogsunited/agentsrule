// Per-tick preparation (CONSTANTS first): the contact buckets (a counting sort of the entities)
// and the offset of each entity's records.

@group(0) @binding(0) var<storage, read_write> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> ent: array<u32>;
@group(0) @binding(2) var<storage, read_write> epos: array<f32>;
@group(0) @binding(3) var<storage, read_write> bkt_in: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> bkt_out: array<u32>;
@group(0) @binding(5) var<storage, read_write> bkt_blocks: array<u32>;
@group(0) @binding(6) var<storage, read_write> cursor: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> norder: array<u32>;
@group(0) @binding(8) var<storage, read_write> gen_in: array<u32>;
@group(0) @binding(9) var<storage, read_write> gen_out: array<u32>;
@group(0) @binding(10) var<storage, read_write> gen_blocks: array<u32>;
@group(0) @binding(11) var<storage, read_write> rec_ent: array<u32>;

fn pf(i: u32) -> f32 { return bitcast<f32>(params[i]); }
const P_NBT = 50u;
const P_ISX = 51u;
const P_ISY = 52u;

@compute @workgroup_size(64)
fn assign(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let bx = min(u32(epos[e * 4u] * pf(P_ISX)), params[P_NBX] - 1u);
    let by = min(u32(epos[e * 4u + 1u] * pf(P_ISY)), params[P_NBY] - 1u);
    let b = by * params[P_NBX] + bx;
    ent[e * ENT + 3u] = b;
    atomicAdd(&bkt_in[b], 1u);
    gen_in[e] = ent[e * ENT + 5u];
}

@compute @workgroup_size(1)
fn finish_nstart() {
    bkt_out[params[P_NBT]] = params[P_N];
}

@compute @workgroup_size(64)
fn scatter(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let b = ent[e * ENT + 3u];
    let p = bkt_out[b] + atomicAdd(&cursor[b], 1u);
    norder[p] = e;
}

@compute @workgroup_size(64)
fn set_roff(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let roff = gen_out[e];
    ent[e * ENT + 8u] = roff * REC;
    let n_genes = ent[e * ENT + 5u];
    for (var k = 0u; k < n_genes; k++) { rec_ent[roff + k] = e; }
}
