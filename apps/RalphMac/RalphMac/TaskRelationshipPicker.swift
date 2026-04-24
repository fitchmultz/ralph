/**
 TaskRelationshipPicker

 Purpose:
 - Display and edit task relationship arrays (dependsOn, blocks, relatesTo).

 Responsibilities:
 - Display and edit task relationship arrays (dependsOn, blocks, relatesTo).
 - Provide a picker to select from available tasks.
 - Prevent self-referencing, duplicates, and circular dependencies.

 Does not handle:
 - Direct persistence (handled by parent view).
 - Complex multi-hop cycle validation (uses simplified cycle detection).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - existingEdges should represent the current state of all task relationships.
 - edgeType determines the direction and semantics of the relationship.
 */

import SwiftUI
import RalphCore

struct TaskRelationshipPicker: View {
    let label: String
    @Binding var relatedTaskIDs: [String]
    let allTaskIDs: [String]
    let currentTaskID: String
    
    // NEW: Edge type and existing edges for cycle detection
    let edgeType: GraphEdge.EdgeType
    let existingEdges: [GraphEdge]
    
    @State private var selectedTaskID: String = ""
    @State private var cycleWarning: String? = nil
    
    // Filter out current task, already-selected tasks, AND tasks that would create cycles
    private var availableTaskIDs: [String] {
        allTaskIDs.filter { candidateID in
            // Filter out self
            guard candidateID != currentTaskID else { return false }
            
            // Filter out already selected
            guard !relatedTaskIDs.contains(candidateID) else { return false }
            
            // NEW: Check if adding this relationship would create a cycle
            let testEdge: GraphEdge
            switch edgeType {
            case .dependency:
                // depends_on: current task depends on selected task
                testEdge = GraphEdge(from: currentTaskID, to: candidateID, type: .dependency)
            case .blocks:
                // blocks: current task blocks selected task
                testEdge = GraphEdge(from: currentTaskID, to: candidateID, type: .blocks)
            case .relatesTo:
                // relates_to: can be bidirectional but doesn't participate in cycles
                return true
            }
            
            return !GraphAlgorithms.wouldCreateCycle(
                existingEdges: existingEdges,
                newEdge: testEdge,
                allTaskIDs: allTaskIDs + [currentTaskID]
            )
        }
    }
    
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Label
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
                .accessibilityLabel("\(label) relationships")
            
            // Selected relationships
            if relatedTaskIDs.isEmpty {
                Text("None")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .italic()
                    .accessibilityLabel("No \(label) relationships set")
            } else {
                // Use FlowLayout for relationship chips
                FlowLayout(spacing: 6) {
                    ForEach(Array(relatedTaskIDs.enumerated()), id: \.element) { index, taskID in
                        RelationshipChip(taskID: taskID) {
                            removeRelationship(taskID)
                        }
                        .accessibilitySortPriority(Double(relatedTaskIDs.count - index))
                    }
                }
                .accessibilityLabel("\(relatedTaskIDs.count) \(label) relationships: \(relatedTaskIDs.joined(separator: ", "))")
            }
            
            // Cycle warning for currently selected task
            if let warning = cycleWarning {
                HStack {
                    Image(systemName: "exclamationmark.triangle")
                        .foregroundStyle(.orange)
                    Text(warning)
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
                .padding(.vertical, 4)
            }
            
            // Add relationship picker
            if !availableTaskIDs.isEmpty {
                HStack {
                    Picker("", selection: $selectedTaskID) {
                        Text("Select task...")
                            .tag("")
                        ForEach(availableTaskIDs, id: \.self) { taskID in
                            Text(taskID)
                                .tag(taskID)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(maxWidth: 200)
                    .accessibilityLabel("Select a task to add as \(label)")
                    .onChange(of: selectedTaskID) { _, newValue in
                        // Clear warning when selection changes
                        if !newValue.isEmpty {
                            cycleWarning = nil
                        }
                    }
                    
                    Button(action: addRelationship) {
                        Image(systemName: "plus.circle.fill")
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(Color.accentColor)
                    .disabled(selectedTaskID.isEmpty)
                    .accessibilityLabel("Add selected task as \(label)")
                    .accessibilityHint("Adds the selected task to the \(label) relationship")
                }
            } else if relatedTaskIDs.count < allTaskIDs.count {
                // Some tasks are unavailable because they would create cycles
                HStack {
                    Image(systemName: "info.circle")
                        .foregroundStyle(.secondary)
                    Text("Remaining tasks would create circular dependencies")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, 4)
            }
        }
    }
    
    private func addRelationship() {
        guard !selectedTaskID.isEmpty else { return }
        guard !relatedTaskIDs.contains(selectedTaskID) else { return }
        relatedTaskIDs.append(selectedTaskID)
        selectedTaskID = ""
    }
    
    private func removeRelationship(_ taskID: String) {
        relatedTaskIDs.removeAll { $0 == taskID }
    }
}

// Relationship Chip - similar to TagChip but different style
struct RelationshipChip: View {
    let taskID: String
    let onRemove: () -> Void
    
    var body: some View {
        HStack(spacing: 4) {
            Text(taskID)
                .font(.caption)
                .monospaced()
            
            Button(action: onRemove) {
                Image(systemName: "xmark")
                    .font(.caption2)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .accessibilityLabel("Remove relationship to \(taskID)")
            .accessibilityHint("Removes this task relationship")
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(Color.secondary.opacity(0.15))
        .foregroundStyle(.primary)
        .clipShape(.rect(cornerRadius: 6))
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Relationship to \(taskID)")
    }
}

// Preview
#Preview {
    TaskRelationshipPicker(
        label: "Depends On",
        relatedTaskIDs: .constant(["RQ-0001", "RQ-0002"]),
        allTaskIDs: ["RQ-0001", "RQ-0002", "RQ-0003", "RQ-0004"],
        currentTaskID: "RQ-0005",
        edgeType: .dependency,
        existingEdges: [
            GraphEdge(from: "RQ-0005", to: "RQ-0001", type: .dependency),
            GraphEdge(from: "RQ-0005", to: "RQ-0002", type: .dependency)
        ]
    )
    .padding()
}
