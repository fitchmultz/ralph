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
        let error = NSError(domain: "VersionError", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Ralph CLI version is too old (0.1.0). Minimum supported version is 0.2.0."
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
}
