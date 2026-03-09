//!
//! TaskExecutionOverrideSupport
//!
//! Purpose:
//! - Hold reusable execution-override helper types and support views.
//!
//! Responsibilities:
//! - Provide section chrome, preset buttons, phase editors, and shared option helpers.
//!
//! Scope:
//! - Presentation and binding helpers for execution override editing.
//!
//! Usage:
//! - Used by `TaskExecutionOverridesSection` and sibling override section files.
//!
//! Invariants/Assumptions:
//! - Agent normalization remains the final write path when mutating draft tasks.

import AppKit
import RalphCore
import SwiftUI

enum TaskExecutionOverrideSupport {
    static let runnerOptions = ["codex", "opencode", "gemini", "claude", "cursor", "kimi", "pi"]
    static let effortOptions = ["low", "medium", "high", "xhigh"]

    static func normalizedRunnerName(_ value: String?) -> String? {
        guard let value else { return nil }
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return normalized.isEmpty ? nil : normalized
    }
}

@MainActor
struct TaskExecutionOverrideGlassSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(.caption, weight: .semibold))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)

            content
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .underPageBackground(cornerRadius: 10, isEmphasized: false)
        }
        .accessibilityLabel("\(title) section")
    }
}

@MainActor
struct TaskExecutionPresetButton: View {
    let preset: RalphTaskExecutionPreset
    let isActive: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 2) {
                Text(preset.displayName)
                    .font(.caption.weight(.semibold))
                Text(preset.description)
                    .font(.caption2)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .foregroundStyle(isActive ? Color.white : Color.primary)
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .frame(minWidth: 160, idealWidth: 180, maxWidth: 220, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(isActive ? Color.accentColor : Color(NSColor.windowBackgroundColor).opacity(0.35))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .stroke(isActive ? Color.accentColor : Color.secondary.opacity(0.25), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }
}

@MainActor
struct PhaseOverrideEditor: View {
    let title: String
    let phase: Int
    @Binding var draftTask: RalphTask
    let mutateTaskAgent: ((inout RalphTaskAgent) -> Void) -> Void

    var body: some View {
        let effortDisabled = phaseEffortDisabled
        let hasOverride = phaseOverride != nil

        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(title)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Clear") {
                    setPhaseOverride(nil)
                }
                .buttonStyle(.borderless)
                .controlSize(.small)
                .disabled(!hasOverride)
            }

            HStack(spacing: 12) {
                Picker("Runner", selection: phaseRunnerBinding) {
                    Text("Inherit").tag("inherit")
                    ForEach(TaskExecutionOverrideSupport.runnerOptions, id: \.self) { runner in
                        Text(runner).tag(runner)
                    }
                }
                .pickerStyle(.menu)
                .frame(width: 160)

                TextField("Model (inherit if empty)", text: phaseModelBinding)
                    .textFieldStyle(.roundedBorder)

                Picker("Effort", selection: phaseEffortBinding) {
                    Text("Inherit").tag("inherit")
                    ForEach(TaskExecutionOverrideSupport.effortOptions, id: \.self) { effort in
                        Text(effort).tag(effort)
                    }
                }
                .pickerStyle(.menu)
                .frame(width: 140)
                .disabled(effortDisabled)
            }

            if effortDisabled {
                Text("Reasoning effort applies only when the effective runner is codex.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(8)
        .background(Color(NSColor.windowBackgroundColor).opacity(0.35))
        .cornerRadius(8)
    }

    private var phaseOverride: RalphTaskPhaseOverride? {
        switch phase {
        case 1: return draftTask.agent?.phaseOverrides?.phase1
        case 2: return draftTask.agent?.phaseOverrides?.phase2
        case 3: return draftTask.agent?.phaseOverrides?.phase3
        default: return nil
        }
    }

    private func setPhaseOverride(_ value: RalphTaskPhaseOverride?) {
        mutateTaskAgent { agent in
            var overrides = agent.phaseOverrides ?? RalphTaskPhaseOverrides()
            switch phase {
            case 1: overrides.phase1 = value
            case 2: overrides.phase2 = value
            case 3: overrides.phase3 = value
            default: break
            }
            agent.phaseOverrides = overrides.isEmpty ? nil : overrides
        }
    }

    private var phaseRunnerBinding: Binding<String> {
        Binding(
            get: { phaseOverride?.runner ?? "inherit" },
            set: { value in
                var updated = phaseOverride ?? RalphTaskPhaseOverride()
                updated.runner = value == "inherit" ? nil : value
                setPhaseOverride(updated.isEmpty ? nil : updated)
            }
        )
    }

    private var phaseModelBinding: Binding<String> {
        Binding(
            get: { phaseOverride?.model ?? "" },
            set: { value in
                var updated = phaseOverride ?? RalphTaskPhaseOverride()
                let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
                updated.model = trimmed.isEmpty ? nil : trimmed
                setPhaseOverride(updated.isEmpty ? nil : updated)
            }
        )
    }

    private var phaseEffortBinding: Binding<String> {
        Binding(
            get: { phaseOverride?.reasoningEffort ?? "inherit" },
            set: { value in
                var updated = phaseOverride ?? RalphTaskPhaseOverride()
                updated.reasoningEffort = value == "inherit" ? nil : value
                setPhaseOverride(updated.isEmpty ? nil : updated)
            }
        )
    }

    private var phaseEffortDisabled: Bool {
        let phaseRunner = TaskExecutionOverrideSupport.normalizedRunnerName(phaseOverride?.runner)
        let taskRunner = TaskExecutionOverrideSupport.normalizedRunnerName(draftTask.agent?.runner)
        guard let effectiveRunner = phaseRunner ?? taskRunner else { return false }
        return effectiveRunner != "codex"
    }
}

@MainActor
struct IgnoredOverridesWarning: View {
    @Binding var draftTask: RalphTask
    let resolvedPhaseCount: Int

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.yellow)
            VStack(alignment: .leading, spacing: 4) {
                Text("Some phase overrides are currently ignored.")
                    .font(.caption)
                    .foregroundStyle(.primary)
                Text("Overrides for phases above your selected phase count are not used until you increase phases again.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button("Trim Ignored") {
                trimIgnoredPhaseOverrides()
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(8)
        .background(Color(NSColor.windowBackgroundColor).opacity(0.35))
        .clipShape(.rect(cornerRadius: 8))
    }

    private func trimIgnoredPhaseOverrides() {
        guard var agent = draftTask.agent else { return }
        guard var overrides = agent.phaseOverrides else { return }

        if resolvedPhaseCount < 2 {
            overrides.phase2 = nil
        }
        if resolvedPhaseCount < 3 {
            overrides.phase3 = nil
        }
        agent.phaseOverrides = overrides.isEmpty ? nil : overrides
        draftTask.agent = RalphTaskAgent.normalizedOverride(agent)
    }
}
