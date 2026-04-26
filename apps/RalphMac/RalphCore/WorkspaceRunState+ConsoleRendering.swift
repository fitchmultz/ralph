/**
 WorkspaceRunState+ConsoleRendering

 Purpose:
 - Isolate console buffering and render-cadence behavior for workspace runner state.

 Responsibilities:
 - Reset console state on run start.
 - Stage streaming text and publish buffered output on cadence.
 - Coordinate cancellation and flushing for pending render refresh tasks.

 Scope:
 - In scope: console text ingestion, flush scheduling, render publication, and pending-task cleanup.
 - Out of scope: operator-state synthesis, run-command routing, and runner metadata models.

 Usage:
 - Called by runner controller execution and machine-output flows, plus ANSI parsing updates.

 Invariants/assumptions callers must respect:
 - Methods run on `MainActor` and mutate `WorkspaceRunState` directly.
 - Console publication is buffered to avoid high-frequency UI churn.
 */
import Foundation

@MainActor
extension WorkspaceRunState {
    func prepareForNewRun(preservingConsole: Bool = false) {
        cancelPendingConsoleRenderRefresh()
        pendingConsoleText.removeAll(keepingCapacity: false)
        if preservingConsole {
            if !outputBuffer.content.hasSuffix("\n"), !outputBuffer.content.isEmpty {
                outputBuffer.append("\n")
            }
            output = outputBuffer.content
        } else {
            output = ""
            outputBuffer.clear()
            attributedOutput = []
            streamProcessor.reset()
        }
        lastExitStatus = nil
        errorMessage = nil
        isPreparingRun = false
        isRunning = true
        executionStartTime = Date()
        currentPhase = nil
        resumeState = nil
        clearQueueBlockingState()
        clearLiveBlockingState()
    }

    func scheduleConsoleRenderRefresh() {
        guard pendingConsoleRenderRefreshTask == nil else { return }
        pendingConsoleRenderRefreshTask = Task { @MainActor [weak self] in
            do {
                try await Task.sleep(nanoseconds: Self.consoleRenderRefreshIntervalNanoseconds)
            } catch {
                return
            }
            guard let self, !Task.isCancelled else { return }
            pendingConsoleRenderRefreshTask = nil
            publishConsoleRenderState()
        }
    }

    func flushConsoleRenderState() {
        cancelPendingConsoleRenderRefresh()
        publishConsoleRenderState()
    }

    func ingestConsoleText(_ text: String) {
        pendingConsoleText.append(text)
        if pendingConsoleText.count > outputBuffer.maxCharacters {
            outputBuffer.append(pendingConsoleText)
            pendingConsoleText.removeAll(keepingCapacity: true)
        }
    }

    func cancelPendingConsoleRenderRefresh() {
        pendingConsoleRenderRefreshTask?.cancel()
        pendingConsoleRenderRefreshTask = nil
    }

    private func publishConsoleRenderState() {
        if !pendingConsoleText.isEmpty {
            outputBuffer.append(pendingConsoleText)
            pendingConsoleText.removeAll(keepingCapacity: true)
        }
        output = outputBuffer.content
        attributedOutput = streamProcessor.displaySegments(
            maxSegments: maxANSISegments,
            maxCharacters: outputBuffer.maxCharacters
        )
    }
}
