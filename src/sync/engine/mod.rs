//! Sync engine, decides, pushes and pulls sessions through a backend.

mod fail;
pub mod index;
pub(super) mod lock;
pub(super) mod plan;
mod pull;
mod push;
mod record;
pub(super) mod run;
pub(super) mod scan;
