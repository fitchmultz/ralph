/**
 WindowCommandRouting

 Responsibilities:
 - Define shared window command identifiers used by menu and command-palette actions.
 - Define focused-scene action contracts for window-scoped commands.
 - Define focused-scene action contracts for workspace-only UI actions.

 Does not handle:
 - Command execution logic (implemented by WindowView).
 - Menu command definitions (implemented in RalphMacApp command types).
 - Notification-based broadcast routing.

 Invariants/assumptions callers must respect:
 - Focused scene values are optional and only available for the active workspace window.
 - Window commands should execute only through focused scene actions to preserve window isolation.
 */

import Foundation
import SwiftUI

/// Shared window command identifiers used by menu and command-palette surfaces.
enum WindowCommandRoute: String {
    case newWindow
    case newTab
    case closeTab
    case closeWindow
    case nextTab
    case previousTab
    case duplicateTab
}

/// Focused-scene actions that mutate the active window's tab/window state.
struct WorkspaceWindowActions {
    let newWindow: () -> Void
    let newTab: () -> Void
    let closeTab: () -> Void
    let closeWindow: () -> Void
    let nextTab: () -> Void
    let previousTab: () -> Void
    let duplicateTab: () -> Void

    func perform(_ command: WindowCommandRoute) {
        switch command {
        case .newWindow:
            newWindow()
        case .newTab:
            newTab()
        case .closeTab:
            closeTab()
        case .closeWindow:
            closeWindow()
        case .nextTab:
            nextTab()
        case .previousTab:
            previousTab()
        case .duplicateTab:
            duplicateTab()
        }
    }
}

private struct WorkspaceWindowActionsKey: FocusedValueKey {
    typealias Value = WorkspaceWindowActions
}

extension FocusedValues {
    var workspaceWindowActions: WorkspaceWindowActions? {
        get { self[WorkspaceWindowActionsKey.self] }
        set { self[WorkspaceWindowActionsKey.self] = newValue }
    }
}

/// Workspace UI actions exposed to commands from the active scene.
struct WorkspaceUIActions {
    let showCommandPalette: () -> Void
    let navigateToSection: (SidebarSection) -> Void
    let toggleSidebar: () -> Void
    let toggleTaskViewMode: () -> Void
    let setTaskViewMode: (TaskViewMode) -> Void
    let showTaskCreation: () -> Void
    let showTaskDecompose: (String?) -> Void
    let showTaskDetail: (String) -> Void
    let startWorkOnSelectedTask: () -> Void
}

private struct WorkspaceUIActionsKey: FocusedValueKey {
    typealias Value = WorkspaceUIActions
}

extension FocusedValues {
    var workspaceUIActions: WorkspaceUIActions? {
        get { self[WorkspaceUIActionsKey.self] }
        set { self[WorkspaceUIActionsKey.self] = newValue }
    }
}
