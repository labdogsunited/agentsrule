// Garbage collection of the genome pools (CONSTANTS first): every live entity's master genome and
// view are copied, one after the other, into fresh pools (siblings stop sharing a genome until
// they divide again), and the entities' offsets are updated.

@group(0) @binding(0) var<storage, read_write> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> ent: array<u32>;
@group(0) @binding(2) var<storage, read_write> mw: array<u32>;
@group(0) @binding(3) var<storage, read_write> gw: array<u32>;
@group(0) @binding(4) var<storage, read_write> mw2: array<u32>;
@group(0) @binding(5) var<storage, read_write> gw2: array<u32>;
@group(0) @binding(6) var<storage, read_write> gm_in: array<u32>;
@group(0) @binding(7) var<storage, read_write> gm_out: array<u32>;
@group(0) @binding(8) var<storage, read_write> gm_blocks: array<u32>;
@group(0) @binding(9) var<storage, read_write> gv_in: array<u32>;
@group(0) @binding(10) var<storage, read_write> gv_out: array<u32>;
@group(0) @binding(11) var<storage, read_write> gv_blocks: array<u32>;

@compute @workgroup_size(64)
fn gc_sizes(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let moff = ent[e * ENT + 12u];
    gm_in[e] = MASTER_HDR + mw[moff + 2u] + mw[moff] * SLOT;
    gv_in[e] = ent[e * ENT + 6u] + ent[e * ENT + 5u] * SLOT;
}

@compute @workgroup_size(64)
fn gc_copy(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let moff = ent[e * ENT + 12u];
    let goff = ent[e * ENT + 4u];
    let to_m = gm_out[e];
    let to_v = gv_out[e];
    for (var q = 0u; q < gm_in[e]; q++) { mw2[to_m + q] = mw[moff + q]; }
    for (var q = 0u; q < gv_in[e]; q++) { gw2[to_v + q] = gw[goff + q]; }
    ent[e * ENT + 12u] = to_m;
    ent[e * ENT + 4u] = to_v;
}
