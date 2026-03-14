/**
 WorkspaceTaskDecomposeTests

 Responsibilities:
 - Verify Workspace task decomposition uses the CLI JSON contract correctly.
 - Exercise preview and write flows against a deterministic mock CLI.
 - Guard against regressions in argument construction, decoding, and post-write reload behavior.

 Does not handle:
 - SwiftUI presentation flows.
 - Real runner-backed planner execution.

 Invariants/assumptions callers must respect:
 - Tests run against an isolated executable script that emulates the CLI contract.
 - The mock CLI records arguments so tests can assert preview/write semantics precisely.
 */

import Foundation
import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceTaskDecomposeTests: RalphCoreTestCase {
    func test_previewTaskDecomposition_decodesPreviewAndPassesExpectedArguments() async throws {
        let fixture = try Self.makeMockCLIFixture()
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: fixture.scriptURL)
        )

        let preview = try await workspace.previewTaskDecomposition(
            source: .freeform("Build OAuth login"),
            options: TaskDecomposeOptions(
                attachToTaskID: "RQ-0042",
                maxDepth: 4,
                maxChildren: 6,
                maxNodes: 40,
                status: .todo,
                childPolicy: .append,
                withDependencies: true
            )
        )

        XCTAssertEqual(preview.plan.totalNodes, 3)
        XCTAssertEqual(preview.plan.leafNodes, 2)
        XCTAssertEqual(preview.childPolicy, .append)
        XCTAssertTrue(preview.withDependencies)
        XCTAssertEqual(preview.attachTarget?.task.id, "RQ-0042")

        let log = try String(contentsOf: fixture.logURL, encoding: .utf8)
        XCTAssertTrue(log.contains("machine task decompose Build OAuth login --max-depth 4 --max-children 6 --max-nodes 40 --status todo --child-policy append --with-dependencies --attach-to RQ-0042"))
        XCTAssertFalse(log.contains("--write"))
    }

    func test_writeTaskDecomposition_decodesWriteResultAndReloadsTasks() async throws {
        let fixture = try Self.makeMockCLIFixture()
        var workspace: Workspace!
        defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }

        workspace = Workspace(
            workingDirectoryURL: fixture.workspaceURL,
            client: try RalphCLIClient(executableURL: fixture.scriptURL)
        )

        let result = try await workspace.writeTaskDecomposition(
            source: .existingTaskID("RQ-0007"),
            options: TaskDecomposeOptions(
                maxDepth: 3,
                maxChildren: 4,
                maxNodes: 25,
                status: .draft,
                childPolicy: .fail,
                withDependencies: false
            )
        )

        XCTAssertEqual(result.parentTaskID, "RQ-0007")
        XCTAssertEqual(result.createdIDs, ["RQ-0101", "RQ-0102"])
        XCTAssertEqual(workspace.taskState.tasks.map(\.id), ["RQ-0007", "RQ-0101", "RQ-0102"])

        let log = try String(contentsOf: fixture.logURL, encoding: .utf8)
        XCTAssertTrue(log.contains("machine task decompose RQ-0007 --max-depth 3 --max-children 4 --max-nodes 25 --status draft --child-policy fail --write"))
        XCTAssertTrue(log.contains("machine queue read"))
    }

    private struct MockCLIFixture {
        let rootURL: URL
        let workspaceURL: URL
        let scriptURL: URL
        let logURL: URL
    }

    private static func makeMockCLIFixture() throws -> MockCLIFixture {
        let queueTasks = [
            RalphMockCLITestSupport.task(
                id: "RQ-0007",
                status: .todo,
                title: "Auth epic",
                priority: .high
            ),
            RalphMockCLITestSupport.task(
                id: "RQ-0101",
                status: .draft,
                title: "Prepare OAuth app",
                priority: .medium
            ),
            RalphMockCLITestSupport.task(
                id: "RQ-0102",
                status: .draft,
                title: "Wire callback flow",
                priority: .medium
            )
        ]
        let fixture = try RalphMockCLITestSupport.makeFixture(
            prefix: "ralph-workspace-decompose",
            workspaceName: "workspace",
            logFileName: "invocations.log",
            seedQueueTasks: queueTasks
        )
        let logURL = try XCTUnwrap(fixture.logURL)

        let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.configResolveDocument(
                workspaceURL: fixture.workspaceURL,
                agent: AgentConfig(model: "gpt-5.3-codex", phases: 2, iterations: 3)
            ),
            in: fixture.rootURL,
            name: "config-resolve.json"
        )
        let queueReadURL = try RalphMockCLITestSupport.writeJSONDocument(
            RalphMockCLITestSupport.queueReadDocument(
                workspaceURL: fixture.workspaceURL,
                activeTasks: queueTasks,
                nextRunnableTaskID: "RQ-0007"
            ),
            in: fixture.rootURL,
            name: "queue-read.json"
        )

        let previewURL = fixture.rootURL.appendingPathComponent("decompose-preview.json", isDirectory: false)
        try Self.previewJSON.write(to: previewURL, atomically: true, encoding: .utf8)
        let writeURL = fixture.rootURL.appendingPathComponent("decompose-write.json", isDirectory: false)
        try Self.writeJSON.write(to: writeURL, atomically: true, encoding: .utf8)

        let script = """
        #!/bin/sh
        set -eu
        printf '%s\n' "$*" >> "\(logURL.path)"
        if [ "$1" = "--version" ] || [ "$1" = "version" ]; then
          echo "ralph \(VersionCompatibility.minimumCLIVersion)"
          exit 0
        fi
        if [ "$1" = "--no-color" ]; then
          shift
        fi
        if [ "$1" = "machine" ] && [ "$2" = "config" ] && [ "$3" = "resolve" ]; then
          cat "\(configResolveURL.path)"
          exit 0
        fi
        if [ "$1" = "machine" ] && [ "$2" = "task" ] && [ "$3" = "decompose" ]; then
          if printf '%s\n' "$*" | grep -q -- '--write'; then
            cat "\(writeURL.path)"
          else
            cat "\(previewURL.path)"
          fi
          exit 0
        fi
        if [ "$1" = "machine" ] && [ "$2" = "queue" ] && [ "$3" = "read" ]; then
          cat "\(queueReadURL.path)"
          exit 0
        fi
        echo "unsupported command: $*" >&2
        exit 1
        """

        _ = try RalphMockCLITestSupport.makeExecutableScript(in: fixture.rootURL, name: fixture.scriptURL.lastPathComponent, body: script)
        return MockCLIFixture(
            rootURL: fixture.rootURL,
            workspaceURL: fixture.workspaceURL,
            scriptURL: fixture.scriptURL,
            logURL: logURL
        )
    }

    private static let previewJSON = """
    {
      "version": 1,
      "result": {
        "version": 1,
        "mode": "preview",
        "preview": {
          "source": {"kind": "freeform", "request": "Build OAuth login"},
          "attach_target": {
            "task": {"id":"RQ-0042","status":"todo","title":"Auth program","priority":"high","tags":[]},
            "has_existing_children": true
          },
          "plan": {
            "root": {
              "planner_key": "root",
              "title": "Build OAuth login",
              "description": "Plan auth integration",
              "plan": ["Inspect current auth entry points"],
              "tags": ["auth"],
              "scope": ["src/auth"],
              "depends_on_keys": [],
              "children": [
                {
                  "planner_key": "prepare-app",
                  "title": "Prepare OAuth app",
                  "description": null,
                  "plan": [],
                  "tags": [],
                  "scope": [],
                  "depends_on_keys": [],
                  "children": []
                },
                {
                  "planner_key": "wire-callback",
                  "title": "Wire callback flow",
                  "description": null,
                  "plan": [],
                  "tags": [],
                  "scope": [],
                  "depends_on_keys": ["prepare-app"],
                  "children": []
                }
              ]
            },
            "warnings": ["Tree capped at two leaves for fixture output."],
            "total_nodes": 3,
            "leaf_nodes": 2,
            "dependency_edges": [
              {"task_title": "Wire callback flow", "depends_on_title": "Prepare OAuth app"}
            ]
          },
          "write_blockers": [],
          "child_status": "todo",
          "child_policy": "append",
          "with_dependencies": true
        },
        "write": null
      }
    }
    """

    private static let writeJSON = """
    {
      "version": 1,
      "result": {
        "version": 1,
        "mode": "write",
        "preview": {
          "source": {"kind": "existing_task", "task": {"id":"RQ-0007","status":"todo","title":"Auth epic","priority":"high","tags":[]}},
          "attach_target": null,
          "plan": {
            "root": {
              "planner_key": "root",
              "title": "Auth epic",
              "description": null,
              "plan": [],
              "tags": [],
              "scope": [],
              "depends_on_keys": [],
              "children": []
            },
            "warnings": [],
            "total_nodes": 1,
            "leaf_nodes": 1,
            "dependency_edges": []
          },
          "write_blockers": [],
          "child_status": "draft",
          "child_policy": "fail",
          "with_dependencies": false
        },
        "write": {
          "root_task_id": null,
          "parent_task_id": "RQ-0007",
          "created_ids": ["RQ-0101", "RQ-0102"],
          "replaced_ids": [],
          "parent_annotated": true
        }
      }
    }
    """
}
