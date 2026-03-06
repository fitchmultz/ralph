# Releasing Ralph

![Release Process](assets/images/2026-02-07-release-process.png)

Purpose: Document Ralph's complete local-only release workflow, including crates.io publication of the `ralph-agent-loop` package that installs the `ralph` executable.

## Overview

Ralph uses a local-only release process that publishes the crate to crates.io and then creates the GitHub release with `gh`. This keeps distribution aligned across Rust ecosystem install flows (`cargo install ralph-agent-loop`) and the project’s tarball releases while avoiding GitHub Actions.

The release script is intentionally rerunnable after a partial release. If crates.io already has the requested version, `scripts/release.sh` skips the publish step and continues with the Git/GitHub release work instead of failing on an already-published crate.

Release order matters:

1. Publish `ralph-agent-loop` to crates.io.
2. Build release artifacts and push git metadata.
3. Create the GitHub release and upload assets.

That ordering means the irreversible step happens first. If crates.io publication succeeds and a later step fails, rerun the release flow with `RALPH_RELEASE_SKIP_PUBLISH=1` after fixing the issue.

## Prerequisites

Before creating a release, ensure you have:

1. **GitHub CLI (`gh`) installed and authenticated**

   ```bash
   # Install gh CLI
   brew install gh  # macOS
   # or visit: https://cli.github.com/

   # Authenticate
   gh auth login
   ```

2. **Rust toolchain installed**

   ```bash
   # Verify cargo is available
   cargo --version
   ```

3. **crates.io authentication configured**

   ```bash
   # Create or refresh your crates.io token
   cargo login
   ```

   The release script checks for either `CARGO_REGISTRY_TOKEN` or the standard Cargo credentials file before attempting publication.

4. **Clean working directory**
   - All changes committed
   - On `main` branch
   - Remote is accessible

5. **GNU Make >= 4 available** (for wrapper targets)
   - On macOS, install with `brew install make`
   - Use `gmake ...` unless `make --version` reports GNU Make

## Changelog Generation

Ralph uses [git-cliff](https://git-cliff.org/) to automatically generate changelog entries from commits following the `RQ-####: <summary>` pattern.

### Install git-cliff

```bash
cargo install git-cliff
```

### Preview Changes

Before releasing, preview what entries will be generated:

```bash
scripts/generate-changelog.sh --dry-run
```

Convenience wrapper:

```bash
make changelog-preview
```

### Generate Changelog

To update the `CHANGELOG.md` Unreleased section with entries from commits:

```bash
scripts/generate-changelog.sh
```

Convenience wrapper:

```bash
make changelog
```

This will:
- Parse commits since the last tag
- Categorize them into Added/Changed/Fixed/Removed/Security sections
- Insert them into the `## [Unreleased]` section
- Preserve any existing manual entries

### Commit Message Patterns

The changelog generator categorizes commits based on the first word after `RQ-####:`:

| Pattern | Section | Examples |
|---------|---------|----------|
| `Add`, `Implement`, `Create`, `Introduce`, `Enable` | **Added** | `RQ-0042: Add new command` |
| `Fix`, `Bugfix`, `Resolve`, `Correct` | **Fixed** | `RQ-0043: Fix race condition` |
| `Update`, `Change`, `Refactor`, `Modify`, `Enhance`, `Improve`, `Redesign`, `Rework`, `Assess`, `Verify`, `Design`, `Integrate` | **Changed** | `RQ-0044: Refactor module` |
| `Remove`, `Delete`, `Deprecate` | **Removed** | `RQ-0045: Remove deprecated API` |
| `Security` | **Security** | `RQ-0046: Security patch` |

### Manual Editing

After running `make changelog`, you can manually edit `CHANGELOG.md` to:
- Add high-level descriptions of features
- Reorganize entries between sections
- Add context or migration notes

The release script will preserve these manual edits when versioning the changelog.

## Release Process

### 1. Prepare for Release

Ensure all changes for the release are merged to `main`:

```bash
git checkout main
git pull origin main
```

### 2. Generate Changelog (Optional)

The release script automatically generates changelog entries, but you can preview or manually update:

```bash
# Preview what will be added
scripts/generate-changelog.sh --dry-run

# Or manually generate entries
scripts/generate-changelog.sh
# Then edit CHANGELOG.md as needed
```

### 3. Run the Release Script

The release script handles the entire process:

```bash
# Full release (e.g., version 0.2.0)
scripts/release.sh 0.2.0

# Dry run first (recommended for testing)
RELEASE_DRY_RUN=1 scripts/release.sh 0.2.0

# Retry after crates.io publication already succeeded
RALPH_RELEASE_SKIP_PUBLISH=1 scripts/release.sh 0.2.0
```

Equivalent Makefile wrappers:

```bash
make release VERSION=0.2.0
make release-dry-run VERSION=0.2.0
# macOS/Homebrew users: gmake release VERSION=0.2.0
```

### 4. What the Script Does

The release script performs these steps:

1. **Pre-release Validation**
   - Checks `gh` CLI is installed and authenticated
   - Checks crates.io publish credentials are available unless publication is explicitly skipped
   - Validates working directory is clean (excluding `.ralph/*`)
   - Confirms on `main` branch
   - Verifies remote is accessible
   - Checks tag doesn't already exist

2. **Version Updates**
   - Updates the canonical `VERSION` file
   - Synchronizes derived version metadata in `crates/ralph/Cargo.toml`, the Xcode project, and app CLI compatibility checks
   - Updates `CHANGELOG.md` with new version section

3. **CI Validation**
   - Runs `make ci` (or `RALPH_MAKE_CMD` override in script) to ensure everything passes
   - Verifies post-CI tracked changes are limited to release-expected files (`VERSION`, `Cargo.toml`, app version metadata, `CHANGELOG.md`, generated schemas)

4. **crates.io Publication**
   - Reviews packaged files with `cargo package --list -p ralph-agent-loop`
   - Runs `cargo publish --dry-run -p ralph-agent-loop --locked --allow-dirty`
   - Publishes `ralph-agent-loop` to crates.io, which installs the `ralph` executable

5. **Build Artifacts**
   - Builds release binary for current platform
   - Creates tarball (`ralph-{version}-{platform}.tar.gz`)
   - Generates SHA256 checksum

6. **Git Operations**
   - Commits version and changelog changes
   - Creates annotated git tag (`v{version}`)
   - Pushes commit and tag to remote

7. **GitHub Release**
   - Creates GitHub release using `gh release create`
   - Uploads release artifacts (binaries + checksums)
   - Uses template from `.github/release-notes-template.md`

### 5. Verify the Release

After the script completes:

```bash
# Verify the published crate
cargo install ralph-agent-loop
ralph --help

# View the release on GitHub
gh release view v0.2.0

# Or open in browser
gh release view v0.2.0 --web
```

## Manual Release (Without Script)

If you need more control, you can perform steps manually:

### Step 1: Update Version

```bash
VERSION=0.2.0
./scripts/versioning.sh sync --version "$VERSION"
```

### Step 2: Update Changelog

```bash
# Add new version section after ## [Unreleased]
today=$(date +%Y-%m-%d)
# Edit CHANGELOG.md manually or use sed
```

### Step 3: Run CI

```bash
make ci
# macOS/Homebrew users: gmake ci
```

### Step 4: Review and Publish the Crate

```bash
cargo package --list -p ralph-agent-loop
cargo publish --dry-run -p ralph-agent-loop --locked
cargo publish -p ralph-agent-loop --locked
```

Users install the published package with:

```bash
cargo install ralph-agent-loop
```

That command installs the `ralph` executable.

### Step 5: Build Artifacts

```bash
# Build for current platform
scripts/build-release-artifacts.sh $VERSION

# Or build for all platforms (requires cross-compilation targets)
scripts/build-release-artifacts.sh --all $VERSION
```

### Step 6: Commit and Tag

```bash
git add VERSION crates/ralph/Cargo.toml apps/RalphMac/RalphMac.xcodeproj/project.pbxproj apps/RalphMac/RalphCore/VersionValidator.swift CHANGELOG.md schemas/config.schema.json schemas/queue.schema.json
git commit -m "Release v$VERSION"
git tag -a "v$VERSION" -m "Release v$VERSION"
git push origin main
git push origin "v$VERSION"
```

### Step 7: Create GitHub Release

```bash
# Create release with notes from tag
gh release create "v$VERSION" \
  --verify-tag \
  --notes-file .github/release-notes-template.md

# Upload artifacts
gh release upload "v$VERSION" target/release-artifacts/*
```

## Supported Platforms

Release artifacts are built for:

| Platform | Architecture | Target Triple |
|----------|-------------|---------------|
| Linux | x86_64 | `x86_64-unknown-linux-gnu` |
| macOS | x86_64 | `x86_64-apple-darwin` |
| macOS | arm64 | `aarch64-apple-darwin` |

### Cross-Compilation Setup

To build for all platforms, install additional targets:

```bash
# macOS ARM64 (for building on Intel Macs)
rustup target add aarch64-apple-darwin

# macOS x86_64 (for building on Apple Silicon)
rustup target add x86_64-apple-darwin

# Linux (requires cross-compilation toolchain)
rustup target add x86_64-unknown-linux-gnu
```

**Note:** Cross-compilation to Linux from macOS requires additional tooling (e.g., `cross` or Docker). For simplicity, the release script builds only for the current platform by default.

## Build Artifacts Only

To build release artifacts without creating a release:

```bash
# Build for current platform
scripts/build-release-artifacts.sh

# Build for all platforms (if targets installed)
scripts/build-release-artifacts.sh --all

# Build with specific version
scripts/build-release-artifacts.sh 0.2.0
```

Artifacts are placed in `target/release-artifacts/`:

```
target/release-artifacts/
├── ralph-0.2.0-macos-arm64.tar.gz
├── ralph-0.2.0-macos-arm64.tar.gz.sha256
└── ...
```

## Dry Run Mode

Test the release process without making changes:

```bash
RELEASE_DRY_RUN=1 scripts/release.sh 0.2.0
```

This will:
- Print all actions that would be taken
- Skip git commits and tags
- Skip crates.io publication
- Skip GitHub release creation
- Skip file modifications

## Rollback

If a release fails or needs to be undone:

```bash
VERSION=0.2.0

# Delete local tag
git tag -d "v$VERSION"

# Delete remote tag
git push origin ":refs/tags/v$VERSION"

# Delete GitHub release
gh release delete "v$VERSION" --yes

# Revert version metadata and changelog changes
git checkout -- VERSION crates/ralph/Cargo.toml apps/RalphMac/RalphMac.xcodeproj/project.pbxproj apps/RalphMac/RalphCore/VersionValidator.swift CHANGELOG.md
```

If crates.io publication already succeeded, do not attempt to republish the same version. Fix the later failure, then rerun with:

```bash
RALPH_RELEASE_SKIP_PUBLISH=1 scripts/release.sh $VERSION
```

## Troubleshooting

### "gh CLI is not authenticated"

Run `gh auth login` and follow the prompts.

### "crates.io publish credentials not found"

Authenticate before retrying:

```bash
cargo login
# or export CARGO_REGISTRY_TOKEN=...
```

### "Not on main branch"

Releases must be created from the `main` branch:

```bash
git checkout main
git pull origin main
```

### "Working directory is not clean"

Commit or stash all changes before releasing:

```bash
git status  # See what's dirty
git add .
git commit -m "Prepare for release"
```

### "Tag already exists"

Delete the existing tag if it was created in error:

```bash
git tag -d v0.2.0
git push origin :refs/tags/v0.2.0
```

### CI Fails During Release

The release script runs `make ci` before creating the release. Fix any failures before retrying:

```bash
make ci  # See what's failing
# Fix issues
scripts/release.sh 0.2.0  # Retry
```

If GNU Make is not your default `make`, set:

```bash
RALPH_MAKE_CMD=gmake scripts/release.sh 0.2.0
```

### crates.io Publish Succeeds, Later Step Fails

The published version is immutable. After fixing the later failure, rerun the release flow without attempting a second publish:

```bash
RALPH_RELEASE_SKIP_PUBLISH=1 scripts/release.sh 0.2.0
```

## Release Checklist

Before running the release script:

- [ ] All changes for this release are merged to `main`
- [ ] `./scripts/versioning.sh check` passes
- [ ] `make ci` passes locally
- [ ] `cargo package --list -p ralph-agent-loop` succeeds
- [ ] `cargo publish --dry-run -p ralph-agent-loop --locked` succeeds
- [ ] `CHANGELOG.md` has content under `## [Unreleased]` (run `scripts/generate-changelog.sh` to generate)
- [ ] Changelog entries are categorized correctly (Added/Changed/Fixed/etc.)
- [ ] Version follows semver (e.g., `0.2.0`)
- [ ] `gh auth status` shows you're authenticated
- [ ] `cargo login` or `CARGO_REGISTRY_TOKEN` is configured
- [ ] Working directory is clean (`git status`)
- [ ] On `main` branch (`git branch --show-current`)

After the release:

- [ ] `cargo install ralph-agent-loop` succeeds
- [ ] `ralph --help` works from the installed crate
- [ ] GitHub release page shows the new version
- [ ] Release artifacts are attached
- [ ] Checksums are available
- [ ] Installation instructions work

## Convenience Makefile Wrappers

Scripts are the canonical release entrypoints. Makefile targets below wrap those scripts for discoverability.

| Target | Description |
|--------|-------------|
| `make release VERSION=0.2.0` | Run full release process (`scripts/release.sh`) |
| `make release-dry-run VERSION=0.2.0` | Test release without side effects |
| `make version-check` | Verify VERSION, Cargo, and app version metadata are synchronized |
| `make version-sync VERSION=0.2.0` | Synchronize all derived version metadata from one canonical semver |
| `make publish-check` | Review packaged files and run crates.io dry-run publication for `ralph-agent-loop` |
| `make publish-crate` | Publish `ralph-agent-loop` to crates.io after running `make publish-check` |
| `make release-artifacts` | Build release artifacts only |
| `make release-artifacts VERSION=0.2.0` | Build artifacts with explicit version |
| `make changelog` | Generate changelog entries from commits |
| `make changelog-preview` | Preview changelog changes without modifying files |
| `make changelog-check` | Check if changelog is up to date |
| `make docs` | Generate rustdocs in `target/doc` |

## Related Files

- `scripts/release.sh` - Main release orchestration script
- `scripts/build-release-artifacts.sh` - Multi-platform build script
- `scripts/generate-changelog.sh` - Changelog generation from commits
- `cliff.toml` - git-cliff configuration for RQ-#### pattern
- `.github/release-notes-template.md` - Release notes template
- `CHANGELOG.md` - Version history
- `VERSION` - Canonical semantic version for the repo
- `crates/ralph/Cargo.toml` - `ralph-agent-loop` package manifest for the `ralph` executable
