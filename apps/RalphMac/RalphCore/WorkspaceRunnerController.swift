/**
 WorkspaceRunnerController

 Purpose:
 - Own the live Ralph subprocess lifecycle for one workspace.

 Responsibilities:
 - Load resolved runner configuration for the workspace.
 - Consume machine run-event streams and derive UI state from structured envelopes.
 - Launch one-shot runs and task loops from app controls.

 Does not handle:
 - Queue watching or queue decoding.
 - Task filtering or presentation.
 - App-wide CLI bootstrap.

 Usage:
 - Owned by a `Workspace` and called from Run Control, app commands, and tests.
 - Keep feature slices in adjacent `WorkspaceRunnerController+...` files.

 Invariants/assumptions callers must respect:
 - Only one active CLI run may exist per workspace.
 - All public methods are main-actor entry points.
 - Loop mode is owned by the subprocess until it exits or receives a stop signal.
 - Behavioral implementation lives in adjacent `WorkspaceRunnerController+...` files.
 */

import Foundation

@MainActor
final class WorkspaceRunnerController {

    weak var workspace: Workspace?
    var activeRun: RalphCLIRun?
    var cancelRequested = false
    var loopStopSignalTask: Task<Void, Never>?
    var runCancellationTask: Task<Void, Never>?
    var runTask: Task<Void, Never>?
    var runTaskRevision: UInt64 = 0

    init(workspace: Workspace) {
        self.workspace = workspace
    }

    deinit {
        loopStopSignalTask?.cancel()
        runCancellationTask?.cancel()
        runTask?.cancel()
    }

    func applyResumeProjection(_ decision: MachineResumeDecision?, workspace: Workspace) {
        workspace.runState.resumeState = decision?.asWorkspaceResumeState()
    }

    func applyConfigResolveDocument(_ document: MachineConfigResolveDocument, workspace: Workspace) {
        workspace.updateResolvedPaths(document.paths)
        applyResumeProjection(document.resumePreview, workspace: workspace)
        let safety = document.safety
        workspace.runState.currentRunnerConfig = Workspace.RunnerConfig(
            model: document.config.agent?.model,
            phases: document.config.agent?.phases,
            maxIterations: document.config.agent?.iterations,
            safety: Workspace.RunnerSafetySummary(
                repoTrusted: safety.repoTrusted,
                dirtyRepo: safety.dirtyRepo,
                gitPublishMode: safety.gitPublishMode,
                approvalMode: safety.approvalMode,
                ciGateEnabled: safety.ciGateEnabled,
                gitRevertMode: safety.gitRevertMode,
                parallelConfigured: safety.parallelConfigured,
                executionInteractivity: safety.executionInteractivity,
                interactiveApprovalSupported: safety.interactiveApprovalSupported
            )
        )
        workspace.runState.runnerConfigErrorMessage = nil
    }

    func clearRunnerConfigState(_ runState: WorkspaceRunState) {
        runState.currentRunnerConfig = nil
    }

    func loadParallelStatus(retryConfiguration: RetryConfiguration = .minimal) async {
        guard let workspace, !workspace.isShutDown else { return }
        await workspace.performRepositoryLoad(
            operation: "loadParallelStatus",
            retryConfiguration: retryConfiguration,
            setLoading: { [runState = workspace.runState] in runState.parallelStatusLoading = $0 },
            clearFailure: { [runState = workspace.runState] in
                runState.parallelStatusErrorMessage = nil
            },
            handleMissingClient: { [runState = workspace.runState] in
                runState.parallelStatusErrorMessage = "CLI client not available."
            },
            load: { client, workingDirectoryURL, retryConfiguration, onRetry in
                try await workspace.decodeMachineRepositoryJSON(
                    MachineParallelStatusDocument.self,
                    client: client,
                    machineArguments: ["run", "parallel-status"],
                    currentDirectoryURL: workingDirectoryURL,
                    retryConfiguration: retryConfiguration,
                    onRetry: onRetry
                )
            },
            apply: { [runState = workspace.runState] decoded in
                runState.parallelStatus = decoded.asWorkspaceParallelStatus()
                runState.parallelStatusErrorMessage = nil
            },
            handleFailure: { [runState = workspace.runState] recoveryError in
                runState.parallelStatusErrorMessage = "Failed to load shared parallel status."
                RalphLogger.shared.error(
                    "Failed to load shared parallel status: \(recoveryError.fullErrorDetails)",
                    category: .workspace
                )
            }
        )
    }

    func loadRunnerConfiguration(retryConfiguration: RetryConfiguration = .minimal) async {
        guard let workspace, !workspace.isShutDown else { return }
        await workspace.performRepositoryLoad(
            operation: "loadRunnerConfiguration",
            retryConfiguration: retryConfiguration,
            setLoading: { [runState = workspace.runState] in runState.runnerConfigLoading = $0 },
            clearFailure: { [runState = workspace.runState] in
                runState.runnerConfigErrorMessage = nil
            },
            handleMissingClient: { [self, runState = workspace.runState] in
                clearRunnerConfigState(runState)
                runState.runnerConfigErrorMessage = "CLI client not available."
            },
            load: { client, workingDirectoryURL, retryConfiguration, onRetry in
                try await workspace.decodeMachineRepositoryJSON(
                    MachineConfigResolveDocument.self,
                    client: client,
                    machineArguments: ["config", "resolve"],
                    currentDirectoryURL: workingDirectoryURL,
                    retryConfiguration: retryConfiguration,
                    onRetry: onRetry
                )
            },
            apply: { [self, workspace] decoded in
                applyConfigResolveDocument(decoded, workspace: workspace)
            },
            handleFailure: { [self, runState = workspace.runState] recoveryError in
                clearRunnerConfigState(runState)
                runState.runnerConfigErrorMessage = recoveryError.message
                RalphLogger.shared.error(
                    "Failed to load runner configuration: \(recoveryError.fullErrorDetails)",
                    category: .workspace
                )
            }
        )
    }

    func prepareForRepositoryRetarget() {
        guard let workspace else { return }
        loopStopSignalTask?.cancel()
        loopStopSignalTask = nil
        workspace.runState.isLoopMode = false
        cancelRequested = false

        cancelPendingRunTask()

        let runToCancel = activeRun
        activeRun = nil
        if let runToCancel {
            scheduleRunCancellation(runToCancel)
        }
    }

    func run(arguments: [String], preservingConsole: Bool = false) {
        guard let workspace, !workspace.isShutDown else { return }
        guard workspace.client != nil else {
            workspace.runState.errorMessage = "CLI client not available."
            return
        }
        guard !hasPendingRunWork(for: workspace) else { return }

        cancelRequested = false

        scheduleRunTask(preservingConsole: preservingConsole) { _, _ in
            arguments
        }
    }

    func runMachine(arguments: [String], preservingConsole: Bool = false) {
        run(arguments: ["--no-color", "machine"] + arguments, preservingConsole: preservingConsole)
    }

    func cancel() {
        guard let workspace else { return }

        workspace.runState.isLoopMode = false
        workspace.runState.stopAfterCurrent = true
        loopStopSignalTask?.cancel()
        loopStopSignalTask = nil

        if activeRun == nil {
            cancelPendingRunTask()
            return
        }

        cancelRequested = true

        guard let run = activeRun else { return }
        scheduleRunCancellation(run)
    }

    func runNextTask(
        taskIDOverride: String? = nil,
        forceDirtyRepo: Bool = false,
        preservingConsole: Bool = false
    ) {
        guard let workspace, !workspace.isShutDown else { return }
        guard !hasPendingRunWork(for: workspace) else { return }

        scheduleRunTask(preservingConsole: preservingConsole) { [weak self] workspace, repositoryContext in
            let requestedTaskID = taskIDOverride?.trimmingCharacters(in: .whitespacesAndNewlines)
            let selectedTaskID = requestedTaskID.flatMap { $0.isEmpty ? nil : $0 }
            let resolvedTaskID = if let selectedTaskID {
                selectedTaskID
            } else {
                await self?.resolveNextRunnableTaskID(repositoryContext: repositoryContext)
            }

            guard !Task.isCancelled, workspace.isCurrentRepositoryContext(repositoryContext) else {
                return nil
            }

            workspace.runState.currentTaskID = resolvedTaskID

            var arguments = ["--no-color", "machine", "run", "one", "--resume"]
            if forceDirtyRepo {
                arguments.append("--force")
            }
            if let resolvedTaskID {
                arguments.append(contentsOf: ["--id", resolvedTaskID])
            }
            return arguments
        }
    }

    func startLoop(forceDirtyRepo: Bool? = nil, parallelWorkers: Int? = nil) {
        guard let workspace, !workspace.isShutDown else { return }
        guard !hasPendingRunWork(for: workspace) else { return }
        guard workspace.client != nil else {
            workspace.runState.errorMessage = "CLI client not available."
            return
        }

        workspace.runState.isLoopMode = true
        workspace.runState.stopAfterCurrent = false
        let shouldForceDirtyRepo = forceDirtyRepo ?? workspace.runState.runControlForceDirtyRepo
        let requestedParallelWorkers = parallelWorkers ?? workspace.runState.runControlParallelWorkersOverride

        var arguments = ["--no-color", "machine", "run", "loop", "--resume", "--max-tasks", "0"]
        if shouldForceDirtyRepo {
            arguments.append("--force")
        }
        if let requestedParallelWorkers, requestedParallelWorkers >= 2 {
            arguments.append(contentsOf: ["--parallel", String(requestedParallelWorkers)])
        }
        run(arguments: arguments)
    }

    func stopLoop() {
        guard let workspace else { return }
        workspace.runState.stopAfterCurrent = true

        if activeRun == nil {
            workspace.runState.isLoopMode = false
            cancelPendingRunTask()
        } else {
            scheduleLoopStopSignalRequest()
        }
    }

}
