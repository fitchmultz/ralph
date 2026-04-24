/**
 WindowCommandRouting

 Purpose:
 - Define shared window command identifiers used by menu and command-palette actions.

 Responsibilities:
 - Define shared window command identifiers used by menu and command-palette actions.
 - Define focused-scene action contracts for window-scoped commands.
 - Define focused-scene action contracts for workspace-only UI actions.

 Does not handle:
 - Command execution logic (implemented by WindowView).
 - Menu command definitions (implemented in RalphMacApp command types).
 - Notification-based broadcast routing.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Focused scene values are optional and only available for the active workspace window.
 - Window commands should execute only through focused scene actions to preserve window isolation.
 */

import Foundation
import SwiftUI
import RalphCore

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
@MainActor
final class WorkspaceWindowActions {
    private var newWindowHandler: (() -> Void)?
    private var newTabHandler: (() -> Void)?
    private var closeTabHandler: (() -> Void)?
    private var closeWindowHandler: (() -> Void)?
    private var nextTabHandler: (() -> Void)?
    private var previousTabHandler: (() -> Void)?
    private var duplicateTabHandler: (() -> Void)?

    init() {}

    func configure(
        newWindow: @escaping () -> Void,
        newTab: @escaping () -> Void,
        closeTab: @escaping () -> Void,
        closeWindow: @escaping () -> Void,
        nextTab: @escaping () -> Void,
        previousTab: @escaping () -> Void,
        duplicateTab: @escaping () -> Void
    ) {
        newWindowHandler = newWindow
        newTabHandler = newTab
        closeTabHandler = closeTab
        closeWindowHandler = closeWindow
        nextTabHandler = nextTab
        previousTabHandler = previousTab
        duplicateTabHandler = duplicateTab
    }

    func perform(_ command: WindowCommandRoute) {
        switch command {
        case .newWindow:
            newWindowHandler?()
        case .newTab:
            newTabHandler?()
        case .closeTab:
            closeTabHandler?()
        case .closeWindow:
            closeWindowHandler?()
        case .nextTab:
            nextTabHandler?()
        case .previousTab:
            previousTabHandler?()
        case .duplicateTab:
            duplicateTabHandler?()
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
@MainActor
final class WorkspaceUIActions {
    private var showCommandPaletteHandler: (() -> Void)?
    private var navigateToSectionHandler: ((SidebarSection) -> Void)?
    private var toggleSidebarHandler: (() -> Void)?
    private var toggleTaskViewModeHandler: (() -> Void)?
    private var setTaskViewModeHandler: ((TaskViewMode) -> Void)?
    private var showTaskCreationHandler: (() -> Void)?
    private var showTaskDecomposeHandler: ((String?) -> Void)?
    private var showTaskDetailHandler: ((String) -> Void)?
    private var startWorkOnSelectedTaskHandler: (() -> Void)?

    init() {}

    func configure(
        showCommandPalette: @escaping () -> Void,
        navigateToSection: @escaping (SidebarSection) -> Void,
        toggleSidebar: @escaping () -> Void,
        toggleTaskViewMode: @escaping () -> Void,
        setTaskViewMode: @escaping (TaskViewMode) -> Void,
        showTaskCreation: @escaping () -> Void,
        showTaskDecompose: @escaping (String?) -> Void,
        showTaskDetail: @escaping (String) -> Void,
        startWorkOnSelectedTask: @escaping () -> Void
    ) {
        showCommandPaletteHandler = showCommandPalette
        navigateToSectionHandler = navigateToSection
        toggleSidebarHandler = toggleSidebar
        toggleTaskViewModeHandler = toggleTaskViewMode
        setTaskViewModeHandler = setTaskViewMode
        showTaskCreationHandler = showTaskCreation
        showTaskDecomposeHandler = showTaskDecompose
        showTaskDetailHandler = showTaskDetail
        startWorkOnSelectedTaskHandler = startWorkOnSelectedTask
    }

    func showCommandPalette() {
        showCommandPaletteHandler?()
    }

    func navigateToSection(_ section: SidebarSection) {
        navigateToSectionHandler?(section)
    }

    func toggleSidebar() {
        toggleSidebarHandler?()
    }

    func toggleTaskViewMode() {
        toggleTaskViewModeHandler?()
    }

    func setTaskViewMode(_ mode: TaskViewMode) {
        setTaskViewModeHandler?(mode)
    }

    func showTaskCreation() {
        showTaskCreationHandler?()
    }

    func showTaskDecompose(_ taskID: String?) {
        showTaskDecomposeHandler?(taskID)
    }

    func showTaskDetail(_ taskID: String) {
        showTaskDetailHandler?(taskID)
    }

    func startWorkOnSelectedTask() {
        startWorkOnSelectedTaskHandler?()
    }
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
