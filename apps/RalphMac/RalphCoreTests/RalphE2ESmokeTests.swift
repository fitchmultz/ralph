/**
 RalphE2ESmokeTests

 Responsibilities:
 - Validate that the real `ralph` binary can be executed end-to-end from Swift.
 - Exercise a minimal workflow in an isolated temp directory:
   - version check
   - `init --force --non-interactive`
   - `machine queue read`

 Does not handle:
 - Comprehensive CLI correctness. This is intentionally a smoke test.

 Invariants/assumptions callers must respect:
 - A deterministic `ralph` binary must be available via either:
   - `RALPH_BIN_PATH`, or
   - the bundled app binary at `RalphMac.app/Contents/MacOS/ralph`.
 - A Rust toolchain is available if `RALPH_E2E_ALLOW_CARGO_BUILD=1` enables fallback cargo builds.
 - Tests must not rely on network access.
 */

import Foundation
import XCTest

@testable import RalphCore

final class RalphE2ESmokeTests: XCTestCase {
    private static let allowCargoBuildEnvKey = "RALPH_E2E_ALLOW_CARGO_BUILD"
    private static let binaryPathEnvKey = "RALPH_BIN_PATH"
    private static let commandTimeoutSeconds: TimeInterval = 30

    func test_e2e_smoke_version_init_and_machineQueueRead_json() async throws {
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
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let initRun = await Self.runAndCollect(
            client: client,
            arguments: ["--no-color", "init", "--force", "--non-interactive"],
            currentDirectoryURL: tempDir
        )
        XCTAssertEqual(initRun.status.code, 0, "init stderr:\n\(initRun.stderr)")

        let listRun = await Self.runAndCollect(
            client: client,
            arguments: ["--no-color", "machine", "queue", "read"],
            currentDirectoryURL: tempDir
        )
        XCTAssertEqual(listRun.status.code, 0, "machine queue read stderr:\n\(listRun.stderr)")

        let data = Data(listRun.stdout.utf8)
        let json = try JSONSerialization.jsonObject(with: data)
        guard let document = json as? [String: Any] else {
            return XCTFail("expected JSON object, got: \(type(of: json))")
        }
        XCTAssertNotNil(document["version"])
        XCTAssertTrue(document["active"] is [String: Any], "expected active queue document")
    }

    func test_e2e_versionCompatibilityCheck() async throws {
        let ralphURL = try Self.resolveRalphBinaryURL()
        let client = try RalphCLIClient(executableURL: ralphURL)

        // Get actual CLI version - try `--version` first, fall back to `version` subcommand
        var versionOutput = await Self.runAndCollect(
            client: client,
            arguments: ["--no-color", "--version"],
            currentDirectoryURL: nil
        )
        if versionOutput.status.code != 0 {
            versionOutput = await Self.runAndCollect(
                client: client,
                arguments: ["--no-color", "version"],
                currentDirectoryURL: nil
            )
        }

        XCTAssertEqual(versionOutput.status.code, 0, "Version command failed")

        let versionString = versionOutput.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        XCTAssertFalse(versionString.isEmpty, "Version string is empty")

        // Validate version string is parseable using default supported range
        let validator = VersionValidator()
        let result = validator.validate(versionString)

        // The bundled CLI should be compatible with the app's supported range
        // If this fails, the VersionCompatibility constants need updating
        XCTAssertTrue(
            result.isCompatible,
            "Bundled CLI version '\(versionString)' is not compatible with supported range \(VersionCompatibility.minimumCLIVersion)-\(VersionCompatibility.maximumCLIVersion). \(result.errorMessage ?? "")"
        )
    }

    func test_resolveRalphBinaryURL_envOverride_success() throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-resolver-env-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        let binaryURL = tempDir.appendingPathComponent("ralph", isDirectory: false)
        try Self.writeExecutableScript(at: binaryURL)

        let resolved = try Self.resolveRalphBinaryURL(
            environment: [Self.binaryPathEnvKey: binaryURL.path],
            repoRoot: tempDir,
            bundledBinaryURL: nil,
            cargoBuilder: { _ in XCTFail("cargoBuilder should not run when RALPH_BIN_PATH is set") }
        )

        XCTAssertEqual(resolved.path, binaryURL.path)
    }

    func test_resolveRalphBinaryURL_missingEnv_fallbackDisabled_failsFast() throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-resolver-no-fallback-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        XCTAssertThrowsError(
            try Self.resolveRalphBinaryURL(
                environment: [:],
                repoRoot: tempDir,
                bundledBinaryURL: nil,
                cargoBuilder: { _ in XCTFail("cargoBuilder should not run when fallback is disabled") }
            )
        ) { error in
            let message = String(describing: error)
            XCTAssertTrue(message.contains(Self.binaryPathEnvKey))
            XCTAssertTrue(message.contains(Self.allowCargoBuildEnvKey))
        }
    }

    func test_resolveRalphBinaryURL_fallbackEnabled_failsFastWithoutCargoBuild() throws {
        let tempDir = try Self.makeTempDir(prefix: "ralph-resolver-fallback-")
        defer { RalphCoreTestSupport.assertRemoved(tempDir) }

        // With the env var set but no binary present, it should still fail fast
        // (no opportunistic cargo builds during tests)
        XCTAssertThrowsError(
            try Self.resolveRalphBinaryURL(
                environment: [Self.allowCargoBuildEnvKey: "1"],
                repoRoot: tempDir,
                bundledBinaryURL: nil
            )
        ) { error in
            let message = String(describing: error)
            XCTAssertTrue(message.contains(Self.binaryPathEnvKey))
            XCTAssertTrue(message.contains("make build") || message.contains("RALPH_BIN_PATH"))
        }
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
            return await runAndCollectWithTimeout(
                run: run,
                arguments: arguments,
                timeoutSeconds: commandTimeoutSeconds
            )
        } catch {
            return Collected(
                status: RalphCLIExitStatus(code: -1, reason: .exit),
                stdout: "",
                stderr: "Failed to start process: \(error)"
            )
        }
    }

    private static func runAndCollectWithTimeout(
        run: RalphCLIRun,
        arguments: [String],
        timeoutSeconds: TimeInterval
    ) async -> Collected {
        await withTaskGroup(of: Collected?.self) { group in
            // Start main collection task
            group.addTask {
                var stdout = ""
                var stderr = ""
                for await event in await run.events {
                    switch event.stream {
                    case .stdout:
                        stdout.append(event.text)
                    case .stderr:
                        stderr.append(event.text)
                    }
                }
                let status = await run.waitUntilExit()
                return Collected(status: status, stdout: stdout, stderr: stderr)
            }

            group.addTask {
                do {
                    try await ContinuousClock().sleep(for: .seconds(timeoutSeconds))
                } catch {
                    return nil
                }
                await run.cancel()
                let command = arguments.joined(separator: " ")
                return Collected(
                    status: RalphCLIExitStatus(code: -1, reason: .exit),
                    stdout: "",
                    stderr: "Timed out after \(Int(timeoutSeconds))s: \(command)"
                )
            }

            var result: Collected? = nil
            while let next = await group.next() {
                if let collected = next {
                    result = collected
                    group.cancelAll()
                    break
                }
            }
            
            return result ?? Collected(
                status: RalphCLIExitStatus(code: -1, reason: .exit),
                stdout: "",
                stderr: "Internal test error: timeout race produced no result."
            )
        }
    }

    private static func timeoutNanoseconds(from seconds: TimeInterval) -> UInt64 {
        let clampedSeconds = max(seconds, 0)
        let nanos = clampedSeconds * 1_000_000_000
        if nanos >= Double(UInt64.max) {
            return UInt64.max
        }
        return UInt64(nanos)
    }

    private static func resolveRalphBinaryURL() throws -> URL {
        try resolveRalphBinaryURL(environment: ProcessInfo.processInfo.environment)
    }

    private static func resolveRalphBinaryURL(
        environment: [String: String],
        repoRoot: URL? = nil,
        bundledBinaryURL: URL? = bundledRalphBinaryURL(),
        cargoBuilder: ((URL) throws -> Void)? = nil
    ) throws -> URL {
        if let override = environment[binaryPathEnvKey]?.trimmingCharacters(in: .whitespacesAndNewlines), !override.isEmpty {
            let overrideURL = URL(fileURLWithPath: override)
            guard FileManager.default.isExecutableFile(atPath: overrideURL.path) else {
                throw resolverError(
                    "Environment variable \(binaryPathEnvKey) points to a non-executable path: \(overrideURL.path). " +
                        "Set \(binaryPathEnvKey) to an executable `ralph` binary."
                )
            }
            return overrideURL
        }

        if let bundledBinaryURL, FileManager.default.isExecutableFile(atPath: bundledBinaryURL.path) {
            return bundledBinaryURL
        }

        guard environment[allowCargoBuildEnvKey] == "1" else {
            throw resolverError(
                "Missing \(binaryPathEnvKey). Set \(binaryPathEnvKey) to an executable `ralph` binary for deterministic tests, " +
                    "or ensure the bundled Ralph binary is present at \(bundledRalphBinaryPathDescription()). " +
                    "If you explicitly want fallback runtime cargo build, set \(allowCargoBuildEnvKey)=1."
            )
        }

        let root = try repoRoot ?? findRepoRoot(startingAt: URL(fileURLWithPath: #filePath))
        let candidate = root.appendingPathComponent("target", isDirectory: true)
            .appendingPathComponent("debug", isDirectory: true)
            .appendingPathComponent("ralph", isDirectory: false)

        // FAIL FAST - never build during tests
        throw resolverError(
            "Missing ralph binary at \(candidate.path). " +
            "Run 'make build' before tests, or set \(binaryPathEnvKey) to a valid binary."
        )
    }

    private static func resolverError(_ message: String) -> NSError {
        NSError(domain: "RalphE2E", code: 2, userInfo: [NSLocalizedDescriptionKey: message])
    }

    private static func bundledRalphBinaryURL() -> URL? {
        let bundleURL = Bundle(for: RalphE2ESmokeTests.self).bundleURL
        let productsDir = bundleURL.deletingLastPathComponent()
        return productsDir
            .appendingPathComponent("RalphMac.app", isDirectory: true)
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("MacOS", isDirectory: true)
            .appendingPathComponent("ralph", isDirectory: false)
    }

    private static func bundledRalphBinaryPathDescription() -> String {
        bundledRalphBinaryURL()?.path ?? "<derived-data>/Build/Products/Debug/RalphMac.app/Contents/MacOS/ralph"
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
        try RalphCoreTestSupport.makeTemporaryDirectory(prefix: prefix)
    }

    private static func writeExecutableScript(at url: URL) throws {
        try "#!/bin/sh\nexit 0\n".write(to: url, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o755))],
            ofItemAtPath: url.path
        )
    }
}
