/**
 ASettingsInfra

 Responsibilities:
 - Host the SwiftUI Settings scene root used by app/menu/url surfaces.
 - Keep settings-window composition, focus control, and diagnostics separate from the main app scene.

 Does not handle:
 - Settings tab content (defined in `AppSettings.swift`).
 - Settings open command wiring (defined in `SettingsService.swift`).
 */

import SwiftUI
import AppKit
import RalphCore

private enum SettingsSceneAccessibilityID {
    static let diagnosticsProbe = "settings-diagnostics-probe"
}

enum SettingsWindowIdentity {
    static let sceneID = "settings"
    static let windowIdentifier = "com.mitchfultz.ralph.settings-window"
    static let legacyWindowIdentifier = "com_apple_SwiftUI_Settings_window"
    static let title = "Settings"
}

@MainActor
final class SettingsWindowService {
    static let shared = SettingsWindowService()

    private var openSettingsWindowHandler: (() -> Void)?

    private init() {}

    func register(openWindow: OpenWindowAction) {
        openSettingsWindowHandler = {
            openWindow(id: SettingsWindowIdentity.sceneID)
        }
    }

    @discardableResult
    func revealOrOpenPreparedWindow() -> Bool {
        if let existingWindow = NSApp.windows.first(where: isSettingsWindow) {
            configure(window: existingWindow)
            existingWindow.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return true
        }

        guard let openSettingsWindowHandler else { return false }
        openSettingsWindowHandler()
        NSApp.activate(ignoringOtherApps: true)
        return true
    }

    func configure(window: NSWindow) {
        window.identifier = NSUserInterfaceItemIdentifier(SettingsWindowIdentity.windowIdentifier)
        window.title = SettingsWindowIdentity.title
        window.collectionBehavior.insert(.moveToActiveSpace)
        window.tabbingMode = .disallowed
    }

    func isSettingsWindow(_ window: NSWindow) -> Bool {
        guard let rawIdentifier = window.identifier?.rawValue else { return false }
        return rawIdentifier == SettingsWindowIdentity.windowIdentifier
            || rawIdentifier == SettingsWindowIdentity.legacyWindowIdentifier
    }
}

@MainActor
struct SettingsWindowOpenActionRegistrar: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Color.clear
            .frame(width: 0, height: 0)
            .allowsHitTesting(false)
            .task {
                SettingsWindowService.shared.register(openWindow: openWindow)
            }
    }
}

struct SettingsWindowHelperSnapshot: Codable, Equatable {
    let className: String
    let title: String
    let identifier: String?
}

struct SettingsWindowDiagnosticsSnapshot: Codable, Equatable {
    var requestSequence: Int
    var source: String?
    var requestedWorkspacePath: String?
    var resolvedWorkspacePath: String?
    var contentWorkspacePath: String?
    var settingsRunnerValue: String?
    var settingsModelValue: String?
    var settingsIsLoading: Bool
    var visibleAppWindowCount: Int
    var visibleWorkspaceWindowCount: Int
    var visibleSettingsWindowCount: Int
    var visibleHelperWindowCount: Int
    var helperWindows: [SettingsWindowHelperSnapshot]
    var settingsWindowTitle: String?
    var firstResponderClassName: String?
    var firstResponderIsTextView: Bool
    var settingsWindowIsKey: Bool

    static let idle = SettingsWindowDiagnosticsSnapshot(
        requestSequence: 0,
        source: nil,
        requestedWorkspacePath: nil,
        resolvedWorkspacePath: nil,
        contentWorkspacePath: nil,
        settingsRunnerValue: nil,
        settingsModelValue: nil,
        settingsIsLoading: false,
        visibleAppWindowCount: 0,
        visibleWorkspaceWindowCount: 0,
        visibleSettingsWindowCount: 0,
        visibleHelperWindowCount: 0,
        helperWindows: [],
        settingsWindowTitle: nil,
        firstResponderClassName: nil,
        firstResponderIsTextView: false,
        settingsWindowIsKey: false
    )

    func encodedForAccessibility() -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        guard let data = try? encoder.encode(self),
              let string = String(data: data, encoding: .utf8)
        else {
            return "{}"
        }
        return string
    }
}

@MainActor
private struct SettingsDiagnosticsAccessibilityProbe: View {
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
            Task { @MainActor [weak window] in
                guard let window else { return }
                SettingsPresentationCoordinator.shared.capture(window: window)
            }

            Task { @MainActor [weak window] in
                await Task.yield()
                guard let window, !Task.isCancelled else { return }
                SettingsPresentationCoordinator.shared.capture(window: window)
            }

            Task { @MainActor [weak window] in
                try? await Task.sleep(nanoseconds: 200_000_000)
                guard let window, !Task.isCancelled else { return }
                SettingsPresentationCoordinator.shared.capture(window: window)
            }
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
    @Published private(set) var diagnostics = SettingsWindowDiagnosticsSnapshot.idle

    private let diagnosticsFileURL: URL?

    private init() {
        if let rawPath = ProcessInfo.processInfo.environment["RALPH_SETTINGS_DIAGNOSTICS_PATH"]?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !rawPath.isEmpty {
            diagnosticsFileURL = URL(fileURLWithPath: rawPath, isDirectory: false)
        } else {
            diagnosticsFileURL = nil
        }
    }

    func prepare(workspace: Workspace?, source: SettingsPresentationSource) {
        self.workspace = workspace
        diagnostics = SettingsWindowDiagnosticsSnapshot(
            requestSequence: diagnostics.requestSequence + 1,
            source: source.rawValue,
            requestedWorkspacePath: workspace?.identityState.workingDirectoryURL.path,
            resolvedWorkspacePath: resolvedWorkspacePath(for: workspace),
            contentWorkspacePath: diagnostics.contentWorkspacePath,
            settingsRunnerValue: diagnostics.settingsRunnerValue,
            settingsModelValue: diagnostics.settingsModelValue,
            settingsIsLoading: diagnostics.settingsIsLoading,
            visibleAppWindowCount: diagnostics.visibleAppWindowCount,
            visibleWorkspaceWindowCount: diagnostics.visibleWorkspaceWindowCount,
            visibleSettingsWindowCount: diagnostics.visibleSettingsWindowCount,
            visibleHelperWindowCount: diagnostics.visibleHelperWindowCount,
            helperWindows: diagnostics.helperWindows,
            settingsWindowTitle: diagnostics.settingsWindowTitle,
            firstResponderClassName: diagnostics.firstResponderClassName,
            firstResponderIsTextView: diagnostics.firstResponderIsTextView,
            settingsWindowIsKey: diagnostics.settingsWindowIsKey
        )
        persistDiagnosticsIfNeeded()
    }

    func capture(window: NSWindow?) {
        let visibleWindows = NSApp.windows.filter(\.isVisible)
        let workspaceWindows = visibleWindows.filter(isWorkspaceWindow)
        let settingsWindows = visibleWindows.filter(isSettingsWindow)
        let helperWindows = visibleWindows.filter { !isWorkspaceWindow($0) && !isSettingsWindow($0) }
        let resolvedSettingsWindow = window.flatMap { isSettingsWindow($0) ? $0 : nil } ?? settingsWindows.first

        diagnostics = SettingsWindowDiagnosticsSnapshot(
            requestSequence: diagnostics.requestSequence,
            source: diagnostics.source,
            requestedWorkspacePath: workspace?.identityState.workingDirectoryURL.path,
            resolvedWorkspacePath: resolvedWorkspacePath(for: workspace),
            contentWorkspacePath: diagnostics.contentWorkspacePath,
            settingsRunnerValue: diagnostics.settingsRunnerValue,
            settingsModelValue: diagnostics.settingsModelValue,
            settingsIsLoading: diagnostics.settingsIsLoading,
            visibleAppWindowCount: visibleWindows.count,
            visibleWorkspaceWindowCount: workspaceWindows.count,
            visibleSettingsWindowCount: settingsWindows.count,
            visibleHelperWindowCount: helperWindows.count,
            helperWindows: helperWindows.map {
                SettingsWindowHelperSnapshot(
                    className: String(describing: type(of: $0)),
                    title: $0.title,
                    identifier: $0.identifier?.rawValue
                )
            },
            settingsWindowTitle: resolvedSettingsWindow?.title,
            firstResponderClassName: resolvedSettingsWindow.flatMap { window in
                window.firstResponder.map { String(describing: type(of: $0)) }
            },
            firstResponderIsTextView: resolvedSettingsWindow?.firstResponder is NSTextView,
            settingsWindowIsKey: resolvedSettingsWindow?.isKeyWindow ?? false
        )
        persistDiagnosticsIfNeeded()
    }

    func captureContent(
        workspacePath: String?,
        runner: String?,
        model: String?,
        isLoading: Bool
    ) {
        diagnostics.contentWorkspacePath = workspacePath
        diagnostics.settingsRunnerValue = runner
        diagnostics.settingsModelValue = model
        diagnostics.settingsIsLoading = isLoading
        persistDiagnosticsIfNeeded()
    }

    func isSettingsWindow(_ window: NSWindow) -> Bool {
        SettingsWindowService.shared.isSettingsWindow(window)
    }

    private func isWorkspaceWindow(_ window: NSWindow) -> Bool {
        window.identifier?.rawValue.contains("AppWindow") == true
    }

    private func resolvedWorkspacePath(for preparedWorkspace: Workspace?) -> String? {
        (preparedWorkspace ?? WorkspaceManager.shared.effectiveWorkspace)?.identityState.workingDirectoryURL.path
    }

    private func persistDiagnosticsIfNeeded() {
        guard let diagnosticsFileURL else { return }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .prettyPrinted]
        guard let data = try? encoder.encode(diagnostics) else { return }
        let directory = diagnosticsFileURL.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try? data.write(to: diagnosticsFileURL, options: .atomic)
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
            .overlay(alignment: .bottomTrailing) {
                SettingsDiagnosticsAccessibilityProbe(snapshot: presentation.diagnostics)
            }
    }
}
