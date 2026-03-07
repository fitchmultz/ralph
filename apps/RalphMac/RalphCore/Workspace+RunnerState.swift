//! Workspace+RunnerState
//!
//! Responsibilities:
//! - Start, cancel, and loop Ralph CLI executions for a workspace.
//! - Track per-workspace runner lifecycle fields such as active run, phase, and history.
//! - Resolve the next runnable task, runner configuration, and execution phase state.
//!
//! Does not handle:
//! - Queue file decoding or file-watcher orchestration.
//! - Task filtering, grouping, or other presentation work.
//! - Task mutation and task creation flows.
//!
//! Invariants/assumptions callers must respect:
//! - Runner state remains window/workspace scoped and must not leak across workspaces.
//! - Only one active run may execute per workspace at a time.
//! - Cancellation must target the active subprocess owned by this workspace.
//! - Runner configuration is resolved by the CLI itself, not reconstructed in-app.

import Foundation

public extension Workspace {
    func loadRunnerConfiguration(retryConfiguration: RetryConfiguration = .minimal) async {
        runnerConfigLoading = true
        runnerConfigErrorMessage = nil

        guard let client else {
            currentRunnerConfig = nil
            runnerConfigErrorMessage = "CLI client not available."
            runnerConfigLoading = false
            return
        }

        do {
            let helper = RetryHelper(configuration: retryConfiguration)
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "config", "show", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )

            let data = Data(collected.stdout.utf8)
            let decoded = try JSONDecoder().decode(ResolvedRunnerConfigDocument.self, from: data)
            currentRunnerConfig = RunnerConfig(
                model: decoded.agent?.model,
                phases: decoded.agent?.phases,
                maxIterations: decoded.agent?.iterations
            )
            runnerConfigErrorMessage = nil
        } catch {
            currentRunnerConfig = nil
            runnerConfigErrorMessage = "Failed to load resolved runner configuration."
            RalphLogger.shared.error(
                "Failed to load runner configuration: \(error)",
                category: .workspace
            )
        }

        runnerConfigLoading = false
    }

    func run(arguments: [String]) {
        guard let client else {
            errorMessage = "CLI client not available."
            return
        }
        guard !isRunning else { return }

        output = ""
        outputBuffer.clear()
        attributedOutput = []
        lastExitStatus = nil
        errorMessage = nil
        isRunning = true
        cancelRequested = false
        executionStartTime = Date()

        Task { @MainActor [weak self] in
            guard let self else { return }

            do {
                let run = try client.start(
                    arguments: arguments,
                    currentDirectoryURL: workingDirectoryURL
                )
                activeRun = run

                for await event in run.events {
                    outputBuffer.append(event.text)
                    output = outputBuffer.content

                    detectPhase(from: output)
                    parseANSICodes(from: output, appendToExisting: false)
                    enforceANSISegmentLimit()
                }

                let status = await run.waitUntilExit()

                lastExitStatus = status
                isRunning = false

                if let startTime = executionStartTime {
                    let record = ExecutionRecord(
                        id: UUID(),
                        taskID: currentTaskID,
                        startTime: startTime,
                        endTime: Date(),
                        exitCode: cancelRequested ? nil : Int(status.code),
                        wasCancelled: cancelRequested
                    )
                    addToHistory(record)
                }

                if isLoopMode && !stopAfterCurrent && !cancelRequested && status.code == 0 {
                    try? await Task.sleep(nanoseconds: 1_000_000_000)
                    if isLoopMode && !stopAfterCurrent && !cancelRequested {
                        activeRun = nil
                        cancelRequested = false
                        runNextTask(forceDirtyRepo: runControlForceDirtyRepo)
                        return
                    }
                }

                if status.code != 0 {
                    isLoopMode = false
                }

                activeRun = nil
                cancelRequested = false
                resetExecutionState()
            } catch {
                let recoveryError = RecoveryError.classify(
                    error: error,
                    operation: "run",
                    workspaceURL: workingDirectoryURL
                )
                errorMessage = recoveryError.message
                lastRecoveryError = recoveryError
                showErrorRecovery = true
                isRunning = false
                activeRun = nil
                cancelRequested = false
                resetExecutionState()
            }
        }
    }

    func cancel() {
        guard isRunning else {
            isLoopMode = false
            stopAfterCurrent = true
            return
        }

        cancelRequested = true
        isLoopMode = false
        stopAfterCurrent = true

        guard let run = activeRun else { return }
        Task {
            await run.cancel()
        }
    }

    /// Run a task from the queue (defaults to the next runnable task).
    func runNextTask(taskIDOverride: String? = nil, forceDirtyRepo: Bool = false) {
        guard !isRunning else { return }

        Task { @MainActor [weak self] in
            guard let self else { return }

            resetExecutionState()

            let requestedTaskID = taskIDOverride?.trimmingCharacters(in: .whitespacesAndNewlines)
            let selectedTaskID = requestedTaskID.flatMap { $0.isEmpty ? nil : $0 }
            let resolvedTaskID: String?
            if let selectedTaskID {
                resolvedTaskID = selectedTaskID
            } else {
                resolvedTaskID = await resolveNextRunnableTaskID()
            }
            currentTaskID = resolvedTaskID

            var arguments = ["--no-color", "run", "one"]
            if forceDirtyRepo {
                arguments.append("--force")
            }
            if let resolvedTaskID {
                arguments.append(contentsOf: ["--id", resolvedTaskID])
            }

            run(arguments: arguments)
        }
    }

    /// Start loop mode (continuously run tasks)
    func startLoop(forceDirtyRepo: Bool? = nil) {
        isLoopMode = true
        stopAfterCurrent = false
        let shouldForceDirtyRepo = forceDirtyRepo ?? runControlForceDirtyRepo
        runNextTask(forceDirtyRepo: shouldForceDirtyRepo)
    }

    /// Stop loop mode (finish current task then stop)
    func stopLoop() {
        isLoopMode = false
        stopAfterCurrent = true
    }

    /// Parse phase information from CLI output
    func detectPhase(from output: String) {
        if output.contains("PHASE 1") || output.contains("Phase 1") ||
            output.contains("PLANNING") || output.contains("Planning") ||
            output.contains("# Phase 1") || output.contains("## Phase 1") {
            currentPhase = .plan
        } else if output.contains("PHASE 2") || output.contains("Phase 2") ||
                    output.contains("IMPLEMENTING") || output.contains("Implementing") ||
                    output.contains("IMPLEMENTATION") || output.contains("# Phase 2") ||
                    output.contains("## Phase 2") {
            currentPhase = .implement
        } else if output.contains("PHASE 3") || output.contains("Phase 3") ||
                    output.contains("REVIEWING") || output.contains("Reviewing") ||
                    output.contains("REVIEW") || output.contains("# Phase 3") ||
                    output.contains("## Phase 3") {
            currentPhase = .review
        }
    }
}

private extension Workspace {
    struct ResolvedRunnerConfigDocument: Decodable, Sendable {
        let agent: AgentConfig?

        struct AgentConfig: Decodable, Sendable {
            let model: String?
            let phases: Int?
            let iterations: Int?
        }
    }

    static func extractTaskID(from text: String) -> String? {
        for token in text.split(whereSeparator: { $0.isWhitespace || $0 == "(" || $0 == ")" || $0 == ":" || $0 == "," }) {
            let candidate = String(token)
            if candidate.hasPrefix("RQ-") {
                return candidate
            }
        }
        return nil
    }

    func resolveNextRunnableTaskID() async -> String? {
        guard let client else { return nextTask()?.id }

        do {
            let dryRun = try await client.runAndCollect(
                arguments: ["--no-color", "run", "one", "--dry-run", "--non-interactive"],
                currentDirectoryURL: workingDirectoryURL
            )
            let combined = dryRun.stdout + "\n" + dryRun.stderr
            if let id = Self.extractTaskID(from: combined) {
                return id
            }
        } catch {
            RalphLogger.shared.debug("Failed to resolve runnable task ID: \(error)", category: .workspace)
        }

        return nextTask()?.id
    }

    /// Reset execution state after completion or cancellation
    func resetExecutionState() {
        currentPhase = nil
        executionStartTime = nil
        currentTaskID = nil
        attributedOutput = []
        // Note: outputBuffer is intentionally preserved for inspection after completion
    }

    /// Add execution record to history (keeps last 50)
    func addToHistory(_ record: ExecutionRecord) {
        executionHistory.insert(record, at: 0)
        if executionHistory.count > 50 {
            executionHistory = Array(executionHistory.prefix(50))
        }
    }
}
