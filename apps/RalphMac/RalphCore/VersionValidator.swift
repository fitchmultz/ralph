/**
 VersionValidator

 Responsibilities:
 - Parse and validate semantic version strings from the ralph CLI.
 - Compare versions against a supported range.
 - Provide clear error messages for version mismatches.

 Does not handle:
 - Executing the CLI (see RalphCLIClient).
 - UI presentation of errors.

 Invariants/assumptions callers must respect:
 - Version strings follow semantic versioning (MAJOR.MINOR.PATCH).
 - The supported version range is defined at compile time.
 */

public import Foundation

/// Constants for the supported bundled CLI version.
/// These values are synchronized from the repo-wide VERSION file.
public enum VersionCompatibility {
    /// Minimum supported CLI version (inclusive).
    public static let minimumCLIVersion = "0.2.2"
    /// Maximum supported CLI version (inclusive).
    public static let maximumCLIVersion = "0.2.2"
    /// Cache duration for version check results (in seconds)
    public static let cacheDuration: TimeInterval = 300 // 5 minutes
}

public struct VersionValidator: Sendable, Equatable {
    public struct SemanticVersion: Sendable, Equatable, Comparable {
        public let major: Int
        public let minor: Int
        public let patch: Int
        
        public init(major: Int, minor: Int, patch: Int) {
            self.major = major
            self.minor = minor
            self.patch = patch
        }
        
        public init?(from string: String) {
            // Parse version strings like "0.1.0", "ralph 0.1.0", "v0.1.0"
            let trimmed = string.trimmingCharacters(in: .whitespacesAndNewlines)
            
            // Extract version number pattern (X.Y.Z)
            let pattern = #"(\d+)\.(\d+)\.(\d+)"#
            guard let regex = try? NSRegularExpression(pattern: pattern),
                  let match = regex.firstMatch(in: trimmed, range: NSRange(trimmed.startIndex..., in: trimmed)) else {
                return nil
            }
            
            guard let majorRange = Range(match.range(at: 1), in: trimmed),
                  let minorRange = Range(match.range(at: 2), in: trimmed),
                  let patchRange = Range(match.range(at: 3), in: trimmed),
                  let major = Int(trimmed[majorRange]),
                  let minor = Int(trimmed[minorRange]),
                  let patch = Int(trimmed[patchRange]) else {
                return nil
            }
            
            self.major = major
            self.minor = minor
            self.patch = patch
        }
        
        public static func < (lhs: SemanticVersion, rhs: SemanticVersion) -> Bool {
            if lhs.major != rhs.major { return lhs.major < rhs.major }
            if lhs.minor != rhs.minor { return lhs.minor < rhs.minor }
            return lhs.patch < rhs.patch
        }
        
        public var description: String {
            "\(major).\(minor).\(patch)"
        }
    }
    
    public enum VersionStatus: Sendable, Equatable {
        case compatible
        case tooOld(SemanticVersion, SemanticVersion)  // (found, minimum)
        case tooNew(SemanticVersion, SemanticVersion)  // (found, maximum)
        case unparsable(String)  // Raw version string that couldn't be parsed
    }
    
    public struct VersionCheckResult: Sendable, Equatable {
        public let status: VersionStatus
        public let rawVersion: String
        
        public var isCompatible: Bool {
            if case .compatible = status { return true }
            return false
        }
        
        public var errorMessage: String? {
            switch status {
            case .compatible:
                return nil
            case .tooOld(let found, let minimum):
                return "Ralph CLI version \(found.description) is too old. Minimum supported version is \(minimum.description)."
            case .tooNew(let found, let maximum):
                return "Ralph CLI version \(found.description) is newer than supported. Maximum tested version is \(maximum.description)."
            case .unparsable(let raw):
                return "Unable to parse Ralph CLI version from: '\(raw)'"
            }
        }
        
        public var guidanceMessage: String? {
            guard !isCompatible else { return nil }
            return "Please reinstall Ralph to ensure the CLI and app versions match. The bundled CLI should be in RalphMac.app/Contents/MacOS/ralph"
        }
    }
    
    public let minimumVersion: SemanticVersion
    public let maximumVersion: SemanticVersion
    
    /// Initialize with supported version range
    /// - Parameters:
    ///   - minimumVersion: Minimum supported CLI version (inclusive)
    ///   - maximumVersion: Maximum supported CLI version (inclusive)
    public init(minimumVersion: SemanticVersion, maximumVersion: SemanticVersion) {
        self.minimumVersion = minimumVersion
        self.maximumVersion = maximumVersion
    }
    
    /// Convenience initializer using version strings
    public init(minimumVersion: String, maximumVersion: String) {
        self.minimumVersion = SemanticVersion(from: minimumVersion) ?? SemanticVersion(major: 0, minor: 1, patch: 0)
        self.maximumVersion = SemanticVersion(from: maximumVersion) ?? SemanticVersion(major: 0, minor: 2, patch: 0)
    }
    
    /// Initialize with the default supported version range from VersionCompatibility
    public init() {
        self.minimumVersion = SemanticVersion(from: VersionCompatibility.minimumCLIVersion) ?? SemanticVersion(major: 0, minor: 1, patch: 0)
        self.maximumVersion = SemanticVersion(from: VersionCompatibility.maximumCLIVersion) ?? SemanticVersion(major: 0, minor: 2, patch: 0)
    }
    
    /// Validate a version string against the supported range
    public func validate(_ versionString: String) -> VersionCheckResult {
        guard let version = SemanticVersion(from: versionString) else {
            return VersionCheckResult(status: .unparsable(versionString), rawVersion: versionString)
        }
        
        let status: VersionStatus
        if version < minimumVersion {
            status = .tooOld(version, minimumVersion)
        } else if version > maximumVersion {
            status = .tooNew(version, maximumVersion)
        } else {
            status = .compatible
        }
        
        return VersionCheckResult(status: status, rawVersion: versionString)
    }
}
