/**
 KanbanColumnView

 Purpose:
 - Display a single Kanban column for a specific status.

 Responsibilities:
 - Display a single Kanban column for a specific status.
 - Show column header with task count badge.
 - Accept dropped tasks and trigger status updates.
 - Display tasks in a scrollable list.

 Does not handle:
 - Actual status change execution (delegates to workspace).
 - Cross-column drag visualization (handled by SwiftUI).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Status is one of RalphTaskStatus cases.
 - Tasks are pre-filtered by the parent board.
 - onTaskDrop is called with the dragged task ID.
 */

import SwiftUI
import RalphCore
// Note: RalphCore provides accessibility helpers for task and status types

struct KanbanColumnView: View {
    let status: RalphTaskStatus
    let tasks: [RalphTask]
    let isTaskBlocked: (RalphTask) -> Bool
    let isTaskOverdue: (RalphTask) -> Bool
    let onTaskDrop: (String) -> Void
    let onTaskSelect: (String) -> Void
    var highlightedTaskIDs: Set<String> = []
    
    // MARK: - Keyboard Navigation
    var focusedTaskID: String? = nil
    var isFocusedColumn: Bool = false

    @State private var isTargeted = false

    var body: some View {
        let column = VStack(spacing: 0) {
            columnHeader
            taskList
        }
        .frame(width: 280)
        .background(Color(NSColor.controlBackgroundColor))
        .clipShape(.rect(cornerRadius: 12))
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(Color.gray.opacity(0.3), lineWidth: 1)
        )
        
        return column
            // MARK: - Accessibility
            // Column-level accessibility for VoiceOver users
            .accessibilityLabel("\(status.displayName) column")
            .accessibilityValue("\(tasks.count) tasks")
            .accessibilityHint("Drop tasks here to change their status to \(status.displayName)")
            .dropDestination(for: String.self) { items, _ in
                guard let taskID = items.first else { return false }
                onTaskDrop(taskID)
                return true
            } isTargeted: { targeted in
                withAnimation(.easeInOut(duration: 0.15)) {
                    isTargeted = targeted
                }
            }
    }
    
    private var columnHeader: some View {
        HStack {
            Circle()
                .fill(statusColor(status))
                .frame(width: 8, height: 8)
                // MARK: - Accessibility
                .accessibilityLabel("Status: \(status.displayName)")

            Text(status.displayName)
                .font(.headline)

            Spacer()

            // Task count badge
            Text("\(tasks.count)")
                .font(.caption.weight(.medium))
                .padding(.horizontal, 8)
                .padding(.vertical, 2)
                .background(Color.gray.opacity(0.15))
                .foregroundStyle(.secondary)
                .clipShape(.rect(cornerRadius: 10))
                // MARK: - Accessibility
                .accessibilityLabel("\(tasks.count) tasks")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(Color(NSColor.controlBackgroundColor))
        .overlay(
            Rectangle()
                .frame(height: 1)
                .foregroundStyle(.gray.opacity(0.3)),
            alignment: .bottom
        )
    }
    
    private var taskList: some View {
        ScrollView {
            LazyVStack(spacing: 8) {
                // MARK: - Accessibility
                // Use enumerated to provide sort priority based on position in list
                ForEach(Array(tasks.enumerated()), id: \.element.id) { index, task in
                    taskCard(for: task, index: index, totalCount: tasks.count)
                }
            }
            .padding(12)
        }
        .background(
            Color(NSColor.controlBackgroundColor)
                .opacity(0.5)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 0)
                .stroke(isTargeted ? Color.accentColor : Color.clear, lineWidth: 2)
        )
    }
    
    private func taskCard(for task: RalphTask, index: Int, totalCount: Int) -> some View {
        KanbanCardView(
            task: task,
            isBlocked: isTaskBlocked(task),
            isOverdue: isTaskOverdue(task),
            hasDependencies: task.dependsOn?.isEmpty == false,
            blockedCount: task.dependsOn?.count ?? 0,
            isHighlighted: highlightedTaskIDs.contains(task.id),
            isFocused: focusedTaskID == task.id,
            isSelected: false
        )
        .contentShape(Rectangle())
        .onTapGesture {
            onTaskSelect(task.id)
        }
        // MARK: - Accessibility
        // Sort priority ensures VoiceOver reads tasks in visual order (top to bottom)
        .accessibilitySortPriority(Double(totalCount - index))
        .draggable(task.id) {
            // Drag preview with accessibility label
            Text(task.title)
                .padding(8)
                .background(Color.accentColor)
                .foregroundStyle(.white)
                .clipShape(.rect(cornerRadius: 8))
                // MARK: - Accessibility
                .accessibilityLabel("Dragging: \(task.title)")
        }
        // MARK: - Accessibility
        // VoiceOver actions for users who cannot use drag and drop
        // Note: These actions require the parent (KanbanBoardView) to handle status changes
        // via the onTaskDrop callback. The actual status change logic is implemented there.
        .accessibilityAction(named: "Move to Todo") {
            if status != .todo { onTaskDrop(task.id) }
        }
        .accessibilityAction(named: "Move to Doing") {
            if status != .doing { onTaskDrop(task.id) }
        }
        .accessibilityAction(named: "Move to Done") {
            if status != .done { onTaskDrop(task.id) }
        }
    }

    private func statusColor(_ status: RalphTaskStatus) -> Color {
        switch status {
        case .draft: return .gray
        case .todo: return .blue
        case .doing: return .orange
        case .done: return .green
        case .rejected: return .red
        }
    }
}

#Preview {
    KanbanColumnView(
        status: .todo,
        tasks: [
            RalphTask(
                id: "RQ-0001",
                status: .todo,
                title: "Build Kanban board view",
                priority: .high,
                tags: ["ui", "macos"],
                createdAt: Date(),
                updatedAt: Date()
            ),
            RalphTask(
                id: "RQ-0002",
                status: .todo,
                title: "Add drag and drop support",
                priority: .medium,
                tags: ["ux"],
                createdAt: Date(),
                updatedAt: Date()
            )
        ],
        isTaskBlocked: { _ in false },
        isTaskOverdue: { _ in false },
        onTaskDrop: { _ in },
        onTaskSelect: { _ in }
    )
    .frame(height: 400)
    .padding()
}
