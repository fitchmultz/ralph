/**
 WorkspaceTaskMutationModels

 Purpose:
 - Define app-side machine JSON contracts for task mutation and queue continuation workflows.

 Responsibilities:
 - Define app-side machine JSON contracts for task mutation and queue continuation workflows.
 - Provide Codable request/response models used by workspace task mutation and recovery flows.
 - Keep continuation payload encoding and decoding centralized for app integrations.

 Does not handle:
 - Executing CLI processes.
 - Diffing tasks or deciding which edits to send.
 - Queue loading or optimistic-lock conflict resolution.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Field names must match the CLI's `TaskEditKey` snake_case values.
 - `expectedUpdatedAt` is encoded as RFC3339/ISO8601 for optimistic locking.
 - Unknown machine payload bodies remain in `RalphJSONValue` instead of being dropped.
 */

import Foundation

struct WorkspaceContinuationAction: Codable, Sendable, Equatable {
  let title: String
  let command: String
  let detail: String
}

struct WorkspaceContinuationSummary: Decodable, Sendable, Equatable {
  let headline: String
  let detail: String
  let blocking: WorkspaceRunnerController.MachineBlockingState?
  let nextSteps: [WorkspaceContinuationAction]

  enum CodingKeys: String, CodingKey {
    case headline
    case detail
    case blocking
    case nextSteps = "next_steps"
  }
}

struct WorkspaceValidationWarning: Decodable, Sendable, Equatable {
  let taskID: String
  let message: String

  enum CodingKeys: String, CodingKey {
    case taskID = "task_id"
    case message
  }
}

struct MachineQueueValidateDocument: Decodable, Sendable, Equatable, VersionedMachineDocument {
  static let expectedVersion = RalphMachineContract.queueValidateVersion
  static let documentName = "machine queue validate"

  let version: Int
  let valid: Bool
  let blocking: WorkspaceRunnerController.MachineBlockingState?
  let warnings: [WorkspaceValidationWarning]
  let continuation: WorkspaceContinuationSummary

  var effectiveBlocking: WorkspaceRunnerController.MachineBlockingState? {
    blocking ?? continuation.blocking
  }
}

struct MachineQueueRepairDocument: Decodable, Sendable, Equatable, VersionedMachineDocument {
  static let expectedVersion = RalphMachineContract.queueRepairVersion
  static let documentName = "machine queue repair"

  let version: Int
  let dryRun: Bool
  let changed: Bool
  let blocking: WorkspaceRunnerController.MachineBlockingState?
  let report: RalphJSONValue
  let continuation: WorkspaceContinuationSummary

  enum CodingKeys: String, CodingKey {
    case version
    case dryRun = "dry_run"
    case changed
    case blocking
    case report
    case continuation
  }

  var effectiveBlocking: WorkspaceRunnerController.MachineBlockingState? {
    blocking ?? continuation.blocking
  }
}

struct MachineQueueUndoDocument: Decodable, Sendable, Equatable, VersionedMachineDocument {
  static let expectedVersion = RalphMachineContract.queueUndoVersion
  static let documentName = "machine queue undo"

  let version: Int
  let dryRun: Bool
  let restored: Bool
  let blocking: WorkspaceRunnerController.MachineBlockingState?
  let result: RalphJSONValue?
  let continuation: WorkspaceContinuationSummary

  enum CodingKeys: String, CodingKey {
    case version
    case dryRun = "dry_run"
    case restored
    case blocking
    case result
    case continuation
  }

  var effectiveBlocking: WorkspaceRunnerController.MachineBlockingState? {
    blocking ?? continuation.blocking
  }
}

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

struct MachineQueueUnlockInspectDocument: Decodable, Sendable, Equatable, VersionedMachineDocument {
  static let expectedVersion = RalphMachineContract.queueUnlockInspectVersion
  static let documentName = "machine queue unlock inspect"

  enum Condition: String, Decodable, Sendable, Equatable {
    case clear
    case live
    case stale
    case ownerMissing = "owner_missing"
    case ownerUnreadable = "owner_unreadable"
  }

  let version: Int
  let condition: Condition
  let blocking: WorkspaceRunnerController.MachineBlockingState?
  let unlockAllowed: Bool
  let continuation: WorkspaceContinuationSummary

  enum CodingKeys: String, CodingKey {
    case version
    case condition
    case blocking
    case unlockAllowed = "unlock_allowed"
    case continuation
  }
}

struct MachineTaskCreateDocument: Codable, Sendable, VersionedMachineDocument {
  static let expectedVersion = RalphMachineContract.taskCreateVersion
  static let documentName = "machine task create"

  let version: Int
  let task: RalphTask
}

struct MachineTaskBuildRequest: Codable, Sendable {
  let version: Int
  let request: String
  let tags: [String]
  let scope: [String]
  let template: String?
  let target: String?
  let strictTemplates: Bool
  let estimatedMinutes: Int?

  enum CodingKeys: String, CodingKey {
    case version
    case request
    case tags
    case scope
    case template
    case target
    case strictTemplates = "strict_templates"
    case estimatedMinutes = "estimated_minutes"
  }

  init(
    version: Int = RalphMachineContract.taskBuildVersion,
    request: String,
    tags: [String],
    scope: [String],
    template: String?,
    target: String?,
    strictTemplates: Bool,
    estimatedMinutes: Int?
  ) {
    self.version = version
    self.request = request
    self.tags = tags
    self.scope = scope
    self.template = template
    self.target = target
    self.strictTemplates = strictTemplates
    self.estimatedMinutes = estimatedMinutes
  }
}

struct MachineTaskBuildDocument: Decodable, Sendable, VersionedMachineDocument {
  static let expectedVersion = RalphMachineContract.taskBuildVersion
  static let documentName = "machine task build"

  let version: Int
  let mode: String
  let blocking: WorkspaceRunnerController.MachineBlockingState?
  let result: MachineTaskBuildResult
  let warnings: [String]
  let continuation: WorkspaceContinuationSummary

  var effectiveBlocking: WorkspaceRunnerController.MachineBlockingState? {
    blocking ?? continuation.blocking
  }
}

struct MachineTaskBuildResult: Decodable, Sendable {
  let createdCount: Int
  let taskIDs: [String]
  let tasks: [RalphTask]

  enum CodingKeys: String, CodingKey {
    case createdCount = "created_count"
    case taskIDs = "task_ids"
    case tasks
  }
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

enum WorkspaceTaskMutationField: String, Sendable {
  case title
  case description
  case status
  case priority
  case tags
  case scope
  case evidence
  case plan
  case notes
  case request
  case dependsOn = "depends_on"
  case blocks
  case relatesTo = "relates_to"
  case duplicates
  case customFields = "custom_fields"
  case scheduledStart = "scheduled_start"
  case estimatedMinutes = "estimated_minutes"
  case actualMinutes = "actual_minutes"
  case agent

  func edit(value: String) -> WorkspaceTaskFieldEdit {
    WorkspaceTaskFieldEdit(field: self, value: value)
  }

  func edit(list values: [String], separator: String) -> WorkspaceTaskFieldEdit {
    edit(value: values.joined(separator: separator))
  }
}

struct WorkspaceTaskFieldEdit: Codable, Sendable {
  let field: String
  let value: String

  init(field: String, value: String) {
    self.field = field
    self.value = value
  }

  init(field: WorkspaceTaskMutationField, value: String) {
    self.init(field: field.rawValue, value: value)
  }
}

struct WorkspaceTaskMutationReport: Codable, Sendable {
  let version: Int
  let atomic: Bool
  let tasks: [WorkspaceTaskMutationTaskReport]
}

struct MachineTaskMutationDocument: Decodable, Sendable, VersionedMachineDocument {
  static let expectedVersion = RalphMachineContract.taskMutateVersion
  static let documentName = "machine task mutate"

  let version: Int
  let blocking: WorkspaceRunnerController.MachineBlockingState?
  let report: WorkspaceTaskMutationReport
  let continuation: WorkspaceContinuationSummary

  var effectiveBlocking: WorkspaceRunnerController.MachineBlockingState? {
    blocking ?? continuation.blocking
  }
}

struct WorkspaceTaskMutationTaskReport: Codable, Sendable {
  let taskID: String
  let appliedEdits: Int

  enum CodingKeys: String, CodingKey {
    case taskID = "task_id"
    case appliedEdits = "applied_edits"
  }
}
