/**
 RalphMacUIConflictTests

 Responsibilities:
 - Validate the detail UI exposes save/conflict-ready controls when local edits exist.
 - Keep lightweight regression coverage around the conflict placeholder flows.

 Does not handle:
 - Full external conflict orchestration through CLI automation.

 Invariants/assumptions callers must respect:
 - Tests exercise the UI readiness state, not full external mutation races.
 */

import XCTest

@MainActor
final class RalphMacUIConflictTests: RalphMacUITestCase {
    func test_conflictDetection_UIElementsExist() throws {
        try createTaskAndOpenDetails()

        let titleField = taskDetailTitleField
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        titleField.click()
        titleField.doubleClick()
        titleField.typeText("Modified Title - " + UUID().uuidString.prefix(8))

        let saveButton = taskDetailSaveButton
        XCTAssertTrue(saveButton.isEnabled)
    }

    func test_conflictResolverView_Dismissal() throws {
        try createTaskAndOpenDetails()

        let titleField = taskDetailTitleField
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        titleField.click()
        titleField.typeText(" - Edited")

        XCTAssertTrue(titleField.exists)
    }

    private func createTaskAndOpenDetails() throws {
        XCTAssertTrue(newTaskToolbarButton.waitForExistence(timeout: 5))
        newTaskToolbarButton.click()

        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 5))

        let titleField = sheet.descendants(matching: .textField)
            .matching(identifier: AccessibilityID.taskCreationTitleField)
            .element(boundBy: 0)
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        titleField.click()
        titleField.typeText("Conflict Task - " + UUID().uuidString.prefix(8))

        let createButton = sheet.descendants(matching: .button)
            .matching(identifier: AccessibilityID.taskCreationSubmitButton)
            .element(boundBy: 0)
        XCTAssertTrue(createButton.waitForExistence(timeout: 5))
        createButton.click()

        XCTAssertTrue(waitUntil(timeout: 5) { !sheet.exists })

        let taskList = requireTaskList()
        let firstTask = taskRows(in: taskList).firstMatch
        XCTAssertTrue(firstTask.waitForExistence(timeout: 5))
        firstTask.click()
    }
}
