/**
 GraphPresentation

 Responsibilities:
 - Build view-ready graph presentation data from CLI graph payloads and task relationships.
 - Produce deterministic initial node placement and cycle metadata for rendering layers.

 Does not handle:
 - Force-directed simulation.
 - SwiftUI rendering concerns.

 Invariants/assumptions callers must respect:
 - Dependency edges come from graph payload dependencies.
 - Blocks and relates-to edges are derived from the full task list when available.
 */

public import Foundation
import CoreGraphics

public struct RalphGraphPresentation: Sendable, Equatable {
    public let nodes: [PositionedNode]
    public let edges: [GraphEdge]
    public let cycleResult: CycleDetectionResult
    public let edgesInCycles: Set<GraphEdge.ID>

    public init(
        nodes: [PositionedNode],
        edges: [GraphEdge],
        cycleResult: CycleDetectionResult,
        edgesInCycles: Set<GraphEdge.ID>
    ) {
        self.nodes = nodes
        self.edges = edges
        self.cycleResult = cycleResult
        self.edgesInCycles = edgesInCycles
    }
}

public enum RalphGraphPresentationBuilder {
    public static func build(
        graphData: RalphGraphDocument,
        tasks: [RalphTask],
        selectedTaskID: String?
    ) -> RalphGraphPresentation {
        let edges = buildEdges(graphData: graphData, tasks: tasks)
        let cycleResult = GraphAlgorithms.detectCycles(edges: edges)
        let edgesInCycles = GraphAlgorithms.edgesInCycles(edges: edges)
        let nodes = buildNodes(tasks: graphData.tasks, selectedTaskID: selectedTaskID)

        return RalphGraphPresentation(
            nodes: nodes,
            edges: edges,
            cycleResult: cycleResult,
            edgesInCycles: edgesInCycles
        )
    }

    private static func buildNodes(tasks: [RalphGraphNode], selectedTaskID: String?) -> [PositionedNode] {
        let sortedTasks = tasks.sorted { $0.id < $1.id }
        let count = max(sortedTasks.count, 1)
        let radius = max(180.0, CGFloat(count) * 18.0)

        return sortedTasks.enumerated().map { index, task in
            let angle = (2.0 * Double.pi * Double(index)) / Double(count)
            let orbit = radius + CGFloat(index / 8) * 36.0
            let position = CGPoint(
                x: cos(angle) * orbit,
                y: sin(angle) * orbit
            )
            return PositionedNode(
                id: task.id,
                position: position,
                task: task,
                isSelected: task.id == selectedTaskID
            )
        }
    }

    private static func buildEdges(graphData: RalphGraphDocument, tasks: [RalphTask]) -> [GraphEdge] {
        var edges: [GraphEdge] = []

        for task in graphData.tasks {
            for dependencyID in task.dependencies {
                edges.append(GraphEdge(from: task.id, to: dependencyID, type: .dependency))
            }
        }

        let tasksByID = Dictionary(uniqueKeysWithValues: tasks.map { ($0.id, $0) })
        for task in graphData.tasks {
            guard let fullTask = tasksByID[task.id] else { continue }

            if let blocks = fullTask.blocks {
                for blockedID in blocks {
                    edges.append(GraphEdge(from: task.id, to: blockedID, type: .blocks))
                }
            }

            if let relatesTo = fullTask.relatesTo {
                for relatedID in relatesTo where task.id < relatedID {
                    edges.append(GraphEdge(from: task.id, to: relatedID, type: .relatesTo))
                }
            }
        }

        return edges
    }
}
