/**
 Workspace+TaskMutations

 Responsibilities:
 - Apply single-task and bulk task mutations through `ralph machine task mutate`.
 - Build atomic mutation requests from app task state and user edits.
 - Surface optimistic-lock conflicts after refreshing workspace tasks from disk.

 Does not handle:
 - Task creation flows.
 - Queue refresh notifications outside post-mutation reloads.
 - Task conflict diffing helpers (see Workspace+ConflictDetection).

 Invariants/assumptions callers must respect:
 - The workspace must have a configured CLI client before mutations run.
 - Mutation requests are sent as atomic transactions through the machine CLI contract.
 - Expected timestamps are derived from the task snapshot the user edited.
 */

public import Foundation

extension Workspace {
  /// Update a task by applying all changed fields through a single CLI transaction.
  /// - Parameters:
  ///   - original: The original task before any edits.
  ///   - updated: The edited task state to persist.
  ///   - originalUpdatedAt: Optional optimistic-lock timestamp captured when editing began.
  public func updateTask(
    from original: RalphTask, to updated: RalphTask, originalUpdatedAt: Date? = nil
  ) async throws {
    let edits = try taskEdits(from: original, to: updated)
    guard !edits.isEmpty else { return }

    let request = WorkspaceTaskMutationRequest(tasks: [
      WorkspaceTaskMutationSpec(
        taskID: updated.id,
        expectedUpdatedAt: encodeExpectedUpdatedAt(originalUpdatedAt ?? original.updatedAt),
        edits: edits
      )
    ])

    _ = try await executeTaskMutationRequest(
      request,
      operationDescription: "update task \(updated.id)"
    )
  }

  /// Update task status via one CLI transaction.
  public func updateTaskStatus(taskID: String, to newStatus: RalphTaskStatus) async throws {
    guard let task = taskState.tasks.first(where: { $0.id == taskID }) else {
      throw WorkspaceError.cliError("Task not found: \(taskID)")
    }

    let request = WorkspaceTaskMutationRequest(tasks: [
      WorkspaceTaskMutationSpec(
        taskID: taskID,
        expectedUpdatedAt: encodeExpectedUpdatedAt(task.updatedAt),
        edits: [WorkspaceTaskMutationField.status.edit(value: newStatus.rawValue)]
      )
    ])

    _ = try await executeTaskMutationRequest(
      request,
      operationDescription: "update status for \(taskID)"
    )
  }

  /// Update status for multiple tasks in bulk with one atomic CLI transaction.
  @discardableResult
  public func bulkUpdateStatus(
    taskIDs: [String], to newStatus: RalphTaskStatus, skipReload: Bool = false
  ) async throws -> [(String, String)] {
    let selectedTasks = try selectedTasks(for: taskIDs)
    let request = WorkspaceTaskMutationRequest(
      tasks: selectedTasks.map { task in
        WorkspaceTaskMutationSpec(
          taskID: task.id,
          expectedUpdatedAt: encodeExpectedUpdatedAt(task.updatedAt),
          edits: [WorkspaceTaskMutationField.status.edit(value: newStatus.rawValue)]
        )
      })

    _ = try await executeTaskMutationRequest(
      request,
      operationDescription: "bulk update status",
      reloadAfterSuccess: !skipReload
    )
    return []
  }

  /// Update priority for multiple tasks in bulk with one atomic CLI transaction.
  @discardableResult
  public func bulkUpdatePriority(
    taskIDs: [String], to newPriority: RalphTaskPriority, skipReload: Bool = false
  ) async throws -> [(String, String)] {
    let selectedTasks = try selectedTasks(for: taskIDs)
    let request = WorkspaceTaskMutationRequest(
      tasks: selectedTasks.map { task in
        WorkspaceTaskMutationSpec(
          taskID: task.id,
          expectedUpdatedAt: encodeExpectedUpdatedAt(task.updatedAt),
          edits: [WorkspaceTaskMutationField.priority.edit(value: newPriority.rawValue)]
        )
      })

    _ = try await executeTaskMutationRequest(
      request,
      operationDescription: "bulk update priority",
      reloadAfterSuccess: !skipReload
    )
    return []
  }

  /// Update tags for multiple tasks in bulk with one atomic CLI transaction.
  @discardableResult
  public func bulkUpdateTags(
    taskIDs: [String], addTags: [String], removeTags: [String], skipReload: Bool = false
  ) async throws -> [(String, String)] {
    guard !addTags.isEmpty || !removeTags.isEmpty else { return [] }

    let selectedTasks = try selectedTasks(for: taskIDs)
    let request = WorkspaceTaskMutationRequest(
      tasks: selectedTasks.map { task in
        var newTags = task.tags
        newTags.removeAll { removeTags.contains($0) }
        for tag in addTags where !newTags.contains(tag) {
          newTags.append(tag)
        }

        return WorkspaceTaskMutationSpec(
          taskID: task.id,
          expectedUpdatedAt: encodeExpectedUpdatedAt(task.updatedAt),
          edits: [WorkspaceTaskMutationField.tags.edit(list: newTags, separator: ", ")]
        )
      })

    _ = try await executeTaskMutationRequest(
      request,
      operationDescription: "bulk update tags",
      reloadAfterSuccess: !skipReload
    )
    return []
  }
}

extension Workspace {
  fileprivate func taskEdits(from original: RalphTask, to updated: RalphTask) throws
    -> [WorkspaceTaskFieldEdit]
  {
    var edits: [WorkspaceTaskFieldEdit] = []

    if original.title != updated.title {
      edits.append(WorkspaceTaskMutationField.title.edit(value: updated.title))
    }

    if (original.description ?? "") != (updated.description ?? "") {
      edits.append(WorkspaceTaskMutationField.description.edit(value: updated.description ?? ""))
    }

    if original.status != updated.status {
      edits.append(WorkspaceTaskMutationField.status.edit(value: updated.status.rawValue))
    }

    if original.priority != updated.priority {
      edits.append(WorkspaceTaskMutationField.priority.edit(value: updated.priority.rawValue))
    }

    if original.tags != updated.tags {
      edits.append(WorkspaceTaskMutationField.tags.edit(list: updated.tags, separator: ", "))
    }

    if (original.scope ?? []) != (updated.scope ?? []) {
      edits.append(
        WorkspaceTaskMutationField.scope.edit(list: updated.scope ?? [], separator: "\n"))
    }

    if (original.evidence ?? []) != (updated.evidence ?? []) {
      edits.append(
        WorkspaceTaskMutationField.evidence.edit(list: updated.evidence ?? [], separator: "\n"))
    }

    if (original.plan ?? []) != (updated.plan ?? []) {
      edits.append(WorkspaceTaskMutationField.plan.edit(list: updated.plan ?? [], separator: "\n"))
    }

    if (original.notes ?? []) != (updated.notes ?? []) {
      edits.append(
        WorkspaceTaskMutationField.notes.edit(list: updated.notes ?? [], separator: "\n"))
    }

    if (original.request ?? "") != (updated.request ?? "") {
      edits.append(WorkspaceTaskMutationField.request.edit(value: updated.request ?? ""))
    }

    if (original.dependsOn ?? []) != (updated.dependsOn ?? []) {
      edits.append(
        WorkspaceTaskMutationField.dependsOn.edit(list: updated.dependsOn ?? [], separator: ", "))
    }

    if (original.blocks ?? []) != (updated.blocks ?? []) {
      edits.append(
        WorkspaceTaskMutationField.blocks.edit(list: updated.blocks ?? [], separator: ", "))
    }

    if (original.relatesTo ?? []) != (updated.relatesTo ?? []) {
      edits.append(
        WorkspaceTaskMutationField.relatesTo.edit(list: updated.relatesTo ?? [], separator: ", "))
    }

    if (original.duplicates ?? "") != (updated.duplicates ?? "") {
      edits.append(WorkspaceTaskMutationField.duplicates.edit(value: updated.duplicates ?? ""))
    }

    if (original.customFields ?? [:]) != (updated.customFields ?? [:]) {
      edits.append(
        WorkspaceTaskMutationField.customFields.edit(
          value: Self.encodeCustomFields(updated.customFields ?? [:])))
    }

    if original.scheduledStart != updated.scheduledStart {
      edits.append(
        WorkspaceTaskMutationField.scheduledStart.edit(
          value: Self.encodeOptionalDate(updated.scheduledStart)))
    }

    if original.estimatedMinutes != updated.estimatedMinutes {
      edits.append(
        WorkspaceTaskMutationField.estimatedMinutes.edit(
          value: updated.estimatedMinutes.map(String.init) ?? ""))
    }

    if original.actualMinutes != updated.actualMinutes {
      edits.append(
        WorkspaceTaskMutationField.actualMinutes.edit(
          value: updated.actualMinutes.map(String.init) ?? ""))
    }

    let originalAgent = RalphTaskAgent.normalizedOverride(original.agent)
    let updatedAgent = RalphTaskAgent.normalizedOverride(updated.agent)
    if originalAgent != updatedAgent {
      edits.append(
        WorkspaceTaskMutationField.agent.edit(
          value: try Self.encodeTaskAgentFieldValue(updatedAgent)))
    }

    return edits
  }

  fileprivate static func encodeOptionalDate(_ date: Date?) -> String {
    guard let date else { return "" }
    return ISO8601DateFormatter().string(from: date)
  }

  fileprivate static func encodeCustomFields(_ fields: [String: String]) -> String {
    fields
      .sorted { $0.key < $1.key }
      .map { "\($0.key)=\($0.value)" }
      .joined(separator: "\n")
  }

  fileprivate func executeTaskMutationRequest(
    _ request: WorkspaceTaskMutationRequest,
    operationDescription: String,
    reloadAfterSuccess: Bool = true
  ) async throws -> WorkspaceTaskMutationReport {
    guard let client else {
      throw WorkspaceError.cliClientUnavailable
    }

    do {
      let collected = try await withTemporaryJSONFile(
        prefix: "ralph-task-mutation",
        payload: request,
        operationName: operationDescription
      ) { tempFileURL in
        try await client.runAndCollectWithRetry(
          arguments: ["--no-color", "machine", "task", "mutate", "--input", tempFileURL.path],
          currentDirectoryURL: identityState.workingDirectoryURL,
          onRetry: { [weak self] attempt, maxAttempts, _ in
            await MainActor.run { [weak self] in
              self?.runState.errorMessage =
                "Retrying \(operationDescription) (attempt \(attempt)/\(maxAttempts))..."
            }
          }
        )
      }

      let reportData = Data(collected.stdout.utf8)
      let report = try RalphMachineContract.decode(
        MachineTaskMutationDocument.self, from: reportData, operation: "task mutate")

      if reloadAfterSuccess {
        await loadTasks()
      }

      return report.report
    } catch {
      if let conflict = await taskConflictFromMutationFailure(error, request: request) {
        throw WorkspaceError.taskConflict(conflict)
      }
      throw error
    }
  }

  fileprivate func selectedTasks(for taskIDs: [String]) throws -> [RalphTask] {
    let requested = Array(
      NSOrderedSet(
        array: taskIDs.compactMap {
          $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0
        })
    )
    .compactMap { $0 as? String }
    guard !requested.isEmpty else {
      throw WorkspaceError.cliError("No tasks selected.")
    }

    var selected: [RalphTask] = []
    for taskID in requested {
      guard let task = taskState.tasks.first(where: { $0.id == taskID }) else {
        throw WorkspaceError.cliError("Task not found: \(taskID)")
      }
      selected.append(task)
    }
    return selected
  }

  fileprivate func taskConflictFromMutationFailure(
    _ error: any Error,
    request: WorkspaceTaskMutationRequest
  ) async -> RalphTask? {
    let isTaskConflict = {
      if let retryable = error as? RetryableError,
        case .processError(_, let stderr) = retryable,
        MachineErrorDocument.decode(from: stderr)?.code == .taskMutationConflict
      {
        return true
      }
      if let machineError = MachineErrorDocument.decode(from: error.localizedDescription) {
        return machineError.code == .taskMutationConflict
      }
      return error.localizedDescription.contains("Task mutation conflict for")
    }()

    guard isTaskConflict, request.tasks.count == 1 else {
      return nil
    }

    await loadTasks(retryConfiguration: .minimal)
    return taskState.tasks.first(where: { $0.id == request.tasks[0].taskID })
  }

  fileprivate func encodeExpectedUpdatedAt(_ date: Date?) -> String? {
    guard let date else { return nil }
    return ISO8601DateFormatter().string(from: date)
  }

  fileprivate static func encodeTaskAgentFieldValue(_ agent: RalphTaskAgent?) throws -> String {
    guard let agent else { return "" }
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let data = try encoder.encode(agent)
    return String(decoding: data, as: UTF8.self)
  }
}
