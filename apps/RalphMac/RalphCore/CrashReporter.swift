/**
 CrashReporter
 
 Responsibilities:
 - Capture uncaught exceptions and crashes in the macOS app.
 - Store crash reports locally for user review and optional export.
 - Provide basic crash metadata (timestamp, stack trace, app version).
 
 Does not handle:
 - Automatic upload to external services (user must manually export).
 - Symbolication of stack traces (raw addresses only).
 
 Invariants/assumptions callers must respect:
 - Must be initialized early in app lifecycle (before crashes can occur).
 - Crash reports are stored in app support directory with .json extension.
 - Maximum of 10 crash reports are retained; older reports are pruned.
 */

public import Foundation
import AppKit
import Darwin

/// Information about a captured crash
public struct CrashReport: Codable, Identifiable, Sendable {
    public let id: UUID
    public let timestamp: Date
    public let exceptionName: String?
    public let exceptionReason: String?
    public let stackTrace: [String]
    public let appVersion: String
    public let osVersion: String
    public let deviceModel: String
    
    public init(
        id: UUID,
        timestamp: Date,
        exceptionName: String?,
        exceptionReason: String?,
        stackTrace: [String],
        appVersion: String,
        osVersion: String,
        deviceModel: String
    ) {
        self.id = id
        self.timestamp = timestamp
        self.exceptionName = exceptionName
        self.exceptionReason = exceptionReason
        self.stackTrace = stackTrace
        self.appVersion = appVersion
        self.osVersion = osVersion
        self.deviceModel = deviceModel
    }
    
    /// Human-readable formatted report
    public var formattedReport: String {
        var lines: [String] = []
        lines.append("=== Ralph Crash Report ===")
        lines.append("ID: \(id.uuidString)")
        lines.append("Timestamp: \(timestamp.formatted(.iso8601))")
        lines.append("App Version: \(appVersion)")
        lines.append("macOS Version: \(osVersion)")
        lines.append("Device: \(deviceModel)")
        lines.append("")
        if let name = exceptionName {
            lines.append("Exception: \(name)")
        }
        if let reason = exceptionReason {
            lines.append("Reason: \(reason)")
        }
        lines.append("")
        lines.append("Stack Trace:")
        for frame in stackTrace {
            lines.append("  \(frame)")
        }
        lines.append("========================")
        return lines.joined(separator: "\n")
    }
}

/// Simple crash reporter that captures uncaught exceptions
public final class CrashReporter: @unchecked Sendable {
    public static let shared = CrashReporter()
    
    private let maxReports = 10
    
    private init() {}
    
    /// Returns the crash reports directory URL
    private func crashReportsDirectory() -> URL {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport.appendingPathComponent("Ralph/CrashReports", isDirectory: true)
    }
    
    /// Install crash handlers. Call this early in app launch.
    public func install() {
        // Ensure directory exists
        try? FileManager.default.createDirectory(at: crashReportsDirectory(), withIntermediateDirectories: true)
        
        // Install exception handler
        NSSetUncaughtExceptionHandler { exception in
            CrashReporter.handleException(exception)
        }
        
        // Note: Signal handling for crashes would require C-style signal handlers
        // which are complex in Swift. For this implementation, we focus on exception handling.
        
        // Clean up old reports to enforce maxReports limit
        pruneOldReports()
        
        RalphLogger.shared.info("Crash reporter installed", category: .crashReporting)
    }
    
    /// Nonisolated handler for exceptions (called from C callback)
    private static func handleException(_ exception: NSException) {
        let report = CrashReport(
            id: UUID(),
            timestamp: Date(),
            exceptionName: exception.name.rawValue,
            exceptionReason: exception.reason,
            stackTrace: exception.callStackSymbols,
            appVersion: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "unknown",
            osVersion: ProcessInfo.processInfo.operatingSystemVersionString,
            deviceModel: deviceModel()
        )
        
        // Save report synchronously (we may crash soon)
        if let data = try? JSONEncoder().encode(report) {
            let filename = "crash_\(report.timestamp.timeIntervalSince1970).json"
            let url = crashReportsDirectoryStatic().appendingPathComponent(filename)
            try? data.write(to: url)
        }
    }
    
    /// Static version for nonisolated contexts
    private static func crashReportsDirectoryStatic() -> URL {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return appSupport.appendingPathComponent("Ralph/CrashReports", isDirectory: true)
    }
    
    /// Get all stored crash reports
    public func getAllReports() -> [CrashReport] {
        guard let files = try? FileManager.default.contentsOfDirectory(at: crashReportsDirectory(), includingPropertiesForKeys: nil) else {
            return []
        }
        
        return files
            .filter { $0.pathExtension == "json" }
            .compactMap { url -> CrashReport? in
                guard let data = try? Data(contentsOf: url) else { return nil }
                return try? JSONDecoder().decode(CrashReport.self, from: data)
            }
            .sorted { $0.timestamp > $1.timestamp }
    }
    
    /// Clear all stored crash reports
    public func clearAllReports() {
        guard let files = try? FileManager.default.contentsOfDirectory(at: crashReportsDirectory(), includingPropertiesForKeys: nil) else {
            return
        }
        
        for file in files where file.pathExtension == "json" {
            try? FileManager.default.removeItem(at: file)
        }
        
        RalphLogger.shared.info("Cleared all crash reports", category: .crashReporting)
    }
    
    /// Export all crash reports as a single string
    public func exportAllReports() -> String {
        let reports = getAllReports()
        if reports.isEmpty {
            return "No crash reports found."
        }
        return reports.map { $0.formattedReport }.joined(separator: "\n\n")
    }
    
    /// Delete old reports if we exceed maxReports
    private func pruneOldReports() {
        let reports = getAllReports()
        if reports.count > maxReports {
            let toDelete = reports.suffix(from: maxReports)
            for report in toDelete {
                let url = crashReportsDirectory().appendingPathComponent("crash_\(report.timestamp.timeIntervalSince1970).json")
                try? FileManager.default.removeItem(at: url)
            }
        }
    }
    
    /// Get device model string
    private static func deviceModel() -> String {
        var size = 0
        sysctlbyname("hw.model", nil, &size, nil, 0)
        var machine = [CChar](repeating: 0, count: size)
        sysctlbyname("hw.model", &machine, &size, nil, 0)
        return String(cString: machine)
    }
}
