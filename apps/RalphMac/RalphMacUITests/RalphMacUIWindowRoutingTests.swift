/**
 RalphMacUIWindowRoutingTests

 Responsibilities:
 - Validate tab and window routing behaviors remain scoped to the focused scene.
 - Cover command-palette and keyboard shortcut multi-window behavior.

 Does not handle:
 - Task editing or navigation-within-a-single-window flows.

 Invariants/assumptions callers must respect:
 - Tests run with multiwindow launch arguments inherited from `RalphMacUITestCase`.
 */

import XCTest

@MainActor
final class RalphMacUIWindowRoutingTests: RalphMacUITestCase {
    func test_createNewTab_andSwitchBetweenTabs() throws {
        let window = app.windows.firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        let before = tabCount(in: window)

        app.menuBars.menuBarItems["Workspace"].click()
        app.menuBars.menuItems["New Tab"].click()

        XCTAssertTrue(
            waitUntil { tabCount(in: window) == before + 1 },
            "New Tab menu action should increase tab count in the active window"
        )
    }

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
        XCTAssertEqual(tabCount(in: secondWindow), secondBefore, "Cmd+T should not add tabs in unfocused windows")
        XCTAssertEqual(workspaceWindowCount(), windowsBefore, "Cmd+T should not create or close windows")

        firstWindow.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(
            waitUntil { tabCount(in: firstWindow) == firstBefore },
            "Cmd+W should close a tab only in the focused window"
        )
        XCTAssertEqual(tabCount(in: secondWindow), secondBefore, "Cmd+W should not close tabs in unfocused windows")
        XCTAssertEqual(workspaceWindowCount(), windowsBefore, "Cmd+W should not close the entire window")

        secondWindow.click()
        secondWindow.typeKey("w", modifierFlags: [.command, .shift])
        XCTAssertTrue(
            waitUntil { workspaceWindowCount() == windowsBefore - 1 },
            "Cmd+Shift+W should close only the focused window (expected \(windowsBefore - 1), got \(workspaceWindowCount()))"
        )
    }

    func test_navigationShortcut_affectsOnlyFocusedWindow() throws {
        ensureSecondWindow()

        let windows = workspaceWindows()
        XCTAssertGreaterThanOrEqual(windows.count, 2, "Expected at least two workspace windows")
        let firstWindow = windows[0]
        let secondWindow = windows[1]

        XCTAssertTrue(taskViewModePicker(in: firstWindow).waitForExistence(timeout: 5))
        XCTAssertTrue(taskViewModePicker(in: secondWindow).waitForExistence(timeout: 5))

        firstWindow.click()
        firstWindow.typeKey("5", modifierFlags: .command)

        XCTAssertTrue(
            waitUntil { !self.taskViewModePicker(in: firstWindow).exists },
            "Cmd+5 should switch only the focused window away from Queue"
        )
        XCTAssertTrue(
            taskViewModePicker(in: secondWindow).exists,
            "Cmd+5 should not mutate the unfocused window's section"
        )
    }

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
        XCTAssertEqual(tabCount(in: secondWindow), secondBefore, "Command palette 'New Tab' should not affect unfocused windows")
        XCTAssertEqual(workspaceWindowCount(), windowsBefore, "Command palette 'New Tab' should not create or close windows")
    }
}
