/**
 SettingsService

 Responsibilities:
 - Provide the shared Settings-window entrypoint used by supported command surfaces.
 - Route all Settings opens through one coordinator so workspace context and diagnostics stay unified.

 Does not handle:
 - Settings UI content.
 - Settings state persistence.
 */

import AppKit
import RalphCore

@MainActor
enum SettingsPresentationSource: String {
    case commandSurface = "command-surface"
    case menuBar = "menu-bar"
    case urlScheme = "url-scheme"
}

@MainActor
enum SettingsService {
    private static var presentationTask: Task<Void, Never>?
    private static var presentationRevision: UInt64 = 0

    static func showSettingsWindow(
        for workspace: Workspace? = WorkspaceManager.shared.effectiveWorkspace,
        source: SettingsPresentationSource = .commandSurface
    ) {
        SettingsPresentationCoordinator.shared.prepare(workspace: workspace, source: source)

        presentationTask?.cancel()
        presentationRevision &+= 1
        let revision = presentationRevision
        presentationTask = Task { @MainActor in
            await Task.yield()
            guard !Task.isCancelled else { return }

            if SettingsWindowService.shared.revealOrOpenPreparedWindow() {
                if presentationRevision == revision {
                    presentationTask = nil
                }
                return
            }

            guard MainWindowService.shared.revealOrOpenPrimaryWindow() else {
                if presentationRevision == revision {
                    presentationTask = nil
                }
                return
            }
            await Task.yield()
            guard !Task.isCancelled else { return }
            _ = SettingsWindowService.shared.revealOrOpenPreparedWindow()
            if presentationRevision == revision {
                presentationTask = nil
            }
        }
    }
}
