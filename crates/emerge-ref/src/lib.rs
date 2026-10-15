//! CPU reference runner for the Emergent core schedule.
//!
//! This is the first executable slice: cells and entities with conserved inventories,
//! run on the four phases of `core.md` §6.1 as the creatures application uses them
//! (spec §7). Worlds are built in Rust; there is no parser yet. Tests in `tests/`
//! state exact expected states, so spec changes have something concrete to keep.
//!
//! Covered: element ledger and audits, upkeep and attempt fees through the payment
//! route, two-stage pro-rata arbitration of entity and world transfers together,
//! overflow sinks, diffusion, entity reactions in genome order, world reactions after
//! transfers, waste venting, disposal before division, newborns acting next tick.
//!
//! Not covered yet: minds and signals (every gene fires every tick), senses, movement,
//! mutation, Philox tie-breaking, derived edges.

pub mod arbitrate;
pub mod chem;
pub mod world;

pub use chem::{Chemistry, Mu, U};
pub use world::{Cause, End, Entity, Event, Gene, Law, Sink, Store, Transfer, Upkeep, World};
