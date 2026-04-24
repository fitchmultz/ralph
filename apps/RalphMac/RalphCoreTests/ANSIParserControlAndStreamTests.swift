/**
 ANSIParserControlAndStreamTests

 Purpose:
 - Validate control-sequence stripping and incremental stream parsing behavior.

 Responsibilities:
 - Validate control-sequence stripping and incremental stream parsing behavior.

 Does not handle:
 - Extended color decoding or ANSIColor utilities.
 - Malformed-sequence recovery beyond streaming semantics.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Incremental stream parsing must preserve ANSI state while remaining phase-neutral.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class ANSIParserControlAndStreamTests: ANSIParserTestCase {
    func test_cursorUp_stripped() async {
        workspace.parseANSICodes(from: "text\u{001B}[1Aafter")

        XCTAssertEqual(workspace.attributedOutput.count, 1)
        XCTAssertEqual(workspace.attributedOutput[0].text, "textafter")
    }

    func test_cursorDown_stripped() async {
        workspace.parseANSICodes(from: "text\u{001B}[2Bafter")
        XCTAssertEqual(workspace.attributedOutput[0].text, "textafter")
    }

    func test_cursorForward_stripped() async {
        workspace.parseANSICodes(from: "text\u{001B}[5Cafter")
        XCTAssertEqual(workspace.attributedOutput[0].text, "textafter")
    }

    func test_cursorBack_stripped() async {
        workspace.parseANSICodes(from: "text\u{001B}[3Dafter")
        XCTAssertEqual(workspace.attributedOutput[0].text, "textafter")
    }

    func test_clearLine_stripped() async {
        workspace.parseANSICodes(from: "text\u{001B}[2Kafter")
        XCTAssertEqual(workspace.attributedOutput[0].text, "textafter")
    }

    func test_clearScreen_stripped() async {
        workspace.parseANSICodes(from: "text\u{001B}[2Jafter")
        XCTAssertEqual(workspace.attributedOutput[0].text, "textafter")
    }

    func test_cursorPosition_stripped() async {
        workspace.parseANSICodes(from: "text\u{001B}[10;20Hafter")
        XCTAssertEqual(workspace.attributedOutput[0].text, "textafter")
    }

    func test_cursorPositionAlt_stripped() async {
        workspace.parseANSICodes(from: "text\u{001B}[10;20fafter")
        XCTAssertEqual(workspace.attributedOutput[0].text, "textafter")
    }

    func test_incrementalStreamParsing_preservesANSIStateAcrossChunks() async {
        workspace.resetStreamProcessingState()

        workspace.consumeStreamTextChunk("\u{001B}[31mred")
        workspace.consumeStreamTextChunk(" still red\u{001B}[0m plain")
        workspace.runState.flushConsoleRenderState()

        XCTAssertEqual(workspace.attributedOutput.count, 2)
        XCTAssertEqual(workspace.attributedOutput[0].text, "red still red")
        XCTAssertEqual(workspace.attributedOutput[0].color, .red)
        XCTAssertEqual(workspace.attributedOutput[1].text, " plain")
        XCTAssertEqual(workspace.attributedOutput[1].color, .default)
    }

    func test_incrementalStreamParsing_doesNotInferRunPhaseFromText() async {
        workspace.resetStreamProcessingState()
        workspace.currentPhase = nil

        workspace.consumeStreamTextChunk("prelude\n")
        workspace.consumeStreamTextChunk("## Phase 2\nimplementing now\n")

        XCTAssertNil(workspace.currentPhase)
    }
}
