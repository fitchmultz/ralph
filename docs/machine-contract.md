# Machine Contract

Ralph exposes a first-class machine API under `ralph machine ...`.

This surface exists for the macOS app and any other automation that needs stable, versioned JSON instead of human-oriented CLI behavior.

## Rules

- Every machine response is a named JSON document with a top-level `version`.
- Breaking wire changes require a version bump for the affected machine document.
- Human CLI output and flags may change without preserving app compatibility.
- Machine run streams emit NDJSON on stdout.
- Machine run terminal summaries are single-line JSON documents so stream consumers can parse them deterministically.

## Current Machine Areas

- `ralph machine system info`
- `ralph machine queue read`
- `ralph machine queue graph`
- `ralph machine queue dashboard`
- `ralph machine queue validate`
- `ralph machine config resolve`
- `ralph machine task create`
- `ralph machine task mutate`
- `ralph machine task decompose`
- `ralph machine run one`
- `ralph machine run loop`
- `ralph machine run parallel-status`
- `ralph machine doctor report`
- `ralph machine cli-spec`
- `ralph machine schema`

## Schemas

Generated machine schemas live in [schemas/machine.schema.json](../schemas/machine.schema.json).

Generate them locally with:

```bash
make generate
```

## App Contract Boundary

The macOS app should consume only machine surfaces for:

- queue snapshots
- config resolution
- task create/mutate/decompose flows
- graph and dashboard reads
- diagnostics consumed by the app
- run status and event streaming
- CLI spec loading

It should not infer app state from human CLI text, hidden commands, or direct queue-file decoding.
