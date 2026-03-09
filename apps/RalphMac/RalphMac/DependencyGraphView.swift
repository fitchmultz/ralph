/**
 DependencyGraphView

 Responsibilities:
 - Host the task dependency graph experience and choose between visual and accessibility-focused presentations.
 - Bridge workspace graph loading to the dedicated dependency-graph view model.

 Does not handle:
 - Graph layout simulation or presentation building.
 - Canvas drawing details.

 Invariants/assumptions:
 - Graph data is loaded through `Workspace.loadGraphData()`.
 - Visual rendering and accessibility rendering are delegated to dedicated subviews.
 */

import SwiftUI
import RalphCore

@MainActor
struct DependencyGraphView: View {
    @ObservedObject var workspace: Workspace
    @Binding var selectedTaskID: String?
    @Environment(\.accessibilityVoiceOverEnabled) private var voiceOverEnabled
    @StateObject private var viewModel = DependencyGraphViewModel()

    var body: some View {
        Group {
            if voiceOverEnabled {
                DependencyGraphAccessibilityView(
                    viewModel: viewModel,
                    selectedTaskID: $selectedTaskID
                )
            } else {
                DependencyGraphCanvasView(
                    viewModel: viewModel,
                    selectedTaskID: $selectedTaskID
                )
            }
        }
        .accessibilityLabel("Task dependency graph")
        .accessibilityHint(
            voiceOverEnabled
                ? "Showing list view of task relationships"
                : "Visual graph showing task dependencies. Enable VoiceOver for list view."
        )
        .task { @MainActor in
            await workspace.loadGraphData()
            await viewModel.refresh(workspace: workspace, selectedTaskID: selectedTaskID)
        }
        .task(id: graphRefreshKey) {
            await viewModel.refresh(workspace: workspace, selectedTaskID: selectedTaskID)
        }
        .onChange(of: selectedTaskID) { _, newValue in
            viewModel.applySelection(taskID: newValue)
        }
        .overlay {
            if workspace.insightsState.graphDataLoading {
                ProgressView("Loading graph...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(.ultraThinMaterial)
            } else if viewModel.isLayoutInFlight {
                VStack {
                    Spacer()
                    ProgressView("Refining layout...")
                        .padding(.horizontal, 14)
                        .padding(.vertical, 10)
                        .background(.ultraThinMaterial)
                        .clipShape(.rect(cornerRadius: 10))
                        .padding()
                }
            }
        }
        .alert("Graph Error", isPresented: .constant(workspace.insightsState.graphDataErrorMessage != nil)) {
            Button("OK") { workspace.insightsState.graphDataErrorMessage = nil }
        } message: {
            Text(workspace.insightsState.graphDataErrorMessage ?? "")
        }
    }

    private var graphRefreshKey: String {
        let graphTasks = workspace.insightsState.graphData?.tasks.map(\.id).joined(separator: "|") ?? "no-graph"
        let taskVersions = workspace.taskState.tasks.map {
            "\($0.id):\($0.updatedAt?.timeIntervalSince1970 ?? 0)"
        }.joined(separator: "|")
        return "\(graphTasks)#\(taskVersions)#\(selectedTaskID ?? "none")"
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
            PreviewWorkspaceSupport.makeWorkspace(label: "dependency-graph")
        }
    }

    return PreviewWrapper()
}
