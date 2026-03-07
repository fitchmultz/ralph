/**
 ErrorRecoveryTests

 Test coverage for error classification, recovery suggestions, and error formatting.
 */

import XCTest
@testable import RalphCore

final class ErrorRecoveryTests: XCTestCase {

    // MARK: - ErrorCategory Tests

    func testErrorCategoryDisplayNames() {
        XCTAssertEqual(ErrorCategory.cliUnavailable.displayName, "CLI Not Available")
        XCTAssertEqual(ErrorCategory.permissionDenied.displayName, "Permission Denied")
        XCTAssertEqual(ErrorCategory.parseError.displayName, "Data Parse Error")
        XCTAssertEqual(ErrorCategory.networkError.displayName, "Network Error")
        XCTAssertEqual(ErrorCategory.queueCorrupted.displayName, "Queue Corrupted")
        XCTAssertEqual(ErrorCategory.resourceBusy.displayName, "Resource Busy")
        XCTAssertEqual(ErrorCategory.versionMismatch.displayName, "Version Mismatch")
        XCTAssertEqual(ErrorCategory.unknown.displayName, "Unknown Error")
    }

    func testErrorCategoryIcons() {
        // Verify all categories have non-empty icons
        for category in ErrorCategory.allCases {
            XCTAssertFalse(category.icon.isEmpty, "Category \(category) should have an icon")
        }

        // Verify specific icons
        XCTAssertEqual(ErrorCategory.cliUnavailable.icon, "terminal.fill")
        XCTAssertEqual(ErrorCategory.permissionDenied.icon, "lock.fill")
        XCTAssertEqual(ErrorCategory.parseError.icon, "doc.text.magnifyingglass")
    }

    func testErrorCategorySuggestedActions() {
        // CLI unavailable should suggest permission checks and reinstallation
        let cliActions = ErrorCategory.cliUnavailable.suggestedActions
        XCTAssertTrue(cliActions.contains(.retry))
        XCTAssertTrue(cliActions.contains(.checkPermissions))
        XCTAssertTrue(cliActions.contains(.reinstallCLI))
        XCTAssertTrue(cliActions.contains(.openLogs))

        // Parse errors should suggest validation
        let parseActions = ErrorCategory.parseError.suggestedActions
        XCTAssertTrue(parseActions.contains(.validateQueue))
        XCTAssertTrue(parseActions.contains(.diagnose))

        // Queue corrupted should prioritize validation
        let queueActions = ErrorCategory.queueCorrupted.suggestedActions
        XCTAssertEqual(queueActions.first, .validateQueue)
    }

    func testErrorCategoryGuidanceMessages() {
        XCTAssertNotNil(ErrorCategory.cliUnavailable.guidanceMessage)
        XCTAssertNotNil(ErrorCategory.permissionDenied.guidanceMessage)
        XCTAssertNotNil(ErrorCategory.parseError.guidanceMessage)

        // Unknown errors should still have guidance
        XCTAssertNotNil(ErrorCategory.unknown.guidanceMessage)
    }

    // MARK: - RecoveryError Tests

    func testRecoveryErrorCreation() {
        let error = RecoveryError(
            category: .cliUnavailable,
            message: "Test message",
            underlyingError: "Underlying details",
            operation: "testOperation",
            suggestions: ["Suggestion 1", "Suggestion 2"],
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )

        XCTAssertEqual(error.category, .cliUnavailable)
        XCTAssertEqual(error.message, "Test message")
        XCTAssertEqual(error.underlyingError, "Underlying details")
        XCTAssertEqual(error.operation, "testOperation")
        XCTAssertEqual(error.suggestions.count, 2)
        XCTAssertNotNil(error.workspaceURL)
    }

    func testRecoveryErrorFullDetailsFormatting() {
        let error = RecoveryError(
            category: .permissionDenied,
            message: "Permission denied",
            underlyingError: "EACCES error",
            operation: "loadTasks",
            suggestions: ["Check permissions", "Run with sudo"]
        )

        let details = error.fullErrorDetails

        XCTAssertTrue(details.contains("=== Ralph Error Report ==="))
        XCTAssertTrue(details.contains("Category: Permission Denied"))
        XCTAssertTrue(details.contains("Operation: loadTasks"))
        XCTAssertTrue(details.contains("Message: Permission denied"))
        XCTAssertTrue(details.contains("Details: EACCES error"))
        XCTAssertTrue(details.contains("Suggestions:"))
        XCTAssertTrue(details.contains("Check permissions"))
        XCTAssertTrue(details.contains("=========================="))
    }

    func testRecoveryErrorFullDetailsWithoutUnderlying() {
        let error = RecoveryError(
            category: .unknown,
            message: "Something went wrong",
            operation: "test"
        )

        let details = error.fullErrorDetails

        XCTAssertTrue(details.contains("Something went wrong"))
        XCTAssertFalse(details.contains("Details:"))
    }

    // MARK: - Error Classification Tests

    func testClassifyCLIClientErrorNotFound() {
        let url = URL(fileURLWithPath: "/nonexistent/ralph")
        let error = RalphCLIClientError.executableNotFound(url)

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")

        XCTAssertEqual(recoveryError.category, .cliUnavailable)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("installed") })
    }

    func testClassifyCLIClientErrorNotExecutable() {
        let url = URL(fileURLWithPath: "/path/to/ralph")
        let error = RalphCLIClientError.executableNotExecutable(url)

        let recoveryError = RecoveryError.classify(error: error, operation: "runCommand")

        XCTAssertEqual(recoveryError.category, .cliUnavailable)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("permissions") })
    }

    func testClassifyPermissionError() {
        let error = NSError(domain: NSPOSIXErrorDomain, code: Int(EACCES), userInfo: [
            NSLocalizedDescriptionKey: "Permission denied"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "saveTask")

        XCTAssertEqual(recoveryError.category, .permissionDenied)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("permissions") })
    }

    func testClassifyParseError() {
        let error = NSError(domain: NSCocoaErrorDomain, code: 3840, userInfo: [
            NSLocalizedDescriptionKey: "JSON parse error: unexpected character"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")

        XCTAssertEqual(recoveryError.category, .parseError)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("validate") })
    }

    func testClassifyDecodingErrorAsParseError() {
        let payload = Data("{\"invalid\":true}".utf8)

        do {
            _ = try JSONDecoder().decode(RalphTaskQueueDocument.self, from: payload)
            XCTFail("Expected decode to fail for invalid fixture shape")
        } catch {
            let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
            XCTAssertEqual(recoveryError.category, .parseError)
            XCTAssertTrue(recoveryError.message.localizedCaseInsensitiveContains("parse"))
        }
    }

    func testClassifyRetryableProcessErrorUsesStderr() {
        let error = RetryableError.processError(
            exitCode: 2,
            stderr: "Error: read queue file /tmp/.ralph/queue.json: No such file or directory (os error 2)"
        )

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")

        XCTAssertEqual(recoveryError.category, .queueCorrupted)
        XCTAssertTrue(recoveryError.message.localizedCaseInsensitiveContains("queue"))
    }

    func testClassifyMissingQueueFileIsActionable() {
        let error = NSError(
            domain: "RalphCore.CLIProcess",
            code: 2,
            userInfo: [
                NSLocalizedDescriptionKey: "Error: read queue file /Users/test/.ralph/queue.json: No such file or directory (os error 2)"
            ]
        )

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")

        XCTAssertEqual(recoveryError.category, .queueCorrupted)
        XCTAssertTrue(recoveryError.message.contains("No Ralph queue file found"))
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("ralph init --non-interactive") })
    }

    func testClassifyRetryableProcessErrorWithoutStderrIncludesExitCode() {
        let error = RetryableError.processError(exitCode: 42, stderr: "")

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")

        XCTAssertEqual(recoveryError.category, .unknown)
        XCTAssertTrue(recoveryError.message.contains("exit code 42"))
    }

    func testClassifyResourceBusyError() {
        let error = NSError(domain: NSPOSIXErrorDomain, code: Int(EAGAIN), userInfo: [
            NSLocalizedDescriptionKey: "Resource temporarily unavailable"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "updateTask")

        XCTAssertEqual(recoveryError.category, .resourceBusy)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("retry") })
    }

    func testClassifyQueueCorruptedError() {
        let error = NSError(domain: "RalphError", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Queue file is corrupted"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")

        XCTAssertEqual(recoveryError.category, .queueCorrupted)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("backup") })
    }

    func testClassifyNetworkError() {
        let error = NSError(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet, userInfo: [
            NSLocalizedDescriptionKey: "Network connection lost"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "sync")

        XCTAssertEqual(recoveryError.category, .networkError)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("network") })
    }

    func testClassifyVersionMismatch() {
        let minimumVersion = VersionCompatibility.minimumCLIVersion
        let foundVersion = "0.0.0"
        let error = NSError(domain: "VersionError", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Ralph CLI version is too old (\(foundVersion)). Minimum supported version is \(minimumVersion)."
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "checkVersion")

        XCTAssertEqual(recoveryError.category, .versionMismatch)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("reinstall") })
    }

    func testClassifyUnknownError() {
        let error = NSError(domain: "UnknownDomain", code: 999, userInfo: [
            NSLocalizedDescriptionKey: "Something mysterious happened"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "unknownOperation")

        XCTAssertEqual(recoveryError.category, .unknown)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("logs") })
    }

    func testClassifyPreservesWorkspaceURL() {
        let workspaceURL = URL(fileURLWithPath: "/path/to/workspace")
        let error = NSError(domain: "Test", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Test error"
        ])

        let recoveryError = RecoveryError.classify(
            error: error,
            operation: "test",
            workspaceURL: workspaceURL
        )

        XCTAssertEqual(recoveryError.workspaceURL, workspaceURL)
    }

    // MARK: - RetryState Tests

    func testRetryStateNotExhausted() {
        let state = RetryState(isRetrying: true, attempt: 1, maxAttempts: 3)

        XCTAssertTrue(state.isRetrying)
        XCTAssertEqual(state.attempt, 1)
        XCTAssertEqual(state.maxAttempts, 3)
        XCTAssertFalse(state.isExhausted)
        XCTAssertFalse(state.canRetryManually)
    }

    func testRetryStateExhausted() {
        let state = RetryState(isRetrying: false, attempt: 3, maxAttempts: 3)

        XCTAssertFalse(state.isRetrying)
        XCTAssertTrue(state.isExhausted)
        XCTAssertTrue(state.canRetryManually)
    }

    func testRetryStateBeyondMax() {
        let state = RetryState(isRetrying: false, attempt: 5, maxAttempts: 3)

        XCTAssertTrue(state.isExhausted)
        XCTAssertTrue(state.canRetryManually)
    }

    // MARK: - RecoveryAction Tests

    func testRecoveryActionRawValues() {
        XCTAssertEqual(RecoveryAction.retry.rawValue, "retry")
        XCTAssertEqual(RecoveryAction.diagnose.rawValue, "diagnose")
        XCTAssertEqual(RecoveryAction.copyErrorDetails.rawValue, "copyErrorDetails")
        XCTAssertEqual(RecoveryAction.openLogs.rawValue, "openLogs")
        XCTAssertEqual(RecoveryAction.dismiss.rawValue, "dismiss")
        XCTAssertEqual(RecoveryAction.checkPermissions.rawValue, "checkPermissions")
        XCTAssertEqual(RecoveryAction.reinstallCLI.rawValue, "reinstallCLI")
        XCTAssertEqual(RecoveryAction.validateQueue.rawValue, "validateQueue")
    }

    func testAllCategoriesHaveSuggestedActions() {
        for category in ErrorCategory.allCases {
            let actions = category.suggestedActions
            XCTAssertFalse(actions.isEmpty, "Category \(category) should have suggested actions")
            XCTAssertTrue(actions.contains(.dismiss), "All categories should include dismiss action")
        }
    }

    func testCriticalErrorsHaveAppropriateActions() {
        // Critical errors should have specific recovery options
        let cliActions = ErrorCategory.cliUnavailable.suggestedActions
        XCTAssertTrue(cliActions.contains(.checkPermissions))
        XCTAssertTrue(cliActions.contains(.reinstallCLI))

        let permissionActions = ErrorCategory.permissionDenied.suggestedActions
        XCTAssertTrue(permissionActions.contains(.checkPermissions))

        let corruptionActions = ErrorCategory.queueCorrupted.suggestedActions
        XCTAssertTrue(corruptionActions.contains(.validateQueue))
    }
    
    // MARK: - Offline Guidance Tests
    
    func testOfflineGuidanceForCLIUnavailable() {
        let guidance = ErrorCategory.cliUnavailable.offlineGuidance
        XCTAssertNotNil(guidance)
        XCTAssertTrue(guidance?.contains("app bundle") ?? false)
        XCTAssertTrue(guidance?.contains("Antivirus") ?? false)
    }
    
    func testOfflineGuidanceForPermissionDenied() {
        let guidance = ErrorCategory.permissionDenied.offlineGuidance
        XCTAssertNotNil(guidance)
        XCTAssertTrue(guidance?.contains("directory was moved") ?? false)
        XCTAssertTrue(guidance?.contains("permissions") ?? false)
    }
    
    func testOfflineGuidanceForNetworkError() {
        let guidance = ErrorCategory.networkError.offlineGuidance
        XCTAssertNotNil(guidance)
        XCTAssertTrue(guidance?.contains("timed out") ?? false)
    }
    
    func testOfflineGuidanceFallbackToGuidanceMessage() {
        // For categories without specific offline guidance, should fallback to regular guidance
        let guidance = ErrorCategory.parseError.offlineGuidance
        XCTAssertEqual(guidance, ErrorCategory.parseError.guidanceMessage)
    }
}

// MARK: - CLIHealthChecker Tests

final class CLIHealthCheckerTests: XCTestCase {
    
    func testHealthStatusAvailable() {
        let status = CLIHealthStatus(
            availability: .available,
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        
        XCTAssertTrue(status.isAvailable)
    }
    
    func testHealthStatusUnavailableCLI() {
        let status = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        
        XCTAssertFalse(status.isAvailable)
    }
    
    func testHealthStatusUnknown() {
        let status = CLIHealthStatus(
            availability: .unknown,
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        
        XCTAssertFalse(status.isAvailable)
    }
    
    func testUnavailabilityReasonErrorCategory() {
        XCTAssertEqual(
            CLIHealthStatus.UnavailabilityReason.cliNotFound.errorCategory,
            .cliUnavailable
        )
        XCTAssertEqual(
            CLIHealthStatus.UnavailabilityReason.permissionDenied.errorCategory,
            .permissionDenied
        )
        XCTAssertEqual(
            CLIHealthStatus.UnavailabilityReason.timeout.errorCategory,
            .networkError
        )
    }
    
    func testIsCLIUnavailableError() {
        let notFoundError = RalphCLIClientError.executableNotFound(
            URL(fileURLWithPath: "/nonexistent")
        )
        XCTAssertTrue(CLIHealthChecker.isCLIUnavailableError(notFoundError))
        
        let notExecError = RalphCLIClientError.executableNotExecutable(
            URL(fileURLWithPath: "/tmp")
        )
        XCTAssertTrue(CLIHealthChecker.isCLIUnavailableError(notExecError))
        
        let genericError = NSError(domain: "Test", code: 1)
        XCTAssertFalse(CLIHealthChecker.isCLIUnavailableError(genericError))
    }
    
    func testDefaultTimeoutValue() {
        XCTAssertEqual(CLIHealthChecker.defaultTimeout, 30)
    }

    func testCheckHealth_usesProvidedExecutableOverride() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-health-override")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let scriptURL = tempDir.appendingPathComponent("mock-ralph", isDirectory: false)
        let script = """
        #!/bin/sh
        if [ "$1" = "--version" ]; then
          echo "ralph 9.9.9"
          exit 0
        fi
        exit 1
        """
        try script.write(to: scriptURL, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o755))],
            ofItemAtPath: scriptURL.path
        )

        let checker = CLIHealthChecker()
        let status = await checker.checkHealth(
            workspaceID: UUID(),
            workspaceURL: tempDir,
            timeout: 2,
            executableURL: scriptURL
        )

        XCTAssertEqual(status.availability, .available)
    }

    func testCheckHealth_fallsBackToVersionSubcommandWhenDashVersionUnsupported() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-health-fallback")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let scriptURL = tempDir.appendingPathComponent("mock-ralph", isDirectory: false)
        let script = """
        #!/bin/sh
        if [ "$1" = "--version" ]; then
          echo "error: unexpected argument '--version' found" >&2
          exit 2
        fi
        if [ "$1" = "version" ]; then
          echo "ralph 9.9.9"
          exit 0
        fi
        exit 1
        """
        try script.write(to: scriptURL, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o755))],
            ofItemAtPath: scriptURL.path
        )

        let checker = CLIHealthChecker()
        let status = await checker.checkHealth(
            workspaceID: UUID(),
            workspaceURL: tempDir,
            timeout: 2,
            executableURL: scriptURL
        )

        XCTAssertEqual(status.availability, .available)
    }

    func testCheckHealth_invalidProvidedExecutableReportsCliNotFound() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-health-missing")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let checker = CLIHealthChecker()
        let status = await checker.checkHealth(
            workspaceID: UUID(),
            workspaceURL: tempDir,
            timeout: 2,
            executableURL: URL(fileURLWithPath: "/definitely/not/a/real/ralph-binary")
        )

        XCTAssertEqual(status.availability, .unavailable(reason: .cliNotFound))
    }

    private static func makeTempDir(prefix: String) throws -> URL {
        let tempRoot = FileManager.default.temporaryDirectory
        let directory = tempRoot.appendingPathComponent("\(prefix)-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }
}

// MARK: - TimeoutConfiguration Tests

final class TimeoutConfigurationTests: XCTestCase {
    
    func testDefaultConfiguration() {
        let config = TimeoutConfiguration.default
        XCTAssertEqual(config.timeout, 30)
        XCTAssertEqual(config.terminationGracePeriod, 2)
    }
    
    func testLongRunningConfiguration() {
        let config = TimeoutConfiguration.longRunning
        XCTAssertEqual(config.timeout, 300)
        XCTAssertEqual(config.terminationGracePeriod, 2)
    }
    
    func testCustomConfiguration() {
        let config = TimeoutConfiguration(timeout: 60, terminationGracePeriod: 5)
        XCTAssertEqual(config.timeout, 60)
        XCTAssertEqual(config.terminationGracePeriod, 5)
    }
}

// MARK: - Workspace Caching Tests

@MainActor
final class WorkspaceCachingTests: XCTestCase {
    
    func testShowOfflineBannerWhenUnavailable() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        
        // Initially no status, should not show banner
        XCTAssertFalse(workspace.showOfflineBanner)
        
        // Set unavailable status
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        
        XCTAssertTrue(workspace.showOfflineBanner)
    }
    
    func testShowOfflineBannerWhenAvailable() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .available,
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        
        XCTAssertFalse(workspace.showOfflineBanner)
    }
    
    func testIsShowingCachedTasks() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        
        // Initially no cached tasks
        XCTAssertFalse(workspace.isShowingCachedTasks)
        
        // Set offline status with cached tasks
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        workspace.cachedTasks = [
            RalphTask(id: "RQ-TEST", status: .todo, title: "Test", priority: .medium)
        ]
        
        XCTAssertTrue(workspace.isShowingCachedTasks)
    }
    
    func testDisplayTasksWhenOffline() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        
        let onlineTask = RalphTask(id: "RQ-ONLINE", status: .todo, title: "Online", priority: .medium)
        let cachedTask = RalphTask(id: "RQ-CACHED", status: .done, title: "Cached", priority: .low)
        
        workspace.tasks = [onlineTask]
        workspace.cachedTasks = [cachedTask]
        
        // Simulate offline status
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        
        // Should return cached tasks when offline
        let displayTasks = workspace.displayTasks()
        XCTAssertEqual(displayTasks.count, 1)
        XCTAssertEqual(displayTasks.first?.id, "RQ-CACHED")
    }
    
    func testDisplayTasksWhenOnline() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        
        let onlineTask = RalphTask(id: "RQ-ONLINE", status: .todo, title: "Online", priority: .medium)
        
        workspace.tasks = [onlineTask]
        workspace.cachedTasks = []
        
        // Simulate online status
        workspace.cliHealthStatus = CLIHealthStatus(
            availability: .available,
            lastChecked: Date(),
            workspaceURL: URL(fileURLWithPath: "/tmp")
        )
        
        // Should return current tasks when online
        let displayTasks = workspace.displayTasks()
        XCTAssertEqual(displayTasks.count, 1)
        XCTAssertEqual(displayTasks.first?.id, "RQ-ONLINE")
    }
    
    func testClearCachedTasks() {
        let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
        
        workspace.cachedTasks = [
            RalphTask(id: "RQ-TEST", status: .todo, title: "Test", priority: .medium)
        ]
        
        workspace.clearCachedTasks()
        
        XCTAssertTrue(workspace.cachedTasks.isEmpty)
    }
}
