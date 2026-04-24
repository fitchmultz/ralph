/**
 RunControlDetailSections+Safety

 Purpose:
 - Render the Run Control safety-status card and its supporting row views.

 Responsibilities:
 - Render the Run Control safety-status card and its supporting row views.
 - Keep safety-contract messaging isolated from task/progress/history sections.

 Does not handle:
 - Task execution controls.
 - History or progress rendering.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

@MainActor
struct RunControlSafetyStatusSection: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        let safety = workspace.runState.currentRunnerConfig?.safety
        let requestedParallelWorkers = workspace.runState.runControlParallelWorkersOverride

        RunControlGlassSection("Safety Status") {
            if let safety {
                VStack(alignment: .leading, spacing: 10) {
                    SafetyStatusRow(
                        icon: safety.repoTrusted ? "checkmark.shield" : "exclamationmark.shield",
                        title: "Repository Trust",
                        value: safety.repoTrusted ? "Trusted" : "Untrusted",
                        emphasis: safety.repoTrusted ? .normal : .warning
                    )
                    SafetyStatusRow(
                        icon: safety.dirtyRepo ? "exclamationmark.triangle" : "checkmark.circle",
                        title: "Working Tree",
                        value: safety.dirtyRepo ? "Dirty repo" : "Clean repo",
                        emphasis: safety.dirtyRepo ? .warning : .normal
                    )
                    SafetyStatusRow(
                        icon: safety.gitPublishMode == "off" ? "lock" : "arrow.up.doc",
                        title: "Git Publish",
                        value: safety.gitPublishMode == "off"
                            ? "Off"
                            : safety.gitPublishMode.replacingOccurrences(of: "_", with: " "),
                        emphasis: safety.gitPublishMode == "off" ? .normal : .warning
                    )
                    SafetyStatusRow(
                        icon: "hand.raised",
                        title: "Requested Approval",
                        value: safety.approvalMode ?? "default",
                        emphasis: .warning
                    )
                    SafetyStatusRow(
                        icon: safety.ciGateEnabled ? "checkmark.seal" : "exclamationmark.octagon",
                        title: "CI Gate",
                        value: safety.ciGateEnabled ? "Enabled" : "Disabled",
                        emphasis: safety.ciGateEnabled ? .normal : .warning
                    )
                    SafetyStatusRow(
                        icon: "arrow.uturn.backward.circle",
                        title: "Revert Mode",
                        value: safety.gitRevertMode,
                        emphasis: safety.gitRevertMode == "disabled" ? .warning : .normal
                    )
                    SafetyStatusRow(
                        icon: requestedParallelWorkers != nil || safety.parallelConfigured
                            ? "sparkles.rectangle.stack"
                            : "square.stack.3d.up",
                        title: "Parallel",
                        value: requestedParallelWorkers.map { "\($0) workers (next loop)" }
                            ?? (safety.parallelConfigured ? "Configured" : "Auto"),
                        emphasis: requestedParallelWorkers != nil || safety.parallelConfigured
                            ? .warning
                            : .normal
                    )
                    SafetyStatusRow(
                        icon: "terminal",
                        title: "App Run Mode",
                        value: safety.executionInteractivity.replacingOccurrences(of: "_", with: " "),
                        emphasis: .warning
                    )
                    SafetyStatusRow(
                        icon: safety.interactiveApprovalSupported ? "checkmark.bubble" : "bubble.left.and.exclamationmark.bubble.right",
                        title: "Interactive Approvals",
                        value: safety.interactiveApprovalSupported ? "Supported" : "Terminal-only",
                        emphasis: safety.interactiveApprovalSupported ? .normal : .warning
                    )

                    Text("App-launched runs stream output only. They cannot answer per-action approval prompts; use terminal-first CLI runs for interactive approval workflows.")
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    if workspace.diagnosticsState.cliHealthStatus?.isAvailable == false {
                        SafetyStatusRow(
                            icon: "terminal",
                            title: "CLI",
                            value: "Unavailable",
                            emphasis: .error
                        )
                    }
                }
            } else {
                Text("Load resolved config to inspect trust, publish, CI, and noninteractive app-run safety status.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private enum SafetyStatusEmphasis {
    case normal
    case warning
    case error

    var color: Color {
        switch self {
        case .normal:
            return .secondary
        case .warning:
            return .orange
        case .error:
            return .red
        }
    }
}

private struct SafetyStatusRow: View {
    let icon: String
    let title: String
    let value: String
    let emphasis: SafetyStatusEmphasis

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: icon)
                .foregroundStyle(emphasis.color)
                .frame(width: 16)

            Text(title)
                .font(.subheadline)

            Spacer()

            Text(value)
                .font(.caption.weight(.semibold))
                .foregroundStyle(emphasis.color)
        }
    }
}
