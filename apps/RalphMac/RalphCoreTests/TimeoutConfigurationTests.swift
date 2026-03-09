/**
 TimeoutConfigurationTests

 Responsibilities:
 - Validate timeout configuration presets and custom values.

 Does not handle:
 - CLI health probing or workspace offline caching behavior.

 Invariants/assumptions callers must respect:
 - Timeout presets are synchronous value types with deterministic defaults.
 */

import XCTest
@testable import RalphCore

final class TimeoutConfigurationTests: XCTestCase {
    func testDefaultConfiguration() {
        let config = TimeoutConfiguration.default
        XCTAssertEqual(config.timeout, 30)
        XCTAssertEqual(config.terminationGracePeriod, 2)
    }

    func testLongRunningConfiguration() {
        let config = TimeoutConfiguration.longRunning
        XCTAssertEqual(config.timeout, 300)
        XCTAssertEqual(config.terminationGracePeriod, 2)
    }

    func testCustomConfiguration() {
        let config = TimeoutConfiguration(timeout: 60, terminationGracePeriod: 5)
        XCTAssertEqual(config.timeout, 60)
        XCTAssertEqual(config.terminationGracePeriod, 5)
    }
}
