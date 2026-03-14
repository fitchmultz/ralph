/**
 RalphMacApp+Support

 Responsibilities:
 - Provide app-level support actions such as log export, crash-report export, and alerts.

 Does not handle:
 - URL routing.
 - Window/bootstrap lifecycle.

 Invariants/assumptions callers must respect:
 - AppKit save panels and alerts must run on the main actor.
 */

import AppKit
import Foundation
import SwiftUI
import RalphCore
import UniformTypeIdentifiers

extension RalphMacApp {
    func exportLogs() {
        guard RalphLogger.shared.canExportLogs else {
            showAlert(title: "Not Available", message: "Log export requires macOS 12 or later.")
            return
        }

        Task { @MainActor in
            do {
                let logContent = try await RalphLogger.shared.exportLogs(hours: 24)
                let savePanel = NSSavePanel()
                savePanel.nameFieldStringValue = "ralph-logs-\(Date().formatted(.iso8601.dateSeparator(.dash).timeSeparator(.omitted))).txt"
                savePanel.allowedContentTypes = [.plainText]

                let result = await savePanel.begin()
                if result == .OK, let url = savePanel.url {
                    do {
                        try logContent.write(to: url, atomically: true, encoding: .utf8)
                    } catch {
                        showAlert(title: "Export Failed", message: "Could not save logs: \(error.localizedDescription)")
                    }
                }
            } catch {
                showAlert(title: "Export Failed", message: "Could not retrieve logs: \(error.localizedDescription)")
            }
        }
    }

    func showCrashReports() {
        let reports = CrashReporter.shared.getAllReports()
        if reports.isEmpty {
            showAlert(title: "No Crash Reports", message: "No crash reports found.")
            return
        }

        let content = CrashReporter.shared.exportAllReports()

        Task { @MainActor in
            let savePanel = NSSavePanel()
            savePanel.nameFieldStringValue = "ralph-crash-reports-\(Date().formatted(.iso8601.dateSeparator(.dash))).txt"
            savePanel.allowedContentTypes = [.plainText]

            let result = await savePanel.begin()
            if result == .OK, let url = savePanel.url {
                do {
                    try content.write(to: url, atomically: true, encoding: .utf8)
                } catch {
                    showAlert(title: "Export Failed", message: "Could not save crash reports: \(error.localizedDescription)")
                }
            }
        }
    }

    func showAlert(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.alertStyle = .informational
        alert.runModal()
    }
}

@MainActor
private final class WeakWorkspaceWindowBox {
    weak var window: NSWindow?
    var windowStateID: UUID?
    var workspaceIDs: [UUID]
    var activeWorkspaceID: UUID?

    init(
        window: NSWindow,
        windowStateID: UUID? = nil,
        workspaceIDs: [UUID] = [],
        activeWorkspaceID: UUID? = nil
    ) {
        self.window = window
        self.windowStateID = windowStateID
        self.workspaceIDs = workspaceIDs
        self.activeWorkspaceID = activeWorkspaceID
    }
}

@MainActor
final class WorkspaceWindowRegistry {
    static let shared = WorkspaceWindowRegistry()

    private var windowsByNumber: [Int: WeakWorkspaceWindowBox] = [:]

    private init() {}

    func register(window: NSWindow, windowStateID: UUID? = nil) {
        pruneReleasedWindows()
        let entry = windowsByNumber[window.windowNumber] ?? WeakWorkspaceWindowBox(window: window)
        entry.window = window
        if let windowStateID {
            entry.windowStateID = windowStateID
        }
        windowsByNumber[window.windowNumber] = entry
    }

    func update(
        window: NSWindow,
        windowStateID: UUID,
        workspaceIDs: [UUID],
        activeWorkspaceID: UUID?
    ) {
        pruneReleasedWindows()
        let entry = windowsByNumber[window.windowNumber] ?? WeakWorkspaceWindowBox(window: window)
        entry.window = window
        entry.windowStateID = windowStateID
        entry.workspaceIDs = workspaceIDs
        entry.activeWorkspaceID = activeWorkspaceID
        windowsByNumber[window.windowNumber] = entry
    }

    func unregister(window: NSWindow) {
        windowsByNumber.removeValue(forKey: window.windowNumber)
        pruneReleasedWindows()
    }

    func contains(window: NSWindow) -> Bool {
        pruneReleasedWindows()
        return windowsByNumber[window.windowNumber]?.window === window
    }

    func workspaceWindows() -> [NSWindow] {
        liveEntries().map(\.window)
    }

    func preferredActiveWorkspaceID() -> UUID? {
        liveEntries().first(where: { $0.entry.activeWorkspaceID != nil })?.entry.activeWorkspaceID
    }

    var hasVisibleWorkspaceWindow: Bool {
        workspaceWindows().contains(where: \.isVisible)
    }

    private func liveEntries() -> [(window: NSWindow, entry: WeakWorkspaceWindowBox)] {
        pruneReleasedWindows()
        return windowsByNumber.values
            .compactMap { entry in
                guard let window = entry.window else { return nil }
                return (window, entry)
            }
            .sorted { lhs, rhs in
                let lhsPriority = priority(for: lhs.window)
                let rhsPriority = priority(for: rhs.window)
                if lhsPriority != rhsPriority {
                    return lhsPriority < rhsPriority
                }
                return lhs.window.windowNumber < rhs.window.windowNumber
            }
    }

    private func priority(for window: NSWindow) -> Int {
        if window.isKeyWindow { return 0 }
        if window.isMainWindow { return 1 }
        if window.isVisible { return 2 }
        return 3
    }

    private func pruneReleasedWindows() {
        windowsByNumber = windowsByNumber.filter { $0.value.window != nil }
    }
}

@MainActor
final class MainWindowService {
    static let shared = MainWindowService()

    private var openMainWindowHandler: (() -> Void)?

    private init() {}

    func register(openWindow: OpenWindowAction) {
        openMainWindowHandler = { openWindow(id: "main") }
    }

    @discardableResult
    func revealOrOpenPrimaryWindow() -> Bool {
        if let window = workspaceWindows().first {
            window.collectionBehavior.insert(.moveToActiveSpace)
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return true
        }

        guard let openMainWindowHandler else { return false }
        openMainWindowHandler()
        return true
    }
    private func workspaceWindows() -> [NSWindow] {
        let registeredWorkspaceWindows = WorkspaceWindowRegistry.shared.workspaceWindows()
        if !registeredWorkspaceWindows.isEmpty {
            return registeredWorkspaceWindows
        }

        let identifiedWorkspaceWindows = NSApp.windows
            .filter { $0.identifier?.rawValue.contains("AppWindow") == true }
            .sorted { $0.windowNumber < $1.windowNumber }
        if !identifiedWorkspaceWindows.isEmpty {
            return identifiedWorkspaceWindows
        }

        return NSApp.windows
            .filter(isFallbackPrimaryWindowCandidate)
            .sorted { $0.windowNumber < $1.windowNumber }
    }

    private func isFallbackPrimaryWindowCandidate(_ window: NSWindow) -> Bool {
        guard !SettingsWindowService.shared.isSettingsWindow(window) else { return false }
        guard window.canBecomeKey else { return false }
        guard !window.isMiniaturized else { return false }
        return true
    }
}

struct MainWindowOpenActionRegistrar: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Color.clear
            .frame(width: 0, height: 0)
            .allowsHitTesting(false)
            .task {
                MainWindowService.shared.register(openWindow: openWindow)
            }
    }
}

@MainActor
final class UITestingWorkspaceOpenBridge {
    static let shared = UITestingWorkspaceOpenBridge()
    nonisolated static let notificationName = Notification.Name("com.mitchfultz.ralph.uitesting.openWorkspace")
    nonisolated static let workspacePathUserInfoKey = "workspacePath"

    private let isUITestingLaunch = ProcessInfo.processInfo.arguments.contains("--uitesting")
    private var observer: (any NSObjectProtocol)?

    private init() {}

    func configureIfNeeded() {
        guard isUITestingLaunch, observer == nil else { return }
        observer = DistributedNotificationCenter.default().addObserver(
            forName: Self.notificationName,
            object: nil,
            queue: .main
        ) { notification in
            guard
                let rawPath = notification.userInfo?[Self.workspacePathUserInfoKey] as? String,
                !rawPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            else {
                return
            }
            MainActor.assumeIsolated {
                RalphURLRouter.openWorkspace(at: URL(fileURLWithPath: rawPath, isDirectory: true))
            }
        }
    }
}
