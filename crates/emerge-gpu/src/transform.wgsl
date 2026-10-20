// Transform (spec §7.3): entity reactions, world reactions and venting. CONSTANTS first.

@group(0) @binding(0) var<storage, read_write> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> ent: array<u32>;
@group(0) @binding(2) var<storage, read_write> amounts: array<u32>;
@group(0) @binding(3) var<storage, read_write> camt: array<u32>;
@group(0) @binding(4) var<storage, read_write> ccap: array<u32>;
@group(0) @binding(5) var<storage, read_write> ecap: array<u32>;
@group(0) @binding(6) var<storage, read_write> rec: array<u32>;
@group(0) @binding(7) var<storage, read_write> gw: array<u32>;
@group(0) @binding(8) var<storage, read_write> dying: array<u32>;
@group(0) @binding(9) var<storage, read_write> reactions: array<u32>;
@group(0) @binding(10) var<storage, read_write> rlaws: array<u32>;
@group(0) @binding(11) var<storage, read_write> waste: array<u32>;
@group(0) @binding(12) var<storage, read_write> stats: array<atomic<u32>>;
@group(0) @binding(13) var<storage, read_write> flows: array<atomic<u32>>;   // entity-reaction extent by reaction

const RX = 20u;
fn flow_add(kind: u32, s: u32, x: u32) {
    if (x == 0u) { return; }
    let i = (kind * params[P_NS] + s) * 2u;
    let old = atomicAdd(&flows[i], x);
    if (old + x < old) { atomicAdd(&flows[i + 1u], 1u); }
}

const P_NWASTE = 38u;
const P_LAWIDX = 49u;

@compute @workgroup_size(64)
fn react_entities(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N] || dying[e] == 1u) { return; }
    let ns = params[P_NS];
    let n_genes = ent[e * ENT + 5u];
    let nsg = ent[e * ENT + 6u];
    let goff = ent[e * ENT + 4u];
    let roff = ent[e * ENT + 8u];
    for (var g = 0u; g < n_genes; g++) {
        let base = roff + g * REC;
        if (rec[base] != R_REACT) { continue; }
        let r = gw[goff + nsg + g * SLOT + 5u] * RX;
        var extent = rec[base + 3u];
        let n_in = reactions[r];
        let n_out = reactions[r + 1u];
        for (var k = 0u; k < n_in; k++) {
            extent = min(extent, amounts[e * ns + reactions[r + 2u + 2u * k]] / reactions[r + 3u + 2u * k]);
        }
        for (var k = 0u; k < n_out; k++) {
            let s = reactions[r + 10u + 2u * k];
            extent = min(extent, (ecap[e * ns + s] - amounts[e * ns + s]) / reactions[r + 11u + 2u * k]);
        }
        for (var k = 0u; k < n_in; k++) { amounts[e * ns + reactions[r + 2u + 2u * k]] -= extent * reactions[r + 3u + 2u * k]; }
        for (var k = 0u; k < n_out; k++) { amounts[e * ns + reactions[r + 10u + 2u * k]] += extent * reactions[r + 11u + 2u * k]; }
        let si = gw[goff + nsg + g * SLOT + 5u] * 2u;
        let old = atomicAdd(&stats[si], extent);
        if (old + extent < old) { atomicAdd(&stats[si + 1u], 1u); }
    }
}

// One world reaction rule (params[P_LAWIDX]) over every cell.
@compute @workgroup_size(64)
fn react_cells(@builtin(global_invocation_id) id: vec3<u32>) {
    let c = id.x;
    let ns = params[P_NS];
    if (c * ns >= arrayLength(&camt)) { return; }
    let l = params[P_LAWIDX];
    let r = rlaws[l * 2u] * RX;
    let rate = u64(rlaws[l * 2u + 1u]);
    var extent = 0xffffffffu;
    let n_in = reactions[r];
    let n_out = reactions[r + 1u];
    for (var k = 0u; k < n_in; k++) {
        let s = reactions[r + 2u + 2u * k];
        extent = min(extent, u32(u64(camt[c * ns + s]) * rate / u64(10000u) / u64(reactions[r + 3u + 2u * k])));
    }
    for (var k = 0u; k < n_out; k++) {
        let s = reactions[r + 10u + 2u * k];
        extent = min(extent, (ccap[c * ns + s] - camt[c * ns + s]) / reactions[r + 11u + 2u * k]);
    }
    for (var k = 0u; k < n_in; k++) { camt[c * ns + reactions[r + 2u + 2u * k]] -= extent * reactions[r + 3u + 2u * k]; }
    for (var k = 0u; k < n_out; k++) { camt[c * ns + reactions[r + 10u + 2u * k]] += extent * reactions[r + 11u + 2u * k]; }
}

@compute @workgroup_size(64)
fn vent(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let ns = params[P_NS];
    for (var k = 0u; k < params[P_NWASTE]; k++) {
        flow_add(1u, waste[k], amounts[e * ns + waste[k]]);
        amounts[e * ns + waste[k]] = 0u;
    }
}

// The audit: the total of each substance in every store, kind 2 of `flows`.
@compute @workgroup_size(64)
fn audit_totals(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let ns = params[P_NS];
    if (i < params[P_N]) {
        for (var s = 0u; s < ns; s++) { flow_add(2u, s, amounts[i * ns + s]); }
    }
    if (i < arrayLength(&camt) / ns) {
        for (var s = 0u; s < ns; s++) { flow_add(2u, s, camt[i * ns + s]); }
    }
}
