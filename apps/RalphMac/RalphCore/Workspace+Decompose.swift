/**
 Workspace+Decompose

 Responsibilities:
 - Execute `ralph machine task decompose` preview and write operations through the CLI.
 - Build deterministic CLI arguments from app-side decomposition inputs.
 - Decode stable JSON responses and refresh workspace task state after writes.

 Does not handle:
 - SwiftUI presentation or form state.
 - Local decomposition planning or queue mutations outside the CLI.

 Invariants/assumptions callers must respect:
 - Preview and write must use the same source/options except for `--write`.
 - Freeform attach workflows use `attachToTaskID`; existing-task decomposition does not.
 - JSON responses must conform to the stable CLI envelope schema.
 */

import Foundation

extension Workspace {
    public func previewTaskDecomposition(
        source: TaskDecomposeSourceInput,
        options: TaskDecomposeOptions
    ) async throws -> DecompositionPreview {
        let document: MachineDecomposeDocument = try await runTaskDecompose(
            source: source,
            options: options,
            write: false,
            decode: MachineDecomposeDocument.self
        )
        return document.result.preview
    }

    public func writeTaskDecomposition(
        source: TaskDecomposeSourceInput,
        options: TaskDecomposeOptions
    ) async throws -> TaskDecomposeWriteResult {
        let document: MachineDecomposeDocument = try await runTaskDecompose(
            source: source,
            options: options,
            write: true,
            decode: MachineDecomposeDocument.self
        )
        guard let writeResult = document.result.write else {
            throw WorkspaceError.cliError("machine task decompose --write returned JSON without a write result payload")
        }
        await loadTasks(retryConfiguration: .minimal)
        return writeResult
    }

    private func runTaskDecompose<T: Decodable>(
        source: TaskDecomposeSourceInput,
        options: TaskDecomposeOptions,
        write: Bool,
        decode: T.Type
    ) async throws -> T {
        guard let client else {
            throw WorkspaceError.cliClientUnavailable
        }

        let helper = RetryHelper(configuration: .default)
        let collected = try await helper.execute(
            operation: { [self] in
                try await client.runAndCollect(
                    arguments: taskDecomposeArguments(source: source, options: options, write: write),
                    currentDirectoryURL: identityState.workingDirectoryURL,
                    timeoutConfiguration: .longRunning
                )
            },
            onProgress: { [weak self] attempt, maxAttempts, _ in
                await MainActor.run { [weak self] in
                    let verb = write ? "write decomposition" : "preview decomposition"
                    self?.runState.errorMessage = "Retrying \(verb) (attempt \(attempt)/\(maxAttempts))..."
                }
            }
        )

        guard collected.status.code == 0 else {
            throw WorkspaceError.cliError(collected.failureMessage(
                fallback: "Failed to run machine task decompose (exit \(collected.status.code))"
            ))
        }

        do {
            let data = Data(collected.stdout.utf8)
            return try JSONDecoder().decode(T.self, from: data)
        } catch {
            throw WorkspaceError.cliError(
                "Failed to decode machine task decompose JSON output: \(error.localizedDescription)"
            )
        }
    }

    private func taskDecomposeArguments(
        source: TaskDecomposeSourceInput,
        options: TaskDecomposeOptions,
        write: Bool
    ) -> [String] {
        var arguments = [
            "--no-color",
            "machine",
            "task",
            "decompose",
            sourceArgument(for: source),
            "--max-depth",
            String(options.maxDepth),
            "--max-children",
            String(options.maxChildren),
            "--max-nodes",
            String(options.maxNodes),
            "--status",
            options.status.rawValue,
            "--child-policy",
            options.childPolicy.rawValue,
        ]

        if options.withDependencies {
            arguments.append("--with-dependencies")
        }

        if case .freeform = source,
           let attachToTaskID = normalizedOptionalString(options.attachToTaskID) {
            arguments.append(contentsOf: ["--attach-to", attachToTaskID])
        }

        if write {
            arguments.append("--write")
        }

        return arguments
    }

    private func sourceArgument(for source: TaskDecomposeSourceInput) -> String {
        switch source {
        case .freeform(let request):
            return request
        case .existingTaskID(let taskID):
            return taskID
        }
    }
}

private func normalizedOptionalString(_ value: String?) -> String? {
    guard let value else { return nil }
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
}
