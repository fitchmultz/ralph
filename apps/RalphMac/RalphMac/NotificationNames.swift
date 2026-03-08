/**
 NotificationNames

 Responsibilities:
 - Define all notification names used across the RalphMac app as typed constants.
 - Provide a single source of truth for notification name strings.

 Does not handle:
 - Notification posting or handling logic.
 - Any business logic related to notifications.

 Invariants/assumptions callers must respect:
 - All notification names should be defined here, not as string literals.
 - Use these static properties instead of raw Notification.Name("string") calls.
 */

import Foundation
import RalphCore

/// Targeted workspace routing payload used when an unfocused surface, such as the menu bar,
/// needs to open content in a specific workspace without mutating other windows.
struct WorkspaceRouteRequest {
    let workspaceID: UUID
    let taskID: String?

    init(workspaceID: UUID, taskID: String? = nil) {
        self.workspaceID = workspaceID
        self.taskID = taskID
    }
}

// MARK: - Window Lifecycle

extension Notification.Name {
    /// Request to save all window states (e.g., on app termination)
    static let saveAllWindowStatesRequested = Notification.Name("saveAllWindowStatesRequested")
}

// MARK: - Workspace Management

extension Notification.Name {
    /// Activate an existing workspace by ID (object contains UUID)
    static let activateWorkspace = Notification.Name("activateWorkspace")

    /// A new workspace was opened from a URL scheme
    static let workspaceOpenedFromURL = Notification.Name("workspaceOpenedFromURL")

    /// Workspace tasks have been updated
    static let workspaceTasksUpdated = Notification.Name("workspaceTasksUpdated")
}

// MARK: - Navigation

extension Notification.Name {
    /// Show a specific sidebar section (object contains SidebarSection)
    static let showSidebarSection = Notification.Name("showSidebarSection")

    /// Toggle sidebar visibility
    static let toggleSidebar = Notification.Name("toggleSidebar")

    /// Toggle task view mode
    static let toggleTaskViewMode = Notification.Name("toggleTaskViewMode")

    /// Set a specific task view mode
    static let setTaskViewMode = Notification.Name("setTaskViewMode")

    /// Show the graph view
    static let showGraphView = Notification.Name("showGraphView")

    /// Show task detail for a specific task (object contains task ID)
    static let showTaskDetail = Notification.Name("showTaskDetail")
}

// MARK: - Task Management

extension Notification.Name {
    /// Show the task creation sheet
    static let showTaskCreation = Notification.Name("showTaskCreation")

    /// Show the task decomposition sheet
    static let showTaskDecompose = Notification.Name("showTaskDecompose")

    /// Start work on the selected task
    static let startWorkOnSelectedTask = Notification.Name("startWorkOnSelectedTask")

    /// Check for CLI updates
    static let checkForCLIUpdates = Notification.Name("checkForCLIUpdates")
}

// MARK: - Menu Bar

extension Notification.Name {
    /// Show main app from menu bar
    static let showMainAppFromMenuBar = Notification.Name("showMainAppFromMenuBar")

    /// Show task detail from menu bar
    static let showTaskDetailFromMenuBar = Notification.Name("showTaskDetailFromMenuBar")

    /// Quick add task from menu bar
    static let quickAddTaskFromMenuBar = Notification.Name("quickAddTaskFromMenuBar")
}
