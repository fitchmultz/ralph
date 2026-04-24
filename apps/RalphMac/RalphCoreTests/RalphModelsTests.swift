/**
 RalphModelsTests

 Purpose:
 - Validate decoding/encoding of the forward-compatible JSON model types.

 Responsibilities:
 - Validate decoding/encoding of the forward-compatible JSON model types.
 - Ensure `RalphCLISpec` can decode arbitrary JSON emitted by a future machine CLI-spec document.

 Does not handle:
 - Validating the *meaning* of any particular CLI spec schema.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - JSON fixtures used in tests are representative enough to catch regressions in generic decoding.
 */

import Foundation
import XCTest

@testable import RalphCore

final class RalphModelsTests: RalphCoreTestCase {
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

    func test_decode_taskQueueDocument_objectShape() throws {
        let json = #"""
        {
          "version": 3,
          "tasks": [
            {
              "id": "RQ-1001",
              "status": "todo",
              "title": "Object shape task",
              "priority": "medium",
              "tags": ["ui"]
            }
          ]
        }
        """#

        let document = try JSONDecoder().decode(RalphTaskQueueDocument.self, from: Data(json.utf8))
        XCTAssertEqual(document.version, 3)
        XCTAssertEqual(document.tasks.count, 1)
        XCTAssertEqual(document.tasks[0].id, "RQ-1001")
    }

    func test_decode_taskQueueDocument_arrayShape_fails() {
        let json = #"""
        [
          {
            "id": "RQ-2001",
            "status": "doing",
            "title": "Array shape task",
            "priority": "high",
            "tags": ["macos"]
          }
        ]
        """#

        XCTAssertThrowsError(try JSONDecoder().decode(RalphTaskQueueDocument.self, from: Data(json.utf8)))
    }

    func test_decode_taskQueueDocument_withAgentOverrides() throws {
        let json = #"""
        {
          "version": 1,
          "tasks": [
            {
              "id": "RQ-3001",
              "status": "todo",
              "title": "Task with overrides",
              "priority": "high",
              "tags": [],
              "agent": {
                "runner": "codex",
                "model": "gpt-5.3-codex",
                "model_effort": "high",
                "phases": 2,
                "iterations": 1,
                "phase_overrides": {
                  "phase1": {
                    "runner": "codex",
                    "model": "gpt-5.3-codex",
                    "reasoning_effort": "high"
                  },
                  "phase2": {
                    "runner": "kimi",
                    "model": "kimi-code/kimi-for-coding"
                  }
                }
              }
            }
          ]
        }
        """#

        let document = try JSONDecoder().decode(RalphTaskQueueDocument.self, from: Data(json.utf8))
        XCTAssertEqual(document.tasks.count, 1)
        let agent = try XCTUnwrap(document.tasks[0].agent)
        XCTAssertEqual(agent.runner, "codex")
        XCTAssertEqual(agent.model, "gpt-5.3-codex")
        XCTAssertEqual(agent.modelEffort, "high")
        XCTAssertEqual(agent.phases, 2)
        XCTAssertEqual(agent.iterations, 1)
        XCTAssertEqual(agent.phaseOverrides?.phase2?.runner, "kimi")
    }

    func test_encode_task_preservesAgentOverrides() throws {
        let task = RalphTask(
            id: "RQ-3002",
            status: .todo,
            title: "Encode overrides",
            priority: .medium,
            tags: [],
            agent: RalphTaskAgent(
                runner: "codex",
                model: "gpt-5.3-codex",
                modelEffort: "high",
                phases: 2,
                iterations: 1,
                phaseOverrides: RalphTaskPhaseOverrides(
                    phase1: RalphTaskPhaseOverride(
                        runner: "codex",
                        model: "gpt-5.3-codex",
                        reasoningEffort: "high"
                    ),
                    phase2: RalphTaskPhaseOverride(
                        runner: "kimi",
                        model: "kimi-code/kimi-for-coding",
                        reasoningEffort: nil
                    )
                )
            )
        )

        let data = try JSONEncoder().encode(task)
        let decoded = try JSONDecoder().decode(RalphTask.self, from: data)
        XCTAssertEqual(decoded.agent?.runner, "codex")
        XCTAssertEqual(decoded.agent?.phases, 2)
        XCTAssertEqual(decoded.agent?.phaseOverrides?.phase2?.runner, "kimi")
    }

    func test_normalizedTaskAgent_clears_invalid_values_and_blanks() {
        let normalized = RalphTaskAgent.normalizedOverride(
            RalphTaskAgent(
                runner: " codex ",
                model: " ",
                modelEffort: "default",
                phases: 9,
                iterations: 0,
                phaseOverrides: RalphTaskPhaseOverrides(
                    phase1: RalphTaskPhaseOverride(runner: " ", model: nil, reasoningEffort: nil),
                    phase2: RalphTaskPhaseOverride(runner: "kimi", model: " kimi-code/kimi-for-coding ", reasoningEffort: " ")
                )
            )
        )

        XCTAssertNotNil(normalized)
        XCTAssertEqual(normalized?.runner, "codex")
        XCTAssertNil(normalized?.model)
        XCTAssertNil(normalized?.modelEffort)
        XCTAssertNil(normalized?.phases)
        XCTAssertNil(normalized?.iterations)
        XCTAssertNil(normalized?.phaseOverrides?.phase1)
        XCTAssertEqual(normalized?.phaseOverrides?.phase2?.runner, "kimi")
        XCTAssertEqual(normalized?.phaseOverrides?.phase2?.model, "kimi-code/kimi-for-coding")
    }

    func test_executionPreset_codexDeep_values() throws {
        let agent = try XCTUnwrap(RalphTaskExecutionPreset.codexDeep.agentOverride)
        XCTAssertEqual(agent.runner, "codex")
        XCTAssertEqual(agent.model, "gpt-5.4")
        XCTAssertEqual(agent.modelEffort, "high")
        XCTAssertEqual(agent.phases, 3)
        XCTAssertEqual(agent.iterations, 1)
    }

    func test_executionPreset_kimiFast_values() throws {
        let agent = try XCTUnwrap(RalphTaskExecutionPreset.kimiFast.agentOverride)
        XCTAssertEqual(agent.runner, "codex")
        XCTAssertEqual(agent.model, "gpt-5.4")
        XCTAssertEqual(agent.modelEffort, "low")
        XCTAssertEqual(agent.phases, 1)
        XCTAssertEqual(agent.iterations, 1)
    }

    func test_executionPreset_matchingPreset_matchesAppliedPreset() {
        let preset = RalphTaskExecutionPreset.hybridCodexKimi
        XCTAssertEqual(
            RalphTaskExecutionPreset.matchingPreset(for: preset.agentOverride),
            .hybridCodexKimi
        )
    }

    func test_executionPreset_matchingPreset_returnsInherit_forNilAgent() {
        XCTAssertEqual(RalphTaskExecutionPreset.matchingPreset(for: nil), .inheritFromConfig)
    }

    func test_executionPreset_matchingPreset_returnsNil_forCustomAgent() {
        let custom = RalphTaskAgent(
            runner: "codex",
            model: "gpt-5.4",
            modelEffort: "xhigh",
            phases: 2,
            iterations: 4
        )
        XCTAssertNil(RalphTaskExecutionPreset.matchingPreset(for: custom))
    }
}
