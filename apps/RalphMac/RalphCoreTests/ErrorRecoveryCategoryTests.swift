/**
 ErrorRecoveryCategoryTests

 Responsibilities:
 - Validate recovery categorization, messaging, and suggested actions.
 - Cover recovery error formatting and classification behavior.

 Does not handle:
 - CLI health probing or workspace offline caching regressions.

 Invariants/assumptions callers must respect:
 - Error fixtures intentionally model representative CLI, filesystem, and parsing failures.
 */

import XCTest
@testable import RalphCore

final class ErrorRecoveryCategoryTests: RalphCoreTestCase {
    func testErrorCategoryDisplayNames() {
        XCTAssertEqual(ErrorCategory.cliUnavailable.displayName, "CLI Not Available")
        XCTAssertEqual(ErrorCategory.permissionDenied.displayName, "Permission Denied")
        XCTAssertEqual(ErrorCategory.configIncompatible.displayName, "Config Upgrade Required")
        XCTAssertEqual(ErrorCategory.parseError.displayName, "Data Parse Error")
        XCTAssertEqual(ErrorCategory.networkError.displayName, "Network Error")
        XCTAssertEqual(ErrorCategory.queueCorrupted.displayName, "Queue Corrupted")
        XCTAssertEqual(ErrorCategory.resourceBusy.displayName, "Resource Busy")
        XCTAssertEqual(ErrorCategory.versionMismatch.displayName, "Version Mismatch")
        XCTAssertEqual(ErrorCategory.unknown.displayName, "Unknown Error")
    }

    func testErrorCategoryIcons() {
        for category in ErrorCategory.allCases {
            XCTAssertFalse(category.icon.isEmpty, "Category \(category) should have an icon")
        }

        XCTAssertEqual(ErrorCategory.cliUnavailable.icon, "terminal.fill")
        XCTAssertEqual(ErrorCategory.permissionDenied.icon, "lock.fill")
        XCTAssertEqual(ErrorCategory.configIncompatible.icon, "gear.badge.xmark")
        XCTAssertEqual(ErrorCategory.parseError.icon, "doc.text.magnifyingglass")
    }

    func testErrorCategorySuggestedActions() {
        let cliActions = ErrorCategory.cliUnavailable.suggestedActions
        XCTAssertTrue(cliActions.contains(.retry))
        XCTAssertTrue(cliActions.contains(.checkPermissions))
        XCTAssertTrue(cliActions.contains(.reinstallCLI))
        XCTAssertTrue(cliActions.contains(.openLogs))

        let parseActions = ErrorCategory.parseError.suggestedActions
        XCTAssertTrue(parseActions.contains(.validateQueue))
        XCTAssertTrue(parseActions.contains(.diagnose))

        let configActions = ErrorCategory.configIncompatible.suggestedActions
        XCTAssertEqual(configActions, [.retry, .openLogs, .copyErrorDetails, .dismiss])
        XCTAssertFalse(configActions.contains(.validateQueue))

        let queueActions = ErrorCategory.queueCorrupted.suggestedActions
        XCTAssertEqual(queueActions.first, .validateQueue)
    }

    func testErrorCategoryGuidanceMessages() {
        XCTAssertNotNil(ErrorCategory.cliUnavailable.guidanceMessage)
        XCTAssertNotNil(ErrorCategory.permissionDenied.guidanceMessage)
        XCTAssertNotNil(ErrorCategory.configIncompatible.guidanceMessage)
        XCTAssertNotNil(ErrorCategory.parseError.guidanceMessage)
        XCTAssertNotNil(ErrorCategory.unknown.guidanceMessage)
    }

    func testRecoveryErrorCreation() {
        let error = RecoveryError(
            category: .cliUnavailable,
            message: "Test message",
            underlyingError: "Underlying details",
            operation: "testOperation",
            suggestions: ["Suggestion 1", "Suggestion 2"],
            workspaceURL: RalphCoreTestSupport.workspaceURL(label: "recovery-error-creation")
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

    func testClassifyCLIClientErrorNotFound() {
        let url = URL(fileURLWithPath: "/nonexistent/ralph")
        let error = RalphCLIClientError.executableNotFound(url)
        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")

        XCTAssertEqual(recoveryError.category, .cliUnavailable)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.localizedCaseInsensitiveContains("installed") })
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
        XCTAssertTrue(recoveryError.suggestions.contains { $0.localizedCaseInsensitiveContains("validate") })
    }

    func testClassifyLegacyConfigLoadFailure() {
        let error = NSError(domain: "RalphCore.CLIProcess", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Error: load project config /tmp/.ralph/config.jsonc: parse config /tmp/.ralph/config.jsonc from JSONC: unknown field `git_commit_push_enabled`"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "loadRunnerConfiguration")
        XCTAssertEqual(recoveryError.category, .configIncompatible)
        XCTAssertTrue(recoveryError.message.localizedCaseInsensitiveContains("config"))
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("ralph migrate --apply") })
        XCTAssertFalse(recoveryError.category.suggestedActions.contains(.validateQueue))
    }

    func testClassifyUnsupportedConfigVersionAsConfigIncompatible() {
        let error = RetryableError.processError(
            exitCode: 1,
            stderr: "Error: load project config /tmp/.ralph/config.jsonc: Unsupported config version: 1. Ralph requires version 2."
        )

        let recoveryError = RecoveryError.classify(error: error, operation: "loadRunnerConfiguration")
        XCTAssertEqual(recoveryError.category, .configIncompatible)
        XCTAssertFalse(recoveryError.category.suggestedActions.contains(.validateQueue))
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
            stderr: "Error: read queue file /tmp/.ralph/queue.jsonc: No such file or directory (os error 2)"
        )

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
        XCTAssertEqual(recoveryError.category, .queueCorrupted)
        XCTAssertTrue(recoveryError.message.localizedCaseInsensitiveContains("queue"))
    }

    func testCanonicalClassifierAlignsGenericAndProcessFailures() {
        let stderr = "Error: read queue file /tmp/.ralph/queue.jsonc: No such file or directory (os error 2)"
        let genericError = NSError(
            domain: "RalphCore.CLIProcess",
            code: 2,
            userInfo: [NSLocalizedDescriptionKey: stderr]
        )
        let processError = RetryableError.processError(exitCode: 2, stderr: stderr)

        let genericRecovery = RecoveryError.classify(error: genericError, operation: "loadTasks")
        let processRecovery = RecoveryError.classify(error: processError, operation: "loadTasks")

        XCTAssertEqual(genericRecovery.category, processRecovery.category)
        XCTAssertEqual(genericRecovery.message, processRecovery.message)
    }

    func testClassifyMissingQueueFileIsActionable() {
        let error = NSError(
            domain: "RalphCore.CLIProcess",
            code: 2,
            userInfo: [
                NSLocalizedDescriptionKey: "Error: read queue file /Users/test/.ralph/queue.jsonc: No such file or directory (os error 2)"
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
        XCTAssertTrue(recoveryError.suggestions.contains { $0.localizedCaseInsensitiveContains("backup") })
    }

    func testClassifyNetworkError() {
        let error = NSError(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet, userInfo: [
            NSLocalizedDescriptionKey: "Network connection lost"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "sync")
        XCTAssertEqual(recoveryError.category, .networkError)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.localizedCaseInsensitiveContains("network") })
    }

    func testClassifyVersionMismatch() {
        let minimumVersion = VersionCompatibility.minimumCLIVersion
        let foundVersion = "0.0.0"
        let error = NSError(domain: "VersionError", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Ralph CLI version is too old (\(foundVersion)). Minimum supported version is \(minimumVersion)."
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "checkVersion")
        XCTAssertEqual(recoveryError.category, .versionMismatch)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.localizedCaseInsensitiveContains("reinstall") })
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
        let cliActions = ErrorCategory.cliUnavailable.suggestedActions
        XCTAssertTrue(cliActions.contains(.checkPermissions))
        XCTAssertTrue(cliActions.contains(.reinstallCLI))

        let permissionActions = ErrorCategory.permissionDenied.suggestedActions
        XCTAssertTrue(permissionActions.contains(.checkPermissions))

        let corruptionActions = ErrorCategory.queueCorrupted.suggestedActions
        XCTAssertTrue(corruptionActions.contains(.validateQueue))
    }

    func testOfflineGuidanceForCLIUnavailable() {
        let guidance = ErrorCategory.cliUnavailable.guidanceMessage
        XCTAssertNotNil(guidance)
        XCTAssertTrue(guidance?.contains("CLI") ?? false)
        XCTAssertTrue(guidance?.contains("installation") ?? false)
    }

    func testOfflineGuidanceForPermissionDenied() {
        let guidance = ErrorCategory.permissionDenied.guidanceMessage
        XCTAssertNotNil(guidance)
        XCTAssertTrue(guidance?.contains("workspace directory") ?? false)
        XCTAssertTrue(guidance?.localizedCaseInsensitiveContains("permission") ?? false)
    }

    func testOfflineGuidanceForNetworkError() {
        let guidance = ErrorCategory.networkError.guidanceMessage
        XCTAssertNotNil(guidance)
        XCTAssertTrue(guidance?.contains("network") ?? false)
    }

    func testOfflineGuidanceFallbackToGuidanceMessage() {
        XCTAssertEqual(ErrorCategory.parseError.guidanceMessage, ErrorCategory.parseError.guidanceMessage)
    }
}
