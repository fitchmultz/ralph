/**
 RunControlExecutionControls+Diagnostics

 Purpose:
 - Render diagnostics text output in a reusable Run Control sheet.

 Responsibilities:
 - Render loading and output states for diagnostics actions.
 - Provide a consistent close affordance for diagnostics output.

 Scope:
 - Diagnostics output-sheet presentation only.

 Does not handle:
 - Diagnostics command execution.
 - Queue/operator status classification.

 Usage:
 - Presented by `RunControlExecutionControlsSection` when operator diagnostics actions run.

 Invariants/Assumptions:
 - Callers provide a title and output text that match the active diagnostics command.
 */

import SwiftUI

@MainActor
struct RunControlDiagnosticsTextSheet: View {
    let title: String
    let text: String
    let isLoading: Bool
    let loadingTitle: String

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VStack {
                if isLoading {
                    ProgressView(loadingTitle)
                        .padding()
                } else {
                    ScrollView {
                        Text(text)
                            .font(.system(.body, design: .monospaced))
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding()
                    }
                }
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .navigationTitle(title)
        }
        .frame(minWidth: 600, minHeight: 400)
    }
}
