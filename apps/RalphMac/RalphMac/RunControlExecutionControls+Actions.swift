/**
 RunControlExecutionControls+Actions

 Purpose:
 - Render operator-action lists for Run Control status cards.

 Responsibilities:
 - Render operator-action titles/details and disposition-specific controls.
 - Keep operator-action icon and row layout logic isolated from status orchestration.

 Scope:
 - Operator-action rendering only.

 Does not handle:
 - Operator-state classification.
 - Action execution side effects.

 Usage:
 - Embedded by status cards in `RunControlOperatorStatusGroup`.

 Invariants/Assumptions:
 - Actions are passed in display order from the owning status surface.
 */

import RalphCore
import SwiftUI

@MainActor
struct RunControlOperatorActionsList: View {
    let actions: [Workspace.RunControlOperatorAction]
    let performAction: (Workspace.RunControlOperatorAction) -> Void
    let isActionDisabled: (Workspace.RunControlOperatorAction) -> Bool
    let isProminentAction: (Workspace.RunControlOperatorAction) -> Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Next")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)

            ForEach(actions) { action in
                RunControlOperatorActionRow(
                    action: action,
                    performAction: performAction,
                    isActionDisabled: isActionDisabled,
                    isProminentAction: isProminentAction
                )
            }
        }
    }
}

@MainActor
private struct RunControlOperatorActionRow: View {
    let action: Workspace.RunControlOperatorAction
    let performAction: (Workspace.RunControlOperatorAction) -> Void
    let isActionDisabled: (Workspace.RunControlOperatorAction) -> Bool
    let isProminentAction: (Workspace.RunControlOperatorAction) -> Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(action.title)
                        .font(.caption.weight(.medium))
                    Text(action.detail)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                Spacer(minLength: 12)

                switch action.disposition {
                case .native:
                    operatorActionButton
                case .copyCommand:
                    Button("Copy Command") {
                        performAction(action)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                case .unsupported:
                    Text("Not Native")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
            }

            switch action.disposition {
            case .native:
                EmptyView()
            case .copyCommand(let command):
                Text(command)
                    .font(.system(.caption2, design: .monospaced))
                    .textSelection(.enabled)
            case .unsupported(let reason, let command):
                Text(reason)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                if let command, !command.isEmpty {
                    Text(command)
                        .font(.system(.caption2, design: .monospaced))
                        .textSelection(.enabled)
                }
            }
        }
    }

    @ViewBuilder
    private var operatorActionButton: some View {
        if isProminentAction(action) {
            Button(action: { performAction(action) }) {
                Label(action.title, systemImage: operatorActionIcon(action))
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)
            .disabled(isActionDisabled(action))
        } else {
            Button(action: { performAction(action) }) {
                Label(action.title, systemImage: operatorActionIcon(action))
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .disabled(isActionDisabled(action))
        }
    }

    private func operatorActionIcon(_ action: Workspace.RunControlOperatorAction) -> String {
        guard case .native(let nativeAction) = action.disposition else {
            return "bolt.circle"
        }

        switch nativeAction {
        case .refreshRunControlStatus, .refreshQueueStatus, .refreshParallelStatus:
            return "arrow.clockwise"
        case .validateQueue:
            return "checkmark.shield"
        case .previewQueueRepair:
            return "wrench.and.screwdriver"
        case .previewQueueUndo:
            return "clock.arrow.circlepath"
        case .stopAfterCurrent:
            return "pause.circle"
        case .inspectQueueLock:
            return "lock.magnifyingglass"
        case .previewQueueUnlock:
            return "lock.open.trianglebadge.exclamationmark"
        case .clearStaleQueueLock:
            return "lock.open"
        }
    }
}
