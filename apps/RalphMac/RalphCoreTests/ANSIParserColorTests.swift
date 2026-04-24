/**
 ANSIParserColorTests

 Purpose:
 - Validate standard, bright, indexed, true-color, and background ANSI color handling.

 Responsibilities:
 - Validate standard, bright, indexed, true-color, and background ANSI color handling.
 - Cover ANSIColor utility semantics used by the UI layer.

 Does not handle:
 - Control-sequence stripping or incremental stream parsing.
 - Malformed-sequence recovery.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Background colors are parsed for stability but not persisted in ANSISegment foreground state.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class ANSIParserColorTests: ANSIParserTestCase {
    func test_foregroundColors_standard() async {
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

    func test_256Color_foreground() async {
        workspace.parseANSICodes(from: "\u{001B}[38;5;196mred256\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput[0].text, "red256")
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
        workspace.parseANSICodes(from: "\u{001B}[48;5;196mtext\u{001B}[0m")

        XCTAssertEqual(workspace.attributedOutput[0].text, "text")
        XCTAssertEqual(workspace.attributedOutput[0].color, .default)
    }

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
        workspace.parseANSICodes(from: "\u{001B}[38;2;300;-10;500mcolor\u{001B}[0m")

        if case .rgb(let r, let g, let b) = workspace.attributedOutput[0].color {
            XCTAssertEqual(r, 255)
            XCTAssertEqual(g, 0)
            XCTAssertEqual(b, 255)
        } else {
            XCTFail("Expected RGB color")
        }
    }

    func test_ansiColorHashable() async {
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
        let colors: [Workspace.ANSIColor] = [
            .default, .black, .red, .green, .yellow, .blue, .magenta, .cyan, .white,
            .brightBlack, .brightRed, .brightGreen, .brightYellow,
            .brightBlue, .brightMagenta, .brightCyan, .brightWhite,
            .indexed(0), .indexed(100), .indexed(255),
            .rgb(0, 0, 0), .rgb(255, 255, 255), .rgb(128, 64, 32)
        ]

        for color in colors {
            _ = color.swiftUIColor
        }
    }

    func test_backgroundColorCodes() async {
        workspace.parseANSICodes(from: "\u{001B}[41mred bg\u{001B}[0m")
        XCTAssertEqual(workspace.attributedOutput[0].color, .default)
    }

    func test_brightBackgroundColorCodes() async {
        workspace.parseANSICodes(from: "\u{001B}[101mbright red bg\u{001B}[0m")
        XCTAssertEqual(workspace.attributedOutput[0].color, .default)
    }
}
