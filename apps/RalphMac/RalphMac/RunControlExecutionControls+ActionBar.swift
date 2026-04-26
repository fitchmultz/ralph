/**
 RunControlExecutionControls+ActionBar

 Purpose:
 - Render run/loop execution buttons and transient execution rows.

 Responsibilities:
 - Render run, loop, and stop controls with existing enable/disable behavior.
 - Render preparing-run, loop-active, and last-exit status rows.

 Scope:
 - Execution action-bar rendering only.

 Does not handle:
 - Operator-state cards or diagnostics actions.
 - Queue lock inspection and repair flows.

 Usage:
 - Embedded by `RunControlExecutionControlsSection`.

 Invariants/Assumptions:
 - Button actions call through to `Workspace` run-control entry points.
 */

import RalphCore
import SwiftUI

@MainActor
struct RunControlExecutionActionBar: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            executionButtons

            if workspace.runState.isPreparingRun {
                HStack {
                    ProgressView()
                        .controlSize(.small)
                    Text("Preparing run")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                }
            }

            if workspace.runState.isLoopMode {
                HStack {
                    Image(systemName: "repeat.circle.fill")
                        .foregroundStyle(.blue)
                    Text("Loop Active")
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

            if let status = workspace.runState.lastExitStatus, !workspace.runState.isExecutionActive {
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

    @ViewBuilder
    private var executionButtons: some View {
        let previewTask = workspace.runControlPreviewTask
        let hasSelectedTask = workspace.selectedRunControlTask != nil

        HStack(spacing: 12) {
            if workspace.runState.isExecutionActive {
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

                Button(action: {
                    workspace.startLoop(
                        forceDirtyRepo: workspace.runState.runControlForceDirtyRepo,
                        parallelWorkers: workspace.runState.runControlParallelWorkersOverride
                    )
                }) {
                    Label("Start Loop", systemImage: "repeat.circle")
                }
                .buttonStyle(GlassButtonStyle())
                .accessibilityLabel("Start loop")
                .accessibilityHint("Runs the machine loop with max tasks set to zero, then streams progress until the loop completes or is stopped")
            }

            Spacer()
        }
    }
}
