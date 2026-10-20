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

const RX = 24u;
fn flow_add(kind: u32, s: u32, x: u32) {
    if (x == 0u) { return; }
    let i = (kind * params[P_NS] + s) * 2u;
    let old = atomicAdd(&flows[i], x);
    if (old + x < old) { atomicAdd(&flows[i + 1u], 1u); }
}

const P_NWASTE = 38u;
const P_LAWIDX = 49u;

// A reaction's cap (or rate) scaled by its catalyst, `c` µ of which is present: the integer form of
// `cap × (1 + (boost − 1) × c / (c + half))`, as `contract::catalyst_scale` on the CPU.
fn catalysed(r: u32, cap: u32, c: u32) -> u32 {
    let frac = (u64(c) << 16u) / (u64(c) + u64(reactions[r + 19u]));
    let g = u64(reactions[r + 20u]) * frac;
    let extra = u64(cap) * (g >> 32u) + ((u64(cap) * (g & u64(0xffffffffu))) >> 32u);
    return u32(min(u64(cap) + extra, u64(0xffffffffu)));
}

@compute @workgroup_size(64)
fn react_entities(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N] || dying[e] == 1u) { return; }
    let ns = params[P_NS];
    let n_genes = ent[e * ENT + 5u];
    let nr = (ent[e * ENT + 13u] >> 8u) & 0xffu;
    let roff = ent[e * ENT + 8u];
    // The reaction records are at the back of the entity's block, the first reaction last.
    for (var q = 0u; q < nr; q++) {
        let base = roff + (n_genes - 1u - q) * REC;
        let r = rec[base + 4u] * RX;
        var extent = rec[base + 3u];
        let cat = reactions[r + 18u];
        if (cat != 0u) { extent = catalysed(r, extent, amounts[e * ns + cat - 1u]); }
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
        let si = rec[base + 4u] * 2u;
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
    var rate = u64(rlaws[l * 2u + 1u]);
    let cat = reactions[r + 18u];
    if (cat != 0u) { rate = u64(min(catalysed(r, u32(rate), camt[c * ns + cat - 1u]), 10000u)); }
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
