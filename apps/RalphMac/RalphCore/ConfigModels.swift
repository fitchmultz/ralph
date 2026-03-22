/**
 ConfigModels

 Responsibilities:
 - Provide Codable models for Ralph configuration parsing and serialization.
 - Mirror the machine-resolved config and path documents used by the app.
 - Decode structured resume preview state from machine config and run-event payloads.

 Does not handle:
 - CLI operations (see RalphCLIClient).
 - Config validation (CLI is source of truth).

 Invariants/assumptions callers must respect:
 - These models are partial; unknown fields are ignored during decoding.
 - Machine config documents are the source of truth for resolved workspace paths.
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
    public var gitPublishMode: String?
    public var notification: NotificationConfig?

    private enum CodingKeys: String, CodingKey {
        case runner
        case model
        case phases
        case iterations
        case reasoningEffort = "reasoning_effort"
        case gitPublishMode = "git_publish_mode"
        case notification
    }

    public init(
        runner: String? = nil,
        model: String? = nil,
        phases: Int? = nil,
        iterations: Int? = nil,
        reasoningEffort: String? = nil,
        gitPublishMode: String? = nil,
        notification: NotificationConfig? = nil
    ) {
        self.runner = runner
        self.model = model
        self.phases = phases
        self.iterations = iterations
        self.reasoningEffort = reasoningEffort
        self.gitPublishMode = gitPublishMode
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

public struct MachineQueuePaths: Codable, Sendable, Equatable {
    public let repoRoot: String
    public let queuePath: String
    public let donePath: String
    public let projectConfigPath: String?
    public let globalConfigPath: String?

    private enum CodingKeys: String, CodingKey {
        case repoRoot = "repo_root"
        case queuePath = "queue_path"
        case donePath = "done_path"
        case projectConfigPath = "project_config_path"
        case globalConfigPath = "global_config_path"
    }
}

public struct MachineSystemInfoDocument: Codable, Sendable, Equatable {
    public let version: Int
    public let cliVersion: String

    private enum CodingKeys: String, CodingKey {
        case version
        case cliVersion = "cli_version"
    }
}

public struct MachineResumeDecision: Codable, Sendable, Equatable {
    public let status: String
    public let scope: String
    public let reason: String
    public let taskID: String?
    public let message: String
    public let detail: String

    private enum CodingKeys: String, CodingKey {
        case status
        case scope
        case reason
        case taskID = "task_id"
        case message
        case detail
    }

    public func asWorkspaceResumeState() -> Workspace.ResumeState? {
        guard let status = Workspace.ResumeState.Status(rawValue: self.status) else {
            return nil
        }
        return Workspace.ResumeState(
            status: status,
            scope: scope,
            reason: reason,
            taskID: taskID,
            message: message,
            detail: detail
        )
    }

    public func asWorkspaceBlockingState() -> Workspace.BlockingState? {
        switch reason {
        case "runner_session_invalid",
            "missing_runner_session_id",
            "resume_confirmation_required",
            "session_timed_out_requires_confirmation":
            return Workspace.BlockingState(
                status: .stalled,
                reason: .runnerRecovery(scope: scope, reason: reason, taskID: taskID),
                taskID: taskID,
                message: message,
                detail: detail
            )
        default:
            return nil
        }
    }
}

public struct MachineConfigResolveDocument: Codable, Sendable, Equatable {
    public let version: Int
    public let paths: MachineQueuePaths
    public let safety: MachineConfigSafetySummary
    public let config: RalphConfig
    public let resumePreview: MachineResumeDecision?

    private enum CodingKeys: String, CodingKey {
        case version
        case paths
        case safety
        case config
        case resumePreview = "resume_preview"
    }
}

public struct MachineConfigSafetySummary: Codable, Sendable, Equatable {
    public let repoTrusted: Bool
    public let dirtyRepo: Bool
    public let gitPublishMode: String
    public let approvalMode: String?
    public let ciGateEnabled: Bool
    public let gitRevertMode: String
    public let parallelConfigured: Bool
    public let executionInteractivity: String
    public let interactiveApprovalSupported: Bool

    private enum CodingKeys: String, CodingKey {
        case repoTrusted = "repo_trusted"
        case dirtyRepo = "dirty_repo"
        case gitPublishMode = "git_publish_mode"
        case approvalMode = "approval_mode"
        case ciGateEnabled = "ci_gate_enabled"
        case gitRevertMode = "git_revert_mode"
        case parallelConfigured = "parallel_configured"
        case executionInteractivity = "execution_interactivity"
        case interactiveApprovalSupported = "interactive_approval_supported"
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
