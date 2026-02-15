/**
 ConfigModels

 Responsibilities:
 - Provide Codable models for Ralph configuration parsing and serialization.
 - Mirror the structure of schemas/config.schema.json for Settings UI binding.

 Does not handle:
 - CLI operations (see RalphCLIClient).
 - Config validation (CLI is source of truth).

 Invariants/assumptions callers must respect:
 - These models are partial; unknown fields are ignored during decoding.
 - Write operations use CLI, not direct file manipulation.
 */

import Foundation

// MARK: - Notification Config

public struct NotificationConfig: Codable, Sendable, Equatable {
    public var enabled: Bool?
    public var notifyOnComplete: Bool?
    public var notifyOnFail: Bool?
    public var notifyOnLoopComplete: Bool?
    public var soundEnabled: Bool?
    public var suppressWhenActive: Bool?

    private enum CodingKeys: String, CodingKey {
        case enabled
        case notifyOnComplete = "notify_on_complete"
        case notifyOnFail = "notify_on_fail"
        case notifyOnLoopComplete = "notify_on_loop_complete"
        case soundEnabled = "sound_enabled"
        case suppressWhenActive = "suppress_when_active"
    }

    public init(
        enabled: Bool? = nil,
        notifyOnComplete: Bool? = nil,
        notifyOnFail: Bool? = nil,
        notifyOnLoopComplete: Bool? = nil,
        soundEnabled: Bool? = nil,
        suppressWhenActive: Bool? = nil
    ) {
        self.enabled = enabled
        self.notifyOnComplete = notifyOnComplete
        self.notifyOnFail = notifyOnFail
        self.notifyOnLoopComplete = notifyOnLoopComplete
        self.soundEnabled = soundEnabled
        self.suppressWhenActive = suppressWhenActive
    }
}

// MARK: - Agent Config (partial for Settings UI)

public struct AgentConfig: Codable, Sendable, Equatable {
    public var runner: String?
    public var model: String?
    public var phases: Int?
    public var iterations: Int?
    public var reasoningEffort: String?
    public var notification: NotificationConfig?

    private enum CodingKeys: String, CodingKey {
        case runner
        case model
        case phases
        case iterations
        case reasoningEffort = "reasoning_effort"
        case notification
    }

    public init(
        runner: String? = nil,
        model: String? = nil,
        phases: Int? = nil,
        iterations: Int? = nil,
        reasoningEffort: String? = nil,
        notification: NotificationConfig? = nil
    ) {
        self.runner = runner
        self.model = model
        self.phases = phases
        self.iterations = iterations
        self.reasoningEffort = reasoningEffort
        self.notification = notification
    }
}

// MARK: - Root Config

public struct RalphConfig: Codable, Sendable, Equatable {
    public var agent: AgentConfig?

    public init(agent: AgentConfig? = nil) {
        self.agent = agent
    }
}

// MARK: - Runner Options (for UI pickers)

public enum ConfigRunner: String, CaseIterable, Identifiable, Sendable {
    case claude = "claude"
    case codex = "codex"
    case opencode = "opencode"
    case gemini = "gemini"
    case cursor = "cursor"
    case kimi = "kimi"
    case pi = "pi"

    public var id: String { rawValue }

    public var displayName: String {
        switch self {
        case .claude: return "Claude"
        case .codex: return "Codex"
        case .opencode: return "OpenCode"
        case .gemini: return "Gemini"
        case .cursor: return "Cursor"
        case .kimi: return "Kimi"
        case .pi: return "Pi"
        }
    }
}

public enum ConfigPhases: Int, CaseIterable, Identifiable, Sendable {
    case single = 1
    case two = 2
    case three = 3

    public var id: Int { rawValue }

    public var displayName: String {
        switch self {
        case .single: return "1 Phase (Single-pass)"
        case .two: return "2 Phases (Plan + Implement)"
        case .three: return "3 Phases (Plan + Implement + Review)"
        }
    }
}

public enum ConfigReasoningEffort: String, CaseIterable, Identifiable, Sendable {
    case low = "low"
    case medium = "medium"
    case high = "high"
    case xhigh = "xhigh"

    public var id: String { rawValue }

    public var displayName: String {
        switch self {
        case .low: return "Low"
        case .medium: return "Medium"
        case .high: return "High"
        case .xhigh: return "Extra High"
        }
    }
}
