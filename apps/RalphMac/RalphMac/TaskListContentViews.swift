//!
//! TaskListContentViews
//!
//! Purpose:
//! - Hold the main task-list content states and row rendering.
//!
//! Responsibilities:
//! - Render loading, recovery, empty, and populated list states.
//! - Provide row-level visuals for tasks.
//!
//! Scope:
//! - Task-list content only.
//!
//! Usage:
//! - Composed by `TaskListView`.
//!
//! Invariants/Assumptions:
//! - Workspace task data is read from `taskState` and `diagnosticsState`.

import RalphCore
import SwiftUI

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
            if workspace.taskState.tasksLoading {
                ProgressView("Loading tasks...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let recoveryError = workspace.diagnosticsState.lastRecoveryError,
                      workspace.diagnosticsState.showErrorRecovery {
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
            } else if let error = workspace.taskState.tasksErrorMessage {
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
