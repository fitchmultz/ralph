//! Workspace+CLISpec
//!
//! Responsibilities:
//! - Load the CLI spec document for Advanced Runner.
//! - Flatten command trees for display and selection.
//! - Build argv arrays from the workspace's advanced-runner UI state.
//!
//! Does not handle:
//! - Running commands.
//! - Queue loading or analytics loading.
//! - Task mutations.
//!
//! Invariants/assumptions callers must respect:
//! - The CLI spec remains the source of truth for command and arg structure.
//! - Hidden commands and args are filtered app-side only for presentation.
//! - Argument building honors single-value versus multi-value inputs.

public import Foundation
public import Combine

@MainActor
public final class WorkspaceCommandState: ObservableObject {
    @Published public var cliSpec: RalphCLISpecDocument?
    @Published public var cliSpecErrorMessage: String?
    @Published public var cliSpecIsLoading = false
    @Published public var advancedSearchText = ""
    @Published public var advancedShowHiddenCommands = false
    @Published public var advancedShowHiddenArgs = false
    @Published public var advancedIncludeNoColor = true
    @Published public var advancedSelectedCommandID: String?
    @Published public var advancedBoolValues: [String: Bool] = [:]
    @Published public var advancedCountValues: [String: Int] = [:]
    @Published public var advancedSingleValues: [String: String] = [:]
    @Published public var advancedMultiValues: [String: String] = [:]

    public init() {}
}

public extension Workspace {
    func loadCLISpec(retryConfiguration: RetryConfiguration = .minimal) async {
        guard let client else {
            cliSpecErrorMessage = "CLI client not available."
            return
        }

        cliSpecIsLoading = true
        cliSpecErrorMessage = nil

        do {
            let helper = RetryHelper(configuration: retryConfiguration)
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "__cli-spec", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )

            guard collected.status.code == 0 else {
                cliSpec = nil
                cliSpecErrorMessage = collected.stderr.isEmpty
                    ? "Failed to load CLI spec (exit \(collected.status.code))."
                    : collected.stderr
                cliSpecIsLoading = false
                return
            }

            cliSpec = try JSONDecoder().decode(RalphCLISpecDocument.self, from: Data(collected.stdout.utf8))
        } catch {
            cliSpec = nil
            let recoveryError = RecoveryError.classify(
                error: error,
                operation: "loadCLISpec",
                workspaceURL: workingDirectoryURL
            )
            cliSpecErrorMessage = recoveryError.message
            lastRecoveryError = recoveryError
            showErrorRecovery = true
        }

        cliSpecIsLoading = false
    }

    func advancedCommands() -> [RalphCLICommandSpec] {
        guard let cliSpec else { return [] }
        var out: [RalphCLICommandSpec] = []
        for sub in cliSpec.root.subcommands {
            collectCommands(sub, includeHidden: advancedShowHiddenCommands, into: &out)
        }
        return out
    }

    func selectedAdvancedCommand() -> RalphCLICommandSpec? {
        guard let id = advancedSelectedCommandID else { return nil }
        return advancedCommands().first(where: { $0.id == id })
    }

    func resetAdvancedInputs() {
        advancedBoolValues.removeAll(keepingCapacity: false)
        advancedCountValues.removeAll(keepingCapacity: false)
        advancedSingleValues.removeAll(keepingCapacity: false)
        advancedMultiValues.removeAll(keepingCapacity: false)
    }

    func buildAdvancedArguments() -> [String] {
        guard let cmd = selectedAdvancedCommand() else { return [] }

        var selections: [String: RalphCLIArgValue] = [:]

        for arg in cmd.args {
            if arg.isCountFlag {
                let n = advancedCountValues[arg.id] ?? 0
                if n > 0 {
                    selections[arg.id] = .count(n)
                }
                continue
            }

            if arg.isBooleanFlag {
                let present = advancedBoolValues[arg.id] ?? false
                selections[arg.id] = .flag(present)
                continue
            }

            guard arg.takesValue else { continue }

            if arg.positional || arg.allowsMultipleValues {
                let raw = advancedMultiValues[arg.id] ?? ""
                let values = raw.split(whereSeparator: \.isNewline)
                    .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                    .filter { !$0.isEmpty }
                if !values.isEmpty {
                    selections[arg.id] = .values(values)
                }
            } else {
                let raw = (advancedSingleValues[arg.id] ?? "")
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                if !raw.isEmpty {
                    selections[arg.id] = .values([raw])
                }
            }
        }

        var globals: [String] = []
        if advancedIncludeNoColor {
            globals.append("--no-color")
        }

        return RalphCLIArgumentBuilder.buildArguments(
            command: cmd,
            selections: selections,
            globalArguments: globals
        )
    }
}

private extension Workspace {
    func collectCommands(
        _ command: RalphCLICommandSpec,
        includeHidden: Bool,
        into out: inout [RalphCLICommandSpec]
    ) {
        if includeHidden || !command.hidden {
            out.append(command)
        }
        for sub in command.subcommands {
            collectCommands(sub, includeHidden: includeHidden, into: &out)
        }
    }
}
