/**
 TaskRelationshipPicker

 Responsibilities:
 - Display and edit task relationship arrays (dependsOn, blocks, relatesTo).
 - Provide a picker to select from available tasks.
 - Prevent self-referencing and duplicates.

 Does not handle:
 - Circular dependency detection.
 - Relationship validation beyond basic checks.
 */

import SwiftUI
import RalphCore

struct TaskRelationshipPicker: View {
    let label: String
    @Binding var relatedTaskIDs: [String]
    let allTaskIDs: [String]
    let currentTaskID: String
    
    @State private var selectedTaskID: String = ""
    
    // Filter out current task and already-selected tasks
    private var availableTaskIDs: [String] {
        allTaskIDs.filter { $0 != currentTaskID && !relatedTaskIDs.contains($0) }
    }
    
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Label
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
            
            // Selected relationships
            if relatedTaskIDs.isEmpty {
                Text("None")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .italic()
            } else {
                // Use FlowLayout for relationship chips
                FlowLayout(spacing: 6) {
                    ForEach(relatedTaskIDs, id: \.self) { taskID in
                        RelationshipChip(taskID: taskID) {
                            removeRelationship(taskID)
                        }
                    }
                }
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
                    
                    Button(action: addRelationship) {
                        Image(systemName: "plus.circle.fill")
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(Color.accentColor)
                    .disabled(selectedTaskID.isEmpty)
                }
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
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(Color.secondary.opacity(0.15))
        .foregroundStyle(.primary)
        .cornerRadius(6)
    }
}

// Preview
#Preview {
    TaskRelationshipPicker(
        label: "Depends On",
        relatedTaskIDs: .constant(["RQ-0001", "RQ-0002"]),
        allTaskIDs: ["RQ-0001", "RQ-0002", "RQ-0003", "RQ-0004"],
        currentTaskID: "RQ-0005"
    )
    .padding()
}
