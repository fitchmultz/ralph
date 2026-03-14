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

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)

        // Disable automatic window tabbing globally
        NSWindow.allowsAutomaticWindowTabbing = false

        configureWindowObservers()
        stabilizeExistingWindows()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in
            self?.stabilizeExistingWindows()
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in
            self?.stabilizeExistingWindows()
        }
    }
    
    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)

        // Disable tabbing before any windows are created
        NSWindow.allowsAutomaticWindowTabbing = false
    }

    func applicationWillTerminate(_ notification: Notification) {
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
            }
        ]
    }

    private func stabilizeExistingWindows() {
        for window in NSApplication.shared.windows {
            normalizeWindow(window)
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
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self, weak window] in
            guard let self, let window else { return }
            self.applyRevealFrame(self.centeredFrame(for: window), to: window)
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { [weak self, weak window] in
            guard let self, let window else { return }
            self.applyRevealFrame(self.centeredFrame(for: window), to: window)
        }
    }

    private func isWorkspaceWindow(_ window: NSWindow) -> Bool {
        // SwiftUI/AppKit can create temporary helper windows for Settings and other system UI.
        // Only Ralph workspace windows participate in the app-level placement flow.
        window.identifier?.rawValue.contains("AppWindow") == true
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
