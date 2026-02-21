# TACTICAL FABRICATION DIRECTIVE

![Parallel Execution](assets/images/2026-02-07-parallel-execution.png)

> Historical implementation plan (pre direct-push rewrite). This document describes a superseded PR/merge-runner design and is retained only as archival context. For current behavior, see `docs/features/parallel-direct-push-rewrite-spec.md` and `docs/features/parallel.md`.

## 1. MISSION BRIEF (The Context Injection)

- **Target Objective:** Replace the parallel run-loop’s git worktree-based isolation with a simpler, more reliable “per-task workspace clone” model, while preserving the user-facing behavior of `ralph run loop --parallel N`. Parallel workers must be able to run simultaneously, run `make ci`, commit/push their branch, create PRs, and a merge runner must merge PRs and optionally AI-resolve conflicts.

- **The Why:**
  - Git worktrees are operationally fragile (pruning, reuse semantics, path conflicts, stale state, and “missing worktree” failure modes).
  - Reliable parallelism fundamentally requires *independent working directories* per worker; clones provide that without git worktree bookkeeping.
  - Merge conflict resolution currently assumes a worktree exists; this must become robust even if the worker workspace disappeared (crash/restart scenarios).

- **Strategy (First Principles):**
  1. **Isolation by clone**: each task gets an independent git clone directory (a “workspace”), checked out on a deterministic branch name (same `parallel.branch_prefix + task_id` behavior).
  2. **Supervisor orchestrates the same flow** as today: select tasks, spawn workers as subprocesses of the same `ralph` binary, create PRs, optionally run merge runner in the background, track state, and cleanup.
  3. **Merge runner becomes self-sufficient**: if a task workspace is missing, it can create an ephemeral merge workspace clone to run conflict resolution + push.
  4. **Minimal behavior change**: preserve `--parallel` semantics, PR creation/merge behavior, branch naming, and state file location. Only the underlying isolation mechanism changes.

- **Ordnance (Dependencies):** None (reuse existing git/gh CLI usage).

---

## 2. FABRICATION (The Heavy Weapons)

### A) Add a “workspace clone” git helper module

**File:** `crates/ralph/src/git/workspace.rs`

```rs
//! Git workspace helpers for parallel task isolation (clone-based).
//!
//! Responsibilities:
//! - Create and remove isolated git workspaces for parallel task execution.
//! - Compute the workspace root path using resolved configuration.
//! - Ensure a workspace exists for merge conflict resolution (create-on-demand).
//!
//! Not handled here:
//! - Task selection or worker orchestration (see `commands::run::parallel`).
//! - PR creation or merge operations (see `git/pr.rs`).
//! - Merge conflict resolution logic (see `commands::run::parallel::merge_runner`).
//!
//! Invariants/assumptions:
//! - `git` is available and the repo root is valid.
//! - Each workspace is an independent clone with its own working directory.
//! - Workspaces are disposable and may be recreated as needed.

use crate::contracts::Config;
use crate::git::error::git_base_command;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceSpec {
    pub task_id: String,
    pub path: PathBuf,
    pub branch: String,
}

pub(crate) fn workspace_root(repo_root: &Path, cfg: &Config) -> PathBuf {
    let root = cfg
        .parallel
        .workspace_root
        .clone()
        .unwrap_or_else(|| default_workspace_root(repo_root));
    if root.is_absolute() {
        root
    } else {
        repo_root.join(root)
    }
}

fn default_workspace_root(repo_root: &Path) -> PathBuf {
    let repo_name = repo_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repo");
    let parent = repo_root.parent().unwrap_or(repo_root);
    parent.join(".workspaces").join(repo_name).join("parallel")
}

pub(crate) fn create_workspace_at(
    repo_root: &Path,
    workspace_root: &Path,
    task_id: &str,
    base_branch: &str,
    branch_prefix: &str,
) -> Result<WorkspaceSpec> {
    let trimmed_id = task_id.trim();
    if trimmed_id.is_empty() {
        bail!("workspace task_id must be non-empty");
    }

    let branch = format!("{}{}", branch_prefix, trimmed_id);
    let path = workspace_root.join(trimmed_id);

    fs::create_dir_all(workspace_root).with_context(|| {
        format!(
            "create workspace root directory {}",
            workspace_root.display()
        )
    })?;

    // Always prefer a clean, deterministic workspace.
    if path.exists() {
        fs::remove_dir_all(&path)
            .with_context(|| format!("remove existing workspace {}", path.display()))?;
    }

    clone_repo_from_local(repo_root, &path)?;
    retarget_origin_to_real_origin(repo_root, &path)?;
    checkout_branch_from_base(&path, &branch, base_branch)?;

    Ok(WorkspaceSpec {
        task_id: trimmed_id.to_string(),
        path,
        branch,
    })
}

pub(crate) fn ensure_workspace_exists(repo_root: &Path, workspace_path: &Path) -> Result<()> {
    if workspace_path.exists() {
        return Ok(());
    }
    let parent = workspace_path
        .parent()
        .context("workspace_path must have a parent directory")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "create workspace parent directory {}",
            parent.display()
        )
    })?;
    clone_repo_from_local(repo_root, workspace_path)?;
    retarget_origin_to_real_origin(repo_root, workspace_path)?;
    Ok(())
}

pub(crate) fn remove_workspace(spec: &WorkspaceSpec, force: bool) -> Result<()> {
    if !spec.path.exists() {
        return Ok(());
    }
    if force {
        fs::remove_dir_all(&spec.path)
            .with_context(|| format!("remove workspace {}", spec.path.display()))?;
        return Ok(());
    }
    fs::remove_dir(&spec.path).with_context(|| format!("remove workspace {}", spec.path.display()))
}

fn clone_repo_from_local(repo_root: &Path, dest: &Path) -> Result<()> {
    let output = git_base_command(repo_root)
        .arg("clone")
        .arg("--no-hardlinks")
        .arg(".")
        .arg(dest)
        .output()
        .with_context(|| format!("run git clone into {}", dest.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git clone failed: {}", stderr.trim());
    }
    Ok(())
}

fn checkout_branch_from_base(workspace_path: &Path, branch: &str, base_branch: &str) -> Result<()> {
    // Reset/force branch locally to base_branch (workspace-local); this matches the previous
    // "reset to base" behavior from worktrees.
    let output = git_base_command(workspace_path)
        .arg("checkout")
        .arg("-B")
        .arg(branch)
        .arg(base_branch)
        .output()
        .with_context(|| format!("run git checkout -B in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git checkout -B failed: {}", stderr.trim());
    }
    Ok(())
}

fn retarget_origin_to_real_origin(repo_root: &Path, workspace_path: &Path) -> Result<()> {
    let origin = origin_url(repo_root).context("resolve origin remote url for workspace")?;
    let output = git_base_command(workspace_path)
        .arg("remote")
        .arg("set-url")
        .arg("origin")
        .arg(origin.trim())
        .output()
        .with_context(|| format!("set workspace origin url in {}", workspace_path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git remote set-url origin failed: {}", stderr.trim());
    }
    Ok(())
}

fn origin_url(repo_root: &Path) -> Result<String> {
    // Prefer push URL if configured.
    let try_get = |args: &[&str]| -> Result<Option<String>> {
        let output = git_base_command(repo_root)
            .args(args)
            .output()
            .with_context(|| format!("run git {} in {}", args.join(" "), repo_root.display()))?;
        if !output.status.success() {
            return Ok(None);
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((!value.is_empty()).then_some(value))
    };

    if let Some(url) = try_get(&["remote", "get-url", "--push", "origin"])? {
        return Ok(url);
    }
    if let Some(url) = try_get(&["remote", "get-url", "origin"])? {
        return Ok(url);
    }

    bail!(
        "No 'origin' remote configured; parallel mode requires a pushable origin remote."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Config, ParallelConfig};
    use crate::testsupport::git as git_test;
    use tempfile::TempDir;

    #[test]
    fn workspace_root_uses_repo_root_for_relative_path() {
        let cfg = Config {
            parallel: ParallelConfig {
                workspace_root: Some(PathBuf::from(".ralph/workspaces/custom")),
                ..ParallelConfig::default()
            },
            ..Config::default()
        };
        let repo_root = PathBuf::from("/tmp/ralph-test");
        let root = workspace_root(&repo_root, &cfg);
        assert_eq!(
            root,
            PathBuf::from("/tmp/ralph-test/.ralph/workspaces/custom")
        );
    }

    #[test]
    fn workspace_root_accepts_absolute_path() {
        let cfg = Config {
            parallel: ParallelConfig {
                workspace_root: Some(PathBuf::from("/tmp/ralph-workspaces")),
                ..ParallelConfig::default()
            },
            ..Config::default()
        };
        let repo_root = PathBuf::from("/tmp/ralph-test");
        let root = workspace_root(&repo_root, &cfg);
        assert_eq!(root, PathBuf::from("/tmp/ralph-workspaces"));
    }

    #[test]
    fn workspace_root_defaults_outside_repo() {
        let cfg = Config {
            parallel: ParallelConfig::default(),
            ..Config::default()
        };
        let repo_root = PathBuf::from("/tmp/ralph-test");
        let root = workspace_root(&repo_root, &cfg);
        assert_eq!(root, PathBuf::from("/tmp/.workspaces/ralph-test/parallel"));
    }

    #[test]
    fn create_and_remove_workspace_round_trips() -> Result<()> {
        let temp = TempDir::new()?;
        git_test::init_repo(temp.path())?;
        std::fs::write(temp.path().join("init.txt"), "init")?;
        git_test::commit_all(temp.path(), "init")?;
        // parallel mode requires an origin remote
        git_test::git_run(temp.path(), &["remote", "add", "origin", "https://example.com/repo.git"])?;

        let base_branch =
            git_test::git_output(temp.path(), &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let root = temp.path().join(".ralph/workspaces/parallel");

        let spec = create_workspace_at(temp.path(), &root, "RQ-0001", &base_branch, "ralph/")?;
        assert!(spec.path.exists(), "workspace path should exist");

        remove_workspace(&spec, true)?;
        Ok(())
    }
}
```

### B) Wire the new module into `git/mod.rs`

**File:** `crates/ralph/src/git/mod.rs`

Replace the worktree module export/re-exports with workspace:

```rs
pub mod workspace;

// ...
pub(crate) use workspace::{WorkspaceSpec, create_workspace_at, ensure_workspace_exists, remove_workspace, workspace_root};
```

And remove:

```rs
pub mod worktree;
pub(crate) use worktree::{WorktreeSpec, create_worktree_at, remove_worktree, worktree_root};
```

(If any other code uses `worktree`, either migrate it or keep `worktree.rs` temporarily but ensure parallel flow no longer imports it.)

---

### C) Update ParallelConfig to remove “worktree” terminology but keep backward compatibility

**File:** `crates/ralph/src/contracts/config.rs`

In `ParallelConfig`, replace:

```rs
pub worktree_root: Option<PathBuf>,
```

with:

```rs
/// Root directory for parallel workspaces (relative to repo root if not absolute).
///
/// Back-compat: also accepts the legacy key `worktree_root`.
#[serde(alias = "worktree_root")]
pub workspace_root: Option<PathBuf>,
```

Update `merge_from` accordingly:

```rs
if other.workspace_root.is_some() {
    self.workspace_root = other.workspace_root;
}
```

Update `Config::default()` parallel defaults to set `workspace_root: None` and remove `worktree_root`.

---

### D) Update the parallel state file structs to store “workspace_path” (alias “worktree_path”)

**File:** `crates/ralph/src/commands/run/parallel/state.rs`

Key edits:
- Replace `use crate::git::WorktreeSpec;` with `use crate::git::WorkspaceSpec;`
- Rename fields in records (keeping serde aliases):

```rs
pub(crate) struct ParallelTaskRecord {
    pub task_id: String,
    #[serde(alias = "worktree_path")]
    pub workspace_path: String,
    pub branch: String,
    pub pid: Option<u32>,
}

impl ParallelTaskRecord {
    pub fn new(task_id: &str, workspace: &WorkspaceSpec, pid: u32) -> Self {
        Self {
            task_id: task_id.to_string(),
            workspace_path: workspace.path.to_string_lossy().to_string(),
            branch: workspace.branch.clone(),
            pid: Some(pid),
        }
    }
}
```

And in `ParallelPrRecord`:

```rs
#[serde(default, alias = "worktree_path")]
pub workspace_path: Option<String>,

pub fn workspace_path(&self) -> Option<PathBuf> {
    self.workspace_path.as_ref().map(PathBuf::from)
}
```

Ensure the tests in this module are updated to reflect the new JSON shape but still accept old persisted state files (because of aliases).

---

### E) Replace worktree usage in the parallel supervisor with workspace clones

**File:** `crates/ralph/src/commands/run/parallel/mod.rs`

Key changes:
- Update module docs: “git worktrees” → “isolated git workspaces (clones)”.
- Replace `git::WorktreeSpec` with `git::WorkspaceSpec`.
- Replace `git::create_worktree_at` with `git::create_workspace_at`.
- Replace `git::remove_worktree` with `git::remove_workspace`.
- Replace `git::worktree_root(...)` with `git::workspace_root(...)`.

Example core swap (illustrative):

```rs
let workspace = git::create_workspace_at(
    &resolved.repo_root,
    &settings.workspace_root,
    &task_id,
    &base_branch,
    &settings.branch_prefix,
)?;
sync_ralph_state(&resolved.repo_root, &workspace.path)?;
let child = spawn_worker(resolved, &workspace.path, &task_id, &opts.agent_overrides, opts.force)?;
let record = state::ParallelTaskRecord::new(&task_id, &workspace, child.id());
```

Update the in-memory maps (`completed_worktrees` → `completed_workspaces`) for clarity, or keep the name but the type must change.

Update cleanup calls to:

```rs
git::remove_workspace(&workspace, true)?;
```

Update any state-record path usage from `record.worktree_path` to `record.workspace_path`.

Update `ParallelSettings` struct fields:

```rs
workspace_root: PathBuf,
```

and in `resolve_parallel_settings`:

```rs
workspace_root: git::workspace_root(&resolved.repo_root, &resolved.config),
```

---

### F) Make merge conflict resolution robust without “workspace must exist”

**File:** `crates/ralph/src/commands/run/parallel/merge_runner.rs`

Goals:
- Rename `worktree_root` parameter to `workspace_root`.
- If the workspace directory for a task is missing, create it on demand using `git::ensure_workspace_exists`, then perform conflict workflow.

Replace the top of `resolve_conflicts`:

```rs
let worktree_path = worktree_root.join(task_id);
if !worktree_path.exists() { bail!(...) }
```

with something like:

```rs
let workspace_path = workspace_root.join(task_id);
git::ensure_workspace_exists(&resolved.repo_root, &workspace_path)
    .with_context(|| format!("ensure merge workspace exists at {}", workspace_path.display()))?;
```

Also fix the checkout logic so it works in a fresh clone:

```rs
git_run(&workspace_path, &["fetch", "origin"])?;
let head_ref = format!("origin/{}", pr.head);
git_run(&workspace_path, &["checkout", "-B", &pr.head, &head_ref])?;

let base_ref = format!("origin/{}", pr.base);
git_run(&workspace_path, &["merge", &base_ref])?;
```

Finally, the rest of the AI conflict resolution flow can remain largely the same (prompt building, runner invocation, add/commit/push).

---

### G) Update docs and schema references (worktrees → workspaces/clones)

Files to update:
- `docs/workflow.md`: Parallel section should describe per-task workspace clones and the default root.
- `docs/cli.md`: `--parallel` description “worktrees” → “isolated git workspaces (clones)”.
- `docs/configuration.md`: `parallel.worktree_root` → `parallel.workspace_root` (mention legacy alias accepted if you keep it).

Schema:
- Run `make generate` after updating contracts so `schemas/config.schema.json` reflects `workspace_root` (and no longer lists `worktree_root`).
- Ensure any schema alignment tests still pass.

---

### H) Add `.ralph/cache/parallel/` to gitignore (optional but strongly recommended)

**File:** `.gitignore`

Add:

```
.ralph/cache/parallel/
```

This avoids parallel state showing up as untracked noise (especially important now that parallel mode is more actively used/restarted).

---

## 3. FIELD ADAPTATION PROTOCOLS (Instructions for the Executioner)

- **Integration Points (must search + fix):**
  - Search for `worktree_root`, `create_worktree_at`, `remove_worktree`, `WorktreeSpec`, and update all callsites.
  - Search for doc references to “worktrees” in docs and CLI help strings; update to “workspace clones” or “workspaces”.
  - Ensure `schemas/config.schema.json` is regenerated and committed.

- **Missing Assets / Unknown Code Paths:**
  - The codebase may have other worktree helpers used elsewhere; if they exist and are unrelated to parallel mode, you can keep them, but ensure **parallel mode no longer relies on git worktrees**.
  - If there is a migration/sanity-check system that expects `worktree_root`, decide whether to:
    - keep compatibility via serde alias (recommended), or
    - add an explicit migration, or
    - accept a breaking change (must be documented loudly in docs/help).

- **Sanity Checks / Reliability Requirements:**
  - Merge runner must not hard-fail merely because the worker workspace is missing; it should be able to recreate a merge workspace and proceed.
  - Verify that workers can still:
    - run `make ci`
    - commit/push their branch
    - create PRs
  - Verify merge runner can still:
    - merge clean PRs
    - detect dirty state and run AI conflict resolution
    - push conflict-resolution commits back to the PR branch

- **Testing Requirements:**
  - Update or replace existing worktree unit tests with workspace tests.
  - Update parallel supervisor unit tests that reference worktree paths/structs.
  - Ensure `make ci` passes.

---

## 4. THE KILL CHAIN (Execution Sequence)

1. **Implement workspace module:** Add `git/workspace.rs` and wire it in `git/mod.rs`.
2. **Config contract update:** Rename `parallel.worktree_root` → `parallel.workspace_root` (add serde alias for backward compatibility).
3. **State file update:** Change state records to store `workspace_path` (alias legacy `worktree_path`).
4. **Parallel supervisor swap:** Replace all worktree creation/removal usage with workspace clone creation/removal.
5. **Merge runner hardening:** Ensure conflict resolution creates a workspace if missing; adjust checkout logic for fresh clones (`checkout -B <branch> origin/<branch>`).
6. **Docs + CLI text:** Update `docs/cli.md`, `docs/workflow.md`, `docs/configuration.md` to remove worktree claims.
7. **Schema regeneration:** Run `make generate`, commit updated `schemas/config.schema.json`.
8. **Sweep & Clear:** Grep for “worktree” references and ensure parallel mode contains none (except back-compat alias documentation if retained).
9. **Verify:** Run `make ci`. Then do a manual smoke test:
   - `ralph run loop --parallel 2 --max-tasks 2` in a test repo with `gh` configured.

<chatName="Replace parallel worktrees with clone-based workspaces"/>
