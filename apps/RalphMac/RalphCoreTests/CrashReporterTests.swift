/**
 CrashReporterTests

 Purpose:
 - Keep CrashReporterTests behavior scoped to its owning RalphMac feature.

 Responsibilities:
 - Provide focused app, core, or test behavior for its owning feature.

 Scope:
 - Limited to this file's owning RalphMac feature boundary.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Keep behavior aligned with Ralph's machine-contract and queue semantics.
 */

/*
 Purpose:
 - Exercise crash-report storage, formatting, and operational-issue behavior in RalphCore tests.

 Responsibilities:
 - Verify crash report model formatting and identity expectations.
 - Verify crash-report persistence surfaces operational issues instead of silently failing.
 - Keep crash-report test fixture setup and teardown isolated through RalphCore test support.

 Scope:
 - CrashReporter and CrashReport test coverage only.

 Usage:
 - Runs as part of the RalphCore XCTest suite.

 Invariants/Assumptions:
 - Tests execute on the main actor because CrashReporter is shared UI-adjacent state.
 - Temporary report directories must be created through RalphCore test support and removed during teardown.
 */

import XCTest
@testable import RalphCore

@MainActor
final class CrashReporterTests: RalphCoreTestCase {
    private var reportsDirectory: URL!

    override func setUp() async throws {
        try await super.setUp()
        reportsDirectory = try RalphCoreTestSupport.makeTemporaryDirectory(prefix: "ralph-crash-tests")
        CrashReporter.shared.setStorageForTesting(makeStorage(directoryURL: reportsDirectory))
        CrashReporter.shared.clearAllReports()
        CrashReporter.shared.clearOperationalIssues()
    }

    override func tearDown() async throws {
        CrashReporter.shared.setStorageForTesting(makeStorage(directoryURL: reportsDirectory))
        CrashReporter.shared.clearAllReports()
        CrashReporter.shared.clearOperationalIssues()
        RalphCoreTestSupport.assertRemoved(reportsDirectory)
        try await super.tearDown()
    }

    private static func makeStorage(
        directoryURL: URL,
        listFiles: (@Sendable (URL) throws -> [URL])? = nil
    ) -> CrashReportStorage {
        CrashReportStorage(
            directoryURL: { directoryURL },
            createDirectory: { url in
                try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
            },
            listFiles: { url in
                if let listFiles {
                    return try listFiles(url)
                }
                return try FileManager.default.contentsOfDirectory(at: url, includingPropertiesForKeys: nil)
            },
            readData: { url in
                try Data(contentsOf: url)
            },
            writeData: { data, url in
                try data.write(to: url, options: .atomic)
            },
            removeItem: { url in
                try FileManager.default.removeItem(at: url)
            }
        )
    }

    private func makeStorage(
        directoryURL: URL,
        listFiles: (@Sendable (URL) throws -> [URL])? = nil
    ) -> CrashReportStorage {
        Self.makeStorage(directoryURL: directoryURL, listFiles: listFiles)
    }

    func testCrashReportCreation() {
        let report = CrashReport(
            id: UUID(),
            timestamp: Date(),
            exceptionName: "TestException",
            exceptionReason: "Test reason",
            stackTrace: ["frame1", "frame2"],
            appVersion: "1.0.0",
            osVersion: "macOS 14.0",
            deviceModel: "MacBookPro"
        )

        XCTAssertEqual(report.exceptionName, "TestException")
        XCTAssertEqual(report.exceptionReason, "Test reason")
        XCTAssertEqual(report.stackTrace.count, 2)
        XCTAssertEqual(report.appVersion, "1.0.0")
    }

    func testCrashReportFormatting() {
        let report = CrashReport(
            id: UUID(),
            timestamp: Date(),
            exceptionName: "TestException",
            exceptionReason: "Test reason",
            stackTrace: ["frame1"],
            appVersion: "1.0.0",
            osVersion: "macOS 14.0",
            deviceModel: "MacBookPro"
        )

        let formatted = report.formattedReport
        XCTAssertTrue(formatted.contains("Ralph Crash Report"))
        XCTAssertTrue(formatted.contains("TestException"))
        XCTAssertTrue(formatted.contains("frame1"))
        XCTAssertTrue(formatted.contains("1.0.0"))
    }

    func testGetAllReportsEmpty() {
        let reports = CrashReporter.shared.getAllReports()
        XCTAssertTrue(reports.isEmpty)
    }

    func testClearAllReports() {
        // After clearing, should be empty
        CrashReporter.shared.clearAllReports()
        let reports = CrashReporter.shared.getAllReports()
        XCTAssertTrue(reports.isEmpty)
    }

    func testExportEmptyReports() {
        let export = CrashReporter.shared.exportAllReports()
        XCTAssertEqual(export, "No crash reports found.")
    }

    func testCrashReportIDUnique() {
        let report1 = CrashReport(
            id: UUID(),
            timestamp: Date(),
            exceptionName: "Test1",
            exceptionReason: "Reason1",
            stackTrace: [],
            appVersion: "1.0.0",
            osVersion: "macOS 14.0",
            deviceModel: "MacBookPro"
        )

        let report2 = CrashReport(
            id: UUID(),
            timestamp: Date(),
            exceptionName: "Test2",
            exceptionReason: "Reason2",
            stackTrace: [],
            appVersion: "1.0.0",
            osVersion: "macOS 14.0",
            deviceModel: "MacBookPro"
        )

        XCTAssertNotEqual(report1.id, report2.id)
    }

    func testInstall_surfacesOperationalIssue_whenStorageInitializationFails() {
        enum StorageFailure: Error {
            case createDirectory
        }

        guard let reportsDirectory else {
            XCTFail("Missing reports directory test fixture")
            return
        }

        CrashReporter.shared.setStorageForTesting(
            CrashReportStorage(
                directoryURL: { reportsDirectory },
                createDirectory: { _ in throw StorageFailure.createDirectory },
                listFiles: { _ in [] },
                readData: { _ in Data() },
                writeData: { _, _ in },
                removeItem: { _ in }
            )
        )

        CrashReporter.shared.install()

        XCTAssertEqual(CrashReporter.shared.operationalIssues.count, 1)
        XCTAssertEqual(CrashReporter.shared.operationalIssues.first?.domain, .crashReporting)
        XCTAssertEqual(CrashReporter.shared.operationalIssues.first?.operation, .install)
    }

    func testGetAllReports_surfacesOperationalIssue_forUnreadableReport() throws {
        try FileManager.default.createDirectory(at: reportsDirectory, withIntermediateDirectories: true)
        let invalidReportURL = reportsDirectory.appendingPathComponent("broken.json")
        try Data("not-json".utf8).write(to: invalidReportURL)

        let reports = CrashReporter.shared.getAllReports()

        XCTAssertTrue(reports.isEmpty)
        XCTAssertEqual(CrashReporter.shared.operationalIssues.count, 1)
        XCTAssertEqual(CrashReporter.shared.operationalIssues.first?.domain, .crashReporting)
        XCTAssertEqual(CrashReporter.shared.operationalIssues.first?.operation, .load)
        let reportedContext = CrashReporter.shared.operationalIssues.first.map { issue in
            URL(fileURLWithPath: issue.context).resolvingSymlinksInPath().path
        }
        XCTAssertEqual(reportedContext, invalidReportURL.resolvingSymlinksInPath().path)
    }
}
