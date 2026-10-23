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
@group(0) @binding(26) var<storage, read_write> work: array<u32>;   // parent of each child, by rank
@group(0) @binding(27) var<storage, read_write> prev_src: array<u32>;   // `prev` as the Set phase left it
@group(0) @binding(28) var<storage, read_write> prev_dst: array<u32>;
@group(0) @binding(29) var<storage, read_write> flows: array<atomic<u32>>;
fn flow_add(kind: u32, s: u32, x: u32) {
    if (x == 0u) { return; }
    let i = (kind * params[P_NS] + s) * 2u;
    let old = atomicAdd(&flows[i], x);
    if (old + x < old) { atomicAdd(&flows[i + 1u], 1u); }
}


const RX = 24u;
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

// Division of entity e: the index of the Divide gene among its active genes, if its fee was paid and the requirement holds.
fn divide_gene(e: u32) -> u32 {
    let na = ent[e * ENT + 13u] & 0xffu;
    let roff = ent[e * ENT + 8u];
    for (var k = 0u; k < na; k++) {
        let base = roff + k * REC;
        if (rec[base] == R_DIVIDE) {
            if (rec[base + 7u] == 0u) { return NONE; }
            let g = rec[base + 2u];
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
        let lin = ent[e * ENT + WORD_LINEAGE];
        acct_add(lin, select(AC_AGED, AC_STARVED, dying[e] == 1u), 1u);
        acct_add(lin, AC_PAY_DIED_WITH_MU, amounts[e * params[P_NS] + params[P_PAYSUB]]);
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
            flow_add(1u, s, amount - min(amount, room));
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
        if (s == params[P_PAYSUB]) {
            acct_add(ent[e * ENT + WORD_LINEAGE], AC_PAY_TO_CHILD_MU, share);
            acct_add(ent[e * ENT + WORD_LINEAGE], AC_PAY_FROM_PARENT_MU, share);
        }
    }
    acct_add(ent[e * ENT + WORD_LINEAGE], AC_BIRTHS, 1u);
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
    ent2[b + 12u] = ent[e * ENT + 12u];   // the master genome is shared with the parent too
    ent2[b + 14u] = 0u;                    // a newborn has made nothing
    ent2[b + 15u] = 0u;
    work[rank] = e;
    ent2[b + WORD_AGE] = 0u;
    ent2[b + WORD_LINEAGE] = ent[e * ENT + WORD_LINEAGE];
    for (var j = 0u; j < NSIG; j++) { sig2[child * NSIG + j] = 0.0; }
}

// ---- 4: survivors move to their place in the new arrays, older and in the cell under them ----
// One thread per word, so that neighbouring threads move neighbouring words. The grid is two-dimensional
// (rows of 32768 groups) when there are many words.
fn tid(id: vec3<u32>) -> u32 { return id.y * 2097152u + id.x; }

@compute @workgroup_size(64)
fn compact_ent(@builtin(global_invocation_id) id: vec3<u32>) {
    let t = tid(id);
    let e = t / ENT;
    let k = t % ENT;
    if (e >= params[P_N] || alive_in[e] == 0u) { return; }
    var v = ent[e * ENT + k];
    if (k == WORD_AGE) {
        v = v + 1u;
    } else if (k == 2u) {
        let gwid = i32(params[P_GW]);
        let gh = i32(params[P_GH]);
        let cx = clamp(i32(floor(epos[e * 4u] * pf(P_CKX))), 0, gwid - 1);
        let cy = clamp(i32(floor(epos[e * 4u + 1u] * pf(P_CKY))), 0, gh - 1);
        v = u32(cy * gwid + cx);
    } else if (k == 7u) {
        // A genome with a `delta` gene keeps the amounts of its last reading.
        if ((v & 4u) != 0u) { v = v | 2u; }
    }
    ent2[alive_out[e] * ENT + k] = v;
}

// The rest of an entity: position and heading, amounts, caps, signals, and `prev` for a `delta` genome.
@compute @workgroup_size(64)
fn compact_rest(@builtin(global_invocation_id) id: vec3<u32>) {
    let ns = params[P_NS];
    let width = 4u + 3u * ns + NSIG;
    let t = tid(id);
    let e = t / width;
    var k = t % width;
    if (e >= params[P_N] || alive_in[e] == 0u) { return; }
    let to = alive_out[e];
    if (k < 4u) { epos2[to * 4u + k] = epos[e * 4u + k]; return; }
    k -= 4u;
    if (k < ns) { amounts2[to * ns + k] = amounts[e * ns + k]; return; }
    k -= ns;
    if (k < ns) { ecap2[to * ns + k] = ecap[e * ns + k]; return; }
    k -= ns;
    if (k < NSIG) { sig2[to * NSIG + k] = sig[e * NSIG + k]; return; }
    k -= NSIG;
    if ((ent[e * ENT + 7u] & 4u) != 0u) { prev_dst[to * ns + k] = prev_src[e * ns + k]; }
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

// After the scans: survivors, division candidates and the children that fit, for the host and the later kernels.
@compute @workgroup_size(1)
fn finish_scans() {
    let n = params[P_N];
    let alive = alive_out[n - 1u] + alive_in[n - 1u];
    let cand = cand_out[n - 1u] + cand_in[n - 1u];
    var children = 0u;
    if (params[P_MAXENT] > alive) { children = min(cand, params[P_MAXENT] - alive); }
    params[P_NALIVE] = alive;
    params[54u] = children;     // P_NCHILD
    atomicStore(&counters[1], alive);
    atomicStore(&counters[2], cand);
    atomicStore(&counters[3], children);
}
