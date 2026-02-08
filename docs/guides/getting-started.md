# Getting Started with Ralph

Welcome to Ralph! This guide will walk you through everything you need to know to get up and running with AI-powered task automation.

## What is Ralph?

Ralph is a Rust-based CLI tool that manages AI agent loops with a structured JSON task queue. It orchestrates AI runners (Claude, Codex, OpenCode, Gemini, Cursor, Kimi, and Pi) to execute development tasks through a structured workflow.

Think of Ralph as your AI conductor—managing tasks, tracking progress, and ensuring quality through systematic phases of execution.

---

## Table of Contents

1. [Installation](#1-installation)
2. [Quick Initialization](#2-quick-initialization)
3. [Your First Task](#3-your-first-task)
4. [Understanding the Workflow](#4-understanding-the-workflow)
5. [Runner Selection](#5-runner-selection)
6. [Configuration Basics](#6-configuration-basics)
7. [Daily Workflow](#7-daily-workflow)
8. [Next Steps](#8-next-steps)

---

## 1. Installation

### From crates.io (Recommended)

The easiest way to install Ralph is via Cargo:

```bash
cargo install ralph
```

This installs the latest published version from crates.io to `~/.cargo/bin/ralph`.

### From Source

If you want the latest development version or to contribute:

```bash
# Clone the repository
git clone https://github.com/mitchfultz/ralph
cd ralph

# Build and install
make install
```

This installs the `ralph` binary to `~/.local/bin/ralph` (or a writable fallback path).

### Verify Installation

Check that Ralph is properly installed:

```bash
ralph version
```

You should see output like:
```
ralph 0.x.x
```

### Add to PATH

If you installed from source, ensure `~/.local/bin` is in your PATH:

```bash
# Add to your shell profile (.bashrc, .zshrc, etc.)
export PATH="$HOME/.local/bin:$PATH"
```

---

## 2. Quick Initialization

Ralph needs to be initialized in each project where you want to use it. Navigate to your project directory and run:

```bash
cd your-project
ralph init
```

### Interactive Wizard

When you run `ralph init` in a terminal (TTY), it launches an interactive wizard that will:

1. **Choose Your AI Runner**: Select from Claude, Codex, OpenCode, Gemini, Cursor, Kimi, or Pi
2. **Select a Model**: Pick the best model for your chosen runner
3. **Configure Workflow Mode**: Choose between 1-phase (quick), 2-phase (standard), or 3-phase (full)
4. **Create Your First Task** (optional): Add an initial task to get started

### Example Walkthrough

```bash
$ cd my-awesome-project
$ ralph init

✓ Initializing Ralph in /home/user/my-awesome-project

┌─────────────────────────────────────────┐
│  Welcome to Ralph! Let's get started.   │
└─────────────────────────────────────────┘

Choose your AI runner:
  [1] Claude - General purpose, excellent reasoning (recommended)
  [2] Codex - Code generation, OpenAI ecosystem
  [3] OpenCode - Flexible model selection
  [4] Gemini - Google ecosystem, cost efficient
  [5] Cursor - Cursor IDE integration
  [6] Kimi - Fast execution with session support
  [7] Pi - OpenAI GPT models

Select [1-7]: 1

Choose a model for Claude:
  [1] sonnet (default) - Balanced speed and capability
  [2] opus - Most capable, slower
  [3] Other (specify ID)

Select [1-3]: 1

Choose workflow mode:
  [1] 1-phase (Quick) - Single-pass execution
  [2] 2-phase (Standard) - Plan → Implement
  [3] 3-phase (Full) - Plan → Implement → Review [Recommended]

Select [1-3]: 3

Would you like to create your first task? [y/N]: y
Enter task title: Add user authentication feature

✓ Created .ralph/config.json
✓ Created .ralph/queue.json
✓ Created first task: RQ-0001
✓ Ralph is ready to use!
```

### Non-Interactive Mode

For CI/CD or scripts, skip the wizard:

```bash
ralph init --non-interactive
```

This uses sensible defaults without prompting.

### Force Reinitialization

To overwrite existing Ralph files:

```bash
ralph init --force
```

---

## 3. Your First Task

After initialization, you have several ways to work with tasks.

### macOS: Open the App (SwiftUI)

On macOS, you can use the Ralph app for interactive queue work:

```bash
ralph app open
```

### Run Your First Task

From the CLI, run the next task in the queue:

```bash
# Run the next available task
ralph run one

# Or run in loop mode until all tasks complete
ralph run loop
```

### View the Queue

```bash
# List all tasks
ralph queue list

# Show the next task
ralph queue next --with-title

# Search tasks
ralph queue search "authentication"
```

### Creating Tasks

**From the CLI:**

```bash
# Quick task creation
ralph task "Add password reset functionality"

# With details
ralph task "Refactor database layer" \
  --request "Move all database access code into a dedicated module" \
  --priority high
```

**From the App (macOS):**

Open the app with `ralph app open` and create tasks from the UI.

### Example Task Session

```bash
# 1. Check what tasks exist
$ ralph queue list
ID       Status  Priority  Title
RQ-0001  todo    medium    Add user authentication feature

# 2. Run the next task
$ ralph run one
Starting RQ-0001: Add user authentication feature

=== Phase 1: Planning ===
Generating implementation plan...
Plan cached to .ralph/cache/plans/RQ-0001.md

=== Phase 2: Implementation ===
Implementing plan...
Running CI gate: make ci
✓ CI passed

=== Phase 3: Review ===
Reviewing changes...
✓ Task completed

# 3. Check the result
$ ralph queue list
ID       Status  Priority  Title
RQ-0001  done    medium    Add user authentication feature
```

---

## 4. Understanding the Workflow

Ralph uses a structured **3-phase workflow** to ensure quality. Understanding these phases helps you choose the right mode for each task.

### The 3 Phases

```
┌─────────────┐    ┌──────────────────┐    ┌──────────────────┐
│ Phase 1     │───▶│ Phase 2          │───▶│ Phase 3          │
│ Planning    │    │ Implementation   │    │ Review           │
├─────────────┤    ├──────────────────┤    ├──────────────────┤
│ • Analyze   │    │ • Execute plan   │    │ • Review diff    │
│ • Research  │    │ • Run CI gate    │    │ • Fix issues     │
│ • Plan      │    │ • Stop if CI     │    │ • Final CI       │
│   (cached)  │    │   fails          │    │ • Complete task  │
└─────────────┘    └──────────────────┘    └──────────────────┘
```

**Phase 1: Planning**
- The AI analyzes the task and creates a detailed implementation plan
- The plan is cached to `.ralph/cache/plans/<TASK_ID>.md`
- You can review and edit this plan before implementation

**Phase 2: Implementation**
- The AI executes the cached plan
- Changes are applied to the codebase
- The CI gate (`make ci`) runs automatically
- If CI fails, the AI attempts to fix issues

**Phase 3: Review**
- The AI reviews all changes against quality standards
- Any flagged issues are addressed
- Final CI gate verification
- Task is marked complete

### Phase Mode Comparison

| Mode | Phases | Best For | Speed | Quality |
|------|--------|----------|-------|---------|
| **1-Phase** | Single pass | Quick fixes, typos, simple refactors | ⚡ Fastest | Basic |
| **2-Phase** | Plan → Implement | Medium complexity, less critical code | 🚀 Fast | Good |
| **3-Phase** | Plan → Implement → Review | Complex features, production code | 🐢 Slower | ⭐ Highest |

### Choosing the Right Mode

- **1-Phase**: Use for typo fixes, comment updates, simple variable renames
- **2-Phase**: Use for internal refactoring, test additions, documentation
- **3-Phase**: Use for new features, API changes, architectural decisions

### Changing Modes

Override phases for a single run:

```bash
# Use 1-phase for a quick fix
ralph run one --phases 1

# Or use the --quick shorthand
ralph run one --quick

# Use 3-phase for careful review
ralph run one --phases 3
```

Set default in config:

```json
{
  "agent": {
    "phases": 2
  }
}
```

---

## 5. Runner Selection

Ralph supports multiple AI runners. Choose based on your needs:

### Runner Comparison

| Runner | Best For | Model Options | Speed | Reasoning |
|--------|----------|---------------|-------|-----------|
| **Claude** | General purpose, complex reasoning | `sonnet` (default), `opus` | Medium | ⭐⭐⭐ Excellent |
| **Codex** | Code generation, OpenAI fans | `gpt-5.3-codex`, `gpt-5.2-codex` | Fast | ⭐⭐⭐ Excellent |
| **Gemini** | Cost efficiency, speed | `gemini-3-pro-preview`, `gemini-3-flash-preview` | ⚡ Fast | ⭐⭐ Good |
| **OpenCode** | Flexible/custom endpoints | Arbitrary model IDs | Varies | Varies |
| **Cursor** | Cursor IDE users | Uses Cursor's `agent` binary | Medium | ⭐⭐⭐ Excellent |
| **Kimi** | Fast execution, session support | `kimi-for-coding` | ⚡ Fast | ⭐⭐⭐ Excellent |
| **Pi** | OpenAI GPT models | `gpt-5.3` | Medium | ⭐⭐⭐ Excellent |

### Recommended Models by Runner

**Claude:**
- `sonnet` - Best balance of speed and capability (recommended)
- `opus` - Maximum capability for complex tasks

**Codex:**
- `gpt-5.3-codex` - Latest and most capable
- `gpt-5.2-codex` - Good balance, slightly faster

**Gemini:**
- `gemini-3-pro-preview` - Best quality
- `gemini-3-flash-preview` - Fastest, good for quick tasks

**Kimi:**
- `kimi-for-coding` - Optimized for coding tasks (default)

### Switching Runners

Override for a single task:

```bash
# Use Claude for this run
ralph run one --runner claude --model sonnet

# Use Codex with high reasoning effort
ralph run one --runner codex --model gpt-5.3-codex --effort high
```

Set default in config:

```json
{
  "agent": {
    "runner": "claude",
    "model": "sonnet"
  }
}
```

### Checking Runner Availability

Verify your runners are installed:

```bash
ralph doctor
```

This checks:
- Git repository status
- Queue file validity
- Runner binary availability
- Configuration correctness

### Installing Runners

Ralph requires the runner CLIs to be installed separately:

- **Claude**: `npm install -g @anthropic-ai/claude-cli` or see Anthropic docs
- **Codex**: `npm install -g @openai/codex`
- **Gemini**: Install the Gemini CLI from Google
- **OpenCode**: Install from OpenCode
- **Cursor**: Use Cursor IDE's built-in agent
- **Kimi**: Install Kimi CLI
- **Pi**: Install Pi CLI

---

## 6. Configuration Basics

Ralph uses a two-layer JSON configuration system:

### Configuration Locations

| Location | Purpose | Precedence |
|----------|---------|------------|
| `~/.config/ralph/config.json` | Global defaults | Lower |
| `.ralph/config.json` | Project-specific settings | Higher |
| CLI flags | One-time overrides | Highest |

### Essential Configuration

A minimal effective configuration:

```json
{
  "version": 1,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "phases": 3,
    "ci_gate_enabled": true,
    "git_commit_push_enabled": false
  },
  "queue": {
    "file": ".ralph/queue.json",
    "done_file": ".ralph/done.json"
  }
}
```

### Key Configuration Options

**Agent Settings:**

```json
{
  "agent": {
    "runner": "claude",           // Default runner
    "model": "sonnet",            // Default model
    "phases": 3,                  // Default phase count (1, 2, or 3)
    "iterations": 1,              // Iterations per task
    "reasoning_effort": "medium", // Codex: low/medium/high/xhigh
    "ci_gate_enabled": true,      // Run make ci before completion
    "ci_gate_command": "make ci", // CI command to run
    "git_commit_push_enabled": false,  // Auto-commit/push on completion
    "git_revert_mode": "ask",     // ask/enabled/disabled
    "update_task_before_run": false    // Auto-update task before run
  }
}
```

**Queue Settings:**

```json
{
  "queue": {
    "file": ".ralph/queue.json",
    "done_file": ".ralph/done.json",
    "id_prefix": "RQ",
    "id_width": 4,
    "auto_archive_terminal_after_days": 7
  }
}
```

### Viewing Current Configuration

```bash
# Show resolved configuration
ralph config show

# Show as JSON for scripting
ralph config show --format json

# Show file paths
ralph config paths
```

### Configuration Profiles

Ralph includes built-in profiles for quick workflow switching:

| Profile | Runner | Model | Phases | Use Case |
|---------|--------|-------|--------|----------|
| `quick` | Kimi | kimi-for-coding | 1 | Fast fixes |
| `thorough` | Claude | opus | 3 | Deep review |

Use a profile:

```bash
ralph run one --profile quick
ralph scan --profile thorough "security audit"
```

Define custom profiles:

```json
{
  "profiles": {
    "my-profile": {
      "runner": "codex",
      "model": "gpt-5.3-codex",
      "phases": 2,
      "reasoning_effort": "high"
    }
  }
}
```

---

## 7. Daily Workflow

### Typical Daily Session

```bash
# 1. Start your day - check the queue
ralph queue list

# 2. macOS (optional): open the app UI
ralph app open

# 3. Add tasks from code review or planning
ralph task "Fix race condition in worker pool"
ralph task "Update API documentation"

# 4. Run specific high-priority tasks
ralph run one --task-id RQ-0005

# 5. End of day - archive completed work
ralph queue archive
```

### CLI Quick Reference

| Command | Description |
|---------|-------------|
| `ralph app open` | Open the macOS app UI |
| `ralph run one` | Run next task |
| `ralph run one --task-id RQ-0001` | Run specific task |
| `ralph run loop` | Run tasks continuously |
| `ralph task "title"` | Create new task |
| `ralph queue list` | List all tasks |
| `ralph queue next` | Show next runnable task |
| `ralph queue archive` | Move done tasks to archive |
| `ralph doctor` | Check environment health |
| `ralph scan "focus"` | Auto-generate tasks |

### Managing Tasks

**Creating good tasks:**

```bash
# Good: Clear, actionable title
ralph task "Add JWT authentication middleware"

# Better: With context
ralph task "Add JWT authentication middleware" \
  --request "Implement JWT token validation in the auth middleware. Use the existing user model." \
  --scope "src/middleware/auth.rs" \
  --priority high

# Best: With evidence/plan
ralph task "Add JWT authentication middleware" \
  --request "Implement JWT token validation..." \
  --evidence "Current auth uses session cookies, need JWT for API" \
  --scope "src/middleware/auth.rs,src/models/user.rs" \
  --priority high \
  --tag security \
  --tag api
```

**Task Dependencies:**

```bash
# Create tasks that depend on others
ralph task "Implement login endpoint" --tag auth
# Returns: RQ-0001

ralph task "Add password reset" \
  --depends-on RQ-0001 \
  --tag auth
```

**Scheduling Tasks:**

```bash
# Schedule for future execution
ralph task "Deploy to production" \
  --scheduled-start "2026-02-15T09:00:00Z"

# Or use relative time
ralph task "Weekly dependency update" \
  --scheduled-start "+7d"
```

### Git Workflow Integration

Ralph works best with a clean git workflow:

```bash
# 1. Ensure working directory is clean
git status

# 2. Run tasks (Ralph will create commits if enabled)
ralph run loop

# 3. Review changes
git log --oneline -5

# 4. Push when satisfied
git push
```

**Auto-commit configuration:**

```json
{
  "agent": {
    "git_commit_push_enabled": true
  }
}
```

⚠️ **Warning**: Enable auto-commit only when you're comfortable with automated git operations.

---

## 8. Next Steps

Now that you're up and running, here's where to go next:

### Learn More

- **[CLI Reference](../cli.md)** - Complete command documentation
- **[Configuration](../configuration.md)** - All configuration options
- **[Queue and Tasks](../queue-and-tasks.md)** - Task management details
- **[Workflow](../workflow.md)** - Deep dive into the 3-phase workflow
- **[App (macOS)](../features/app.md)** - macOS SwiftUI app guide

### Advanced Features

**Scan for Tasks:**

Automatically discover tasks in your codebase:

```bash
# Find maintenance issues
ralph scan --mode maintenance "code quality gaps"

# Find feature opportunities
ralph scan --mode innovation "missing features"
```

**Parallel Execution:**

Run multiple tasks concurrently (CLI only):

```bash
ralph run loop --parallel 4 --max-tasks 10
```

**Daemon Mode:**

Run Ralph continuously in the background:

```bash
# Start daemon
ralph daemon start

# Check status
ralph daemon status

# Stop daemon
ralph daemon stop
```

**PRD to Tasks:**

Convert Product Requirements Documents into tasks:

```bash
ralph prd convert requirements.md
```

### Best Practices

1. **Start small**: Begin with 1-phase tasks to get familiar
2. **Review plans**: Always check Phase 1 plans before implementation
3. **Use the app (macOS)**: Keep the queue visible while you work
4. **Archive regularly**: Keep your queue clean with `ralph queue archive`
5. **Run doctor**: Check `ralph doctor` if something seems off
6. **Version control**: Keep your `.ralph/` directory in git
7. **CI gate**: Always ensure `make ci` passes before considering work done

### Getting Help

- **Check the docs**: Start with `docs/index.md`
- **Run doctor**: `ralph doctor` diagnoses common issues
- **Validate queue**: `ralph queue validate` checks for problems
- **Verbose output**: Use `--verbose` flag for more details

### Community

- **Issues**: Report bugs or request features
- **Contributing**: See `CONTRIBUTING.md` for guidelines
- **Security**: See `SECURITY.md` for vulnerability reporting

---

## Quick Reference Card

```
┌────────────────────────────────────────────────────────────────┐
│ RALPH QUICK REFERENCE                                          │
├────────────────────────────────────────────────────────────────┤
│ INSTALL    cargo install ralph                                 │
│ INIT       ralph init                                          │
│ APP (macOS) ralph app open                                     │
│ RUN        ralph run one        # next task                    │
│            ralph run loop       # continuous                   │
│ TASK       ralph task "title"                                  │
│ LIST       ralph queue list                                    │
│ ARCHIVE    ralph queue archive                                 │
│ DOCTOR     ralph doctor                                        │
├────────────────────────────────────────────────────────────────┤
│ PHASES     --phases 1 (quick)  --phases 2 (plan+impl)          │
│            --phases 3 (full)   --quick (1-phase shorthand)     │
├────────────────────────────────────────────────────────────────┤
│ RUNNERS    --runner claude|codex|gemini|opencode|cursor|kimi|pi │
│            --model <model-id>  --effort low|medium|high|xhigh   │
└────────────────────────────────────────────────────────────────┘
```

---

Happy automating! 🤖
