/**
 Workspace+TaskMutations

 Responsibilities:
 - Apply single-task and bulk task mutations through `ralph task mutate`.
 - Build atomic mutation requests from app task state and user edits.
 - Surface optimistic-lock conflicts after refreshing workspace tasks from disk.

 Does not handle:
 - Task creation flows.
 - Queue refresh notifications outside post-mutation reloads.
 - Task conflict diffing helpers (see Workspace+ConflictDetection).

 Invariants/assumptions callers must respect:
 - The workspace must have a configured CLI client before mutations run.
 - Mutation requests are sent as atomic transactions through the CLI.
 - Expected timestamps are derived from the task snapshot the user edited.
 */

public import Foundation

public extension Workspace {
    /// Update a task by applying all changed fields through a single CLI transaction.
    /// - Parameters:
    ///   - original: The original task before any edits.
    ///   - updated: The edited task state to persist.
    ///   - originalUpdatedAt: Optional optimistic-lock timestamp captured when editing began.
    func updateTask(from original: RalphTask, to updated: RalphTask, originalUpdatedAt: Date? = nil) async throws {
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
    func updateTaskStatus(taskID: String, to newStatus: RalphTaskStatus) async throws {
        guard let task = tasks.first(where: { $0.id == taskID }) else {
            throw WorkspaceError.cliError("Task not found: \(taskID)")
        }

        let request = WorkspaceTaskMutationRequest(tasks: [
            WorkspaceTaskMutationSpec(
                taskID: taskID,
                expectedUpdatedAt: encodeExpectedUpdatedAt(task.updatedAt),
                edits: [WorkspaceTaskFieldEdit(field: "status", value: newStatus.rawValue)]
            )
        ])

        _ = try await executeTaskMutationRequest(
            request,
            operationDescription: "update status for \(taskID)"
        )
    }

    /// Update status for multiple tasks in bulk with one atomic CLI transaction.
    @discardableResult
    func bulkUpdateStatus(taskIDs: [String], to newStatus: RalphTaskStatus, skipReload: Bool = false) async throws -> [(String, String)] {
        let selectedTasks = try selectedTasks(for: taskIDs)
        let request = WorkspaceTaskMutationRequest(tasks: selectedTasks.map { task in
            WorkspaceTaskMutationSpec(
                taskID: task.id,
                expectedUpdatedAt: encodeExpectedUpdatedAt(task.updatedAt),
                edits: [WorkspaceTaskFieldEdit(field: "status", value: newStatus.rawValue)]
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
    func bulkUpdatePriority(taskIDs: [String], to newPriority: RalphTaskPriority, skipReload: Bool = false) async throws -> [(String, String)] {
        let selectedTasks = try selectedTasks(for: taskIDs)
        let request = WorkspaceTaskMutationRequest(tasks: selectedTasks.map { task in
            WorkspaceTaskMutationSpec(
                taskID: task.id,
                expectedUpdatedAt: encodeExpectedUpdatedAt(task.updatedAt),
                edits: [WorkspaceTaskFieldEdit(field: "priority", value: newPriority.rawValue)]
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
    func bulkUpdateTags(taskIDs: [String], addTags: [String], removeTags: [String], skipReload: Bool = false) async throws -> [(String, String)] {
        guard !addTags.isEmpty || !removeTags.isEmpty else { return [] }

        let selectedTasks = try selectedTasks(for: taskIDs)
        let request = WorkspaceTaskMutationRequest(tasks: selectedTasks.map { task in
            var newTags = task.tags
            newTags.removeAll { removeTags.contains($0) }
            for tag in addTags where !newTags.contains(tag) {
                newTags.append(tag)
            }

            return WorkspaceTaskMutationSpec(
                taskID: task.id,
                expectedUpdatedAt: encodeExpectedUpdatedAt(task.updatedAt),
                edits: [WorkspaceTaskFieldEdit(field: "tags", value: newTags.joined(separator: ", "))]
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

private extension Workspace {
    func taskEdits(from original: RalphTask, to updated: RalphTask) throws -> [WorkspaceTaskFieldEdit] {
        var edits: [WorkspaceTaskFieldEdit] = []

        if original.title != updated.title {
            edits.append(WorkspaceTaskFieldEdit(field: "title", value: updated.title))
        }

        if (original.description ?? "") != (updated.description ?? "") {
            edits.append(WorkspaceTaskFieldEdit(field: "description", value: updated.description ?? ""))
        }

        if original.status != updated.status {
            edits.append(WorkspaceTaskFieldEdit(field: "status", value: updated.status.rawValue))
        }

        if original.priority != updated.priority {
            edits.append(WorkspaceTaskFieldEdit(field: "priority", value: updated.priority.rawValue))
        }

        if original.tags != updated.tags {
            edits.append(WorkspaceTaskFieldEdit(field: "tags", value: updated.tags.joined(separator: ", ")))
        }

        if (original.scope ?? []) != (updated.scope ?? []) {
            edits.append(WorkspaceTaskFieldEdit(field: "scope", value: (updated.scope ?? []).joined(separator: "\n")))
        }

        if (original.evidence ?? []) != (updated.evidence ?? []) {
            edits.append(WorkspaceTaskFieldEdit(field: "evidence", value: (updated.evidence ?? []).joined(separator: "\n")))
        }

        if (original.plan ?? []) != (updated.plan ?? []) {
            edits.append(WorkspaceTaskFieldEdit(field: "plan", value: (updated.plan ?? []).joined(separator: "\n")))
        }

        if (original.notes ?? []) != (updated.notes ?? []) {
            edits.append(WorkspaceTaskFieldEdit(field: "notes", value: (updated.notes ?? []).joined(separator: "\n")))
        }

        if (original.dependsOn ?? []) != (updated.dependsOn ?? []) {
            edits.append(WorkspaceTaskFieldEdit(field: "depends_on", value: (updated.dependsOn ?? []).joined(separator: ", ")))
        }

        if (original.blocks ?? []) != (updated.blocks ?? []) {
            edits.append(WorkspaceTaskFieldEdit(field: "blocks", value: (updated.blocks ?? []).joined(separator: ", ")))
        }

        if (original.relatesTo ?? []) != (updated.relatesTo ?? []) {
            edits.append(WorkspaceTaskFieldEdit(field: "relates_to", value: (updated.relatesTo ?? []).joined(separator: ", ")))
        }

        let originalAgent = RalphTaskAgent.normalizedOverride(original.agent)
        let updatedAgent = RalphTaskAgent.normalizedOverride(updated.agent)
        if originalAgent != updatedAgent {
            edits.append(WorkspaceTaskFieldEdit(field: "agent", value: try Self.encodeTaskAgentFieldValue(updatedAgent)))
        }

        return edits
    }

    func executeTaskMutationRequest(
        _ request: WorkspaceTaskMutationRequest,
        operationDescription: String,
        reloadAfterSuccess: Bool = true
    ) async throws -> WorkspaceTaskMutationReport {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        let tempFileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("ralph-task-mutation-\(UUID().uuidString)", isDirectory: false)
            .appendingPathExtension("json")
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(request).write(to: tempFileURL, options: .atomic)
        defer { try? FileManager.default.removeItem(at: tempFileURL) }

        do {
            let collected = try await client.runAndCollectWithRetry(
                arguments: ["--no-color", "task", "mutate", "--input", tempFileURL.path],
                currentDirectoryURL: workingDirectoryURL,
                onRetry: { [weak self] attempt, maxAttempts, _ in
                    await MainActor.run { [weak self] in
                        self?.errorMessage = "Retrying \(operationDescription) (attempt \(attempt)/\(maxAttempts))..."
                    }
                }
            )

            let reportData = Data(collected.stdout.utf8)
            let report = try JSONDecoder().decode(WorkspaceTaskMutationReport.self, from: reportData)

            if reloadAfterSuccess {
                await loadTasks()
            }

            return report
        } catch {
            if let conflict = await taskConflictFromMutationFailure(error, request: request) {
                throw WorkspaceError.taskConflict(conflict)
            }
            throw WorkspaceError.cliError(error.localizedDescription)
        }
    }

    func selectedTasks(for taskIDs: [String]) throws -> [RalphTask] {
        let requested = Array(NSOrderedSet(array: taskIDs.compactMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 }))
            .compactMap { $0 as? String }
        guard !requested.isEmpty else {
            throw WorkspaceError.cliError("No tasks selected.")
        }

        var selected: [RalphTask] = []
        for taskID in requested {
            guard let task = tasks.first(where: { $0.id == taskID }) else {
                throw WorkspaceError.cliError("Task not found: \(taskID)")
            }
            selected.append(task)
        }
        return selected
    }

    func taskConflictFromMutationFailure(
        _ error: any Error,
        request: WorkspaceTaskMutationRequest
    ) async -> RalphTask? {
        let description = error.localizedDescription
        guard description.contains("Task mutation conflict for"),
              request.tasks.count == 1 else {
            return nil
        }

        await loadTasks(retryConfiguration: .minimal)
        return tasks.first(where: { $0.id == request.tasks[0].taskID })
    }

    func encodeExpectedUpdatedAt(_ date: Date?) -> String? {
        guard let date else { return nil }
        return ISO8601DateFormatter().string(from: date)
    }

    static func encodeTaskAgentFieldValue(_ agent: RalphTaskAgent?) throws -> String {
        guard let agent else { return "" }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(agent)
        return String(decoding: data, as: UTF8.self)
    }
}
