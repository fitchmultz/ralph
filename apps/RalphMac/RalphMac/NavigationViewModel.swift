/**
 NavigationViewModel

 Responsibilities:
 - Manage the selected sidebar section (Queue, Quick Actions, Advanced Runner)
 - Track the selected task ID for the Queue section
 - Track the selected command ID for the Advanced section
 - Control sidebar visibility state (collapsed/expanded)
 - Persist window-local navigation state for the active workspace tab.

 Does not handle:
 - Window-level tab state (see WindowState)
 - Workspace data/content (see Workspace)
 - Direct UI rendering

 Invariants/assumptions callers must respect:
 - Must be created as @StateObject at the view level that needs navigation state
 - Navigation mutations should flow through the owning workspace scene actions.
 */

public import SwiftUI

private let navigationStateKey = "com.mitchfultz.ralph.navigationState"
private let navigationStateVersion = 1

/// Represents the persisted navigation state for a workspace
public struct NavigationState: Codable {
    public let version: Int
    public let selectedSection: SidebarSection
    public let taskViewMode: TaskViewMode
    public let selectedTaskID: String?
    public let selectedTaskIDs: [String]?

    public init(
        version: Int,
        selectedSection: SidebarSection,
        taskViewMode: TaskViewMode,
        selectedTaskID: String?,
        selectedTaskIDs: [String]?
    ) {
        self.version = version
        self.selectedSection = selectedSection
        self.taskViewMode = taskViewMode
        self.selectedTaskID = selectedTaskID
        self.selectedTaskIDs = selectedTaskIDs
    }
}

/// Represents the main sidebar navigation sections
public enum SidebarSection: String, CaseIterable, Identifiable, Codable {
    case queue = "Queue"
    case quickActions = "Quick Actions"
    case runControl = "Run Control"
    case advancedRunner = "Advanced Runner"
    case analytics = "Analytics"

    public var id: String { rawValue }

    public var icon: String {
        switch self {
        case .queue: return "list.bullet.rectangle"
        case .quickActions: return "bolt.fill"
        case .runControl: return "play.circle.fill"
        case .advancedRunner: return "terminal.fill"
        case .analytics: return "chart.bar.fill"
        }
    }

    public var keyboardShortcut: KeyEquivalent {
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
public enum TaskViewMode: String, CaseIterable, Identifiable, Codable {
    case list = "List"
    case kanban = "Kanban"
    case graph = "Graph"

    public var id: String { rawValue }

    public var icon: String {
        switch self {
        case .list: return "list.bullet"
        case .kanban: return "rectangle.split.3x3"
        case .graph: return "point.3.connected.trianglepath.dotted"
        }
    }
}

@MainActor
public final class NavigationViewModel: ObservableObject {
    // MARK: - Published Properties

    @Published public var selectedSection: SidebarSection = .queue {
        didSet { saveNavigationState() }
    }
    @Published public var selectedTaskID: String? = nil {
        didSet { saveNavigationState() }
    }
    /// Set of task IDs for multi-select mode (Cmd+click selection)
    @Published public var selectedTaskIDs: Set<String> = [] {
        didSet { saveNavigationState() }
    }
    @Published public var sidebarVisibility: NavigationSplitViewVisibility = .automatic
    @Published public var taskViewMode: TaskViewMode = .list {
        didSet { saveNavigationState() }
    }
    
    /// Whether multi-select mode is active (has more than one selection)
    public var isMultiSelectActive: Bool {
        selectedTaskIDs.count > 1
    }
    
    /// Clear all task selections
    public func clearAllTaskSelections() {
        selectedTaskID = nil
        selectedTaskIDs.removeAll()
    }

    // MARK: - Private Properties

    private let workspaceID: UUID?

    // MARK: - Initialization

    /// Creates a new NavigationViewModel, optionally loading persisted state for a specific workspace
    /// - Parameter workspaceID: The ID of the workspace to load/save state for, or nil for generic state
    public init(workspaceID: UUID? = nil) {
        self.workspaceID = workspaceID
        loadNavigationState()
    }

    // MARK: - Public Methods

    /// Navigate to a specific sidebar section
    public func navigate(to section: SidebarSection) {
        selectedSection = section
    }

    /// Toggle sidebar visibility between automatic and detail-only
    public func toggleSidebar() {
        sidebarVisibility = sidebarVisibility == .detailOnly ? .automatic : .detailOnly
    }

    /// Select a task by ID (clears if already selected)
    public func selectTask(_ taskID: String?) {
        selectedTaskID = taskID
    }

    /// Clear the current task selection
    public func clearTaskSelection() {
        selectedTaskID = nil
        selectedTaskIDs.removeAll()
    }

    /// Toggle between list, kanban, and graph view modes
    public func toggleTaskViewMode() {
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
    public func setTaskViewMode(_ mode: TaskViewMode) {
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
                selectedTaskID: selectedTaskID,
                selectedTaskIDs: Array(selectedTaskIDs)
            )

            if let data = try? JSONEncoder().encode(state) {
                RalphAppDefaults.userDefaults.set(data, forKey: stateKey)
            }
        }
    }

    private func loadNavigationState() {
        guard let data = RalphAppDefaults.userDefaults.data(forKey: stateKey),
              let state = try? JSONDecoder().decode(NavigationState.self, from: data),
              state.version == navigationStateVersion else {
            // Use defaults if no saved state or version mismatch
            return
        }

        selectedSection = state.selectedSection
        taskViewMode = state.taskViewMode
        selectedTaskID = state.selectedTaskID
        if let taskIDs = state.selectedTaskIDs {
            selectedTaskIDs = Set(taskIDs)
        }
    }
}
