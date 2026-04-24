/**
 RalphCLIRun

 Purpose:
 - Own a single running Ralph CLI subprocess.

 Responsibilities:
 - Own a single running Ralph CLI subprocess.
 - Bridge pipe readability callbacks into async event streams.
 - Coordinate cooperative termination and final exit-status delivery.

 Does not handle:
 - Building commands to execute.
 - Retry policies or health checks.
 - Parsing streamed output into domain models.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Instances are created only by `RalphCLIClient.start(...)`.
 - Cancellation is best-effort: interrupt first, then escalate to terminate and hard-kill after the grace period on Darwin.
 - Event streams finish only after process termination and both pipes reach EOF.
 */

import Foundation

#if canImport(Darwin)
import Darwin
#endif

public actor RalphCLIRun {
    public let events: AsyncStream<RalphCLIEvent>

    nonisolated func requestCancel(gracePeriod: TimeInterval = 2) {
        Task { [weak self] in
            await self?.cancel(gracePeriod: gracePeriod)
        }
    }

    private let ioQueue: DispatchQueue
    private let process: Process
    private let stdoutHandle: FileHandle
    private let stderrHandle: FileHandle

    private var eventsContinuation: AsyncStream<RalphCLIEvent>.Continuation?
    private var didRequestCancel = false
    private var didFinishEvents = false
    private var didTerminateProcess = false
    private var didEscalateTermination = false
    private var stdoutClosed = false
    private var stderrClosed = false
    private var exitStatus: RalphCLIExitStatus?
    private var exitWaiters: [CheckedContinuation<RalphCLIExitStatus, Never>] = []

    internal init(
        ioQueue: DispatchQueue,
        process: Process,
        stdoutHandle: FileHandle,
        stderrHandle: FileHandle
    ) {
        self.ioQueue = ioQueue
        self.process = process
        self.stdoutHandle = stdoutHandle
        self.stderrHandle = stderrHandle

        var continuation: AsyncStream<RalphCLIEvent>.Continuation?
        let stream = AsyncStream<RalphCLIEvent> { cont in
            continuation = cont
        }
        events = stream
        eventsContinuation = continuation
        eventsContinuation?.onTermination = { @Sendable [weak self] _ in
            self?.requestCancel()
        }

        setupIOHandlers()
    }

    deinit {
        requestCancel()
    }

    public func cancel() {
        cancel(gracePeriod: 2)
    }

    func cancel(gracePeriod: TimeInterval) {
        guard !didRequestCancel else { return }
        didRequestCancel = true

        guard process.isRunning else { return }
        process.interrupt()

        #if canImport(Darwin)
        let pid = process.processIdentifier
        ioQueue.asyncAfter(deadline: .now() + gracePeriod) { [weak self] in
            guard let self else { return }
            Task { [weak self] in
                await self?.terminateIfStillRunning()
            }
        }
        ioQueue.asyncAfter(deadline: .now() + (gracePeriod * 2)) { [weak self] in
            guard let self else { return }
            Task { [weak self] in
                await self?.killIfStillRunning(pid: pid)
            }
        }
        #endif
    }

    private func terminateIfStillRunning() {
        guard process.isRunning else { return }
        guard !didEscalateTermination else { return }
        didEscalateTermination = true
        process.terminate()
    }

    #if canImport(Darwin)
    private func killIfStillRunning(pid: pid_t) {
        guard process.isRunning else { return }
        _ = kill(pid, SIGKILL)
    }
    #endif

    public func waitUntilExit() async -> RalphCLIExitStatus {
        if let existing = exitStatus {
            return existing
        }

        return await withCheckedContinuation { cont in
            if let existing = exitStatus {
                cont.resume(returning: existing)
                return
            }
            exitWaiters.append(cont)
        }
    }

    private nonisolated func setupIOHandlers() {
        stdoutHandle.readabilityHandler = { [weak self] handle in
            guard let self else { return }
            Task {
                await self.handleReadable(stream: .stdout, handle: handle)
            }
        }

        stderrHandle.readabilityHandler = { [weak self] handle in
            guard let self else { return }
            Task {
                await self.handleReadable(stream: .stderr, handle: handle)
            }
        }

        process.terminationHandler = { [weak self] process in
            guard let self else { return }
            Task {
                await self.handleTermination(process: process)
            }
        }
    }

    private func handleReadable(stream: RalphCLIEvent.Stream, handle: FileHandle) {
        let data = handle.availableData
        if data.isEmpty {
            handle.readabilityHandler = nil

            switch stream {
            case .stdout:
                stdoutClosed = true
            case .stderr:
                stderrClosed = true
            }

            finishIfComplete()
            return
        }

        eventsContinuation?.yield(RalphCLIEvent(stream: stream, data: data))
    }

    private func handleTermination(process: Process) {
        didTerminateProcess = true
        RalphLogger.shared.debug("CLI process terminated with status: \(process.terminationStatus)", category: .cli)

        let reason: RalphCLIExitStatus.TerminationReason
        switch process.terminationReason {
        case .exit:
            reason = .exit
        case .uncaughtSignal:
            reason = .uncaughtSignal
        @unknown default:
            reason = .exit
        }

        let status = RalphCLIExitStatus(code: process.terminationStatus, reason: reason)
        stdoutHandle.readabilityHandler = nil
        stderrHandle.readabilityHandler = nil

        let remainingStdout = stdoutHandle.readDataToEndOfFile()
        if !remainingStdout.isEmpty {
            eventsContinuation?.yield(RalphCLIEvent(stream: .stdout, data: remainingStdout))
        }

        let remainingStderr = stderrHandle.readDataToEndOfFile()
        if !remainingStderr.isEmpty {
            eventsContinuation?.yield(RalphCLIEvent(stream: .stderr, data: remainingStderr))
        }

        stdoutClosed = true
        stderrClosed = true

        if exitStatus == nil {
            exitStatus = status
            let waiters = exitWaiters
            exitWaiters.removeAll(keepingCapacity: false)
            for waiter in waiters {
                waiter.resume(returning: status)
            }
        }

        finishIfComplete()
    }

    private func finishIfComplete() {
        guard didTerminateProcess, stdoutClosed, stderrClosed else { return }
        guard !didFinishEvents else { return }

        didFinishEvents = true
        eventsContinuation?.finish()
        eventsContinuation = nil
    }
}
