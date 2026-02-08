/**
 RalphE2ESmokeTests

 Responsibilities:
 - Validate that the real `ralph` binary can be executed end-to-end from Swift.
 - Exercise a minimal workflow in an isolated temp directory:
   - version check
   - `init --force --non-interactive`
   - `queue list --format json`

 Does not handle:
 - Comprehensive CLI correctness. This is intentionally a smoke test.

 Invariants/assumptions callers must respect:
 - A Rust toolchain is available if the `ralph` binary needs to be built for the test.
 - Tests must not rely on network access.
 */

import Foundation
import XCTest

@testable import RalphCore

final class RalphE2ESmokeTests: XCTestCase {
    func test_e2e_smoke_version_init_and_queueList_json() async throws {
        let ralphURL = try Self.resolveRalphBinaryURL()
        let client = try RalphCLIClient(executableURL: ralphURL)

        // Some versions of Ralph expose `version` as a subcommand only.
        // Prefer `--version` when it exists but fall back to `version` to keep the GUI usable.
        let version1 = await Self.runAndCollect(
            client: client,
            arguments: ["--no-color", "--version"],
            currentDirectoryURL: nil
        )
        if version1.status.code != 0 {
            let version2 = await Self.runAndCollect(
                client: client,
                arguments: ["--no-color", "version"],
                currentDirectoryURL: nil
            )
            XCTAssertEqual(version2.status.code, 0)
            XCTAssertFalse(version2.stdout.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        } else {
            XCTAssertFalse(version1.stdout.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }

        let tempDir = try Self.makeTempDir(prefix: "ralph-e2e-")
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let initRun = await Self.runAndCollect(
            client: client,
            arguments: ["--no-color", "init", "--force", "--non-interactive"],
            currentDirectoryURL: tempDir
        )
        XCTAssertEqual(initRun.status.code, 0, "init stderr:\n\(initRun.stderr)")

        let listRun = await Self.runAndCollect(
            client: client,
            arguments: ["--no-color", "queue", "list", "--format", "json"],
            currentDirectoryURL: tempDir
        )
        XCTAssertEqual(listRun.status.code, 0, "queue list stderr:\n\(listRun.stderr)")

        let data = Data(listRun.stdout.utf8)
        let json = try JSONSerialization.jsonObject(with: data)
        XCTAssertTrue(json is [Any], "expected JSON array, got: \(type(of: json))")
    }

    private struct Collected {
        let status: RalphCLIExitStatus
        let stdout: String
        let stderr: String
    }

    private static func runAndCollect(
        client: RalphCLIClient,
        arguments: [String],
        currentDirectoryURL: URL?
    ) async -> Collected {
        do {
            let run = try client.start(arguments: arguments, currentDirectoryURL: currentDirectoryURL)

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
            return Collected(status: status, stdout: stdout, stderr: stderr)
        } catch {
            return Collected(
                status: RalphCLIExitStatus(code: -1, reason: .exit),
                stdout: "",
                stderr: "Failed to start process: \(error)"
            )
        }
    }

    private static func resolveRalphBinaryURL() throws -> URL {
        if let override = ProcessInfo.processInfo.environment["RALPH_BIN_PATH"], !override.isEmpty {
            return URL(fileURLWithPath: override)
        }

        let repoRoot = try findRepoRoot(startingAt: URL(fileURLWithPath: #filePath))
        let candidate = repoRoot.appendingPathComponent("target", isDirectory: true)
            .appendingPathComponent("debug", isDirectory: true)
            .appendingPathComponent("ralph", isDirectory: false)

        if FileManager.default.isExecutableFile(atPath: candidate.path) {
            return candidate
        }

        // Build if missing.
        try runBlocking(
            executableURL: URL(fileURLWithPath: "/usr/bin/env"),
            arguments: ["cargo", "build", "-p", "ralph"],
            currentDirectoryURL: repoRoot
        )

        guard FileManager.default.isExecutableFile(atPath: candidate.path) else {
            throw RalphCLIClientError.executableNotFound(candidate)
        }

        return candidate
    }

    private static func findRepoRoot(startingAt url: URL) throws -> URL {
        var current = url.deletingLastPathComponent()
        while true {
            let cargoToml = current.appendingPathComponent("Cargo.toml", isDirectory: false)
            if FileManager.default.fileExists(atPath: cargoToml.path) {
                return current
            }
            let parent = current.deletingLastPathComponent()
            if parent.path == current.path {
                throw NSError(domain: "RalphE2E", code: 1, userInfo: [NSLocalizedDescriptionKey: "Failed to locate repo root"])
            }
            current = parent
        }
    }

    private static func runBlocking(
        executableURL: URL,
        arguments: [String],
        currentDirectoryURL: URL
    ) throws {
        let process = Process()
        process.executableURL = executableURL
        process.arguments = arguments
        process.currentDirectoryURL = currentDirectoryURL

        let out = Pipe()
        let err = Pipe()
        process.standardOutput = out
        process.standardError = err

        try process.run()
        process.waitUntilExit()

        if process.terminationStatus != 0 {
            let stderr = String(decoding: err.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            throw NSError(
                domain: "RalphE2E",
                code: Int(process.terminationStatus),
                userInfo: [NSLocalizedDescriptionKey: "Command failed (\(arguments.joined(separator: " "))):\n\(stderr)"]
            )
        }
    }

    private static func makeTempDir(prefix: String) throws -> URL {
        let base = FileManager.default.temporaryDirectory
        let dir = base.appendingPathComponent("\(prefix)\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }
}
