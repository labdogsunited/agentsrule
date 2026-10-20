//! Hot-swaps of a world kept on the GPU (spec §13.2, `proposals/engine-api.md` H2). The CPU side
//! (`emerge_ref::swap::Live`) keeps the source and revision and stages the patch; the GPU world
//! is preflighted against its own balances and takes the new configuration.

use crate::resident::Resident;
use emerge_ref::swap::{preflight, Live, Rejected};

/// Stages `patch` against revision `base`, preflights it on the device's balances and commits it
/// between ticks. Returns the pause (preflight and commit, waited for) in seconds, and the compile
/// time apart from it.
pub fn hotswap(live: &mut Live, r: &mut Resident, patch: &str, base: u64) -> Result<(f64, f64), Rejected> {
    let mut staged = live.stage(patch, base)?;
    let t = std::time::Instant::now();
    let ns = r.ns();
    let (cells, amounts, ids) = r.balances();
    preflight(&staged.config, &staged.world, cells.chunks(ns), ids.into_iter().zip(amounts.chunks(ns)))?;
    r.apply_config(&mut staged.world);
    r.sync();
    let compile = staged.compile_s;
    live.advance(staged);
    Ok((t.elapsed().as_secs_f64(), compile))
}
