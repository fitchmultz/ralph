/**
 GraphLayoutEngine

 Responsibilities:
 - Perform force-directed graph layout independent of SwiftUI views.
 - Apply deterministic physics settings for dependency-heavy task graphs.

 Does not handle:
 - Graph data fetching or presentation building.
 - Rendering, hit testing, or viewport transforms.

 Invariants/assumptions callers must respect:
 - Layout operates on positioned nodes and graph edges already prepared for rendering.
 - Only dependency edges contribute spring attraction.
 */

public import Foundation
import CoreGraphics

public struct GraphLayoutEngine: Sendable {
    public struct Settings: Sendable, Equatable {
        public var repulsionForce: CGFloat
        public var attractionForce: CGFloat
        public var springLength: CGFloat
        public var damping: CGFloat
        public var minimumDistance: CGFloat
        public var centeringForce: CGFloat

        public init(
            repulsionForce: CGFloat = 5_000,
            attractionForce: CGFloat = 0.012,
            springLength: CGFloat = 160,
            damping: CGFloat = 0.78,
            minimumDistance: CGFloat = 18,
            centeringForce: CGFloat = 0.002
        ) {
            self.repulsionForce = repulsionForce
            self.attractionForce = attractionForce
            self.springLength = springLength
            self.damping = damping
            self.minimumDistance = minimumDistance
            self.centeringForce = centeringForce
        }
    }

    public let settings: Settings

    public init(settings: Settings = Settings()) {
        self.settings = settings
    }

    public func runLayout(
        nodes: [PositionedNode],
        edges: [GraphEdge],
        iterations: Int
    ) -> [PositionedNode] {
        guard iterations > 0 else { return nodes }

        var currentNodes = nodes
        for _ in 0..<iterations {
            currentNodes = runStep(nodes: currentNodes, edges: edges)
        }
        return currentNodes
    }

    public func runStep(nodes: [PositionedNode], edges: [GraphEdge]) -> [PositionedNode] {
        guard nodes.count > 1 else { return nodes }

        var forces: [String: CGVector] = Dictionary(
            uniqueKeysWithValues: nodes.map { ($0.id, .zero) }
        )
        let nodesByID = Dictionary(uniqueKeysWithValues: nodes.map { ($0.id, $0) })

        for index in nodes.indices {
            for otherIndex in nodes.indices where otherIndex > index {
                let nodeA = nodes[index]
                let nodeB = nodes[otherIndex]
                let dx = nodeA.position.x - nodeB.position.x
                let dy = nodeA.position.y - nodeB.position.y
                let distance = max(sqrt(dx * dx + dy * dy), settings.minimumDistance)
                let force = settings.repulsionForce / (distance * distance)
                let fx = (dx / distance) * force
                let fy = (dy / distance) * force

                forces[nodeA.id, default: .zero].dx += fx
                forces[nodeA.id, default: .zero].dy += fy
                forces[nodeB.id, default: .zero].dx -= fx
                forces[nodeB.id, default: .zero].dy -= fy
            }
        }

        for edge in edges where edge.type == .dependency {
            guard let fromNode = nodesByID[edge.from], let toNode = nodesByID[edge.to] else { continue }
            let dx = toNode.position.x - fromNode.position.x
            let dy = toNode.position.y - fromNode.position.y
            let distance = max(sqrt(dx * dx + dy * dy), settings.minimumDistance)
            let force = (distance - settings.springLength) * settings.attractionForce
            let fx = (dx / distance) * force
            let fy = (dy / distance) * force

            forces[fromNode.id, default: .zero].dx += fx
            forces[fromNode.id, default: .zero].dy += fy
            forces[toNode.id, default: .zero].dx -= fx
            forces[toNode.id, default: .zero].dy -= fy
        }

        return nodes.map { node in
            var updated = node
            let force = forces[node.id, default: .zero]
            updated.position.x += (force.dx - (node.position.x * settings.centeringForce)) * settings.damping
            updated.position.y += (force.dy - (node.position.y * settings.centeringForce)) * settings.damping
            return updated
        }
    }
}
