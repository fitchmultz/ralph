/**
 KanbanCardView

 Purpose:
 - Display a task as a card in the Kanban board.

 Responsibilities:
 - Display a task as a card in the Kanban board.
 - Show priority, status, tags, and visual indicators.
 - Support drag initiation for moving between columns.
 - Indicate blocked status and dependency relationships.

 Does not handle:
 - Drop operations (handled by parent column).
 - Status changes directly (delegates to parent).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Task is non-nil and valid.
 - isBlocked is pre-computed by parent.
 */

import SwiftUI
import RalphCore

struct KanbanCardView: View {
    let task: RalphTask
    let isBlocked: Bool
    let isOverdue: Bool
    let hasDependencies: Bool
    let blockedCount: Int
    var isHighlighted: Bool = false
    var isFocused: Bool = false
    var isSelected: Bool = false

    @State private var isDragging = false

    var body: some View {
        cardContent
            .padding(12)
            .background(cardBackground)
            .overlay(blockedOverlay)
            .opacity(isDragging ? 0.5 : 1.0)
            .scaleEffect(isDragging ? 0.95 : 1.0)
            .animation(.easeInOut(duration: 0.15), value: isDragging)
            .overlay(highlightOverlay)
            // MARK: - Accessibility
            // Card-level accessibility for VoiceOver users
            .accessibilityElement(children: .combine)
            .accessibilityLabel("\(task.id): \(task.title)")
            .accessibilityValue(buildAccessibilityValue())
            .accessibilityHint("Double click to open details. Drag to another column to change status.")
            .accessibilityAddTraits(.isButton)
    }

    // MARK: - View Components

    private var cardContent: some View {
        VStack(alignment: .leading, spacing: 8) {
            cardHeader
            cardTitle
            cardTags
        }
    }

    private var cardHeader: some View {
        HStack {
            priorityIndicator
            Spacer()
            statusIndicators
            taskID
        }
    }

    private var priorityIndicator: some View {
        Circle()
            .fill(priorityColor(task.priority))
            .frame(width: 8, height: 8)
            // MARK: - Accessibility
            .accessibilityLabel("Priority: \(task.priority.displayName)")
    }

    private var statusIndicators: some View {
        HStack(spacing: 4) {
            blockedIndicator
            overdueIndicator
            dependenciesIndicator
        }
    }

    @ViewBuilder
    private var blockedIndicator: some View {
        if isBlocked {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.caption2)
                .foregroundStyle(.red)
                .help("Blocked by dependencies")
                // MARK: - Accessibility
                .accessibilityLabel("Blocked: This task has unresolved dependencies")
        }
    }

    @ViewBuilder
    private var overdueIndicator: some View {
        if isOverdue {
            Image(systemName: "clock.badge.exclamationmark.fill")
                .font(.caption2)
                .foregroundStyle(.orange)
                .help("Overdue task")
                // MARK: - Accessibility
                .accessibilityLabel("Overdue: This task is past due")
        }
    }

    @ViewBuilder
    private var dependenciesIndicator: some View {
        if hasDependencies {
            HStack(spacing: 2) {
                Image(systemName: "link")
                    .font(.caption2)
                if blockedCount > 0 {
                    Text("\(blockedCount)")
                        .font(.caption2)
                }
            }
            .foregroundStyle(.secondary)
            // MARK: - Accessibility
            .accessibilityLabel("\(blockedCount) blocking dependencies")
        }
    }

    private var taskID: some View {
        Text(task.id)
            .font(.caption2)
            .foregroundStyle(.secondary)
            .monospaced()
            // MARK: - Accessibility
            .accessibilityLabel("Task ID: \(task.id)")
    }

    private var cardTitle: some View {
        Text(task.title)
            .font(.system(.body, design: .default))
            .lineLimit(3)
            .foregroundStyle(isBlocked ? .secondary : .primary)
    }

    @ViewBuilder
    private var cardTags: some View {
        if !task.tags.isEmpty {
            FlowLayout(spacing: 4) {
                ForEach(task.tags.prefix(3), id: \.self) { tag in
                    Text(tag)
                        .font(.caption2)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(.secondary.opacity(0.12))
                        .foregroundStyle(.secondary)
                        .clipShape(.rect(cornerRadius: 4))
                }
            }
        }
    }

    private var cardBackground: some View {
        RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(Color(NSColor.controlBackgroundColor))
            .shadow(color: .black.opacity(0.05), radius: 2, x: 0, y: 1)
    }

    private var blockedOverlay: some View {
        RoundedRectangle(cornerRadius: 10, style: .continuous)
            .stroke(isBlocked ? Color.red.opacity(0.3) : Color.clear, lineWidth: 1)
    }

    private var highlightOverlay: some View {
        RoundedRectangle(cornerRadius: 10, style: .continuous)
            .stroke(
                isFocused ? Color.accentColor.opacity(0.8) :
                isHighlighted ? Color.accentColor.opacity(0.6) : Color.clear,
                lineWidth: isFocused ? 3 : 2
            )
            .animation(.easeInOut(duration: 0.3), value: isHighlighted)
            .animation(.easeInOut(duration: 0.15), value: isFocused)
    }

    // MARK: - Accessibility
    /// Builds a descriptive accessibility value for the task card
    /// Combines priority, status, blocked/overdue state, and tags into a single string
    private func buildAccessibilityValue() -> String {
        var parts: [String] = []
        parts.append("Priority: \(task.priority.displayName)")
        parts.append("Status: \(task.status.displayName)")
        if isBlocked { parts.append("Blocked by dependencies") }
        if isOverdue { parts.append("Overdue") }
        if !task.tags.isEmpty { parts.append("Tags: \(task.tags.joined(separator: ", "))") }
        return parts.joined(separator: ", ")
    }

    private func priorityColor(_ priority: RalphTaskPriority) -> Color {
        switch priority {
        case .critical: return .red
        case .high: return .orange
        case .medium: return .yellow
        case .low: return .gray
        }
    }
}

#Preview {
    KanbanCardView(
        task: RalphTask(
            id: "RQ-0001",
            status: .todo,
            title: "Build Kanban board view for task management",
            description: "Create a visual Kanban board",
            priority: .high,
            tags: ["ui", "macos", "swiftui"],
            scope: ["RalphMac"],
            createdAt: Date(),
            updatedAt: Date()
        ),
        isBlocked: false,
        isOverdue: true,
        hasDependencies: true,
        blockedCount: 2
    )
    .frame(width: 260)
    .padding()
}
