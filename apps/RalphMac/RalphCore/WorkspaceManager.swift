/**
 WorkspaceManager

 Responsibilities:
 - Own the shared WorkspaceManager singleton and its stored app-wide state.
 - Coordinate initialization across decomposed lifecycle, restoration, versioning, and routing files.

 Does not handle:
 - Per-workspace rendering or command execution details.
 - The concrete implementation of restoration, versioning, or defaults management in this file.

 Invariants/assumptions callers must respect:
 - The shared instance is the sole app-wide workspace manager.
 - Behavioral surfaces are implemented in `WorkspaceManager+...` files.
 */

public import Foundation
public import Combine

@MainActor
public final class WorkspaceManager: ObservableObject {
    public static let shared = WorkspaceManager()
    public static let cliBinaryOverrideEnvKey = "RALPH_BIN_PATH"

    @Published public internal(set) var workspaces: [Workspace] = []
    @Published public var errorMessage: String?
    @Published public internal(set) var versionCheckResult: VersionValidator.VersionCheckResult?
    @Published public var focusedWorkspace: Workspace?
    @Published public internal(set) var persistenceIssue: PersistenceIssue?

    public internal(set) var client: RalphCLIClient?

    let restorationKey = "com.mitchfultz.ralph.windowRestorationState"
    let versionCheckCacheKey = "com.mitchfultz.ralph.versionCheckCache"
    var unclaimedWindowStates: [WindowState] = []
    var restorationPoolInitialized = false
    let sceneRouter = WorkspaceSceneRouter()

    private init() {
        let preparation = RalphAppDefaults.prepareForLaunch()
        persistenceIssue = preparation.persistenceIssue

        if !configureInitialClient() {
            return
        }

        Task { @MainActor in
            await performVersionCheck()
        }

        migrateLegacyStateIfNeeded()
    }
}
