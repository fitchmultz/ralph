/**
 QueueFileWatcher

 Responsibilities:
 - Monitor `.ralph/queue.jsonc`, `.ralph/done.jsonc`, and `.ralph/config.jsonc` for external changes using FSEvents.
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
    public struct WatchTargets: Sendable, Equatable {
        public let workingDirectoryURL: URL
        public let queueFileURL: URL
        public let doneFileURL: URL
        public let projectConfigFileURL: URL?

        public init(
            workingDirectoryURL: URL,
            queueFileURL: URL,
            doneFileURL: URL,
            projectConfigFileURL: URL?
        ) {
            self.workingDirectoryURL = Self.normalize(workingDirectoryURL)
            self.queueFileURL = Self.normalize(queueFileURL)
            self.doneFileURL = Self.normalize(doneFileURL)
            self.projectConfigFileURL = projectConfigFileURL.map(Self.normalize)
        }

        public static func `default`(for workingDirectoryURL: URL) -> WatchTargets {
            let baseURL = normalize(workingDirectoryURL)
            return WatchTargets(
                workingDirectoryURL: baseURL,
                queueFileURL: baseURL.appendingPathComponent(".ralph/queue.jsonc", isDirectory: false),
                doneFileURL: baseURL.appendingPathComponent(".ralph/done.jsonc", isDirectory: false),
                projectConfigFileURL: baseURL.appendingPathComponent(".ralph/config.jsonc", isDirectory: false)
            )
        }

        fileprivate var watchedFiles: [WatchedFileKind: URL] {
            var files: [WatchedFileKind: URL] = [
                .queue: queueFileURL,
                .done: doneFileURL,
            ]
            if let projectConfigFileURL {
                files[.config] = projectConfigFileURL
            }
            return files
        }

        fileprivate var watchedDirectories: [URL] {
            Array(Set(watchedFiles.values.map { $0.deletingLastPathComponent().standardizedFileURL }))
                .sorted { $0.path < $1.path }
        }

        private static func normalize(_ url: URL) -> URL {
            url.standardizedFileURL.resolvingSymlinksInPath()
        }
    }

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
            targets: .default(for: workingDirectoryURL),
            configuration: configuration,
            system: .live
        )
    }

    convenience init(
        targets: WatchTargets,
        configuration: Configuration = Configuration()
    ) {
        self.init(
            targets: targets,
            configuration: configuration,
            system: .live
        )
    }

    convenience init(
        workingDirectoryURL: URL,
        configuration: Configuration,
        system: StreamSystem
    ) {
        self.init(
            targets: .default(for: workingDirectoryURL),
            configuration: configuration,
            system: system
        )
    }

    init(
        targets: WatchTargets,
        configuration: Configuration,
        system: StreamSystem
    ) {
        let stream = AsyncStream.makeStream(of: Event.self)
        self.events = stream.stream
        self.continuation = stream.continuation
        self.runtime = QueueFileWatcherRuntime(
            targets: targets,
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
        guard !Task.isCancelled else { return }
        await runtime.start()
    }

    public func stop() async {
        await runtime.stop()
    }

    public func updateWorkingDirectory(_ url: URL) async {
        await runtime.updateTargets(.default(for: url))
    }

    public func updateTargets(_ targets: WatchTargets) async {
        await runtime.updateTargets(targets)
    }
}

private enum WatchedFileKind: Sendable, CaseIterable {
    case queue
    case done
    case config
}

private struct WatchedFileSignature: Equatable, Sendable {
    let exists: Bool
    let fileSize: UInt64?
    let modificationDate: Date?
    let inode: UInt64?

    static func current(at url: URL) -> WatchedFileSignature {
        do {
            let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
            let fileSize = (attributes[.size] as? NSNumber)?.uint64Value
            let modificationDate = attributes[.modificationDate] as? Date
            let inode = (attributes[.systemFileNumber] as? NSNumber)?.uint64Value
            return WatchedFileSignature(
                exists: true,
                fileSize: fileSize,
                modificationDate: modificationDate,
                inode: inode
            )
        } catch {
            return WatchedFileSignature(
                exists: false,
                fileSize: nil,
                modificationDate: nil,
                inode: nil
            )
        }
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
    private var targets: QueueFileWatcher.WatchTargets
    private var stream: FSEventStreamRef?
    private var callbackContext: CallbackContext?
    private var shouldWatch = false
    private var pendingChanges = Set<String>()
    private var debounceTask: Task<Void, Never>?
    private var retryTask: Task<Void, Never>?
    private var startAttempts = 0
    private var lastKnownSignatures: [WatchedFileKind: WatchedFileSignature]

    init(
        targets: QueueFileWatcher.WatchTargets,
        configuration: QueueFileWatcher.Configuration,
        system: QueueFileWatcher.StreamSystem,
        emit: @escaping @Sendable (QueueFileWatcher.Event) -> Void
    ) {
        self.targets = targets
        self.configuration = configuration
        self.system = system
        self.callbackQueue = DispatchQueue(
            label: "com.mitchfultz.ralph.filewatcher.\(targets.workingDirectoryURL.lastPathComponent)"
        )
        self.emit = emit
        self.lastKnownSignatures = Self.captureSignatures(for: targets.watchedFiles)
    }

    func start() {
        guard !Task.isCancelled else { return }
        guard !shouldWatch else { return }
        shouldWatch = true
        lastKnownSignatures = Self.captureSignatures(for: targets.watchedFiles)
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
        lastKnownSignatures = Self.captureSignatures(for: targets.watchedFiles)

        if let stream {
            system.stop(stream)
            system.invalidate(stream)
        }
        stream = nil
        callbackContext = nil
        emitHealth(.stopped)
    }

    func updateTargets(_ targets: QueueFileWatcher.WatchTargets) {
        guard self.targets != targets else {
            if !shouldWatch {
                emitHealth(.idle)
            }
            return
        }

        let wasWatching = shouldWatch
        stop()
        self.targets = targets
        lastKnownSignatures = Self.captureSignatures(for: targets.watchedFiles)
        if wasWatching {
            start()
        } else {
            emitHealth(.idle)
        }
    }

    private func attemptStart() {
        guard shouldWatch, stream == nil else { return }

        let watchedDirectories = targets.watchedDirectories
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
            watchedDirectories.map(\.path) as [NSString],
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
        RalphLogger.shared.info(
            "Started watching \(watchedDirectories.map(\.path).joined(separator: ", "))",
            category: .fileWatching
        )
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
            guard await self.sleepUnlessCancelled(for: delay) else { return }
            await self.attemptStart()
        }
    }

    private func handleFSEvents(
        paths: [String],
        flags: [FSEventStreamEventFlags]
    ) {
        guard shouldWatch, stream != nil else { return }
        let changedKinds = changedFiles(paths: paths, flags: flags)
        guard !changedKinds.isEmpty else { return }
        pendingChanges.formUnion(changedKinds.map { fileName(for: $0) })
        scheduleDebounce()
    }

    private func scheduleDebounce() {
        debounceTask?.cancel()
        debounceTask = Task { [weak self] in
            guard let self else { return }
            guard await self.sleepUnlessCancelled(for: configuration.debounceInterval) else { return }
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
        emit(.healthChanged(QueueWatcherHealth(state: state, workingDirectoryURL: targets.workingDirectoryURL)))
    }

    private func isRelevantChange(_ flag: FSEventStreamEventFlags) -> Bool {
        (flag & UInt32(kFSEventStreamEventFlagItemModified)) != 0
            || (flag & UInt32(kFSEventStreamEventFlagItemCreated)) != 0
            || (flag & UInt32(kFSEventStreamEventFlagItemRenamed)) != 0
            || (flag & UInt32(kFSEventStreamEventFlagItemRemoved)) != 0
    }

    private func changedFiles(
        paths: [String],
        flags: [FSEventStreamEventFlags]
    ) -> Set<WatchedFileKind> {
        var changedKinds = Set<WatchedFileKind>()
        var requiresSignatureScan = false

        for (path, flag) in zip(paths, flags) {
            guard isRelevantChange(flag) || requiresSignatureValidation(flag) else { continue }

            let eventURL = URL(fileURLWithPath: path, isDirectory: false)
                .standardizedFileURL
                .resolvingSymlinksInPath()

            if let directKind = watchedFileKind(for: eventURL), isRelevantChange(flag) {
                changedKinds.insert(directKind)
            }

            if isWithinWatchedDirectory(eventURL) {
                requiresSignatureScan = true
            }
        }

        if requiresSignatureScan {
            changedKinds.formUnion(scanForSignatureChanges())
        } else if !changedKinds.isEmpty {
            refreshSignatures(for: changedKinds)
        }

        return changedKinds
    }

    private func watchedFileKind(for url: URL) -> WatchedFileKind? {
        targets.watchedFiles.first { $0.value == url }?.key
    }

    private func isWithinWatchedDirectory(_ url: URL) -> Bool {
        let standardizedURL = url.standardizedFileURL.resolvingSymlinksInPath()
        return targets.watchedDirectories.contains { directoryURL in
            isSameOrDescendantPath(standardizedURL.path, of: directoryURL.path)
        }
    }

    private func requiresSignatureValidation(_ flag: FSEventStreamEventFlags) -> Bool {
        (flag & UInt32(kFSEventStreamEventFlagMustScanSubDirs)) != 0
            || (flag & UInt32(kFSEventStreamEventFlagRootChanged)) != 0
            || (flag & UInt32(kFSEventStreamEventFlagMount)) != 0
            || (flag & UInt32(kFSEventStreamEventFlagUnmount)) != 0
    }

    private func scanForSignatureChanges() -> Set<WatchedFileKind> {
        let currentSignatures = Self.captureSignatures(for: targets.watchedFiles)
        let changedKinds = Set(currentSignatures.compactMap { kind, signature in
            lastKnownSignatures[kind] == signature ? nil : kind
        })
        lastKnownSignatures = currentSignatures
        return changedKinds
    }

    private func refreshSignatures(for kinds: Set<WatchedFileKind>) {
        for kind in kinds {
            guard let url = targets.watchedFiles[kind] else { continue }
            lastKnownSignatures[kind] = WatchedFileSignature.current(at: url)
        }
    }

    private func fileName(for kind: WatchedFileKind) -> String {
        targets.watchedFiles[kind]?.lastPathComponent ?? kind.defaultFileName
    }

    private static func captureSignatures(
        for watchedFiles: [WatchedFileKind: URL]
    ) -> [WatchedFileKind: WatchedFileSignature] {
        watchedFiles.reduce(into: [WatchedFileKind: WatchedFileSignature]()) { result, entry in
            result[entry.key] = WatchedFileSignature.current(at: entry.value)
        }
    }

    private func sleepUnlessCancelled(for duration: Duration) async -> Bool {
        do {
            try await Task.sleep(for: duration)
            return !Task.isCancelled
        } catch is CancellationError {
            return false
        } catch {
            emitHealth(.degraded(
                reason: "Queue watcher timer failed: \(error.localizedDescription)",
                retryCount: startAttempts,
                nextRetryAt: nil
            ))
            RalphLogger.shared.error(
                "Queue watcher timer failed: \(error.localizedDescription)",
                category: .fileWatching
            )
            return false
        }
    }

    private func isSameOrDescendantPath(_ candidatePath: String, of directoryPath: String) -> Bool {
        guard candidatePath == directoryPath || candidatePath.hasPrefix(directoryPath + "/") else {
            return false
        }
        return true
    }
}

private extension WatchedFileKind {
    var defaultFileName: String {
        switch self {
        case .queue:
            return "queue.jsonc"
        case .done:
            return "done.jsonc"
        case .config:
            return "config.jsonc"
        }
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
