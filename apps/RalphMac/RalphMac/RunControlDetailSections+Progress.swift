/**
 RunControlDetailSections+Progress

 Purpose:
 - Render Run Control phase progress and next-task targeting cards.

 Responsibilities:
 - Render Run Control phase progress and next-task targeting cards.
 - Keep queue-preview selection UI isolated from safety/history/configuration sections.

 Does not handle:
 - Execution-control button actions.
 - History or safety-card rendering.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

@MainActor
struct RunControlPhaseProgressSection: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        RunControlGlassSection("Phase Progress") {
            VStack(alignment: .leading, spacing: 16) {
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        RoundedRectangle(cornerRadius: 6)
                            .fill(.quaternary.opacity(0.3))
                            .frame(height: 12)

                        if let phase = workspace.runState.currentPhase {
                            RoundedRectangle(cornerRadius: 6)
                                .fill(phase.color)
                                .frame(width: geo.size.width * phase.progressFraction, height: 12)
                                .animation(.easeInOut(duration: 0.3), value: phase)
                        }

                        HStack(spacing: 0) {
                            ForEach(Workspace.ExecutionPhase.allCases, id: \.self) { _ in
                                Rectangle()
                                    .fill(.separator.opacity(0.5))
                                    .frame(width: 1, height: 12)
                                    .frame(maxWidth: .infinity, alignment: .trailing)
                            }
                        }
                    }
                }
                .frame(height: 12)
                .accessibilityElement(children: .combine)
                .accessibilityLabel("Phase progress: \(workspace.runState.currentPhase?.displayName ?? "Not started")")

                HStack(spacing: 0) {
                    ForEach(Workspace.ExecutionPhase.allCases, id: \.self) { phase in
                        HStack(spacing: 4) {
                            Image(systemName: phase.icon)
                                .font(.caption)
                            Text(phase.displayName)
                                .font(.caption)
                        }
                        .foregroundStyle(phase == workspace.runState.currentPhase ? phase.color : .secondary)
                        .frame(maxWidth: .infinity)
                    }
                }
            }
        }
    }
}

@MainActor
struct RunControlRunTargetSection: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        RunControlGlassSection("Up Next") {
            VStack(alignment: .leading, spacing: 12) {
                if let previewTask = workspace.runControlPreviewTask {
                    HStack(alignment: .top, spacing: 10) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(previewTask.id)
                                .font(.system(.caption, design: .monospaced))
                                .foregroundStyle(.secondary)
                            Text(previewTask.title)
                                .font(.subheadline.weight(.semibold))
                                .lineLimit(2)
                        }

                        Spacer()

                        PriorityBadge(priority: previewTask.priority)
                    }
                } else {
                    Text(
                        workspace.runControlTodoTasks.isEmpty
                            ? "No todo tasks in this workspace queue."
                            : "No runnable task is currently available."
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }

                HStack(alignment: .firstTextBaseline, spacing: 12) {
                    Picker("Task", selection: Binding(
                        get: { workspace.runState.runControlSelectedTaskID },
                        set: { workspace.runState.runControlSelectedTaskID = $0 }
                    )) {
                        Text("Auto (next runnable)")
                            .tag(Optional<String>.none)
                        ForEach(workspace.runControlTodoTasks, id: \.id) { task in
                            Text("\(task.id) · \(task.title)")
                                .lineLimit(1)
                                .tag(Optional(task.id))
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(maxWidth: 420, alignment: .leading)

                    Toggle("Force", isOn: Binding(
                        get: { workspace.runState.runControlForceDirtyRepo },
                        set: { workspace.runState.runControlForceDirtyRepo = $0 }
                    ))
                    .toggleStyle(.switch)
                    .controlSize(.small)
                    .help("Pass --force to run commands when repo is dirty.")

                    Spacer()

                    Button {
                        Task { @MainActor in
                            await workspace.refreshRunControlData()
                        }
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .buttonStyle(.plain)
                    .help("Refresh queue + config")
                }

                if workspace.runState.runControlSelectedTaskID != nil {
                    Text("Loop mode still follows queue order; selected task applies to one-off run.")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}
