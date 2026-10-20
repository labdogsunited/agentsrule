const OUTW = 1100u;
const P_JOBS = 53u;
@group(0) @binding(4) var<storage, read_write> jobs: array<u32>;
@group(0) @binding(5) var<storage, read_write> out: array<u32>;

// The batch test: jobs = [parent master offset, lineage, child id low, tick], results in `out`.
@compute @workgroup_size(64)
fn mutate_batch(@builtin(global_invocation_id) id: vec3<u32>) {
    let j = id.x;
    if (j >= params[P_JOBS]) { return; }
    load_master(jobs[j * 4u]);
    dstream = jobs[j * 4u + 2u];
    dtick = jobs[j * 4u + 3u];
    dn = 0u;
    mutate_core(jobs[j * 4u + 1u]);
    let o = j * OUTW;
    out[o] = ns;
    out[o + 1u] = nm;
    out[o + 2u] = nk;
    out[o + 3u] = dn;   // draws consumed, for the comparison
    for (var q = 0u; q < 48u; q++) { out[o + 1000u + q] = trace[q]; }
    for (var q = 0u; q < 8u; q++) { out[o + 1050u + q] = ph[q]; }
    for (var m = 0u; m < MAXMODS; m++) { out[o + 4u + m] = select(0u, mx[m], m < nm); }
    for (var k = 0u; k < nk; k++) { out[o + MASTER_HDR + k] = bitcast<u32>(kp[k]); }
    for (var q = 0u; q < ns * SLOT; q++) { out[o + MASTER_HDR + nk + q] = sl[q]; }
}
