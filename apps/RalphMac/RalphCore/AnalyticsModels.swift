/**
 AnalyticsModels

 Responsibilities:
 - Define Codable models for parsing analytics JSON from `ralph productivity` and `ralph queue` commands.
 - Provide type-safe representations of productivity stats, burndown data, and queue statistics.

 Does not handle:
 - CLI execution (see Workspace.swift).
 - View rendering (see AnalyticsDashboardView.swift).

 Invariants/assumptions callers must respect:
 - JSON structure matches CLI output with --format json flag.
 - Dates are ISO8601 format.
 */

import Foundation

// MARK: - Productivity Models

public struct ProductivitySummaryReport: Codable, Sendable, Equatable {
    public let totalCompleted: Int
    public let currentStreak: Int
    public let longestStreak: Int
    public let lastCompletedDate: String?
    public let nextMilestone: Int?
    public let milestones: [Milestone]
    public let recentCompletions: [CompletedTaskRef]
    
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

public enum TimeRange: String, CaseIterable, Identifiable {
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

/// Status of an individual dashboard section.
public enum SectionStatus: String, Codable, Sendable, Equatable {
    case ok
    case unavailable
}

/// Wrapper for a section that may have succeeded or failed.
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

/// All dashboard sections with status wrappers.
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

/// Aggregated dashboard response from `ralph queue dashboard`.
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
