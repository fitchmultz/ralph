# Releasing Ralph

Purpose: Document the complete local release workflow for Ralph without GitHub Actions.

## Overview

Ralph uses a local-only release process that leverages the GitHub CLI (`gh`) for release creation and artifact publishing. This approach avoids GitHub Actions while still providing automated versioning, changelog generation, and binary distribution.

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

3. **Clean working directory**
   - All changes committed
   - On `main` branch
   - Remote is accessible

## Release Process

### 1. Prepare for Release

Ensure all changes for the release are merged to `main`:

```bash
git checkout main
git pull origin main
```

### 2. Run the Release Script

The release script handles the entire process:

```bash
# Full release (e.g., version 0.2.0)
scripts/release.sh 0.2.0

# Dry run first (recommended for testing)
RELEASE_DRY_RUN=1 scripts/release.sh 0.2.0
```

### 3. What the Script Does

The release script performs these steps:

1. **Pre-release Validation**
   - Checks `gh` CLI is installed and authenticated
   - Validates working directory is clean (excluding `.ralph/*`)
   - Confirms on `main` branch
   - Verifies remote is accessible
   - Checks tag doesn't already exist

2. **Version Updates**
   - Updates version in `crates/ralph/Cargo.toml`
   - Updates `CHANGELOG.md` with new version section

3. **CI Validation**
   - Runs `make ci` to ensure everything passes

4. **Build Artifacts**
   - Builds release binary for current platform
   - Creates tarball (`ralph-{version}-{platform}.tar.gz`)
   - Generates SHA256 checksum

5. **Git Operations**
   - Commits version and changelog changes
   - Creates annotated git tag (`v{version}`)
   - Pushes commit and tag to remote

6. **GitHub Release**
   - Creates GitHub release using `gh release create`
   - Uploads release artifacts (binaries + checksums)
   - Uses template from `.github/release-notes-template.md`

### 4. Verify the Release

After the script completes:

```bash
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
sed -i.bak -E "s/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/version = \"$VERSION\"/" crates/ralph/Cargo.toml
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
```

### Step 4: Build Artifacts

```bash
# Build for current platform
scripts/build-release-artifacts.sh $VERSION

# Or build for all platforms (requires cross-compilation targets)
scripts/build-release-artifacts.sh --all $VERSION
```

### Step 5: Commit and Tag

```bash
git add crates/ralph/Cargo.toml CHANGELOG.md
git commit -m "Release v$VERSION"
git tag -a "v$VERSION" -m "Release v$VERSION"
git push origin main
git push origin "v$VERSION"
```

### Step 6: Create GitHub Release

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

# Revert changes to Cargo.toml and CHANGELOG.md
git checkout -- crates/ralph/Cargo.toml CHANGELOG.md

# Or reset to last commit if changes were committed
git reset --hard HEAD~1
```

## Troubleshooting

### "gh CLI is not authenticated"

Run `gh auth login` and follow the prompts.

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

## Release Checklist

Before running the release script:

- [ ] All changes for this release are merged to `main`
- [ ] `make ci` passes locally
- [ ] `CHANGELOG.md` has content under `## [Unreleased]`
- [ ] Version follows semver (e.g., `0.2.0`)
- [ ] `gh auth status` shows you're authenticated
- [ ] Working directory is clean (`git status`)
- [ ] On `main` branch (`git branch --show-current`)

After the release:

- [ ] GitHub release page shows the new version
- [ ] Release artifacts are attached
- [ ] Checksums are available
- [ ] Installation instructions work

## Makefile Targets

| Target | Description |
|--------|-------------|
| `make release VERSION=0.2.0` | Run full release process |
| `make release-dry-run VERSION=0.2.0` | Test release without side effects |
| `make release-artifacts` | Build release artifacts only |
| `make build-release` | Build release binary |

## Related Files

- `scripts/release.sh` - Main release orchestration script
- `scripts/build-release-artifacts.sh` - Multi-platform build script
- `.github/release-notes-template.md` - Release notes template
- `CHANGELOG.md` - Version history
- `crates/ralph/Cargo.toml` - Package manifest
