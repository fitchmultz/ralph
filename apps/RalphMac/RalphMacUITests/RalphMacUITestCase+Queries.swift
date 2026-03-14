/**
 Purpose:
 - Provide shared UI-query and interaction helpers for Ralph macOS UI tests.

 Responsibilities:
 - Resolve the active workspace window and common task-list/detail controls.
 - Centralize repeated task-creation/open-detail flows used across suites.
 - Keep multi-window probing and tab counting out of scenario files.

 Scope:
 - XCUI query helpers and reusable interactions only.

 Usage:
 - Focused UI suites call these helpers instead of recreating element queries inline.

 Invariants/Assumptions:
 - Workspace windows expose `window-tab-count-probe` for reliable window detection.
 - Task creation uses the shared quick-create sheet identifiers.
 */

import XCTest

@MainActor
extension RalphMacUITestCase {
    func currentWorkspaceWindow(file: StaticString = #filePath, line: UInt = #line) -> XCUIElement {
        app.activate()
        assertEventually(
            "Expected at least one app window to appear",
            timeout: 8,
            file: file,
            line: line
        ) {
            !app.windows.allElementsBoundByIndex.isEmpty
                && app.windows.allElementsBoundByIndex.contains(where: \.exists)
        }

        let workspaceCandidates = workspaceWindows()
        let fallbackCandidates = app.windows.allElementsBoundByIndex.filter(\.exists)
        let window = workspaceCandidates.first(where: \.isHittable)
            ?? workspaceCandidates.first
            ?? fallbackCandidates.first(where: \.isHittable)
            ?? fallbackCandidates.first
            ?? app.windows.firstMatch
        assertExists(window, message: "Expected a workspace window", file: file, line: line)
        return window
    }

    func taskViewModePicker() -> XCUIElement {
        taskViewModePicker(in: currentWorkspaceWindow())
    }

    func taskViewModePicker(in window: XCUIElement) -> XCUIElement {
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

    struct WorkspaceStateProbeSnapshot: Decodable {
        let workspaceID: String
        let workspacePath: String
        let projectDisplayName: String
        let taskCount: Int
        let tasksLoading: Bool
        let tasksErrorMessage: String?
        let isPlaceholder: Bool
        let retargetRevision: UInt64
        let workspaceCount: Int
        let focusedWorkspaceID: String?
        let effectiveWorkspaceID: String?
    }

    func workspaceStateProbe(in window: XCUIElement) -> XCUIElement {
        let staticText = window.staticTexts[AccessibilityID.workspaceStateProbe]
        if staticText.exists {
            return staticText
        }
        return window.descendants(matching: .any)
            .matching(identifier: AccessibilityID.workspaceStateProbe)
            .firstMatch
    }

    func readWorkspaceStateProbe(
        in window: XCUIElement? = nil,
        file: StaticString = #filePath,
        line: UInt = #line
    ) -> WorkspaceStateProbeSnapshot {
        let resolvedWindow = window ?? currentWorkspaceWindow(file: file, line: line)
        let probe = workspaceStateProbe(in: resolvedWindow)
        assertExists(
            probe,
            timeout: 4,
            message: "Workspace state probe should exist",
            file: file,
            line: line
        )

        let rawValue = (probe.value as? String)
            ?? (probe.label == AccessibilityID.workspaceStateProbe ? nil : probe.label)
            ?? "{}"

        do {
            return try JSONDecoder().decode(
                WorkspaceStateProbeSnapshot.self,
                from: Data(rawValue.utf8)
            )
        } catch {
            XCTFail(
                "Failed to decode workspace state probe: \(rawValue) (error: \(error))",
                file: file,
                line: line
            )
            return WorkspaceStateProbeSnapshot(
                workspaceID: "",
                workspacePath: "",
                projectDisplayName: "",
                taskCount: -1,
                tasksLoading: false,
                tasksErrorMessage: "decode-error",
                isPlaceholder: false,
                retargetRevision: 0,
                workspaceCount: -1,
                focusedWorkspaceID: nil,
                effectiveWorkspaceID: nil
            )
        }
    }

    func requireTaskList(
        timeout: TimeInterval = 5,
        file: StaticString = #filePath,
        line: UInt = #line
    ) -> XCUIElement {
        let window = currentWorkspaceWindow(file: file, line: line)
        let candidates = [
            window.outlines[AccessibilityID.taskListContainer],
            window.tables[AccessibilityID.taskListContainer],
            window.collectionViews[AccessibilityID.taskListContainer],
            window.scrollViews[AccessibilityID.taskListContainer],
            window.otherElements[AccessibilityID.taskListContainer]
        ]

        assertEventually(
            "Task list container should exist",
            timeout: timeout,
            file: file,
            line: line
        ) {
            candidates.contains(where: \.exists)
        }

        return candidates.first(where: \.exists) ?? candidates[0]
    }

    func taskRows(in taskList: XCUIElement) -> XCUIElementQuery {
        if taskList.cells.count > 0 {
            return taskList.cells
        }
        return taskList.descendants(matching: .cell)
    }

    func taskText(_ title: String, in taskList: XCUIElement) -> XCUIElement {
        let exactLabel = NSPredicate(format: "label == %@", title)
        let rowLabel = NSPredicate(format: "label CONTAINS %@", title)

        let staticText = taskList.descendants(matching: .staticText)
            .matching(exactLabel)
            .firstMatch
        if staticText.exists {
            return staticText
        }

        let combinedRow = taskList.descendants(matching: .any)
            .matching(rowLabel)
            .firstMatch
        if combinedRow.exists {
            return combinedRow
        }

        return taskList.descendants(matching: .staticText)
            .matching(exactLabel)
            .firstMatch
    }

    func selectTaskViewMode(_ mode: String, file: StaticString = #filePath, line: UInt = #line) {
        let picker = taskViewModePicker()
        assertExists(
            picker,
            message: "Task view mode picker should exist",
            file: file,
            line: line
        )

        let radioButton = picker.radioButtons[mode]
        if radioButton.exists {
            radioButton.click()
            return
        }

        let button = picker.buttons[mode]
        assertExists(
            button,
            timeout: 2,
            message: "Expected task view mode option '\(mode)'",
            file: file,
            line: line
        )
        button.click()
    }

    @discardableResult
    func createTask(
        titlePrefix: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) -> String {
        let title = "\(titlePrefix) - \(UUID().uuidString.prefix(8))"
        assertExists(
            newTaskToolbarButton,
            message: "New task toolbar button should exist",
            file: file,
            line: line
        )
        newTaskToolbarButton.click()

        let sheet = app.sheets.firstMatch
        assertExists(sheet, message: "Task creation sheet should appear", file: file, line: line)

        let titleField = sheet.descendants(matching: .textField)
            .matching(identifier: AccessibilityID.taskCreationTitleField)
            .element(boundBy: 0)
        assertExists(
            titleField,
            message: "Task title field should exist in the creation sheet",
            file: file,
            line: line
        )
        titleField.click()
        titleField.typeText(title)

        let createButton = sheet.descendants(matching: .button)
            .matching(identifier: AccessibilityID.taskCreationSubmitButton)
            .element(boundBy: 0)
        assertExists(
            createButton,
            message: "Create task button should exist in the creation sheet",
            file: file,
            line: line
        )
        createButton.click()

        assertDoesNotExist(
            sheet,
            message: "Task creation sheet should dismiss after creating a task",
            file: file,
            line: line
        )
        return title
    }

    func openFirstTaskDetails(file: StaticString = #filePath, line: UInt = #line) {
        let taskList = requireTaskList(file: file, line: line)
        let firstTask = taskRows(in: taskList).firstMatch
        assertExists(firstTask, message: "Expected at least one task row", file: file, line: line)
        firstTask.click()
    }

    func ensureSecondWindow() {
        guard workspaceWindowCount() < 2 else { return }
        if waitUntil(timeout: 6, condition: { workspaceWindowCount() >= 2 }) {
            return
        }
        app.typeKey("n", modifierFlags: .command)
        assertEventually("Expected a second window to open for multi-window shortcut tests", timeout: 8) {
            workspaceWindowCount() >= 2
        }
    }

    func workspaceWindows() -> [XCUIElement] {
        app.windows.allElementsBoundByIndex.filter {
            $0.otherElements["window-tab-count-probe"].exists
        }
    }

    func workspaceWindowCount() -> Int {
        workspaceWindows().count
    }

    func tabCount(in window: XCUIElement) -> Int {
        let probe = window.otherElements["window-tab-count-probe"]
        if waitUntil(timeout: 2, condition: { probe.exists }) {
            if let value = probe.value as? NSNumber {
                return value.intValue
            }
            if let value = probe.value as? String, let count = Int(value) {
                return count
            }
            let prefix = "window-tab-count-"
            if probe.label.hasPrefix(prefix), let count = Int(probe.label.dropFirst(prefix.count)) {
                return count
            }
        }
        return window.tabs.count
    }
}
