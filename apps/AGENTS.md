# RalphMac - AI Task Queue GUI

## Project Overview

RalphMac is a native macOS SwiftUI application that provides a graphical interface for the `ralph` CLI tool. The `ralph` CLI is a Rust-based task queue management system for AI agent workflows. This GUI wraps the CLI to provide visual task management, analytics, and execution control.

**Key Features:**
- Multi-window, multi-tab workspace interface with native macOS tab bar integration
- Task queue visualization (List, Kanban board, and Dependency graph views)
- Real-time task execution with Run Control panel
- Analytics dashboard with productivity metrics, burndown charts, and velocity tracking
- Advanced CLI command runner with auto-generated UI from CLI spec
- File watching for external queue changes

## Project Structure

```
RalphMac/
├── RalphMac.xcodeproj/        # Xcode project configuration
├── RalphMac/                   # Main macOS app target
│   ├── RalphMacApp.swift       # App entry point (@main)
│   ├── WindowView.swift        # Window/tab management
│   ├── WorkspaceView.swift     # Main 3-column UI layout
│   ├── NavigationViewModel.swift  # Sidebar navigation state
│   ├── VisualEffectView.swift  # macOS glass morphism effects
│   ├── Info.plist              # Bundle configuration
│   └── Assets.xcassets/        # App icons and assets
│   └── [View files...]         # Task, Kanban, Graph, Analytics views
├── RalphCore/                  # Framework target (core business logic)
│   ├── RalphCLIClient.swift    # Subprocess spawning and streaming
│   ├── RalphCLIExecutableLocator.swift  # Bundle executable resolution
│   ├── RalphModels.swift       # Codable models for CLI JSON output
│   ├── GraphModels.swift       # Dependency graph data models
│   ├── AnalyticsModels.swift   # Analytics data models
│   ├── Workspace.swift         # Per-project workspace state & operations
│   ├── WorkspaceManager.swift  # Global workspace lifecycle management
│   ├── WindowState.swift       # Window restoration state
│   └── QueueFileWatcher.swift  # FSEvents file watching
└── RalphCoreTests/             # Unit and integration tests
    ├── RalphCLIClientTests.swift   # CLI client unit tests
    ├── RalphModelsTests.swift      # Model decoding tests
    └── RalphE2ESmokeTests.swift    # End-to-end smoke tests
```

## Technology Stack

- **Language:** Swift 5.9+
- **UI Framework:** SwiftUI with AppKit integration (NSVisualEffectView)
- **Build System:** Xcode project (`.xcodeproj`)
- **Reactive Programming:** Combine framework
- **File System:** FSEvents (CoreServices) for file watching
- **Process Management:** Foundation.Process for CLI execution
- **Persistence:** UserDefaults for state, JSON for CLI communication

## Build and Run

### Prerequisites

- macOS 14.0+ (Sonoma)
- Xcode 15.0+
- The `ralph` CLI binary must be available (bundled during build)

### Build Commands

```bash
# Open in Xcode
open RalphMac/RalphMac.xcodeproj

# Or build from command line
xcodebuild -project RalphMac/RalphMac.xcodeproj -scheme RalphMac -configuration Debug build

# Build RalphCore framework
xcodebuild -project RalphMac/RalphMac.xcodeproj -scheme RalphCore -configuration Debug build

# Run tests
xcodebuild -project RalphMac/RalphMac.xcodeproj -scheme RalphCoreTests -destination 'platform=macOS' test
```

### Bundling the Ralph CLI

The GUI expects a `ralph` executable in the app bundle at `Contents/MacOS/ralph`. The Xcode project includes a build phase that copies this binary. During development, the executable can be located via:

1. **Bundled binary:** Placed next to the app executable in the bundle
2. **Environment variable:** `RALPH_BIN_PATH` for testing/development
3. **Cargo build:** E2E tests will auto-build the Rust project if binary not found

## Code Style Guidelines

### File Header Documentation

Every Swift file must include a documentation header with these sections:

```swift
/**
 FileName

 Responsibilities:
 - List what this file/module does

 Does not handle:
 - List what is explicitly out of scope

 Invariants/assumptions callers must respect:
 - List important assumptions and constraints
 */
```

### Architecture Patterns

- **MVVM:** Views use `@StateObject` or `@ObservedObject` with ViewModels
- **Observable Pattern:** Core classes extend `ObservableObject` with `@Published` properties
- **Singleton Pattern:** `WorkspaceManager.shared` for global workspace coordination
- **Dependency Injection:** CLI client is injected into Workspaces
- **MainActor:** UI-updating classes are marked `@MainActor`

### Naming Conventions

- **Files:** PascalCase matching the primary type (`RalphModels.swift`)
- **Types:** PascalCase (`RalphCLIClient`, `WorkspaceManager`)
- **Functions/Variables:** camelCase (`loadTasks()`, `workingDirectoryURL`)
- **Constants:** Use descriptive names, not ALL_CAPS
- **Notification Names:** `static let newWorkspaceTabRequested = Notification.Name("newWorkspaceTabRequested")`

### Access Control

- Use `public` explicitly for framework exports (RalphCore)
- Use `internal` (default) for app-private code
- Use `private` for implementation details
- Use `public import` for framework imports

## Testing Strategy

### Test Organization

| Test File | Purpose |
|-----------|---------|
| `RalphCLIClientTests.swift` | Unit tests for subprocess spawning, streaming, cancellation |
| `RalphModelsTests.swift` | JSON decoding/encoding, forward compatibility |
| `RalphE2ESmokeTests.swift` | End-to-end workflow with real ralph binary |

### Running Tests

```bash
# Run all tests in Xcode
Cmd+U

# Run from command line
xcodebuild -project RalphMac/RalphMac.xcodeproj -scheme RalphCoreTests test

# With specific environment (for E2E tests)
RALPH_BIN_PATH=/path/to/ralph xcodebuild -project RalphMac/RalphMac.xcodeproj -scheme RalphCoreTests test
```

### UI Testing (RalphMacUITests)

UI tests are **excluded by default** in CI because they take over the mouse and keyboard, making your computer unusable during test execution.

```bash
# Run tests excluding UI tests (default)
make macos-test

# Build/sign UI bundles once for a local debugging session
make macos-ui-build-for-testing

# Re-run UI tests without rebuilding/signing again
make macos-ui-retest
RALPH_UI_ONLY_TESTING=RalphMacUITests/RalphMacUITests/test_createNewTask_viaQuickCreate make macos-ui-retest

# Run tests including UI tests end-to-end (interactive - will take over mouse/keyboard)
make macos-test-ui

# Or toggle via environment variable
RALPH_UI_TESTS=1 make macos-test   # Include UI tests (headed/interactive)
RALPH_UI_TESTS=0 make macos-test   # Skip UI tests (default)
```

**Warning:** UI tests (`RALPH_UI_TESTS=1` or `make macos-ui-retest`) will move your mouse cursor and send keyboard events. Do not use your computer while UI tests are running.

### E2E Test Behavior

E2E tests look for the `ralph` binary in this order:
1. `RALPH_BIN_PATH` environment variable
2. `target/debug/ralph` relative to repo root
3. Auto-build with `cargo build -p ralph-agent-loop` if not found

Tests create temporary directories that are cleaned up after each test.

## Module Responsibilities

### RalphCore Framework

| File | Purpose |
|------|---------|
| `RalphCLIClient.swift` | Process spawning, stdout/stderr streaming, async/await API |
| `RalphModels.swift` | CLI spec parsing, task models, argument building |
| `GraphModels.swift` | Dependency graph data structures |
| `AnalyticsModels.swift` | Productivity, velocity, burndown data models |
| `Workspace.swift` | Per-project state, CLI operations, task management |
| `WorkspaceManager.swift` | Global workspace lifecycle, window restoration |
| `QueueFileWatcher.swift` | FSEvents-based file watching for queue changes |

### RalphMac App

| File | Purpose |
|------|---------|
| `RalphMacApp.swift` | App entry, menu commands, window configuration |
| `WindowView.swift` | Tab-based window management |
| `WorkspaceView.swift` | 3-column NavigationSplitView layout |
| `NavigationViewModel.swift` | Sidebar section state, view modes |
| `VisualEffectView.swift` | Glass morphism effects, custom button styles |

## Key Design Patterns

### Workspace Isolation

Each project/workspace has:
- Independent working directory
- Isolated CLI execution context
- Separate task list and state
- Per-workspace file watching

### CLI Integration

The GUI communicates with the CLI via:
1. **JSON output:** `--format json` for structured data
2. **CLI spec introspection:** `ralph __cli-spec --format json` for command discovery
3. **Streaming output:** Real-time stdout/stderr via AsyncStream
4. **Exit status:** RalphCLIExitStatus with code and termination reason

### State Persistence

- **Window state:** Restored via `WindowState` Codable + UserDefaults
- **Workspace state:** Per-workspace UserDefaults with namespaced keys (`com.mitchfultz.ralph.workspace.{id}.{field}`)
- **Recent directories:** Stored per-workspace, validated on load

### Notification System

Cross-view communication uses NotificationCenter:
- `newWorkspaceTabRequested`, `closeActiveTabRequested` - Tab management
- `showSidebarSection`, `toggleSidebar` - Navigation
- `showTaskCreation`, `toggleTaskViewMode` - Task UI
- `queueFilesExternallyChanged` - File watcher events

## Security Considerations

1. **Executable Validation:** `RalphCLIClient` verifies executable exists and is executable
2. **Sandboxing:** App uses standard macOS sandbox (configured in entitlements)
3. **File Access:** Working directories are user-selected via NSOpenPanel
4. **No Network:** No direct network calls from GUI; all network through CLI subprocess

## Common Tasks

### Adding a New View

1. Create Swift file in `RalphMac/RalphMac/`
2. Add to `project.pbxproj` (or let Xcode handle it)
3. Follow header documentation template
4. Use `GlassButtonStyle` for buttons
5. Use `VisualEffectView` for backgrounds

### Adding a New Model

1. Add to appropriate file in `RalphCore/` (or create new)
2. Make it `Codable, Sendable, Equatable` for JSON and thread safety
3. Use `RalphJSONValue` for forward-compatible JSON fields
4. Add unit tests in `RalphCoreTests/`

### Adding CLI Command Support

1. Ensure CLI outputs JSON with `--format json`
2. Create corresponding model in RalphCore
3. Add method to `Workspace` for the operation
4. Wire up UI in appropriate View

## Debugging Tips

- **CLI not found:** Check `RalphCLIExecutableLocator.bundledRalphExecutableURL()` and build phase
- **No task updates:** Check `QueueFileWatcher` is started and FSEvents working
- **JSON decoding errors:** Verify CLI output format matches model expectations
- **SwiftUI preview crashes:** Previews may not work with Process/Bundle lookups

## Dependencies

The project has **no external package dependencies**. All functionality uses:
- Swift Standard Library
- Foundation
- SwiftUI
- Combine
- AppKit (for NSVisualEffectView, NSOpenPanel)
- CoreServices (for FSEvents)
- XCTest (for testing)
