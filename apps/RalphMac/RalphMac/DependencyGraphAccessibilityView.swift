/**
 DependencyGraphAccessibilityView

 Responsibilities:
 - Present dependency graph relationships as an accessibility-friendly list.
 - Surface cycle warnings and task relationship summaries without relying on canvas rendering.

 Does not handle:
 - Layout simulation or canvas navigation.
 - Graph data fetching.

 Invariants/assumptions:
 - Nodes and edges are already synchronized by the dependency graph view model.
 */

import SwiftUI
import RalphCore

@MainActor
struct DependencyGraphAccessibilityView: View {
    @ObservedObject var viewModel: DependencyGraphViewModel
    @Binding var selectedTaskID: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("Task Relationships")
                    .font(.headline)
                    .padding(.horizontal)

                if viewModel.cycleResult.hasCycle {
                    VStack(alignment: .leading, spacing: 8) {
                        Label("Warning: Circular Dependencies Detected", systemImage: "exclamationmark.triangle.fill")
                            .font(.subheadline.bold())
                            .foregroundStyle(.red)

                        ForEach(Array(viewModel.cycleResult.cycles.prefix(3).enumerated()), id: \.offset) { _, cycle in
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
                    .clipShape(.rect(cornerRadius: 8))
                    .padding(.horizontal)
                    .accessibilityLabel("Warning: Circular dependencies detected in the graph")
                }

                if viewModel.nodes.isEmpty {
                    Text("No tasks to display")
                        .foregroundStyle(.secondary)
                        .padding(.horizontal)
                } else {
                    ForEach(viewModel.nodes) { node in
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

            if let dependencies = taskDependencies(for: node), !dependencies.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Depends on:")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    ForEach(dependencies, id: \.self) { dependency in
                        Text("• \(dependency)")
                            .font(.caption)
                    }
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel("Depends on: \(dependencies.joined(separator: ", "))")
            }

            if let blocked = taskBlocked(for: node), !blocked.isEmpty {
                Text("Blocks: \(blocked.joined(separator: ", "))")
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .accessibilityLabel("Blocks: \(blocked.joined(separator: ", "))")
            }

            if let related = taskRelated(for: node), !related.isEmpty {
                Text("Related: \(related.joined(separator: ", "))")
                    .font(.caption)
                    .foregroundStyle(.blue)
                    .accessibilityLabel("Related to: \(related.joined(separator: ", "))")
            }

            Button("Select Task") {
                selectedTaskID = node.id
                viewModel.applySelection(taskID: node.id)
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("Select \(node.id)")
        }
        .padding()
        .background(Color(NSColor.controlBackgroundColor))
        .clipShape(.rect(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(selectedTaskID == node.id ? Color.accentColor : Color.clear, lineWidth: 2)
        )
        .padding(.horizontal)
    }

    private func taskDependencies(for node: PositionedNode) -> [String]? {
        let dependencies = viewModel.edges
            .filter { $0.from == node.id && $0.type == .dependency }
            .map(\.to)
        return dependencies.isEmpty ? nil : dependencies
    }

    private func taskBlocked(for node: PositionedNode) -> [String]? {
        let blocked = viewModel.edges
            .filter { $0.from == node.id && $0.type == .blocks }
            .map(\.to)
        return blocked.isEmpty ? nil : blocked
    }

    private func taskRelated(for node: PositionedNode) -> [String]? {
        let related = viewModel.edges
            .filter { ($0.from == node.id || $0.to == node.id) && $0.type == .relatesTo }
            .map { $0.from == node.id ? $0.to : $0.from }
        return related.isEmpty ? nil : related
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

    private func statusIcon(_ status: RalphTaskStatus?) -> String {
        guard let status else { return "circle" }
        switch status {
        case .draft: return "pencil.circle"
        case .todo: return "circle"
        case .doing: return "arrow.triangle.2.circlepath"
        case .done: return "checkmark.circle.fill"
        case .rejected: return "xmark.circle"
        }
    }
}
