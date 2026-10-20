// Set phase (spec §7.1) in contract mode, one thread per entity: senses, the mind, intents.
// CONSTANTS and CONTRACT are inserted before this text.

@group(0) @binding(0) var<storage, read_write> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> ent: array<u32>;
@group(0) @binding(2) var<storage, read_write> epos: array<f32>;
@group(0) @binding(3) var<storage, read_write> amounts: array<u32>;
@group(0) @binding(4) var<storage, read_write> prev: array<u32>;
@group(0) @binding(5) var<storage, read_write> gw: array<u32>;
@group(0) @binding(6) var<storage, read_write> sig: array<f32>;
@group(0) @binding(7) var<storage, read_write> camt: array<u32>;
@group(0) @binding(8) var<storage, read_write> ccap: array<u32>;
@group(0) @binding(9) var<storage, read_write> nstart: array<u32>;
@group(0) @binding(10) var<storage, read_write> norder: array<u32>;
@group(0) @binding(11) var<storage, read_write> rec: array<u32>;
@group(0) @binding(12) var<storage, read_write> prev_out: array<u32>;

const READING_LIMIT = 1000000.0;
const SIGNAL_LIMIT = 10000.0;
const K = 4u;

var<private> nb_j: array<u32, 4>;
var<private> nb_d: array<f32, 4>;
var<private> nb_cnt: u32;

fn TAU_C() -> f32 { return bitcast<f32>(0x40c90fdbu); }
fn units(a: u32) -> f32 { return f32c(a) * pf(P_UNIT); }

fn offset_cell(c: u32, dx: i32, dy: i32) -> u32 {
    let gw = i32(params[P_GW]);
    let gh = i32(params[P_GH]);
    var x = i32(c % params[P_GW]) + dx;
    var y = i32(c / params[P_GW]) + dy;
    if (params[P_WRAP] == 1u) {
        x = ((x % gw) + gw) % gw;
        y = ((y % gh) + gh) % gh;
    } else {
        x = clamp(x, 0, gw - 1);
        y = clamp(y, 0, gh - 1);
    }
    return u32(y * gw + x);
}

fn cell_at(p: vec2<f32>) -> u32 {
    let gw = i32(params[P_GW]);
    let gh = i32(params[P_GH]);
    let x = clamp(i32(floor(p.x * pf(P_CKX))), 0, gw - 1);
    let y = clamp(i32(floor(p.y * pf(P_CKY))), 0, gh - 1);
    return u32(y * gw + x);
}

fn dist2(a: vec2<f32>, b: vec2<f32>) -> f32 {
    var dx = abs(a.x - b.x);
    var dy = abs(a.y - b.y);
    if (params[P_WRAP] == 1u) {
        dx = min(dx, pf(P_W) - dx);
        dy = min(dy, pf(P_H) - dy);
    }
    return fm(dx, dx, dy * dy);
}

fn id_less(a: u32, b: u32) -> bool {
    let ah = ent[a * ENT + 1u];
    let bh = ent[b * ENT + 1u];
    if (ah != bh) { return ah < bh; }
    return ent[a * ENT] < ent[b * ENT];
}

// The other entities within r2 of entity e, as a count; the nearest K are kept in nb_*,
// ordered by squared distance and then by ID.
fn scan(e: u32, r2: f32, keep: bool) -> u32 {
    if (keep) { nb_cnt = 0u; }
    var count = 0u;
    let pos = vec2<f32>(epos[e * 4u], epos[e * 4u + 1u]);
    let b = ent[e * ENT + 3u];
    let bw = i32(params[P_NBX]);
    let bh = i32(params[P_NBY]);
    let bx = i32(b) % bw;
    let by = i32(b) / bw;
    var seen: array<i32, 9>;
    var n_seen = 0u;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            var x = bx + dx;
            var y = by + dy;
            if (params[P_WRAP] == 1u) {
                x = ((x % bw) + bw) % bw;
                y = ((y % bh) + bh) % bh;
            } else if (x < 0 || x >= bw || y < 0 || y >= bh) {
                continue;
            }
            let k = y * bw + x;
            var dup = false;
            for (var t = 0u; t < n_seen; t++) {
                if (seen[t] == k) { dup = true; }
            }
            if (dup) { continue; }
            seen[n_seen] = k;
            n_seen++;
            for (var t = nstart[k]; t < nstart[k + 1]; t++) {
                let j = norder[t];
                if (j == e) { continue; }
                let d2 = dist2(pos, vec2<f32>(epos[j * 4u], epos[j * 4u + 1u]));
                if (d2 <= r2) {
                    count++;
                    if (keep) {
                        // Insert into the sorted list of the nearest K.
                        var at = nb_cnt;
                        for (var q = 0u; q < nb_cnt; q++) {
                            if (d2 < nb_d[q] || (d2 == nb_d[q] && id_less(j, nb_j[q]))) { at = q; break; }
                        }
                        if (at < K) {
                            var last = min(nb_cnt, K - 1u);
                            for (var q = last; q > at; q--) {
                                nb_d[q] = nb_d[q - 1u];
                                nb_j[q] = nb_j[q - 1u];
                            }
                            nb_d[at] = d2;
                            nb_j[at] = j;
                            nb_cnt = min(nb_cnt + 1u, K);
                        }
                    }
                }
            }
        }
    }
    return count;
}

fn read_sense(e: u32, w: u32, cell: u32) -> vec2<f32> {
    let kind = gw[w];
    let ns = params[P_NS];
    let s = gw[w + 5u];
    let pos = vec2<f32>(epos[e * 4u], epos[e * 4u + 1u]);
    let heading = vec2<f32>(epos[e * 4u + 2u], epos[e * 4u + 3u]);
    var r = vec2<f32>(0.0, 0.0);
    switch kind {
        case SENSE_INVENTORY: { r.x = units(amounts[e * ns + s]); }
        case SENSE_CELL: { r.x = units(camt[cell * ns + s]); }
        case SENSE_DELTA: {
            if ((ent[e * ENT + 7u] & 2u) != 0u) {
                r.x = amount_diff(amounts[e * ns + s], prev[e * ns + s], pf(P_UNIT));
            }
        }
        case SENSE_GRADIENT: {
            let a = camt[offset_cell(cell, 1, 0) * ns + s];
            let b = camt[offset_cell(cell, -1, 0) * ns + s];
            let c = camt[offset_cell(cell, 0, 1) * ns + s];
            let d = camt[offset_cell(cell, 0, -1) * ns + s];
            let gx = amount_diff(a, b, pf(P_KX));
            let gy = amount_diff(c, d, pf(P_KY));
            r = vec2<f32>(fm(gx, heading.x, gy * heading.y), fm(gy, heading.x, -(gx * heading.y)));
        }
        case SENSE_PROBE: {
            let dir = turned(heading, bitcast<f32>(gw[w + 4u]));
            let dist = bitcast<f32>(gw[w + 6u]);
            let p = vec2<f32>(fm(dir.x, dist, pos.x), fm(dir.y, dist, pos.y));
            let wrap = params[P_WRAP] == 1u;
            let q = vec2<f32>(fit(p.x, pf(P_W), wrap), fit(p.y, pf(P_H), wrap));
            r.x = units(camt[cell_at(q) * ns + s]);
        }
        case SENSE_CROWD: {
            let rad = min(bitcast<f32>(gw[w + 4u]), pf(P_RADIUS));
            r.x = f32(scan(e, rad * rad, false));
        }
        case SENSE_NEIGHBOUR: {
            let rank = gw[w + 6u];
            if (rank < nb_cnt) { r.x = units(amounts[nb_j[rank] * ns + s]); }
        }
        case SENSE_CLOCK: {
            let period = gw[w + 6u];
            let p24 = u32((u64(params[P_TICK] % period) << 24u) / u64(period));
            let phase = f32c(p24) * bitcast<f32>(0x33800000u);
            r.x = sincos(TAU_C() * phase).x;
        }
        default: {}
    }
    return vec2<f32>(sanitize(r.x, READING_LIMIT), sanitize(r.y, READING_LIMIT));
}

fn scale(o: f32, mx: u32) -> u32 {
    return u32(floor(clamp(o, 0.0, 1.0) * f32c(mx)));
}

fn put(base: u32, code: u32, g: u32, s: u32, amount: u32, fee: u32, other: u32, fbits: u32) {
    rec[base] = code;
    rec[base + 1u] = g;
    rec[base + 2u] = s;
    rec[base + 3u] = amount;
    rec[base + 4u] = fee;
    rec[base + 5u] = other;
    rec[base + 6u] = fbits;
    rec[base + 7u] = 0u;
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let e = id.x;
    if (e >= params[P_N]) { return; }
    let ns = params[P_NS];
    let cell = ent[e * ENT + 2u];
    let goff = ent[e * ENT + 4u];
    let n_genes = ent[e * ENT + 5u];
    let nsg = ent[e * ENT + 6u];
    let flags = ent[e * ENT + 7u];
    let roff = ent[e * ENT + 8u];
    let slots = goff + nsg;

    var s: array<f32, NSIG>;
    for (var j = 0u; j < nsg; j++) { s[j] = sig[e * NSIG + j]; }

    nb_cnt = 0u;
    if ((flags & 1u) != 0u) { scan(e, pf(P_R2), true); }

    // Senses read once.
    var rd1: array<f32, MAXG>;
    var rd2: array<f32, MAXG>;
    for (var g = 0u; g < n_genes; g++) {
        let w = slots + g * SLOT;
        if (gw[w] <= SENSE_DELTA) {
            let r = read_sense(e, w, cell);
            rd1[g] = r.x;
            rd2[g] = r.y;
        }
    }
    if ((flags & 4u) != 0u) {
        for (var k = 0u; k < ns; k++) { prev_out[e * ns + k] = amounts[e * ns + k]; }
    }

    // The mind.
    var acc: array<f32, NSIG>;
    for (var step = 0u; step < params[P_STEPS]; step++) {
        for (var j = 0u; j < nsg; j++) { acc[j] = 0.0; }
        for (var g = 0u; g < n_genes; g++) {
            let w = slots + g * SLOT;
            let kind = gw[w];
            if (kind <= SENSE_DELTA) {
                let out = gw[w + 2u];
                acc[out] += rd1[g];
                let out2 = gw[w + 3u];
                if (out2 != NONE) { acc[out2] += rd2[g]; }
            } else if (kind == WEIGH) {
                var x = bitcast<f32>(gw[w + 4u]);
                let n_in = gw[w + 6u];
                for (var k = 0u; k < n_in; k++) {
                    x = fm(bitcast<f32>(gw[w + 8u + 2u * k]), s[gw[w + 9u + 2u * k]], x);
                }
                let out = gw[w + 2u];
                let curve = gw[w + 5u];
                if (curve == 0u) { acc[out] += x; }
                else if (curve == 1u) { acc[out] += select(0.0, 1.0, x > 0.0); }
                else if (curve == 2u) { acc[out] += sigmoid_d(x); }
                else { acc[out] = tanh_into(x, acc[out]); }
            } else if (kind == PRODUCT) {
                let out = gw[w + 2u];
                acc[out] = fm(s[gw[w + 8u]], s[gw[w + 9u]], acc[out]);
            }
        }
        for (var j = 0u; j < nsg; j++) {
            s[j] = sanitize(fm(bitcast<f32>(gw[goff + j]), s[j], acc[j]), SIGNAL_LIMIT);
        }
    }
    for (var j = 0u; j < nsg; j++) { sig[e * NSIG + j] = s[j]; }

    // Intents and reaction caps, in gene order.
    var moves = false;
    var turns = false;
    var divides = false;
    var fired: array<u32, 4>;
    let one = params[P_ONE] == 1u;
    for (var g = 0u; g < n_genes; g++) {
        let w = slots + g * SLOT;
        let kind = gw[w];
        let base = roff + g * REC;
        rec[base] = 0u;
        if (kind < REACT) { continue; }
        let src = gw[w + 2u];
        var o = bitcast<f32>(gw[w + 3u]);
        if (src != NONE) { o = s[src]; }
        let threshold = bitcast<f32>(gw[w + 4u]);
        let fee = gw[w + 7u];
        let og = gw[w + 1u];
        let sub = gw[w + 5u];
        if (kind == REACT) {
            put(base, R_REACT, og, 0u, scale(o, gw[w + 8u]), 0u, 0u, 0u);
        } else if (kind == MOVE) {
            if (o > threshold && !moves) {
                moves = true;
                let distance = clamp(o, 0.0, 1.0) * bitcast<f32>(gw[w + 8u]);
                let f = u32(ceil(f32c(gw[w + 9u]) * distance));
                put(base, R_MOVE, og, 0u, 0u, f, 0u, bitcast<u32>(distance));
            }
        } else if (kind == TURN) {
            if (o > threshold && !turns) {
                turns = true;
                let angle = clamp(o, -1.0, 1.0) * bitcast<f32>(gw[w + 8u]);
                put(base, R_TURN, og, 0u, 0u, fee, 0u, bitcast<u32>(angle));
            }
        } else if (kind == TAKE || kind == DROP) {
            let take = kind == TAKE;
            let kk = select(1u, 0u, take);
            if (o > threshold && ccap[cell * ns + sub] > 0u) {
                let new_one = !one || (fired[kk] & (1u << sub)) == 0u;
                fired[kk] = fired[kk] | (1u << sub);
                if (new_one) {
                    put(base, select(R_DROP, R_TAKE, take), og, sub, scale(o, gw[w + 8u]), fee, 0u, 0u);
                }
            }
        } else if (kind == GIVE || kind == STEAL) {
            let give = kind == GIVE;
            let kk = select(3u, 2u, give);
            if (o > threshold && nb_cnt > 0u) {
                let new_one = !one || (fired[kk] & (1u << sub)) == 0u;
                fired[kk] = fired[kk] | (1u << sub);
                if (new_one) {
                    put(base, select(R_STEAL, R_GIVE, give), og, sub, scale(o, gw[w + 8u]), fee, nb_j[0], 0u);
                }
            }
        } else if (kind == DIVIDE) {
            if (o > threshold && !divides) {
                divides = true;
                put(base, R_DIVIDE, og, 0u, 0u, fee, 0u, 0u);
            }
        } else if (kind == GUARD) {
            if (o > threshold) { put(base, R_GUARD, og, sub, 0u, fee, 0u, 0u); }
        }
    }
}
