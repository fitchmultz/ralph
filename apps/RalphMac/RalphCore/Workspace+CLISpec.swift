//! Workspace+CLISpec
//!
//! Purpose:
//! - Load the machine CLI spec document for Advanced Runner.
//!
//! Responsibilities:
//! - Load the machine CLI spec document for Advanced Runner.
//! - Flatten command trees for display and selection.
//! - Build argv arrays from the workspace's advanced-runner UI state.
//!
//! Does not handle:
//! - Running commands.
//! - Queue loading or analytics loading.
//! - Task mutations.
//!
//!
//! Usage:
//! - Used by the RalphMac app or RalphCore tests through its owning feature surface.
//! Invariants/assumptions callers must respect:
//! - The machine CLI spec remains the source of truth for command and arg structure.
//! - Hidden commands and args are filtered app-side only for presentation.
//! - Argument building honors single-value versus multi-value inputs.
//!
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
        await performRepositoryLoad(
            operation: "loadCLISpec",
            retryConfiguration: retryConfiguration,
            setLoading: { [commandState] in commandState.cliSpecIsLoading = $0 },
            clearFailure: { [commandState] in
                commandState.cliSpecErrorMessage = nil
            },
            handleMissingClient: { [commandState] in
                commandState.cliSpec = nil
                commandState.cliSpecErrorMessage = "CLI client not available."
            },
            load: { [self] client, workingDirectoryURL, retryConfiguration, onRetry in
                let document = try await self.decodeMachineRepositoryJSON(
                    MachineCLISpecDocument.self,
                    client: client,
                    machineArguments: ["cli-spec"],
                    currentDirectoryURL: workingDirectoryURL,
                    retryConfiguration: retryConfiguration,
                    onRetry: onRetry
                )
                try Self.validateMachineCLISpecVersion(document)
                return document
            },
            apply: { [commandState] document in
                commandState.cliSpec = document.spec
            },
            handleFailure: { [commandState] recoveryError in
                commandState.cliSpec = nil
                commandState.cliSpecErrorMessage = recoveryError.message
            }
        )
    }

    func advancedCommands() -> [RalphCLICommandSpec] {
        guard
            let cliSpec = commandState.cliSpec,
            let machineRoot = cliSpec.root.subcommands.first(where: { $0.name == "machine" })
        else { return [] }
        var out: [RalphCLICommandSpec] = []
        for sub in machineRoot.subcommands {
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
    nonisolated static let supportedMachineCLISpecDocumentVersion = 2
    nonisolated static let supportedCLISpecVersion = 2

    nonisolated static func validateMachineCLISpecVersion(_ document: MachineCLISpecDocument) throws {
        guard document.version == supportedMachineCLISpecDocumentVersion else {
            throw NSError(
                domain: "RalphMachineContract",
                code: 3,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Unsupported machine cli-spec version \(document.version). RalphMac requires version \(supportedMachineCLISpecDocumentVersion)."
                ]
            )
        }

        guard document.spec.version == supportedCLISpecVersion else {
            throw NSError(
                domain: "RalphMachineContract",
                code: 4,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Unsupported cli-spec schema version \(document.spec.version). RalphMac requires version \(supportedCLISpecVersion)."
                ]
            )
        }
    }

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
