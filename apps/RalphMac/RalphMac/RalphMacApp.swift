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

    let manager = WorkspaceManager.shared
    @State private var menuBarManager = MenuBarManager.shared
    @State private var uiTestingMenuBarVisible = false
    let isUITesting = ProcessInfo.processInfo.arguments.contains("--uitesting")

    init() {
        CrashReporter.shared.install()
    }

    var body: some Scene {
        WindowGroup(id: "main") {
            WindowViewContainer()
                .background(
                    VisualEffectView(material: .windowBackground, blendingMode: .behindWindow)
                        .ignoresSafeArea()
                )
                .onOpenURL(perform: handleOpenURL)
        }
        .handlesExternalEvents(matching: ["ralph"])
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
            AppHelpCommands(
                exportLogsAction: exportLogs,
                showCrashReportsAction: showCrashReports
            )
            AppSettingsCommands()
        }

        MenuBarExtra(
            isInserted: menuBarVisibilityBinding,
            content: { MenuBarContentView() },
            label: { MenuBarIconView() }
        )
        .menuBarExtraStyle(.menu)
    }

    private var menuBarVisibilityBinding: Binding<Bool> {
        if isUITesting {
            return $uiTestingMenuBarVisible
        }
        return $menuBarManager.isMenuBarExtraVisible
    }
}
