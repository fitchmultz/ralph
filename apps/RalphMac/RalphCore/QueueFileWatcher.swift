/**
 QueueFileWatcher

 Responsibilities:
 - Monitor `.ralph/queue.{json,jsonc}`, `.ralph/done.{json,jsonc}`, and `.ralph/config.{json,jsonc}` for external changes using FSEvents.
 - Emit typed watcher health and file-change events through a single async event stream.
 - Retry FSEvent stream creation on transient failures without exposing unsafe shared mutable state.

 Does not handle:
 - Parsing or interpreting queue contents.
 - Main-actor UI updates.
 - Workspace retry/recovery policy beyond watcher startup retries.

 Invariants/assumptions callers must respect:
 - Call `start()` before consuming live file-change events.
 - Stop or drop the watcher when the owning workspace no longer needs observation.
 - Event consumers should treat `.failed` watcher health as a stale-data risk that needs surfaced remediation.
 */

public import Foundation
import CoreServices

public final class QueueFileWatcher: Sendable {
    public struct FileChangeBatch: Sendable, Equatable {
        public let fileNames: Set<String>

        public init(fileNames: Set<String>) {
            self.fileNames = fileNames
        }

        public var affectsQueueSnapshot: Bool {
            !fileNames.isDisjoint(with: ["queue.json", "queue.jsonc", "done.json", "done.jsonc"])
        }

        public var affectsRunnerConfiguration: Bool {
            !fileNames.isDisjoint(with: ["config.json", "config.jsonc"])
        }

        func merged(with other: FileChangeBatch) -> FileChangeBatch {
            FileChangeBatch(fileNames: fileNames.union(other.fileNames))
        }
    }

    public enum Event: Sendable, Equatable {
        case healthChanged(QueueWatcherHealth)
        case filesChanged(FileChangeBatch)
    }

    public struct Configuration: Sendable {
        public let debounceInterval: Duration
        public let retryBaseDelay: Duration
        public let maxStartAttempts: Int
        public let streamLatency: CFTimeInterval

        public init(
            debounceInterval: Duration = .milliseconds(500),
            retryBaseDelay: Duration = .milliseconds(500),
            maxStartAttempts: Int = 3,
            streamLatency: CFTimeInterval = 0.1
        ) {
            self.debounceInterval = debounceInterval
            self.retryBaseDelay = retryBaseDelay
            self.maxStartAttempts = maxStartAttempts
            self.streamLatency = streamLatency
        }
    }

    struct StreamSystem: Sendable {
        let create: @Sendable (
            FSEventStreamCallback,
            UnsafeMutablePointer<FSEventStreamContext>,
            [NSString],
            CFTimeInterval,
            FSEventStreamCreateFlags
        ) -> FSEventStreamRef?
        let setDispatchQueue: @Sendable (FSEventStreamRef, DispatchQueue) -> Void
        let start: @Sendable (FSEventStreamRef) -> Bool
        let stop: @Sendable (FSEventStreamRef) -> Void
        let invalidate: @Sendable (FSEventStreamRef) -> Void

        static let live = StreamSystem(
            create: { callback, context, paths, latency, flags in
                FSEventStreamCreate(
                    kCFAllocatorDefault,
                    callback,
                    context,
                    paths as CFArray,
                    FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
                    latency,
                    flags
                )
            },
            setDispatchQueue: { stream, queue in
                FSEventStreamSetDispatchQueue(stream, queue)
            },
            start: { stream in
                FSEventStreamStart(stream)
            },
            stop: { stream in
                FSEventStreamStop(stream)
            },
            invalidate: { stream in
                FSEventStreamInvalidate(stream)
            }
        )
    }

    public let events: AsyncStream<Event>

    private let continuation: AsyncStream<Event>.Continuation
    private let runtime: QueueFileWatcherRuntime

    public convenience init(
        workingDirectoryURL: URL,
        configuration: Configuration = Configuration()
    ) {
        self.init(
            workingDirectoryURL: workingDirectoryURL,
            configuration: configuration,
            system: .live
        )
    }

    init(
        workingDirectoryURL: URL,
        configuration: Configuration,
        system: StreamSystem
    ) {
        let stream = AsyncStream.makeStream(of: Event.self)
        self.events = stream.stream
        self.continuation = stream.continuation
        self.runtime = QueueFileWatcherRuntime(
            workingDirectoryURL: workingDirectoryURL,
            configuration: configuration,
            system: system,
            emit: { stream.continuation.yield($0) }
        )
    }

    deinit {
        let runtime = self.runtime
        continuation.finish()
        Task {
            await runtime.stop()
        }
    }

    public func start() async {
        await runtime.start()
    }

    public func stop() async {
        await runtime.stop()
    }

    public func updateWorkingDirectory(_ url: URL) async {
        await runtime.updateWorkingDirectory(url)
    }
}

private final class CallbackContext: @unchecked Sendable {
    let forward: @Sendable ([String], [FSEventStreamEventFlags]) -> Void

    init(forward: @escaping @Sendable ([String], [FSEventStreamEventFlags]) -> Void) {
        self.forward = forward
    }
}

private actor QueueFileWatcherRuntime {
    private static let callback: FSEventStreamCallback = { _, info, numEvents, eventPaths, eventFlags, _ in
        guard let info else { return }
        let context = Unmanaged<CallbackContext>.fromOpaque(info).takeUnretainedValue()
        guard let paths = Unmanaged<CFArray>.fromOpaque(eventPaths).takeUnretainedValue() as? [String] else {
            return
        }
        let flags = (0..<numEvents).map { eventFlags[$0] }
        context.forward(paths, flags)
    }

    private static let retainCallback: CFAllocatorRetainCallBack = { info in
        guard let info else { return nil }
        _ = Unmanaged<CallbackContext>.fromOpaque(info).retain()
        return UnsafeRawPointer(info)
    }

    private static let releaseCallback: CFAllocatorReleaseCallBack = { info in
        guard let info else { return }
        Unmanaged<CallbackContext>.fromOpaque(info).release()
    }

    private let configuration: QueueFileWatcher.Configuration
    private let system: QueueFileWatcher.StreamSystem
    private let callbackQueue: DispatchQueue
    private let emit: @Sendable (QueueFileWatcher.Event) -> Void
    private let relevantFiles = Set([
        "queue.json", "queue.jsonc",
        "done.json", "done.jsonc",
        "config.json", "config.jsonc",
    ])

    private var workingDirectoryURL: URL
    private var stream: FSEventStreamRef?
    private var callbackContext: CallbackContext?
    private var shouldWatch = false
    private var pendingChanges = Set<String>()
    private var debounceTask: Task<Void, Never>?
    private var retryTask: Task<Void, Never>?
    private var startAttempts = 0

    init(
        workingDirectoryURL: URL,
        configuration: QueueFileWatcher.Configuration,
        system: QueueFileWatcher.StreamSystem,
        emit: @escaping @Sendable (QueueFileWatcher.Event) -> Void
    ) {
        self.workingDirectoryURL = workingDirectoryURL
        self.configuration = configuration
        self.system = system
        self.callbackQueue = DispatchQueue(
            label: "com.mitchfultz.ralph.filewatcher.\(workingDirectoryURL.lastPathComponent)"
        )
        self.emit = emit
    }

    func start() {
        guard !shouldWatch else { return }
        shouldWatch = true
        emitHealth(.starting(attempt: max(startAttempts + 1, 1)))
        attemptStart()
    }

    func stop() {
        shouldWatch = false
        startAttempts = 0
        retryTask?.cancel()
        retryTask = nil
        debounceTask?.cancel()
        debounceTask = nil
        pendingChanges.removeAll()

        if let stream {
            system.stop(stream)
            system.invalidate(stream)
        }
        stream = nil
        callbackContext = nil
        emitHealth(.stopped)
    }

    func updateWorkingDirectory(_ url: URL) {
        let wasWatching = shouldWatch
        stop()
        workingDirectoryURL = url
        if wasWatching {
            start()
        } else {
            emitHealth(.idle)
        }
    }

    private func attemptStart() {
        guard shouldWatch, stream == nil else { return }

        let queueDir = workingDirectoryURL.appendingPathComponent(".ralph", isDirectory: true)
        let context = CallbackContext { [weak self] paths, flags in
            guard let self else { return }
            Task {
                await self.handleFSEvents(paths: paths, flags: flags)
            }
        }
        callbackContext = context

        var streamContext = FSEventStreamContext(
            version: 0,
            info: Unmanaged.passRetained(context).toOpaque(),
            retain: Self.retainCallback,
            release: Self.releaseCallback,
            copyDescription: nil
        )

        let flags = FSEventStreamCreateFlags(
            kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagUseCFTypes
        )
        guard let createdStream = system.create(
            Self.callback,
            &streamContext,
            [queueDir.path as NSString],
            configuration.streamLatency,
            flags
        ) else {
            handleStartFailure(reason: "Failed to create FSEvent stream")
            return
        }

        system.setDispatchQueue(createdStream, callbackQueue)
        guard system.start(createdStream) else {
            system.invalidate(createdStream)
            handleStartFailure(reason: "Failed to start FSEvent stream")
            return
        }

        stream = createdStream
        startAttempts = 0
        retryTask?.cancel()
        retryTask = nil
        emitHealth(.watching)
        RalphLogger.shared.info("Started watching \(queueDir.path)", category: .fileWatching)
    }

    private func handleStartFailure(reason: String) {
        stream = nil
        callbackContext = nil
        startAttempts += 1

        guard shouldWatch else { return }
        guard startAttempts < configuration.maxStartAttempts else {
            emitHealth(.failed(reason: reason, attempts: startAttempts))
            RalphLogger.shared.error(
                "Queue watcher failed after \(startAttempts) attempts: \(reason)",
                category: .fileWatching
            )
            return
        }

        let delay = configuration.retryBaseDelay * startAttempts
        let nextRetryAt = Date().addingTimeInterval(delay.timeInterval)
        emitHealth(.degraded(reason: reason, retryCount: startAttempts, nextRetryAt: nextRetryAt))
        RalphLogger.shared.info(
            "Queue watcher retrying after failure: \(reason) (attempt \(startAttempts)/\(configuration.maxStartAttempts))",
            category: .fileWatching
        )

        retryTask?.cancel()
        retryTask = Task { [weak self] in
            guard let self else { return }
            try? await Task.sleep(for: delay)
            await self.attemptStart()
        }
    }

    private func handleFSEvents(
        paths: [String],
        flags: [FSEventStreamEventFlags]
    ) {
        guard shouldWatch, stream != nil else { return }

        for (path, flag) in zip(paths, flags) {
            let fileName = URL(fileURLWithPath: path).lastPathComponent
            guard relevantFiles.contains(fileName) else { continue }
            guard isRelevantChange(flag) else { continue }
            pendingChanges.insert(fileName)
        }

        guard !pendingChanges.isEmpty else { return }
        scheduleDebounce()
    }

    private func scheduleDebounce() {
        debounceTask?.cancel()
        debounceTask = Task { [weak self] in
            guard let self else { return }
            try? await Task.sleep(for: configuration.debounceInterval)
            await self.flushPendingChanges()
        }
    }

    private func flushPendingChanges() {
        guard shouldWatch, !pendingChanges.isEmpty else { return }
        let batch = QueueFileWatcher.FileChangeBatch(fileNames: pendingChanges)
        pendingChanges.removeAll()
        emit(.filesChanged(batch))
    }

    private func emitHealth(_ state: QueueWatcherHealth.State) {
        emit(.healthChanged(QueueWatcherHealth(state: state, workingDirectoryURL: workingDirectoryURL)))
    }

    private func isRelevantChange(_ flag: FSEventStreamEventFlags) -> Bool {
        (flag & UInt32(kFSEventStreamEventFlagItemModified)) != 0
            || (flag & UInt32(kFSEventStreamEventFlagItemCreated)) != 0
            || (flag & UInt32(kFSEventStreamEventFlagItemRenamed)) != 0
            || (flag & UInt32(kFSEventStreamEventFlagItemRemoved)) != 0
    }
}

private extension Duration {
    static func * (lhs: Duration, rhs: Int) -> Duration {
        lhs * Double(rhs)
    }

    static func * (lhs: Duration, rhs: Double) -> Duration {
        .seconds(lhs.timeInterval * rhs)
    }

    var timeInterval: TimeInterval {
        let components = components
        let seconds = Double(components.seconds)
        let attoseconds = Double(components.attoseconds) / 1_000_000_000_000_000_000
        return seconds + attoseconds
    }
}
