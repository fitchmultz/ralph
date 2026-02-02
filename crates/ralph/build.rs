//! Build script for Ralph CLI.
//!
//! Responsibilities:
//! - Capture build-time information (git commit hash, build timestamp).
//! - Emit environment variables for use in the compiled binary.
//!
//! Not handled here:
//! - Version bumping or release management.
//! - Code generation or asset embedding.
//!
//! Invariants/assumptions:
//! - vergen crate is available and configured in Cargo.toml.
//! - Git info may be unavailable (e.g., building from tarball); vergen handles this gracefully.

use vergen::EmitBuilder;

fn main() {
    EmitBuilder::builder()
        .build_timestamp()
        .git_sha(true) // short hash
        .emit()
        .expect("Failed to emit build info");
}
