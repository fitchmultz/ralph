/**
 ErrorRecoveryTests

 Responsibilities:
 - Provide shared imports and low-level test helpers for recovery-oriented RalphCore tests.
 - Centralize temporary-directory and process-exit helpers used by split recovery suites.

 Does not handle:
 - Defining the actual recovery, CLI health, timeout, or workspace caching assertions.

 Invariants/assumptions callers must respect:
 - These helpers are test-only and may assume temporary filesystem access.
 - Callers are responsible for cleaning up created directories.
 */

import XCTest
@testable import RalphCore

#if canImport(Darwin)
import Darwin
#endif

enum ErrorRecoveryTestSupport {
    static func makeTempDir(prefix: String) throws -> URL {
        let tempRoot = FileManager.default.temporaryDirectory
        let directory = tempRoot.appendingPathComponent("\(prefix)-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }

    static func waitForProcessExit(_ pid: pid_t, timeout: TimeInterval) -> Bool {
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
}
