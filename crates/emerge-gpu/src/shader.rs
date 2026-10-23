//! WGSL shared by the kernels: the contract-mode functions of `emerge_ref::contract`, with
//! explicit `fma` wherever a multiply meets an add (checked against the CPU in
//! `emerge-gpu-spike a3`), and the constants of `pack.rs`.

use crate::pack::*;
use emerge_ref::contract as c;

fn lit(x: f32) -> String {
    format!("bitcast<f32>({:#x}u)", x.to_bits())
}

pub fn constants() -> String {
    let mut s = String::new();
    for (name, v) in [
        ("ENT", ENT as u32), ("SLOT", SLOT as u32), ("NSIG", NSIG as u32), ("MAXG", MAXG as u32), ("NONE", NONE), ("REC", REC as u32),
        ("SENSE_INVENTORY", SENSE_INVENTORY), ("SENSE_CELL", SENSE_CELL), ("SENSE_GRADIENT", SENSE_GRADIENT), ("SENSE_PROBE", SENSE_PROBE),
        ("SENSE_CROWD", SENSE_CROWD), ("SENSE_NEIGHBOUR", SENSE_NEIGHBOUR), ("SENSE_CLOCK", SENSE_CLOCK), ("SENSE_DELTA", SENSE_DELTA),
        ("WEIGH", WEIGH), ("PRODUCT", PRODUCT), ("REACT", REACT), ("MOVE", MOVE), ("TURN", TURN), ("TAKE", TAKE), ("DROP", DROP),
        ("GIVE", GIVE), ("STEAL", STEAL), ("GUARD", GUARD), ("DIVIDE", DIVIDE),
        ("MN_SUBS", MN_SUBS as u32), ("MN_FIELD", MN_FIELD as u32), ("MN_FLAGS", MN_FLAGS as u32), ("MN_PROBE", MN_PROBE as u32), ("MN_CROWD", MN_CROWD as u32),
        ("MN_NEIGH", MN_NEIGH as u32), ("MN_CLOCK", MN_CLOCK as u32), ("MN_WEIGH", MN_WEIGH as u32), ("MN_ACT", MN_ACT as u32), ("ACTW", ACTW as u32),
        ("MN_DIV_REQ", MN_DIV_REQ as u32), ("MN_REACT", MN_REACT as u32), ("MN_KEEP", MN_KEEP as u32), ("ACTION_PRESENT", ACTION_PRESENT as u32), ("MASTER_HDR", MASTER_HDR as u32),
        ("RATES", RATES as u32), ("F_INVENTORY", F_INVENTORY), ("F_DELTA", F_DELTA), ("F_GRADIENT", F_GRADIENT), ("F_PRODUCT", F_PRODUCT), ("F_PROBE", F_PROBE),
        ("F_CROWD", F_CROWD), ("F_NEIGH", F_NEIGH), ("F_CLOCK", F_CLOCK), ("F_WEIGH", F_WEIGH), ("A_MOVE", A_MOVE as u32), ("A_TURN", A_TURN as u32),
        ("A_TAKE", A_TAKE as u32), ("A_DROP", A_DROP as u32), ("A_GIVE", A_GIVE as u32), ("A_STEAL", A_STEAL as u32), ("A_GUARD", A_GUARD as u32), ("A_DIVIDE", A_DIVIDE as u32),
        ("R_MOVE", R_MOVE), ("R_TURN", R_TURN), ("R_TAKE", R_TAKE), ("R_DROP", R_DROP), ("R_GIVE", R_GIVE), ("R_STEAL", R_STEAL),
        ("R_GUARD", R_GUARD), ("R_DIVIDE", R_DIVIDE), ("R_REACT", R_REACT),
        ("P_N", P_N as u32), ("P_NS", P_NS as u32), ("P_TICK", P_TICK as u32), ("P_STEPS", P_STEPS as u32), ("P_ONE", P_ONE as u32),
        ("P_GW", P_GW as u32), ("P_GH", P_GH as u32), ("P_WRAP", P_WRAP as u32), ("P_NBX", P_NBX as u32), ("P_NBY", P_NBY as u32),
        ("P_W", P_W as u32), ("P_H", P_H as u32), ("P_R2", P_R2 as u32), ("P_RADIUS", P_RADIUS as u32), ("P_KX", P_KX as u32),
        ("P_KY", P_KY as u32), ("DEG_BITS", c::DEG.to_bits()), ("P_CKX", P_CKX as u32), ("P_CKY", P_CKY as u32), ("P_UNIT", P_UNIT as u32),
    ] {
        s.push_str(&format!("const {name} = {v}u;\n"));
    }
    s
}

/// Per-lineage resource accounts (`emerge run --accounts`, by lineage): `acct_add` adds into the tail of
/// the `flows` buffer, which every kernel that counts binds as `flows`, when `params[P_ACC]` (the word where
/// the counters start, plus 1) is not 0. Two words per counter, low then high.
pub fn accounts() -> String {
    let mut s = format!("const P_ACC = {}u;\nconst P_PAYSUB = {}u;\nconst NCOL = {}u;\n", P_ACC, P_PAY, emerge_ref::world::ACCOUNT_COLUMNS.len());
    for (i, name) in emerge_ref::world::ACCOUNT_COLUMNS.iter().enumerate() {
        s.push_str(&format!("const AC_{} = {}u;\n", name.to_uppercase(), i));
    }
    s.push_str(&format!("const P_LSTAT = {}u;\nconst P_NLAWSK = {}u;\nconst LS_PASSED = 0u;\nconst LS_REQUESTS = 1u;\nconst LS_REQUESTED = 2u;\nconst LS_DELIVERED = 3u;\nconst LS_CUT_O = 4u;\nconst LS_CUT_D = 5u;\nconst LS_RAN = 6u;\nconst LS_EXTENT = 7u;\nconst NLS = {}u;\n", P_LSTAT, P_NLAWS, NLS));
    s.push_str(r#"fn ls_add(law: u32, col: u32, x: u32) {
    let base = params[P_LSTAT];
    if (base == 0u || x == 0u) { return; }
    let i = base - 1u + (law * NLS + col) * 2u;
    let old = atomicAdd(&flows[i], x);
    if (old + x < old) { atomicAdd(&flows[i + 1u], 1u); }
}
// A workgroup works on one statement, so it sums its counters in shared memory first (thousands of
// threads adding to the same words would wait for one another) and adds them to `flows` once.
var<workgroup> lsw_lo: array<atomic<u32>, 8>;
var<workgroup> lsw_hi: array<atomic<u32>, 8>;
fn lsw_clear(t: u32) {
    if (t < NLS) { atomicStore(&lsw_lo[t], 0u); atomicStore(&lsw_hi[t], 0u); }
}
fn lsw_add(col: u32, x: u32) {
    if (params[P_LSTAT] == 0u || x == 0u) { return; }
    let old = atomicAdd(&lsw_lo[col], x);
    if (old + x < old) { atomicAdd(&lsw_hi[col], 1u); }
}
fn lsw_flush(t: u32, law: u32) {
    let base = params[P_LSTAT];
    if (base == 0u || t >= NLS) { return; }
    let lo = atomicLoad(&lsw_lo[t]);
    let hi = atomicLoad(&lsw_hi[t]);
    if (lo != 0u || hi != 0u) {
        let i = base - 1u + (law * NLS + t) * 2u;
        let old = atomicAdd(&flows[i], lo);
        atomicAdd(&flows[i + 1u], hi + select(0u, 1u, old + lo < old));
    }
}
// One request of a world statement: what it asked, what the first stage left, what arrived.
fn ls_req(asked: u32, g1: u32, g2: u32) {
    lsw_add(LS_REQUESTS, 1u);
    lsw_add(LS_REQUESTED, asked);
    lsw_add(LS_DELIVERED, g2);
    lsw_add(LS_CUT_O, select(0u, 1u, g1 < asked));
    lsw_add(LS_CUT_D, select(0u, 1u, g2 < g1));
}
fn acct_add(lineage: u32, col: u32, x: u32) {
    let base = params[P_ACC];
    if (base == 0u || x == 0u) { return; }
    let i = base - 1u + (lineage * NCOL + col) * 2u;
    let old = atomicAdd(&flows[i], x);
    if (old + x < old) { atomicAdd(&flows[i + 1u], 1u); }
}
"#);
    s
}

/// The contract functions. `params` must be bound as `array<u32>` named `params`.
pub fn contract() -> String {
    let sin: Vec<String> = c::SIN.iter().map(|&x| lit(x)).collect();
    let cos: Vec<String> = c::COS.iter().map(|&x| lit(x)).collect();
    format!(
        r#"
fn tiny_c() -> f32 {{ return {tiny}; }}
fn flush(x: f32) -> f32 {{ return select(x, 0.0, abs(x) < tiny_c()); }}
fn fm(a: f32, b: f32, c: f32) -> f32 {{ return flush(fma(a, b, c)); }}
fn tiny_in(x: f32) -> f32 {{ return select(x, 0.0, abs(x) < tiny_c()); }}
fn pf(i: u32) -> f32 {{ return bitcast<f32>(params[i]); }}
// A u32 as an f32 with one rounding (to nearest even), whatever the converter does.
fn f32c(a: u32) -> f32 {{ return fma(f32(a >> 12u), 4096.0, f32(a & 4095u)); }}
fn sanitize(x: f32, limit: f32) -> f32 {{
    if (!(abs(x) <= {fmax})) {{ return 0.0; }}
    return flush(clamp(x, -limit, limit));
}}
fn amount_diff(a: u32, b: u32, k: f32) -> f32 {{
    if (a >= b) {{ return f32c(a - b) * k; }}
    return -(f32c(b - a) * k);
}}
fn tanh_into(x: f32, acc: f32) -> f32 {{
    let u = clamp(tiny_in(x) * 0.5, -1.0, 1.0);
    return fm(u, 2.0 - abs(u), acc);
}}
fn sigmoid_d(x: f32) -> f32 {{
    let v = clamp(tiny_in(x) * 0.25, -1.0, 1.0);
    return 0.5 + 0.5 * (v * (2.0 - abs(v)));
}}
fn sincos(x0: f32) -> vec2<f32> {{
    var x = tiny_in(x0);
    if (x > {pi}) {{ x = x - {tau}; }} else if (x < -{pi}) {{ x = x + {tau}; }}
    let a = abs(x);
    var r = a;
    var flip = false;
    if (a > {half_pi}) {{ r = {pi} - a; flip = true; }}
    let r2 = r * r;
    var ps = {sin5};
    ps = fm(ps, r2, {sin4}); ps = fm(ps, r2, {sin3}); ps = fm(ps, r2, {sin2}); ps = fm(ps, r2, {sin1}); ps = fm(ps, r2, {sin0});
    var pc = {cos6};
    pc = fm(pc, r2, {cos5}); pc = fm(pc, r2, {cos4}); pc = fm(pc, r2, {cos3}); pc = fm(pc, r2, {cos2}); pc = fm(pc, r2, {cos1}); pc = fm(pc, r2, {cos0});
    var s = flush(r * ps);
    if (x < 0.0) {{ s = -s; }}
    var cc = pc;
    if (flip) {{ cc = -cc; }}
    return vec2<f32>(s, cc);
}}
// The heading turned by an angle, without renormalising (for `probe`).
fn turned(h: vec2<f32>, angle_deg: f32) -> vec2<f32> {{
    let sc = sincos(angle_deg * {deg});
    return vec2<f32>(fm(h.x, sc.y, -(h.y * sc.x)), fm(h.x, sc.x, h.y * sc.y));
}}
fn fit(v: f32, size: f32, wrap: bool) -> f32 {{
    if (wrap) {{
        var r = v;
        if (v < 0.0) {{ r = v + size; }} else if (v >= size) {{ r = v - size; }}
        if (r >= size || r < 0.0) {{ return 0.0; }}
        return r;
    }}
    return clamp(v, 0.0, bitcast<f32>(bitcast<u32>(size) - 1u));
}}
"#,
        tiny = lit(c::TINY),
        fmax = lit(f32::MAX),
        pi = lit(c::PI),
        tau = lit(c::TAU),
        half_pi = lit(c::HALF_PI),
        deg = lit(c::DEG),
        sin0 = sin[0], sin1 = sin[1], sin2 = sin[2], sin3 = sin[3], sin4 = sin[4], sin5 = sin[5],
        cos0 = cos[0], cos1 = cos[1], cos2 = cos[2], cos3 = cos[3], cos4 = cos[4], cos5 = cos[5], cos6 = cos[6],
    )
}

/// `cond_holds(at, c)`: whether the condition program at word `at` of `table` (`pack::cond_program`)
/// holds in cell `c`. It reads the cells as the tick began (`camt0`), their caps (`ccap`), the tick
/// and the grid (`P_GW`, `P_GH`, `P_WRAP`). Values are `i64` held as `u64` in two's complement, with
/// the saturation of `cond.rs`. WGSL has no recursion, so a reduction's body runs in a second
/// evaluator, `cond_body`, which has no reductions and reads the current neighbour (`C_ELEM`).
pub fn cond(table: &str) -> String {
    let ops = |elem: bool| {
        let push = format!(
            r#"if (op == {c_const}) {{
            st[sp] = (u64({table}[i + 1u]) << 32u) | u64({table}[i]);
            i += 2u;
            sp++;
        }} else if (op == {c_tick}) {{
            st[sp] = u64(params[P_TICK]);
            sp++;
        }} else if (op == {c_own}) {{
            st[sp] = u64(camt0[c * ns + (arg & 0xffu)]);
            sp++;
        }} else if (op == {c_cap}) {{
            st[sp] = u64(ccap[c * ns + (arg & 0xffu)]);
            sp++;
        }} else if (op == {c_nb}) {{
            let n = c_nb(c, arg >> 8u);
            st[sp] = u64(0u);
            if (n != NONE) {{ st[sp] = u64(camt0[n * ns + (arg & 0xffu)]); }}
            sp++;
        }}"#,
            c_const = C_CONST, c_tick = C_TICK, c_own = C_OWN, c_cap = C_CAP, c_nb = C_NB,
        );
        let elem = if elem { format!(" else if (op == {C_ELEM}) {{\n            st[sp] = u64(camt0[nb * ns + (arg & 0xffu)]);\n            sp++;\n        }}") } else { String::new() };
        let rest = format!(
            r#" else if (op == {c_not}) {{
            st[sp - 1u] = c_bool(st[sp - 1u] == u64(0u));
        }} else {{
            let b = st[sp - 1u];
            let a = st[sp - 2u];
            sp--;
            var r = u64(0u);
            if (op == {c_add}) {{ r = c_add(a, b); }}
            else if (op == {c_sub}) {{ r = c_sub(a, b); }}
            else if (op == {c_mul}) {{ r = c_mul(a, b); }}
            else if (op == {c_eq}) {{ r = c_bool(a == b); }}
            else if (op == {c_ne}) {{ r = c_bool(a != b); }}
            else if (op == {c_lt}) {{ r = c_bool(c_lt(a, b)); }}
            else if (op == {c_le}) {{ r = c_bool(!c_lt(b, a)); }}
            else if (op == {c_gt}) {{ r = c_bool(c_lt(b, a)); }}
            else if (op == {c_ge}) {{ r = c_bool(!c_lt(a, b)); }}
            else if (op == {c_and}) {{ r = c_bool(a != u64(0u) && b != u64(0u)); }}
            else {{ r = c_bool(a != u64(0u) || b != u64(0u)); }}
            st[sp - 1u] = r;
        }}"#,
            c_not = C_NOT, c_add = C_ADD, c_sub = C_SUB, c_mul = C_MUL, c_eq = C_EQ, c_ne = C_NE, c_lt = C_LT, c_le = C_LE,
            c_gt = C_GT, c_ge = C_GE, c_and = C_AND,
        );
        format!("{push}{elem}{rest}")
    };
    let offsets: Vec<String> = emerge_ref::cond::OFFSETS.iter().map(|(dx, dy)| format!("vec2<i32>({dx}, {dy})")).collect();
    format!(
        r#"
fn c_min() -> u64 {{ return u64(1u) << 63u; }}
fn c_max() -> u64 {{ return c_min() - u64(1u); }}
fn c_neg(x: u64) -> bool {{ return (x >> 63u) == u64(1u); }}
fn c_lt(a: u64, b: u64) -> bool {{ return (a ^ c_min()) < (b ^ c_min()); }}
fn c_add(a: u64, b: u64) -> u64 {{
    let r = a + b;
    if (c_neg((a ^ r) & (b ^ r))) {{ return select(c_max(), c_min(), c_neg(a)); }}
    return r;
}}
fn c_sub(a: u64, b: u64) -> u64 {{
    let r = a - b;
    if (c_neg((a ^ b) & (a ^ r))) {{ return select(c_max(), c_min(), c_neg(a)); }}
    return r;
}}
fn c_mul(a: u64, b: u64) -> u64 {{
    let neg = c_neg(a) != c_neg(b);
    let ua = select(a, u64(0u) - a, c_neg(a));
    let ub = select(b, u64(0u) - b, c_neg(b));
    let lim = select(c_max(), c_min(), neg);
    if (ua != u64(0u) && ub > lim / ua) {{ return lim; }}
    let p = ua * ub;
    return select(p, u64(0u) - p, neg);
}}
fn c_bool(b: bool) -> u64 {{ return select(u64(0u), u64(1u), b); }}
// The neighbour of cell c in direction d (N, E, S, W, NE, SE, SW, NW), wrapped if the grid wraps,
// else NONE outside it (`cond::neighbour`).
fn c_nb(c: u32, d: u32) -> u32 {{
    var offs = array<vec2<i32>, 8>({offsets});
    let w = i32(params[P_GW]);
    let h = i32(params[P_GH]);
    var x = i32(c % params[P_GW]) + offs[d].x;
    var y = i32(c / params[P_GW]) + offs[d].y;
    if (params[P_WRAP] != 0u) {{
        if (x < 0) {{ x += w; }} else if (x >= w) {{ x -= w; }}
        if (y < 0) {{ y += h; }} else if (y >= h) {{ y -= h; }}
    }} else if (x < 0 || x >= w || y < 0 || y >= h) {{
        return NONE;
    }}
    return u32(y * w + x);
}}
// A reduction's body, words [start, end), for cell c and its neighbour nb.
fn cond_body(start: u32, end: u32, c: u32, nb: u32) -> u64 {{
    var st: array<u64, {stack}>;
    var sp = 0u;
    let ns = params[P_NS];
    var i = start;
    loop {{
        if (i >= end) {{ break; }}
        let op = {table}[i] & 0xffu;
        let arg = {table}[i] >> 8u;
        i++;
        {body_ops}
    }}
    return st[0];
}}
fn cond_holds(at: u32, c: u32) -> bool {{
    var st: array<u64, {stack}>;
    var sp = 0u;
    let ns = params[P_NS];
    let end = at + 1u + {table}[at];
    var i = at + 1u;
    loop {{
        if (i >= end) {{ break; }}
        let op = {table}[i] & 0xffu;
        let arg = {table}[i] >> 8u;
        i++;
        if (op == {c_reduce}) {{
            let red = arg & 0xfu;
            let nd = select(4u, 8u, ((arg >> 4u) & 0xfu) == 1u);
            let len = {table}[i];
            let body = i + 1u;
            i = body + len;
            var acc = u64(0u);
            var any = false;
            var count = 0u;
            for (var d = 0u; d < nd; d++) {{
                let nb = c_nb(c, d);
                if (nb == NONE) {{ continue; }}
                let v = cond_body(body, body + len, c, nb);
                if (red == {red_sum}) {{ acc = c_add(acc, v); }}
                else if (red == {red_count}) {{ if (v != u64(0u)) {{ count++; }} }}
                else if (red == {red_min}) {{ if (!any || c_lt(v, acc)) {{ acc = v; }} }}
                else {{ if (!any || c_lt(acc, v)) {{ acc = v; }} }}
                any = true;
            }}
            st[sp] = select(acc, u64(count), red == {red_count});
            sp++;
        }} else {outer_ops}
    }}
    return st[0] != u64(0u);
}}
"#,
        stack = emerge_ref::cond::GPU_STACK,
        offsets = offsets.join(", "),
        body_ops = ops(true),
        outer_ops = ops(false),
        c_reduce = C_REDUCE,
        red_sum = RED_SUM, red_count = RED_COUNT, red_min = RED_MIN,
    )
}
