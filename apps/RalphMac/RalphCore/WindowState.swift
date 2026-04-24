/**
 WindowState

 Purpose:
 - Represent the state of a single window including its tabs (workspace IDs).

 Responsibilities:
 - Represent the state of a single window including its tabs (workspace IDs).
 - Support persistence for window restoration across app launches.
 - Track the active/selected tab index.

 Does not handle:
 - Workspace content or CLI operations (see Workspace).
 - Direct UI rendering.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

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
    public let version: Int  // Added for migration support

    public init(
        id: UUID = UUID(),
        workspaceIDs: [UUID],
        selectedTabIndex: Int = 0,
        version: Int = 1
    ) {
        self.id = id
        self.workspaceIDs = workspaceIDs
        self.selectedTabIndex = selectedTabIndex
        self.version = version
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
