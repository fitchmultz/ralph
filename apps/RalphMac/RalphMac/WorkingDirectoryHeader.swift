/**
 WorkingDirectoryHeader

 Purpose:
 - Display workspace name and working directory path.

 Responsibilities:
 - Display workspace name and working directory path.
 - Provide recents menu and directory chooser button.
 - Shared component used across multiple sections.

 Does not handle:
 - Directory selection logic (delegated to Workspace).
 - Navigation or routing decisions.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Workspace is injected via @ObservedObject.
 - Should be placed within a VStack or similar container.
 */

import SwiftUI
import RalphCore

@MainActor
struct WorkingDirectoryHeader: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 4) {
                Text(workspace.identityState.name)
                    .font(.headline)
                    .accessibilityLabel("Workspace: \(workspace.identityState.name)")
                Text(workspace.identityState.workingDirectoryURL.path)
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .accessibilityLabel("Working directory: \(workspace.identityState.workingDirectoryURL.path)")
            }

            Spacer()

            if !workspace.identityState.recentWorkingDirectories.isEmpty {
                Menu("Recents") {
                    ForEach(workspace.identityState.recentWorkingDirectories, id: \.path) { url in
                        Button(url.path) {
                            workspace.selectRecentWorkingDirectory(url)
                        }
                    }
                }
            }

            Button("Choose…") {
                workspace.chooseWorkingDirectory()
            }
        }
    }
}
