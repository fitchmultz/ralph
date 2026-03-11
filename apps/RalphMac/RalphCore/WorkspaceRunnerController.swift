/**
 WorkspaceRunnerController

 Responsibilities:
 - Own the live Ralph subprocess lifecycle for one workspace.
 - Load resolved runner configuration for the workspace.
 - Consume machine run-event streams and derive UI state from structured envelopes.
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
    private weak var workspace: Workspace?
    private var activeRun: RalphCLIRun?
    private var cancelRequested = false
    private var loopContinuationTask: Task<Void, Never>?
    private var loopForceDirtyRepo = false

    init(workspace: Workspace) {
        self.workspace = workspace
    }

    func loadRunnerConfiguration(retryConfiguration: RetryConfiguration = .minimal) async {
        guard let workspace else { return }
        await workspace.performRepositoryLoad(
            operation: "loadRunnerConfiguration",
            retryConfiguration: retryConfiguration,
            setLoading: { [runState = workspace.runState] in runState.runnerConfigLoading = $0 },
            clearFailure: { [runState = workspace.runState] in
                runState.runnerConfigErrorMessage = nil
            },
            handleMissingClient: { [runState = workspace.runState] in
                runState.currentRunnerConfig = nil
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
            apply: { [workspace, runState = workspace.runState] decoded in
                workspace.updateResolvedPaths(decoded.paths)
                runState.currentRunnerConfig = Workspace.RunnerConfig(
                    model: decoded.config.agent?.model,
                    phases: decoded.config.agent?.phases,
                    maxIterations: decoded.config.agent?.iterations
                )
                runState.runnerConfigErrorMessage = nil
            },
            handleFailure: { [runState = workspace.runState] recoveryError in
                runState.currentRunnerConfig = nil
                runState.runnerConfigErrorMessage = "Failed to load resolved runner configuration."
                RalphLogger.shared.error(
                    "Failed to load runner configuration: \(recoveryError.fullErrorDetails)",
                    category: .workspace
                )
            }
        )
    }

    func prepareForRepositoryRetarget() {
        guard let workspace else { return }
        loopContinuationTask?.cancel()
        loopContinuationTask = nil
        workspace.runState.isLoopMode = false
        cancelRequested = false

        let runToCancel = activeRun
        activeRun = nil
        if let runToCancel {
            Task {
                await runToCancel.cancel()
            }
        }
    }

    func run(arguments: [String], preservingConsole: Bool = false) {
        guard let workspace else { return }
        guard let client = workspace.client else {
            workspace.runState.errorMessage = "CLI client not available."
            return
        }
        guard !workspace.runState.isRunning else { return }

        loopContinuationTask?.cancel()
        loopContinuationTask = nil
        workspace.runState.prepareForNewRun(preservingConsole: preservingConsole)
        cancelRequested = false

        Task { @MainActor [weak self] in
            guard let self, let workspace = self.workspace else { return }
            let repositoryContext = workspace.currentRepositoryContext()

            do {
                let run = try client.start(
                    arguments: arguments,
                    currentDirectoryURL: workspace.identityState.workingDirectoryURL
                )
                activeRun = run
                var machineDecoder = MachineRunOutputDecoder()
                let usesMachineRunEvents = Self.isMachineRunCommand(arguments)

                for await event in run.events {
                    guard workspace.isCurrentRepositoryContext(repositoryContext), activeRun === run else { continue }
                    if usesMachineRunEvents, event.stream == .stdout {
                        for item in machineDecoder.append(event.text) {
                            self.applyMachineRunOutputItem(item, workspace: workspace)
                        }
                    } else {
                        self.appendConsoleText(event.text, workspace: workspace)
                    }
                }

                if usesMachineRunEvents {
                    for item in machineDecoder.finish() {
                        self.applyMachineRunOutputItem(item, workspace: workspace)
                    }
                }

                let status = await run.waitUntilExit()
                finalizeRun(
                    status: status,
                    run: run,
                    repositoryContext: repositoryContext,
                    workspace: workspace
                )
            } catch {
                guard workspace.isCurrentRepositoryContext(repositoryContext) else { return }
                let recoveryError = RecoveryError.classify(
                    error: error,
                    operation: "run",
                    workspaceURL: workspace.identityState.workingDirectoryURL
                )
                workspace.runState.errorMessage = recoveryError.message
                workspace.diagnosticsState.lastRecoveryError = recoveryError
                workspace.diagnosticsState.showErrorRecovery = true
                workspace.runState.isRunning = false
                activeRun = nil
                cancelRequested = false
                workspace.resetExecutionState()
            }
        }
    }

    func runMachine(arguments: [String], preservingConsole: Bool = false) {
        run(arguments: ["--no-color", "machine"] + arguments, preservingConsole: preservingConsole)
    }

    func cancel() {
        guard let workspace else { return }
        guard workspace.runState.isRunning else {
            workspace.runState.isLoopMode = false
            workspace.runState.stopAfterCurrent = true
            return
        }

        cancelRequested = true
        workspace.runState.isLoopMode = false
        workspace.runState.stopAfterCurrent = true

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
        guard let workspace else { return }
        guard !workspace.runState.isRunning else { return }

        Task { @MainActor [weak self] in
            guard let self, let workspace = self.workspace else { return }

            workspace.resetExecutionState()

            let requestedTaskID = taskIDOverride?.trimmingCharacters(in: .whitespacesAndNewlines)
            let selectedTaskID = requestedTaskID.flatMap { $0.isEmpty ? nil : $0 }
            let resolvedTaskID: String?
            if let selectedTaskID {
                resolvedTaskID = selectedTaskID
            } else {
                resolvedTaskID = await resolveNextRunnableTaskID()
            }
            workspace.runState.currentTaskID = resolvedTaskID

            var arguments = ["run", "one"]
            if forceDirtyRepo {
                arguments.append("--force")
            }
            if let resolvedTaskID {
                arguments.append(contentsOf: ["--id", resolvedTaskID])
            }

            runMachine(arguments: arguments, preservingConsole: preservingConsole)
        }
    }

    func startLoop(forceDirtyRepo: Bool? = nil) {
        guard let workspace else { return }
        workspace.runState.isLoopMode = true
        workspace.runState.stopAfterCurrent = false
        loopForceDirtyRepo = forceDirtyRepo ?? workspace.runState.runControlForceDirtyRepo
        runNextTask(forceDirtyRepo: loopForceDirtyRepo)
    }

    func stopLoop() {
        guard let workspace else { return }
        workspace.runState.isLoopMode = false
        workspace.runState.stopAfterCurrent = true
        loopContinuationTask?.cancel()
        loopContinuationTask = nil
    }

    private func finalizeRun(
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

        let shouldContinueLoop = workspace.runState.isLoopMode
            && !workspace.runState.stopAfterCurrent
            && !cancelRequested
            && status.code == 0

        if status.code != 0 {
            workspace.runState.isLoopMode = false
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
            guard let self, let workspace = self.workspace else { return }
            loopContinuationTask = nil
            guard workspace.runState.isLoopMode, !workspace.runState.stopAfterCurrent else { return }
            runNextTask(forceDirtyRepo: loopForceDirtyRepo, preservingConsole: true)
        }
    }

    private func resolveNextRunnableTaskID() async -> String? {
        guard let workspace else { return nil }
        guard let client = workspace.client else { return workspace.nextTask()?.id }

        do {
            let snapshot = try await workspace.decodeMachineRepositoryJSON(
                MachineQueueReadDocument.self,
                client: client,
                machineArguments: ["queue", "read"],
                currentDirectoryURL: workspace.identityState.workingDirectoryURL,
                retryConfiguration: .minimal
            )
            workspace.updateResolvedPaths(snapshot.paths)
            if let id = snapshot.nextRunnableTaskID {
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

    private func appendConsoleText(_ text: String, workspace: Workspace) {
        workspace.runState.outputBuffer.append(text)
        workspace.runState.output = workspace.runState.outputBuffer.content
        workspace.consumeStreamTextChunk(text)
    }

    private func applyMachineRunOutputItem(_ item: MachineRunOutputDecoder.Item, workspace: Workspace) {
        switch item {
        case .event(let event):
            switch event.kind {
            case .runStarted:
                workspace.runState.currentTaskID = event.taskID ?? workspace.runState.currentTaskID
                if let document = event.payload?.decode(MachineConfigResolveDocument.self, for: "config") {
                    workspace.updateResolvedPaths(document.paths)
                }
            case .taskSelected:
                workspace.runState.currentTaskID = event.taskID ?? workspace.runState.currentTaskID
            case .phaseEntered:
                workspace.runState.currentPhase = Workspace.ExecutionPhase(machineValue: event.phase)
            case .phaseCompleted:
                if workspace.runState.currentPhase == Workspace.ExecutionPhase(machineValue: event.phase) {
                    workspace.runState.currentPhase = nil
                }
            case .runnerOutput:
                if let text = event.payload?.string(for: "text") {
                    appendConsoleText(text, workspace: workspace)
                }
            case .queueSnapshot:
                if let paths = event.payload?.decode(MachineQueuePaths.self, for: "paths") {
                    workspace.updateResolvedPaths(paths)
                }
            case .configResolved:
                if let document = event.payload?.decode(MachineConfigResolveDocument.self, for: "config") {
                    workspace.updateResolvedPaths(document.paths)
                }
            case .warning:
                if let message = event.message, !message.isEmpty {
                    appendConsoleText("[warning] \(message)\n", workspace: workspace)
                }
            case .runFinished:
                break
            }
        case .summary(let summary):
            if let taskID = summary.taskID {
                workspace.runState.currentTaskID = taskID
            }
        case .rawText(let text):
            appendConsoleText(text, workspace: workspace)
        }
    }

    private static func isMachineRunCommand(_ arguments: [String]) -> Bool {
        let filtered = arguments.filter { $0 != "--no-color" }
        return filtered.starts(with: ["machine", "run"])
    }
}

private extension WorkspaceRunnerController {
    struct MachineRunEventEnvelope: Decodable, Sendable {
        let version: Int
        let kind: Kind
        let taskID: String?
        let phase: String?
        let message: String?
        let payload: RalphJSONValue?

        enum Kind: String, Decodable, Sendable {
            case runStarted = "run_started"
            case queueSnapshot = "queue_snapshot"
            case configResolved = "config_resolved"
            case taskSelected = "task_selected"
            case phaseEntered = "phase_entered"
            case phaseCompleted = "phase_completed"
            case runnerOutput = "runner_output"
            case warning
            case runFinished = "run_finished"
        }

        enum CodingKeys: String, CodingKey {
            case version
            case kind
            case taskID = "task_id"
            case phase
            case message
            case payload
        }
    }

    struct MachineRunSummaryDocument: Decodable, Sendable {
        let version: Int
        let taskID: String?
        let exitCode: Int
        let outcome: String

        enum CodingKeys: String, CodingKey {
            case version
            case taskID = "task_id"
            case exitCode = "exit_code"
            case outcome
        }
    }

    struct MachineRunOutputDecoder {
        enum Item {
            case event(MachineRunEventEnvelope)
            case summary(MachineRunSummaryDocument)
            case rawText(String)
        }

        private var buffered = ""

        mutating func append(_ chunk: String) -> [Item] {
            buffered.append(chunk)
            return drainCompleteLines()
        }

        mutating func finish() -> [Item] {
            defer { buffered.removeAll(keepingCapacity: false) }
            guard !buffered.isEmpty else { return [] }
            return decodeLine(buffered)
        }

        private mutating func drainCompleteLines() -> [Item] {
            var items: [Item] = []
            while let newlineIndex = buffered.firstIndex(of: "\n") {
                let line = String(buffered[..<newlineIndex])
                buffered.removeSubrange(...newlineIndex)
                items.append(contentsOf: decodeLine(line))
            }
            return items
        }

        private func decodeLine(_ line: String) -> [Item] {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else { return [] }
            let data = Data(trimmed.utf8)
            let decoder = JSONDecoder()

            if let event = try? decoder.decode(MachineRunEventEnvelope.self, from: data) {
                return [.event(event)]
            }
            if let summary = try? decoder.decode(MachineRunSummaryDocument.self, from: data) {
                return [.summary(summary)]
            }
            return [.rawText(line + "\n")]
        }
    }
}

private extension Workspace.ExecutionPhase {
    init?(machineValue: String?) {
        switch machineValue {
        case "plan":
            self = .plan
        case "implement":
            self = .implement
        case "review":
            self = .review
        default:
            return nil
        }
    }
}

private extension RalphJSONValue {
    func string(for key: String) -> String? {
        guard case .object(let object) = self, let value = object[key] else { return nil }
        return value.stringValue
    }

    func decode<T: Decodable>(_ type: T.Type, for key: String) -> T? {
        guard case .object(let object) = self, let value = object[key] else { return nil }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return try? decoder.decode(type, from: data)
    }
}
