/**
 RalphCoreTestCase

 Purpose:
 - Reset unit-test persistence state around every RalphCore XCTest case.

 Responsibilities:
 - Reset unit-test persistence state around every RalphCore XCTest case.
 - Provide one shared base class for deterministic test isolation.

 Does not handle:
 - Temp-directory or async wait helpers.
 - Production workspace behavior.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests inheriting from this base class must keep all persistent state scoped to test helpers.
 */

import XCTest

@testable import RalphCore

class RalphCoreTestCase: XCTestCase {
    override func setUpWithError() throws {
        try super.setUpWithError()
        RalphCoreTestSupport.resetPersistentTestState()
    }

    override func tearDownWithError() throws {
        RalphCoreTestSupport.resetPersistentTestState()
        try super.tearDownWithError()
    }
}
