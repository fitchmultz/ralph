/**
 RalphModelsTests

 Responsibilities:
 - Validate decoding/encoding of the forward-compatible JSON model types.
 - Ensure `RalphCLISpec` can decode arbitrary JSON emitted by a future `__cli-spec` command.

 Does not handle:
 - Validating the *meaning* of any particular CLI spec schema.

 Invariants/assumptions callers must respect:
 - JSON fixtures used in tests are representative enough to catch regressions in generic decoding.
 */

import Foundation
import XCTest

@testable import RalphCore

final class RalphModelsTests: XCTestCase {
    func test_decode_cliSpec_rawJSON() throws {
        let json = #"""
        {
          "tool": "ralph",
          "version": "0.1.0",
          "commands": [
            { "name": "queue", "about": "Inspect and manage the task queue" },
            { "name": "init", "about": "Bootstrap Ralph files" }
          ],
          "flags": [
            { "long": "--force", "short": "-f", "takes_value": false }
          ],
          "meta": {
            "generated_at": 1738960000,
            "stable": true,
            "notes": null
          }
        }
        """#

        let spec = try JSONDecoder().decode(RalphCLISpec.self, from: Data(json.utf8))
        guard case .object(let obj) = spec.raw else {
            return XCTFail("expected top-level object")
        }

        XCTAssertEqual(obj["tool"]?.stringValue, "ralph")
        XCTAssertEqual(obj["version"]?.stringValue, "0.1.0")

        let commands = obj["commands"]?.arrayValue
        XCTAssertEqual(commands?.count, 2)

        let meta = obj["meta"]?.objectValue
        XCTAssertEqual(meta?["stable"]?.boolValue, true)
        XCTAssertEqual(meta?["notes"], .null)
    }

    func test_decode_cliSpecDocument_v2_minimal() throws {
        let json = #"""
        {
          "version": 2,
          "root": {
            "name": "ralph",
            "path": ["ralph"],
            "about": "Ralph is a Rust CLI for running AI agent loops against a JSON task queue",
            "hidden": false,
            "args": [
              {
                "id": "no_color",
                "long": "no-color",
                "help": "Disable color output",
                "required": false,
                "global": true,
                "hidden": false,
                "positional": false,
                "action": "SetTrue",
                "default_values": [],
                "possible_values": [],
                "value_enum": false,
                "num_args_min": 0,
                "num_args_max": 0
              }
            ],
            "subcommands": [
              {
                "name": "queue",
                "path": ["ralph", "queue"],
                "hidden": false,
                "args": [],
                "subcommands": [
                  {
                    "name": "list",
                    "path": ["ralph", "queue", "list"],
                    "hidden": false,
                    "args": [
                      {
                        "id": "format",
                        "long": "format",
                        "help": "Output format",
                        "required": false,
                        "global": false,
                        "hidden": false,
                        "positional": false,
                        "action": "Set",
                        "possible_values": ["json", "text"],
                        "default_values": ["text"],
                        "value_enum": true,
                        "num_args_min": 1,
                        "num_args_max": 1
                      }
                    ],
                    "subcommands": []
                  }
                ]
              }
            ]
          }
        }
        """#

        let doc = try JSONDecoder().decode(RalphCLISpecDocument.self, from: Data(json.utf8))
        XCTAssertEqual(doc.version, 2)
        XCTAssertEqual(doc.root.name, "ralph")
        XCTAssertEqual(doc.root.path, ["ralph"])
        XCTAssertEqual(doc.root.subcommands.first?.name, "queue")
    }

    func test_cliArgumentBuilder_buildsExpectedArgv() throws {
        let command = RalphCLICommandSpec(
            name: "list",
            path: ["ralph", "queue", "list"],
            about: nil,
            longAbout: nil,
            afterLongHelp: nil,
            hidden: false,
            args: [
                RalphCLIArgSpec(
                    id: "format",
                    long: "format",
                    short: nil,
                    help: nil,
                    longHelp: nil,
                    required: false,
                    global: false,
                    hidden: false,
                    positional: false,
                    index: nil,
                    action: "Set",
                    defaultValues: nil,
                    possibleValues: nil,
                    valueEnum: nil,
                    numArgsMin: 1,
                    numArgsMax: 1
                ),
                RalphCLIArgSpec(
                    id: "verbose",
                    long: "verbose",
                    short: "v",
                    help: nil,
                    longHelp: nil,
                    required: false,
                    global: false,
                    hidden: false,
                    positional: false,
                    index: nil,
                    action: "Count",
                    defaultValues: nil,
                    possibleValues: nil,
                    valueEnum: nil,
                    numArgsMin: 0,
                    numArgsMax: 0
                ),
                RalphCLIArgSpec(
                    id: "task_id",
                    long: nil,
                    short: nil,
                    help: nil,
                    longHelp: nil,
                    required: true,
                    global: false,
                    hidden: false,
                    positional: true,
                    index: 1,
                    action: "Set",
                    defaultValues: nil,
                    possibleValues: nil,
                    valueEnum: nil,
                    numArgsMin: 1,
                    numArgsMax: 1
                ),
            ],
            subcommands: []
        )

        let argv = RalphCLIArgumentBuilder.buildArguments(
            command: command,
            selections: [
                "format": .values(["json"]),
                "verbose": .count(2),
                "task_id": .values(["RQ-0001"]),
            ],
            globalArguments: ["--no-color"]
        )

        XCTAssertEqual(argv, ["--no-color", "queue", "list", "--format", "json", "--verbose", "--verbose", "RQ-0001"])
    }

    func test_roundTrip_encode_decode_preservesShape() throws {
        let value: RalphJSONValue = .object([
            "a": .number(1),
            "b": .array([.string("x"), .null]),
            "c": .bool(false),
        ])

        let data = try JSONEncoder().encode(RalphCLISpec(raw: value))
        let decoded = try JSONDecoder().decode(RalphCLISpec.self, from: data)
        XCTAssertEqual(decoded.raw, value)
    }
}
