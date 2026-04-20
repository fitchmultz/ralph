/**
 WorkspaceRunnerController+Execution

 Responsibilities:
 - Schedule workspace run tasks and own subprocess finalization.
 - Resolve next-task selection for app-launched one-shot runs.
 - Request CLI loop stop signals without cancelling the active run immediately.
 - Keep cancellation and repository-retarget cleanup centralized outside the facade file.

 Does not handle:
 - Machine output decoding details.
 - Queue watching.
 */

import Foundation

@MainActor
extension WorkspaceRunnerController {
    func hasPendingRunWork(for workspace: Workspace) -> Bool {
        runTask != nil || activeRun != nil || workspace.runState.isRunning
    }

    func scheduleRunTask(
        preservingConsole: Bool,
        operation: @escaping @MainActor (Workspace, Workspace.RepositoryContext) async -> [String]?
    ) {
        guard let workspace, !workspace.isShutDown else { return }

        runTaskRevision &+= 1
        let revision = runTaskRevision
        let repositoryContext = workspace.currentRepositoryContext()
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
            workspace.runState.errorMessage = recoveryError.message
            workspace.diagnosticsState.lastRecoveryError = recoveryError
            workspace.diagnosticsState.showErrorRecovery = true
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
    }

    func cancelPendingRunTask() {
        runTask?.cancel()
        runTask = nil
        runTaskRevision &+= 1
        if let workspace, activeRun == nil {
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
                let run = try client.start(
                    arguments: ["queue", "stop"],
                    currentDirectoryURL: workingDirectoryURL
                )
                self.appendConsoleText("[ralph] Stop after current requested.\n", workspace: workspace)
                for await event in run.events {
                    guard !Task.isCancelled else {
                        await run.cancel()
                        return
                    }
                    self.appendConsoleText(event.text, workspace: workspace)
                }
                let status = await run.waitUntilExit()
                if status.code != 0 {
                    self.appendConsoleText(
                        "[warning] Failed to request loop stop; queue stop exited \(status.code).\n",
                        workspace: workspace
                    )
                }
            } catch is CancellationError {
                return
            } catch {
                self.appendConsoleText(
                    "[warning] Failed to request loop stop: \(error.localizedDescription)\n",
                    workspace: workspace
                )
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
        workspace.runState.lastExitStatus = status
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

    func resolveNextRunnableTaskID(repositoryContext: Workspace.RepositoryContext) async -> String? {
        guard let workspace else { return nil }
        guard let client = workspace.client else { return workspace.nextTask()?.id }

        do {
            let snapshot = try await workspace.decodeMachineRepositoryJSON(
                MachineQueueReadDocument.self,
                client: client,
                machineArguments: ["queue", "read"],
                currentDirectoryURL: repositoryContext.workingDirectoryURL,
                retryConfiguration: .minimal
            )
            guard !Task.isCancelled, workspace.isCurrentRepositoryContext(repositoryContext) else {
                return nil
            }
            workspace.updateResolvedPaths(snapshot.paths)
            return snapshot.nextRunnableTaskID
        } catch is CancellationError {
            return nil
        } catch {
            RalphLogger.shared.debug(
                "Failed to resolve runnable task ID: \(error)",
                category: .workspace
            )
        }

        guard workspace.isCurrentRepositoryContext(repositoryContext) else {
            return nil
        }
        return workspace.nextTask()?.id
    }

    nonisolated static func isMachineRunCommand(_ arguments: [String]) -> Bool {
        let filtered = arguments.filter { $0 != "--no-color" }
        return filtered.starts(with: ["machine", "run"])
    }

    nonisolated static func validateMachineConfigResolveVersion(_ version: Int) throws {
        guard version == supportedMachineConfigResolveVersion else {
            throw NSError(
                domain: "RalphMachineContract",
                code: 2,
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "Unsupported machine config resolve version \(version). RalphMac requires version \(supportedMachineConfigResolveVersion)."
                ]
            )
        }
    }
}
