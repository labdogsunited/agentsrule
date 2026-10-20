//! Exclusive prefix sums as generated WGSL kernels.

/// Kernels `{name}_local`, `{name}_scan_blocks`, `{name}_add` that scan `{name}_in` into
/// `{name}_out` (length `len`, an expression), with `{name}_blocks` for the block sums.
/// `load` is the expression that reads input element `i` (`{name}_in[i]`, or `atomicLoad(&{name}_in[i])`).
pub fn kernels(name: &str, len: &str, load: &str) -> String {
    let load = load.replace("{name}", name);
    format!(
        r#"
var<workgroup> tile_{name}: array<u32, 256>;
@compute @workgroup_size(256)
fn {name}_local(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {{
    let i = gid.x;
    var v = 0u;
    if (i < {len}) {{ v = {load}; }}
    tile_{name}[lid.x] = v;
    workgroupBarrier();
    for (var off = 1u; off < 256u; off = off * 2u) {{
        var t = 0u;
        if (lid.x >= off) {{ t = tile_{name}[lid.x - off]; }}
        workgroupBarrier();
        tile_{name}[lid.x] = tile_{name}[lid.x] + t;
        workgroupBarrier();
    }}
    if (i < {len}) {{ {name}_out[i] = tile_{name}[lid.x] - v; }}
    if (lid.x == 255u) {{ {name}_blocks[wid.x] = tile_{name}[255u]; }}
}}
var<workgroup> btot_{name}: array<u32, 256>;
// The block sums, scanned by one workgroup: each thread scans a slice of them, then the slices' totals.
@compute @workgroup_size(256)
fn {name}_scan_blocks(@builtin(local_invocation_id) lid: vec3<u32>) {{
    let nb = ({len} + 255u) / 256u;
    let per = (nb + 255u) / 256u;
    let first = min(lid.x * per, nb);
    let last = min(first + per, nb);
    var run = 0u;
    for (var b = first; b < last; b++) {{ run += {name}_blocks[b]; }}
    btot_{name}[lid.x] = run;
    workgroupBarrier();
    for (var off = 1u; off < 256u; off = off * 2u) {{
        var t = 0u;
        if (lid.x >= off) {{ t = btot_{name}[lid.x - off]; }}
        workgroupBarrier();
        btot_{name}[lid.x] = btot_{name}[lid.x] + t;
        workgroupBarrier();
    }}
    var base = btot_{name}[lid.x] - run;
    for (var b = first; b < last; b++) {{
        let t = {name}_blocks[b];
        {name}_blocks[b] = base;
        base += t;
    }}
}}
@compute @workgroup_size(256)
fn {name}_add(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {{
    if (gid.x < {len}) {{ {name}_out[gid.x] += {name}_blocks[wid.x]; }}
}}
"#
    )
}
