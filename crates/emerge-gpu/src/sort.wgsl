
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
