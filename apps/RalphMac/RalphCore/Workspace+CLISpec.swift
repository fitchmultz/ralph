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
            commandState.cliSpecErrorMessage = "CLI client not available."
            return
        }

        commandState.cliSpecIsLoading = true
        commandState.cliSpecErrorMessage = nil

        do {
            let helper = RetryHelper(configuration: retryConfiguration)
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "__cli-spec", "--format", "json"],
                        currentDirectoryURL: identityState.workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )

            guard collected.status.code == 0 else {
                commandState.cliSpec = nil
                commandState.cliSpecErrorMessage = collected.stderr.isEmpty
                    ? "Failed to load CLI spec (exit \(collected.status.code))."
                    : collected.stderr
                commandState.cliSpecIsLoading = false
                return
            }

            commandState.cliSpec = try JSONDecoder().decode(
                RalphCLISpecDocument.self,
                from: Data(collected.stdout.utf8)
            )
        } catch {
            commandState.cliSpec = nil
            let recoveryError = RecoveryError.classify(
                error: error,
                operation: "loadCLISpec",
                workspaceURL: identityState.workingDirectoryURL
            )
            commandState.cliSpecErrorMessage = recoveryError.message
            diagnosticsState.lastRecoveryError = recoveryError
            diagnosticsState.showErrorRecovery = true
        }

        commandState.cliSpecIsLoading = false
    }

    func advancedCommands() -> [RalphCLICommandSpec] {
        guard let cliSpec = commandState.cliSpec else { return [] }
        var out: [RalphCLICommandSpec] = []
        for sub in cliSpec.root.subcommands {
            collectCommands(sub, includeHidden: commandState.advancedShowHiddenCommands, into: &out)
        }
        return out
    }

    func selectedAdvancedCommand() -> RalphCLICommandSpec? {
        guard let id = commandState.advancedSelectedCommandID else { return nil }
        return advancedCommands().first(where: { $0.id == id })
    }

    func resetAdvancedInputs() {
        commandState.advancedBoolValues.removeAll(keepingCapacity: false)
        commandState.advancedCountValues.removeAll(keepingCapacity: false)
        commandState.advancedSingleValues.removeAll(keepingCapacity: false)
        commandState.advancedMultiValues.removeAll(keepingCapacity: false)
    }

    func buildAdvancedArguments() -> [String] {
        guard let cmd = selectedAdvancedCommand() else { return [] }

        var selections: [String: RalphCLIArgValue] = [:]

        for arg in cmd.args {
            if arg.isCountFlag {
                let n = commandState.advancedCountValues[arg.id] ?? 0
                if n > 0 {
                    selections[arg.id] = .count(n)
                }
                continue
            }

            if arg.isBooleanFlag {
                let present = commandState.advancedBoolValues[arg.id] ?? false
                selections[arg.id] = .flag(present)
                continue
            }

            guard arg.takesValue else { continue }

            if arg.positional || arg.allowsMultipleValues {
                let raw = commandState.advancedMultiValues[arg.id] ?? ""
                let values = raw.split(whereSeparator: \.isNewline)
                    .map { $0.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines) }
                    .filter { !$0.isEmpty }
                if !values.isEmpty {
                    selections[arg.id] = .values(values)
                }
            } else {
                let raw = (commandState.advancedSingleValues[arg.id] ?? "")
                    .trimmingCharacters(in: CharacterSet.whitespacesAndNewlines)
                if !raw.isEmpty {
                    selections[arg.id] = .values([raw])
                }
            }
        }

        var globals: [String] = []
        if commandState.advancedIncludeNoColor {
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
