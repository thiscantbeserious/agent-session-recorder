//! Build script for AGR - embeds git commit hash for dev builds
//!
//! When the `release` feature is NOT set (default dev builds):
//! - Emits `VERGEN_GIT_SHA` environment variable with the commit hash
//!
//! When the `release` feature IS set (CI/official builds):
//! - Does not emit git info (clean version string)

fn main() {
    // Only emit git info when NOT building with --features release
    #[cfg(not(feature = "release"))]
    {
        use vergen_gitcl::{Emitter, GitclBuilder};

        // Configure git info - we only need the SHA
        let git = GitclBuilder::default()
            .sha(true)
            .build()
            .expect("Failed to configure git info");

        // Emit the environment variables
        if let Err(e) = Emitter::default()
            .add_instructions(&git)
            .expect("Failed to add git instructions")
            .emit()
        {
            // If git info fails (e.g., not in a git repo), emit fallback value
            eprintln!("cargo:warning=Failed to get git info: {}", e);
            println!("cargo:rustc-env=VERGEN_GIT_SHA=unknown");
        }
    }

    // For release builds, emit nothing - the main.rs will use clean version
    #[cfg(feature = "release")]
    {
        // No git info for official release builds
    }
}
