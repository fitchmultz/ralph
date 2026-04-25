/**
 AnalyticsDashboardView

 Purpose:
 - Display productivity metrics and analytics charts using per-section workspace state.

 Responsibilities:
 - Display productivity metrics and analytics charts using per-section workspace state.
 - Trigger analytics refreshes when the selected time range changes.
 - Surface stale-data, empty-state, and section-failure messaging inline.

 Does not handle:
 - CLI execution.
 - Analytics state modeling.
 - Queue mutation.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Workspace analytics state is updated through `Workspace.loadAnalytics`.
 - Chart views receive only loaded data; failure/empty states are handled here.
 - Requires macOS 13+ for SwiftUI Charts support.
 */

import SwiftUI
import Charts
import RalphCore

@MainActor
struct AnalyticsDashboardView: View {
    @ObservedObject var workspace: Workspace
    @State private var selectedChart: ChartType = .burndown

    enum ChartType: String, CaseIterable, Identifiable {
        case burndown = "Burndown"
        case velocity = "Velocity"
        case tags = "Tags"
        case history = "History"

        var id: String { rawValue }

        var icon: String {
            switch self {
            case .burndown: return "chart.line.uptrend.xyaxis"
            case .velocity: return "chart.bar"
            case .tags: return "chart.pie"
            case .history: return "chart.line.uptrend.xyaxis"
            }
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            header

            Divider()

            ScrollView {
                VStack(spacing: 20) {
                    metricsRow
                        .padding(.horizontal)

                    chartSelection
                        .padding(.horizontal)

                    mainChart
                        .padding(.horizontal)

                    secondaryMetrics
                        .padding(.horizontal)
                        .padding(.bottom)
                }
                .padding(.vertical)
            }
        }
        .task {
            await Task.yield()
            guard !workspace.insightsState.analytics.isLoading else { return }
            guard workspace.insightsState.analytics.lastRefreshedAt == nil else { return }
            await refreshAnalytics()
        }
        .onChange(of: workspace.insightsState.analytics.timeRange) { _, _ in
            Task { @MainActor in
                await refreshAnalytics()
            }
        }
    }

    private var selectedTimeRangeBinding: Binding<TimeRange> {
        Binding(
            get: { workspace.insightsState.analytics.timeRange },
            set: { workspace.insightsState.analytics.timeRange = $0 }
        )
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("Analytics Dashboard")
                    .font(.title2)
                    .fontWeight(.semibold)

                Text("Track productivity, queue health, and failure modes by section.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Picker("Time Range", selection: selectedTimeRangeBinding) {
                ForEach(TimeRange.allCases) { range in
                    Text(range.displayName).tag(range)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 280)

            Button {
                Task { @MainActor in
                    await refreshAnalytics()
                }
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.borderless)
            .disabled(workspace.insightsState.analytics.isLoading)
            .accessibilityLabel("Refresh analytics")
        }
        .padding()
    }

    private var metricsRow: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible()), count: 4), spacing: 16) {
            metricCard(
                title: "Total Completed",
                value: workspace.insightsState.analytics.productivitySummaryValue.map { String($0.totalCompleted) } ?? placeholderValue(for: workspace.insightsState.analytics.productivitySummaryRenderState),
                icon: "checkmark.circle.fill",
                color: .green,
                renderState: workspace.insightsState.analytics.productivitySummaryRenderState
            )

            metricCard(
                title: "Current Streak",
                value: workspace.insightsState.analytics.productivitySummaryValue.map { "\($0.currentStreak) days" } ?? placeholderValue(for: workspace.insightsState.analytics.productivitySummaryRenderState),
                icon: "flame.fill",
                color: .orange,
                renderState: workspace.insightsState.analytics.productivitySummaryRenderState
            )

            metricCard(
                title: "Completion Rate",
                value: workspace.insightsState.analytics.queueStatsValue.map { String(format: "%.1f%%", $0.summary.terminalRate) } ?? placeholderValue(for: workspace.insightsState.analytics.queueStatsRenderState),
                icon: "percent",
                color: .blue,
                renderState: workspace.insightsState.analytics.queueStatsRenderState
            )

            metricCard(
                title: "Active Tasks",
                value: workspace.insightsState.analytics.queueStatsValue.map { String($0.summary.active) } ?? placeholderValue(for: workspace.insightsState.analytics.queueStatsRenderState),
                icon: "list.bullet",
                color: .purple,
                renderState: workspace.insightsState.analytics.queueStatsRenderState
            )
        }
    }

    private var chartSelection: some View {
        Picker("Chart", selection: $selectedChart) {
            ForEach(ChartType.allCases) { type in
                Label(type.rawValue, systemImage: type.icon).tag(type)
            }
        }
        .pickerStyle(.segmented)
    }

    private var mainChart: some View {
        chartContainer(for: selectedChart)
            .frame(height: 300)
            .background(.quaternary.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 12))
    }

    private var secondaryMetrics: some View {
        HStack(alignment: .top, spacing: 20) {
            PriorityDistributionCard(tasks: workspace.taskState.tasks)
                .frame(maxWidth: .infinity)

            TaskAgingCard(tasks: workspace.taskState.tasks)
                .frame(maxWidth: .infinity)

            velocityDetailsCard
                .frame(maxWidth: .infinity)
        }
    }

    @ViewBuilder
    private var velocityDetailsCard: some View {
        switch workspace.insightsState.analytics.productivityVelocityRenderState {
        case .content:
            VelocityDetailsCard(velocity: workspace.insightsState.analytics.productivityVelocityValue)
        case .loading(_, let hasPreviousData):
            if hasPreviousData {
                VelocityDetailsCard(velocity: workspace.insightsState.analytics.productivityVelocityValue)
            } else {
                AnalyticsStatusCard(title: "Loading Velocity", message: "Fetching velocity analytics.", systemImage: "hourglass")
            }
        case .idle(let message):
            AnalyticsStatusCard(title: "Velocity Not Loaded", message: message, systemImage: "chart.bar")
        case .empty(let message):
            AnalyticsStatusCard(title: "Velocity Empty", message: message, systemImage: "chart.bar")
        case .failed(let message, let hasPreviousData):
            if hasPreviousData {
                VStack(spacing: 12) {
                    VelocityDetailsCard(velocity: workspace.insightsState.analytics.productivityVelocityValue)
                    failureBanner(message: message)
                }
            } else {
                AnalyticsStatusCard(title: "Velocity Failed", message: message, systemImage: "exclamationmark.triangle")
            }
        }
    }

    @ViewBuilder
    private func chartContainer(for chart: ChartType) -> some View {
        switch chart {
        case .burndown:
            sectionContent(
                renderState: workspace.insightsState.analytics.burndownRenderState,
                loadedContent: {
                    BurndownChartView(burndown: workspace.insightsState.analytics.burndownValue)
                },
                staleContent: {
                    BurndownChartView(burndown: workspace.insightsState.analytics.burndownValue)
                },
                emptyTitle: "Burndown Empty",
                failedTitle: "Burndown Failed"
            )
        case .velocity:
            sectionContent(
                renderState: workspace.insightsState.analytics.historyRenderState,
                loadedContent: {
                    VelocityChartView(history: workspace.insightsState.analytics.historyValue)
                },
                staleContent: {
                    VelocityChartView(history: workspace.insightsState.analytics.historyValue)
                },
                emptyTitle: "Velocity Empty",
                failedTitle: "Velocity Failed"
            )
        case .tags:
            sectionContent(
                renderState: workspace.insightsState.analytics.queueStatsRenderState,
                loadedContent: {
                    TagBreakdownChart(tagBreakdown: workspace.insightsState.analytics.queueStatsValue?.tagBreakdown ?? [])
                },
                staleContent: {
                    TagBreakdownChart(tagBreakdown: workspace.insightsState.analytics.queueStatsValue?.tagBreakdown ?? [])
                },
                emptyTitle: "Tag Breakdown Empty",
                failedTitle: "Tag Breakdown Failed"
            )
        case .history:
            sectionContent(
                renderState: workspace.insightsState.analytics.historyRenderState,
                loadedContent: {
                    CompletionHistoryChart(history: workspace.insightsState.analytics.historyValue)
                },
                staleContent: {
                    CompletionHistoryChart(history: workspace.insightsState.analytics.historyValue)
                },
                emptyTitle: "History Empty",
                failedTitle: "History Failed"
            )
        }
    }

    @ViewBuilder
    private func sectionContent<Loaded: View, Stale: View>(
        renderState: AnalyticsRenderableState,
        loadedContent: () -> Loaded,
        staleContent: () -> Stale,
        emptyTitle: String,
        failedTitle: String
    ) -> some View {
        switch renderState {
        case .content:
            loadedContent()
        case .loading(_, let hasPreviousData):
            if hasPreviousData {
                VStack(spacing: 12) {
                    staleContent()
                    Text("Refreshing analytics...")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            } else {
                AnalyticsStatusCard(title: "Loading Analytics", message: "Fetching section data for the selected time range.", systemImage: "hourglass")
            }
        case .idle(let message):
            AnalyticsStatusCard(title: "Analytics Not Loaded", message: message, systemImage: "chart.bar")
        case .empty(let message):
            AnalyticsStatusCard(title: emptyTitle, message: message, systemImage: "chart.bar")
        case .failed(let message, let hasPreviousData):
            if hasPreviousData {
                VStack(spacing: 12) {
                    staleContent()
                    failureBanner(message: message)
                }
            } else {
                AnalyticsStatusCard(title: failedTitle, message: message, systemImage: "exclamationmark.triangle")
            }
        }
    }

    private func metricCard(
        title: String,
        value: String,
        icon: String,
        color: Color,
        renderState: AnalyticsRenderableState
    ) -> some View {
        MetricCard(title: title, value: value, icon: icon, color: color)
            .overlay(alignment: .topTrailing) {
                switch renderState {
                case .failed:
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                        .padding(10)
                case .loading:
                    Image(systemName: "arrow.triangle.2.circlepath")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .symbolEffect(.rotate, isActive: true)
                        .frame(width: 16, height: 16)
                        .padding(10)
                default:
                    EmptyView()
                }
            }
    }

    private func failureBanner(message: String) -> some View {
        Text(message)
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.orange.opacity(0.12))
            .clipShape(.rect(cornerRadius: 8))
    }

    private func placeholderValue(for state: AnalyticsRenderableState) -> String {
        switch state {
        case .content:
            return "0"
        case .loading:
            return "..."
        case .idle, .empty:
            return "No data"
        case .failed:
            return "Failed"
        }
    }

    private func refreshAnalytics() async {
        await workspace.loadAnalytics(timeRange: workspace.insightsState.analytics.timeRange)
    }
}

struct MetricCard: View {
    let title: String
    let value: String
    let icon: String
    let color: Color

    var body: some View {
        VStack(spacing: 12) {
            HStack {
                Image(systemName: icon)
                    .foregroundStyle(color)
                    .font(.title3)
                Spacer()
            }

            VStack(alignment: .leading, spacing: 4) {
                Text(value)
                    .font(.title2)
                    .bold()
                Text(title)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding()
        .background(.quaternary.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 12))
    }
}
