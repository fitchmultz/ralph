/**
 QueueFileWatcher

 Responsibilities:
 - Monitor .ralph/queue.json and .ralph/done.json for external changes using FSEvents.
 - Emit notifications when files change with debouncing to batch rapid changes.
 - Handle file system events efficiently with minimal resource usage.

 Does not handle:
 - Direct UI updates (delegates via NotificationCenter).
 - Parsing or interpreting file contents.
 - Retry logic for transient errors (logs and continues watching).

 Invariants/assumptions callers must respect:
 - start() must be called to begin monitoring; stop() to clean up.
 - Debounce interval batches multiple rapid changes into single notification.
 - Callbacks occur on the main actor.
 */

public import Foundation
import CoreServices

@MainActor
public final class QueueFileWatcher: Sendable {
    // MARK: - Types

    public struct ChangeEvent: Sendable {
        public let fileURL: URL
        public let changeType: ChangeType

        public enum ChangeType: Sendable {
            case modified
            case renamed
            case deleted
        }
    }

    // MARK: - Properties

    // FSEvents state is accessed from callback queue and main actor
    private nonisolated(unsafe) var stream: FSEventStreamRef?
    // Mutable working directory - accessed only from callbackQueue.sync blocks
    // Marked as nonisolated(unsafe) because access is serialized via callbackQueue
    private nonisolated(unsafe) var workingDirectoryURL: URL
    private nonisolated let debounceInterval: TimeInterval = 0.5  // 500ms debounce

    // Shared state protected by lock (accessed from callback queue and main actor via debounce)
    private let lock = NSLock()
    private nonisolated(unsafe) var pendingChanges: Set<String> = []
    private nonisolated(unsafe) var debounceWorkItem: DispatchWorkItem?

    // Dispatch queue for FSEvents callbacks (must be nonisolated for C callback compatibility)
    private nonisolated let callbackQueue: DispatchQueue

    /// Callback invoked on MainActor when file changes are detected (after debounce)
    public var onFileChanged: (@MainActor () -> Void)?

    /// Whether the watcher is currently active
    public private(set) var isWatching = false

    // MARK: - Initialization

    public init(workingDirectoryURL: URL) {
        self.workingDirectoryURL = workingDirectoryURL
        self.callbackQueue = DispatchQueue(label: "com.mitchfultz.ralph.filewatcher.\(workingDirectoryURL.lastPathComponent)")
    }

    deinit {
        // Use nonisolated helper to avoid @MainActor isolation in deinit
        stopInternal()
    }

    // MARK: - Public Methods

    /// Start watching the queue files for changes
    public func start() {
        callbackQueue.sync { [weak self] in
            guard let self else { return }
            self.startInternal()
        }
    }

    /// Stop watching and clean up resources
    public func stop() {
        callbackQueue.sync { [weak self] in
            guard let self else { return }
            self.stopInternal()
        }
    }

    /// Update the working directory and restart watching if active
    public func updateWorkingDirectory(_ url: URL) {
        callbackQueue.sync { [weak self] in
            guard let self else { return }
            let wasWatching = self.stream != nil
            self.stopInternal()
            self.workingDirectoryURL = url
            if wasWatching {
                self.startInternal()
            }
        }
    }

    // MARK: - Internal Methods (called from callbackQueue.sync blocks)

    private nonisolated func startInternal() {
        guard stream == nil else { return }

        let queueDir = workingDirectoryURL.appendingPathComponent(".ralph")
        let pathsToWatch = [queueDir.path as NSString]

        // Create context with self reference
        var context = FSEventStreamContext(
            version: 0,
            info: Unmanaged.passUnretained(self).toOpaque(),
            retain: nil,
            release: nil,
            copyDescription: nil
        )

        // Create the event stream
        self.stream = FSEventStreamCreate(
            kCFAllocatorDefault,
            { (_, clientCallBackInfo, numEvents, eventPaths, eventFlags, _) in
                guard let info = clientCallBackInfo else { return }
                let watcher = Unmanaged<QueueFileWatcher>.fromOpaque(info).takeUnretainedValue()
                watcher.handleFSEvents(
                    numEvents: numEvents,
                    eventPaths: eventPaths,
                    eventFlags: eventFlags
                )
            },
            &context,
            pathsToWatch as CFArray,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            0.1,  // Latency in seconds
            FSEventStreamCreateFlags(
                kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagUseCFTypes)
        )

        guard let stream = self.stream else {
            print("[QueueFileWatcher] Failed to create FSEvent stream")
            return
        }

        // Use dispatch queue instead of run loop (modern approach)
        FSEventStreamSetDispatchQueue(stream, self.callbackQueue)

        guard FSEventStreamStart(stream) else {
            print("[QueueFileWatcher] Failed to start FSEvent stream")
            FSEventStreamInvalidate(stream)
            self.stream = nil
            return
        }

        // Update isWatching on main actor
        Task { @MainActor [weak self] in
            self?.isWatching = true
        }
        print("[QueueFileWatcher] Started watching \(queueDir.path)")
    }

    private nonisolated func stopInternal() {
        guard let stream = self.stream else { return }

        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        // Don't release since we're using dispatch queue

        self.stream = nil

        // Update isWatching on main actor
        Task { @MainActor [weak self] in
            self?.isWatching = false
        }

        // Clean up debounce work item
        lock.withLock {
            self.debounceWorkItem?.cancel()
            self.debounceWorkItem = nil
            self.pendingChanges.removeAll()
        }
    }

    // MARK: - Private Methods

    private nonisolated func handleFSEvents(
        numEvents: Int, eventPaths: UnsafeMutableRawPointer,
        eventFlags: UnsafePointer<FSEventStreamEventFlags>
    ) {
        guard let paths = Unmanaged<CFArray>.fromOpaque(eventPaths).takeUnretainedValue() as? [String]
        else {
            return
        }

        let relevantFiles = ["queue.json", "done.json"]
        var hasRelevantChange = false

        for i in 0..<numEvents {
            let path = paths[i]
            let flags = eventFlags[i]

            // Check if this is one of our target files
            guard relevantFiles.contains(where: { path.hasSuffix($0) }) else {
                continue
            }

            // Check for modification or creation events
            let isModified = (flags & UInt32(kFSEventStreamEventFlagItemModified)) != 0
            let isCreated = (flags & UInt32(kFSEventStreamEventFlagItemCreated)) != 0
            let isRenamed = (flags & UInt32(kFSEventStreamEventFlagItemRenamed)) != 0
            let isRemoved = (flags & UInt32(kFSEventStreamEventFlagItemRemoved)) != 0

            if isModified || isCreated || isRenamed || isRemoved {
                hasRelevantChange = true
                lock.withLock {
                    pendingChanges.insert(path)
                }
            }
        }

        if hasRelevantChange {
            scheduleDebouncedNotification()
        }
    }

    private nonisolated func scheduleDebouncedNotification() {
        // Cancel existing work item
        lock.withLock {
            debounceWorkItem?.cancel()
        }

        // Create new work item
        let workItem = DispatchWorkItem { [weak self] in
            guard let self else { return }

            self.lock.withLock {
                self.pendingChanges.removeAll()
            }

            // Notify on main actor
            Task { @MainActor [weak self] in
                self?.onFileChanged?()
            }
        }

        lock.withLock {
            debounceWorkItem = workItem
        }

        // Schedule after debounce interval
        callbackQueue.asyncAfter(deadline: .now() + debounceInterval, execute: workItem)
    }
}

// MARK: - NSLock Helper

private extension NSLock {
    func withLock<T>(_ body: () -> T) -> T {
        lock()
        defer { unlock() }
        return body()
    }
}
