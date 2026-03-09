/**
 RalphMacApp+URLRouting

 Responsibilities:
 - Handle incoming `ralph://open` URLs and route or create workspaces.
 - Reuse bootstrap workspaces when the app launches into a placeholder home directory.

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

extension RalphMacApp {
    func handleOpenURL(_ url: URL) {
        guard url.scheme == "ralph" else {
            RalphLogger.shared.info("Received URL with unexpected scheme: \(url.scheme ?? "nil")", category: .lifecycle)
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

        let workspaceURL = URL(fileURLWithPath: path, isDirectory: true)
            .standardizedFileURL
            .resolvingSymlinksInPath()

        var isDir: ObjCBool = false
        let exists = FileManager.default.fileExists(atPath: path, isDirectory: &isDir)
        guard exists && isDir.boolValue else {
            RalphLogger.shared.error("Workspace path does not exist or is not a directory: \(path)", category: .workspace)
            return
        }

        if let existingWorkspace = manager.workspaces.first(where: {
            $0.identityState.workingDirectoryURL
                .standardizedFileURL
                .resolvingSymlinksInPath()
                .path == workspaceURL.path
        }) {
            manager.revealWorkspace(existingWorkspace.id)
            NSApp.activate(ignoringOtherApps: true)
            RalphLogger.shared.info("Activated existing workspace: \(path)", category: .workspace)
            return
        }

        if let bootstrapWorkspace = bootstrapWorkspaceForURLOpen() {
            bootstrapWorkspace.setWorkingDirectory(workspaceURL)
            manager.revealWorkspace(bootstrapWorkspace.id)
            NSApp.activate(ignoringOtherApps: true)
            RalphLogger.shared.info("Repurposed bootstrap workspace for URL: \(path)", category: .workspace)
            return
        }

        let workspace = manager.createWorkspace(workingDirectory: workspaceURL)
        manager.revealWorkspace(workspace.id)
        NSApp.activate(ignoringOtherApps: true)
        RalphLogger.shared.info("Created new workspace from URL: \(path)", category: .workspace)
    }

    func bootstrapWorkspaceForURLOpen() -> Workspace? {
        guard manager.workspaces.count == 1, let workspace = manager.workspaces.first else { return nil }

        let homePath = FileManager.default.homeDirectoryForCurrentUser
            .standardizedFileURL
            .resolvingSymlinksInPath()
            .path
        let workspacePath = workspace.identityState.workingDirectoryURL
            .standardizedFileURL
            .resolvingSymlinksInPath()
            .path
        guard workspacePath == homePath else { return nil }
        guard !workspace.hasRalphQueueFile else { return nil }

        return workspace
    }
}
