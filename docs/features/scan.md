# Ralph Scan System
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Feature Documentation](README.md)


![Repository Scan](../assets/images/2026-02-07-11-32-24-scan.png)

The scan system automatically identifies opportunities, issues, and improvements in your repository by dispatching an AI agent to analyze the codebase. Unlike manual task creation, scanning explores the repository systematically to discover work you might not have explicitly recognized.

---

## Overview

Scanning is Ralph's **discovery mechanism** for finding work that should be done. It uses an autonomous AI agent to:

- Explore the repository structure and code
- Identify bugs, gaps, and opportunities
- Generate properly structured tasks in the queue
- Prioritize findings based on impact and severity

### Use Cases

| Scenario | How Scan Helps |
|----------|----------------|
| **Onboarding to a codebase** | Quickly identify critical issues, technical debt, and missing documentation |
| **Release preparation** | Find bugs, security issues, and workflow gaps before shipping |
| **Quarterly planning** | Discover feature gaps and enhancement opportunities for roadmap planning |
| **Security audits** | Systematically identify security vulnerabilities and unsafe patterns |
| **Performance optimization** | Locate performance bottlenecks and optimization opportunities |
| **Code maintenance** | Find dead code, duplicated logic, and maintainability issues |
| **Pre-flight checks** | Validate that CI, tests, and development workflows are functioning |

### Key Benefits

- **Autonomous discovery**: The agent explores beyond what you explicitly ask for
- **Evidence-based**: Every finding includes concrete evidence from the codebase
- **Actionable output**: Results are structured as ready-to-work tasks in the queue
- **Prioritized**: Tasks are ranked by impact (critical → low)
- **Deduplicated**: The agent checks existing tasks to avoid duplicates

---

## Scan Modes

Scan mode determines the **focus and evaluation criteria** the agent uses when analyzing your repository.

### Maintenance Mode (`--mode maintenance`)

**Purpose**: Find bugs, workflow gaps, design flaws, and violations

**Best for**:
- Security audits
- Pre-release bug hunts
- Technical debt assessment
- CI/workflow validation
- Code hygiene improvements

**Evaluation Rubric**:
The maintenance agent evaluates findings against engineering principles:

| Principle | What the Agent Looks For |
|-----------|-------------------------|
| **KISS** | Unnecessary complexity, overengineering, needless indirection |
| **YAGNI** | Dead code, unused flags/config, speculative abstractions |
| **DRY** | Duplicated rules/validation, multiple sources of truth |
| **SoC/SRP** | Tangled responsibilities, scattered changes required |
| **Bugs-hard-to-write** | Representable illegal states, weak boundary validation |
| **Fail fast** | Swallowed errors, silent fallbacks, ambiguous retries |
| **Least astonishment** | Surprising behavior, hidden IO, inconsistent naming |
| **Operational hygiene** | Logging/metrics gaps, confusing failure modes, flaky tests |
| **Security baseline** | Unsafe defaults, injection risks, secrets handling issues |
| **Consistency/Integrity** | Documentation-code mismatches, incomplete edge case handling |

**Example**:
```bash
# Full maintenance scan
ralph scan --mode maintenance

# Maintenance scan with specific focus
ralph scan --mode maintenance "security vulnerabilities in authentication"

# Maintenance check using a fast-local profile
ralph scan --mode maintenance --profile fast-local "CI workflow gaps"
```

### Innovation Mode (`--mode innovation`)

**Purpose**: Find feature gaps, use-case completeness issues, and enhancement opportunities

**Best for**:
- Roadmap planning
- Feature gap analysis
- UX improvement discovery
- Competitive analysis
- Modernization opportunities

**Evaluation Rubric**:
The innovation agent evaluates opportunities using product lenses:

| Lens | What the Agent Assesses |
|------|------------------------|
| **User value** | Does it remove friction, increase output, reduce steps? |
| **Coverage** | Does it close a real workflow gap end-to-end? |
| **Differentiation** | Does it add hard-to-copy or unusually effective capability? |
| **Reliability + safety** | Does it reduce failure rates and make outcomes predictable? |
| **Time-to-ship** | Can it be delivered incrementally? |
| **Cost/Performance** | Does it reduce compute, latency, or maintenance burden? |
| **Simplicity** | Does it reduce complexity while increasing capability? |

**Example**:
```bash
# Full innovation scan
ralph scan --mode innovation

# Innovation scan with specific focus
ralph scan --mode innovation "CLI ergonomics and UX improvements"

# Deep innovation analysis using a deep-review profile
ralph scan --mode innovation --profile deep-review "missing webhook integrations"
```

### General Mode (Default)

**Purpose**: User-guided scan without maintenance or innovation specific criteria

**When Used**:
- When you provide a focus prompt without specifying `--mode`
- When you want the agent to use task-building instructions without rubric constraints

**Example**:
```bash
# General scan with focus (no --mode flag)
ralph scan "production readiness gaps"
ralph scan "evaluate error handling patterns"
ralph scan "identify missing test coverage"
```

---

## Command Usage

### Basic Syntax

```bash
ralph scan [OPTIONS] [PROMPT]...
```

### Arguments

| Argument | Description |
|----------|-------------|
| `[PROMPT]...` | Optional focus prompt as positional arguments (alternative to `--focus`) |

### Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--focus <TEXT>` | | Focus prompt to guide the scan (backward compatible) |
| `--mode <MODE>` | `-m` | Scan mode: `maintenance`, `innovation`, or `general` |
| `--profile <NAME>` | | Named config profile to apply (e.g., `fast-local`, `deep-review`) |
| `--runner <RUNNER>` | | Override runner for this scan |
| `--model <MODEL>` | | Override model for this scan |
| `--effort <LEVEL>` | `-e` | Reasoning effort: `low`, `medium`, `high`, `xhigh` (Codex and Pi only) |
| `--repo-prompt <MODE>` | | RepoPrompt mode: `tools`, `plan`, `off` |
| `--force` | `-f` | Bypass clean-repo check and stale locks |

### Runner CLI Override Flags

| Flag | Description |
|------|-------------|
| `--approval-mode <MODE>` | Runner approval: `default`, `auto-edits`, `yolo`, `safe` |
| `--sandbox <MODE>` | Sandbox mode: `default`, `enabled`, `disabled` |
| `--verbosity <LEVEL>` | Output verbosity: `quiet`, `normal`, `verbose` |
| `--plan-mode <MODE>` | Plan mode: `default`, `enabled`, `disabled` |
| `--output-format <FMT>` | Output format: `stream-json`, `json`, `text` |
| `--unsupported-option-policy <POL>` | Policy for unsupported options: `ignore`, `warn`, `error` |

### Examples

```bash
# Basic scans
ralph scan                                                          # Error: needs mode or focus
ralph scan --mode maintenance                                       # Full maintenance scan
ralph scan --mode innovation                                        # Full innovation scan
ralph scan "production readiness gaps"                              # General mode with focus

# With focus prompt
ralph scan --focus "security audit"                                 # Using --focus flag
ralph scan --mode maintenance "security audit"                      # Maintenance + focus
ralph scan --mode innovation "feature gaps for CLI"                 # Innovation + focus

# With configured profiles
ralph scan --profile fast-local "quick bug fixes"                   # Fast custom profile
ralph scan --profile deep-review "deep risk audit"                  # Deep custom profile

# With runner overrides
ralph scan --runner opencode --model gpt-5.3 "CI and safety gaps"
ralph scan --runner gemini --model gemini-3-flash-preview "risk audit"
ralph scan --runner codex --model gpt-5.3-codex --effort high "queue correctness"
ralph scan --runner claude --model opus "complex architecture review"

# With approval and safety settings
ralph scan --approval-mode auto-edits --runner claude "auto edits review"
ralph scan --sandbox disabled --runner codex "sandbox audit"
ralph scan --force "scan even with uncommitted changes"

# With RepoPrompt
ralph scan --repo-prompt plan "Deep codebase analysis"              # Plan + tools mode
ralph scan --repo-prompt tools "Tool-guided scan"                   # Tools only mode
ralph scan --repo-prompt off "Quick surface scan"                   # No RepoPrompt

# Combining options
ralph scan --mode maintenance --profile deep-review --runner claude \
           --model opus "comprehensive security audit"
```

---

## Focus Prompt

The focus prompt guides the scan agent toward specific areas of interest. While the agent will explore broadly, the focus prompt helps prioritize and contextualize findings.

### Writing Effective Focus Prompts

**Good focus prompts**:
- Are specific about scope or domain
- Mention concrete concerns (security, performance, UX)
- Reference specific subsystems or features
- Include constraints or requirements

**Examples by Use Case**:

```bash
# Security-focused
ralph scan --mode maintenance "authentication and authorization vulnerabilities"
ralph scan --mode maintenance "secrets handling and injection risks"
ralph scan --mode maintenance "input validation and sanitization gaps"

# Performance-focused
ralph scan --mode maintenance "database query performance and N+1 issues"
ralph scan --mode innovation "caching opportunities and lazy loading"

# Architecture-focused
ralph scan --mode maintenance "error handling consistency across the codebase"
ralph scan --mode innovation "API design gaps and missing endpoints"

# Workflow-focused
ralph scan --mode maintenance "CI/CD pipeline reliability and failure modes"
ralph scan --mode innovation "developer experience improvements"

# Feature-focused
ralph scan --mode innovation "missing webhook event types"
ralph scan --mode innovation "CLI command completeness compared to competitors"
```

### Focus vs Mode

| | Mode | Focus |
|--|------|-------|
| **Purpose** | Defines evaluation criteria | Narrows scope of exploration |
| **Required** | No (defaults to `general` if focus provided) | No (but recommended) |
| **Example** | `--mode maintenance` | `"security audit"` |
| **Effect** | Changes rubric for findings | Guides where to look |

You can use **both** together for targeted analysis:
```bash
# Use maintenance criteria, focused on auth system
ralph scan --mode maintenance "authentication system bugs"

# Use innovation criteria, focused on API gaps
ralph scan --mode innovation "REST API completeness"
```

---

## Runner Selection

Different runners have different strengths for scanning. Choose based on your needs:

### Runner Comparison for Scanning

| Runner | Best For | Reasoning |
|--------|----------|-----------|
| **Codex** | Deep analysis, complex bugs | Strong reasoning, configurable effort levels |
| **Claude** | Architecture review, nuanced findings | Excellent for understanding context and implications |
| **Gemini** | Fast initial scans, large codebases | Fast processing, good for broad exploration |
| **OpenCode** | Quick scans, familiar patterns | Good for common issue patterns |
| **Kimi** | Long-context scans | Handles large repositories well |

### Model Selection Guidelines

**Maintenance scans** (finding bugs):
- Use high-reasoning models (Codex with `--effort high`, Claude Opus)
- Bugs often require deep analysis to identify correctly

**Innovation scans** (finding features):
- Balanced models work well (Gemini, Claude Sonnet)
- Feature gaps are often visible at surface level

**Quick scans**:
- Use a fast custom profile such as `fast-local`
- Good for initial exploration or frequent scanning

**Deep scans**:
- Use a deep custom profile such as `deep-review`
- Best for critical audits before releases

### Examples

```bash
# Deep security audit with high reasoning
ralph scan --mode maintenance --runner codex --effort high \
           "security vulnerabilities"

# Fast initial exploration
ralph scan --mode innovation --runner gemini \
           --model gemini-3-flash-preview "feature opportunities"

# Architecture review with nuanced analysis
ralph scan --mode maintenance --runner claude --model opus \
           "design patterns and coupling issues"
```

---

## RepoPrompt Integration

RepoPrompt provides repository context to the scanning agent through an MCP (Model Context Protocol) server.

### RepoPrompt Modes

| Mode | Behavior | Best For |
|------|----------|----------|
| `tools` | Injects RepoPrompt tool reminders | Ensuring agent uses available tools |
| `plan` | Injects planning requirements + tool reminders | Complex scans requiring structured approach |
| `off` | Disables RepoPrompt integration | Faster scans when context isn't needed |

### Using `--repo-prompt`

```bash
# Enable RepoPrompt tools mode (default when configured)
ralph scan --repo-prompt tools "scan with tool access"

# Enable planning mode for complex analysis
ralph scan --repo-prompt plan "deep architectural review"

# Disable RepoPrompt for faster scan
ralph scan --repo-prompt off "quick surface scan"
```

**INTENDED BEHAVIOR**: The `--repo-prompt` flag should control whether RepoPrompt tool reminders and planning requirements are injected into the scan prompt.

**CURRENTLY IMPLEMENTED BEHAVIOR**: The flag controls `repoprompt_tool_injection` which wraps the prompt with RepoPrompt requirements via `prompts::wrap_with_repoprompt_requirement()`.

---

## Prompt Templates

Scan prompts are rendered from templates that define the agent's mission, evaluation criteria, and output format.

### Template Versions

| Version | Templates Available | Default |
|---------|---------------------|---------|
| V1 | `scan_maintenance_v1.md`, `scan_innovation_v1.md` | No |
| V2 | `scan_general_v2.md`, `scan_maintenance_v2.md`, `scan_innovation_v2.md` | **Yes** |

Configure in `.ralph/config.jsonc`:
```json
{
  "agent": {
    "scan_prompt_version": "v2"
  }
}
```

### Template Selection Matrix

| Mode | V1 Template | V2 Template |
|------|-------------|-------------|
| `general` | N/A | `scan_general_v2.md` |
| `maintenance` | `scan_maintenance_v1.md` | `scan_maintenance_v2.md` |
| `innovation` | `scan_innovation_v1.md` | `scan_innovation_v2.md` |

### Customizing Scan Prompts

You can override the default templates by creating files in `.ralph/prompts/`:

```
.ralph/
  prompts/
    scan_general_v2.md        # General scan mode (v2)
    scan_maintenance_v1.md    # Maintenance scan mode (v1)
    scan_maintenance_v2.md    # Maintenance scan mode (v2)
    scan_innovation_v1.md     # Innovation scan mode (v1)
    scan_innovation_v2.md     # Innovation scan mode (v2)
```

Use the filename that matches the active `scan_prompt_version` and scan mode.

**Important**: Custom templates must preserve required placeholders:
- `{{USER_FOCUS}}` - The focus prompt (normalized to "(none)" if empty)
- `{{PROJECT_TYPE_GUIDANCE}}` - Project type specific guidance

### Template Variables

Templates support variable expansion via `{{VARIABLE_NAME}}` syntax:

| Variable | Description |
|----------|-------------|
| `{{USER_FOCUS}}` | The user's focus prompt |
| `{{PROJECT_TYPE_GUIDANCE}}` | Guidance based on `project_type` config |

Custom variables can be defined in config:
```json
{
  "prompt_variables": {
    "TEAM_STANDARDS": "See docs/standards.md"
  }
}
```

---

## Output

### What Scan Produces

After a successful scan, the agent:

1. **Adds tasks to `.ralph/queue.jsonc`**
   - Tasks are inserted near the top in priority order
   - Higher priority tasks come first
   - Includes evidence, plan, scope, and tags

2. **Logs results to console**
   ```
   Scan added 12 task(s):
   - RQ-0045: Fix unsafe deserialization in auth module
   - RQ-0046: Add input validation for user preferences
   - RQ-0047: Resolve race condition in queue operations
   ...
   ```

3. **Backfills metadata**
   - Sets `request` field to `"scan: <focus>"`
   - Sets `created_at` and `updated_at` timestamps
   - Adds appropriate tags (`maintenance`, `innovation`, or `scan`)

4. **Validates queue integrity**
   - Ensures edited queue passes `ralph queue validate`
   - Rejects invalid task relationships or malformed fields

### Task Fields Set by Scan

| Field | Value |
|-------|-------|
| `id` | Auto-generated via `ralph queue next-id` |
| `status` | `"todo"` |
| `priority` | `critical`, `high`, `medium`, or `low` |
| `title` | Outcome-sized description |
| `description` | Detailed context and goal |
| `tags` | Includes `"maintenance"`, `"innovation"`, or `"scan"` |
| `scope` | Relevant file paths and commands |
| `evidence` | Concrete findings from codebase |
| `plan` | Specific steps to resolve |
| `request` | `"scan: <focus>"` (backfilled when missing) |
| `custom_fields.scan_agent` | `"scan-maintenance"`, `"scan-innovation"`, or `"scan-general"` |

### Minimum Task Generation

**INTENDED AND IMPLEMENTED BEHAVIOR**:
- V1 templates: Minimum 7 tasks.
- V2 templates: Target 10+ meaningful tasks when supported by evidence.
- In V2, quality beats quota: if fewer than 10 verifiable findings exist, the scan should return fewer and explain why.

### Deduplication

The scan agent checks for existing tasks before adding new ones:
- Searches by similar title keywords
- Checks overlapping scope paths
- Matches by tags

If a duplicate is found, the agent skips adding it and reports this in the output.

---

## Best Practices

### When to Use Each Mode

**Use Maintenance Mode when**:
- Preparing for a release
- Investigating reliability issues
- Auditing security
- Onboarding to an unfamiliar codebase
- CI/workflow is failing
- You want to reduce technical debt

**Use Innovation Mode when**:
- Planning quarterly roadmap
- Competitor analysis
- User feedback indicates missing features
- Modernizing old code
- Exploring new capabilities

**Use General Mode when**:
- You have a specific, well-defined concern
- The rubric constraints of maintenance/innovation don't apply
- You want the agent to use task-building instructions freely

### Effective Focus Prompts

**DO**:
```bash
# Be specific about domain
ralph scan --mode maintenance "error handling in async code"

# Include context about concerns
ralph scan --mode innovation "missing CLI flags compared to similar tools"

# Reference specific subsystems
ralph scan --mode maintenance "database transaction handling"
```

**DON'T**:
```bash
# Too vague
ralph scan --mode maintenance "fix stuff"

# Too narrow (agent needs room to explore)
ralph scan --mode maintenance "line 42 of main.rs"

# Implementation detail (scan finds problems, doesn't implement)
ralph scan --mode maintenance "add null checks"
```

### Scanning Workflow

1. **Initial scan** (when onboarding):
   ```bash
   ralph scan --mode maintenance --profile deep-review "comprehensive codebase review"
   ```

2. **Regular maintenance** (weekly/bi-weekly):
   ```bash
   ralph scan --mode maintenance --profile fast-local "recent changes review"
   ```

3. **Pre-release audit**:
   ```bash
   ralph scan --mode maintenance --profile deep-review "release readiness"
   ralph scan --mode innovation --profile deep-review "missing features for launch"
   ```

4. **Roadmap planning** (quarterly):
   ```bash
   ralph scan --mode innovation --profile deep-review "strategic opportunities"
   ```

### Profile Selection

| Goal | Profile | Why |
|------|---------|-----|
| Quick check | `fast-local` | Fast feedback with a lightweight custom profile |
| Deep audit | `deep-review` | Comprehensive analysis with a deeper custom profile |
| Daily scan | None (default) | Balanced approach |
| Critical path | `deep-review` | Maximum thoroughness for important decisions |

### Safety Considerations

- **Clean repo check**: Scan requires a clean repository (except for `.ralph/queue.jsonc` and `.ralph/done.jsonc`)
- **Git revert on failure**: If scan fails, changes are automatically reverted
- **Queue validation**: Queue is validated before and after scanning
- **Force flag**: Use `--force` to bypass clean-repo check if needed

---

## Scan vs Task Build

Both `ralph scan` and `ralph task build` create tasks automatically, but they serve different purposes:

| Aspect | `ralph scan` | `ralph task build` |
|--------|--------------|-------------------|
| **Primary Use** | Discover unknown issues/opportunities | Create known refactoring tasks |
| **Agent Role** | Exploratory - finds problems | Directed - creates specific tasks |
| **Input** | Mode + optional focus | Specific command + parameters |
| **Output** | Variable number of tasks based on findings | Predictable number based on parameters |
| **Knowledge Required** | None - agent discovers | You know what needs refactoring |

### When to Use Scan

Use `ralph scan` when:
- You don't know what problems exist
- You want a comprehensive audit
- You're exploring a new codebase
- You need prioritized findings
- You want evidence-based task descriptions

### When to Use Task Build

Use `ralph task build` when:
- You know files need refactoring (large files, complex modules)
- You want systematic refactoring tasks
- You're doing codebase maintenance with clear scope
- You need predictable task counts

### Examples

```bash
# SCAN: Discover what needs work
ralph scan --mode maintenance "performance issues"
# Result: Agent explores and finds 8 performance bottlenecks

# TASK BUILD: Create tasks for known work
ralph task build-refactor --threshold 1000
# Result: Creates 5 tasks for files exceeding 1000 LOC

# SCAN: Find security issues
ralph scan --mode maintenance "security vulnerabilities"
# Result: Agent discovers 3 security issues

# TASK BUILD: Refactor specific module
ralph task build-refactor --path crates/ralph/src/auth
# Result: Creates tasks for large files in auth module
```

---

## Advanced Usage

### Combining with Workflows

**Scan → Triage → Execute**:
```bash
# 1. Run scan
ralph scan --mode maintenance "pre-release audit"

# 2. Review and triage tasks
ralph queue list --status todo

# 3. Prioritize critical tasks
ralph queue sort  # Reorders by priority

# 4. Execute
ralph run loop --max-tasks 5
```

**Continuous Monitoring**:
```bash
# Add to CI pipeline (dry-run to preview)
ralph scan --mode maintenance --profile fast-local "CI health check"
```

### Parallel Scanning (Future)

While parallel execution is available for `ralph run loop --parallel`, scans are currently sequential. For large codebases, consider:
- Using a fast custom profile such as `fast-local` for faster results
- Focusing on specific subdirectories via focus prompt
- Running separate scans for different subsystems

### Integration with External Tools

Scan-generated tasks include rich metadata that can be exported:

```bash
# Export scan findings for external tracking
ralph queue export --format json --tag maintenance > audit-findings.json

# Import into another Ralph instance
ralph queue import --format json --input audit-findings.json
```

---

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "Please provide one of: A focus prompt, A scan mode, or Both" | No mode or focus specified | Add `--mode` or a focus prompt |
| Clean repo check fails | Uncommitted changes | Commit changes or use `--force` |
| Scan validation failed | Queue state issue | Run `ralph queue validate` |
| No tasks generated | Agent found no issues | Try broader focus or different mode |
| Too many low-value tasks | Focus too broad | Narrow focus prompt |

### Debug Mode

Enable debug mode for troubleshooting:
```bash
ralph scan --mode maintenance --debug "debug scan"
```

This saves raw (unredacted) output to `.ralph/logs/debug.log` for investigation.

---

## Summary

The Ralph Scan System is your **autonomous discovery engine** for codebase improvement. By choosing the right mode, crafting effective focus prompts, and selecting appropriate runners, you can systematically identify bugs, gaps, and opportunities that might otherwise go unnoticed.

**Key takeaways**:
- Use `--mode maintenance` for bugs and technical debt
- Use `--mode innovation` for features and improvements
- Use focus prompts to guide exploration
- Leverage profiles such as `fast-local` and `deep-review` for appropriate depth
- Review and prioritize scan results before execution
- Integrate scanning into your regular workflow for continuous improvement
