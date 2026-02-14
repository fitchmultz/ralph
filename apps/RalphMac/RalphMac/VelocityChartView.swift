/**
 VelocityChartView

 Responsibilities:
 - Render a bar chart showing tasks completed per day.
 - Uses SwiftUI Charts BarMark for visualization.
 */

import SwiftUI
import Charts
import RalphCore

struct VelocityChartView: View {
    let history: HistoryReport?
    
    private var dailyCompletions: [(date: String, count: Int)] {
        guard let history = history else { return [] }
        return history.days.map { day in
            (date: day.date, count: day.completed.count)
        }
    }
    
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Daily Velocity")
                .font(.headline)
                .padding(.horizontal)
                .padding(.top)
            
            let data = dailyCompletions
            if !data.isEmpty {
                Chart(data, id: \.date) { item in
                    BarMark(
                        x: .value("Date", formatDate(item.date)),
                        y: .value("Completed", item.count)
                    )
                    .foregroundStyle(item.count > 0 ? Color.green : Color.gray.opacity(0.3))
                    .clipShape(.rect(cornerRadius: 4))
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
                .accessibilityLabel("Daily velocity chart")
                .accessibilityHint("Bar chart showing tasks completed per day")
                .accessibilityHidden(true)
                
                // Accessible alternative for VoiceOver
                .accessibilityElement(children: .combine)
                .accessibilityLabel(velocityAccessibilityText(data: data))
            } else {
                emptyStateView(message: "No velocity data available")
            }
        }
    }
    
    private func velocityAccessibilityText(data: [(date: String, count: Int)]) -> String {
        let total = data.reduce(0) { $0 + $1.count }
        let daysWithActivity = data.filter { $0.count > 0 }.count
        return "Daily velocity: \(total) tasks completed across \(daysWithActivity) active days."
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
