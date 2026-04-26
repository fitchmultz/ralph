/**
 WorkspaceRunnerController+Execution

 Purpose:
 - Schedule workspace run tasks and own subprocess finalization.

 Responsibilities:
 - Schedule workspace run tasks and own subprocess finalization.
 - Request CLI loop stop signals without cancelling the active run immediately.
 - Keep cancellation and repository-retarget cleanup centralized outside the facade file.

 Does not handle:
 - Machine output decoding details.
 - Queue watching.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import Foundation

@MainActor
extension WorkspaceRunnerController {
    func hasPendingRunWork(for workspace: Workspace) -> Bool {
        runTask != nil || activeRun != nil || workspace.runState.isExecutionActive
    }

    func scheduleRunTask(
        preservingConsole: Bool,
        operation: @escaping @MainActor (Workspace, Workspace.RepositoryContext) async -> [String]?
    ) {
        guard let workspace, !workspace.isShutDown else { return }

        runTaskRevision &+= 1
        let revision = runTaskRevision
        let repositoryContext = workspace.currentRepositoryContext()
        workspace.runState.isPreparingRun = true
        runTask = Task { @MainActor [weak self] in
            guard let self else { return }
            defer { self.finishRunTask(revision) }
            await self.executeRunTask(
                revision: revision,
                repositoryContext: repositoryContext,
                preservingConsole: preservingConsole,
                operation: operation
            )
        }
    }

    func executeRunTask(
        revision: UInt64,
        repositoryContext: Workspace.RepositoryContext,
        preservingConsole: Bool,
        operation: @escaping @MainActor (Workspace, Workspace.RepositoryContext) async -> [String]?
    ) async {
        guard
            let workspace,
            !Task.isCancelled,
            !workspace.isShutDown,
            runTaskRevision == revision,
            workspace.isCurrentRepositoryContext(repositoryContext)
        else {
            return
        }

        guard let arguments = await operation(workspace, repositoryContext) else {
            workspace.runState.isPreparingRun = false
            return
        }

        guard
            !Task.isCancelled,
            !workspace.isShutDown,
            runTaskRevision == revision,
            workspace.isCurrentRepositoryContext(repositoryContext)
        else {
            return
        }

        guard let client = workspace.client else {
            workspace.runState.errorMessage = "CLI client not available."
            workspace.runState.isPreparingRun = false
            return
        }

        do {
            let run = try client.start(
                arguments: arguments,
                currentDirectoryURL: workspace.identityState.workingDirectoryURL
            )
            activeRun = run

            guard
                !Task.isCancelled,
                !workspace.isShutDown,
                runTaskRevision == revision,
                workspace.isCurrentRepositoryContext(repositoryContext)
            else {
                activeRun = nil
                workspace.runState.isPreparingRun = false
                await run.cancel()
                return
            }

            workspace.runState.prepareForNewRun(preservingConsole: preservingConsole)
            var machineDecoder = MachineRunOutputDecoder()
            let usesMachineRunEvents = Self.isMachineRunCommand(arguments)

            for await event in run.events {
                if Task.isCancelled || runTaskRevision != revision {
                    await run.cancel()
                    continue
                }
                guard workspace.isCurrentRepositoryContext(repositoryContext), activeRun === run else { continue }
                if usesMachineRunEvents, event.stream == .stdout {
                    for item in machineDecoder.append(event.text) {
                        applyMachineRunOutputItem(item, workspace: workspace)
                    }
                } else {
                    appendConsoleText(event.text, workspace: workspace)
                }
            }

            if usesMachineRunEvents,
               !Task.isCancelled,
               runTaskRevision == revision,
               workspace.isCurrentRepositoryContext(repositoryContext),
               activeRun === run {
                for item in machineDecoder.finish() {
                    applyMachineRunOutputItem(item, workspace: workspace)
                }
            }

            let status = await run.waitUntilExit()
            guard !Task.isCancelled, runTaskRevision == revision else { return }
            finalizeRun(
                status: status,
                run: run,
                repositoryContext: repositoryContext,
                workspace: workspace
            )
        } catch is CancellationError {
            return
        } catch {
            guard workspace.isCurrentRepositoryContext(repositoryContext), runTaskRevision == revision else { return }
            let recoveryError = RecoveryError.classify(
                error: error,
                operation: "run",
                workspaceURL: workspace.identityState.workingDirectoryURL
            )
            workspace.runState.flushConsoleRenderState()
            workspace.runState.errorMessage = recoveryError.message
            workspace.diagnosticsState.lastRecoveryError = recoveryError
            workspace.diagnosticsState.showErrorRecovery = true
            workspace.runState.isPreparingRun = false
            workspace.runState.isRunning = false
            workspace.runState.isLoopMode = false
            workspace.runState.stopAfterCurrent = false
            activeRun = nil
            runCancellationTask = nil
            loopStopSignalTask?.cancel()
            loopStopSignalTask = nil
            cancelRequested = false
            workspace.resetExecutionState()
        }
    }

    func finishRunTask(_ revision: UInt64) {
        guard runTaskRevision == revision else { return }
        runTask = nil
        if let workspace, !workspace.runState.isRunning {
            workspace.runState.isPreparingRun = false
        }
    }

    func cancelPendingRunTask() {
        runTask?.cancel()
        runTask = nil
        runTaskRevision &+= 1
        if let workspace, activeRun == nil {
            workspace.runState.isPreparingRun = false
            workspace.runState.isRunning = false
            workspace.resetExecutionState()
        }
    }

    func scheduleRunCancellation(_ run: RalphCLIRun) {
        runCancellationTask?.cancel()
        runCancellationTask = Task { @MainActor [weak self] in
            await run.cancel()
            guard let self, self.activeRun == nil else { return }
            self.runCancellationTask = nil
        }
    }

    func scheduleLoopStopSignalRequest() {
        guard let workspace, let client = workspace.client else { return }
        let workingDirectoryURL = workspace.identityState.workingDirectoryURL
        loopStopSignalTask?.cancel()
        loopStopSignalTask = Task { @MainActor [weak self, weak workspace] in
            guard let self, let workspace, !Task.isCancelled else { return }
            do {
                let collected = try await client.runAndCollect(
                    arguments: ["--no-color", "machine", "run", "stop"],
                    currentDirectoryURL: workingDirectoryURL,
                    timeoutConfiguration: .longRunning
                )

                guard collected.status.code == 0 else {
                    throw Workspace.WorkspaceError.cliError(
                        collected.failureMessage(
                            operation: "request loop stop",
                            fallback: "Failed to request loop stop (exit \(collected.status.code))"
                        )
                    )
                }

                let document = try RalphMachineContract.decode(
                    MachineRunStopDocument.self,
                    from: Data(collected.stdout.utf8),
                    operation: "machine run stop"
                )
                workspace.updateResolvedPaths(document.paths)
                if let blocking = document.effectiveBlocking?.asWorkspaceBlockingState() {
                    workspace.runState.setLiveBlockingState(blocking)
                }
                workspace.runState.stopAfterCurrent =
                    document.action != .wouldCreate || document.marker.existsAfter
                workspace.runState.errorMessage = nil
                workspace.diagnosticsState.lastRecoveryError = nil
                workspace.diagnosticsState.showErrorRecovery = false
            } catch is CancellationError {
                return
            } catch {
                let recoveryError = RecoveryError.classify(
                    error: error,
                    operation: "request loop stop",
                    workspaceURL: workingDirectoryURL
                )
                workspace.runState.errorMessage = recoveryError.message
                workspace.diagnosticsState.lastRecoveryError = recoveryError
                workspace.diagnosticsState.showErrorRecovery = true
                workspace.runState.stopAfterCurrent = false
            }
            guard self.loopStopSignalTask != nil else { return }
            self.loopStopSignalTask = nil
        }
    }

    func finalizeRun(
        status: RalphCLIExitStatus,
        run: RalphCLIRun,
        repositoryContext: Workspace.RepositoryContext,
        workspace: Workspace
    ) {
        guard workspace.isCurrentRepositoryContext(repositoryContext), activeRun === run else { return }
        workspace.runState.flushConsoleRenderState()
        workspace.runState.lastExitStatus = status
        workspace.runState.isPreparingRun = false
        workspace.runState.isRunning = false

        if let startTime = workspace.runState.executionStartTime {
            let record = Workspace.ExecutionRecord(
                id: UUID(),
                taskID: workspace.runState.currentTaskID,
                startTime: startTime,
                endTime: Date(),
                exitCode: cancelRequested ? nil : Int(status.code),
                wasCancelled: cancelRequested
            )
            workspace.addToHistory(record)
        }

        if workspace.runState.isLoopMode || status.code != 0 {
            workspace.runState.isLoopMode = false
        }

        activeRun = nil
        runCancellationTask = nil
        loopStopSignalTask?.cancel()
        loopStopSignalTask = nil
        if !cancelRequested {
            workspace.runState.stopAfterCurrent = false
        }
        cancelRequested = false
        workspace.resetExecutionState()
    }

    nonisolated static func isMachineRunCommand(_ arguments: [String]) -> Bool {
        let filtered = arguments.filter { $0 != "--no-color" }
        return filtered.starts(with: ["machine", "run"])
    }

}
