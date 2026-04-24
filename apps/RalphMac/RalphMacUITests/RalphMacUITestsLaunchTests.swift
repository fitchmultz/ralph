/**
 RalphMacUITestsLaunchTests

 Purpose:
 - Measure application launch performance.

 Responsibilities:
 - Measure application launch performance.
 - Verify app launches successfully.

 Does not handle:
 - Functional testing (see RalphMacUITests).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import XCTest

@MainActor
final class RalphMacUITestsLaunchTests: XCTestCase {
    private func waitForTermination(of app: XCUIApplication, timeout: TimeInterval = 10) {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if app.state == .notRunning {
                return
            }
            RunLoop.current.run(
                mode: .default,
                before: min(deadline, Date().addingTimeInterval(0.1))
            )
        }
        XCTAssertEqual(app.state, .notRunning, "Launch test app should terminate during cleanup")
    }


    override class var runsForEachTargetApplicationUIConfiguration: Bool {
        true
    }

    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    func testLaunch() throws {
        let app = XCUIApplication()
        app.launch()
        addTeardownBlock {
            app.terminate()
            self.waitForTermination(of: app)
        }

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
            let app = XCUIApplication()
            measure(metrics: [XCTApplicationLaunchMetric()]) {
                app.launch()
                app.terminate()
                self.waitForTermination(of: app)
            }
        }
    }
}
