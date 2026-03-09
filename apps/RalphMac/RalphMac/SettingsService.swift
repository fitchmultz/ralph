/**
 SettingsService

 Responsibilities:
 - Define the shared settings service contract used by app command surfaces.
 - Provide the default stub implementation extended by `ASettingsInfra.swift`.

 Does not handle:
 - Settings window controller behavior.
 - Settings UI content.

 Invariants/assumptions callers must respect:
 - Concrete behavior is supplied by an extension in `ASettingsInfra.swift`.
 */

import SwiftUI

@MainActor
enum SettingsService {
    static func initialize() {
        _ = SettingsWindowController.shared
    }

    static func showSettingsWindow() {
        SettingsWindowController.shared.show()
    }
}

@MainActor
struct OpenSettingsButton: View {
    var body: some View {
        Button("Settings...") {
            SettingsService.showSettingsWindow()
        }
        .keyboardShortcut(",", modifiers: .command)
    }
}
