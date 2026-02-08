/**
 WindowState

 Responsibilities:
 - Represent the state of a single window including its tabs (workspace IDs).
 - Support persistence for window restoration across app launches.
 - Track the active/selected tab index.

 Does not handle:
 - Workspace content or CLI operations (see Workspace).
 - Direct UI rendering.

 Invariants/assumptions callers must respect:
 - WindowState is Codable for JSON persistence.
 - workspaceIDs reference valid Workspace instances managed by WorkspaceManager.
 - Empty workspaceIDs array is invalid and should be handled gracefully.
 */

public import Foundation

public struct WindowState: Codable, Equatable, Identifiable {
    public let id: UUID
    public var workspaceIDs: [UUID]
    public var selectedTabIndex: Int

    public init(
        id: UUID = UUID(),
        workspaceIDs: [UUID],
        selectedTabIndex: Int = 0
    ) {
        self.id = id
        self.workspaceIDs = workspaceIDs
        self.selectedTabIndex = selectedTabIndex
    }

    /// Validates that the selected index is within bounds
    public mutating func validateSelection() {
        if workspaceIDs.isEmpty {
            selectedTabIndex = 0
        } else {
            selectedTabIndex = min(max(0, selectedTabIndex), workspaceIDs.count - 1)
        }
    }
}
