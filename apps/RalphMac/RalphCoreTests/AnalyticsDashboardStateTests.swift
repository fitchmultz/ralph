/**
 AnalyticsDashboardStateTests

 Responsibilities:
 - Verify analytics section state renders idle/loading/empty/failed/content correctly.
 - Protect stale-data behavior when refreshes fail after a successful load.
 - Ensure empty successful reports do not masquerade as transport failures.
 */

import Foundation
import XCTest

@testable import RalphCore

final class AnalyticsDashboardStateTests: RalphCoreTestCase {
    func testProductivitySummaryRenderStateTreatsZeroReportAsEmpty() {
        let report = ProductivitySummaryReport(
            totalCompleted: 0,
            currentStreak: 0,
            longestStreak: 0,
            lastCompletedDate: nil,
            nextMilestone: nil,
            milestones: [],
            recentCompletions: []
        )

        let state = AnalyticsDashboardState(
            productivitySummary: .loaded(report)
        )

        XCTAssertEqual(
            state.productivitySummaryRenderState,
            .empty(message: AnalyticsSectionKind.productivitySummary.emptyMessage)
        )
    }

    func testHistoryFailureRetainsPreviousDataForRendering() {
        let report = HistoryReport(
            window: HistoryWindow(days: 7, startDate: "2026-03-01", endDate: "2026-03-07"),
            days: [
                HistoryDay(date: "2026-03-07", created: ["RQ-1"], completed: ["RQ-1"])
            ]
        )

        let state = AnalyticsDashboardState(
            history: .failed(message: "decode failed", previous: report)
        )

        XCTAssertEqual(
            state.historyRenderState,
            .failed(message: "decode failed", hasPreviousData: true)
        )
        XCTAssertEqual(state.historyValue, report)
    }

    func testLoadingAllSectionsPreservesPreviousValues() {
        let velocity = ProductivityVelocityReport(
            windowDays: 30,
            totalCompleted: 4,
            averagePerDay: 0.13,
            bestDay: BestDay(date: "2026-03-05", count: 2)
        )

        let state = AnalyticsDashboardState(
            productivityVelocity: .loaded(velocity)
        )
        let loading = state.loadingAllSections()

        XCTAssertEqual(
            loading.productivityVelocity,
            .loading(previous: velocity)
        )
        XCTAssertTrue(loading.isLoading)
    }

    func testQueueStatsRenderStateTreatsNonEmptyQueueAsContent() {
        let report = QueueStatsReport(
            summary: StatsSummary(
                total: 3,
                done: 1,
                rejected: 0,
                terminal: 1,
                active: 2,
                terminalRate: 33.3
            ),
            tagBreakdown: []
        )

        let state = AnalyticsDashboardState(queueStats: .loaded(report))

        XCTAssertEqual(state.queueStatsRenderState, .content)
    }
}
