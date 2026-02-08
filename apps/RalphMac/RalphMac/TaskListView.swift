/**
 TaskListView

 Responsibilities:
 - Display a rich, sortable, filterable list of tasks from the Ralph queue.
 - Show task metadata with visual indicators (status badges, priority dots, tag chips).
 - Provide search, filtering, and sorting controls.
 - Display "What's Next" section highlighting the next todo task.

 Does not handle:
 - Task editing (see TaskDetailView).
 - Task creation (see TaskCreationView).
 - Direct CLI execution (delegates to Workspace).

 Invariants/assumptions callers must respect:
 - Workspace is injected via @ObservedObject.
 - Tasks are loaded via workspace.loadTasks() before display.
 */

import SwiftUI
import RalphCore

struct TaskListView: View {
    @ObservedObject var workspace: Workspace
    @State private var selectedTaskID: String?
    @State private var selectedTaskForEditing: RalphTask?

    var body: some View {
        VStack(spacing: 0) {
            // What's Next section
            if let nextTask = workspace.nextTask() {
                whatsNextSection(task: nextTask)
                    .padding(.horizontal, 16)
                    .padding(.top, 16)
                    .padding(.bottom, 12)
            }

            // Filter and sort controls
            filterControls()
                .padding(.horizontal, 16)
                .padding(.bottom, 12)

            // Task list
            taskList()
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
        }
        .background(.clear)
        .task {
            await workspace.loadTasks()
        }
    }

    // MARK: - What's Next Section

    @ViewBuilder
    private func whatsNextSection(task: RalphTask) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Label("What's Next", systemImage: "sparkles")
                    .font(.headline)
                    .foregroundStyle(.primary)

                Spacer()

                priorityDot(priority: task.priority)
            }

            Text(task.title)
                .font(.system(.body, design: .default))
                .lineLimit(2)

            HStack {
                statusBadge(status: task.status)

                if !task.tags.isEmpty {
                    tagChips(tags: Array(task.tags.prefix(3)))
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
        .cornerRadius(10)
    }

    // MARK: - Filter Controls

    @ViewBuilder
    private func filterControls() -> some View {
        VStack(spacing: 8) {
            // Search bar
            HStack {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)

                TextField("Search tasks...", text: $workspace.taskFilterText)
                    .textFieldStyle(.plain)

                if !workspace.taskFilterText.isEmpty {
                    Button(action: { workspace.taskFilterText = "" }) {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(8)
            .background(.tertiary.opacity(0.1))
            .cornerRadius(8)

            // Filter and sort row
            HStack(spacing: 12) {
                // Status filter
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

                // Priority filter
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

                Spacer()

                // Sort picker
                Picker("Sort", selection: $workspace.taskSortBy) {
                    ForEach(Workspace.TaskSortOption.allCases, id: \.self) { option in
                        Text(option.rawValue)
                            .tag(option)
                    }
                }
                .pickerStyle(.menu)
                .fixedSize()

                // Sort direction toggle
                Button(action: { workspace.taskSortAscending.toggle() }) {
                    Image(systemName: workspace.taskSortAscending ? "arrow.up" : "arrow.down")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)

                Divider()
                    .frame(height: 20)

                // Refresh button
                Button(action: {
                    Task { @MainActor in
                        await workspace.loadTasks()
                    }
                }) {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .disabled(workspace.tasksLoading)
            }
        }
    }

    // MARK: - Task List

    @ViewBuilder
    private func taskList() -> some View {
        let filteredTasks = workspace.filteredAndSortedTasks()

        VStack(spacing: 0) {
            if workspace.tasksLoading {
                ProgressView("Loading tasks...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = workspace.tasksErrorMessage {
                VStack(spacing: 12) {
                    Image(systemName: "exclamationmark.triangle")
                        .font(.largeTitle)
                        .foregroundStyle(.red)
                    Text("Failed to load tasks")
                        .font(.headline)
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if filteredTasks.isEmpty {
                VStack(spacing: 12) {
                    Image(systemName: "checkmark.circle")
                        .font(.largeTitle)
                        .foregroundStyle(.secondary)
                    Text("No tasks found")
                        .font(.headline)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List(filteredTasks, selection: $selectedTaskID) { task in
                    TaskRow(task: task)
                        .tag(task.id)
                        .listRowSeparator(.visible)
                        .listRowInsets(EdgeInsets(top: 8, leading: 12, bottom: 8, trailing: 12))
                        .contentShape(Rectangle())
                        .onTapGesture {
                            selectedTaskForEditing = task
                        }
                }
                .listStyle(.plain)
                #if swift(>=5.9)
                .alternatingRowBackgrounds(.automatic)
                #endif
                .sheet(item: $selectedTaskForEditing) { task in
                    TaskDetailView(
                        workspace: workspace,
                        task: task,
                        isPresented: Binding(
                            get: { selectedTaskForEditing != nil },
                            set: { if !$0 { selectedTaskForEditing = nil } }
                        )
                    )
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.clear)
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .stroke(.separator.opacity(0.3), lineWidth: 0.5)
        )
        .cornerRadius(10)
    }

    // MARK: - Helper Views

    @ViewBuilder
    private func statusBadge(status: RalphTaskStatus) -> some View {
        Text(status.displayName)
            .font(.caption2.weight(.medium))
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(statusColor(status).opacity(0.2))
            .foregroundStyle(statusColor(status))
            .cornerRadius(4)
    }

    @ViewBuilder
    private func priorityDot(priority: RalphTaskPriority) -> some View {
        HStack(spacing: 4) {
            Circle()
                .fill(priorityColor(priority))
                .frame(width: 8, height: 8)
            Text(priority.displayName)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private func tagChips(tags: [String]) -> some View {
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

    private func statusColor(_ status: RalphTaskStatus) -> Color {
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

    private func priorityColor(_ priority: RalphTaskPriority) -> Color {
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

// MARK: - Task Row

struct TaskRow: View {
    let task: RalphTask

    var body: some View {
        HStack(spacing: 12) {
            // Priority indicator
            Circle()
                .fill(priorityColor(task.priority))
                .frame(width: 8, height: 8)

            VStack(alignment: .leading, spacing: 4) {
                // Title
                Text(task.title)
                    .font(.system(.body, design: .default))
                    .lineLimit(1)

                HStack(spacing: 8) {
                    // Status badge
                    Text(task.status.displayName)
                        .font(.caption2.weight(.medium))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 1)
                        .background(statusColor(task.status).opacity(0.2))
                        .foregroundStyle(statusColor(task.status))
                        .cornerRadius(4)

                    // Tags
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
                    }
                }
            }

            Spacer()

            // Task ID
            Text(task.id)
                .font(.caption)
                .foregroundStyle(.secondary)
                .monospaced()
        }
        .padding(.vertical, 4)
    }

    private func statusColor(_ status: RalphTaskStatus) -> Color {
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

    private func priorityColor(_ priority: RalphTaskPriority) -> Color {
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


