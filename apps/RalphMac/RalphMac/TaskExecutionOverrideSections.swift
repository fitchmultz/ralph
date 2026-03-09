//!
//! TaskExecutionOverrideSections
//!
//! Purpose:
//! - Break task execution override editing into focused, intent-based sections.
//!
//! Responsibilities:
//! - Render presets, summary, task-level overrides, and per-phase overrides.
//! - Keep binding logic grouped with the subsection that owns it.
//!
//! Scope:
//! - Execution override section rendering only.
//!
//! Usage:
//! - Composed by `TaskExecutionOverridesSection`.
//!
//! Invariants/Assumptions:
//! - Task-agent mutations flow through the provided closure so normalization stays centralized.

import RalphCore
import SwiftUI

@MainActor
struct TaskExecutionPresetSection: View {
    @Binding var draftTask: RalphTask
    let mutateTaskAgent: ((inout RalphTaskAgent) -> Void) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Quick Presets")
                .font(.caption)
                .foregroundStyle(.secondary)

            ViewThatFits(in: .horizontal) {
                FlowLayout(spacing: 8) {
                    presetButtons
                }
                ScrollView(.horizontal) {
                    HStack(spacing: 8) {
                        presetButtons
                    }
                }
                .scrollIndicators(.hidden)
            }

            if activeExecutionPreset == nil, draftTask.agent != nil {
                Label("Custom override active", systemImage: "slider.horizontal.3")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private var presetButtons: some View {
        ForEach(RalphTaskExecutionPreset.allCases) { preset in
            TaskExecutionPresetButton(
                preset: preset,
                isActive: activeExecutionPreset == preset,
                action: {
                    draftTask.agent = RalphTaskAgent.normalizedOverride(preset.agentOverride)
                }
            )
        }
    }

    private var activeExecutionPreset: RalphTaskExecutionPreset? {
        RalphTaskExecutionPreset.matchingPreset(for: draftTask.agent)
    }
}

@MainActor
struct TaskExecutionSummarySection: View {
    @Binding var draftTask: RalphTask
    let workspace: Workspace

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(
                overrideSummaryCaption,
                systemImage: draftTask.agent == nil ? "arrow.down.circle" : "slider.horizontal.3"
            )
            .font(.caption)
            .foregroundStyle(.secondary)

            Text(inheritedConfigCaption)
                .font(.caption2)
                .foregroundStyle(.secondary)

            Text(taskEffortDisabled
                ? "Reasoning effort is ignored unless runner is codex. Set runner to codex or inherit."
                : "Reasoning effort is only used when the resolved runner is codex."
            )
            .font(.caption2)
            .foregroundStyle(.secondary)
        }
    }

    private var overrideSummaryCaption: String {
        guard let agent = RalphTaskAgent.normalizedOverride(draftTask.agent) else {
            return "No task override. Runner/model/phases/iterations inherit from config."
        }

        var parts: [String] = []
        if let runner = agent.runner { parts.append("runner \(runner)") }
        if let model = agent.model { parts.append("model \(model)") }
        if let effort = agent.modelEffort { parts.append("effort \(effort)") }
        if let phases = agent.phases { parts.append("phases \(phases)") }
        if let iterations = agent.iterations { parts.append("iterations \(iterations)") }
        if let overrides = agent.phaseOverrides, !overrides.isEmpty {
            let count = [overrides.phase1, overrides.phase2, overrides.phase3].compactMap { $0 }.count
            parts.append("\(count) phase override\(count == 1 ? "" : "s")")
        }
        return parts.isEmpty ? "Task override active" : "Task override: \(parts.joined(separator: ", "))"
    }

    private var inheritedConfigCaption: String {
        let inheritedModel = workspace.runState.currentRunnerConfig?.model ?? "default"
        let inheritedIterations = workspace.runState.currentRunnerConfig?.maxIterations.map(String.init) ?? "default"
        let inheritedPhases = workspace.runState.currentRunnerConfig?.phases.map(String.init) ?? "default"
        return "Current inherited config: model \(inheritedModel), phases \(inheritedPhases), iterations \(inheritedIterations)."
    }

    private var taskEffortDisabled: Bool {
        guard let runner = TaskExecutionOverrideSupport.normalizedRunnerName(draftTask.agent?.runner) else {
            return false
        }
        return runner != "codex"
    }
}

@MainActor
struct TaskExecutionMainOverridesSection: View {
    @Binding var draftTask: RalphTask
    let mutateTaskAgent: ((inout RalphTaskAgent) -> Void) -> Void
    let workspace: Workspace

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 16) {
                Picker("Runner", selection: taskRunnerBinding) {
                    Text("Inherit").tag("inherit")
                    ForEach(TaskExecutionOverrideSupport.runnerOptions, id: \.self) { runner in
                        Text(runner).tag(runner)
                    }
                }
                .pickerStyle(.menu)
                .frame(width: 170)

                VStack(alignment: .leading, spacing: 4) {
                    Text("Model")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextField("Inherit from config", text: taskModelBinding)
                        .textFieldStyle(.roundedBorder)
                        .frame(minWidth: 220)
                }

                Spacer()
            }

            HStack(spacing: 16) {
                Picker("Reasoning Effort", selection: taskEffortBinding) {
                    Text("Inherit").tag("inherit")
                    ForEach(TaskExecutionOverrideSupport.effortOptions, id: \.self) { effort in
                        Text(effort).tag(effort)
                    }
                }
                .pickerStyle(.menu)
                .frame(width: 170)
                .disabled(taskEffortDisabled)

                Picker("Phases", selection: taskPhasesBinding) {
                    Text("Inherit").tag(0)
                    Text("1").tag(1)
                    Text("2").tag(2)
                    Text("3").tag(3)
                }
                .pickerStyle(.menu)
                .frame(width: 130)

                Picker("Iterations", selection: taskIterationsBinding) {
                    Text("Inherit").tag(0)
                    ForEach(1...10, id: \.self) { iteration in
                        Text(String(iteration)).tag(iteration)
                    }
                }
                .pickerStyle(.menu)
                .frame(width: 130)

                Spacer()
            }
        }
    }

    private var taskEffortDisabled: Bool {
        guard let runner = TaskExecutionOverrideSupport.normalizedRunnerName(draftTask.agent?.runner) else {
            return false
        }
        return runner != "codex"
    }

    private var taskRunnerBinding: Binding<String> {
        Binding(
            get: { draftTask.agent?.runner ?? "inherit" },
            set: { value in
                mutateTaskAgent { agent in
                    agent.runner = value == "inherit" ? nil : value
                }
            }
        )
    }

    private var taskModelBinding: Binding<String> {
        Binding(
            get: { draftTask.agent?.model ?? "" },
            set: { value in
                mutateTaskAgent { agent in
                    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
                    agent.model = trimmed.isEmpty ? nil : trimmed
                }
            }
        )
    }

    private var taskEffortBinding: Binding<String> {
        Binding(
            get: { draftTask.agent?.modelEffort ?? "inherit" },
            set: { value in
                mutateTaskAgent { agent in
                    agent.modelEffort = value == "inherit" ? nil : value
                }
            }
        )
    }

    private var taskPhasesBinding: Binding<Int> {
        Binding(
            get: { draftTask.agent?.phases ?? 0 },
            set: { value in
                mutateTaskAgent { agent in
                    agent.phases = value == 0 ? nil : value
                }
            }
        )
    }

    private var taskIterationsBinding: Binding<Int> {
        Binding(
            get: { draftTask.agent?.iterations ?? 0 },
            set: { value in
                mutateTaskAgent { agent in
                    agent.iterations = value == 0 ? nil : value
                }
            }
        )
    }
}

@MainActor
struct TaskExecutionPhaseOverridesSection: View {
    @Binding var draftTask: RalphTask
    let mutateTaskAgent: ((inout RalphTaskAgent) -> Void) -> Void
    let workspace: Workspace

    var body: some View {
        Divider()

        HStack {
            Text("Per-Phase Overrides")
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Text("Using \(resolvedPhaseCount) phase\(resolvedPhaseCount == 1 ? "" : "s")")
                .font(.caption2)
                .foregroundStyle(.secondary)
        }

        ForEach(1...resolvedPhaseCount, id: \.self) { phase in
            PhaseOverrideEditor(
                title: phaseTitle(phase),
                phase: phase,
                draftTask: $draftTask,
                mutateTaskAgent: mutateTaskAgent
            )
        }

        if hasIgnoredPhaseOverrides {
            IgnoredOverridesWarning(
                draftTask: $draftTask,
                resolvedPhaseCount: resolvedPhaseCount
            )
        }
    }

    private var resolvedPhaseCount: Int {
        let taskPhases = draftTask.agent?.phases
        let inheritedPhases = workspace.runState.currentRunnerConfig?.phases
        return min(max(taskPhases ?? inheritedPhases ?? 3, 1), 3)
    }

    private func phaseTitle(_ phase: Int) -> String {
        switch phase {
        case 1: return "Phase 1 (Planning)"
        case 2: return "Phase 2 (Implementation)"
        case 3: return "Phase 3 (Review)"
        default: return "Phase \(phase)"
        }
    }

    private var hasIgnoredPhaseOverrides: Bool {
        let overrides = draftTask.agent?.phaseOverrides
        if resolvedPhaseCount < 3, overrides?.phase3 != nil { return true }
        if resolvedPhaseCount < 2, overrides?.phase2 != nil { return true }
        return false
    }
}
