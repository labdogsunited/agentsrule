// Sizes the grouping of the oversubscribed requests on the device, so the host need not wait for the
// count. One thread. `ind` holds three dispatch sizes (three words each): per entry (the histogram,
// the scatter and `share`), one per bucket (the bucket sort), and per request of an entity.

@group(0) @binding(0) var<storage, read_write> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> counters: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> ind: array<u32>;
@group(0) @binding(3) var<storage, read_write> rlist: array<atomic<u32>>;

const P_SBN = 56u;
const SB_MAXC = 32768u;     // one workgroup per bucket, and a dimension holds at most 65535

@compute @workgroup_size(1)
fn plan_buckets() {
    let m = atomicLoad(&counters[0]);
    let want = m / 32u;
    var buckets = 1u;
    if (want > 1u) { buckets = 1u << (32u - countLeadingZeros(want - 1u)); }   // the next power of two
    buckets = min(buckets, SB_MAXC);
    params[P_SBN] = buckets;
    ind[0] = (m + 63u) / 64u;
    ind[1] = 1u;
    ind[2] = 1u;
    ind[3] = buckets;
    ind[4] = 1u;
    ind[5] = 1u;
}

// After the requests of the entities are made: how many groups of 64 cover them.
@compute @workgroup_size(1)
fn plan_requests() {
    ind[6] = (atomicLoad(&rlist[0]) + 63u) / 64u;
    ind[7] = 1u;
    ind[8] = 1u;
}
