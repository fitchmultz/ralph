/**
 Purpose:
 - Define the shared Ralph macOS UI-test base case and its core state.

 Responsibilities:
 - Own app/workspace/executable lifecycle for split UI suites.
 - Centralize shared identifiers and launch configuration.

 Scope:
 - Base XCTest case state only. Helper behavior lives in focused extensions beside this file.

 Usage:
 - Subclass `RalphMacUITestCase` from focused UI suites.

 Invariants/Assumptions:
 - UI tests launch Ralph with `--uitesting`.
 - Each test uses an isolated temporary workspace and executable path.
 */

import XCTest

@MainActor
class RalphMacUITestCase: XCTestCase {
    struct UITaskSnapshot: Decodable {
        let id: String
        let status: String
        let title: String
    }

    enum ScreenshotCaptureMode: Equatable {
        case off
        case checkpoints
        case timeline

        static func fromEnvironment(_ environment: [String: String]) -> Self {
            if let rawMode = environment["RALPH_UI_SCREENSHOT_MODE"]?
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased(),
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

    enum AccessibilityID {
        static let newTaskToolbarButton = "new-task-toolbar-button"
        static let taskListContainer = "task-list-container"
        static let taskSearchField = "task-search-field"
        static let taskViewModePicker = "task-view-mode-picker"
        static let workspaceStateProbe = "workspace-state-probe"
        static let taskCreationTitleField = "task-creation-title-field"
        static let taskCreationSubmitButton = "task-creation-submit-button"
        static let taskDecomposeRequestField = "task-decompose-request-field"
        static let taskDecomposePreviewButton = "task-decompose-preview-button"
        static let taskDecomposeWriteButton = "task-decompose-write-button"
        static let taskDetailTitleField = "task-detail-title-field"
        static let taskDetailSaveButton = "task-detail-save-button"
    }

    enum LaunchEnvironment {
        static let uiTestWorkspacePath = "RALPH_UI_TEST_WORKSPACE_PATH"
        static let ralphBinPath = "RALPH_BIN_PATH"
    }

    let timelineInterval: TimeInterval = 1
    let timelineMaxFrames: Int = 180

    var app: XCUIApplication!
    var screenshotMode: ScreenshotCaptureMode = .off
    var screenshotSequence: Int = 0
    var timelineCaptureTask: Task<Void, Never>?
    var capturedFailureScreenshot = false
    var uiTestWorkspaceURL: URL?
    var ralphExecutableURL: URL?

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
        if name.contains("windowShortcuts")
            || name.contains("commandPaletteNewTab")
            || name.contains("navigationShortcut") {
            app.launchArguments.append("--uitesting-multiwindow")
        }
        app.launch()
        app.activate()
        captureScreenshot(named: "launch")
        startTimelineCaptureIfNeeded()
    }

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
            XCTAssertNoThrow(try removeItemIfExists(uiTestWorkspaceURL))
            self.uiTestWorkspaceURL = nil
        }
        ralphExecutableURL = nil
        try await super.tearDown()
    }

    var newTaskToolbarButton: XCUIElement {
        currentWorkspaceWindow().toolbars.descendants(matching: .button)
            .matching(identifier: AccessibilityID.newTaskToolbarButton)
            .element(boundBy: 0)
    }

    var taskSearchField: XCUIElement {
        currentWorkspaceWindow().descendants(matching: .textField)
            .matching(identifier: AccessibilityID.taskSearchField)
            .element(boundBy: 0)
    }

    var taskDetailTitleField: XCUIElement {
        currentWorkspaceWindow().descendants(matching: .textField)
            .matching(identifier: AccessibilityID.taskDetailTitleField)
            .element(boundBy: 0)
    }

    var taskDetailSaveButton: XCUIElement {
        currentWorkspaceWindow().descendants(matching: .button)
            .matching(identifier: AccessibilityID.taskDetailSaveButton)
            .element(boundBy: 0)
    }
}
