//! Workspace+Analytics
//!
//! Responsibilities:
//! - Load analytics data for dashboard surfaces.
//! - Decode the aggregated dashboard endpoint into per-section dashboard state.
//! - Preserve explicit per-section loading, empty, and failure states for analytics consumers.
//!
//! Does not handle:
//! - Analytics view rendering.
//! - Queue mutations or task presentation.
//! - Graph loading.
//!
//! Invariants/assumptions callers must respect:
//! - Analytics state is derived from the dashboard endpoint only.
//! - Each analytics section owns its own lifecycle instead of sharing a global error bit.
//! - Failed loads should retain stale data only when the caller has previous section content.

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

public extension Workspace {
    func loadAnalytics(timeRange: TimeRange = .sevenDays) async {
        let repositoryContext = currentRepositoryContext()
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
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            insightsState.analytics = analyticsFailureState(
                previous: previousState,
                timeRange: timeRange,
                message: "CLI client not available."
            )
            return
        }

        let days = timeRange.days ?? 30

        do {
            let dashboard = try await loadDashboard(client: client, days: days)
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            insightsState.analytics = analyticsState(
                from: dashboard,
                timeRange: timeRange,
                previous: previousState
            )
        } catch {
            guard isCurrentRepositoryContext(repositoryContext) else { return }
            insightsState.analytics = analyticsFailureState(
                previous: previousState,
                timeRange: timeRange,
                message: "Failed to load dashboard analytics."
            )
        }
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

    func loadDashboard(client: RalphCLIClient, days: Int) async throws -> DashboardReport {
        let document = try await self.decodeMachineRepositoryJSON(
            MachineDashboardReadDocument.self,
            client: client,
            machineArguments: ["queue", "dashboard", "--days", String(days)],
            currentDirectoryURL: identityState.workingDirectoryURL,
            retryConfiguration: .minimal
        )
        return document.dashboard
    }
}
