//! The engine (spec §12–§14, `proposals/engine-api.md`): runs on the CPU runner or the GPU,
//! hot-swaps, the run journal, the JSON-RPC API and the MCP adapter.

pub mod api;
pub mod exit2;
pub mod journal;
pub mod mcp;
pub mod picture;
pub mod run;
pub mod serve;
