//! `agr sync migrate`: moves the per-session gists of the old layout into packs.

mod recognise;

pub use recognise::{recognise, Kind, LegacyGist};
