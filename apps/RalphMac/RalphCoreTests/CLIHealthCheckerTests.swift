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

#if canImport(Darwin)
import Darwin
#endif

final class CLIHealthCheckerTests: XCTestCase {
    private static func makeTempDir(prefix: String) throws -> URL {
        let tempRoot = FileManager.default.temporaryDirectory
        let directory = tempRoot.appendingPathComponent("\(prefix)-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }

    private static func waitForProcessExit(_ pid: pid_t, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            #if canImport(Darwin)
            if kill(pid, 0) != 0 && errno == ESRCH {
                return true
            }
            #endif

            Thread.sleep(forTimeInterval: 0.05)
        }
        return false
    }

    private static func waitForFile(_ url: URL, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if FileManager.default.fileExists(atPath: url.path) {
                return true
            }

            Thread.sleep(forTimeInterval: 0.05)
        }
        return false
    }

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
        XCTAssertEqual(CLIHealthStatus.UnavailabilityReason.cliNotFound.errorCategory, .cliUnavailable)
        XCTAssertEqual(CLIHealthStatus.UnavailabilityReason.permissionDenied.errorCategory, .permissionDenied)
        XCTAssertEqual(CLIHealthStatus.UnavailabilityReason.timeout.errorCategory, .networkError)
    }

    func testIsCLIUnavailableError() {
        let notFoundError = RalphCLIClientError.executableNotFound(URL(fileURLWithPath: "/nonexistent"))
        XCTAssertTrue(CLIHealthChecker.isCLIUnavailableError(notFoundError))

        let notExecError = RalphCLIClientError.executableNotExecutable(URL(fileURLWithPath: "/tmp"))
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

        XCTAssertEqual(status.availability, CLIHealthStatus.Availability.available)
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

        XCTAssertEqual(status.availability, CLIHealthStatus.Availability.available)
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

        XCTAssertEqual(
            status.availability,
            CLIHealthStatus.Availability.unavailable(reason: .cliNotFound)
        )
    }

    func testCheckHealth_timeoutTerminatesUnderlyingProcess() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-health-timeout")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let pidFileURL = tempDir.appendingPathComponent("health.pid", isDirectory: false)
        let scriptURL = tempDir.appendingPathComponent("mock-ralph", isDirectory: false)
        let script = """
        #!/bin/sh
        if [ "$1" = "--version" ]; then
          echo $$ > "\(pidFileURL.path)"
          trap '' TERM INT
          sleep 30
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
        let healthTask = Task {
            await checker.checkHealth(
                workspaceID: UUID(),
                workspaceURL: tempDir,
                timeout: 3,
                executableURL: scriptURL
            )
        }

        XCTAssertTrue(
            Self.waitForFile(pidFileURL, timeout: 2),
            "Health-check timeout fixture should record its process identifier before the deadline expires"
        )

        let status = await healthTask.value

        XCTAssertEqual(
            status.availability,
            CLIHealthStatus.Availability.unavailable(reason: .timeout)
        )
        let pidText = try XCTUnwrap(String(contentsOf: pidFileURL, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines))
        let pid = pid_t(try XCTUnwrap(Int32(pidText)))
        XCTAssertTrue(
            Self.waitForProcessExit(pid, timeout: 3),
            "Health-check timeout should terminate the launched process"
        )
    }
}
