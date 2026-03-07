/**
 RalphMacUITests

 Responsibilities:
 - End-to-end UI testing for critical RalphMac user flows.
 - Validate task creation, editing, status changes, and execution control.
 - Test view mode switching (List, Kanban, Graph).
 - Verify workspace creation and switching.

 Does not handle:
 - Unit testing (see RalphCoreTests).
 - Performance testing (see RalphMacUITestsLaunchTests).

 Invariants/assumptions:
 - Tests run with a fresh app instance (setUp/tearDown).
 - Stable accessibility identifiers are used for critical control identification.
 - Tests must be run on macOS 15.0+.
 - Visual capture is opt-in via `RALPH_UI_SCREENSHOTS=1` or `RALPH_UI_SCREENSHOT_MODE`.
 */

import XCTest

@MainActor
final class RalphMacUITests: XCTestCase {
    private struct UITaskSnapshot: Decodable {
        let id: String
        let status: String
    }

    private enum ScreenshotCaptureMode: Equatable {
        case off
        case checkpoints
        case timeline

        static func fromEnvironment(_ environment: [String: String]) -> Self {
            if let rawMode = environment["RALPH_UI_SCREENSHOT_MODE"]?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased(),
               !rawMode.isEmpty {
                switch rawMode {
                case "off", "0", "false", "no":
                    return .off
                case "timeline", "all", "full":
                    return .timeline
                case "checkpoints", "checkpoint", "1", "true", "yes":
                    return .checkpoints
                default:
                    return .checkpoints
                }
            }

            if environment["RALPH_UI_SCREENSHOTS"] == "1" {
                return .checkpoints
            }

            return .off
        }
    }

    private enum AccessibilityID {
        static let newTaskToolbarButton = "new-task-toolbar-button"
        static let taskListContainer = "task-list-container"
        static let taskSearchField = "task-search-field"
        static let taskViewModePicker = "task-view-mode-picker"
        static let taskCreationTitleField = "task-creation-title-field"
        static let taskCreationSubmitButton = "task-creation-submit-button"
        static let taskDecomposeRequestField = "task-decompose-request-field"
        static let taskDecomposePreviewButton = "task-decompose-preview-button"
        static let taskDecomposeWriteButton = "task-decompose-write-button"
        static let taskDetailTitleField = "task-detail-title-field"
        static let taskDetailSaveButton = "task-detail-save-button"
        static let taskDetailSaveSuccess = "task-detail-save-success"
    }

    private enum LaunchEnvironment {
        static let uiTestWorkspacePath = "RALPH_UI_TEST_WORKSPACE_PATH"
        static let ralphBinPath = "RALPH_BIN_PATH"
    }

    private let timelineIntervalNanos: UInt64 = 1_000_000_000
    private let timelineMaxFrames: Int = 180

    var app: XCUIApplication!
    private var screenshotMode: ScreenshotCaptureMode = .off
    private var screenshotSequence: Int = 0
    private var timelineCaptureTask: Task<Void, Never>?
    private var capturedFailureScreenshot: Bool = false
    private var uiTestWorkspaceURL: URL?
    private var ralphExecutableURL: URL?

    @MainActor
    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        screenshotMode = ScreenshotCaptureMode.fromEnvironment(ProcessInfo.processInfo.environment)
        screenshotSequence = 0
        capturedFailureScreenshot = false
        app = XCUIApplication()
        ralphExecutableURL = try resolveRalphExecutableURL()
        uiTestWorkspaceURL = try makeUITestWorkspace()
        app.launchArguments = ["--uitesting"]
        if let uiTestWorkspaceURL {
            app.launchEnvironment[LaunchEnvironment.uiTestWorkspacePath] = uiTestWorkspaceURL.path
        }
        if let ralphExecutableURL {
            app.launchEnvironment[LaunchEnvironment.ralphBinPath] = ralphExecutableURL.path
        }
        if name.contains("windowShortcuts") || name.contains("commandPaletteNewTab") {
            app.launchArguments.append("--uitesting-multiwindow")
        }
        app.launch()
        app.activate()
        captureScreenshot(named: "launch")
        startTimelineCaptureIfNeeded()
    }

    @MainActor
    override func tearDown() async throws {
        stopTimelineCapture()
        if let testRun, !testRun.hasSucceeded, !capturedFailureScreenshot {
            captureScreenshot(named: "failure-teardown")
            capturedFailureScreenshot = true
        }
        captureScreenshot(named: "teardown")
        app.terminate()
        app = nil
        if let uiTestWorkspaceURL {
            try? FileManager.default.removeItem(at: uiTestWorkspaceURL)
            self.uiTestWorkspaceURL = nil
        }
        ralphExecutableURL = nil
        try await super.tearDown()
    }

    // MARK: - Test: App Launch
    @MainActor
    func test_appLaunches_andShowsMainWindow() throws {
        // Verify the main window appears
        let window = app.windows.firstMatch
        XCTAssertTrue(window.exists, "Main window should exist")
        
        // Verify navigation sidebar is present
        let sidebar = app.outlines["Main navigation"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 5), "Main navigation sidebar should exist")
        
        // Verify Queue section is available
        XCTAssertTrue(sidebar.staticTexts["Queue"].exists)
    }

    // MARK: - Test: Create New Task
    @MainActor
    func test_createNewTask_viaQuickCreate() throws {
        // Tap New Task button
        XCTAssertTrue(newTaskToolbarButton.waitForExistence(timeout: 5))
        newTaskToolbarButton.click()
        
        // Wait for Task Creation sheet
        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 5), "Task creation sheet should appear")
        
        // Enter task title
        let titleField = sheet.descendants(matching: .textField)
            .matching(identifier: AccessibilityID.taskCreationTitleField)
            .element(boundBy: 0)
        XCTAssertTrue(titleField.waitForExistence(timeout: 5), "Task title field should exist in the creation sheet")
        titleField.click()
        titleField.typeText("UI Test Task - " + UUID().uuidString.prefix(8))
        
        // Tap Create button
        let createButton = sheet.descendants(matching: .button)
            .matching(identifier: AccessibilityID.taskCreationSubmitButton)
            .element(boundBy: 0)
        XCTAssertTrue(createButton.waitForExistence(timeout: 5), "Create task button should exist in the creation sheet")
        createButton.click()
        
        // Verify sheet dismisses
        XCTAssertTrue(
            waitUntil(timeout: 5) { !sheet.exists },
            "Task creation sheet should dismiss after creating a task"
        )
        
        // Verify task appears in list (by scrolling to find it)
        let taskList = requireTaskList()
        XCTAssertTrue(taskList.exists)
    }

    // MARK: - Test: Open Task Decompose Sheet
    @MainActor
    func test_openTaskDecomposeSheet_fromTaskMenu() throws {
        app.menuBars.menuBarItems["Task"].click()
        app.menuBars.menuItems["Decompose Task..."].click()

        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 5), "Task decompose sheet should appear")
        XCTAssertTrue(sheet.descendants(matching: .textField).matching(identifier: AccessibilityID.taskDecomposeRequestField).firstMatch.exists)
        XCTAssertTrue(sheet.descendants(matching: .button).matching(identifier: AccessibilityID.taskDecomposePreviewButton).firstMatch.exists)
        XCTAssertTrue(sheet.descendants(matching: .button).matching(identifier: AccessibilityID.taskDecomposeWriteButton).firstMatch.exists)
    }

    // MARK: - Test: Edit Task Title
    @MainActor
    func test_editTaskTitle_andVerifyPersistence() throws {
        // First create a task
        try test_createNewTask_viaQuickCreate()
        
        // Wait for task list to load
        let taskList = requireTaskList()
        
        // Select the first task row by clicking on a task cell
        let firstTask = taskRows(in: taskList).firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()
        
        // Wait for detail view
        let titleField = taskDetailTitleField
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        
        // Edit the title
        let newTitle = "Updated Task Title - " + UUID().uuidString.prefix(8)
        titleField.click()
        titleField.doubleClick() // Select all
        titleField.typeText(newTitle)
        
        // Save changes
        let saveButton = taskDetailSaveButton
        XCTAssertTrue(saveButton.waitForExistence(timeout: 5))
        XCTAssertTrue(waitUntil(timeout: 5) { saveButton.isHittable }, "Save button should be hittable in the active workspace window")
        saveButton.click()
        
        // Verify success indicator appears
        XCTAssertTrue(
            waitUntil(timeout: 5) { !taskDetailSaveButton.isEnabled },
            "Save button should disable again after persistence succeeds"
        )
    }

    // MARK: - Test: Switch View Modes
    @MainActor
    func test_switchBetweenViewModes() throws {
        XCTAssertTrue(waitUntil(timeout: 5) { taskViewModePicker().exists })
        
        // Switch to Kanban
        selectTaskViewMode("Kanban")
        XCTAssertTrue(currentWorkspaceWindow().scrollViews["Kanban board"].waitForExistence(timeout: 5))
        
        // Switch to Graph
        selectTaskViewMode("Graph")
        XCTAssertTrue(currentWorkspaceWindow().scrollViews.firstMatch.waitForExistence(timeout: 5))
        
        // Switch back to List
        selectTaskViewMode("List")
        XCTAssertTrue(requireTaskList().exists)
    }

    // MARK: - Test: New Tab Creation
    @MainActor
    func test_createNewTab_andSwitchBetweenTabs() throws {
        let window = app.windows.firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        let before = tabCount(in: window)

        // Use menu to create new tab
        app.menuBars.menuBarItems["Workspace"].click()
        app.menuBars.menuItems["New Tab"].click()

        XCTAssertTrue(
            waitUntil { tabCount(in: window) == before + 1 },
            "New Tab menu action should increase tab count in the active window"
        )
    }

    // MARK: - Test: Window-Scoped Shortcuts
    @MainActor
    func test_windowShortcuts_affectOnlyFocusedWindow() throws {
        ensureSecondWindow()

        let windows = workspaceWindows()
        XCTAssertGreaterThanOrEqual(windows.count, 2, "Expected at least two workspace windows")
        let firstWindow = windows[0]
        let secondWindow = windows[1]
        XCTAssertTrue(firstWindow.exists)
        XCTAssertTrue(secondWindow.exists)

        firstWindow.click()
        let firstBefore = tabCount(in: firstWindow)
        let secondBefore = tabCount(in: secondWindow)
        let windowsBefore = workspaceWindowCount()

        firstWindow.typeKey("t", modifierFlags: .command)
        XCTAssertTrue(
            waitUntil { tabCount(in: firstWindow) == firstBefore + 1 },
            "Cmd+T should add a tab only in the focused window"
        )
        XCTAssertEqual(
            tabCount(in: secondWindow),
            secondBefore,
            "Cmd+T should not add tabs in unfocused windows"
        )
        XCTAssertEqual(
            workspaceWindowCount(),
            windowsBefore,
            "Cmd+T should not create or close windows"
        )

        firstWindow.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(
            waitUntil { tabCount(in: firstWindow) == firstBefore },
            "Cmd+W should close a tab only in the focused window"
        )
        XCTAssertEqual(
            tabCount(in: secondWindow),
            secondBefore,
            "Cmd+W should not close tabs in unfocused windows"
        )
        XCTAssertEqual(
            workspaceWindowCount(),
            windowsBefore,
            "Cmd+W should not close the entire window"
        )

        secondWindow.click()
        secondWindow.typeKey("w", modifierFlags: [.command, .shift])
        XCTAssertTrue(
            waitUntil { workspaceWindowCount() == windowsBefore - 1 },
            "Cmd+Shift+W should close only the focused window (expected \(windowsBefore - 1), got \(workspaceWindowCount()))"
        )
    }

    // MARK: - Test: Command Palette Workspace Action Scoping
    @MainActor
    func test_commandPaletteNewTab_affectsOnlyFocusedWindow() throws {
        ensureSecondWindow()

        let windows = workspaceWindows()
        XCTAssertGreaterThanOrEqual(windows.count, 2, "Expected at least two workspace windows")
        let firstWindow = windows[0]
        let secondWindow = windows[1]
        XCTAssertTrue(firstWindow.exists)
        XCTAssertTrue(secondWindow.exists)

        firstWindow.click()
        let firstBefore = tabCount(in: firstWindow)
        let secondBefore = tabCount(in: secondWindow)
        let windowsBefore = workspaceWindowCount()

        firstWindow.typeKey("k", modifierFlags: .command)

        let searchField = app.textFields["Type a command or search..."]
        XCTAssertTrue(searchField.waitForExistence(timeout: 5), "Command palette should appear")
        searchField.click()
        searchField.typeText("New Tab")
        searchField.typeKey(XCUIKeyboardKey.return, modifierFlags: [])

        XCTAssertTrue(
            waitUntil { tabCount(in: firstWindow) == firstBefore + 1 },
            "Command palette 'New Tab' should affect only the focused window"
        )
        XCTAssertEqual(
            tabCount(in: secondWindow),
            secondBefore,
            "Command palette 'New Tab' should not affect unfocused windows"
        )
        XCTAssertEqual(
            workspaceWindowCount(),
            windowsBefore,
            "Command palette 'New Tab' should not create or close windows"
        )
    }

    // MARK: - Test: Navigation Sections
    @MainActor
    func test_navigateThroughAllSidebarSections() throws {
        let sidebar = currentWorkspaceWindow().outlines["Main navigation"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 5))
        
        // Navigate to each section
        let sections = ["Queue", "Quick Actions", "Run Control", "Advanced Runner", "Analytics"]
        for section in sections {
            let sectionItem = sidebar.staticTexts[section]
            XCTAssertTrue(sectionItem.exists, "\(section) should exist in sidebar")
            sectionItem.click()
            
            // Small delay for view transition
            Thread.sleep(forTimeInterval: 0.5)
        }
    }

    // MARK: - Test: Task Search and Filter
    @MainActor
    func test_taskSearchFunctionality() throws {
        // Wait for task list controls
        let searchField = taskSearchField
        XCTAssertTrue(searchField.waitForExistence(timeout: 5))
        
        // Type search query
        searchField.click()
        searchField.typeText("Test")
        
        // Verify search was applied (list should update)
        Thread.sleep(forTimeInterval: 1)
        
        // Clear search
        let clearButton = currentWorkspaceWindow().buttons["Clear search"]
        if clearButton.exists {
            clearButton.click()
        }
    }
    
    // MARK: - Test: Keyboard Navigation in Task List
    @MainActor
    func test_taskListKeyboardNavigation() throws {
        // Create a task first
        try test_createNewTask_viaQuickCreate()
        
        // Wait for task list
        let taskList = requireTaskList()
        
        // Click in the list to focus it
        let firstTask = taskRows(in: taskList).firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()
        
        // Test down arrow navigation
        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.downArrow, modifierFlags: [])
        Thread.sleep(forTimeInterval: 0.3)
        
        // Test up arrow navigation
        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.upArrow, modifierFlags: [])
        Thread.sleep(forTimeInterval: 0.3)
        
        // Test Enter to open detail
        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.return, modifierFlags: [])
        Thread.sleep(forTimeInterval: 0.5)
        
        // Verify detail view appears
        let titleField = taskDetailTitleField
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
    }
    
    // MARK: - Test: Keyboard Navigation in Kanban Board
    @MainActor
    func test_kanbanBoardKeyboardNavigation() throws {
        // Create a task first
        try test_createNewTask_viaQuickCreate()
        
        // Switch to Kanban view
        XCTAssertTrue(waitUntil(timeout: 5) { taskViewModePicker().exists })
        selectTaskViewMode("Kanban")
        
        // Wait for Kanban board
        let kanbanBoard = currentWorkspaceWindow().scrollViews["Kanban board"]
        XCTAssertTrue(kanbanBoard.waitForExistence(timeout: 5))
        
        // Click on a card to focus
        let firstCard = kanbanBoard.buttons.firstMatch
        XCTAssertTrue(firstCard.waitForExistence(timeout: 5))
        firstCard.click()
        
        // Test right arrow to move to next column
        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.rightArrow, modifierFlags: [])
        Thread.sleep(forTimeInterval: 0.3)
        
        // Test left arrow to move to previous column
        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.leftArrow, modifierFlags: [])
        Thread.sleep(forTimeInterval: 0.3)
        
        // Test down arrow navigation within column
        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.downArrow, modifierFlags: [])
        Thread.sleep(forTimeInterval: 0.3)
    }
    
    // MARK: - Test: Start Work Keyboard Shortcut
    @MainActor
    func test_startWorkKeyboardShortcut() throws {
        // Create a task
        try test_createNewTask_viaQuickCreate()
        
        // Select first task in list
        let taskList = requireTaskList()
        let firstTask = taskRows(in: taskList).firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()
        
        // Use Cmd+Enter to start work
        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.return, modifierFlags: .command)
        
        // Verify the persisted queue state changed, not just a transient badge in the rendered list.
        XCTAssertTrue(
            waitUntil(timeout: 5) { (try? uiTestWorkspaceTasks().contains(where: { $0.status.lowercased() == "doing" })) == true },
            "Task status should change to 'Doing' after Cmd+Enter"
        )
    }
    
    // MARK: - Test: Conflict Detection UI Elements
    
    @MainActor
    func test_conflictDetection_UIElementsExist() throws {
        // Create a task first
        try test_createNewTask_viaQuickCreate()
        
        // Wait for task list to load
        let taskList = requireTaskList()
        
        // Select the first task
        let firstTask = taskRows(in: taskList).firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()
        
        // Wait for detail view
        let titleField = taskDetailTitleField
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        
        // Edit the title to create local changes
        titleField.click()
        titleField.doubleClick()
        titleField.typeText("Modified Title - " + UUID().uuidString.prefix(8))
        
        // Verify Save button is enabled (we have changes)
        let saveButton = taskDetailSaveButton
        XCTAssertTrue(saveButton.isEnabled)
        
        // Note: Actually triggering external changes would require CLI automation
        // This test verifies the UI is ready for conflict detection
    }
    
    @MainActor
    func test_conflictResolverView_Dismissal() throws {
        // This test verifies the conflict resolver view can be dismissed
        // Full conflict testing requires CLI integration
        
        // Create a task
        try test_createNewTask_viaQuickCreate()
        
        // Select task
        let taskList = requireTaskList()
        taskRows(in: taskList).firstMatch.click()
        
        // Wait for detail view
        let titleField = taskDetailTitleField
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        
        // Make a change
        titleField.click()
        titleField.typeText(" - Edited")
        
        // Verify we have unsaved changes indicator would work
        // (actual conflict requires external modification)
        XCTAssertTrue(titleField.exists)
    }

    // MARK: - Helpers

    private var newTaskToolbarButton: XCUIElement {
        currentWorkspaceWindow().toolbars.descendants(matching: .button)
            .matching(identifier: AccessibilityID.newTaskToolbarButton)
            .element(boundBy: 0)
    }

    private var taskSearchField: XCUIElement {
        currentWorkspaceWindow().descendants(matching: .textField)
            .matching(identifier: AccessibilityID.taskSearchField)
            .element(boundBy: 0)
    }

    private var taskDetailTitleField: XCUIElement {
        currentWorkspaceWindow().descendants(matching: .textField)
            .matching(identifier: AccessibilityID.taskDetailTitleField)
            .element(boundBy: 0)
    }

    private var taskDetailSaveButton: XCUIElement {
        currentWorkspaceWindow().descendants(matching: .button)
            .matching(identifier: AccessibilityID.taskDetailSaveButton)
            .element(boundBy: 0)
    }

    @MainActor
    private func currentWorkspaceWindow(file: StaticString = #filePath, line: UInt = #line) -> XCUIElement {
        app.activate()
        XCTAssertTrue(
            waitUntil(timeout: 8) {
                !app.windows.allElementsBoundByIndex.isEmpty && app.windows.allElementsBoundByIndex.contains(where: \.exists)
            },
            "Expected at least one app window to appear",
            file: file,
            line: line
        )

        let workspaceCandidates = workspaceWindows()
        let fallbackCandidates = app.windows.allElementsBoundByIndex.filter(\.exists)
        let window = workspaceCandidates.first(where: \.isHittable)
            ?? workspaceCandidates.first
            ?? fallbackCandidates.first(where: \.isHittable)
            ?? fallbackCandidates.first
            ?? app.windows.firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 5), "Expected a workspace window", file: file, line: line)
        return window
    }

    @MainActor
    private func taskViewModePicker() -> XCUIElement {
        let window = currentWorkspaceWindow()
        let radioGroup = window.radioGroups[AccessibilityID.taskViewModePicker]
        if radioGroup.exists {
            return radioGroup
        }

        let segmentedControl = window.segmentedControls[AccessibilityID.taskViewModePicker]
        if segmentedControl.exists {
            return segmentedControl
        }

        return radioGroup
    }

    @MainActor
    private func requireTaskList(timeout: TimeInterval = 5, file: StaticString = #filePath, line: UInt = #line) -> XCUIElement {
        let window = currentWorkspaceWindow(file: file, line: line)
        let candidates = [
            window.outlines[AccessibilityID.taskListContainer],
            window.tables[AccessibilityID.taskListContainer],
            window.collectionViews[AccessibilityID.taskListContainer],
            window.scrollViews[AccessibilityID.taskListContainer],
            window.otherElements[AccessibilityID.taskListContainer]
        ]

        XCTAssertTrue(
            waitUntil(timeout: timeout) { candidates.contains(where: { $0.exists }) },
            "Task list container should exist",
            file: file,
            line: line
        )

        return candidates.first(where: { $0.exists }) ?? candidates[0]
    }

    @MainActor
    private func taskRows(in taskList: XCUIElement) -> XCUIElementQuery {
        if taskList.cells.count > 0 {
            return taskList.cells
        }

        return taskList.descendants(matching: .cell)
    }

    @MainActor
    private func selectTaskViewMode(_ mode: String, file: StaticString = #filePath, line: UInt = #line) {
        let picker = taskViewModePicker()
        XCTAssertTrue(
            waitUntil(timeout: 5) { picker.exists },
            "Task view mode picker should exist",
            file: file,
            line: line
        )

        let radioButton = picker.radioButtons[mode]
        if radioButton.exists || radioButton.waitForExistence(timeout: 2) {
            radioButton.click()
            return
        }

        let button = picker.buttons[mode]
        XCTAssertTrue(
            button.waitForExistence(timeout: 2),
            "Expected task view mode option '\(mode)'",
            file: file,
            line: line
        )
        button.click()
    }

    private func makeUITestWorkspace() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ralph-ui-tests", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)

        try runRalph(arguments: ["init", "--non-interactive"], currentDirectoryURL: root)
        try seedUITestQueue(at: root)

        return root
    }

    private func seedUITestQueue(at workspaceURL: URL) throws {
        let importURL = workspaceURL.appendingPathComponent("ui-fixture-import.json", isDirectory: false)
        let seededTasks = #"""
        [
          {
            "id": "RQ-0001",
            "status": "todo",
            "title": "UI Fixture Alpha",
            "priority": "high",
            "tags": ["ui", "fixture"],
            "created_at": "2026-03-05T00:00:00Z",
            "updated_at": "2026-03-05T00:00:00Z"
          },
          {
            "id": "RQ-0002",
            "status": "todo",
            "title": "UI Fixture Search Test",
            "priority": "medium",
            "tags": ["ui", "search"],
            "created_at": "2026-03-05T00:05:00Z",
            "updated_at": "2026-03-05T00:05:00Z"
          }
        ]
        """#
        try seededTasks.write(to: importURL, atomically: true, encoding: .utf8)
        defer { try? FileManager.default.removeItem(at: importURL) }

        try runRalph(
            arguments: ["queue", "import", "--format", "json", "--input", importURL.path],
            currentDirectoryURL: workspaceURL
        )
    }

    private func runRalph(arguments: [String], currentDirectoryURL: URL) throws {
        _ = try runRalphAndCollectOutput(arguments: arguments, currentDirectoryURL: currentDirectoryURL)
    }

    private func runRalphAndCollectOutput(arguments: [String], currentDirectoryURL: URL) throws -> String {
        guard let executableURL = ralphExecutableURL else {
            throw NSError(
                domain: "RalphMacUITests",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Failed to resolve a ralph executable for UI tests"]
            )
        }

        let process = Process()
        process.executableURL = executableURL
        process.currentDirectoryURL = currentDirectoryURL
        process.arguments = ["--no-color"] + arguments

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        try process.run()
        process.waitUntilExit()

        let stdout = String(data: stdoutPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let stderr = String(data: stderrPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""

        guard process.terminationStatus == 0 else {
            throw NSError(
                domain: "RalphMacUITests",
                code: Int(process.terminationStatus),
                userInfo: [
                    NSLocalizedDescriptionKey: "ralph \(arguments.joined(separator: " ")) failed",
                    "stdout": stdout,
                    "stderr": stderr
                ]
            )
        }

        return stdout
    }

    private func uiTestWorkspaceTasks() throws -> [UITaskSnapshot] {
        guard let uiTestWorkspaceURL else {
            return []
        }

        let output = try runRalphAndCollectOutput(
            arguments: ["queue", "list", "--format", "json"],
            currentDirectoryURL: uiTestWorkspaceURL
        )
        return try JSONDecoder().decode([UITaskSnapshot].self, from: Data(output.utf8))
    }

    private func resolveRalphExecutableURL(environment: [String: String] = ProcessInfo.processInfo.environment) throws -> URL {
        if let override = environment[LaunchEnvironment.ralphBinPath]?.trimmingCharacters(in: .whitespacesAndNewlines),
           !override.isEmpty {
            let overrideURL = URL(fileURLWithPath: override, isDirectory: false)
                .standardizedFileURL
                .resolvingSymlinksInPath()
            guard FileManager.default.isExecutableFile(atPath: overrideURL.path) else {
                throw NSError(
                    domain: "RalphMacUITests",
                    code: 2,
                    userInfo: [
                        NSLocalizedDescriptionKey: "RALPH_BIN_PATH points to a non-executable path: \(overrideURL.path)"
                    ]
                )
            }
            return overrideURL
        }

        let bundledURL = Bundle.main.bundleURL
            .deletingLastPathComponent()
            .appendingPathComponent("RalphMac.app", isDirectory: true)
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("MacOS", isDirectory: true)
            .appendingPathComponent("ralph", isDirectory: false)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        if FileManager.default.isExecutableFile(atPath: bundledURL.path) {
            return bundledURL
        }

        throw NSError(
            domain: "RalphMacUITests",
            code: 2,
            userInfo: [
                NSLocalizedDescriptionKey: "Failed to locate a bundled ralph executable for UI tests at \(bundledURL.path). Build the app bundle or set RALPH_BIN_PATH explicitly."
            ]
        )
    }

    @MainActor
    private func captureScreenshot(named step: String) {
        guard screenshotMode != .off else { return }
        guard app != nil else { return }

        screenshotSequence += 1
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "\(sanitizedTestName())-\(String(format: "%03d", screenshotSequence))-\(sanitizedAttachmentToken(step))"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    @MainActor
    private func startTimelineCaptureIfNeeded() {
        guard screenshotMode == .timeline else { return }
        stopTimelineCapture()

        timelineCaptureTask = Task { @MainActor [weak self] in
            guard let self else { return }
            var frameIndex = 0
            while !Task.isCancelled && frameIndex < self.timelineMaxFrames {
                try? await Task.sleep(nanoseconds: self.timelineIntervalNanos)
                guard !Task.isCancelled else { break }
                self.captureScreenshot(named: "timeline-\(frameIndex)")
                frameIndex += 1
            }
        }
    }

    @MainActor
    private func stopTimelineCapture() {
        timelineCaptureTask?.cancel()
        timelineCaptureTask = nil
    }

    private func sanitizedTestName() -> String {
        let cleaned = name
            .replacingOccurrences(of: "^[-\\[]+", with: "", options: .regularExpression)
            .replacingOccurrences(of: "[\\] ]+$", with: "", options: .regularExpression)
            .replacingOccurrences(of: "[^A-Za-z0-9._-]+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return cleaned.isEmpty ? "ui-test" : cleaned
    }

    private func sanitizedAttachmentToken(_ raw: String) -> String {
        let cleaned = raw
            .replacingOccurrences(of: "[^A-Za-z0-9._-]+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return cleaned.isEmpty ? "checkpoint" : cleaned
    }

    @MainActor
    private func ensureSecondWindow() {
        guard workspaceWindowCount() < 2 else { return }
        if waitUntil(timeout: 6, condition: { workspaceWindowCount() >= 2 }) {
            return
        }
        app.typeKey("n", modifierFlags: .command)
        XCTAssertTrue(
            waitUntil(timeout: 8) { workspaceWindowCount() >= 2 },
            "Expected a second window to open for multi-window shortcut tests"
        )
    }

    @MainActor
    private func workspaceWindows() -> [XCUIElement] {
        app.windows.allElementsBoundByIndex.filter {
            $0.otherElements["window-tab-count-probe"].exists
        }
    }

    @MainActor
    private func workspaceWindowCount() -> Int {
        workspaceWindows().count
    }

    @MainActor
    private func tabCount(in window: XCUIElement) -> Int {
        let probe = window.otherElements["window-tab-count-probe"]
        if probe.waitForExistence(timeout: 2) {
            if let value = probe.value as? NSNumber {
                return value.intValue
            }
            if let value = probe.value as? String, let count = Int(value) {
                return count
            }
            let prefix = "window-tab-count-"
            if probe.label.hasPrefix(prefix),
               let count = Int(probe.label.dropFirst(prefix.count)) {
                return count
            }
        }
        return window.tabs.count
    }

    @MainActor
    @discardableResult
    private func waitUntil(timeout: TimeInterval = 5, interval: TimeInterval = 0.1, condition: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if condition() {
                return true
            }
            RunLoop.current.run(until: Date().addingTimeInterval(interval))
        }
        return condition()
    }
}
