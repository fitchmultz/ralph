//! Workspace+ConflictDetection
//!
//! Responsibilities:
//! - Diff task snapshots to identify added, removed, and changed tasks.
//! - Detect optimistic-locking conflicts from updated-at timestamps.
//! - Compute field-level conflict details for merge and review flows.
//!
//! Does not handle:
//! - Applying task mutations.
//! - Queue file watching or notifications.
//! - Task presentation or sorting.
//!
//! Invariants/assumptions callers must respect:
//! - Task IDs are unique within a snapshot.
//! - Timestamp conflict checks are best-effort and require the original timestamp.
//! - Field diffing compares app-visible task fields only.

public import Foundation

public extension Workspace {
    struct TaskConflict: Sendable {
        public let localTask: RalphTask
        public let externalTask: RalphTask
        public let conflictedFields: [String]

        public init(localTask: RalphTask, externalTask: RalphTask, conflictedFields: [String]) {
            self.localTask = localTask
            self.externalTask = externalTask
            self.conflictedFields = conflictedFields
        }
    }

    struct TaskChanges: Sendable {
        public let added: [RalphTask]
        public let removed: [RalphTask]
        public let changed: [RalphTask]

        public init(added: [RalphTask], removed: [RalphTask], changed: [RalphTask]) {
            self.added = added
            self.removed = removed
            self.changed = changed
        }

        public var hasChanges: Bool {
            !added.isEmpty || !removed.isEmpty || !changed.isEmpty
        }

        public static func diff(previous: [RalphTask], current: [RalphTask]) -> Self {
            let previousIDs = Set(previous.map(\.id))
            let currentIDs = Set(current.map(\.id))

            let added = current.filter { !previousIDs.contains($0.id) }
            let removed = previous.filter { !currentIDs.contains($0.id) }
            let previousByID = Dictionary(uniqueKeysWithValues: previous.map { ($0.id, $0) })

            var changed: [RalphTask] = []
            changed.reserveCapacity(current.count)

            for task in current {
                guard let previousTask = previousByID[task.id] else { continue }
                if task.status != previousTask.status ||
                    task.title != previousTask.title ||
                    task.priority != previousTask.priority ||
                    task.tags != previousTask.tags ||
                    task.agent != previousTask.agent {
                    changed.append(task)
                }
            }

            return TaskChanges(added: added, removed: removed, changed: changed)
        }
    }

    func detectTaskChanges(previous: [RalphTask], current: [RalphTask]) -> TaskChanges {
        TaskChanges.diff(previous: previous, current: current)
    }

    func checkForConflict(taskID: String, originalUpdatedAt: Date?) -> RalphTask? {
        guard let currentTask = taskState.tasks.first(where: { $0.id == taskID }) else {
            return nil
        }

        guard let originalUpdatedAt else {
            return nil
        }

        if let currentUpdatedAt = currentTask.updatedAt, currentUpdatedAt != originalUpdatedAt {
            return currentTask
        }

        return nil
    }

    func detectConflictedFields(local: RalphTask, external: RalphTask) -> [String] {
        TaskConflictField.allCases
            .filter { $0.differs(local: local, external: external) }
            .map(\.rawValue)
    }
}

public typealias TaskConflict = Workspace.TaskConflict
public typealias TaskChanges = Workspace.TaskChanges
