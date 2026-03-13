/**
 ASettingsInfra

 Responsibilities:
 - Host the SwiftUI Settings scene root used by app/menu surfaces.
 - Keep settings-window composition separate from the main app scene.

 Does not handle:
 - Settings tab content (defined in `AppSettings.swift`).
 - Settings open commands (defined in `SettingsService.swift`).
 */

import SwiftUI
import AppKit
import RalphCore

@MainActor
private struct SettingsWindowFocusAnchor: NSViewRepresentable {
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
            guard let window, window.identifier?.rawValue == "com_apple_SwiftUI_Settings_window" else {
                removeWindowObservation()
                return
            }

            if observedWindow !== window {
                removeWindowObservation()
                observedWindow = window
                NotificationCenter.default.addObserver(
                    self,
                    selector: #selector(handleWindowDidBecomeKey(_:)),
                    name: NSWindow.didBecomeKeyNotification,
                    object: window
                )
            }

            configureInitialResponder(for: window)
        }

        @objc
        private func handleWindowDidBecomeKey(_ notification: Notification) {
            guard let window = notification.object as? NSWindow else { return }
            configureInitialResponder(for: window)
            if window.firstResponder !== self {
                window.makeFirstResponder(self)
            }
        }

        private func configureInitialResponder(for window: NSWindow) {
            window.initialFirstResponder = self
        }

        private func removeWindowObservation() {
            if let observedWindow {
                NotificationCenter.default.removeObserver(
                    self,
                    name: NSWindow.didBecomeKeyNotification,
                    object: observedWindow
                )
            }
            observedWindow = nil
        }

        deinit {
            NotificationCenter.default.removeObserver(self)
        }
    }
}

enum AppColorScheme: String, CaseIterable, Codable, Identifiable {
    case system
    case light
    case dark

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .system:
            return "System"
        case .light:
            return "Light"
        case .dark:
            return "Dark"
        }
    }

    var helperText: String {
        switch self {
        case .system:
            return "Follow the current macOS appearance."
        case .light:
            return "Force Ralph into light mode."
        case .dark:
            return "Force Ralph into dark mode."
        }
    }

    var preferredColorScheme: ColorScheme? {
        switch self {
        case .system:
            return nil
        case .light:
            return .light
        case .dark:
            return .dark
        }
    }

    var nsAppearance: NSAppearance? {
        switch self {
        case .system:
            return nil
        case .light:
            return NSAppearance(named: .aqua)
        case .dark:
            return NSAppearance(named: .darkAqua)
        }
    }
}

@MainActor
final class AppAppearanceController: ObservableObject {
    static let shared = AppAppearanceController()

    @Published var selection: AppColorScheme {
        didSet {
            guard selection != oldValue else { return }
            RalphAppDefaults.userDefaults.set(selection.rawValue, forKey: Self.colorSchemeKey)
            applyAppearance()
        }
    }

    var preferredColorScheme: ColorScheme? {
        selection.preferredColorScheme
    }

    private static let colorSchemeKey = "colorScheme"

    private init() {
        let storedValue = RalphAppDefaults.userDefaults.string(forKey: Self.colorSchemeKey)
        self.selection = AppColorScheme(rawValue: storedValue ?? "") ?? .system
        applyAppearance()
    }

    func applyAppearance() {
        NSApp.appearance = selection.nsAppearance
    }
}

@MainActor
final class SettingsPresentationCoordinator: ObservableObject {
    static let shared = SettingsPresentationCoordinator()

    @Published private(set) var workspace: Workspace?

    private init() {}

    func prepare(workspace: Workspace?) {
        self.workspace = workspace
    }
}

@MainActor
struct SettingsSceneRoot: View {
    @ObservedObject private var presentation = SettingsPresentationCoordinator.shared

    var body: some View {
        let workspace = presentation.workspace ?? WorkspaceManager.shared.effectiveWorkspace

        SettingsContentContainer(workspace: workspace)
            .frame(minWidth: 640, minHeight: 480)
            .preferredColorScheme(AppAppearanceController.shared.preferredColorScheme)
            .background(SettingsWindowFocusAnchor())
    }
}
