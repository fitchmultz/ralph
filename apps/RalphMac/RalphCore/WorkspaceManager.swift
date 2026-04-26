/**
 WorkspaceManager

 Purpose:
 - Own the shared WorkspaceManager singleton and its stored app-wide state.

 Responsibilities:
 - Own the shared WorkspaceManager singleton and its stored app-wide state.
 - Coordinate initialization across decomposed lifecycle, restoration, versioning, and routing files.

 Does not handle:
 - Per-workspace rendering or command execution details.
 - The concrete implementation of restoration, versioning, or defaults management in this file.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - The shared instance is the sole app-wide workspace manager.
 - Behavioral surfaces are implemented in `WorkspaceManager+...` files.
 */

public import Foundation
public import Combine

struct WorkspaceRevealRetryConfiguration {
    var maxAttempts: Int = 60
    var yieldAttempts: Int = 10
    var retryDelayNanoseconds: UInt64 = 20_000_000
    var yield: @Sendable () async -> Void = { await Task.yield() }
    var sleep: @Sendable (UInt64) async -> Void = { nanoseconds in
        try? await Task.sleep(nanoseconds: nanoseconds)
    }
}

struct PendingWorkspaceReveal {
    let workspaceID: UUID
    let revision: UInt64
    let startedAt: Date
    var attempts: Int
}

@MainActor
public final class WorkspaceManager: ObservableObject {
    public static let shared = WorkspaceManager()
    public static let cliBinaryOverrideEnvKey = "RALPH_BIN_PATH"

    @Published public internal(set) var workspaces: [Workspace] = []
    @Published public var errorMessage: String?
    @Published public internal(set) var versionCheckResult: VersionValidator.VersionCheckResult?
    @Published public var focusedWorkspace: Workspace?
    @Published public internal(set) var lastActiveWorkspaceID: UUID?
    @Published public internal(set) var persistenceIssue: PersistenceIssue?

    public internal(set) var client: RalphCLIClient?

    let restorationKey = "com.mitchfultz.ralph.windowRestorationState"
    let versionCheckCacheKey = "com.mitchfultz.ralph.versionCheckCache"
    var unclaimedWindowStates: [WindowState] = []
    var restorationPoolInitialized = false
    let sceneRouter = WorkspaceSceneRouter()

    var versionCheckTask: Task<Void, Never>?
    var versionCheckRevision: UInt64 = 0
    var workspaceBootstrapTasks: [UUID: Task<Void, Never>] = [:]
    var workspaceBootstrapRevisions: [UUID: UInt64] = [:]
    var workspaceRevealTask: Task<Void, Never>?
    var workspaceRevealRevision: UInt64 = 0
    var pendingWorkspaceReveal: PendingWorkspaceReveal?
    var workspaceRevealConfiguration = WorkspaceRevealRetryConfiguration()

    private init() {
        let preparation = RalphAppDefaults.prepareForLaunch()
        persistenceIssue = preparation.persistenceIssue

        if !configureInitialClient() {
            return
        }

        scheduleVersionCheck()
        migrateLegacyStateIfNeeded()
    }

    deinit {
        versionCheckTask?.cancel()
        workspaceBootstrapTasks.values.forEach { $0.cancel() }
        workspaceRevealTask?.cancel()
    }
}
