/**
 RalphModels

 Responsibilities:
 - Provide Codable models used by the macOS GUI for structured Ralph data.
 - Decode the JSON payload emitted by `ralph __cli-spec --format json` into a stable, typed model.
 - Offer a forward-compatible representation (`RalphJSONValue`) for unknown/extended JSON fields.
 - Provide helper logic for building CLI argv arrays from a selected command + user-entered values.

 Does not handle:
 - Spawning subprocesses or collecting output (see `RalphCLIClient.swift`).
 - Full semantic validation of clap rules (conflicts, requirements groups, etc.). The CLI remains the
   source of truth for validation and error messages.

 Invariants/assumptions callers must respect:
 - `__cli-spec` is expected to output valid JSON.
 - `RalphCLISpecDocument.version` is bumped only for breaking schema changes.
 - Unknown JSON fields must not crash decoding so the GUI remains forward-compatible within a major
   CLI spec version.
 */

import Foundation

/// A JSON value that preserves unknown shapes for forward compatibility.
public enum RalphJSONValue: Codable, Equatable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([RalphJSONValue])
    case object([String: RalphJSONValue])

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()

        if container.decodeNil() {
            self = .null
            return
        }
        if let b = try? container.decode(Bool.self) {
            self = .bool(b)
            return
        }
        if let d = try? container.decode(Double.self) {
            self = .number(d)
            return
        }
        if let s = try? container.decode(String.self) {
            self = .string(s)
            return
        }
        if let arr = try? container.decode([RalphJSONValue].self) {
            self = .array(arr)
            return
        }
        if let obj = try? container.decode([String: RalphJSONValue].self) {
            self = .object(obj)
            return
        }

        throw DecodingError.typeMismatch(
            RalphJSONValue.self,
            DecodingError.Context(
                codingPath: container.codingPath,
                debugDescription: "Unsupported JSON value"
            )
        )
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()

        switch self {
        case .null:
            try container.encodeNil()
        case .bool(let b):
            try container.encode(b)
        case .number(let d):
            try container.encode(d)
        case .string(let s):
            try container.encode(s)
        case .array(let a):
            try container.encode(a)
        case .object(let o):
            try container.encode(o)
        }
    }

    public var objectValue: [String: RalphJSONValue]? {
        guard case .object(let obj) = self else { return nil }
        return obj
    }

    public var arrayValue: [RalphJSONValue]? {
        guard case .array(let arr) = self else { return nil }
        return arr
    }

    public var stringValue: String? {
        guard case .string(let s) = self else { return nil }
        return s
    }

    public var boolValue: Bool? {
        guard case .bool(let b) = self else { return nil }
        return b
    }

    public var numberValue: Double? {
        guard case .number(let d) = self else { return nil }
        return d
    }
}

/// Top-level container for the JSON emitted by `ralph __cli-spec`.
///
/// The output format is treated as an opaque JSON blob so this model remains usable
/// while the CLI spec evolves.
public struct RalphCLISpec: Codable, Equatable, Sendable {
    public let raw: RalphJSONValue

    public init(raw: RalphJSONValue) {
        self.raw = raw
    }

    public init(from decoder: Decoder) throws {
        self.raw = try RalphJSONValue(from: decoder)
    }

    public func encode(to encoder: Encoder) throws {
        try raw.encode(to: encoder)
    }
}

/// Stable, versioned schema for `ralph __cli-spec --format json`.
public struct RalphCLISpecDocument: Codable, Equatable, Sendable {
    public let version: Int
    public let root: RalphCLICommandSpec

    public init(version: Int, root: RalphCLICommandSpec) {
        self.version = version
        self.root = root
    }
}

public struct RalphCLICommandSpec: Codable, Equatable, Sendable, Identifiable, Hashable {
    public var id: String {
        path.joined(separator: " ")
    }

    public let name: String
    public let path: [String]
    public let about: String?
    public let longAbout: String?
    public let afterLongHelp: String?
    public let hidden: Bool
    public let args: [RalphCLIArgSpec]
    public let subcommands: [RalphCLICommandSpec]

    public init(
        name: String,
        path: [String],
        about: String?,
        longAbout: String?,
        afterLongHelp: String?,
        hidden: Bool,
        args: [RalphCLIArgSpec],
        subcommands: [RalphCLICommandSpec]
    ) {
        self.name = name
        self.path = path
        self.about = about
        self.longAbout = longAbout
        self.afterLongHelp = afterLongHelp
        self.hidden = hidden
        self.args = args
        self.subcommands = subcommands
    }

    private enum CodingKeys: String, CodingKey {
        case name
        case path
        case about
        case longAbout = "long_about"
        case afterLongHelp = "after_long_help"
        case hidden
        case args
        case subcommands
    }
}

public struct RalphCLIArgSpec: Codable, Equatable, Sendable, Identifiable, Hashable {
    public let id: String
    public let long: String?
    public let short: String?
    public let help: String?
    public let longHelp: String?
    public let required: Bool
    public let global: Bool
    public let hidden: Bool
    public let positional: Bool
    public let index: Int?
    public let action: String

    // Extended fields (optional for forward/backward compatibility as the Rust emitter evolves).
    public let defaultValues: [String]?
    public let possibleValues: [String]?
    public let valueEnum: Bool?
    public let numArgsMin: Int?
    public let numArgsMax: Int?

    public init(
        id: String,
        long: String?,
        short: String?,
        help: String?,
        longHelp: String?,
        required: Bool,
        global: Bool,
        hidden: Bool,
        positional: Bool,
        index: Int?,
        action: String,
        defaultValues: [String]?,
        possibleValues: [String]?,
        valueEnum: Bool?,
        numArgsMin: Int?,
        numArgsMax: Int?
    ) {
        self.id = id
        self.long = long
        self.short = short
        self.help = help
        self.longHelp = longHelp
        self.required = required
        self.global = global
        self.hidden = hidden
        self.positional = positional
        self.index = index
        self.action = action
        self.defaultValues = defaultValues
        self.possibleValues = possibleValues
        self.valueEnum = valueEnum
        self.numArgsMin = numArgsMin
        self.numArgsMax = numArgsMax
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case long
        case short
        case help
        case longHelp = "long_help"
        case required
        case defaultValues = "default_values"
        case possibleValues = "possible_values"
        case valueEnum = "value_enum"
        case numArgsMin = "num_args_min"
        case numArgsMax = "num_args_max"
        case global
        case hidden
        case positional
        case index
        case action
    }
}

public enum RalphCLIArgValue: Equatable, Sendable, Hashable {
    case flag(Bool)
    case count(Int)
    case values([String])
}

public enum RalphCLIArgumentBuilder {
    /// Build argv suitable for `Process.arguments` (does not include the executable path).
    ///
    /// `command.path` is expected to include the root binary name (e.g. `["ralph","queue","list"]`).
    /// The builder will drop the first segment.
    public static func buildArguments(
        command: RalphCLICommandSpec,
        selections: [String: RalphCLIArgValue],
        globalArguments: [String] = []
    ) -> [String] {
        var argv: [String] = []
        argv.append(contentsOf: globalArguments)
        argv.append(contentsOf: command.path.dropFirst())

        let (positionals, options): ([RalphCLIArgSpec], [RalphCLIArgSpec]) = command.args.reduce(into: ([], [])) { acc, arg in
            if arg.positional {
                acc.0.append(arg)
            } else {
                acc.1.append(arg)
            }
        }

        for arg in options {
            guard let value = selections[arg.id] else { continue }
            argv.append(contentsOf: buildOptionTokens(arg: arg, value: value))
        }

        let sortedPositionals = positionals.sorted { (a, b) in
            (a.index ?? Int.max) < (b.index ?? Int.max)
        }
        for arg in sortedPositionals {
            guard let value = selections[arg.id] else { continue }
            argv.append(contentsOf: buildPositionalTokens(arg: arg, value: value))
        }

        return argv
    }

    private static func buildPositionalTokens(arg: RalphCLIArgSpec, value: RalphCLIArgValue) -> [String] {
        guard arg.positional else { return [] }
        switch value {
        case .values(let values):
            return values
        case .flag, .count:
            return []
        }
    }

    private static func buildOptionTokens(arg: RalphCLIArgSpec, value: RalphCLIArgValue) -> [String] {
        guard !arg.positional else { return [] }
        guard let token = arg.preferredToken else {
            // No short/long. Probably a generated positional or internal clap arg; ignore.
            return []
        }

        switch value {
        case .flag(let present):
            return present ? [token] : []
        case .count(let n):
            guard n > 0 else { return [] }
            return Array(repeating: token, count: n)
        case .values(let values):
            let normalized = values.filter { !$0.isEmpty }
            guard !normalized.isEmpty else { return [] }

            if arg.numArgsMax == nil || (arg.numArgsMax ?? 0) > 1 {
                return [token] + normalized
            }

            if arg.action.contains("Append") {
                var out: [String] = []
                out.reserveCapacity(normalized.count * 2)
                for v in normalized {
                    out.append(token)
                    out.append(v)
                }
                return out
            }

            // Default: single value (take the first).
            return [token, normalized[0]]
        }
    }
}

public extension RalphCLIArgSpec {
    var preferredToken: String? {
        if let long {
            return "--\(long)"
        }
        if let short, !short.isEmpty {
            return "-\(short)"
        }
        return nil
    }

    var isCountFlag: Bool {
        action.contains("Count")
    }

    var isBooleanFlag: Bool {
        action.contains("SetTrue") || action.contains("SetFalse") || action.contains("Help") || action.contains("Version")
    }

    var takesValue: Bool {
        if positional { return true }
        if isCountFlag || isBooleanFlag { return false }
        // Prefer the emitted num-args contract when available.
        if let max = numArgsMax {
            return max > 0
        }
        // Unbounded/unknown: assume value-taking.
        return true
    }

    var allowsMultipleValues: Bool {
        if action.contains("Append") {
            return true
        }
        if numArgsMax == nil {
            return true
        }
        return (numArgsMax ?? 0) > 1
    }
}
