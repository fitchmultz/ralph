/**
 RalphMacApp

 Responsibilities:
 - Define the macOS SwiftUI app entry point and scene graph.
 - Wire the shared workspace manager, menu bar, and app-level command surfaces together.

 Does not handle:
 - URL routing implementation details.
 - Window bootstrap or UI-testing window policy.

 Invariants/assumptions callers must respect:
 - App support behaviors are implemented in adjacent `RalphMacApp+...` files and related app helpers.
 */

import SwiftUI
import AppKit
import RalphCore

@MainActor
@main
struct RalphMacApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    @StateObject private var appearance = AppAppearanceController.shared

    init() {
        _ = RalphAppDefaults.prepareForLaunch()
        CrashReporter.shared.install()
    }

    var body: some Scene {
        WindowGroup(id: "main") {
            WindowViewContainer()
                .background(
                    VisualEffectView(material: .windowBackground, blendingMode: .behindWindow)
                        .ignoresSafeArea()
                )
                .preferredColorScheme(appearance.preferredColorScheme)
                .background(MainWindowOpenActionRegistrar())
        }
        .restorationBehavior(.disabled)
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unified(showsTitle: false))
        .defaultSize(width: 1400, height: 900)
        .defaultPosition(.center)
        .commands {
            WorkspaceCommands()
            NavigationCommands()
            TaskCommands()
            CommandPaletteCommands()
            AppSettingsCommands()
            AppHelpCommands(
                exportLogsAction: exportLogs,
                showCrashReportsAction: showCrashReports
            )
        }
    }
}
