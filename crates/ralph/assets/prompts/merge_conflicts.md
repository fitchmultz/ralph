# Merge Conflict Resolution

You are resolving Git merge conflicts.

Conflicted files:
{{CONFLICT_FILES}}

## Special Guidance for `.ralph/queue.json` / `.ralph/done.json`

If either of these files appears in the conflict list above, follow these additional rules to preserve Ralph's queue semantics:

### If `.ralph/queue.json` is conflicted:
- Preserve **all** tasks from both sides; do not drop tasks to "make it merge".
- Preserve task ordering semantics: **file order is execution order**; do not sort/reorder tasks unless required to resolve a direct conflict, and keep relative order stable.
- Do not renumber/rename task IDs unless necessary to resolve a true duplicate-ID collision; never delete tasks to resolve collisions.
- If an ID collision must be resolved: choose a new unique ID and update every reference to it (`depends_on`, `blocks`, `relates_to`, `duplicates`) to keep the graph consistent.
- Keep `created_at`/`updated_at` as valid RFC3339 strings; do not invent new timestamps unless already required by the chosen side's content.

### If `.ralph/done.json` is conflicted:
- Keep only terminal tasks (`done`/`rejected`) in `done.json`.
- Do not move active tasks from `queue.json` into `done.json` as part of conflict resolution.

### For both files:
- Remove all conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`) and ensure strict JSON validity (no trailing commas, no syntax errors).
- Ralph will run queue/done JSON repair + semantic validation before commit, so your merge resolution must satisfy these invariants.

## General Instructions

- Open each conflicted file and resolve the conflicts.
- Remove all conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`).
- Do not change unrelated files.
- Ensure `git status` shows no unmerged paths.
- Keep changes minimal and focused on resolving conflicts.
