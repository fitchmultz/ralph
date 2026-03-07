/**
 Workspace+Decompose

 Responsibilities:
 - Execute `ralph task decompose` preview and write operations through the CLI.
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
        let envelope: TaskDecomposeEnvelope = try await runTaskDecompose(
            source: source,
            options: options,
            write: false,
            decode: TaskDecomposeEnvelope.self
        )
        return envelope.preview
    }

    public func writeTaskDecomposition(
        source: TaskDecomposeSourceInput,
        options: TaskDecomposeOptions
    ) async throws -> TaskDecomposeWriteResult {
        let envelope: TaskDecomposeEnvelope = try await runTaskDecompose(
            source: source,
            options: options,
            write: true,
            decode: TaskDecomposeEnvelope.self
        )
        guard let writeResult = envelope.write else {
            throw WorkspaceError.cliError("task decompose --write returned JSON without a write result payload")
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
                    currentDirectoryURL: workingDirectoryURL,
                    timeoutConfiguration: .longRunning
                )
            },
            onProgress: { [weak self] attempt, maxAttempts, _ in
                await MainActor.run { [weak self] in
                    let verb = write ? "write decomposition" : "preview decomposition"
                    self?.errorMessage = "Retrying \(verb) (attempt \(attempt)/\(maxAttempts))..."
                }
            }
        )

        guard collected.status.code == 0 else {
            throw WorkspaceError.cliError(
                collected.stderr.isEmpty
                    ? "Failed to run task decompose (exit \(collected.status.code))"
                    : collected.stderr
            )
        }

        do {
            let data = Data(collected.stdout.utf8)
            return try JSONDecoder().decode(T.self, from: data)
        } catch {
            throw WorkspaceError.cliError(
                "Failed to decode task decompose JSON output: \(error.localizedDescription)"
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
            "task",
            "decompose",
            sourceArgument(for: source),
            "--format",
            "json",
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
