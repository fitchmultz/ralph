//! Workspace+ANSIParsing
//!
//! Responsibilities:
//! - Parse ANSI SGR escape sequences into styled console segments.
//! - Retain a bounded attributed-output model for SwiftUI console rendering.
//! - Support incremental/delta-aware parsing for hot stream paths.
//!
//! Does not handle:
//! - Raw output buffering.
//! - CLI execution or stream collection.
//! - Console view rendering.
//!
//! Invariants/assumptions callers must respect:
//! - Parsed state is stored in `workspace.attributedOutput`.
//! - Background colors are parsed for correctness but not yet rendered in `ANSISegment`.
//! - Truncation keeps the newest parsed segments.

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

    func parseANSICodes(from rawOutput: String, appendToExisting: Bool = false) {
        if appendToExisting {
            attributedOutput = runState.streamProcessor.append(
                chunk: rawOutput,
                maxSegments: maxANSISegments,
                finalizeTrailingEscape: true
            ).segments
        } else {
            attributedOutput = runState.streamProcessor.replace(
                content: rawOutput,
                maxSegments: maxANSISegments,
                finalizeTrailingEscape: true
            ).segments
        }
    }

    func enforceANSISegmentLimit() {
        attributedOutput = runState.streamProcessor.displaySegments(maxSegments: maxANSISegments)
    }
}

extension Workspace {
    func consumeStreamTextChunk(_ text: String) {
        let snapshot = runState.streamProcessor.append(
            chunk: text,
            maxSegments: maxANSISegments,
            finalizeTrailingEscape: false
        )
        attributedOutput = snapshot.segments
        if let detectedPhase = snapshot.detectedPhase {
            currentPhase = detectedPhase
        }
    }

    func resetStreamProcessingState() {
        runState.streamProcessor.reset()
        attributedOutput = []
    }
}

struct WorkspaceStreamSnapshot {
    let segments: [Workspace.ANSISegment]
    let detectedPhase: Workspace.ExecutionPhase?
}

final class WorkspaceStreamProcessor {
    private let ansiParser = WorkspaceANSIStreamParser()
    private let phaseTracker = WorkspacePhaseTracker()

    func reset() {
        ansiParser.reset()
        phaseTracker.reset()
    }

    func replace(
        content: String,
        maxSegments: Int,
        finalizeTrailingEscape: Bool
    ) -> WorkspaceStreamSnapshot {
        reset()
        return append(
            chunk: content,
            maxSegments: maxSegments,
            finalizeTrailingEscape: finalizeTrailingEscape
        )
    }

    func append(
        chunk: String,
        maxSegments: Int,
        finalizeTrailingEscape: Bool
    ) -> WorkspaceStreamSnapshot {
        let detectedPhase = phaseTracker.append(text: chunk)
        let segments = ansiParser.append(
            chunk: chunk,
            maxSegments: maxSegments,
            finalizeTrailingEscape: finalizeTrailingEscape
        )
        return WorkspaceStreamSnapshot(segments: segments, detectedPhase: detectedPhase)
    }

    func displaySegments(maxSegments: Int) -> [Workspace.ANSISegment] {
        ansiParser.displaySegments(maxSegments: maxSegments)
    }
}

private final class WorkspacePhaseTracker {
    private static let phaseMarkers: [(Workspace.ExecutionPhase, [String])] = [
        (.review, ["PHASE 3", "Phase 3", "REVIEWING", "Reviewing", "REVIEW", "# Phase 3", "## Phase 3"]),
        (.implement, ["PHASE 2", "Phase 2", "IMPLEMENTING", "Implementing", "IMPLEMENTATION", "# Phase 2", "## Phase 2"]),
        (.plan, ["PHASE 1", "Phase 1", "PLANNING", "Planning", "# Phase 1", "## Phase 1"]),
    ]
    private static let maxMarkerLength = phaseMarkers
        .flatMap { $0.1 }
        .map(\.count)
        .max() ?? 0

    private var rollingTail = ""

    func reset() {
        rollingTail = ""
    }

    func append(text: String) -> Workspace.ExecutionPhase? {
        let scanWindow = rollingTail + text
        let detected = Self.phaseMarkers.first { _, markers in
            markers.contains { scanWindow.contains($0) }
        }?.0
        rollingTail = String(scanWindow.suffix(Self.maxMarkerLength))
        return detected
    }
}

private final class WorkspaceANSIStreamParser {
    private var segments: [Workspace.ANSISegment] = []
    private var style = WorkspaceANSIStyleState()
    private var currentText = ""
    private var pendingEscape: String?
    private var truncated = false

    func reset() {
        segments.removeAll(keepingCapacity: false)
        style.reset()
        currentText.removeAll(keepingCapacity: false)
        pendingEscape = nil
        truncated = false
    }

    func append(
        chunk: String,
        maxSegments: Int,
        finalizeTrailingEscape: Bool
    ) -> [Workspace.ANSISegment] {
        for character in chunk {
            process(character)
        }

        if finalizeTrailingEscape, let pendingEscape {
            currentText.append(pendingEscape)
            self.pendingEscape = nil
        }

        flushCurrentText()
        enforceSegmentLimit(maxSegments: maxSegments)
        return displaySegments(maxSegments: maxSegments)
    }

    func displaySegments(maxSegments: Int) -> [Workspace.ANSISegment] {
        enforceSegmentLimit(maxSegments: maxSegments)
        guard truncated, !segments.isEmpty else { return segments }
        let indicator = Workspace.ANSISegment(
            text: "\n... [console output truncated due to length] ...\n",
            color: .yellow,
            isBold: false,
            isItalic: true
        )
        if segments.first?.text == indicator.text {
            return segments
        }
        return [indicator] + segments
    }

    private func process(_ character: Character) {
        if var pendingEscape {
            pendingEscape.append(character)
            if isEscapeTerminator(character) {
                applyEscapeSequence(pendingEscape)
                self.pendingEscape = nil
            } else {
                self.pendingEscape = pendingEscape
            }
            return
        }

        if character == "\u{001B}" {
            pendingEscape = String(character)
            return
        }

        currentText.append(character)
    }

    private func isEscapeTerminator(_ character: Character) -> Bool {
        guard let scalar = character.unicodeScalars.first?.value else { return false }
        return (65 ... 90).contains(scalar) || (97 ... 122).contains(scalar)
    }

    private func applyEscapeSequence(_ sequence: String) {
        guard sequence.hasPrefix("\u{001B}[") else {
            currentText.append(sequence)
            return
        }

        guard let command = sequence.last else {
            currentText.append(sequence)
            return
        }

        guard command == "m" else {
            return
        }

        flushCurrentText()

        let parameterString = String(sequence.dropFirst(2).dropLast())
        var parameters = parameterString.split(separator: ";").compactMap { Int($0) }
        if parameters.isEmpty {
            parameters = [0]
        }

        var index = 0
        while index < parameters.count {
            let code = parameters[index]
            if code == 38 && index + 1 < parameters.count {
                let subCode = parameters[index + 1]
                if subCode == 5 && index + 2 < parameters.count {
                    style.foregroundColor = .indexed(UInt8(parameters[index + 2]))
                    index += 3
                } else if subCode == 2 && index + 4 < parameters.count {
                    style.foregroundColor = .rgb(
                        UInt8(clamping: parameters[index + 2]),
                        UInt8(clamping: parameters[index + 3]),
                        UInt8(clamping: parameters[index + 4])
                    )
                    index += 5
                } else {
                    index += 1
                }
            } else if code == 48 && index + 1 < parameters.count {
                let subCode = parameters[index + 1]
                if subCode == 5 && index + 2 < parameters.count {
                    style.backgroundColor = .indexed(UInt8(parameters[index + 2]))
                    index += 3
                } else if subCode == 2 && index + 4 < parameters.count {
                    style.backgroundColor = .rgb(
                        UInt8(clamping: parameters[index + 2]),
                        UInt8(clamping: parameters[index + 3]),
                        UInt8(clamping: parameters[index + 4])
                    )
                    index += 5
                } else {
                    index += 1
                }
            } else {
                style.applySGR(code)
                index += 1
            }
        }
    }

    private func flushCurrentText() {
        guard !currentText.isEmpty else { return }
        appendSegment(
            Workspace.ANSISegment(
                text: currentText,
                color: style.foregroundColor,
                isBold: style.isBold,
                isItalic: style.isItalic
            )
        )
        currentText.removeAll(keepingCapacity: false)
    }

    private func appendSegment(_ segment: Workspace.ANSISegment) {
        guard !segment.text.isEmpty else { return }
        if let last = segments.last,
           last.color == segment.color,
           last.isBold == segment.isBold,
           last.isItalic == segment.isItalic {
            segments[segments.count - 1] = Workspace.ANSISegment(
                text: last.text + segment.text,
                color: segment.color,
                isBold: segment.isBold,
                isItalic: segment.isItalic
            )
        } else {
            segments.append(segment)
        }
    }

    private func enforceSegmentLimit(maxSegments: Int) {
        guard segments.count > maxSegments else { return }
        truncated = true
        segments = Array(segments.suffix(maxSegments))
    }
}

private struct WorkspaceANSIStyleState {
    var foregroundColor: Workspace.ANSIColor = .default
    var backgroundColor: Workspace.ANSIColor = .default
    var isBold = false
    var isItalic = false
    var isDim = false
    var isUnderline = false

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
        case 39: foregroundColor = .default
        case 40 ... 47: backgroundColor = colorFromCode(code - 10)
        case 49: backgroundColor = .default
        case 90 ... 97: foregroundColor = colorFromCode(code)
        case 100 ... 107: backgroundColor = colorFromCode(code - 10)
        default: break
        }
    }

    private func colorFromCode(_ code: Int) -> Workspace.ANSIColor {
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
