//! `made` in views (`proposals/visual-language.md` §11): each agent counts the µ of a substance its
//! own reactions made over an explicit window. Counting changes nothing in the run. CPU runner only.

use emerge_server::journal::Journal;
use emerge_server::run::Run;

mod common;

#[test]
fn counting_made_changes_nothing_and_finds_the_producers() {
    let mut plain = Run::create(common::spec(false), Journal::memory()).unwrap();
    let mut counted = Run::create(common::spec(false), Journal::memory()).unwrap();
    plain.step(60);
    counted.step(60);
    counted.count_made("body").unwrap();
    plain.step(40);
    counted.step(40);
    let (mut a, mut b) = (plain.frame(), counted.frame());
    let window = b.made.take().expect("a window was started");
    assert_eq!((window.substance.as_str(), window.from_tick, b.tick), ("body", 60, 100));
    // The same world, tick for tick: only the counts differ.
    let producers = b.agents.iter().filter(|x| x.made > 0).count();
    for x in a.agents.iter_mut().chain(b.agents.iter_mut()) {
        x.made = 0;
    }
    assert_eq!(a, b);
    assert_eq!((plain.births(), plain.population()), (counted.births(), counted.population()));
    // `Builder` runs `build`; `Drifter` has no body reaction in its seed, so producers are mostly builders.
    let builders = counted.frame().agents.iter().filter(|x| x.lineage == 1 && x.made > 0).count();
    assert!(producers > 0 && builders > 0, "producers {producers}, builders among them {builders}");
    assert!(counted.count_made("gold").is_err());
}
