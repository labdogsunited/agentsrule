//! A world whose only life is a state field (`proposals/field-rules.md` step 1): it runs in the
//! engine with no entities, and the GPU backend refuses it. CPU only.

use emerge_server::journal::Journal;
use emerge_server::run::{Run, Spec};

const LIFE: &str = include_str!("../../../examples/life.em");

#[test]
fn an_automaton_runs_without_entities_and_the_gpu_refuses_it() {
    let spec = |gpu| Spec { source: LIFE.to_string(), lineages: Vec::new(), seed: 1, mutation: false, contract: false, gpu };
    let mut run = Run::create(spec(false), Journal::memory()).unwrap();
    assert_eq!(run.population(), 0);
    assert!(run.alive());
    assert_eq!(run.step(30), 30, "no entities, but the field keeps changing");
    let frame = run.frame();
    assert_eq!(frame.states.len(), 1);
    assert!(frame.states[0].cells.contains(&1), "the gun keeps cells alive");
    let refused = Run::create(spec(true), Journal::memory()).err().map(|e| e.to_string());
    assert!(refused.as_deref().is_some_and(|m| m.contains("CPU only")), "{refused:?}");
}
