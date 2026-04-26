/**
 RetryHelperTests

 Purpose:
 - Validate RetryHelper retry logic, backoff calculation, and error classification.

 Responsibilities:
 - Validate RetryHelper retry logic, backoff calculation, and error classification.
 - Cover success on first attempt, success after retries, and max retries exceeded.

 Does not handle:
 - Integration with actual CLI operations (see RetryIntegrationTests).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import Foundation
import XCTest
@testable import RalphCore

// Actor to safely track attempt counts from concurrent contexts
@globalActor
actor TestCounter {
    static let shared = TestCounter()
    private var count = 0
    
    func increment() -> Int {
        count += 1
        return count
    }
    
    func get() -> Int {
        return count
    }
    
    func reset() {
        count = 0
    }
}

final class RetryHelperTests: RalphCoreTestCase {
    
    // MARK: - Basic Retry Tests
    
    func test_successOnFirstAttempt_noRetries() async throws {
        let helper = RetryHelper(configuration: .minimal)
        
        let result = try await helper.execute {
            return "success"
        }
        
        XCTAssertEqual(result, "success")
    }
    
    func test_successAfterRetries() async throws {
        let config = RetryConfiguration(maxRetries: 3, baseDelay: 0.01, jitterRange: 0...0)
        let helper = RetryHelper(configuration: config)
        
        // Use an actor to safely track attempts
        actor AttemptTracker {
            var count = 0
            func increment() -> Int {
                count += 1
                return count
            }
        }
        let tracker = AttemptTracker()
        
        let result = try await helper.execute {
            let attempt = await tracker.increment()
            if attempt < 3 {
                throw RetryableError.resourceBusy
            }
            return "success"
        }
        
        XCTAssertEqual(result, "success")
        let finalCount = await tracker.count
        XCTAssertEqual(finalCount, 3)
    }
    
    func test_maxRetriesExceeded_throwsError() async {
        let config = RetryConfiguration(maxRetries: 2, baseDelay: 0.01, jitterRange: 0...0)
        let helper = RetryHelper(configuration: config)
        
        do {
            _ = try await helper.execute {
                throw RetryableError.fileLocked
            }
            XCTFail("Expected error to be thrown")
        } catch {
            // Expected
        }
    }
    
    func test_nonRetryableError_failsImmediately() async {
        let helper = RetryHelper(configuration: .default)
        
        struct NonRetryableError: Error {}
        
        do {
            _ = try await helper.execute(
                operation: {
                    throw NonRetryableError()
                },
                shouldRetry: { _ in false }
            )
            XCTFail("Expected error to be thrown")
        } catch {
            // Expected
        }
    }
    
    // MARK: - Progress Callback Tests
    
    func test_progressCallbackCalledOnRetry() async throws {
        let config = RetryConfiguration(maxRetries: 3, baseDelay: 0.01, jitterRange: 0...0)
        let helper = RetryHelper(configuration: config)
        
        actor ProgressTracker {
            var calls: [(attempt: Int, maxAttempts: Int)] = []
            func record(attempt: Int, maxAttempts: Int) {
                calls.append((attempt, maxAttempts))
            }
        }
        let tracker = ProgressTracker()
        
        actor AttemptTracker {
            var count = 0
            func increment() -> Int {
                count += 1
                return count
            }
        }
        let attemptTracker = AttemptTracker()
        
        _ = try await helper.execute(
            operation: {
                let attempt = await attemptTracker.increment()
                if attempt < 3 {
                    throw RetryableError.ioTimeout
                }
                return "success"
            },
            onProgress: { attempt, maxAttempts, _ in
                await tracker.record(attempt: attempt, maxAttempts: maxAttempts)
            }
        )
        
        let calls = await tracker.calls
        XCTAssertEqual(calls.count, 2) // Called for attempts 1 and 2
        XCTAssertEqual(calls[0].attempt, 1)
        XCTAssertEqual(calls[0].maxAttempts, 3)
        XCTAssertEqual(calls[1].attempt, 2)
    }
    
    // MARK: - Error Classification Tests
    
    func test_defaultShouldRetry_recognizesRetryableErrors() {
        XCTAssertTrue(RetryHelper.defaultShouldRetry(RetryableError.fileLocked))
        XCTAssertTrue(RetryHelper.defaultShouldRetry(RetryableError.resourceBusy))
        XCTAssertTrue(RetryHelper.defaultShouldRetry(RetryableError.ioTimeout))
        XCTAssertTrue(RetryHelper.defaultShouldRetry(RetryableError.resourceTemporarilyUnavailable))
    }
    
    func test_defaultShouldRetry_recognizesProcessErrorWithRetryableStderr() {
        let error = RetryableError.processError(
            exitCode: 1,
            stderr: "Resource temporarily unavailable"
        )
        XCTAssertTrue(RetryHelper.defaultShouldRetry(error))
        
        let lockedError = RetryableError.processError(
            exitCode: 1,
            stderr: "File is locked by another process"
        )
        XCTAssertTrue(RetryHelper.defaultShouldRetry(lockedError))
    }

    func test_defaultShouldRetry_usesMachineErrorRetryableFlag() throws {
        let document = MachineErrorDocument(
            version: 1,
            code: .resourceBusy,
            message: "Resource temporarily unavailable.",
            detail: "resource busy",
            retryable: true
        )
        let stderr = String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        let error = RetryableError.processError(exitCode: 1, stderr: stderr)
        XCTAssertTrue(RetryHelper.defaultShouldRetry(error))
    }

    func test_defaultShouldRetry_rejectsUnsupportedMachineErrorVersion() throws {
        let document = MachineErrorDocument(
            version: 999,
            code: .resourceBusy,
            message: "Resource temporarily unavailable.",
            detail: "resource busy",
            retryable: true
        )
        let stderr = String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        let error = RetryableError.processError(exitCode: 1, stderr: stderr)
        XCTAssertFalse(RetryHelper.defaultShouldRetry(error))
    }

    func test_machineErrorDocument_userFacingDescriptionIncludesStructuredFields() {
        let document = MachineErrorDocument(
            version: 1,
            code: .queueCorrupted,
            message: "Queue is invalid.",
            detail: "read queue file .ralph/queue.jsonc: missing terminal completed_at",
            retryable: false
        )

        XCTAssertEqual(
            document.userFacingDescription,
            """
            Code: queue_corrupted
            Message: Queue is invalid.
            Detail: read queue file .ralph/queue.jsonc: missing terminal completed_at
            Retryable: no
            """
        )
    }

    func test_retryableProcessError_localizedDescriptionIncludesExitCodeAndStderr() {
        let error = RetryableError.processError(exitCode: 7, stderr: "queue read failed")
        XCTAssertEqual(
            error.localizedDescription,
            "CLI command failed with exit code 7: queue read failed"
        )
    }

    func test_retryableProcessError_localizedDescriptionPrefersStructuredMachineError() throws {
        let document = MachineErrorDocument(
            version: 1,
            code: .queueCorrupted,
            message: "Queue is invalid.",
            detail: "read queue file .ralph/queue.jsonc: missing terminal completed_at",
            retryable: false
        )
        let error = RetryableError.processError(
            exitCode: 7,
            stderr: String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        )

        XCTAssertEqual(error.localizedDescription, document.userFacingDescription)
    }

    func test_retryableProcessError_localizedDescriptionSurfacesVersionMismatchForUnsupportedMachineError(
    ) throws {
        let document = MachineErrorDocument(
            version: 999,
            code: .resourceBusy,
            message: "Resource temporarily unavailable.",
            detail: "resource busy",
            retryable: true
        )
        let error = RetryableError.processError(
            exitCode: 7,
            stderr: String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        )

        XCTAssertTrue(error.localizedDescription.contains("Unsupported machine error version 999"))
    }
    
    func test_isRetryableFailure_detectsRetryablePatterns() {
        let lockedOutput = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: "file is locked"
        )
        XCTAssertTrue(lockedOutput.isRetryableFailure)
        
        let busyOutput = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: "resource busy"
        )
        XCTAssertTrue(busyOutput.isRetryableFailure)
        
        let permanentOutput = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: "file not found"
        )
        XCTAssertFalse(permanentOutput.isRetryableFailure)
        
        let successOutput = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 0, reason: .exit),
            stdout: "success",
            stderr: ""
        )
        XCTAssertFalse(successOutput.isRetryableFailure)
    }

    func test_isRetryableFailure_usesMachineErrorDocument() throws {
        let document = MachineErrorDocument(
            version: 1,
            code: .resourceBusy,
            message: "Resource temporarily unavailable.",
            detail: "resource busy",
            retryable: true
        )
        let output = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        )
        XCTAssertTrue(output.isRetryableFailure)
        XCTAssertEqual(try output.machineError(operation: "retry helper test"), document)
    }

    func test_isRetryableFailure_rejectsUnsupportedMachineErrorVersion() throws {
        let document = MachineErrorDocument(
            version: 999,
            code: .resourceBusy,
            message: "Resource temporarily unavailable.",
            detail: "resource busy",
            retryable: true
        )
        let output = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        )
        XCTAssertFalse(output.isRetryableFailure)
    }

    func test_failureMessage_prefersStructuredMachineErrorDocument() throws {
        let document = MachineErrorDocument(
            version: 1,
            code: .queueCorrupted,
            message: "Queue is invalid.",
            detail: "read queue file .ralph/queue.jsonc: missing terminal completed_at",
            retryable: false
        )
        let output = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        )

        XCTAssertEqual(
            output.failureMessage(fallback: "fallback message"),
            document.userFacingDescription
        )
    }

    func test_failureMessage_surfacesVersionMismatchForUnsupportedMachineErrorDocument() throws {
        let document = MachineErrorDocument(
            version: 999,
            code: .queueCorrupted,
            message: "Queue is invalid.",
            detail: "read queue file .ralph/queue.jsonc: missing terminal completed_at",
            retryable: true
        )
        let output = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: String(decoding: try JSONEncoder().encode(document), as: UTF8.self)
        )

        XCTAssertTrue(
            output.failureMessage(
                operation: "retry helper test failure message",
                fallback: "fallback message"
            ).contains("Unsupported machine error version 999")
        )
    }

    func test_failureMessage_fallsBackToTrimmedStderrThenDefaultMessage() {
        let stderrOutput = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: "  queue read failed  \n"
        )
        XCTAssertEqual(
            stderrOutput.failureMessage(fallback: "fallback message"),
            "queue read failed"
        )

        let emptyOutput = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: " \n "
        )
        XCTAssertEqual(
            emptyOutput.failureMessage(fallback: "fallback message"),
            "fallback message"
        )
    }
    
    // MARK: - Configuration Tests
    
    func test_defaultConfiguration() {
        let config = RetryConfiguration.default
        XCTAssertEqual(config.maxRetries, 3)
        XCTAssertEqual(config.baseDelay, 0.1)
        XCTAssertEqual(config.maxDelay, 1.6)
        XCTAssertEqual(config.jitterRange, 0.01...0.03)
    }
    
    func test_aggressiveConfiguration() {
        let config = RetryConfiguration.aggressive
        XCTAssertEqual(config.maxRetries, 5)
    }
    
    func test_minimalConfiguration() {
        let config = RetryConfiguration.minimal
        XCTAssertEqual(config.maxRetries, 1)
    }
    
    // MARK: - RetryResult Tests
    
    func test_executeWithResult_returnsSuccess() async {
        let helper = RetryHelper(configuration: .minimal)
        
        let result = await helper.executeWithResult {
            return "success"
        }
        
        switch result {
        case .success(let value):
            XCTAssertEqual(value, "success")
        case .failure:
            XCTFail("Expected success")
        }
    }
    
    func test_executeWithResult_returnsFailure() async {
        let config = RetryConfiguration(maxRetries: 2, baseDelay: 0.01, jitterRange: 0...0)
        let helper = RetryHelper(configuration: config)
        
        struct TestError: Error {}
        
        let result = await helper.executeWithResult {
            throw TestError()
        }
        
        switch result {
        case .success:
            XCTFail("Expected failure")
        case .failure(_, let attempts):
            XCTAssertEqual(attempts, 2)
        }
    }
    
    // MARK: - Delay Calculation Tests
    
    func test_delayCalculation_withBackoff() async throws {
        let config = RetryConfiguration(maxRetries: 4, baseDelay: 0.1, maxDelay: 1.0, jitterRange: 0...0)
        let helper = RetryHelper(configuration: config)
        
        actor DelayTracker {
            var delays: [TimeInterval] = []
            func record(_ delay: TimeInterval) {
                delays.append(delay)
            }
        }
        let tracker = DelayTracker()
        
        _ = try? await helper.execute(
            operation: {
                throw RetryableError.resourceBusy
            },
            onProgress: { _, _, delay in
                await tracker.record(delay)
            }
        )
        
        let delays = await tracker.delays
        // Should have 3 delays for 4 retries (no delay after last attempt)
        XCTAssertEqual(delays.count, 3)
        // First delay should be ~0.1s (base)
        XCTAssertEqual(delays[0], 0.1, accuracy: 0.01)
        // Second delay should be ~0.2s (base * 2)
        XCTAssertEqual(delays[1], 0.2, accuracy: 0.01)
        // Third delay should be ~0.4s (base * 4)
        XCTAssertEqual(delays[2], 0.4, accuracy: 0.01)
    }
    
    func test_delayCalculation_respectsMaxDelay() async throws {
        let config = RetryConfiguration(maxRetries: 5, baseDelay: 0.1, maxDelay: 0.3, jitterRange: 0...0)
        let helper = RetryHelper(configuration: config)
        
        actor DelayTracker {
            var delays: [TimeInterval] = []
            func record(_ delay: TimeInterval) {
                delays.append(delay)
            }
        }
        let tracker = DelayTracker()
        
        _ = try? await helper.execute(
            operation: {
                throw RetryableError.resourceBusy
            },
            onProgress: { _, _, delay in
                await tracker.record(delay)
            }
        )
        
        let delays = await tracker.delays
        // Delays should be capped at maxDelay
        for delay in delays {
            XCTAssertLessThanOrEqual(delay, config.maxDelay + 0.001) // Small tolerance
        }
    }
}
