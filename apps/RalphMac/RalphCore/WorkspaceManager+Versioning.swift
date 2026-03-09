/**
 WorkspaceManager+Versioning

 Responsibilities:
 - Probe the bundled CLI version and validate compatibility with the app.
 - Cache successful version checks to reduce startup subprocess churn.
 - Record persistence issues tied to version-cache IO.

 Does not handle:
 - Workspace lifecycle or restoration.
 - Scene routing.

 Invariants/assumptions callers must respect:
 - Only compatible version checks are cached.
 - `--version` is preferred, with `version` as the fallback compatibility path.
 */

import Foundation

private struct CachedVersionResult: Codable {
    let timestamp: Date
    let isCompatible: Bool
    let versionString: String
}

public extension WorkspaceManager {
    @MainActor
    func performVersionCheck() async {
        if let cached = checkCachedVersionResult(), cached.isCompatible {
            RalphLogger.shared.debug("Using cached CLI version check result", category: .cli)
            versionCheckResult = cached
            return
        }

        let result = await executeVersionCheck()
        if let result {
            versionCheckResult = result

            if result.isCompatible {
                cacheVersionResult(result)
                RalphLogger.shared.info("CLI version compatible: \(result.rawVersion)", category: .cli)
            } else {
                var message = result.errorMessage ?? "Unknown version error"
                if let guidance = result.guidanceMessage {
                    message += "\n\n" + guidance
                }
                errorMessage = message
                RalphLogger.shared.error("CLI version incompatible: \(message)", category: .cli)
            }
        }
    }

    @MainActor
    func executeVersionCheck() async -> VersionValidator.VersionCheckResult? {
        guard let client else {
            errorMessage = "Cannot check CLI version: client not initialized"
            return nil
        }

        do {
            var output = try await client.runAndCollect(arguments: ["--version"])
            if output.status.code != 0 {
                output = try await client.runAndCollect(arguments: ["version"])
            }

            guard output.status.code == 0 else {
                let message = "CLI version check failed with exit code \(output.status.code)"
                errorMessage = message
                RalphLogger.shared.error("CLI version check failed: \(message)", category: .cli)
                return nil
            }

            let versionString = output.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
            let validator = VersionValidator()
            return validator.validate(versionString)
        } catch {
            let message = "Failed to check CLI version: \(error.localizedDescription)"
            errorMessage = message
            RalphLogger.shared.error("Failed to check CLI version: \(message)", category: .cli)
            return nil
        }
    }

    func checkCachedVersionResult() -> VersionValidator.VersionCheckResult? {
        guard let data = RalphAppDefaults.userDefaults.data(forKey: versionCheckCacheKey) else {
            return nil
        }
        let cached: CachedVersionResult
        do {
            cached = try JSONDecoder().decode(CachedVersionResult.self, from: data)
            clearPersistenceIssue(domain: .versionCache)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .versionCache,
                    operation: .load,
                    context: versionCheckCacheKey,
                    error: error
                )
            )
            RalphAppDefaults.userDefaults.removeObject(forKey: versionCheckCacheKey)
            return nil
        }

        let age = Date().timeIntervalSince(cached.timestamp)
        guard age < VersionCompatibility.cacheDuration else {
            RalphAppDefaults.userDefaults.removeObject(forKey: versionCheckCacheKey)
            return nil
        }

        if cached.isCompatible {
            return VersionValidator.VersionCheckResult(status: .compatible, rawVersion: cached.versionString)
        }

        return nil
    }

    func cacheVersionResult(_ result: VersionValidator.VersionCheckResult) {
        guard result.isCompatible else { return }

        let cached = CachedVersionResult(
            timestamp: Date(),
            isCompatible: true,
            versionString: result.rawVersion
        )

        do {
            let data = try JSONEncoder().encode(cached)
            RalphAppDefaults.userDefaults.set(data, forKey: versionCheckCacheKey)
            clearPersistenceIssue(domain: .versionCache)
        } catch {
            recordPersistenceIssue(
                PersistenceIssue(
                    domain: .versionCache,
                    operation: .save,
                    context: versionCheckCacheKey,
                    error: error
                )
            )
        }
    }

    @MainActor
    func checkForCLIUpdates() async -> VersionValidator.VersionCheckResult? {
        RalphAppDefaults.userDefaults.removeObject(forKey: versionCheckCacheKey)

        guard let result = await executeVersionCheck() else {
            return nil
        }

        versionCheckResult = result

        if result.isCompatible {
            cacheVersionResult(result)
        } else {
            var message = result.errorMessage ?? "Unknown version error"
            if let guidance = result.guidanceMessage {
                message += "\n\n" + guidance
            }
            errorMessage = message
        }

        return result
    }

    func recordPersistenceIssue(_ issue: PersistenceIssue) {
        persistenceIssue = issue
        RalphLogger.shared.error(
            "WorkspaceManager persistence \(issue.domain.rawValue) \(issue.operation.rawValue) failed for \(issue.context): \(issue.message)",
            category: .workspace
        )
    }

    func clearPersistenceIssue(domain: PersistenceIssue.Domain) {
        guard persistenceIssue?.domain == domain else { return }
        persistenceIssue = nil
    }
}
