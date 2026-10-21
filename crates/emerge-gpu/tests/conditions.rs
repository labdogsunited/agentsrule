//! `when`, `else` and `let` in world rules on the GPU (`proposals/rule-conditions.md`, "Stage 1 on
//! the GPU"): the stage 1 fixtures run on both runners in contract mode and give the same state word
//! for word, the 64-bit arithmetic saturates as the CPU's does, the ledger stays exact, and swaps add,
//! change and remove a conditional rule. Needs the GPU.

use emerge_gpu::gpu::Gpu;
use emerge_gpu::m3::{compare_state, StateView};
use emerge_gpu::resident::Resident;
use emerge_gpu::swap::hotswap;
use emerge_ref::compile::{compile, Options};
use emerge_ref::swap::{Live, Rejected};
use emerge_ref::World;

const DISH: &str = include_str!("../../../examples/dish.em");

fn small() -> String {
    DISH.replace("Grid(w: 256, h: 256)", "Grid(w: 96, h: 96)").replace("count: 2_000;", "count: 300;")
}

fn world(src: &str, lineages: &[&str], seed: u64) -> World {
    let mut w = compile(src, lineages, Options { seed, allow_partial: true, mutation: true }).unwrap_or_else(|e| panic!("{e}")).world;
    w.contract = true;
    w
}

/// Whether the GPU's state after `tick` is the CPU's: every cell, entity and genome, and the counts.
fn check(w: &World, r: &Resident, tick: u64) {
    let s = r.snapshot();
    assert_eq!((s.n, s.next_id, s.tick as u64), (w.entities.len(), w.next_id, w.tick), "tick {tick}: counts differ");
    let view = StateView { ent: &s.ent, epos: &s.epos, amounts: &s.amounts, ecap: &s.ecap, sig: &s.sig, camt: &s.camt, gw: &s.gw, mw: &s.mw };
    let mut note = |m: String| println!("tick {tick}: {m}");
    assert_eq!(compare_state(w, &view, s.ns, &mut note), (0, 0), "tick {tick}: CPU and GPU differ");
    assert_eq!((r.births, r.deaths, r.extent()), (w.births, w.deaths, w.entity_extent.clone()), "tick {tick}: births, deaths or extent differ");
}

/// CPU and GPU from the same start, compared every `every` ticks and at the end, with the GPU's ledger
/// audited every tick and the CPU's at the end.
fn same_on_both(src: &str, lineages: &[&str], seed: u64, ticks: u64, every: u64) {
    let gpu = Gpu::new();
    let mut w = world(src, lineages, seed);
    assert!(w.gpu_unsupported().is_none());
    assert!(w.laws.iter().any(|l| l.is_conditional()), "the fixture has no condition");
    let mut r = Resident::new(&gpu, &mut w);
    r.enable_audit();
    for tick in 0..ticks {
        r.tick();
        w.run_tick();
        if tick % every == 0 || tick + 1 == ticks {
            check(&w, &r, tick);
        }
    }
    let a = r.audit().unwrap();
    assert_eq!((a.ticks, a.drifted), (ticks, 0), "the GPU ledger drifted");
    w.audit().unwrap();
}

const SUN: &str = "transfer(light, from: sun, to: self, amount: 40mu, overflow: void) @transfer_light;";
const ROT: &str = "react(rot, rate: 50bp) @react_rot;";
const SPREAD: &str = "diffuse_conserved([sugar, carbon, nitrate, body], rate: 300bp) @diffuse_conserved;";

fn cut_rules(src: &str, names: &[&str]) -> String {
    let mut out = src.to_string();
    for n in names {
        let a = out.find(&format!("    rule {n} on")).unwrap_or_else(|| panic!("no rule {n}"));
        let b = a + out[a..].find("\n    }\n").unwrap() + "\n    }\n".len();
        out.replace_range(a..b, "");
    }
    out
}

/// No creatures and only the rules given: the dish's chemistry, cells at 20u nitrate.
fn bare(rules: &str) -> String {
    let mut src = cut_rules(&small(), &["sunlight", "weather", "rotting", "spread"]);
    let at = src.find("    genesis {").unwrap();
    src.insert_str(at, &format!("{rules}\n"));
    src
}

fn substance(w: &World, name: &str) -> usize {
    w.chem.substances.iter().position(|s| s.name == name).unwrap()
}

#[test]
fn a_conditional_reaction_reads_the_cells_as_the_tick_began() {
    // Photosynthesis needs light, which only the sun's transfer brings. Conditioned on the light at the
    // start of the tick, it cannot run in the first tick on either runner, though the light has arrived
    // by Transform; from the second tick it does. A GPU that read the cells after Move would differ.
    let mut src = cut_rules(&small(), &["weather", "rotting", "spread"]);
    let at = src.find("    genesis {").unwrap();
    src.insert_str(at, "    rule photo on cells {\n        when self.light >= 40mu { react(photosynthesis, rate: 10000bp) @react_photo; }\n    }\n\n");
    let gpu = Gpu::new();
    let mut w = world(&src, &[], 1);
    let sugar = substance(&w, "sugar");
    let mut r = Resident::new(&gpu, &mut w);
    let total = |camt: &[u32], ns: usize| camt.chunks(ns).map(|c| c[sugar] as u64).sum::<u64>();
    r.tick();
    w.run_tick();
    let s = r.snapshot();
    assert_eq!(total(&s.camt, s.ns), 0, "the GPU reacted on light that arrived this tick");
    r.tick();
    w.run_tick();
    let s = r.snapshot();
    assert!(total(&s.camt, s.ns) > 0, "no reaction in the second tick");
    let cpu: Vec<u32> = w.cells.iter().flat_map(|c| c.amounts[..s.ns].to_vec()).collect();
    assert_eq!(s.camt[..cpu.len()], cpu[..]);
}

#[test]
fn conditions_that_always_hold_run_as_on_the_cpu() {
    let wrapped = small()
        .replace(SUN, &format!("when 1 == 1 {{ {SUN} }}"))
        .replace(ROT, &format!("when tick >= 0 {{ {ROT} }}"))
        .replace(SPREAD, &format!("when self.carbon >= 0u and not (self.nitrate > cap(nitrate)) {{ {SPREAD} }}"));
    for seed in [3, 7] {
        same_on_both(&wrapped, &["Drifter", "Builder"], seed, 600, 50);
    }
}

#[test]
fn a_condition_that_never_holds_is_absence_on_the_gpu() {
    let never = small().replace(SUN, &format!("when 1 == 2 {{ {SUN} }}"));
    same_on_both(&never, &["Drifter", "Builder"], 3, 300, 50);
    // And it is the rule's absence: the GPU world without the rule is in the same state.
    let gpu = Gpu::new();
    let (mut a, mut b) = (world(&never, &["Drifter", "Builder"], 3), world(&cut_rules(&small(), &["sunlight"]), &["Drifter", "Builder"], 3));
    let (mut ra, mut rb) = (Resident::new(&gpu, &mut a), Resident::new(&gpu, &mut b));
    for _ in 0..100 {
        ra.tick();
        rb.tick();
    }
    let (sa, sb) = (ra.snapshot(), rb.snapshot());
    assert_eq!((sa.n, &sa.camt, &sa.amounts, &sa.gw), (sb.n, &sb.camt, &sb.amounts, &sb.gw));
}

#[test]
fn a_conditional_inflow_stops_at_its_threshold_on_the_gpu() {
    let src = bare("    rule top_up on cells {\n        when self.nitrate < 25u {\n            transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_nitrate;\n        }\n    }\n");
    let gpu = Gpu::new();
    let mut w = world(&src, &[], 1);
    let n = substance(&w, "nitrate");
    let mut r = Resident::new(&gpu, &mut w);
    for tick in 1..=12u32 {
        r.tick();
        let s = r.snapshot();
        let want = (20_000 + 1_000 * tick).min(25_000);
        assert!(s.camt.chunks(s.ns).take(w.cells.len()).all(|c| c[n] == want), "tick {tick}: expected every cell at {want}µ");
    }
    same_on_both(&src, &[], 1, 12, 1);
}

#[test]
fn let_else_and_nesting_run_as_on_the_cpu() {
    let gate = bare(
        "    rule gate on cells {\n        let low = self.nitrate < 25u;\n        when low {\n            when tick < 30 {\n                transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_nitrate;\n            }\n        } else {\n            transfer(carbon, from: rock, to: self, amount: 1u, overflow: void) @transfer_carbon;\n        }\n    }\n\n    rule late_rot on cells {\n        when not (self.nitrate < 25u) and tick >= 8 { react(rot, rate: 200bp) @react_rot; }\n    }\n",
    );
    same_on_both(&gate, &[], 1, 20, 1);
    // With creatures, diffusion and a conditional reaction together, so cells differ from one another.
    let mixed = small()
        .replace(ROT, &format!("let rich = self.carbon > 50u + 5u * 2; when rich {{ {ROT} }} else {{ react(rot, rate: 200bp) @react_rot_fast; }}"))
        .replace(SPREAD, &format!("when self.sugar < cap(sugar) - 1u or tick < 5 {{ {SPREAD} }}"));
    same_on_both(&mixed, &["Drifter", "Builder"], 4, 400, 25);
}

#[test]
fn the_gpu_saturates_as_the_cpu_does() {
    // Six inflows of 2^k µ of nitrate, each under a condition whose arithmetic leaves the range of i64
    // in some ticks. The nitrate added by tick t tells which conditions held in which ticks. Every
    // operation involves `tick` (`tick * 0 - x` for -x), so the checker folds none of them: literal-only
    // arithmetic is folded before the runner sees it, in the narrower range of language.md §4.1.
    let conds = [
        "tick * 4611686018427387904 > 4611686018427387904",
        "0 - tick * 4611686018427387904 < tick * 0 - 4611686018427387904",
        "tick * 3074457345618258602 + 3074457345618258602 >= 9223372036854775807",
        "tick * 0 - 9223372036854775807 - tick == tick * 0 - 9223372036854775807 - 1",
        "(0 - tick) * 4611686018427387904 <= tick * 0 - 9223372036854775807",
        "(tick * 0 - 9223372036854775807 - 1) * (0 - tick) == 9223372036854775807",
    ];
    let mut rules = String::from("    rule edges on cells {\n");
    for (k, c) in conds.iter().enumerate() {
        rules += &format!("        when {c} {{ transfer(nitrate, from: rock, to: self, amount: {}mu, overflow: void) @transfer_{k}; }}\n", 1 << k);
    }
    rules += "    }\n";
    let src = bare(&rules);
    // What each condition is in tick t, with i64's saturating arithmetic, and with wrapping arithmetic.
    let (m, max) = (i64::MIN, i64::MAX);
    let saturating = |t: i64| -> [bool; 6] {
        [
            t.saturating_mul(1 << 62) > 1 << 62,
            0i64.saturating_sub(t.saturating_mul(1 << 62)) < -(1 << 62),
            t.saturating_mul(3074457345618258602).saturating_add(3074457345618258602) >= max,
            (-max).saturating_sub(t) == m,
            (-t).saturating_mul(1 << 62) <= -max,
            m.saturating_mul(-t) == max,
        ]
    };
    let wrapping = |t: i64| -> [bool; 6] {
        [
            t.wrapping_mul(1 << 62) > 1 << 62,
            0i64.wrapping_sub(t.wrapping_mul(1 << 62)) < -(1 << 62),
            t.wrapping_mul(3074457345618258602).wrapping_add(3074457345618258602) >= max,
            (-max).wrapping_sub(t) == m,
            (-t).wrapping_mul(1 << 62) <= -max,
            m.wrapping_mul(-t) == max,
        ]
    };
    let added = |f: &dyn Fn(i64) -> [bool; 6], ticks: i64| -> u32 { (0..ticks).map(|t| f(t).iter().enumerate().filter(|(_, &h)| h).map(|(k, _)| 1u32 << k).sum::<u32>()).sum() };
    let ticks = 8;
    assert_ne!(added(&saturating, ticks), added(&wrapping, ticks), "the fixture does not tell saturation from wrapping");
    for k in 0..6 {
        let seen: Vec<bool> = (0..ticks).map(|t| saturating(t)[k]).collect();
        assert!(seen.contains(&true) && seen.contains(&false), "condition {k} does not change over the ticks: {seen:?}");
    }
    let gpu = Gpu::new();
    let mut w = world(&src, &[], 1);
    let n = substance(&w, "nitrate");
    let mut r = Resident::new(&gpu, &mut w);
    for _ in 0..ticks {
        r.tick();
        w.run_tick();
    }
    let s = r.snapshot();
    let want = 20_000 + added(&saturating, ticks);
    assert_eq!(w.cells[0].amounts[n], want, "the CPU");
    assert!(s.camt.chunks(s.ns).take(w.cells.len()).all(|c| c[n] == want), "the GPU: cell 0 holds {}µ, {want}µ expected", s.camt[n]);
}

#[test]
fn conditional_rules_can_be_swapped_in_changed_and_removed_on_the_gpu() {
    let live = |src: &str| {
        let mut l = Live::new(src, &["Drifter", "Builder"], Options { seed: 5, allow_partial: true, mutation: true }).unwrap();
        l.world.contract = true;
        l
    };
    let src = small();
    let swaps = [
        (3, "patch Dish {\n    add after rules.spread: rule gate on cells { when tick >= 0 and self.nitrate < 30u { transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_gate; } }\n}\n"),
        (9, "patch Dish {\n    add after rules.gate: rule decay on cells { when self.body > 1u { react(rot, rate: 400bp) @react_decay; } }\n}\n"),
        (15, "patch Dish {\n    replace rules.gate { when self.nitrate < 22u { transfer(nitrate, from: rock, to: self, amount: 2u, overflow: void) @transfer_gate; } }\n}\n"),
        (24, "patch Dish {\n    remove rules.gate;\n    remove rules.decay;\n}\n"),
    ];
    let gpu = Gpu::new();
    let mut cpu = live(&src);
    let mut on_gpu = live(&src);
    let mut r = Resident::new(&gpu, &mut on_gpu.world);
    r.enable_audit();
    for tick in 0..40u64 {
        for (at, patch) in &swaps {
            if *at == tick {
                cpu.hotswap(patch, cpu.revision).unwrap_or_else(|e| panic!("CPU, tick {tick}: {e}"));
                let base = on_gpu.revision;
                hotswap(&mut on_gpu, &mut r, patch, base).unwrap_or_else(|e| panic!("GPU, tick {tick}: {e}"));
            }
        }
        cpu.world.run_tick();
        r.tick();
        check(&cpu.world, &r, tick);
        if tick == 12 {
            assert_eq!(cpu.world.laws.iter().filter(|l| l.is_conditional()).count(), 2);
        }
    }
    assert_eq!(cpu.revision, 4);
    assert!(!cpu.world.laws.iter().any(|l| l.is_conditional()));
    let a = r.audit().unwrap();
    assert_eq!((a.ticks, a.drifted), (40, 0), "the GPU ledger drifted");
    cpu.world.audit().unwrap();
}

#[test]
fn a_condition_deeper_than_the_gpu_stack_is_refused_with_the_reason() {
    let deep = |levels: usize| (0..levels).fold("self.carbon".to_string(), |e, _| format!("self.carbon + ({e})"));
    let rule = |levels: usize| format!("    rule deep on cells {{\n        when self.nitrate < {} {{ transfer(nitrate, from: rock, to: self, amount: 1u, overflow: void) @transfer_deep; }}\n    }}\n", deep(levels));
    // 14 levels need a stack of 16 (the comparison and the sum): it runs, and as on the CPU.
    let fits = world(&bare(&rule(14)), &[], 1);
    assert_eq!(fits.laws.iter().filter_map(|l| l.parts().1).map(|c| c.depth()).max(), Some(16));
    same_on_both(&bare(&rule(14)), &[], 1, 3, 1);
    let too_deep = world(&bare(&rule(15)), &[], 1);
    assert!(too_deep.gpu_unsupported().unwrap().contains("17 values deep"), "{:?}", too_deep.gpu_unsupported());
    // A swap that brings one in is refused, and the GPU world runs on unchanged.
    let gpu = Gpu::new();
    let mut live = Live::new(&small(), &["Drifter"], Options { seed: 2, allow_partial: true, mutation: true }).unwrap();
    live.world.contract = true;
    let mut r = Resident::new(&gpu, &mut live.world);
    let patch = format!("patch Dish {{\n    add after rules.spread: {}\n}}\n", rule(15).trim().replace('\n', " "));
    let base = live.revision;
    assert!(matches!(hotswap(&mut live, &mut r, &patch, base), Err(Rejected::NotSwappable(why)) if why.contains("values deep")));
    r.tick();
    assert_eq!(live.revision, 0);
}
