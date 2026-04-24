/**
 SettingsWindowFocus

 Purpose:
 - Keep the Settings window off AppKit's default text-field focus path.

 Responsibilities:
 - Keep the Settings window off AppKit's default text-field focus path.
 - Expose an accessibility diagnostics probe for deterministic contract tests.
 - Refresh diagnostics after key/main-window transitions.

 Does not handle:
 - Window creation.
 - Settings snapshot persistence.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import AppKit
import SwiftUI

private enum SettingsSceneAccessibilityID {
    static let diagnosticsProbe = "settings-diagnostics-probe"
}

@MainActor
struct SettingsDiagnosticsAccessibilityProbe: View {
    let snapshot: SettingsWindowDiagnosticsSnapshot

    var body: some View {
        Color.clear
            .frame(width: 1, height: 1)
            .allowsHitTesting(false)
            .accessibilityElement(children: .ignore)
            .accessibilityIdentifier(SettingsSceneAccessibilityID.diagnosticsProbe)
            .accessibilityLabel("settings-window-diagnostics")
            .accessibilityValue(snapshot.encodedForAccessibility())
    }
}

@MainActor
struct SettingsWindowFocusAnchor: NSViewRepresentable {
    func makeNSView(context: Context) -> FocusAnchorView {
        let view = FocusAnchorView()
        view.installIntoSettingsWindowIfNeeded()
        return view
    }

    func updateNSView(_ nsView: FocusAnchorView, context: Context) {
        nsView.installIntoSettingsWindowIfNeeded()
    }

    final class FocusAnchorView: NSView {
        private weak var observedWindow: NSWindow?
        private var diagnosticsCaptureTasks: [Task<Void, Never>] = []

        // Keep Settings off the initial text-field path. Allowing AppKit to pick the
        // first editable key view constructs the shared field editor early enough to
        // spawn the extra helper windows seen during Cmd+, open.

        override init(frame frameRect: NSRect) {
            super.init(frame: frameRect)
            alphaValue = 0.001
        }

        required init?(coder: NSCoder) {
            fatalError("init(coder:) has not been implemented")
        }

        override var acceptsFirstResponder: Bool {
            true
        }

        override func becomeFirstResponder() -> Bool {
            true
        }

        override func resignFirstResponder() -> Bool {
            true
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            installIntoSettingsWindowIfNeeded()
        }

        func installIntoSettingsWindowIfNeeded() {
            guard let window else {
                removeWindowObservation()
                return
            }

            SettingsWindowService.shared.configure(window: window)

            if observedWindow !== window {
                removeWindowObservation()
                observedWindow = window
                NotificationCenter.default.addObserver(
                    self,
                    selector: #selector(handleWindowDidBecomeKey(_:)),
                    name: NSWindow.didBecomeKeyNotification,
                    object: window
                )
                NotificationCenter.default.addObserver(
                    self,
                    selector: #selector(handleWindowDidBecomeMain(_:)),
                    name: NSWindow.didBecomeMainNotification,
                    object: window
                )
            }

            configureInitialResponder(for: window)
            scheduleDiagnosticsCapture(for: window)
        }

        @objc
        private func handleWindowDidBecomeKey(_ notification: Notification) {
            guard let window = notification.object as? NSWindow else { return }
            configureInitialResponder(for: window)
            if window.firstResponder !== self {
                window.makeFirstResponder(self)
            }
            scheduleDiagnosticsCapture(for: window)
        }

        @objc
        private func handleWindowDidBecomeMain(_ notification: Notification) {
            guard let window = notification.object as? NSWindow else { return }
            configureInitialResponder(for: window)
            scheduleDiagnosticsCapture(for: window)
        }

        private func configureInitialResponder(for window: NSWindow) {
            window.initialFirstResponder = self
        }

        private func scheduleDiagnosticsCapture(for window: NSWindow) {
            cancelDiagnosticsCaptureTasks()
            diagnosticsCaptureTasks = [
                Task { @MainActor [weak window] in
                    guard let window else { return }
                    SettingsPresentationCoordinator.shared.capture(window: window)
                },
                Task { @MainActor [weak window] in
                    await Task.yield()
                    guard let window, !Task.isCancelled else { return }
                    SettingsPresentationCoordinator.shared.capture(window: window)
                },
                Task { @MainActor [weak window] in
                    try? await Task.sleep(nanoseconds: 200_000_000)
                    guard let window, !Task.isCancelled else { return }
                    SettingsPresentationCoordinator.shared.capture(window: window)
                }
            ]
        }

        private func cancelDiagnosticsCaptureTasks() {
            diagnosticsCaptureTasks.forEach { $0.cancel() }
            diagnosticsCaptureTasks.removeAll(keepingCapacity: false)
        }

        private func removeWindowObservation() {
            cancelDiagnosticsCaptureTasks()
            if let observedWindow {
                NotificationCenter.default.removeObserver(
                    self,
                    name: NSWindow.didBecomeKeyNotification,
                    object: observedWindow
                )
                NotificationCenter.default.removeObserver(
                    self,
                    name: NSWindow.didBecomeMainNotification,
                    object: observedWindow
                )
            }
            observedWindow = nil
        }

        deinit {
            diagnosticsCaptureTasks.forEach { $0.cancel() }
            NotificationCenter.default.removeObserver(self)
        }
    }
}
