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
//! - vergen-gitcl crate is available and configured in Cargo.toml.
//! - Git info may be unavailable (e.g., building from tarball); vergen handles this gracefully.

use vergen_gitcl::{BuildBuilder, Emitter, GitclBuilder};

fn main() {
    let build = BuildBuilder::default()
        .build_timestamp(true)
        .build()
        .expect("Failed to build vergen build instructions");
    let git = GitclBuilder::default()
        .sha(true) // short hash
        .build()
        .expect("Failed to build vergen git instructions");

    Emitter::default()
        .add_instructions(&build)
        .expect("Failed to add build instructions")
        .add_instructions(&git)
        .expect("Failed to add git instructions")
        .emit()
        .expect("Failed to emit build info");
}
