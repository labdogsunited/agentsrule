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
@group(0) @binding(7) var<storage, read_write> rlist: array<atomic<u32>>;   // word 0: the number of requests from entities, then their slots
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
@group(0) @binding(22) var<storage, read_write> flows: array<atomic<u32>>;
@group(0) @binding(23) var<storage, read_write> sb_in: array<atomic<u32>>;     // requests per bucket
@group(0) @binding(24) var<storage, read_write> sb_out: array<u32>;             // where each bucket starts
@group(0) @binding(25) var<storage, read_write> sb_blocks: array<u32>;
@group(0) @binding(26) var<storage, read_write> sb_cur: array<atomic<u32>>;   // emitted, absorbed, (audit) totals by substance, 64-bit as two words
@group(0) @binding(27) var<storage, read_write> wgr1: array<u32>;   // world request grants that differ from the request: stage 1 grant + 1, 0 if none
@group(0) @binding(28) var<storage, read_write> wgr2: array<u32>;   // the same for the final grant
@group(0) @binding(29) var<storage, read_write> camt0: array<u32>;   // the cell amounts when the requests were made (the commit changes camt)
const P_BECOMES = 55u;
fn flow_add(kind: u32, s: u32, x: u32) {
    if (x == 0u) { return; }
    let i = (kind * params[P_NS] + s) * 2u;
    let old = atomicAdd(&flows[i], x);
    if (old + x < old) { atomicAdd(&flows[i + 1u], 1u); }
}


const RQ = 8u;      // words per request: origin, dest, s, overflow, requested, steal, stage1, stage2
const LAW = 28u;    // words per law
const LAW_COND = 25u;   // where the law's condition program starts in `laws`, plus 1; 0 for none
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

// Stage 1 grants go to words 6 and 7 (a request that is not shared in stage 2 keeps it); stage 2 only to word 7.
// A world request (slot R0 and up) has no stored words: it is computed from the cells (`world_req`), and
// only a grant that differs from its request is kept, as grant + 1 (0 means none; a grant never
// exceeds the request, so a wrapped 0xffffffff + 1 reads back as the request itself).
fn set_grant(r: u32, stage: u32, v: u32) {
    let r0 = params[P_R0];
    if (r < r0) {
        if (stage == 0u) { rq[r * RQ + 6u] = v; }
        rq[r * RQ + 7u] = v;
        return;
    }
    if (stage == 0u) { wgr1[r - r0] = v + 1u; }
    wgr2[r - r0] = v + 1u;
}
fn get_grant(r: u32, stage: u32) -> u32 {
    let r0 = params[P_R0];
    if (r < r0) { return rq[r * RQ + 7u]; }
    let o = wgr2[r - r0];
    if (o != 0u) { return o - 1u; }
    return world_req(r).amt;
}

struct Req { origin: u32, dest: u32, s: u32, ovf: u32, amt: u32, steal: u32, g1: u32, g2: u32 }

// Whether law `lb` acts in cell c: it has no condition, or its condition holds on the cells as the tick began.
fn law_on(lb: u32, c: u32) -> bool {
    let p = laws[lb + LAW_COND];
    return p == 0u || cond_holds(p - 1u, c);
}

// The request of world slot i, computed from the laws and the cell amounts at the start of the phase (`camt0`). `amt` is 0 for an empty slot.
fn world_req(i: u32) -> Req {
    let ns = params[P_NS];
    for (var l = 0u; l < params[P_NLAWS]; l++) {
        let lb = l * LAW;
        let off = params[P_R0] + laws[lb + 7u];
        let w = laws[lb + 8u];
        let total = w * params[P_NCELLS];
        if (i < off || i >= off + total) { continue; }
        let c = (i - off) / w;
        let k = (i - off) % w;
        // A law whose condition fails in the cell makes no request there.
        if (!law_on(lb, c)) { break; }
        if (laws[lb] == 0u) {
            // Inflow from a source to the cell.
            let a = laws[lb + 3u];
            return Req((2u << TAG) | laws[lb + 2u], c, laws[lb + 1u], laws[lb + 4u], a, 0u, a, a);
        }
        if (laws[lb] == 2u) {
            // Shift: from the cell to its neighbour in one direction; none past the edge of a grid that does not wrap.
            let nb = c_nb(c, laws[lb + 5u]);
            if (nb == NONE) { break; }
            let a = laws[lb + 3u];
            return Req(c, nb, laws[lb + 1u], NONE, a, 0u, a, a);
        }
        // Diffusion: k = substance index * 4 + neighbour index.
        let si = k / 4u;
        let ni = k % 4u;
        let nb = nbr[c * 4u + ni];
        if (si >= laws[lb + 6u] || nb == NONE) { break; }
        var count = 0u;
        for (var q = 0u; q < 4u; q++) { if (nbr[c * 4u + q] != NONE) { count++; } }
        let s = laws[lb + 9u + si];
        let offer = u32((u64(camt0[c * ns + s]) * u64(laws[lb + 5u])) / u64(10000u));
        let a = offer / count;
        return Req(c, nb, s, NONE, a, 0u, a, a);
    }
    return Req(0u, 0u, 0u, NONE, 0u, 0u, 0u, 0u);
}

// Request i with its grants.
fn load_req(i: u32) -> Req {
    let r0 = params[P_R0];
    if (i < r0) {
        let b = i * RQ;
        if (rq[b + 4u] == 0u) { return Req(0u, 0u, 0u, NONE, 0u, 0u, 0u, 0u); }
        return Req(rq[b], rq[b + 1u], rq[b + 2u], rq[b + 3u], rq[b + 4u], rq[b + 5u], rq[b + 6u], rq[b + 7u]);
    }
    var q = world_req(i);
    if (q.amt != 0u) {
        let o1 = wgr1[i - r0];
        if (o1 != 0u) { q.g1 = o1 - 1u; }
        let o2 = wgr2[i - r0];
        if (o2 != 0u) { q.g2 = o2 - 1u; }
    }
    return q;
}

// The ledger of what sources emit and sinks absorb is summed per workgroup first: thousands of
// threads adding to the same few words would wait for one another.
var<workgroup> wf_lo: array<atomic<u32>, 64>;   // kind * 32 + substance
var<workgroup> wf_hi: array<atomic<u32>, 64>;
fn wflow_add(kind: u32, s: u32, x: u32) {
    if (x == 0u) { return; }
    let old = atomicAdd(&wf_lo[kind * 32u + s], x);
    if (old + x < old) { atomicAdd(&wf_hi[kind * 32u + s], 1u); }
}

// Adds the workgroup's sums to the ledger (thread t has the word for kind t / 32, substance t % 32).
fn wflow_flush(t: u32) {
    let lo = atomicLoad(&wf_lo[t]);
    let hi = atomicLoad(&wf_hi[t]);
    if (lo != 0u || hi != 0u) {
        let at = ((t / 32u) * params[P_NS] + (t % 32u)) * 2u;
        let old = atomicAdd(&flows[at], lo);
        atomicAdd(&flows[at + 1u], hi + select(0u, 1u, old + lo < old));
    }
}

// ---- A: upkeep, fees and guards, one thread per entity ----
@compute @workgroup_size(64)
fn upkeep_fees(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    atomicStore(&wf_lo[lid.x], 0u);
    atomicStore(&wf_hi[lid.x], 0u);
    workgroupBarrier();
    if (id.x < params[P_N]) { upkeep_one(id.x); }
    workgroupBarrier();
    wflow_flush(lid.x);
}

fn upkeep_one(e: u32) {
    let ns = params[P_NS];
    let pay = params[P_PAY];
    let n_genes = ent[e * ENT + 5u];
    let na = ent[e * ENT + 13u] & 0xffu;
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
        wflow_add(1u, params[P_BECOMES], u32(cost));
        acct_add(ent[e * ENT + 11u], AC_UPKEEP_MU, u32(cost));
    } else {
        dead = true;
    }
    dying[e] = select(0u, 1u, dead);
    // Fees, in genome order, for the fired actions; and the signals this entity guards, for the requests of others.
    var guarded = 0u;
    for (var g = 0u; g < na; g++) {
        let base = roff + g * REC;
        var live = !dead;
        let fee = rec[base + 4u];
        if (live && fee > 0u) {
            if (atomicLoad(&amounts[e * ns + pay]) >= fee) {
                atomicSub(&amounts[e * ns + pay], fee);
                wflow_add(1u, params[P_BECOMES], fee);
                acct_add(ent[e * ENT + 11u], AC_FEES_MU, fee);
            } else {
                live = false;
            }
        }
        rec[base + 7u] = select(0u, 1u, live);
        if (live && rec[base] == R_GUARD) { guarded = guarded | (1u << rec[base + 2u]); }
    }
    guards[e] = guarded;
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

// ---- B: requests, one thread per entity (the world's are computed where used) ----
// A request is stored at its slot, the entity's first record slot plus the record's place; the slots of the
// requests go into `rlist`, where the later kernels take them from. A workgroup reserves room for all of
// its requests at once.
var<workgroup> gen_total: atomic<u32>;
var<workgroup> gen_base: u32;

@compute @workgroup_size(64)
fn gen_requests(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let e = id.x;
    if (lid.x == 0u) { atomicStore(&gen_total, 0u); }
    workgroupBarrier();
    var mine: array<u32, 48>;
    var m = 0u;
    if (e < params[P_N]) {
        let na = ent[e * ENT + 13u] & 0xffu;
        let roff = ent[e * ENT + 8u];
        let cell = ent[e * ENT + 2u];
        let me = (1u << TAG) | e;
        for (var g = 0u; g < na; g++) {
            let base = roff + g * REC;
            let code = rec[base];
            if (rec[base + 7u] == 0u) { continue; }
            let s = rec[base + 2u];
            let amount = rec[base + 3u];
            let other = rec[base + 5u];
            let i = roff / REC + g;
            var made = false;
            if (code == R_TAKE) { set_rq(i, cell, me, s, NONE, amount, 0u); made = true; }
            else if (code == R_DROP) { set_rq(i, me, cell, s, NONE, amount, 0u); made = true; }
            else if (code == R_GIVE) { if (dying[other] == 0u) { set_rq(i, me, (1u << TAG) | other, s, NONE, amount, 0u); made = true; } }
            else if (code == R_STEAL) {
                if ((guards[other] & (1u << s)) == 0u && dying[other] == 0u) { set_rq(i, (1u << TAG) | other, me, s, NONE, amount, 1u); made = true; }
            }
            if (made && amount != 0u) { mine[m] = i; m++; }
        }
    }
    let at = atomicAdd(&gen_total, m);
    workgroupBarrier();
    if (lid.x == 0u) { gen_base = atomicAdd(&rlist[0], atomicLoad(&gen_total)); }
    workgroupBarrier();
    for (var k = 0u; k < m; k++) { atomicStore(&rlist[1u + gen_base + at + k], mine[k]); }
}

// For tests that read the requests back: writes the world requests into `rq`, before the commit.
@compute @workgroup_size(64)
fn materialize_world(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i < params[P_R0] || i >= params[P_RTOT]) { return; }
    let q = load_req(i);
    let b = i * RQ;
    rq[b] = q.origin;
    rq[b + 1u] = q.dest;
    rq[b + 2u] = q.s;
    rq[b + 3u] = q.ovf;
    rq[b + 4u] = q.amt;
    rq[b + 5u] = q.steal;
    rq[b + 6u] = q.g1;
    rq[b + 7u] = q.g2;
}

// ---- C: stage sums and the oversubscribed list ----
fn add_sum(k: u32, x: u32) {
    let old = atomicAdd(&sum_lo[k], x);
    if (old + x < old) { atomicAdd(&sum_hi[k], 1u); }
}

// The sums, flags and commit below are for the entity requests (slots below R0). The world's requests are
// handled in bundles by the `_world` kernels further down.
// stage 1 sums use the origin and the requested amount; stage 2 sums use the destination and the stage 1 grant
@compute @workgroup_size(64)
fn sums(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= atomicLoad(&rlist[0])) { return; }
    let i = atomicLoad(&rlist[1u + id.x]);
    let stage = params[P_STAGE]; // 0 or 1, set by the host
    let q = load_req(i);
    if (q.amt == 0u) { return; }
    let k = store_key(select(q.origin, q.dest, stage == 1u), q.s);
    if (k == NONE) { return; }
    add_sum(k, select(q.amt, q.g1, stage == 1u));
}

@compute @workgroup_size(64)
fn flag_over(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= atomicLoad(&rlist[0])) { return; }
    let i = atomicLoad(&rlist[1u + id.x]);
    let stage = params[P_STAGE];
    let q = load_req(i);
    if (q.amt == 0u) { return; }
    let p = select(q.origin, q.dest, stage == 1u);
    let k = store_key(p, q.s);
    if (k == NONE) { return; }
    let total = (u64(atomicLoad(&sum_hi[k])) << 32u) | u64(atomicLoad(&sum_lo[k]));
    let limit = select(balance(p, q.s), headroom(p, q.s), stage == 1u);
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
    let fq = load_req(first);
    let p = select(fq.origin, fq.dest, stage == 1u);
    let s = fq.s;
    let available = select(balance(p, s), headroom(p, s), stage == 1u);
    if (size == 1u) {
        set_grant(first, stage, available);
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
        let rr = load_req(r);
        total += u64(select(rr.amt, rr.g1, stage == 1u));
    }
    var granted_sum: u64 = u64(0u);
    for (var q = pos; q < end; q++) {
        let r = u32(list[q] & u64(0xffffffffu));
        let rr = load_req(r);
        let w = u64(select(rr.amt, rr.g1, stage == 1u));
        let scaled = w * u64(available);
        let g = scaled / total;
        scr[q] = scaled % total;      // the fractional part
        granted_sum += g;
        set_grant(r, stage, u32(g));
    }
    // The leftover µ, one each, to the `leftover` largest fractions; ties by the draw, then the position
    // (the order `arbitrate::share` sorts by on the CPU). Rather than picking the best one at a time,
    // which costs a draw per member per µ, the draws are made once (in the upper half of `scr`), and the
    // cut is found by bisection: first the smallest fraction that gets one, then among the members with
    // that fraction the largest draw that does. About 100 passes over the group, whatever its size.
    var k = u64(available) - granted_sum;
    if (k == u64(0u)) { return; }
    let half = arrayLength(&scr) / 2u;
    var top_frac = u64(0u);
    for (var q = pos; q < end; q++) {
        scr[half + q] = u64(draw(stream, domain, s, pas, q - pos));
        top_frac = max(top_frac, scr[q]);
    }
    // The largest t with at least k members whose fraction is t or more.
    var lo = u64(0u);
    var hi = top_frac;
    while (lo < hi) {
        let mid = lo + (hi - lo + u64(1u)) / u64(2u);
        var n = u64(0u);
        for (var q = pos; q < end; q++) { if (scr[q] >= mid) { n += u64(1u); } }
        if (n >= k) { lo = mid; } else { hi = mid - u64(1u); }
    }
    let t = lo;
    var above = u64(0u);
    var top_draw = u64(0u);
    for (var q = pos; q < end; q++) {
        if (scr[q] > t) { above += u64(1u); }
        if (scr[q] == t) { top_draw = max(top_draw, scr[half + q]); }
    }
    // Among the members at fraction t, k - above get one: the smallest draw d with that many at or below it.
    let want = k - above;
    var dlo = u64(0u);
    var dhi = top_draw;
    while (dlo < dhi) {
        let mid = dlo + (dhi - dlo) / u64(2u);
        var n = u64(0u);
        for (var q = pos; q < end; q++) { if (scr[q] == t && scr[half + q] <= mid) { n += u64(1u); } }
        if (n >= want) { dhi = mid; } else { dlo = mid + u64(1u); }
    }
    let d = dlo;
    var below = u64(0u);
    for (var q = pos; q < end; q++) { if (scr[q] == t && scr[half + q] < d) { below += u64(1u); } }
    // Members at (t, d) take the rest in list order.
    var rest = want - below;
    for (var q = pos; q < end; q++) {
        var gets = scr[q] > t || (scr[q] == t && scr[half + q] < d);
        if (!gets && scr[q] == t && scr[half + q] == d && rest > u64(0u)) {
            gets = true;
            rest -= u64(1u);
        }
        if (gets) {
            let r = u32(list[q] & u64(0xffffffffu));
            set_grant(r, stage, get_grant(r, stage) + 1u);
        }
    }
}

// Stage 2 starts from the stage 1 grants.
@compute @workgroup_size(64)
fn init_stage2(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= atomicLoad(&rlist[0])) { return; }
    let i = atomicLoad(&rlist[1u + id.x]);
    if (rq[i * RQ + 4u] == 0u) { return; }
    rq[i * RQ + 7u] = rq[i * RQ + 6u];
}

// One request into the transfer recording, ends as packed tags: a creature is named by its ID.
fn trace_req(law: u32, o: u32, d: u32, s: u32, steal: u32, req: u32, delivered: u32, overflowed: u32) {
    if (params[P_TRACE] == 0u) { return; }
    var olo = idx_of(o);
    var ohi = 0u;
    if (tag_of(o) == 1u) { olo = ent[idx_of(o) * ENT]; ohi = ent[idx_of(o) * ENT + 1u]; }
    var dlo = idx_of(d);
    var dhi = 0u;
    if (tag_of(d) == 1u) { dlo = ent[idx_of(d) * ENT]; dhi = ent[idx_of(d) * ENT + 1u]; }
    trace_put(law, tag_of(o), olo, ohi, tag_of(d), dlo, dhi, s, steal, req, delivered, overflowed);
}

// ---- commit ----
@compute @workgroup_size(64)
fn commit(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    atomicStore(&wf_lo[lid.x], 0u);
    atomicStore(&wf_hi[lid.x], 0u);
    workgroupBarrier();
    if (id.x < atomicLoad(&rlist[0])) {
        let q = load_req(atomicLoad(&rlist[1u + id.x]));
        if (q.amt != 0u) {
            let s = q.s;
            let delivered = q.g2;
            var overflowed = 0u;
            if (q.ovf != NONE && sinkacc[q.ovf * params[P_NS] + s] == 1u) { overflowed = q.g1 - q.g2; }
            let debit = delivered + overflowed;
            let o = q.origin;
            let d = q.dest;
            trace_req(NOLAW, o, d, s, q.steal, q.amt, delivered, overflowed);
            if (tag_of(o) == 0u) { atomicSub(&camt[idx_of(o) * params[P_NS] + s], debit); }
            else if (tag_of(o) == 1u) { atomicSub(&amounts[idx_of(o) * params[P_NS] + s], debit); }
            if (tag_of(d) == 0u) { atomicAdd(&camt[idx_of(d) * params[P_NS] + s], delivered); }
            else if (tag_of(d) == 1u) { atomicAdd(&amounts[idx_of(d) * params[P_NS] + s], delivered); }
            if (tag_of(o) == 2u) { wflow_add(0u, s, debit); }
            if (tag_of(d) == 3u) { wflow_add(1u, s, delivered); }
            wflow_add(1u, s, overflowed);
            if (params[P_ACC] != 0u && delivered > 0u && s == params[P_PAYSUB]) {
                let ot = tag_of(o);
                let dt = tag_of(d);
                if (ot == 0u && dt == 1u) {
                    acct_add(ent[idx_of(d) * ENT + 11u], AC_PAY_TAKEN_MU, delivered);
                } else if (ot == 1u && dt == 1u) {
                    let gi = ent[idx_of(d) * ENT + 11u];
                    let go = ent[idx_of(o) * ENT + 11u];
                    if (q.steal != 0u) {
                        acct_add(gi, AC_PAY_STOLEN_IN_MU, delivered);
                        acct_add(go, AC_PAY_STOLEN_OUT_MU, delivered);
                    } else {
                        acct_add(gi, AC_PAY_GIFT_IN_MU, delivered);
                        acct_add(go, AC_PAY_GIFT_OUT_MU, delivered);
                    }
                } else if (ot == 1u && (dt == 0u || dt == 3u)) {
                    acct_add(ent[idx_of(o) * ENT + 11u], AC_PAY_DROPPED_MU, debit);
                }
            }
        }
    }
    workgroupBarrier();
    wflow_flush(lid.x);
}

// ---- the world's requests, in bundles: a cell's inflow (one request) or a cell's diffusion of one
// substance (up to four, to the neighbours). The grid is (bundles, laws); a thread of law y takes bundle
// x. Per bundle this does what the per-request kernels do for its requests, with fewer atomics.
struct Bundle { law: u32, c: u32, si: u32, ok: bool }

fn bundle_of(id: vec3<u32>) -> Bundle {
    let lb = id.y * LAW;
    let nsub = select(1u, laws[lb + 6u], laws[lb] == 1u);
    let total = params[P_NCELLS] * nsub;
    if (id.y >= params[P_NLAWS] || id.x >= total) { return Bundle(0u, 0u, 0u, false); }
    return Bundle(id.y, id.x / nsub, id.x % nsub, true);
}

// A diffusion bundle: the substance, the neighbours that exist, and the amount asked of each.
struct Diff { s: u32, count: u32, a: u32 }
fn diff_of(lb: u32, c: u32, si: u32) -> Diff {
    var count = 0u;
    for (var q = 0u; q < 4u; q++) { if (nbr[c * 4u + q] != NONE) { count++; } }
    let s = laws[lb + 9u + si];
    if (count == 0u || !law_on(lb, c)) { return Diff(s, 0u, 0u); }
    let offer = u32((u64(camt0[c * params[P_NS] + s]) * u64(laws[lb + 5u])) / u64(10000u));
    return Diff(s, count, offer / count);
}

// A shift law: the substance, the neighbour it sends to, and the amount asked (0 if the condition fails or there is no neighbour).
struct Shift { s: u32, nb: u32, a: u32 }
fn shift_of(lb: u32, c: u32) -> Shift {
    let s = laws[lb + 1u];
    let nb = c_nb(c, laws[lb + 5u]);
    if (nb == NONE || !law_on(lb, c)) { return Shift(s, nb, 0u); }
    return Shift(s, nb, laws[lb + 3u]);
}
fn shift_slot(lb: u32, c: u32) -> u32 { return params[P_R0] + laws[lb + 7u] + c; }

// The slot of the request to neighbour ni.
fn diff_slot(lb: u32, c: u32, si: u32, ni: u32) -> u32 { return params[P_R0] + laws[lb + 7u] + c * laws[lb + 8u] + si * 4u + ni; }

fn slot_g1(j: u32, a: u32) -> u32 {
    let o = wgr1[j - params[P_R0]];
    return select(a, o - 1u, o != 0u);
}
fn slot_g2(j: u32, a: u32) -> u32 {
    let o = wgr2[j - params[P_R0]];
    return select(a, o - 1u, o != 0u);
}

@compute @workgroup_size(64)
fn sums_world(@builtin(global_invocation_id) id: vec3<u32>) {
    let b = bundle_of(id);
    if (!b.ok) { return; }
    let lb = b.law * LAW;
    let stage = params[P_STAGE];
    let ns = params[P_NS];
    if (laws[lb] == 0u) {
        // Inflow: the source is no store, so only the destination sums (stage 2) count; its grant is the request.
        let amt = select(0u, laws[lb + 3u], law_on(lb, b.c));
        if (stage == 1u && amt != 0u) { add_sum(b.c * ns + laws[lb + 1u], amt); }
        return;
    }
    if (laws[lb] == 2u) {
        let sh = shift_of(lb, b.c);
        if (sh.a == 0u) { return; }
        if (stage == 0u) { add_sum(b.c * ns + sh.s, sh.a); } else { add_sum(sh.nb * ns + sh.s, slot_g1(shift_slot(lb, b.c), sh.a)); }
        return;
    }
    let d = diff_of(lb, b.c, b.si);
    if (d.a == 0u) { return; }
    if (stage == 0u) {
        add_sum(b.c * ns + d.s, d.count * d.a);
    } else {
        for (var ni = 0u; ni < 4u; ni++) {
            let nb = nbr[b.c * 4u + ni];
            if (nb != NONE) { add_sum(nb * ns + d.s, slot_g1(diff_slot(lb, b.c, b.si, ni), d.a)); }
        }
    }
}

fn list_add(key: u32, j: u32) {
    let slot = atomicAdd(&counters[0], 1u);
    list[slot] = (u64(key) << 32u) | u64(j);
}
fn sum_of(k: u32) -> u64 { return (u64(atomicLoad(&sum_hi[k])) << 32u) | u64(atomicLoad(&sum_lo[k])); }

@compute @workgroup_size(64)
fn flag_world(@builtin(global_invocation_id) id: vec3<u32>) {
    let b = bundle_of(id);
    if (!b.ok) { return; }
    let lb = b.law * LAW;
    let stage = params[P_STAGE];
    let ns = params[P_NS];
    if (laws[lb] == 0u) {
        let amt = select(0u, laws[lb + 3u], law_on(lb, b.c));
        if (stage == 1u && amt != 0u) {
            let s = laws[lb + 1u];
            let k = b.c * ns + s;
            if (sum_of(k) > u64(headroom(b.c, s))) { list_add(k, params[P_R0] + laws[lb + 7u] + b.c); }
        }
        return;
    }
    if (laws[lb] == 2u) {
        let sh = shift_of(lb, b.c);
        if (sh.a == 0u) { return; }
        let k = select(sh.nb, b.c, stage == 0u) * ns + sh.s;
        let over = select(headroom(sh.nb, sh.s), balance(b.c, sh.s), stage == 0u);
        if (sum_of(k) > u64(over)) { list_add(k, shift_slot(lb, b.c)); }
        return;
    }
    let d = diff_of(lb, b.c, b.si);
    if (d.a == 0u) { return; }
    if (stage == 0u) {
        let k = b.c * ns + d.s;
        if (sum_of(k) > u64(balance(b.c, d.s))) {
            for (var ni = 0u; ni < 4u; ni++) {
                if (nbr[b.c * 4u + ni] != NONE) { list_add(k, diff_slot(lb, b.c, b.si, ni)); }
            }
        }
    } else {
        for (var ni = 0u; ni < 4u; ni++) {
            let nb = nbr[b.c * 4u + ni];
            if (nb == NONE) { continue; }
            let k = nb * ns + d.s;
            if (sum_of(k) > u64(headroom(nb, d.s))) { list_add(k, diff_slot(lb, b.c, b.si, ni)); }
        }
    }
}

@compute @workgroup_size(64)
fn commit_world(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {
    atomicStore(&wf_lo[lid.x], 0u);
    atomicStore(&wf_hi[lid.x], 0u);
    lsw_clear(lid.x);
    workgroupBarrier();
    let b = bundle_of(id);
    if (b.ok) {
        let lb = b.law * LAW;
        let ns = params[P_NS];
        if (b.si == 0u && law_on(lb, b.c)) { lsw_add(LS_PASSED, 1u); }
        if (laws[lb] == 0u) {
            let amt = select(0u, laws[lb + 3u], law_on(lb, b.c));
            if (amt != 0u) {
                let s = laws[lb + 1u];
                let j = params[P_R0] + laws[lb + 7u] + b.c;
                let g1 = slot_g1(j, amt);
                let delivered = slot_g2(j, amt);
                let ovf = laws[lb + 4u];
                var overflowed = 0u;
                if (ovf != NONE && sinkacc[ovf * ns + s] == 1u) { overflowed = g1 - delivered; }
                ls_req(amt, g1, delivered);
                trace_req(b.law, (2u << TAG) | laws[lb + 2u], b.c, s, 0u, amt, delivered, overflowed);
                atomicAdd(&camt[b.c * ns + s], delivered);
                wflow_add(0u, s, delivered + overflowed);
                wflow_add(1u, s, overflowed);
            }
        } else if (laws[lb] == 2u) {
            let sh = shift_of(lb, b.c);
            if (sh.a != 0u) {
                let g2 = slot_g2(shift_slot(lb, b.c), sh.a);
                ls_req(sh.a, slot_g1(shift_slot(lb, b.c), sh.a), g2);
                trace_req(b.law, b.c, sh.nb, sh.s, 0u, sh.a, g2, 0u);
                if (g2 != 0u) {
                    atomicAdd(&camt[sh.nb * ns + sh.s], g2);
                    atomicSub(&camt[b.c * ns + sh.s], g2);
                }
            }
        } else {
            let d = diff_of(lb, b.c, b.si);
            if (d.a != 0u) {
                var out = 0u;
                for (var ni = 0u; ni < 4u; ni++) {
                    let nb = nbr[b.c * 4u + ni];
                    if (nb == NONE) { continue; }
                    let g2 = slot_g2(diff_slot(lb, b.c, b.si, ni), d.a);
                    ls_req(d.a, slot_g1(diff_slot(lb, b.c, b.si, ni), d.a), g2);
                    trace_req(b.law, b.c, nb, d.s, 0u, d.a, g2, 0u);
                    out += g2;
                    if (g2 != 0u) { atomicAdd(&camt[nb * ns + d.s], g2); }
                }
                if (out != 0u) { atomicSub(&camt[b.c * ns + d.s], out); }
            }
        }
    }
    workgroupBarrier();
    wflow_flush(lid.x);
    lsw_flush(lid.x, wid.y);
}

// ---- movement (2.4), one thread per entity ----
@compute @workgroup_size(64)
fn movement(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let na = ent[e * ENT + 13u] & 0xffu;
    let roff = ent[e * ENT + 8u];
    var pos = vec2<f32>(epos[e * 4u], epos[e * 4u + 1u]);
    var h = vec2<f32>(epos[e * 4u + 2u], epos[e * 4u + 3u]);
    for (var g = 0u; g < na; g++) {
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
    for (var g = 0u; g < na; g++) {
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

// ---- grouping the oversubscribed requests: a bucket sort by hashed key, then one workgroup per bucket ----
// The list holds (key << 32 | request) entries in no order. They are spread into buckets by a hash of
// the key (equal keys land together), and each bucket is sorted in shared memory, so every group is
// contiguous and in request order, which is all `share` needs.
const P_SBN = 56u;
var<workgroup> btile: array<u64, 256>;

fn bucket_of(key: u32) -> u32 {
    var h = key * 2654435761u;
    h = h ^ (h >> 15u);
    h = h * 2246822519u;
    h = h ^ (h >> 13u);
    return h & (params[P_SBN] - 1u);
}

@compute @workgroup_size(64)
fn bucket_hist(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= atomicLoad(&counters[0])) { return; }
    atomicAdd(&sb_in[bucket_of(u32(list[i] >> 32u))], 1u);
}

@compute @workgroup_size(64)
fn bucket_scatter(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= atomicLoad(&counters[0])) { return; }
    let e = list[i];
    let b = bucket_of(u32(e >> 32u));
    scr[sb_out[b] + atomicAdd(&sb_cur[b], 1u)] = e;
}

@compute @workgroup_size(256)
fn bucket_sort(@builtin(workgroup_id) wid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let b = wid.x;
    if (b >= params[P_SBN]) { return; }
    let off = sb_out[b];
    let cnt = atomicLoad(&sb_in[b]);
    if (cnt > 256u) {
        bucket_sort_big(off, cnt, lid.x);
        return;
    }
    var v = U64_MAX();
    if (lid.x < cnt) { v = scr[off + lid.x]; }
    btile[lid.x] = v;
    workgroupBarrier();
    // Only the first `kmax` places can hold an entry, so the network need not be wider.
    var kmax = 2u;
    while (kmax < cnt) { kmax = kmax * 2u; }
    for (var k = 2u; k <= kmax; k = k * 2u) {
        for (var j = k / 2u; j > 0u; j = j / 2u) {
            let i = lid.x;
            let l = i ^ j;
            if (l > i) {
                let a = btile[i];
                let c = btile[l];
                let up = (i & k) == 0u;
                if ((up && a > c) || (!up && a < c)) {
                    btile[i] = c;
                    btile[l] = a;
                }
            }
            workgroupBarrier();
        }
    }
    if (lid.x < cnt) { list[off + lid.x] = btile[lid.x]; }
}

// A bucket larger than the tile (one store asked by many, or keys that share a bucket): the same
// workgroup sorts it where it lies in `scr`, then copies it to `list`. Slow, but rare and correct.
// Every step compares upwards (the first step of each merge pairs i with the mirror place), so the
// places past `cnt`, which count as the largest key, never move and are never touched.
fn bucket_sort_big(off: u32, cnt: u32, t: u32) {
    var size = 2u;
    while (size < cnt) { size = size * 2u; }
    let half = size / 2u;
    for (var k = 2u; k <= size; k = k * 2u) {
        for (var p = t; p < half; p += 256u) {
            let w = p % (k / 2u);
            let i = (p / (k / 2u)) * k + w;
            big_exchange(off, cnt, i, i + k - 1u - 2u * w);
        }
        storageBarrier();
        for (var j = k / 4u; j > 0u; j = j / 2u) {
            for (var p = t; p < half; p += 256u) {
                let i = (p / j) * 2u * j + p % j;
                big_exchange(off, cnt, i, i + j);
            }
            storageBarrier();
        }
    }
    for (var i = t; i < cnt; i += 256u) { list[off + i] = scr[off + i]; }
}

fn big_exchange(off: u32, cnt: u32, i: u32, l: u32) {
    if (l >= cnt) { return; }
    let a = scr[off + i];
    let c = scr[off + l];
    if (a > c) {
        scr[off + i] = c;
        scr[off + l] = a;
    }
}

// The starts of the buckets: an exclusive scan of the counts by one workgroup (each thread scans a slice).
var<workgroup> sbt: array<u32, 1024>;
@compute @workgroup_size(1024)
fn sb_scan(@builtin(local_invocation_id) lid: vec3<u32>) {
    let nb = params[P_SBN];
    let per = (nb + 1023u) / 1024u;
    let first = min(lid.x * per, nb);
    let last = min(first + per, nb);
    var run = 0u;
    for (var b = first; b < last; b++) { run += atomicLoad(&sb_in[b]); }
    sbt[lid.x] = run;
    workgroupBarrier();
    for (var off = 1u; off < 1024u; off = off * 2u) {
        var t = 0u;
        if (lid.x >= off) { t = sbt[lid.x - off]; }
        workgroupBarrier();
        sbt[lid.x] = sbt[lid.x] + t;
        workgroupBarrier();
    }
    var base = sbt[lid.x] - run;
    for (var b = first; b < last; b++) {
        sb_out[b] = base;
        base += atomicLoad(&sb_in[b]);
    }
}
