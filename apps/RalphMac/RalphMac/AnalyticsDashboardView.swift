/**
 AnalyticsDashboardView

 Responsibilities:
 - Display productivity metrics and queue statistics using SwiftUI Charts.
 - Show burndown charts, velocity metrics, tag breakdowns, and completion history.
 - Provide time range selection (7d, 30d, 90d, all time).

 Does not handle:
 - Data loading (see Workspace.swift).
 - Direct CLI calls.

 Invariants/assumptions callers must respect:
 - Requires macOS 13+ for SwiftUI Charts support.
 - Workspace must be injected with loaded analytics data.
 */

import SwiftUI
import Charts
import RalphCore

@MainActor
struct AnalyticsDashboardView: View {
    @ObservedObject var workspace: Workspace
    @State private var selectedTimeRange: TimeRange = .sevenDays
    @State private var selectedChart: ChartType? = .burndown
    
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
            case .history: return "chart.line"
            }
        }
    }
    
    var body: some View {
        VStack(spacing: 0) {
            // Header with time range picker
            headerView()
                .padding()
            
            Divider()
            
            // Scrollable chart content
            ScrollView {
                VStack(spacing: 20) {
                    // Key Metrics Row
                    metricsRow()
                        .padding(.horizontal)
                    
                    // Chart Selection
                    chartSelectionView()
                        .padding(.horizontal)
                    
                    // Main Chart
                    mainChartView()
                        .padding(.horizontal)
                    
                    // Secondary Metrics
                    secondaryMetricsView()
                        .padding(.horizontal)
                        .padding(.bottom)
                }
                .padding(.vertical)
            }
        }
        .onAppear {
            let range = selectedTimeRange
            Task { @MainActor in
                await workspace.loadAnalytics(timeRange: range)
            }
        }
        .onChange(of: selectedTimeRange) { _, newRange in
            Task { @MainActor in
                await workspace.loadAnalytics(timeRange: newRange)
            }
        }
    }
    
    // MARK: - Header View
    
    @ViewBuilder
    private func headerView() -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("Analytics Dashboard")
                    .font(.title2)
                    .font(.body.weight(.semibold))
                
                Text("Track your productivity and task metrics")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            
            Spacer()
            
            // Time Range Picker
            Picker("Time Range", selection: $selectedTimeRange) {
                ForEach(TimeRange.allCases) { range in
                    Text(range.displayName).tag(range)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 280)
            .accessibilityLabel("Time range")
            .accessibilityHint("Select the time period for analytics data")
            
            // Refresh Button
            Button(action: {
                let range = selectedTimeRange
                Task { @MainActor in
                    await workspace.loadAnalytics(timeRange: range)
                }
            }) {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.borderless)
            .disabled(workspace.analyticsLoading)
            .accessibilityLabel("Refresh analytics")
            .accessibilityHint("Reload analytics data for selected time range")
        }
    }
    
    // MARK: - Metrics Row
    
    @ViewBuilder
    private func metricsRow() -> some View {
        LazyVGrid(columns: [
            GridItem(.flexible()),
            GridItem(.flexible()),
            GridItem(.flexible()),
            GridItem(.flexible())
        ], spacing: 16) {
            MetricCard(
                title: "Total Completed",
                value: String(workspace.analyticsData.productivitySummary?.totalCompleted ?? 0),
                icon: "checkmark.circle.fill",
                color: .green
            )
            .accessibilityLabel("Total Completed: \(String(workspace.analyticsData.productivitySummary?.totalCompleted ?? 0))")
            
            MetricCard(
                title: "Current Streak",
                value: "\(workspace.analyticsData.productivitySummary?.currentStreak ?? 0) days",
                icon: "flame.fill",
                color: .orange
            )
            .accessibilityLabel("Current Streak: \(workspace.analyticsData.productivitySummary?.currentStreak ?? 0) days")
            
            MetricCard(
                title: "Completion Rate",
                value: String(format: "%.1f%%", workspace.analyticsData.queueStats?.summary.terminalRate ?? 0),
                icon: "percent",
                color: .blue
            )
            .accessibilityLabel("Completion Rate: \(String(format: "%.1f%%", workspace.analyticsData.queueStats?.summary.terminalRate ?? 0))")
            
            MetricCard(
                title: "Active Tasks",
                value: String(workspace.analyticsData.queueStats?.summary.active ?? 0),
                icon: "list.bullet",
                color: .purple
            )
            .accessibilityLabel("Active Tasks: \(String(workspace.analyticsData.queueStats?.summary.active ?? 0))")
        }
    }
    
    // MARK: - Chart Selection
    
    @ViewBuilder
    private func chartSelectionView() -> some View {
        Picker("Chart", selection: $selectedChart) {
            ForEach(ChartType.allCases) { type in
                Label(type.rawValue, systemImage: type.icon).tag(type as ChartType?)
            }
        }
        .pickerStyle(.segmented)
        .accessibilityLabel("Chart type")
        .accessibilityHint("Select the type of chart to display")
    }
    
    // MARK: - Main Chart View
    
    @ViewBuilder
    private func mainChartView() -> some View {
        Group {
            switch selectedChart {
            case .burndown:
                BurndownChartView(burndown: workspace.analyticsData.burndown)
            case .velocity:
                VelocityChartView(history: workspace.analyticsData.history)
            case .tags:
                TagBreakdownChart(tagBreakdown: workspace.analyticsData.queueStats?.tagBreakdown ?? [])
            case .history:
                CompletionHistoryChart(history: workspace.analyticsData.history)
            case .none:
                EmptyView()
            }
        }
        .frame(height: 300)
        .background(.quaternary.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: 12))
    }
    
    // MARK: - Secondary Metrics
    
    @ViewBuilder
    private func secondaryMetricsView() -> some View {
        HStack(alignment: .top, spacing: 20) {
            // Priority Distribution
            PriorityDistributionCard(tasks: workspace.tasks)
                .frame(maxWidth: .infinity)
            
            // Task Aging
            TaskAgingCard(tasks: workspace.tasks)
                .frame(maxWidth: .infinity)
            
            // Velocity Details
            VelocityDetailsCard(velocity: workspace.analyticsData.velocity)
                .frame(maxWidth: .infinity)
        }
    }
}

// MARK: - Metric Card

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
        .background(.quaternary.opacity(0.15))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .accessibilityElement(children: .combine)
    }
}
