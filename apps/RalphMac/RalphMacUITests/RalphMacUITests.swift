/**
 RalphMacUITests

 Responsibilities:
 - Provide shared launch, screenshot, fixture, and window helpers for split RalphMac UI suites.
 - Centralize UI-test workspace provisioning and CLI invocation utilities.

 Does not handle:
 - Defining the individual UI assertions for launch, task flows, navigation, or conflict handling.

 Invariants/assumptions callers must respect:
 - Subclasses inherit from `RalphMacUITestCase`.
 - UI tests run with `--uitesting` and an isolated workspace defaults domain.
 */

import XCTest

@MainActor
class RalphMacUITestCase: XCTestCase {
    struct UITaskSnapshot: Decodable {
        let id: String
        let status: String
    }

    enum ScreenshotCaptureMode: Equatable {
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

    enum AccessibilityID {
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
    }

    enum LaunchEnvironment {
        static let uiTestWorkspacePath = "RALPH_UI_TEST_WORKSPACE_PATH"
        static let ralphBinPath = "RALPH_BIN_PATH"
    }

    let timelineIntervalNanos: UInt64 = 1_000_000_000
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
            try? FileManager.default.removeItem(at: uiTestWorkspaceURL)
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

    func currentWorkspaceWindow(file: StaticString = #filePath, line: UInt = #line) -> XCUIElement {
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

    func requireTaskList(timeout: TimeInterval = 5, file: StaticString = #filePath, line: UInt = #line) -> XCUIElement {
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

    func taskRows(in taskList: XCUIElement) -> XCUIElementQuery {
        if taskList.cells.count > 0 {
            return taskList.cells
        }

        return taskList.descendants(matching: .cell)
    }

    func selectTaskViewMode(_ mode: String, file: StaticString = #filePath, line: UInt = #line) {
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

    func makeUITestWorkspace() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("ralph-ui-tests", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)

        try runRalph(arguments: ["init", "--non-interactive"], currentDirectoryURL: root)
        try seedUITestQueue(at: root)

        return root
    }

    func seedUITestQueue(at workspaceURL: URL) throws {
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

    func runRalph(arguments: [String], currentDirectoryURL: URL) throws {
        _ = try runRalphAndCollectOutput(arguments: arguments, currentDirectoryURL: currentDirectoryURL)
    }

    func runRalphAndCollectOutput(arguments: [String], currentDirectoryURL: URL) throws -> String {
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

    func uiTestWorkspaceTasks() throws -> [UITaskSnapshot] {
        guard let uiTestWorkspaceURL else {
            return []
        }

        let output = try runRalphAndCollectOutput(
            arguments: ["queue", "list", "--format", "json"],
            currentDirectoryURL: uiTestWorkspaceURL
        )
        return try JSONDecoder().decode([UITaskSnapshot].self, from: Data(output.utf8))
    }

    func resolveRalphExecutableURL(environment: [String: String] = ProcessInfo.processInfo.environment) throws -> URL {
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

    func captureScreenshot(named step: String) {
        guard screenshotMode != .off else { return }
        guard app != nil else { return }

        screenshotSequence += 1
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "\(sanitizedTestName())-\(String(format: "%03d", screenshotSequence))-\(sanitizedAttachmentToken(step))"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func startTimelineCaptureIfNeeded() {
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

    func stopTimelineCapture() {
        timelineCaptureTask?.cancel()
        timelineCaptureTask = nil
    }

    func sanitizedTestName() -> String {
        let cleaned = name
            .replacingOccurrences(of: "^[-\\[]+", with: "", options: .regularExpression)
            .replacingOccurrences(of: "[\\] ]+$", with: "", options: .regularExpression)
            .replacingOccurrences(of: "[^A-Za-z0-9._-]+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return cleaned.isEmpty ? "ui-test" : cleaned
    }

    func sanitizedAttachmentToken(_ raw: String) -> String {
        let cleaned = raw
            .replacingOccurrences(of: "[^A-Za-z0-9._-]+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return cleaned.isEmpty ? "checkpoint" : cleaned
    }

    func ensureSecondWindow() {
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

    @discardableResult
    func waitUntil(timeout: TimeInterval = 5, interval: TimeInterval = 0.1, condition: () -> Bool) -> Bool {
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
