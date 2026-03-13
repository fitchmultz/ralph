/**
 SettingsViewModel

 Responsibilities:
 - Load and cache current configuration from CLI.
 - Provide @Published properties for Settings UI binding.
 - Debounce and persist config changes via CLI.
 - Handle per-workspace config (project .ralph/config.jsonc).

 Does not handle:
 - Settings view layout (see SettingsView).
 - Global config editing (only project config for now).

 Invariants/assumptions callers must respect:
 - Must be created on MainActor.
 - Changes are debounced with 500ms delay before persisting.
 - Programmatic config loads suppress user-change side effects.
 */

import SwiftUI
import RalphCore

@MainActor
@Observable
final class SettingsViewModel {
    // MARK: - Runner Settings
    var runner: String = "codex"
    var model: String = "gpt-5.4"
    var phases: Int = 3
    var iterations: Int = 1
    var reasoningEffort: String = "medium"

    // MARK: - Notification Settings
    var notificationsEnabled: Bool = true
    var notifyOnComplete: Bool = true
    var notifyOnFail: Bool = true
    var notifyOnLoopComplete: Bool = true
    var soundEnabled: Bool = false
    var suppressWhenActive: Bool = true

    // MARK: - UI State
    var isLoading: Bool = false
    var errorMessage: String?
    var hasUnsavedChanges: Bool = false

    // MARK: - Private
    private let workspace: Workspace
    private var saveTask: Task<Void, Never>?
    private var client: RalphCLIClient? { WorkspaceManager.shared.client }
    private var loadedConfigDict: [String: Any] = [:]
    private var hasLoadedConfig = false
    private var isApplyingLoadedValues = false

    // MARK: - Constants
    let availableRunners = ConfigRunner.allCases
    let availablePhases = ConfigPhases.allCases
    let availableEfforts = ConfigReasoningEffort.allCases

    // Common model options per runner
    let commonModels: [String: [String]] = [
        "claude": ["sonnet", "opus", "haiku"],
        "codex": ["gpt-5.4", "gpt-5.3-codex", "gpt-5.3-codex-spark", "gpt-5.3"],
        "opencode": ["default"],
        "gemini": ["gemini-2.0-flash", "gemini-1.5-pro"],
        "cursor": ["default"],
        "kimi": ["kimi-code/kimi-for-coding"],
        "pi": ["default"]
    ]

    // MARK: - Initialization

    init(workspace: Workspace) {
        self.workspace = workspace
    }

    // MARK: - Loading

    func loadConfigIfNeeded() async {
        guard !hasLoadedConfig else { return }
        await loadConfig()
    }

    func loadConfig() async {
        isLoading = true
        errorMessage = nil

        guard let client else {
            errorMessage = "CLI not available"
            isLoading = false
            return
        }

        // Load from ralph machine config resolve
        do {
            let result = try await client.runAndCollect(
                arguments: ["--no-color", "machine", "config", "resolve"],
                currentDirectoryURL: workspace.identityState.workingDirectoryURL
            )

            guard result.status.code == 0 else {
                throw NSError(domain: "ConfigLoad", code: Int(result.status.code))
            }

            let document = try JSONDecoder().decode(MachineConfigResolveDocument.self, from: Data(result.stdout.utf8))
            workspace.updateResolvedPaths(document.paths)

            if let rawDocument = try JSONSerialization.jsonObject(with: Data(result.stdout.utf8)) as? [String: Any],
               let rawConfig = rawDocument["config"] as? [String: Any]
            {
                self.loadedConfigDict = rawConfig
            } else if let rawConfig = try JSONSerialization.jsonObject(with: Data(result.stdout.utf8))
                as? [String: Any]
            {
                self.loadedConfigDict = rawConfig
            } else {
                self.loadedConfigDict = [:]
            }

            let config = document.config

            // Apply to properties
            applyResolvedConfig(config)

            hasUnsavedChanges = false
            hasLoadedConfig = true
        } catch {
            errorMessage = "Failed to load config: \(error.localizedDescription)"
            RalphLogger.shared.error("Failed to load config: \(error)", category: .config)
        }

        isLoading = false
    }

    // MARK: - Saving

    /// Schedule a debounced save of the current settings
    func scheduleSave() {
        guard !isApplyingLoadedValues else { return }
        hasUnsavedChanges = true

        // Cancel existing save task
        saveTask?.cancel()

        // Schedule new save after 500ms debounce
        saveTask = Task {
            try? await Task.sleep(nanoseconds: 500_000_000) // 500ms

            guard !Task.isCancelled else { return }
            await saveConfig()
        }
    }

    /// Immediately save all current settings
    func saveConfig() async {
        // Build agent config object to write
        let agentConfig: [String: Any] = [
            "runner": runner,
            "model": model,
            "phases": phases,
            "iterations": iterations,
            "reasoning_effort": reasoningEffort,
            "notification": [
                "enabled": notificationsEnabled,
                "notify_on_complete": notifyOnComplete,
                "notify_on_fail": notifyOnFail,
                "notify_on_loop_complete": notifyOnLoopComplete,
                "sound_enabled": soundEnabled,
                "suppress_when_active": suppressWhenActive
            ]
        ]

        let configURL = workspace.projectConfigFileURL
            ?? workspace.identityState.workingDirectoryURL.appendingPathComponent(".ralph/config.jsonc")

        do {
            // Ensure .ralph directory exists
            let ralphDir = configURL.deletingLastPathComponent()
            if !FileManager.default.fileExists(atPath: ralphDir.path) {
                try FileManager.default.createDirectory(at: ralphDir, withIntermediateDirectories: true)
            }

            // Merge against the last resolved machine config payload
            // so we preserve fields that Settings UI does not manage, even when the repo
            // uses JSONC comments on disk.
            var existingDict = loadedConfigDict

            // Deep merge: preserve existing agent fields not managed by Settings UI
            // (e.g., runner_cli, runner_retry, webhook, phase_overrides, etc.)
            var existingAgent = existingDict["agent"] as? [String: Any] ?? [:]
            for (key, value) in agentConfig {
                existingAgent[key] = value
            }
            existingDict["agent"] = existingAgent

            let jsonData = try JSONSerialization.data(
                withJSONObject: existingDict,
                options: [.sortedKeys, .prettyPrinted]
            )
            try jsonData.write(to: configURL, options: .atomic)

            hasUnsavedChanges = false
            errorMessage = nil

            RalphLogger.shared.info("Saved config to \(configURL.path)", category: .config)
        } catch {
            errorMessage = "Failed to save config: \(error.localizedDescription)"
            RalphLogger.shared.error("Failed to save config: \(error)", category: .config)
        }
    }

    // MARK: - Helpers

    var suggestedModels: [String] {
        commonModels[runner] ?? ["default"]
    }

    func handleRunnerChanged(to newValue: String) {
        guard !isApplyingLoadedValues else { return }
        if let firstModel = commonModels[newValue]?.first, model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            model = firstModel
        }
        scheduleSave()
    }

    func selectSuggestedModel(_ selectedModel: String) {
        guard model != selectedModel else { return }
        model = selectedModel
        scheduleSave()
    }

    func resetToDefaults() {
        runner = "codex"
        model = "gpt-5.4"
        phases = 3
        iterations = 1
        reasoningEffort = "medium"
        notificationsEnabled = true
        notifyOnComplete = true
        notifyOnFail = true
        notifyOnLoopComplete = true
        soundEnabled = false
        suppressWhenActive = true

        scheduleSave()
    }

    private func applyResolvedConfig(_ config: RalphConfig) {
        isApplyingLoadedValues = true
        defer { isApplyingLoadedValues = false }

        let agent = config.agent
        runner = agent?.runner ?? "codex"
        model = agent?.model ?? "gpt-5.4"
        phases = agent?.phases ?? 3
        iterations = agent?.iterations ?? 1
        reasoningEffort = agent?.reasoningEffort ?? "medium"

        let notification = agent?.notification
        notificationsEnabled = notification?.enabled ?? true
        notifyOnComplete = notification?.notifyOnComplete ?? true
        notifyOnFail = notification?.notifyOnFail ?? true
        notifyOnLoopComplete = notification?.notifyOnLoopComplete ?? true
        soundEnabled = notification?.soundEnabled ?? false
        suppressWhenActive = notification?.suppressWhenActive ?? true
    }
}
