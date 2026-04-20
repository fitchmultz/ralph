/**
 SettingsSmokeContractRunner

 Responsibilities:
 - Run the noninteractive macOS Settings smoke contract inside the app process.
 - Keep contract-mode window presentation offscreen so local verification does not steal focus.
 - Write a machine-readable report for `scripts/macos-settings-smoke.sh` and exit explicitly.

 Does not handle:
 - Interactive UI automation.
 - General app launch policy outside Settings contract mode.
 - Settings UI content or state persistence.

 Invariants/assumptions callers must respect:
 - Contract mode is enabled only via `--settings-smoke-contract`.
 - The script provides disposable workspace A/B paths and a report path via environment variables.
 - Contract-mode windows may be ordered and keyed for AppKit correctness, but they must stay offscreen and never call `NSApp.activate(...)`.
 */

import AppKit
import Darwin
import Foundation
import RalphCore

@MainActor
struct RalphMacPresentationRuntime {
    static var suppressUserActivation: Bool {
        RalphAppDefaults.isMacOSContract
    }

    static var activationPolicy: NSApplication.ActivationPolicy {
        suppressUserActivation ? .accessory : .regular
    }

    static func reveal(_ window: NSWindow, center: Bool = false) {
        if suppressUserActivation {
            moveOffscreen(window)
            window.orderFront(nil)
            window.makeMain()
            window.makeKey()
            return
        }

        if center {
            window.center()
        }
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    static func applyRevealFrame(_ frame: NSRect, to window: NSWindow) {
        if suppressUserActivation {
            window.setFrame(offscreenFrame(for: frame.size), display: true)
            window.orderFront(nil)
            window.makeMain()
            window.makeKey()
            return
        }

        window.setFrame(frame, display: true)
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    static func activateApplicationIfAllowed() {
        guard !suppressUserActivation else { return }
        NSApp.activate(ignoringOtherApps: true)
    }

    private static func moveOffscreen(_ window: NSWindow) {
        window.setFrame(offscreenFrame(for: window.frame.size), display: true)
    }

    private static func offscreenFrame(for size: NSSize) -> NSRect {
        let availableScreens = NSScreen.screens
        let minimumX = availableScreens.map { $0.frame.minX }.min() ?? 0
        let maximumY = availableScreens.map { $0.visibleFrame.maxY }.max() ?? 900
        let width = max(size.width, 760)
        let height = max(size.height, 520)
        return NSRect(
            x: minimumX - width - 240,
            y: maximumY - height - 80,
            width: width,
            height: height
        )
    }
}

@MainActor
private struct SettingsSmokeContractStepReport: Codable {
    let name: String
    let snapshot: SettingsWindowDiagnosticsSnapshot
}

private struct SettingsSmokeContractReport: Codable {
    let passed: Bool
    let runtimeMode: String
    let workspaceAPath: String
    let workspaceBPath: String
    let steps: [SettingsSmokeContractStepReport]
    let failureMessage: String?
}

private struct SettingsSmokeContractFailure: LocalizedError {
    let message: String

    var errorDescription: String? {
        message
    }
}

@MainActor
final class SettingsSmokeContractRunner {
    static let shared = SettingsSmokeContractRunner()

    private enum EnvironmentKey {
        static let workspaceA = "RALPH_SETTINGS_SMOKE_WORKSPACE_A"
        static let workspaceB = "RALPH_SETTINGS_SMOKE_WORKSPACE_B"
        static let reportPath = "RALPH_SETTINGS_SMOKE_REPORT_PATH"
    }

    private struct Configuration {
        let workspaceAURL: URL
        let workspaceBURL: URL
        let reportURL: URL

        var workspaceAPath: String {
            Self.normalizedPath(workspaceAURL)
        }

        var workspaceBPath: String {
            Self.normalizedPath(workspaceBURL)
        }

        static func loadFromEnvironment(
            _ environment: [String: String] = ProcessInfo.processInfo.environment
        ) throws -> Self {
            func requiredURL(_ key: String, directory: Bool) throws -> URL {
                guard let rawValue = environment[key]?.trimmingCharacters(in: .whitespacesAndNewlines),
                      !rawValue.isEmpty else {
                    throw SettingsSmokeContractFailure(message: "Missing required environment variable: \(key)")
                }
                return URL(fileURLWithPath: rawValue, isDirectory: directory)
                    .standardizedFileURL
                    .resolvingSymlinksInPath()
            }

            return Configuration(
                workspaceAURL: try requiredURL(EnvironmentKey.workspaceA, directory: true),
                workspaceBURL: try requiredURL(EnvironmentKey.workspaceB, directory: true),
                reportURL: try requiredURL(EnvironmentKey.reportPath, directory: false)
            )
        }

        private static func normalizedPath(_ url: URL) -> String {
            url.standardizedFileURL.resolvingSymlinksInPath().path
        }
    }

    private var contractTask: Task<Void, Never>?

    private init() {}

    func configureIfNeeded() {
        guard RalphAppDefaults.isSettingsSmokeContract else { return }
        guard contractTask == nil else { return }

        contractTask = Task { @MainActor [weak self] in
            guard let self else { return }
            await self.runAndExit()
        }
    }

    private func runAndExit() async {
        let configuration: Configuration
        do {
            configuration = try Configuration.loadFromEnvironment()
        } catch {
            fputs("Settings smoke contract misconfigured: \(error.localizedDescription)\n", stderr)
            Darwin.exit(1)
        }

        do {
            let report = try await runContract(configuration)
            try writeReport(report, to: configuration.reportURL)
            print("Settings smoke contract passed.")
            Darwin.exit(0)
        } catch {
            let failureReport = SettingsSmokeContractReport(
                passed: false,
                runtimeMode: "settings-smoke-contract",
                workspaceAPath: configuration.workspaceAPath,
                workspaceBPath: configuration.workspaceBPath,
                steps: [],
                failureMessage: error.localizedDescription
            )
            try? writeReport(failureReport, to: configuration.reportURL)
            fputs("Settings smoke contract failed: \(error.localizedDescription)\n", stderr)
            Darwin.exit(1)
        }
    }

    private func runContract(_ configuration: Configuration) async throws -> SettingsSmokeContractReport {
        try await waitForInitialWorkspace(path: configuration.workspaceAPath)

        var steps: [SettingsSmokeContractStepReport] = []

        SettingsService.showSettingsWindow(
            for: WorkspaceManager.shared.effectiveWorkspace,
            source: .commandSurface
        )
        let keyboardSnapshot = try await waitForSnapshot(
            stepName: "keyboard-shortcut",
            expectedSequence: 1,
            expectedSource: SettingsPresentationSource.commandSurface,
            expectedWorkspacePath: configuration.workspaceAPath,
            expectedRunner: nil,
            expectedModel: nil
        )
        steps.append(SettingsSmokeContractStepReport(name: "keyboard-shortcut", snapshot: keyboardSnapshot))

        SettingsService.showSettingsWindow(
            for: WorkspaceManager.shared.effectiveWorkspace,
            source: .commandSurface
        )
        let appMenuSnapshot = try await waitForSnapshot(
            stepName: "app-menu",
            expectedSequence: 2,
            expectedSource: SettingsPresentationSource.commandSurface,
            expectedWorkspacePath: configuration.workspaceAPath,
            expectedRunner: nil,
            expectedModel: nil
        )
        steps.append(SettingsSmokeContractStepReport(name: "app-menu", snapshot: appMenuSnapshot))

        RalphURLRouter.handle(workspaceOpenURL(for: configuration.workspaceBURL))
        try await waitForWorkspaceRetarget(path: configuration.workspaceBPath)

        RalphURLRouter.handle(URL(string: "ralph://settings")!)
        let urlSnapshot = try await waitForSnapshot(
            stepName: "url-scheme",
            expectedSequence: 3,
            expectedSource: SettingsPresentationSource.urlScheme,
            expectedWorkspacePath: configuration.workspaceBPath,
            expectedRunner: "gemini",
            expectedModel: "gemini-1.5-pro"
        )
        steps.append(SettingsSmokeContractStepReport(name: "url-scheme", snapshot: urlSnapshot))

        try await verifyProjectConfigSavePreservesLiteralSlashes(workspaceURL: configuration.workspaceBURL)

        return SettingsSmokeContractReport(
            passed: true,
            runtimeMode: "settings-smoke-contract",
            workspaceAPath: configuration.workspaceAPath,
            workspaceBPath: configuration.workspaceBPath,
            steps: steps,
            failureMessage: nil
        )
    }

    private func waitForInitialWorkspace(path expectedPath: String) async throws {
        try await waitForCondition(
            description: "initial workspace bootstrap for \(expectedPath)",
            timeout: 20
        ) {
            let currentPath = Self.normalizedPath(
                WorkspaceManager.shared.effectiveWorkspace?.identityState.workingDirectoryURL.path
            )
            return currentPath == expectedPath && self.visibleWorkspaceWindowCount() == 1
        }
    }

    private func waitForWorkspaceRetarget(path expectedPath: String) async throws {
        try await waitForCondition(
            description: "URL-open retarget to \(expectedPath)",
            timeout: 20
        ) {
            let currentPath = Self.normalizedPath(
                WorkspaceManager.shared.effectiveWorkspace?.identityState.workingDirectoryURL.path
            )
            return currentPath == expectedPath
        }
    }

    private func waitForSnapshot(
        stepName: String,
        expectedSequence: Int,
        expectedSource: SettingsPresentationSource,
        expectedWorkspacePath: String,
        expectedRunner: String?,
        expectedModel: String?
    ) async throws -> SettingsWindowDiagnosticsSnapshot {
        let deadline = Date().addingTimeInterval(25)
        var lastSnapshot = SettingsPresentationCoordinator.shared.diagnostics
        var lastFailures = ["snapshot not captured yet"]

        while Date() < deadline {
            lastSnapshot = SettingsPresentationCoordinator.shared.diagnostics
            lastFailures = snapshotFailures(
                lastSnapshot,
                expectedSequence: expectedSequence,
                expectedSource: expectedSource,
                expectedWorkspacePath: expectedWorkspacePath,
                expectedRunner: expectedRunner,
                expectedModel: expectedModel
            )
            if lastFailures.isEmpty {
                return lastSnapshot
            }
            try? await Task.sleep(nanoseconds: 50_000_000)
        }

        throw SettingsSmokeContractFailure(
            message: "Timed out waiting for \(stepName) Settings snapshot. Failures: \(lastFailures.joined(separator: "; ")). Last snapshot: \(lastSnapshot.encodedForAccessibility())"
        )
    }

    private func snapshotFailures(
        _ snapshot: SettingsWindowDiagnosticsSnapshot,
        expectedSequence: Int,
        expectedSource: SettingsPresentationSource,
        expectedWorkspacePath: String,
        expectedRunner: String?,
        expectedModel: String?
    ) -> [String] {
        var failures: [String] = []

        if snapshot.requestSequence != expectedSequence {
            failures.append("requestSequence=\(snapshot.requestSequence) expected \(expectedSequence)")
        }
        if snapshot.source != expectedSource.rawValue {
            failures.append("source=\(snapshot.source ?? "nil") expected \(expectedSource.rawValue)")
        }
        if Self.normalizedPath(snapshot.resolvedWorkspacePath) != expectedWorkspacePath {
            failures.append("resolvedWorkspacePath=\(snapshot.resolvedWorkspacePath ?? "nil") expected \(expectedWorkspacePath)")
        }
        if Self.normalizedPath(snapshot.contentWorkspacePath) != expectedWorkspacePath {
            failures.append("contentWorkspacePath=\(snapshot.contentWorkspacePath ?? "nil") expected \(expectedWorkspacePath)")
        }
        if snapshot.visibleAppWindowCount != 2 {
            failures.append("visibleAppWindowCount=\(snapshot.visibleAppWindowCount) expected 2")
        }
        if snapshot.visibleWorkspaceWindowCount != 1 {
            failures.append("visibleWorkspaceWindowCount=\(snapshot.visibleWorkspaceWindowCount) expected 1")
        }
        if snapshot.visibleSettingsWindowCount != 1 {
            failures.append("visibleSettingsWindowCount=\(snapshot.visibleSettingsWindowCount) expected 1")
        }
        if snapshot.visibleHelperWindowCount != 0 {
            failures.append("visibleHelperWindowCount=\(snapshot.visibleHelperWindowCount) expected 0")
        }
        if snapshot.firstResponderIsTextView {
            failures.append("firstResponderIsTextView should be false")
        }
        if snapshot.settingsIsLoading {
            failures.append("settingsIsLoading should be false")
        }
        if snapshot.settingsWindowTitle != SettingsWindowIdentity.title {
            failures.append("settingsWindowTitle=\(snapshot.settingsWindowTitle ?? "nil") expected \(SettingsWindowIdentity.title)")
        }
        if let expectedRunner, snapshot.settingsRunnerValue != expectedRunner {
            failures.append("settingsRunnerValue=\(snapshot.settingsRunnerValue ?? "nil") expected \(expectedRunner)")
        }
        if let expectedModel, snapshot.settingsModelValue != expectedModel {
            failures.append("settingsModelValue=\(snapshot.settingsModelValue ?? "nil") expected \(expectedModel)")
        }

        return failures
    }

    private func verifyProjectConfigSavePreservesLiteralSlashes(workspaceURL: URL) async throws {
        guard let workspace = WorkspaceManager.shared.effectiveWorkspace,
              workspace.matchesWorkingDirectory(workspaceURL) else {
            throw SettingsSmokeContractFailure(
                message: "Cannot verify Settings save serialization because workspace B is not active"
            )
        }

        let viewModel = SettingsViewModel(workspace: workspace)
        await viewModel.loadConfig()
        if let errorMessage = viewModel.errorMessage {
            throw SettingsSmokeContractFailure(
                message: "Failed to load workspace B Settings config before save probe: \(errorMessage)"
            )
        }

        viewModel.model = "zai/glm-5.1"
        await viewModel.saveConfig()
        if let errorMessage = viewModel.errorMessage {
            throw SettingsSmokeContractFailure(
                message: "Failed to save workspace B Settings config during slash serialization probe: \(errorMessage)"
            )
        }

        let configURL = workspace.projectConfigFileURL
            ?? workspaceURL.appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
        let savedConfig = try String(contentsOf: configURL, encoding: .utf8)
        if savedConfig.contains(#"\/"#) {
            throw SettingsSmokeContractFailure(
                message: "Settings save escaped forward slashes in \(configURL.path)"
            )
        }
        guard savedConfig.contains(#""model" : "zai/glm-5.1""#)
                || savedConfig.contains(#""model": "zai/glm-5.1""#) else {
            throw SettingsSmokeContractFailure(
                message: "Settings save did not persist slash-bearing model in \(configURL.path)"
            )
        }
    }

    private func waitForCondition(
        description: String,
        timeout: TimeInterval,
        condition: @escaping @MainActor () -> Bool
    ) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if condition() {
                return
            }
            try? await Task.sleep(nanoseconds: 50_000_000)
        }

        throw SettingsSmokeContractFailure(
            message: "Timed out waiting for \(description). Last diagnostics: \(SettingsPresentationCoordinator.shared.diagnostics.encodedForAccessibility())"
        )
    }

    private func visibleWorkspaceWindowCount() -> Int {
        NSApp.windows.filter {
            $0.isVisible && ($0.identifier?.rawValue.contains("AppWindow") == true)
        }.count
    }

    private func workspaceOpenURL(for workspaceURL: URL) -> URL {
        var components = URLComponents()
        components.scheme = "ralph"
        components.host = "open"
        components.queryItems = [
            URLQueryItem(name: "workspace", value: workspaceURL.path)
        ]
        return components.url!
    }

    private func writeReport(_ report: SettingsSmokeContractReport, to url: URL) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(report)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true,
            attributes: nil
        )
        try data.write(to: url, options: .atomic)
    }

    private static func normalizedPath(_ path: String?) -> String? {
        guard let path, !path.isEmpty else { return nil }
        return URL(fileURLWithPath: path, isDirectory: true)
            .standardizedFileURL
            .resolvingSymlinksInPath()
            .path
    }
}
