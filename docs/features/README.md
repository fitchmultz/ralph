# Ralph Features Documentation
Status: Active
Owner: Maintainers
Source of truth: this document for its stated scope
Parent: [Ralph Documentation](../index.md)


Welcome to the Ralph features documentation. This index provides organized navigation to all feature guides, from core concepts to advanced integration options.

---

## Overview

Ralph is a Rust CLI for running AI agent loops against a structured JSON task queue. The features documentation covers all major capabilities, configuration options, and workflows.

---

## Quick Start

New to Ralph? Start here:

- **[Quick Start Guide](../quick-start.md)** — Get up and running in minutes
- **[CLI Reference](../cli.md)** — Complete command-line documentation
- **[Configuration](../configuration.md)** — Full configuration reference

---

## Core Concepts

Understand the fundamental building blocks of Ralph:

| Document | Description |
|----------|-------------|
| **[Phases](./phases.md)** | Multi-phase execution workflow (Plan → Implement → Review) |
| **[Queue](./queue.md)** | Task queue management, lifecycle, and operations |
| **[Dependencies](./dependencies.md)** | Task relationships, DAG execution, and critical path analysis |
| **[Context](./context.md)** | RepoPrompt integration and context building |
| **[Tasks](./tasks.md)** | Task system index with split schema, lifecycle, relationships, and operations references |

---

## Execution

Learn how Ralph executes tasks and manages the execution lifecycle:

| Document | Description |
|----------|-------------|
| **[Runners](./runners.md)** | AI runner orchestration (Claude, Codex, Kimi, Gemini, etc.) |
| **[Parallel](./parallel.md)** | Parallel task execution with direct push integration |
| **[Session Management](./session-management.md)** | Crash recovery and session resumption |
| **[Supervision](./supervision.md)** | CI gate enforcement, git operations, and human-in-the-loop oversight |

---

## Integration

Connect Ralph to external systems and notifications:

| Document | Description |
|----------|-------------|
| **[Webhooks](./webhooks.md)** | HTTP event notifications for Slack, Discord, CI/CD |
| **[Plugins](./plugins.md)** | Custom runner and processor plugins |
| **[Notifications](./notifications.md)** | Desktop notifications and sound alerts |
| **[Import/Export](./import-export.md)** | Queue import/export formats and workflows |

---

## Workflow

Tools for managing and monitoring your development workflow:

| Document | Description |
|----------|-------------|
| **[App (macOS)](./app.md)** | macOS SwiftUI app for interactive task management |
| **[Scan](./scan.md)** | AI-powered repository scanning for task discovery |
| **[Daemon and Watch](./daemon-and-watch.md)** | Background execution and automatic task detection |

---

## Configuration

Detailed configuration guides for specific features:

| Document | Description |
|----------|-------------|
| **[Configuration](./configuration.md)** | Feature-level configuration map and operator guidance |
| **[Agent and Runner Configuration](./configuration-agent.md)** | Runner, model, phase, CI gate, permission, and retry settings |
| **[Queue and Parallel Configuration](./configuration-operations.md)** | Queue paths, task aging, auto-archive, and parallel workspace settings |
| **[Integration and Profile Configuration](./configuration-integrations.md)** | Notifications, webhooks, plugins, profiles, and environment variables |
| **[Complete Configuration Example](./configuration-example.md)** | Long assembled configuration sample |
| **[Profiles](./profiles.md)** | Configuration profiles for quick workflow switching |
| **[Prompts](./prompts.md)** | Custom prompt templates and overrides |
| **[Migrations](./migrations.md)** | Configuration and data migration guide |

---

## Security

Security-related documentation:

| Document | Description |
|----------|-------------|
| **[Security Features](./security.md)** | Security features and configuration |
| **[Security Policy](../../SECURITY.md)** — Main security policy and vulnerability reporting |

---

## Reference

Additional reference documentation:

| Document | Description |
|----------|-------------|
| **[CLI Reference](../cli.md)** | Complete command-line documentation |
| **[Error Handling](../error-handling.md)** | Patterns and best practices |
| **[Environment](../environment.md)** | Environment variables reference |
| **[Tasks](./tasks.md)** | Task system index and focused task reference pages |
| **[Queue and Tasks](../queue-and-tasks.md)** | Legacy combined queue and task reference |
| **[Workflow](../workflow.md)** | High-level workflow documentation |
| **[Releasing](../releasing.md)** | Release process documentation |

---

## Schema Reference

Ralph uses JSON schemas for validation:

- **[Config Schema](../../schemas/config.schema.json)** — Configuration schema
- **[Queue Schema](../../schemas/queue.schema.json)** — Queue and task schema
- **[Machine Schema](../../schemas/machine.schema.json)** — Machine / structured integration schema

---

## Documentation by Use Case

### I want to...

**Get started quickly**
- [Quick Start Guide](../quick-start.md)
- [App (macOS)](./app.md) — Interactive interface

**Configure my runner**
- [Runners](./runners.md) — Runner-specific setup
- [Configuration](../configuration.md) — General configuration
- [Profiles](./profiles.md) — Workflow presets

**Set up parallel execution**
- [Parallel](./parallel.md) — Parallel mode
- [Configuration](../configuration.md#parallel-configuration) — Parallel settings

**Integrate with Slack/Discord**
- [Webhooks](./webhooks.md) — Webhook setup
- [Notifications](./notifications.md) — Desktop notifications

**Automate task detection**
- [Daemon and Watch](./daemon-and-watch.md) — Watch mode for TODO comments
- [Scan](./scan.md) — Repository scanning

**Handle failures and recovery**
- [Session Management](./session-management.md) — Resume interrupted tasks
- [Phases](./phases.md) — Human-in-the-loop review (supervision workflows)
- [Error Handling](../error-handling.md) — Error patterns

**Manage task dependencies**
- [Dependencies](./dependencies.md) — Dependency relationships
- [Queue](./queue.md) — Queue operations

**Customize prompts**
- [Prompts](./prompts.md) — Prompt customization
- [Phases](./phases.md) — Phase-specific prompts

**Migrate configurations**
- [Migrations](./migrations.md) — Migration guide
- `ralph migrate` command in [CLI](../cli.md)

---

## Contributing to Documentation

When adding new features or updating existing ones:

1. Update the relevant feature document in `docs/features/`
2. Update this index if adding a new feature category
3. Update `docs/index.md` with any new top-level references
4. Run `make agent-ci` to validate docs-only checks, including markdown links and documented path guards

See [Contributing Guidelines](../../CONTRIBUTING.md) for more details.
