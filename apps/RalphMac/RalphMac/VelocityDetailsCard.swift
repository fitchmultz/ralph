/**
 VelocityDetailsCard

 Purpose:
 - Show detailed velocity metrics with best day and average.

 Responsibilities:
 - Show detailed velocity metrics with best day and average.

 Scope:
 - Limited to the responsibilities listed above.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import SwiftUI
import RalphCore

struct VelocityDetailsCard: View {
    let velocity: ProductivityVelocityReport?
    
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Velocity Details")
                .font(.headline)
            
            if let velocity = velocity {
                VStack(spacing: 10) {
                    DetailRow(label: "Window", value: "\(velocity.windowDays) days")
                    DetailRow(label: "Total Done", value: "\(velocity.totalCompleted)")
                    DetailRow(label: "Average/Day", value: String(format: "%.1f", velocity.averagePerDay))
                    
                    if let bestDay = velocity.bestDay {
                        DetailRow(
                            label: "Best Day",
                            value: "\(bestDay.date) (\(bestDay.count))"
                        )
                    }
                }
            } else {
                Text("No velocity data")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding()
        .background(.quaternary.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

struct DetailRow: View {
    let label: String
    let value: String
    
    var body: some View {
        HStack {
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .font(.caption)
                .font(.body.weight(.medium))
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(label): \(value)")
    }
}
