/**
 WorkspaceRunnerController+MachineOutput

 Responsibilities:
 - Decode machine-run envelopes and summaries emitted by the CLI.
 - Apply structured run-event updates to workspace run state and console output.
 - Keep machine-contract helpers separate from runner lifecycle orchestration.

 Does not handle:
 - Process start/stop scheduling.
 - Queue watching or workspace retarget lifecycle.
 */

import Foundation

@MainActor
extension WorkspaceRunnerController {
    func appendConsoleText(_ text: String, workspace: Workspace) {
        workspace.runState.outputBuffer.append(text)
        workspace.runState.output = workspace.runState.outputBuffer.content
        workspace.consumeStreamTextChunk(text)
    }

    func applyMachineRunOutputItem(_ item: MachineRunOutputDecoder.Item, workspace: Workspace) {
        switch item {
        case .event(let event):
            switch event.kind {
            case .runStarted:
                workspace.runState.currentTaskID = event.taskID ?? workspace.runState.currentTaskID
                workspace.runState.setBlockingState(nil)
                if let document = event.payload?.decode(MachineConfigResolveDocument.self, at: ["config"]) {
                    applyConfigResolveDocument(document, workspace: workspace)
                }
            case .taskSelected:
                workspace.runState.currentTaskID = event.taskID ?? workspace.runState.currentTaskID
                workspace.runState.setBlockingState(nil)
            case .phaseEntered:
                workspace.runState.currentPhase = Workspace.ExecutionPhase(machineValue: event.phase)
                workspace.runState.setBlockingState(nil)
            case .phaseCompleted:
                if workspace.runState.currentPhase == Workspace.ExecutionPhase(machineValue: event.phase) {
                    workspace.runState.currentPhase = nil
                }
            case .resumeDecision:
                if let decision = decodeResumeDecision(from: event.payload) {
                    applyResumeProjection(decision, workspace: workspace)
                    appendResumeDecision(decision, workspace: workspace)
                } else if let message = event.message, !message.isEmpty {
                    appendConsoleText("\(message)\n", workspace: workspace)
                }
            case .runnerOutput:
                if let text = event.payload?.value(at: ["text"])?.stringValue {
                    appendConsoleText(text, workspace: workspace)
                }
            case .blockedStateChanged:
                if let state = decodeBlockingState(from: event.payload) {
                    workspace.runState.setBlockingState(state.asWorkspaceBlockingState())
                    if !state.isRunnerRecovery {
                        appendBlockingState(state, workspace: workspace)
                    }
                } else if let message = event.message, !message.isEmpty {
                    appendConsoleText("\(message)\n", workspace: workspace)
                }
            case .blockedStateCleared:
                workspace.runState.setBlockingState(nil)
            case .queueSnapshot:
                if let paths = event.payload?.decode(MachineQueuePaths.self, at: ["paths"]) {
                    workspace.updateResolvedPaths(paths)
                }
            case .configResolved:
                if let document = event.payload?.decode(MachineConfigResolveDocument.self, at: ["config"]) {
                    applyConfigResolveDocument(document, workspace: workspace)
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
            if let blocking = summary.blocking {
                workspace.runState.setBlockingState(blocking.asWorkspaceBlockingState())
            }
        case .rawText(let text):
            appendConsoleText(text, workspace: workspace)
        }
    }

    private func decodeResumeDecision(from payload: RalphJSONValue?) -> MachineResumeDecision? {
        payload?.decode(MachineResumeDecision.self)
    }

    private func decodeBlockingState(from payload: RalphJSONValue?) -> MachineBlockingState? {
        payload?.decode(MachineBlockingState.self)
    }

    private func appendResumeDecision(_ decision: MachineResumeDecision, workspace: Workspace) {
        var lines = [decision.message]
        if !decision.detail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            lines.append("  \(decision.detail)")
        }
        appendConsoleText(lines.joined(separator: "\n") + "\n", workspace: workspace)
    }

    private func appendBlockingState(_ state: MachineBlockingState, workspace: Workspace) {
        var lines = ["[\(state.status.rawValue)] \(state.message)"]
        if !state.detail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            lines.append("  \(state.detail)")
        }
        appendConsoleText(lines.joined(separator: "\n") + "\n", workspace: workspace)
    }
}

extension WorkspaceRunnerController {
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
            case resumeDecision = "resume_decision"
            case taskSelected = "task_selected"
            case phaseEntered = "phase_entered"
            case phaseCompleted = "phase_completed"
            case runnerOutput = "runner_output"
            case blockedStateChanged = "blocked_state_changed"
            case blockedStateCleared = "blocked_state_cleared"
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
        let blocking: MachineBlockingState?

        enum CodingKeys: String, CodingKey {
            case version
            case taskID = "task_id"
            case exitCode = "exit_code"
            case outcome
            case blocking
        }
    }

    struct MachineBlockingState: Decodable, Sendable, Equatable {
        let status: Workspace.BlockingStatus
        let reason: MachineBlockingReason
        let taskID: String?
        let message: String
        let detail: String

        enum CodingKeys: String, CodingKey {
            case status
            case reason
            case taskID = "task_id"
            case message
            case detail
        }

        var isRunnerRecovery: Bool {
            reason.kind == "runner_recovery"
        }

        func asWorkspaceBlockingState() -> Workspace.BlockingState {
            Workspace.BlockingState(
                status: status,
                reason: reason.asWorkspaceBlockingReason(),
                taskID: taskID,
                message: message,
                detail: detail
            )
        }
    }

    struct MachineBlockingReason: Decodable, Sendable, Equatable {
        let kind: String
        let includeDraft: Bool?
        let blockedTasks: Int?
        let nextRunnableAt: String?
        let secondsUntilNextRunnable: Int?
        let lockPath: String?
        let owner: String?
        let ownerPID: Int?
        let pattern: String?
        let exitCode: Int?
        let scope: String?
        let reason: String?
        let taskID: String?
        let suggestedCommand: String?
        let dependencyBlocked: Int?
        let scheduleBlocked: Int?
        let statusFiltered: Int?

        enum CodingKeys: String, CodingKey {
            case kind
            case includeDraft = "include_draft"
            case blockedTasks = "blocked_tasks"
            case nextRunnableAt = "next_runnable_at"
            case secondsUntilNextRunnable = "seconds_until_next_runnable"
            case lockPath = "lock_path"
            case owner
            case ownerPID = "owner_pid"
            case pattern
            case exitCode = "exit_code"
            case scope
            case reason
            case taskID = "task_id"
            case suggestedCommand = "suggested_command"
            case dependencyBlocked = "dependency_blocked"
            case scheduleBlocked = "schedule_blocked"
            case statusFiltered = "status_filtered"
        }

        func asWorkspaceBlockingReason() -> Workspace.BlockingReason {
            switch kind {
            case "idle":
                return .idle(includeDraft: includeDraft ?? false)
            case "dependency_blocked":
                return .dependencyBlocked(blockedTasks: blockedTasks ?? 0)
            case "schedule_blocked":
                return .scheduleBlocked(
                    blockedTasks: blockedTasks ?? 0,
                    nextRunnableAt: nextRunnableAt,
                    secondsUntilNextRunnable: secondsUntilNextRunnable
                )
            case "lock_blocked":
                return .lockBlocked(lockPath: lockPath, owner: owner, ownerPID: ownerPID)
            case "ci_blocked":
                return .ciBlocked(pattern: pattern, exitCode: exitCode)
            case "runner_recovery":
                return .runnerRecovery(scope: scope ?? "unknown", reason: reason ?? "unknown", taskID: taskID)
            case "operator_recovery":
                return .operatorRecovery(
                    scope: scope ?? "unknown",
                    reason: reason ?? "unknown",
                    suggestedCommand: suggestedCommand
                )
            default:
                return .mixedQueue(
                    dependencyBlocked: dependencyBlocked ?? 0,
                    scheduleBlocked: scheduleBlocked ?? 0,
                    statusFiltered: statusFiltered ?? 0
                )
            }
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
