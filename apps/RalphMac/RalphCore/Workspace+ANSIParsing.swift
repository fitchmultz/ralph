//! Workspace+ANSIParsing
//!
//! Responsibilities:
//! - Parse ANSI SGR escape sequences into styled console segments.
//! - Retain a bounded attributed-output model for SwiftUI console rendering.
//! - Merge adjacent compatible segments to reduce memory pressure.
//!
//! Does not handle:
//! - Raw output buffering.
//! - CLI execution or stream collection.
//! - Console view rendering.
//!
//! Invariants/assumptions callers must respect:
//! - Parsed state is stored in `workspace.attributedOutput`.
//! - Background colors are parsed for correctness but not yet rendered in `ANSISegment`.
//! - Truncation keeps the newest segments.

public import Foundation
public import SwiftUI

public extension Workspace {
    struct ANSISegment: Identifiable, Sendable {
        public let id = UUID()
        public let text: String
        public let color: ANSIColor
        public let isBold: Bool
        public let isItalic: Bool

        public init(text: String, color: ANSIColor = .default, isBold: Bool = false, isItalic: Bool = false) {
            self.text = text
            self.color = color
            self.isBold = isBold
            self.isItalic = isItalic
        }
    }

    enum ANSIColor: Sendable, Hashable {
        case `default`
        case black
        case red
        case green
        case yellow
        case blue
        case magenta
        case cyan
        case white
        case brightBlack
        case brightRed
        case brightGreen
        case brightYellow
        case brightBlue
        case brightMagenta
        case brightCyan
        case brightWhite
        case indexed(UInt8)
        case rgb(UInt8, UInt8, UInt8)

        public var swiftUIColor: SwiftUI.Color {
            switch self {
            case .default: return .primary
            case .black: return .black
            case .red: return .red
            case .green: return .green
            case .yellow: return .yellow
            case .blue: return .blue
            case .magenta: return .purple
            case .cyan: return .cyan
            case .white: return .white
            case .brightBlack: return .gray
            case .brightRed: return .red.opacity(0.8)
            case .brightGreen: return .green.opacity(0.8)
            case .brightYellow: return .yellow.opacity(0.8)
            case .brightBlue: return .blue.opacity(0.8)
            case .brightMagenta: return .purple.opacity(0.8)
            case .brightCyan: return .cyan.opacity(0.8)
            case .brightWhite: return .white.opacity(0.9)
            case .indexed(let index):
                return Self.colorFrom256(index)
            case .rgb(let r, let g, let b):
                return Color(red: Double(r) / 255, green: Double(g) / 255, blue: Double(b) / 255)
            }
        }

        private static func colorFrom256(_ index: UInt8) -> Color {
            if index < 16 {
                let colors: [ANSIColor] = [
                    .black, .red, .green, .yellow, .blue, .magenta, .cyan, .white,
                    .brightBlack, .brightRed, .brightGreen, .brightYellow,
                    .brightBlue, .brightMagenta, .brightCyan, .brightWhite,
                ]
                return colors[Int(index)].swiftUIColor
            } else if index < 232 {
                let i = Int(index) - 16
                let r = i / 36
                let g = (i % 36) / 6
                let b = i % 6
                let rf = Double(r == 0 ? 0 : r * 40 + 55) / 255
                let gf = Double(g == 0 ? 0 : g * 40 + 55) / 255
                let bf = Double(b == 0 ? 0 : b * 40 + 55) / 255
                return Color(red: rf, green: gf, blue: bf)
            } else {
                let gray = 8 + (Int(index) - 232) * 10
                let gf = Double(gray) / 255
                return Color(red: gf, green: gf, blue: gf)
            }
        }
    }

    func parseANSICodes(from rawOutput: String, appendToExisting: Bool = true) {
        if !appendToExisting {
            attributedOutput = []
        }

        var segments: [ANSISegment] = []
        var currentState = ANSIStyleState()
        var currentText = ""
        var index = rawOutput.startIndex

        while index < rawOutput.endIndex {
            if rawOutput[index] == "\u{001B}",
                index < rawOutput.index(before: rawOutput.endIndex),
                rawOutput[rawOutput.index(after: index)] == "[" {
                let afterBracket = rawOutput.index(index, offsetBy: 2)
                var commandEnd = afterBracket
                var commandChars = ""

                while commandEnd < rawOutput.endIndex {
                    let char = rawOutput[commandEnd]
                    if (char >= "A" && char <= "Z") || (char >= "a" && char <= "z" && char != "[") {
                        if char == "m" {
                            if !currentText.isEmpty {
                                segments.append(ANSISegment(
                                    text: currentText,
                                    color: currentState.foregroundColor,
                                    isBold: currentState.isBold,
                                    isItalic: currentState.isItalic
                                ))
                                currentText = ""
                            }

                            if commandChars.isEmpty {
                                currentState.reset()
                            } else {
                                var params = commandChars.split(separator: ";").compactMap { Int($0) }
                                if params.isEmpty {
                                    params = [0]
                                }

                                var i = 0
                                while i < params.count {
                                    let code = params[i]
                                    if code == 38 && i + 1 < params.count {
                                        let subCode = params[i + 1]
                                        if subCode == 5 && i + 2 < params.count {
                                            currentState.foregroundColor = .indexed(UInt8(params[i + 2]))
                                            i += 3
                                        } else if subCode == 2 && i + 4 < params.count {
                                            currentState.foregroundColor = .rgb(
                                                UInt8(max(0, min(255, params[i + 2]))),
                                                UInt8(max(0, min(255, params[i + 3]))),
                                                UInt8(max(0, min(255, params[i + 4])))
                                            )
                                            i += 5
                                        } else {
                                            i += 1
                                        }
                                    } else if code == 48 && i + 1 < params.count {
                                        let subCode = params[i + 1]
                                        if subCode == 5 && i + 2 < params.count {
                                            currentState.backgroundColor = .indexed(UInt8(params[i + 2]))
                                            i += 3
                                        } else if subCode == 2 && i + 4 < params.count {
                                            currentState.backgroundColor = .rgb(
                                                UInt8(max(0, min(255, params[i + 2]))),
                                                UInt8(max(0, min(255, params[i + 3]))),
                                                UInt8(max(0, min(255, params[i + 4])))
                                            )
                                            i += 5
                                        } else {
                                            i += 1
                                        }
                                    } else {
                                        currentState.applySGR(code)
                                        i += 1
                                    }
                                }
                            }
                        }

                        index = rawOutput.index(after: commandEnd)
                        break
                    } else if char == "[" {
                        index = afterBracket
                        break
                    } else {
                        commandChars.append(char)
                        commandEnd = rawOutput.index(after: commandEnd)
                    }
                }

                if commandEnd >= rawOutput.endIndex {
                    currentText.append(rawOutput[index])
                    index = rawOutput.index(after: index)
                }
            } else {
                currentText.append(rawOutput[index])
                index = rawOutput.index(after: index)
            }
        }

        if !currentText.isEmpty {
            segments.append(ANSISegment(
                text: currentText,
                color: currentState.foregroundColor,
                isBold: currentState.isBold,
                isItalic: currentState.isItalic
            ))
        }

        if attributedOutput.isEmpty {
            attributedOutput = segments
        } else {
            for segment in segments {
                if let last = attributedOutput.last,
                    last.color == segment.color,
                    last.isBold == segment.isBold,
                    last.isItalic == segment.isItalic {
                    attributedOutput[attributedOutput.count - 1] = ANSISegment(
                        text: last.text + segment.text,
                        color: segment.color,
                        isBold: segment.isBold,
                        isItalic: segment.isItalic
                    )
                } else {
                    attributedOutput.append(segment)
                }
            }
        }

        attributedOutput = mergeAdjacentSegments(attributedOutput)
    }

    func enforceANSISegmentLimit() {
        guard attributedOutput.count > maxANSISegments else { return }

        attributedOutput = Array(attributedOutput.suffix(maxANSISegments))

        let indicatorText = "\n... [console output truncated due to length] ...\n"
        if !attributedOutput.isEmpty, attributedOutput[0].text != indicatorText {
            let indicator = ANSISegment(
                text: indicatorText,
                color: .yellow,
                isBold: false,
                isItalic: true
            )
            attributedOutput.insert(indicator, at: 0)
        }
    }
}

private extension Workspace {
    struct ANSIStyleState {
        var foregroundColor: ANSIColor = .default
        var backgroundColor: ANSIColor = .default
        var isBold: Bool = false
        var isItalic: Bool = false
        var isDim: Bool = false
        var isUnderline: Bool = false

        mutating func reset() {
            foregroundColor = .default
            backgroundColor = .default
            isBold = false
            isItalic = false
            isDim = false
            isUnderline = false
        }

        mutating func applySGR(_ code: Int) {
            switch code {
            case 0: reset()
            case 1: isBold = true
            case 2: isDim = true
            case 3: isItalic = true
            case 4: isUnderline = true
            case 22:
                isBold = false
                isDim = false
            case 23: isItalic = false
            case 24: isUnderline = false
            case 30 ... 37: foregroundColor = colorFromCode(code)
            case 38: break
            case 39: foregroundColor = .default
            case 40 ... 47: backgroundColor = colorFromCode(code - 10)
            case 48: break
            case 49: backgroundColor = .default
            case 90 ... 97: foregroundColor = colorFromCode(code)
            case 100 ... 107: backgroundColor = colorFromCode(code - 10)
            default: break
            }
        }

        private func colorFromCode(_ code: Int) -> ANSIColor {
            switch code {
            case 30, 40: return .black
            case 31, 41: return .red
            case 32, 42: return .green
            case 33, 43: return .yellow
            case 34, 44: return .blue
            case 35, 45: return .magenta
            case 36, 46: return .cyan
            case 37, 47: return .white
            case 90, 100: return .brightBlack
            case 91, 101: return .brightRed
            case 92, 102: return .brightGreen
            case 93, 103: return .brightYellow
            case 94, 104: return .brightBlue
            case 95, 105: return .brightMagenta
            case 96, 106: return .brightCyan
            case 97, 107: return .brightWhite
            default: return .default
            }
        }
    }

    func mergeAdjacentSegments(_ segments: [ANSISegment]) -> [ANSISegment] {
        guard segments.count > 1 else { return segments }

        var merged: [ANSISegment] = []
        merged.reserveCapacity(segments.count)

        for segment in segments {
            if let last = merged.last,
                last.color == segment.color,
                last.isBold == segment.isBold,
                last.isItalic == segment.isItalic {
                merged[merged.count - 1] = ANSISegment(
                    text: last.text + segment.text,
                    color: segment.color,
                    isBold: segment.isBold,
                    isItalic: segment.isItalic
                )
            } else {
                merged.append(segment)
            }
        }

        return merged
    }
}
