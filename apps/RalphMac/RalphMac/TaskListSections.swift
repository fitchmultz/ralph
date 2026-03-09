/**
 TaskListSections

 Responsibilities:
 - Provide decomposed presentation components for `TaskListView`.
 - Keep row rendering, filter controls, refresh banners, and content-state rendering out of the root list surface.
 - Encapsulate task-list-specific visuals while delegating behavior to closures and bindings.

 Does not handle:
 - Loading tasks from the workspace.
 - Persisting mutations or deciding navigation outcomes.

 Invariants/assumptions callers must respect:
 - Components expect a live `Workspace` model and bindings from `TaskListView`.
 - Selection and action callbacks are supplied by the owning view.
 */

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

    var body: some View {
        VStack(spacing: 8) {
            HStack {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)

                TextField("Search tasks...", text: $workspace.taskFilterText)
                    .textFieldStyle(.plain)
                    .accessibilityIdentifier("task-search-field")

                if !workspace.taskFilterText.isEmpty {
                    Button(action: { workspace.taskFilterText = "" }) {
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
                Picker("Status", selection: $workspace.taskStatusFilter) {
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

                Picker("Priority", selection: $workspace.taskPriorityFilter) {
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

                Picker("Sort", selection: $workspace.taskSortBy) {
                    ForEach(Workspace.TaskSortOption.allCases, id: \.self) { option in
                        Text(option.rawValue)
                            .tag(option)
                    }
                }
                .pickerStyle(.menu)
                .fixedSize()
                .accessibilityLabel("Sort tasks")

                Button(action: { workspace.taskSortAscending.toggle() }) {
                    Image(systemName: workspace.taskSortAscending ? "arrow.up" : "arrow.down")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .accessibilityLabel("Toggle sort direction")
                .accessibilityHint("Currently sorted \(workspace.taskSortAscending ? "ascending" : "descending")")

                Divider()
                    .frame(height: 20)

                Button(action: onRefresh) {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(workspace.tasksLoading)
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

struct TaskListContent: View {
    @ObservedObject var workspace: Workspace
    let selectedTaskIDs: Binding<Set<String>>
    let selectedTaskID: String?
    let focusedTaskID: FocusState<String?>.Binding
    let highlightedTaskIDs: Set<String>
    let onTaskTap: (String) -> Void
    let onTaskDecompose: (String) -> Void
    let onOpenSelectedTask: () -> Void
    let onNavigate: (_ direction: Int, _ tasks: [RalphTask]) -> Void

    var body: some View {
        let presentation = workspace.taskPresentation()
        let filteredTasks = presentation.tasks

        VStack(spacing: 0) {
            if workspace.tasksLoading {
                ProgressView("Loading tasks...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let recoveryError = workspace.lastRecoveryError,
                      workspace.showErrorRecovery {
                ErrorRecoveryView(
                    error: recoveryError,
                    workspace: workspace,
                    onRetry: {
                        Task { @MainActor in
                            workspace.clearErrorRecovery()
                            await workspace.loadTasks()
                        }
                    },
                    onDismiss: {
                        workspace.clearErrorRecovery()
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = workspace.tasksErrorMessage {
                TaskListEmptyState(
                    systemImage: "exclamationmark.triangle",
                    title: "Failed to load tasks",
                    message: error,
                    accentColor: .red
                )
            } else if filteredTasks.isEmpty {
                TaskListEmptyState(
                    systemImage: "checkmark.circle",
                    title: "No tasks found",
                    message: nil,
                    accentColor: .secondary
                )
            } else {
                List(filteredTasks, selection: selectedTaskIDs) { task in
                    TaskRow(
                        task: task,
                        isHighlighted: highlightedTaskIDs.contains(task.id),
                        isSelected: selectedTaskIDs.wrappedValue.contains(task.id),
                        isFocused: focusedTaskID.wrappedValue == task.id
                    )
                    .tag(task.id)
                    .focused(focusedTaskID, equals: task.id)
                    .contextMenu {
                        Button("Decompose Task...") {
                            onTaskTap(task.id)
                            focusedTaskID.wrappedValue = task.id
                            onTaskDecompose(task.id)
                        }
                    }
                    .onTapGesture {
                        onTaskTap(task.id)
                        focusedTaskID.wrappedValue = task.id
                    }
                    .transition(.asymmetric(
                        insertion: .slide.combined(with: .opacity),
                        removal: .opacity
                    ))
                    .listRowSeparator(.visible)
                    .listRowInsets(EdgeInsets(top: 8, leading: 12, bottom: 8, trailing: 12))
                    .contentShape(Rectangle())
                }
                .listStyle(.plain)
                .accessibilityIdentifier("task-list-container")
                #if swift(>=5.9)
                .alternatingRowBackgrounds(.automatic)
                #endif
                .onKeyPress(.upArrow) {
                    onNavigate(-1, filteredTasks)
                    return .handled
                }
                .onKeyPress(.downArrow) {
                    onNavigate(1, filteredTasks)
                    return .handled
                }
                .onKeyPress(.return) {
                    onOpenSelectedTask()
                    return .handled
                }
                .onKeyPress(.space) {
                    onOpenSelectedTask()
                    return .handled
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.clear)
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .stroke(.separator.opacity(0.3), lineWidth: 0.5)
        )
        .clipShape(.rect(cornerRadius: 10))
    }
}

private struct TaskListEmptyState: View {
    let systemImage: String
    let title: String
    let message: String?
    let accentColor: Color

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: systemImage)
                .font(.largeTitle)
                .foregroundStyle(accentColor)
                .accessibilityLabel(title)
            Text(title)
                .font(.headline)
                .foregroundStyle(message == nil ? .secondary : .primary)
            if let message {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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

private enum TaskListPalette {
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

struct TaskRow: View {
    let task: RalphTask
    var isHighlighted: Bool = false
    var isSelected: Bool = false
    var isFocused: Bool = false

    var body: some View {
        HStack(spacing: 12) {
            Circle()
                .fill(TaskListPalette.priorityColor(task.priority))
                .frame(width: 8, height: 8)
                .accessibilityLabel("Priority: \(task.priority.displayName)")

            VStack(alignment: .leading, spacing: 4) {
                Text(task.title)
                    .font(.system(.body, design: .default))
                    .lineLimit(1)

                HStack(spacing: 8) {
                    Text(task.status.displayName)
                        .font(.caption2.weight(.medium))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 1)
                        .background(TaskListPalette.statusColor(task.status).opacity(0.2))
                        .foregroundStyle(TaskListPalette.statusColor(task.status))
                        .cornerRadius(4)
                        .accessibilityLabel("Status: \(task.status.displayName)")

                    if !task.tags.isEmpty {
                        HStack(spacing: 4) {
                            ForEach(task.tags.prefix(3), id: \.self) { tag in
                                Text(tag)
                                    .font(.caption2)
                                    .padding(.horizontal, 4)
                                    .padding(.vertical, 1)
                                    .background(.secondary.opacity(0.12))
                                    .foregroundStyle(.secondary)
                                    .cornerRadius(3)
                            }
                        }
                        .accessibilityLabel("Tags: \(task.tags.joined(separator: ", "))")
                    }
                }
            }

            Spacer()

            Text(task.id)
                .font(.caption)
                .foregroundStyle(.secondary)
                .monospaced()
                .accessibilityLabel("Task ID: \(task.id)")
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 8)
        .background(backgroundColor)
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .stroke(isFocused ? Color.accentColor : Color.clear, lineWidth: 2)
        )
        .cornerRadius(6)
        .animation(.easeInOut(duration: 0.2), value: isHighlighted)
        .animation(.easeInOut(duration: 0.15), value: isFocused)
        .animation(.easeInOut(duration: 0.15), value: isSelected)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(task.id): \(task.title)")
        .accessibilityValue("Priority \(task.priority.displayName), Status \(task.status.displayName), Tags: \(task.tags.joined(separator: ", "))")
        .accessibilityHint("Select to view task details. Use arrow keys to navigate.")
        .accessibilityAddTraits(.isButton)
    }

    private var backgroundColor: Color {
        if isHighlighted { return Color.accentColor.opacity(0.15) }
        if isSelected { return Color.accentColor.opacity(0.1) }
        return Color.clear
    }
}
