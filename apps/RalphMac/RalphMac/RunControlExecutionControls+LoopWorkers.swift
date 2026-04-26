/**
 RunControlExecutionControls+LoopWorkers

 Purpose:
 - Render loop worker override controls for Run Control execution actions.

 Responsibilities:
 - Render loop worker mode and custom count controls.
 - Keep loop-worker summary/help copy localized to a focused view.

 Scope:
 - Loop worker override rendering and bindings only.

 Does not handle:
 - Operator-state rendering.
 - Run/loop action buttons.

 Usage:
 - Embedded by `RunControlExecutionControlsSection`.

 Invariants/Assumptions:
 - Worker override values are clamped to the control-provided min/max range.
 */

import RalphCore
import SwiftUI

@MainActor
struct RunControlLoopWorkersControls: View {
    @ObservedObject var workspace: Workspace

    private enum LoopWorkerMode: String {
        case auto
        case custom
    }

    private var loopWorkersControl: MachineParallelWorkersControl? {
        workspace.runState.currentRunnerConfig?.executionControls?.parallelWorkers
    }

    private var loopWorkersMinimum: Int {
        Int(loopWorkersControl?.min ?? 2)
    }

    private var loopWorkersMaximum: Int {
        Int(loopWorkersControl?.max ?? UInt8.max)
    }

    private var loopWorkersDefaultMissingValue: Int {
        Int(loopWorkersControl?.defaultMissingValue ?? 2)
    }

    private var loopWorkersModeBinding: Binding<LoopWorkerMode> {
        Binding(
            get: {
                workspace.runState.runControlParallelWorkersOverride == nil ? .auto : .custom
            },
            set: { mode in
                switch mode {
                case .auto:
                    workspace.runState.runControlParallelWorkersOverride = nil
                case .custom:
                    if workspace.runState.runControlParallelWorkersOverride == nil {
                        workspace.runState.runControlParallelWorkersOverride = loopWorkersDefaultMissingValue
                    }
                }
            }
        )
    }

    private var customLoopWorkersBinding: Binding<Int> {
        Binding(
            get: { workspace.runState.runControlParallelWorkersOverride ?? loopWorkersDefaultMissingValue },
            set: { value in
                workspace.runState.runControlParallelWorkersOverride = min(
                    max(value, loopWorkersMinimum),
                    loopWorkersMaximum
                )
            }
        )
    }

    private var loopWorkersSummary: String {
        if let workers = workspace.runState.runControlParallelWorkersOverride {
            return "\(workers) workers"
        }
        if workspace.runState.currentRunnerConfig?.safety?.parallelConfigured == true {
            return "Auto (configured)"
        }
        return "Auto (sequential)"
    }

    private var loopWorkersHelp: String {
        if let workers = workspace.runState.runControlParallelWorkersOverride {
            return "The next loop will pass --parallel \(workers) and start a shared \(workers)-worker coordinator."
        }
        if workspace.runState.currentRunnerConfig?.safety?.parallelConfigured == true {
            return "No app override is set. The next loop will use the repository or global parallel worker setting from resolved config."
        }
        return "No app override is set. If you later enable --parallel without a value, Ralph defaults to \(loopWorkersDefaultMissingValue) workers. Valid range: \(loopWorkersMinimum)-\(loopWorkersMaximum)."
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 12) {
                Label("Loop workers", systemImage: "square.stack.3d.up")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)

                Picker("Loop workers mode", selection: loopWorkersModeBinding) {
                    Text("Auto").tag(LoopWorkerMode.auto)
                    Text("Custom").tag(LoopWorkerMode.custom)
                }
                .pickerStyle(.menu)
                .frame(maxWidth: 180, alignment: .leading)
                .help("Auto uses resolved config. Custom passes an explicit --parallel worker count for the next loop.")

                if workspace.runState.runControlParallelWorkersOverride != nil {
                    TextField("Workers", value: customLoopWorkersBinding, format: .number)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 76)

                    Stepper(
                        "",
                        value: customLoopWorkersBinding,
                        in: loopWorkersMinimum...loopWorkersMaximum
                    )
                    .labelsHidden()
                }

                Text(loopWorkersSummary)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)

                Spacer()
            }

            Text(loopWorkersHelp)
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
    }
}
