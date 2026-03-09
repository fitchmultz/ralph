/**
 RalphMacUILaunchAndTaskFlowTests

 Responsibilities:
 - Cover launch, task creation, task editing, and view-mode switching flows.
 - Validate start-work status persistence through the real CLI-backed fixture workspace.

 Does not handle:
 - Multi-window routing or conflict-resolution placeholder assertions.

 Invariants/assumptions callers must respect:
 - Tests inherit setup and helper behavior from `RalphMacUITestCase`.
 */

import XCTest

@MainActor
final class RalphMacUILaunchAndTaskFlowTests: RalphMacUITestCase {
    func test_appLaunches_andShowsMainWindow() throws {
        let window = app.windows.firstMatch
        XCTAssertTrue(window.exists, "Main window should exist")

        let sidebar = app.outlines["Main navigation"]
        XCTAssertTrue(sidebar.waitForExistence(timeout: 5), "Main navigation sidebar should exist")
        XCTAssertTrue(sidebar.staticTexts["Queue"].exists)
    }

    func test_createNewTask_viaQuickCreate() throws {
        XCTAssertTrue(newTaskToolbarButton.waitForExistence(timeout: 5))
        newTaskToolbarButton.click()

        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 5), "Task creation sheet should appear")

        let titleField = sheet.descendants(matching: .textField)
            .matching(identifier: AccessibilityID.taskCreationTitleField)
            .element(boundBy: 0)
        XCTAssertTrue(titleField.waitForExistence(timeout: 5), "Task title field should exist in the creation sheet")
        titleField.click()
        titleField.typeText("UI Test Task - " + UUID().uuidString.prefix(8))

        let createButton = sheet.descendants(matching: .button)
            .matching(identifier: AccessibilityID.taskCreationSubmitButton)
            .element(boundBy: 0)
        XCTAssertTrue(createButton.waitForExistence(timeout: 5), "Create task button should exist in the creation sheet")
        createButton.click()

        XCTAssertTrue(
            waitUntil(timeout: 5) { !sheet.exists },
            "Task creation sheet should dismiss after creating a task"
        )

        let taskList = requireTaskList()
        XCTAssertTrue(taskList.exists)
    }

    func test_editTaskTitle_andVerifyPersistence() throws {
        try test_createNewTask_viaQuickCreate()

        let taskList = requireTaskList()
        let firstTask = taskRows(in: taskList).firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()

        let titleField = taskDetailTitleField
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))

        let newTitle = "Updated Task Title - " + UUID().uuidString.prefix(8)
        titleField.click()
        titleField.doubleClick()
        titleField.typeText(newTitle)

        let saveButton = taskDetailSaveButton
        XCTAssertTrue(saveButton.waitForExistence(timeout: 5))
        XCTAssertTrue(waitUntil(timeout: 5) { saveButton.isHittable }, "Save button should be hittable in the active workspace window")
        saveButton.click()

        XCTAssertTrue(
            waitUntil(timeout: 5) { !taskDetailSaveButton.isEnabled },
            "Save button should disable again after persistence succeeds"
        )
    }

    func test_switchBetweenViewModes() throws {
        XCTAssertTrue(waitUntil(timeout: 5) { taskViewModePicker().exists })

        selectTaskViewMode("Kanban")
        XCTAssertTrue(currentWorkspaceWindow().scrollViews["Kanban board"].waitForExistence(timeout: 5))

        selectTaskViewMode("Graph")
        XCTAssertTrue(currentWorkspaceWindow().scrollViews.firstMatch.waitForExistence(timeout: 5))

        selectTaskViewMode("List")
        XCTAssertTrue(requireTaskList().exists)
    }

    func test_startWorkKeyboardShortcut() throws {
        try test_createNewTask_viaQuickCreate()

        let taskList = requireTaskList()
        let firstTask = taskRows(in: taskList).firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()

        currentWorkspaceWindow().typeKey(XCUIKeyboardKey.return, modifierFlags: .command)

        XCTAssertTrue(
            waitUntil(timeout: 5) { (try? uiTestWorkspaceTasks().contains(where: { $0.status.lowercased() == "doing" })) == true },
            "Task status should change to 'Doing' after Cmd+Enter"
        )
    }
}
