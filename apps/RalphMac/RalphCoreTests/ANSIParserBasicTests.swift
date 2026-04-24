/**
 ANSIParserBasicTests

 Purpose:
 - Validate baseline text parsing, text attributes, reset behavior, and segment merging.

 Responsibilities:
 - Validate baseline text parsing, text attributes, reset behavior, and segment merging.

 Does not handle:
 - Extended color modes or control-sequence stripping.
 - Stream-state carryover and malformed-sequence recovery.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Tests operate on a fresh Workspace from ANSIParserTestCase.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class ANSIParserBasicTests: ANSIParserTestCase {
    func test_plainText_noANSI() async {
        workspace.parseANSICodes(from: "Hello world")

        XCTAssertEqual(workspace.attributedOutput.count, 1)
        XCTAssertEqual(workspace.attributedOutput[0].text, "Hello world")
        XCTAssertEqual(workspace.attributedOutput[0].color, .default)
        XCTAssertFalse(workspace.attributedOutput[0].isBold)
        XCTAssertFalse(workspace.attributedOutput[0].isItalic)
    }

    func test_emptyString() async {
        workspace.parseANSICodes(from: "")
        XCTAssertEqual(workspace.attributedOutput.count, 0)
    }

    func test_onlyWhitespace() async {
        workspace.parseANSICodes(from: "   \n\t  ")

        XCTAssertEqual(workspace.attributedOutput.count, 1)
        XCTAssertEqual(workspace.attributedOutput[0].text, "   \n\t  ")
    }

    func test_boldAttribute() async {
        workspace.parseANSICodes(from: "\u{001B}[1mbold text\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput[0].text, "bold text")
        XCTAssertTrue(workspace.attributedOutput[0].isBold)
    }

    func test_italicAttribute() async {
        workspace.parseANSICodes(from: "\u{001B}[3mitalic text\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput[0].text, "italic text")
        XCTAssertTrue(workspace.attributedOutput[0].isItalic)
    }

    func test_boldAndItalicCombined() async {
        workspace.parseANSICodes(from: "\u{001B}[1;3mbold italic\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput[0].text, "bold italic")
        XCTAssertTrue(workspace.attributedOutput[0].isBold)
        XCTAssertTrue(workspace.attributedOutput[0].isItalic)
    }

    func test_resetCode() async {
        workspace.parseANSICodes(from: "\u{001B}[31mred\u{001B}[0mdefault")

        XCTAssertEqual(workspace.attributedOutput.count, 2)
        XCTAssertEqual(workspace.attributedOutput[0].text, "red")
        XCTAssertEqual(workspace.attributedOutput[0].color, .red)
        XCTAssertEqual(workspace.attributedOutput[1].text, "default")
        XCTAssertEqual(workspace.attributedOutput[1].color, .default)
    }

    func test_emptySGR_isReset() async {
        workspace.parseANSICodes(from: "\u{001B}[31mred\u{001B}[mdefault")

        XCTAssertEqual(workspace.attributedOutput.count, 2)
        XCTAssertEqual(workspace.attributedOutput[1].color, .default)
    }

    func test_boldReset_withCode22() async {
        workspace.parseANSICodes(from: "\u{001B}[1mbold\u{001B}[22mnormal")

        XCTAssertEqual(workspace.attributedOutput.count, 2)
        XCTAssertTrue(workspace.attributedOutput[0].isBold)
        XCTAssertFalse(workspace.attributedOutput[1].isBold)
    }

    func test_italicReset_withCode23() async {
        workspace.parseANSICodes(from: "\u{001B}[3mitalic\u{001B}[23mnormal")

        XCTAssertEqual(workspace.attributedOutput.count, 2)
        XCTAssertTrue(workspace.attributedOutput[0].isItalic)
        XCTAssertFalse(workspace.attributedOutput[1].isItalic)
    }

    func test_adjacentSameStyle_merged() async {
        workspace.parseANSICodes(from: "\u{001B}[31mred1red2\u{001B}[0m")

        let redSegments = workspace.attributedOutput.filter {
            if case .red = $0.color { return true }
            return false
        }
        XCTAssertEqual(redSegments.count, 1)
        XCTAssertEqual(redSegments[0].text, "red1red2")
    }

    func test_adjacentDifferentStyle_notMerged() async {
        workspace.parseANSICodes(from: "\u{001B}[31mred\u{001B}[32mgreen")

        XCTAssertEqual(workspace.attributedOutput.count, 2)
        XCTAssertEqual(workspace.attributedOutput[0].text, "red")
        XCTAssertEqual(workspace.attributedOutput[1].text, "green")
    }

    func test_newlinesPreserved() async {
        workspace.parseANSICodes(from: "line1\nline2\nline3")

        XCTAssertEqual(workspace.attributedOutput.count, 1)
        XCTAssertEqual(workspace.attributedOutput[0].text, "line1\nline2\nline3")
    }

    func test_newlinesWithColors() async {
        workspace.parseANSICodes(from: "\u{001B}[31mline1\nline2\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput.count, 1)
        XCTAssertEqual(workspace.attributedOutput[0].text, "line1\nline2")
        XCTAssertEqual(workspace.attributedOutput[0].color, .red)
    }

    func test_multipleSequentialResets() async {
        workspace.parseANSICodes(from: "\u{001B}[0m\u{001B}[0mtext")

        XCTAssertEqual(workspace.attributedOutput.count, 1)
        XCTAssertEqual(workspace.attributedOutput[0].text, "text")
    }

    func test_mixedAttributesAndColors() async {
        workspace.parseANSICodes(from: "\u{001B}[1;31mbold red\u{001B}[22mnormal red\u{001B}[0mplain")

        XCTAssertEqual(workspace.attributedOutput.count, 3)
        XCTAssertTrue(workspace.attributedOutput[0].isBold)
        XCTAssertEqual(workspace.attributedOutput[0].color, .red)
        XCTAssertFalse(workspace.attributedOutput[1].isBold)
        XCTAssertEqual(workspace.attributedOutput[1].color, .red)
        XCTAssertEqual(workspace.attributedOutput[2].color, .default)
    }
}
