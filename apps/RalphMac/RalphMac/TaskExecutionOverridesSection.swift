//!
//! TaskExecutionOverridesSection
//!
//! Purpose:
//! - Provide the thin orchestration layer for task execution override editing.
//!
//! Responsibilities:
//! - Compose the task-level and per-phase override subsections.
//! - Keep the root section focused on layout and clear/reset affordances.
//!
//! Scope:
//! - Root execution override section only.
//!
//! Usage:
//! - Embedded by task detail editing surfaces.
//!
//! Invariants/Assumptions:
//! - Override rendering and bindings live in sibling support files.

import RalphCore
import SwiftUI

@MainActor
struct TaskExecutionOverridesSection: View {
    @Binding var draftTask: RalphTask
    let workspace: Workspace
    let mutateTaskAgent: ((inout RalphTaskAgent) -> Void) -> Void

    var body: some View {
        TaskExecutionOverrideGlassSection("Execution Overrides") {
            VStack(alignment: .leading, spacing: 14) {
                TaskExecutionPresetSection(
                    draftTask: $draftTask,
                    mutateTaskAgent: mutateTaskAgent
                )
                TaskExecutionSummarySection(
                    draftTask: $draftTask,
                    workspace: workspace
                )
                TaskExecutionMainOverridesSection(
                    draftTask: $draftTask,
                    mutateTaskAgent: mutateTaskAgent,
                    workspace: workspace
                )
                TaskExecutionPhaseOverridesSection(
                    draftTask: $draftTask,
                    mutateTaskAgent: mutateTaskAgent,
                    workspace: workspace
                )

                HStack {
                    Spacer()
                    Button("Clear Execution Overrides") {
                        draftTask.agent = nil
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(draftTask.agent == nil)
                }
            }
        }
    }
}
