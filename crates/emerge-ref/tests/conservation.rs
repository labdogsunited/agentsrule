//! Random worlds must keep the element ledger exact and every store within its caps.

mod common;

use common::{dish, dish_cell};
use emerge_ref::{End, Event, Gene, Law, U};

/// xorshift64*, so the test needs no dependencies and every seed is reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn random_worlds_conserve_every_element() {
    let (mut births, mut deaths) = (0, 0);
    for seed in 1..=40u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let (mut w, d) = dish(rng.below(40) as u32, rng.below(6) as u32);
        let subs = [d.light, d.energy, d.heat, d.sugar, d.carbon, d.nitrate, d.body];
        let cell_subs = [d.light, d.sugar, d.carbon, d.nitrate, d.body];
        let reactions = [d.photosynthesis, d.respiration, d.build, d.rot];
        let sources = [d.sun, d.spark, d.rock];

        // A ring of cells, with a few extra links.
        let n = 3 + rng.below(8) as usize;
        for _ in 0..n {
            let c = dish_cell(&mut w, &d);
            for &s in &cell_subs {
                w.endow(End::Cell(c), s, rng.below(50 * U as u64) as u32);
            }
        }
        for c in 0..n {
            w.link(c, (c + 1) % n);
            if rng.below(3) == 0 {
                w.link(c, rng.below(n as u64) as usize);
            }
        }

        for _ in 0..rng.below(5) {
            let s = cell_subs[rng.below(5) as usize];
            let overflow = if rng.below(2) == 0 { Some(d.void) } else { None };
            w.laws.push(Law::Inflow { s, source: sources[rng.below(3) as usize], amount: rng.below(3 * U as u64) as u32, overflow });
        }
        if rng.below(2) == 0 {
            w.laws.push(Law::Diffuse { substances: cell_subs.to_vec(), rate_bp: rng.below(2_000) as u32 });
        }
        for _ in 0..rng.below(3) {
            w.laws.push(Law::React { reaction: reactions[rng.below(4) as usize], rate_bp: rng.below(500) as u32 });
        }

        w.max_age = 20 + rng.below(200);
        w.max_entities = 5 + rng.below(60) as usize;
        for _ in 0..rng.below(20) {
            let mut genes = Vec::new();
            for _ in 0..rng.below(6) {
                let s = subs[rng.below(7) as usize];
                let max = rng.below(2 * U as u64) as u32;
                let fee = rng.below(20) as u32;
                genes.push(match rng.below(4) {
                    0 => Gene::Take { s, max, fee },
                    1 => Gene::Drop { s, max, fee },
                    2 => Gene::React { reaction: reactions[rng.below(4) as usize], rate: rng.below(2 * U as u64) as u32 },
                    _ => Gene::Divide { split_bp: rng.below(10_001) as u32, requires: (s, rng.below(20 * U as u64) as u32), fee },
                });
            }
            let e = w.spawn(rng.below(n as u64) as usize, genes);
            for &s in &subs {
                w.endow(End::Entity(e), s, rng.below(20 * U as u64) as u32);
            }
        }

        for _ in 0..300 {
            if let Err(err) = w.step() {
                panic!("seed {seed}: {err:?}");
            }
            let stores = w.cells.iter().chain(w.entities.iter().map(|e| &e.store));
            for store in stores {
                for (s, (&a, &cap)) in store.amounts.iter().zip(&store.caps).enumerate() {
                    assert!(a <= cap, "seed {seed}: substance {s} at {a} over cap {cap}");
                }
            }
        }
        births += w.events.iter().filter(|e| matches!(e, Event::Born { .. })).count();
        deaths += w.events.iter().filter(|e| matches!(e, Event::Died { .. })).count();
    }
    // The worlds must actually exercise division and disposal.
    assert!(births > 100 && deaths > 100, "births {births}, deaths {deaths}");
}
