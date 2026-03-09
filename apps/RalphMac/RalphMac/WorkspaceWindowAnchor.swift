/**
 WorkspaceWindowAnchor

 Responsibilities:
 - Apply minimum window geometry and deterministic UI-testing placement.
 - Enforce UI-testing window-count policy via explicit coordinator state.

 Does not handle:
 - Workspace scene restoration or selection.
 - Main app command routing.

 Invariants/assumptions callers must respect:
 - UI-testing launches target one window by default and two windows for multiwindow runs.
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
    private var observerTokens: [AnyObject] = []

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
            ) { _ in
                Task { @MainActor in
                    self.enforceExpectedWindowCount()
                }
            },
            center.addObserver(
                forName: NSWindow.willCloseNotification,
                object: nil,
                queue: .main
            ) { _ in
                Task { @MainActor in
                    self.enforceExpectedWindowCount()
                }
            }
        ]

        enforceExpectedWindowCount()
    }

    func openAdditionalWindowIfNeeded(openWindow: OpenWindowAction) {
        guard isUITestingMultiwindowLaunch else { return }
        guard !didOpenSecondaryWindow else { return }

        didOpenSecondaryWindow = true
        openWindow(id: "main")
    }

    func enforceExpectedWindowCount() {
        guard isUITestingLaunch else { return }

        let expectedWindowCount = isUITestingMultiwindowLaunch ? 2 : 1
        let workspaceWindows = NSApp.windows
            .filter { $0.identifier?.rawValue.contains("AppWindow") == true }
            .sorted { $0.windowNumber < $1.windowNumber }

        guard workspaceWindows.count > expectedWindowCount else { return }
        for window in workspaceWindows.dropFirst(expectedWindowCount) {
            window.close()
        }
    }
}

struct WorkspaceWindowAnchor: NSViewRepresentable {
    let minimumSize: NSSize
    let uiTestingEnabled: Bool

    func makeNSView(context: Context) -> NSView {
        NSView(frame: .zero)
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        Task { @MainActor in
            guard let window = nsView.window else { return }
            configure(window: window)
        }
    }

    private func configure(window: NSWindow) {
        applyMinimumSize(to: window)

        guard uiTestingEnabled else { return }
        let screen = window.screen ?? NSScreen.main
        let visibleFrame = screen?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let workspaceWindows = NSApp.windows
            .filter { $0.isVisible && $0.identifier?.rawValue.contains("AppWindow") == true }
            .sorted { $0.windowNumber < $1.windowNumber }

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

        window.collectionBehavior.insert(.moveToActiveSpace)
        window.tabbingMode = .disallowed
        window.setFrame(NSRect(origin: origin, size: NSSize(width: width, height: height)), display: true)
        window.orderFrontRegardless()
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)

        UITestingWindowCoordinator.shared.enforceExpectedWindowCount()
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
