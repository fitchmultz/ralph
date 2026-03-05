/**
 RalphMacUITestsLaunchTests

 Responsibilities:
 - Measure application launch performance.
 - Verify app launches successfully.

 Does not handle:
 - Functional testing (see RalphMacUITests).
 */

import XCTest

@MainActor
final class RalphMacUITestsLaunchTests: XCTestCase {

    override class var runsForEachTargetApplicationUIConfiguration: Bool {
        true
    }

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func testLaunch() throws {
        let app = XCUIApplication()
        app.launch()

        // Verify window appears after launch
        let window = app.windows.firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 10))

        // Capture screenshot for debugging when visual capture mode is enabled.
        let environment = ProcessInfo.processInfo.environment
        let screenshotsEnabled = environment["RALPH_UI_SCREENSHOTS"] == "1"
            || (environment["RALPH_UI_SCREENSHOT_MODE"]?.lowercased() != nil
                && environment["RALPH_UI_SCREENSHOT_MODE"]?.lowercased() != "off")
        if screenshotsEnabled {
            let attachment = XCTAttachment(screenshot: app.screenshot())
            attachment.name = "Launch-Screen"
            attachment.lifetime = .keepAlways
            add(attachment)
        }
    }

    func testLaunchPerformance() throws {
        if #available(macOS 14.0, *) {
            measure(metrics: [XCTApplicationLaunchMetric()]) {
                XCUIApplication().launch()
            }
        }
    }
}
