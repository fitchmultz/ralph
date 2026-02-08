/**
 RalphMacApp

 Responsibilities:
 - Define the macOS SwiftUI app entry point.
 - Configure multi-window support with native macOS tab bar integration.
 - Handle window restoration on app relaunch.
 - Provide menu commands for window/tab management.

 Does not handle:
 - Individual workspace content or CLI operations (see Workspace and WindowView).

 Invariants/assumptions callers must respect:
 - The app bundle includes an executable named `ralph` placed alongside the app binary.
 - Window restoration state is stored in UserDefaults.
 */

public import SwiftUI
import RalphCore

@main
struct RalphMacApp: App {
    @StateObject private var manager = WorkspaceManager.shared
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            WindowView(windowState: WindowState(workspaceIDs: [manager.createWorkspace().id]))
                .background(
                    VisualEffectView(material: .windowBackground, blendingMode: .behindWindow)
                        .ignoresSafeArea()
                )
        }
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unified(showsTitle: false))
        .defaultSize(width: 1200, height: 800)
        .defaultPosition(.center)
        
        .commands {
            workspaceCommands
        }
    }

    private var workspaceCommands: some Commands {
        CommandMenu("Workspace") {
            Button("New Tab") {
                NotificationCenter.default.post(
                    name: .newWorkspaceTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("t", modifiers: .command)

            Button("New Window") {
                NotificationCenter.default.post(
                    name: .newWindowRequested,
                    object: nil
                )
            }
            .keyboardShortcut("n", modifiers: [.command, .shift])

            Divider()

            Button("Close Tab") {
                NotificationCenter.default.post(
                    name: .closeActiveTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("w", modifiers: .command)

            Button("Close Window") {
                NotificationCenter.default.post(
                    name: .closeActiveWindowRequested,
                    object: nil
                )
            }
            .keyboardShortcut("w", modifiers: [.command, .shift])

            Divider()

            Button("Next Tab") {
                NotificationCenter.default.post(
                    name: .selectNextTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("]", modifiers: [.command, .shift])

            Button("Previous Tab") {
                NotificationCenter.default.post(
                    name: .selectPreviousTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("[", modifiers: [.command, .shift])

            Divider()

            Button("Duplicate Tab") {
                NotificationCenter.default.post(
                    name: .duplicateActiveTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("d", modifiers: .command)
        }
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let newWorkspaceTabRequested = Notification.Name("newWorkspaceTabRequested")
    static let newWindowRequested = Notification.Name("newWindowRequested")
    static let closeActiveTabRequested = Notification.Name("closeActiveTabRequested")
    static let closeActiveWindowRequested = Notification.Name("closeActiveWindowRequested")
    static let selectNextTabRequested = Notification.Name("selectNextTabRequested")
    static let selectPreviousTabRequested = Notification.Name("selectPreviousTabRequested")
    static let duplicateActiveTabRequested = Notification.Name("duplicateActiveTabRequested")
}
