//! The run journal (`proposals/engine-api.md` H3): a run with swaps, replayed from its journal alone,
//! reaches the same state; a journal that was changed does not replay. CPU runner only.

use emerge_server::journal::replay;

mod common;

use common::journalled;

#[test]
fn a_run_replays_from_its_journal() {
    let (run, text) = journalled(false);
    assert_eq!(text.lines().count(), 7, "header, five swaps, end");
    let again = replay(&text).unwrap();
    assert_eq!((again.tick(), again.revision(), again.digest()), (run.tick(), run.revision(), run.digest()));
    assert_eq!(again.live.source, run.live.source);
}

#[test]
fn a_changed_journal_does_not_replay() {
    let (_, text) = journalled(false);
    let patch = text.replace("rate = 90bp", "rate = 91bp");
    let err = |t: &str| replay(t).err().expect("the journal should not replay");
    assert!(err(&patch).contains("replays as"), "a changed patch gives another source");
    let end = text.lines().last().unwrap().to_string();
    let digest = end.replace("\"digest\":\"", "\"digest\":\"f");
    assert!(err(&text.replace(&end, &digest)).contains("differs"));
    let mutation = text.replacen("\"mutation\":true", "\"mutation\":false", 1);
    assert!(replay(&mutation).is_err(), "another mode gives another state");
}

#[test]
fn a_run_with_fee_swaps_replays() {
    use emerge_server::journal::Journal;
    use emerge_server::run::Run;
    let mut run = Run::create(common::spec(false), Journal::memory()).unwrap();
    run.step(15);
    run.hotswap("patch Dish {\n    set actions.take.cost = 8mu;\n    set actions.move.cost = 30mu;\n}\n", 0, "dearer foraging").unwrap();
    run.step(30);
    run.hotswap("patch Dish {\n    set actions.take.cost = 2mu;\n}\n", 1, "cheaper again").unwrap();
    run.step(30);
    run.close();
    let text = run.journal.text();
    let again = replay(&text).unwrap();
    assert_eq!((again.tick(), again.revision(), again.digest()), (run.tick(), run.revision(), run.digest()));
}

#[test]
fn a_run_with_a_spill_replays() {
    use emerge_server::journal::Journal;
    use emerge_server::run::Run;
    let mut run = Run::create(common::spec(false), Journal::memory()).unwrap();
    run.step(20);
    run.hotswap("patch Dish {\n    set limits.inventory_cap = 6u move_to: void;\n}\n", 0, "less room").unwrap();
    run.step(30);
    run.close();
    let text = run.journal.text();
    assert!(text.contains("move_to: void"));
    let again = replay(&text).unwrap();
    assert_eq!((again.tick(), again.revision(), again.digest()), (run.tick(), run.revision(), run.digest()));
}
