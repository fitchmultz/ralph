/**
 GraphAlgorithms

 Responsibilities:
 - Provide graph algorithms for dependency analysis (cycle detection, etc.).
 - Detect circular dependencies using DFS with recursion stack.
 - Support prevention of cycle creation during editing.

 Does not handle:
 - UI rendering or visualization (handled by DependencyGraphView).
 - Direct graph data fetching (handled by Workspace).

 Invariants/assumptions:
 - Edge types: dependency and blocks participate in cycle detection.
 - relatesTo edges are inherently bidirectional and excluded from cycle detection.
 - Self-loops (A → A) are treated as cycles.
 */

public import Foundation

/// Represents the result of a cycle detection operation
public struct CycleDetectionResult: Equatable, Sendable {
    public let hasCycle: Bool
    /// Each cycle is an array of task IDs forming the cycle (e.g., ["A", "B", "C"] for A→B→C→A)
    public let cycles: [[String]]

    public init(hasCycle: Bool, cycles: [[String]] = []) {
        self.hasCycle = hasCycle
        self.cycles = cycles
    }

    public static let noCycles = CycleDetectionResult(hasCycle: false, cycles: [])
}

/// Graph algorithms for dependency analysis
public enum GraphAlgorithms {

    /// Edge types that participate in cycle detection
    private static let cycleEdgeTypes: [GraphEdge.EdgeType] = [.dependency, .blocks]

    /// Detects cycles in a directed graph using DFS with recursion stack.
    /// Mirrors the algorithm in validation.rs has_cycle() function.
    ///
    /// Only considers `.dependency` and `.blocks` edge types.
    /// `.relatesTo` edges are excluded as they are inherently bidirectional.
    ///
    /// - Parameter edges: All edges in the graph
    /// - Returns: CycleDetectionResult containing found cycles
    public static func detectCycles(edges: [GraphEdge]) -> CycleDetectionResult {
        // Filter to only edges that participate in cycle detection
        let cycleEdges = edges.filter { cycleEdgeTypes.contains($0.type) }

        // Build adjacency list
        var adjacencyList: [String: [String]] = [:]
        for edge in cycleEdges {
            adjacencyList[edge.from, default: []].append(edge.to)
        }

        // Collect all nodes (both sources and destinations)
        var allNodes = Set<String>()
        for edge in cycleEdges {
            allNodes.insert(edge.from)
            allNodes.insert(edge.to)
        }

        var visited = Set<String>()
        var recStack = Set<String>()
        var foundCycles: [[String]] = []

        for node in allNodes {
            if !visited.contains(node) {
                var currentPath: [String] = []
                findCyclesDFS(
                    node: node,
                    adjacencyList: adjacencyList,
                    visited: &visited,
                    recStack: &recStack,
                    currentPath: &currentPath,
                    foundCycles: &foundCycles
                )
            }
        }

        return CycleDetectionResult(
            hasCycle: !foundCycles.isEmpty,
            cycles: foundCycles
        )
    }

    /// Recursive DFS to find all cycles
    private static func findCyclesDFS(
        node: String,
        adjacencyList: [String: [String]],
        visited: inout Set<String>,
        recStack: inout Set<String>,
        currentPath: inout [String],
        foundCycles: inout [[String]]
    ) {
        visited.insert(node)
        recStack.insert(node)
        currentPath.append(node)

        if let neighbors = adjacencyList[node] {
            for neighbor in neighbors {
                // Self-loop is a cycle
                if neighbor == node {
                    foundCycles.append([node])
                    continue
                }

                if !visited.contains(neighbor) {
                    findCyclesDFS(
                        node: neighbor,
                        adjacencyList: adjacencyList,
                        visited: &visited,
                        recStack: &recStack,
                        currentPath: &currentPath,
                        foundCycles: &foundCycles
                    )
                } else if recStack.contains(neighbor) {
                    // Found a cycle - extract the cycle from currentPath
                    if let cycleStartIndex = currentPath.firstIndex(of: neighbor) {
                        let cycle = Array(currentPath[cycleStartIndex...])
                        // Normalize cycle: start from smallest element for consistent ordering
                        let normalizedCycle = normalizeCycle(cycle)
                        if !foundCycles.contains(normalizedCycle) {
                            foundCycles.append(normalizedCycle)
                        }
                    }
                }
            }
        }

        recStack.remove(node)
        currentPath.removeLast()
    }

    /// Normalizes a cycle by rotating it to start from the smallest element
    private static func normalizeCycle(_ cycle: [String]) -> [String] {
        guard let minElement = cycle.min(), let minIndex = cycle.firstIndex(of: minElement) else {
            return cycle
        }
        // Rotate so smallest element is first
        let rotated = Array(cycle[minIndex...] + cycle[..<minIndex])
        return rotated
    }

    /// Checks if adding a new edge would create a cycle.
    /// Used by TaskRelationshipPicker to prevent cycle creation during editing.
    ///
    /// - Parameters:
    ///   - existingEdges: Current edges in the graph
    ///   - newEdge: The edge being considered for addition
    ///   - allTaskIDs: All task IDs in the workspace (to ensure nodes exist)
    /// - Returns: True if adding the edge would create a cycle
    public static func wouldCreateCycle(
        existingEdges: [GraphEdge],
        newEdge: GraphEdge,
        allTaskIDs: [String]
    ) -> Bool {
        // Self-loop is always a cycle
        if newEdge.from == newEdge.to {
            return true
        }

        // Only dependency and blocks edges can create cycles
        guard cycleEdgeTypes.contains(newEdge.type) else {
            return false
        }

        // Build graph with existing edges + new edge
        var adjacencyList: [String: [String]] = [:]
        let cycleEdges = existingEdges.filter { cycleEdgeTypes.contains($0.type) }

        for edge in cycleEdges {
            adjacencyList[edge.from, default: []].append(edge.to)
        }
        // Add the new edge
        adjacencyList[newEdge.from, default: []].append(newEdge.to)

        // Check if there's now a path from newEdge.to back to newEdge.from
        // This would mean: newEdge.from → newEdge.to → ... → newEdge.from (cycle)
        return canReach(
            from: newEdge.to,
            to: newEdge.from,
            adjacencyList: adjacencyList,
            visited: []
        )
    }

    /// DFS to check if target is reachable from start
    private static func canReach(
        from: String,
        to: String,
        adjacencyList: [String: [String]],
        visited: Set<String>
    ) -> Bool {
        if from == to {
            return true
        }

        var visited = visited
        visited.insert(from)

        guard let neighbors = adjacencyList[from] else {
            return false
        }

        for neighbor in neighbors {
            if !visited.contains(neighbor) {
                if canReach(from: neighbor, to: to, adjacencyList: adjacencyList, visited: visited) {
                    return true
                }
            }
        }

        return false
    }

    /// Returns all edge IDs that are part of any cycle.
    /// Used for visual highlighting of cycle edges.
    ///
    /// - Parameter edges: All edges in the graph
    /// - Returns: Set of edge IDs that participate in cycles
    public static func edgesInCycles(edges: [GraphEdge]) -> Set<GraphEdge.ID> {
        let cycleResult = detectCycles(edges: edges)
        guard cycleResult.hasCycle else {
            return []
        }

        var edgesInCycles = Set<GraphEdge.ID>()

        // Build a map for quick edge lookup
        // We need to identify edges that connect consecutive nodes in a cycle
        for cycle in cycleResult.cycles {
            if cycle.count == 1 {
                // Self-loop: find edge A → A
                let node = cycle[0]
                if let edge = edges.first(where: { $0.from == node && $0.to == node && cycleEdgeTypes.contains($0.type) }) {
                    edgesInCycles.insert(edge.id)
                }
            } else {
                // Multi-node cycle: find edges between consecutive nodes
                for i in 0..<cycle.count {
                    let from = cycle[i]
                    let to = cycle[(i + 1) % cycle.count]
                    if let edge = edges.first(where: {
                        $0.from == from && $0.to == to && cycleEdgeTypes.contains($0.type)
                    }) {
                        edgesInCycles.insert(edge.id)
                    }
                }
            }
        }

        return edgesInCycles
    }
}
