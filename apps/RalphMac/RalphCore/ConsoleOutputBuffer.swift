/**
 ConsoleOutputBuffer

 Purpose:
 - Manage a size-limited rolling buffer for console output text

 Responsibilities:
 - Manage a size-limited rolling buffer for console output text
 - Maintain configurable maximum character count with truncation
 - Track original content length for truncation reporting
 - Provide thread-safe access to buffer contents (via MainActor)

 Does not handle:
 - ANSI parsing or attributed output (see Workspace.parseANSICodes)
 - UI presentation or display logic
 - Persistence of buffer contents across sessions

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - All mutations must occur on MainActor (matches Workspace requirements)
 - Truncation preserves trailing (most recent) content
 - The truncation indicator is prepended when truncation occurs
 */

import Foundation

/// A size-limited rolling buffer for console output to prevent memory exhaustion.
@MainActor
public final class ConsoleOutputBuffer: Sendable {
    /// Hard upper bound for retained console text to avoid catastrophic memory usage.
    public static let hardMaxCharacters: Int = 2_000_000

    /// Maximum number of characters to retain in buffer (default: 100KB)
    public var maxCharacters: Int {
        didSet {
            let clamped = Self.clampMaxCharacters(maxCharacters)
            if clamped != maxCharacters {
                maxCharacters = clamped
                return
            }
            if maxCharacters != oldValue {
                enforceLimit()
            }
        }
    }

    /// Indicator string prepended when content is truncated
    public var truncationIndicator: String

    /// Current buffer content (may be truncated from original input)
    public private(set) var content: String = ""

    /// Original content length before any truncation
    public private(set) var originalLength: Int = 0

    /// Whether content has been truncated
    public var isTruncated: Bool { originalLength > maxCharacters }

    /// UserDefaults key for max characters persistence
    private static let defaultsKey = "com.mitchfultz.ralph.consoleOutputBuffer.maxCharacters"
    /// UserDefaults key for truncation indicator persistence
    private static let indicatorDefaultsKey = "com.mitchfultz.ralph.consoleOutputBuffer.truncationIndicator"

    /// Default maximum characters (100KB)
    public static let defaultMaxCharacters: Int = 100_000

    /// Default truncation indicator template
    public static let defaultTruncationIndicator = "\n... [output truncated, showing last {remaining} of {total} characters] ...\n"

    /// Creates a new console output buffer with the specified limits.
    ///
    /// - Parameters:
    ///   - maxCharacters: Maximum characters to retain (default: 100KB)
    ///   - truncationIndicator: Template string for truncation notice with {remaining} and {total} placeholders
    public init(
        maxCharacters: Int = defaultMaxCharacters,
        truncationIndicator: String = defaultTruncationIndicator
    ) {
        self.maxCharacters = Self.clampMaxCharacters(maxCharacters)
        self.truncationIndicator = truncationIndicator
    }

    /// Load settings from UserDefaults, falling back to defaults if not set.
    public static func loadFromUserDefaults() -> ConsoleOutputBuffer {
        let defaults = RalphAppDefaults.userDefaults
        let maxChars = defaults.integer(forKey: defaultsKey)
        let indicator = defaults.string(forKey: indicatorDefaultsKey)

        return ConsoleOutputBuffer(
            maxCharacters: maxChars > 0 ? maxChars : defaultMaxCharacters,
            truncationIndicator: indicator ?? defaultTruncationIndicator
        )
    }

    /// Save current settings to UserDefaults.
    public func saveToUserDefaults() {
        let defaults = RalphAppDefaults.userDefaults
        defaults.set(maxCharacters, forKey: Self.defaultsKey)
        defaults.set(truncationIndicator, forKey: Self.indicatorDefaultsKey)
    }

    /// Append text to buffer, enforcing size limits.
    ///
    /// - Parameter text: The text to append
    public func append(_ text: String) {
        content.append(text)
        originalLength += text.count
        enforceLimit()
    }

    /// Set buffer content directly (replaces existing content).
    ///
    /// - Parameter text: The new content
    public func setContent(_ text: String) {
        content = text
        originalLength = text.count
        enforceLimit()
    }

    /// Clear all content and reset tracking.
    public func clear() {
        content = ""
        originalLength = 0
    }

    /// Enforce the maximum character limit by truncating from the beginning if needed.
    /// Preserves trailing (most recent) content and prepends an indicator when truncated.
    private func enforceLimit() {
        guard content.count > maxCharacters else { return }

        let indicator = truncationIndicator
            .replacingOccurrences(of: "{remaining}", with: String(maxCharacters))
            .replacingOccurrences(of: "{total}", with: String(originalLength))

        let indicatorLength = indicator.count
        let availableForContent = max(0, maxCharacters - indicatorLength)

        if availableForContent > 0 {
            let startIndex = content.index(content.endIndex, offsetBy: -availableForContent)
            content = indicator + String(content[startIndex...])
        } else {
            // Indicator itself exceeds limit, just truncate hard from beginning
            content = String(content.suffix(maxCharacters))
        }
    }

    private static func clampMaxCharacters(_ value: Int) -> Int {
        max(1, min(value, hardMaxCharacters))
    }
}

// MARK: - CustomStringConvertible

extension ConsoleOutputBuffer {
    /// A textual representation of the buffer state.
    public var description: String {
        let status = isTruncated ? "truncated (showing \(content.count)/\(originalLength) chars)" : "\(content.count) chars"
        return "ConsoleOutputBuffer(\(status), max: \(maxCharacters))"
    }
}
