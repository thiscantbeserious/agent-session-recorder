//! Sync engine, decides, pushes and pulls sessions through a backend.

pub(in crate::sync) mod fail;
pub mod index;
mod item;
pub mod lock;
pub(super) mod observe;
mod pack_load;
mod pack_pull;
mod pack_push;
pub(super) mod pack_run;
pub(super) mod plan;
mod pull;
mod push;
pub mod queue;
mod record;
mod remote;
pub(super) mod run;
pub(super) mod scan;
pub mod select;
