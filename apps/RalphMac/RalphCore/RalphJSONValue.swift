/**
 RalphJSONValue

 Responsibilities:
 - Provide a forward-compatible JSON value model for CLI payloads and opaque task settings.
 - Preserve unknown JSON shapes during decode/encode round trips.

 Does not handle:
 - CLI schema validation.
 - Task-specific business rules.

 Invariants/assumptions callers must respect:
 - Unsupported decoder payloads should fail fast with a type mismatch.
 - The enum remains value-semantic and sendable.
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

    public init(from decoder: any Decoder) throws {
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

    public func encode(to encoder: any Encoder) throws {
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
