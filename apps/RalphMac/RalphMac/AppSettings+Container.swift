/**
 AppSettings+Container

 Purpose:
 - Provide preview and workspace/no-workspace container shells for the Settings scene.

 Responsibilities:
 - Provide preview and workspace/no-workspace container shells for the Settings scene.
 - Keep scene-content indirection out of the root settings view file.

 Does not handle:
 - Settings tab layouts.
 - Settings window diagnostics or focus management.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

#Preview {
    SettingsView(
        workspace: Workspace(
            workingDirectoryURL: URL(fileURLWithPath: "/Users/example/project")
        ),
        presentationToken: "preview"
    )
}

/// Container view referenced from `SettingsSceneRoot` using a stable wrapper so the settings scene can
/// swap prepared workspace context without embedding the selection logic in the root view file.
@MainActor
struct SettingsContentContainer: View {
    let workspace: Workspace?
    let presentationToken: String

    var body: some View {
        Group {
            if let workspace {
                SettingsView(workspace: workspace, presentationToken: presentationToken)
                    .id(presentationToken)
            } else {
                NoWorkspaceSettingsView()
            }
        }
    }
}

@MainActor
struct NoWorkspaceSettingsView: View {
    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "gearshape.2")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)

            Text("No Workspace Available")
                .font(.headline)

            Text("Open a workspace to configure settings.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }
}
