/**
 SettingsWindowService

 Purpose:
 - Manage the dedicated Settings window controller and presentation refresh cadence.

 Responsibilities:
 - Manage the dedicated Settings window controller and presentation refresh cadence.
 - Install fresh settings-scene content before each reveal.
 - Keep AppKit window configuration and reveal policy out of the scene/root view files.

 Does not handle:
 - Diagnostics snapshot persistence.
 - Settings tab content rendering.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import AppKit
import SwiftUI
import RalphCore

@MainActor
final class SettingsWindowService: NSObject, NSWindowDelegate {
    static let shared = SettingsWindowService()

    private var windowController: NSWindowController?
    private var postRevealTasks: [Task<Void, Never>] = []

    private override init() {
        super.init()
    }

    @discardableResult
    func revealOrOpenPreparedWindow() -> Bool {
        let controller = ensureWindowController()
        guard let window = controller.window else { return false }

        installFreshRootView(on: controller)
        configure(window: window)
        controller.showWindow(nil)
        RalphMacPresentationRuntime.reveal(window, center: true)
        SettingsPresentationCoordinator.shared.capture(window: window)
        scheduleKeyWindowRefresh(for: window)
        return true
    }

    func configure(window: NSWindow) {
        window.identifier = NSUserInterfaceItemIdentifier(SettingsWindowIdentity.windowIdentifier)
        window.title = SettingsWindowIdentity.title
        window.collectionBehavior.insert(.moveToActiveSpace)
        window.tabbingMode = .disallowed
        window.minSize = NSSize(width: 640, height: 480)
        if window.frame.width < 760 || window.frame.height < 520 {
            window.setContentSize(NSSize(width: 760, height: 520))
        }
        if !RalphAppDefaults.isMacOSContract {
            window.center()
        }
    }

    func isSettingsWindow(_ window: NSWindow) -> Bool {
        guard let rawIdentifier = window.identifier?.rawValue else { return false }
        return rawIdentifier == SettingsWindowIdentity.windowIdentifier
            || rawIdentifier == SettingsWindowIdentity.legacyWindowIdentifier
    }

    func windowWillClose(_ notification: Notification) {
        guard let window = notification.object as? NSWindow else { return }
        guard window === windowController?.window else { return }
        cancelPostRevealTasks()
        windowController = nil
    }

    private func installFreshRootView(on controller: NSWindowController) {
        controller.contentViewController = NSHostingController(rootView: makeRootView())
    }

    private func scheduleKeyWindowRefresh(for window: NSWindow) {
        cancelPostRevealTasks()
        postRevealTasks = [
            Task { @MainActor [weak self, weak window] in
                guard let self, let window else { return }
                self.refreshPresentedWindow(window)
            },
            Task { @MainActor [weak self, weak window] in
                await Task.yield()
                guard let self, let window, !Task.isCancelled else { return }
                self.refreshPresentedWindow(window)
            },
            Task { @MainActor [weak self, weak window] in
                try? await Task.sleep(nanoseconds: 150_000_000)
                guard let self, let window, !Task.isCancelled else { return }
                self.refreshPresentedWindow(window)
            },
            Task { @MainActor [weak self, weak window] in
                try? await Task.sleep(nanoseconds: 400_000_000)
                guard let self, let window, !Task.isCancelled else { return }
                self.refreshPresentedWindow(window)
            }
        ]
    }

    private func refreshPresentedWindow(_ window: NSWindow) {
        guard window === windowController?.window else { return }
        configure(window: window)
        RalphMacPresentationRuntime.reveal(window, center: true)
        SettingsPresentationCoordinator.shared.capture(window: window)
    }

    private func cancelPostRevealTasks() {
        postRevealTasks.forEach { $0.cancel() }
        postRevealTasks.removeAll(keepingCapacity: false)
    }

    private func makeRootView() -> AnyView {
        AnyView(
            SettingsSceneRoot()
                .id(SettingsPresentationCoordinator.shared.contentIdentity)
        )
    }

    private func ensureWindowController() -> NSWindowController {
        if let windowController, let window = windowController.window {
            configure(window: window)
            return windowController
        }

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 760, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.contentViewController = NSHostingController(rootView: makeRootView())
        window.delegate = self
        window.isReleasedWhenClosed = false
        configure(window: window)

        let controller = NSWindowController(window: window)
        windowController = controller
        return controller
    }
}
