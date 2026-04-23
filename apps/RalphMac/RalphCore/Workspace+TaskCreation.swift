/**
 Workspace+TaskCreation

 Responsibilities:
 - Create new tasks through the machine task-create contract.
 - Encode structured task-create requests from app state.
 - Refresh workspace state after successful machine-side creation.

 Does not handle:
 - Editing existing tasks.
 - Bulk task mutations or optimistic-lock enforcement.
 - Queue refresh notifications outside post-create reloads.

 Invariants/assumptions callers must respect:
 - The workspace must have a configured CLI client before task creation runs.
 - Created tasks are reloaded from the machine queue-read contract after success.
 - Template and direct-create requests both flow through the bundled CLI machine surface.
 */

import Foundation

public extension Workspace {
    /// Create a new task using the versioned machine task-create flow.
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
        let request = MachineTaskCreateRequest(
            title: title,
            description: normalizedOptionalString(description),
            priority: priority.rawValue,
            tags: tags,
            scope: scope ?? [],
            template: normalizedOptionalString(template),
            target: normalizedOptionalString(target)
        )

        let collected = try await helper.execute(
            operation: { [self] in
                try await createTaskUsingMachineRequest(
                    client: client,
                    request: request
                )
            },
            onProgress: { [weak self] attempt, maxAttempts, _ in
                await MainActor.run { [weak self] in
                    self?.runState.errorMessage = "Retrying create task (attempt \(attempt)/\(maxAttempts))..."
                }
            }
        )

        guard collected.status.code == 0 else {
            throw WorkspaceError.cliError(collected.failureMessage(
                fallback: "Failed to create task (exit \(collected.status.code))"
            ))
        }

        await loadTasks(retryConfiguration: .minimal)
    }
}

private extension Workspace {
    func createTaskUsingMachineRequest(
        client: RalphCLIClient,
        request: MachineTaskCreateRequest
    ) async throws -> RalphCLIClient.CollectedOutput {
        return try await withTemporaryJSONFile(
            prefix: "ralph-machine-task-create",
            payload: request,
            operationName: "create task"
        ) { tempFileURL in
            let result = try await client.runAndCollect(
                arguments: [
                    "--no-color",
                    "machine",
                    "task",
                    "create",
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

private func normalizedOptionalString(_ value: String?) -> String? {
    guard let value else { return nil }
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
}
