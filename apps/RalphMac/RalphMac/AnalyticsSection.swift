/**
 AnalyticsSection

 Responsibilities:
 - Render the analytics detail column using explicit per-section analytics states.
 - Distinguish idle, loading, empty, failed, and content states for productivity summary content.
 - Keep reusable analytics empty/error cards out of the larger dashboard view.

 Does not handle:
 - Data fetching or refresh orchestration.
 - Chart rendering.
 - Queue mutations.

 Invariants/assumptions callers must respect:
 - Workspace analytics state is the single source of truth for dashboard/column rendering.
 - Empty summary content is a successful load with no meaningful data, not a transport failure.
 */

import SwiftUI
import RalphCore

@MainActor
struct AnalyticsDetailColumn: View {
    @ObservedObject var workspace: Workspace
    let navTitle: (String) -> String

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                detailContent
            }
            .padding(20)
        }
        .background(.clear)
        .navigationTitle(navTitle("Analytics"))
    }

    @ViewBuilder
    private var detailContent: some View {
        switch workspace.insightsState.analytics.productivitySummaryRenderState {
        case .idle(let message):
            AnalyticsStatusCard(
                title: "Analytics Not Loaded",
                message: message,
                systemImage: "chart.bar"
            )
        case .loading(_, let hasPreviousData):
            if let report = workspace.insightsState.analytics.productivitySummaryValue, hasPreviousData {
                productivityContent(report: report, showsStaleBadge: true)
            } else {
                AnalyticsStatusCard(
                    title: "Loading Analytics",
                    message: "Fetching productivity summary for the selected time range.",
                    systemImage: "hourglass"
                )
            }
        case .failed(let message, let hasPreviousData):
            if let report = workspace.insightsState.analytics.productivitySummaryValue, hasPreviousData {
                productivityContent(report: report, showsStaleBadge: true)
                AnalyticsStatusCard(
                    title: "Summary Load Failed",
                    message: message,
                    systemImage: "exclamationmark.triangle"
                )
            } else {
                AnalyticsStatusCard(
                    title: "Summary Load Failed",
                    message: message,
                    systemImage: "exclamationmark.triangle"
                )
            }
        case .empty(let message):
            AnalyticsStatusCard(
                title: "No Analytics Data",
                message: message,
                systemImage: "chart.bar"
            )
        case .content:
            if let report = workspace.insightsState.analytics.productivitySummaryValue {
                productivityContent(report: report, showsStaleBadge: false)
            }
        }
    }

    @ViewBuilder
    private func productivityContent(
        report: ProductivitySummaryReport,
        showsStaleBadge: Bool
    ) -> some View {
        GlassGroupBox(title: "Productivity Summary") {
            VStack(alignment: .leading, spacing: 12) {
                if showsStaleBadge {
                    Text("Showing previous successful load")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                VStack(alignment: .leading, spacing: 8) {
                    AnalyticsDetailRow(label: "Total Completed", value: "\(report.totalCompleted)")
                    AnalyticsDetailRow(label: "Current Streak", value: "\(report.currentStreak) days")
                    AnalyticsDetailRow(label: "Longest Streak", value: "\(report.longestStreak) days")

                    if let nextMilestone = report.nextMilestone {
                        AnalyticsDetailRow(label: "Next Milestone", value: "\(nextMilestone) tasks")
                    }
                }
            }
        }

        if !report.milestones.isEmpty {
            GlassGroupBox(title: "Milestones Achieved") {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(Array(report.milestones.prefix(5).enumerated()), id: \.offset) { _, milestone in
                        HStack {
                            Image(systemName: milestone.celebrated ? "checkmark.circle.fill" : "circle")
                                .foregroundStyle(milestone.celebrated ? .green : .secondary)
                            Text("\(milestone.threshold) tasks")
                            Spacer()
                            Text(String(milestone.achievedAt.prefix(10)))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .font(.caption)
                    }
                }
            }
        }
    }
}

@MainActor
struct AnalyticsStatusCard: View {
    let title: String
    let message: String
    let systemImage: String

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: systemImage)
                .font(.system(size: 40))
                .foregroundStyle(.secondary)

            Text(title)
                .font(.headline)

            Text(message)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 320)
        }
        .frame(maxWidth: .infinity, minHeight: 220)
        .padding()
        .background(.clear)
    }
}

@MainActor
struct AnalyticsDetailRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .font(.system(.body, design: .monospaced))
        }
    }
}
