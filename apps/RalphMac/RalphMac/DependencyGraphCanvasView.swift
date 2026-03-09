/**
 DependencyGraphCanvasView

 Responsibilities:
 - Render the visual dependency graph canvas, overlays, and viewport controls.
 - Manage canvas-local pan and zoom state for graph navigation.

 Does not handle:
 - Graph layout simulation or presentation assembly.
 - Accessibility list rendering.

 Invariants/assumptions:
 - Node positions are supplied by `DependencyGraphViewModel`.
 - Canvas coordinates are centered within the available geometry.
 */

import SwiftUI
import RalphCore

private enum DependencyGraphMetrics {
    static let nodeWidth: CGFloat = 140
    static let nodeHeight: CGFloat = 60
}

@MainActor
struct DependencyGraphCanvasView: View {
    @ObservedObject var viewModel: DependencyGraphViewModel
    @Binding var selectedTaskID: String?

    @State private var scale: CGFloat = 1.0
    @State private var offset: CGSize = .zero
    @State private var lastDragLocation: CGPoint?

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                Color.clear

                TimelineView(.animation) { timeline in
                    Canvas { context, size in
                        drawGraph(
                            in: &context,
                            size: size,
                            pulsePhase: timeline.date.timeIntervalSinceReferenceDate
                        )
                    }
                }
                .gesture(canvasDragGesture)
                .gesture(canvasMagnificationGesture)
                .onTapGesture { location in
                    selectedTaskID = viewModel.selectNode(
                        at: location,
                        canvasSize: geometry.size,
                        scale: scale,
                        offset: offset,
                        nodeSize: CGSize(
                            width: DependencyGraphMetrics.nodeWidth,
                            height: DependencyGraphMetrics.nodeHeight
                        )
                    )
                }

                if viewModel.cycleResult.hasCycle {
                    DependencyGraphCycleBanner(cycleResult: viewModel.cycleResult)
                        .position(x: geometry.size.width / 2, y: 40)
                }

                VStack {
                    HStack {
                        Spacer()
                        zoomControls
                    }
                    Spacer()
                    DependencyGraphLegendView(hasCycle: viewModel.cycleResult.hasCycle)
                }
                .padding()
            }
        }
    }

    private var canvasDragGesture: some Gesture {
        DragGesture()
            .onChanged { value in
                if let last = lastDragLocation {
                    let delta = CGSize(
                        width: value.location.x - last.x,
                        height: value.location.y - last.y
                    )
                    offset.width += delta.width
                    offset.height += delta.height
                }
                lastDragLocation = value.location
            }
            .onEnded { _ in
                lastDragLocation = nil
            }
    }

    private var canvasMagnificationGesture: some Gesture {
        MagnificationGesture()
            .onChanged { value in
                scale = min(max(scale * value, 0.3), 3.0)
            }
    }

    private var zoomControls: some View {
        VStack(spacing: 8) {
            Button(action: { scale = min(scale * 1.2, 3.0) }) {
                Image(systemName: "plus.magnifyingglass")
            }
            .buttonStyle(.borderedProminent)
            .accessibilityLabel("Zoom in")

            Button(action: { scale = 1.0; offset = .zero }) {
                Image(systemName: "arrow.counterclockwise")
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Reset zoom")

            Button(action: { scale = max(scale / 1.2, 0.3) }) {
                Image(systemName: "minus.magnifyingglass")
            }
            .buttonStyle(.borderedProminent)
            .accessibilityLabel("Zoom out")
        }
    }

    private func drawGraph(in context: inout GraphicsContext, size: CGSize, pulsePhase: TimeInterval) {
        let center = CGPoint(x: size.width / 2 + offset.width, y: size.height / 2 + offset.height)

        for edge in viewModel.edges {
            drawEdge(edge, in: &context, center: center, pulsePhase: pulsePhase)
        }

        for node in viewModel.nodes {
            drawNode(node, in: &context, center: center)
        }
    }

    private func drawEdge(
        _ edge: GraphEdge,
        in context: inout GraphicsContext,
        center: CGPoint,
        pulsePhase: TimeInterval
    ) {
        guard let fromNode = viewModel.nodes.first(where: { $0.id == edge.from }),
              let toNode = viewModel.nodes.first(where: { $0.id == edge.to }) else { return }

        let fromPoint = CGPoint(
            x: center.x + fromNode.position.x * scale,
            y: center.y + fromNode.position.y * scale
        )
        let toPoint = CGPoint(
            x: center.x + toNode.position.x * scale,
            y: center.y + toNode.position.y * scale
        )

        var path = Path()
        path.move(to: fromPoint)
        path.addLine(to: toPoint)

        let isInCycle = viewModel.edgesInCycles.contains(edge.id)
        var strokeStyle = StrokeStyle(lineWidth: 2 * scale)
        let color: Color

        switch edge.type {
        case .dependency:
            if isInCycle {
                color = .red
                strokeStyle = StrokeStyle(lineWidth: 3 * scale)
            } else {
                color = fromNode.task.isCritical && toNode.task.isCritical ? .red : .gray
            }
        case .blocks:
            if isInCycle {
                color = .red
                strokeStyle = StrokeStyle(lineWidth: 3 * scale, dash: [5, 5])
            } else {
                color = .orange
                strokeStyle = StrokeStyle(lineWidth: 2 * scale, dash: [5, 5])
            }
        case .relatesTo:
            color = .blue.opacity(0.5)
            strokeStyle = StrokeStyle(lineWidth: 1 * scale, dash: [3, 3])
        }

        if isInCycle {
            let opacity = 0.5 + 0.5 * sin(pulsePhase * 4)
            context.stroke(path, with: .color(color.opacity(opacity)), style: strokeStyle)
        } else {
            context.stroke(path, with: .color(color), style: strokeStyle)
        }

        if edge.type == .dependency {
            let arrowColor = isInCycle ? Color.red.opacity(0.5 + 0.5 * sin(pulsePhase * 4)) : color
            drawArrowHead(from: fromPoint, to: toPoint, in: &context, color: arrowColor)
        }
    }

    private func drawArrowHead(from: CGPoint, to: CGPoint, in context: inout GraphicsContext, color: Color) {
        let arrowLength: CGFloat = 10 * scale
        let arrowAngle: CGFloat = .pi / 6

        let angle = atan2(to.y - from.y, to.x - from.x)
        let tipX = to.x - cos(angle) * (DependencyGraphMetrics.nodeWidth / 2 * scale)
        let tipY = to.y - sin(angle) * (DependencyGraphMetrics.nodeHeight / 2 * scale)

        var path = Path()
        path.move(to: CGPoint(x: tipX, y: tipY))
        path.addLine(to: CGPoint(
            x: tipX - arrowLength * cos(angle - arrowAngle),
            y: tipY - arrowLength * sin(angle - arrowAngle)
        ))
        path.move(to: CGPoint(x: tipX, y: tipY))
        path.addLine(to: CGPoint(
            x: tipX - arrowLength * cos(angle + arrowAngle),
            y: tipY - arrowLength * sin(angle + arrowAngle)
        ))

        context.stroke(path, with: .color(color), lineWidth: 2 * scale)
    }

    private func drawNode(_ node: PositionedNode, in context: inout GraphicsContext, center: CGPoint) {
        let rect = CGRect(
            x: center.x + node.position.x * scale - DependencyGraphMetrics.nodeWidth * scale / 2,
            y: center.y + node.position.y * scale - DependencyGraphMetrics.nodeHeight * scale / 2,
            width: DependencyGraphMetrics.nodeWidth * scale,
            height: DependencyGraphMetrics.nodeHeight * scale
        )

        let backgroundColor = node.isSelected ? Color.accentColor : Color(NSColor.controlBackgroundColor)
        let borderColor = node.task.isCritical ? Color.red : (node.isSelected ? Color.accentColor : Color.gray.opacity(0.3))
        let borderWidth: CGFloat = node.task.isCritical ? 3 : (node.isSelected ? 2 : 1)

        let rectPath = Path(roundedRect: rect, cornerRadius: 8 * scale)
        context.fill(rectPath, with: .color(backgroundColor))
        context.stroke(rectPath, with: .color(borderColor), lineWidth: borderWidth)

        let dotRect = CGRect(
            x: rect.minX + 8 * scale,
            y: rect.minY + 8 * scale,
            width: 8 * scale,
            height: 8 * scale
        )
        context.fill(Path(ellipseIn: dotRect), with: .color(statusColor(node.task.statusEnum)))

        let idText = context.resolve(Text(node.id).font(.system(size: 9 * scale)).monospaced())
        let idSize = idText.measure(in: rect.size)
        context.draw(idText, at: CGPoint(
            x: rect.maxX - idSize.width / 2 - 8 * scale,
            y: rect.minY + idSize.height / 2 + 4 * scale
        ))

        let title = node.task.title.count > 25
            ? String(node.task.title.prefix(25)) + "..."
            : node.task.title
        let titleText = context.resolve(Text(title).font(.system(size: 11 * scale)))
        context.draw(titleText, at: CGPoint(
            x: center.x + node.position.x * scale,
            y: center.y + node.position.y * scale + 4 * scale
        ))
    }

    private func statusColor(_ status: RalphTaskStatus?) -> Color {
        guard let status else { return .gray }
        switch status {
        case .draft: return .gray
        case .todo: return .blue
        case .doing: return .orange
        case .done: return .green
        case .rejected: return .red
        }
    }
}

private struct DependencyGraphLegendView: View {
    let hasCycle: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Label("Dependency", systemImage: "arrow.right")
                .font(.caption)
                .foregroundStyle(.secondary)
            Label("Blocks", systemImage: "line.diagonal")
                .font(.caption)
                .foregroundStyle(.orange)
            Label("Relates To", systemImage: "line.diagonal")
                .font(.caption)
                .foregroundStyle(.blue.opacity(0.5))
            Label("Critical Path", systemImage: "exclamationmark.triangle")
                .font(.caption)
                .foregroundStyle(.red)
            if hasCycle {
                Label("Cycle Edge", systemImage: "exclamationmark.circle")
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
        .padding(8)
        .background(.ultraThinMaterial)
        .cornerRadius(8)
    }
}

private struct DependencyGraphCycleBanner: View {
    let cycleResult: CycleDetectionResult

    var body: some View {
        VStack(spacing: 4) {
            HStack {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.white)
                Text("Circular Dependencies Detected")
                    .font(.subheadline.bold())
                    .foregroundStyle(.white)
            }

            ForEach(Array(cycleResult.cycles.prefix(3).enumerated()), id: \.offset) { _, cycle in
                if cycle.count == 1 {
                    Text("\(cycle[0]) → \(cycle[0])")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.9))
                        .monospaced()
                } else {
                    Text(cycle.joined(separator: " → ") + " → " + cycle[0])
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.9))
                        .monospaced()
                }
            }

            if cycleResult.cycles.count > 3 {
                Text("... and \(cycleResult.cycles.count - 3) more")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(0.7))
            }
        }
        .padding()
        .background(Color.red.opacity(0.9))
        .cornerRadius(8)
        .shadow(radius: 4)
        .accessibilityLabel("Warning: Circular dependencies detected")
        .accessibilityHint("The dependency graph contains cycles that should be resolved")
    }
}
