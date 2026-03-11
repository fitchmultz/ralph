/**
 RalphTaskModels

 Responsibilities:
 - Define task, agent-override, queue-document, and queue machine-document models shared across the app.
 - Normalize task-level execution overrides into canonical forms.

 Does not handle:
 - Graph visualization or analytics aggregation.
 - Workspace mutations or persistence side effects.

 Invariants/assumptions callers must respect:
 - Queue payloads decode from the current queue document object shape.
 - Task-agent normalization is the single cutover surface for override cleanup.
 */

public import Foundation

public enum RalphTaskStatus: String, Codable, Sendable, Equatable, CaseIterable {
    case draft = "draft"
    case todo = "todo"
    case doing = "doing"
    case done = "done"
    case rejected = "rejected"

    public var displayName: String {
        switch self {
        case .draft: return "Draft"
        case .todo: return "Todo"
        case .doing: return "Doing"
        case .done: return "Done"
        case .rejected: return "Rejected"
        }
    }
}

public enum RalphTaskPriority: String, Codable, Sendable, Equatable, CaseIterable {
    case critical = "critical"
    case high = "high"
    case medium = "medium"
    case low = "low"

    public var displayName: String {
        switch self {
        case .critical: return "Critical"
        case .high: return "High"
        case .medium: return "Medium"
        case .low: return "Low"
        }
    }

    public var sortOrder: Int {
        switch self {
        case .critical: return 4
        case .high: return 3
        case .medium: return 2
        case .low: return 1
        }
    }
}

public struct RalphTaskPhaseOverride: Codable, Sendable, Equatable {
    public var runner: String?
    public var model: String?
    public var reasoningEffort: String?

    private enum CodingKeys: String, CodingKey {
        case runner
        case model
        case reasoningEffort = "reasoning_effort"
    }

    public init(
        runner: String? = nil,
        model: String? = nil,
        reasoningEffort: String? = nil
    ) {
        self.runner = runner
        self.model = model
        self.reasoningEffort = reasoningEffort
    }

    public var isEmpty: Bool {
        (runner?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true)
            && (model?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true)
            && (reasoningEffort?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true)
    }
}

public struct RalphTaskPhaseOverrides: Codable, Sendable, Equatable {
    public var phase1: RalphTaskPhaseOverride?
    public var phase2: RalphTaskPhaseOverride?
    public var phase3: RalphTaskPhaseOverride?

    public init(
        phase1: RalphTaskPhaseOverride? = nil,
        phase2: RalphTaskPhaseOverride? = nil,
        phase3: RalphTaskPhaseOverride? = nil
    ) {
        self.phase1 = phase1
        self.phase2 = phase2
        self.phase3 = phase3
    }

    public var isEmpty: Bool {
        (phase1?.isEmpty ?? true) && (phase2?.isEmpty ?? true) && (phase3?.isEmpty ?? true)
    }
}

public struct RalphTaskAgent: Codable, Sendable, Equatable {
    public var runner: String?
    public var model: String?
    public var modelEffort: String?
    public var phases: Int?
    public var iterations: Int?
    public var followupReasoningEffort: String?
    public var runnerCLI: RalphJSONValue?
    public var phaseOverrides: RalphTaskPhaseOverrides?

    private enum CodingKeys: String, CodingKey {
        case runner
        case model
        case modelEffort = "model_effort"
        case phases
        case iterations
        case followupReasoningEffort = "followup_reasoning_effort"
        case runnerCLI = "runner_cli"
        case phaseOverrides = "phase_overrides"
    }

    public init(
        runner: String? = nil,
        model: String? = nil,
        modelEffort: String? = nil,
        phases: Int? = nil,
        iterations: Int? = nil,
        followupReasoningEffort: String? = nil,
        runnerCLI: RalphJSONValue? = nil,
        phaseOverrides: RalphTaskPhaseOverrides? = nil
    ) {
        self.runner = runner
        self.model = model
        self.modelEffort = modelEffort
        self.phases = phases
        self.iterations = iterations
        self.followupReasoningEffort = followupReasoningEffort
        self.runnerCLI = runnerCLI
        self.phaseOverrides = phaseOverrides
    }

    public var isEmpty: Bool {
        let runnerEmpty = runner?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true
        let modelEmpty = model?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true
        let modelEffortEmpty = modelEffort?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true
        let followupEmpty = followupReasoningEffort?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true
        return runnerEmpty
            && modelEmpty
            && modelEffortEmpty
            && phases == nil
            && iterations == nil
            && followupEmpty
            && runnerCLI == nil
            && (phaseOverrides?.isEmpty ?? true)
    }
}

public extension RalphTaskAgent {
    static func normalizedOverride(_ agent: RalphTaskAgent?) -> RalphTaskAgent? {
        guard var normalized = agent else { return nil }

        normalized.runner = normalizeOptionalString(normalized.runner)
        normalized.model = normalizeOptionalString(normalized.model)
        normalized.modelEffort = normalizeOptionalString(normalized.modelEffort)
        if normalized.modelEffort?.lowercased() == "default" {
            normalized.modelEffort = nil
        }
        normalized.followupReasoningEffort = normalizeOptionalString(normalized.followupReasoningEffort)

        if let phases = normalized.phases, !(1...3).contains(phases) {
            normalized.phases = nil
        }
        if let iterations = normalized.iterations, iterations < 1 {
            normalized.iterations = nil
        }

        if var phaseOverrides = normalized.phaseOverrides {
            phaseOverrides.phase1 = normalizePhaseOverride(phaseOverrides.phase1)
            phaseOverrides.phase2 = normalizePhaseOverride(phaseOverrides.phase2)
            phaseOverrides.phase3 = normalizePhaseOverride(phaseOverrides.phase3)
            normalized.phaseOverrides = phaseOverrides.isEmpty ? nil : phaseOverrides
        }

        return normalized.isEmpty ? nil : normalized
    }
}

public enum RalphTaskExecutionPreset: String, CaseIterable, Sendable, Identifiable {
    case codexDeep
    case codexBalanced
    case kimiFast
    case hybridCodexKimi
    case inheritFromConfig

    public var id: String { rawValue }

    public var displayName: String {
        switch self {
        case .codexDeep:
            return "Codex Deep"
        case .codexBalanced:
            return "Codex Balanced"
        case .kimiFast:
            return "Codex Fast"
        case .hybridCodexKimi:
            return "Codex Phased"
        case .inheritFromConfig:
            return "Inherit Config"
        }
    }

    public var description: String {
        switch self {
        case .codexDeep:
            return "High-reasoning Codex with full 3-phase flow."
        case .codexBalanced:
            return "Codex with medium effort and a 2-phase flow."
        case .kimiFast:
            return "Codex with low reasoning and a 1-phase flow."
        case .hybridCodexKimi:
            return "Codex with phase-specific effort tuning across the 3-phase flow."
        case .inheritFromConfig:
            return "Remove task overrides and use .ralph/config.jsonc."
        }
    }

    public var agentOverride: RalphTaskAgent? {
        switch self {
        case .codexDeep:
            return RalphTaskAgent(
                runner: "codex",
                model: "gpt-5.4",
                modelEffort: "high",
                phases: 3,
                iterations: 1
            )
        case .codexBalanced:
            return RalphTaskAgent(
                runner: "codex",
                model: "gpt-5.4",
                modelEffort: "medium",
                phases: 2,
                iterations: 1
            )
        case .kimiFast:
            return RalphTaskAgent(
                runner: "codex",
                model: "gpt-5.4",
                modelEffort: "low",
                phases: 1,
                iterations: 1
            )
        case .hybridCodexKimi:
            return RalphTaskAgent(
                phases: 3,
                iterations: 1,
                phaseOverrides: RalphTaskPhaseOverrides(
                    phase1: RalphTaskPhaseOverride(
                        runner: "codex",
                        model: "gpt-5.4",
                        reasoningEffort: "high"
                    ),
                    phase2: RalphTaskPhaseOverride(
                        runner: "codex",
                        model: "gpt-5.4",
                        reasoningEffort: "medium"
                    ),
                    phase3: RalphTaskPhaseOverride(
                        runner: "codex",
                        model: "gpt-5.4",
                        reasoningEffort: "medium"
                    )
                )
            )
        case .inheritFromConfig:
            return nil
        }
    }

    public static func matchingPreset(for agent: RalphTaskAgent?) -> RalphTaskExecutionPreset? {
        let normalizedAgent = RalphTaskAgent.normalizedOverride(agent)
        for preset in Self.allCases where preset != .inheritFromConfig {
            if RalphTaskAgent.normalizedOverride(preset.agentOverride) == normalizedAgent {
                return preset
            }
        }
        if normalizedAgent == nil {
            return .inheritFromConfig
        }
        return nil
    }
}

public struct RalphTask: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public var status: RalphTaskStatus
    public var title: String
    public var description: String?
    public var priority: RalphTaskPriority
    public var tags: [String]
    public var scope: [String]?
    public var evidence: [String]?
    public var plan: [String]?
    public var notes: [String]?
    public var request: String?
    public var agent: RalphTaskAgent?
    public var createdAt: Date?
    public var updatedAt: Date?
    public var startedAt: Date?
    public var completedAt: Date?
    public var estimatedMinutes: Int?
    public var actualMinutes: Int?
    public var dependsOn: [String]?
    public var blocks: [String]?
    public var relatesTo: [String]?
    public var customFields: [String: String]?

    private enum CodingKeys: String, CodingKey {
        case id, status, title, description, priority, tags, scope, evidence, plan, notes
        case request, agent, dependsOn = "depends_on", blocks, relatesTo = "relates_to"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case startedAt = "started_at"
        case completedAt = "completed_at"
        case estimatedMinutes = "estimated_minutes"
        case actualMinutes = "actual_minutes"
        case customFields = "custom_fields"
    }

    public init(
        id: String,
        status: RalphTaskStatus,
        title: String,
        description: String? = nil,
        priority: RalphTaskPriority,
        tags: [String] = [],
        scope: [String]? = nil,
        evidence: [String]? = nil,
        plan: [String]? = nil,
        notes: [String]? = nil,
        request: String? = nil,
        agent: RalphTaskAgent? = nil,
        createdAt: Date? = nil,
        updatedAt: Date? = nil,
        startedAt: Date? = nil,
        completedAt: Date? = nil,
        estimatedMinutes: Int? = nil,
        actualMinutes: Int? = nil,
        dependsOn: [String]? = nil,
        blocks: [String]? = nil,
        relatesTo: [String]? = nil,
        customFields: [String: String]? = nil
    ) {
        self.id = id
        self.status = status
        self.title = title
        self.description = description
        self.priority = priority
        self.tags = tags
        self.scope = scope
        self.evidence = evidence
        self.plan = plan
        self.notes = notes
        self.request = request
        self.agent = agent
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.startedAt = startedAt
        self.completedAt = completedAt
        self.estimatedMinutes = estimatedMinutes
        self.actualMinutes = actualMinutes
        self.dependsOn = dependsOn
        self.blocks = blocks
        self.relatesTo = relatesTo
        self.customFields = customFields
    }
}

/// Represents the top-level on-disk queue document under `.ralph/queue.jsonc`.
public struct RalphTaskQueueDocument: Codable, Sendable, Equatable {
    public let version: Int
    public let tasks: [RalphTask]

    private enum CodingKeys: String, CodingKey {
        case version
        case tasks
    }

    public init(version: Int = 1, tasks: [RalphTask]) {
        self.version = version
        self.tasks = tasks
    }

    public init(from decoder: any Decoder) throws {
        if let keyed = try? decoder.container(keyedBy: CodingKeys.self),
           keyed.contains(.tasks) {
            self.version = try keyed.decodeIfPresent(Int.self, forKey: .version) ?? 1
            self.tasks = try keyed.decode([RalphTask].self, forKey: .tasks)
            return
        }

        throw DecodingError.typeMismatch(
            RalphTaskQueueDocument.self,
            DecodingError.Context(
                codingPath: decoder.codingPath,
                debugDescription: "Expected queue document object with tasks key"
            )
        )
    }
}

public struct MachineQueueReadDocument: Codable, Sendable, Equatable {
    public let version: Int
    public let paths: MachineQueuePaths
    public let active: RalphTaskQueueDocument
    public let done: RalphTaskQueueDocument
    public let nextRunnableTaskID: String?
    public let runnability: RalphJSONValue

    private enum CodingKeys: String, CodingKey {
        case version
        case paths
        case active
        case done
        case nextRunnableTaskID = "next_runnable_task_id"
        case runnability
    }
}

private func normalizeOptionalString(_ value: String?) -> String? {
    guard let value else { return nil }
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
}

private func normalizePhaseOverride(_ overrideValue: RalphTaskPhaseOverride?) -> RalphTaskPhaseOverride? {
    guard var normalized = overrideValue else { return nil }
    normalized.runner = normalizeOptionalString(normalized.runner)
    normalized.model = normalizeOptionalString(normalized.model)
    normalized.reasoningEffort = normalizeOptionalString(normalized.reasoningEffort)
    return normalized.isEmpty ? nil : normalized
}
