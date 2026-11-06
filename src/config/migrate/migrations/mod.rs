//! Versioned config migrations.
//!
//! Each migration lives in its own file (`v1.rs`, `v2.rs`, …) and exposes:
//! - `pub const VERSION: u32` — the target version this migration produces
//! - `pub fn migrate(root: &mut Table, result: &mut MigrateResult)` — the migration logic
//!
//! To add a new migration:
//! 1. Create `vN.rs` with `VERSION` and `migrate()`
//! 2. Add `mod vN;` below
//! 3. Bump `CURRENT_VERSION` in the parent module

mod v1;
mod v2;
mod v3;
mod v4;
mod v5;

use crate::config::MigrateResult;
use toml_edit::Table;

type MigrationFn = fn(&mut Table, &mut MigrateResult);

/// Migration registry. Each entry is (target_version, migration_fn).
/// A migration runs when the stored config version is < target_version.
#[rustfmt::skip]
const MIGRATIONS: &[(u32, MigrationFn)] = &[
    (v1::VERSION, v1::migrate),
    (v2::VERSION, v2::migrate),
    (v3::VERSION, v3::migrate),
    (v4::VERSION, v4::migrate),
    (v5::VERSION, v5::migrate),
];

/// Run all applicable migrations from `from_version` to latest.
pub fn run(from_version: u32, root: &mut Table, result: &mut MigrateResult) {
    for &(target, migrate_fn) in MIGRATIONS {
        if from_version < target {
            migrate_fn(root, result);
        }
    }
}
