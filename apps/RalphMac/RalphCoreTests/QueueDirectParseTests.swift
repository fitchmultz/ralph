/**
 QueueDirectParseTests

 Responsibilities:
 - Validate direct queue file parsing for file-watcher-triggered refreshes.
 - Ensure RalphTaskQueueDocument correctly decodes valid queue files.
 - Verify both document format and legacy array format are supported.

 Does not handle:
 - Testing the CLI subprocess path (covered by integration tests).

 Invariants/assumptions callers must respect:
 - Tests use in-memory JSON data matching real queue file schemas.
*/

import Foundation
import XCTest

@testable import RalphCore

final class QueueDirectParseTests: XCTestCase {
    // MARK: - Document Format Tests

    func test_decode_validQueueDocument_succeeds() throws {
        // Given: Valid queue.json document format with tasks
        let json = #"""
        {
          "version": 1,
          "tasks": [
            {
              "id": "RQ-TEST-001",
              "status": "todo",
              "title": "Test task",
              "priority": "medium",
              "tags": ["test"],
              "created_at": "2026-02-14T10:00:00Z",
              "updated_at": "2026-02-14T10:00:00Z"
            }
          ]
        }
        """#

        // When: Direct parse is attempted
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let document = try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8))

        // Then: Tasks are parsed correctly
        XCTAssertEqual(document.version, 1)
        XCTAssertEqual(document.tasks.count, 1)
        XCTAssertEqual(document.tasks.first?.id, "RQ-TEST-001")
        XCTAssertEqual(document.tasks.first?.title, "Test task")
        XCTAssertEqual(document.tasks.first?.status, .todo)
    }

    func test_decode_emptyTasksArray_succeeds() throws {
        // Given: queue.json with empty tasks array
        let json = #"""
        {
          "version": 1,
          "tasks": []
        }
        """#

        // When: Direct parse is attempted
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let document = try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8))

        // Then: Empty array is parsed successfully
        XCTAssertEqual(document.version, 1)
        XCTAssertEqual(document.tasks.count, 0)
    }

    func test_decode_multipleTasks_succeeds() throws {
        // Given: Valid queue with multiple tasks
        let json = #"""
        {
          "version": 1,
          "tasks": [
            {
              "id": "RQ-001",
              "status": "todo",
              "title": "First task",
              "priority": "high",
              "tags": [],
              "created_at": "2026-02-14T10:00:00Z",
              "updated_at": "2026-02-14T10:00:00Z"
            },
            {
              "id": "RQ-002",
              "status": "doing",
              "title": "Second task",
              "priority": "medium",
              "tags": ["in-progress"],
              "created_at": "2026-02-14T11:00:00Z",
              "updated_at": "2026-02-14T12:00:00Z"
            },
            {
              "id": "RQ-003",
              "status": "done",
              "title": "Completed task",
              "priority": "low",
              "tags": [],
              "created_at": "2026-02-14T09:00:00Z",
              "updated_at": "2026-02-14T13:00:00Z"
            }
          ]
        }
        """#

        // When: Direct parse is attempted
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let document = try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8))

        // Then: All tasks are parsed with correct ordering
        XCTAssertEqual(document.tasks.count, 3)
        XCTAssertEqual(document.tasks[0].id, "RQ-001")
        XCTAssertEqual(document.tasks[1].status, .doing)
        XCTAssertEqual(document.tasks[2].status, .done)
    }

    // MARK: - Legacy Array Format Tests

    func test_decode_legacyArrayFormat_succeeds() throws {
        // Given: Legacy array format (without version wrapper)
        let json = #"""
        [
          {
            "id": "RQ-LEGACY-001",
            "status": "todo",
            "title": "Legacy format task",
            "priority": "high",
            "tags": ["legacy"],
            "created_at": "2026-02-14T10:00:00Z",
            "updated_at": "2026-02-14T10:00:00Z"
          }
        ]
        """#

        // When: Direct parse is attempted
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let document = try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8))

        // Then: RalphTaskQueueDocument decodes array format correctly
        XCTAssertEqual(document.version, 1) // Default version for legacy format
        XCTAssertEqual(document.tasks.count, 1)
        XCTAssertEqual(document.tasks.first?.id, "RQ-LEGACY-001")
    }

    // MARK: - Error Cases

    func test_decode_malformedJSON_throwsError() {
        // Given: queue.json with invalid JSON
        let json = "{ invalid json"

        // When/Then: Decoding throws an error
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        XCTAssertThrowsError(try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8)))
    }

    func test_decode_missingRequiredFields_throwsError() {
        // Given: Task missing required fields
        let json = #"""
        {
          "version": 1,
          "tasks": [
            {
              "id": "RQ-INCOMPLETE"
            }
          ]
        }
        """#

        // When/Then: Decoding throws an error
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        XCTAssertThrowsError(try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8)))
    }

    func test_decode_invalidStatus_throwsError() {
        // Given: Task with invalid status value
        let json = #"""
        {
          "version": 1,
          "tasks": [
            {
              "id": "RQ-BAD-STATUS",
              "status": "invalid_status",
              "title": "Bad status",
              "priority": "medium",
              "tags": [],
              "created_at": "2026-02-14T10:00:00Z",
              "updated_at": "2026-02-14T10:00:00Z"
            }
          ]
        }
        """#

        // When/Then: Decoding throws an error
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        XCTAssertThrowsError(try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8)))
    }

    // MARK: - Edge Cases

    func test_decode_withAllOptionalFields_succeeds() throws {
        // Given: Task with all optional fields populated
        let json = #"""
        {
          "version": 1,
          "tasks": [
            {
              "id": "RQ-FULL",
              "status": "todo",
              "title": "Full task",
              "description": "Detailed description",
              "priority": "high",
              "tags": ["tag1", "tag2"],
              "scope": ["file1.swift", "file2.swift"],
              "evidence": ["evidence1"],
              "plan": ["step1", "step2"],
              "notes": ["note1"],
              "request": "scan: none",
              "created_at": "2026-02-14T10:00:00Z",
              "updated_at": "2026-02-14T11:00:00Z",
              "depends_on": ["RQ-001"],
              "blocks": ["RQ-003"],
              "relates_to": ["RQ-002"],
              "custom_fields": {
                "extra": "value"
              }
            }
          ]
        }
        """#

        // When: Direct parse is attempted
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let document = try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8))

        // Then: All fields are parsed correctly
        XCTAssertEqual(document.tasks.count, 1)
        let task = document.tasks.first!
        XCTAssertEqual(task.id, "RQ-FULL")
        XCTAssertEqual(task.description, "Detailed description")
        XCTAssertEqual(task.tags, ["tag1", "tag2"])
        XCTAssertEqual(task.scope, ["file1.swift", "file2.swift"])
        XCTAssertEqual(task.dependsOn, ["RQ-001"])
        XCTAssertEqual(task.blocks, ["RQ-003"])
    }

    func test_decode_withUnknownFields_succeeds() throws {
        // Given: Task with unknown fields (forward compatibility)
        let json = #"""
        {
          "version": 1,
          "tasks": [
            {
              "id": "RQ-FUTURE",
              "status": "todo",
              "title": "Future task",
              "priority": "medium",
              "tags": [],
              "created_at": "2026-02-14T10:00:00Z",
              "updated_at": "2026-02-14T10:00:00Z",
              "future_field": "unknown value",
              "another_new_field": 123
            }
          ]
        }
        """#

        // When: Direct parse is attempted
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let document = try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8))

        // Then: Unknown fields are ignored, known fields parsed correctly
        XCTAssertEqual(document.tasks.count, 1)
        XCTAssertEqual(document.tasks.first?.id, "RQ-FUTURE")
    }

    func test_decode_dateFormat_iso8601() throws {
        // Given: Task with ISO8601 dates
        let json = #"""
        {
          "version": 1,
          "tasks": [
            {
              "id": "RQ-DATE",
              "status": "todo",
              "title": "Date test",
              "priority": "medium",
              "tags": [],
              "created_at": "2026-02-14T10:30:45Z",
              "updated_at": "2026-02-14T15:20:10Z"
            }
          ]
        }
        """#

        // When: Direct parse is attempted
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let document = try decoder.decode(RalphTaskQueueDocument.self, from: Data(json.utf8))

        // Then: Dates are parsed correctly
        let task = document.tasks.first!
        XCTAssertNotNil(task.createdAt)
        XCTAssertNotNil(task.updatedAt)

        // Verify dates are reasonable (year and month should be consistent regardless of timezone)
        let calendar = Calendar(identifier: .gregorian)
        let createdComponents = calendar.dateComponents(
            [.year, .month],
            from: task.createdAt!
        )
        XCTAssertEqual(createdComponents.year, 2026)
        XCTAssertEqual(createdComponents.month, 2)

        // Verify updated at is after created at
        XCTAssertGreaterThan(task.updatedAt!, task.createdAt!)
    }

    func test_queueRefreshEvent_tracksAddedAndChangedTaskIDs() {
        let previousTasks = [
            RalphTask(
                id: "RQ-001",
                status: .todo,
                title: "Original title",
                priority: .medium,
                createdAt: Date(),
                updatedAt: Date()
            )
        ]
        let currentTasks = [
            RalphTask(
                id: "RQ-001",
                status: .doing,
                title: "Original title",
                priority: .medium,
                createdAt: Date(),
                updatedAt: Date()
            ),
            RalphTask(
                id: "RQ-002",
                status: .todo,
                title: "New task",
                priority: .high,
                createdAt: Date(),
                updatedAt: Date()
            ),
        ]

        let event = Workspace.QueueRefreshEvent(
            source: .externalFileChange,
            previousTasks: previousTasks,
            currentTasks: currentTasks
        )

        XCTAssertEqual(event.highlightedTaskIDs, Set(["RQ-001", "RQ-002"]))
        XCTAssertEqual(event.previousTasks, previousTasks)
        XCTAssertEqual(event.currentTasks, currentTasks)
    }
}
