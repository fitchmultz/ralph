/**
 WorkspaceTaskMutationModels

 Responsibilities:
 - Define the app-side machine JSON contracts for task create and task mutate.
 - Provide Codable request/response models used by workspace task mutation flows.
 - Keep mutation payload encoding centralized and consistent across single-task and bulk edits.

 Does not handle:
 - Executing CLI processes.
 - Diffing tasks or deciding which edits to send.
 - Queue loading or optimistic-lock conflict resolution.

 Invariants/assumptions callers must respect:
 - Field names must match the CLI's `TaskEditKey` snake_case values.
 - `expectedUpdatedAt` is encoded as RFC3339/ISO8601 for optimistic locking.
 - Mutation requests target active-queue tasks only.
 */

import Foundation

struct WorkspaceTaskMutationRequest: Codable, Sendable {
    let version: Int
    let atomic: Bool
    let tasks: [WorkspaceTaskMutationSpec]

    init(version: Int = 1, atomic: Bool = true, tasks: [WorkspaceTaskMutationSpec]) {
        self.version = version
        self.atomic = atomic
        self.tasks = tasks
    }
}

struct MachineTaskCreateRequest: Codable, Sendable {
    let version: Int
    let title: String
    let description: String?
    let priority: String
    let tags: [String]
    let scope: [String]
    let template: String?
    let target: String?

    init(
        version: Int = 1,
        title: String,
        description: String?,
        priority: String,
        tags: [String],
        scope: [String],
        template: String?,
        target: String?
    ) {
        self.version = version
        self.title = title
        self.description = description
        self.priority = priority
        self.tags = tags
        self.scope = scope
        self.template = template
        self.target = target
    }
}

struct MachineTaskCreateDocument: Codable, Sendable {
    let version: Int
    let task: RalphTask
}

struct WorkspaceTaskMutationSpec: Codable, Sendable {
    let taskID: String
    let expectedUpdatedAt: String?
    let edits: [WorkspaceTaskFieldEdit]

    enum CodingKeys: String, CodingKey {
        case taskID = "task_id"
        case expectedUpdatedAt = "expected_updated_at"
        case edits
    }
}

struct WorkspaceTaskFieldEdit: Codable, Sendable {
    let field: String
    let value: String
}

struct WorkspaceTaskMutationReport: Codable, Sendable {
    let version: Int
    let atomic: Bool
    let tasks: [WorkspaceTaskMutationTaskReport]
}

struct MachineTaskMutationDocument: Codable, Sendable {
    let version: Int
    let report: WorkspaceTaskMutationReport
}

struct WorkspaceTaskMutationTaskReport: Codable, Sendable {
    let taskID: String
    let appliedEdits: Int

    enum CodingKeys: String, CodingKey {
        case taskID = "task_id"
        case appliedEdits = "applied_edits"
    }
}
