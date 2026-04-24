/**
 SettingsSceneRoot

 Purpose:
 - Compose the prepared workspace, diagnostics overlay, and preferred appearance into the Settings scene root.

 Responsibilities:
 - Compose the prepared workspace, diagnostics overlay, and preferred appearance into the Settings scene root.
 - Refresh scene identity when prepared settings context changes.

 Does not handle:
 - Window creation or reveal timing.
 - Settings tab content definitions.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import RalphCore
import SwiftUI

@MainActor
struct SettingsSceneRoot: View {
    @StateObject private var presentation = SettingsPresentationCoordinator.shared
    @State private var refreshToken = UUID()

    var body: some View {
        let workspace = presentation.workspace ?? WorkspaceManager.shared.effectiveWorkspace

        SettingsContentContainer(
            workspace: workspace,
            presentationToken: presentation.contentIdentity
        )
            .id("\(refreshToken.uuidString)|\(presentation.contentIdentity)")
            .frame(minWidth: 640, minHeight: 480)
            .preferredColorScheme(AppAppearanceController.shared.preferredColorScheme)
            .background(SettingsWindowFocusAnchor())
            .overlay(alignment: .bottomTrailing) {
                SettingsDiagnosticsAccessibilityProbe(snapshot: presentation.diagnostics)
            }
            .onReceive(NotificationCenter.default.publisher(for: SettingsPresentationCoordinator.contextDidChangeNotification)) { _ in
                refreshToken = UUID()
            }
    }
}
