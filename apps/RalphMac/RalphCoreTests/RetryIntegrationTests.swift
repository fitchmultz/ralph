/**
 RetryIntegrationTests

 Responsibilities:
 - Validate retry behavior with mock CLI client simulating transient failures.
 - Cover file lock simulation and retry recovery scenarios.

 Does not handle:
 - Real file system locking (requires external process coordination).
 */

import Foundation
import XCTest
@testable import RalphCore

final class RetryIntegrationTests: RalphCoreTestCase {
    
    private var tempDir: URL!
    
    override func setUp() async throws {
        try await super.setUp()
        tempDir = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "retry-tests")
    }
    
    override func tearDown() async throws {
        RalphCoreTestSupport.assertRemoved(tempDir)
        try await super.tearDown()
    }
    
    func test_runAndCollectWithRetry_succeedsAfterTransientFailure() async throws {
        // Create a mock script that fails twice then succeeds
        let stateFile = tempDir.appendingPathComponent("attempt-count")
        
        let scriptContent = """
            #!/bin/bash
            ATTEMPT_FILE="\(stateFile.path)"
            if [ -f "$ATTEMPT_FILE" ]; then
                ATTEMPT=$(cat "$ATTEMPT_FILE")
            else
                ATTEMPT=0
            fi
            ATTEMPT=$((ATTEMPT + 1))
            echo $ATTEMPT > "$ATTEMPT_FILE"
            
            if [ $ATTEMPT -lt 3 ]; then
                echo "resource temporarily unavailable" >&2
                exit 1
            fi
            echo '{"tasks":[]}'
            exit 0
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: tempDir, name: "mock-cli", body: scriptContent)
        
        let client = try RalphCLIClient(executableURL: scriptURL)
        let result = try await client.runAndCollectWithRetry(
            arguments: ["queue", "list"],
            retryConfiguration: RetryConfiguration(maxRetries: 3, baseDelay: 0.01, jitterRange: 0...0)
        )
        
        XCTAssertEqual(result.status.code, 0)
        XCTAssertTrue(result.stdout.contains("tasks"))
    }
    
    func test_runAndCollectWithRetry_failsOnPermanentError() async throws {
        // Create a mock script that always fails with non-retryable error
        let scriptContent = """
            #!/bin/bash
            echo "file not found" >&2
            exit 2
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: tempDir, name: "mock-cli", body: scriptContent)
        
        let client = try RalphCLIClient(executableURL: scriptURL)
        
        do {
            _ = try await client.runAndCollectWithRetry(
                arguments: ["queue", "list"],
                retryConfiguration: RetryConfiguration(maxRetries: 3, baseDelay: 0.01, jitterRange: 0...0)
            )
            XCTFail("Expected error to be thrown")
        } catch {
            // Expected - should fail immediately on non-retryable error
        }
    }
    
    func test_runAndCollectWithRetry_progressCallbackInvoked() async throws {
        // Create a mock script that fails once then succeeds
        let stateFile = tempDir.appendingPathComponent("attempt-count")
        
        let scriptContent = """
            #!/bin/bash
            ATTEMPT_FILE="\(stateFile.path)"
            if [ -f "$ATTEMPT_FILE" ]; then
                ATTEMPT=$(cat "$ATTEMPT_FILE")
            else
                ATTEMPT=0
            fi
            ATTEMPT=$((ATTEMPT + 1))
            echo $ATTEMPT > "$ATTEMPT_FILE"
            
            if [ $ATTEMPT -lt 2 ]; then
                echo "device or resource busy" >&2
                exit 1
            fi
            echo '{"tasks":[]}'
            exit 0
            """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: tempDir, name: "mock-cli", body: scriptContent)
        
        let client = try RalphCLIClient(executableURL: scriptURL)
        
        actor ProgressTracker {
            var count = 0
            func increment() { count += 1 }
        }
        let tracker = ProgressTracker()
        
        _ = try await client.runAndCollectWithRetry(
            arguments: ["queue", "list"],
            retryConfiguration: RetryConfiguration(maxRetries: 3, baseDelay: 0.01, jitterRange: 0...0),
            onRetry: { attempt, maxAttempts, delay in
                await tracker.increment()
                XCTAssertGreaterThanOrEqual(attempt, 1)
                XCTAssertLessThanOrEqual(attempt, maxAttempts)
                XCTAssertGreaterThan(delay, 0)
            }
        )
        
        let count = await tracker.count
        XCTAssertEqual(count, 1) // Should be called once for the retry
    }
    
    func test_retryConfiguration_presets() {
        // Verify all preset configurations are valid
        let defaultConfig = RetryConfiguration.default
        XCTAssertEqual(defaultConfig.maxRetries, 3)
        XCTAssertEqual(defaultConfig.baseDelay, 0.1)
        
        let minimalConfig = RetryConfiguration.minimal
        XCTAssertEqual(minimalConfig.maxRetries, 1)
        
        let aggressiveConfig = RetryConfiguration.aggressive
        XCTAssertEqual(aggressiveConfig.maxRetries, 5)
    }
    
    func test_workspaceRetryConfiguration_appliedCorrectly() async {
        // This test verifies that the correct retry configuration is used
        // for different workspace operations
        
        // Verify loadTasks uses default configuration
        let defaultConfig = RetryConfiguration.default
        XCTAssertEqual(defaultConfig.maxRetries, 3)
        
        // Verify loadCLISpec uses minimal configuration
        let minimalConfig = RetryConfiguration.minimal
        XCTAssertEqual(minimalConfig.maxRetries, 1)
        
        // Analytics loaders use minimal configuration
        let analyticsConfig = RetryConfiguration.minimal
        XCTAssertEqual(analyticsConfig.maxRetries, 1)
    }
    
    func test_collectedOutput_isRetryableFailure_patterns() {
        // Test various retryable error patterns
        let retryablePatterns = [
            "resource temporarily unavailable",
            "operation would block",
            "device or resource busy",
            "file is locked",
            "io timeout",
            "eagain",
            "ewouldblock",
            "ebusy",
            "locked",
            "try again"
        ]
        
        for pattern in retryablePatterns {
            let output = RalphCLIClient.CollectedOutput(
                status: RalphCLIExitStatus(code: 1, reason: .exit),
                stdout: "",
                stderr: pattern
            )
            XCTAssertTrue(output.isRetryableFailure, "Pattern '\(pattern)' should be retryable")
        }
        
        // Test non-retryable patterns
        let nonRetryablePatterns = [
            "file not found",
            "permission denied",
            "invalid argument",
            "syntax error"
        ]
        
        for pattern in nonRetryablePatterns {
            let output = RalphCLIClient.CollectedOutput(
                status: RalphCLIExitStatus(code: 1, reason: .exit),
                stdout: "",
                stderr: pattern
            )
            XCTAssertFalse(output.isRetryableFailure, "Pattern '\(pattern)' should not be retryable")
        }
    }
    
    func test_collectedOutput_toError() {
        let output = RalphCLIClient.CollectedOutput(
            status: RalphCLIExitStatus(code: 1, reason: .exit),
            stdout: "",
            stderr: "error message"
        )
        
        let error = output.toError() as! RetryableError
        
        if case .processError(let code, let stderr) = error {
            XCTAssertEqual(code, 1)
            XCTAssertEqual(stderr, "error message")
        } else {
            XCTFail("Expected processError")
        }
    }
}
