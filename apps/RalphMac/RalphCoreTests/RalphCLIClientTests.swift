/**
 RalphCLIClientTests

 Responsibilities:
 - Validate `RalphCLIClient` subprocess behavior in isolation.
 - Cover success, non-zero exit, stdout/stderr streaming, working directory configuration, and cancellation.

 Does not handle:
 - Ralph-specific command semantics (covered by E2E smoke tests).
 - UI rendering or SwiftUI integration.

 Invariants/assumptions callers must respect:
 - Tests run on macOS with `/bin/sh` and `/bin/echo` available.
 */

public import Foundation
public import XCTest

@testable import RalphCore

final class RalphCLIClientTests: XCTestCase {
    func test_success_exitCodeZero_and_streamsStdout() async throws {
        let client = try RalphCLIClient(executableURL: URL(fileURLWithPath: "/bin/echo"))
        let run = try client.start(arguments: ["hello"])

        var stdout = Data()
        var stderr = Data()
        for await event in run.events {
            switch event.stream {
            case .stdout:
                stdout.append(event.data)
            case .stderr:
                stderr.append(event.data)
            }
        }

        let status = await run.waitUntilExit()
        XCTAssertEqual(status.code, 0)
        XCTAssertEqual(status.reason, .exit)
        XCTAssertEqual(String(decoding: stdout, as: UTF8.self), "hello\n")
        XCTAssertTrue(stderr.isEmpty)
    }

    func test_failure_exitCodeNonZero() async throws {
        let client = try RalphCLIClient(executableURL: URL(fileURLWithPath: "/bin/sh"))
        let run = try client.start(arguments: ["-c", "exit 42"])

        for await _ in run.events {
            // Drain.
        }

        let status = await run.waitUntilExit()
        XCTAssertEqual(status.code, 42)
        XCTAssertEqual(status.reason, .exit)
    }

    func test_runAndCollect_collectsStdoutStderr_and_exitStatus() async throws {
        let client = try RalphCLIClient(executableURL: URL(fileURLWithPath: "/bin/sh"))
        let collected = try await client.runAndCollect(arguments: ["-c", "echo out1; echo err1 1>&2; exit 7"])

        XCTAssertEqual(collected.status.reason, .exit)
        XCTAssertEqual(collected.status.code, 7)
        XCTAssertTrue(collected.stdout.contains("out1"))
        XCTAssertTrue(collected.stderr.contains("err1"))
    }

    func test_streaming_stdout_and_stderr() async throws {
        let client = try RalphCLIClient(executableURL: URL(fileURLWithPath: "/bin/sh"))
        let script = "echo out1; echo err1 1>&2; echo out2; echo err2 1>&2"
        let run = try client.start(arguments: ["-c", script])

        var stdout = ""
        var stderr = ""
        for await event in run.events {
            switch event.stream {
            case .stdout:
                stdout.append(event.text)
            case .stderr:
                stderr.append(event.text)
            }
        }

        let status = await run.waitUntilExit()
        XCTAssertEqual(status.code, 0)
        XCTAssertTrue(stdout.contains("out1"))
        XCTAssertTrue(stdout.contains("out2"))
        XCTAssertTrue(stderr.contains("err1"))
        XCTAssertTrue(stderr.contains("err2"))
    }

    func test_currentDirectoryURL_used() async throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-cli-client-cwd-")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let client = try RalphCLIClient(executableURL: URL(fileURLWithPath: "/bin/sh"))
        let run = try client.start(
            arguments: ["-c", "pwd"],
            currentDirectoryURL: tempDir
        )

        var stdout = ""
        for await event in run.events {
            if event.stream == .stdout {
                stdout.append(event.text)
            }
        }

        _ = await run.waitUntilExit()

        let printed = stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        let printedURL = URL(fileURLWithPath: printed).resolvingSymlinksInPath()
        XCTAssertEqual(printedURL.path, tempDir.resolvingSymlinksInPath().path)
    }

    func test_cancellation_terminatesProcess() async throws {
        let client = try RalphCLIClient(executableURL: URL(fileURLWithPath: "/bin/sleep"))
        let run = try client.start(arguments: ["60"])

        // Small delay to ensure process has actually started before cancel
        try await Task.sleep(nanoseconds: 100_000_000) // 100ms

        run.cancel()
        for await _ in run.events {
            // Drain until process exits.
        }

        let status = await run.waitUntilExit()
        XCTAssertTrue(status.reason == .uncaughtSignal || status.reason == .exit)
        XCTAssertNotEqual(status.code, 0)
    }

    private static func makeTempDir(prefix: String) throws -> URL {
        let base = FileManager.default.temporaryDirectory
        let dir = base.appendingPathComponent("\(prefix)\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }
}
