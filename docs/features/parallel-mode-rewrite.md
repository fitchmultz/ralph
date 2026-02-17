# Briefing: Ralph Parallel Execution Architecture Redesign

- Author: AI Agent (Claude)
- Date: 2026-02-17
- Audience: Mitch Fultz, Project Owner / Lead Developer
- Status: Investigation Complete, Design Finalized, Implementation Not Started
- Confidence: High (thorough codebase analysis + clear architecture decision)

 ────────────────────────────────────────────────────────────────────────────────

 1) Executive Summary

 1. User Concern: The current parallel execution feature requires too much manual intervention, especially
 around merge conflicts and queue synchronization. User has "no confidence" in it despite heavy daily use of
 sequential mode.
 2. Root Cause Identified: Current architecture uses completion signals (async, fragile), internal merge
 runner thread (limited intelligence), and complex state tracking. Queue file changes don't flow naturally
 through git.
 3. Solution Designed: New architecture with dumb coordinator, full agent workers, and spawned merge agents.
 Queue updates happen locally after PR merge via ralph task done CLI. No completion signals. No complex
 state.
 4. Key Insight: Workers run complete phase cycles (per user config: 3 phases × 2 iterations) just like
 sequential mode. They're isolated in git workspace clones. Merge is handled by separate agent subprocess
 with full AI capabilities.
 5. Eliminates: Completion signal system (~1,000 lines), complex state reconciliation, internal merge runner
 thread, queue sync via signals, "finished_without_pr" tracking.
 6. Blocker: None. Design is finalized and ready for implementation.
 7. Next Action 1: Create new ralph run merge-agent command (subprocess that merges PR + marks task done
 locally).
 8. Next Action 2: Simplify orchestration.rs coordinator to spawn workers/agents and wait (remove complex
 state management).
 9. Next Action 3: Remove completion signal system and simplify state file to minimal PID tracking only.

 ────────────────────────────────────────────────────────────────────────────────

 1) Original Request

### 2.1 Verbatim User Request

 │ "I am constantly running loops... One thing I do not have any confidence in, though, is the parallel
 │ feature. Reason being, it still requires too much manual intervention. It's just not intelligent enough,
 │ merging at the end, all that kind of stuff. I just don't have any confidence in it. I want you to
 │ thoroughly investigate this."

 │ "There's almost always merge conflicts because of the queue. So I've had to just outright ignore any of
 │ the ralph files to make sure that they don't prevent... so they don't cause merge conflicts."

 │ "If workspace A finishes before workspace B. Are we just going to sit around and wait for workspace B to
 │ complete?... At this point, I think workspace B, let's say it's still in progress. We have workspace A's
 │ done. That's when we pull out our merge agent... once it executes the merge... workspace A agents have
 │ been done. The merge for Workspace A's PR has been done, and by now, hopefully, Workspace B has been
 │ done, so then we do the process again."

### 2.2 Interpreted Goal

 Desired End State:
 A parallel execution system where:
- Multiple tasks run concurrently in isolated git workspace clones
- Each worker runs the FULL agent loop (phases/iterations per config) exactly like sequential mode
- No special "parallel worker" logic - workers are unchanged from single-task mode
- When worker succeeds, coordinator creates PR and spawns merge agent (full AI subprocess)
- Merge agent merges PR and marks task done locally via ralph task done
- Queue updates are local (not via git merge), avoiding queue.json conflicts
- System is debuggable - can run merge agent manually if stuck
- No complex state files - minimal tracking only

 Success Criteria:
 1. User can run ralph run loop --parallel 2 and tasks execute in parallel
 2. Queue updates happen automatically without manual intervention
 3. Merge conflicts are resolved by intelligent agent, not simple text merge
 4. No "stuck" states requiring manual state file edits
 5. Can debug by running merge agent manually: ralph run merge-agent --pr 123 --task RQ-0001

 Constraints:
- Must work with user's current config (3 phases, 2 iterations, different runners per phase)
- .ralph/queue.json is gitignored (cannot track queue changes via git merge)
- Must use existing runner infrastructure (claude, codex, opencode, etc.)
- CI gate runs per-worker, not after merge (already passing before PR created)

 ────────────────────────────────────────────────────────────────────────────────

 1) Scope and Non-Goals

### In Scope

- Redesign parallel execution architecture
- Create new ralph run merge-agent subprocess command
- Simplify coordinator to spawn workers/agents and wait
- Remove completion signal system entirely
- Simplify state file to minimal tracking
- Update merge conflict resolution to use full agent loop
- Ensure queue updates happen locally after PR merge
- Support user's existing phase/iteration configuration

### Out of Scope (Explicit)

- No changes to sequential (run one) execution
- No changes to worker agent loop (phases implementation)
- No changes to task selection logic
- No changes to git workspace creation/cloning
- No changes to PR creation via gh CLI
- No support for tracking queue.json in git (remains gitignored)
- No automatic rebase of in-flight workspaces (worker bases on original commit)
- No changes to notification system
- No changes to webhook system

 ────────────────────────────────────────────────────────────────────────────────

 1) Key Context

### Systems Involved

 Ralph CLI: Rust-based task queue system for AI agents. Repository at /Users/mitchfultz/Projects/AI/ralph.

 Parallel Execution Module: Located at crates/ralph/src/commands/run/parallel/. Currently ~3,500 lines
 including:
- orchestration.rs: Main supervisor loop (complex state machine)
- worker.rs: Worker subprocess spawning
- merge_runner/: Internal merge runner thread (limited AI)
- state.rs: Complex state tracking (tasks, PRs, finished_without_pr, blockers)
- completion.rs: Completion signal handling (fragile)
- cleanup_guard.rs: Resource cleanup

 Worker Process: Subprocess running ralph run one --id <TASK> --parallel-worker in isolated git workspace
 clone.

 Merge Agent (New): Subprocess that will run full AI loop to:
- Check out PR branch
- Merge to main (resolving conflicts if needed)
- Run ralph task done <TASK> in main repo
- Push and complete merge

 Git Workspaces: Isolated repo clones created at parallel.workspace_root (default:
 <repo-parent>/.workspaces/<repo-name>/parallel/<task-id>).

 Phase System: Tasks execute in phases:
- Phase 1: Planning
- Phase 2: Implementation + CI gate
- Phase 3: Review + completion

 User's Current Config (/Users/mitchfultz/Projects/AI/ralph/.ralph/config.jsonc):
- 3 phases, 2 iterations
- Phase 1: opencode + glm-5
- Phase 2: opencode + k2p5
- Phase 3: opencode + k2p5
- CI gate: make macos-ci

### Environment

- OS: macOS (Unix-like)
- Repo: Git repository with GitHub origin
- CLI Tool: gh CLI required for PR operations
- Queue File: .ralph/queue.json (gitignored)
- Done File: .ralph/done.json (gitignored)

### Definitions

- Coordinator: Main ralph run loop --parallel process. Spawns workers and merge agents.
- Worker: Subprocess running full task execution in workspace clone.
- Merge Agent: New subprocess that handles PR merge + queue update.
- Completion Signal: Current mechanism (to be removed) where worker writes signal file, coordinator applies
 after merge.
- Workspace: Git clone of repo at specific commit, isolated branch for task.

 ────────────────────────────────────────────────────────────────────────────────

 1) Work Completed

 ┌─────────────┬───────────────────────────────────┬───────────────────┬───────────────────────────────────┐
 │ When        │ Change                            │ Rationale         │ Evidence                          │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ User reported lack of confidence  │ Starting point    │ User transcript                   │
 │ 13:35       │ in parallel feature               │ for investigation │                                   │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Analyzed parallel codebase        │ Understand        │ find crates/ralph/src -name       │
 │ 13:36       │ structure                         │ current           │ "*parallel*"                      │
 │             │                                   │ implementation    │                                   │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Read contracts/config/parallel.rs │ Understand        │ File:                             │
 │ 13:37       │                                   │ configuration     │ crates/ralph/src/contracts/config │
 │             │                                   │ options           │ /parallel.rs                      │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Read commands/run/parallel/mod.rs │ Understand module │ File:                             │
 │ 13:38       │                                   │ structure and     │ crates/ralph/src/commands/run/par │
 │             │                                   │ settings          │ allel/mod.rs (862 LOC)            │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Read                              │ Understand main   │ File:                             │
 │ 13:40       │ commands/run/parallel/orchestrati │ supervisor loop   │ crates/ralph/src/commands/run/par │
 │             │ on.rs                             │                   │ allel/orchestration.rs (986 LOC)  │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Read                              │ Understand        │ Directory:                        │
 │ 13:42       │ commands/run/parallel/merge_runne │ current merge     │ crates/ralph/src/commands/run/par │
 │             │ r/                                │ handling          │ allel/merge_runner/               │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Read                              │ Understand state  │ File:                             │
 │ 13:45       │ commands/run/parallel/state.rs    │ management        │ crates/ralph/src/commands/run/par │
 │             │                                   │ complexity        │ allel/state.rs (509 LOC)          │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Read parallel execution tests     │ Understand        │ Files: tests/parallel_*_test.rs   │
 │ 13:48       │                                   │ current behavior  │ (4 files)                         │
 │             │                                   │ and edge cases    │                                   │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Read user config file             │ Understand user's │ File:                             │
 │ 13:52       │                                   │ phase/runner      │ /Users/mitchfultz/Projects/AI/ral │
 │             │                                   │ configuration     │ ph/.ralph/config.jsonc            │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ User proposed merge agent         │ Key insight:      │ User transcript                   │
 │ 13:55       │ architecture                      │ spawn agent       │                                   │
 │             │                                   │ subprocess for    │                                   │
 │             │                                   │ merge             │                                   │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Identified queue file gitignore   │ Queue changes     │ Discussion                        │
 │ 14:00       │ issue                             │ can't flow        │                                   │
 │             │                                   │ through git merge │                                   │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Finalized architecture:           │ Queue updated     │ Agreement                         │
 │ 14:05       │ coordinator spawns workers and    │ locally via ralph │                                   │
 │             │ merge agents                      │ task done after   │                                   │
 │             │                                   │ PR merge          │                                   │
 ├─────────────┼───────────────────────────────────┼───────────────────┼───────────────────────────────────┤
 │ 2026-02-17  │ Documented complete flow          │ Architecture      │ This briefing                     │
 │ 14:10       │                                   │ design complete   │                                   │
 └─────────────┴───────────────────────────────────┴───────────────────┴───────────────────────────────────┘

 ────────────────────────────────────────────────────────────────────────────────

 1) Current State

### 6.1 Deliverables Created

 None yet. This is a design phase. Implementation has not started.

### 6.2 What Works (Current System)

- Workers run in isolated git workspace clones ✓
- Workers execute full phase loops ✓
- PR creation via gh CLI ✓
- Merge conflict detection ✓
- Crash recovery via state file ✓

### 6.3 What Does Not Work / Known Issues

 Critical Issues (Why User Has No Confidence):

 1. Queue Merge Conflicts (Critical)
     - Queue files are gitignored to avoid conflicts
     - Completion signals used as workaround (fragile)
     - If signal lost/corrupted, queue never updates
     - User must manually edit state files to recover
 2. Limited Merge Intelligence (High)
     - Current merge runner is internal thread
     - Single AI prompt for conflict resolution
     - No iteration, no tool use
     - Cannot handle complex queue.json semantic merges
 3. Complex State Management (High)
     - State file tracks: tasks_in_flight, prs, finished_without_pr, merge_blockers
     - Requires manual intervention for recovery
     - Base branch mismatches, head mismatches, stale records
 4. "Finished Without PR" Stuck State (Medium)
     - If PR creation fails, task recorded as finished_without_pr
     - Blocks re-runs for 24h TTL
     - Must manually edit state file to clear
 5. No Debuggability (Medium)
     - Merge runner runs as thread in coordinator
     - Cannot run manually to debug stuck PRs
     - No per-merge logs

### 6.4 Decisions Already Made

 ┌─────────────────────────────────────┬───────────────────────────────────────────────┬───────────────────┐
 │ Decision                            │ Rationale                                     │ Who Decided       │
 ├─────────────────────────────────────┼───────────────────────────────────────────────┼───────────────────┤
 │ Workers remain unchanged            │ Workers already run full phase loops          │ User + Agent      │
 │                                     │ correctly. No need to modify.                 │                   │
 ├─────────────────────────────────────┼───────────────────────────────────────────────┼───────────────────┤
 │ Create new merge agent subprocess   │ Needs full AI capabilities (tools,            │ User + Agent      │
 │                                     │ iteration), must be debuggable                │                   │
 ├─────────────────────────────────────┼───────────────────────────────────────────────┼───────────────────┤
 │ Queue updated locally after merge   │ Queue files are gitignored, cannot flow       │ Agent (User       │
 │                                     │ through git merge                             │ confirmed)        │
 ├─────────────────────────────────────┼───────────────────────────────────────────────┼───────────────────┤
 │ No completion signals               │ Too fragile, complexity not justified         │ Agent (User       │
 │                                     │                                               │ confirmed)        │
 ├─────────────────────────────────────┼───────────────────────────────────────────────┼───────────────────┤
 │ Extra commit acceptable             │ If queue tracked, commit marks completion. If │ User              │
 │                                     │ gitignored, no commit.                        │                   │
 ├─────────────────────────────────────┼───────────────────────────────────────────────┼───────────────────┤
 │ CI gate runs per-worker             │ Already passing before PR created. No need to │ User              │
 │                                     │ run after merge.                              │                   │
 ├─────────────────────────────────────┼───────────────────────────────────────────────┼───────────────────┤
 │ Sequential merge ordering           │ Wait for merge agent before starting next.    │ User + Agent      │
 │                                     │ Simpler, clearer conflicts.                   │                   │
 ├─────────────────────────────────────┼───────────────────────────────────────────────┼───────────────────┤
 │ Reuse phase override pattern for    │ Consistent with existing config               │ Agent             │
 │ merge agent runner                  │                                               │                   │
 └─────────────────────────────────────┴───────────────────────────────────────────────┴───────────────────┘

 ────────────────────────────────────────────────────────────────────────────────

 1) Open Items and Next Steps (Prioritized)

 ┌──────────┬─────────────────┬───────────┬────────────┬────────────────────────────┬──────────────────────┐
 │ Priority │ Action          │ Owner     │ Dependency │ Expected Outcome           │ Verification         │
 ├──────────┼─────────────────┼───────────┼────────────┼────────────────────────────┼──────────────────────┤
 │ 1        │ Create ralph    │ Developer │ None       │ New command available:     │ ralph run            │
 │          │ run merge-agent │           │            │ ralph run merge-agent --pr │ merge-agent --help   │
 │          │  command        │           │            │ <N> --task <ID>            │ shows options        │
 ├──────────┼─────────────────┼───────────┼────────────┼────────────────────────────┼──────────────────────┤
 │ 2        │ Implement merge │ Developer │ Step 1     │ Agent can check out PR,    │ Run manually on test │
 │          │ agent prompt    │           │            │ merge, run ralph task      │ PR, verify merge +   │
 │          │                 │           │            │ done, push                 │ queue update         │
 ├──────────┼─────────────────┼───────────┼────────────┼────────────────────────────┼──────────────────────┤
 │ 3        │ Simplify        │ Developer │ Step 2     │ Remove complex state       │ Code review:         │
 │          │ coordinator     │           │            │ machine, just spawn and    │ orchestration.rs <   │
 │          │ orchestration   │           │            │ wait                       │ 200 lines            │
 ├──────────┼─────────────────┼───────────┼────────────┼────────────────────────────┼──────────────────────┤
 │ 4        │ Update worker   │ Developer │ Step 3     │ Worker no longer writes    │ grep -r              │
 │          │ to remove       │           │            │ completion signals         │ "completion_signal"  │
 │          │ completion      │           │            │                            │ src/ shows no worker │
 │          │ signal write    │           │            │                            │ usage                │
 ├──────────┼─────────────────┼───────────┼────────────┼────────────────────────────┼──────────────────────┤
 │ 5        │ Simplify state  │ Developer │ Step 4     │ Only track tasks_in_flight │ state.rs < 100 lines │
 │          │ file to minimal │           │            │ PIDs and pending merges    │                      │
 │          │ tracking        │           │            │                            │                      │
 ├──────────┼─────────────────┼───────────┼────────────┼────────────────────────────┼──────────────────────┤
 │ 6        │ Remove          │ Developer │ Step 5     │ Delete completion.rs,      │ Files removed, no    │
 │          │ completion      │           │            │ queue_sync.rs              │ references           │
 │          │ signal system   │           │            │                            │                      │
 │          │ entirely        │           │            │                            │                      │
 ├──────────┼─────────────────┼───────────┼────────────┼────────────────────────────┼──────────────────────┤
 │ 7        │ Update parallel │ Developer │ Step 6     │ Tests pass with new        │ cargo test parallel  │
 │          │ execution tests │           │            │ architecture               │ passes               │
 ├──────────┼─────────────────┼───────────┼────────────┼────────────────────────────┼──────────────────────┤
 │ 8        │ Integration     │ Developer │ Step 7     │ Run ralph run loop         │ Two tasks complete,  │
 │          │ test full flow  │           │            │ --parallel 2 on test repo, │ PRs merged, queue    │
 │          │                 │           │            │ verify end-to-end          │ updated              │
 ├──────────┼─────────────────┼───────────┼────────────┼────────────────────────────┼──────────────────────┤
 │ 9        │ Update          │ Developer │ Step 8     │ docs/features/parallel.md  │ Doc review           │
 │          │ documentation   │           │            │ reflects new architecture  │                      │
 └──────────┴─────────────────┴───────────┴────────────┴────────────────────────────┴──────────────────────┘

 ────────────────────────────────────────────────────────────────────────────────

 1) Risks, Constraints, and Tradeoffs

### Risks

 ┌─────────────────────────────────┬────────────┬────────┬─────────────────────────────────────────────────┐
 │ Risk                            │ Likelihood │ Impact │ Mitigation                                      │
 ├─────────────────────────────────┼────────────┼────────┼─────────────────────────────────────────────────┤
 │ Merge agent fails after PR      │ Medium     │ High   │ Merge agent is idempotent - can re-run ralph    │
 │ merged but before queue update  │            │        │ task done. If queue already shows done, skips.  │
 ├─────────────────────────────────┼────────────┼────────┼─────────────────────────────────────────────────┤
 │ User runs out of API credits    │ Low        │ Medium │ Merge agent uses same runner config as phases.  │
 │ during merge agent execution    │            │        │ User already manages credits for main work.     │
 ├─────────────────────────────────┼────────────┼────────┼─────────────────────────────────────────────────┤
 │ Complex merge conflicts         │ Medium     │ Medium │ Agent can escalate (exit with error).           │
 │ overwhelm agent                 │            │        │ Coordinator leaves PR open for manual           │
 │                                 │            │        │ resolution.                                     │
 ├─────────────────────────────────┼────────────┼────────┼─────────────────────────────────────────────────┤
 │ Workspace B conflicts with      │ High       │ Low    │ Expected behavior. Merge agent resolves. If     │
 │ merged Workspace A changes      │            │        │ fails, escalate to user.                        │
 └─────────────────────────────────┴────────────┴────────┴─────────────────────────────────────────────────┘

### Tradeoffs Chosen

 Tradeoff: Spawn merge agent per-PR vs batch merge agent
- Rationale: Per-PR is simpler, more debuggable, easier to reason about. Sequential ordering (wait for
 each) ensures clear conflict resolution.
- Alternative rejected: Batch agent could optimize merge order but adds complexity. User preferred
 simplicity.

 Tradeoff: Update queue locally via ralph task done vs track queue in git
- Rationale: Queue is already gitignored. Changing that would cause merge conflicts on every parallel task.
 Local update is clean.
- Alternative rejected: Track queue in git and resolve conflicts. User already avoiding this (gitignores
 queue files).

 Tradeoff: Coordinator waits for merge agent vs fire-and-forget
- Rationale: Waiting ensures main is up-to-date before next merge. Simpler conflict resolution.
- Alternative rejected: Parallel merges would require complex dependency tracking. Not worth complexity for
 user's use case.

 Tradeoff: Remove completion signals vs keep as fallback
- Rationale: Signals add fragility. New architecture doesn't need them. Simpler to remove entirely.
- Alternative rejected: Keep signals as backup mechanism. Adds code complexity for edge case that shouldn't
 occur.

 ────────────────────────────────────────────────────────────────────────────────

 1) Questions Needing Answers

 Q: Should the merge agent run CI gate after merge?
- Why it matters: User's config has ci_gate_enabled: true with make macos-ci
- Options:
  - A) No, trust per-worker CI (already passed)
  - B) Yes, run CI after merge to catch integration issues
  - C) Configurable via parallel.merge_ci_gate: true/false
- Default assumption if unanswered: Option A (no post-merge CI). User stated CI already passed before PR
 created.

 Q: What should happen if merge agent cannot resolve conflicts?
- Why it matters: Need failure mode that doesn't block queue
- Options:
  - A) Leave PR open, create "merge task" in queue for manual resolution
  - B) Leave PR open, log error, skip to next task (queue shows task still doing)
  - C) Exit parallel mode entirely with error
- Default assumption if unanswered: Option B. Queue consistency is more important than completing all
 tasks.

 Q: Should we delete merge agent workspaces immediately after success?
- Why it matters: Storage usage vs ability to debug
- Options:
  - A) Delete immediately on success
  - B) Keep for 24h then auto-delete
  - C) Configurable via parallel.keep_workspaces: <hours>
- Default assumption if unanswered: Option A. Workspaces are large, can recreate for debug if needed.

 ────────────────────────────────────────────────────────────────────────────────

 1) Appendix

### A) Artifacts and References

 ┌─────────────────────┬──────────┬──────────────────────────────────────────┬─────────────────────────────┐
 │ Name                │ Type     │ Location                                 │ Description                 │
 ├─────────────────────┼──────────┼──────────────────────────────────────────┼─────────────────────────────┤
 │ Current parallel    │ Source   │ crates/ralph/src/commands/run/parallel/  │ Existing implementation     │
 │ module              │          │                                          │ (~3,500 LOC)                │
 ├─────────────────────┼──────────┼──────────────────────────────────────────┼─────────────────────────────┤
 │ Parallel config     │ Source   │ crates/ralph/src/contracts/config/parall │ Configuration struct        │
 │ contract            │          │ el.rs                                    │                             │
 ├─────────────────────┼──────────┼──────────────────────────────────────────┼─────────────────────────────┤
 │ User config         │ Source   │ .ralph/config.jsonc (repo root)          │ Mitch's current             │
 │                     │          │                                          │ configuration               │
 ├─────────────────────┼──────────┼──────────────────────────────────────────┼─────────────────────────────┤
 │ Parallel docs       │ Document │ docs/features/parallel.md                │ Current documentation       │
 ├─────────────────────┼──────────┼──────────────────────────────────────────┼─────────────────────────────┤
 │ Parallel tests      │ Tests    │ crates/ralph/tests/parallel_*_test.rs    │ 4 test files for parallel   │
 │                     │          │                                          │ mode                        │
 ├─────────────────────┼──────────┼──────────────────────────────────────────┼─────────────────────────────┤
 │ Merge conflicts     │ Prompt   │ crates/ralph/assets/prompts/merge_confli │ Current simple prompt       │
 │ prompt              │          │ cts.md                                   │                             │
 └─────────────────────┴──────────┴──────────────────────────────────────────┴─────────────────────────────┘

### B) Glossary

- Coordinator: Main ralph run loop --parallel process
- Worker: Subprocess executing task in workspace clone
- Merge Agent: New subprocess for PR merge + queue update
- Phase: One of 1-3 execution stages (plan/implement/review)
- Iteration: Repeat of phases for refinement (user config: 2 iterations)
- Workspace: Git clone directory for isolated task execution
- Completion Signal: Current mechanism (to be removed) for queue sync

### C) Raw Log Pointers

- Messages 1-20: User describes parallel feature concerns
- Messages 21-60: Investigation of current implementation
- Messages 61-100: Architecture discussion, completion signals
- Messages 101-140: User proposes merge agent approach
- Messages 141-180: Detailed flow design, queue gitignore issue resolved
- Messages 181+: Final architecture confirmation
