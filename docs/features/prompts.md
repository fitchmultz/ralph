# Ralph Prompt System
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Feature Documentation](README.md)


![Prompt System](../assets/images/2026-02-07-11-32-24-prompts.png)

Purpose: Comprehensive guide to Ralph's prompt template system, including embedded defaults, override mechanisms, template variables, and prompt flow.

---

## Overview

Ralph uses a sophisticated prompt system to guide AI agents through task execution. The system is designed around these core principles:

1. **Embedded Defaults**: All default prompts are embedded in the Rust binary at compile time, ensuring Ralph works out-of-the-box without external dependencies.
2. **Repository Overrides**: Teams can customize prompts per repository by placing override files in `.ralph/prompts/`.
3. **Template Variables**: Dynamic placeholders (`{{TASK_ID}}`, `{{USER_REQUEST}}`, etc.) are replaced at runtime with context-specific values.
4. **Multi-Phase Composition**: Worker prompts are composed by combining base prompts with phase-specific wrappers.
5. **RepoPrompt Integration**: Optional integration with RepoPrompt tools for enhanced codebase exploration and planning.

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                     PROMPT RESOLUTION FLOW                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────┐    ┌──────────────────┐                   │
│  │  Embedded       │    │  Repository      │                   │
│  │  Defaults       │    │  Overrides       │                   │
│  │  (Binary)       │    │  (.ralph/prompts)│                   │
│  └────────┬────────┘    └────────┬─────────┘                   │
│           │                      │                              │
│           └──────────┬───────────┘                              │
│                      ▼                                          │
│           ┌─────────────────┐                                  │
│           │  Template       │                                  │
│           │  Loading        │                                  │
│           │  (with fallback)│                                  │
│           └────────┬────────┘                                  │
│                    │                                            │
│                    ▼                                            │
│           ┌─────────────────┐                                  │
│           │  Variable       │                                  │
│           │  Expansion      │                                  │
│           │  ({{VAR}})      │                                  │
│           └────────┬────────┘                                  │
│                    │                                            │
│                    ▼                                            │
│           ┌─────────────────┐                                  │
│           │  Prompt         │                                  │
│           │  Composition    │                                  │
│           │  (Phase wraps)  │                                  │
│           └────────┬────────┘                                  │
│                    │                                            │
│                    ▼                                            │
│           ┌─────────────────┐                                  │
│           │  Final Rendered │                                  │
│           │  Prompt         │                                  │
│           │  → Runner       │                                  │
│           └─────────────────┘                                  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Embedded Defaults

Default prompts are embedded in the Ralph binary using Rust's `include_str!` macro. They live in:

```
crates/ralph/assets/prompts/
├── worker.md                     # Base worker prompt
├── worker_phase1.md              # Phase 1 (planning) wrapper
├── worker_phase2.md              # Phase 2 (implementation, 2-phase)
├── worker_phase2_handoff.md      # Phase 2 handoff (3-phase)
├── worker_phase3.md              # Phase 3 (review) wrapper
├── worker_single_phase.md        # Single-pass execution
├── task_builder.md               # Task creation from user request
├── task_updater.md               # Task field updates
├── scan_general_v2.md            # General scan mode (v2)
├── scan_maintenance_v1.md        # Maintenance scan (v1)
├── scan_maintenance_v2.md        # Maintenance scan (v2)
├── scan_innovation_v1.md         # Innovation scan (v1)
├── scan_innovation_v2.md         # Innovation scan (v2)
├── merge_conflicts.md            # Merge conflict resolution
├── code_review.md                # Phase 3 code review body
├── completion_checklist.md       # Implementation completion
├── phase2_handoff_checklist.md   # Phase 2 handoff steps
└── iteration_checklist.md        # Refinement iteration steps
```

### Fallback Behavior

When loading a prompt, Ralph follows this resolution order:

1. Check for override at `.ralph/prompts/<name>.md`
2. If override exists → use it
3. If override missing → use embedded default
4. If override errors → propagate error (don't silently fall back)

This ensures that:
- New repositories work immediately with sensible defaults
- Teams can incrementally customize only the prompts they need
- Malformed override files are caught early

---

## Prompt Overrides

To customize prompts for a repository, create files in `.ralph/prompts/`:

```bash
mkdir -p .ralph/prompts

# Override the base worker prompt
cat > .ralph/prompts/worker.md << 'EOF'
# CUSTOM MISSION
You are an autonomous engineer specializing in this codebase...

# CONTEXT (READ IN ORDER)
1. `AGENTS.md`
2. `.ralph/README.md`
3. Task details via `ralph task show {{TASK_ID}}`

{{PROJECT_TYPE_GUIDANCE}}
EOF

# Override phase 1 planning
cat > .ralph/prompts/worker_phase1.md << 'EOF'
# CUSTOM PLANNING MODE

CURRENT TASK: {{TASK_ID}}

{{BASE_WORKER_PROMPT}}

## OUTPUT REQUIREMENT
Produce a detailed implementation plan...
EOF
```

### File Naming Convention

| Prompt File | Purpose | Required Placeholders |
|-------------|---------|----------------------|
| `worker.md` | Base worker behavior | `{{PROJECT_TYPE_GUIDANCE}}` (if enabled) |
| `worker_phase1.md` | Planning phase | `{{TASK_ID}}`, `{{TOTAL_PHASES}}`, `{{PLAN_PATH}}`, `{{BASE_WORKER_PROMPT}}` |
| `worker_phase2.md` | Implementation (2-phase) | `{{TASK_ID}}`, `{{PLAN_TEXT}}`, `{{CHECKLIST}}` |
| `worker_phase2_handoff.md` | Implementation (3-phase) | Same as phase2 |
| `worker_phase3.md` | Code review phase | `{{TASK_ID}}`, `{{CODE_REVIEW_BODY}}`, `{{COMPLETION_CHECKLIST}}` |
| `worker_single_phase.md` | Single-pass execution | `{{TASK_ID}}`, `{{CHECKLIST}}`, `{{BASE_WORKER_PROMPT}}` |
| `task_builder.md` | Task creation | `{{USER_REQUEST}}`, `{{HINT_TAGS}}`, `{{HINT_SCOPE}}` |
| `task_updater.md` | Task updates | `{{TASK_ID}}` |
| `scan_general_v2.md` | Repository scanning | `{{USER_FOCUS}}`, `{{PROJECT_TYPE_GUIDANCE}}` |
| `merge_conflicts.md` | Conflict resolution | `{{CONFLICT_FILES}}` |
| `code_review.md` | Review body | `{{TASK_ID}}` |
| `completion_checklist.md` | Completion steps | `{{TASK_ID}}` |

### Validation Rules

Override files must preserve required placeholders:

```rust
// From registry.rs - task_builder requires these placeholders:
const TASK_BUILDER_REQUIRED: &[RequiredPlaceholder] = &[
    RequiredPlaceholder {
        token: "{{USER_REQUEST}}",
        error_message: "Template error: task builder prompt template is missing...",
    },
    // ...
];
```

If an override is missing required placeholders, Ralph fails fast with a clear error message.

### Instruction Files

Configured `agent.instruction_files` are prepended as authoritative content at the top of every prompt.
Ralph does not auto-inject `~/.codex/AGENTS.md` or repo `AGENTS.md`; prompt text should not imply otherwise.

---

## Available Prompts

### Base Worker (`worker.md`)

The foundation prompt used across all phases. Defines:
- Mission statement
- Context reading order (AGENTS.md, .ralph/README.md, etc.)
- Operating rules (don't ask permission, fix root causes, etc.)
- Pre-flight safety checks
- Stop/cancel semantics

**Key Placeholders:**
- `{{TASK_ID}}` - Current task identifier
- `{{PROJECT_TYPE_GUIDANCE}}` - Code vs Docs priorities
- `{{INTERACTIVE_INSTRUCTIONS}}` - TTY-specific guidance (usually empty)

### Phase 1: Planning (`worker_phase1.md`)

Wraps the base worker for the planning phase:
- Instructs the agent to produce a plan file only
- Prohibits file modifications (except plan cache)
- Emphasizes standalone plan quality for Phase 2 execution

**Key Placeholders:**
- `{{TOTAL_PHASES}}` - Number of phases (2 or 3)
- `{{PLAN_PATH}}` - Where to write the plan (e.g., `.ralph/cache/plans/RQ-0001.md`)
- `{{ITERATION_CONTEXT}}` - Refinement guidance for multi-iteration runs
- `{{REPOPROMPT_BLOCK}}` - Tool instructions when RepoPrompt is enabled

### Phase 2: Implementation (`worker_phase2.md`)

Wraps the base worker for implementation (2-phase workflow):
- Receives the plan from Phase 1
- Executes implementation
- Runs the configured CI gate when enabled and completes task

**Key Placeholders:**
- `{{PLAN_TEXT}}` - Content of the Phase 1 plan
- `{{CHECKLIST}}` - Completion checklist
- `{{ITERATION_COMPLETION_BLOCK}}` - Rules for non-final iterations

### Phase 2 Handoff (`worker_phase2_handoff.md`)

3-phase workflow variant that stops after implementation:
- Same implementation focus as worker_phase2
- Stops after configured Phase 2 validation is satisfied; if the CI gate is disabled, no CI pass is required
- Leaves dirty working tree for Phase 3 review

### Phase 3: Review (`worker_phase3.md`)

Code review and finalization phase:
- Reviews Phase 2 changes against standards
- Makes refinements if needed
- Handles final completion

**Key Placeholders:**
- `{{CODE_REVIEW_BODY}}` - The code_review.md content
- `{{PHASE2_FINAL_RESPONSE}}` - Context from Phase 2 execution
- `{{PHASE3_COMPLETION_GUIDANCE}}` - Final vs non-final iteration rules

### Single Phase (`worker_single_phase.md`)

Combined plan+implement for simple tasks:
- Brief planning allowed
- Direct implementation
- No separate plan file required

### Task Builder (`task_builder.md`)

Converts user requests into queue tasks:
- Analyzes user request
- Generates proper task JSON
- Inserts into `.ralph/queue.jsonc`

**Key Placeholders:**
- `{{USER_REQUEST}}` - Original user input
- `{{HINT_TAGS}}` - Suggested tags (may be empty)
- `{{HINT_SCOPE}}` - Suggested scope (may be empty)

Example user request flow:
```bash
ralph task build "Fix the login button styling"
# → Loads task_builder.md
# → Renders with {{USER_REQUEST}} = "Fix the login button styling"
# → Agent creates task in queue.json
```

### Scan Prompts (`scan_*.md`)

Repository scanning for actionable tasks:
- **General**: Broad codebase analysis
- **Maintenance**: Focus on tech debt, bugs, upkeep
- **Innovation**: Feature gaps, improvements, new capabilities

**Key Placeholders:**
- `{{USER_FOCUS}}` - Area of focus (e.g., "authentication module")
- `{{PROJECT_TYPE_GUIDANCE}}` - Code vs Docs priorities

### Merge Conflicts (`merge_conflicts.md`)

Parallel run conflict resolution:
- Special handling for queue.json/done.json
- General conflict resolution for other files

**Key Placeholders:**
- `{{CONFLICT_FILES}}` - List of files with conflicts

### Code Review (`code_review.md`)

Review body injected in Phase 3:
- Coding standards reference
- Review responsibilities
- CI gate policies

### Checklists

**completion_checklist.md**: Steps for finishing implementation
**phase2_handoff_checklist.md**: Steps for 3-phase handoff
**iteration_checklist.md**: Steps for refinement iterations

---

## Template Variables

### Standard Placeholders

| Placeholder | Description | Example |
|-------------|-------------|---------|
| `{{TASK_ID}}` | Current task identifier | `RQ-0001` |
| `{{USER_REQUEST}}` | Original user input | "Fix login button" |
| `{{USER_FOCUS}}` | Scan focus area | "authentication" |
| `{{HINT_TAGS}}` | Suggested tags | `["ui", "bug"]` |
| `{{HINT_SCOPE}}` | Suggested scope | `["src/auth/"]` |
| `{{PLAN_PATH}}` | Plan cache file path | `.ralph/cache/plans/RQ-0001.md` |
| `{{PLAN_TEXT}}` | Content of plan file | (full plan markdown) |
| `{{TOTAL_PHASES}}` | Phase count | `2` or `3` |
| `{{CONFLICT_FILES}}` | Conflicted file list | `["file1.rs", "file2.rs"]` |

### Config Access

Access configuration values via `{{config.section.key}}`:

```markdown
CI Gate Command: {{config.agent.ci_gate_display}}
CI Gate Enabled: {{config.agent.ci_gate_enabled}}
Git Commit/Push: {{config.agent.git_publish_mode}}
Runner: {{config.agent.runner}}
Model: {{config.agent.model}}
Queue Prefix: {{config.queue.id_prefix}}
```

Available config paths:
- `config.agent.runner`
- `config.agent.model`
- `config.agent.reasoning_effort`
- `config.agent.iterations`
- `config.agent.followup_reasoning_effort`
- `config.agent.claude_permission_mode`
- `config.agent.ci_gate_display`
- `config.agent.ci_gate_enabled`
- `config.agent.git_publish_mode`
- `config.queue.id_prefix`
- `config.queue.id_width`
- `config.project_type`
- `config.version`

### Environment Variables

Access environment variables with shell-style syntax:

```markdown
Home directory: ${HOME}
With default: ${UNKNOWN_VAR:-default_value}
Escaped: $${LITERAL} or \${LITERAL}
```

Environment variables are expanded before config values.

---

## Project Type Guidance

The `{{PROJECT_TYPE_GUIDANCE}}` placeholder injects project-specific priorities:

### Code Projects

```markdown
## PROJECT TYPE: CODE

This is a code repository. Prioritize:
- Implementation correctness and type safety
- Test coverage and regression prevention
- Performance and resource efficiency
- Clean, maintainable code structure
```

### Documentation Projects

```markdown
## PROJECT TYPE: DOCS

This is a documentation repository. Prioritize:
- Clear, accurate information
- Consistent formatting and structure
- Accessibility and readability
- Examples and practical guidance
```

### Configuration

Set project type in `.ralph/config.jsonc`:

```json
{
  "project_type": "code"
}
```

There is no dedicated `ralph config set` subcommand; edit the config file directly.

### Template Control

Not all prompts receive project type guidance. The registry controls this:

```rust
// From registry.rs
PromptTemplateId::Worker => PromptTemplate {
    // ...
    project_type_guidance: true,  // Injected
},
PromptTemplateId::WorkerPhase1 => PromptTemplate {
    // ...
    project_type_guidance: false, // Not injected (inherits from base)
},
```

---

## RepoPrompt Integration

When RepoPrompt tooling is enabled, Ralph injects additional instructions:

### Tool Injection (`repoprompt_tool_injection`)

Adds preference-based RepoPrompt guidance:

```markdown
## REPOPROMPT TOOLING (WHEN CONNECTED)
You are running in a RepoPrompt-enabled environment. Prefer RepoPrompt tools when they are available in this harness.
```

The injected guidance describes the usual RepoPrompt MCP tool inventory while making it clear that other repository tools remain valid when RepoPrompt is unavailable.

### Plan Requirement (`repoprompt_plan_required`)

Adds RepoPrompt planning guidance:

```markdown
## REPOPROMPT PLANNING FLOW
When `context_builder` is available, use it as the standard planning path.
```

The planning block still keeps the hard artifact boundary intact: Phase 1 must write the final plan to `{{PLAN_PATH}}` because later phases read that file.

### Configuration

Enable RepoPrompt integration in `.ralph/config.jsonc`:

```json
{
  "agent": {
    "repoprompt_tool_injection": true,
    "repoprompt_plan_required": true
  }
}
```

### CLI Fallback

The instructions include a CLI fallback for when MCP tools are unavailable:

```markdown
## CLI FALLBACK (WHEN MCP TOOLS ARE UNAVAILABLE)
If RepoPrompt MCP tools are unavailable, prefer the RepoPrompt CLI when it exists:
- Start with `rp-cli --help`
- Optionally use `rp -h` if the wrapper is installed
- `rp-cli` commonly uses `-e` to execute an expression such as `rp-cli -e 'tree'`
```

---

## Prompt Flow

### Worker Prompt Composition

```
┌─────────────────────────────────────────────────────────┐
│                  WORKER PROMPT FLOW                      │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Phase 1: Planning                                       │
│  ┌─────────────┐                                         │
│  │ worker.md   │  Base prompt with PROJECT_TYPE_GUIDANCE │
│  └──────┬──────┘                                         │
│         │                                                │
│         ▼                                                │
│  ┌─────────────────┐                                     │
│  │ worker_phase1   │  Wraps base, adds REPOPROMPT_BLOCK  │
│  │   .md           │  and planning constraints           │
│  └────────┬────────┘                                     │
│           │                                              │
│           ▼                                              │
│  ┌─────────────────┐     ┌─────────────┐                │
│  │ Rendered Prompt │  +  │ ITERATION_  │  (if multi-   │
│  │                 │     │ CONTEXT       │   iteration)  │
│  └─────────────────┘     └─────────────┘                │
│                                                          │
│  ─────────────────────────────────────────────────────  │
│                                                          │
│  Phase 2: Implementation                                 │
│  ┌─────────────┐                                         │
│  │ worker.md   │  Base prompt                            │
│  └──────┬──────┘                                         │
│         │                                                │
│         ▼                                                │
│  ┌─────────────────┐                                     │
│  │ worker_phase2   │  Wraps base, injects PLAN_TEXT      │
│  │   .md           │  and CHECKLIST                      │
│  └────────┬────────┘                                     │
│           │                                              │
│           ▼                                              │
│  ┌─────────────────┐     ┌─────────────────────────┐    │
│  │ Rendered Prompt │  +  │ ITERATION_COMPLETION_   │    │
│  │                 │     │ BLOCK (if non-final)    │    │
│  └─────────────────┘     └─────────────────────────┘    │
│                                                          │
│  ─────────────────────────────────────────────────────  │
│                                                          │
│  Phase 3: Review                                         │
│  ┌─────────────┐                                         │
│  │ worker.md   │  Base prompt                            │
│  └──────┬──────┘                                         │
│         │                                                │
│         ▼                                                │
│  ┌─────────────────┐                                     │
│  │ worker_phase3   │  Wraps base, injects CODE_REVIEW_   │
│  │   .md           │  BODY and COMPLETION_CHECKLIST      │
│  └────────┬────────┘                                     │
│           │                                              │
│           ▼                                              │
│  ┌─────────────────┐     ┌─────────────────────────┐    │
│  │ Rendered Prompt │  +  │ PHASE3_COMPLETION_      │    │
│  │                 │     │ GUIDANCE                │    │
│  └─────────────────┘     └─────────────────────────┘    │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Scan Prompt Flow

```
┌─────────────────┐
│  scan_general   │  Load template
│    _v2.md       │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Expand Config   │  {{config.agent.*}}
│   Variables     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Inject Project  │  {{PROJECT_TYPE_GUIDANCE}}
│  Type Guidance  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Replace Focus   │  {{USER_FOCUS}}
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Validate: No    │  Ensure all {{...}} resolved
│ Unresolved Vars │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Final Prompt   │  → Runner
└─────────────────┘
```

### Task Builder Flow

```
User Request ──► Load task_builder.md ──► Validate Required Placeholders
                                               │
                                               ▼
                    ┌──────────────────────────────────────────┐
                    │  Render with:                            │
                    │  - {{USER_REQUEST}} = user input         │
                    │  - {{HINT_TAGS}} = suggested tags        │
                    │  - {{HINT_SCOPE}} = suggested scope      │
                    │  - {{PROJECT_TYPE_GUIDANCE}}             │
                    └─────────────────────┬────────────────────┘
                                          │
                                          ▼
                    ┌──────────────────────────────────────────┐
                    │  Validate: No unresolved placeholders    │
                    └─────────────────────┬────────────────────┘
                                          │
                                          ▼
                    ┌──────────────────────────────────────────┐
                    │  Agent creates task in queue.json        │
                    └──────────────────────────────────────────┘
```

---

## Best Practices

### Creating Overrides

1. **Start with the default**: Copy the embedded default as a starting point
2. **Preserve required placeholders**: Check the registry for required tokens
3. **Test validation**: Run `ralph queue validate` after changes
4. **Document changes**: Add comments explaining customizations

Example override with comments:
```markdown
<!-- Custom worker prompt for MyProject -->
<!-- Changes: Added security review requirement -->

# MISSION
You are an autonomous engineer for MyProject...

## SECURITY REVIEW (ADDED)
Before completing any task:
- Check for exposed secrets in new code
- Verify input validation on new endpoints
- Review SQL queries for injection risks

# CONTEXT (READ IN ORDER)
...
```

### Variable Safety

- Always use `{{VAR}}` syntax for template variables
- Use `${VAR}` for environment variables
- Escape literal `${` with `$${` or `\${`
- The validation step catches unresolved `{{...}}` placeholders

### Multi-Repository Setup

For organizations with multiple repositories:

```bash
# Create a shared prompt template repository
git clone https://github.com/org/ralph-prompts.git

# Link to each project
cd project-a
ln -s ../ralph-prompts/worker.md .ralph/prompts/worker.md
ln -s ../ralph-prompts/worker_phase1.md .ralph/prompts/worker_phase1.md
```

### Debugging Prompts

Use `--debug` flag to see rendered prompts:

```bash
# Debug mode writes raw prompts to log
ralph run --debug

# Check the debug log
cat .ralph/logs/debug.log | grep -A 50 "Rendered prompt"
```

---

## Reference

### Source Code Locations

| Component | Path |
|-----------|------|
| Public API | `crates/ralph/src/prompts.rs` |
| Internal Modules | `crates/ralph/src/prompts_internal/` |
| Registry | `crates/ralph/src/prompts_internal/registry.rs` |
| Utilities | `crates/ralph/src/prompts_internal/util.rs` |
| Worker Phases | `crates/ralph/src/prompts_internal/worker_phases.rs` |
| Template System | `crates/ralph/src/template/` |
| Embedded Defaults | `crates/ralph/assets/prompts/` |

### Related Documentation

- [Workflow](../workflow.md) - Phase execution and prompt overrides
- [Configuration](../configuration.md) - Config options affecting prompts
- [Queue and Tasks](../queue-and-tasks.md) - Task structure and fields
- [Scan](./scan.md) - Repository scanning details
