/**
 QueueFileWatcher

 Purpose:
 - Monitor `.ralph/queue.jsonc`, `.ralph/done.jsonc`, and `.ralph/config.jsonc` for external changes using FSEvents.

 Responsibilities:
 - Monitor `.ralph/queue.jsonc`, `.ralph/done.jsonc`, and `.ralph/config.jsonc` for external changes using FSEvents.
 - Emit typed watcher health and file-change events through a single async event stream.
 - Retry FSEvent stream creation on transient failures without exposing unsafe shared mutable state.

 Does not handle:
 - Parsing or interpreting queue contents.
 - Main-actor UI updates.
 - Workspace retry/recovery policy beyond watcher startup retries.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

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

        var watchedFiles: [WatchedFileKind: URL] {
            var files: [WatchedFileKind: URL] = [
                .queue: queueFileURL,
                .done: doneFileURL,
            ]
            if let projectConfigFileURL {
                files[.config] = projectConfigFileURL
            }
            return files
        }

        var watchedDirectories: [URL] {
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
