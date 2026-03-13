/**
 SettingsService

 Responsibilities:
 - Provide the shared settings-window entrypoint used by app command surfaces.
 - Route settings opens through the dedicated `SettingsWindowController`.

 Does not handle:
 - Settings UI content.
 - Settings state persistence.
 */

import AppKit
import RalphCore

@MainActor
enum SettingsPresentationSource {
    case appMenu
    case keyboardShortcut
    case menuBar
}

@MainActor
enum SettingsService {
    static func showSettingsWindow(
        for workspace: Workspace? = WorkspaceManager.shared.effectiveWorkspace,
        source _: SettingsPresentationSource = .appMenu
    ) {
        SettingsPresentationCoordinator.shared.prepare(workspace: workspace)

        Task { @MainActor in
            await Task.yield()
            guard !Task.isCancelled else { return }
            NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
            NSApp.activate(ignoringOtherApps: true)
        }
    }
}
