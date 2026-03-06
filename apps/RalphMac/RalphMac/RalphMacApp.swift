/**
 RalphMacApp

 Responsibilities:
 - Define the macOS SwiftUI app entry point.
 - Configure multi-window support with native macOS tab bar integration.
 - Handle window restoration on app relaunch.
 - Provide menu commands for window/tab management and navigation.

 Does not handle:
 - Individual workspace content or CLI operations (see Workspace and WindowView).
 - Sidebar navigation state (see NavigationViewModel).

 Invariants/assumptions callers must respect:
 - The app can use either the bundled `ralph` binary or a launcher-provided override.
 - Window restoration state is stored in UserDefaults.
 - Navigation notifications are sent via NotificationCenter.
 */

import SwiftUI
import AppKit
import RalphCore

@MainActor
@main
struct RalphMacApp: App {
    // Connect AppDelegate for AppKit-level lifecycle handling
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    private let manager = WorkspaceManager.shared
    @State private var menuBarManager = MenuBarManager.shared
    @State private var uiTestingMenuBarVisible = false
    private let isUITesting = ProcessInfo.processInfo.arguments.contains("--uitesting")

    init() {
        // Initialize crash reporter early in app lifecycle to catch launch crashes
        CrashReporter.shared.install()
    }

    var body: some Scene {
        WindowGroup(id: "main") {
            WindowViewContainer()
                .background(
                    VisualEffectView(material: .windowBackground, blendingMode: .behindWindow)
                        .ignoresSafeArea()
                )
                .onOpenURL(perform: handleOpenURL)
                .onReceive(NotificationCenter.default.publisher(for: .showMainAppFromMenuBar)) { _ in
                    // Bring app to front when requested from menu bar
                    NSApp.activate(ignoringOtherApps: true)
                }
        }
        // Limit external event matching to the Ralph URL scheme route.
        .handlesExternalEvents(matching: ["ralph"])
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unified(showsTitle: false))
        .defaultSize(width: 1400, height: 900)
        .defaultPosition(.center)
        .commands {
            WorkspaceCommands()
            navigationCommands
            taskCommands
            CommandPaletteCommands()
            helpCommands
            settingsCommands
        }
        
        MenuBarExtra(
            isInserted: menuBarVisibilityBinding,
            content: { MenuBarContentView() },
            label: { MenuBarIconView() }
        )
        .menuBarExtraStyle(.menu)
        
        // Settings window is created programmatically via SettingsWindowController
    }
    
    private var settingsCommands: some Commands {
        CommandGroup(replacing: .appSettings) {
            OpenSettingsButton()
        }
    }

    private var menuBarVisibilityBinding: Binding<Bool> {
        if isUITesting {
            return $uiTestingMenuBarVisible
        }
        return $menuBarManager.isMenuBarExtraVisible
    }

    /// Handle incoming URL from CLI or external source
    private func handleOpenURL(_ url: URL) {
        guard url.scheme == "ralph" else {
            RalphLogger.shared.info("Received URL with unexpected scheme: \(url.scheme ?? "nil")", category: .lifecycle)
            return
        }

        guard url.host == "open" else {
            RalphLogger.shared.info("Received ralph:// URL with unexpected host: \(url.host ?? "nil")", category: .lifecycle)
            return
        }

        // Extract workspace path from query parameters
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: true),
              let queryItems = components.queryItems,
              let workspaceItem = queryItems.first(where: { $0.name == "workspace" }),
              let encodedPath = workspaceItem.value,
              let path = encodedPath.removingPercentEncoding else {
            RalphLogger.shared.info("Received ralph://open URL without valid workspace parameter", category: .lifecycle)
            return
        }

        if let cliPath = queryItems.first(where: { $0.name == "cli" })?.value?.removingPercentEncoding {
            manager.adoptCLIExecutable(path: cliPath)
        }

        let workspaceURL = URL(fileURLWithPath: path, isDirectory: true)
            .standardizedFileURL
            .resolvingSymlinksInPath()

        // Validate the path exists and is a directory
        var isDir: ObjCBool = false
        let exists = FileManager.default.fileExists(atPath: path, isDirectory: &isDir)

        guard exists && isDir.boolValue else {
            RalphLogger.shared.error("Workspace path does not exist or is not a directory: \(path)", category: .workspace)
            return
        }

        // Check if we already have a workspace for this directory
        if let existingWorkspace = manager.workspaces.first(where: {
            $0.workingDirectoryURL
                .standardizedFileURL
                .resolvingSymlinksInPath()
                .path == workspaceURL.path
        }) {
            // Activate existing workspace - post notification for WindowView to handle
            NotificationCenter.default.post(
                name: .activateWorkspace,
                object: existingWorkspace.id
            )
            RalphLogger.shared.info("Activated existing workspace: \(path)", category: .workspace)
        } else {
            // If this launch bootstrapped a default home workspace, repurpose it for the URL.
            if let bootstrapWorkspace = bootstrapWorkspaceForURLOpen() {
                bootstrapWorkspace.setWorkingDirectory(workspaceURL)
                NotificationCenter.default.post(
                    name: .activateWorkspace,
                    object: bootstrapWorkspace.id
                )
                RalphLogger.shared.info("Repurposed bootstrap workspace for URL: \(path)", category: .workspace)
                return
            }

            // Create new workspace with the specified directory
            let workspace = manager.createWorkspace(workingDirectory: workspaceURL)
            NotificationCenter.default.post(
                name: .workspaceOpenedFromURL,
                object: workspace.id
            )
            RalphLogger.shared.info("Created new workspace from URL: \(path)", category: .workspace)
        }
    }

    private func bootstrapWorkspaceForURLOpen() -> Workspace? {
        guard manager.workspaces.count == 1, let workspace = manager.workspaces.first else { return nil }

        let homePath = FileManager.default.homeDirectoryForCurrentUser
            .standardizedFileURL
            .resolvingSymlinksInPath()
            .path
        let workspacePath = workspace.workingDirectoryURL
            .standardizedFileURL
            .resolvingSymlinksInPath()
            .path
        guard workspacePath == homePath else { return nil }
        guard !workspace.hasRalphQueueFile else { return nil }

        return workspace
    }

    private var navigationCommands: some Commands {
        CommandMenu("Navigation") {
            Button("Show Queue") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.queue
                )
            }
            .keyboardShortcut("1", modifiers: .command)

            Button("Show Quick Actions") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.quickActions
                )
            }
            .keyboardShortcut("2", modifiers: .command)

            Button("Show Run Control") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.runControl
                )
            }
            .keyboardShortcut("3", modifiers: .command)

            Button("Show Advanced Runner") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.advancedRunner
                )
            }
            .keyboardShortcut("4", modifiers: .command)

            Button("Show Analytics") {
                NotificationCenter.default.post(
                    name: .showSidebarSection,
                    object: SidebarSection.analytics
                )
            }
            .keyboardShortcut("5", modifiers: .command)

            Divider()

            Button("Toggle Sidebar") {
                NotificationCenter.default.post(
                    name: .toggleSidebar,
                    object: nil
                )
            }
            .keyboardShortcut("s", modifiers: [.command, .control])

            Divider()

            Button("Toggle View Mode") {
                NotificationCenter.default.post(
                    name: .toggleTaskViewMode,
                    object: nil
                )
            }
            .keyboardShortcut("k", modifiers: [.command, .shift])

            Button("Show Graph View") {
                NotificationCenter.default.post(
                    name: .showGraphView,
                    object: nil
                )
            }
            .keyboardShortcut("g", modifiers: [.command, .shift])
        }
    }

    private var taskCommands: some Commands {
        CommandMenu("Task") {
            Button("New Task...") {
                NotificationCenter.default.post(
                    name: .showTaskCreation,
                    object: nil
                )
            }
            .keyboardShortcut("n", modifiers: [.command, .option])
            
            Divider()
            
            Button("Start Work") {
                NotificationCenter.default.post(
                    name: .startWorkOnSelectedTask,
                    object: nil
                )
            }
            .keyboardShortcut(.return, modifiers: .command)
            .help("Change selected task status to Doing (⌘Enter)")
            
            Divider()
            
            Button("Check for CLI Updates") {
                NotificationCenter.default.post(
                    name: .checkForCLIUpdates,
                    object: nil
                )
            }
        }
    }

    private var helpCommands: some Commands {
        CommandGroup(replacing: .help) {
            Button("Export Logs...") {
                exportLogs()
            }
            .keyboardShortcut("l", modifiers: [.command, .shift])
            
            Button("View Crash Reports...") {
                showCrashReports()
            }
            .keyboardShortcut("r", modifiers: [.command, .shift])
            
            Divider()

            if let docsURL = URL(string: "https://github.com/mitchfultz/ralph") {
                Link("Ralph Documentation", destination: docsURL)
            }
        }
    }

    private func exportLogs() {
        guard RalphLogger.shared.canExportLogs else {
            showAlert(title: "Not Available", message: "Log export requires macOS 12 or later.")
            return
        }
        
        RalphLogger.shared.exportLogs(hours: 24) { logContent in
            guard let logContent = logContent else {
                Task { @MainActor in
                    showAlert(title: "Export Failed", message: "Could not retrieve logs.")
                }
                return
            }
            
            // Show save panel on main thread
            Task { @MainActor in
                let savePanel = NSSavePanel()
                savePanel.nameFieldStringValue = "ralph-logs-\(Date().formatted(.iso8601.dateSeparator(.dash).timeSeparator(.omitted))).txt"
                savePanel.allowedContentTypes = [.plainText]
                
                let result = await savePanel.begin()
                if result == .OK, let url = savePanel.url {
                    try? logContent.write(to: url, atomically: true, encoding: .utf8)
                }
            }
        }
    }

    private func showCrashReports() {
        let reports = CrashReporter.shared.getAllReports()
        if reports.isEmpty {
            showAlert(title: "No Crash Reports", message: "No crash reports found.")
            return
        }
        
        let content = CrashReporter.shared.exportAllReports()
        
        // Show save panel on main thread
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

    private func showAlert(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.alertStyle = .informational
        alert.runModal()
    }
}

// MARK: - Workspace Commands

/// Window and tab management commands routed through focused workspace window actions.
@MainActor
private struct WorkspaceCommands: Commands {
    @FocusedValue(\.workspaceWindowActions) private var workspaceWindowActions

    private func routeWindowCommand(_ command: WindowCommandRoute) {
        workspaceWindowActions?.perform(command)
    }

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Divider()

            Button("Close Tab") {
                routeWindowCommand(.closeTab)
            }
            .keyboardShortcut("w", modifiers: .command)

            Button("Close Window") {
                routeWindowCommand(.closeWindow)
            }
            .keyboardShortcut("w", modifiers: [.command, .shift])
        }

        CommandMenu("Workspace") {
            Button("New Tab") {
                routeWindowCommand(.newTab)
            }
            .keyboardShortcut("t", modifiers: .command)

            Divider()

            Button("Close Tab") {
                routeWindowCommand(.closeTab)
            }

            Button("Close Window") {
                routeWindowCommand(.closeWindow)
            }

            Divider()

            Button("Next Tab") {
                routeWindowCommand(.nextTab)
            }
            .keyboardShortcut("]", modifiers: [.command, .shift])

            Button("Previous Tab") {
                routeWindowCommand(.previousTab)
            }
            .keyboardShortcut("[", modifiers: [.command, .shift])

            Divider()

            Button("Duplicate Tab") {
                routeWindowCommand(.duplicateTab)
            }
            .keyboardShortcut("d", modifiers: .command)
        }
    }
}

// MARK: - Command Palette Commands

/// Command palette commands routed through focused workspace UI actions.
@MainActor
private struct CommandPaletteCommands: Commands {
    @FocusedValue(\.workspaceUIActions) private var workspaceUIActions

    private var hasFocusedWorkspace: Bool {
        workspaceUIActions != nil
    }

    var body: some Commands {
        CommandMenu("Tools") {
            Button("Command Palette...") {
                workspaceUIActions?.showCommandPalette()
            }
            .keyboardShortcut("p", modifiers: [.command, .shift])
            .disabled(!hasFocusedWorkspace)

            Button("Quick Command...") {
                workspaceUIActions?.showCommandPalette()
            }
            .keyboardShortcut("k", modifiers: .command)
            .disabled(!hasFocusedWorkspace)
        }
    }
}

// MARK: - Window View Container

/// Container view that handles workspace initialization to avoid state mutation during view init.
/// Claims a unique saved window state per scene (if available) and persists that mapping via
/// scene storage to prevent multiple tabs/windows from sharing the same workspace state.
@MainActor
struct WindowViewContainer: View {
    private let manager = WorkspaceManager.shared
    @State private var windowState: WindowState?
    @State private var didResolveSceneWindowState = false
    @SceneStorage("windowStateID") private var persistedWindowStateID: String = ""
    @Environment(\.openWindow) private var openWindow

    private static let uiTestingWorkspacePathEnvKey = "RALPH_UI_TEST_WORKSPACE_PATH"
    private static let isUITestingLaunch = ProcessInfo.processInfo.arguments.contains("--uitesting")
    private static let isUITestingMultiwindowLaunch = ProcessInfo.processInfo.arguments.contains("--uitesting-multiwindow")

    var body: some View {
        Group {
            if let state = windowState {
                WindowView(windowState: state)
                    .background(UITestingWindowAnchor(isEnabled: Self.isUITestingLaunch))
            } else {
                ProgressView("Initializing...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task { @MainActor in
            initializeWindowStateIfNeeded()
            openAdditionalWindowForUITestingIfNeeded()
            closeUnexpectedWindowsForUITestingIfNeeded()
            // Initialize settings system (extension in ASettingsInfra.swift)
            SettingsService.initialize()
        }
    }

    private func initializeWindowStateIfNeeded() {
        guard !didResolveSceneWindowState else { return }

        if let uiTestingState = uiTestingWindowState() {
            windowState = uiTestingState
            persistedWindowStateID = ""
            didResolveSceneWindowState = true
            performInitialWorkspaceHealthCheck(for: uiTestingState)
            return
        }

        let preferredID = UUID(uuidString: persistedWindowStateID) ?? windowState?.id
        let claimedState = manager.claimWindowState(preferredID: preferredID)
        windowState = claimedState
        persistedWindowStateID = claimedState.id.uuidString
        manager.saveWindowState(claimedState)
        didResolveSceneWindowState = true
        performInitialWorkspaceHealthCheck(for: claimedState)
    }

    private func uiTestingWindowState() -> WindowState? {
        guard ProcessInfo.processInfo.arguments.contains("--uitesting") else { return nil }
        guard let rawPath = ProcessInfo.processInfo.environment[Self.uiTestingWorkspacePathEnvKey],
              !rawPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }

        let workspaceURL = URL(fileURLWithPath: rawPath, isDirectory: true)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let workspace = manager.createWorkspace(workingDirectory: workspaceURL)
        return WindowState(workspaceIDs: [workspace.id])
    }

    private func performInitialWorkspaceHealthCheck(for state: WindowState) {
        guard let firstWorkspaceID = state.workspaceIDs.first,
              let workspace = manager.workspaces.first(where: { $0.id == firstWorkspaceID }) else {
            return
        }

        Task { @MainActor in
            _ = await workspace.checkHealth()
            if workspace.showOfflineBanner {
                workspace.loadCachedTasks()
            }
        }
    }

    private func openAdditionalWindowForUITestingIfNeeded() {
        guard ProcessInfo.processInfo.arguments.contains("--uitesting-multiwindow") else {
            return
        }
        guard !UITestingWindowBootstrap.didOpenSecondaryWindow else {
            return
        }

        UITestingWindowBootstrap.didOpenSecondaryWindow = true
        openWindow(id: "main")
    }

    private func closeUnexpectedWindowsForUITestingIfNeeded() {
        guard Self.isUITestingLaunch else { return }

        let expectedWindowCount = Self.isUITestingMultiwindowLaunch ? 2 : 1
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) {
            let workspaceWindows = NSApp.windows
                .filter { $0.identifier?.rawValue.contains("AppWindow") == true }
                .sorted { $0.windowNumber < $1.windowNumber }

            guard workspaceWindows.count > expectedWindowCount else { return }
            for window in workspaceWindows.dropFirst(expectedWindowCount) {
                window.close()
            }
        }
    }
}

@MainActor
private enum UITestingWindowBootstrap {
    static var didOpenSecondaryWindow = false
}

/// Ensures UI-test windows are visible and frontmost before XCTest begins clicking screen coordinates.
private struct UITestingWindowAnchor: NSViewRepresentable {
    let isEnabled: Bool

    func makeNSView(context: Context) -> NSView {
        NSView(frame: .zero)
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard isEnabled else { return }

        DispatchQueue.main.async {
            guard let window = nsView.window else { return }
            configure(window: window)
        }
    }

    private func configure(window: NSWindow) {
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

        let width: CGFloat
        let height = max(700, min(900, visibleFrame.height - (verticalInset * 2)))
        let origin: NSPoint

        if windowCount > 1 {
            let preferredWidth = min(1200, max(1024, visibleFrame.width - 120))
            let sideBySideWidth = (visibleFrame.width - (horizontalSpacing * 3)) / 2

            if sideBySideWidth >= 1024 {
                width = min(preferredWidth, sideBySideWidth)
                let x = visibleFrame.minX + horizontalSpacing + CGFloat(min(windowIndex, 1)) * (width + horizontalSpacing)
                let y = visibleFrame.maxY - verticalInset - height
                origin = NSPoint(x: x, y: y)
            } else {
                // Keep the full NavigationSplitView visible even on smaller screens by cascading windows
                // instead of squeezing them below the sidebar/detail minimum widths.
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
            width = max(960, min(1400, visibleFrame.width - 80))
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
    }
}

// MARK: - Settings Service Protocol

/// Protocol for settings-related functionality.
/// Implementation is provided via extension in ASettingsInfra.swift
@MainActor
protocol SettingsServiceProtocol {
    static func initialize()
}

/// Stub type that will be extended in ASettingsInfra.swift
@MainActor
enum SettingsService: SettingsServiceProtocol {
    // Implementation provided in ASettingsInfra.swift via extension
}

// MARK: - Settings Button

/// Notification to show the settings window.
/// Posted by the settings button, observed by SettingsWindowController.
extension Notification.Name {
    static let showRalphSettings = Notification.Name("showRalphSettings")
}

/// Button that opens the settings window by posting a notification.
@MainActor
struct OpenSettingsButton: View {
    var body: some View {
        Button("Settings...") {
            NotificationCenter.default.post(name: .showRalphSettings, object: nil)
        }
        .keyboardShortcut(",", modifiers: .command)
    }
}
