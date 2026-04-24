/**
 Workspace+TestAccessors

 Purpose:
 - Expose narrowly scoped test-only accessors over internal Workspace state for RalphCore tests.

 Responsibilities:
 - Expose narrowly scoped test-only accessors over internal Workspace state for RalphCore tests.

 Does not handle:
 - Production API surface changes.
 - Test fixture creation.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Accessors mirror existing internal state without adding new behavior.
 - Test code should prefer these accessors over broadening production visibility.
 */

import Foundation

@testable import RalphCore

@MainActor
extension Workspace {
    var workingDirectoryURL: URL {
        identityState.workingDirectoryURL
    }

    var tasks: [RalphTask] {
        get { taskState.tasks }
        set { taskState.tasks = newValue }
    }

    var tasksLoading: Bool {
        get { taskState.tasksLoading }
        set { taskState.tasksLoading = newValue }
    }

    var tasksErrorMessage: String? {
        get { taskState.tasksErrorMessage }
        set { taskState.tasksErrorMessage = newValue }
    }

    var cliSpecErrorMessage: String? {
        get { commandState.cliSpecErrorMessage }
        set { commandState.cliSpecErrorMessage = newValue }
    }

    var cliSpecIsLoading: Bool {
        get { commandState.cliSpecIsLoading }
        set { commandState.cliSpecIsLoading = newValue }
    }

    var cliHealthStatus: CLIHealthStatus? {
        get { diagnosticsState.cliHealthStatus }
        set { diagnosticsState.cliHealthStatus = newValue }
    }

    var cachedTasks: [RalphTask] {
        get { diagnosticsState.cachedTasks }
        set { diagnosticsState.cachedTasks = newValue }
    }

    var operationalIssues: [WorkspaceOperationalIssue] {
        diagnosticsState.operationalIssues
    }

    var operationalSummary: WorkspaceOperationalSummary {
        diagnosticsState.operationalSummary
    }

    var showOfflineBanner: Bool {
        diagnosticsState.cliHealthStatus?.isAvailable == false
    }

    var isShowingCachedTasks: Bool {
        showOfflineBanner && !diagnosticsState.cachedTasks.isEmpty
    }

    var output: String {
        get { runState.output }
        set { runState.output = newValue }
    }

    var isRunning: Bool {
        get { runState.isRunning }
        set { runState.isRunning = newValue }
    }

    var lastExitStatus: RalphCLIExitStatus? {
        get { runState.lastExitStatus }
        set { runState.lastExitStatus = newValue }
    }

    var currentTaskID: String? {
        get { runState.currentTaskID }
        set { runState.currentTaskID = newValue }
    }

    var currentPhase: Workspace.ExecutionPhase? {
        get { runState.currentPhase }
        set { runState.currentPhase = newValue }
    }

    var executionHistory: [Workspace.ExecutionRecord] {
        get { runState.executionHistory }
        set { runState.executionHistory = newValue }
    }

    var attributedOutput: [Workspace.ANSISegment] {
        get { runState.attributedOutput }
        set { runState.attributedOutput = newValue }
    }

    var maxANSISegments: Int {
        get { runState.maxANSISegments }
        set { runState.maxANSISegments = newValue }
    }

    var runControlSelectedTaskID: String? {
        get { runState.runControlSelectedTaskID }
        set { runState.runControlSelectedTaskID = newValue }
    }

    var isLoopMode: Bool {
        get { runState.isLoopMode }
        set { runState.isLoopMode = newValue }
    }

    var stopAfterCurrent: Bool {
        get { runState.stopAfterCurrent }
        set { runState.stopAfterCurrent = newValue }
    }
}
