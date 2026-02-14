/**
 PriorityDistributionCard

 Responsibilities:
 - Show a compact bar chart of tasks by priority.
 */

import SwiftUI
import Charts
import RalphCore

struct PriorityDistributionCard: View {
    let tasks: [RalphTask]
    
    private var priorityCounts: [(priority: RalphTaskPriority, count: Int)] {
        let grouped = Dictionary(grouping: tasks) { $0.priority }
        return RalphTaskPriority.allCases.map { priority in
            (priority: priority, count: grouped[priority]?.count ?? 0)
        }
    }
    
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("By Priority")
                .font(.headline)
            
            let data = priorityCounts
            if data.contains(where: { $0.count > 0 }) {
                Chart(data, id: \.priority) { item in
                    BarMark(
                        x: .value("Priority", item.priority.displayName),
                        y: .value("Count", item.count)
                    )
                    .foregroundStyle(priorityColor(item.priority))
                    .clipShape(.rect(cornerRadius: 4))
                }
                .chartYAxis {
                    AxisMarks(position: .leading)
                }
                .frame(height: 100)
                .accessibilityLabel("Priority distribution chart")
                .accessibilityHint("Bar chart showing tasks by priority")
                .accessibilityHidden(true)
                
                // Accessible alternative
                .accessibilityElement(children: .combine)
                .accessibilityLabel(priorityDistributionAccessibilityText(data: data))
            } else {
                Text("No tasks")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(height: 100)
            }
        }
        .padding()
        .background(.quaternary.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
    
    private func priorityColor(_ priority: RalphTaskPriority) -> Color {
        switch priority {
        case .critical: return .red
        case .high: return .orange
        case .medium: return .yellow
        case .low: return .green
        }
    }
    
    private func priorityDistributionAccessibilityText(data: [(priority: RalphTaskPriority, count: Int)]) -> String {
        let items = data.filter { $0.count > 0 }.map { "\($0.priority.displayName): \($0.count)" }.joined(separator: ", ")
        return "Priority distribution: \(items)"
    }
}
