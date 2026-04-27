# Daemon Mode and Watch Mode
Status: Active
Owner: Maintainers
Source of truth: this document for daemon/watch feature navigation and high-level overview
Parent: [Feature Documentation](README.md)

![Daemon & Watch Mode](../assets/images/2026-02-07-11-32-24-daemon-watch.png)

Ralph provides two background-workflow capabilities:

- **Daemon mode** runs Ralph as a background service that continuously processes runnable tasks.
- **Watch mode** monitors source files for actionable comments and creates or reconciles queue tasks.

## Start Here

| Need | Guide |
|------|-------|
| Run Ralph continuously in the background | [Daemon Mode](./daemon-watch/daemon.md) |
| Create tasks automatically from TODO/FIXME/HACK/XXX comments | [Watch Mode](./daemon-watch/watch.md) |
| Run daemon and watch together | [Operations](./daemon-watch/operations.md) |
| Diagnose stuck daemons, stale state, or watch detection issues | [Troubleshooting](./daemon-watch/troubleshooting.md) |

## Quick Commands

```bash
ralph daemon start --notify-when-unblocked
ralph daemon status
ralph daemon stop

ralph watch --auto-queue --close-removed --notify
```

## Platform Notes

Daemon mode is fully implemented on Unix systems (Linux and macOS). On Windows, run `ralph run loop --continuous` directly or configure a Windows service.

## See Also

- [Workflow](../workflow.md)
- [CLI Reference](../cli.md)
- [Queue and Tasks](../queue-and-tasks.md)
