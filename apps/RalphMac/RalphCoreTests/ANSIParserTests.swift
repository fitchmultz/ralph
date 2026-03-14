/**
 ANSIParserTests

 Responsibilities:
 - Validate ANSI escape sequence parsing in Workspace.
 - Cover SGR codes for colors (16-color, 256-color, true color),
   text attributes (bold, italic), and edge cases.

 Does not handle:
 - UI rendering (covered by RunControlConsoleView tests).
 - CLI client functionality (covered by RalphCLIClientTests).

 Invariants/assumptions callers must respect:
 - Tests run with a fresh Workspace instance.
 - parseANSICodes operates on the attributedOutput property.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class ANSIParserTests: RalphCoreTestCase {

    var workspace: Workspace!

    override func setUp() async throws {
        try await super.setUp()
        workspace = Workspace(workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "ansi-parser"))
    }

    override func tearDown() async throws {
        workspace = nil
        try await super.tearDown()
    }

    // MARK: - Basic Parsing

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

    // MARK: - 16-Color Support

    func test_foregroundColors_standard() async {
        // Test each standard color (30-37)
        let colorTests: [(code: Int, color: Workspace.ANSIColor)] = [
            (30, .black), (31, .red), (32, .green), (33, .yellow),
            (34, .blue), (35, .magenta), (36, .cyan), (37, .white)
        ]

        for (code, expectedColor) in colorTests {
            workspace.attributedOutput = []
            workspace.parseANSICodes(from: "\u{001B}[\(code)mtest\u{001B}[0m")

            XCTAssertEqual(workspace.attributedOutput.count, 1, "Failed for code \(code)")
            XCTAssertEqual(workspace.attributedOutput[0].text, "test", "Failed for code \(code)")
            XCTAssertEqual(workspace.attributedOutput[0].color, expectedColor, "Failed for code \(code)")
        }
    }

    func test_foregroundColors_bright() async {
        // Test bright colors (90-97)
        let colorTests: [(code: Int, color: Workspace.ANSIColor)] = [
            (90, .brightBlack), (91, .brightRed), (92, .brightGreen), (93, .brightYellow),
            (94, .brightBlue), (95, .brightMagenta), (96, .brightCyan), (97, .brightWhite)
        ]

        for (code, expectedColor) in colorTests {
            workspace.attributedOutput = []
            workspace.parseANSICodes(from: "\u{001B}[\(code)mtest\u{001B}[0m")

            XCTAssertEqual(workspace.attributedOutput.count, 1, "Failed for code \(code)")
            XCTAssertEqual(workspace.attributedOutput[0].color, expectedColor, "Failed for code \(code)")
        }
    }

    // MARK: - Text Attributes

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
        // Empty SGR (just ESC[m) should reset
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

    // MARK: - 256-Color Support

    func test_256Color_foreground() async {
        // Test various 256-color indices
        workspace.parseANSICodes(from: "\u{001B}[38;5;196mred256\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput[0].text, "red256")
        // 196 is in the red region of the 6x6x6 cube
        if case .indexed(let index) = workspace.attributedOutput[0].color {
            XCTAssertEqual(index, 196)
        } else {
            XCTFail("Expected indexed color")
        }
    }

    func test_256Color_grayscale() async {
        workspace.parseANSICodes(from: "\u{001B}[38;5;245mgray\u{001B}[0m")

        if case .indexed(let index) = workspace.attributedOutput[0].color {
            XCTAssertEqual(index, 245)
        } else {
            XCTFail("Expected indexed color")
        }
    }

    func test_256Color_background_parsedButNotInSegment() async {
        // Background color is parsed but not stored in ANSISegment
        // This test ensures it doesn't crash
        workspace.parseANSICodes(from: "\u{001B}[48;5;196mtext\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput[0].text, "text")
        // Foreground should be default, background was parsed but not stored
        XCTAssertEqual(workspace.attributedOutput[0].color, .default)
    }

    // MARK: - True Color (24-bit) Support

    func test_trueColor_rgb() async {
        workspace.parseANSICodes(from: "\u{001B}[38;2;255;128;0morange\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput[0].text, "orange")
        if case .rgb(let r, let g, let b) = workspace.attributedOutput[0].color {
            XCTAssertEqual(r, 255)
            XCTAssertEqual(g, 128)
            XCTAssertEqual(b, 0)
        } else {
            XCTFail("Expected RGB color")
        }
    }

    func test_trueColor_black() async {
        workspace.parseANSICodes(from: "\u{001B}[38;2;0;0;0mblack\u{001B}[0m")

        if case .rgb(let r, let g, let b) = workspace.attributedOutput[0].color {
            XCTAssertEqual(r, 0)
            XCTAssertEqual(g, 0)
            XCTAssertEqual(b, 0)
        } else {
            XCTFail("Expected RGB color")
        }
    }

    func test_trueColor_white() async {
        workspace.parseANSICodes(from: "\u{001B}[38;2;255;255;255mwhite\u{001B}[0m")

        if case .rgb(let r, let g, let b) = workspace.attributedOutput[0].color {
            XCTAssertEqual(r, 255)
            XCTAssertEqual(g, 255)
            XCTAssertEqual(b, 255)
        } else {
            XCTFail("Expected RGB color")
        }
    }

    func test_trueColor_clampsValues() async {
        // Values outside 0-255 should be clamped
        workspace.parseANSICodes(from: "\u{001B}[38;2;300;-10;500mcolor\u{001B}[0m")

        if case .rgb(let r, let g, let b) = workspace.attributedOutput[0].color {
            XCTAssertEqual(r, 255)
            XCTAssertEqual(g, 0)
            XCTAssertEqual(b, 255)
        } else {
            XCTFail("Expected RGB color")
        }
    }

    // MARK: - Cursor/Control Codes (Should be Stripped)

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

    // MARK: - Segment Merging

    func test_adjacentSameStyle_merged() async {
        // When parsing output with the same style in sequence, segments should merge
        // Note: parseANSICodes is stateless per call - state does not persist between calls
        workspace.parseANSICodes(from: "\u{001B}[31mred1red2\u{001B}[0m")

        // Should be a single segment (no reset in middle, so all red text is one segment)
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

    func test_incrementalStreamParsing_preservesANSIStateAcrossChunks() async {
        workspace.resetStreamProcessingState()

        workspace.consumeStreamTextChunk("\u{001B}[31mred")
        workspace.consumeStreamTextChunk(" still red\u{001B}[0m plain")

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

    // MARK: - Complex/Real-World Cases

    func test_cargoBuildStyle_output() async {
        // Simulates typical cargo build output
        let output = """
            \u{001B}[0m\u{001B}[0m\u{001B}[1m\u{001B}[32m   Compiling\u{001B}[0m mycrate v0.1.0
            \u{001B}[0m\u{001B}[0m\u{001B}[1m\u{001B}[32m    Finished\u{001B}[0m dev [unoptimized]
            """

        workspace.parseANSICodes(from: output)

        // Should parse without crashing and have content
        XCTAssertGreaterThan(workspace.attributedOutput.count, 0)
        // Check that some segments have bold styling
        let hasBold = workspace.attributedOutput.contains { $0.isBold }
        XCTAssertTrue(hasBold, "Should have bold segments")
    }

    func test_gitStatusStyle_output() async {
        // Simulates git status output with colors
        let output = """
            On branch main
            \u{001B}[31mdeleted:\u{001B}[m    file1.txt
            \u{001B}[32mnew file:\u{001B}[m   file2.txt
            \u{001B}[33mmodified:\u{001B}[m   file3.txt
            """

        workspace.parseANSICodes(from: output)

        // Should parse without crashing
        XCTAssertGreaterThan(workspace.attributedOutput.count, 0)

        // Verify colors are detected
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
        let output = "\u{001B}[31mred\u{001B}[32mgreen\u{001B}[34mblue\u{001B}[0mreset"

        workspace.parseANSICodes(from: output)

        XCTAssertEqual(workspace.attributedOutput.count, 4)
    }

    func test_incompleteEscapeSequence() async {
        // Incomplete escape should be treated as literal
        workspace.parseANSICodes(from: "text\u{001B}")

        XCTAssertEqual(workspace.attributedOutput[0].text, "text\u{001B}")
    }

    func test_incompleteCSI_noTerminator() async {
        // CSI without terminator should be treated as literal
        workspace.parseANSICodes(from: "text\u{001B}[31")

        XCTAssertEqual(workspace.attributedOutput[0].text, "text\u{001B}[31")
    }

    func test_invalidSGRCode_ignored() async {
        // Invalid codes should be ignored, not crash
        workspace.parseANSICodes(from: "\u{001B}[999mtext\u{001B}[0m")

        // Should still have the text, style may be default
        XCTAssertTrue(workspace.attributedOutput[0].text.contains("text"))
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
        // First segment: bold red
        XCTAssertTrue(workspace.attributedOutput[0].isBold)
        XCTAssertEqual(workspace.attributedOutput[0].color, .red)
        // Second segment: normal red (bold removed)
        XCTAssertFalse(workspace.attributedOutput[1].isBold)
        XCTAssertEqual(workspace.attributedOutput[1].color, .red)
        // Third segment: plain
        XCTAssertEqual(workspace.attributedOutput[2].color, .default)
    }

    func test_ansiColorHashable() async {
        // Test that ANSIColor is properly Hashable for comparison
        let color1: Workspace.ANSIColor = .red
        let color2: Workspace.ANSIColor = .red
        let color3: Workspace.ANSIColor = .blue
        let indexed1: Workspace.ANSIColor = .indexed(100)
        let indexed2: Workspace.ANSIColor = .indexed(100)
        let rgb1: Workspace.ANSIColor = .rgb(255, 128, 0)
        let rgb2: Workspace.ANSIColor = .rgb(255, 128, 0)

        XCTAssertEqual(color1, color2)
        XCTAssertNotEqual(color1, color3)
        XCTAssertEqual(indexed1, indexed2)
        XCTAssertEqual(rgb1, rgb2)
        XCTAssertNotEqual(color1, indexed1)
    }

    func test_ansiColor_swiftUIColor() async {
        // Test that swiftUIColor conversion works for all color types
        let colors: [Workspace.ANSIColor] = [
            .default, .black, .red, .green, .yellow, .blue, .magenta, .cyan, .white,
            .brightBlack, .brightRed, .brightGreen, .brightYellow,
            .brightBlue, .brightMagenta, .brightCyan, .brightWhite,
            .indexed(0), .indexed(100), .indexed(255),
            .rgb(0, 0, 0), .rgb(255, 255, 255), .rgb(128, 64, 32)
        ]

        for color in colors {
            _ = color.swiftUIColor  // Should not crash
        }
    }

    func test_backgroundColorCodes() async {
        // Background colors should be parsed but don't affect foreground
        workspace.parseANSICodes(from: "\u{001B}[41mred bg\u{001B}[0m")

        // Text should be default color (background color is parsed but not in segment)
        XCTAssertEqual(workspace.attributedOutput[0].color, .default)
    }

    func test_brightBackgroundColorCodes() async {
        workspace.parseANSICodes(from: "\u{001B}[101mbright red bg\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput[0].color, .default)
    }
}
