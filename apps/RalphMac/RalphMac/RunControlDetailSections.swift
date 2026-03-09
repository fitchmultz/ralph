//!
//! RunControlDetailSections
//!
//! Purpose:
//! - Host focused Run Control detail subsections so the root section file stays orchestration-only.
//!
//! Responsibilities:
//! - Render current-task, phase-progress, target-selection, configuration, controls, and history cards.
//!
//! Scope:
//! - Detail-column rendering only.
//!
//! Usage:
//! - Composed by `RunControlDetailColumn`.
//!
//! Invariants/Assumptions:
//! - Workspace state is read from `taskState` and `runState`.

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
                    Text("No todo tasks in this workspace queue.")
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

@MainActor
struct RunControlRunnerConfigurationSection: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        RunControlGlassSection("Runner Configuration") {
            VStack(alignment: .leading, spacing: 8) {
                if workspace.runState.runnerConfigLoading {
                    HStack(spacing: 8) {
                        ProgressView()
                            .controlSize(.small)
                        Text("Loading resolved config...")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                RunControlConfigRow(icon: "cpu", label: "Model", value: workspace.runState.currentRunnerConfig?.model ?? "Default")
                RunControlConfigRow(icon: "square.split.2x1", label: "Phases", value: workspace.runState.currentRunnerConfig?.phases.map(String.init) ?? "Auto")
                RunControlConfigRow(icon: "number", label: "Max Iterations", value: workspace.runState.currentRunnerConfig?.maxIterations.map(String.init) ?? "Auto")

                if let configError = workspace.runState.runnerConfigErrorMessage {
                    Text(configError)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}

@MainActor
struct RunControlExecutionControlsSection: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        RunControlGlassSection("Controls") {
            VStack(spacing: 12) {
                let previewTask = workspace.runControlPreviewTask
                let hasSelectedTask = workspace.selectedRunControlTask != nil

                HStack(spacing: 12) {
                    if workspace.runState.isRunning {
                        Button(action: { workspace.cancel() }) {
                            Label("Stop", systemImage: "stop.circle.fill")
                                .foregroundStyle(.red)
                        }
                        .buttonStyle(GlassButtonStyle())
                        .accessibilityLabel("Stop execution")
                        .accessibilityHint("Cancel the current task execution")

                        if workspace.runState.isLoopMode {
                            Button(action: { workspace.stopLoop() }) {
                                Label("Stop After Current", systemImage: "pause.circle")
                                    .foregroundStyle(.orange)
                            }
                            .buttonStyle(GlassButtonStyle())
                        }
                    } else {
                        Button(action: {
                            workspace.runNextTask(
                                taskIDOverride: workspace.runState.runControlSelectedTaskID,
                                forceDirtyRepo: workspace.runState.runControlForceDirtyRepo
                            )
                        }) {
                            Label(hasSelectedTask ? "Run Selected Task" : "Run Next Task", systemImage: "play.circle.fill")
                        }
                        .buttonStyle(GlassButtonStyle())
                        .disabled(previewTask == nil)
                        .accessibilityLabel("Run next task")
                        .accessibilityHint("Starts execution of the selected task or next task in the queue")

                        Button(action: { workspace.startLoop(forceDirtyRepo: workspace.runState.runControlForceDirtyRepo) }) {
                            Label("Start Loop", systemImage: "repeat.circle")
                        }
                        .buttonStyle(GlassButtonStyle())
                        .disabled(workspace.nextTask() == nil)
                        .accessibilityLabel("Start task loop")
                        .accessibilityHint("Continuously run tasks until stopped")
                    }

                    Spacer()
                }

                if workspace.runState.isLoopMode {
                    HStack {
                        Image(systemName: "repeat.circle.fill")
                            .foregroundStyle(.blue)
                        Text("Loop Mode Active")
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        if workspace.runState.stopAfterCurrent {
                            Text("(Stopping after current)")
                                .font(.caption)
                                .foregroundStyle(.orange)
                        }

                        Spacer()
                    }
                }

                if let status = workspace.runState.lastExitStatus, !workspace.runState.isRunning {
                    HStack {
                        Image(systemName: status.code == 0 ? "checkmark.circle.fill" : "xmark.circle.fill")
                            .foregroundStyle(status.code == 0 ? .green : .red)
                        Text("Exit: \(status.code)")
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(status.code == 0 ? .green : .red)
                        Spacer()
                    }
                }
            }
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
