/**
 ANSIParserTestSupport

 Purpose:
 - Provide a fresh Workspace test fixture for split ANSI parser suites.

 Responsibilities:
 - Provide a fresh Workspace test fixture for split ANSI parser suites.

 Does not handle:
 - Defining parser assertions for specific escape-sequence behaviors.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests are main-actor isolated because Workspace is main-actor isolated.
 - Each test receives a fresh Workspace with empty attributed output.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
class ANSIParserTestCase: RalphCoreTestCase {
    var workspace: Workspace!

    override func setUp() async throws {
        try await super.setUp()
        workspace = Workspace(workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "ansi-parser"))
    }

    override func tearDown() async throws {
        workspace = nil
        try await super.tearDown()
    }
}
