/**
 RalphCoreTestSupport+Async

 Purpose:
 - Provide timeout-bounded async wait helpers for RalphCore tests.

 Responsibilities:
 - Provide timeout-bounded async wait helpers for RalphCore tests.
 - Centralize readiness checks so individual tests avoid ad hoc sleeps.

 Does not handle:
 - Filesystem fixture creation.
 - Production concurrency behavior.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Wait helpers always perform a final condition evaluation before failing.
 - Process-exit probes are best-effort and portable within the current test targets.
 */

import Foundation
import XCTest

#if canImport(Darwin)
import Darwin
#endif

extension RalphCoreTestSupport {
    static func waitUntil(
        timeout: Duration = .seconds(5),
        pollInterval: Duration = .milliseconds(20),
        condition: @escaping @Sendable () async -> Bool
    ) async -> Bool {
        if await condition() {
            return true
        }

        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: timeout)
        var now = clock.now

        while now < deadline {
            do {
                try await clock.sleep(for: pollInterval)
            } catch {
                return await condition()
            }

            if await condition() {
                return true
            }
            now = clock.now
        }

        return await condition()
    }

    static func waitForFile(
        _ url: URL,
        timeout: Duration = .seconds(2),
        pollInterval: Duration = .milliseconds(20)
    ) async -> Bool {
        await waitUntil(timeout: timeout, pollInterval: pollInterval) {
            FileManager.default.fileExists(atPath: url.path)
        }
    }

    static func waitForProcessExit(
        _ pid: pid_t,
        timeout: Duration = .seconds(3),
        pollInterval: Duration = .milliseconds(20)
    ) async -> Bool {
        await waitUntil(timeout: timeout, pollInterval: pollInterval) {
            processHasExited(pid)
        }
    }

    static func assertEventually(
        _ message: @autoclosure () -> String,
        timeout: Duration = .seconds(5),
        pollInterval: Duration = .milliseconds(20),
        file: StaticString = #filePath,
        line: UInt = #line,
        condition: @escaping @Sendable () async -> Bool
    ) async {
        let satisfied = await waitUntil(timeout: timeout, pollInterval: pollInterval, condition: condition)
        if !satisfied {
            XCTFail(message(), file: file, line: line)
        }
    }

    private static func processHasExited(_ pid: pid_t) -> Bool {
        #if canImport(Darwin)
        if kill(pid, 0) != 0 && errno == ESRCH {
            return true
        }
        #endif
        return false
    }
}
