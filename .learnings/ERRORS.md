# Errors

## [ERR-20260127-001] ralph queue list

**Logged**: 2026-01-27T17:16:58Z
**Priority**: low
**Status**: pending
**Area**: docs

### Summary
Used unsupported flag `--with-title` for `ralph queue list`.

### Error
```
error: unexpected argument '--with-title' found

Usage: ralph queue list [OPTIONS]

For more information, try '--help'.
```

### Context
- Command attempted: `ralph queue list --with-title`
- Input/parameters: `--with-title` (not supported)
- Environment: local CLI invocation in repo root

### Suggested Fix
Use `ralph queue list --format long` or `--help` to discover available options.

### Metadata
- Reproducible: yes
- Related Files: docs/cli.md
- See Also: 

---
