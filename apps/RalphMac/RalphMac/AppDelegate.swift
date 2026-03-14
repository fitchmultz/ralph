/**
 AppDelegate

 Responsibilities:
 - Configure activation policy and window behavior before SwiftUI takes over.
 - Disable automatic window tabbing globally before any windows are created.
 - Keep existing windows out of automatic tabbing mode after launch.

 Does not handle:
 - SwiftUI view hierarchy.
 - Window content management.
 - App command routing or keyboard shortcut dispatch.

 Invariants/assumptions callers must respect:
 - Must be connected via @NSApplicationDelegateAdaptor in the SwiftUI App struct.
 - applicationWillFinishLaunching is called before any windows are created.
 - Workspace window commands are routed through focused scene values in SwiftUI.
 */

import AppKit
import SwiftUI
import RalphCore

@MainActor
class AppDelegate: NSObject, NSApplicationDelegate {
    private var windowObserverTokens: [any NSObjectProtocol] = []
    private var normalizedWindowNumbers = Set<Int>()
    private var launchStabilizationTasks: [Task<Void, Never>] = []
    private var delayedWindowNormalizationTasks: [Int: [Task<Void, Never>]] = [:]
    private var primaryWindowBootstrapTasks: [Task<Void, Never>] = []
    private var didRequestPrimaryWindowBootstrap = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)

        // Disable automatic window tabbing globally
        NSWindow.allowsAutomaticWindowTabbing = false

        configureWindowObservers()
        UITestingWorkspaceOpenBridge.shared.configureIfNeeded()
        stabilizeExistingWindows()
        schedulePrimaryWindowBootstrap(after: 50_000_000)
        schedulePrimaryWindowBootstrap(after: 250_000_000)
        schedulePrimaryWindowBootstrap(after: 800_000_000)
        scheduleLaunchStabilization(after: 200_000_000)
        scheduleLaunchStabilization(after: 800_000_000)
    }
    
    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)

        // Disable tabbing before any windows are created
        NSWindow.allowsAutomaticWindowTabbing = false
    }

    func applicationWillTerminate(_ notification: Notification) {
        cancelLaunchStabilizationTasks()
        cancelPrimaryWindowBootstrapTasks()
        cancelAllDelayedWindowNormalizationTasks()
        WorkspaceManager.shared.persistRegisteredWindowStates()
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        guard !flag else { return false }
        return MainWindowService.shared.revealOrOpenPrimaryWindow()
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        for url in urls {
            RalphURLRouter.handle(url)
        }
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        stabilizeExistingWindows()
    }

    private func configureWindowObservers() {
        guard windowObserverTokens.isEmpty else { return }

        let center = NotificationCenter.default
        windowObserverTokens = [
            center.addObserver(
                forName: NSWindow.didBecomeKeyNotification,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let window = notification.object as? NSWindow else { return }
                MainActor.assumeIsolated {
                    self?.normalizeWindow(window)
                }
            },
            center.addObserver(
                forName: NSWindow.didBecomeMainNotification,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let window = notification.object as? NSWindow else { return }
                MainActor.assumeIsolated {
                    self?.normalizeWindow(window)
                }
            },
            center.addObserver(
                forName: NSWindow.willCloseNotification,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let window = notification.object as? NSWindow else { return }
                MainActor.assumeIsolated {
                    self?.cancelDelayedWindowNormalizationTasks(for: window.windowNumber)
                    self?.normalizedWindowNumbers.remove(window.windowNumber)
                }
            }
        ]
    }

    private func stabilizeExistingWindows() {
        for window in NSApplication.shared.windows {
            normalizeWindow(window)
        }
    }

    private func scheduleLaunchStabilization(after nanoseconds: UInt64) {
        let task = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: nanoseconds)
            guard let self, !Task.isCancelled else { return }
            self.stabilizeExistingWindows()
        }
        launchStabilizationTasks.append(task)
    }

    private func cancelLaunchStabilizationTasks() {
        launchStabilizationTasks.forEach { $0.cancel() }
        launchStabilizationTasks.removeAll(keepingCapacity: false)
    }

    private func schedulePrimaryWindowBootstrap(after nanoseconds: UInt64) {
        let task = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: nanoseconds)
            guard let self, !Task.isCancelled else { return }
            guard !self.didRequestPrimaryWindowBootstrap else { return }
            guard !self.hasWorkspaceWindowCandidate else { return }

            if MainWindowService.shared.revealOrOpenPrimaryWindow() {
                self.didRequestPrimaryWindowBootstrap = true
            }
        }
        primaryWindowBootstrapTasks.append(task)
    }

    private func cancelPrimaryWindowBootstrapTasks() {
        primaryWindowBootstrapTasks.forEach { $0.cancel() }
        primaryWindowBootstrapTasks.removeAll(keepingCapacity: false)
    }

    private func scheduleDelayedWindowNormalization(for window: NSWindow, after nanoseconds: UInt64) {
        let windowNumber = window.windowNumber
        let task = Task { @MainActor [weak self, weak window] in
            try? await Task.sleep(nanoseconds: nanoseconds)
            guard let self, let window, !Task.isCancelled else { return }
            self.applyRevealFrame(self.centeredFrame(for: window), to: window)
        }
        delayedWindowNormalizationTasks[windowNumber, default: []].append(task)
    }

    private func cancelDelayedWindowNormalizationTasks(for windowNumber: Int) {
        delayedWindowNormalizationTasks[windowNumber]?.forEach { $0.cancel() }
        delayedWindowNormalizationTasks[windowNumber] = nil
    }

    private func cancelAllDelayedWindowNormalizationTasks() {
        delayedWindowNormalizationTasks.values.flatMap { $0 }.forEach { $0.cancel() }
        delayedWindowNormalizationTasks.removeAll(keepingCapacity: false)
    }

    private var hasVisibleWorkspaceWindow: Bool {
        WorkspaceWindowRegistry.shared.hasVisibleWorkspaceWindow
            || NSApp.windows.contains { window in
                isWorkspaceWindow(window) && window.isVisible
            }
    }

    private var hasWorkspaceWindowCandidate: Bool {
        !WorkspaceWindowRegistry.shared.workspaceWindows().isEmpty
            || NSApp.windows.contains { window in
                isWorkspaceWindow(window)
                    || (!SettingsWindowService.shared.isSettingsWindow(window)
                        && window.canBecomeKey
                        && !window.isMiniaturized)
            }
    }

    private func normalizeWindow(_ window: NSWindow) {
        guard isWorkspaceWindow(window) else { return }

        window.tabbingMode = .disallowed
        window.collectionBehavior.insert(.moveToActiveSpace)

        guard shouldNormalizePlacement(for: window) else { return }
        let shouldApplyInitialPlacement = normalizedWindowNumbers.insert(window.windowNumber).inserted
        guard shouldApplyInitialPlacement || requiresPlacementReset(for: window) else { return }

        let frame = centeredFrame(for: window)
        applyRevealFrame(frame, to: window)
        cancelDelayedWindowNormalizationTasks(for: window.windowNumber)
        scheduleDelayedWindowNormalization(for: window, after: 150_000_000)
        scheduleDelayedWindowNormalization(for: window, after: 600_000_000)
    }

    private func isWorkspaceWindow(_ window: NSWindow) -> Bool {
        // SwiftUI/AppKit can create temporary helper windows for Settings and other system UI.
        // Only Ralph workspace windows participate in the app-level placement flow.
        WorkspaceWindowRegistry.shared.contains(window: window)
            || window.identifier?.rawValue.contains("AppWindow") == true
    }

    private func shouldNormalizePlacement(for window: NSWindow) -> Bool {
        let frame = window.frame
        guard frame.width >= 400, frame.height >= 240 else { return false }
        return true
    }

    private func requiresPlacementReset(for window: NSWindow) -> Bool {
        guard let activeVisibleFrame = NSScreen.main?.visibleFrame ?? NSScreen.screens.first?.visibleFrame else {
            return false
        }

        let intersection = window.frame.intersection(activeVisibleFrame)
        guard !intersection.isNull else { return true }
        let minimumVisibleWidth = max(240, min(window.frame.width * 0.4, window.frame.width))
        let minimumVisibleHeight = max(180, min(window.frame.height * 0.4, window.frame.height))
        return intersection.width < minimumVisibleWidth || intersection.height < minimumVisibleHeight
    }

    private func centeredFrame(for window: NSWindow) -> NSRect {
        let activeVisibleFrame = preferredLaunchScreen()?.visibleFrame
            ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let width = max(900, min(window.frame.width, activeVisibleFrame.width - 80))
        let height = max(640, min(window.frame.height, activeVisibleFrame.height - 80))
        return NSRect(
            x: activeVisibleFrame.midX - (width / 2),
            y: activeVisibleFrame.midY - (height / 2),
            width: width,
            height: height
        )
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

    private func applyRevealFrame(_ frame: NSRect, to window: NSWindow) {
        window.setFrame(frame, display: true)
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}
