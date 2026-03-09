/**
 ConsoleView

 Responsibilities:
 - Display console output from workspace with styling.
 - Show error messages if present.
 - Shared component used across Quick Actions and Advanced Runner.

 Does not handle:
 - CLI execution or output buffering (handled by Workspace).
 - ANSI color parsing (handled by RunControlConsoleView).

 Invariants/assumptions callers must respect:
 - Workspace is injected via @ObservedObject.
 - Output updates trigger view refresh automatically.
 */

import SwiftUI
import RalphCore

@MainActor
struct ConsoleView: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Console Output")
                    .font(.system(.caption, weight: .semibold))
                    .foregroundStyle(.secondary)

                Spacer()

                if let error = workspace.runState.errorMessage {
                    Text(error)
                        .foregroundStyle(.red)
                        .font(.system(.caption))
                }
            }

            ScrollView {
                Text(workspace.runState.output.isEmpty ? "(no output yet)" : workspace.runState.output)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .font(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                    .padding(12)
            }
            .frame(minHeight: 200)
            .underPageBackground(cornerRadius: 10, isEmphasized: false)
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(.separator.opacity(0.3), lineWidth: 0.5)
            )
        }
    }
}
