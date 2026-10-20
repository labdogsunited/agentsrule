//! The run journal on the GPU (`proposals/engine-api.md` H3): a GPU run with swaps, replayed from its
//! journal on the same device, reaches the same state. Needs the GPU.

mod common;

use common::journalled;
use emerge_server::journal::replay;

#[test]
fn a_gpu_run_replays_from_its_journal() {
    let (run, text) = journalled(true);
    assert!(text.lines().next().unwrap().contains("\"backend\":\"gpu: "));
    let again = replay(&text).unwrap();
    assert_eq!((again.tick(), again.revision(), again.digest()), (run.tick(), run.revision(), run.digest()));
}
