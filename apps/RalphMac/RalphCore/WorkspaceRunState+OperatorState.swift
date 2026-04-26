/**
 WorkspaceRunState+OperatorState

 Purpose:
 - Isolate run-control operator-state synthesis and blocking precedence updates.

 Responsibilities:
 - Track live and queue-derived blocking snapshots.
 - Build and publish run-control operator state for UI consumption.
 - Reset operator-state fields when execution context changes.

 Scope:
 - In scope: operator-state mutation APIs and synthesis helper.
 - Out of scope: console render buffering, run command dispatch, and runner metadata type definitions.

 Usage:
 - Called by machine-output handlers, queue-refresh flows, and run-control surfaces.

 Invariants/assumptions callers must respect:
 - `liveBlockingState` has highest precedence over parallel/resume/queue-derived states.
 - `blockingState` mirrors the selected operator-state snapshot.
 - Methods run on `MainActor`.
 */
import Foundation

@MainActor
extension WorkspaceRunState {
    func setLiveBlockingState(_ state: Workspace.BlockingState?) {
        liveBlockingState = state
        refreshOperatorState()
    }

    func clearLiveBlockingState() {
        setLiveBlockingState(nil)
    }

    func setQueueBlockingState(_ state: Workspace.BlockingState?) {
        queueBlockingState = state
        refreshOperatorState()
    }

    func clearQueueBlockingState() {
        setQueueBlockingState(nil)
    }

    func clearRunControlOperatorState() {
        liveBlockingState = nil
        queueBlockingState = nil
        resumeState = nil
        blockingState = nil
        runControlOperatorState = nil
    }

    func clearParallelStatus() {
        parallelStatus = nil
        parallelStatusLoading = false
        parallelStatusErrorMessage = nil
    }

    func refreshOperatorStateForDisplay() {
        refreshOperatorState()
    }

    func refreshOperatorState() {
        let operatorState = Workspace.RunControlOperatorState.build(
            liveBlockingState: liveBlockingState,
            parallelStatus: parallelStatus,
            resumeState: resumeState,
            queueBlockingState: queueBlockingState,
            isLoopMode: isLoopMode,
            stopAfterCurrent: stopAfterCurrent
        )
        runControlOperatorState = operatorState
        blockingState = operatorState?.blockingState
    }
}
