/**
 WorkspaceRunnerController

 Responsibilities:
 - Own the live Ralph subprocess lifecycle for one workspace.
 - Load resolved runner configuration for the workspace.
 - Schedule loop continuation explicitly after a run completes without sleep-based polling.

 Does not handle:
 - Queue watching or queue decoding.
 - Task filtering or presentation.
 - App-wide CLI bootstrap.

 Invariants/assumptions callers must respect:
 - Only one active CLI run may exist per workspace.
 - All public methods are main-actor entry points.
 - Loop continuation is scheduled only after the previous run fully finalizes.
 */

import Foundation

@MainActor
final class WorkspaceRunnerController {
    private unowned let workspace: Workspace
    private var activeRun: RalphCLIRun?
    private var cancelRequested = false
    private var loopContinuationTask: Task<Void, Never>?
    private var loopForceDirtyRepo = false

    init(workspace: Workspace) {
        self.workspace = workspace
    }

    func loadRunnerConfiguration(retryConfiguration: RetryConfiguration = .minimal) async {
        workspace.runnerConfigLoading = true
        workspace.runnerConfigErrorMessage = nil

        guard let client = workspace.client else {
            workspace.currentRunnerConfig = nil
            workspace.runnerConfigErrorMessage = "CLI client not available."
            workspace.runnerConfigLoading = false
            return
        }

        do {
            let helper = RetryHelper(configuration: retryConfiguration)
            let collected = try await helper.execute(
                operation: { [workspace] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "config", "show", "--format", "json"],
                        currentDirectoryURL: workspace.workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )

            let data = Data(collected.stdout.utf8)
            let decoded = try JSONDecoder().decode(ResolvedRunnerConfigDocument.self, from: data)
            workspace.currentRunnerConfig = Workspace.RunnerConfig(
                model: decoded.agent?.model,
                phases: decoded.agent?.phases,
                maxIterations: decoded.agent?.iterations
            )
            workspace.runnerConfigErrorMessage = nil
        } catch {
            workspace.currentRunnerConfig = nil
            workspace.runnerConfigErrorMessage = "Failed to load resolved runner configuration."
            RalphLogger.shared.error(
                "Failed to load runner configuration: \(error)",
                category: .workspace
            )
        }

        workspace.runnerConfigLoading = false
    }

    func run(arguments: [String], preservingConsole: Bool = false) {
        guard let client = workspace.client else {
            workspace.errorMessage = "CLI client not available."
            return
        }
        guard !workspace.isRunning else { return }

        loopContinuationTask?.cancel()
        loopContinuationTask = nil
        workspace.runState.prepareForNewRun(preservingConsole: preservingConsole)
        cancelRequested = false

        Task { @MainActor [weak self] in
            guard let self else { return }

            do {
                let run = try client.start(
                    arguments: arguments,
                    currentDirectoryURL: workspace.workingDirectoryURL
                )
                activeRun = run

                for await event in run.events {
                    workspace.outputBuffer.append(event.text)
                    workspace.output = workspace.outputBuffer.content
                    workspace.consumeStreamTextChunk(event.text)
                }

                let status = await run.waitUntilExit()
                finalizeRun(status: status)
            } catch {
                let recoveryError = RecoveryError.classify(
                    error: error,
                    operation: "run",
                    workspaceURL: workspace.workingDirectoryURL
                )
                workspace.errorMessage = recoveryError.message
                workspace.lastRecoveryError = recoveryError
                workspace.showErrorRecovery = true
                workspace.isRunning = false
                activeRun = nil
                cancelRequested = false
                workspace.resetExecutionState()
            }
        }
    }

    func cancel() {
        guard workspace.isRunning else {
            workspace.isLoopMode = false
            workspace.stopAfterCurrent = true
            return
        }

        cancelRequested = true
        workspace.isLoopMode = false
        workspace.stopAfterCurrent = true

        guard let run = activeRun else { return }
        Task {
            await run.cancel()
        }
    }

    func runNextTask(
        taskIDOverride: String? = nil,
        forceDirtyRepo: Bool = false,
        preservingConsole: Bool = false
    ) {
        guard !workspace.isRunning else { return }

        Task { @MainActor [weak self] in
            guard let self else { return }

            workspace.resetExecutionState()

            let requestedTaskID = taskIDOverride?.trimmingCharacters(in: .whitespacesAndNewlines)
            let selectedTaskID = requestedTaskID.flatMap { $0.isEmpty ? nil : $0 }
            let resolvedTaskID: String?
            if let selectedTaskID {
                resolvedTaskID = selectedTaskID
            } else {
                resolvedTaskID = await resolveNextRunnableTaskID()
            }
            workspace.currentTaskID = resolvedTaskID

            var arguments = ["--no-color", "run", "one"]
            if forceDirtyRepo {
                arguments.append("--force")
            }
            if let resolvedTaskID {
                arguments.append(contentsOf: ["--id", resolvedTaskID])
            }

            run(arguments: arguments, preservingConsole: preservingConsole)
        }
    }

    func startLoop(forceDirtyRepo: Bool? = nil) {
        workspace.isLoopMode = true
        workspace.stopAfterCurrent = false
        loopForceDirtyRepo = forceDirtyRepo ?? workspace.runControlForceDirtyRepo
        runNextTask(forceDirtyRepo: loopForceDirtyRepo)
    }

    func stopLoop() {
        workspace.isLoopMode = false
        workspace.stopAfterCurrent = true
        loopContinuationTask?.cancel()
        loopContinuationTask = nil
    }

    private func finalizeRun(status: RalphCLIExitStatus) {
        workspace.lastExitStatus = status
        workspace.isRunning = false

        if let startTime = workspace.executionStartTime {
            let record = Workspace.ExecutionRecord(
                id: UUID(),
                taskID: workspace.currentTaskID,
                startTime: startTime,
                endTime: Date(),
                exitCode: cancelRequested ? nil : Int(status.code),
                wasCancelled: cancelRequested
            )
            workspace.addToHistory(record)
        }

        let shouldContinueLoop = workspace.isLoopMode
            && !workspace.stopAfterCurrent
            && !cancelRequested
            && status.code == 0

        if status.code != 0 {
            workspace.isLoopMode = false
        }

        activeRun = nil
        cancelRequested = false
        workspace.resetExecutionState()

        if shouldContinueLoop {
            scheduleLoopContinuation()
        }
    }

    private func scheduleLoopContinuation() {
        loopContinuationTask?.cancel()
        loopContinuationTask = Task { @MainActor [weak self] in
            guard let self else { return }
            loopContinuationTask = nil
            guard workspace.isLoopMode, !workspace.stopAfterCurrent else { return }
            runNextTask(forceDirtyRepo: loopForceDirtyRepo, preservingConsole: true)
        }
    }

    private func resolveNextRunnableTaskID() async -> String? {
        guard let client = workspace.client else { return workspace.nextTask()?.id }

        do {
            let dryRun = try await client.runAndCollect(
                arguments: ["--no-color", "run", "one", "--dry-run", "--non-interactive"],
                currentDirectoryURL: workspace.workingDirectoryURL
            )
            let combined = dryRun.stdout + "\n" + dryRun.stderr
            if let id = Self.extractTaskID(from: combined) {
                return id
            }
        } catch {
            RalphLogger.shared.debug(
                "Failed to resolve runnable task ID: \(error)",
                category: .workspace
            )
        }

        return workspace.nextTask()?.id
    }

    private static func extractTaskID(from text: String) -> String? {
        for token in text.split(whereSeparator: {
            $0.isWhitespace || $0 == "(" || $0 == ")" || $0 == ":" || $0 == ","
        }) {
            let candidate = String(token)
            if candidate.hasPrefix("RQ-") {
                return candidate
            }
        }
        return nil
    }
}

private extension WorkspaceRunnerController {
    struct ResolvedRunnerConfigDocument: Decodable, Sendable {
        let agent: AgentConfig?

        struct AgentConfig: Decodable, Sendable {
            let model: String?
            let phases: Int?
            let iterations: Int?
        }
    }
}
