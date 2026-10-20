// Children's genomes at division (CONSTANTS, CONTRACT and mutate.wgsl come first): each new child
// of a lineage with variation gets a mutated copy of its parent's master genome, allocated in the
// pools, with its view compiled; an unchanged copy shares the parent's genome.

@group(0) @binding(4) var<storage, read_write> ent: array<u32>;
@group(0) @binding(5) var<storage, read_write> ent2: array<u32>;
@group(0) @binding(6) var<storage, read_write> gwp: array<u32>;
@group(0) @binding(7) var<storage, read_write> pool: array<atomic<u32>>;
@group(0) @binding(8) var<storage, read_write> work: array<u32>;

const P_NALIVE = 46u;
const P_NEXTID_LO = 42u;
const P_NEXTID_HI = 43u;
const P_NCHILD = 54u;
const WORD_LINEAGE = 11u;
const WORD_MASTER = 12u;

fn same_as_parent(moff: u32) -> bool {
    if (ns != mw[moff] || nm != mw[moff + 1u] || nk != mw[moff + 2u]) { return false; }
    // No early exit inside the loops: the loads then overlap instead of waiting for one another.
    var diff = 0u;
    for (var m = 0u; m < nm; m++) { diff |= mx[m] ^ mw[moff + 4u + m]; }
    for (var k = 0u; k < nk; k++) { diff |= bitcast<u32>(kp[k]) ^ mw[moff + MASTER_HDR + k]; }
    let sb = moff + MASTER_HDR + nk;
    for (var q = 0u; q < ns * SLOT; q++) { diff |= sl[q] ^ mw[sb + q]; }
    return diff == 0u;
}

fn is_active(i: u32) -> bool { return gw_(i, 17u) == 1u && mx[gw_(i, 16u)] == 1u; }

// One child per workgroup of one thread: the children run in separate warps, so their different
// paths do not take turns.
@compute @workgroup_size(1)
fn mutate_children(@builtin(global_invocation_id) id: vec3<u32>) {
    let r = id.y * 32768u + id.x;     // the grid is rows of 32768 children
    if (r >= params[P_NCHILD]) { return; }
    let e = work[r];
    let lineage = ent[e * ENT + WORD_LINEAGE];
    if (rates[lineage * RATES + 15u] != 1u) { return; }
    let moff = ent[e * ENT + WORD_MASTER];
    load_master(moff);
    let child_id = ((u64(params[P_NEXTID_HI]) << 32u) | u64(params[P_NEXTID_LO])) + u64(r);
    dstream = u32(child_id & u64(0xffffffffu));
    dtick = params[P_TICK];
    dn = 0u;
    blk_at = 0xffffffffu;
    mutate_core(lineage);
    if (same_as_parent(moff)) { return; }

    // The master genome.
    let new_moff = atomicAdd(&pool[0], MASTER_HDR + nk + ns * SLOT);
    mw[new_moff] = ns;
    mw[new_moff + 1u] = nm;
    mw[new_moff + 2u] = nk;
    mw[new_moff + 3u] = 0u;
    for (var m = 0u; m < MAXMODS; m++) { mw[new_moff + 4u + m] = select(0u, mx[m], m < nm); }
    for (var k = 0u; k < nk; k++) { mw[new_moff + MASTER_HDR + k] = bitcast<u32>(kp[k]); }
    for (var q = 0u; q < ns * SLOT; q++) { mw[new_moff + MASTER_HDR + nk + q] = sl[q]; }

    // The view: the keep values and the active genes, and what is derived from them.
    var n_active = 0u;
    var needs_near = false;
    var delta = false;
    var written: array<u32, 32>;
    var read_: array<u32, 32>;
    for (var i = 0u; i < ns; i++) {
        let kind = gw_(i, 0u);
        if (kind == SENSE_DELTA) { delta = true; }
        if (!is_active(i)) { continue; }
        n_active++;
        if (kind == SENSE_NEIGHBOUR || kind == GIVE || kind == STEAL) { needs_near = true; }
        if (kind <= SENSE_DELTA) {
            written[gw_(i, 2u)] = 1u;
            if (gw_(i, 3u) != NONE) { written[gw_(i, 3u)] = 1u; }
        } else if (kind == WEIGH) {
            written[gw_(i, 2u)] = 1u;
            for (var k = 0u; k < gw_(i, 6u); k++) { read_[gw_(i, 9u + 2u * k)] = 1u; }
        } else if (kind == PRODUCT) {
            written[gw_(i, 2u)] = 1u;
            read_[gw_(i, 8u)] = 1u;
            read_[gw_(i, 9u)] = 1u;
        } else if (gw_(i, 2u) != NONE) {
            read_[gw_(i, 2u)] = 1u;
        }
    }
    var in_use = 0u;
    for (var s = 0u; s < nk; s++) { if (written[s] == 1u && read_[s] == 1u) { in_use = in_use | (1u << s); } }
    let new_goff = atomicAdd(&pool[1], nk + n_active * SLOT);
    for (var k = 0u; k < nk; k++) { gwp[new_goff + k] = bitcast<u32>(kp[k]); }
    var at = new_goff + nk;
    for (var i = 0u; i < ns; i++) {
        if (!is_active(i)) { continue; }
        for (var q = 0u; q < SLOT; q++) { gwp[at + q] = gw_(i, q); }
        gwp[at + 1u] = i;
        gwp[at + 16u] = 0u;
        gwp[at + 17u] = 0u;
        at = at + SLOT;
    }
    let c = (params[P_NALIVE] + r) * ENT;
    ent2[c + 4u] = new_goff;
    ent2[c + 5u] = n_active;
    ent2[c + 6u] = nk;
    ent2[c + 7u] = select(0u, 1u, needs_near) | (select(0u, 1u, delta) << 2u);
    ent2[c + 9u] = in_use;
    ent2[c + WORD_MASTER] = new_moff;
}
