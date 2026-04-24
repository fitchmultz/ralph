/**
 NotificationNames

 Purpose:
 - Preserve the historical file location for app-scoped routing infrastructure.

 Responsibilities:
 - Preserve the historical file location for app-scoped routing infrastructure.
 - Document the immediate cutover away from NotificationCenter routing.

 Does not handle:
 - Defining process-wide notification names.
 - Broadcasting app, workspace, window, or queue refresh events.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - App routing now lives in scene-scoped actions registered through `WorkspaceManager`.
 - Queue refresh state now lives on each `Workspace` instance.
 */
