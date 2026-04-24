/**
 WorkspaceTaskMutationAgentTests

 Purpose:
 - Validate task-mutation payload generation for shared field encoders and agent override edits.

 Responsibilities:
 - Validate task-mutation payload generation for shared field encoders and agent override edits.
 - Cover multi-field diffs plus add, clear, and semantic-noop override scenarios.

 Does not handle:
 - Run-control streaming or runner-configuration refresh.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Mock CLIs log argv and payload bodies so assertions inspect serialized mutation requests.
 */

import XCTest

@testable import RalphCore

@MainActor
final class WorkspaceTaskMutationAgentTests: WorkspacePerformanceTestCase {
  func test_updateTask_agentOverride_emitsAgentEditCommand() async throws {
    let fixture = try Self.makeMutationFixture(
      prefix: "ralph-workspace-agent-edit",
      scriptName: "mock-ralph-task-mutate-agent",
      mutationReportJSON:
        #"{"version":2,"blocking":null,"report":{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-9001","applied_edits":1}]},"continuation":{"headline":"Task mutation has been applied.","detail":"Ralph wrote 1 task mutation(s) atomically and created an undo checkpoint first.","blocking":null,"next_steps":[{"title":"Continue work","command":"ralph machine run one --resume","detail":"Proceed from the updated task state."}]}}"#
    )
    var workspace: Workspace!
    defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
    workspace = Workspace(
      workingDirectoryURL: fixture.workspaceURL,
      client: try RalphCLIClient(executableURL: fixture.scriptURL)
    )

    let original = RalphTask(
      id: "RQ-9001",
      status: .todo,
      title: "Task",
      priority: .medium
    )
    var updated = original
    updated.agent = RalphTaskAgent(
      runner: "codex",
      model: "gpt-5.3-codex",
      modelEffort: "high",
      phases: 2,
      iterations: 1,
      phaseOverrides: RalphTaskPhaseOverrides(
        phase2: RalphTaskPhaseOverride(
          runner: "kimi",
          model: "kimi-code/kimi-for-coding",
          reasoningEffort: nil
        )
      )
    )

    try await workspace.updateTask(from: original, to: updated)

    let log = try String(contentsOf: fixture.logURL, encoding: .utf8)
    let lines = log.split(separator: "\n").map(String.init)
    let payloadLine = lines.first { $0.contains("\"task_id\" : \"RQ-9001\"") }

    XCTAssertNotNil(payloadLine)
    XCTAssertTrue(log.contains("\"field\" : \"agent\""))
    XCTAssertTrue(log.contains("\\\"runner\\\":\\\"codex\\\""))
    XCTAssertTrue(log.contains("\\\"model\\\":\\\"gpt-5.3-codex\\\""))
    XCTAssertTrue(log.contains("\\\"model_effort\\\":\\\"high\\\""))
    XCTAssertTrue(log.contains("\\\"phases\\\":2"))
    XCTAssertTrue(log.contains("\\\"iterations\\\":1"))
    XCTAssertTrue(log.contains("\\\"phase_overrides\\\":{\\\"phase2\\\""))
    XCTAssertTrue(lines.contains { $0.contains("<--no-color><machine><queue><read>") })
  }

  func test_updateTask_multipleFieldChanges_emitSharedEncodedEdits() async throws {
    let fixture = try Self.makeMutationFixture(
      prefix: "ralph-workspace-multi-edit",
      scriptName: "mock-ralph-task-mutate-multi-edit",
      mutationReportJSON:
        #"{"version":2,"blocking":null,"report":{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-9004","applied_edits":13}]},"continuation":{"headline":"Task mutation has been applied.","detail":"Ralph wrote 1 task mutation(s) atomically and created an undo checkpoint first.","blocking":null,"next_steps":[{"title":"Continue work","command":"ralph machine run one --resume","detail":"Proceed from the updated task state."}]}}"#
    )
    var workspace: Workspace!
    defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
    workspace = Workspace(
      workingDirectoryURL: fixture.workspaceURL,
      client: try RalphCLIClient(executableURL: fixture.scriptURL)
    )

    let updatedAt = Date(timeIntervalSince1970: 1_700_000_000)
    let original = RalphTask(
      id: "RQ-9004",
      status: .todo,
      title: "Original Task",
      description: "Brief description",
      priority: .medium,
      tags: ["macos"],
      updatedAt: updatedAt,
      dependsOn: ["RQ-1000"]
    )
    var updated = original
    updated.title = "Updated Task"
    updated.description = "Expanded description"
    updated.status = .doing
    updated.priority = .high
    updated.tags = ["macos", "ux"]
    updated.scope = ["apps/RalphMac", "RalphCore"]
    updated.evidence = ["screenshot.png", "console.log"]
    updated.plan = ["Audit payload", "Verify diff"]
    updated.notes = ["First note", "Second note"]
    updated.dependsOn = ["RQ-1000", "RQ-1001"]
    updated.blocks = ["RQ-1002"]
    updated.relatesTo = ["RQ-1003", "RQ-1004"]
    updated.request = "Build a polished native task editor"
    updated.duplicates = "RQ-1999"
    updated.customFields = ["severity": "high", "source": "app"]
    updated.scheduledStart = Date(timeIntervalSince1970: 1_700_003_600)
    updated.estimatedMinutes = 45
    updated.actualMinutes = 50
    updated.agent = RalphTaskAgent(
      runner: "codex",
      model: "gpt-5.3-codex",
      phases: 2
    )

    try await workspace.updateTask(from: original, to: updated)

    let request = try Self.loggedMutationRequest(at: fixture.logURL)
    XCTAssertTrue(request.atomic)
    XCTAssertEqual(request.tasks.count, 1)

    let mutation = try XCTUnwrap(request.tasks.first)
    XCTAssertEqual(mutation.taskID, "RQ-9004")
    XCTAssertEqual(mutation.expectedUpdatedAt, ISO8601DateFormatter().string(from: updatedAt))
    XCTAssertEqual(
      mutation.edits.filter { $0.field != "agent" }.map { "\($0.field)=\($0.value)" },
      [
        "title=Updated Task",
        "description=Expanded description",
        "status=doing",
        "priority=high",
        "tags=macos, ux",
        "scope=apps/RalphMac\nRalphCore",
        "evidence=screenshot.png\nconsole.log",
        "plan=Audit payload\nVerify diff",
        "notes=First note\nSecond note",
        "request=Build a polished native task editor",
        "depends_on=RQ-1000, RQ-1001",
        "blocks=RQ-1002",
        "relates_to=RQ-1003, RQ-1004",
        "duplicates=RQ-1999",
        "custom_fields=severity=high\nsource=app",
        "scheduled_start=2023-11-14T23:13:20Z",
        "estimated_minutes=45",
        "actual_minutes=50",
      ]
    )

    let agentEdit = try XCTUnwrap(mutation.edits.first { $0.field == "agent" })
    let decodedAgent = try JSONDecoder().decode(
      RalphTaskAgent.self, from: Data(agentEdit.value.utf8))
    XCTAssertEqual(decodedAgent, RalphTaskAgent.normalizedOverride(updated.agent))
  }

  func test_updateTask_clearingAgentOverride_emitsEmptyAgentValue() async throws {
    let fixture = try Self.makeMutationFixture(
      prefix: "ralph-workspace-agent-clear",
      scriptName: "mock-ralph-task-mutate-agent-clear",
      mutationReportJSON:
        #"{"version":2,"blocking":null,"report":{"version":1,"atomic":true,"tasks":[{"task_id":"RQ-9002","applied_edits":1}]},"continuation":{"headline":"Task mutation has been applied.","detail":"Ralph wrote 1 task mutation(s) atomically and created an undo checkpoint first.","blocking":null,"next_steps":[{"title":"Continue work","command":"ralph machine run one --resume","detail":"Proceed from the updated task state."}]}}"#
    )
    var workspace: Workspace!
    defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
    workspace = Workspace(
      workingDirectoryURL: fixture.workspaceURL,
      client: try RalphCLIClient(executableURL: fixture.scriptURL)
    )

    let original = RalphTask(
      id: "RQ-9002",
      status: .todo,
      title: "Task",
      priority: .medium,
      agent: RalphTaskAgent(
        runner: "codex",
        model: "gpt-5.3-codex",
        phases: 2
      )
    )
    var updated = original
    updated.agent = nil

    try await workspace.updateTask(from: original, to: updated)

    let log = try String(contentsOf: fixture.logURL, encoding: .utf8)
    XCTAssertTrue(log.contains("\"task_id\" : \"RQ-9002\""))
    XCTAssertTrue(log.contains("\"field\" : \"agent\""))
    XCTAssertTrue(log.contains("\"value\" : \"\""))
  }

  func test_updateTask_semanticallyEmptyAgentOverride_doesNotEmitAgentEdit() async throws {
    let fixture = try Self.makeMutationFixture(
      prefix: "ralph-workspace-agent-noop",
      scriptName: "mock-ralph-task-mutate-agent-noop",
      mutationReportJSON:
        #"{"version":2,"blocking":null,"report":{"version":1,"atomic":true,"tasks":[]},"continuation":{"headline":"Task mutation has been applied.","detail":"Ralph wrote 0 task mutation(s) atomically and created an undo checkpoint first.","blocking":null,"next_steps":[{"title":"Continue work","command":"ralph machine run one --resume","detail":"Proceed from the updated task state."}]}}"#
    )
    var workspace: Workspace!
    defer { RalphCoreTestSupport.shutdownAndRemove(fixture.rootURL, workspace) }
    workspace = Workspace(
      workingDirectoryURL: fixture.workspaceURL,
      client: try RalphCLIClient(executableURL: fixture.scriptURL)
    )

    let original = RalphTask(
      id: "RQ-9003",
      status: .todo,
      title: "Task",
      priority: .medium
    )
    var updated = original
    updated.agent = RalphTaskAgent(
      runner: "   ",
      model: "  ",
      modelEffort: "default",
      phases: 8,
      iterations: 0
    )

    try await workspace.updateTask(from: original, to: updated)

    if FileManager.default.fileExists(atPath: fixture.logURL.path) {
      let log = try String(contentsOf: fixture.logURL, encoding: .utf8)
      let lines = log.split(separator: "\n").map(String.init)
      XCTAssertFalse(
        lines.contains { $0.contains("<--no-color><machine><task><mutate><--input><") })
      XCTAssertFalse(log.contains("\"field\" : \"agent\""))
    }
  }

  private struct MutationFixture {
    let rootURL: URL
    let workspaceURL: URL
    let scriptURL: URL
    let logURL: URL
  }

  private static func loggedMutationRequest(at logURL: URL) throws -> WorkspaceTaskMutationRequest {
    let log = try String(contentsOf: logURL, encoding: .utf8)
    let start = try XCTUnwrap(log.firstIndex(of: "{"))

    var depth = 0
    var end: String.Index?
    for index in log[start...].indices {
      switch log[index] {
      case "{":
        depth += 1
      case "}":
        depth -= 1
        if depth == 0 {
          end = index
          break
        }
      default:
        break
      }
    }

    let payloadEnd = try XCTUnwrap(end)
    let payload = String(log[start...payloadEnd])
    return try JSONDecoder().decode(WorkspaceTaskMutationRequest.self, from: Data(payload.utf8))
  }

  private static func makeMutationFixture(
    prefix: String,
    scriptName: String,
    mutationReportJSON: String
  ) throws -> MutationFixture {
    let fixture = try RalphMockCLITestSupport.makeFixture(
      prefix: prefix,
      scriptName: scriptName,
      logFileName: "commands.log",
      seedQueueTasks: []
    )
    let logURL = try XCTUnwrap(fixture.logURL)

    let configResolveURL = try RalphMockCLITestSupport.writeJSONDocument(
      RalphMockCLITestSupport.configResolveDocument(
        workspaceURL: fixture.workspaceURL,
        agent: AgentConfig(model: "codex", phases: 2, iterations: 1)
      ),
      in: fixture.rootURL,
      name: "config-resolve.json"
    )
    let queueReadURL = try RalphMockCLITestSupport.writeJSONDocument(
      RalphMockCLITestSupport.queueReadDocument(
        workspaceURL: fixture.workspaceURL,
        activeTasks: []
      ),
      in: fixture.rootURL,
      name: "queue-read.json"
    )
    let mutationReportURL = fixture.rootURL.appendingPathComponent(
      "mutation-report.json", isDirectory: false)
    try mutationReportJSON.write(to: mutationReportURL, atomically: true, encoding: .utf8)

    let script = """
      #!/bin/sh
      log_file="\(logURL.path)"

      case "$*" in
      *"--no-color machine config resolve"*)
        for arg in "$@"; do
          printf '<%s>' "$arg" >> "$log_file"
        done
        printf '\n' >> "$log_file"
        cat "\(configResolveURL.path)"
        exit 0
        ;;
      *"--no-color machine queue read"*)
        for arg in "$@"; do
          printf '<%s>' "$arg" >> "$log_file"
        done
        printf '\n' >> "$log_file"
        cat "\(queueReadURL.path)"
        exit 0
        ;;
      *"--no-color machine task mutate --input "*)
        for arg in "$@"; do
          printf '<%s>' "$arg" >> "$log_file"
        done
        printf '\n' >> "$log_file"
        cat "$6" >> "$log_file"
        printf '\n' >> "$log_file"
        cat "\(mutationReportURL.path)"
        exit 0
        ;;
      esac

      echo "unexpected args: $*" >&2
      exit 64
      """

    _ = try RalphMockCLITestSupport.makeExecutableScript(
      in: fixture.rootURL, name: scriptName, body: script)
    return MutationFixture(
      rootURL: fixture.rootURL,
      workspaceURL: fixture.workspaceURL,
      scriptURL: fixture.scriptURL,
      logURL: logURL
    )
  }
}
