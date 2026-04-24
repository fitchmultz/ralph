# RalphMac Settings Window Investigation
Status: Archived
Owner: Maintainers
Source of truth: historical snapshot; current guidance lives in linked active docs
Parent: [Ralph Documentation](../index.md)


Date: 2026-03-13

## Scope

Investigate why opening Settings still creates extra untitled RalphMac-owned CGWindows after the app was cut over from a custom AppKit settings presenter to a standard SwiftUI `Settings` scene.

## Current live symptom

Opening Settings produces:

- one real visible settings window
- one untitled black helper surface around `500x500`
- one untitled rounded-rect helper surface around `312x237` at a high layer

This still reproduces after:

- removing `MenuBarExtra` from the app scene graph
- removing the old `SettingsWindowController` path
- defining Settings through `Settings { SettingsSceneRoot() }`

## Code paths reviewed

- `/Users/mitchfultz/Projects/AI/ralph/apps/RalphMac/RalphMac/RalphMacApp.swift`
- `/Users/mitchfultz/Projects/AI/ralph/apps/RalphMac/RalphMac/ASettingsInfra.swift`
- `/Users/mitchfultz/Projects/AI/ralph/apps/RalphMac/RalphMac/SettingsService.swift`
- `/Users/mitchfultz/Projects/AI/ralph/apps/RalphMac/RalphMac/AppDelegate.swift`
- `/Users/mitchfultz/Projects/AI/ralph/apps/RalphMac/RalphMac/AppSettings.swift`
- `/Users/mitchfultz/Projects/AI/ralph/apps/RalphMac/RalphMac/SettingsViewModel.swift`
- `/Users/mitchfultz/Projects/AI/ralph/apps/RalphMac/RalphMac/RalphMacCommands.swift`
- `/Users/mitchfultz/Projects/AI/ralph/apps/RalphMac/RalphMac/RalphMacApp+URLRouting.swift`

RepoPrompt context-builder analysis was also run against the current RalphMac settings/window paths.

## What is no longer the leading theory

- There is not an obvious second active Settings scene in `RalphMacApp.swift`.
- The app entrypoint currently defines a single SwiftUI settings scene:

```swift
Settings {
    SettingsSceneRoot()
}
```

- `AppSettingsCommands` still exists on disk, but it is not currently installed in `RalphMacApp.commands`.
- The old custom `SettingsWindowController` path is no longer the active settings implementation.

Conclusion: the remaining helpers are more likely caused by scene lifecycle, window normalization, or settings content composition than by a duplicate top-level settings scene.

## Most likely remaining causes

### 1. AppDelegate is force-normalizing every AppKit window

`AppDelegate.normalizeWindow(_:)` currently:

- runs for existing windows on startup
- runs again on `NSWindow.didBecomeKeyNotification`
- runs again on `NSWindow.didBecomeMainNotification`
- accepts any window at least `400x240`
- force-fronts the window with:

```swift
window.orderFrontRegardless()
window.makeKeyAndOrderFront(nil)
NSApp.activate(ignoringOtherApps: true)
```

This is the strongest code-level match for the untitled black `500x500` helper surface. If SwiftUI/AppKit creates any temporary settings host window during construction, Ralph is currently eligible to drag that window on-screen and keep surfacing it.

### 2. Settings loading overlay still uses `.regularMaterial`

`SettingsView` currently overlays:

```swift
if viewModel.isLoading {
    ProgressView("Loading...")
        .padding()
        .background(.regularMaterial)
        .cornerRadius(8)
}
```

`SettingsViewModel.loadConfigIfNeeded()` always flips `isLoading` during first open, so this overlay is guaranteed to appear during settings bootstrap. This is the strongest match for the rounded high-layer helper surface.

### 3. Settings content is recreated on workspace changes

`SettingsSceneRoot` currently does:

```swift
SettingsContentContainer(workspace: workspace)
    .id(workspace?.id)
```

That can recreate the entire settings content tree, which means:

- a new `SettingsViewModel`
- another `loadConfigIfNeeded()`
- another `isLoading` cycle
- another material overlay cycle

This is probably an amplifier rather than the root cause, but it increases the odds of helper surfaces reappearing.

### 4. SettingsService still adds async presentation churn

`SettingsService.showSettingsWindow()` currently does:

```swift
Task { @MainActor in
    await Task.yield()
    NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
    NSApp.activate(ignoringOtherApps: true)
}
```

This is not the primary theory anymore, especially because the built-in Settings scene path likely bypasses parts of the custom helper flow, but the extra defer/activate sequence can still widen lifecycle races for custom open paths such as menu bar or URL routing.

## Prioritized remediation plan

### 1. Narrow AppDelegate window normalization first

High-confidence first cut:

- stop normalizing every `NSApp.windows` entry
- skip untitled/system/helper windows
- skip settings windows entirely
- reserve reveal/placement logic for Ralph workspace windows only
- remove `orderFrontRegardless()` from generic normalization if possible

This should be the first live validation cut because it is the best match for the black `500x500` stray surface.

### 2. Remove the material-backed settings loading overlay

Second cut:

- replace `.background(.regularMaterial)` with a flat color or inline loading state
- ideally avoid overlay composition entirely during initial config load

This should be the first content-side fix because it is the best match for the rounded high-layer helper surface.

### 3. Reduce settings content churn

Third cut:

- revisit `.id(workspace?.id)` in `SettingsSceneRoot`
- make sure Settings only rebuilds when it truly needs to retarget workspace state

This is especially relevant if the helper windows still appear after the first two fixes.

### 4. Simplify SettingsService after window scoping is fixed

Fourth cut:

- remove the extra `Task.yield()` unless a concrete need remains
- make settings activation ordering as direct as possible

This is cleanup, but it should come after the two stronger causes above.

## Fast confirmation instrumentation

Add temporary logging in:

- `AppDelegate.normalizeWindow(_:)`
  - log class, title, frame, identifier, level, and `windowNumber`
- `SettingsSceneRoot.onAppear`
  - log `NSApp.windows`
- `SettingsView.task`
  - log load start/end and `isLoading`
- `SettingsService.showSettingsWindow()`
  - log before yield, after yield, and after `showSettingsWindow:`

Expected value of this instrumentation:

- confirms whether the untitled `500x500` surface is being normalized/fronted by Ralph
- confirms whether the rounded helper appears only while `isLoading == true`

## Bottom line

The current best explanation is not “two settings windows.” It is:

1. SwiftUI/AppKit creates temporary settings-related windows or helper hosts.
2. Ralph’s app delegate currently treats those like normal app windows and force-fronts them.
3. Settings content also creates a material-backed loading overlay during first load, which likely accounts for the rounded high-layer helper surface.

The highest-confidence next move is to scope `AppDelegate` window normalization to real workspace windows only, then remove the material-backed loading overlay and re-run CGWindow validation.

## Follow-up validation after code changes

Date: 2026-03-13

Validation method:

- rebuilt `RalphMac.app` locally with `xcodebuild -project apps/RalphMac/RalphMac.xcodeproj -scheme RalphMac -configuration Debug -destination 'platform=macOS' ... build`
- relaunched the built app from DerivedData
- opened Settings with `Cmd+,`
- inspected live window inventory with `peekaboo list windows --app RalphMac --json`

### Cut 1: narrow `AppDelegate` normalization to workspace windows only

Changes applied:

- `AppDelegate.normalizeWindow(_:)` now bails unless the window identifier contains `AppWindow`
- `orderFrontRegardless()` was removed from generic frame reveal

Observed result:

- the visible workspace window remained normal
- opening Settings still created:
  - one visible `RalphMac Settings` window
  - one untitled `500x500` layer-0 surface
  - one untitled `312x237` layer-101 surface

Conclusion:

- the broad `AppDelegate` path was worth fixing, but it did **not** eliminate either helper surface in the default `Cmd+,` reproduction path

### Cut 2: remove the material-backed loading overlay

Changes applied:

- removed the `.overlay { ProgressView ... .background(.regularMaterial) }` block from `SettingsView`
- replaced it with an inline loading banner inside the main settings column

Observed result:

- the untitled `312x237` layer-101 surface still appeared
- the untitled `500x500` layer-0 surface still appeared

Conclusion:

- the material overlay was **not** the root cause of the rounded helper surface, at least in the default open flow

### Cut 3: remove settings-tree recreation via `.id(workspace?.id)`

Changes applied:

- removed `.id(workspace?.id)` from `SettingsSceneRoot`

Observed result:

- the untitled `312x237` layer-101 surface still appeared
- the untitled `500x500` layer-0 surface still appeared

Conclusion:

- settings-tree recreation may still be a churn amplifier in other flows, but it does **not** explain the initial helper surfaces on first open

## Updated takeaways

- `Cmd+,` reproduces the issue even though `SettingsService.showSettingsWindow()` is not in that path, so `Task.yield()` / `NSApp.activate(...)` in `SettingsService` are no longer a leading explanation for the standard Settings shortcut path
- the three strongest original theories have now been tested directly and did not remove the extra CGWindows
- the remaining investigation should shift toward initial responder / field-editor behavior inside the SwiftUI Settings host rather than generic window placement or Settings scene duplication

## First-responder evidence

Date: 2026-03-13

Additional temporary instrumentation was added in `ASettingsInfra.swift` to log each settings-window `firstResponder` during the first few run loops.

Observed sequence before the final fix:

- fresh `Cmd+,` open: settings window appears with no visible duplicate Settings scene
- next run loop: Settings window `firstResponder=NSTextView`
- at that same point the untitled `TUINSWindow` helper already exists
- by roughly `150ms`: Settings window `firstResponder=SwiftUI.AppKitWindow`, but the helper windows have already been created
- the untitled `SPRoundedWindow` helper can still remain visible later in the open cycle

Interpretation:

- the helper windows were being created during the initial field-editor path, before any later `makeFirstResponder(nil)` cleanup ran
- clearing focus after `didBecomeKey` is too late because AppKit has already selected the first editable key view and constructed the shared field editor
- `window.initialFirstResponder = nil` is not a neutral state here; it falls back to the first editable responder and reproduces the helper-window creation path

## Final cut: explicit non-text initial responder

Changes applied:

- kept the `AppDelegate` workspace-window scoping cleanup in place so Settings/system windows stay out of workspace placement normalization
- replaced the Settings focus-anchor experiment that cleared focus to `nil`
- made the Settings anchor itself the explicit initial first responder for the SwiftUI Settings window
- on `didBecomeKey`, reasserted the Settings anchor as first responder instead of clearing to `nil`

Result from fresh-launch `Cmd+,` validation:

- settings-window logs now show the Settings window first responder stabilizing on the custom focus anchor instead of `NSTextView`
- `peekaboo list windows --app RalphMac --json` now shows the normal Settings window and the workspace window only; the prior untitled `500x500` and `312x237` helper surfaces no longer appear
- this confirms the extra windows were tied to early field-editor creation, not to window placement or the loading overlay

## Resolution summary

Root cause:

- the SwiftUI Settings window was allowing AppKit to auto-focus the first editable control during initial keying
- that auto-focus created the shared `NSTextView` field editor immediately
- field-editor creation triggered the `TUINSWindow` and `SPRoundedWindow` helper surfaces seen in CGWindow inspection

Fix:

- give the Settings window a concrete non-text initial first responder before it becomes key
- preserve that non-text responder on initial keying so the first editable text field is not auto-focused until the user explicitly interacts with it
