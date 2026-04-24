/**
 RunControlDetailSections+TaskStatus

 Purpose:
 - Render current-task, last-run, and idle-state cards for the Run Control detail column.

 Responsibilities:
 - Render current-task, last-run, and idle-state cards for the Run Control detail column.
 - Keep execution-summary presentation isolated from progress/configuration sections.

 Does not handle:
 - Run-control buttons or queue-target selection.
 - Safety-status rendering.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

@MainActor
struct RunControlCurrentTaskSection: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        if workspace.runState.isRunning,
           let taskID = workspace.runState.currentTaskID,
           let task = workspace.taskState.tasks.first(where: { $0.id == taskID }) {
            RunControlGlassSection("Current Task") {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text(task.id)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .accessibilityLabel("Task ID: \(task.id)")

                        Spacer()

                        PriorityBadge(priority: task.priority)
                    }

                    Text(task.title)
                        .font(.headline)
                        .lineLimit(2)

                    if let description = task.description, !description.isEmpty {
                        Text(description)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(3)
                    }

                    HStack {
                        StatusBadge(status: task.status)

                        if !task.tags.isEmpty {
                            RunControlTagChips(tags: Array(task.tags.prefix(3)))
                        }

                        Spacer()

                        if let startTime = workspace.runState.executionStartTime {
                            ElapsedTimeView(startTime: startTime)
                                .font(.system(.caption, design: .monospaced))
                                .foregroundStyle(.secondary)
                                .accessibilityLabel("Elapsed time")
                        }
                    }
                }
            }
        } else if !workspace.runState.executionHistory.isEmpty {
            RunControlLastRunSummary(workspace: workspace)
        } else {
            RunControlNoExecutionView()
        }
    }
}

@MainActor
struct RunControlExecutionHistorySection: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        if !workspace.runState.executionHistory.isEmpty {
            RunControlGlassSection("Recent History") {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(workspace.runState.executionHistory.prefix(5)) { record in
                        RunControlExecutionHistoryRow(record: record)
                    }
                }
            }
        }
    }
}

@MainActor
private struct RunControlNoExecutionView: View {
    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "play.circle")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)

            Text("No Active Execution")
                .font(.headline)

            Text("Run a task to see execution progress and live output.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .frame(maxWidth: .infinity, minHeight: 200)
    }
}

@MainActor
private struct RunControlLastRunSummary: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        if let lastRun = workspace.runState.executionHistory.first {
            RunControlGlassSection("Last Run") {
                HStack {
                    RunControlExecutionStatusIcon(record: lastRun)

                    if let taskID = lastRun.taskID {
                        Text(taskID)
                            .font(.system(.body, design: .monospaced))
                    }

                    Spacer()

                    if let duration = lastRun.duration {
                        Text(RunControlDurationFormatter.string(for: duration))
                            .font(.system(.body, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }
}
