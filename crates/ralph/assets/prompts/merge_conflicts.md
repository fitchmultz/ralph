<!-- Purpose: Prompt for focused Git merge conflict resolution. -->
# Merge Conflict Resolution
You are resolving Git merge conflicts for a Ralph workspace.

# Goal
Produce a conflict-free working tree that preserves both upstream intent and task intent with minimal unrelated change.

# Conflicted Files
{{CONFLICT_FILES}}

# Special Guidance for `{{config.queue.file}}` / `{{config.queue.done_file}}`
If `{{config.queue.file}}` is conflicted:
- Preserve **all** tasks from both sides; do not drop tasks to make the merge easier.
- Preserve file order semantics: file order is execution order. Keep relative order stable unless a direct conflict requires a local choice.
- Do not renumber/rename task IDs unless needed for a true duplicate-ID collision.
- If a duplicate ID must be resolved, prefer Ralph tooling when available. Otherwise choose a unique ID consistent with the queue format and update all references (`depends_on`, `blocks`, `relates_to`, `duplicates`).
- Keep timestamps valid RFC3339; do not invent timestamps unless the chosen content requires one.

If `{{config.queue.done_file}}` is conflicted:
- Keep only terminal tasks (`done`/`rejected`) there.
- Do not move active tasks into done as part of conflict resolution.

For both files:
- Remove all conflict markers.
- Preserve the root queue/done structure and schema.
- Run `ralph queue validate` if either queue/done file was touched and fix validation errors.

# General Resolution Flow
1. Open each conflicted file.
2. Resolve markers while preserving both sides' intended behavior.
3. Keep edits focused on conflict resolution.
4. Run `git status --porcelain` or equivalent and confirm no unmerged paths remain.
5. Run targeted validation when the conflict touched executable behavior.

# Final Response Shape
- Files resolved
- Validation run and result
- Semantic choices made, especially for queue/done conflicts or duplicate IDs
