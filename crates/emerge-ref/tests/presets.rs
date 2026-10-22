//! The presets under `examples/presets/` (`proposals/world-presets.md`): the one mechanism each
//! relies on that can be checked exactly, on a small grid with no creatures.

use emerge_ref::compile::{compile, Options};

const LONG_NIGHT: &str = include_str!("../../../examples/presets/long_night.em");
const HARSH: &str = include_str!("../../../examples/presets/long_night_harsh.em");

/// Light in cell 0 after each of `ticks` ticks (no creatures, so nothing eats it). The cell's
/// capacity is raised (`holds` gives a capacity) so that the inflow never hits it.
fn light_per_tick(src: &str, ticks: u32) -> Vec<u32> {
    let src = src.replace("Grid(w: 256, h: 256)", "Grid(w: 8, h: 8)").replace("light: 1u,", "light: 100u,");
    let mut w = compile(&src, &[], Options { seed: 1, allow_partial: true, mutation: false }).unwrap_or_else(|e| panic!("{e}")).world;
    let l = w.chem.substances.iter().position(|s| s.name == "light").unwrap();
    (0..ticks)
        .map(|t| {
            w.step().unwrap_or_else(|e| panic!("tick {t}: {e:?}"));
            w.cells[0].amounts[l]
        })
        .collect()
}

#[test]
fn the_long_night_changes_the_inflow_at_exactly_its_ticks() {
    for (src, from, to) in [(LONG_NIGHT, 300u32, 400u32), (HARSH, 300, 450)] {
        let light = light_per_tick(src, to + 20);
        // `tick` counts from 0 in the condition; light[t] is the amount after tick t has run.
        let gained: Vec<u32> = (0..light.len()).map(|t| light[t] - if t == 0 { 0 } else { light[t - 1] }).collect();
        let supplied: Vec<usize> = (0..gained.len()).filter(|&t| gained[t] > 0).collect();
        assert!(supplied.contains(&((from - 1) as usize)), "light still arrives in the last tick before the night");
        assert!(!supplied.iter().any(|&t| (from as usize..to as usize).contains(&t)), "no light arrives from tick {from} to {}", to - 1);
        assert!(supplied.contains(&(to as usize)), "light arrives again at tick {to}");
        assert_eq!(supplied.iter().filter(|&&t| t < from as usize).count(), from as usize, "every tick before the night supplies light");
    }
}
