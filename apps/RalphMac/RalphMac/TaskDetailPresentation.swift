/**
 TaskDetailPresentation

 Responsibilities:
 - Provide presentation-only helpers for task detail sections.
 - Centralize formatting and relationship-edge derivation used by decomposed task detail components.
 - Keep color and metadata display logic out of section bodies.

 Does not handle:
 - Mutable editor state.
 - Conflict resolution or persistence side effects.

 Invariants/assumptions callers must respect:
 - Helpers are side-effect free and safe to call during SwiftUI rendering.
 - Relationship edge construction expects the full workspace task set.
 */

import RalphCore
import SwiftUI

enum TaskDetailPresentation {
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

    static func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }

    static func formatDateForAccessibility(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .long
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }

    static func existingEdges(from tasks: [RalphTask]) -> [GraphEdge] {
        var edges: [GraphEdge] = []

        for task in tasks {
            for depID in task.dependsOn ?? [] {
                edges.append(GraphEdge(from: task.id, to: depID, type: .dependency))
            }
            for blockedID in task.blocks ?? [] {
                edges.append(GraphEdge(from: task.id, to: blockedID, type: .blocks))
            }
            for relatedID in task.relatesTo ?? [] where task.id < relatedID {
                edges.append(GraphEdge(from: task.id, to: relatedID, type: .relatesTo))
            }
        }

        return edges
    }
}
