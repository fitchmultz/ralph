/**
 CompletionHistoryChart

 Purpose:
 - Render a line chart showing both tasks created and completed over time.

 Responsibilities:
 - Render a line chart showing both tasks created and completed over time.

 Scope:
 - Limited to the responsibilities listed above.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import SwiftUI
import Charts
import RalphCore

struct CompletionHistoryChart: View {
    let history: HistoryReport?
    
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Task Activity")
                .font(.headline)
                .padding(.horizontal)
                .padding(.top)
            
            if let history = history, !history.days.isEmpty {
                Chart {
                    ForEach(history.days, id: \.date) { day in
                        LineMark(
                            x: .value("Date", formatDate(day.date)),
                            y: .value("Created", day.created.count)
                        )
                        .foregroundStyle(.blue)
                        .lineStyle(StrokeStyle(lineWidth: 2))
                        
                        LineMark(
                            x: .value("Date", formatDate(day.date)),
                            y: .value("Completed", day.completed.count)
                        )
                        .foregroundStyle(.green)
                        .lineStyle(StrokeStyle(lineWidth: 2))
                    }
                }
                .chartXAxis {
                    AxisMarks { value in
                        AxisValueLabel {
                            if let dateStr = value.as(String.self) {
                                Text(dateStr)
                                    .font(.caption)
                            }
                        }
                    }
                }
                .chartYAxis {
                    AxisMarks(position: .leading)
                }
                .chartLegend(position: .top, alignment: .trailing)
                .padding()
                .accessibilityLabel("Task activity chart")
                .accessibilityHint("Line chart showing tasks created and completed over time")
                .accessibilityHidden(true)
                
                // Accessible alternative for VoiceOver
                .accessibilityElement(children: .combine)
                .accessibilityLabel(historyAccessibilityText(history: history))
            } else {
                emptyStateView(message: "No history data available")
            }
        }
    }
    
    private func historyAccessibilityText(history: HistoryReport) -> String {
        let totalCreated = history.days.reduce(0) { $0 + $1.created.count }
        let totalCompleted = history.days.reduce(0) { $0 + $1.completed.count }
        return "Task activity over \(history.days.count) days: \(totalCreated) tasks created, \(totalCompleted) tasks completed."
    }
    
    private func formatDate(_ dateString: String) -> String {
        let components = dateString.split(separator: "-")
        if components.count == 3 {
            return "\(components[1])-\(components[2])"
        }
        return dateString
    }
    
    @ViewBuilder
    private func emptyStateView(message: String) -> some View {
        VStack {
            Spacer()
            Text(message)
                .foregroundStyle(.secondary)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }
}
