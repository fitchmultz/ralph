/**
 RalphCLITransientErrorPolicy

 Purpose:
 - Provide one canonical transient-error signal policy shared by retry and recovery classifiers.

 Responsibilities:
 - Centralize retryable exit-code, POSIX, and Cocoa error-code policy for transient failures.
 - Centralize transient phrase matching so retry and recovery surfaces cannot drift.
 - Map shared transient signals to recovery categories without forcing caller-specific final decisions.

 Does not handle:
 - Queue-lock/config/version-specific recovery branches.
 - Final retry admission decisions that require typed domain context.
 - Process execution, backoff, or UI rendering.

 Usage:
 - `RetryHelper` and `RalphCLIRecoveryClassifier` call this module.

 Invariants/assumptions callers must respect:
 - Input text can be free-form stderr or localized descriptions.
 - Callers own precedence when structured machine-error payloads are available.
 */

import Foundation

#if canImport(Darwin)
import Darwin
#endif

enum RalphCLITransientSignal: Equatable {
    case resourceBusy
    case networkError
}

enum RalphCLITransientErrorPolicy {
    static let retryableProcessExitCodes: Set<Int32> = [75, 111]
    static let retryablePOSIXCodes: Set<Int32> = [EAGAIN, EWOULDBLOCK, EBUSY, EDEADLK, EINTR]
    static let retryableCocoaCodes: Set<Int> = [NSFileReadUnknownError, NSFileWriteUnknownError, NSFileLockingError]

    private static let transientPatterns: [(needle: String, signal: RalphCLITransientSignal)] = [
        ("resource temporarily unavailable", .resourceBusy),
        ("operation would block", .resourceBusy),
        ("device or resource busy", .resourceBusy),
        ("resource busy", .resourceBusy),
        ("file is locked", .resourceBusy),
        ("file locked", .resourceBusy),
        ("locked", .resourceBusy),
        ("eagain", .resourceBusy),
        ("ewouldblock", .resourceBusy),
        ("ebusy", .resourceBusy),
        ("try again", .resourceBusy),
        ("io timeout", .networkError),
        ("timed out", .networkError),
        ("connection reset", .networkError),
        ("broken pipe", .networkError),
    ]

    static func transientSignal(in text: String) -> RalphCLITransientSignal? {
        let normalized = text.lowercased()
        return transientPatterns.first(where: { normalized.contains($0.needle) })?.signal
    }

    static func isRetryableProcessError(exitCode: Int32, stderr: String) -> Bool {
        if retryableProcessExitCodes.contains(exitCode) {
            return true
        }
        return transientSignal(in: stderr) != nil
    }

    static func isRetryableUnderlyingError(_ error: any Error) -> Bool {
        let nsError = error as NSError

        if nsError.domain == NSPOSIXErrorDomain {
            if retryablePOSIXCodes.contains(Int32(nsError.code)) {
                return true
            }
        }

        if nsError.domain == NSCocoaErrorDomain {
            if retryableCocoaCodes.contains(nsError.code) {
                return true
            }
        }

        return transientSignal(in: error.localizedDescription) != nil
    }

    static func recoveryCategory(for text: String) -> ErrorCategory? {
        switch transientSignal(in: text) {
        case .resourceBusy:
            return .resourceBusy
        case .networkError:
            return .networkError
        case nil:
            return nil
        }
    }
}
