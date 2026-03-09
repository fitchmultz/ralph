//!
//! TaskListSupportViews
//!
//! Purpose:
//! - Hold filter controls, banners, cards, badges, and palette helpers for task-list rendering.
//!
//! Responsibilities:
//! - Provide task-list specific controls and reusable visual helpers.
//!
//! Scope:
//! - Shared task-list presentation only.
//!
//! Usage:
//! - Consumed by `TaskListView` and `TaskListContent`.
//!
//! Invariants/Assumptions:
//! - Filtering and sorting mutate `taskState` directly.

import RalphCore
import SwiftUI

struct TaskListWhatsNextCard: View {
    let task: RalphTask
    let onSelect: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Label("What's Next", systemImage: "sparkles")
                    .font(.headline)
                    .foregroundStyle(.primary)

                Spacer()

                TaskPriorityBadge(priority: task.priority)
            }

            Text(task.title)
                .font(.system(.body, design: .default))
                .lineLimit(2)

            HStack {
                TaskStatusBadge(status: task.status)

                if !task.tags.isEmpty {
                    TaskTagChips(tags: Array(task.tags.prefix(3)))
                }

                Spacer()

                Text(task.id)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .monospaced()
            }
        }
        .padding(12)
        .background(Color.accentColor.opacity(0.1))
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .stroke(Color.accentColor.opacity(0.3), lineWidth: 1)
        )
        .clipShape(.rect(cornerRadius: 10))
        .contentShape(Rectangle())
        .onTapGesture(perform: onSelect)
        .accessibilityLabel("What's Next: \(task.title)")
        .accessibilityHint("Double click to open task details")
        .accessibilityValue("Priority \(task.priority.displayName), Status \(task.status.displayName)")
    }
}

struct TaskListFilterControls: View {
    @ObservedObject var workspace: Workspace
    let onRefresh: () -> Void

    private var filterTextBinding: Binding<String> {
        Binding(
            get: { workspace.taskState.taskFilterText },
            set: { workspace.taskState.taskFilterText = $0 }
        )
    }

    private var statusFilterBinding: Binding<RalphTaskStatus?> {
        Binding(
            get: { workspace.taskState.taskStatusFilter },
            set: { workspace.taskState.taskStatusFilter = $0 }
        )
    }

    private var priorityFilterBinding: Binding<RalphTaskPriority?> {
        Binding(
            get: { workspace.taskState.taskPriorityFilter },
            set: { workspace.taskState.taskPriorityFilter = $0 }
        )
    }

    private var sortOptionBinding: Binding<Workspace.TaskSortOption> {
        Binding(
            get: { workspace.taskState.taskSortBy },
            set: { workspace.taskState.taskSortBy = $0 }
        )
    }

    var body: some View {
        VStack(spacing: 8) {
            HStack {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)

                TextField("Search tasks...", text: filterTextBinding)
                    .textFieldStyle(.plain)
                    .accessibilityIdentifier("task-search-field")

                if !workspace.taskState.taskFilterText.isEmpty {
                    Button(action: { workspace.taskState.taskFilterText = "" }) {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Clear search")
                }
            }
            .padding(8)
            .background(.tertiary.opacity(0.1))
            .clipShape(.rect(cornerRadius: 8))

            HStack(spacing: 12) {
                Picker("Status", selection: statusFilterBinding) {
                    Text("All Status")
                        .tag(nil as RalphTaskStatus?)
                    ForEach(RalphTaskStatus.allCases, id: \.self) { status in
                        Text(status.displayName)
                            .tag(status as RalphTaskStatus?)
                    }
                }
                .pickerStyle(.menu)
                .fixedSize()
                .accessibilityLabel("Filter by Status")

                Picker("Priority", selection: priorityFilterBinding) {
                    Text("All Priorities")
                        .tag(nil as RalphTaskPriority?)
                    ForEach(RalphTaskPriority.allCases, id: \.self) { priority in
                        Text(priority.displayName)
                            .tag(priority as RalphTaskPriority?)
                    }
                }
                .pickerStyle(.menu)
                .fixedSize()
                .accessibilityLabel("Filter by Priority")

                Spacer()

                Picker("Sort", selection: sortOptionBinding) {
                    ForEach(Workspace.TaskSortOption.allCases, id: \.self) { option in
                        Text(option.rawValue)
                            .tag(option)
                    }
                }
                .pickerStyle(.menu)
                .fixedSize()
                .accessibilityLabel("Sort tasks")

                Button(action: { workspace.taskState.taskSortAscending.toggle() }) {
                    Image(systemName: workspace.taskState.taskSortAscending ? "arrow.up" : "arrow.down")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .accessibilityLabel("Toggle sort direction")
                .accessibilityHint("Currently sorted \(workspace.taskState.taskSortAscending ? "ascending" : "descending")")

                Divider()
                    .frame(height: 20)

                Button(action: onRefresh) {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(workspace.taskState.tasksLoading)
                .accessibilityLabel("Refresh task list")
            }
        }
    }
}

struct TaskListExternalUpdateBanner: View {
    let isVisible: Bool

    var body: some View {
        if isVisible {
            HStack(spacing: 8) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .foregroundStyle(.blue)
                Text("Queue updated externally")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.primary)
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.blue.opacity(0.1))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(.blue.opacity(0.3), lineWidth: 1)
            )
            .cornerRadius(6)
            .transition(.move(edge: .top).combined(with: .opacity))
            .accessibilityLabel("Queue updated externally")
        }
    }
}

struct TaskStatusBadge: View {
    let status: RalphTaskStatus

    var body: some View {
        Text(status.displayName)
            .font(.caption2.weight(.medium))
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(TaskListPalette.statusColor(status).opacity(0.2))
            .foregroundStyle(TaskListPalette.statusColor(status))
            .clipShape(.rect(cornerRadius: 4))
    }
}

struct TaskPriorityBadge: View {
    let priority: RalphTaskPriority

    var body: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(TaskListPalette.priorityColor(priority))
                .frame(width: 8, height: 8)
            Text(priority.displayName)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .accessibilityLabel("Priority: \(priority.displayName)")
    }
}

struct TaskTagChips: View {
    let tags: [String]

    var body: some View {
        HStack(spacing: 4) {
            ForEach(tags, id: \.self) { tag in
                Text(tag)
                    .font(.caption2)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(.secondary.opacity(0.15))
                    .foregroundStyle(.secondary)
                    .cornerRadius(4)
            }
        }
    }
}

enum TaskListPalette {
    static func statusColor(_ status: RalphTaskStatus) -> Color {
        switch status {
        case .draft:
            return .gray
        case .todo:
            return .blue
        case .doing:
            return .orange
        case .done:
            return .green
        case .rejected:
            return .red
        }
    }

    static func priorityColor(_ priority: RalphTaskPriority) -> Color {
        switch priority {
        case .critical:
            return .red
        case .high:
            return .orange
        case .medium:
            return .yellow
        case .low:
            return .gray
        }
    }
}
