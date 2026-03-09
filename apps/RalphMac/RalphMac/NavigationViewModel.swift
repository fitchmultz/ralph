//!
//! NavigationViewModel
//!
//! Purpose:
//! - Own workspace-local navigation state for the macOS workspace surface.
//!
//! Responsibilities:
//! - Track the selected sidebar section, task selection, and queue presentation mode.
//! - Persist and restore navigation state through `NavigationStateStore`.
//! - Surface persistence failures so workspace operational health can report them.
//!
//! Scope:
//! - Window-local navigation only. Workspace data, routing, and rendering live elsewhere.
//!
//! Usage:
//! - Construct as `@StateObject` from the owning workspace view and provide an issue sink when
//!   navigation persistence should participate in workspace operational health.
//!
//! Invariants/Assumptions:
//! - Navigation mutations occur on the main actor.
//! - Version mismatches reset to defaults rather than attempting compatibility shims.

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
    @Published public var selectedSection: SidebarSection = .queue {
        didSet { persistNavigationState() }
    }
    @Published public var selectedTaskID: String? = nil {
        didSet { persistNavigationState() }
    }
    @Published public var selectedTaskIDs: Set<String> = [] {
        didSet { persistNavigationState() }
    }
    @Published public var sidebarVisibility: NavigationSplitViewVisibility = .automatic
    @Published public var taskViewMode: TaskViewMode = .list {
        didSet { persistNavigationState() }
    }
    @Published public private(set) var persistenceIssue: PersistenceIssue?

    public var isMultiSelectActive: Bool {
        selectedTaskIDs.count > 1
    }

    private let workspaceID: UUID?
    private let store: NavigationStateStore
    private let issueSink: (PersistenceIssue?) -> Void

    public init(
        workspaceID: UUID? = nil,
        store: NavigationStateStore = NavigationStateStore(),
        issueSink: @escaping (PersistenceIssue?) -> Void = { _ in }
    ) {
        self.workspaceID = workspaceID
        self.store = store
        self.issueSink = issueSink
        loadNavigationState()
    }

    public func clearAllTaskSelections() {
        selectedTaskID = nil
        selectedTaskIDs.removeAll()
    }

    public func navigate(to section: SidebarSection) {
        selectedSection = section
    }

    public func toggleSidebar() {
        sidebarVisibility = sidebarVisibility == .detailOnly ? .automatic : .detailOnly
    }

    public func selectTask(_ taskID: String?) {
        selectedTaskID = taskID
    }

    public func clearTaskSelection() {
        selectedTaskID = nil
        selectedTaskIDs.removeAll()
    }

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

    public func setTaskViewMode(_ mode: TaskViewMode) {
        taskViewMode = mode
    }

    private var stateKey: String {
        if let workspaceID {
            return "\(navigationStateKey).\(workspaceID.uuidString)"
        }
        return navigationStateKey
    }

    private func persistNavigationState() {
        let state = NavigationState(
            version: navigationStateVersion,
            selectedSection: selectedSection,
            taskViewMode: taskViewMode,
            selectedTaskID: selectedTaskID,
            selectedTaskIDs: Array(selectedTaskIDs)
        )

        do {
            try store.saveState(state, forKey: stateKey)
            updatePersistenceIssue(nil)
        } catch {
            updatePersistenceIssue(
                PersistenceIssue(
                    domain: .navigationState,
                    operation: .save,
                    context: stateKey,
                    error: error
                )
            )
        }
    }

    private func loadNavigationState() {
        do {
            guard let state = try store.loadState(forKey: stateKey) else {
                updatePersistenceIssue(nil)
                return
            }

            guard state.version == navigationStateVersion else {
                store.removeState(forKey: stateKey)
                updatePersistenceIssue(nil)
                return
            }

            selectedSection = state.selectedSection
            taskViewMode = state.taskViewMode
            selectedTaskID = state.selectedTaskID
            selectedTaskIDs = Set(state.selectedTaskIDs ?? [])
            updatePersistenceIssue(nil)
        } catch {
            store.removeState(forKey: stateKey)
            updatePersistenceIssue(
                PersistenceIssue(
                    domain: .navigationState,
                    operation: .load,
                    context: stateKey,
                    error: error
                )
            )
        }
    }

    private func updatePersistenceIssue(_ issue: PersistenceIssue?) {
        persistenceIssue = issue
        issueSink(issue)
    }
}
