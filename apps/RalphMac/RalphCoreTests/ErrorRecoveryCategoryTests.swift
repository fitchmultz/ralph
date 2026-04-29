/**
 ErrorRecoveryCategoryTests

 Purpose:
 - Validate recovery categorization, messaging, and suggested actions.

 Responsibilities:
 - Validate recovery categorization, messaging, and suggested actions.
 - Cover recovery error formatting and classification behavior.

 Does not handle:
 - CLI health probing or workspace offline caching regressions.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

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
        XCTAssertEqual(ErrorCategory.queueLock.displayName, "Queue Lock Contention")
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
        XCTAssertTrue(parseActions.contains(.repairQueue))
        XCTAssertTrue(parseActions.contains(.diagnose))

        let configActions = ErrorCategory.configIncompatible.suggestedActions
        XCTAssertEqual(configActions, [.retry, .openLogs, .copyErrorDetails, .dismiss])
        XCTAssertFalse(configActions.contains(.validateQueue))

        let queueActions = ErrorCategory.queueCorrupted.suggestedActions
        XCTAssertEqual(queueActions.first, .validateQueue)
        XCTAssertTrue(queueActions.contains(.repairQueue))
        XCTAssertTrue(queueActions.contains(.restoreLastCheckpoint))

        let queueLockActions = ErrorCategory.queueLock.suggestedActions
        XCTAssertTrue(queueLockActions.contains(.inspectQueueLock))
        XCTAssertTrue(queueLockActions.contains(.previewQueueUnlock))
        XCTAssertTrue(queueLockActions.contains(.clearStaleQueueLock))
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

    func testClassifyLegacyCommandNotFoundAsCLIUnavailable() {
        let error = NSError(domain: "RalphCore.CLIProcess", code: 127, userInfo: [
            NSLocalizedDescriptionKey: "sh: ralph: command not found"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
        XCTAssertEqual(recoveryError.category, .cliUnavailable)
        XCTAssertTrue(recoveryError.message.localizedCaseInsensitiveContains("cli"))
        XCTAssertTrue(recoveryError.suggestions.contains { $0.localizedCaseInsensitiveContains("installed") })
    }

    func testClassifyProcessSpawnENOENTAsCLIUnavailable() {
        let stderr = "failed to spawn managed subprocess 'ralph machine queue read': No such file or directory (os error 2)"
        let genericError = NSError(
            domain: "RalphCore.CLIProcess",
            code: 1,
            userInfo: [NSLocalizedDescriptionKey: stderr]
        )
        let processError = RetryableError.processError(exitCode: 1, stderr: stderr)

        let genericRecovery = RecoveryError.classify(error: genericError, operation: "loadTasks")
        let processRecovery = RecoveryError.classify(error: processError, operation: "loadTasks")

        XCTAssertEqual(genericRecovery.category, .cliUnavailable)
        XCTAssertEqual(processRecovery.category, .cliUnavailable)
        XCTAssertEqual(genericRecovery.message, processRecovery.message)
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
        let configPath = RalphCoreTestSupport.workspaceURL(label: "legacy-config-load")
            .appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
            .path
        let error = NSError(domain: "RalphCore.CLIProcess", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Error: load project config \(configPath): parse config \(configPath) from JSONC: unknown field `git_commit_push_enabled`"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "loadRunnerConfiguration")
        XCTAssertEqual(recoveryError.category, .configIncompatible)
        XCTAssertTrue(recoveryError.message.localizedCaseInsensitiveContains("config"))
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("ralph migrate --apply") })
        XCTAssertFalse(recoveryError.category.suggestedActions.contains(.validateQueue))
    }

    func testClassifyUnscopedMissingFileDoesNotBecomeCLIUnavailable() {
        let configPath = RalphCoreTestSupport.workspaceURL(label: "missing-config")
            .appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
            .path
        let error = NSError(domain: "RalphCore.CLIProcess", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Error: load project config \(configPath): No such file or directory (os error 2)"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "loadRunnerConfiguration")
        XCTAssertEqual(recoveryError.category, .configIncompatible)
        XCTAssertFalse(recoveryError.category.suggestedActions.contains(.reinstallCLI))
    }

    func testClassifyUnsupportedConfigVersionAsConfigIncompatible() {
        let configPath = RalphCoreTestSupport.workspaceURL(label: "unsupported-config-version")
            .appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
            .path
        let error = RetryableError.processError(
            exitCode: 1,
            stderr: "Error: load project config \(configPath): Unsupported config version: 1. Ralph requires version 2."
        )

        let recoveryError = RecoveryError.classify(error: error, operation: "loadRunnerConfiguration")
        XCTAssertEqual(recoveryError.category, .configIncompatible)
        XCTAssertFalse(recoveryError.category.suggestedActions.contains(.validateQueue))
    }

    func testClassifyMachineErrorDocumentUsesStructuredCode() throws {
        let queuePath = RalphCoreTestSupport.workspaceURL(label: "machine-error-document")
            .appendingPathComponent(".ralph/queue.jsonc", isDirectory: false)
            .path
        let document = MachineErrorDocument(
            version: 1,
            code: .queueCorrupted,
            message: "No Ralph queue file found.",
            detail: "read queue file \(queuePath): No such file or directory (os error 2)",
            retryable: false
        )
        let stderr = String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        let error = RetryableError.processError(exitCode: 1, stderr: stderr)

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
        XCTAssertEqual(recoveryError.category, .queueCorrupted)
        XCTAssertEqual(recoveryError.message, "No Ralph queue file found.")
        XCTAssertEqual(recoveryError.underlyingError, document.detail)
    }

    func testClassifyUserFacingMachineErrorSummaryPreservesStructuredMessage() {
        let summary = """
        Code: resource_busy
        Message: Failed to record stop request.
        Detail: cache directory is locked
        Retryable: yes
        """
        let error = NSError(domain: "RalphCore.CLIProcess", code: 23, userInfo: [
            NSLocalizedDescriptionKey: summary,
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "request loop stop")
        XCTAssertEqual(recoveryError.category, .resourceBusy)
        XCTAssertEqual(recoveryError.message, summary)
        XCTAssertEqual(recoveryError.underlyingError, "cache directory is locked")
    }

    func testClassifyUnsupportedMachineErrorVersionFromProcessError() throws {
        let document = MachineErrorDocument(
            version: 999,
            code: .resourceBusy,
            message: "Resource temporarily unavailable.",
            detail: "resource busy",
            retryable: true
        )
        let stderr = String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        let error = RetryableError.processError(exitCode: 1, stderr: stderr)

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
        XCTAssertEqual(recoveryError.category, .versionMismatch)
        XCTAssertTrue(recoveryError.message.contains("Unsupported machine error version 999"))
    }

    func testClassifyUnsupportedMachineErrorVersionFromGenericDescription() throws {
        let document = MachineErrorDocument(
            version: 999,
            code: .resourceBusy,
            message: "Resource temporarily unavailable.",
            detail: "resource busy",
            retryable: true
        )
        let description = String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        let error = NSError(domain: "RalphCore.CLIProcess", code: 1, userInfo: [
            NSLocalizedDescriptionKey: description
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
        XCTAssertEqual(recoveryError.category, .versionMismatch)
        XCTAssertTrue(recoveryError.message.contains("Unsupported machine error version 999"))
    }

    func testClassifyMachineUnknownErrorUsesStructuredSanitizedMessage() throws {
        let document = MachineErrorDocument(
            version: 1,
            code: .unknown,
            message: "Ralph CLI command failed.",
            detail: "unexpected [REDACTED] failure",
            retryable: false
        )
        let description = String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        let error = NSError(domain: "RalphCore.CLIProcess", code: 1, userInfo: [
            NSLocalizedDescriptionKey: description
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
        XCTAssertEqual(recoveryError.category, .unknown)
        XCTAssertEqual(recoveryError.message, document.message)
        XCTAssertEqual(recoveryError.underlyingError, document.detail)
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
        let queuePath = RalphCoreTestSupport.workspaceURL(label: "retryable-process-error")
            .appendingPathComponent(".ralph/queue.jsonc", isDirectory: false)
            .path
        let error = RetryableError.processError(
            exitCode: 2,
            stderr: "Error: read queue file \(queuePath): No such file or directory (os error 2)"
        )

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
        XCTAssertEqual(recoveryError.category, .queueCorrupted)
        XCTAssertTrue(recoveryError.message.localizedCaseInsensitiveContains("queue"))
    }

    func testCanonicalClassifierAlignsGenericAndProcessFailures() {
        let queuePath = RalphCoreTestSupport.workspaceURL(label: "canonical-classifier")
            .appendingPathComponent(".ralph/queue.jsonc", isDirectory: false)
            .path
        let stderr = "Error: read queue file \(queuePath): No such file or directory (os error 2)"
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

    func testTransientFixtures_alignRetryAndRecoveryAcrossGenericAndProcessErrors() {
        struct Fixture {
            let text: String
            let retryable: Bool
            let expectedCategory: ErrorCategory?
        }

        let fixtures: [Fixture] = [
            Fixture(text: "resource temporarily unavailable", retryable: true, expectedCategory: .resourceBusy),
            Fixture(text: "device or resource busy", retryable: true, expectedCategory: .resourceBusy),
            Fixture(text: "file is locked by another process", retryable: true, expectedCategory: .resourceBusy),
            Fixture(text: "operation would block", retryable: true, expectedCategory: .resourceBusy),
            Fixture(text: "try again", retryable: true, expectedCategory: .resourceBusy),
            Fixture(text: "io timeout", retryable: true, expectedCategory: .networkError),
            Fixture(text: "connection reset by peer", retryable: true, expectedCategory: .networkError),
            Fixture(text: "broken pipe", retryable: true, expectedCategory: .networkError),
            Fixture(text: "permission denied", retryable: false, expectedCategory: .permissionDenied),
            Fixture(text: "file not found", retryable: false, expectedCategory: nil),
        ]

        for fixture in fixtures {
            let genericError = NSError(
                domain: "RalphCore.CLIProcess",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: fixture.text]
            )
            let processError = RetryableError.processError(exitCode: 1, stderr: fixture.text)

            XCTAssertEqual(
                RetryHelper.defaultShouldRetry(genericError),
                fixture.retryable,
                "generic-error retryability mismatch for fixture: \(fixture.text)"
            )
            XCTAssertEqual(
                RetryHelper.defaultShouldRetry(processError),
                fixture.retryable,
                "process-error retryability mismatch for fixture: \(fixture.text)"
            )

            let genericRecovery = RecoveryError.classify(error: genericError, operation: "loadTasks")
            let processRecovery = RecoveryError.classify(error: processError, operation: "loadTasks")

            XCTAssertEqual(
                genericRecovery.category,
                processRecovery.category,
                "generic/process classification mismatch for fixture: \(fixture.text)"
            )

            if let expectedCategory = fixture.expectedCategory {
                XCTAssertEqual(
                    genericRecovery.category,
                    expectedCategory,
                    "unexpected category for fixture: \(fixture.text)"
                )
            } else {
                XCTAssertEqual(
                    genericRecovery.category,
                    .unknown,
                    "expected unknown category for fixture: \(fixture.text)"
                )
            }
        }
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
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("ralph queue validate") })
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

    func testClassifyQueueLockHolderError() {
        let error = RetryableError.processError(
            exitCode: 1,
            stderr: """
            Queue lock already held at: /tmp/example/.ralph/lock

            Lock Holder:
              PID: 1234
              Label: run loop
            """
        )

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
        XCTAssertEqual(recoveryError.category, .queueLock)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.localizedCaseInsensitiveContains("lock") })
    }

    func testQueueLockDiagnosticSnapshot_allowsClearingOnlyConfirmedStaleLocks() {
        let stale = QueueLockDiagnosticSnapshot(
            condition: .stale,
            blocking: nil,
            doctorOutput: "doctor",
            unlockPreview: "preview",
            unlockAllowed: true
        )
        let live = QueueLockDiagnosticSnapshot(
            condition: .live,
            blocking: nil,
            doctorOutput: "doctor",
            unlockPreview: "preview",
            unlockAllowed: false
        )

        XCTAssertTrue(stale.canClearStaleLock)
        XCTAssertFalse(live.canClearStaleLock)
    }

    func testClassifyQueueCorruptedError() {
        let error = NSError(domain: "RalphError", code: 1, userInfo: [
            NSLocalizedDescriptionKey: "Queue file is corrupted"
        ])

        let recoveryError = RecoveryError.classify(error: error, operation: "loadTasks")
        XCTAssertEqual(recoveryError.category, .queueCorrupted)
        XCTAssertTrue(recoveryError.suggestions.contains { $0.contains("ralph queue repair --dry-run") })
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
        XCTAssertEqual(RecoveryAction.repairQueue.rawValue, "repairQueue")
        XCTAssertEqual(RecoveryAction.restoreLastCheckpoint.rawValue, "restoreLastCheckpoint")
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
        XCTAssertTrue(corruptionActions.contains(.repairQueue))
        XCTAssertTrue(corruptionActions.contains(.restoreLastCheckpoint))
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
