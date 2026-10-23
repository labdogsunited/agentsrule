// Birth and death log for ancestry. Side output only: it reads the restructure buffers and changes none of them.
// A record is 6 words: kind (0 born, 1 starved, 2 aged), tick, id (lo, hi), parent id (lo, hi) for a birth.
const P_NEXTID_LO = 42u;
const P_NEXTID_HI = 43u;
@group(0) @binding(0) var<storage, read_write> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> ent: array<u32>;
@group(0) @binding(2) var<storage, read_write> work: array<u32>;
@group(0) @binding(3) var<storage, read_write> list: array<u64>;
@group(0) @binding(4) var<storage, read_write> dying: array<u32>;
@group(0) @binding(5) var<storage, read_write> evlog: array<u32>;
@group(0) @binding(6) var<storage, read_write> evn: array<atomic<u32>>;   // [records asked for, tick, capacity, removed, children]

fn put(kind: u32, id_lo: u32, id_hi: u32, p_lo: u32, p_hi: u32) {
    let slot = atomicAdd(&evn[0], 1u);
    if (slot >= atomicLoad(&evn[2])) { return; }
    let b = slot * 6u;
    evlog[b] = kind;
    evlog[b + 1u] = atomicLoad(&evn[1]);
    evlog[b + 2u] = id_lo;
    evlog[b + 3u] = id_hi;
    evlog[b + 4u] = p_lo;
    evlog[b + 5u] = p_hi;
}

@compute @workgroup_size(64)
fn log_deaths(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= atomicLoad(&evn[3])) { return; }
    let e = u32(list[id.x] & u64(0xffffffffu));
    put(select(2u, 1u, dying[e] == 1u), ent[e * ENT], ent[e * ENT + 1u], 0u, 0u);
}

@compute @workgroup_size(64)
fn log_births(@builtin(global_invocation_id) id: vec3<u32>) {
    let rank = id.x;
    if (rank >= atomicLoad(&evn[4])) { return; }
    let e = work[rank];
    let child = ((u64(params[P_NEXTID_HI]) << 32u) | u64(params[P_NEXTID_LO])) + u64(rank);
    put(0u, u32(child & u64(0xffffffffu)), u32(child >> 32u), ent[e * ENT], ent[e * ENT + 1u]);
}
