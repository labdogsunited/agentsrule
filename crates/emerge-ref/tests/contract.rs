//! Contract mode (`contract.rs`, `proposals/gpu-runtime.md` M0): the defined float
//! operations are accurate enough for the world, and a contract-mode run is deterministic.

use emerge_ref::compile::{compile, Options};
use emerge_ref::contract::{fit, normal, rotate, sincos, sigmoid_d, tanh_d, tanh_into, DEG, TINY};
use emerge_ref::mind::{think_contract, Gene, Genome};
use emerge_ref::Curve;

const DISH: &str = include_str!("../../../examples/dish.em");

#[test]
fn sincos_is_accurate_on_every_angle_the_world_uses() {
    // Turns and probes (±180°), the clock (0 to 2π), with some margin.
    let (mut worst, mut worst_unit) = (0f64, 0f64);
    let mut x = -7.0f32;
    while x < 7.0 {
        let (s, c) = sincos(x);
        let (rs, rc) = ((x as f64).sin(), (x as f64).cos());
        worst = worst.max((s as f64 - rs).abs()).max((c as f64 - rc).abs());
        worst_unit = worst_unit.max(((s as f64).powi(2) + (c as f64).powi(2) - 1.0).abs());
        x += 1.0e-5;
    }
    // Outside ±(2π + 1) the reduction is not defined; inside, the error is about f32 epsilon.
    assert!(worst < 3.0e-7, "worst error {worst:e}");
    assert!(worst_unit < 5.0e-7, "sin² + cos² off by {worst_unit:e}");
    assert_eq!(sincos(0.0), (0.0, 1.0));
}

#[test]
fn a_heading_stays_a_unit_vector_over_twenty_thousand_turns() {
    let mut h = [1.0f32, 0.0];
    let mut angle = 0.1f32;
    for k in 0..20_000 {
        // The turn gene's range is 0 to 30 degrees; vary the angle, both ways.
        angle = (angle * 1.37 + 0.011).rem_euclid(30.0);
        let a = if k % 2 == 0 { angle } else { -angle };
        let (s, c) = sincos(a * DEG);
        h = rotate(h, s, c);
        let len = ((h[0] as f64).powi(2) + (h[1] as f64).powi(2)).sqrt();
        assert!((len - 1.0).abs() < 1.0e-6, "after {k} turns the heading has length {len}");
    }
}

#[test]
fn the_curves_are_bounded_and_flush_tiny_inputs() {
    for x in [-1.0e30f32, -3.0, -0.5, 0.0, 0.5, 3.0, 1.0e30] {
        assert!((-1.0..=1.0).contains(&tanh_d(x)));
        assert!((0.0..=1.0).contains(&sigmoid_d(x)));
    }
    assert_eq!(tanh_d(2.0), 1.0);
    assert_eq!(tanh_d(-2.0), -1.0);
    assert_eq!(sigmoid_d(0.0), 0.5);
    assert_eq!(tanh_d(TINY * 0.5), 0.0);
    assert_eq!(tanh_into(0.0, 0.25), 0.25);
    assert_eq!(tanh_into(1.0, 0.25), 0.25 + 0.75);
}

#[test]
fn the_normal_draw_has_mean_zero_and_variance_one() {
    let mut state = 12345u64;
    let mut unit = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 40) as f32) * (1.0 / 16_777_216.0)
    };
    let draws: Vec<f64> = (0..200_000).map(|_| normal(&mut unit) as f64).collect();
    let mean = draws.iter().sum::<f64>() / draws.len() as f64;
    let var = draws.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / draws.len() as f64;
    assert!(mean.abs() < 0.02, "mean {mean}");
    assert!((var - 1.0).abs() < 0.03, "variance {var}");
    assert!(draws.iter().all(|x| x.abs() <= 6.0));
}

#[test]
fn positions_wrap_once_into_the_space() {
    assert_eq!(fit(-0.25, 512.0, true), 511.75);
    assert_eq!(fit(512.5, 512.0, true), 0.5);
    assert_eq!(fit(-1.0e-9, 512.0, true), 0.0); // rounds up to the size, which is not a position
    assert_eq!(fit(10.0, 512.0, true), 10.0);
    assert!(fit(1000.0, 512.0, false) < 512.0);
    assert_eq!(fit(-3.0, 512.0, false), 0.0);
}

#[test]
fn a_mind_step_is_a_sum_from_zero_then_one_fused_keep_times_signal() {
    // out0 <- 0.5 + 2·s1 (linear), out0 also gets product s1·s1; keep0 = 0.5, s0 = 1.0, s1 = 3.0.
    let genes = vec![
        Gene::Weigh { bias: 0.5, curve: Curve::Linear, inputs: vec![(2.0, 1)], out: 0 },
        Gene::Product { a: 1, b: 1, out: 0 },
    ];
    let mut g = Genome::simple(genes);
    g.keep = vec![0.5, 0.0];
    let mut signals = vec![1.0f32, 3.0];
    think_contract(&g, &mut signals, &[[0.0; 2], [0.0; 2]], 1);
    // acc = (0.5 + 2·3) + 3·3 = 15.5; s0 = 0.5·1.0 + 15.5 = 16; s1 = 0.0·3.0 + 0 = 0.
    assert_eq!(signals, vec![16.0, 0.0]);
}

fn state(contract: bool, ticks: u64) -> (Vec<String>, u64) {
    let mut w = compile(DISH, &["Drifter", "Builder"], Options { seed: 3, allow_partial: true, mutation: true }).unwrap().world;
    w.contract = contract;
    for _ in 0..ticks {
        w.run_tick();
    }
    let mut out = vec![format!("tick {} births {} deaths {}", w.tick, w.births, w.deaths)];
    for e in &w.entities {
        out.push(format!("{} {:?} {:?} {:?} {:?}", e.id, e.pos.map(f32::to_bits), e.heading.map(f32::to_bits), e.store.amounts, e.genome));
    }
    (out, w.births)
}

#[test]
fn a_contract_mode_run_is_deterministic_and_keeps_a_population() {
    let (a, births) = state(true, 150);
    let (b, _) = state(true, 150);
    assert!(a.len() > 100 && births > 1000, "{} entities, {births} births", a.len());
    assert_eq!(a, b);
}

#[test]
fn contract_mode_is_a_different_but_similar_world_and_is_off_by_default() {
    let (default, _) = state(false, 150);
    let (contract, _) = state(true, 150);
    assert_ne!(default, contract);
    let (n1, n2) = (default.len() as f64, contract.len() as f64);
    assert!((n1 - n2).abs() / n1 < 0.1, "populations {n1} and {n2}");
}
