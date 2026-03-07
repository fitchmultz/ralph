//! Workspace+Analytics
//!
//! Responsibilities:
//! - Load analytics data for dashboard surfaces.
//! - Prefer the aggregated dashboard endpoint and fall back to legacy per-command loads.
//! - Decode analytics payloads into workspace state.
//!
//! Does not handle:
//! - Analytics view rendering.
//! - Queue mutations or task presentation.
//! - Graph loading.
//!
//! Invariants/assumptions callers must respect:
//! - Aggregated dashboard loading is the preferred fast path.
//! - Partial analytics availability is acceptable.
//! - Failures degrade gracefully to nil section data.

import Foundation

public extension Workspace {
    func loadAnalytics(timeRange: TimeRange = .sevenDays) async {
        guard let client else {
            analyticsErrorMessage = "CLI client not available."
            return
        }

        analyticsLoading = true
        analyticsErrorMessage = nil

        let days = timeRange.days ?? 30

        if let dashboard = await loadDashboardAggregated(client: client, days: days) {
            var newData = AnalyticsData()

            if dashboard.sections.productivitySummary.status == .ok,
                let data = dashboard.sections.productivitySummary.data {
                newData.productivitySummary = data
            }
            if dashboard.sections.productivityVelocity.status == .ok,
                let data = dashboard.sections.productivityVelocity.data {
                newData.velocity = data
            }
            if dashboard.sections.burndown.status == .ok,
                let data = dashboard.sections.burndown.data {
                newData.burndown = data
            }
            if dashboard.sections.queueStats.status == .ok,
                let data = dashboard.sections.queueStats.data {
                newData.queueStats = data
            }
            if dashboard.sections.history.status == .ok,
                let data = dashboard.sections.history.data {
                newData.history = data
            }

            analyticsData = newData
            analyticsLoading = false
            return
        }

        async let summaryTask = loadProductivitySummary(client: client)
        async let velocityTask = loadVelocity(client: client, days: days)
        async let burndownTask = loadBurndown(client: client, days: days)
        async let statsTask = loadQueueStats(client: client)
        async let historyTask = loadHistory(client: client, days: days)

        let (summary, velocity, burndown, stats, history) = await (
            summaryTask,
            velocityTask,
            burndownTask,
            statsTask,
            historyTask
        )

        var newData = AnalyticsData()
        newData.productivitySummary = summary
        newData.velocity = velocity
        newData.burndown = burndown
        newData.queueStats = stats
        newData.history = history

        analyticsData = newData
        analyticsLoading = false
    }
}

private extension Workspace {
    func loadDashboardAggregated(client: RalphCLIClient, days: Int) async -> DashboardReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "dashboard", "--days", String(days)],
                        currentDirectoryURL: workingDirectoryURL
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

    func loadProductivitySummary(client: RalphCLIClient) async -> ProductivitySummaryReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "productivity", "summary", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            return try JSONDecoder().decode(ProductivitySummaryReport.self, from: Data(collected.stdout.utf8))
        } catch {
            return nil
        }
    }

    func loadVelocity(client: RalphCLIClient, days: Int) async -> ProductivityVelocityReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "productivity", "velocity", "--format", "json", "--days", String(days)],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            return try JSONDecoder().decode(ProductivityVelocityReport.self, from: Data(collected.stdout.utf8))
        } catch {
            return nil
        }
    }

    func loadBurndown(client: RalphCLIClient, days: Int) async -> BurndownReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "burndown", "--format", "json", "--days", String(days)],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            return try JSONDecoder().decode(BurndownReport.self, from: Data(collected.stdout.utf8))
        } catch {
            return nil
        }
    }

    func loadQueueStats(client: RalphCLIClient) async -> QueueStatsReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "stats", "--format", "json"],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            return try JSONDecoder().decode(QueueStatsReport.self, from: Data(collected.stdout.utf8))
        } catch {
            return nil
        }
    }

    func loadHistory(client: RalphCLIClient, days: Int) async -> HistoryReport? {
        let helper = RetryHelper(configuration: .minimal)
        do {
            let collected = try await helper.execute(
                operation: { [self] in
                    let result = try await client.runAndCollect(
                        arguments: ["--no-color", "queue", "history", "--format", "json", "--days", String(days)],
                        currentDirectoryURL: workingDirectoryURL
                    )
                    if result.status.code != 0 {
                        throw result.toError()
                    }
                    return result
                }
            )
            guard collected.status.code == 0 else { return nil }
            return try JSONDecoder().decode(HistoryReport.self, from: Data(collected.stdout.utf8))
        } catch {
            return nil
        }
    }
}
