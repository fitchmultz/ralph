//! Workspace+Analytics
//!
//! Responsibilities:
//! - Load analytics data for dashboard surfaces.
//! - Prefer the aggregated dashboard endpoint and fall back to legacy per-command loads.
//! - Preserve explicit per-section loading, empty, and failure states for analytics consumers.
//!
//! Does not handle:
//! - Analytics view rendering.
//! - Queue mutations or task presentation.
//! - Graph loading.
//!
//! Invariants/assumptions callers must respect:
//! - Aggregated dashboard loading is the preferred fast path.
//! - Each analytics section owns its own lifecycle instead of sharing a global error bit.
//! - Fallback loading should retain stale data only when the caller has previous section content.

public import Foundation
public import Combine

@MainActor
public final class WorkspaceInsightsState: ObservableObject {
    @Published public var graphData: RalphGraphDocument?
    @Published public var graphDataLoading = false
    @Published public var graphDataErrorMessage: String?
    @Published public var analytics = AnalyticsDashboardState()

    public init() {}
}

private enum AnalyticsCLISectionFetch<Value: Sendable & Equatable> {
    case loaded(Value)
    case failed(String)
}

private struct LegacyAnalyticsFetchResult {
    let productivitySummary: AnalyticsCLISectionFetch<ProductivitySummaryReport>
    let productivityVelocity: AnalyticsCLISectionFetch<ProductivityVelocityReport>
    let burndown: AnalyticsCLISectionFetch<BurndownReport>
    let queueStats: AnalyticsCLISectionFetch<QueueStatsReport>
    let history: AnalyticsCLISectionFetch<HistoryReport>
}

public extension Workspace {
    func loadAnalytics(timeRange: TimeRange = .sevenDays) async {
        let previousState = insightsState.analytics
        insightsState.analytics = AnalyticsDashboardState(
            timeRange: timeRange,
            lastRefreshedAt: previousState.lastRefreshedAt,
            productivitySummary: .loading(previous: previousState.productivitySummaryValue),
            productivityVelocity: .loading(previous: previousState.productivityVelocityValue),
            burndown: .loading(previous: previousState.burndownValue),
            queueStats: .loading(previous: previousState.queueStatsValue),
            history: .loading(previous: previousState.historyValue)
        )

        guard let client else {
            insightsState.analytics = analyticsFailureState(
                previous: previousState,
                timeRange: timeRange,
                message: "CLI client not available."
            )
            return
        }

        let days = timeRange.days ?? 30

        if let dashboard = await loadDashboardAggregated(client: client, days: days) {
            insightsState.analytics = analyticsState(
                from: dashboard,
                timeRange: timeRange,
                previous: previousState
            )
            return
        }

        let legacyResult = await loadLegacyAnalytics(client: client, days: days)
        insightsState.analytics = analyticsState(
            from: legacyResult,
            timeRange: timeRange,
            previous: previousState
        )
    }
}

private extension Workspace {
    func analyticsFailureState(
        previous: AnalyticsDashboardState,
        timeRange: TimeRange,
        message: String
    ) -> AnalyticsDashboardState {
        AnalyticsDashboardState(
            timeRange: timeRange,
            lastRefreshedAt: previous.lastRefreshedAt,
            productivitySummary: .failed(message: message, previous: previous.productivitySummaryValue),
            productivityVelocity: .failed(message: message, previous: previous.productivityVelocityValue),
            burndown: .failed(message: message, previous: previous.burndownValue),
            queueStats: .failed(message: message, previous: previous.queueStatsValue),
            history: .failed(message: message, previous: previous.historyValue)
        )
    }

    func analyticsState(
        from dashboard: DashboardReport,
        timeRange: TimeRange,
        previous: AnalyticsDashboardState
    ) -> AnalyticsDashboardState {
        AnalyticsDashboardState(
            timeRange: timeRange,
            lastRefreshedAt: Date(),
            productivitySummary: sectionState(
                dashboard.sections.productivitySummary,
                kind: .productivitySummary,
                previous: previous.productivitySummaryValue
            ),
            productivityVelocity: sectionState(
                dashboard.sections.productivityVelocity,
                kind: .productivityVelocity,
                previous: previous.productivityVelocityValue
            ),
            burndown: sectionState(
                dashboard.sections.burndown,
                kind: .burndown,
                previous: previous.burndownValue
            ),
            queueStats: sectionState(
                dashboard.sections.queueStats,
                kind: .queueStats,
                previous: previous.queueStatsValue
            ),
            history: sectionState(
                dashboard.sections.history,
                kind: .history,
                previous: previous.historyValue
            )
        )
    }

    func analyticsState(
        from legacy: LegacyAnalyticsFetchResult,
        timeRange: TimeRange,
        previous: AnalyticsDashboardState
    ) -> AnalyticsDashboardState {
        AnalyticsDashboardState(
            timeRange: timeRange,
            lastRefreshedAt: Date(),
            productivitySummary: sectionState(
                legacy.productivitySummary,
                kind: .productivitySummary,
                previous: previous.productivitySummaryValue
            ),
            productivityVelocity: sectionState(
                legacy.productivityVelocity,
                kind: .productivityVelocity,
                previous: previous.productivityVelocityValue
            ),
            burndown: sectionState(
                legacy.burndown,
                kind: .burndown,
                previous: previous.burndownValue
            ),
            queueStats: sectionState(
                legacy.queueStats,
                kind: .queueStats,
                previous: previous.queueStatsValue
            ),
            history: sectionState(
                legacy.history,
                kind: .history,
                previous: previous.historyValue
            )
        )
    }

    func sectionState<Value: Sendable & Equatable>(
        _ result: SectionResult<Value>,
        kind: AnalyticsSectionKind,
        previous: Value?
    ) -> AnalyticsSectionLoadState<Value> {
        switch result.status {
        case .ok:
            if let data = result.data {
                return .loaded(data)
            }
            let message = result.errorMessage?.trimmingCharacters(in: .whitespacesAndNewlines)
            if let message, !message.isEmpty {
                return .failed(message: message, previous: previous)
            }
            return .empty(message: kind.emptyMessage)
        case .unavailable:
            let message = result.errorMessage?.trimmingCharacters(in: .whitespacesAndNewlines)
            return .failed(
                message: (message?.isEmpty == false ? message! : "Failed to load \(kind.displayName.lowercased())."),
                previous: previous
            )
        }
    }

    func sectionState<Value: Sendable & Equatable>(
        _ result: AnalyticsCLISectionFetch<Value>,
        kind: AnalyticsSectionKind,
        previous: Value?
    ) -> AnalyticsSectionLoadState<Value> {
        switch result {
        case .loaded(let value):
            return .loaded(value)
        case .failed(let message):
            return .failed(message: message, previous: previous)
        }
    }

    func loadLegacyAnalytics(client: RalphCLIClient, days: Int) async -> LegacyAnalyticsFetchResult {
        async let summaryTask = loadProductivitySummary(client: client)
        async let velocityTask = loadVelocity(client: client, days: days)
        async let burndownTask = loadBurndown(client: client, days: days)
        async let statsTask = loadQueueStats(client: client)
        async let historyTask = loadHistory(client: client, days: days)

        return await LegacyAnalyticsFetchResult(
            productivitySummary: summaryTask,
            productivityVelocity: velocityTask,
            burndown: burndownTask,
            queueStats: statsTask,
            history: historyTask
        )
    }

    func loadDashboardAggregated(client: RalphCLIClient, days: Int) async -> DashboardReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "dashboard", "--days", String(days)],
                        currentDirectoryURL: identityState.workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            return try JSONDecoder().decode(DashboardReport.self, from: Data(collected.stdout.utf8))
        } catch {
            return nil
        }
    }

    func loadProductivitySummary(client: RalphCLIClient) async -> AnalyticsCLISectionFetch<ProductivitySummaryReport> {
        await loadAnalyticsSection(
            kind: .productivitySummary,
            client: client,
            arguments: ["--no-color", "productivity", "summary", "--format", "json"],
            decode: ProductivitySummaryReport.self
        )
    }

    func loadVelocity(client: RalphCLIClient, days: Int) async -> AnalyticsCLISectionFetch<ProductivityVelocityReport> {
        await loadAnalyticsSection(
            kind: .productivityVelocity,
            client: client,
            arguments: ["--no-color", "productivity", "velocity", "--format", "json", "--days", String(days)],
            decode: ProductivityVelocityReport.self
        )
    }

    func loadBurndown(client: RalphCLIClient, days: Int) async -> AnalyticsCLISectionFetch<BurndownReport> {
        await loadAnalyticsSection(
            kind: .burndown,
            client: client,
            arguments: ["--no-color", "queue", "burndown", "--format", "json", "--days", String(days)],
            decode: BurndownReport.self
        )
    }

    func loadQueueStats(client: RalphCLIClient) async -> AnalyticsCLISectionFetch<QueueStatsReport> {
        await loadAnalyticsSection(
            kind: .queueStats,
            client: client,
            arguments: ["--no-color", "queue", "stats", "--format", "json"],
            decode: QueueStatsReport.self
        )
    }

    func loadHistory(client: RalphCLIClient, days: Int) async -> AnalyticsCLISectionFetch<HistoryReport> {
        await loadAnalyticsSection(
            kind: .history,
            client: client,
            arguments: ["--no-color", "queue", "history", "--format", "json", "--days", String(days)],
            decode: HistoryReport.self
        )
    }

    func loadAnalyticsSection<Value: Decodable & Sendable & Equatable>(
        kind: AnalyticsSectionKind,
        client: RalphCLIClient,
        arguments: [String],
        decode type: Value.Type
    ) async -> AnalyticsCLISectionFetch<Value> {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: arguments,
                        currentDirectoryURL: identityState.workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )

            guard collected.status.code == 0 else {
                let message = collected.stderr.trimmingCharacters(in: CharacterSet.whitespacesAndNewlines)
                return .failed(
                    message.isEmpty
                        ? "Failed to load \(kind.displayName.lowercased()) (exit \(collected.status.code))."
                        : message
                )
            }

            do {
                let value = try JSONDecoder().decode(type, from: Data(collected.stdout.utf8))
                return .loaded(value)
            } catch {
                return .failed("Failed to decode \(kind.displayName.lowercased()): \(error.localizedDescription)")
            }
        } catch {
            return .failed(error.localizedDescription)
        }
    }
}
