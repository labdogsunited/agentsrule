//! CPU reference runner for the Emergent core schedule.
//!
//! This is the first executable slice: cells and entities with conserved inventories,
//! run on the four phases of `core.md` §6.1 as the creatures application uses them
//! (spec §7). `compile` builds a world from an `.em` file, and tests also build worlds
//! in Rust. Tests in `tests/` state exact expected states, so spec changes have
//! something concrete to keep.
//!
//! Covered: element ledger and audits; upkeep (active genes, signals in use, retention,
//! carried units) and attempt fees through the payment route; minds (inventory and cell
//! senses, `weigh` and `product` genes, `keep`, `mind_steps`, expression of genes and
//! modules) driving action sizes, thresholds and reaction speeds; two-stage pro-rata
//! arbitration of entity and world transfers together, with Philox tie-breaking, overflow
//! sinks and diffusion; entity reactions in genome order; world reactions after transfers;
//! waste venting; disposal before division; newborns acting next tick; continuous 2D space
//! with a grid of cells, `move`, `turn`, the `gradient` sense and child placement.
//!
//! The `emerge` binary formats `.em` files (`emerge fmt`) and runs them (`emerge run`).
//!
//! Not covered yet: see `DISCREPANCIES.md`.

pub mod arbitrate;
pub mod chem;
pub mod compile;
pub mod mind;
pub mod philox;
pub mod world;

pub use chem::{Chemistry, Mu, U};
pub use mind::{Curve, Gene, Genome, Input, Sense, Slot, ONE};
pub use world::{Cause, End, Entity, Event, Law, Sink, Store, Transfer, Upkeep, World};
