# Competitive Analysis: snarktank/ralph vs mitchfultz/ralph

## Executive Summary

This analysis compares two tools named "Ralph" that both orchestrate AI agent workflows but with fundamentally different architectures and philosophies.

| | **snarktank/ralph** | **mitchfultz/ralph** (this repo) |
|---|---|---|
| **Philosophy** | Stateless simplicity, fresh context | Persistent state, rich orchestration |
| **Implementation** | Bash script (~200 lines) | Rust CLI (~15k+ lines) |
| **State Model** | Git history + files | JSON queue with schema validation |
| **Best For** | Quick setup, hackable workflows | Production use, team coordination |

---

## snarktank/ralph Analysis

### Overview
A lightweight bash wrapper around Amp CLI and Claude Code that implements a "fresh context per iteration" pattern. Each loop spawns a completely new AI instance with no memory of previous iterations except what is explicitly persisted to files.

---

### Architecture

```
ralph.sh
├── prd.json          # User stories with passes: true/false
├── progress.txt      # Append-only learnings
├── prompt.md         # Amp prompt template
├── CLAUDE.md         # Claude Code prompt template
└── archive/          # Auto-archived previous runs
```

### Core Workflow

1. **Create PRD** using skill → `tasks/prd-[feature].md`
2. **Convert to JSON** using skill → `prd.json`
3. **Run loop** → `./ralph.sh [max_iterations]`
4. **Each iteration**:
   - Spawn fresh AI instance (clean context)
   - Pick highest priority incomplete story
   - Implement and run quality checks
   - Commit if checks pass
   - Update `prd.json` and append to `progress.txt`
5. **Stop** when `<promise>COMPLETE</promise>` found in output

### PRD Format (prd.json)

```json
{
  "project": "MyApp",
  "branchName": "ralph/feature-name",
  "description": "Feature description",
  "userStories": [
    {
      "id": "US-001",
      "title": "Story title",
      "description": "As a user...",
      "acceptanceCriteria": ["Criterion 1", "Criterion 2"],
      "priority": 1,
      "passes": false,
      "notes": ""
    }
  ]
}
```

### Key Features

| Feature | Implementation |
|---------|---------------|
| Fresh context per iteration | Spawns new AI process each loop |
| Auto-handoff | Amp setting for large stories (`"amp.experimental.autoHandoff"`) |
| Browser verification | UI stories use "dev-browser skill" |
| Auto-archiving | Previous runs saved to `archive/YYYY-MM-DD-feature-name/` |
| AGENTS.md updates | Documents patterns/gotchas for future iterations |
| Skills system | PRD generation and conversion skills for Amp/Claude |

### Strengths

1. **Simplicity**: Single bash file, easy to understand and modify
2. **Zero dependencies**: Only requires `jq` and git
3. **Fresh context guarantee**: Each iteration starts clean (no context pollution)
4. **Auto-archiving**: Historical runs automatically preserved
5. **Interactive flowchart**: Visual documentation at snarktank.github.io/ralph
6. **Small tasks philosophy**: Forces decomposition into context-window-sized chunks

### Weaknesses

1. **Limited runner support**: Only Amp and Claude Code
2. **No persistent queue**: Tasks live in JSON file, no rich management
3. **No interactive UI**: Purely CLI-driven
4. **No configuration layers**: Single config approach
5. **No validation/schema**: No structured validation of task files
6. **Manual PRD conversion**: Requires skill invocation to convert PRD to JSON
7. **No dependency management**: Cannot express task dependencies
8. **No priority system**: Simple pass/fail, no gradation

---

## mitchfultz/ralph (This Repository) Analysis

### Overview
A comprehensive Rust CLI with persistent state management, rich TUI, and multi-runner support. Implements a three-phase workflow with CI gates and schema validation.

### Architecture

```
crates/ralph/
├── src/
│   ├── cli/           # CLI commands
│   ├── tui/           # Interactive terminal UI
│   ├── runner/        # Runner integrations
│   ├── queue/         # Queue operations
│   └── commands/      # Command implementations
└── assets/prompts/    # Embedded prompt templates

.ralph/
├── queue.json         # Active tasks (source of truth)
├── done.json          # Archived completed tasks
├── config.json        # Project configuration
├── cache/
│   ├── plans/         # Phase 1 cached plans
│   └── completions/   # Completion signals
└── prompts/           # Optional prompt overrides
```

### Core Workflow (Three-Phase)

1. **Phase 1 (Planning)**: Generate detailed plan → `.ralph/cache/plans/<TASK_ID>.md`
2. **Phase 2 (Implementation + CI)**: Apply changes, run `make ci`, stop
3. **Phase 3 (Review + Completion)**: Review diff, re-run CI, complete, commit/push

### Task Format (queue.json)

```json
{
  "id": "RQ-0001",
  "title": "Task title",
  "status": "todo",
  "priority": "high",
  "created_at": "2026-01-25T03:45:00Z",
  "updated_at": "2026-01-25T03:45:00Z",
  "tags": ["cli", "rust"],
  "scope": ["src/main.rs"],
  "plan": ["Step 1", "Step 2"],
  "evidence": ["make ci"],
  "depends_on": [],
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "phases": 3
  }
}
```

### Key Features

| Feature | Implementation |
|---------|---------------|
| Multi-runner support | Codex, OpenCode, Gemini, Claude, Cursor |
| TUI | Interactive ratatui-based interface with command palette |
| Queue management | Full CRUD, filtering, sorting, archiving |
| Schema validation | JSON schemas for config and queue |
| Config layers | CLI → Project → Global → Schema defaults |
| Dependency management | `depends_on` field with validation |
| Task builder | AI-assisted task creation from natural language |
| RepoPrompt integration | Context-aware planning with tooling reminders |
| Redaction | Automatic secret redaction in debug output |
| Lock management | Atomic queue operations with PID-based locks |

### Strengths

1. **Rich TUI**: Interactive task management with filters, search, command palette
2. **Multi-runner**: Supports 5 different AI runners with unified interface
3. **Persistent state**: Structured queue with validation and archiving
4. **Three-phase workflow**: Planning → Implementation → Review with CI gates
5. **Configuration system**: Layered config with CLI overrides
6. **Security**: Built-in redaction, debug logging safeguards
7. **Queue analytics**: Stats, history, burndown charts
8. **Task dependencies**: Express and validate task relationships
9. **Schema validation**: Prevents corruption, enables repair commands
10. **Atomic operations**: Lock management prevents race conditions

### Weaknesses

1. **Complexity**: Larger codebase, steeper learning curve
2. **Compilation required**: Rust toolchain needed for development
3. **Context persistence**: Risk of context pollution across iterations
4. **No auto-archiving**: Manual archive command required
5. **No browser verification**: No built-in UI testing workflow
6. **No AGENTS.md auto-update**: Project context not automatically preserved
7. **No skills system**: No reusable skill definitions for runners

---

## Comparative Analysis

### Architecture Comparison

| Aspect | snarktank/ralph | mitchfultz/ralph |
|--------|-----------------|------------------|
| **Language** | Bash | Rust |
| **Size** | ~200 lines | ~15k+ lines |
| **State** | Stateless (git + files) | Persistent JSON queue |
| **Install** | Copy script | `cargo install` or `make install` |
| **Dependencies** | jq, git | Rust toolchain, runner binaries |

### Workflow Comparison

| Aspect | snarktank/ralph | mitchfultz/ralph |
|--------|-----------------|------------------|
| **Task definition** | PRD → JSON conversion | Direct queue entry or AI builder |
| **Execution** | Iteration loop | Phased workflow (1-3 phases) |
| **Context** | Fresh per iteration | Persistent across phases |
| **Completion** | `<promise>COMPLETE</promise>` | Status + archive |
| **CI integration** | Quality checks per iteration | `make ci` gate per phase |

### State Management Comparison

| Aspect | snarktank/ralph | mitchfultz/ralph |
|--------|-----------------|------------------|
| **Task storage** | `prd.json` | `.ralph/queue.json` |
| **Archive** | `archive/YYYY-MM-DD-*/` | `.ralph/done.json` |
| **Learnings** | `progress.txt` | Task `notes` field |
| **Validation** | None | JSON Schema |
| **Dependencies** | None | `depends_on` field |

---

## UX Gap Analysis

### What snarktank/ralph Does Better

1. **Onboarding Simplicity**
   - Single file to copy and run
   - No build step, no dependencies beyond jq
   - Clear, minimal workflow: PRD → Convert → Run

2. **Fresh Context Guarantee**
   - Each iteration spawns a new AI instance
   - No risk of context pollution or "baggage"
   - Forces small, completable tasks

3. **Auto-Archiving**
   - Historical runs automatically preserved
   - Easy to reference previous attempts
   - Branch-based organization

4. **Project Context Preservation**
   - AGENTS.md auto-updates with learnings
   - Conventions and gotchas documented automatically
   - Future iterations inherit knowledge

5. **Browser Verification**
   - Built-in workflow for UI testing
   - Dev-browser skill integration

6. **Visual Documentation**
   - Interactive flowchart website
   - Clear visual representation of workflow

### What mitchfultz/ralph Does Better

1. **Interactive Task Management**
   - Rich TUI with real-time filtering and search
   - Command palette for discoverability
   - Visual task status and priority

2. **Multi-Runner Flexibility**
   - 5 supported runners with unified interface
   - Per-task runner/model overrides
   - Normalized CLI options across runners

3. **Structured Queue Operations**
   - Schema validation prevents corruption
   - Repair commands for data recovery
   - Archive and prune operations

4. **Configuration System**
   - Layered config (CLI → Project → Global)
   - Sensible defaults with overrides
   - JSON Schema documentation

5. **Security Features**
   - Automatic secret redaction
   - Debug logging with safeguards
   - Lock management for atomicity

6. **Analytics and Reporting**
   - Task statistics and completion rates
   - Burndown charts and history
   - Tag and scope breakdowns

---

## UX Improvement Recommendations

### High Priority

1. **Onboarding Wizard**
   - `ralph init` should be interactive
   - Guide new users through first task creation
   - Explain the 3-phase workflow with examples
   - Set up default config with explanations

2. **Quick Start Mode**
   - Add `--quick` flag to `run one` that skips planning phase
   - For users who want immediate execution like snarktank/ralph
   - Single-phase execution with minimal ceremony

3. **Auto-Archive on Completion**
   - Config option to auto-archive done/rejected tasks
   - Default to prompt (ask before archiving)
   - Prevents queue bloat without manual action

4. **Project Context Template**
   - Auto-generate AGENTS.md equivalent
   - Template with project conventions
   - Command to update with new learnings

### Medium Priority

5. **Task Templates**
   - Pre-defined task patterns (bug fix, feature, refactor)
   - Template selection in TUI (`N` → choose template)
   - Auto-populate tags, scope hints, plan structure

6. **Progress Visualization**
   - Show iteration progress in TUI execution view
   - Visual indicator of phase completion
   - Time tracking per phase

7. **Browser Verification Integration**
   - Add browser verification step for UI tasks
   - Configurable verification command
   - Integration with task scope/tags

8. **Simplified PRD Workflow**
   - `ralph prd create` command
   - Convert PRD markdown to task(s) automatically
   - Similar to snarktank's PRD → JSON flow

### Lower Priority

9. **Interactive Flowchart**
   - Terminal-based workflow visualization
   - Show current position in 3-phase flow
   - ASCII art or rich terminal graphics

10. **Task Dependencies Visualization**
    - Graph view of task dependencies
    - Show blocking/blocked relationships
    - Critical path highlighting

11. **Export/Import**
    - Export queue to markdown/JSON
    - Import from snarktank/ralph format
    - Migration path for users switching

12. **Notification Integration**
    - Desktop notifications on task completion
    - Sound alerts (optional)
    - Integration with macOS/Linux notification systems

### Documentation Improvements

13. **Quick Start Guide**
    - Single-page getting started doc
    - Common workflows with examples
    - Comparison with other tools

14. **Video/GIF Demos**
    - TUI walkthrough recordings
    - Show before/after for key features
    - Embed in README

15. **Interactive Tutorial**
    - Built-in tutorial mode (`ralph tutorial`)
    - Step-by-step guided first task
    - Teaches TUI keybindings interactively

---

## Conclusion

**snarktank/ralph** excels at simplicity and the "fresh context" philosophy. It's ideal for users who want minimal overhead and prefer stateless iteration. The bash implementation makes it highly hackable.

**mitchfultz/ralph** excels at structured workflow management and multi-runner flexibility. The TUI and persistent queue provide visibility and control that bash scripts cannot match. The three-phase workflow with CI gates ensures higher quality outputs.

### Recommendation

This repository should focus on:

1. **Reducing onboarding friction** (wizard, quick start)
2. **Adding auto-archiving and project context preservation**
3. **Improving visual feedback during execution**
4. **Creating better quick-start documentation**

The goal is to maintain the power and flexibility of the Rust implementation while capturing the simplicity and clarity that makes snarktank/ralph appealing.
