/**
 TaskAgingCard

 Purpose:
 - Show task aging distribution with visual indicators.

 Responsibilities:
 - Show task aging distribution with visual indicators.
 - Categories: Fresh, Warning, Stale, Rotten

 Scope:
 - Limited to the responsibilities listed above.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import SwiftUI
import RalphCore

struct TaskAgingCard: View {
    let tasks: [RalphTask]
    
    private var agingCounts: (fresh: Int, warning: Int, stale: Int, rotten: Int) {
        let now = Date()
        var fresh = 0, warning = 0, stale = 0, rotten = 0
        
        for task in tasks where task.status == .todo || task.status == .doing {
            guard let createdAt = task.createdAt else {
                fresh += 1
                continue
            }
            let days = Calendar.current.dateComponents([.day], from: createdAt, to: now).day ?? 0
            
            switch days {
            case 0...7: fresh += 1
            case 8...14: warning += 1
            case 15...30: stale += 1
            default: rotten += 1
            }
        }
        
        return (fresh, warning, stale, rotten)
    }
    
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Task Aging")
                .font(.headline)
            
            let counts = agingCounts
            VStack(spacing: 8) {
                AgingRow(label: "Fresh", count: counts.fresh, color: .green, threshold: "≤7d")
                    .accessibilityLabel("Fresh: \(counts.fresh) tasks, ≤7d")
                AgingRow(label: "Warning", count: counts.warning, color: .yellow, threshold: "8-14d")
                    .accessibilityLabel("Warning: \(counts.warning) tasks, 8-14d")
                AgingRow(label: "Stale", count: counts.stale, color: .orange, threshold: "15-30d")
                    .accessibilityLabel("Stale: \(counts.stale) tasks, 15-30d")
                AgingRow(label: "Rotten", count: counts.rotten, color: .red, threshold: ">30d")
                    .accessibilityLabel("Rotten: \(counts.rotten) tasks, >30d")
            }
        }
        .padding()
        .background(.quaternary.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

struct AgingRow: View {
    let label: String
    let count: Int
    let color: Color
    let threshold: String
    
    var body: some View {
        HStack {
            HStack(spacing: 6) {
                Circle()
                    .fill(color)
                    .frame(width: 8, height: 8)
                Text(label)
                    .font(.caption)
            }
            
            Spacer()
            
            Text(threshold)
                .font(.caption2)
                .foregroundStyle(.secondary)
            
            Text("\(count)")
                .font(.caption)
                .font(.body.weight(.semibold))
                .frame(width: 30, alignment: .trailing)
        }
        .accessibilityElement(children: .combine)
    }
}
