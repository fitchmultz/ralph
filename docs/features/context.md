# Context / AGENTS.md System

![Project Context](../assets/images/2026-02-07-11-32-24-context.png)

The Context system in Ralph generates and maintains an `AGENTS.md` file that documents project conventions, build commands, testing guidelines, and workflow contracts for AI agents working on the codebase.

---

## Overview

`AGENTS.md` is a project-level documentation file specifically designed for AI agents. While `README.md` files are written for human contributors, `AGENTS.md` complements them by containing the extra context that coding agents need: build steps, tests, conventions, and workflows that might clutter a README or aren't relevant to human contributors.

### Why AGENTS.md?

- **Clear, predictable location**: Agents know exactly where to find project-specific guidance
- **Keep READMEs concise**: Human-focused documentation stays focused on quick starts and overviews
- **Precise, agent-focused guidance**: Technical conventions that complement existing documentation
- **Prompt injection**: Can be automatically injected into agent prompts for consistent context

### Key Features

- **Automatic project detection**: Detects Rust, Python, TypeScript, Go, or Generic projects
- **Template-based generation**: Project-type-specific templates with sensible defaults
- **Interactive wizard**: Guided setup for customizing commands and descriptions
- **Section-based updates**: Add new learnings without regenerating the entire file
- **Validation**: Ensure AGENTS.md stays complete and up-to-date

---

## Generating Context

### `ralph context init`

Generate an initial `AGENTS.md` file for your project.

```bash
# Auto-detect project type and generate AGENTS.md
ralph context init

# Force overwrite if AGENTS.md already exists
ralph context init --force

# Specify a project type explicitly
ralph context init --project-type rust

# Custom output location
ralph context init --output docs/AGENTS.md

# Interactive mode with guided prompts
ralph context init --interactive
```

#### Flags

| Flag | Description |
|------|-------------|
| `--force` | Overwrite existing AGENTS.md if it already exists |
| `--project-type <type>` | Override auto-detection (`rust`, `python`, `typescript`, `go`, `generic`) |
| `--output <PATH>` | Output path for AGENTS.md (default: `AGENTS.md` in repo root) |
| `--interactive` | Run interactive wizard to customize commands and description |

#### Interactive Mode

The interactive wizard guides you through:

1. **Project type selection**: Choose from detected or manual options
2. **Output path**: Use default or specify custom location
3. **Build/test commands**: Customize CI, build, test, lint, and format commands
4. **Project description**: Add a brief description of your project
5. **Preview and confirm**: Review before writing

```bash
$ ralph context init --interactive
? Select project type › Rust
? Use default output path (AGENTS.md)? › Yes
? Customize build/test commands? › No
? Add a project description? › Yes
? Project description › A CLI tool for managing task queues
? Preview and confirm before writing? › Yes
```

---

## Project Type Detection

Ralph automatically detects your project type based on files in the repository root:

| Project Type | Detection Criteria |
|--------------|-------------------|
| **Rust** | `Cargo.toml` exists |
| **Python** | `pyproject.toml`, `setup.py`, or `requirements.txt` exists |
| **TypeScript** | `package.json` exists |
| **Go** | `go.mod` exists |
| **Generic** | No specific markers detected |

Detection order: Rust → Python → TypeScript → Go → Generic

### Overriding Detection

Use `--project-type` to force a specific template:

```bash
# Force Rust template even in a mixed project
ralph context init --project-type rust

# Use generic template for custom project structures
ralph context init --project-type generic
```

---

## AGENTS.md Structure

The generated `AGENTS.md` follows a consistent structure across all project types:

### Required Sections

These sections must be present for validation to pass:

- **Non-Negotiables**: Rules that must always be followed
- **Repository Map**: Overview of codebase structure
- **Build, Test, and CI**: Commands for building, testing, and CI

### Recommended Sections

Additional sections for comprehensive documentation:

- **Testing**: Testing guidelines and patterns
- **Workflow Contracts**: How tasks are tracked and executed
- **Configuration**: Config precedence and key settings
- **Git Hygiene**: Commit message format and git practices
- **Documentation Maintenance**: When to update documentation
- **Troubleshooting**: Common issues and solutions

### Language-Specific Sections

Each project type includes relevant conventions:

**Rust**: Rust Conventions (formatting, visibility, error handling, cohesion)
**Python**: Python Conventions (formatting, typing, imports)
**TypeScript**: TypeScript Conventions (linting, types, imports)
**Go**: Go Conventions (formatting, idioms, error handling)

### Example Structure

```markdown
# Repository Guidelines (MyProject)

Brief project description.

## Non-Negotiables

- CI gate: `{ci_command}` MUST pass before claiming completion
- Source docs: every new/changed source file MUST start with module docs
- Tests: all new/changed behavior must be covered

## Repository Map

- `src/`: Source code
- `tests/`: Integration tests
- `docs/`: Documentation

## Build, Test, and CI

- `{ci_command}`: local CI gate
- `make test`: run all tests (for Rust repos this is commonly nextest workspace tests + explicit doc tests)
- `make build`: build the project

## Testing

- Unit tests: colocate with implementation
- Integration tests: use `tests/` directory

## Troubleshooting

- CI failing: run `{ci_command}`
- Test issues: check temp directory permissions

---
*Generated by Ralph v1.0.0 - 2026-01-28T12:34:56Z*
*Template version: 1*
```

---

## Updating Context

### `ralph context update`

Add new learnings to `AGENTS.md` without regenerating the entire file. Updates append content to existing sections.

```bash
# Interactive mode - select sections and add content
ralph context update --interactive

# Update specific sections from a file
ralph context update --file new_learnings.md

# Update specific sections (with file)
ralph context update --section troubleshooting --section git-hygiene --file updates.md

# Preview changes without writing
ralph context update --file updates.md --dry-run
```

#### Flags

| Flag | Description |
|------|-------------|
| `--section <NAME>` | Section to update (can be specified multiple times) |
| `--file <PATH>` | File containing new learnings to append |
| `--interactive` | Interactive mode to select sections and input content |
| `--dry-run` | Preview changes without writing to disk |
| `--output <PATH>` | Output path (default: existing AGENTS.md location) |

#### Interactive Update Mode

The interactive update wizard:

1. **Select sections**: Multi-select menu of existing sections
2. **Input method**: Choose editor (multi-line) or single-line input
3. **Add content**: Enter new content for each selected section
4. **Confirm**: Review and confirm before applying

```bash
$ ralph context update --interactive
? Select sections to update (Space to select, Enter to confirm) ›
◉ Non-Negotiables
◯ Repository Map
◉ Troubleshooting

? How would you like to add content to 'Non-Negotiables'? ›
  Type in editor (multi-line)

[Editor opens - add content and save]

? Update 2 section(s) with new content? › Yes
```

#### Update File Format

Update files use markdown sections that match AGENTS.md section names:

```markdown
## Troubleshooting

- New issue: description and solution
- Another issue: how to fix it

## Non-Negotiables

- New rule: always do this
```

Only sections specified with `--section` (or all sections if none specified) will be processed.

---

## Validating Context

### `ralph context validate`

Ensure `AGENTS.md` exists and contains required sections.

```bash
# Basic validation - check required sections
ralph context validate

# Strict validation - check recommended sections too
ralph context validate --strict

# Validate custom path
ralph context validate --path docs/AGENTS.md
```

#### Validation Criteria

**Standard Mode**:
- File exists
- Contains required sections: `Non-Negotiables`, `Repository Map`, `Build, Test, and CI`

**Strict Mode** (with `--strict`):
- All standard checks
- Plus all recommended sections: `Testing`, `Workflow Contracts`, `Configuration`, `Git Hygiene`, `Documentation Maintenance`, `Troubleshooting`

#### Exit Codes

- `0`: Validation passed
- Non-zero: Validation failed (missing sections or file not found)

---

## Context in Prompts

`AGENTS.md` can be injected into agent prompts to provide consistent project context for every task.

### Configuration

To enable AGENTS.md injection, add it to your configuration:

**`.ralph/config.jsonc`**:
```json
{
  "agent": {
    "instruction_files": ["AGENTS.md"]
  }
}
```

**`~/.config/ralph/config.jsonc`** (global):
```json
{
  "agent": {
    "instruction_files": ["~/path/to/global/agents.md"]
  }
}
```

### How Injection Works

When `AGENTS.md` is configured in `instruction_files`, Ralph:

1. Reads the file at prompt compilation time
2. Wraps it with a preamble indicating authoritative instructions
3. Prepends it to the worker prompt

Example injected prompt structure:

```markdown
## AGENTS / GLOBAL INSTRUCTIONS (AUTHORITATIVE)
The following instruction files are authoritative for this run. Follow them exactly.

### Source: AGENTS.md

# Repository Guidelines (MyProject)

[AGENTS.md content here]

---

[Rest of worker prompt...]
```

### Multiple Instruction Files

You can specify multiple instruction files for layered context:

```json
{
  "agent": {
    "instruction_files": [
      "AGENTS.md",
      "docs/security-guidelines.md",
      "~/global-coding-standards.md"
    ]
  }
}
```

Files are processed in order and separated by `---` dividers.

### Path Resolution

Instruction file paths support:

- **Relative paths**: Resolved from repository root (`AGENTS.md`)
- **Absolute paths**: Used as-is (`/path/to/file.md`)
- **Home directory**: `~` expanded to home directory (`~/global.md`)

### Validation

Ralph validates instruction files at config load time:

- File must exist
- Must be valid UTF-8
- Size limit: 1MB per file

Use `ralph doctor` to check instruction file configuration:

```bash
$ ralph doctor
✓ runner/agents_md: AGENTS.md configured and readable
```

---

## Best Practices

### Initial Setup

1. **Run `ralph context init` early** in your project to establish conventions
2. **Use `--interactive`** to customize commands for your project structure
3. **Review the generated file** before committing
4. **Commit AGENTS.md** to version control so all agents see the same guidelines

### Keeping AGENTS.md Current

1. **Update after significant changes**:
   ```bash
   ralph context update --interactive
   ```

2. **Add troubleshooting entries** as you encounter and solve issues:
   ```bash
   ralph context update --section troubleshooting
   ```

3. **Validate in CI** to ensure documentation stays complete:
   ```bash
   ralph context validate --strict
   ```

### Content Guidelines

1. **Be specific**: Include exact command names and flags
2. **Include failure modes**: Document what can go wrong and how to fix it
3. **Link to deeper docs**: Reference `docs/` for detailed information
4. **Keep it actionable**: Every item should guide agent behavior

### Integration with Workflow

1. **Inject into prompts** via `instruction_files` config for consistent context
2. **Reference in task plans**: When planning work, note which AGENTS.md sections apply
3. **Update when conventions change**: If you change build commands or patterns, update AGENTS.md

### Example Workflow

```bash
# Initialize AGENTS.md for new project
ralph context init --interactive

# Commit the initial version
git add AGENTS.md
git commit -m "Add AGENTS.md with project conventions"

# Later: add a new troubleshooting entry after fixing an issue
ralph context update --section troubleshooting

# Before major release: validate everything is documented
ralph context validate --strict
```

---

## Template Reference

Ralph includes embedded templates for each project type:

| Template | Location |
|----------|----------|
| Rust | `crates/ralph/assets/agents_templates/rust.md` |
| Python | `crates/ralph/assets/agents_templates/python.md` |
| TypeScript | `crates/ralph/assets/agents_templates/typescript.md` |
| Go | `crates/ralph/assets/agents_templates/go.md` |
| Generic | `crates/ralph/assets/agents_templates/generic.md` |

### Template Placeholders

Templates use these placeholders that are replaced at generation time:

| Placeholder | Description |
|-------------|-------------|
| `{project_name}` | Directory name of repository root |
| `{project_description}` | Description from interactive mode or default |
| `{repository_map}` | Auto-generated from detected directories/files |
| `{ci_command}` | CI command (default: `make ci`) |
| `{build_command}` | Build command (default: `make build`) |
| `{test_command}` | Test command (default: `make test`) |
| `{lint_command}` | Lint command (default: `make lint`) |
| `{format_command}` | Format command (default: `make format`) |
| `{package_name}` | Project name in kebab-case |
| `{module_name}` | Project name in snake_case |
| `{id_prefix}` | Task ID prefix from config (default: `RQ`) |
| `{version}` | Ralph version number |
| `{timestamp}` | Generation timestamp (RFC3339) |
| `{template_version}` | Template version for tracking |

---

## Troubleshooting

### AGENTS.md exists but is not being injected

Check that it's configured in your config file:

```bash
ralph config show --format json | jq '.agent.instruction_files'
```

If empty, add it:

```bash
# Edit .ralph/config.jsonc
{
  "agent": {
    "instruction_files": ["AGENTS.md"]
  }
}
```

### Validation fails for custom sections

The validator checks for specific section names. If you add custom sections:

- Standard mode: Only required sections must match exactly
- Strict mode: All recommended sections must match exactly

Custom sections are allowed but won't count toward validation.

### Interactive mode fails

Interactive mode requires a TTY terminal. In non-TTY environments (CI, scripts), use non-interactive flags:

```bash
# Instead of --interactive, use:
ralph context init --project-type rust
ralph context update --file updates.md
```

### Updates append rather than replace

The `update` command intentionally appends to preserve existing content. To replace content:

1. Use `ralph context init --force` to regenerate
2. Or manually edit the file

---

## See Also

- [CLI Reference](../cli.md#ralph-context) - Complete command documentation
- [Configuration](../configuration.md) - Config file format and options
- [Workflow](../workflow.md) - How tasks use AGENTS.md context
