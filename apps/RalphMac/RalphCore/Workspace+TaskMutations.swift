//! Workspace+TaskMutations
//!
//! Responsibilities:
//! - Apply single-task edits through the Ralph CLI with retry and optimistic locking.
//! - Execute bulk task mutations and surface aggregated failures clearly.
//! - Create new tasks through the direct queue-import and template-backed flows.
//!
//! Does not handle:
//! - Task filtering or task presentation for views.
//! - Runner execution state, looping, or cancellation.
//! - Publishing queue refreshes from file-watcher events.
//!
//! Invariants/assumptions callers must respect:
//! - The workspace must have a configured CLI client before mutations run.
//! - Task creation must go through the CLI-backed queue mutation flows.
//! - Callers rely on `loadTasks()` to publish the final post-mutation state.

public import Foundation

public extension Workspace {
    /// Update a task by applying changes via the CLI and reloading the task list.
    /// This compares the original task with the updated task and generates appropriate CLI commands.
    /// - Parameters:
    ///   - original: The original task before any edits
    ///   - updated: The updated task with edits applied
    ///   - originalUpdatedAt: The updatedAt timestamp at the time editing began (for optimistic locking)
    /// - Throws: WorkspaceError.taskConflict if the task has been modified externally
    func updateTask(from original: RalphTask, to updated: RalphTask, originalUpdatedAt: Date? = nil) async throws {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        if let originalUpdatedAt = originalUpdatedAt {
            if let currentTask = tasks.first(where: { $0.id == updated.id }),
               let currentUpdatedAt = currentTask.updatedAt,
               currentUpdatedAt != originalUpdatedAt {
                throw WorkspaceError.taskConflict(currentTask)
            }
        }

        var editCommands: [(field: String, value: String)] = []

        if original.title != updated.title {
            editCommands.append(("title", updated.title))
        }

        let originalDesc = original.description ?? ""
        let updatedDesc = updated.description ?? ""
        if originalDesc != updatedDesc {
            editCommands.append(("description", updatedDesc))
        }

        if original.status != updated.status {
            editCommands.append(("status", updated.status.rawValue))
        }

        if original.priority != updated.priority {
            editCommands.append(("priority", updated.priority.rawValue))
        }

        if original.tags != updated.tags {
            let value = updated.tags.joined(separator: ", ")
            editCommands.append(("tags", value))
        }

        let originalScope = original.scope ?? []
        let updatedScope = updated.scope ?? []
        if originalScope != updatedScope {
            let value = updatedScope.joined(separator: "\n")
            editCommands.append(("scope", value))
        }

        let originalEvidence = original.evidence ?? []
        let updatedEvidence = updated.evidence ?? []
        if originalEvidence != updatedEvidence {
            let value = updatedEvidence.joined(separator: "\n")
            editCommands.append(("evidence", value))
        }

        let originalPlan = original.plan ?? []
        let updatedPlan = updated.plan ?? []
        if originalPlan != updatedPlan {
            let value = updatedPlan.joined(separator: "\n")
            editCommands.append(("plan", value))
        }

        let originalNotes = original.notes ?? []
        let updatedNotes = updated.notes ?? []
        if originalNotes != updatedNotes {
            let value = updatedNotes.joined(separator: "\n")
            editCommands.append(("notes", value))
        }

        let originalDepends = original.dependsOn ?? []
        let updatedDepends = updated.dependsOn ?? []
        if originalDepends != updatedDepends {
            let value = updatedDepends.joined(separator: ", ")
            editCommands.append(("depends_on", value))
        }

        let originalBlocks = original.blocks ?? []
        let updatedBlocks = updated.blocks ?? []
        if originalBlocks != updatedBlocks {
            let value = updatedBlocks.joined(separator: ", ")
            editCommands.append(("blocks", value))
        }

        let originalRelates = original.relatesTo ?? []
        let updatedRelates = updated.relatesTo ?? []
        if originalRelates != updatedRelates {
            let value = updatedRelates.joined(separator: ", ")
            editCommands.append(("relates_to", value))
        }

        let originalAgent = RalphTaskAgent.normalizedOverride(original.agent)
        let updatedAgent = RalphTaskAgent.normalizedOverride(updated.agent)
        if originalAgent != updatedAgent {
            let value = try Self.encodeTaskAgentFieldValue(updatedAgent)
            editCommands.append(("agent", value))
        }

        let helper = RetryHelper(configuration: .default)

        for (field, value) in editCommands {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "task", "edit", field, value, updated.id],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                },
                onProgress: { [weak self] attempt, maxAttempts, _ in
                    await MainActor.run { [weak self] in
                        self?.errorMessage = "Retrying edit \(field) (attempt \(attempt)/\(maxAttempts))..."
                    }
                }
            )

            guard collected.status.code == 0 else {
                throw WorkspaceError.cliError(
                    "Failed to edit \(field): \(collected.stderr.isEmpty ? "Exit \(collected.status.code)" : collected.stderr)"
                )
            }
        }

        await loadTasks()
    }

    /// Update status for multiple tasks in bulk.
    /// Executes CLI commands for each task sequentially with retry logic.
    /// - Parameters:
    ///   - taskIDs: Array of task IDs to update
    ///   - newStatus: The new status to apply
    ///   - skipReload: If true, skips calling `loadTasks()` after completion (useful when combining multiple bulk operations)
    /// - Returns: Array of (taskID, error) tuples for any failures (empty if all succeeded)
    @discardableResult
    func bulkUpdateStatus(taskIDs: [String], to newStatus: RalphTaskStatus, skipReload: Bool = false) async throws -> [(String, String)] {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        let helper = RetryHelper(configuration: .default)
        var failures: [(String, String)] = []

        for taskID in taskIDs {
            let arguments = ["--no-color", "task", "edit", "status", newStatus.rawValue, taskID]

            do {
                _ = try await helper.execute(
                    operation: { [self] in
                        let result = try await client.runAndCollect(
                            arguments: arguments,
                            currentDirectoryURL: workingDirectoryURL
                        )
                        if result.status.code != 0 {
                            throw result.toError()
                        }
                        return result
                    }
                )

                if newStatus == .doing {
                    let dateFormatter = ISO8601DateFormatter()
                    let startedAt = dateFormatter.string(from: Date())
                    _ = try? await helper.execute(
                        operation: { [self] in
                            let result = try await client.runAndCollect(
                                arguments: ["--no-color", "task", "edit", "started_at", startedAt, taskID],
                                currentDirectoryURL: workingDirectoryURL
                            )
                            return result
                        }
                    )
                }
            } catch {
                failures.append((taskID, error.localizedDescription))
            }
        }

        if !skipReload {
            await loadTasks()
        }

        if !failures.isEmpty {
            throw WorkspaceError.cliError(formatBulkFailureMessage(failures, operation: "status"))
        }
        return failures
    }

    /// Update priority for multiple tasks in bulk.
    /// - Parameters:
    ///   - taskIDs: Array of task IDs to update
    ///   - newPriority: The new priority to apply
    ///   - skipReload: If true, skips calling `loadTasks()` after completion
    /// - Returns: Array of (taskID, error) tuples for any failures (empty if all succeeded)
    @discardableResult
    func bulkUpdatePriority(taskIDs: [String], to newPriority: RalphTaskPriority, skipReload: Bool = false) async throws -> [(String, String)] {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        let helper = RetryHelper(configuration: .default)
        var failures: [(String, String)] = []

        for taskID in taskIDs {
            let arguments = ["--no-color", "task", "edit", "priority", newPriority.rawValue, taskID]

            do {
                _ = try await helper.execute(
                    operation: { [self] in
                        let result = try await client.runAndCollect(
                            arguments: arguments,
                            currentDirectoryURL: workingDirectoryURL
                        )
                        if result.status.code != 0 {
                            throw result.toError()
                        }
                        return result
                    }
                )
            } catch {
                failures.append((taskID, error.localizedDescription))
            }
        }

        if !skipReload {
            await loadTasks()
        }

        if !failures.isEmpty {
            throw WorkspaceError.cliError(formatBulkFailureMessage(failures, operation: "priority"))
        }
        return failures
    }

    /// Update tags for multiple tasks in bulk.
    /// - Parameters:
    ///   - taskIDs: Array of task IDs to update
    ///   - addTags: Tags to add to each task
    ///   - removeTags: Tags to remove from each task
    ///   - skipReload: If true, skips calling `loadTasks()` after completion
    /// - Returns: Array of (taskID, error) tuples for any failures (empty if all succeeded)
    @discardableResult
    func bulkUpdateTags(taskIDs: [String], addTags: [String], removeTags: [String], skipReload: Bool = false) async throws -> [(String, String)] {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        guard !addTags.isEmpty || !removeTags.isEmpty else { return [] }

        let helper = RetryHelper(configuration: .default)
        var failures: [(String, String)] = []

        for taskID in taskIDs {
            guard let task = tasks.first(where: { $0.id == taskID }) else {
                failures.append((taskID, "Task not found"))
                continue
            }

            var newTags = task.tags
            newTags.removeAll { removeTags.contains($0) }

            for tag in addTags where !newTags.contains(tag) {
                newTags.append(tag)
            }

            let value = newTags.joined(separator: ", ")
            let arguments = ["--no-color", "task", "edit", "tags", value, taskID]

            do {
                _ = try await helper.execute(
                    operation: { [self] in
                        let result = try await client.runAndCollect(
                            arguments: arguments,
                            currentDirectoryURL: workingDirectoryURL
                        )
                        if result.status.code != 0 {
                            throw result.toError()
                        }
                        return result
                    }
                )
            } catch {
                failures.append((taskID, error.localizedDescription))
            }
        }

        if !skipReload {
            await loadTasks()
        }

        if !failures.isEmpty {
            throw WorkspaceError.cliError(formatBulkFailureMessage(failures, operation: "tags"))
        }
        return failures
    }

    /// Create a new task using a deterministic direct-create flow.
    ///
    /// Non-template tasks are created via `queue next-id` + `queue import --format json` so the
    /// app can create structured tasks immediately without invoking the AI task builder.
    /// Template-backed tasks use `task from template`, which is the explicit direct template-create
    /// command in the CLI.
    func createTask(
        title: String,
        description: String? = nil,
        priority: RalphTaskPriority,
        tags: [String] = [],
        scope: [String]? = nil,
        template: String? = nil,
        target: String? = nil
    ) async throws {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        let helper = RetryHelper(configuration: .default)
        let nextTaskID = try await reserveNextTaskID(using: client)

        let collected = try await helper.execute(
            operation: { [self] in
                if let template {
                    return try await createTaskFromTemplate(
                        client: client,
                        taskID: nextTaskID,
                        template: template,
                        title: title,
                        tags: tags,
                        target: target
                    )
                }

                return try await importStructuredTask(
                    client: client,
                    taskID: nextTaskID,
                    title: title,
                    description: description,
                    priority: priority,
                    tags: tags,
                    scope: scope
                )
            },
            onProgress: { [weak self] attempt, maxAttempts, _ in
                await MainActor.run { [weak self] in
                    self?.errorMessage = "Retrying create task (attempt \(attempt)/\(maxAttempts))..."
                }
            }
        )

        guard collected.status.code == 0 else {
            throw WorkspaceError.cliError(
                collected.stderr.isEmpty ? "Failed to create task (exit \(collected.status.code))" : collected.stderr
            )
        }

        await loadTasks(retryConfiguration: .minimal)
    }
}

private extension Workspace {
    /// Formats a list of failures into a truncated error message.
    /// Shows up to `maxShown` failures with a count summary if truncated.
    func formatBulkFailureMessage(_ failures: [(String, String)], operation: String, maxShown: Int = 5) -> String {
        let shown = failures.prefix(maxShown)
        let failureList = shown.map { "\($0.0): \($0.1)" }.joined(separator: "; ")
        if failures.count > maxShown {
            return "Partial failure updating \(operation): \(failureList); and \(failures.count - maxShown) more"
        }
        return "Partial failure updating \(operation): \(failureList)"
    }

    static func encodeTaskAgentFieldValue(_ agent: RalphTaskAgent?) throws -> String {
        guard let agent else { return "" }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(agent)
        return String(decoding: data, as: UTF8.self)
    }

    func reserveNextTaskID(using client: RalphCLIClient) async throws -> String {
        let nextIDResult = try await client.runAndCollect(
            arguments: ["--no-color", "queue", "next-id"],
            currentDirectoryURL: workingDirectoryURL
        )
        guard nextIDResult.status.code == 0 else {
            throw nextIDResult.toError()
        }

        let nextTaskID = nextIDResult.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !nextTaskID.isEmpty else {
            throw WorkspaceError.cliError("queue next-id returned an empty task ID")
        }
        return nextTaskID
    }

    func createTaskFromTemplate(
        client: RalphCLIClient,
        taskID: String,
        template: String,
        title: String,
        tags: [String],
        target: String?
    ) async throws -> RalphCLIClient.CollectedOutput {
        var arguments: [String] = ["--no-color", "task", "from", "template", template, "--title", title]
        if !tags.isEmpty {
            arguments.append(contentsOf: ["--tags", tags.joined(separator: ",")])
        }
        if let target, !target.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            arguments.append(contentsOf: ["--set", "target=\(target)"])
        }

        let result = try await client.runAndCollect(
            arguments: arguments,
            currentDirectoryURL: workingDirectoryURL
        )
        if result.status.code != 0 {
            throw result.toError()
        }

        RalphLogger.shared.info("Created template task \(taskID) via task from template", category: .workspace)
        return result
    }

    func importStructuredTask(
        client: RalphCLIClient,
        taskID: String,
        title: String,
        description: String?,
        priority: RalphTaskPriority,
        tags: [String],
        scope: [String]?
    ) async throws -> RalphCLIClient.CollectedOutput {
        struct ImportedTask: Encodable {
            let id: String
            let status: String
            let title: String
            let description: String?
            let priority: String
            let tags: [String]
            let scope: [String]?
            let createdAt: String
            let updatedAt: String

            enum CodingKeys: String, CodingKey {
                case id
                case status
                case title
                case description
                case priority
                case tags
                case scope
                case createdAt = "created_at"
                case updatedAt = "updated_at"
            }
        }

        let timestamp = ISO8601DateFormatter().string(from: Date())
        let payload = [ImportedTask(
            id: taskID,
            status: RalphTaskStatus.todo.rawValue,
            title: title,
            description: description?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false ? description : nil,
            priority: priority.rawValue,
            tags: tags,
            scope: scope,
            createdAt: timestamp,
            updatedAt: timestamp
        )]

        let tempFileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("ralph-task-import-\(UUID().uuidString)", isDirectory: false)
            .appendingPathExtension("json")
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(payload).write(to: tempFileURL, options: .atomic)
        defer { try? FileManager.default.removeItem(at: tempFileURL) }

        let result = try await client.runAndCollect(
            arguments: [
                "--no-color",
                "queue",
                "import",
                "--format",
                "json",
                "--input",
                tempFileURL.path
            ],
            currentDirectoryURL: workingDirectoryURL
        )
        if result.status.code != 0 {
            throw result.toError()
        }

        return result
    }
}
