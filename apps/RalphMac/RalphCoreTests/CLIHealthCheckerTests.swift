/**
 CLIHealthCheckerTests

 Responsibilities:
 - Validate CLI health status classification and executable probing behavior.
 - Cover timeout cleanup and fallback version probing behavior.

 Does not handle:
 - General recovery category formatting or workspace offline banners.

 Invariants/assumptions callers must respect:
 - Mock executables behave like small shell scripts and must be marked executable.
 */

import Foundation
import XCTest
@testable import RalphCore

final class CLIHealthCheckerTests: RalphCoreTestCase {
    func testHealthStatusAvailable() {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "cli-health-available")
        let status = CLIHealthStatus(
            availability: .available,
            lastChecked: Date(),
            workspaceURL: workspaceURL
        )
        XCTAssertTrue(status.isAvailable)
    }

    func testHealthStatusUnavailableCLI() {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "cli-health-unavailable")
        let status = CLIHealthStatus(
            availability: .unavailable(reason: .cliNotFound),
            lastChecked: Date(),
            workspaceURL: workspaceURL
        )
        XCTAssertFalse(status.isAvailable)
    }

    func testHealthStatusUnknown() {
        let workspaceURL = RalphCoreTestSupport.workspaceURL(label: "cli-health-unknown")
        let status = CLIHealthStatus(
            availability: .unknown,
            lastChecked: Date(),
            workspaceURL: workspaceURL
        )
        XCTAssertFalse(status.isAvailable)
    }

    func testUnavailabilityReasonErrorCategory() {
        XCTAssertEqual(CLIHealthStatus.UnavailabilityReason.cliNotFound.errorCategory, .cliUnavailable)
        XCTAssertEqual(CLIHealthStatus.UnavailabilityReason.permissionDenied.errorCategory, .permissionDenied)
        XCTAssertEqual(CLIHealthStatus.UnavailabilityReason.timeout.errorCategory, .networkError)
    }

    func testIsCLIUnavailableError() {
        let notFoundError = RalphCLIClientError.executableNotFound(URL(fileURLWithPath: "/nonexistent"))
        XCTAssertTrue(CLIHealthChecker.isCLIUnavailableError(notFoundError))

        let notExecError = RalphCLIClientError.executableNotExecutable(
            RalphCoreTestSupport.workspaceURL(label: "cli-health-not-executable")
        )
        XCTAssertTrue(CLIHealthChecker.isCLIUnavailableError(notExecError))

        let genericError = NSError(domain: "Test", code: 1)
        XCTAssertFalse(CLIHealthChecker.isCLIUnavailableError(genericError))
    }

    func testDefaultTimeoutValue() {
        XCTAssertEqual(CLIHealthChecker.defaultTimeout, 30)
    }

    func testCheckHealth_usesProvidedExecutableOverride() async throws {
        let tempDir = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "ralph-health-override")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let script = """
        #!/bin/sh
        if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "system" ] && [ "$4" = "info" ]; then
          echo '{"version":1,"cli_version":"9.9.9"}'
          exit 0
        fi
        exit 1
        """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: tempDir, body: script)

        let checker = CLIHealthChecker()
        let status = await checker.checkHealth(
            workspaceID: UUID(),
            workspaceURL: tempDir,
            timeout: 2,
            executableURL: scriptURL
        )

        XCTAssertEqual(status.availability, CLIHealthStatus.Availability.available)
    }

    func testCheckHealth_fallsBackToVersionSubcommandWhenDashVersionUnsupported() async throws {
        let tempDir = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "ralph-health-fallback")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let script = """
        #!/bin/sh
        if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "system" ] && [ "$4" = "info" ]; then
          echo '{"version":1,"cli_version":"9.9.9"}'
          exit 0
        fi
        exit 1
        """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: tempDir, body: script)

        let checker = CLIHealthChecker()
        let status = await checker.checkHealth(
            workspaceID: UUID(),
            workspaceURL: tempDir,
            timeout: 2,
            executableURL: scriptURL
        )

        XCTAssertEqual(status.availability, CLIHealthStatus.Availability.available)
    }

    func testCheckHealth_invalidProvidedExecutableReportsCliNotFound() async throws {
        let tempDir = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "ralph-health-missing")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let checker = CLIHealthChecker()
        let status = await checker.checkHealth(
            workspaceID: UUID(),
            workspaceURL: tempDir,
            timeout: 2,
            executableURL: URL(fileURLWithPath: "/definitely/not/a/real/ralph-binary")
        )

        XCTAssertEqual(
            status.availability,
            CLIHealthStatus.Availability.unavailable(reason: .cliNotFound)
        )
    }

    func testCheckHealth_timeoutTerminatesUnderlyingProcess() async throws {
        let tempDir = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "ralph-health-timeout")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let pidFileURL = tempDir.appendingPathComponent("health.pid", isDirectory: false)
        let script = """
        #!/bin/sh
        echo $$ > "\(pidFileURL.path)"
        if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "system" ] && [ "$4" = "info" ]; then
          trap '' TERM INT
          sleep 30
          exit 0
        fi
        exit 1
        """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: tempDir, body: script)

        let checker = CLIHealthChecker()
        let healthTask = Task {
            await checker.checkHealth(
                workspaceID: UUID(),
                workspaceURL: tempDir,
                timeout: 3,
                executableURL: scriptURL
            )
        }

        let recordedPID = await RalphCoreTestSupport.waitForFile(pidFileURL, timeout: .seconds(2))
        XCTAssertTrue(
            recordedPID,
            "Health-check timeout fixture should record its process identifier before the deadline expires"
        )

        let status = await healthTask.value

        XCTAssertEqual(
            status.availability,
            CLIHealthStatus.Availability.unavailable(reason: .timeout)
        )
        let pidText = try XCTUnwrap(String(contentsOf: pidFileURL, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines))
        let pid = pid_t(try XCTUnwrap(Int32(pidText)))
        let terminated = await RalphCoreTestSupport.waitForProcessExit(pid, timeout: .seconds(3))
        XCTAssertTrue(terminated, "Health-check timeout should terminate the launched process")
    }

    func testCheckHealth_taskCancellationTerminatesUnderlyingProcess() async throws {
        let tempDir = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "ralph-health-cancel")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let logURL = tempDir.appendingPathComponent("health-cancel.log", isDirectory: false)
        let script = """
        #!/bin/sh
        if [ "$1" = "--no-color" ] && [ "$2" = "machine" ] && [ "$3" = "system" ] && [ "$4" = "info" ]; then
          trap 'printf "canceled\\n" >> "\(logURL.path)"; exit 130' INT TERM
          printf 'started\n' >> "\(logURL.path)"
          sleep 30
          printf 'finished\n' >> "\(logURL.path)"
          echo '{"version":1,"cli_version":"9.9.9"}'
          exit 0
        fi
        exit 1
        """
        let scriptURL = try RalphMockCLITestSupport.makeExecutableScript(in: tempDir, body: script)

        let checker = CLIHealthChecker()
        let task = Task {
            await checker.checkHealth(
                workspaceID: UUID(),
                workspaceURL: tempDir,
                timeout: 10,
                executableURL: scriptURL
            )
        }

        let started = await RalphCoreTestSupport.waitUntil(timeout: .seconds(2)) {
            (try? String(contentsOf: logURL, encoding: .utf8).contains("started")) == true
        }
        XCTAssertTrue(started)

        task.cancel()
        let status = await task.value

        XCTAssertEqual(status.availability, .unknown)

        let canceled = await RalphCoreTestSupport.waitUntil(timeout: .seconds(3)) {
            (try? String(contentsOf: logURL, encoding: .utf8).contains("canceled")) == true
        }
        XCTAssertTrue(canceled)

        let log = try String(contentsOf: logURL, encoding: .utf8)
        XCTAssertFalse(log.contains("finished"))
    }
}
