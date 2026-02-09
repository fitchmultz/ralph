/**
 RetryHelper

 Responsibilities:
 - Provide configurable retry logic with exponential backoff for transient failures.
 - Apply jitter to prevent thundering herd in multi-instance scenarios.
 - Classify errors as retryable or non-retryable based on error type and content.

 Does not handle:
 - UI progress indication (callers handle user-visible feedback).
 - Infinite retries (max attempts is always enforced).

 Invariants/assumptions callers must respect:
 - Operations must be idempotent or safe to repeat.
 - Retry configuration is immutable once created.
 */

public import Foundation

#if canImport(Darwin)
import Darwin
#endif

/// Configuration for retry behavior
public struct RetryConfiguration: Sendable {
    public let maxRetries: Int
    public let baseDelay: TimeInterval
    public let maxDelay: TimeInterval
    public let jitterRange: ClosedRange<TimeInterval>
    
    public init(
        maxRetries: Int = 3,
        baseDelay: TimeInterval = 0.1,
        maxDelay: TimeInterval = 1.6,
        jitterRange: ClosedRange<TimeInterval> = 0.01...0.03
    ) {
        self.maxRetries = maxRetries
        self.baseDelay = baseDelay
        self.maxDelay = maxDelay
        self.jitterRange = jitterRange
    }
    
    /// Default configuration: 3 retries, 100ms base, 1600ms max, 10-30ms jitter
    public static let `default` = RetryConfiguration()
    
    /// Aggressive configuration for critical operations: 5 retries
    public static let aggressive = RetryConfiguration(maxRetries: 5)
    
    /// Minimal configuration for fast operations: 1 retry
    public static let minimal = RetryConfiguration(maxRetries: 1)
}

/// Errors that can trigger a retry
public enum RetryableError: Error, Sendable, Equatable {
    case fileLocked
    case resourceBusy
    case ioTimeout
    case resourceTemporarilyUnavailable
    case processError(exitCode: Int32, stderr: String)
    case underlying(any Error)
    
    public static func == (lhs: RetryableError, rhs: RetryableError) -> Bool {
        switch (lhs, rhs) {
        case (.fileLocked, .fileLocked),
             (.resourceBusy, .resourceBusy),
             (.ioTimeout, .ioTimeout),
             (.resourceTemporarilyUnavailable, .resourceTemporarilyUnavailable):
            return true
        case let (.processError(lCode, lStderr), .processError(rCode, rStderr)):
            return lCode == rCode && lStderr == rStderr
        case (.underlying, .underlying):
            // Can't compare underlying errors reliably
            return false
        default:
            return false
        }
    }
}

/// Result of a retry operation
public enum RetryResult<T: Sendable>: Sendable {
    case success(T)
    case failure(any Error, attempts: Int)
}

/// Progress callback for retry attempts
public typealias RetryProgressHandler = @Sendable (_ attempt: Int, _ maxAttempts: Int, _ delay: TimeInterval) -> Void

/// Helper for executing operations with retry logic
/// 
/// Note: This is not @MainActor-isolated to allow use from any context.
/// All state is immutable (Sendable) and operations are executed on the caller's context.
public final class RetryHelper: Sendable {
    private let configuration: RetryConfiguration
    
    public init(configuration: RetryConfiguration = .default) {
        self.configuration = configuration
    }
    
    /// Execute an operation with exponential backoff retry
    ///
    /// - Parameters:
    ///   - operation: The async operation to execute
    ///   - shouldRetry: Optional closure to determine if error is retryable
    ///   - onProgress: Optional callback for progress updates (attempt number, max attempts, next delay)
    /// - Returns: The result of the operation
    /// - Throws: The last error encountered after all retries are exhausted
    public func execute<T: Sendable>(
        operation: @Sendable () async throws -> T,
        shouldRetry: (@Sendable (any Error) -> Bool)? = nil,
        onProgress: RetryProgressHandler? = nil
    ) async throws -> T {
        let effectiveShouldRetry = shouldRetry ?? Self.defaultShouldRetry
        var lastError: (any Error)?
        
        for attempt in 1...configuration.maxRetries {
            do {
                let result = try await operation()
                return result
            } catch {
                lastError = error
                
                // Check if this is a retryable error
                guard effectiveShouldRetry(error) else {
                    throw error
                }
                
                // Don't delay after the last attempt
                guard attempt < configuration.maxRetries else {
                    break
                }
                
                // Calculate delay with exponential backoff and jitter
                let delay = calculateDelay(attempt: attempt)
                
                // Report progress
                onProgress?(attempt, configuration.maxRetries, delay)
                
                // Wait before retrying
                try await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            }
        }
        
        throw lastError ?? RetryHelperError.maxRetriesExceeded
    }
    
    /// Execute an operation returning a RetryResult (non-throwing variant)
    public func executeWithResult<T: Sendable>(
        operation: @Sendable () async throws -> T,
        shouldRetry: (@Sendable (any Error) -> Bool)? = nil,
        onProgress: RetryProgressHandler? = nil
    ) async -> RetryResult<T> {
        do {
            let result = try await execute(
                operation: operation,
                shouldRetry: shouldRetry,
                onProgress: onProgress
            )
            return .success(result)
        } catch {
            return .failure(error, attempts: configuration.maxRetries)
        }
    }
    
    /// Calculate delay for a given attempt using exponential backoff with jitter
    private func calculateDelay(attempt: Int) -> TimeInterval {
        // Exponential backoff: baseDelay * 2^(attempt-1)
        let exponentialDelay = configuration.baseDelay * pow(2.0, Double(attempt - 1))
        let cappedDelay = min(exponentialDelay, configuration.maxDelay)
        
        // Add jitter to prevent thundering herd
        let jitter = TimeInterval.random(in: configuration.jitterRange)
        
        return cappedDelay + jitter
    }
    
    /// Default implementation to determine if an error is retryable
    public static func defaultShouldRetry(_ error: any Error) -> Bool {
        // Check for specific error types by checking the type directly
        // Note: We can't use 'as?' cast with existential in Swift 6 in this context
        // Instead, we check the error's localized description and NSError properties
        
        // First check if it's a RetryableError by checking NSError domain/code
        let nsError = error as NSError
        if nsError.domain == "RetryableError" {
            // Check the error's user info for the case type
            let description = error.localizedDescription.lowercased()
            if description.contains("file locked") || 
               description.contains("resource busy") ||
               description.contains("io timeout") ||
               description.contains("resource temporarily unavailable") {
                return true
            }
        }
        
        // Check for process error patterns in description
        let description = error.localizedDescription.lowercased()
        let retryablePatterns = [
            "file locked",
            "resource busy",
            "io timeout",
            "resource temporarily unavailable",
            "operation would block",
            "device or resource busy",
            "eagain",
            "ewouldblock",
            "ebusy"
        ]
        
        if retryablePatterns.contains(where: { description.contains($0) }) {
            return true
        }
        
        // Check for common retryable underlying errors via NSError
        return isRetryableUnderlyingError(error)
    }
    
    /// Check if a process error is retryable based on exit code and stderr
    private static func isRetryableProcessError(exitCode: Int32, stderr: String) -> Bool {
        // Exit codes that indicate transient failures
        let retryableExitCodes: Set<Int32> = [75, 111] // EX_TEMPFAIL, EX_UNAVAILABLE
        
        if retryableExitCodes.contains(exitCode) {
            return true
        }
        
        // Check stderr for retryable error patterns
        let lowercasedStderr = stderr.lowercased()
        let retryablePatterns = [
            "resource temporarily unavailable",
            "operation would block",
            "device or resource busy",
            "file is locked",
            "io timeout",
            "eagain",
            "ewouldblock",
            "ebusy"
        ]
        
        return retryablePatterns.contains { lowercasedStderr.contains($0) }
    }
    
    /// Check if an underlying error is retryable
    private static func isRetryableUnderlyingError(_ error: any Error) -> Bool {
        let nsError = error as NSError
        
        // Check for POSIX error codes
        let retryablePOSIXCodes: Set<Int32> = [
            EAGAIN,      // Resource temporarily unavailable
            EWOULDBLOCK, // Operation would block
            EBUSY,       // Device or resource busy
            EDEADLK,     // Resource deadlock would occur
            EINTR        // Interrupted system call
        ]
        
        if nsError.domain == NSPOSIXErrorDomain {
            return retryablePOSIXCodes.contains(Int32(nsError.code))
        }
        
        // Check for Cocoa/Foundation errors that indicate transient issues
        let retryableCocoaCodes: Set<Int> = [
            NSFileReadUnknownError,
            NSFileWriteUnknownError,
            NSFileLockingError
        ]
        
        if nsError.domain == NSCocoaErrorDomain {
            return retryableCocoaCodes.contains(nsError.code)
        }
        
        // Check error description for common transient error patterns
        let errorDescription = error.localizedDescription.lowercased()
        let retryablePatterns = [
            "resource temporarily unavailable",
            "operation would block",
            "device or resource busy",
            "file is locked",
            "try again",
            "io timeout",
            "connection reset",
            "broken pipe"
        ]
        
        return retryablePatterns.contains { errorDescription.contains($0) }
    }
}

public enum RetryHelperError: Error, LocalizedError {
    case maxRetriesExceeded
    
    public var errorDescription: String? {
        switch self {
        case .maxRetriesExceeded:
            return "Operation failed after maximum retry attempts"
        }
    }
}
