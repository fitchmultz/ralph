/**
 CrashReporter

 Purpose:
 - Capture uncaught exceptions and crashes in the macOS app.

 Responsibilities:
 - Capture uncaught exceptions and crashes in the macOS app.
 - Store crash reports locally for user review and optional export.
 - Surface crash-report persistence failures as observable operational state.
 - Provide basic crash metadata (timestamp, stack trace, app version).

 Does not handle:
 - Automatic upload to external services (user must manually export).
 - Symbolication of stack traces (raw addresses only).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Must be initialized early in app lifecycle (before crashes can occur).
 - Crash reports are stored in app support directory with .json extension.
 - Maximum of 10 crash reports are retained; older reports are pruned.
 */

public import Foundation
import AppKit
import Darwin

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

    public var formattedReport: String {
        var lines: [String] = []
        lines.append("=== Ralph Crash Report ===")
        lines.append("ID: \(id.uuidString)")
        lines.append("Timestamp: \(timestamp.formatted(.iso8601))")
        lines.append("App Version: \(appVersion)")
        lines.append("macOS Version: \(osVersion)")
        lines.append("Device: \(deviceModel)")
        lines.append("")
        if let exceptionName {
            lines.append("Exception: \(exceptionName)")
        }
        if let exceptionReason {
            lines.append("Reason: \(exceptionReason)")
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

struct CrashReportStorage: Sendable {
    var directoryURL: @Sendable () -> URL
    var createDirectory: @Sendable (URL) throws -> Void
    var listFiles: @Sendable (URL) throws -> [URL]
    var readData: @Sendable (URL) throws -> Data
    var writeData: @Sendable (Data, URL) throws -> Void
    var removeItem: @Sendable (URL) throws -> Void

    static let live = CrashReportStorage(
        directoryURL: {
            let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
                ?? FileManager.default.temporaryDirectory
            return appSupport.appendingPathComponent("Ralph/CrashReports", isDirectory: true)
        },
        createDirectory: { url in
            try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        },
        listFiles: { url in
            try FileManager.default.contentsOfDirectory(at: url, includingPropertiesForKeys: nil)
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

@MainActor
public final class CrashReporter: ObservableObject {
    public static let shared = CrashReporter()

    @Published public private(set) var operationalIssues: [PersistenceIssue] = []

    private let maxReports = 10
    private static let storageLock = NSLock()
    private static var storage = CrashReportStorage.live

    private init() {}

    public func install() {
        let storage = Self.currentStorage()
        do {
            try storage.createDirectory(storage.directoryURL())
            clearIssues(for: .install)
        } catch {
            recordIssue(
                PersistenceIssue(
                    domain: .crashReporting,
                    operation: .install,
                    context: storage.directoryURL().path,
                    error: error
                )
            )
            return
        }

        NSSetUncaughtExceptionHandler { exception in
            CrashReporter.handleException(exception)
        }

        pruneOldReports()
        RalphLogger.shared.info("Crash reporter installed", category: .crashReporting)
    }

    public func getAllReports() -> [CrashReport] {
        let storage = Self.currentStorage()
        let directory = storage.directoryURL()
        do {
            let files = try storage.listFiles(directory)
            var encounteredLoadIssue = false
            let reports = files
                .filter { $0.pathExtension == "json" }
                .compactMap { fileURL -> (CrashReport, URL)? in
                    do {
                        let data = try storage.readData(fileURL)
                        let report = try JSONDecoder().decode(CrashReport.self, from: data)
                        return (report, fileURL)
                    } catch {
                        encounteredLoadIssue = true
                        recordIssue(
                            PersistenceIssue(
                                domain: .crashReporting,
                                operation: .load,
                                context: fileURL.path,
                                error: error
                            )
                        )
                        return nil
                    }
                }
                .sorted { $0.0.timestamp > $1.0.timestamp }

            if !encounteredLoadIssue {
                clearIssues(for: .load)
            }
            return reports.map(\.0)
        } catch {
            recordIssue(
                PersistenceIssue(
                    domain: .crashReporting,
                    operation: .load,
                    context: directory.path,
                    error: error
                )
            )
            return []
        }
    }

    public func clearAllReports() {
        let storage = Self.currentStorage()
        let directory = storage.directoryURL()
        do {
            let files = try storage.listFiles(directory)
            for file in files where file.pathExtension == "json" {
                try storage.removeItem(file)
            }
            clearIssues(for: .delete)
            RalphLogger.shared.info("Cleared all crash reports", category: .crashReporting)
        } catch {
            recordIssue(
                PersistenceIssue(
                    domain: .crashReporting,
                    operation: .delete,
                    context: directory.path,
                    error: error
                )
            )
        }
    }

    public func exportAllReports() -> String {
        let reports = getAllReports()
        if reports.isEmpty {
            return "No crash reports found."
        }
        return reports.map(\.formattedReport).joined(separator: "\n\n")
    }

    public func clearOperationalIssues() {
        operationalIssues.removeAll(keepingCapacity: false)
    }

    func setStorageForTesting(_ storage: CrashReportStorage) {
        Self.storageLock.lock()
        Self.storage = storage
        Self.storageLock.unlock()
    }

    func reportsDirectoryURLForTesting() -> URL {
        Self.currentStorage().directoryURL()
    }

    private func pruneOldReports() {
        let storage = Self.currentStorage()
        let directory = storage.directoryURL()
        do {
            let files = try storage.listFiles(directory)
                .filter { $0.pathExtension == "json" }
                .sorted { $0.lastPathComponent < $1.lastPathComponent }
            guard files.count > maxReports else {
                clearIssues(for: .prune)
                return
            }

            for file in files.prefix(files.count - maxReports) {
                try storage.removeItem(file)
            }
            clearIssues(for: .prune)
        } catch {
            recordIssue(
                PersistenceIssue(
                    domain: .crashReporting,
                    operation: .prune,
                    context: directory.path,
                    error: error
                )
            )
        }
    }

    private func recordIssue(_ issue: PersistenceIssue) {
        operationalIssues.removeAll {
            $0.domain == issue.domain &&
            $0.operation == issue.operation &&
            $0.context == issue.context
        }
        operationalIssues.append(issue)
        RalphLogger.shared.error(
            "Crash reporter \(issue.operation.rawValue) failed for \(issue.context): \(issue.message)",
            category: .crashReporting
        )
    }

    private func clearIssues(for operation: PersistenceIssue.Operation) {
        operationalIssues.removeAll {
            $0.domain == .crashReporting && $0.operation == operation
        }
    }

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

        do {
            try persistCrashReport(report)
        } catch {
            Task { @MainActor in
                CrashReporter.shared.recordIssue(
                    PersistenceIssue(
                        domain: .crashReporting,
                        operation: .save,
                        context: Self.currentStorage().directoryURL().path,
                        error: error
                    )
                )
            }
        }
    }

    private static func persistCrashReport(_ report: CrashReport) throws {
        let storage = currentStorage()
        let directory = storage.directoryURL()
        try storage.createDirectory(directory)
        let data = try JSONEncoder().encode(report)
        let filename = "crash_\(report.timestamp.timeIntervalSince1970).json"
        try storage.writeData(data, directory.appendingPathComponent(filename))
    }

    private static func currentStorage() -> CrashReportStorage {
        storageLock.lock()
        defer { storageLock.unlock() }
        return storage
    }

    private static func deviceModel() -> String {
        var size = 0
        sysctlbyname("hw.model", nil, &size, nil, 0)
        var machine = [CChar](repeating: 0, count: size)
        sysctlbyname("hw.model", &machine, &size, nil, 0)
        let trimmed = machine.prefix { $0 != 0 }
        let bytes = trimmed.map { UInt8(bitPattern: $0) }
        return String(decoding: bytes, as: UTF8.self)
    }
}
