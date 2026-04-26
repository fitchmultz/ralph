/**
 SettingsWindowDiagnostics

 Purpose:
 - Define settings-window diagnostic snapshots and persistence helpers.

 Responsibilities:
 - Define settings-window diagnostic snapshots and persistence helpers.
 - Coordinate prepared-workspace context, visible-window capture, and content diagnostics.
 - Surface a stable diagnostics payload for smoke-contract validation.

 Does not handle:
 - AppKit focus management.
 - Settings tab rendering.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import AppKit
import RalphCore
import SwiftUI

struct SettingsWindowHelperSnapshot: Codable, Equatable {
    let className: String
    let title: String
    let identifier: String?
}

struct SettingsWindowDiagnosticsSnapshot: Codable, Equatable {
    var requestSequence: Int
    var source: String?
    var requestedWorkspacePath: String?
    var resolvedWorkspacePath: String?
    var contentWorkspacePath: String?
    var settingsRunnerValue: String?
    var settingsModelValue: String?
    var settingsIsLoading: Bool
    var visibleAppWindowCount: Int
    var visibleWorkspaceWindowCount: Int
    var visibleSettingsWindowCount: Int
    var visibleHelperWindowCount: Int
    var helperWindows: [SettingsWindowHelperSnapshot]
    var settingsWindowTitle: String?
    var firstResponderClassName: String?
    var firstResponderIsTextView: Bool
    var settingsWindowIsKey: Bool
    var persistence: ContractDiagnosticsPersistenceStatus

    static let idle = SettingsWindowDiagnosticsSnapshot(
        requestSequence: 0,
        source: nil,
        requestedWorkspacePath: nil,
        resolvedWorkspacePath: nil,
        contentWorkspacePath: nil,
        settingsRunnerValue: nil,
        settingsModelValue: nil,
        settingsIsLoading: false,
        visibleAppWindowCount: 0,
        visibleWorkspaceWindowCount: 0,
        visibleSettingsWindowCount: 0,
        visibleHelperWindowCount: 0,
        helperWindows: [],
        settingsWindowTitle: nil,
        firstResponderClassName: nil,
        firstResponderIsTextView: false,
        settingsWindowIsKey: false,
        persistence: .disabled
    )

    func encodedForAccessibility() -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(self),
              let string = String(data: data, encoding: .utf8)
        else {
            return "{}"
        }
        return string
    }
}

@MainActor
final class SettingsPresentationCoordinator: ObservableObject {
    static let shared = SettingsPresentationCoordinator()
    nonisolated static let contextDidChangeNotification = Notification.Name("com.mitchfultz.ralph.settings.contextDidChange")

    @Published private(set) var workspace: Workspace?
    @Published private(set) var diagnostics = SettingsWindowDiagnosticsSnapshot.idle

    private let diagnosticsFileURL: URL?
    private let persistenceStorage: ContractDiagnosticsPersistenceStorage

    init(
        diagnosticsFileURL: URL?,
        persistenceStorage: ContractDiagnosticsPersistenceStorage = .live
    ) {
        self.diagnosticsFileURL = diagnosticsFileURL
        self.persistenceStorage = persistenceStorage
    }

    private convenience init() {
        self.init(
            diagnosticsFileURL: Self.resolveDiagnosticsFileURL(),
            persistenceStorage: .live
        )
    }

    private static func resolveDiagnosticsFileURL() -> URL? {
        guard let rawPath = ProcessInfo.processInfo.environment["RALPH_SETTINGS_DIAGNOSTICS_PATH"]?
            .trimmingCharacters(in: .whitespacesAndNewlines),
            !rawPath.isEmpty
        else {
            return nil
        }
        return URL(fileURLWithPath: rawPath, isDirectory: false)
    }

    func prepare(workspace: Workspace?, source: SettingsPresentationSource) {
        self.workspace = workspace
        diagnostics = SettingsWindowDiagnosticsSnapshot(
            requestSequence: diagnostics.requestSequence + 1,
            source: source.rawValue,
            requestedWorkspacePath: workspace?.identityState.workingDirectoryURL.path,
            resolvedWorkspacePath: resolvedWorkspacePath(for: workspace),
            contentWorkspacePath: nil,
            settingsRunnerValue: nil,
            settingsModelValue: nil,
            settingsIsLoading: workspace != nil,
            visibleAppWindowCount: diagnostics.visibleAppWindowCount,
            visibleWorkspaceWindowCount: diagnostics.visibleWorkspaceWindowCount,
            visibleSettingsWindowCount: diagnostics.visibleSettingsWindowCount,
            visibleHelperWindowCount: diagnostics.visibleHelperWindowCount,
            helperWindows: diagnostics.helperWindows,
            settingsWindowTitle: diagnostics.settingsWindowTitle,
            firstResponderClassName: diagnostics.firstResponderClassName,
            firstResponderIsTextView: diagnostics.firstResponderIsTextView,
            settingsWindowIsKey: diagnostics.settingsWindowIsKey,
            persistence: diagnostics.persistence
        )
        persistDiagnosticsIfNeeded()
        NotificationCenter.default.post(name: Self.contextDidChangeNotification, object: nil)
        refreshDiagnosticsContentForPreparedWorkspace()
    }

    var contentIdentity: String {
        let workspacePath = workspace?.identityState.workingDirectoryURL.path ?? "no-workspace"
        let workspaceID = workspace?.id.uuidString ?? "no-workspace-id"
        let retargetRevision = workspace.map { String($0.identityState.retargetRevision) } ?? "0"
        return [
            String(diagnostics.requestSequence),
            workspaceID,
            workspacePath,
            retargetRevision,
        ].joined(separator: "|")
    }

    var requiresFreshWindowPresentation: Bool {
        let preparedPath = normalizePath(resolvedWorkspacePath(for: workspace))
        let contentPath = normalizePath(diagnostics.contentWorkspacePath)
        return preparedPath != contentPath
    }

    func capture(window: NSWindow?) {
        let visibleWindows = NSApp.windows.filter(\.isVisible)
        let workspaceWindows = visibleWindows.filter(isWorkspaceWindow)
        let settingsWindows = visibleWindows.filter(isSettingsWindow)
        let helperWindows = visibleWindows.filter { !isWorkspaceWindow($0) && !isSettingsWindow($0) }
        let resolvedSettingsWindow = window.flatMap { isSettingsWindow($0) ? $0 : nil } ?? settingsWindows.first

        diagnostics = SettingsWindowDiagnosticsSnapshot(
            requestSequence: diagnostics.requestSequence,
            source: diagnostics.source,
            requestedWorkspacePath: workspace?.identityState.workingDirectoryURL.path,
            resolvedWorkspacePath: resolvedWorkspacePath(for: workspace),
            contentWorkspacePath: diagnostics.contentWorkspacePath,
            settingsRunnerValue: diagnostics.settingsRunnerValue,
            settingsModelValue: diagnostics.settingsModelValue,
            settingsIsLoading: diagnostics.settingsIsLoading,
            visibleAppWindowCount: visibleWindows.count,
            visibleWorkspaceWindowCount: workspaceWindows.count,
            visibleSettingsWindowCount: settingsWindows.count,
            visibleHelperWindowCount: helperWindows.count,
            helperWindows: helperWindows.map {
                SettingsWindowHelperSnapshot(
                    className: String(describing: type(of: $0)),
                    title: $0.title,
                    identifier: $0.identifier?.rawValue
                )
            },
            settingsWindowTitle: resolvedSettingsWindow?.title,
            firstResponderClassName: resolvedSettingsWindow.flatMap { window in
                window.firstResponder.map { String(describing: type(of: $0)) }
            },
            firstResponderIsTextView: resolvedSettingsWindow?.firstResponder is NSTextView,
            settingsWindowIsKey: resolvedSettingsWindow?.isKeyWindow ?? false,
            persistence: diagnostics.persistence
        )
        persistDiagnosticsIfNeeded()
    }

    func captureContent(
        workspacePath: String?,
        runner: String?,
        model: String?,
        isLoading: Bool
    ) {
        diagnostics.contentWorkspacePath = workspacePath
        diagnostics.settingsRunnerValue = runner
        diagnostics.settingsModelValue = model
        diagnostics.settingsIsLoading = isLoading
        persistDiagnosticsIfNeeded()
    }

    func isSettingsWindow(_ window: NSWindow) -> Bool {
        SettingsWindowService.shared.isSettingsWindow(window)
    }

    private func isWorkspaceWindow(_ window: NSWindow) -> Bool {
        window.identifier?.rawValue.contains("AppWindow") == true
    }

    private func resolvedWorkspacePath(for preparedWorkspace: Workspace?) -> String? {
        (preparedWorkspace ?? WorkspaceManager.shared.effectiveWorkspace)?.identityState.workingDirectoryURL.path
    }

    private func normalizePath(_ path: String?) -> String? {
        guard let path, !path.isEmpty else { return nil }
        return URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL.path
    }

    private func refreshDiagnosticsContentForPreparedWorkspace() {
        guard let preparedWorkspace = workspace else {
            diagnostics.contentWorkspacePath = nil
            diagnostics.settingsRunnerValue = nil
            diagnostics.settingsModelValue = nil
            diagnostics.settingsIsLoading = false
            persistDiagnosticsIfNeeded()
            return
        }

        let preparedDirectoryURL = preparedWorkspace.identityState.workingDirectoryURL
        let preparedPath = preparedDirectoryURL.path
        let configURL = preparedWorkspace.projectConfigFileURL
            ?? preparedDirectoryURL.appendingPathComponent(".ralph/config.jsonc")

        diagnostics.contentWorkspacePath = preparedPath
        diagnostics.settingsRunnerValue = nil
        diagnostics.settingsModelValue = nil
        diagnostics.settingsIsLoading = true

        do {
            let snapshot = try SettingsProjectConfigLoader.loadSynchronously(from: configURL)
            diagnostics.settingsRunnerValue = snapshot.config.agent?.runner
            diagnostics.settingsModelValue = snapshot.config.agent?.model
        } catch {
            diagnostics.settingsRunnerValue = nil
            diagnostics.settingsModelValue = nil
        }

        diagnostics.settingsIsLoading = false
        persistDiagnosticsIfNeeded()
    }

    private func persistDiagnosticsIfNeeded() {
        diagnostics.persistence = ContractDiagnosticsPersistence.persist(
            snapshot: diagnostics,
            diagnosticsFileURL: diagnosticsFileURL,
            storage: persistenceStorage,
            diagnosticsType: "settings",
            applyStatus: { snapshot, status in
                snapshot.persistence = status
            }
        )
    }
}
