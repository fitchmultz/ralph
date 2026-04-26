/**
 Workspace+RunnerState

 Purpose:
 - Define the workspace-scoped run-state storage anchor used by runner orchestration.

 Responsibilities:
 - Store published run lifecycle, console, blocking, and operator-state fields.
 - Maintain shared storage used by runner-state companion extensions.
 - Expose lightweight computed state used by run-control UI surfaces.

 Scope:
 - In scope: `WorkspaceRunState` stored properties, initialization, and tiny computed helpers.
 - Out of scope: operator-state synthesis, console rendering lifecycle, run commands, and runner metadata models.

 Usage:
 - Owned by `Workspace` and consumed by runner controller and run-control views.

 Invariants/assumptions callers must respect:
 - State is workspace-local and must not be shared across workspaces.
 - Only one active execution lifecycle is tracked at a time.
 - Companion extensions are responsible for mutating the corresponding subdomain state.
 */
public import Combine
public import Foundation

@MainActor
public final class WorkspaceRunState: ObservableObject {
    static let consoleRenderRefreshIntervalNanoseconds: UInt64 = 50_000_000

    @Published public var output = ""
    @Published public var isPreparingRun = false
    @Published public var isRunning = false
    @Published public var lastExitStatus: RalphCLIExitStatus?
    @Published public var errorMessage: String?
    @Published public var currentTaskID: String?
    @Published public var currentPhase: Workspace.ExecutionPhase?
    @Published public var executionStartTime: Date?
    @Published public var isLoopMode = false
    @Published public var stopAfterCurrent = false
    @Published public var executionHistory: [Workspace.ExecutionRecord] = []
    @Published public var currentRunnerConfig: Workspace.RunnerConfig?
    @Published public var runnerConfigLoading = false
    @Published public var runnerConfigErrorMessage: String?
    @Published public var parallelStatusLoading = false
    @Published public var parallelStatusErrorMessage: String?
    @Published public var runControlSelectedTaskID: String?
    @Published public var runControlForceDirtyRepo = false
    @Published public var runControlParallelWorkersOverride: Int?
    @Published public var resumeState: Workspace.ResumeState? {
        didSet { refreshOperatorState() }
    }
    @Published public internal(set) var blockingState: Workspace.BlockingState?
    @Published public var parallelStatus: Workspace.ParallelStatus? {
        didSet { refreshOperatorState() }
    }
    @Published public internal(set) var runControlOperatorState: Workspace.RunControlOperatorState?
    @Published public var attributedOutput: [Workspace.ANSISegment] = []
    @Published public var outputBuffer: ConsoleOutputBuffer
    @Published public var maxANSISegments = 1_000 {
        didSet {
            if maxANSISegments != oldValue {
                attributedOutput = streamProcessor.displaySegments(
                    maxSegments: maxANSISegments,
                    maxCharacters: outputBuffer.maxCharacters
                )
            }
        }
    }

    let streamProcessor = WorkspaceStreamProcessor()
    var liveBlockingState: Workspace.BlockingState?
    var queueBlockingState: Workspace.BlockingState?
    var pendingConsoleRenderRefreshTask: Task<Void, Never>?
    var pendingConsoleText = ""

    var hasMeaningfulParallelStatus: Bool {
        parallelStatus?.isMeaningful == true
    }

    public var isExecutionActive: Bool {
        isPreparingRun || isRunning
    }

    public var shouldShowRunControlParallelStatus: Bool {
        parallelStatusLoading
            || parallelStatusErrorMessage != nil
            || runControlParallelWorkersOverride != nil
            || currentRunnerConfig?.safety?.parallelConfigured == true
            || hasMeaningfulParallelStatus
    }

    public init(outputBuffer: ConsoleOutputBuffer) {
        self.outputBuffer = outputBuffer
    }
}
