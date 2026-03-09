/**
 RalphMacUINavigationAndKeyboardTests

 Responsibilities:
 - Validate decompose-sheet, sidebar navigation, search, and keyboard traversal behaviors.
 - Cover list and kanban keyboard interactions inside a single workspace window.

 Does not handle:
 - Multi-window routing or conflict placeholder behavior.

 Invariants/assumptions callers must respect:
 - Tests inherit launch helpers and fixture seeding from `RalphMacUITestCase`.
 */

import XCTest

@MainActor
final class RalphMacUINavigationAndKeyboardTests: RalphMacUITestCase {
    func test_openTaskDecomposeSheet_fromTaskMenu() throws {
        app.menuBars.menuBarItems["Task"].click()
        app.menuBars.menuItems["Decompose Task..."].click()

        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 5), "Task decompose sheet should appear")
        XCTAssertTrue(sheet.descendants(matching: .textField).matching(identifier: AccessibilityID.taskDecomposeRequestField).firstMatch.exists)
        XCTAssertTrue(sheet.descendants(matching: .button).matching(identifier: AccessibilityID.taskDecomposePreviewButton).firstMatch.exists)
        XCTAssertTrue(sheet.descendants(matching: .button).matching(identifier: AccessibilityID.taskDecomposeWriteButton).firstMatch.exists)
    }

    func test_navigateThroughAllSidebarSections() throws {
        let sidebar = currentWorkspaceWindow().outlines["Main navigation"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 5))

        let sections = ["Queue", "Quick Actions", "Run Control", "Advanced Runner", "Analytics"]
        for section in sections {
            let sectionItem = sidebar.staticTexts[section]
            XCTAssertTrue(sectionItem.exists, "\(section) should exist in sidebar")
            sectionItem.click()
            RunLoop.current.run(until: Date().addingTimeInterval(0.5))
        }
    }

    func test_taskSearchFunctionality() throws {
        let searchField = taskSearchField
        XCTAssertTrue(searchField.waitForExistence(timeout: 5))

        searchField.click()
        searchField.typeText("Test")
        RunLoop.current.run(until: Date().addingTimeInterval(1))

        let clearButton = currentWorkspaceWindow().buttons["Clear search"]
        if clearButton.exists {
            clearButton.click()
        }
    }

    func test_taskListKeyboardNavigation() throws {
        try createTaskForKeyboardFlows()

        let taskList = requireTaskList()
        let firstTask = taskRows(in: taskList).firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()

        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.downArrow, modifierFlags: [])
        RunLoop.current.run(until: Date().addingTimeInterval(0.3))

        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.upArrow, modifierFlags: [])
        RunLoop.current.run(until: Date().addingTimeInterval(0.3))

        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.return, modifierFlags: [])
        RunLoop.current.run(until: Date().addingTimeInterval(0.5))

        XCTAssertTrue(taskDetailTitleField.waitForExistence(timeout: 5))
    }

    func test_kanbanBoardKeyboardNavigation() throws {
        try createTaskForKeyboardFlows()

        XCTAssertTrue(waitUntil(timeout: 5) { taskViewModePicker().exists })
        selectTaskViewMode("Kanban")

        let kanbanBoard = currentWorkspaceWindow().scrollViews["Kanban board"]
        XCTAssertTrue(kanbanBoard.waitForExistence(timeout: 5))

        let firstCard = kanbanBoard.buttons.firstMatch
        XCTAssertTrue(firstCard.waitForExistence(timeout: 5))
        firstCard.click()

        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.rightArrow, modifierFlags: [])
        RunLoop.current.run(until: Date().addingTimeInterval(0.3))

        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.leftArrow, modifierFlags: [])
        RunLoop.current.run(until: Date().addingTimeInterval(0.3))

        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.downArrow, modifierFlags: [])
        RunLoop.current.run(until: Date().addingTimeInterval(0.3))
    }

    private func createTaskForKeyboardFlows() throws {
        XCTAssertTrue(newTaskToolbarButton.waitForExistence(timeout: 5))
        newTaskToolbarButton.click()

        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 5))

        let titleField = sheet.descendants(matching: .textField)
            .matching(identifier: AccessibilityID.taskCreationTitleField)
            .element(boundBy: 0)
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        titleField.click()
        titleField.typeText("Keyboard Flow Task - " + UUID().uuidString.prefix(8))

        let createButton = sheet.descendants(matching: .button)
            .matching(identifier: AccessibilityID.taskCreationSubmitButton)
            .element(boundBy: 0)
        XCTAssertTrue(createButton.waitForExistence(timeout: 5))
        createButton.click()

        XCTAssertTrue(waitUntil(timeout: 5) { !sheet.exists })
    }
}
