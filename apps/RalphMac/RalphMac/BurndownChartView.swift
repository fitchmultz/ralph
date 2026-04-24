/**
 BurndownChartView

 Purpose:
 - Render a line chart showing remaining tasks over time.

 Responsibilities:
 - Render a line chart showing remaining tasks over time.
 - Uses SwiftUI Charts LineMark for visualization.

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

struct BurndownChartView: View {
    let burndown: BurndownReport?
    
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Task Burndown")
                .font(.headline)
                .padding(.horizontal)
                .padding(.top)
            
            if let burndown = burndown, !burndown.dailyCounts.isEmpty {
                Chart(burndown.dailyCounts, id: \.date) { day in
                    LineMark(
                        x: .value("Date", formatDate(day.date)),
                        y: .value("Remaining", day.remaining)
                    )
                    .foregroundStyle(.blue)
                    .lineStyle(StrokeStyle(lineWidth: 2))
                    
                    AreaMark(
                        x: .value("Date", formatDate(day.date)),
                        y: .value("Remaining", day.remaining)
                    )
                    .foregroundStyle(.blue.opacity(0.1))
                    
                    if day.remaining > 0 {
                        PointMark(
                            x: .value("Date", formatDate(day.date)),
                            y: .value("Remaining", day.remaining)
                        )
                        .foregroundStyle(.blue)
                        .symbolSize(30)
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
                .padding()
                .accessibilityLabel("Burndown chart")
                .accessibilityHint("Line chart showing remaining tasks over time")
                .accessibilityHidden(true)
                
                // Accessible alternative for VoiceOver
                .accessibilityElement(children: .combine)
                .accessibilityLabel(burndownAccessibilityText(burndown: burndown))
            } else {
                emptyStateView(message: "No burndown data available")
            }
        }
    }
    
    private func burndownAccessibilityText(burndown: BurndownReport) -> String {
        guard let first = burndown.dailyCounts.first,
              let last = burndown.dailyCounts.last else {
            return "No burndown data available"
        }
        let completed = first.remaining - last.remaining
        return "Burndown: Started with \(first.remaining) tasks, now at \(last.remaining). \(completed) tasks completed over \(burndown.dailyCounts.count) days."
    }
    
    private func formatDate(_ dateString: String) -> String {
        // Convert YYYY-MM-DD to a shorter format
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
