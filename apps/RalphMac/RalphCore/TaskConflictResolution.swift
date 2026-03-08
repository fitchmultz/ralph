/**
 TaskConflictResolution

 Responsibilities:
 - Define the canonical task-conflict fields and their display metadata.
 - Build testable field/section presentation models for conflict-resolution UI.
 - Apply a user's merge selections to produce a resolved `RalphTask`.

 Does not handle:
 - Optimistic-lock detection timing (see `Workspace+ConflictDetection.swift`).
 - SwiftUI view state storage or sheet presentation.
 - Persisting resolved tasks.

 Invariants/assumptions callers must respect:
 - Local and external tasks refer to the same task identity.
 - Conflict selections are keyed by `TaskConflictField`, not raw strings.
 - External task is the merge base unless a field is explicitly switched to local.
 */

import Foundation

public enum TaskConflictMergeChoice: String, CaseIterable, Sendable, Equatable {
    case local = "Local"
    case external = "External"
}

public enum TaskConflictFieldSection: String, CaseIterable, Identifiable, Sendable {
    case basicInformation = "Basic Information"
    case tags = "Tags"
    case arrays = "Arrays"
    case relationships = "Relationships"
    case executionOverrides = "Execution Overrides"

    public var id: String { rawValue }
}

public enum TaskConflictField: String, CaseIterable, Identifiable, Sendable {
    case title
    case description
    case status
    case priority
    case tags
    case scope
    case evidence
    case plan
    case notes
    case dependsOn
    case blocks
    case relatesTo
    case agent

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .title: return "Title"
        case .description: return "Description"
        case .status: return "Status"
        case .priority: return "Priority"
        case .tags: return "Tags"
        case .scope: return "Scope"
        case .evidence: return "Evidence"
        case .plan: return "Plan"
        case .notes: return "Notes"
        case .dependsOn: return "Depends On"
        case .blocks: return "Blocks"
        case .relatesTo: return "Relates To"
        case .agent: return "Agent Overrides"
        }
    }

    public var section: TaskConflictFieldSection {
        switch self {
        case .title, .description, .status, .priority:
            return .basicInformation
        case .tags:
            return .tags
        case .scope, .evidence, .plan, .notes:
            return .arrays
        case .dependsOn, .blocks, .relatesTo:
            return .relationships
        case .agent:
            return .executionOverrides
        }
    }

    public func differs(local: RalphTask, external: RalphTask) -> Bool {
        switch self {
        case .title:
            return local.title != external.title
        case .description:
            return local.description != external.description
        case .status:
            return local.status != external.status
        case .priority:
            return local.priority != external.priority
        case .tags:
            return local.tags != external.tags
        case .scope:
            return local.scope != external.scope
        case .evidence:
            return local.evidence != external.evidence
        case .plan:
            return local.plan != external.plan
        case .notes:
            return local.notes != external.notes
        case .dependsOn:
            return local.dependsOn != external.dependsOn
        case .blocks:
            return local.blocks != external.blocks
        case .relatesTo:
            return local.relatesTo != external.relatesTo
        case .agent:
            return local.agent != external.agent
        }
    }

    public func formattedValue(in task: RalphTask) -> String {
        switch self {
        case .title:
            return task.title
        case .description:
            return task.description ?? "(none)"
        case .status:
            return task.status.displayName
        case .priority:
            return task.priority.displayName
        case .tags:
            return Self.formatInlineList(task.tags)
        case .scope:
            return Self.formatOptionalList(task.scope)
        case .evidence:
            return Self.formatOptionalList(task.evidence)
        case .plan:
            return Self.formatOptionalList(task.plan)
        case .notes:
            return Self.formatOptionalList(task.notes)
        case .dependsOn:
            return Self.formatOptionalList(task.dependsOn)
        case .blocks:
            return Self.formatOptionalList(task.blocks)
        case .relatesTo:
            return Self.formatOptionalList(task.relatesTo)
        case .agent:
            return Self.formatAgent(task.agent)
        }
    }

    public func applyLocalValue(from local: RalphTask, into resolved: inout RalphTask) {
        switch self {
        case .title:
            resolved.title = local.title
        case .description:
            resolved.description = local.description
        case .status:
            resolved.status = local.status
        case .priority:
            resolved.priority = local.priority
        case .tags:
            resolved.tags = local.tags
        case .scope:
            resolved.scope = local.scope
        case .evidence:
            resolved.evidence = local.evidence
        case .plan:
            resolved.plan = local.plan
        case .notes:
            resolved.notes = local.notes
        case .dependsOn:
            resolved.dependsOn = local.dependsOn
        case .blocks:
            resolved.blocks = local.blocks
        case .relatesTo:
            resolved.relatesTo = local.relatesTo
        case .agent:
            resolved.agent = local.agent
        }
    }

    private static func formatInlineList(_ values: [String]) -> String {
        values.isEmpty ? "(none)" : values.joined(separator: ", ")
    }

    private static func formatOptionalList(_ values: [String]?) -> String {
        guard let values, !values.isEmpty else { return "(none)" }
        return values.joined(separator: ", ")
    }

    private static func formatAgent(_ agent: RalphTaskAgent?) -> String {
        guard let agent else { return "(none)" }

        var parts: [String] = []
        if let runner = agent.runner, !runner.isEmpty { parts.append("runner=\(runner)") }
        if let model = agent.model, !model.isEmpty { parts.append("model=\(model)") }
        if let effort = agent.modelEffort, !effort.isEmpty { parts.append("effort=\(effort)") }
        if let phases = agent.phases { parts.append("phases=\(phases)") }
        if let iterations = agent.iterations { parts.append("iterations=\(iterations)") }
        if let phaseOverrides = agent.phaseOverrides, !phaseOverrides.isEmpty {
            parts.append("phase_overrides=yes")
        }

        return parts.isEmpty ? "(none)" : parts.joined(separator: ", ")
    }
}

public struct TaskConflictFieldPresentation: Identifiable, Sendable, Equatable {
    public let field: TaskConflictField
    public let localValue: String
    public let externalValue: String

    public var id: String { field.id }
    public var label: String { field.label }
    public var section: TaskConflictFieldSection { field.section }
    public var hasConflict: Bool { localValue != externalValue }
}

public struct TaskConflictSectionPresentation: Identifiable, Sendable, Equatable {
    public let section: TaskConflictFieldSection
    public let fields: [TaskConflictFieldPresentation]

    public var id: String { section.id }
}

public struct TaskConflictResolutionModel: Sendable, Equatable {
    public let localTask: RalphTask
    public let externalTask: RalphTask
    public let fieldPresentations: [TaskConflictFieldPresentation]
    public let sections: [TaskConflictSectionPresentation]
    public let initialSelections: [TaskConflictField: TaskConflictMergeChoice]

    public init(localTask: RalphTask, externalTask: RalphTask) {
        self.localTask = localTask
        self.externalTask = externalTask

        let presentations = TaskConflictField.allCases.compactMap { field -> TaskConflictFieldPresentation? in
            guard field.differs(local: localTask, external: externalTask) else { return nil }
            return TaskConflictFieldPresentation(
                field: field,
                localValue: field.formattedValue(in: localTask),
                externalValue: field.formattedValue(in: externalTask)
            )
        }
        fieldPresentations = presentations

        sections = TaskConflictFieldSection.allCases.compactMap { section in
            let sectionFields = presentations.filter { $0.section == section }
            guard !sectionFields.isEmpty else { return nil }
            return TaskConflictSectionPresentation(section: section, fields: sectionFields)
        }

        initialSelections = Dictionary(
            uniqueKeysWithValues: presentations.map { ($0.field, .external) }
        )
    }

    public func applySelections(_ selections: [TaskConflictField: TaskConflictMergeChoice]) -> RalphTask {
        Self.applySelections(localTask: localTask, externalTask: externalTask, selections: selections)
    }

    public static func applySelections(
        localTask: RalphTask,
        externalTask: RalphTask,
        selections: [TaskConflictField: TaskConflictMergeChoice]
    ) -> RalphTask {
        var resolved = externalTask
        for field in TaskConflictField.allCases where selections[field] == .local {
            field.applyLocalValue(from: localTask, into: &resolved)
        }
        return resolved
    }
}
