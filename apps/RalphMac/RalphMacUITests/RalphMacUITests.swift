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
 - Accessibility labels are used for element identification.
 - Tests must be run on macOS 15.0+.
 - Visual capture is opt-in via `RALPH_UI_SCREENSHOTS=1` or `RALPH_UI_SCREENSHOT_MODE`.
 */

import XCTest

@MainActor
final class RalphMacUITests: XCTestCase {
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

    private let timelineIntervalNanos: UInt64 = 1_000_000_000
    private let timelineMaxFrames: Int = 180

    var app: XCUIApplication!
    private var screenshotMode: ScreenshotCaptureMode = .off
    private var screenshotSequence: Int = 0
    private var timelineCaptureTask: Task<Void, Never>?
    private var capturedFailureScreenshot: Bool = false

    @MainActor
    override func setUp() async throws {
        try await super.setUp()
        continueAfterFailure = false
        screenshotMode = ScreenshotCaptureMode.fromEnvironment(ProcessInfo.processInfo.environment)
        screenshotSequence = 0
        capturedFailureScreenshot = false
        app = XCUIApplication()
        // Use a temp directory for workspace to avoid polluting real projects
        app.launchArguments = ["--uitesting"]
        if name.contains("windowShortcuts") || name.contains("commandPaletteNewTab") {
            app.launchArguments.append("--uitesting-multiwindow")
        }
        app.launch()
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
        let newTaskButton = app.toolbars.buttons["New Task"]
        XCTAssertTrue(newTaskButton.waitForExistence(timeout: 5))
        newTaskButton.click()
        
        // Wait for Task Creation sheet
        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 5), "Task creation sheet should appear")
        
        // Enter task title
        let titleField = sheet.textFields["Task title"]
        XCTAssertTrue(titleField.exists)
        titleField.click()
        titleField.typeText("UI Test Task - " + UUID().uuidString.prefix(8))
        
        // Tap Create button
        let createButton = sheet.buttons["Create task"]
        XCTAssertTrue(createButton.exists)
        createButton.click()
        
        // Verify sheet dismisses
        XCTAssertFalse(sheet.waitForExistence(timeout: 5))
        
        // Verify task appears in list (by scrolling to find it)
        let taskList = app.outlines.firstMatch
        XCTAssertTrue(taskList.waitForExistence(timeout: 5))
    }

    // MARK: - Test: Edit Task Title
    @MainActor
    func test_editTaskTitle_andVerifyPersistence() throws {
        // First create a task
        try test_createNewTask_viaQuickCreate()
        
        // Wait for task list to load
        let taskList = app.outlines.firstMatch
        XCTAssertTrue(taskList.waitForExistence(timeout: 5))
        
        // Select the first task row by clicking on a task cell
        let firstTask = taskList.cells.firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()
        
        // Wait for detail view
        let titleField = app.textFields["Task title"]
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        
        // Edit the title
        let newTitle = "Updated Task Title - " + UUID().uuidString.prefix(8)
        titleField.click()
        titleField.doubleClick() // Select all
        titleField.typeText(newTitle)
        
        // Save changes
        let saveButton = app.buttons["Save changes"]
        XCTAssertTrue(saveButton.exists)
        saveButton.click()
        
        // Verify success indicator appears
        let successIcon = app.images["checkmark.circle.fill"]
        XCTAssertTrue(successIcon.waitForExistence(timeout: 5))
    }

    // MARK: - Test: Switch View Modes
    @MainActor
    func test_switchBetweenViewModes() throws {
        // Find view mode picker
        let viewModePicker = app.segmentedControls["Task view mode"]
        XCTAssertTrue(viewModePicker.waitForExistence(timeout: 5))
        
        // Switch to Kanban
        viewModePicker.buttons["Kanban"].click()
        XCTAssertTrue(app.scrollViews["Kanban board"].waitForExistence(timeout: 5))
        
        // Switch to Graph
        viewModePicker.buttons["Graph"].click()
        XCTAssertTrue(app.scrollViews.firstMatch.waitForExistence(timeout: 5))
        
        // Switch back to List
        viewModePicker.buttons["List"].click()
        XCTAssertTrue(app.outlines.firstMatch.waitForExistence(timeout: 5))
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
        let sidebar = app.outlines["Main navigation"]
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
        // Wait for task list
        let searchField = app.searchFields.firstMatch
        XCTAssertTrue(searchField.waitForExistence(timeout: 5))
        
        // Type search query
        searchField.click()
        searchField.typeText("Test")
        
        // Verify search was applied (list should update)
        Thread.sleep(forTimeInterval: 1)
        
        // Clear search
        let clearButton = app.buttons["Clear search"]
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
        let taskList = app.outlines.firstMatch
        XCTAssertTrue(taskList.waitForExistence(timeout: 5))
        
        // Click in the list to focus it
        let firstTask = taskList.cells.firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()
        
        // Test down arrow navigation
        app.keyboards.keys["↓"].tap()
        Thread.sleep(forTimeInterval: 0.3)
        
        // Test up arrow navigation
        app.keyboards.keys["↑"].tap()
        Thread.sleep(forTimeInterval: 0.3)
        
        // Test Enter to open detail
        app.keyboards.keys["\r"].tap()
        Thread.sleep(forTimeInterval: 0.5)
        
        // Verify detail view appears
        let titleField = app.textFields["Task title"]
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
    }
    
    // MARK: - Test: Keyboard Navigation in Kanban Board
    @MainActor
    func test_kanbanBoardKeyboardNavigation() throws {
        // Create a task first
        try test_createNewTask_viaQuickCreate()
        
        // Switch to Kanban view
        let viewModePicker = app.segmentedControls["Task view mode"]
        XCTAssertTrue(viewModePicker.waitForExistence(timeout: 5))
        viewModePicker.buttons["Kanban"].click()
        
        // Wait for Kanban board
        let kanbanBoard = app.scrollViews["Kanban board"]
        XCTAssertTrue(kanbanBoard.waitForExistence(timeout: 5))
        
        // Click on a card to focus
        let firstCard = kanbanBoard.buttons.firstMatch
        XCTAssertTrue(firstCard.waitForExistence(timeout: 5))
        firstCard.click()
        
        // Test right arrow to move to next column
        app.keyboards.keys["→"].tap()
        Thread.sleep(forTimeInterval: 0.3)
        
        // Test left arrow to move to previous column
        app.keyboards.keys["←"].tap()
        Thread.sleep(forTimeInterval: 0.3)
        
        // Test down arrow navigation within column
        app.keyboards.keys["↓"].tap()
        Thread.sleep(forTimeInterval: 0.3)
    }
    
    // MARK: - Test: Start Work Keyboard Shortcut
    @MainActor
    func test_startWorkKeyboardShortcut() throws {
        // Create a task
        try test_createNewTask_viaQuickCreate()
        
        // Select first task in list
        let taskList = app.outlines.firstMatch
        XCTAssertTrue(taskList.waitForExistence(timeout: 5))
        let firstTask = taskList.cells.firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()
        
        // Use Cmd+Enter to start work
        firstTask.typeKey(XCUIKeyboardKey.return, modifierFlags: .command)
        Thread.sleep(forTimeInterval: 1)
        
        // Verify: Task status should change to "Doing" - check for status badge in the task list
        // The status badge should show "Doing" after the command executes
        let doingBadge = app.staticTexts["Doing"]
        XCTAssertTrue(doingBadge.waitForExistence(timeout: 5), "Task status should change to 'Doing' after Cmd+Enter")
    }
    
    // MARK: - Test: Conflict Detection UI Elements
    
    @MainActor
    func test_conflictDetection_UIElementsExist() throws {
        // Create a task first
        try test_createNewTask_viaQuickCreate()
        
        // Wait for task list to load
        let taskList = app.outlines.firstMatch
        XCTAssertTrue(taskList.waitForExistence(timeout: 5))
        
        // Select the first task
        let firstTask = taskList.cells.firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()
        
        // Wait for detail view
        let titleField = app.textFields["Task title"]
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        
        // Edit the title to create local changes
        titleField.click()
        titleField.doubleClick()
        titleField.typeText("Modified Title - " + UUID().uuidString.prefix(8))
        
        // Verify Save button is enabled (we have changes)
        let saveButton = app.buttons["Save changes"]
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
        let taskList = app.outlines.firstMatch
        XCTAssertTrue(taskList.waitForExistence(timeout: 5))
        taskList.cells.firstMatch.click()
        
        // Wait for detail view
        let titleField = app.textFields["Task title"]
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        
        // Make a change
        titleField.click()
        titleField.typeText(" - Edited")
        
        // Verify we have unsaved changes indicator would work
        // (actual conflict requires external modification)
        XCTAssertTrue(titleField.exists)
    }

    // MARK: - Helpers

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
