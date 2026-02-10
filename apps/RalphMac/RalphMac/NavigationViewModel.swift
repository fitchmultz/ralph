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

private let navigationStateKey = "com.mitchfultz.ralph.navigationState"
private let navigationStateVersion = 1

/// Represents the persisted navigation state for a workspace
struct NavigationState: Codable {
    let version: Int
    let selectedSection: SidebarSection
    let taskViewMode: TaskViewMode
    let selectedTaskID: String?
}

/// Represents the main sidebar navigation sections
enum SidebarSection: String, CaseIterable, Identifiable, Codable {
    case queue = "Queue"
    case quickActions = "Quick Actions"
    case runControl = "Run Control"
    case advancedRunner = "Advanced Runner"
    case analytics = "Analytics"

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .queue: return "list.bullet.rectangle"
        case .quickActions: return "bolt.fill"
        case .runControl: return "play.circle.fill"
        case .advancedRunner: return "terminal.fill"
        case .analytics: return "chart.bar.fill"
        }
    }

    var keyboardShortcut: KeyEquivalent {
        switch self {
        case .queue: return "1"
        case .quickActions: return "2"
        case .runControl: return "3"
        case .advancedRunner: return "4"
        case .analytics: return "5"
        }
    }
}

/// Represents the task view mode for the Queue section
enum TaskViewMode: String, CaseIterable, Identifiable, Codable {
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

    @Published var selectedSection: SidebarSection = .queue {
        didSet { saveNavigationState() }
    }
    @Published var selectedTaskID: String? = nil {
        didSet { saveNavigationState() }
    }
    @Published var sidebarVisibility: NavigationSplitViewVisibility = .automatic
    @Published var taskViewMode: TaskViewMode = .list {
        didSet { saveNavigationState() }
    }

    // MARK: - Private Properties

    private var cancellables = Set<AnyCancellable>()
    private let workspaceID: UUID?

    // MARK: - Initialization

    /// Creates a new NavigationViewModel, optionally loading persisted state for a specific workspace
    /// - Parameter workspaceID: The ID of the workspace to load/save state for, or nil for generic state
    init(workspaceID: UUID? = nil) {
        self.workspaceID = workspaceID
        loadNavigationState()
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

    // MARK: - Persistence

    private var stateKey: String {
        if let workspaceID = workspaceID {
            return "\(navigationStateKey).\(workspaceID.uuidString)"
        }
        return navigationStateKey
    }

    private func saveNavigationState() {
        // Debounce saves to avoid excessive writes during rapid changes
        Task { @MainActor in
            let state = NavigationState(
                version: navigationStateVersion,
                selectedSection: selectedSection,
                taskViewMode: taskViewMode,
                selectedTaskID: selectedTaskID
            )

            if let data = try? JSONEncoder().encode(state) {
                UserDefaults.standard.set(data, forKey: stateKey)
            }
        }
    }

    private func loadNavigationState() {
        guard let data = UserDefaults.standard.data(forKey: stateKey),
              let state = try? JSONDecoder().decode(NavigationState.self, from: data),
              state.version == navigationStateVersion else {
            // Use defaults if no saved state or version mismatch
            return
        }

        selectedSection = state.selectedSection
        taskViewMode = state.taskViewMode
        selectedTaskID = state.selectedTaskID
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
            .sink { _ in
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
        
        // Handle set task view mode (for command palette)
        NotificationCenter.default.publisher(for: .setTaskViewMode)
            .compactMap { $0.object as? TaskViewMode }
            .receive(on: DispatchQueue.main)
            .sink { [weak self] mode in
                self?.setTaskViewMode(mode)
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
    static let setTaskViewMode = Notification.Name("setTaskViewMode")
    static let showGraphView = Notification.Name("showGraphView")
    static let queueFilesExternallyChanged = Notification.Name("queueFilesExternallyChanged")
    static let showTaskDetail = Notification.Name("showTaskDetail")
}
