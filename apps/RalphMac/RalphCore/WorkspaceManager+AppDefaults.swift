/**
 WorkspaceManager+AppDefaults

 Responsibilities:
 - Prepare app defaults for normal and UI-testing launches.
 - Encapsulate persisted window-state storage helpers used by WorkspaceManager.
 - Resolve the initial CLI client from environment override or bundled binary.

 Does not handle:
 - Workspace restoration flow.
 - Version compatibility checks.

 Invariants/assumptions callers must respect:
 - UI-testing defaults use a dedicated suite and are reset on launch.
 - Production defaults prune stale UI-testing state before the app boots.
 */

public import Foundation

public struct RalphAppLaunchPreparationResult {
    public let persistenceIssue: PersistenceIssue?
}

struct WindowStateStore {
    private let defaults: UserDefaults
    private let restorationKey: String

    init(
        defaults: UserDefaults = RalphAppDefaults.userDefaults,
        restorationKey: String = RalphAppDefaults.productionDomainIdentifier + ".windowRestorationState"
    ) {
        self.defaults = defaults
        self.restorationKey = restorationKey
    }

    func loadAll() throws -> [WindowState] {
        guard let data = defaults.data(forKey: restorationKey) else {
            return []
        }
        return try JSONDecoder().decode([WindowState].self, from: data)
    }

    func saveAll(_ states: [WindowState]) throws {
        let data = try JSONEncoder().encode(states)
        defaults.set(data, forKey: restorationKey)
    }

    func clear() {
        defaults.removeObject(forKey: restorationKey)
    }
}

public enum RalphAppDefaults {
    public static let productionDomainIdentifier = "com.mitchfultz.ralph"
    public static let uiTestingDomainIdentifier = productionDomainIdentifier + ".uitesting"

    private static let uiTestingPathMarker = "/ralph-ui-tests/"
    private static let workspaceKeyPrefix = productionDomainIdentifier + ".workspace."
    private static let navigationKeyPrefix = productionDomainIdentifier + ".navigationState."
    private static let restorationKey = productionDomainIdentifier + ".windowRestorationState"

    public static var isUITesting: Bool {
        ProcessInfo.processInfo.arguments.contains("--uitesting")
    }

    public static var userDefaults: UserDefaults {
        if isUITesting, let suiteDefaults = UserDefaults(suiteName: uiTestingDomainIdentifier) {
            return suiteDefaults
        }
        return .standard
    }

    @MainActor
    public static func prepareForLaunch() -> RalphAppLaunchPreparationResult {
        if isUITesting {
            resetUITestingDefaults()
            return RalphAppLaunchPreparationResult(persistenceIssue: nil)
        }

        return RalphAppLaunchPreparationResult(
            persistenceIssue: pruneUITestingStateFromProductionDefaults()
        )
    }

    private static func resetUITestingDefaults() {
        guard let suiteDefaults = UserDefaults(suiteName: uiTestingDomainIdentifier) else { return }
        suiteDefaults.removePersistentDomain(forName: uiTestingDomainIdentifier)
    }

    private static func pruneUITestingStateFromProductionDefaults() -> PersistenceIssue? {
        let defaults = UserDefaults.standard
        let dictionary = defaults.dictionaryRepresentation()
        var contaminatedWorkspaceIDs = Set<UUID>()
        for key in dictionary.keys where key.hasPrefix(workspaceKeyPrefix) && key.hasSuffix(".snapshot") {
            guard let data = dictionary[key] as? Data else { continue }
            do {
                let snapshot = try JSONDecoder().decode(RalphWorkspaceDefaultsSnapshot.self, from: data)
                guard snapshot.workingDirectoryURL.path.contains(uiTestingPathMarker),
                      let workspaceID = workspaceID(fromWorkspaceKey: key) else {
                    continue
                }
                contaminatedWorkspaceIDs.insert(workspaceID)
            } catch {
                return PersistenceIssue(
                    domain: .appDefaultsPreparation,
                    operation: .load,
                    context: key,
                    error: error
                )
            }
        }

        guard !contaminatedWorkspaceIDs.isEmpty else { return nil }

        for workspaceID in contaminatedWorkspaceIDs {
            removeWorkspaceState(workspaceID, from: defaults)
        }

        do {
            let store = WindowStateStore(defaults: defaults, restorationKey: restorationKey)
            let states = try store.loadAll()
            let filteredStates = states.compactMap { state -> WindowState? in
                var updated = state
                updated.workspaceIDs.removeAll { contaminatedWorkspaceIDs.contains($0) }
                updated.validateSelection()
                return updated.workspaceIDs.isEmpty ? nil : updated
            }
            try store.saveAll(filteredStates)
            return nil
        } catch {
            return PersistenceIssue(
                domain: .appDefaultsPreparation,
                operation: .load,
                context: restorationKey,
                error: error
            )
        }
    }

    private static func removeWorkspaceState(_ workspaceID: UUID, from defaults: UserDefaults) {
        let workspacePrefix = workspaceKeyPrefix + workspaceID.uuidString + "."
        for key in defaults.dictionaryRepresentation().keys where key.hasPrefix(workspacePrefix) {
            defaults.removeObject(forKey: key)
        }

        defaults.removeObject(forKey: navigationKeyPrefix + workspaceID.uuidString)
    }

    private static func workspaceID(fromWorkspaceKey key: String) -> UUID? {
        let suffix = key.dropFirst(workspaceKeyPrefix.count)
        guard let separatorIndex = suffix.firstIndex(of: ".") else { return nil }
        return UUID(uuidString: String(suffix[..<separatorIndex]))
    }
}

public extension WorkspaceManager {
    @discardableResult
    func configureInitialClient() -> Bool {
        let environment = ProcessInfo.processInfo.environment
        if let overridePath = environment[Self.cliBinaryOverrideEnvKey],
           !overridePath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let overrideURL = URL(fileURLWithPath: overridePath, isDirectory: false)
                .standardizedFileURL
                .resolvingSymlinksInPath()
            do {
                client = try RalphCLIClient(executableURL: overrideURL)
                errorMessage = nil
                RalphLogger.shared.info(
                    "Using CLI override from environment: \(overrideURL.path)",
                    category: .cli
                )
                return true
            } catch {
                RalphLogger.shared.error(
                    "Ignoring invalid CLI override '\(overridePath)': \(error)",
                    category: .cli
                )
            }
        }

        do {
            client = try RalphCLIClient.bundled()
            errorMessage = nil
            return true
        } catch {
            errorMessage = "Failed to locate bundled ralph executable: \(error)"
            return false
        }
    }

    /// Reject CLI executable paths provided by URL/launcher context.
    func adoptCLIExecutable(path: String) {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        RalphLogger.shared.error(
            "Rejected insecure URL-driven CLI override: \(trimmed)",
            category: .cli
        )
    }
}
