# Daemon and Watch Troubleshooting
Status: Active
Owner: Maintainers
Source of truth: this document for daemon/watch troubleshooting workflows
Parent: [Daemon and Watch](../daemon-and-watch.md)

Use this guide to diagnose and recover common daemon and watch issues.

## Troubleshooting

### Daemon Issues

| Issue | Solution |
|-------|----------|
| `Daemon is already running` | Run `ralph daemon status` to verify, then `ralph daemon stop` if needed |
| `Daemon failed to start` | Check `.ralph/logs/daemon.log` for errors |
| Stale state file | Run `ralph daemon status` to auto-clean, or manually remove `.ralph/cache/daemon.json` |
| Won't stop gracefully | Use `kill -9 <PID>` as last resort |

### Watch Issues

| Issue | Solution |
|-------|----------|
| Not detecting changes | Check `--patterns` match your files; verify file is in watched path |
| Too many tasks created | Enable deduplication by ensuring tasks have `watch.fingerprint` |
| High CPU usage | Increase `--debounce-ms` to reduce processing frequency |
| Missing comments | Check `--comments` includes the types you're using |

### Common Patterns

```bash
# Check daemon logs
tail -f .ralph/logs/daemon.log

# Verify watch is detecting files
ralph watch --patterns "*.rs"  # Run interactively to see output

# Clean up and restart
ralph daemon stop
rm -f .ralph/cache/daemon.json
rm -f .ralph/cache/stop_requested
ralph daemon start
```

---

## See Also

- [Daemon and Watch overview](../daemon-and-watch.md)
- [CLI Reference](../../cli.md)
- [Queue and Tasks](../../queue-and-tasks.md)
- [Daemon Mode](./daemon.md)
- [Watch Mode](./watch.md)
- [Operations](./operations.md)
