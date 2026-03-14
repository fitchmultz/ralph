/**
 ConflictDetectionTests

 Responsibilities:
 - Validate conflict detection logic for external task changes.
 - Ensure checkForConflict and detectConflictedFields work correctly.
 - Test optimistic locking behavior with updatedAt timestamps.

 Does not handle:
 - UI-level conflict resolution (see RalphMacUITests).
 - File watching integration (tested separately).

 Invariants/assumptions callers must respect:
 - Tests run on the main actor.
 - Uses synthetic RalphTask data; actual file structure not required.
 */

import Foundation
import XCTest
@testable import RalphCore

@MainActor
final class ConflictDetectionTests: RalphCoreTestCase {
    var workspace: Workspace!
    
    override func setUp() async throws {
        try await super.setUp()
        workspace = Workspace(workingDirectoryURL: RalphCoreTestSupport.workspaceURL(label: "conflict-detection"))
    }
    
    override func tearDown() async throws {
        workspace = nil
        try await super.tearDown()
    }
    
    // MARK: - checkForConflict Tests
    
    func testCheckForConflict_NoConflict_SameTimestamp() {
        let timestamp = Date()
        let task = RalphTask(
            id: "RQ-TEST-001",
            status: .todo,
            title: "Test Task",
            priority: .medium,
            updatedAt: timestamp
        )
        
        // Set workspace tasks
        workspace.taskState.tasks = [task]
        
        // Check with same timestamp - should return nil (no conflict)
        let conflict = workspace.checkForConflict(taskID: "RQ-TEST-001", originalUpdatedAt: timestamp)
        XCTAssertNil(conflict)
    }
    
    func testCheckForConflict_Conflict_DifferentTimestamp() {
        let originalTimestamp = Date(timeIntervalSince1970: 1000)
        let newTimestamp = Date(timeIntervalSince1970: 2000)
        
        let task = RalphTask(
            id: "RQ-TEST-002",
            status: .todo,
            title: "Modified Task",
            priority: .high,
            updatedAt: newTimestamp
        )
        
        workspace.taskState.tasks = [task]
        
        // Check with old timestamp - should return the current task (conflict detected)
        let conflict = workspace.checkForConflict(taskID: "RQ-TEST-002", originalUpdatedAt: originalTimestamp)
        XCTAssertNotNil(conflict)
        XCTAssertEqual(conflict?.title, "Modified Task")
    }
    
    func testCheckForConflict_TaskDeleted() {
        let timestamp = Date()
        workspace.taskState.tasks = [] // Empty - task was deleted
        
        let conflict = workspace.checkForConflict(taskID: "RQ-TEST-003", originalUpdatedAt: timestamp)
        XCTAssertNil(conflict) // Returns nil when task is deleted
    }
    
    func testCheckForConflict_NoOriginalTimestamp() {
        let task = RalphTask(
            id: "RQ-TEST-004",
            status: .todo,
            title: "Test",
            priority: .medium,
            updatedAt: Date()
        )
        
        workspace.taskState.tasks = [task]
        
        // Check with nil timestamp - should return nil (can't detect conflict)
        let conflict = workspace.checkForConflict(taskID: "RQ-TEST-004", originalUpdatedAt: nil)
        XCTAssertNil(conflict)
    }
    
    func testCheckForConflict_ExternalTaskHasNoTimestamp() {
        let originalTimestamp = Date(timeIntervalSince1970: 1000)
        
        let task = RalphTask(
            id: "RQ-TEST-005",
            status: .todo,
            title: "Test",
            priority: .medium,
            updatedAt: nil // No timestamp on external task
        )
        
        workspace.taskState.tasks = [task]
        
        // Check with original timestamp but external has none - should return nil
        let conflict = workspace.checkForConflict(taskID: "RQ-TEST-005", originalUpdatedAt: originalTimestamp)
        XCTAssertNil(conflict)
    }
    
    // MARK: - detectConflictedFields Tests
    
    func testDetectConflictedFields_NoDifferences() {
        let timestamp = Date()
        let task1 = RalphTask(
            id: "RQ-TEST-006",
            status: .doing,
            title: "Same Task",
            description: "Same description",
            priority: .high,
            tags: ["swift", "ui"],
            updatedAt: timestamp
        )
        
        let task2 = RalphTask(
            id: "RQ-TEST-006",
            status: .doing,
            title: "Same Task",
            description: "Same description",
            priority: .high,
            tags: ["swift", "ui"],
            updatedAt: timestamp
        )
        
        let fields = workspace.detectConflictedFields(local: task1, external: task2)
        XCTAssertTrue(fields.isEmpty)
    }
    
    func testDetectConflictedFields_MultipleDifferences() {
        let local = RalphTask(
            id: "RQ-TEST-007",
            status: .doing,
            title: "Local Title",
            description: "Local Desc",
            priority: .high,
            tags: ["swift"],
            scope: ["file1.swift"],
            updatedAt: Date()
        )
        
        let external = RalphTask(
            id: "RQ-TEST-007",
            status: .todo,
            title: "External Title",
            description: "Local Desc", // Same
            priority: .high, // Same
            tags: ["rust"],
            scope: ["file1.swift"], // Same
            updatedAt: Date()
        )
        
        let fields = workspace.detectConflictedFields(local: local, external: external)
        
        XCTAssertTrue(fields.contains("title"))
        XCTAssertTrue(fields.contains("status"))
        XCTAssertTrue(fields.contains("tags"))
        XCTAssertFalse(fields.contains("description"))
        XCTAssertFalse(fields.contains("priority"))
        XCTAssertFalse(fields.contains("scope"))
    }
    
    func testDetectConflictedFields_AllFields() {
        let local = RalphTask(
            id: "RQ-TEST-008",
            status: .doing,
            title: "Local",
            description: "Local",
            priority: .critical,
            tags: ["a"],
            scope: ["a"],
            evidence: ["a"],
            plan: ["a"],
            notes: ["a"],
            dependsOn: ["RQ-DEP-1"],
            blocks: ["RQ-BLK-1"],
            relatesTo: ["RQ-REL-1"],
            updatedAt: Date()
        )
        
        let external = RalphTask(
            id: "RQ-TEST-008",
            status: .todo,
            title: "External",
            description: "External",
            priority: .low,
            tags: ["b"],
            scope: ["b"],
            evidence: ["b"],
            plan: ["b"],
            notes: ["b"],
            dependsOn: ["RQ-DEP-2"],
            blocks: ["RQ-BLK-2"],
            relatesTo: ["RQ-REL-2"],
            updatedAt: Date()
        )
        
        let fields = workspace.detectConflictedFields(local: local, external: external)
        
        XCTAssertEqual(fields.count, 12)
        XCTAssertTrue(fields.contains("title"))
        XCTAssertTrue(fields.contains("description"))
        XCTAssertTrue(fields.contains("status"))
        XCTAssertTrue(fields.contains("priority"))
        XCTAssertTrue(fields.contains("tags"))
        XCTAssertTrue(fields.contains("scope"))
        XCTAssertTrue(fields.contains("evidence"))
        XCTAssertTrue(fields.contains("plan"))
        XCTAssertTrue(fields.contains("notes"))
        XCTAssertTrue(fields.contains("dependsOn"))
        XCTAssertTrue(fields.contains("blocks"))
        XCTAssertTrue(fields.contains("relatesTo"))
    }
    
    func testDetectConflictedFields_NilFields() {
        let local = RalphTask(
            id: "RQ-TEST-009",
            status: .todo,
            title: "Test",
            description: nil,
            priority: .medium,
            scope: nil,
            updatedAt: Date()
        )
        
        let external = RalphTask(
            id: "RQ-TEST-009",
            status: .todo,
            title: "Test",
            description: "Has description",
            priority: .medium,
            scope: ["file.swift"],
            updatedAt: Date()
        )
        
        let fields = workspace.detectConflictedFields(local: local, external: external)
        
        XCTAssertTrue(fields.contains("description"))
        XCTAssertTrue(fields.contains("scope"))
    }

    func testDetectConflictedFields_AgentOverrides() {
        let local = RalphTask(
            id: "RQ-TEST-009A",
            status: .todo,
            title: "Test",
            priority: .medium,
            agent: RalphTaskAgent(
                runner: "codex",
                model: "gpt-5.3-codex",
                phases: 2,
                iterations: 1
            ),
            updatedAt: Date()
        )

        let external = RalphTask(
            id: "RQ-TEST-009A",
            status: .todo,
            title: "Test",
            priority: .medium,
            agent: RalphTaskAgent(
                runner: "kimi",
                model: "kimi-code/kimi-for-coding",
                phases: 3,
                iterations: 1
            ),
            updatedAt: Date()
        )

        let fields = workspace.detectConflictedFields(local: local, external: external)
        XCTAssertTrue(fields.contains("agent"))
    }
    
    // MARK: - TaskConflict Struct Tests
    
    func testTaskConflictStruct() {
        let local = RalphTask(
            id: "RQ-TEST-010",
            status: .todo,
            title: "Local",
            priority: .medium,
            updatedAt: Date()
        )
        
        let external = RalphTask(
            id: "RQ-TEST-010",
            status: .doing,
            title: "External",
            priority: .high,
            updatedAt: Date()
        )
        
        let conflict = TaskConflict(
            localTask: local,
            externalTask: external,
            conflictedFields: ["title", "status", "priority"]
        )
        
        XCTAssertEqual(conflict.localTask.title, "Local")
        XCTAssertEqual(conflict.externalTask.title, "External")
        XCTAssertEqual(conflict.conflictedFields, ["title", "status", "priority"])
    }
    
    // MARK: - Integration Tests
    
    func testConflictDetectionFlow() {
        // Simulate initial task state
        let originalTimestamp = Date()
        let task = RalphTask(
            id: "RQ-TEST-011",
            status: .todo,
            title: "Original Title",
            priority: .medium,
            updatedAt: originalTimestamp
        )
        
        workspace.tasks = [task]
        
        // Verify no conflict at start
        let initialConflict = workspace.checkForConflict(taskID: "RQ-TEST-011", originalUpdatedAt: originalTimestamp)
        XCTAssertNil(initialConflict)
        
        // Simulate external modification (newer timestamp)
        let newTimestamp = Date().addingTimeInterval(300)
        let modifiedTask = RalphTask(
            id: "RQ-TEST-011",
            status: .doing,
            title: "Modified Title",
            priority: .high,
            updatedAt: newTimestamp
        )
        
        workspace.tasks = [modifiedTask]
        
        // Verify conflict detected with original timestamp
        let detectedConflict = workspace.checkForConflict(taskID: "RQ-TEST-011", originalUpdatedAt: originalTimestamp)
        XCTAssertNotNil(detectedConflict)
        XCTAssertEqual(detectedConflict?.title, "Modified Title")
        XCTAssertEqual(detectedConflict?.status, .doing)
        
        // Verify no conflict when using new timestamp
        let noConflict = workspace.checkForConflict(taskID: "RQ-TEST-011", originalUpdatedAt: newTimestamp)
        XCTAssertNil(noConflict)
    }
}
