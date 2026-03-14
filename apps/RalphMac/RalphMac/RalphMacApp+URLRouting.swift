/**
 RalphMacApp+URLRouting

 Responsibilities:
 - Handle incoming `ralph://open` URLs and route or create workspaces.
 - Reuse bootstrap workspaces when the app launches into a placeholder workspace.

 Does not handle:
 - Command menu wiring.
 - Window bootstrap mechanics.

 Invariants/assumptions callers must respect:
 - Only `ralph://open?workspace=...` URLs are supported.
 - URL-provided CLI overrides are always rejected.
 */

import AppKit
import Foundation
import RalphCore

@MainActor
enum RalphURLRouter {
    static func handle(_ url: URL) {
        guard url.scheme == "ralph" else {
            RalphLogger.shared.info("Received URL with unexpected scheme: \(url.scheme ?? "nil")", category: .lifecycle)
            return
        }

        if url.host == "settings" {
            SettingsService.showSettingsWindow(source: .urlScheme)
            RalphLogger.shared.info("Opened settings via ralph://settings", category: .lifecycle)
            return
        }

        guard url.host == "open" else {
            RalphLogger.shared.info("Received ralph:// URL with unexpected host: \(url.host ?? "nil")", category: .lifecycle)
            return
        }

        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: true),
              let queryItems = components.queryItems,
              let workspaceItem = queryItems.first(where: { $0.name == "workspace" }),
              let encodedPath = workspaceItem.value,
              let path = encodedPath.removingPercentEncoding else {
            RalphLogger.shared.info("Received ralph://open URL without valid workspace parameter", category: .lifecycle)
            return
        }

        if queryItems.contains(where: { $0.name == "cli" }) {
            RalphLogger.shared.error(
                "Ignoring deprecated insecure cli= URL parameter",
                category: .cli
            )
        }

        openWorkspace(at: URL(fileURLWithPath: path, isDirectory: true))
    }

    static func openWorkspace(at rawWorkspaceURL: URL) {
        let workspaceURL = Workspace.normalizedWorkingDirectoryURL(rawWorkspaceURL)
        let path = workspaceURL.path

        var isDir: ObjCBool = false
        let exists = FileManager.default.fileExists(atPath: path, isDirectory: &isDir)
        guard exists && isDir.boolValue else {
            RalphLogger.shared.error("Workspace path does not exist or is not a directory: \(path)", category: .workspace)
            return
        }

        if let existingWorkspace = WorkspaceManager.shared.workspaces.first(where: { $0.matchesWorkingDirectory(workspaceURL) }) {
            revealWorkspaceAfterEnsuringWindow(existingWorkspace.id)
            RalphLogger.shared.info("Activated existing workspace: \(path)", category: .workspace)
            return
        }

        if let bootstrapWorkspace = bootstrapWorkspaceForURLOpen() {
            closeOtherBootstrapPlaceholders(except: bootstrapWorkspace.id)
            bootstrapWorkspace.setWorkingDirectory(workspaceURL)
            revealWorkspaceAfterEnsuringWindow(bootstrapWorkspace.id)
            RalphLogger.shared.info("Repurposed bootstrap workspace for URL: \(path)", category: .workspace)
            return
        }

        let workspace = WorkspaceManager.shared.createWorkspace(workingDirectory: workspaceURL)
        revealWorkspaceAfterEnsuringWindow(workspace.id)
        RalphLogger.shared.info("Created new workspace from URL: \(path)", category: .workspace)
    }

    static func bootstrapWorkspaceForURLOpen() -> Workspace? {
        let manager = WorkspaceManager.shared
        let placeholderWorkspaces = manager.workspaces.filter(\.isURLRoutingPlaceholderWorkspace)
        guard !placeholderWorkspaces.isEmpty else { return nil }

        if let registeredWorkspaceID = WorkspaceWindowRegistry.shared.preferredActiveWorkspaceID(),
           let registeredWorkspace = placeholderWorkspaces.first(where: { $0.id == registeredWorkspaceID }) {
            return registeredWorkspace
        }

        if let focusedWorkspace = manager.focusedWorkspace,
           placeholderWorkspaces.contains(where: { $0.id == focusedWorkspace.id }) {
            return focusedWorkspace
        }

        if let effectiveWorkspace = manager.effectiveWorkspace,
           placeholderWorkspaces.contains(where: { $0.id == effectiveWorkspace.id }) {
            return effectiveWorkspace
        }

        if let onlyVisiblePlaceholder = placeholderWorkspaces.first(where: { workspace in
            workspace.id == manager.lastActiveWorkspaceID
        }) {
            return onlyVisiblePlaceholder
        }

        guard placeholderWorkspaces.count == 1 else { return nil }
        return placeholderWorkspaces[0]
    }

    private static func closeOtherBootstrapPlaceholders(except workspaceID: UUID) {
        let manager = WorkspaceManager.shared
        let duplicatePlaceholders = manager.workspaces.filter {
            $0.id != workspaceID && $0.isURLRoutingPlaceholderWorkspace
        }
        for workspace in duplicatePlaceholders {
            manager.closeWorkspace(workspace)
        }
    }

    private static func revealWorkspaceAfterEnsuringWindow(_ workspaceID: UUID) {
        MainWindowService.shared.revealOrOpenPrimaryWindow()
        WorkspaceManager.shared.scheduleWorkspaceReveal(workspaceID)
        NSApp.activate(ignoringOtherApps: true)
    }
}
