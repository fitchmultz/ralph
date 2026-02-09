/**
 DependencyGraphView

 Responsibilities:
 - Display task dependency graph using SwiftUI Canvas.
 - Implement force-directed layout algorithm for node positioning.
 - Handle pan and zoom gestures for navigation.
 - Draw edges with different styles for dependency types.
 - Highlight critical path and show cycle warnings.

 Does not handle:
 - Graph data fetching (delegates to Workspace).
 - Task editing (navigates to TaskDetailView via selection).

 Invariants/assumptions:
 - Graph data is loaded via workspace.loadGraphData().
 - Node positions persist during view lifetime but reset on reload.
 - Canvas coordinate system: origin at center, y increases downward.
 */

import SwiftUI
import RalphCore

struct DependencyGraphView: View {
    @ObservedObject var workspace: Workspace
    @Binding var selectedTaskID: String?
    @Environment(\.accessibilityVoiceOverEnabled) private var voiceOverEnabled
    
    // MARK: - Layout State
    @State private var nodes: [PositionedNode] = []
    @State private var edges: [GraphEdge] = []
    @State private var scale: CGFloat = 1.0
    @State private var offset: CGSize = .zero
    @State private var isDraggingCanvas = false
    @State private var lastDragLocation: CGPoint?
    @State private var simulationRunning = false
    @State private var cycleResult: CycleDetectionResult = .noCycles
    @State private var edgesInCycles: Set<GraphEdge.ID> = []
    @State private var pulsePhase: TimeInterval = 0
    
    // MARK: - Constants
    private let nodeWidth: CGFloat = 140
    private let nodeHeight: CGFloat = 60
    private let repulsionForce: CGFloat = 5000
    private let attractionForce: CGFloat = 0.01
    private let springLength: CGFloat = 150
    private let damping: CGFloat = 0.8
    
    var body: some View {
        GeometryReader { geometry in
            Group {
                if voiceOverEnabled {
                    accessibleListView()
                } else {
                    canvasGraphView(in: geometry)
                }
            }
        }
        .accessibilityLabel("Task dependency graph")
        .accessibilityHint(voiceOverEnabled ? 
            "Showing list view of task relationships" : 
            "Visual graph showing task dependencies. Enable VoiceOver for list view.")
        .task {
            await workspace.loadGraphData()
            initializeGraph()
        }
        .onChange(of: workspace.graphData) { _, _ in
            initializeGraph()
        }
        .overlay {
            if workspace.graphDataLoading {
                ProgressView("Loading graph...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(.ultraThinMaterial)
            }
        }
        .alert("Graph Error", isPresented: .constant(workspace.graphDataErrorMessage != nil)) {
            Button("OK") { workspace.graphDataErrorMessage = nil }
        } message: {
            Text(workspace.graphDataErrorMessage ?? "")
        }
        .onReceive(Timer.publish(every: 0.05, on: .main, in: .common).autoconnect()) { _ in
            // Update pulse phase for cycle edge animation
            if !edgesInCycles.isEmpty {
                pulsePhase = Date().timeIntervalSince1970
            }
        }
    }
    
    @ViewBuilder
    private func canvasGraphView(in geometry: GeometryProxy) -> some View {
        ZStack {
            // Background
            Color.clear
            
            // Graph Canvas
            Canvas { context, size in
                drawGraph(in: &context, size: size)
            }
            .gesture(
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
            )
            .gesture(
                MagnificationGesture()
                    .onChanged { value in
                        scale = min(max(scale * value, 0.3), 3.0)
                    }
            )
            .onTapGesture { location in
                handleCanvasTap(at: location, in: geometry)
            }
            
            // Cycle Warning Banner
            if cycleResult.hasCycle {
                cycleWarningBanner()
                    .position(x: geometry.size.width / 2, y: 40)
            }
            
            // Overlay Controls
            VStack {
                HStack {
                    Spacer()
                    zoomControls()
                }
                Spacer()
                legendView()
            }
            .padding()
        }
    }
    
    @ViewBuilder
    private func accessibleListView() -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Task Relationships")
                    .font(.headline)
                    .padding(.horizontal)
                
                // Accessible cycle warning
                if cycleResult.hasCycle {
                    VStack(alignment: .leading, spacing: 8) {
                        Label("Warning: Circular Dependencies Detected", systemImage: "exclamationmark.triangle.fill")
                            .font(.subheadline.bold())
                            .foregroundStyle(.red)
                        
                        ForEach(Array(cycleResult.cycles.prefix(3).enumerated()), id: \.offset) { _, cycle in
                            if cycle.count == 1 {
                                Text("Self-loop: \(cycle[0])")
                                    .font(.caption)
                                    .monospaced()
                            } else {
                                Text(cycle.joined(separator: " → ") + " → " + cycle[0])
                                    .font(.caption)
                                    .monospaced()
                            }
                        }
                    }
                    .padding()
                    .background(Color.red.opacity(0.1))
                    .cornerRadius(8)
                    .padding(.horizontal)
                    .accessibilityLabel("Warning: Circular dependencies detected in the graph")
                }
                
                if nodes.isEmpty {
                    Text("No tasks to display")
                        .foregroundStyle(.secondary)
                        .padding(.horizontal)
                } else {
                    ForEach(nodes) { node in
                        accessibleTaskCard(for: node)
                    }
                }
            }
            .padding(.vertical)
        }
    }
    
    @ViewBuilder
    private func accessibleTaskCard(for node: PositionedNode) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Image(systemName: statusIcon(node.task.statusEnum))
                    .foregroundStyle(statusColor(node.task.statusEnum))
                    .accessibilityLabel("Status: \(node.task.statusEnum?.displayName ?? "Unknown")")
                
                Text(node.id)
                    .font(.caption)
                    .monospaced()
                
                Spacer()
                
                if node.task.isCritical {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(.red)
                        .accessibilityLabel("Critical path task")
                }
            }
            
            Text(node.task.title)
                .font(.headline)
            
            // Dependencies
            if let deps = taskDependencies(for: node), !deps.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Depends on:")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    ForEach(deps, id: \.self) { dep in
                        Text("• \(dep)")
                            .font(.caption)
                    }
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel("Depends on: \(deps.joined(separator: ", "))")
            }
            
            // Blocked tasks
            if let blocked = taskBlocked(for: node), !blocked.isEmpty {
                Text("Blocks: \(blocked.joined(separator: ", "))")
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .accessibilityLabel("Blocks: \(blocked.joined(separator: ", "))")
            }
            
            // Related tasks
            if let related = taskRelated(for: node), !related.isEmpty {
                Text("Related: \(related.joined(separator: ", "))")
                    .font(.caption)
                    .foregroundStyle(.blue)
                    .accessibilityLabel("Related to: \(related.joined(separator: ", "))")
            }
            
            Button("Select Task") {
                selectedTaskID = node.id
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Select \(node.id)")
        }
        .padding()
        .background(Color(NSColor.controlBackgroundColor))
        .cornerRadius(10)
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(selectedTaskID == node.id ? Color.accentColor : Color.clear, lineWidth: 2)
        )
        .padding(.horizontal)
    }
    
    // MARK: - Initialization
    
    private func initializeGraph() {
        guard let graphData = workspace.graphData else { return }
        
        // Create nodes
        nodes = graphData.tasks.map { task in
            PositionedNode(
                id: task.id,
                position: CGPoint(
                    x: CGFloat.random(in: -200...200),
                    y: CGFloat.random(in: -200...200)
                ),
                task: task,
                isSelected: task.id == selectedTaskID
            )
        }
        
        // Create edges from relationships
        edges = []
        for task in graphData.tasks {
            // Dependencies (depends_on) - stored on the dependent task, pointing to dependencies
            for depId in task.dependencies {
                edges.append(GraphEdge(from: task.id, to: depId, type: .dependency))
            }
        }
        
        // Add blocks and relates_to relationships from workspace tasks
        for taskNode in graphData.tasks {
            if let fullTask = workspace.tasks.first(where: { $0.id == taskNode.id }) {
                // Blocks relationships
                if let blocks = fullTask.blocks {
                    for blockedId in blocks {
                        edges.append(GraphEdge(from: taskNode.id, to: blockedId, type: .blocks))
                    }
                }
                // Relates_to relationships - only add one direction to avoid duplicates
                if let relatesTo = fullTask.relatesTo {
                    for relatedId in relatesTo where taskNode.id < relatedId {
                        edges.append(GraphEdge(from: taskNode.id, to: relatedId, type: .relatesTo))
                    }
                }
            }
        }
        
        // Detect cycles after edge creation
        cycleResult = GraphAlgorithms.detectCycles(edges: edges)
        edgesInCycles = GraphAlgorithms.edgesInCycles(edges: edges)
        
        // Start force-directed simulation
        startSimulation()
    }
    
    // MARK: - Force-Directed Layout
    
    private func startSimulation() {
        guard !simulationRunning else { return }
        simulationRunning = true
        
        Task {
            for _ in 0..<100 {
                runSimulationStep()
                try? await Task.sleep(nanoseconds: 16_000_000) // ~60fps
            }
            simulationRunning = false
        }
    }
    
    @MainActor
    private func runSimulationStep() {
        var forces: [String: CGVector] = [:]
        
        // Initialize forces
        for node in nodes {
            forces[node.id] = CGVector(dx: 0, dy: 0)
        }
        
        // Repulsion between all nodes
        for i in 0..<nodes.count {
            for j in (i+1)..<nodes.count {
                let nodeA = nodes[i]
                let nodeB = nodes[j]
                let dx = nodeA.position.x - nodeB.position.x
                let dy = nodeA.position.y - nodeB.position.y
                let distance = sqrt(dx*dx + dy*dy)
                
                if distance > 0 {
                    let force = repulsionForce / (distance * distance)
                    let fx = (dx / distance) * force
                    let fy = (dy / distance) * force
                    
                    forces[nodeA.id, default: CGVector.zero].dx += fx
                    forces[nodeA.id, default: CGVector.zero].dy += fy
                    forces[nodeB.id, default: CGVector.zero].dx -= fx
                    forces[nodeB.id, default: CGVector.zero].dy -= fy
                }
            }
        }
        
        // Attraction along edges
        for edge in edges where edge.type == .dependency {
            guard let fromIndex = nodes.firstIndex(where: { $0.id == edge.from }),
                  let toIndex = nodes.firstIndex(where: { $0.id == edge.to }) else { continue }
            
            let fromNode = nodes[fromIndex]
            let toNode = nodes[toIndex]
            let dx = toNode.position.x - fromNode.position.x
            let dy = toNode.position.y - fromNode.position.y
            let distance = sqrt(dx*dx + dy*dy)
            
            if distance > 0 {
                let force = (distance - springLength) * attractionForce
                let fx = (dx / distance) * force
                let fy = (dy / distance) * force
                
                forces[fromNode.id, default: CGVector.zero].dx += fx
                forces[fromNode.id, default: CGVector.zero].dy += fy
                forces[toNode.id, default: CGVector.zero].dx -= fx
                forces[toNode.id, default: CGVector.zero].dy -= fy
            }
        }
        
        // Apply forces with damping
        for i in 0..<nodes.count {
            var node = nodes[i]
            if let force = forces[node.id] {
                node.position.x += force.dx * damping
                node.position.y += force.dy * damping
            }
            nodes[i] = node
        }
    }
    
    // MARK: - Drawing
    
    private func drawGraph(in context: inout GraphicsContext, size: CGSize) {
        let center = CGPoint(x: size.width / 2 + offset.width, y: size.height / 2 + offset.height)
        
        // Draw edges
        for edge in edges {
            drawEdge(edge, in: &context, center: center)
        }
        
        // Draw nodes
        for node in nodes {
            drawNode(node, in: &context, center: center)
        }
    }
    
    private func drawEdge(_ edge: GraphEdge, in context: inout GraphicsContext, center: CGPoint) {
        guard let fromNode = nodes.first(where: { $0.id == edge.from }),
              let toNode = nodes.first(where: { $0.id == edge.to }) else { return }
        
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
        
        // Check if this edge is part of a cycle
        let isInCycle = edgesInCycles.contains(edge.id)
        
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
        
        // Apply pulsing animation for cycle edges
        if isInCycle {
            let pulseOpacity = 0.5 + 0.5 * sin(pulsePhase * 4)
            context.stroke(path, with: .color(color.opacity(pulseOpacity)), style: strokeStyle)
        } else {
            context.stroke(path, with: .color(color), style: strokeStyle)
        }
        
        // Draw arrow head for dependencies
        if edge.type == .dependency {
            let arrowColor = isInCycle ? Color.red.opacity(0.5 + 0.5 * sin(pulsePhase * 4)) : color
            drawArrowHead(from: fromPoint, to: toPoint, in: &context, color: arrowColor)
        }
    }
    
    private func drawArrowHead(from: CGPoint, to: CGPoint, in context: inout GraphicsContext, color: Color) {
        let arrowLength: CGFloat = 10 * scale
        let arrowAngle: CGFloat = .pi / 6
        
        let angle = atan2(to.y - from.y, to.x - from.x)
        let tipX = to.x - cos(angle) * (nodeWidth/2 * scale)
        let tipY = to.y - sin(angle) * (nodeHeight/2 * scale)
        
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
            x: center.x + node.position.x * scale - nodeWidth * scale / 2,
            y: center.y + node.position.y * scale - nodeHeight * scale / 2,
            width: nodeWidth * scale,
            height: nodeHeight * scale
        )
        
        // Background
        let backgroundColor = node.isSelected ? Color.accentColor : Color(NSColor.controlBackgroundColor)
        let borderColor = node.task.isCritical ? Color.red : (node.isSelected ? Color.accentColor : Color.gray.opacity(0.3))
        let borderWidth: CGFloat = node.task.isCritical ? 3 : (node.isSelected ? 2 : 1)
        
        let rectPath = Path(roundedRect: rect, cornerRadius: 8 * scale)
        context.fill(rectPath, with: .color(backgroundColor))
        context.stroke(rectPath, with: .color(borderColor), lineWidth: borderWidth)
        
        // Status indicator dot
        let dotRect = CGRect(
            x: rect.minX + 8 * scale,
            y: rect.minY + 8 * scale,
            width: 8 * scale,
            height: 8 * scale
        )
        let dotPath = Path(ellipseIn: dotRect)
        context.fill(dotPath, with: .color(statusColor(node.task.statusEnum)))
        
        // Task ID (top right)
        let idText = context.resolve(Text(node.id).font(.system(size: 9 * scale)).monospaced())
        let idSize = idText.measure(in: rect.size)
        context.draw(idText, at: CGPoint(
            x: rect.maxX - idSize.width / 2 - 8 * scale,
            y: rect.minY + idSize.height / 2 + 4 * scale
        ))
        
        // Title (center)
        let title = node.task.title.count > 25 
            ? String(node.task.title.prefix(25)) + "..."
            : node.task.title
        let titleText = context.resolve(Text(title).font(.system(size: 11 * scale)))
        context.draw(titleText, at: CGPoint(
            x: center.x + node.position.x * scale,
            y: center.y + node.position.y * scale + 4 * scale
        ))
    }
    
    // MARK: - Interaction
    
    private func handleCanvasTap(at location: CGPoint, in geometry: GeometryProxy) {
        let center = CGPoint(
            x: geometry.size.width / 2 + offset.width,
            y: geometry.size.height / 2 + offset.height
        )
        
        // Find tapped node
        if let tappedNode = nodes.first(where: { node in
            let nodeRect = CGRect(
                x: center.x + node.position.x * scale - nodeWidth * scale / 2,
                y: center.y + node.position.y * scale - nodeHeight * scale / 2,
                width: nodeWidth * scale,
                height: nodeHeight * scale
            )
            return nodeRect.contains(location)
        }) {
            selectedTaskID = tappedNode.id
            updateNodeSelection()
        } else {
            selectedTaskID = nil
            updateNodeSelection()
        }
    }
    
    private func updateNodeSelection() {
        for i in 0..<nodes.count {
            nodes[i].isSelected = nodes[i].id == selectedTaskID
        }
    }
    
    // MARK: - Helper Views
    
    private func zoomControls() -> some View {
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
    
    private func legendView() -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Label("Dependency", systemImage: "arrow.right")
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityLabel("Dependency relationship")
            Label("Blocks", systemImage: "line.diagonal")
                .font(.caption)
                .foregroundStyle(.orange)
                .accessibilityLabel("Blocks relationship")
            Label("Relates To", systemImage: "line.diagonal")
                .font(.caption)
                .foregroundStyle(.blue.opacity(0.5))
                .accessibilityLabel("Relates to relationship")
            Label("Critical Path", systemImage: "exclamationmark.triangle")
                .font(.caption)
                .foregroundStyle(.red)
                .accessibilityLabel("Critical path indicator")
            if cycleResult.hasCycle {
                Label("Cycle Edge", systemImage: "exclamationmark.circle")
                    .font(.caption)
                    .foregroundStyle(.red)
                    .accessibilityLabel("Cycle edge indicator")
            }
        }
        .padding(8)
        .background(.ultraThinMaterial)
        .cornerRadius(8)
    }
    
    // MARK: - Helpers
    
    // MARK: - Cycle Warning
    
    @ViewBuilder
    private func cycleWarningBanner() -> some View {
        VStack(spacing: 4) {
            HStack {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.white)
                Text("Circular Dependencies Detected")
                    .font(.subheadline.bold())
                    .foregroundStyle(.white)
            }
            
            // Show each detected cycle (limit to first 3 to avoid overwhelming UI)
            ForEach(Array(cycleResult.cycles.prefix(3).enumerated()), id: \.offset) { _, cycle in
                if cycle.count == 1 {
                    // Self-loop
                    Text("\(cycle[0]) → \(cycle[0])")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(0.9))
                        .monospaced()
                } else {
                    // Multi-node cycle
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
    
    private func statusColor(_ status: RalphTaskStatus?) -> Color {
        guard let status = status else { return .gray }
        switch status {
        case .draft: return .gray
        case .todo: return .blue
        case .doing: return .orange
        case .done: return .green
        case .rejected: return .red
        }
    }
    
    private func taskDependencies(for node: PositionedNode) -> [String]? {
        let deps = edges.filter { $0.from == node.id && $0.type == .dependency }
            .map { $0.to }
        return deps.isEmpty ? nil : deps
    }
    
    private func taskBlocked(for node: PositionedNode) -> [String]? {
        let blocked = edges.filter { $0.from == node.id && $0.type == .blocks }
            .map { $0.to }
        return blocked.isEmpty ? nil : blocked
    }
    
    private func taskRelated(for node: PositionedNode) -> [String]? {
        let related = edges.filter { ($0.from == node.id || $0.to == node.id) && $0.type == .relatesTo }
            .map { $0.from == node.id ? $0.to : $0.from }
        return related.isEmpty ? nil : related
    }
    
    private func statusIcon(_ status: RalphTaskStatus?) -> String {
        guard let status = status else { return "circle" }
        switch status {
        case .draft: return "pencil.circle"
        case .todo: return "circle"
        case .doing: return "arrow.triangle.2.circlepath"
        case .done: return "checkmark.circle.fill"
        case .rejected: return "xmark.circle"
        }
    }
}

#Preview {
    struct PreviewWrapper: View {
        @State private var selectedTaskID: String?
        
        var body: some View {
            DependencyGraphView(
                workspace: previewWorkspace(),
                selectedTaskID: $selectedTaskID
            )
        }
        
        func previewWorkspace() -> Workspace {
            let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
            return workspace
        }
    }
    
    return PreviewWrapper()
}
