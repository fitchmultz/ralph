/**
 GraphModels

 Responsibilities:
 - Define Codable models for parsing `ralph queue graph --format json` output.
 - Represent graph nodes, edges, critical paths, and summary statistics.

 Does not handle:
 - Graph layout computation (handled by DependencyGraphView).
 - Direct CLI calls (handled by Workspace).
 */

public import Foundation

/// Represents the full graph data from `ralph queue graph --format json`
public struct RalphGraphDocument: Codable, Sendable, Equatable {
    public let summary: RalphGraphSummary
    public let criticalPaths: [RalphCriticalPath]
    public let tasks: [RalphGraphNode]
    
    private enum CodingKeys: String, CodingKey {
        case summary, criticalPaths = "critical_paths", tasks
    }
}

public struct RalphGraphSummary: Codable, Sendable, Equatable {
    public let totalTasks: Int
    public let runnableTasks: Int
    public let blockedTasks: Int
    
    private enum CodingKeys: String, CodingKey {
        case totalTasks = "total_tasks"
        case runnableTasks = "runnable_tasks"
        case blockedTasks = "blocked_tasks"
    }
}

public struct RalphCriticalPath: Codable, Sendable, Equatable {
    public let path: [String]
    public let length: Int
    public let isBlocked: Bool
    
    private enum CodingKeys: String, CodingKey {
        case path, length, isBlocked = "blocked"
    }
}

public struct RalphGraphNode: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let title: String
    public let status: String
    public let dependencies: [String]
    public let dependents: [String]
    public let isCritical: Bool
    
    private enum CodingKeys: String, CodingKey {
        case id, title, status, dependencies, dependents, isCritical = "critical"
    }
    
    public var statusEnum: RalphTaskStatus? {
        RalphTaskStatus(rawValue: status)
    }
}

/// Computed edge representation for drawing
public struct GraphEdge: Identifiable, Equatable, Sendable {
    public let id: UUID
    public let from: String
    public let to: String
    public let type: EdgeType
    
    public enum EdgeType: Sendable {
        case dependency      // depends_on (solid arrow)
        case blocks          // blocks (red line)
        case relatesTo       // relates_to (dashed gray)
    }
    
    public init(id: UUID = UUID(), from: String, to: String, type: EdgeType) {
        self.id = id
        self.from = from
        self.to = to
        self.type = type
    }
}

/// Positioned node for rendering
public struct PositionedNode: Identifiable, Equatable, Sendable {
    public let id: String
    public var position: CGPoint
    public let task: RalphGraphNode
    public var isSelected: Bool
    
    public init(id: String, position: CGPoint, task: RalphGraphNode, isSelected: Bool = false) {
        self.id = id
        self.position = position
        self.task = task
        self.isSelected = isSelected
    }
    
    public static func == (lhs: PositionedNode, rhs: PositionedNode) -> Bool {
        lhs.id == rhs.id && lhs.position == rhs.position && lhs.isSelected == rhs.isSelected
    }
}

/// Extension to make CGPoint Sendable-compatible.
/// CGPoint is a value type composed of CGFloat (Double) - inherently thread-safe.
/// Using @unchecked Sendable for retroactive conformance since CoreGraphics may declare
/// this in a future SDK version. This is safe because CGPoint contains no reference types
/// and has no mutable shared state.
extension CGPoint: @unchecked Sendable {}
