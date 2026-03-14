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

struct SettingsProjectConfigSnapshot: Sendable {
    let config: RalphConfig
    let rawConfigData: Data
}

enum SettingsProjectConfigLoader {
    static func load(from configURL: URL) async throws -> SettingsProjectConfigSnapshot {
        try await Task.detached(priority: .userInitiated) {
            try loadSynchronously(from: configURL)
        }.value
    }

    static func loadSynchronously(from configURL: URL) throws -> SettingsProjectConfigSnapshot {
        let rawData = try loadRawConfigData(from: configURL)
        let config = try JSONDecoder().decode(RalphConfig.self, from: rawData)
        return SettingsProjectConfigSnapshot(config: config, rawConfigData: rawData)
    }

    private static func loadRawConfigData(from configURL: URL) throws -> Data {
        guard FileManager.default.fileExists(atPath: configURL.path) else {
            return Data("{}".utf8)
        }

        let source = try String(contentsOf: configURL, encoding: .utf8)
        let sanitized = sanitizeJSONC(source)
        return Data(sanitized.utf8)
    }

    private static func sanitizeJSONC(_ source: String) -> String {
        removeTrailingCommas(from: stripComments(from: source))
    }

    private static func stripComments(from source: String) -> String {
        var output = String()
        var iterator = source.makeIterator()
        var current = iterator.next()
        var inString = false
        var escaping = false
        var inLineComment = false
        var inBlockComment = false

        while let character = current {
            let next = iterator.next()

            if inLineComment {
                if character == "\n" {
                    inLineComment = false
                    output.append(character)
                }
                current = next
                continue
            }

            if inBlockComment {
                if character == "*", next == "/" {
                    inBlockComment = false
                    current = iterator.next()
                } else {
                    current = next
                }
                continue
            }

            if inString {
                output.append(character)
                if escaping {
                    escaping = false
                } else if character == "\\" {
                    escaping = true
                } else if character == "\"" {
                    inString = false
                }
                current = next
                continue
            }

            if character == "\"" {
                inString = true
                output.append(character)
                current = next
                continue
            }

            if character == "/", next == "/" {
                inLineComment = true
                current = iterator.next()
                continue
            }

            if character == "/", next == "*" {
                inBlockComment = true
                current = iterator.next()
                continue
            }

            output.append(character)
            current = next
        }

        return output
    }

    private static func removeTrailingCommas(from source: String) -> String {
        let characters = Array(source)
        var output = String()
        var index = 0
        var inString = false
        var escaping = false

        while index < characters.count {
            let character = characters[index]

            if inString {
                output.append(character)
                if escaping {
                    escaping = false
                } else if character == "\\" {
                    escaping = true
                } else if character == "\"" {
                    inString = false
                }
                index += 1
                continue
            }

            if character == "\"" {
                inString = true
                output.append(character)
                index += 1
                continue
            }

            if character == "," {
                var lookahead = index + 1
                while lookahead < characters.count, characters[lookahead].isWhitespace {
                    lookahead += 1
                }
                if lookahead < characters.count,
                   characters[lookahead] == "}" || characters[lookahead] == "]" {
                    index += 1
                    continue
                }
            }

            output.append(character)
            index += 1
        }

        return output
    }
}

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

        let configURL = workspace.projectConfigFileURL
            ?? workspace.identityState.workingDirectoryURL.appendingPathComponent(".ralph/config.jsonc")

        do {
            let snapshot = try await SettingsProjectConfigLoader.load(from: configURL)

            if let rawConfig = try JSONSerialization.jsonObject(with: snapshot.rawConfigData) as? [String: Any] {
                self.loadedConfigDict = rawConfig
            } else {
                self.loadedConfigDict = [:]
            }

            applyResolvedConfig(snapshot.config)
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
