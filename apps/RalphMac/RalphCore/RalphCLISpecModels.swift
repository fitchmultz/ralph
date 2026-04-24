/**
 RalphCLISpecModels

 Purpose:
 - Define typed models for the CLI spec emitted by `ralph machine cli-spec`.

 Responsibilities:
 - Define typed models for the CLI spec emitted by `ralph machine cli-spec`.
 - Keep both opaque and versioned schema representations available to the app.

 Does not handle:
 - Building argv tokens from user selections.
 - CLI process execution.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - `RalphCLISpecDocument.version` changes only for breaking schema updates.
 - `MachineCLISpecDocument.version` versions the app-facing wrapper independently.
 - Unknown raw JSON should remain decodable through `RalphCLISpec`.
 */

import Foundation

/// Top-level container for the JSON emitted by `ralph machine cli-spec`.
///
/// The output format is treated as an opaque JSON blob so this model remains usable
/// while the CLI spec evolves.
public struct RalphCLISpec: Codable, Equatable, Sendable {
    public let raw: RalphJSONValue

    public init(raw: RalphJSONValue) {
        self.raw = raw
    }

    public init(from decoder: any Decoder) throws {
        self.raw = try RalphJSONValue(from: decoder)
    }

    public func encode(to encoder: any Encoder) throws {
        try raw.encode(to: encoder)
    }
}

/// Stable, versioned schema for the CLI spec nested inside `ralph machine cli-spec`.
public struct RalphCLISpecDocument: Codable, Equatable, Sendable {
    public let version: Int
    public let root: RalphCLICommandSpec

    public init(version: Int, root: RalphCLICommandSpec) {
        self.version = version
        self.root = root
    }
}

public struct MachineCLISpecDocument: Codable, Equatable, Sendable, VersionedMachineDocument {
    public static let expectedVersion = RalphMachineContract.cliSpecVersion
    public static let documentName = "machine cli-spec"

    public let version: Int
    public let spec: RalphCLISpecDocument
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
        if let max = numArgsMax {
            return max > 0
        }
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
