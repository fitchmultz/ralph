/**
 RalphLogger
 
 Responsibilities:
 - Provide centralized structured logging using modern OSLog Logger API.
 - Define consistent subsystem and category structure for all app logging.
 - Support log levels: debug, info, error, fault with appropriate privacy qualifiers.
 - Provide crash reporting integration hooks.
 - Support log export functionality via OSLogStore (macOS 12+).
 
 Does not handle:
 - Direct UI presentation of logs (see views for that).
 - Log rotation or archival management (OSLog handles this).
 
 Invariants/assumptions callers must respect:
 - Use appropriate privacy qualifiers - mark sensitive data as .private.
 - Use appropriate log levels - fault for catastrophic failures only.
 - Categories are fixed enums - do not create dynamic category strings.
 */

import Foundation
public import OSLog

/// Centralized logging system for Ralph macOS app
/// Note: OSLog.Logger is thread-safe, so this class is marked as @unchecked Sendable
public final class RalphLogger: @unchecked Sendable {
    public static let shared = RalphLogger()
    
    /// Log categories for organizing log output
    public enum Category: String, CaseIterable, Sendable {
        case fileWatching = "FileWatching"
        case cli = "CLI"
        case workspace = "Workspace"
        case ui = "UI"
        case lifecycle = "Lifecycle"
        case crashReporting = "CrashReporting"
        case config = "Config"
        
        public var description: String {
            return rawValue
        }
    }
    
    /// The subsystem identifier for all Ralph logs
    public static let subsystem = "com.mitchfultz.ralph"
    
    /// Logger instances for each category - immutable after init, safe for concurrent access
    private let loggers: [Category: Logger]
    
    private init() {
        // Initialize loggers for all categories
        var loggersTemp: [Category: Logger] = [:]
        for category in Category.allCases {
            loggersTemp[category] = Logger(subsystem: RalphLogger.subsystem, category: category.rawValue)
        }
        loggers = loggersTemp
    }
    
    /// Get the logger for a specific category
    public func logger(for category: Category) -> Logger {
        return loggers[category] ?? Logger(subsystem: RalphLogger.subsystem, category: "Default")
    }
    
    // MARK: - Convenience Methods
    
    /// Log a debug message
    public func debug(_ message: String, category: Category, file: String = #file, function: String = #function, line: Int = #line) {
        let logger = logger(for: category)
        let fileName = (file as NSString).lastPathComponent
        logger.debug("[\(fileName, privacy: .public):\(line, privacy: .public)] \(message, privacy: .public)")
    }
    
    /// Log an info message
    public func info(_ message: String, category: Category, file: String = #file, function: String = #function, line: Int = #line) {
        let logger = logger(for: category)
        let fileName = (file as NSString).lastPathComponent
        logger.info("[\(fileName, privacy: .public):\(line, privacy: .public)] \(message, privacy: .public)")
    }
    
    /// Log an error message
    public func error(_ message: String, category: Category, file: String = #file, function: String = #function, line: Int = #line) {
        let logger = logger(for: category)
        let fileName = (file as NSString).lastPathComponent
        logger.error("[\(fileName, privacy: .public):\(line, privacy: .public)] \(message, privacy: .public)")
    }
    
    /// Log a fault message (for catastrophic failures)
    public func fault(_ message: String, category: Category, file: String = #file, function: String = #function, line: Int = #line) {
        let logger = logger(for: category)
        let fileName = (file as NSString).lastPathComponent
        logger.fault("[\(fileName, privacy: .public):\(line, privacy: .public)] \(message, privacy: .public)")
    }
    
    /// Log with private data (redacted in release builds)
    public func debugPrivate(_ message: String, privateData: String, category: Category, file: String = #file, function: String = #function, line: Int = #line) {
        let logger = logger(for: category)
        let fileName = (file as NSString).lastPathComponent
        logger.debug("[\(fileName, privacy: .public):\(line, privacy: .public)] \(message, privacy: .public): \(privateData, privacy: .private)")
    }
    
    // MARK: - Log Export
    
    /// Export recent logs for the Ralph subsystem
    /// - Parameters:
    ///   - hours: Number of hours of logs to export (default 24)
    ///   - completion: Callback with the exported log string or nil if unavailable
    @available(macOS 12.0, *)
    public func exportLogs(hours: Int = 24, completion: @escaping @Sendable (String?) -> Void) {
        Task {
            do {
                let store = try OSLogStore(scope: .currentProcessIdentifier)
                let startDate = Date().addingTimeInterval(-TimeInterval(hours * 3600))
                let position = store.position(date: startDate)
                
                let entries = try store.getEntries(at: position)
                    .compactMap { $0 as? OSLogEntryLog }
                    .filter { $0.subsystem == RalphLogger.subsystem }
                    .map { "[\($0.date.formatted())] [\($0.category)] \($0.composedMessage)" }
                
                let logOutput = entries.joined(separator: "\n")
                completion(logOutput.isEmpty ? "No logs found for the specified time period." : logOutput)
            } catch {
                completion("Failed to export logs: \(error.localizedDescription)")
            }
        }
    }
    
    /// Check if log export is available (macOS 12+)
    public var canExportLogs: Bool {
        if #available(macOS 12.0, *) {
            return true
        }
        return false
    }
}

