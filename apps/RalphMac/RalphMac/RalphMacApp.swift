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
import RalphCore
import OSLog

@MainActor
@main
struct RalphMacApp: App {
    private let manager = WorkspaceManager.shared
    @State private var menuBarManager = MenuBarManager.shared
    @Environment(\.scenePhase) private var scenePhase
    
    init() {
        // Initialize crash reporter early in app lifecycle to catch launch crashes
        CrashReporter.shared.install()
    }

    var body: some Scene {
        WindowGroup {
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
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unified(showsTitle: false))
        .defaultSize(width: 1400, height: 900)
        .defaultPosition(.center)
        .commands {
            workspaceCommands
            navigationCommands
            taskCommands
            commandPaletteCommands
            helpCommands
        }
        
        // Menu Bar Extra for quick access
        MenuBarExtra(
            isInserted: $menuBarManager.isMenuBarExtraVisible,
            content: {
                MenuBarContentView()
            },
            label: {
                MenuBarIconView()
            }
        )
        .menuBarExtraStyle(.menu)
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

    private var workspaceCommands: some Commands {
        CommandMenu("Workspace") {
            Button("New Tab") {
                NotificationCenter.default.post(
                    name: .newWorkspaceTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("t", modifiers: .command)

            Button("New Window") {
                NotificationCenter.default.post(
                    name: .newWindowRequested,
                    object: nil
                )
            }
            .keyboardShortcut("n", modifiers: [.command, .shift])

            Divider()

            Button("Close Tab") {
                NotificationCenter.default.post(
                    name: .closeActiveTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("w", modifiers: .command)

            Button("Close Window") {
                NotificationCenter.default.post(
                    name: .closeActiveWindowRequested,
                    object: nil
                )
            }
            .keyboardShortcut("w", modifiers: [.command, .shift])

            Divider()

            Button("Next Tab") {
                NotificationCenter.default.post(
                    name: .selectNextTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("]", modifiers: [.command, .shift])

            Button("Previous Tab") {
                NotificationCenter.default.post(
                    name: .selectPreviousTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("[", modifiers: [.command, .shift])

            Divider()

            Button("Duplicate Tab") {
                NotificationCenter.default.post(
                    name: .duplicateActiveTabRequested,
                    object: nil
                )
            }
            .keyboardShortcut("d", modifiers: .command)
        }
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

    private var commandPaletteCommands: some Commands {
        CommandMenu("Tools") {
            Button("Command Palette...") {
                NotificationCenter.default.post(name: .showCommandPalette, object: nil)
            }
            .keyboardShortcut("p", modifiers: [.command, .shift])
            
            Button("Quick Command...") {
                NotificationCenter.default.post(name: .showCommandPalette, object: nil)
            }
            .keyboardShortcut("k", modifiers: .command)
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

    var body: some View {
        Group {
            if let state = windowState {
                WindowView(windowState: state)
            } else {
                ProgressView("Initializing...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .onAppear {
                        initializeWindowStateIfNeeded()
                    }
                    .task { @MainActor in
                        initializeWindowStateIfNeeded()
                    }
            }
        }
        .onAppear {
            initializeWindowStateIfNeeded()
        }
    }

    private func initializeWindowStateIfNeeded() {
        guard !didResolveSceneWindowState else { return }

        let preferredID = UUID(uuidString: persistedWindowStateID) ?? windowState?.id
        let claimedState = manager.claimWindowState(preferredID: preferredID)
        windowState = claimedState
        persistedWindowStateID = claimedState.id.uuidString
        manager.saveWindowState(claimedState)
        didResolveSceneWindowState = true

        // Perform health check after workspace is set up.
        if let firstWorkspaceID = claimedState.workspaceIDs.first,
           let workspace = manager.workspaces.first(where: { $0.id == firstWorkspaceID }) {
            Task { @MainActor in
                _ = await workspace.checkHealth()
                // Load cached tasks if CLI is unavailable.
                if workspace.showOfflineBanner {
                    workspace.loadCachedTasks()
                }
            }
        }
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let newWorkspaceTabRequested = Notification.Name("newWorkspaceTabRequested")
    static let newWindowRequested = Notification.Name("newWindowRequested")
    static let closeActiveTabRequested = Notification.Name("closeActiveTabRequested")
    static let closeActiveWindowRequested = Notification.Name("closeActiveWindowRequested")
    static let selectNextTabRequested = Notification.Name("selectNextTabRequested")
    static let selectPreviousTabRequested = Notification.Name("selectPreviousTabRequested")
    static let duplicateActiveTabRequested = Notification.Name("duplicateActiveTabRequested")
    static let showTaskCreation = Notification.Name("showTaskCreation")
    static let checkForCLIUpdates = Notification.Name("checkForCLIUpdates")
    static let startWorkOnSelectedTask = Notification.Name("startWorkOnSelectedTask")
    // New notifications for URL handling
    static let activateWorkspace = Notification.Name("activateWorkspace")
    static let workspaceOpenedFromURL = Notification.Name("workspaceOpenedFromURL")
    static let saveAllWindowStatesRequested = Notification.Name("saveAllWindowStatesRequested")
    static let showCommandPalette = Notification.Name("showCommandPalette")
}
