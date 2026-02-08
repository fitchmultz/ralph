/**
 NavigationViewModel

 Responsibilities:
 - Manage the selected sidebar section (Queue, Quick Actions, Advanced Runner)
 - Track the selected task ID for the Queue section
 - Track the selected command ID for the Advanced section
 - Control sidebar visibility state (collapsed/expanded)
 - Handle navigation notifications from keyboard shortcuts

 Does not handle:
 - Window-level tab state (see WindowState)
 - Workspace data/content (see Workspace)
 - Direct UI rendering

 Invariants/assumptions callers must respect:
 - Must be created as @StateObject at the view level that needs navigation state
 - Notifications are sent via NotificationCenter for cross-view communication
 */

import SwiftUI
import Combine
import RalphCore

/// Represents the main sidebar navigation sections
enum SidebarSection: String, CaseIterable, Identifiable {
    case queue = "Queue"
    case quickActions = "Quick Actions"
    case runControl = "Run Control"
    case advancedRunner = "Advanced Runner"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .queue: return "list.bullet.rectangle"
        case .quickActions: return "bolt.fill"
        case .runControl: return "play.circle.fill"
        case .advancedRunner: return "terminal.fill"
        }
    }

    var keyboardShortcut: KeyEquivalent {
        switch self {
        case .queue: return "1"
        case .quickActions: return "2"
        case .runControl: return "3"
        case .advancedRunner: return "4"
        }
    }
}

/// Represents the task view mode for the Queue section
enum TaskViewMode: String, CaseIterable, Identifiable {
    case list = "List"
    case kanban = "Kanban"
    case graph = "Graph"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .list: return "list.bullet"
        case .kanban: return "rectangle.split.3x3"
        case .graph: return "point.3.connected.trianglepath.dotted"
        }
    }
}

@MainActor
final class NavigationViewModel: ObservableObject {
    // MARK: - Published Properties

    @Published var selectedSection: SidebarSection = .queue
    @Published var selectedTaskID: String?
    @Published var sidebarVisibility: NavigationSplitViewVisibility = .automatic
    @Published var taskViewMode: TaskViewMode = .list

    // MARK: - Private Properties

    private var cancellables = Set<AnyCancellable>()

    // MARK: - Initialization

    init() {
        setupNotificationHandlers()
    }

    // MARK: - Public Methods

    /// Navigate to a specific sidebar section
    func navigate(to section: SidebarSection) {
        selectedSection = section
    }

    /// Toggle sidebar visibility between automatic and detail-only
    func toggleSidebar() {
        sidebarVisibility = sidebarVisibility == .detailOnly ? .automatic : .detailOnly
    }

    /// Select a task by ID (clears if already selected)
    func selectTask(_ taskID: String?) {
        selectedTaskID = taskID
    }

    /// Clear the current task selection
    func clearTaskSelection() {
        selectedTaskID = nil
    }

    /// Toggle between list, kanban, and graph view modes
    func toggleTaskViewMode() {
        switch taskViewMode {
        case .list:
            taskViewMode = .kanban
        case .kanban:
            taskViewMode = .graph
        case .graph:
            taskViewMode = .list
        }
    }
    
    /// Switch to a specific view mode
    func setTaskViewMode(_ mode: TaskViewMode) {
        taskViewMode = mode
    }

    // MARK: - Private Methods

    private func setupNotificationHandlers() {
        // Handle show sidebar section notifications
        NotificationCenter.default.publisher(for: .showSidebarSection)
            .compactMap { $0.object as? SidebarSection }
            .receive(on: DispatchQueue.main)
            .sink { [weak self] section in
                self?.navigate(to: section)
            }
            .store(in: &cancellables)

        // Handle toggle sidebar notifications
        NotificationCenter.default.publisher(for: .toggleSidebar)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in
                self?.toggleSidebar()
            }
            .store(in: &cancellables)

        // Handle clear task selection when workspace changes
        NotificationCenter.default.publisher(for: .workspaceTasksUpdated)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in
                // Validate selected task still exists
                // This is handled by the view, but we could add validation here
            }
            .store(in: &cancellables)

        // Handle toggle task view mode
        NotificationCenter.default.publisher(for: .toggleTaskViewMode)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in
                self?.toggleTaskViewMode()
            }
            .store(in: &cancellables)
        
        // Handle show graph view
        NotificationCenter.default.publisher(for: .showGraphView)
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in
                self?.setTaskViewMode(.graph)
            }
            .store(in: &cancellables)
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let showSidebarSection = Notification.Name("showSidebarSection")
    static let toggleSidebar = Notification.Name("toggleSidebar")
    static let workspaceTasksUpdated = Notification.Name("workspaceTasksUpdated")
    static let toggleTaskViewMode = Notification.Name("toggleTaskViewMode")
    static let showGraphView = Notification.Name("showGraphView")
    static let queueFilesExternallyChanged = Notification.Name("queueFilesExternallyChanged")
}
