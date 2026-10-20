// Move phase (spec §7.2) in contract mode: upkeep, fees, requests, two-stage arbitration,
// commit and movement. CONSTANTS and CONTRACT are inserted before this text.
// Needs the SHADER_INT64 feature: the pro-rata sums and divisions are 64-bit.

@group(0) @binding(0) var<storage, read_write> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> ent: array<u32>;
@group(0) @binding(2) var<storage, read_write> epos: array<f32>;
@group(0) @binding(3) var<storage, read_write> amounts: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> camt: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> ccap: array<u32>;
@group(0) @binding(6) var<storage, read_write> rec: array<u32>;
@group(0) @binding(7) var<storage, read_write> rec_ent: array<u32>;
@group(0) @binding(8) var<storage, read_write> laws: array<u32>;
@group(0) @binding(9) var<storage, read_write> sinkacc: array<u32>;
@group(0) @binding(10) var<storage, read_write> nbr: array<u32>;
@group(0) @binding(11) var<storage, read_write> dying: array<u32>;
@group(0) @binding(12) var<storage, read_write> guards: array<u32>;
@group(0) @binding(13) var<storage, read_write> rq: array<u32>;
@group(0) @binding(14) var<storage, read_write> sum_lo: array<atomic<u32>>;
@group(0) @binding(15) var<storage, read_write> sum_hi: array<atomic<u32>>;
@group(0) @binding(16) var<storage, read_write> list: array<u64>;
@group(0) @binding(17) var<storage, read_write> counters: array<atomic<u32>>;
@group(0) @binding(18) var<storage, read_write> gw: array<u32>;
@group(0) @binding(19) var<storage, read_write> ecap: array<u32>;
@group(0) @binding(20) var<storage, read_write> scr: array<u64>;
@group(0) @binding(21) var<storage, read_write> sortp: array<u32>;

const RQ = 8u;      // words per request: origin, dest, s, overflow, requested, steal, stage1, stage2
const LAW = 28u;    // words per law
const P_PAY = 24u;
const P_UBASE = 25u;
const P_UGENE = 26u;
const P_USIG = 27u;
const P_UKEEP = 28u;
const P_UCARRY = 29u;
const P_SEEDLO = 30u;
const P_SEEDHI = 31u;
const P_NCELLS = 32u;
const P_NLAWS = 33u;
const P_R0 = 34u;
const P_RTOT = 35u;
const P_NSINK = 36u;
const P_STAGE = 48u;
const TAG = 30u;
const IDX = 0x3fffffffu;

fn U64_MAX() -> u64 { return (u64(0xffffffffu) << 32u) | u64(0xffffffffu); }
fn tag_of(p: u32) -> u32 { return p >> TAG; }
fn idx_of(p: u32) -> u32 { return p & IDX; }
fn store_key(p: u32, s: u32) -> u32 {
    let t = tag_of(p);
    if (t == 0u) { return idx_of(p) * params[P_NS] + s; }
    if (t == 1u) { return (params[P_NCELLS] + idx_of(p)) * params[P_NS] + s; }
    return NONE;
}
fn balance(p: u32, s: u32) -> u32 {
    let t = tag_of(p);
    if (t == 0u) { return atomicLoad(&camt[idx_of(p) * params[P_NS] + s]); }
    if (t == 1u) { return atomicLoad(&amounts[idx_of(p) * params[P_NS] + s]); }
    return 0u;
}
fn headroom(p: u32, s: u32) -> u32 {
    let t = tag_of(p);
    if (t == 0u) { let i = idx_of(p) * params[P_NS] + s; return ccap[i] - atomicLoad(&camt[i]); }
    if (t == 1u) { let i = idx_of(p) * params[P_NS] + s; return ecap[i] - atomicLoad(&amounts[i]); }
    return 0xffffffffu;
}

fn philox(ctr_in: vec4<u32>, key_in: vec2<u32>) -> vec4<u32> {
    var ctr = ctr_in;
    var key = key_in;
    for (var round = 0u; round < 10u; round++) {
        if (round > 0u) {
            key.x = key.x + 0x9E3779B9u;
            key.y = key.y + 0xBB67AE85u;
        }
        let p0 = u64(0xD2511F53u) * u64(ctr.x);
        let p1 = u64(0xCD9E8D57u) * u64(ctr.z);
        let hi0 = u32(p0 >> 32u);
        let lo0 = u32(p0);
        let hi1 = u32(p1 >> 32u);
        let lo1 = u32(p1);
        ctr = vec4<u32>(hi1 ^ ctr.y ^ key.x, lo1, hi0 ^ ctr.w ^ key.y, lo0);
    }
    return ctr;
}

// Draw `index` of the stream (seed, tick, stream, domain, aux, pas).
fn draw(stream: u32, domain: u32, aux: u32, pas: u32, index: u32) -> u32 {
    let key = vec2<u32>(params[P_SEEDLO], params[P_SEEDHI]);
    let ctr = vec4<u32>(index / 4u, params[P_TICK], stream, (pas << 24u) | (domain << 16u) | aux);
    let r = philox(ctr, key);
    let q = index % 4u;
    if (q == 0u) { return r.x; }
    if (q == 1u) { return r.y; }
    if (q == 2u) { return r.z; }
    return r.w;
}

// ---- A: upkeep and fees, one thread per entity ----
@compute @workgroup_size(64)
fn upkeep_fees(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let ns = params[P_NS];
    let pay = params[P_PAY];
    let n_genes = ent[e * ENT + 5u];
    let roff = ent[e * ENT + 8u];
    let nsg = ent[e * ENT + 6u];
    let goff = ent[e * ENT + 4u];
    let mask = ent[e * ENT + 9u];
    // Upkeep (spec §3.3), in 64 bits.
    var keep: u64 = u64(0u);
    for (var s = 0u; s < nsg; s++) {
        if ((mask & (1u << s)) != 0u) {
            keep += u64(u32(ceil(f32c(params[P_UKEEP]) * bitcast<f32>(gw[goff + s]))));
        }
    }
    var carried: u64 = u64(0u);
    for (var k = 0u; k < ns; k++) { carried += u64(atomicLoad(&amounts[e * ns + k])); }
    carried = carried / u64(1000u);
    let cost = u64(params[P_UBASE]) + u64(params[P_UGENE]) * u64(n_genes) + u64(params[P_USIG]) * u64(countOneBits(mask)) + keep + u64(params[P_UCARRY]) * carried;
    var dead = false;
    let have = u64(atomicLoad(&amounts[e * ns + pay]));
    if (have >= cost) {
        atomicSub(&amounts[e * ns + pay], u32(cost));
    } else {
        dead = true;
    }
    dying[e] = select(0u, 1u, dead);
    // Fees, in genome order.
    for (var g = 0u; g < n_genes; g++) {
        let base = (roff + g * REC);
        let code = rec[base];
        if (code == 0u || code == R_REACT) { continue; }
        var live = !dead;
        let fee = rec[base + 4u];
        if (live && fee > 0u) {
            if (atomicLoad(&amounts[e * ns + pay]) >= fee) {
                atomicSub(&amounts[e * ns + pay], fee);
            } else {
                live = false;
            }
        }
        rec[base + 7u] = select(0u, 1u, live);
    }
}

// ---- A2: guards, one thread per entity ----
@compute @workgroup_size(64)
fn collect_guards(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    var m = 0u;
    let n_genes = ent[e * ENT + 5u];
    let roff = ent[e * ENT + 8u];
    for (var g = 0u; g < n_genes; g++) {
        let base = roff + g * REC;
        if (rec[base] == R_GUARD && rec[base + 7u] == 1u) { m = m | (1u << rec[base + 2u]); }
    }
    guards[e] = m;
}

fn set_rq(i: u32, origin: u32, dest: u32, s: u32, ovf: u32, requested: u32, steal: u32) {
    let b = i * RQ;
    rq[b] = origin;
    rq[b + 1u] = dest;
    rq[b + 2u] = s;
    rq[b + 3u] = ovf;
    rq[b + 4u] = requested;
    rq[b + 5u] = steal;
    rq[b + 6u] = requested;
    rq[b + 7u] = requested;
}

// ---- B: requests, one thread per request slot ----
@compute @workgroup_size(64)
fn gen_requests(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= params[P_RTOT]) { return; }
    rq[i * RQ + 4u] = 0u;     // empty unless set below; only this word is read for an empty slot
    let ns = params[P_NS];
    if (i < params[P_R0]) {
        let e = rec_ent[i];
        let base = i * REC;
        let code = rec[base];
        if (code == 0u || code == R_REACT || rec[base + 7u] == 0u) { return; }
        let s = rec[base + 2u];
        let amount = rec[base + 3u];
        let other = rec[base + 5u];
        let cell = ent[e * ENT + 2u];
        let me = (1u << TAG) | e;
        let them = (1u << TAG) | other;
        let cellp = cell;
        if (code == R_TAKE) { set_rq(i, cellp, me, s, NONE, amount, 0u); }
        else if (code == R_DROP) { set_rq(i, me, cellp, s, NONE, amount, 0u); }
        else if (code == R_GIVE) { if (dying[other] == 0u) { set_rq(i, me, them, s, NONE, amount, 0u); } }
        else if (code == R_STEAL) {
            if ((guards[other] & (1u << s)) == 0u && dying[other] == 0u) { set_rq(i, them, me, s, NONE, amount, 1u); }
        }
        return;
    }
    // A world rule: find the law whose request range holds i.
    for (var l = 0u; l < params[P_NLAWS]; l++) {
        let lb = l * LAW;
        let off = params[P_R0] + laws[lb + 7u];
        let w = laws[lb + 8u];
        let total = w * params[P_NCELLS];
        if (i < off || i >= off + total) { continue; }
        let c = (i - off) / w;
        let k = (i - off) % w;
        if (laws[lb] == 0u) {
            // Inflow from a source to the cell.
            set_rq(i, (2u << TAG) | laws[lb + 2u], c, laws[lb + 1u], laws[lb + 4u], laws[lb + 3u], 0u);
        } else {
            // Diffusion: k = substance index * 4 + neighbour index.
            let si = k / 4u;
            let ni = k % 4u;
            let nb = nbr[c * 4u + ni];
            if (si >= laws[lb + 6u] || nb == NONE) { return; }
            var count = 0u;
            for (var q = 0u; q < 4u; q++) { if (nbr[c * 4u + q] != NONE) { count++; } }
            let s = laws[lb + 9u + si];
            let offer = u32((u64(atomicLoad(&camt[c * ns + s])) * u64(laws[lb + 5u])) / u64(10000u));
            set_rq(i, c, nb, s, NONE, offer / count, 0u);
        }
        return;
    }
}

// ---- C: stage sums and the oversubscribed list ----
fn add_sum(k: u32, x: u32) {
    let old = atomicAdd(&sum_lo[k], x);
    if (old + x < old) { atomicAdd(&sum_hi[k], 1u); }
}

// stage 1 sums use the origin and the requested amount; stage 2 sums use the destination and the stage 1 grant
@compute @workgroup_size(64)
fn sums(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= params[P_RTOT]) { return; }
    let stage = params[P_STAGE]; // 0 or 1, set by the host
    let b = i * RQ;
    let amount = select(rq[b + 4u], rq[b + 6u], stage == 1u);
    if (rq[b + 4u] == 0u) { return; }
    let p = select(rq[b], rq[b + 1u], stage == 1u);
    let k = store_key(p, rq[b + 2u]);
    if (k == NONE) { return; }
    add_sum(k, amount);
}

@compute @workgroup_size(64)
fn flag_over(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= params[P_RTOT]) { return; }
    let stage = params[P_STAGE];
    let b = i * RQ;
    if (rq[b + 4u] == 0u) { return; }
    let p = select(rq[b], rq[b + 1u], stage == 1u);
    let s = rq[b + 2u];
    let k = store_key(p, s);
    if (k == NONE) { return; }
    let total = (u64(atomicLoad(&sum_hi[k])) << 32u) | u64(atomicLoad(&sum_lo[k]));
    let limit = select(balance(p, s), headroom(p, s), stage == 1u);
    if (total > u64(limit)) {
        let slot = atomicAdd(&counters[0], 1u);
        list[slot] = (u64(k) << 32u) | u64(i);
    }
}

// Pad the list up to a power of two with the largest key.
@compute @workgroup_size(64)
fn pad_list(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let m = atomicLoad(&counters[0]);
    let n = sortp[2];
    if (i >= m && i < n) { list[i] = U64_MAX(); }
}

// One compare-exchange step of a bitonic sort: sortp = [k, j, n].
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

// ---- stage 1 and stage 2 group sharing: one thread per list position that heads a group ----
@compute @workgroup_size(64)
fn share(@builtin(global_invocation_id) id: vec3<u32>) {
    let pos = id.x;
    let m = atomicLoad(&counters[0]);
    if (pos >= m) { return; }
    let stage = params[P_STAGE];
    let key = u32(list[pos] >> 32u);
    if (pos > 0u && u32(list[pos - 1u] >> 32u) == key) { return; }
    var end = pos + 1u;
    while (end < m && u32(list[end] >> 32u) == key) { end++; }
    let size = end - pos;
    let first = u32(list[pos] & u64(0xffffffffu));
    let fb = first * RQ;
    let p = select(rq[fb], rq[fb + 1u], stage == 1u);
    let s = rq[fb + 2u];
    let available = select(balance(p, s), headroom(p, s), stage == 1u);
    if (size == 1u) {
        rq[fb + select(6u, 7u, stage == 1u)] = available;
        return;
    }
    // Stream of the store being shared.
    var stream = idx_of(p);
    var domain = 1u;
    if (tag_of(p) == 1u) { stream = ent[idx_of(p) * ENT]; domain = 0u; }
    let pas = select(1u, 2u, stage == 1u);
    var total: u64 = u64(0u);
    for (var q = pos; q < end; q++) {
        let r = u32(list[q] & u64(0xffffffffu));
        total += u64(select(rq[r * RQ + 4u], rq[r * RQ + 6u], stage == 1u));
    }
    var granted_sum: u64 = u64(0u);
    for (var q = pos; q < end; q++) {
        let r = u32(list[q] & u64(0xffffffffu));
        let w = u64(select(rq[r * RQ + 4u], rq[r * RQ + 6u], stage == 1u));
        let scaled = w * u64(available);
        let g = scaled / total;
        scr[q] = scaled % total;      // the fractional part
        granted_sum += g;
        rq[r * RQ + select(6u, 7u, stage == 1u)] = u32(g);
    }
    // The leftover µ, one at a time, to the largest fractions; ties by the draw, then the position.
    var leftover = u64(available) - granted_sum;
    // scr_taken: mark by setting the fraction to all ones after use.
    while (leftover > u64(0u)) {
        var best = 0xffffffffu;
        var best_frac: u64 = u64(0u);
        var best_draw = 0u;
        for (var q = pos; q < end; q++) {
            let frac = scr[q];
            if (frac == U64_MAX()) { continue; }
            let d = draw(stream, domain, s, pas, q - pos);
            if (best == 0xffffffffu || frac > best_frac || (frac == best_frac && d < best_draw)) {
                best = q;
                best_frac = frac;
                best_draw = d;
            }
        }
        let r = u32(list[best] & u64(0xffffffffu));
        rq[r * RQ + select(6u, 7u, stage == 1u)] += 1u;
        scr[best] = U64_MAX();
        leftover -= u64(1u);
    }
}

// Stage 2 starts from the stage 1 grants.
@compute @workgroup_size(64)
fn init_stage2(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= params[P_RTOT] || rq[i * RQ + 4u] == 0u) { return; }
    rq[i * RQ + 7u] = rq[i * RQ + 6u];
}

// ---- commit ----
@compute @workgroup_size(64)
fn commit(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= params[P_RTOT]) { return; }
    let b = i * RQ;
    if (rq[b + 4u] == 0u) { return; }
    let s = rq[b + 2u];
    let delivered = rq[b + 7u];
    var overflowed = 0u;
    let ovf = rq[b + 3u];
    if (ovf != NONE && sinkacc[ovf * params[P_NS] + s] == 1u) { overflowed = rq[b + 6u] - rq[b + 7u]; }
    let debit = delivered + overflowed;
    let o = rq[b];
    let d = rq[b + 1u];
    if (tag_of(o) == 0u) { atomicSub(&camt[idx_of(o) * params[P_NS] + s], debit); }
    else if (tag_of(o) == 1u) { atomicSub(&amounts[idx_of(o) * params[P_NS] + s], debit); }
    if (tag_of(d) == 0u) { atomicAdd(&camt[idx_of(d) * params[P_NS] + s], delivered); }
    else if (tag_of(d) == 1u) { atomicAdd(&amounts[idx_of(d) * params[P_NS] + s], delivered); }
}

// ---- movement (2.4), one thread per entity ----
@compute @workgroup_size(64)
fn movement(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let n_genes = ent[e * ENT + 5u];
    let roff = ent[e * ENT + 8u];
    var pos = vec2<f32>(epos[e * 4u], epos[e * 4u + 1u]);
    var h = vec2<f32>(epos[e * 4u + 2u], epos[e * 4u + 3u]);
    for (var g = 0u; g < n_genes; g++) {
        let base = roff + g * REC;
        if (rec[base] == R_TURN && rec[base + 7u] == 1u) {
            let sc = sincos(bitcast<f32>(rec[base + 6u]) * bitcast<f32>(DEG_BITS));
            let nx = fm(h.x, sc.y, -(h.y * sc.x));
            let ny = fm(h.x, sc.x, h.y * sc.y);
            let q = fm(nx, nx, ny * ny);
            let t = fm(q, -0.5, 1.5);
            h = vec2<f32>(nx * t, ny * t);
        }
    }
    for (var g = 0u; g < n_genes; g++) {
        let base = roff + g * REC;
        if (rec[base] == R_MOVE && rec[base + 7u] == 1u) {
            let dist = bitcast<f32>(rec[base + 6u]);
            let wrap = params[P_WRAP] == 1u;
            let p = vec2<f32>(fm(h.x, dist, pos.x), fm(h.y, dist, pos.y));
            pos = vec2<f32>(fit(p.x, pf(P_W), wrap), fit(p.y, pf(P_H), wrap));
        }
    }
    epos[e * 4u] = pos.x;
    epos[e * 4u + 1u] = pos.y;
    epos[e * 4u + 2u] = h.x;
    epos[e * 4u + 3u] = h.y;
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
