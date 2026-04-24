/**
 ConfigModels

 Purpose:
 - Provide Codable models for Ralph configuration parsing and serialization.

 Responsibilities:
 - Provide Codable models for Ralph configuration parsing and serialization.
 - Mirror the machine-resolved config, error, and path documents used by the app.
 - Decode structured resume preview and shared parallel-status payloads from machine surfaces.

 Does not handle:
 - CLI operations (see RalphCLIClient).
 - Config validation (CLI is source of truth).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

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
    public var notifyOnWatchNewTasks: Bool?
    public var soundEnabled: Bool?
    public var suppressWhenActive: Bool?

    private enum CodingKeys: String, CodingKey {
        case enabled
        case notifyOnComplete = "notify_on_complete"
        case notifyOnFail = "notify_on_fail"
        case notifyOnLoopComplete = "notify_on_loop_complete"
        case notifyOnWatchNewTasks = "notify_on_watch_new_tasks"
        case soundEnabled = "sound_enabled"
        case suppressWhenActive = "suppress_when_active"
    }

    public init(
        enabled: Bool? = nil,
        notifyOnComplete: Bool? = nil,
        notifyOnFail: Bool? = nil,
        notifyOnLoopComplete: Bool? = nil,
        notifyOnWatchNewTasks: Bool? = nil,
        soundEnabled: Bool? = nil,
        suppressWhenActive: Bool? = nil
    ) {
        self.enabled = enabled
        self.notifyOnComplete = notifyOnComplete
        self.notifyOnFail = notifyOnFail
        self.notifyOnLoopComplete = notifyOnLoopComplete
        self.notifyOnWatchNewTasks = notifyOnWatchNewTasks
        self.soundEnabled = soundEnabled
        self.suppressWhenActive = suppressWhenActive
    }
}

// MARK: - Webhook Config (partial; mirrors `agent.webhook` from machine-resolved config)

/// Subset of CLI `WebhookConfig` needed for RalphCore decoding of `machine config resolve` payloads.
/// Additional webhook keys from the CLI are ignored on decode.
public struct WebhookConfig: Codable, Sendable, Equatable {
    public var enabled: Bool?
    public var url: String?
    public var allowInsecureHttp: Bool?
    public var allowPrivateTargets: Bool?
    public var retryCount: UInt32?
    public var retryBackoffMs: UInt32?

    private enum CodingKeys: String, CodingKey {
        case enabled
        case url
        case allowInsecureHttp = "allow_insecure_http"
        case allowPrivateTargets = "allow_private_targets"
        case retryCount = "retry_count"
        case retryBackoffMs = "retry_backoff_ms"
    }

    public init(
        enabled: Bool? = nil,
        url: String? = nil,
        allowInsecureHttp: Bool? = nil,
        allowPrivateTargets: Bool? = nil,
        retryCount: UInt32? = nil,
        retryBackoffMs: UInt32? = nil
    ) {
        self.enabled = enabled
        self.url = url
        self.allowInsecureHttp = allowInsecureHttp
        self.allowPrivateTargets = allowPrivateTargets
        self.retryCount = retryCount
        self.retryBackoffMs = retryBackoffMs
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
    public var webhook: WebhookConfig?

    private enum CodingKeys: String, CodingKey {
        case runner
        case model
        case phases
        case iterations
        case reasoningEffort = "reasoning_effort"
        case gitPublishMode = "git_publish_mode"
        case notification
        case webhook
    }

    public init(
        runner: String? = nil,
        model: String? = nil,
        phases: Int? = nil,
        iterations: Int? = nil,
        reasoningEffort: String? = nil,
        gitPublishMode: String? = nil,
        notification: NotificationConfig? = nil,
        webhook: WebhookConfig? = nil
    ) {
        self.runner = runner
        self.model = model
        self.phases = phases
        self.iterations = iterations
        self.reasoningEffort = reasoningEffort
        self.gitPublishMode = gitPublishMode
        self.notification = notification
        self.webhook = webhook
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

public enum MachineErrorCode: String, Codable, Sendable, Equatable {
    case cliUnavailable = "cli_unavailable"
    case permissionDenied = "permission_denied"
    case configIncompatible = "config_incompatible"
    case parseError = "parse_error"
    case networkError = "network_error"
    case queueCorrupted = "queue_corrupted"
    case resourceBusy = "resource_busy"
    case versionMismatch = "version_mismatch"
    case taskMutationConflict = "task_mutation_conflict"
    case unknown
}

public struct MachineErrorDocument: Codable, Sendable, Equatable {
    public let version: Int
    public let code: MachineErrorCode
    public let message: String
    public let detail: String?
    public let retryable: Bool

    public static func decode(from raw: String) -> MachineErrorDocument? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        guard let data = trimmed.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(MachineErrorDocument.self, from: data)
    }

    public var userFacingDescription: String {
        var lines = [
            "Code: \(code.rawValue)",
            "Message: \(message)",
        ]

        if let detail = sanitizedDetail {
            lines.append("Detail: \(detail)")
        }

        lines.append("Retryable: \(retryable ? "yes" : "no")")
        return lines.joined(separator: "\n")
    }

    private var sanitizedDetail: String? {
        guard let detail else { return nil }
        let trimmed = detail.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
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
                detail: detail,
                observedAt: nil
            )
        default:
            return nil
        }
    }
}

public struct MachineConfigResolveDocument: Codable, Sendable, Equatable, VersionedMachineDocument {
    public static let expectedVersion = RalphMachineContract.configResolveVersion
    public static let documentName = "machine config resolve"

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

public struct MachineWorkspaceOverviewDocument: Codable, Sendable, Equatable, VersionedMachineDocument {
    public static let expectedVersion = RalphMachineContract.workspaceOverviewVersion
    public static let documentName = "machine workspace overview"

    public let version: Int
    public let queue: MachineQueueReadDocument
    public let config: MachineConfigResolveDocument
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

public enum ParallelWorkerLifecycle: String, Codable, Sendable, Equatable {
    case running
    case integrating
    case completed
    case failed
    case blockedPush = "blocked_push"
}

public struct ParallelWorkerStatus: Codable, Sendable, Equatable, Identifiable {
    public let taskID: String
    public let lifecycle: ParallelWorkerLifecycle

    public var id: String { taskID }

    enum CodingKeys: String, CodingKey {
        case taskID = "task_id"
        case lifecycle
    }
}

public struct ParallelLifecycleCounts: Sendable, Equatable, Codable {
    public let running: Int
    public let integrating: Int
    public let completed: Int
    public let failed: Int
    public let blocked: Int
    public let total: Int

    private enum CodingKeys: String, CodingKey {
        case running
        case integrating
        case completed
        case failed
        case blocked
        case total
    }

    public init(running: Int, integrating: Int, completed: Int, failed: Int, blocked: Int, total: Int) {
        self.running = running
        self.integrating = integrating
        self.completed = completed
        self.failed = failed
        self.blocked = blocked
        self.total = total
    }

    init(workers: [ParallelWorkerStatus]) {
        let running = workers.filter { $0.lifecycle == .running }.count
        let integrating = workers.filter { $0.lifecycle == .integrating }.count
        let completed = workers.filter { $0.lifecycle == .completed }.count
        let failed = workers.filter { $0.lifecycle == .failed }.count
        let blocked = workers.filter { $0.lifecycle == .blockedPush }.count
        let total = running + integrating + completed + failed + blocked
        self.init(
            running: running,
            integrating: integrating,
            completed: completed,
            failed: failed,
            blocked: blocked,
            total: total
        )
    }

    public var hasActive: Bool { running > 0 || integrating > 0 }
}

public struct ParallelStatusSnapshot: Codable, Sendable, Equatable {
    public let schemaVersion: Int?
    public let targetBranch: String?
    public let workers: [ParallelWorkerStatus]
    /// When present on `machine run parallel-status` documents, mirrors the top-level `lifecycle_counts` field.
    private let documentLifecycleCounts: ParallelLifecycleCounts?

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case targetBranch = "target_branch"
        case workers
        case documentLifecycleCounts = "lifecycle_counts"
    }

    public init(
        schemaVersion: Int?,
        targetBranch: String?,
        workers: [ParallelWorkerStatus],
        documentLifecycleCounts: ParallelLifecycleCounts? = nil
    ) {
        self.schemaVersion = schemaVersion
        self.targetBranch = targetBranch
        self.workers = workers
        self.documentLifecycleCounts = documentLifecycleCounts
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        schemaVersion = try container.decodeIfPresent(Int.self, forKey: .schemaVersion)
        targetBranch = try container.decodeIfPresent(String.self, forKey: .targetBranch)
        workers = try container.decodeIfPresent([ParallelWorkerStatus].self, forKey: .workers) ?? []
        documentLifecycleCounts = try container.decodeIfPresent(
            ParallelLifecycleCounts.self,
            forKey: .documentLifecycleCounts
        )
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encodeIfPresent(schemaVersion, forKey: .schemaVersion)
        try container.encodeIfPresent(targetBranch, forKey: .targetBranch)
        try container.encode(workers, forKey: .workers)
        try container.encodeIfPresent(documentLifecycleCounts, forKey: .documentLifecycleCounts)
    }

    public var lifecycleCounts: ParallelLifecycleCounts {
        documentLifecycleCounts ?? ParallelLifecycleCounts(workers: workers)
    }
}

public struct ParallelStatusStep: Sendable, Equatable {
    public let command: String
    public let detail: String
}

struct MachineParallelStatusDocument: Decodable, Sendable, Equatable, VersionedMachineDocument {
    static let expectedVersion = RalphMachineContract.parallelStatusVersion
    static let documentName = "machine parallel status"

    let version: Int
    let lifecycleCounts: ParallelLifecycleCounts
    let blocking: WorkspaceRunnerController.MachineBlockingState?
    let continuation: WorkspaceContinuationSummary
    let status: ParallelStatusSnapshot

    private enum CodingKeys: String, CodingKey {
        case version
        case lifecycleCounts = "lifecycle_counts"
        case blocking
        case continuation
        case status
    }

    var effectiveBlocking: WorkspaceRunnerController.MachineBlockingState? {
        blocking ?? continuation.blocking
    }

    func asWorkspaceParallelStatus() -> Workspace.ParallelStatus {
        let snapshot = ParallelStatusSnapshot(
            schemaVersion: status.schemaVersion,
            targetBranch: status.targetBranch,
            workers: status.workers,
            documentLifecycleCounts: lifecycleCounts
        )
        return Workspace.ParallelStatus(
            headline: continuation.headline,
            detail: continuation.detail,
            blocking: effectiveBlocking?.asWorkspaceBlockingState(),
            nextSteps: continuation.nextSteps.map { ParallelStatusStep(command: $0.command, detail: $0.detail) },
            snapshot: snapshot
        )
    }
}

struct MachineDoctorReportDocument: Decodable, Sendable, Equatable, VersionedMachineDocument {
    static let expectedVersion = RalphMachineContract.doctorReportVersion
    static let documentName = "machine doctor report"

    let version: Int
    let blocking: WorkspaceRunnerController.MachineBlockingState?
    let report: RalphJSONValue
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
