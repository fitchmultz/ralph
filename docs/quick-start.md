# Quick Start Guide

Get up and running with Ralph in minutes.

## Installation

### From crates.io (recommended)

```bash
cargo install ralph
```

This installs the latest published version from crates.io.

### From source

```bash
# Clone the repository
git clone <repository-url>
cd ralph

# Build and install
make install
```

This installs the `ralph` binary to `~/.local/bin/ralph` (or a writable fallback path).

## Initialize Your Project

Navigate to your project directory and run:

```bash
cd your-project
ralph init
```

This launches an interactive wizard that will:

1. **Help you choose an AI runner**: Claude, Codex, OpenCode, Gemini, or Cursor
2. **Select a model**: Pick the best model for your chosen runner
3. **Explain the workflow modes**:
   - **3-phase (Full)**: Plan → Implement + CI → Review + Complete [Recommended]
   - **2-phase (Standard)**: Plan → Implement (faster, less review)
   - **1-phase (Quick)**: Single-pass execution (simple fixes only)
4. **Create your first task**: Optionally add your first task to get started

### Non-Interactive Mode

For CI/CD or scripts, skip the wizard:

```bash
ralph init --non-interactive
```

## Your First Task

After initialization, you have several options:

### Launch the Interactive UI

```bash
ralph tui
```

The TUI provides a visual interface for:
- Viewing and managing tasks
- Running tasks with a single keystroke
- Editing task fields
- Creating new tasks

### Run Your First Task

```bash
# Run the next task in the queue
ralph run one

# Or run in loop mode until all tasks are complete
ralph run loop
```

### View the Queue

```bash
# List all tasks
ralph queue list

# Show the next task
ralph queue next --with-title
```

## Understanding the 3-Phase Workflow

Ralph uses a structured workflow to ensure quality:

- **Phase 1: Planning**: The agent analyzes the task and creates a detailed implementation plan
- **Phase 2: Implementation**: The agent executes the plan and runs the CI gate
- **Phase 3: Review**: Final review and completion steps

Choose fewer phases for quick fixes, more phases for complex features.

## Runner Comparison

Ralph supports multiple AI runners. Choose based on your needs:

| Runner | Best For | Model Options | Notes |
|--------|----------|---------------|-------|
| **Claude** | General purpose, reasoning | `sonnet` (default), `opus`, or arbitrary IDs | Full tool use support, excellent for complex tasks |
| **Codex** | Code generation, OpenAI ecosystem | `gpt-5.2-codex`, `gpt-5.2` only | Reasoning effort control (`low` to `xhigh`) |
| **OpenCode** | Flexible model selection | Arbitrary model IDs (e.g., `zai-coding-plan/glm-4.7`) | Good for custom model endpoints |
| **Gemini** | Google ecosystem, cost efficiency | `gemini-3-pro-preview`, `gemini-3-flash-preview`, or arbitrary IDs | Fast, good for quick iterations |
| **Cursor** | Cursor IDE users | Uses Cursor's `agent` binary | Integrates with Cursor workflow |

## Phase Mode Comparison

| Mode | Phases | Best For | Trade-off |
|------|--------|----------|-----------|
| **1-Phase** | Single pass | Quick fixes, simple refactors, typo corrections | Fastest, but no planning or review |
| **2-Phase** | Plan → Implement | Medium complexity tasks where review is less critical | Faster than 3-phase, skips formal review |
| **3-Phase** | Plan → Implement → Review | Complex features, architectural changes, production code | Slowest, but highest quality and safety |

## Creating Tasks

### From the CLI

```bash
ralph task "Add user authentication feature"
```

### With Details

```bash
ralph task "Refactor database layer" --request "Move all database access code into a dedicated module"
```

### From the TUI

Press `n` in the TUI to create a new task interactively.

## Configuration

The wizard creates `.ralph/config.json` with your selections. You can customize:

```json
{
  "version": 1,
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "phases": 3,
    "iterations": 1
  }
}
```

See `docs/configuration.md` for all options.

## Common Workflows

### Daily Development

```bash
# Start the TUI
ralph tui

# Press Enter to run the next task
# Press 'l' to toggle loop mode
# Press 'a' to archive completed tasks
```

### Adding Tasks from Code Review

```bash
# Quick task creation
ralph task "Fix memory leak in parser"

# Or use the TUI for more detail
ralph tui
# Press 'n' to add a task
```

### Running Specific Tasks

```bash
# Run a specific task by ID
ralph run one --task-id RQ-0005

# Or find it in the TUI and press Enter
```

## Next Steps

- Read the full [CLI Reference](cli.md) for all commands
- Learn about [Queue and Task Management](queue-and-tasks.md)
- Configure [Runner Settings](configuration.md)
- Set up [AGENTS.md](index.md) for your project

## Troubleshooting

### "ralph: command not found"

Ensure `~/.local/bin` is in your PATH:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Wizard doesn't appear

If running in a non-TTY environment (like some CI systems), use:

```bash
ralph init --interactive  # Force wizard
# or
ralph init --non-interactive  # Skip wizard
```

### Check Your Setup

```bash
ralph doctor
```

This verifies:
- Git repository status
- Queue file validity
- Runner binary availability
- Configuration correctness
