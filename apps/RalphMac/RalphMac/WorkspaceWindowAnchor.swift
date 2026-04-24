/**
 WorkspaceWindowAnchor

 Purpose:
 - Apply minimum window geometry and deterministic UI-testing placement.

 Responsibilities:
 - Apply minimum window geometry and deterministic UI-testing placement.
 - Enforce UI-testing window-count policy via explicit coordinator state.
 - Route workspace-window reveal behavior through the shared presentation runtime so contract launches stay offscreen.

 Does not handle:
 - Workspace scene restoration or selection.
 - Main app command routing.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - UI-testing launches target one window by default and two windows for multiwindow runs.
 - Noninteractive macOS contract launches must never activate the app or flash a workspace window onscreen.
 - Window sizing should preserve NavigationSplitView usability.
 */

import SwiftUI
import AppKit

@MainActor
final class UITestingWindowCoordinator {
    static let shared = UITestingWindowCoordinator()

    private let isUITestingLaunch = ProcessInfo.processInfo.arguments.contains("--uitesting")
    private let isUITestingMultiwindowLaunch = ProcessInfo.processInfo.arguments.contains("--uitesting-multiwindow")
    private var didOpenSecondaryWindow = false
    private var isConfigured = false
    private var observerTokens: [any NSObjectProtocol] = []
    private var enforcementTask: Task<Void, Never>?
    private var closingWindowNumbers = Set<Int>()
    private var isClosingWindows = false

    private init() {}

    func configureIfNeeded() {
        guard isUITestingLaunch, !isConfigured else { return }
        isConfigured = true

        let center = NotificationCenter.default
        observerTokens = [
            center.addObserver(
                forName: NSWindow.didBecomeKeyNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.scheduleWindowCountEnforcement()
                }
            },
            center.addObserver(
                forName: NSWindow.willCloseNotification,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let window = notification.object as? NSWindow else { return }
                MainActor.assumeIsolated {
                    self?.closingWindowNumbers.insert(window.windowNumber)
                    self?.scheduleWindowCountEnforcement()
                }
            }
        ]

        scheduleWindowCountEnforcement()
    }

    func openAdditionalWindowIfNeeded(openWindow: OpenWindowAction) {
        guard isUITestingMultiwindowLaunch else { return }
        guard !didOpenSecondaryWindow else { return }

        didOpenSecondaryWindow = true
        openWindow(id: "main")
    }

    func scheduleWindowCountEnforcement() {
        guard isUITestingLaunch else { return }

        enforcementTask?.cancel()
        enforcementTask = Task { @MainActor [weak self] in
            await Task.yield()
            guard let self, !Task.isCancelled else { return }
            self.enforceExpectedWindowCount()
            self.enforcementTask = nil
        }
    }

    func enforceExpectedWindowCount() {
        guard isUITestingLaunch else { return }
        guard !isClosingWindows else { return }

        let expectedWindowCount = isUITestingMultiwindowLaunch ? 2 : 1
        let workspaceWindows = currentWorkspaceWindows()

        guard workspaceWindows.count > expectedWindowCount else {
            closingWindowNumbers.formIntersection(Set(workspaceWindows.map(\.windowNumber)))
            return
        }

        isClosingWindows = true
        defer { isClosingWindows = false }

        for window in workspaceWindows.dropFirst(expectedWindowCount) {
            closingWindowNumbers.insert(window.windowNumber)
            window.close()
        }
    }

    private func currentWorkspaceWindows() -> [NSWindow] {
        let registeredWindows = WorkspaceWindowRegistry.shared.workspaceWindows()
            .filter { !closingWindowNumbers.contains($0.windowNumber) }
        if !registeredWindows.isEmpty {
            return registeredWindows
        }

        return NSApp.windows
            .filter {
                $0.identifier?.rawValue.contains("AppWindow") == true
                    && !closingWindowNumbers.contains($0.windowNumber)
            }
            .sorted { $0.windowNumber < $1.windowNumber }
    }

    deinit {
        enforcementTask?.cancel()
    }
}

struct WorkspaceWindowAnchor: NSViewRepresentable {
    let minimumSize: NSSize
    let uiTestingEnabled: Bool
    let onWindowResolved: (NSWindow) -> Void

    final class Coordinator {
        private var resolvedWindowNumbers = Set<Int>()
        private var configurationTask: Task<Void, Never>?

        @MainActor
        func markResolved(_ window: NSWindow) -> Bool {
            resolvedWindowNumbers.insert(window.windowNumber).inserted
        }

        @MainActor
        func scheduleConfiguration(for view: NSView, anchor: WorkspaceWindowAnchor) {
            configurationTask?.cancel()
            configurationTask = Task { @MainActor [weak view] in
                defer { self.configurationTask = nil }

                for attempt in 0..<60 {
                    guard !Task.isCancelled else { return }
                    if let window = view?.window {
                        anchor.configure(window: window, coordinator: self)
                        return
                    }

                    if attempt < 10 {
                        await Task.yield()
                    } else {
                        try? await Task.sleep(nanoseconds: 20_000_000)
                    }
                }
            }
        }

        deinit {
            configurationTask?.cancel()
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> NSView {
        NSView(frame: .zero)
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        context.coordinator.scheduleConfiguration(for: nsView, anchor: self)
    }

    private func configure(window: NSWindow, coordinator: Coordinator) {
        applyMinimumSize(to: window)
        configureWindowBehavior(window)
        onWindowResolved(window)

        if uiTestingEnabled {
            applyUITestingPlacement(to: window)
            UITestingWindowCoordinator.shared.enforceExpectedWindowCount()
            return
        }

        if coordinator.markResolved(window) {
            applyInitialPlacement(to: window)
        } else {
            stabilizeVisiblePlacement(for: window)
        }
    }

    private func configureWindowBehavior(_ window: NSWindow) {
        window.collectionBehavior.insert(.moveToActiveSpace)
        window.tabbingMode = .disallowed
    }

    private func stabilizeVisiblePlacement(for window: NSWindow) {
        guard requiresVisibleFrameReset(window) else { return }

        RalphMacPresentationRuntime.applyRevealFrame(centeredFrame(for: window), to: window)
    }

    private func applyInitialPlacement(to window: NSWindow) {
        RalphMacPresentationRuntime.applyRevealFrame(
            centeredFrame(for: window, preferredScreen: preferredLaunchScreen()),
            to: window
        )
    }

    private func requiresVisibleFrameReset(_ window: NSWindow) -> Bool {
        let currentFrame = window.frame
        guard currentFrame.width > 0, currentFrame.height > 0 else { return true }

        guard let activeVisibleFrame = (NSScreen.main ?? window.screen ?? NSScreen.screens.first)?.visibleFrame else {
            return false
        }

        let intersection = currentFrame.intersection(activeVisibleFrame)
        guard !intersection.isNull else { return true }
        return intersection.width < minimumVisibleWidth(for: currentFrame)
            || intersection.height < minimumVisibleHeight(for: currentFrame)
    }

    private func minimumVisibleWidth(for frame: NSRect) -> CGFloat {
        max(240, min(frame.width * 0.4, frame.width))
    }

    private func minimumVisibleHeight(for frame: NSRect) -> CGFloat {
        max(180, min(frame.height * 0.4, frame.height))
    }

    private func centeredFrame(for window: NSWindow, preferredScreen: NSScreen? = nil) -> NSRect {
        let targetVisibleFrame = (preferredScreen ?? window.screen ?? NSScreen.main ?? NSScreen.screens.first)?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let minimumFrameSize = window.frameRect(forContentRect: NSRect(origin: .zero, size: minimumSize)).size
        let width = max(minimumFrameSize.width, min(1400, targetVisibleFrame.width - 80))
        let height = max(minimumFrameSize.height, min(900, targetVisibleFrame.height - 80))
        return NSRect(
            x: targetVisibleFrame.midX - (width / 2),
            y: targetVisibleFrame.midY - (height / 2),
            width: width,
            height: height
        )
    }

    private func applyUITestingPlacement(to window: NSWindow) {
        let screen = window.screen ?? NSScreen.main
        let visibleFrame = screen?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let workspaceWindows = visibleWorkspaceWindows()

        let windowIndex = workspaceWindows.firstIndex(of: window) ?? 0
        let isMultiwindowLaunch = ProcessInfo.processInfo.arguments.contains("--uitesting-multiwindow")
        let expectedWindowCount = isMultiwindowLaunch ? 2 : 1
        let windowCount = max(min(workspaceWindows.count, expectedWindowCount), 1)
        let horizontalSpacing: CGFloat = 24
        let verticalInset: CGFloat = 40
        let minimumFrameSize = window.frameRect(forContentRect: NSRect(origin: .zero, size: minimumSize)).size

        let width: CGFloat
        let height = max(minimumFrameSize.height, min(900, visibleFrame.height - (verticalInset * 2)))
        let origin: NSPoint

        if windowCount > 1 {
            let preferredWidth = max(minimumFrameSize.width, min(1200, visibleFrame.width - 120))
            let sideBySideWidth = (visibleFrame.width - (horizontalSpacing * 3)) / 2

            if sideBySideWidth >= minimumFrameSize.width {
                width = min(preferredWidth, sideBySideWidth)
                let x = visibleFrame.minX + horizontalSpacing + CGFloat(min(windowIndex, 1)) * (width + horizontalSpacing)
                let y = visibleFrame.maxY - verticalInset - height
                origin = NSPoint(x: x, y: y)
            } else {
                width = preferredWidth
                let cascadeOffset = CGFloat(windowIndex) * 72
                let x = min(
                    visibleFrame.maxX - width - horizontalSpacing,
                    visibleFrame.minX + horizontalSpacing + cascadeOffset
                )
                let y = max(
                    visibleFrame.minY + verticalInset,
                    visibleFrame.maxY - verticalInset - height - cascadeOffset
                )
                origin = NSPoint(x: x, y: y)
            }
        } else {
            width = max(minimumFrameSize.width, min(1400, visibleFrame.width - 80))
            origin = NSPoint(
                x: visibleFrame.midX - (width / 2),
                y: visibleFrame.midY - (height / 2)
            )
        }

        window.setFrame(NSRect(origin: origin, size: NSSize(width: width, height: height)), display: true)
        reveal(window)
    }

    private func visibleWorkspaceWindows() -> [NSWindow] {
        let registeredWindows = WorkspaceWindowRegistry.shared.workspaceWindows()
            .filter(\.isVisible)
        if !registeredWindows.isEmpty {
            return registeredWindows
        }

        return NSApp.windows
            .filter { $0.isVisible && $0.identifier?.rawValue.contains("AppWindow") == true }
            .sorted { $0.windowNumber < $1.windowNumber }
    }

    private func reveal(_ window: NSWindow) {
        RalphMacPresentationRuntime.reveal(window)
    }

    private func activeScreen() -> NSScreen? {
        let mouseLocation = NSEvent.mouseLocation
        return NSScreen.screens.first(where: { NSMouseInRect(mouseLocation, $0.frame, false) })
            ?? NSScreen.main
            ?? NSScreen.screens.first
    }

    private func preferredLaunchScreen() -> NSScreen? {
        NSScreen.screens.first(where: { $0.frame.origin == .zero })
            ?? activeScreen()
    }

    private func applyMinimumSize(to window: NSWindow) {
        let contentMinimumRect = NSRect(origin: .zero, size: minimumSize)
        let frameMinimumSize = window.frameRect(forContentRect: contentMinimumRect).size
        if window.contentMinSize != minimumSize {
            window.contentMinSize = minimumSize
        }
        if window.minSize != frameMinimumSize {
            window.minSize = frameMinimumSize
        }

        let currentFrame = window.frame
        let resizedFrame = NSRect(
            x: currentFrame.origin.x,
            y: currentFrame.origin.y,
            width: max(currentFrame.width, frameMinimumSize.width),
            height: max(currentFrame.height, frameMinimumSize.height)
        )
        if resizedFrame.size != currentFrame.size {
            window.setFrame(resizedFrame, display: true)
        }
    }
}
