// Restructure (spec §7.5) without mutation: disposal, division, ageing and compaction.
// CONSTANTS and CONTRACT are inserted before this text; the scan kernels are generated.

@group(0) @binding(0) var<storage, read_write> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> ent: array<u32>;
@group(0) @binding(2) var<storage, read_write> epos: array<f32>;
@group(0) @binding(3) var<storage, read_write> amounts: array<u32>;
@group(0) @binding(4) var<storage, read_write> ecap: array<u32>;
@group(0) @binding(5) var<storage, read_write> sig: array<f32>;
@group(0) @binding(6) var<storage, read_write> camt: array<u32>;
@group(0) @binding(7) var<storage, read_write> ccap: array<u32>;
@group(0) @binding(8) var<storage, read_write> rec: array<u32>;
@group(0) @binding(9) var<storage, read_write> dying: array<u32>;
@group(0) @binding(10) var<storage, read_write> gw: array<u32>;
@group(0) @binding(11) var<storage, read_write> reactions: array<u32>;
@group(0) @binding(12) var<storage, read_write> ent2: array<u32>;
@group(0) @binding(13) var<storage, read_write> epos2: array<f32>;
@group(0) @binding(14) var<storage, read_write> amounts2: array<u32>;
@group(0) @binding(15) var<storage, read_write> ecap2: array<u32>;
@group(0) @binding(16) var<storage, read_write> sig2: array<f32>;
@group(0) @binding(17) var<storage, read_write> alive_in: array<u32>;
@group(0) @binding(18) var<storage, read_write> alive_out: array<u32>;
@group(0) @binding(19) var<storage, read_write> alive_blocks: array<u32>;
@group(0) @binding(20) var<storage, read_write> cand_in: array<u32>;
@group(0) @binding(21) var<storage, read_write> cand_out: array<u32>;
@group(0) @binding(22) var<storage, read_write> cand_blocks: array<u32>;
@group(0) @binding(23) var<storage, read_write> list: array<u64>;
@group(0) @binding(24) var<storage, read_write> counters: array<atomic<u32>>;
@group(0) @binding(25) var<storage, read_write> sortp: array<u32>;

const RX = 20u;
const P_MAXAGE = 40u;
const P_MAXENT = 41u;
const P_NEXTID_LO = 42u;
const P_NEXTID_HI = 43u;
const P_DECOMP = 44u;
const P_CHILDOFF = 45u;
const P_NALIVE = 46u;     // survivors, set by the host between steps
const P_NSUCCESS = 47u;   // children, set by the host between steps
const P_LAGE = 10u;
const P_LLINEAGE = 11u;
const WORD_AGE = 10u;
const WORD_LINEAGE = 11u;

fn U64_MAX() -> u64 { return (u64(0xffffffffu) << 32u) | u64(0xffffffffu); }

// ---- 1: who is removed, who may divide ----
fn removed_of(e: u32) -> bool {
    let age = ent[e * ENT + WORD_AGE];
    return dying[e] == 1u || age + 1u >= params[P_MAXAGE];
}

// Division of entity e: the Divide record, if its fee was paid and the requirement holds.
fn divide_gene(e: u32) -> u32 {
    let n_genes = ent[e * ENT + 5u];
    let roff = ent[e * ENT + 8u];
    for (var g = 0u; g < n_genes; g++) {
        let base = roff + g * REC;
        if (rec[base] == R_DIVIDE) {
            if (rec[base + 7u] == 0u) { return NONE; }
            let w = ent[e * ENT + 4u] + ent[e * ENT + 6u] + g * SLOT;
            if (amounts[e * params[P_NS] + gw[w + 8u]] < gw[w + 9u]) { return NONE; }
            return g;
        }
    }
    return NONE;
}

@compute @workgroup_size(64)
fn mark(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let removed = removed_of(e);
    alive_in[e] = select(1u, 0u, removed);
    cand_in[e] = select(select(0u, 1u, divide_gene(e) != NONE), 0u, removed);
    if (removed) {
        let slot = atomicAdd(&counters[0], 1u);
        list[slot] = (u64(ent[e * ENT + 2u]) << 32u) | u64(e);
    }
}

@compute @workgroup_size(64)
fn pad_list(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let m = atomicLoad(&counters[0]);
    let n = sortp[2];
    if (i >= m && i < n) { list[i] = U64_MAX(); }
}

@compute @workgroup_size(64)
fn bitonic(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let n = sortp[2];
    if (i >= n) { return; }
    let k = sortp[0];
    let j = sortp[1];
    let l = i ^ j;
    if (l > i) {
        let a = list[i];
        let b = list[l];
        let up = (i & k) == 0u;
        if ((up && a > b) || (!up && a < b)) {
            list[i] = b;
            list[l] = a;
        }
    }
}

// ---- 2: disposal, one thread per cell that has removed entities, in entity order ----
@compute @workgroup_size(64)
fn dispose(@builtin(global_invocation_id) id: vec3<u32>) {
    let pos = id.x;
    let m = atomicLoad(&counters[0]);
    if (pos >= m) { return; }
    let cell = u32(list[pos] >> 32u);
    if (pos > 0u && u32(list[pos - 1u] >> 32u) == cell) { return; }
    let ns = params[P_NS];
    var q = pos;
    while (q < m && u32(list[q] >> 32u) == cell) {
        let e = u32(list[q] & u64(0xffffffffu));
        // `decompose`: the reaction to its full extent, no cap on its outputs.
        let d = params[P_DECOMP];
        if (d != NONE) {
            let r = d * RX;
            var extent = 0xffffffffu;
            let n_in = reactions[r];
            let n_out = reactions[r + 1u];
            for (var k = 0u; k < n_in; k++) {
                extent = min(extent, amounts[e * ns + reactions[r + 2u + 2u * k]] / reactions[r + 3u + 2u * k]);
            }
            for (var k = 0u; k < n_in; k++) { amounts[e * ns + reactions[r + 2u + 2u * k]] -= extent * reactions[r + 3u + 2u * k]; }
            for (var k = 0u; k < n_out; k++) { amounts[e * ns + reactions[r + 10u + 2u * k]] += extent * reactions[r + 11u + 2u * k]; }
        }
        for (var s = 0u; s < ns; s++) {
            let amount = amounts[e * ns + s];
            let room = ccap[cell * ns + s] - camt[cell * ns + s];
            camt[cell * ns + s] += min(amount, room);
            amounts[e * ns + s] = 0u;
        }
        q++;
    }
}

// ---- 3: division, one thread per candidate parent; children are written after the survivors ----
@compute @workgroup_size(64)
fn divide(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N] || cand_in[e] == 0u) { return; }
    let rank = cand_out[e];
    if (params[P_NALIVE] + rank >= params[P_MAXENT]) { return; }
    let ns = params[P_NS];
    let g = divide_gene(e);
    let w = ent[e * ENT + 4u] + ent[e * ENT + 6u] + g * SLOT;
    let split_bp = u64(gw[w + 5u]);
    let child = params[P_NALIVE] + rank;
    let child_id = ((u64(params[P_NEXTID_HI]) << 32u) | u64(params[P_NEXTID_LO])) + u64(rank);
    for (var s = 0u; s < ns; s++) {
        let share = u32(u64(amounts[e * ns + s]) * split_bp / u64(10000u));
        amounts[e * ns + s] -= share;
        amounts2[child * ns + s] = share;
        ecap2[child * ns + s] = ecap[e * ns + s];
    }
    // Placed behind the parent, after its movement (spec §7.5).
    let h = vec2<f32>(epos[e * 4u + 2u], epos[e * 4u + 3u]);
    let off = bitcast<f32>(params[P_CHILDOFF]);
    let wrap = params[P_WRAP] == 1u;
    let p = vec2<f32>(fm(-h.x, off, epos[e * 4u]), fm(-h.y, off, epos[e * 4u + 1u]));
    let q = vec2<f32>(fit(p.x, pf(P_W), wrap), fit(p.y, pf(P_H), wrap));
    epos2[child * 4u] = q.x;
    epos2[child * 4u + 1u] = q.y;
    epos2[child * 4u + 2u] = h.x;
    epos2[child * 4u + 3u] = h.y;
    let gwid = i32(params[P_GW]);
    let gh = i32(params[P_GH]);
    let cx = clamp(i32(floor(q.x * pf(P_CKX))), 0, gwid - 1);
    let cy = clamp(i32(floor(q.y * pf(P_CKY))), 0, gh - 1);
    let b = child * ENT;
    ent2[b] = u32(child_id & u64(0xffffffffu));
    ent2[b + 1u] = u32(child_id >> 32u);
    ent2[b + 2u] = u32(cy * gwid + cx);
    ent2[b + 3u] = 0u;
    ent2[b + 4u] = ent[e * ENT + 4u];      // the genome words are shared with the parent
    ent2[b + 5u] = ent[e * ENT + 5u];
    ent2[b + 6u] = ent[e * ENT + 6u];
    ent2[b + 7u] = ent[e * ENT + 7u] & 5u;   // the genome's flags; a newborn has no `prev`
    ent2[b + 8u] = 0u;
    ent2[b + 9u] = ent[e * ENT + 9u];
    ent2[b + WORD_AGE] = 0u;
    ent2[b + WORD_LINEAGE] = ent[e * ENT + WORD_LINEAGE];
    for (var j = 0u; j < NSIG; j++) { sig2[child * NSIG + j] = 0.0; }
}

// ---- 4: survivors move to their place in the new arrays, older and in the cell under them ----
@compute @workgroup_size(64)
fn compact(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N] || alive_in[e] == 0u) { return; }
    let ns = params[P_NS];
    let to = alive_out[e];
    for (var k = 0u; k < ENT; k++) { ent2[to * ENT + k] = ent[e * ENT + k]; }
    ent2[to * ENT + WORD_AGE] = ent[e * ENT + WORD_AGE] + 1u;
    let gwid = i32(params[P_GW]);
    let gh = i32(params[P_GH]);
    let cx = clamp(i32(floor(epos[e * 4u] * pf(P_CKX))), 0, gwid - 1);
    let cy = clamp(i32(floor(epos[e * 4u + 1u] * pf(P_CKY))), 0, gh - 1);
    ent2[to * ENT + 2u] = u32(cy * gwid + cx);
    for (var k = 0u; k < 4u; k++) { epos2[to * 4u + k] = epos[e * 4u + k]; }
    for (var s = 0u; s < ns; s++) {
        amounts2[to * ns + s] = amounts[e * ns + s];
        ecap2[to * ns + s] = ecap[e * ns + s];
    }
    for (var j = 0u; j < NSIG; j++) { sig2[to * NSIG + j] = sig[e * NSIG + j]; }
}

// Bitonic sort of `list` (sortp = [k, j, size, kmax]). Chunks of 1024 are sorted in shared
// memory; steps with j >= 1024 are global; the tail of each merge (j < 1024) is local again.
var<workgroup> tile: array<u64, 1024>;

@compute @workgroup_size(1024)
fn sort_local(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let size = sortp[2];
    let kmax = sortp[3];
    let base = wid.x * 1024u;
    let i = base + lid.x;
    var v = U64_MAX();
    if (i < size && i < atomicLoad(&counters[0])) { v = list[i]; }
    tile[lid.x] = v;
    workgroupBarrier();
    for (var k = 2u; k <= kmax; k = k * 2u) {
        for (var j = k / 2u; j > 0u; j = j / 2u) {
            let l = lid.x ^ j;
            if (l > lid.x) {
                let a = tile[lid.x];
                let b = tile[l];
                let up = (i & k) == 0u;
                if ((up && a > b) || (!up && a < b)) {
                    tile[lid.x] = b;
                    tile[l] = a;
                }
            }
            workgroupBarrier();
        }
    }
    if (i < size) { list[i] = tile[lid.x]; }
}

// The local tail of the merge stage k: j from 512 down to 1.
@compute @workgroup_size(1024)
fn sort_merge_local(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let size = sortp[2];
    let k = sortp[0];
    let base = wid.x * 1024u;
    let i = base + lid.x;
    if (i < size) { tile[lid.x] = list[i]; }
    workgroupBarrier();
    for (var j = 512u; j > 0u; j = j / 2u) {
        let l = lid.x ^ j;
        if (l > lid.x && i < size) {
            let a = tile[lid.x];
            let b = tile[l];
            let up = (i & k) == 0u;
            if ((up && a > b) || (!up && a < b)) {
                tile[lid.x] = b;
                tile[l] = a;
            }
        }
        workgroupBarrier();
    }
    if (i < size) { list[i] = tile[lid.x]; }
}
