/**
 AnalyticsModels

 Responsibilities:
 - Define Codable models for parsing analytics JSON from `ralph productivity` and `ralph queue` commands.
 - Model per-section analytics loading state so the UI can distinguish loading, empty, and failed sections.
 - Provide render-focused helpers that classify loaded reports as content-bearing or data-empty.

 Does not handle:
 - CLI execution (see `Workspace+Analytics.swift`).
 - SwiftUI view composition (see `AnalyticsDashboardView.swift` and `AnalyticsSection.swift`).

 Invariants/assumptions callers must respect:
 - JSON structure matches CLI output with `--format json`.
 - Empty analytics reports are represented as successful loads with no meaningful data, not as failures.
 - Section failure states should preserve prior values only when the caller explicitly carries them forward.
 */

public import Foundation

public enum AnalyticsSectionKind: String, CaseIterable, Identifiable, Sendable {
    case productivitySummary
    case productivityVelocity
    case burndown
    case queueStats
    case history

    public var id: String { rawValue }

    public var displayName: String {
        switch self {
        case .productivitySummary: return "Productivity Summary"
        case .productivityVelocity: return "Velocity"
        case .burndown: return "Burndown"
        case .queueStats: return "Queue Stats"
        case .history: return "History"
        }
    }

    public var emptyMessage: String {
        switch self {
        case .productivitySummary:
            return "No completed-task analytics are available for the selected range."
        case .productivityVelocity:
            return "Velocity is empty because there are no completed tasks in the selected range."
        case .burndown:
            return "Burndown data is empty for the selected range."
        case .queueStats:
            return "Queue stats are empty because the queue has no tracked tasks yet."
        case .history:
            return "History is empty because there are no created or completed tasks in the selected range."
        }
    }

    public var idleMessage: String {
        "Select a time range and refresh to load \(displayName.lowercased())."
    }
}

public enum AnalyticsRenderableState: Sendable, Equatable {
    case idle(message: String)
    case loading(message: String, hasPreviousData: Bool)
    case content
    case empty(message: String)
    case failed(message: String, hasPreviousData: Bool)
}

public enum AnalyticsSectionLoadState<Value: Sendable & Equatable>: Sendable, Equatable {
    case idle
    case loading(previous: Value?)
    case loaded(Value)
    case empty(message: String)
    case failed(message: String, previous: Value?)

    public var currentValue: Value? {
        switch self {
        case .idle, .empty:
            return nil
        case .loading(let previous), .failed(_, let previous):
            return previous
        case .loaded(let value):
            return value
        }
    }

    public var isLoading: Bool {
        if case .loading = self {
            return true
        }
        return false
    }

    public var failureMessage: String? {
        if case .failed(let message, _) = self {
            return message
        }
        return nil
    }

    public var emptyMessage: String? {
        if case .empty(let message) = self {
            return message
        }
        return nil
    }

    public var hasPreviousData: Bool {
        switch self {
        case .loading(let previous), .failed(_, let previous):
            return previous != nil
        case .loaded:
            return true
        case .idle, .empty:
            return false
        }
    }

    public func renderState(
        kind: AnalyticsSectionKind,
        isMeaningful: (Value) -> Bool
    ) -> AnalyticsRenderableState {
        switch self {
        case .idle:
            return .idle(message: kind.idleMessage)
        case .loading(let previous):
            return .loading(
                message: "Loading \(kind.displayName.lowercased())...",
                hasPreviousData: previous != nil
            )
        case .loaded(let value):
            return isMeaningful(value) ? .content : .empty(message: kind.emptyMessage)
        case .empty(let message):
            return .empty(message: message)
        case .failed(let message, let previous):
            return .failed(message: message, hasPreviousData: previous != nil)
        }
    }
}

public struct AnalyticsDashboardState: Sendable, Equatable {
    public var timeRange: TimeRange
    public var lastRefreshedAt: Date?
    public var productivitySummary: AnalyticsSectionLoadState<ProductivitySummaryReport>
    public var productivityVelocity: AnalyticsSectionLoadState<ProductivityVelocityReport>
    public var burndown: AnalyticsSectionLoadState<BurndownReport>
    public var queueStats: AnalyticsSectionLoadState<QueueStatsReport>
    public var history: AnalyticsSectionLoadState<HistoryReport>

    public init(
        timeRange: TimeRange = .sevenDays,
        lastRefreshedAt: Date? = nil,
        productivitySummary: AnalyticsSectionLoadState<ProductivitySummaryReport> = .idle,
        productivityVelocity: AnalyticsSectionLoadState<ProductivityVelocityReport> = .idle,
        burndown: AnalyticsSectionLoadState<BurndownReport> = .idle,
        queueStats: AnalyticsSectionLoadState<QueueStatsReport> = .idle,
        history: AnalyticsSectionLoadState<HistoryReport> = .idle
    ) {
        self.timeRange = timeRange
        self.lastRefreshedAt = lastRefreshedAt
        self.productivitySummary = productivitySummary
        self.productivityVelocity = productivityVelocity
        self.burndown = burndown
        self.queueStats = queueStats
        self.history = history
    }

    public var isLoading: Bool {
        productivitySummary.isLoading
            || productivityVelocity.isLoading
            || burndown.isLoading
            || queueStats.isLoading
            || history.isLoading
    }

    public var hasFailures: Bool {
        productivitySummary.failureMessage != nil
            || productivityVelocity.failureMessage != nil
            || burndown.failureMessage != nil
            || queueStats.failureMessage != nil
            || history.failureMessage != nil
    }

    public var productivitySummaryValue: ProductivitySummaryReport? { productivitySummary.currentValue }
    public var productivityVelocityValue: ProductivityVelocityReport? { productivityVelocity.currentValue }
    public var burndownValue: BurndownReport? { burndown.currentValue }
    public var queueStatsValue: QueueStatsReport? { queueStats.currentValue }
    public var historyValue: HistoryReport? { history.currentValue }

    public var productivitySummaryRenderState: AnalyticsRenderableState {
        productivitySummary.renderState(kind: .productivitySummary) { !$0.isEmptyForAnalyticsPresentation }
    }

    public var productivityVelocityRenderState: AnalyticsRenderableState {
        productivityVelocity.renderState(kind: .productivityVelocity) { !$0.isEmptyForAnalyticsPresentation }
    }

    public var burndownRenderState: AnalyticsRenderableState {
        burndown.renderState(kind: .burndown) { !$0.isEmptyForAnalyticsPresentation }
    }

    public var queueStatsRenderState: AnalyticsRenderableState {
        queueStats.renderState(kind: .queueStats) { !$0.isEmptyForAnalyticsPresentation }
    }

    public var historyRenderState: AnalyticsRenderableState {
        history.renderState(kind: .history) { !$0.isEmptyForAnalyticsPresentation }
    }

    public func loadingAllSections() -> Self {
        Self(
            timeRange: timeRange,
            lastRefreshedAt: lastRefreshedAt,
            productivitySummary: .loading(previous: productivitySummary.currentValue),
            productivityVelocity: .loading(previous: productivityVelocity.currentValue),
            burndown: .loading(previous: burndown.currentValue),
            queueStats: .loading(previous: queueStats.currentValue),
            history: .loading(previous: history.currentValue)
        )
    }
}

// MARK: - Productivity Models

public struct ProductivitySummaryReport: Codable, Sendable, Equatable {
    public let totalCompleted: Int
    public let currentStreak: Int
    public let longestStreak: Int
    public let lastCompletedDate: String?
    public let nextMilestone: Int?
    public let milestones: [Milestone]
    public let recentCompletions: [CompletedTaskRef]

    public var isEmptyForAnalyticsPresentation: Bool {
        totalCompleted == 0
            && milestones.isEmpty
            && recentCompletions.isEmpty
            && lastCompletedDate == nil
            && nextMilestone == nil
    }

    private enum CodingKeys: String, CodingKey {
        case totalCompleted = "total_completed"
        case currentStreak = "current_streak"
        case longestStreak = "longest_streak"
        case lastCompletedDate = "last_completed_date"
        case nextMilestone = "next_milestone"
        case milestones
        case recentCompletions = "recent_completions"
    }
}

public struct Milestone: Codable, Sendable, Equatable {
    public let threshold: Int
    public let achievedAt: String
    public let celebrated: Bool

    private enum CodingKeys: String, CodingKey {
        case threshold
        case achievedAt = "achieved_at"
        case celebrated
    }
}

public struct CompletedTaskRef: Codable, Sendable, Equatable {
    public let id: String
    public let title: String
    public let completedAt: String

    private enum CodingKeys: String, CodingKey {
        case id, title
        case completedAt = "completed_at"
    }
}

public struct ProductivityVelocityReport: Codable, Sendable, Equatable {
    public let windowDays: Int
    public let totalCompleted: Int
    public let averagePerDay: Double
    public let bestDay: BestDay?

    public var isEmptyForAnalyticsPresentation: Bool {
        totalCompleted == 0 && bestDay == nil && averagePerDay == 0
    }

    private enum CodingKeys: String, CodingKey {
        case windowDays = "window_days"
        case totalCompleted = "total_completed"
        case averagePerDay = "average_per_day"
        case bestDay = "best_day"
    }
}

public struct BestDay: Codable, Sendable, Equatable {
    public let date: String
    public let count: Int

    public init(date: String, count: Int) {
        self.date = date
        self.count = count
    }

    public init(from decoder: any Decoder) throws {
        var container = try decoder.unkeyedContainer()
        date = try container.decode(String.self)
        count = try container.decode(Int.self)
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(date)
        try container.encode(count)
    }
}

// MARK: - Burndown Models

public struct BurndownReport: Codable, Sendable, Equatable {
    public let window: BurndownWindow
    public let dailyCounts: [BurndownDay]
    public let maxCount: Int

    public var isEmptyForAnalyticsPresentation: Bool {
        dailyCounts.isEmpty || maxCount == 0
    }

    private enum CodingKeys: String, CodingKey {
        case window
        case dailyCounts = "daily_counts"
        case maxCount = "max_count"
    }
}

public struct BurndownWindow: Codable, Sendable, Equatable {
    public let days: Int
    public let startDate: String
    public let endDate: String

    private enum CodingKeys: String, CodingKey {
        case days
        case startDate = "start_date"
        case endDate = "end_date"
    }
}

public struct BurndownDay: Codable, Sendable, Equatable {
    public let date: String
    public let remaining: Int
}

// MARK: - Queue Stats Models

public struct QueueStatsReport: Codable, Sendable, Equatable {
    public let summary: StatsSummary
    public let tagBreakdown: [TagBreakdown]

    public var isEmptyForAnalyticsPresentation: Bool {
        summary.total == 0
    }

    private enum CodingKeys: String, CodingKey {
        case summary
        case tagBreakdown = "tag_breakdown"
    }
}

public struct StatsSummary: Codable, Sendable, Equatable {
    public let total: Int
    public let done: Int
    public let rejected: Int
    public let terminal: Int
    public let active: Int
    public let terminalRate: Double

    private enum CodingKeys: String, CodingKey {
        case total, done, rejected, terminal, active
        case terminalRate = "terminal_rate"
    }
}

public struct TagBreakdown: Codable, Sendable, Equatable {
    public let tag: String
    public let count: Int
    public let percentage: Double
}

// MARK: - History Models

public struct HistoryReport: Codable, Sendable, Equatable {
    public let window: HistoryWindow
    public let days: [HistoryDay]

    public var isEmptyForAnalyticsPresentation: Bool {
        days.isEmpty || days.allSatisfy { $0.created.isEmpty && $0.completed.isEmpty }
    }
}

public struct HistoryWindow: Codable, Sendable, Equatable {
    public let days: Int
    public let startDate: String
    public let endDate: String

    private enum CodingKeys: String, CodingKey {
        case days
        case startDate = "start_date"
        case endDate = "end_date"
    }
}

public struct HistoryDay: Codable, Sendable, Equatable {
    public let date: String
    public let created: [String]
    public let completed: [String]
}

// MARK: - Time Range Enum

public enum TimeRange: String, CaseIterable, Identifiable, Sendable {
    case sevenDays = "7d"
    case thirtyDays = "30d"
    case ninetyDays = "90d"
    case allTime = "all"

    public var id: String { rawValue }

    public var displayName: String {
        switch self {
        case .sevenDays: return "7 Days"
        case .thirtyDays: return "30 Days"
        case .ninetyDays: return "90 Days"
        case .allTime: return "All Time"
        }
    }

    public var days: Int? {
        switch self {
        case .sevenDays: return 7
        case .thirtyDays: return 30
        case .ninetyDays: return 90
        case .allTime: return nil
        }
    }
}

// MARK: - Aggregated Dashboard Models

public enum SectionStatus: String, Codable, Sendable, Equatable {
    case ok
    case unavailable
}

public struct SectionResult<T: Codable & Sendable & Equatable>: Codable, Sendable, Equatable {
    public let status: SectionStatus
    public let data: T?
    public let errorMessage: String?

    private enum CodingKeys: String, CodingKey {
        case status
        case data
        case errorMessage = "error_message"
    }
}

public struct DashboardSections: Codable, Sendable, Equatable {
    public let productivitySummary: SectionResult<ProductivitySummaryReport>
    public let productivityVelocity: SectionResult<ProductivityVelocityReport>
    public let burndown: SectionResult<BurndownReport>
    public let queueStats: SectionResult<QueueStatsReport>
    public let history: SectionResult<HistoryReport>

    private enum CodingKeys: String, CodingKey {
        case productivitySummary = "productivity_summary"
        case productivityVelocity = "productivity_velocity"
        case burndown
        case queueStats = "queue_stats"
        case history
    }
}

public struct DashboardReport: Codable, Sendable, Equatable {
    public let windowDays: Int
    public let generatedAt: String
    public let sections: DashboardSections

    private enum CodingKeys: String, CodingKey {
        case windowDays = "window_days"
        case generatedAt = "generated_at"
        case sections
    }
}
