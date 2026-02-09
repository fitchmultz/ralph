import XCTest
@testable import RalphCore

@MainActor
final class CrashReporterTests: XCTestCase {
    
    override func setUp() {
        super.setUp()
        CrashReporter.shared.clearAllReports()
    }
    
    override func tearDown() {
        CrashReporter.shared.clearAllReports()
        super.tearDown()
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
}
