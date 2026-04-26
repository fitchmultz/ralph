/**
 Workspace+RunnerCommands

 Purpose:
 - Provide the thin `Workspace` runner-command facade and local execution bookkeeping helpers.

 Responsibilities:
 - Forward run-control commands to `WorkspaceRunnerController`.
 - Expose async runner/parallel status refresh entrypoints.
 - Maintain shared execution-state reset and bounded history bookkeeping.

 Scope:
 - In scope: command forwarding APIs and local history/reset helpers.
 - Out of scope: subprocess management, machine-contract parsing, and run-state storage definitions.

 Usage:
 - Called by run-control UI actions and runner controller integration flows.

 Invariants/assumptions callers must respect:
 - `runnerController` owns execution side effects.
 - History remains capped at 50 records to bound retained state.
 */
import Foundation

public extension Workspace {
    func loadRunnerConfiguration(retryConfiguration: RetryConfiguration = .minimal) async {
        await runnerController.loadRunnerConfiguration(retryConfiguration: retryConfiguration)
    }

    func loadParallelStatus(retryConfiguration: RetryConfiguration = .minimal) async {
        await runnerController.loadParallelStatus(retryConfiguration: retryConfiguration)
    }

    func run(arguments: [String], preservingConsole: Bool = false) {
        runnerController.run(arguments: arguments, preservingConsole: preservingConsole)
    }

    func cancel() {
        runnerController.cancel()
    }

    func runNextTask(
        taskIDOverride: String? = nil,
        forceDirtyRepo: Bool = false,
        preservingConsole: Bool = false
    ) {
        runnerController.runNextTask(
            taskIDOverride: taskIDOverride,
            forceDirtyRepo: forceDirtyRepo,
            preservingConsole: preservingConsole
        )
    }

    func startLoop(forceDirtyRepo: Bool? = nil, parallelWorkers: Int? = nil) {
        runnerController.startLoop(forceDirtyRepo: forceDirtyRepo, parallelWorkers: parallelWorkers)
    }

    func stopLoop() {
        runnerController.stopLoop()
    }
}

extension Workspace {
    func resetExecutionState() {
        runState.currentPhase = nil
        runState.executionStartTime = nil
        runState.currentTaskID = nil
        resetStreamProcessingState()
    }

    func addToHistory(_ record: ExecutionRecord) {
        runState.executionHistory.insert(record, at: 0)
        if runState.executionHistory.count > 50 {
            runState.executionHistory = Array(runState.executionHistory.prefix(50))
        }
    }
}
