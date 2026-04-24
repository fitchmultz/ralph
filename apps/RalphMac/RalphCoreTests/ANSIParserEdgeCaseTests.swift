/**
 ANSIParserEdgeCaseTests

 Purpose:
 - Validate complex real-world ANSI payloads and malformed-sequence recovery behavior.

 Responsibilities:
 - Validate complex real-world ANSI payloads and malformed-sequence recovery behavior.

 Does not handle:
 - Baseline color tables or control-sequence stripping.
 - Shared Workspace fixture lifecycle.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Malformed or incomplete ANSI sequences must not crash parsing and should preserve readable output.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class ANSIParserEdgeCaseTests: ANSIParserTestCase {
    func test_cargoBuildStyle_output() async {
        let output = """
            \u{001B}[0m\u{001B}[0m\u{001B}[1m\u{001B}[32m   Compiling\u{001B}[0m mycrate v0.1.0
            \u{001B}[0m\u{001B}[0m\u{001B}[1m\u{001B}[32m    Finished\u{001B}[0m dev [unoptimized]
            """

        workspace.parseANSICodes(from: output)

        XCTAssertGreaterThan(workspace.attributedOutput.count, 0)
        XCTAssertTrue(workspace.attributedOutput.contains { $0.isBold }, "Should have bold segments")
    }

    func test_gitStatusStyle_output() async {
        let output = """
            On branch main
            \u{001B}[31mdeleted:\u{001B}[m    file1.txt
            \u{001B}[32mnew file:\u{001B}[m   file2.txt
            \u{001B}[33mmodified:\u{001B}[m   file3.txt
            """

        workspace.parseANSICodes(from: output)

        XCTAssertGreaterThan(workspace.attributedOutput.count, 0)

        let hasRed = workspace.attributedOutput.contains {
            if case .red = $0.color { return true }
            return false
        }
        let hasGreen = workspace.attributedOutput.contains {
            if case .green = $0.color { return true }
            return false
        }
        XCTAssertTrue(hasRed || hasGreen, "Should have colored output")
    }

    func test_multipleColorChanges() async {
        workspace.parseANSICodes(from: "\u{001B}[31mred\u{001B}[32mgreen\u{001B}[34mblue\u{001B}[0mreset")
        XCTAssertEqual(workspace.attributedOutput.count, 4)
    }

    func test_incompleteEscapeSequence() async {
        workspace.parseANSICodes(from: "text\u{001B}")
        XCTAssertEqual(workspace.attributedOutput[0].text, "text\u{001B}")
    }

    func test_incompleteCSI_noTerminator() async {
        workspace.parseANSICodes(from: "text\u{001B}[31")
        XCTAssertEqual(workspace.attributedOutput[0].text, "text\u{001B}[31")
    }

    func test_invalidSGRCode_ignored() async {
        workspace.parseANSICodes(from: "\u{001B}[999mtext\u{001B}[0m")
        XCTAssertTrue(workspace.attributedOutput[0].text.contains("text"))
    }
}
