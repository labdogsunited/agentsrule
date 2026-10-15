//! Positions, movement, turning, the gradient sense and child placement (spec §5.5, §6.1,
//! §7.2 step 4, §7.5). Exact expected states.

mod common;

use common::dish;
use emerge_ref::world::Space;
use emerge_ref::{End, Gene, Genome, Input, Sense, World, ONE, U};

/// An 8 × 8 wrapping space with a 4 × 4 grid: cells are 2 × 2, as in Dish.
fn small(w: &mut World) {
    let caps = vec![2_000 * U; w.chem.substances.len()];
    w.grid(Space { w: 8.0, h: 8.0, wrap: true, grid_w: 4, grid_h: 4, child_offset: 0.5, radius: 2.0 }, caps);
}

fn mover(src: Input) -> Gene {
    Gene::Move { speed: 1.0, fee_per_distance: 20, src, threshold: 0.0 }
}

fn turner(src: Input) -> Gene {
    Gene::Turn { angle_deg: 30.0, fee: 10, src, threshold: 0.0 }
}

#[test]
fn moving_wraps_charges_the_requested_distance_and_changes_cell_at_the_end_of_the_tick() {
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let e = w.spawn_at([7.5, 1.0], [1.0, 0.0], Genome::simple(vec![mover(Input::Const(0.75))]));
    w.endow(End::Entity(e), d.energy, 100);
    assert_eq!(w.entities[0].cell, 3);
    w.step().unwrap();
    let e = &w.entities[0];
    // 7.5 + 0.75 wraps to 0.25; the fee is ceil(20 × 0.75) = 15mu.
    assert_eq!(e.pos, [0.25, 1.0]);
    assert_eq!(e.store.amounts[d.energy], 85);
    assert_eq!(e.cell, 0);
}

#[test]
fn an_unpaid_move_does_not_happen() {
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let e = w.spawn_at([1.0, 1.0], [1.0, 0.0], Genome::simple(vec![mover(ONE)]));
    w.endow(End::Entity(e), d.energy, 19);
    w.step().unwrap();
    assert_eq!(w.entities[0].pos, [1.0, 1.0]);
    assert_eq!(w.entities[0].store.amounts[d.energy], 19);
}

#[test]
fn turning_is_to_the_left_by_the_signal_times_the_angle() {
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let e = w.spawn_at([1.0, 1.0], [1.0, 0.0], Genome::simple(vec![turner(ONE)]));
    w.endow(End::Entity(e), d.energy, 100);
    w.step().unwrap();
    let [hx, hy] = w.entities[0].heading;
    assert!((hx - 30f32.to_radians().cos()).abs() < 1e-6 && (hy - 0.5).abs() < 1e-6, "{hx}, {hy}");
    assert_eq!(w.entities[0].store.amounts[d.energy], 90);
}

#[test]
fn a_negative_signal_does_not_fire_a_turn_with_the_default_threshold() {
    // Dish's seeds read `steer` with threshold 0.0, so they turn left toward food but never
    // right (DISCREPANCIES.md). A negative signal does not fire: no turn and no fee.
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let e = w.spawn_at([1.0, 1.0], [1.0, 0.0], Genome::simple(vec![turner(Input::Const(-0.5))]));
    w.endow(End::Entity(e), d.energy, 100);
    w.step().unwrap();
    assert_eq!(w.entities[0].heading, [1.0, 0.0]);
    assert_eq!(w.entities[0].store.amounts[d.energy], 100);
    // With threshold -1.0 it fires and turns right by 15°.
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let gene = Gene::Turn { angle_deg: 30.0, fee: 10, src: Input::Const(-0.5), threshold: -1.0 };
    let e = w.spawn_at([1.0, 1.0], [1.0, 0.0], Genome::simple(vec![gene]));
    w.endow(End::Entity(e), d.energy, 100);
    w.step().unwrap();
    assert!((w.entities[0].heading[1] + 15f32.to_radians().sin()).abs() < 1e-6);
}

#[test]
fn the_gradient_is_reported_along_the_heading_and_to_the_left() {
    // The entity's cell is (1, 1). East (2, 1) holds 3u of body, west (0, 1) 1u,
    // south (1, 2) 2u, north (1, 0) nothing. Per unit distance, with cells 2 wide:
    // gx = (3 − 1) / 4 = 0.5 and gy = (2 − 0) / 4 = 0.5.
    let reading = |heading: [f32; 2]| {
        let (mut w, d) = dish(0, 0);
        small(&mut w);
        w.endow(End::Cell(6), d.body, 3 * U);
        w.endow(End::Cell(4), d.body, U);
        w.endow(End::Cell(9), d.body, 2 * U);
        let sense = Gene::Sense { sense: Sense::Gradient(d.body), out: 0, out2: Some(1) };
        w.spawn_at([3.0, 3.0], heading, Genome::simple(vec![sense]));
        w.step().unwrap();
        w.entities[0].signals.clone()
    };
    // Readings are f32(amount) × 0.001, so gx is 0.50000006, not exactly 0.5.
    let units = |mu: f32| mu * 0.001;
    let gx = (units(3_000.0) - units(1_000.0)) * 0.25;
    let gy = (units(2_000.0) - units(0.0)) * 0.25;
    // Heading east: forward is gx, left is gy.
    assert_eq!(reading([1.0, 0.0]), vec![gx, gy]);
    // Heading south (+y): forward is gy, left is −gx.
    assert_eq!(reading([0.0, 1.0]), vec![gy, -gx]);
}

#[test]
fn a_child_is_placed_behind_its_parent_after_the_parent_moves() {
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    let genes = vec![mover(ONE), Gene::divide(5_000, (d.body, U), 0)];
    let e = w.spawn_at([4.0, 4.0], [1.0, 0.0], Genome::simple(genes));
    w.endow(End::Entity(e), d.body, 2 * U);
    w.endow(End::Entity(e), d.energy, 100);
    w.step().unwrap();
    let pos: Vec<_> = w.entities.iter().map(|e| e.pos).collect();
    assert_eq!(pos, vec![[5.0, 4.0], [4.5, 4.0]]);
    assert_eq!(w.entities[1].heading, [1.0, 0.0]);
}

#[test]
fn takes_use_the_snapshot_cell_even_after_moving() {
    // The entity starts in cell 0 and moves into cell 1 this tick. Its take still draws
    // from cell 0; from the next tick on it draws from cell 1.
    let (mut w, d) = dish(0, 0);
    small(&mut w);
    w.endow(End::Cell(0), d.carbon, 10 * U);
    w.endow(End::Cell(1), d.carbon, 10 * U);
    let genes = vec![mover(ONE), Gene::take(d.carbon, U, 0)];
    let e = w.spawn_at([1.5, 1.0], [1.0, 0.0], Genome::simple(genes));
    w.endow(End::Entity(e), d.energy, 100);
    w.step().unwrap();
    assert_eq!((w.cells[0].amounts[d.carbon], w.cells[1].amounts[d.carbon]), (9 * U, 10 * U));
    assert_eq!(w.entities[0].cell, 1);
    w.step().unwrap();
    assert_eq!((w.cells[0].amounts[d.carbon], w.cells[1].amounts[d.carbon]), (9 * U, 9 * U));
}
