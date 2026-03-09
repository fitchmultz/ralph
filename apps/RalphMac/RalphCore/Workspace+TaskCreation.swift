/**
 Workspace+TaskCreation

 Responsibilities:
 - Create new tasks through deterministic CLI-backed flows.
 - Reserve task IDs and import structured task payloads.
 - Bridge template-backed task creation into the workspace.

 Does not handle:
 - Editing existing tasks.
 - Bulk task mutations or optimistic-lock enforcement.
 - Queue refresh notifications outside post-create reloads.

 Invariants/assumptions callers must respect:
 - The workspace must have a configured CLI client before task creation runs.
 - Created tasks are reloaded from the CLI after success.
 - Template and direct-import creation both flow through the bundled CLI.
 */

import Foundation

public extension Workspace {
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
                    self?.runState.errorMessage = "Retrying create task (attempt \(attempt)/\(maxAttempts))..."
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
    func reserveNextTaskID(using client: RalphCLIClient) async throws -> String {
        let nextIDResult = try await client.runAndCollect(
            arguments: ["--no-color", "queue", "next-id"],
            currentDirectoryURL: identityState.workingDirectoryURL
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
            currentDirectoryURL: identityState.workingDirectoryURL
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

        return try await withTemporaryJSONFile(
            prefix: "ralph-task-import",
            payload: payload,
            operationName: "import structured task"
        ) { tempFileURL in
            let result = try await client.runAndCollect(
                arguments: [
                    "--no-color",
                    "queue",
                    "import",
                    "--format",
                    "json",
                    "--input",
                    tempFileURL.path,
                ],
                currentDirectoryURL: identityState.workingDirectoryURL
            )
            if result.status.code != 0 {
                throw result.toError()
            }
            return result
        }
    }
}
