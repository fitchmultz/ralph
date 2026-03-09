//!
//! RunControlSection
//!
//! Purpose:
//! - Provide the thin orchestration surface for the Run Control columns.
//!
//! Responsibilities:
//! - Compose the content and detail columns from focused Run Control subsections.
//! - Trigger workspace refresh when the workspace root changes.
//!
//! Scope:
//! - Column composition only. Subsection rendering lives in sibling Run Control files.
//!
//! Usage:
//! - Embedded by `WorkspaceView` for the Run Control sidebar section.
//!
//! Invariants/Assumptions:
//! - Workspace state is read through dedicated domain owners.

import RalphCore
import SwiftUI

@MainActor
struct RunControlContentColumn: View {
    @ObservedObject var workspace: Workspace
    let navTitle: (String) -> String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            WorkingDirectoryHeader(workspace: workspace)
                .padding(16)

            Divider()

            RunControlConsoleView(workspace: workspace)
                .padding(16)
        }
        .contentBackground(cornerRadius: 12)
        .navigationTitle(navTitle("Run Control"))
    }
}

@MainActor
struct RunControlDetailColumn: View {
    @ObservedObject var workspace: Workspace
    let navTitle: (String) -> String

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                RunControlCurrentTaskSection(workspace: workspace)

                if workspace.runState.isRunning {
                    RunControlPhaseProgressSection(workspace: workspace)
                }

                RunControlRunTargetSection(workspace: workspace)
                RunControlRunnerConfigurationSection(workspace: workspace)
                RunControlExecutionControlsSection(workspace: workspace)
                RunControlExecutionHistorySection(workspace: workspace)
            }
            .padding(20)
        }
        .background(.clear)
        .navigationTitle(navTitle("Run Control"))
        .task(id: workspace.identityState.workingDirectoryURL.path) {
            await workspace.refreshRunControlData()
        }
    }
}
