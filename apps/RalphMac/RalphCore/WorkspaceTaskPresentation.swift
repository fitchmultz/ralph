//! WorkspaceTaskPresentation
//!
//! Responsibilities:
//! - Build filtered and sorted task snapshots for workspace views.
//! - Provide deterministic, strict task ordering for both ascending and descending sorts.
//! - Pre-group visible tasks by status so kanban/list surfaces can share one projection.
//!
//! Does not handle:
//! - Mutating queue files or task state.
//! - UI rendering or SwiftUI selection state.
//! - Runner execution and analytics loading.
//!
//! Invariants/assumptions callers must respect:
//! - The snapshot is derived from the workspace's current task/filter/sort state.
//! - Ordering is deterministic even when primary sort keys tie.
//! - Grouped tasks preserve the same order as the flat filtered list.

import Foundation

public struct WorkspaceTaskPresentation: Sendable {
    public let tasks: [RalphTask]
    public let orderedTaskIDs: [String]
    public let tasksByStatus: [RalphTaskStatus: [RalphTask]]

    public init(tasks: [RalphTask]) {
        self.tasks = tasks
        self.orderedTaskIDs = tasks.map(\.id)
        self.tasksByStatus = Dictionary(grouping: tasks, by: \.status)
    }
}

public extension Workspace {
    func taskPresentation() -> WorkspaceTaskPresentation {
        var result = taskState.tasks

        let filterText = taskState.taskFilterText.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines)
        if !filterText.isEmpty {
            result = result.filter { task in
                let matchesTitle = task.title.localizedCaseInsensitiveContains(filterText)
                let matchesDescription = task.description?.localizedCaseInsensitiveContains(filterText) ?? false
                let matchesTags = task.tags.contains { $0.localizedCaseInsensitiveContains(filterText) }
                return matchesTitle || matchesDescription || matchesTags
            }
        }

        if let statusFilter = taskState.taskStatusFilter {
            result = result.filter { $0.status == statusFilter }
        }

        if let priorityFilter = taskState.taskPriorityFilter {
            result = result.filter { $0.priority == priorityFilter }
        }

        if let tagFilter = taskState.taskTagFilter, !tagFilter.isEmpty {
            result = result.filter { $0.tags.contains(tagFilter) }
        }

        result.sort { lhs, rhs in
            let forward = self.compareTasks(lhs, rhs)
            if self.taskState.taskSortAscending {
                return forward == .orderedAscending
            }
            return forward == .orderedDescending
        }
        return WorkspaceTaskPresentation(tasks: result)
    }

    func filteredAndSortedTasks() -> [RalphTask] {
        taskPresentation().tasks
    }

    private func compareTasks(_ lhs: RalphTask, _ rhs: RalphTask) -> ComparisonResult {
        let primaryResult: ComparisonResult = switch taskState.taskSortBy {
        case .priority:
            compare(lhs.priority.sortOrder, rhs.priority.sortOrder)
        case .created:
            compare(lhs.createdAt ?? .distantPast, rhs.createdAt ?? .distantPast)
        case .updated:
            compare(lhs.updatedAt ?? .distantPast, rhs.updatedAt ?? .distantPast)
        case .status:
            compare(lhs.status.rawValue, rhs.status.rawValue)
        case .title:
            lhs.title.localizedStandardCompare(rhs.title)
        }

        if primaryResult != .orderedSame {
            return primaryResult
        }

        let statusResult = compare(lhs.status.rawValue, rhs.status.rawValue)
        if statusResult != .orderedSame {
            return statusResult
        }

        let priorityResult = compare(lhs.priority.sortOrder, rhs.priority.sortOrder)
        if priorityResult != .orderedSame {
            return priorityResult
        }

        let updatedResult = compare(lhs.updatedAt ?? .distantPast, rhs.updatedAt ?? .distantPast)
        if updatedResult != .orderedSame {
            return updatedResult
        }

        let createdResult = compare(lhs.createdAt ?? .distantPast, rhs.createdAt ?? .distantPast)
        if createdResult != .orderedSame {
            return createdResult
        }

        return lhs.id.localizedStandardCompare(rhs.id)
    }
}

private func compare<T: Comparable>(_ lhs: T, _ rhs: T) -> ComparisonResult {
    if lhs < rhs {
        return .orderedAscending
    }
    if lhs > rhs {
        return .orderedDescending
    }
    return .orderedSame
}
