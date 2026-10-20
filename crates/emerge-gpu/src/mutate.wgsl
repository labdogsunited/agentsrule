// Variation at division (spec §8.1) in contract mode: a port of `emerge-ref/src/mutate.rs`, one
// thread per child. The draws are the same Philox draws in the same order. CONSTANTS, CONTRACT
// and the menu constants come first.
//
// The working genome is held in private arrays: `sl` (slots, in module order), `mx` (module
// expression bits), `kp` (keep values).

@group(0) @binding(0) var<storage, read_write> params: array<u32>;
@group(0) @binding(1) var<storage, read_write> menus: array<u32>;
@group(0) @binding(2) var<storage, read_write> rates: array<u32>;
@group(0) @binding(3) var<storage, read_write> mw: array<u32>;
@group(0) @binding(4) var<storage, read_write> jobs: array<u32>;
@group(0) @binding(5) var<storage, read_write> out: array<u32>;

const MAXS = 48u;
const MAXMODS = 16u;
const NEARBY = 8u;
const THRESHOLD_LIMIT = 16.0;
const OUTW = 1100u;
const P_JOBS = 53u;
const P_SEEDLO = 30u;
const P_SEEDHI = 31u;

var<private> sl: array<u32, 864>;
var<private> ns: u32;
var<private> mx: array<u32, 16>;
var<private> nm: u32;
var<private> kp: array<f32, 32>;
var<private> nk: u32;
var<private> dn: u32;
var<private> trace: array<u32, 48>;
var<private> ph: array<u32, 8>;
var<private> dstream: u32;
var<private> dtick: u32;
var<private> cap_genes: u32;
var<private> cap_signals: u32;
var<private> cap_modules: u32;

fn U64_MAX() -> u64 { return (u64(0xffffffffu) << 32u) | u64(0xffffffffu); }

// ---------- draws ----------
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
        ctr = vec4<u32>(u32(p1 >> 32u) ^ ctr.y ^ key.x, u32(p1), u32(p0 >> 32u) ^ ctr.w ^ key.y, u32(p0));
    }
    return ctr;
}

fn unit() -> f32 {
    let index = dn;
    dn = dn + 1u;
    let key = vec2<u32>(params[P_SEEDLO], params[P_SEEDHI]);
    // Stream: (seed, tick, child id, entity domain 0, aux 0, pass MUTATE = 3).
    let ctr = vec4<u32>(index / 4u, dtick, dstream, 3u << 24u);
    let r = philox(ctr, key);
    let q = index % 4u;
    var d = r.w;
    if (q == 0u) { d = r.x; } else if (q == 1u) { d = r.y; } else if (q == 2u) { d = r.z; }
    return f32(d >> 8u) * bitcast<f32>(0x33800000u);
}

fn chance(p: f32) -> bool { return unit() < p; }
fn below(n: u32) -> u32 { return min(u32(unit() * f32(n)), n - 1u); }
fn normal() -> f32 {
    var s = 0.0;
    for (var k = 0u; k < 12u; k++) { s = s + unit(); }
    return s - 6.0;
}
fn mad(a: f32, b: f32, c: f32) -> f32 { return fm(a, b, c); }
fn round_c(x: f32) -> f32 { return floor(x + 0.5); }
fn round_prod(a: f32, b: f32) -> f32 { return floor(fma(a, b, 0.5)); }

fn perturb(x: f32, lo: f32, hi: f32, sigma: f32) -> f32 {
    let n = normal();
    return clamp(mad(n * sigma, hi - lo, x), lo, hi);
}
fn perturb_mu(x: u32, lo: f32, hi: f32, sigma: f32) -> u32 { return u32(round_c(perturb(f32c(x), lo, hi, sigma))); }
fn threshold_step(t: f32, sigma: f32) -> f32 {
    let n = normal();
    return clamp(mad(n, sigma, t), -THRESHOLD_LIMIT, THRESHOLD_LIMIT);
}

// ---------- tables ----------
fn mf(i: u32) -> f32 { return bitcast<f32>(menus[i]); }
fn flag(f: u32) -> bool { return (menus[MN_FLAGS] & f) != 0u; }
fn act_base(a: u32) -> u32 { return MN_ACT + a * ACTW; }
fn act_present(a: u32) -> bool { return menus[act_base(a) + ACTION_PRESENT] == 1u; }
// A random entry of the list stored at `base` (count, entries): NONE if it is empty (no draw).
fn pick_list(base: u32) -> u32 {
    let n = menus[base];
    if (n == 0u) { return NONE; }
    return menus[base + 1u + below(n)];
}
fn pick_act_subs(a: u32) -> u32 { return pick_list(act_base(a) + 3u); }

// ---------- slot access ----------
fn gw_(i: u32, k: u32) -> u32 { return sl[i * SLOT + k]; }
fn gf(i: u32, k: u32) -> f32 { return bitcast<f32>(sl[i * SLOT + k]); }
fn set_w(i: u32, k: u32, v: u32) { sl[i * SLOT + k] = v; }
fn set_f(i: u32, k: u32, v: f32) { sl[i * SLOT + k] = bitcast<u32>(v); }

fn clear_slot(i: u32) { for (var k = 0u; k < SLOT; k++) { sl[i * SLOT + k] = 0u; } }
fn copy_slot(src: u32, dst: u32) { for (var k = 0u; k < SLOT; k++) { sl[dst * SLOT + k] = sl[src * SLOT + k]; } }
// Makes room at `at` by shifting the slots from `at` up.
fn open_slot(at: u32) {
    var i = ns;
    while (i > at) { copy_slot(i - 1u, i); i = i - 1u; }
    ns = ns + 1u;
}
fn close_slot(at: u32) {
    for (var i = at; i + 1u < ns; i++) { copy_slot(i + 1u, i); }
    ns = ns - 1u;
}

// Whether the gene writes a signal (sense, weigh, product).
fn writes_signal(kind: u32) -> bool { return kind <= SENSE_DELTA || kind == WEIGH || kind == PRODUCT; }

// ---------- point mutation of one gene ----------
fn move_reaction(i: u32) {
    let n = menus[MN_REACT];
    if (n < 2u) { return; }
    let reaction = gw_(i, 5u);
    var at = 0u;
    for (var k = 0u; k < n; k++) {
        if (menus[MN_REACT + 1u + 2u * k] == reaction) { at = k; break; }
    }
    var to = 0u;
    if (chance(0.5)) {
        let step = 1u + below(NEARBY);
        if (chance(0.5)) { to = min(at + step, n - 1u); } else { to = select(0u, at - step, at >= step); }
    } else {
        to = below(n);
    }
    set_w(i, 5u, menus[MN_REACT + 1u + 2u * to]);
    set_w(i, 8u, min(gw_(i, 8u), menus[MN_REACT + 2u + 2u * to]));
}

fn substance_action(i: u32, a: u32, sigma: f32) {
    let b = act_base(a);
    let r = below(3u);
    if (r == 0u) {
        if (act_present(a)) {
            let s = pick_act_subs(a);
            if (s != NONE) { set_w(i, 5u, s); }
        }
    } else if (r == 1u) {
        if (act_present(a)) { set_w(i, 8u, perturb_mu(gw_(i, 8u), mf(b), mf(b + 1u), sigma)); }
    } else {
        set_f(i, 4u, threshold_step(gf(i, 4u), sigma));
    }
}

fn point(i: u32, sigma: f32) {
    let kind = gw_(i, 0u);
    if (kind == SENSE_INVENTORY || kind == SENSE_CELL || kind == SENSE_DELTA) {
        let s = pick_list(MN_SUBS);
        if (s != NONE) { set_w(i, 5u, s); }
    } else if (kind == SENSE_GRADIENT) {
        let s = pick_list(MN_FIELD);
        if (s != NONE) { set_w(i, 5u, s); }
    } else if (kind == SENSE_PROBE) {
        let r = below(3u);
        if (r == 0u) {
            let s = pick_list(MN_FIELD);
            if (s != NONE) { set_w(i, 5u, s); }
        } else if (flag(F_PROBE)) {
            if (r == 1u) { set_f(i, 4u, perturb(gf(i, 4u), mf(MN_PROBE), mf(MN_PROBE + 1u), sigma)); }
            else { set_f(i, 6u, perturb(gf(i, 6u), mf(MN_PROBE + 2u), mf(MN_PROBE + 3u), sigma)); }
        }
    } else if (kind == SENSE_CROWD) {
        if (flag(F_CROWD)) { set_f(i, 4u, perturb(gf(i, 4u), mf(MN_CROWD), mf(MN_CROWD + 1u), sigma)); }
    } else if (kind == SENSE_NEIGHBOUR) {
        let c = chance(0.5);
        if (flag(F_NEIGH) && c) {
            set_w(i, 6u, u32(round_c(perturb(f32c(gw_(i, 6u)), mf(MN_NEIGH), mf(MN_NEIGH + 1u), sigma))));
        } else {
            let s = pick_list(MN_SUBS);
            if (s != NONE) { set_w(i, 5u, s); }
        }
    } else if (kind == SENSE_CLOCK) {
        if (flag(F_CLOCK)) {
            let v = round_c(perturb(f32c(gw_(i, 6u)), mf(MN_CLOCK), mf(MN_CLOCK + 1u), sigma));
            set_w(i, 6u, u32(max(v, 1.0)));
        }
    } else if (kind == WEIGH) {
        if (!flag(F_WEIGH)) { return; }
        let n_in = gw_(i, 6u);
        let r = below(2u + n_in);
        if (r == 0u) { set_f(i, 4u, perturb(gf(i, 4u), mf(MN_WEIGH), mf(MN_WEIGH + 1u), sigma)); }
        else if (r == 1u) {
            let nc = menus[MN_WEIGH + 6u];
            if (nc > 0u) { set_w(i, 5u, menus[MN_WEIGH + 7u + below(nc)]); }
        } else {
            let k = r - 2u;
            set_f(i, 8u + 2u * k, perturb(gf(i, 8u + 2u * k), mf(MN_WEIGH + 2u), mf(MN_WEIGH + 3u), sigma));
        }
    } else if (kind == PRODUCT) {
        return;
    } else if (kind == REACT) {
        if (chance(0.5)) {
            move_reaction(i);
        } else {
            let n = menus[MN_REACT];
            let reaction = gw_(i, 5u);
            for (var k = 0u; k < n; k++) {
                if (menus[MN_REACT + 1u + 2u * k] == reaction) {
                    set_w(i, 8u, perturb_mu(gw_(i, 8u), 0.0, f32c(menus[MN_REACT + 2u + 2u * k]), sigma));
                    break;
                }
            }
        }
    } else if (kind == MOVE || kind == TURN) {
        let a = select(A_TURN, A_MOVE, kind == MOVE);
        let c = chance(0.5);
        if (act_present(a) && c) {
            let b = act_base(a);
            set_f(i, 8u, perturb(gf(i, 8u), mf(b), mf(b + 1u), sigma));
        } else {
            set_f(i, 4u, threshold_step(gf(i, 4u), sigma));
        }
    } else if (kind == TAKE) { substance_action(i, A_TAKE, sigma); }
    else if (kind == DROP) { substance_action(i, A_DROP, sigma); }
    else if (kind == GIVE) { substance_action(i, A_GIVE, sigma); }
    else if (kind == STEAL) { substance_action(i, A_STEAL, sigma); }
    else if (kind == GUARD) {
        if (chance(0.5)) {
            if (act_present(A_GUARD)) {
                let s = pick_act_subs(A_GUARD);
                if (s != NONE) { set_w(i, 5u, s); }
            }
        } else {
            set_f(i, 4u, threshold_step(gf(i, 4u), sigma));
        }
    } else if (kind == DIVIDE) {
        let c = chance(0.5);
        if (act_present(A_DIVIDE) && c) {
            let b = act_base(A_DIVIDE);
            let frac = f32c(gw_(i, 5u)) * bitcast<f32>(0x38d1b717u);   // basis points as a fraction: * 0.0001
            let split = perturb(frac, mf(b), mf(b + 1u), sigma);
            set_w(i, 5u, u32(round_prod(split, 10000.0)));
        } else {
            set_f(i, 4u, threshold_step(gf(i, 4u), sigma));
        }
    }
}

// ---------- signals ----------
// Calls f on every signal reference of slot i, in the order of `signals_mut`; mode 0 = collect into
// the `refs` private list, mode 1 = renumber through `map`.

// The word indexes of slot i's signal references, in the order of `signals_mut`.
fn sig_words(i: u32, out_words: ptr<function, array<u32, 8>>) -> u32 {
    let kind = gw_(i, 0u);
    var n = 0u;
    if (kind <= SENSE_DELTA) {
        (*out_words)[n] = 2u; n++;
        if (gw_(i, 3u) != NONE) { (*out_words)[n] = 3u; n++; }
    } else if (kind == WEIGH) {
        let n_in = gw_(i, 6u);
        for (var k = 0u; k < n_in; k++) { (*out_words)[n] = 9u + 2u * k; n++; }
        (*out_words)[n] = 2u; n++;
    } else if (kind == PRODUCT) {
        (*out_words)[n] = 8u; n++;
        (*out_words)[n] = 9u; n++;
        (*out_words)[n] = 2u; n++;
    } else {
        if (gw_(i, 2u) != NONE) { (*out_words)[n] = 2u; n++; }
    }
    return n;
}

fn fresh() -> u32 {
    if (nk >= cap_signals) { return NONE; }
    kp[nk] = 0.0;
    nk = nk + 1u;
    return nk - 1u;
}

// ---------- random_gene ----------
// Builds a new gene in slot `at` (already opened); returns false if none could be made (draws
// may have been consumed). The slot is left cleared on failure.
fn random_gene(at: u32) -> bool {
    var kinds: array<u32, 18>;
    var nkinds = 0u;
    if (flag(F_INVENTORY) && menus[MN_SUBS] > 0u) { kinds[nkinds] = 0u; nkinds++; }
    if (flag(F_GRADIENT) && menus[MN_FIELD] > 0u) { kinds[nkinds] = 1u; nkinds++; }
    if (flag(F_WEIGH) && nk > 0u) { kinds[nkinds] = 2u; nkinds++; }
    if (flag(F_PRODUCT) && nk > 0u) { kinds[nkinds] = 3u; nkinds++; }
    if (menus[MN_REACT] > 0u) { kinds[nkinds] = 4u; nkinds++; }
    if (act_present(A_MOVE)) { kinds[nkinds] = 5u; nkinds++; }
    if (act_present(A_TURN)) { kinds[nkinds] = 6u; nkinds++; }
    if (act_present(A_TAKE)) { kinds[nkinds] = 7u; nkinds++; }
    if (act_present(A_DROP)) { kinds[nkinds] = 8u; nkinds++; }
    if (act_present(A_DIVIDE)) { kinds[nkinds] = 9u; nkinds++; }
    if (flag(F_PROBE) && menus[MN_FIELD] > 0u) { kinds[nkinds] = 10u; nkinds++; }
    if (flag(F_CROWD)) { kinds[nkinds] = 11u; nkinds++; }
    if (flag(F_NEIGH)) { kinds[nkinds] = 12u; nkinds++; }
    if (flag(F_CLOCK)) { kinds[nkinds] = 13u; nkinds++; }
    if (act_present(A_GIVE)) { kinds[nkinds] = 14u; nkinds++; }
    if (act_present(A_STEAL)) { kinds[nkinds] = 15u; nkinds++; }
    if (flag(F_DELTA)) { kinds[nkinds] = 16u; nkinds++; }
    if (act_present(A_GUARD)) { kinds[nkinds] = 17u; nkinds++; }
    clear_slot(at);
    if (nkinds == 0u) { return false; }
    let existing = nk;
    let k = kinds[below(nkinds)];
    set_w(at, 2u, NONE);
    set_w(at, 3u, NONE);
    if (k == 0u) {
        let s = pick_list(MN_SUBS);
        if (s == NONE) { return false; }
        let o = fresh();
        if (o == NONE) { return false; }
        set_w(at, 0u, SENSE_INVENTORY); set_w(at, 5u, s); set_w(at, 2u, o);
        return true;
    }
    if (k == 1u) {
        let s = pick_list(MN_FIELD);
        if (s == NONE) { return false; }
        if (nk + 2u > cap_signals) { return false; }
        let o = fresh();
        let o2 = fresh();
        set_w(at, 0u, SENSE_GRADIENT); set_w(at, 5u, s); set_w(at, 2u, o); set_w(at, 3u, o2);
        return true;
    }
    if (k == 2u) {
        let lo_in = menus[MN_WEIGH + 4u];
        let hi_in = menus[MN_WEIGH + 5u];
        let n = lo_in + below(hi_in - lo_in + 1u);
        set_w(at, 0u, WEIGH);
        set_w(at, 6u, n);
        for (var q = 0u; q < n; q++) {
            let wt = fm(unit(), mf(MN_WEIGH + 3u) - mf(MN_WEIGH + 2u), mf(MN_WEIGH + 2u));
            set_f(at, 8u + 2u * q, wt);
            set_w(at, 9u + 2u * q, below(existing));
        }
        let nc = menus[MN_WEIGH + 6u];
        if (nc == 0u) { return false; }
        set_w(at, 5u, menus[MN_WEIGH + 7u + below(nc)]);
        let bias = fm(unit(), mf(MN_WEIGH + 1u) - mf(MN_WEIGH), mf(MN_WEIGH));
        set_f(at, 4u, bias);
        let o = fresh();
        if (o == NONE) { clear_slot(at); return false; }
        set_w(at, 2u, o);
        return true;
    }
    if (k == 3u) {
        let a = below(existing);
        let b = below(existing);
        let o = fresh();
        if (o == NONE) { return false; }
        set_w(at, 0u, PRODUCT); set_w(at, 8u, a); set_w(at, 9u, b); set_w(at, 2u, o);
        return true;
    }
    if (k == 4u) {
        let n = menus[MN_REACT];
        let q = below(n);
        let reaction = menus[MN_REACT + 1u + 2u * q];
        let mx_rate = menus[MN_REACT + 2u + 2u * q];
        let u = unit();
        set_w(at, 0u, REACT); set_w(at, 5u, reaction);
        set_w(at, 8u, u32(round_prod(u, f32c(mx_rate))));
        set_w(at, 3u, bitcast<u32>(1.0));
        return true;
    }
    if (k == 5u || k == 6u) {
        let a = select(A_TURN, A_MOVE, k == 5u);
        let b = act_base(a);
        let v = fm(unit(), mf(b + 1u) - mf(b), mf(b));
        set_w(at, 0u, select(TURN, MOVE, k == 5u));
        set_w(at, 3u, bitcast<u32>(1.0));
        set_f(at, 8u, v);
        if (k == 5u) { set_w(at, 9u, menus[b + 2u]); } else { set_w(at, 7u, menus[b + 2u]); }
        return true;
    }
    if (k == 7u || k == 8u || k == 14u || k == 15u) {
        let a = select(select(select(A_STEAL, A_GIVE, k == 14u), A_DROP, k == 8u), A_TAKE, k == 7u);
        let b = act_base(a);
        let s = pick_act_subs(a);
        if (s == NONE) { return false; }
        let v = fm(unit(), mf(b + 1u) - mf(b), mf(b));
        set_w(at, 0u, select(select(select(STEAL, GIVE, k == 14u), DROP, k == 8u), TAKE, k == 7u));
        set_w(at, 3u, bitcast<u32>(1.0));
        set_w(at, 5u, s);
        set_w(at, 8u, u32(round_c(v)));
        set_w(at, 7u, menus[b + 2u]);
        return true;
    }
    if (k == 9u) {
        let b = act_base(A_DIVIDE);
        let v = fm(unit(), mf(b + 1u) - mf(b), mf(b));
        set_w(at, 0u, DIVIDE);
        set_w(at, 3u, bitcast<u32>(1.0));
        set_w(at, 5u, u32(round_prod(v, 10000.0)));
        set_w(at, 8u, menus[MN_DIV_REQ]);
        set_w(at, 9u, menus[MN_DIV_REQ + 1u]);
        set_w(at, 7u, menus[b + 2u]);
        return true;
    }
    if (k == 10u) {
        let s = pick_list(MN_FIELD);
        if (s == NONE) { return false; }
        let angle = fm(unit(), mf(MN_PROBE + 1u) - mf(MN_PROBE), mf(MN_PROBE));
        let dist = fm(unit(), mf(MN_PROBE + 3u) - mf(MN_PROBE + 2u), mf(MN_PROBE + 2u));
        let o = fresh();
        if (o == NONE) { return false; }
        set_w(at, 0u, SENSE_PROBE); set_w(at, 5u, s); set_f(at, 4u, angle); set_f(at, 6u, dist); set_w(at, 2u, o);
        return true;
    }
    if (k == 11u) {
        let radius = fm(unit(), mf(MN_CROWD + 1u) - mf(MN_CROWD), mf(MN_CROWD));
        let o = fresh();
        if (o == NONE) { return false; }
        set_w(at, 0u, SENSE_CROWD); set_f(at, 4u, radius); set_w(at, 2u, o);
        return true;
    }
    if (k == 12u) {
        let lo = ceil(mf(MN_NEIGH));
        let rank = u32(lo) + below(u32(floor(mf(MN_NEIGH + 1u)) - lo) + 1u);
        let s = pick_list(MN_SUBS);
        if (s == NONE) { return false; }
        let o = fresh();
        if (o == NONE) { return false; }
        set_w(at, 0u, SENSE_NEIGHBOUR); set_w(at, 5u, s); set_w(at, 6u, rank); set_w(at, 2u, o);
        return true;
    }
    if (k == 13u) {
        let lo = ceil(mf(MN_CLOCK));
        let period = max(u32(lo) + below(u32(floor(mf(MN_CLOCK + 1u)) - lo) + 1u), 1u);
        let o = fresh();
        if (o == NONE) { return false; }
        set_w(at, 0u, SENSE_CLOCK); set_w(at, 6u, period); set_w(at, 2u, o);
        return true;
    }
    if (k == 16u) {
        let s = pick_list(MN_SUBS);
        if (s == NONE) { return false; }
        let o = fresh();
        if (o == NONE) { return false; }
        set_w(at, 0u, SENSE_DELTA); set_w(at, 5u, s); set_w(at, 2u, o);
        return true;
    }
    if (k == 17u) {
        let s = pick_act_subs(A_GUARD);
        if (s == NONE) { return false; }
        set_w(at, 0u, GUARD); set_w(at, 3u, bitcast<u32>(1.0)); set_w(at, 5u, s); set_w(at, 7u, menus[act_base(A_GUARD) + 2u]);
        return true;
    }
    return false;
}

// ---------- the job ----------
fn compact_signals() {
    var used: array<u32, 32>;
    for (var i = 0u; i < ns; i++) {
        var w: array<u32, 8>;
        let n = sig_words(i, &w);
        for (var q = 0u; q < n; q++) { used[gw_(i, w[q])] = 1u; }
    }
    var map: array<u32, 32>;
    var m = 0u;
    var keep_new: array<f32, 32>;
    for (var s = 0u; s < nk; s++) {
        if (used[s] == 1u) {
            map[s] = m;
            keep_new[m] = kp[s];
            m++;
        }
    }
    for (var i = 0u; i < ns; i++) {
        var w: array<u32, 8>;
        let n = sig_words(i, &w);
        for (var q = 0u; q < n; q++) { set_w(i, w[q], map[gw_(i, w[q])]); }
    }
    for (var s = 0u; s < m; s++) { kp[s] = keep_new[s]; }
    nk = m;
}

fn load_master(moff: u32) {
    ns = mw[moff];
    nm = mw[moff + 1u];
    nk = mw[moff + 2u];
    for (var m = 0u; m < nm; m++) { mx[m] = mw[moff + 4u + m]; }
    for (var k = 0u; k < nk; k++) { kp[k] = bitcast<f32>(mw[moff + MASTER_HDR + k]); }
    let sb = moff + MASTER_HDR + nk;
    for (var q = 0u; q < ns * SLOT; q++) { sl[q] = mw[sb + q]; }
}

fn mod_start(m: u32) -> u32 {
    var i = 0u;
    while (i < ns && gw_(i, 16u) < m) { i++; }
    return i;
}
fn mod_len(m: u32) -> u32 {
    var n = 0u;
    for (var i = 0u; i < ns; i++) { if (gw_(i, 16u) == m) { n++; } }
    return n;
}

fn rewire(i: u32) {
    let kind = gw_(i, 0u);
    if (kind == WEIGH && gw_(i, 6u) > 0u) {
        let q = below(gw_(i, 6u));
        set_w(i, 9u + 2u * q, below(nk));
    } else if (kind == PRODUCT) {
        let to = below(nk);
        if (chance(0.5)) { set_w(i, 8u, to); } else { set_w(i, 9u, to); }
    } else if (kind >= REACT) {
        let to = below(nk + 1u);
        if (to == nk) { set_w(i, 2u, NONE); set_w(i, 3u, bitcast<u32>(1.0)); }
        else { set_w(i, 2u, to); set_w(i, 3u, 0u); }
    }
}

fn mutate_core(lineage: u32) {
    let rb = lineage * RATES;
    let rate = bitcast<f32>(rates[rb]);
    let sigma = bitcast<f32>(rates[rb + 1u]);
    let flip = bitcast<f32>(rates[rb + 2u]);
    let p_rewire = bitcast<f32>(rates[rb + 3u]);
    let p_diverge = bitcast<f32>(rates[rb + 4u]);
    let p_dup_gene = bitcast<f32>(rates[rb + 5u]);
    let p_insert = bitcast<f32>(rates[rb + 6u]);
    let p_delete = bitcast<f32>(rates[rb + 7u]);
    let p_dup_mod = bitcast<f32>(rates[rb + 8u]);
    let p_del_mod = bitcast<f32>(rates[rb + 9u]);
    let p_split = bitcast<f32>(rates[rb + 10u]);
    let p_merge = bitcast<f32>(rates[rb + 11u]);
    cap_genes = rates[rb + 12u];
    cap_signals = rates[rb + 13u];
    cap_modules = rates[rb + 14u];

    // 1. Point mutations: parameters, keep factors, expression bits.
    for (var i = 0u; i < ns; i++) {
        if (chance(rate)) { point(i, sigma); }
        trace[i] = dn;
    }
    for (var k = 0u; k < nk; k++) {
        if (chance(rate)) { kp[k] = perturb(kp[k], mf(MN_KEEP), mf(MN_KEEP + 1u), sigma); }   // (0, 1) from the table, so the compiler cannot fold the range
    }
    ph[3] = dn;
    var gi = 0u;
    for (var m = 0u; m < nm; m++) {
        while (gi < ns && gw_(gi, 16u) == m) {
            if (chance(flip)) { set_w(gi, 17u, 1u - gw_(gi, 17u)); }
            gi++;
        }
        if (chance(flip)) { mx[m] = 1u - mx[m]; }
    }

    ph[0] = dn;
    // 2. Wiring: rewire one input, or point one output at a fresh signal.
    for (var i = 0u; i < ns; i++) {
        let c = chance(p_rewire);
        if (c && nk > 0u) { rewire(i); }
        if (chance(p_diverge)) {
            let kind = gw_(i, 0u);
            if (writes_signal(kind)) {
                let f = fresh();
                if (f != NONE) {
                    if (kind <= SENSE_DELTA) {
                        if (gw_(i, 3u) != NONE) {
                            if (chance(0.5)) { set_w(i, 3u, f); } else { set_w(i, 2u, f); }
                        } else {
                            set_w(i, 2u, f);
                        }
                    } else {
                        set_w(i, 2u, f);
                    }
                }
            }
        }
    }

    ph[1] = dn;
    // 3. Growth, gene by gene and then module by module. New material starts dormant.
    for (var mi = 0u; mi < nm; mi++) {
        let start = mod_start(mi);
        var len = mod_len(mi);
        var g = 0u;
        while (g < len) {
            let c1 = chance(p_dup_gene);
            if (c1 && ns < cap_genes) {
                open_slot(start + g + 1u);
                copy_slot(start + g, start + g + 1u);
                set_w(start + g + 1u, 17u, 0u);
                len++;
                g++;
            }
            let c2 = chance(p_insert);
            if (c2 && ns < cap_genes) {
                let at = start + g + 1u;
                open_slot(at);
                if (random_gene(at)) {
                    set_w(at, 16u, mi);
                    set_w(at, 17u, 0u);
                    len++;
                    g++;
                } else {
                    close_slot(at);
                }
            }
            if (chance(p_delete)) {
                close_slot(start + g);
                len--;
                continue;
            }
            g++;
        }
    }
    ph[2] = dn;
    var mi = 0u;
    while (mi < nm) {
        let c1 = chance(p_dup_mod);
        if (c1 && nm < cap_modules && ns + mod_len(mi) <= cap_genes) {
            // Private signals (used by no gene outside the module) are replaced in the copy.
            var outside: array<u32, 32>;
            for (var i = 0u; i < ns; i++) {
                if (gw_(i, 16u) != mi) {
                    var w: array<u32, 8>;
                    let n = sig_words(i, &w);
                    for (var q = 0u; q < n; q++) { outside[gw_(i, w[q])] = 1u; }
                }
            }
            let start = mod_start(mi);
            let len = mod_len(mi);
            var privs: array<u32, 32>;
            var np = 0u;
            for (var i = start; i < start + len; i++) {
                var w: array<u32, 8>;
                let n = sig_words(i, &w);
                for (var q = 0u; q < n; q++) {
                    let s = gw_(i, w[q]);
                    if (outside[s] == 0u) {
                        var seen = false;
                        for (var t = 0u; t < np; t++) { if (privs[t] == s) { seen = true; } }
                        if (!seen) { privs[np] = s; np++; }
                    }
                }
            }
            if (nk + np <= cap_signals) {
                let base = nk;
                for (var t = 0u; t < np; t++) { kp[nk] = kp[privs[t]]; nk++; }
                // Later modules move up one; the copy goes after this module's genes.
                for (var i = 0u; i < ns; i++) { if (gw_(i, 16u) > mi) { set_w(i, 16u, gw_(i, 16u) + 1u); } }
                let at = start + len;
                for (var q = 0u; q < len; q++) { open_slot(at + q); }
                for (var q = 0u; q < len; q++) {
                    copy_slot(start + q, at + q);
                    set_w(at + q, 16u, mi + 1u);
                    var w: array<u32, 8>;
                    let n = sig_words(at + q, &w);
                    for (var r = 0u; r < n; r++) {
                        let s = gw_(at + q, w[r]);
                        for (var t = 0u; t < np; t++) { if (privs[t] == s) { set_w(at + q, w[r], base + t); break; } }
                    }
                }
                var m = nm;
                while (m > mi + 1u) { mx[m] = mx[m - 1u]; m = m - 1u; }
                mx[mi + 1u] = 0u;
                nm++;
                mi++;
            }
        }
        let c2 = chance(p_del_mod);
        if (c2 && nm > 1u) {
            let start = mod_start(mi);
            let len = mod_len(mi);
            for (var q = 0u; q < len; q++) { close_slot(start); }
            for (var i = 0u; i < ns; i++) { if (gw_(i, 16u) > mi) { set_w(i, 16u, gw_(i, 16u) - 1u); } }
            for (var m = mi; m + 1u < nm; m++) { mx[m] = mx[m + 1u]; }
            nm--;
            continue;
        }
        let c3 = chance(p_split);
        if (c3 && mod_len(mi) >= 2u && nm < cap_modules) {
            let len = mod_len(mi);
            let at = 1u + below(len - 1u);
            let start = mod_start(mi);
            for (var i = 0u; i < ns; i++) { if (gw_(i, 16u) > mi) { set_w(i, 16u, gw_(i, 16u) + 1u); } }
            for (var q = at; q < len; q++) { set_w(start + q, 16u, mi + 1u); }
            var m = nm;
            while (m > mi + 1u) { mx[m] = mx[m - 1u]; m = m - 1u; }
            mx[mi + 1u] = mx[mi];
            nm++;
            mi++;
        }
        let c4 = chance(p_merge);
        if (c4 && mi + 1u < nm) {
            for (var i = 0u; i < ns; i++) {
                if (gw_(i, 16u) == mi + 1u) { set_w(i, 16u, mi); }
                else if (gw_(i, 16u) > mi + 1u) { set_w(i, 16u, gw_(i, 16u) - 1u); }
            }
            for (var m = mi + 1u; m + 1u < nm; m++) { mx[m] = mx[m + 1u]; }
            nm--;
        }
        mi++;
    }
    compact_signals();
}

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
