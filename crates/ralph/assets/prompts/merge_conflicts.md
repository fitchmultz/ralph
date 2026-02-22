# Merge Conflict Resolution

## AGENT SWARM INSTRUCTION
Use agent swarms, parallel agents, and sub-agents aggressively. Spawn sub-agents via your available tools to work efficiently and effectively—analyze conflicts in parallel, resolve multiple files concurrently, and validate resolutions using multiple agents working together.

You are resolving Git merge conflicts.

Conflicted files:
{{CONFLICT_FILES}}

## Special Guidance for `{{config.queue.file}}` / `{{config.queue.done_file}}`

If either of these files appears in the conflict list above, follow these additional rules to preserve Ralph's queue semantics:

### If `{{config.queue.file}}` is conflicted:
- Preserve **all** tasks from both sides; do not drop tasks to "make it merge".
- Preserve task ordering semantics: **file order is execution order**; do not sort/reorder tasks unless required to resolve a direct conflict, and keep relative order stable.
- Do not renumber/rename task IDs unless necessary to resolve a true duplicate-ID collision; never delete tasks to resolve collisions.
- If an ID collision must be resolved: choose a new unique ID and update every reference to it (`depends_on`, `blocks`, `relates_to`, `duplicates`) to keep the graph consistent.
- Keep `created_at`/`updated_at` as valid RFC3339 strings; do not invent new timestamps unless already required by the chosen side's content.

### If `{{config.queue.done_file}}` is conflicted:
- Keep only terminal tasks (`done`/`rejected`) in `{{config.queue.done_file}}`.
- Do not move active tasks from `{{config.queue.file}}` into `{{config.queue.done_file}}` as part of conflict resolution.

### For both files:
- Remove all conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`) and ensure strict queue/done file validity (no syntax errors, no malformed structure).
- Ralph will run queue/done repair + semantic validation before commit, so your merge resolution must satisfy these invariants.

## General Instructions

- Open each conflicted file and resolve the conflicts.
- Remove all conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`).
- Do not change unrelated files.
- Ensure `git status` shows no unmerged paths.
- Keep changes minimal and focused on resolving conflicts.
