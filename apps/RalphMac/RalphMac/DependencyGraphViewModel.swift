/**
 DependencyGraphViewModel

 Responsibilities:
 - Own dependency-graph presentation state, selection state, and async layout refinement.
 - Translate workspace graph payloads into rendered nodes/edges via RalphCore services.

 Does not handle:
 - SwiftUI canvas drawing or gesture plumbing.
 - Graph data loading from the CLI.

 Invariants/assumptions:
 - Graph layout work is cancelable and only the latest refresh should publish state.
 - Node selection is represented directly on `PositionedNode.isSelected`.
 */

import SwiftUI
import RalphCore

@MainActor
final class DependencyGraphViewModel: ObservableObject {
    @Published private(set) var nodes: [PositionedNode] = []
    @Published private(set) var edges: [GraphEdge] = []
    @Published private(set) var cycleResult: CycleDetectionResult = .noCycles
    @Published private(set) var edgesInCycles: Set<GraphEdge.ID> = []
    @Published private(set) var isLayoutInFlight = false

    private let layoutEngine = GraphLayoutEngine()
    private var layoutTask: Task<Void, Never>?
    private var selectedTaskID: String?

    deinit {
        layoutTask?.cancel()
    }

    func refresh(workspace: Workspace, selectedTaskID: String?) async {
        self.selectedTaskID = selectedTaskID
        layoutTask?.cancel()

        guard let graphData = workspace.graphData else {
            nodes = []
            edges = []
            cycleResult = .noCycles
            edgesInCycles = []
            isLayoutInFlight = false
            return
        }

        let presentation = RalphGraphPresentationBuilder.build(
            graphData: graphData,
            tasks: workspace.tasks,
            selectedTaskID: selectedTaskID
        )

        edges = presentation.edges
        cycleResult = presentation.cycleResult
        edgesInCycles = presentation.edgesInCycles
        nodes = presentation.nodes

        beginLayout(for: presentation.nodes, edges: presentation.edges, selectedTaskID: selectedTaskID)
    }

    func applySelection(taskID: String?) {
        selectedTaskID = taskID
        nodes = nodes.map { node in
            var updated = node
            updated.isSelected = node.id == taskID
            return updated
        }
    }

    func selectNode(
        at location: CGPoint,
        canvasSize: CGSize,
        scale: CGFloat,
        offset: CGSize,
        nodeSize: CGSize
    ) -> String? {
        let center = CGPoint(
            x: canvasSize.width / 2 + offset.width,
            y: canvasSize.height / 2 + offset.height
        )

        let tappedNode = nodes.first { node in
            let nodeRect = CGRect(
                x: center.x + node.position.x * scale - nodeSize.width * scale / 2,
                y: center.y + node.position.y * scale - nodeSize.height * scale / 2,
                width: nodeSize.width * scale,
                height: nodeSize.height * scale
            )
            return nodeRect.contains(location)
        }

        applySelection(taskID: tappedNode?.id)
        return tappedNode?.id
    }

    private func beginLayout(
        for initialNodes: [PositionedNode],
        edges: [GraphEdge],
        selectedTaskID: String?
    ) {
        guard initialNodes.count > 1 else {
            isLayoutInFlight = false
            return
        }

        isLayoutInFlight = true
        let engine = layoutEngine

        layoutTask = Task.detached(priority: .userInitiated) { [initialNodes, edges] in
            var currentNodes = initialNodes

            for step in 1...120 {
                if Task.isCancelled { return }
                currentNodes = engine.runStep(nodes: currentNodes, edges: edges)

                guard step.isMultiple(of: 20) || step == 120 else { continue }
                let snapshot = currentNodes
                await MainActor.run {
                    self.nodes = snapshot.map { node in
                        var updated = node
                        updated.isSelected = node.id == selectedTaskID
                        return updated
                    }
                }
            }

            await MainActor.run {
                self.isLayoutInFlight = false
            }
        }
    }
}
